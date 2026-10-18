//! `chain_tour` — the frozen `code_reality/chain_tour.py` contract:
//! callchain markdown → one CodeTour `.tour` per scenario. Tree-frame
//! DFS order = step order; step lines re-anchored against graph.db
//! (same/moved/moved-file); unanchorable frames (no anchor / external /
//! name collision) are skipped with the real reason recorded in the tour
//! description.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{anchor_pattern, assert_db_unchanged, connect_ro, db_mtime_ns};
use crate::profile::{is_excluded, load_profile, Profile};
use crate::symbol_resolve::{
    explicit_line_candidates, explicit_trait_collision, parse_qualifier, resolve_candidates,
    same_file_rows, FillCandidate, FillOutcome, FrameQualifier, QualifiedMethod,
};
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Re-export: the ident extractor moved to the shared resolver module
/// (`symbol_resolve`) — the `chain_tour::best_ident` path stays valid.
pub use crate::symbol_resolve::best_ident;

fn ref_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"([\w./\-]+\.(?:py|rs)):(\d+)").unwrap())
}

fn lineless_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[\w./\-]+\.(?:py|rs)$").unwrap())
}

fn any_path_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[\w./\-]+\.(?:py|rs)").unwrap())
}

/// CR-7 line-less anchor extraction: bounded tail-position grammar. After
/// `#`-note strip and folding back trailing `)`/`]`/`）` closers, the path
/// must END the frame, preceded by a token boundary (start / whitespace /
/// `(` / `[` / `（` — kills URLs like `https://host/foo.py`); the
/// extension must be the token's true end (rejects `foo.pyc`,
/// `foo.py.bak`, residual `foo.py:xxx`); and the symbol remainder must
/// not carry a second independent path token (`a.py b.py` is prose, not
/// an anchor; `client.py_helper` is one word whose `.py` is buried —
/// trailing word-chars disqualify a candidate). Returns (path, symbol
/// remainder). A prose mention that survives the grammar (`see foo.py`)
/// is indistinguishable from an anchor here by design — the graph-fill
/// gate downstream turns a false hit into a visible `noanchor` skip, not
/// a fake step.
fn lineless_tail(content: &str) -> Option<(String, String)> {
    // CR-7 calibration (mosaic corpus, 2026-10-11): single-space ` # `
    // notes are the corpus norm — cut the note segment before tail
    // extraction. The double-space split upstream is legacy-frozen for
    // explicit frames; line-less frames have no legacy face to preserve.
    let anchor_region = content.split(" # ").next().unwrap_or(content);
    let trimmed = anchor_region.trim_end().trim_end_matches([')', ']', '）']);
    let m = lineless_re().find(trimmed)?;
    if m.end() != trimmed.len() {
        return None;
    }
    match trimmed[..m.start()].chars().next_back() {
        None => {}
        Some(c) if c.is_whitespace() || c == '(' || c == '[' || c == '（' => {}
        _ => return None,
    }
    let rest = trimmed[..m.start()]
        .trim_end()
        .trim_end_matches(['(', '[', '（'])
        .trim_end();
    if stray_path_token(rest) {
        return None;
    }
    Some((m.as_str().to_string(), rest.to_string()))
}

/// Second-path-token detector for the line-less symbol remainder (CR-7
/// review F6): a `.py`/`.rs` occurrence only counts as an independent
/// path token when the extension is not buried inside a longer word —
/// `client.py_helper` is one identifier, `a.py extra` is two tokens.
/// Conservative direction: a false positive rejects the anchor to noref
/// (fail-closed), never fabricates one.
fn stray_path_token(text: &str) -> bool {
    for m in any_path_re().find_iter(text) {
        match text[m.end()..].chars().next() {
            None => return true,
            Some(c) if !c.is_ascii_alphanumeric() && c != '_' => return true,
            _ => {}
        }
    }
    false
}

const TREE_PREFIX_CHARS: [char; 5] = ['│', '├', '└', '─', ' '];

pub fn prefix_len(line: &str) -> usize {
    let mut n = 0;
    for ch in line.chars() {
        if TREE_PREFIX_CHARS.contains(&ch) {
            n += 1;
        } else {
            break;
        }
    }
    n
}

/// Code blocks containing tree frames (├/└) plus their nearest preceding
/// heading (= scenario name).
pub fn parse_blocks(text: &str) -> Vec<(String, Vec<String>)> {
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    let mut heading = String::new();
    let mut in_code = false;
    let mut cur: Option<(String, Vec<String>)> = None;
    for ln in text.split('\n') {
        if !in_code && ln.trim_start().starts_with('#') {
            heading = ln.trim_start_matches(['#', ' ']).trim().to_string();
        }
        if ln.trim_start().starts_with("```") {
            if !in_code {
                cur = Some((heading.clone(), Vec::new()));
                in_code = true;
            } else {
                if let Some((h, lines)) = cur.take() {
                    if lines.iter().any(|l| l.contains('├') || l.contains('└')) {
                        blocks.push((h, lines));
                    }
                }
                in_code = false;
            }
            continue;
        }
        if in_code {
            if let Some((_, lines)) = cur.as_mut() {
                lines.push(ln.to_string());
            }
        }
    }
    blocks
}

/// Best ident from symbol text — moved to [`crate::symbol_resolve`]
/// (shared with the CR-15 tour reanchor/validate faces); the re-export
/// above keeps this module's public path.

/// Whole-frame token of a frame's symbol remainder (CR-9 COD-1 gate):
/// trim, then cut one trailing `(...)` parameter group, so `main()` /
/// `execute(ctx)` reduce to `main` / `execute` while a prose-wrapped
/// remainder (`see Cls.method`) stays a multi-word string. A frame is
/// disambiguation-eligible (R1/R2) only when this token equals the
/// frame's full ident — prose around the token carries no
/// disambiguation evidence.
fn whole_frame_token(symbol: &str) -> &str {
    let t = symbol.trim();
    if t.ends_with(')') {
        if let Some(i) = t.rfind('(') {
            return t[..i].trim();
        }
    }
    t
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub depth: usize,
    pub parent: Option<usize>,
    pub symbol: String,
    pub ident: String,
    /// Full dotted form of the winning ident token (CR-9 R2): the
    /// qualified name the doc wrote (`Outer.A.execute`), empty when the
    /// frame carries no ident.
    pub ident_full: String,
    /// CR-18: the parsed md trait qualifier (`Price::fmt[Display]`).
    /// `WellFormed` frames query by the declared identity (method =
    /// name axis; receiver+trait verified by the shared selector) and
    /// never fall back to best_ident ranking, line proximity, or
    /// R1/R2; `Malformed` frames skip under `qualifier_malformed`;
    /// `None` keeps the frozen CR-17/R1/R2/R3 semantics.
    pub qualifier: FrameQualifier,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub note: String,
    pub prefix: String,
}

/// Tree frame lines → frames (depth from a stack, not pl//3 — the POC's
/// pl//3 mislevels `│+4 spaces` indentation).
pub fn parse_frames(lines: &[String]) -> Vec<Frame> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        let pl = prefix_len(raw);
        let content = raw.chars().skip(pl).collect::<String>().trim().to_string();
        if content.is_empty() {
            continue;
        }
        while let Some(&(top_pl, _)) = stack.last() {
            if top_pl >= pl {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().map(|&(_, idx)| idx);
        let depth = stack.len();
        let (mut content2, mut note) = if content.contains("  # ") || content.starts_with('#') {
            match content.split_once('#') {
                Some((c, n)) => (c.trim().to_string(), n.trim().to_string()),
                None => (content.clone(), String::new()),
            }
        } else {
            (content.clone(), String::new())
        };
        let mut path = None;
        let mut line_no = None;
        if let Some(m) = ref_re().captures(&content2) {
            // CR-7 calibration (mosaic sample C): a single-space ` # `
            // note may cite its own path:line — if the explicit match lies
            // INSIDE the note region, the frame is line-less with a
            // citing note, not an explicit anchor on the citation
            // (leftmost-match already protects true explicit frames, so
            // this reclassification only touches frames that would
            // otherwise anchor to the citation — a latent misparse)
            let m_start = m.get(0).unwrap().start();
            let in_note = content2
                .find(" # ")
                .map(|i| m_start > i)
                .unwrap_or(false);
            if in_note {
                if let Some((p, rest)) = lineless_tail(&content2) {
                    path = Some(p);
                    if let Some((_, n)) = content2.split_once(" # ") {
                        note = n.trim().to_string();
                    }
                    content2 = rest;
                }
            } else {
                path = Some(m[1].to_string());
                line_no = Some(m[2].parse().unwrap_or(0));
                let range = ref_re().find(&content2).map(|m| m.range());
                if let Some(r) = range {
                    content2 = format!("{}{}", &content2[..r.start], &content2[r.end..])
                        .trim()
                        .to_string();
                }
            }
        } else if let Some((p, rest)) = lineless_tail(&content2) {
            // CR-7: line-less anchor — the path token is stripped so symbol
            // extraction sees only the remainder; line stays None (the
            // line-less decision branch downstream fills it or skips).
            // Calibration: recover the single-space ` # ` note mosaic's
            // corpus uses (the upstream split only fires on double-space)
            path = Some(p);
            if note.is_empty() {
                if let Some((_, n)) = content2.split_once(" # ") {
                    note = n.trim().to_string();
                }
            }
            content2 = rest;
        }
        // CR-18: the trait qualifier (`Price::fmt[Display]`) parses off
        // the post-strip remainder and REPLACES best_ident for the query
        // identity — the POC proved best_ident length-ranks the trait
        // name (`Display`) on the paren-less canonical form and the
        // receiver on others. A leftover empty `()` from a
        // `(path:line)` wrapper folds off the kept author text.
        let qualifier = parse_qualifier(&content2);
        if matches!(qualifier, FrameQualifier::WellFormed(_)) {
            if let Some(t) = content2.trim_end().strip_suffix("()") {
                content2 = t.trim_end().to_string();
            }
        }
        let (ident, ident_full) = match &qualifier {
            FrameQualifier::WellFormed(qm) => {
                (qm.method.clone(), format!("{}::{}", qm.receiver, qm.method))
            }
            _ => best_ident(&content2),
        };
        frames.push(Frame {
            depth,
            parent,
            symbol: content2.clone(),
            ident,
            ident_full,
            qualifier,
            path,
            line: line_no,
            note,
            prefix: raw.chars().take(pl).collect(),
        });
        stack.push((pl, frames.len() - 1));
    }
    frames
}

/// Package-relative path → absolute (direct → suffix → ctx majority →
/// ambiguous). Generic fallback (no profile) scans the whole repo —
/// same-name files under exclusion prefixes stay out of the pool.
pub struct PathResolver {
    repo_root: PathBuf,
    profile: Option<Profile>,
    pkg_roots: Vec<PathBuf>,
    ctx_dirs: HashMap<String, i64>,
}

impl PathResolver {
    pub fn new(repo_root: &Path) -> Result<Self, String> {
        let repo_root = crate::common::resolve(repo_root);
        let profile = load_profile(&repo_root)?;
        let pkg_roots: Vec<PathBuf> = profile
            .as_ref()
            .map(|p| {
                p.modules
                    .iter()
                    .map(|r| repo_root.join(r.prefix.trim_end_matches('/')))
                    .collect()
            })
            .filter(|v: &Vec<PathBuf>| !v.is_empty())
            .unwrap_or_else(|| vec![repo_root.clone()]);
        Ok(Self {
            repo_root,
            profile,
            pkg_roots,
            ctx_dirs: HashMap::new(),
        })
    }

    fn bump_dir(&mut self, p: &Path) {
        let d = p
            .parent()
            .map(|x| x.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        *self.ctx_dirs.entry(d).or_insert(0) += 1;
    }

    pub fn resolve(&mut self, path: &str) -> (Option<PathBuf>, &'static str) {
        for pkg in &self.pkg_roots {
            let direct = pkg.join(path);
            if direct.exists() {
                self.bump_dir(&direct);
                return (Some(direct), "direct");
            }
        }
        let mut name_matches: BTreeMap<String, PathBuf> = BTreeMap::new();
        let fname = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut opts = glob::MatchOptions::new();
        opts.require_literal_leading_dot = false;
        for pkg in &self.pkg_roots {
            let pattern = pkg
                .join(format!("**/{fname}"))
                .to_string_lossy()
                .into_owned();
            for p in glob::glob_with(&pattern, opts)
                .into_iter()
                .flatten()
                .filter_map(|r| r.ok())
            {
                if let Ok(rel) = p.strip_prefix(&self.repo_root) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if !is_excluded(&rel, self.profile.as_ref()) {
                        name_matches.insert(p.to_string_lossy().replace('\\', "/"), p);
                    }
                }
            }
        }
        // suffix match needs a / boundary; name-only pool is the loose
        // fallback when suffix comes up empty (POC semantics)
        let mut pool: BTreeMap<String, PathBuf> = name_matches
            .iter()
            .filter(|(k, _)| k.as_str() == path || k.ends_with(&format!("/{path}")))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if pool.is_empty() {
            pool = name_matches;
        }
        if pool.is_empty() {
            return (None, "none");
        }
        if pool.len() == 1 {
            let p = pool.into_values().next().unwrap();
            self.bump_dir(&p);
            return (Some(p), "suffix");
        }
        let mut scored: Vec<(i64, String, PathBuf)> = pool
            .into_iter()
            .map(|(k, v)| {
                let dir = v
                    .parent()
                    .map(|x| x.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                (-self.ctx_dirs.get(&dir).copied().unwrap_or(0), k, v)
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let (best_score, _, best) = scored.swap_remove(0);
        if -best_score > 0 {
            self.bump_dir(&best);
            return (Some(best), "ctx");
        }
        (None, "ambiguous")
    }
}

/// Substring anchor check: ok / drift / drift-far / missing / nocheck.
pub fn check_anchor(abs_path: &Path, line_no: i64, ident: &str) -> String {
    if ident.is_empty() || !abs_path.exists() {
        return "nocheck".into();
    }
    let text = std::fs::read_to_string(abs_path).unwrap_or_default();
    let lines: Vec<&str> = text.split('\n').collect();
    if (line_no - 1) >= 0
        && ((line_no - 1) as usize) < lines.len()
        && lines[(line_no - 1) as usize].contains(ident)
    {
        return "ok".into();
    }
    let lo = ((line_no - 9).max(0) as usize).min(lines.len());
    let hi = ((line_no + 8).max(0) as usize).min(lines.len());
    let window = &lines[lo..hi];
    if window.iter().any(|w| w.contains(ident)) {
        return "drift".into();
    }
    if lines.iter().any(|w| w.contains(ident)) {
        return "drift-far".into();
    }
    "missing".into()
}

fn like_escape_chain(part: &str) -> String {
    part.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// graph.db re-anchoring: same-file nearest line_start (gated by the
/// CR-17 explicit-line trait-collision safe selection — see
/// [`GraphAnchor::anchor`]) → cross-file move detection →
/// not-in-graph. Cross-file gate (POC discipline, legacy face only):
/// ident must be substring-missing AND length ≥ 4 — a CR-18 declared
/// identity (`qm`) bypasses both gates (the triple comparison is the
/// precision) and its zero-rels terminal refuses under
/// `qualifier_no_hit`.
pub struct GraphAnchor {
    conn: Connection,
    repo_root: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct GraphHit {
    pub g: String,
    pub g_line: Option<i64>,
    pub g_file: Option<String>,
    pub g_files: Vec<String>,
    /// True SCIP symbol of the selected definition occurrence (AIR-80):
    /// identity for CR reanchor maintenance — file+name+nearest-line is a
    /// lookup key, not a complete identity.
    pub g_symbol: Option<String>,
    /// CR-17 explicit-line trait-collision payload: the full same-file
    /// same-name candidate set (superset of the colliding group — a
    /// same-name free fn is a sibling the legacy nearest-line SQL
    /// picked from, so it stays disclosed), present when the candidates
    /// include ≥2 impl-shaped symbols with the same inherent type and
    /// differing trait groups. Non-empty on BOTH resolution modes — the
    /// exact-hit kept anchor (`g == "same"`, disclosed on the step) and
    /// the fail-closed refusal (`g == "explicit_trait_ambiguous"`,
    /// candidates listed in the skip hint).
    pub g_trait_candidates: Vec<FillCandidate>,
    /// CR-18: step-description note for a qualifier-selected success
    /// (declared-identity evidence — the author's
    /// `Receiver::method[Trait]` picked this anchor, line drift
    /// notwithstanding).
    pub g_qualifier_note: Option<String>,
    /// CR-18: candidate payload for a qualifier refusal
    /// (`qualifier_no_hit` / `qualifier_ambiguous` /
    /// `qualifier_unverified` / `cross_file_trait_ambiguous`) — the
    /// full candidate set of the face that refused, disclosed on the
    /// description hint and the CLI WARN like the CR-17 payload.
    pub g_qualifier_candidates: Vec<FillCandidate>,
}

impl GraphAnchor {
    pub fn new(db: &Path, repo_root: &Path) -> Result<Self, String> {
        let conn = connect_ro(db)?;
        // schema probe (review C4): the anchor face reads nodes.symbol
        // (unique tie-break) — a legacy-schema db would silently degrade
        // every same-file anchor to not-in-graph; fail loud instead
        conn.prepare("SELECT symbol FROM nodes LIMIT 1").map_err(|_| {
            format!(
                "--graph 指定的 db 非自有格式（nodes.symbol 缺）：{}——重跑 `code-reality graph_db build --repo <repo>`",
                db.display()
            )
        })?;
        Ok(Self {
            conn,
            repo_root: crate::common::resolve(repo_root),
        })
    }

    /// CR-17 explicit-line safe selection: the same-file same-name
    /// candidate set is read in full (ordering stays `ABS(distance),
    /// line_start, symbol`, so the no-collision pick is byte-identical
    /// to the legacy `LIMIT 1` nearest face). When the set carries a
    /// trait collision (≥2 impl-shaped symbols, same inherent type,
    /// differing trait groups — the NT `Price::fmt` shape): a doc line
    /// that exactly hits ONE candidate keeps that anchor (`same`, the
    /// exact line is real evidence) with the collision disclosed;
    /// anything else (drifted line, or a non-unique exact hit) refuses
    /// the nearest-line guess — `explicit_trait_ambiguous`, no anchor.
    /// A failed candidate read (prepare/query_map/row decode) returns
    /// `graph-read-error` — the frame degrades to a doc-line step with
    /// no identity rather than guessing from a partial set (C1).
    ///
    /// CR-18 qualifier face (`qm` present): the shared qualified
    /// selector decides over the same full candidate set BEFORE the
    /// collision gate — a unique declared-identity match selects
    /// (`same`/`moved`, collision semantics bypassed: the author named
    /// the identity, no WARN), every refusal returns its own
    /// `qualifier_*` key with the candidate payload, and nothing ever
    /// falls back to nearest-line or R1/R2. The substring/length
    /// cross-file gates apply ONLY to the legacy face (`qm == None`) —
    /// a qualifier frame with zero same-file candidates falls through
    /// to the cross-file convergence query (C1/F-1: both faces treat a
    /// declared identity alike; the zero-rels terminal answers
    /// `qualifier_no_hit` instead of a silent `not-in-graph` doc-line
    /// step; C2: a short method ident — `fmt` — is no longer gated,
    /// the triple comparison is the precision). The cross-file branch
    /// (F7): single-file convergence now reads the FULL candidate set
    /// (fail-closed — a read error degrades to `graph-read-error`,
    /// never a silently partial set) and runs the same determination —
    /// qualifier selection, else the collision gate (refusing under
    /// `cross_file_trait_ambiguous` instead of the legacy silent
    /// lowest-line pick), else the byte-identical legacy pick.
    /// `rels > 1` keeps `moved-file-ambiguous` — a qualifier never
    /// selects across files on its own.
    pub fn anchor(
        &self,
        abs_path: &Path,
        line_no: i64,
        ident: &str,
        substring_status: &str,
        qm: Option<&QualifiedMethod>,
    ) -> GraphHit {
        let rel = abs_path
            .strip_prefix(&self.repo_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let like = format!("%/{}", like_escape_chain(&rel));
        // C1 (review): fail-closed candidate read — prepare/query_map/row
        // decode errors return a `graph-read-error` hit instead of an
        // empty set (an empty set falls through to not-in-graph /
        // cross-file nearest, and a partially-read set could silently
        // drop a collision member and mint an unverified identity).
        let rows_res: Result<Vec<(i64, String, String)>, rusqlite::Error> =
            match self.conn.prepare(
                "SELECT line_start, symbol, kind FROM nodes \
                 WHERE file_path LIKE ?1 ESCAPE '\\' AND name=?2 \
                 AND line_start IS NOT NULL \
                 ORDER BY ABS(line_start-?3), line_start, symbol",
            ) {
                Ok(mut stmt) => stmt
                    .query_map((&like, ident, line_no), |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .and_then(|mapped| mapped.collect::<Result<Vec<_>, _>>()),
                Err(e) => Err(e),
            };
        let rows: Vec<(i64, String, String)> = match rows_res {
            Ok(rows) => rows,
            Err(_) => {
                return GraphHit {
                    g: "graph-read-error".into(),
                    ..Default::default()
                }
            }
        };
        if !rows.is_empty() {
            if let Some(qm) = qm {
                // CR-18: declared identity decides first — a unique
                // triple-match selects (counting under the legacy
                // same/moved keys for distribution comparability);
                // refusals carry their own keys, never a fallback.
                return match crate::symbol_resolve::qualified_select(&rows, qm) {
                    crate::symbol_resolve::QualifiedVerdict::Unique { row } => {
                        let (g_line, g_symbol, _) = &rows[row];
                        let delta = g_line - line_no;
                        GraphHit {
                            g: if delta == 0 {
                                "same".into()
                            } else {
                                "moved".into()
                            },
                            g_line: Some(*g_line),
                            g_file: None,
                            g_files: vec![],
                            g_symbol: Some(g_symbol.clone()),
                            g_trait_candidates: Vec::new(),
                            g_qualifier_note: None,
                            g_qualifier_candidates: Vec::new(),
                        }
                    }
                    crate::symbol_resolve::QualifiedVerdict::Refused { reason, candidates } => {
                        GraphHit {
                            g: reason.key().into(),
                            g_trait_candidates: Vec::new(),
                            g_qualifier_note: None,
                            g_qualifier_candidates: candidates,
                            ..Default::default()
                        }
                    }
                };
            }
            if explicit_trait_collision(&rows) {
                let cands = explicit_line_candidates(&rows);
                let exact: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, (l, _, _))| *l == line_no)
                    .map(|(i, _)| i)
                    .collect();
                if exact.len() == 1 {
                    let (g_line, g_symbol, _) = &rows[exact[0]];
                    return GraphHit {
                        g: "same".into(),
                        g_line: Some(*g_line),
                        g_file: None,
                        g_files: vec![],
                        g_symbol: Some(g_symbol.clone()),
                        g_trait_candidates: cands,
                        g_qualifier_note: None,
                        g_qualifier_candidates: Vec::new(),
                    };
                }
                return GraphHit {
                    g: "explicit_trait_ambiguous".into(),
                    g_trait_candidates: cands,
                    ..Default::default()
                };
            }
            let (g_line, g_symbol, _) = &rows[0];
            let delta = g_line - line_no;
            return GraphHit {
                g: if delta == 0 {
                    "same".into()
                } else {
                    "moved".into()
                },
                g_line: Some(*g_line),
                g_file: None,
                g_files: vec![],
                g_symbol: Some(g_symbol.clone()),
                g_trait_candidates: Vec::new(),
                g_qualifier_note: None,
                g_qualifier_candidates: Vec::new(),
            };
        }
        // CR-18 review (C1/F-1/C2): the substring/length cross-file
        // gates are LEGACY-face discipline (POC short-name /
        // substring-present guard against fuzzy cross-file moves). A
        // declared identity (`qm`) is precise — the triple comparison
        // rules candidates in or out on receiver+trait, never on
        // prose-substring or name length — so a qualifier frame with
        // zero same-file candidates falls through to the cross-file
        // convergence query instead of a `not-in-graph` doc-line step
        // with zero WARN (the line-less face already refused; the two
        // faces must treat a declared identity alike).
        if qm.is_none()
            && (substring_status != "missing" || ident.chars().count() < 4)
        {
            return GraphHit {
                g: "not-in-graph".into(),
                ..Default::default()
            };
        }
        let prefix = like_escape_chain(&format!("{}/", self.repo_root.display()));
        // CR-18 (F7 read integrity): the cross-file rels read is
        // fail-closed too — a swallowed row error here would shrink the
        // rels list and fake a single-file convergence (the CR-17 C1
        // lesson applied to the sibling query).
        let rels_res: Result<Vec<String>, rusqlite::Error> = match self.conn.prepare(
            "SELECT DISTINCT substr(file_path, length(?1)+2) FROM nodes \
             WHERE name=?2 AND file_path LIKE ?3 ESCAPE '\\' LIMIT 6",
        ) {
            Ok(mut stmt) => match stmt.query_map(
                (
                    self.repo_root.display().to_string(),
                    ident,
                    format!("{prefix}%"),
                ),
                |r| r.get::<_, Option<String>>(0),
            ) {
                Ok(rows) => rows
                    .collect::<Result<Vec<Option<String>>, _>>()
                    .map(|v| v.into_iter().flatten().collect()),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        let rels: Vec<String> = match rels_res {
            Ok(rels) => rels,
            Err(_) => {
                return GraphHit {
                    g: "graph-read-error".into(),
                    ..Default::default()
                }
            }
        };
        if rels.len() == 1 {
            // CR-18 (F7): the converged file's candidate set is read in
            // FULL (fail-closed — same C1 discipline) before any
            // selection: qualifier determination first, then the
            // collision gate, then the byte-identical legacy
            // lowest-line pick (the old `LIMIT 1` face).
            let rows_res: Result<Vec<(i64, String, String)>, rusqlite::Error> = match self
                .conn
                .prepare(
                    "SELECT line_start, symbol, kind FROM nodes WHERE name=?1 \
                     AND file_path LIKE ?2 ESCAPE '\\' \
                     AND line_start IS NOT NULL \
                     ORDER BY line_start, symbol",
                )
            {
                Ok(mut stmt) => match stmt.query_map(
                    (ident, format!("{prefix}{}", like_escape_chain(&rels[0]))),
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                ) {
                    Ok(rows) => rows.collect::<Result<Vec<_>, _>>(),
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            let rows: Vec<(i64, String, String)> = match rows_res {
                Ok(rows) => rows,
                Err(_) => {
                    return GraphHit {
                        g: "graph-read-error".into(),
                        ..Default::default()
                    }
                }
            };
            if let Some(qm) = qm {
                return match crate::symbol_resolve::qualified_select(&rows, qm) {
                    crate::symbol_resolve::QualifiedVerdict::Unique { row } => GraphHit {
                        g: "moved-file".into(),
                        g_line: Some(rows[row].0),
                        g_file: Some(rels[0].clone()),
                        g_files: vec![],
                        g_symbol: Some(rows[row].1.clone()),
                        g_trait_candidates: Vec::new(),
                        g_qualifier_note: None,
                        g_qualifier_candidates: Vec::new(),
                    },
                    crate::symbol_resolve::QualifiedVerdict::Refused { reason, candidates } => {
                        GraphHit {
                            g: reason.key().into(),
                            g_file: Some(rels[0].clone()),
                            g_trait_candidates: Vec::new(),
                            g_qualifier_note: None,
                            g_qualifier_candidates: candidates,
                            ..Default::default()
                        }
                    }
                };
            }
            if explicit_trait_collision(&rows) {
                return GraphHit {
                    g: "cross_file_trait_ambiguous".into(),
                    g_file: Some(rels[0].clone()),
                    g_trait_candidates: Vec::new(),
                    g_qualifier_note: None,
                    g_qualifier_candidates: explicit_line_candidates(&rows),
                    ..Default::default()
                };
            }
            // rows keep the (line_start, symbol) order — rows[0] IS the
            // legacy `ORDER BY line_start, symbol LIMIT 1` pick
            let (ln, g_symbol) = rows
                .first()
                .map(|(l, s, _)| (Some(*l), Some(s.clone())))
                .unwrap_or((None, None));
            return GraphHit {
                g: "moved-file".into(),
                g_line: ln,
                g_file: Some(rels[0].clone()),
                g_files: vec![],
                g_symbol,
                g_trait_candidates: Vec::new(),
                g_qualifier_note: None,
                g_qualifier_candidates: Vec::new(),
            };
        }
        if rels.len() > 1 {
            return GraphHit {
                g: "moved-file-ambiguous".into(),
                g_files: rels.into_iter().take(4).collect(),
                ..Default::default()
            };
        }
        // CR-18 review (C1/F-1): zero rels — the declared identity
        // exists nowhere in the graph. The legacy face keeps its
        // `not-in-graph` doc-line step; the qualifier face refuses
        // fail-loud (`qualifier_no_hit` → skip + WARN pipeline) — a
        // declared identity is never silently degraded to an unverified
        // step. (rels==1 convergence and rels>1 moved-file-ambiguous
        // keep their Plan ⑦ semantics on both faces.)
        if qm.is_some() {
            return GraphHit {
                g: "qualifier_no_hit".into(),
                ..Default::default()
            };
        }
        GraphHit {
            g: "not-in-graph".into(),
            ..Default::default()
        }
    }
}

// FillCandidate / FillOutcome and the R1/R2/R3 ambiguity core
// (descriptor_region / descriptor_chain / pseudo_ctor_symbol /
// r1_pseudo_ctor_pair / r2_qualified_unique / fill_candidates) moved to
// [`crate::symbol_resolve`] (CR-15 extraction) — chain_tour consumes
// them via the `use` at the top; behavior is frozen (the existing
// chain_tour suites are the gate).

impl GraphAnchor {
    /// CR-7 symbol-only backfill: unique same-file anchor by EXACT
    /// path+name. Strict identity (adversarial-review AC): no suffix LIKE,
    /// no nearest-line blind pick, no cross-file inference — zero or
    /// multiple candidates both fail closed; SQLite errors propagate loud
    /// instead of folding into a miss.
    ///
    /// CR-9 addendum (revises the CR-7 adversarial AC; not a "compatible"
    /// blur): multiple candidates MAY collapse to a fill only under
    /// independent verification, inside the CLOSED same-file same-name
    /// candidate set — (R1) the set is exactly {Type `X#`, pseudo-ctor
    /// `X().`} with equal line_start (the producer's constructor-call
    /// mint pairs one logical definition with two nodes; the fill stamps
    /// the Type symbol — canonicalization, not an equivalence claim over
    /// the two SCIP symbols), or (R2) the doc's qualified dotted name
    /// uniquely identifies exactly one candidate by structured
    /// scope-chain tail alignment. A bare (unqualified) doc token fills
    /// only two ways: R1, or R2 tier-1 exact full-chain equality —
    /// exactly one candidate whose complete scope chain IS the token
    /// itself (a top-level `X` whose chain is just `X`). Tier-2 never
    /// fires for a bare token: every same-file same-name candidate
    /// chain ends in that name, so once no chain equals the token
    /// outright, every chain is strictly longer AND tail-aligns — the
    /// tier-2 survivor set is the entire candidate set, a forced
    /// multi-hit. Everything else stays fail-closed ambiguous; the R3
    /// hint line surfaces the candidates to the author instead. A frame
    /// whose symbol remainder carries prose around the token (e.g.
    /// `see Cls.method`) is not eligible for disambiguation and stays
    /// ambiguous with hints — the whole-frame gate covers R1 and R2
    /// alike.
    /// Expected shift: `filled` counts move up from the CR-7 baseline
    /// exactly by the R1/R2 resolved frames; the `ambiguous` key keeps
    /// its semantics.
    ///
    /// Assumption (review F7): the graph
    /// db was produced for THIS repo checkout (in-repo `.code-reality/`
    /// producer face stores canonical absolute paths) — a db copied
    /// across checkouts degrades line-less anchors to `not-in-graph`
    /// while the explicit-line face keeps matching via its suffix LIKE;
    /// both directions are fail-safe.
    pub fn fill_anchor(
        &self,
        abs_path: &Path,
        ident: &str,
        ident_full: &str,
        whole: bool,
        qm: Option<&QualifiedMethod>,
    ) -> Result<FillOutcome, String> {
        if ident.is_empty() {
            return Ok(FillOutcome::NoAnchor("no-ident".into(), Vec::new()));
        }
        let rel = abs_path
            .strip_prefix(&self.repo_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let exact = format!("{}/{}", self.repo_root.display(), rel);
        let rows = same_file_rows(&self.conn, &exact, ident)?;
        // CR-18: a declared identity replaces the R1/R2/R3 machinery —
        // unique triple-match fills, refusals carry their own
        // `qualifier_*` reasons (never R1/R2 disambiguation, never a
        // nearest fallback).
        if let Some(qm) = qm {
            return Ok(match crate::symbol_resolve::qualified_select(&rows, qm) {
                crate::symbol_resolve::QualifiedVerdict::Unique { row } => {
                    FillOutcome::Filled(rows[row].0, Some(rows[row].1.clone()))
                }
                crate::symbol_resolve::QualifiedVerdict::Refused { reason, candidates } => {
                    FillOutcome::NoAnchor(reason.key().to_string(), candidates)
                }
            });
        }
        Ok(resolve_candidates(&rows, ident_full, whole))
    }
}

/// CR-11: a scenario whose every frame skipped (zero anchored steps) —
/// a shell. Collected by build_tours alongside the (unchanged)
/// per-scenario tour face; run() drops shells from the write set (no
/// .tour file, no manifest row), warns per scenario, and fails closed
/// when the whole md is shells.
#[derive(Clone)]
pub struct ShellScenario {
    pub title: String,
    pub frames: usize,
    pub skip_reasons: BTreeMap<String, i64>,
}

/// CR-17: one explicit-line trait-collision encounter — the frame's
/// same-file same-name candidate set carried ≥2 impl-shaped symbols
/// with the same inherent type and differing trait groups. `exact`
/// marks the resolution mode: the doc anchor line exactly hit one
/// candidate (anchor kept, collision disclosed on the step) vs no
/// unique exact hit (nearest-line guessing refused, frame skipped).
#[derive(Clone)]
pub struct TraitCollisionEncounter {
    pub frame: String,
    pub doc_anchor: String,
    pub exact: bool,
    pub candidates: Vec<FillCandidate>,
}

/// CR-18: one qualifier-refusal encounter — a frame whose declared
/// identity could not be confirmed (`qualifier_no_hit` /
/// `qualifier_ambiguous` / `qualifier_unverified` /
/// `qualifier_malformed`) or whose cross-file single-file convergence
/// hit an unresolved same-name trait collision
/// (`cross_file_trait_ambiguous`). `converged_file` names the
/// relocation target on the cross-file face. Disclosed on the same
/// three faces as the CR-17 payload: scenario description hint, kept-
/// step-adjacent skip reason, and one CLI WARN per encounter.
#[derive(Clone)]
pub struct QualifierRefusal {
    pub frame: String,
    pub doc_anchor: String,
    pub reason: String,
    pub converged_file: Option<String>,
    pub candidates: Vec<FillCandidate>,
}

pub struct ScenarioTours {
    pub tours: Vec<serde_json::Value>,
    pub frames: usize,
    pub skipped: usize,
    pub g_counts: BTreeMap<String, i64>,
    /// Shell scenarios (all frames skipped) in doc order — disclosed by
    /// run() instead of written
    pub shells_omitted: Vec<ShellScenario>,
    /// CR-17 explicit-line trait-collision encounters in doc order —
    /// disclosed by run() as one stdout WARN per encounter
    pub trait_collisions: Vec<TraitCollisionEncounter>,
    /// CR-18 qualifier refusals in doc order — same WARN discipline
    pub qualifier_refusals: Vec<QualifierRefusal>,
}

impl ScenarioTours {
    /// CR-11 write-set view: shell scenarios (zero anchored steps)
    /// dropped from `tours`. build_tours keeps every scenario in `tours`
    /// (the all-skip description stays the doc-truth record); the run()
    /// gate decides what lands on disk.
    fn without_shells(&self) -> ScenarioTours {
        ScenarioTours {
            tours: self
                .tours
                .iter()
                .filter(|t| {
                    t["steps"]
                        .as_array()
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
                })
                .cloned()
                .collect(),
            frames: self.frames,
            skipped: self.skipped,
            g_counts: self.g_counts.clone(),
            shells_omitted: self.shells_omitted.clone(),
            trait_collisions: self.trait_collisions.clone(),
            qualifier_refusals: self.qualifier_refusals.clone(),
        }
    }
}

#[allow(clippy::too_many_arguments)] // frame-field decomposition of the frozen dict walk
fn step_of(
    path: &Option<String>,
    line: Option<i64>,
    g: &GraphHit,
    status: &str,
    symbol: &str,
    note: &str,
    prefix: &str,
    abs_path: &Path,
    repo_root: &Path,
    lines_cache: &mut HashMap<String, Vec<String>>,
) -> serde_json::Value {
    let mut file_rel = abs_path
        .strip_prefix(repo_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut line = line.unwrap_or(1);
    let mut parts: Vec<String> = Vec::new();
    let anchor_disp = format!("文檔錨 {}:{line}", path.clone().unwrap_or_default());
    match g.g.as_str() {
        "filled" => {
            // CR-7: the doc carried no line — the coordinates above come
            // from the graph's unique same-file hit; no 文檔錨 fake line
            parts.push(format!("graph 補行號 :{line}（文檔無行號）"));
            parts.push(format!(
                "文檔錨 {}",
                path.clone().unwrap_or_default()
            ));
        }
        "moved" => {
            // delta vs the DOC anchor line — compute before reassigning
            let doc_line = line;
            line = g.g_line.unwrap_or(line);
            let delta = g.g_line.map(|l| l - doc_line).unwrap_or(0);
            parts.push(format!(
                "graph {}{delta} → :{}",
                if delta > 0 { "+" } else { "" },
                g.g_line.unwrap_or(line)
            ));
            parts.push(anchor_disp);
        }
        "moved-file" => {
            file_rel = g.g_file.clone().unwrap_or(file_rel);
            line = g.g_line.unwrap_or(1);
            parts.push(format!(
                "文檔錨 {}:{line}，已搬家 {}",
                path.clone().unwrap_or_default(),
                file_rel
            ));
        }
        "same" => {
            parts.push("graph ✓ 行號一致".into());
            parts.push(anchor_disp);
            if !g.g_trait_candidates.is_empty() {
                // CR-17: the anchor survived on exact-line evidence —
                // keep the colliding siblings visible on the step for
                // human review (tour_reanchor would otherwise carry the
                // picked identity as maintenance truth)
                parts.push(format!(
                    "trait 碰撞：候選 {}——行號精確命中，保留此錨；請人工確認 trait 語意",
                    trait_cands_inline(&g.g_trait_candidates)
                ));
            }
        }
        "moved-file-ambiguous" => {
            parts.push(format!(
                "同名多檔無法自動判定：{}",
                py_list_inline(&g.g_files)
            ));
            parts.push(anchor_disp);
        }
        "graph-read-error" => {
            // C1 (review): the candidate read failed mid-way — no
            // identity, no nearest guess; the doc anchor itself stays
            // the step coordinate (the same availability stance as
            // not-in-graph frames, with the degradation named)
            parts.push("graph 讀取錯誤——fail-closed 不錨定（僅文檔錨）".into());
            parts.push(anchor_disp);
        }
        _ if matches!(status, "drift" | "drift-far" | "missing") => {
            parts.push(format!("graph 未索引；substring 判定 {status}"));
            parts.push(anchor_disp);
        }
        _ => parts.push(anchor_disp),
    }
    if let Some(qn) = &g.g_qualifier_note {
        // CR-18: the anchor was selected by the author's declared
        // identity — the evidence line for why line drift still kept
        // this symbol
        parts.push(qn.clone());
    }
    if !note.is_empty() {
        parts.push(format!("註：{note}"));
    }
    let mut step = serde_json::json!({
        "file": file_rel,
        "line": line,
        "title": format!("{prefix}{symbol}"),
        "description": parts.join("\n"),
    });
    // pattern: read the FINAL file/line (moved/moved-file re-anchored
    // coordinates); same-file cache; unreadable/out-of-range/blank → no
    // emission (delta-omission semantics alignment). split('\n') not
    // splitlines: graph line_start counts only \n
    let p = repo_root.join(&file_rel);
    let key = p.to_string_lossy().replace('\\', "/");
    lines_cache.entry(key).or_insert_with(|| {
        std::fs::read_to_string(&p)
            .map(|t| t.split('\n').map(|s| s.to_string()).collect())
            .unwrap_or_default()
    });
    if let Some(lines) = lines_cache.get(&p.to_string_lossy().replace('\\', "/")) {
        if line >= 1
            && ((line - 1) as usize) < lines.len()
            && !lines[(line - 1) as usize].trim().is_empty()
        {
            step.as_object_mut().unwrap().insert(
                "pattern".into(),
                serde_json::Value::String(anchor_pattern(&lines[(line - 1) as usize])),
            );
        }
    }
    // namespaced CR identity (AIR-80): the true SCIP symbol of the selected
    // definition occurrence — CR reanchor maintenance reads this instead of
    // re-deriving identity from file+name+nearest-line. Absent (not-in-graph
    // / ambiguous / legacy rows) → field omitted; consumers must tolerate.
    if let Some(sym) = &g.g_symbol {
        if !sym.is_empty() {
            step.as_object_mut()
                .unwrap()
                .insert("x-codeReality".into(), serde_json::json!({ "symbol": sym }));
        }
    }
    step
}

fn py_list_inline(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}

/// CR-9 R3: the tour-description candidate hint line for frames that
/// stayed ambiguous after R1/R2. One line at the description tail with
/// the fixed prefix 「歧義候選：」; per frame at most 4 candidates
/// (ellipsis marker when truncated), per scenario at most 3 frames
/// detailed with the rest folded into an explicit remaining count.
/// Candidates render descriptor + line + kind suffix, preserving the
/// fill query's line_start,symbol order. Two shapes stay banned in the
/// rendering: markdown-link form `[..](..)` — the shape
/// tour_validate's `file_ref()` actually counts in tour descriptions —
/// and `path.py:NN` form, which chain_tour's own `ref_re` parse face
/// would read back as a file:line anchor. An empty frame list yields
/// an empty string: tours with no ambiguity keep byte-identical
/// descriptions.
fn ambig_hint_line(frames: &[(String, Vec<FillCandidate>)]) -> String {
    if frames.is_empty() {
        return String::new();
    }
    let mut segs: Vec<String> = Vec::new();
    for (sym, cands) in frames.iter().take(3) {
        let mut parts: Vec<String> = cands
            .iter()
            .take(4)
            .map(|c| format!("{} :{}（{}）", c.descriptor, c.line, c.kind))
            .collect();
        if cands.len() > 4 {
            parts.push("…".into());
        }
        segs.push(format!("{sym} → {}", parts.join("／")));
    }
    if frames.len() > 3 {
        segs.push(format!("另有 {} 幀（略）", frames.len() - 3));
    }
    format!("歧義候選：{}", segs.join("；"))
}

/// CR-17 candidate list rendering (`descriptor :line（kind）`, first 4
/// shown + an explicit omitted count when truncated — the R3 segment
/// shape) — shared by the kept-step note, the tour-description hint
/// line and the CLI WARN so every face shows the identical list.
/// Descriptor brackets render full-width (`[`→`［`, `]`→`］`): ASCII
/// brackets in a step description parse back as a markdown reference
/// link (`[Debug][Price]` shape) and FAIL tour_validate's check_links —
/// the escaped form is link-inert while `x-codeReality.symbol` keeps
/// the raw SCIP symbol untouched.
fn trait_cands_inline(cands: &[FillCandidate]) -> String {
    let mut parts: Vec<String> = cands
        .iter()
        .take(4)
        .map(|c| {
            format!(
                "{} :{}（{}）",
                c.descriptor.replace('[', "［").replace(']', "］"),
                c.line,
                c.kind
            )
        })
        .collect();
    if cands.len() > 4 {
        parts.push(format!("…（另有 {} 候選）", cands.len() - 4));
    }
    parts.join("／")
}

/// CR-17: the tour-description trait-collision line — mirrors the R3
/// hint caps (≤3 frames detailed with an explicit fold count, first 4
/// candidates each with an explicit omitted count) while keeping the
/// explicit-line specifics: the doc
/// anchor and the resolution mode per frame. Same shape bans as the R3
/// rendering (no markdown-link / `path:NN` forms — the candidates are
/// descriptor+line, the anchor is already a parsed doc coordinate). An
/// empty encounter list yields an empty string: tours without
/// collisions keep byte-identical descriptions.
fn trait_hint_line(encounters: &[TraitCollisionEncounter]) -> String {
    if encounters.is_empty() {
        return String::new();
    }
    let mut segs: Vec<String> = Vec::new();
    for e in encounters.iter().take(3) {
        let mode = if e.exact {
            "行號精確命中，保留該錨"
        } else {
            "行號未精確命中，拒絕最近猜測"
        };
        segs.push(format!(
            "{}（文檔錨 {}）→ {}（{mode}）",
            e.frame,
            e.doc_anchor,
            trait_cands_inline(&e.candidates)
        ));
    }
    if encounters.len() > 3 {
        segs.push(format!("另有 {} 幀（略）", encounters.len() - 3));
    }
    format!("trait 碰撞候選：{}", segs.join("；"))
}

/// CR-17 per-encounter WARN lines — one per explicit-line trait
/// collision: the candidates plus the resolution mode. Rides stdout in
/// the success face (the [OK]/[LOG] report section) and stderr in the
/// all-shell fail-closed face.
fn trait_warn_lines(encounters: &[TraitCollisionEncounter]) -> Vec<String> {
    encounters
        .iter()
        .map(|e| {
            let cands = trait_cands_inline(&e.candidates);
            if e.exact {
                format!(
                    "[WARN] trait 碰撞：{}（文檔錨 {}）候選 {cands}——文檔錨行號精確命中，保留該錨；請人工確認 trait 語意（explicit_trait_ambiguous）",
                    e.frame, e.doc_anchor
                )
            } else {
                format!(
                    "[WARN] trait 碰撞：{}（文檔錨 {}）候選 {cands}——行號已漂移（未精確命中），拒絕最近行號猜測，該幀不產生錨定（explicit_trait_ambiguous）",
                    e.frame, e.doc_anchor
                )
            }
        })
        .collect()
}

/// CR-18: does this graph-distribution key belong to the qualifier
/// refusal family — the states that must SKIP in build_tours (even
/// with an explicit doc line) and disclose on the WARN faces?
fn is_qualifier_refusal(g: &str) -> bool {
    matches!(
        g,
        "qualifier_no_hit"
            | "qualifier_ambiguous"
            | "qualifier_malformed"
            | "qualifier_unverified"
            | "cross_file_trait_ambiguous"
    )
}

/// CR-18 refusal reason → the human mode description shared by the
/// WARN line and the description hint (one source, three faces).
fn qualifier_mode_desc(reason: &str) -> &'static str {
    match reason {
        "qualifier_no_hit" => "候選皆不符合宣告身分，拒絕最近猜測",
        "qualifier_ambiguous" => "多候選符合宣告身分，拒絕猜測",
        "qualifier_unverified" => "存在無法剖析的 impl 候選，唯一命中未證實",
        "qualifier_malformed" => "qualifier 語法無法剖析（須 Receiver::method[Trait] 形）",
        "cross_file_trait_ambiguous" => "跨檔收斂後同名 trait 碰撞未消歧，拒絕最低行猜測",
        _ => "拒絕錨定",
    }
}

/// CR-18: the tour-description qualifier-refusal line — mirrors the
/// CR-17 trait hint caps (≤3 frames detailed with an explicit fold
/// count, first 4 candidates each via the shared renderer). An empty
/// refusal list yields an empty string: tours without qualifier
/// refusals keep byte-identical descriptions.
fn qualifier_hint_line(refusals: &[QualifierRefusal]) -> String {
    if refusals.is_empty() {
        return String::new();
    }
    let mut segs: Vec<String> = Vec::new();
    for e in refusals.iter().take(3) {
        let anchor = if e.doc_anchor.is_empty() {
            String::new()
        } else {
            format!("（文檔錨 {}）", e.doc_anchor)
        };
        let file = e
            .converged_file
            .as_deref()
            .filter(|f| !f.is_empty())
            .map(|f| format!("（收斂至 {f}）"))
            .unwrap_or_default();
        let cands = if e.candidates.is_empty() {
            String::new()
        } else {
            format!("候選 {}——", trait_cands_inline(&e.candidates))
        };
        segs.push(format!(
            "{}{anchor}{file}：{cands}{}（{}）",
            e.frame,
            qualifier_mode_desc(&e.reason),
            e.reason
        ));
    }
    if refusals.len() > 3 {
        segs.push(format!("另有 {} 幀（略）", refusals.len() - 3));
    }
    format!("qualifier 拒絕：{}", segs.join("；"))
}

/// CR-18 per-encounter WARN lines — one per qualifier refusal, the
/// candidate payload identical to the description hint (one renderer).
/// Rides stdout in the success face and stderr in the all-shell
/// fail-closed face, like the CR-17 WARNs.
fn qualifier_warn_lines(refusals: &[QualifierRefusal]) -> Vec<String> {
    refusals
        .iter()
        .map(|e| {
            let anchor = if e.doc_anchor.is_empty() {
                String::new()
            } else {
                format!("（文檔錨 {}）", e.doc_anchor)
            };
            let file = e
                .converged_file
                .as_deref()
                .filter(|f| !f.is_empty())
                .map(|f| format!("（收斂至 {f}）"))
                .unwrap_or_default();
            let cands = if e.candidates.is_empty() {
                String::new()
            } else {
                format!("候選 {}——", trait_cands_inline(&e.candidates))
            };
            format!(
                "[WARN] qualifier 拒絕：{}{anchor}{file}：{cands}{}（{}）",
                e.frame,
                qualifier_mode_desc(&e.reason),
                e.reason
            )
        })
        .collect()
}

/// chain md → one tour per scenario (`chain_tour.py:331-404`).
pub fn build_tours(
    chain_md: &Path,
    repo_root: &Path,
    graph_db: Option<&Path>,
) -> Result<ScenarioTours, String> {
    let repo_root = crate::common::resolve(repo_root);
    let text = std::fs::read_to_string(chain_md)
        .map_err(|e| format!("{} 讀取失敗：{e}", chain_md.display()))?;
    let blocks = parse_blocks(&text);
    if blocks.is_empty() {
        return Err(format!(
            "非 callstack 文檔格式（無樹狀 code block——需含 ├/└ 幀行）：{}；參考 ai-analysis/blueprint/callstack-v1/ 慣例",
            chain_md.display()
        ));
    }
    let mut resolver = PathResolver::new(&repo_root)?;
    let m0 = graph_db.map(db_mtime_ns).transpose()?;
    let ga = match graph_db {
        Some(db) => Some(GraphAnchor::new(db, &repo_root)?),
        None => None,
    };
    let mut tours: Vec<serde_json::Value> = Vec::new();
    let mut lines_cache: HashMap<String, Vec<String>> = HashMap::new();
    let (mut frames_total, mut skipped_total) = (0usize, 0usize);
    let mut g_counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut shells_omitted: Vec<ShellScenario> = Vec::new();
    // CR-17: explicit-line trait-collision encounters in doc order —
    // the CLI WARN payload
    let mut trait_encounters: Vec<TraitCollisionEncounter> = Vec::new();
    // CR-18: qualifier refusals in doc order — the CLI WARN payload
    let mut qualifier_refusals: Vec<QualifierRefusal> = Vec::new();
    for (heading, lines) in &blocks {
        let frames = parse_frames(lines);
        let mut steps: Vec<serde_json::Value> = Vec::new();
        let mut skipped = 0usize;
        let mut skip_examples: Vec<String> = Vec::new();
        let mut skip_reasons: BTreeMap<String, i64> = BTreeMap::new();
        let mut scen_g: BTreeMap<String, i64> = BTreeMap::new();
        // CR-9 R3: frames that stayed ambiguous after R1/R2, with their
        // renderable candidates, in doc-frame order
        let mut ambig_frames: Vec<(String, Vec<FillCandidate>)> = Vec::new();
        // CR-17: this scenario's explicit-line trait collisions (doc
        // order) — the description hint payload
        let mut trait_frames: Vec<TraitCollisionEncounter> = Vec::new();
        // CR-18: this scenario's qualifier refusals (doc order) — the
        // description hint + CLI WARN payload
        let mut qualifier_frames: Vec<QualifierRefusal> = Vec::new();
        for f in &frames {
            let mut status = "noref".to_string();
            let mut g = GraphHit {
                g: "noref".into(),
                ..Default::default()
            };
            let mut abs_path: Option<PathBuf> = None;
            let mut filled_line: Option<i64> = None;
            // COD-1 whole-frame gate: only a remainder that is exactly
            // the anchor token (plus one trailing parameter group) may
            // spend R1/R2 disambiguation — prose stays ambiguous
            let whole = !f.ident_full.is_empty() && whole_frame_token(&f.symbol) == f.ident_full;
            // CR-18: the declared-identity query triple (None on the
            // legacy face)
            let qm: Option<&QualifiedMethod> = match &f.qualifier {
                FrameQualifier::WellFormed(qm) => Some(qm),
                _ => None,
            };
            if matches!(f.qualifier, FrameQualifier::Malformed) {
                // CR-18: qualifier-looking syntax that does not parse is
                // a doc defect — the frame skips before any anchoring
                // (an explicit line number never rescues it), disclosed
                // on the same three faces as every other refusal
                let key = "qualifier_malformed";
                *g_counts.entry(key.into()).or_insert(0) += 1;
                *scen_g.entry(key.into()).or_insert(0) += 1;
                qualifier_frames.push(QualifierRefusal {
                    frame: f.symbol.chars().take(30).collect(),
                    doc_anchor: match (&f.path, f.line) {
                        (Some(p), Some(l)) => format!("{p}:{l}"),
                        (Some(p), None) => p.clone(),
                        _ => String::new(),
                    },
                    reason: key.into(),
                    converged_file: None,
                    candidates: Vec::new(),
                });
                skipped += 1;
                *skip_reasons.entry(key.into()).or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            }
            if let Some(p) = &f.path {
                let (resolved, kind) = resolver.resolve(p);
                match resolved {
                    None => {
                        status = if kind == "none" {
                            "external".into()
                        } else {
                            "unresolved".into()
                        }
                    }
                    Some(ap) => {
                        if f.line.is_none() {
                            // CR-7 line-less: strict fill or typed skip —
                            // never a pseudo coordinate
                            match ga.as_ref() {
                                None => {
                                    g = GraphHit {
                                        g: "graph-unavailable".into(),
                                        ..Default::default()
                                    };
                                }
                                Some(ga) => {
                                    match ga.fill_anchor(&ap, &f.ident, &f.ident_full, whole, qm)? {
                                        FillOutcome::Filled(line, symbol) => {
                                            // review F1: the fill is only as
                                            // good as its target line — blank,
                                            // out-of-range, or unreadable
                                            // targets downgrade to a typed
                                            // skip (tour_validate skips an
                                            // absent pattern silently, so the
                                            // gate must live here)
                                            let key = ap.to_string_lossy().replace('\\', "/");
                                            let target_ok = {
                                                let lines = lines_cache
                                                    .entry(key)
                                                    .or_insert_with(|| {
                                                        std::fs::read_to_string(&ap)
                                                            .map(|t| {
                                                                t.split('\n')
                                                                    .map(|s| s.to_string())
                                                                    .collect()
                                                            })
                                                            .unwrap_or_default()
                                                    });
                                                line >= 1
                                                    && ((line - 1) as usize) < lines.len()
                                                    && !lines[(line - 1) as usize]
                                                        .trim()
                                                        .is_empty()
                                            };
                                            if target_ok {
                                                status = "filled".into();
                                                g = GraphHit {
                                                    g: "filled".into(),
                                                    g_line: Some(line),
                                                    g_file: None,
                                                    g_files: Vec::new(),
                                                    g_symbol: symbol,
                                                    g_trait_candidates: Vec::new(),
                                                    g_qualifier_note: None,
                                                    g_qualifier_candidates: Vec::new(),
                                                };
                                                filled_line = Some(line);
                                            } else {
                                                g = GraphHit {
                                                    g: "invalid-line".into(),
                                                    ..Default::default()
                                                };
                                            }
                                        }
                                        FillOutcome::NoAnchor(reason, cands) => {
                                            if reason == "ambiguous" && !cands.is_empty() {
                                                ambig_frames.push((
                                                    f.symbol.chars().take(30).collect(),
                                                    cands,
                                                ));
                                                g = GraphHit {
                                                    g: reason,
                                                    ..Default::default()
                                                };
                                            } else {
                                                // CR-18 review (F-2): a
                                                // qualifier-refusal reason
                                                // must carry its candidate
                                                // set through to the
                                                // three-face disclosure —
                                                // `NoAnchor(reason, cands)`
                                                // dropping cands left
                                                // `g_qualifier_candidates`
                                                // permanently empty on the
                                                // line-less face (the WARN
                                                // and description renderers
                                                // print only non-empty
                                                // candidate lists). Legacy
                                                // non-refusal reasons keep
                                                // the empty payload,
                                                // byte-identical.
                                                g = GraphHit {
                                                    g_qualifier_candidates:
                                                        if is_qualifier_refusal(&reason) {
                                                            cands
                                                        } else {
                                                            Vec::new()
                                                        },
                                                    g: reason,
                                                    ..Default::default()
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            status = check_anchor(&ap, f.line.unwrap_or(0), &f.ident);
                            if let Some(ga) = &ga {
                                if !f.ident.is_empty() {
                                    g = ga.anchor(&ap, f.line.unwrap_or(0), &f.ident, &status, qm);
                                }
                            }
                        }
                        abs_path = Some(ap);
                    }
                }
            }
            *g_counts.entry(g.g.clone()).or_insert(0) += 1;
            *scen_g.entry(g.g.clone()).or_insert(0) += 1;
            if !g.g_trait_candidates.is_empty() {
                // CR-17: record the encounter for the description hint
                // and the CLI WARN. An anchor kept on exact-line
                // evidence additionally bumps the independent collision
                // counter (the distribution shows it alongside `same`);
                // refused frames already carry it as their g key.
                if g.g == "same" {
                    *g_counts
                        .entry("explicit_trait_ambiguous".into())
                        .or_insert(0) += 1;
                    *scen_g.entry("explicit_trait_ambiguous".into()).or_insert(0) += 1;
                }
                trait_frames.push(TraitCollisionEncounter {
                    frame: f.symbol.chars().take(30).collect(),
                    doc_anchor: format!(
                        "{}:{}",
                        f.path.clone().unwrap_or_default(),
                        f.line.unwrap_or(0)
                    ),
                    exact: g.g == "same",
                    candidates: g.g_trait_candidates.clone(),
                });
            }
            // CR-18: a qualifier-selected success gets its evidence note
            // (counts under the legacy same/moved/filled/moved-file keys
            // — Plan ⑨'s comparability)
            if let Some(qm) = qm {
                if matches!(g.g.as_str(), "same" | "moved" | "filled" | "moved-file") {
                    g.g_qualifier_note = Some(format!(
                        "qualifier 命中 {}::{}［{}］——宣告式身分選取（行號漂移亦選中）",
                        qm.receiver, qm.method, qm.trait_name
                    ));
                }
            }
            // CR-18 rejected-outcome gate (Plan ⑨): every refusal state
            // skips — an explicit doc line never rescues the frame into
            // step_of — and discloses on the same three faces (g_counts,
            // skip reasons, WARN) as one payload
            if is_qualifier_refusal(&g.g) {
                qualifier_frames.push(QualifierRefusal {
                    frame: f.symbol.chars().take(30).collect(),
                    doc_anchor: match (&f.path, f.line) {
                        (Some(p), Some(l)) => format!("{p}:{l}"),
                        (Some(p), None) => p.clone(),
                        _ => String::new(),
                    },
                    reason: g.g.clone(),
                    converged_file: g.g_file.clone(),
                    candidates: g.g_qualifier_candidates.clone(),
                });
                skipped += 1;
                *skip_reasons.entry(g.g.clone()).or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            }
            let Some(ap) = abs_path else {
                skipped += 1;
                *skip_reasons.entry(status.clone()).or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            };
            if g.g == "explicit_trait_ambiguous" {
                // CR-17 fail-closed: the doc line has drifted off every
                // candidate of a trait-colliding set — a nearest-line
                // pick would mint an unverified SCIP identity. No step;
                // the reason rides the skip axis, the candidates ride
                // the description hint + CLI WARN.
                skipped += 1;
                *skip_reasons
                    .entry("explicit_trait_ambiguous".into())
                    .or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            }
            if f.line.is_none() && status != "filled" {
                // CR-7: path resolved but no verified fill — typed skip
                // (the reason lives on the graph-distribution axis), no
                // pseudo step
                skipped += 1;
                *skip_reasons.entry("noanchor".into()).or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            }
            steps.push(step_of(
                &f.path,
                if status == "filled" { filled_line } else { f.line },
                &g,
                &status,
                &f.symbol,
                &f.note,
                &f.prefix,
                &ap,
                &repo_root,
                &mut lines_cache,
            ));
        }
        if steps.is_empty() {
            // CR-11: zero anchored frames — a shell scenario. No
            // frame-count floor: a 1-2-frame real-anchor scenario is
            // legal; all-skip is the precise shell criterion.
            shells_omitted.push(ShellScenario {
                title: heading.clone(),
                frames: frames.len(),
                skip_reasons: skip_reasons.clone(),
            });
        }
        let dist: Vec<String> = scen_g.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        let reasons = if skip_reasons.is_empty() {
            "無".to_string()
        } else {
            skip_reasons
                .iter()
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<_>>()
                .join("／")
        };
        let examples = if skip_examples.is_empty() {
            "無".to_string()
        } else {
            skip_examples.join("、")
        };
        let mut description = format!(
            "{} 幀 → {} 步；{skipped} 幀跳過（{reasons}——例：{examples}）。\ngraph 重錨分佈：{}",
            frames.len(),
            steps.len(),
            dist.join(" ")
        );
        let hint = ambig_hint_line(&ambig_frames);
        if !hint.is_empty() {
            description.push('\n');
            description.push_str(&hint);
        }
        let thint = trait_hint_line(&trait_frames);
        if !thint.is_empty() {
            description.push('\n');
            description.push_str(&thint);
        }
        let qhint = qualifier_hint_line(&qualifier_frames);
        if !qhint.is_empty() {
            description.push('\n');
            description.push_str(&qhint);
        }
        trait_encounters.append(&mut trait_frames);
        qualifier_refusals.append(&mut qualifier_frames);
        tours.push(serde_json::json!({
            "title": heading,
            "description": description,
            "steps": steps,
        }));
        frames_total += frames.len();
        skipped_total += skipped;
    }
    drop(ga);
    if let (Some(db), Some(m)) = (graph_db, m0) {
        assert_db_unchanged(db, m)?;
    }
    Ok(ScenarioTours {
        tours,
        frames: frames_total,
        skipped: skipped_total,
        g_counts,
        shells_omitted,
        trait_collisions: trait_encounters,
        qualifier_refusals,
    })
}

/// Write tours: `{SS}.tour` in-family sequence-number filenames (SS =
/// scenario order within the family — init_scan's generator guess depends
/// on the bare numeric names); titles follow the family template
/// `{NN} - {label}｜{heading}` (`｜` = U+FF5C; the `NN - ` prefix keeps
/// upstream getTourTitle / Prev/Next / ts_key regexes matching). NN = the
/// `^\d{2,}` prefix of the FINAL out_dir basename (post dup-family
/// redirect — resolved here so the caller's redirect is inherently
/// honored); label = the manifest `[family."NN"]` label (read-only via
/// the tour_manifest extra roundtrip) or the basename `NN-` suffix —
/// both present and disagreeing fails loud (drift must not be
/// auto-resolved), both absent folds to `{NN} - {heading}` with a WARN.
/// A corrupted-but-present manifest (unparseable, or the label of the
/// wrong TOML type) is NOT absence: the label still folds, but the WARN
/// names the real cause (manifest-corruption face, CR-2 leg-3 F1a).
/// A basename without a family number degrades to the legacy
/// `{SS} - {heading}` title plus a WARN naming the fix. Primary members
/// carry isPrimary.
pub fn write_tours(
    st: &ScenarioTours,
    out_dir: &Path,
    primary: &std::collections::BTreeSet<usize>,
    warns: &mut Vec<String>,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{} 建立失敗：{e}", out_dir.display()))?;
    let base = out_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nn = crate::tour_validate::nn_prefix(&base);
    let label: Option<String> = match &nn {
        Some(nn) => {
            let manifest_label = manifest_family_label(out_dir, nn);
            let suffix_label = base
                .strip_prefix(nn.as_str())
                .and_then(|rest| rest.strip_prefix('-'))
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            match (manifest_label, suffix_label) {
                (ManifestFamilyLabel::Label(m), Some(s)) => {
                    // F5 adjudication (CR-2 leg-3): the dual-source check
                    // is exact PRE-NORMALIZATION equality — sources are
                    // compared verbatim (the fullwidth-hyphen label
                    // sanitization below runs only after this gate).
                    // Deliberately conservative: a normalized comparison
                    // could auto-reconcile drift that must fail loud.
                    if m != s {
                        return Err(format!(
                            "族 {nn} label 雙源不一致：manifest [family.\"{nn}\"] label=\"{m}\" ≠ 目錄 {nn}- 後綴 \"{s}\"——drift 禁自動擇一，對齊後重產"
                        ));
                    }
                    Some(m)
                }
                (ManifestFamilyLabel::Label(m), None) => Some(m),
                (ManifestFamilyLabel::Absent, Some(s)) => Some(s),
                (ManifestFamilyLabel::Absent, None) => {
                    warns.push(format!(
                        "[WARN] 族 {nn} 無 label（manifest [family.\"{nn}\"] 與目錄 {nn}- 後綴皆缺）——title 收摺「{nn} - 場景標題」；修法：目錄改名 {nn}-label 或 manifest 補 [family.\"{nn}\"] label"
                    ));
                    None
                }
                (ManifestFamilyLabel::Unreadable(e), suffix) => {
                    if let Some(s) = &suffix {
                        warns.push(format!(
                            "[WARN] manifest 不可讀（{e}）——族 {nn} label 僅取目錄 {nn}- 後綴 \"{s}\"（manifest 在但解析失敗，雙源仲裁待修復後重產）"
                        ));
                        suffix
                    } else {
                        warns.push(format!(
                            "[WARN] manifest 不可讀（{e}）——族 {nn} label 無可讀源，title 收摺「{nn} - 場景標題」（manifest 在但解析失敗，非 label 雙源缺失）；修法：修復 manifest 補 [family.\"{nn}\"] label 或目錄改名 {nn}-label"
                        ));
                        None
                    }
                }
                (ManifestFamilyLabel::Malformed, suffix) => {
                    // C3 (leg-4): Malformed covers three distinct causes
                    // (family key not a table / NN row not a table / label
                    // not a string) — the WARN enumerates them instead of
                    // asserting the label-type cause it cannot know
                    if let Some(s) = &suffix {
                        warns.push(format!(
                            "[WARN] manifest [family.\"{nn}\"] 結構型別錯（family 非 table／NN 列非 table／label 非 string 之一）——族 {nn} label 僅取目錄 {nn}- 後綴 \"{s}\""
                        ));
                        suffix
                    } else {
                        warns.push(format!(
                            "[WARN] manifest [family.\"{nn}\"] 結構型別錯（family 非 table／NN 列非 table／label 非 string 之一）——title 收摺「{nn} - 場景標題」（label 源在但結構錯，非 label 雙源缺失）；修法：修復 [family.\"{nn}\"] 結構（family/列 table、label string）或目錄改名 {nn}-label"
                        ));
                        None
                    }
                }
            }
        }
        None => {
            warns.push(format!(
                "[WARN] out_dir {} basename 無族號前綴（^\\d{{2,}}）——title 退化舊格式「SS - 場景標題」（CodeTour 樹無法依族聚合）；修法：chain md 改名 NN-label.md（預設 out-dir 取 stem）或顯式 --out-dir .tours/arch/NN-label",
                out_dir.display()
            ));
            None
        }
    };
    let label = label.map(|l| {
        if l.contains('-') {
            warns.push(format!(
                "[WARN] 族 label「{l}」含 ASCII '-'——已代換全形「－」（上游 getTourTitle 於首 '-' 截斷）"
            ));
            l.replace('-', "－")
        } else {
            l
        }
    });
    let mut paths = Vec::new();
    for (i, tour) in st.tours.iter().enumerate() {
        let n = i + 1;
        let p = out_dir.join(format!("{n:02}.tour"));
        let heading = tour["title"].as_str().unwrap_or("");
        let title = match (&nn, &label) {
            (Some(nn), Some(label)) => format!("{nn} - {label}｜{heading}"),
            (Some(nn), None) => format!("{nn} - {heading}"),
            (None, _) => format!("{n:02} - {heading}"),
        };
        if nn.is_some() && heading.contains('-') {
            // author prose is never rewritten; disclose what upstream
            // getTourTitle would truncate the link key to
            let effective = crate::tour_validate::ts_key(&title);
            warns.push(format!(
                "[WARN] 場景標題「{heading}」含 ASCII '-'——作者散文原樣保留；上游 getTourTitle 有效截斷鍵＝「{effective}」"
            ));
        }
        let mut emitted = tour.clone();
        emitted
            .as_object_mut()
            .unwrap()
            .insert("title".into(), serde_json::Value::String(title));
        if primary.contains(&n) {
            emitted
                .as_object_mut()
                .unwrap()
                .insert("isPrimary".into(), serde_json::Value::Bool(true));
        }
        std::fs::write(&p, crate::common::to_json_indent1(&emitted))
            .map_err(|e| format!("{} 寫入失敗：{e}", p.display()))?;
        paths.push(p);
    }
    // legacy filename residue warning
    let mut opts = glob::MatchOptions::new();
    opts.require_literal_leading_dot = false;
    let legacy: Vec<PathBuf> =
        glob::glob_with(&out_dir.join("chain-*.tour").to_string_lossy(), opts)
            .into_iter()
            .flatten()
            .filter_map(|r| r.ok())
            .collect();
    let mut legacy_sorted = legacy;
    legacy_sorted.sort();
    if !legacy_sorted.is_empty() {
        // rides the warns vec (R2, CR-2 leg-4): run() routes it to stderr
        // like every other WARN, and in-process consumers see it too
        warns.push(format!(
            "[WARN] 舊檔名格式殘留 {} 檔（chain-*.tour）——新舊同 title 會使 player 撞鍵靜默落第一條（corpus 靜默雙份）；重錨過渡＝刪舊檔＋manifest 重建（rm {}/chain-*.tour 後重產或 init-scan）",
            legacy_sorted.len(),
            out_dir.display()
        ));
    }
    Ok(paths)
}

/// Manifest `[family."NN"]` label probe (via the tour_manifest::load
/// pipe — unknown-key extra roundtrip, read-only; the generator never
/// writes family rows). Four states so the fold WARN can tell the truth
/// (CR-2 leg-3 F1a): an unreadable manifest or a wrong-typed label is a
/// corruption face, NOT "both label sources absent".
enum ManifestFamilyLabel {
    /// `label = "..."` under `[family."NN"]`
    Label(String),
    /// No manifest tree/file, no family table, no NN row, or no label
    /// key — the honest "label source absent"
    Absent,
    /// Manifest present but load fails (read or TOML parse error);
    /// carries the load error text for the WARN
    Unreadable(String),
    /// family key/NN row not a table, or label present but not a string
    Malformed,
}

/// Read the manifest `[family."NN"]` label, keeping the corruption faces
/// distinguishable. Out-of-tree or label-less manifests yield `Absent`.
fn manifest_family_label(out_dir: &Path, nn: &str) -> ManifestFamilyLabel {
    let root = crate::tour_manifest::tours_root_of(out_dir);
    if root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .as_deref()
        != Some(".tours")
    {
        return ManifestFamilyLabel::Absent;
    }
    let m = match crate::tour_manifest::load(&root.join("manifest.toml")) {
        Ok(m) => m,
        Err(e) => return ManifestFamilyLabel::Unreadable(e),
    };
    for (k, v) in &m.extra {
        if k == "family" {
            let Some(t) = v.as_table() else {
                return ManifestFamilyLabel::Malformed;
            };
            let Some(row) = t.get(nn) else {
                return ManifestFamilyLabel::Absent;
            };
            let Some(row_t) = row.as_table() else {
                return ManifestFamilyLabel::Malformed;
            };
            let Some(label) = row_t.get("label") else {
                return ManifestFamilyLabel::Absent;
            };
            let Some(s) = label.as_str() else {
                return ManifestFamilyLabel::Malformed;
            };
            return ManifestFamilyLabel::Label(s.to_string());
        }
    }
    ManifestFamilyLabel::Absent
}

/// CR-11 per-shell-scenario WARN line — 族名＋場景標題＋幀數＋skip 原因
/// 分佈. Shared by the partial face (stderr WARN, run continues) and the
/// all-shell face (the WARN lines ride the fail-closed stderr).
fn shell_warn_lines(fam: &str, shells: &[ShellScenario]) -> Vec<String> {
    shells
        .iter()
        .map(|sh| {
            let reasons = if sh.skip_reasons.is_empty() {
                "無".to_string()
            } else {
                sh.skip_reasons
                    .iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect::<Vec<_>>()
                    .join("／")
            };
            format!(
                "[WARN] 族 {fam} 場景「{title}」{n} 幀全 skip（零可解析幀：{reasons}）——殼省略：不寫 .tour、不進 manifest",
                title = sh.title,
                n = sh.frames,
                reasons = reasons
            )
        })
        .collect()
}

/// Route a `code-reality chain_tour ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 chain_tour");
    };
    let spec = ToolSpec {
        flags: &[
            FlagSpec {
                long: "--graph",
                short: None,
                kind: Kind::Value { metavar: "GRAPH" },
            },
            FlagSpec {
                long: "--repo",
                short: None,
                kind: Kind::Value { metavar: "REPO" },
            },
            FlagSpec {
                long: "--out-dir",
                short: None,
                kind: Kind::Value { metavar: "OUT_DIR" },
            },
            FlagSpec {
                long: "--primary",
                short: None,
                kind: Kind::Value { metavar: "PRIMARY" },
            },
        ],
        positionals: &["chain_md"],
    };
    let (values, positionals) = match parse(&spec, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: concat!(
                    "usage: chain_tour [-h] [--graph GRAPH] [--repo REPO]\n",
                    "                 [--out-dir OUT_DIR] [--primary PRIMARY] chain_md\n",
                    "\n",
                    "chain tour——callchain 文檔 → 每場景一條 CodeTour `.tour`。\n",
                    "\n",
                    "positional arguments:\n",
                    "  chain_md              callchain 文檔（樹狀 code block）\n",
                    "\n",
                    "options:\n",
                    "  -h, --help            show this help message and exit\n",
                    "  --graph GRAPH         graph.db 覆寫（預設 <repo>/.code-reality/graph.db；不存在則退化純文檔錨）\n",
                    "  --repo REPO           repo 根\n",
                    "  --out-dir OUT_DIR     輸出目錄（預設 .tours/arch/<文檔 stem>/——stem 即 subgroup label）\n",
                    "  --primary PRIMARY     標 isPrimary 的場景編號（1-based 逗號分隔；md 場景序，殼省略場景也計入）；預設不標\n",
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok {
            values,
            positionals,
        } => (values, positionals),
    };
    let chain_md = PathBuf::from(&positionals[0]);
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut stdout = String::new();
    let graph_db: Option<PathBuf> = match values.get("--graph").and_then(|v| v.clone()) {
        Some(g) => {
            let p = PathBuf::from(g);
            if !p.exists() {
                return ToolOutput::crash(format!("--graph 指定但不存在：{}", p.display()));
            }
            Some(p)
        }
        None => {
            let (default_db, warns) = crate::graph_db::consumer_db(&repo);
            for w in warns {
                stdout.push_str(&w);
            }
            default_db
        }
    };
    let explicit_out_dir = values.get("--out-dir").and_then(|v| v.clone()).is_some();
    let mut out_dir = values
        .get("--out-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let stem = chain_md
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            PathBuf::from(".tours").join("arch").join(stem)
        });
    // Duplicate-family guard (mosaic relay 2026-08-26, 204->219 incident):
    // a regen whose source md already feeds a DIFFERENT family in the
    // manifest must upsert that family instead of silently creating a
    // stem duplicate whose old tours keep stale patterns. Explicit
    // --out-dir wins (rename migration uses it) but still warns.
    let repo_abs = crate::common::resolve(&repo);
    let chain_abs = crate::common::resolve(&chain_md);
    let src_rel = chain_abs
        .strip_prefix(&repo_abs)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| chain_abs.to_string_lossy().into_owned());
    let guard_root = crate::tour_manifest::tours_root_of(&crate::common::resolve(&out_dir));
    if guard_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .as_deref()
        == Some(".tours")
    {
        // C1 preflight (CR-2 leg-4): a present-but-broken manifest (TOML
        // parse error, or a wrong-typed known key since R1) must abort
        // BEFORE any tour file lands — the old write-then-reload order
        // crashed at the provenance leg with the corpus already
        // half-written. Same refusal doctrine as the F1b upsert site
        // below; the load here doubles as the dup-family guard's input.
        let mpre = match crate::tour_manifest::load(&guard_root.join("manifest.toml")) {
            Ok(m) => m,
            Err(e) => {
                return ToolOutput::crash(format!(
                    "{e}——manifest 損壞拒絕生成（tour 檔未寫，零 partial state）；修復或刪除重建 manifest 後重跑"
                ))
            }
        };
        let cur_fam = crate::common::resolve(&out_dir)
            .strip_prefix(&guard_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if let Some(decision) = dup_family_decision(&mpre, &cur_fam, &src_rel, explicit_out_dir) {
            if decision.redirect {
                stdout.push_str(&format!(
                    "[WARN] duplicate-family 防護：source {src_rel} 已有族 {}——重產改寫入既有族，不建 stem 重複\n",
                    decision.fam
                ));
                out_dir = guard_root.join(&decision.fam);
            } else {
                stdout.push_str(&format!(
                    "[WARN] duplicate-family：source {src_rel} 已有族 {}，而 --out-dir 指定 {cur_fam}——兩族將並存（確認是否改名遷移；舊族 pattern 會 stale）\n",
                    decision.fam
                ));
            }
        }
    }
    let primary_s = values
        .get("--primary")
        .and_then(|v| v.clone())
        .unwrap_or_default();
    let mut primary: std::collections::BTreeSet<usize> = Default::default();
    for x in primary_s.split(',') {
        if !x.trim().is_empty() {
            match x.trim().parse::<usize>() {
                Ok(v) => {
                    primary.insert(v);
                }
                Err(_) => return ToolOutput::crash(format!("--primary 非整數：{x}")),
            }
        }
    }
    let st = match build_tours(&chain_md, &repo, graph_db.as_deref()) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    // CR-11 scenario-level shell gate: a scenario with zero anchored
    // frames is omitted from the write set. Partial omission → stderr
    // WARN + shells-omitted summary counter, exit 0 (a corpus fact, not
    // a tool failure). The whole md shell-shaped → the existing (a)
    // fail-closed face: exit 1, zero writes — no .tour file, no
    // manifest row — no third exit state.
    let fam = out_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| out_dir.to_string_lossy().into_owned());
    let shell_warns = shell_warn_lines(&fam, &st.shells_omitted);
    if st.shells_omitted.len() == st.tours.len() {
        // every scenario a shell (blocks are non-empty, so tours is
        // non-empty here too)
        let mut stderr = String::new();
        for w in &shell_warns {
            stderr.push_str(w);
            stderr.push('\n');
        }
        // CR-17: trait-collision WARNs ride the fail-closed face too —
        // a shell caused by refused anchors must still name the
        // candidates (the tours are not written on this face)
        for w in trait_warn_lines(&st.trait_collisions) {
            stderr.push_str(&w);
            stderr.push('\n');
        }
        // CR-18: qualifier-refusal WARNs ride the same fail-closed face
        for w in qualifier_warn_lines(&st.qualifier_refusals) {
            stderr.push_str(&w);
            stderr.push('\n');
        }
        stderr.push_str(&crate::msg_line(
            "FAIL",
            &format!(
                "全場景殼形：md 全部 {} 場景皆零可解析幀——fail-closed 零寫入（無 .tour 檔、無 manifest row）；語料事實非工具失敗，md 保留為敘事文檔或補可解析錨（path.py:line）後重產",
                st.shells_omitted.len()
            ),
        ));
        // C: an EXISTING family's tours/rows survive the zero-write face
        // (the gate never deletes) — disclose the residual for manual
        // cleanup instead of letting it silently rot
        let gate_abs = crate::common::resolve(&out_dir);
        let gate_root = crate::tour_manifest::tours_root_of(&gate_abs);
        if gate_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .as_deref()
            == Some(".tours")
        {
            if let Ok(m) = crate::tour_manifest::load(&gate_root.join("manifest.toml")) {
                let fam_rel = gate_abs
                    .strip_prefix(&gate_root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let n = m
                    .tour
                    .keys()
                    .filter(|rel| {
                        ::std::path::Path::new(rel)
                            .parent()
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .as_deref()
                            == Some(fam_rel.as_str())
                    })
                    .count();
                if n > 0 {
                    stderr.push_str(&format!(
                        "[WARN] 既有族 {fam_rel} 留有 {n} 條 tour 檔／manifest rows 於 corpus——零寫入面不自動清除；確認後手動清族目錄＋manifest 列\n"
                    ));
                }
            }
        }
        // WARN-before-crash keeps the gated stdout (consumer_db /
        // duplicate-family notes) by building the struct directly
        return ToolOutput {
            stdout,
            stderr,
            exit_code: 1,
        };
    }
    // B: `--primary` numbers md scenarios (1-based, shells included) —
    // the pre-CR-11 order where written order ≡ md order. Resolve to the
    // compressed written position here; a primary pointing at an
    // omitted shell fails loud (the pre-existing argument-crash class),
    // zero writes.
    let md_len = st.tours.len();
    let written_md: Vec<usize> = st
        .tours
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t["steps"]
                .as_array()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .collect();
    let valid: std::collections::BTreeSet<usize> = (1..=md_len).collect();
    let out_of_range: Vec<usize> = primary.difference(&valid).copied().collect();
    if !out_of_range.is_empty() {
        return ToolOutput::crash(format!(
            "--primary 越界（共 {} 場景）：{:?}——輸入錯誤要大聲",
            md_len, out_of_range
        ));
    }
    let mut primary_written: std::collections::BTreeSet<usize> = Default::default();
    for n in &primary {
        let t = &st.tours[n - 1];
        let is_shell = t["steps"]
            .as_array()
            .map(|s| s.is_empty())
            .unwrap_or(false);
        if is_shell {
            let title = t["title"].as_str().unwrap_or("");
            return ToolOutput::crash(format!(
                "--primary {n} 指向殼省略場景「{title}」（全幀 skip，未寫出）——md 場景序含殼；改指已寫出場景或補錨重產"
            ));
        }
        let pos = written_md
            .iter()
            .position(|&i| i == n - 1)
            .expect("non-shell md scenario is in the written set")
            + 1;
        primary_written.insert(pos);
    }
    // partial shells: write set drops them; everything downstream
    // (files, manifest rows, summary) sees written tours only
    let renumber_note: Vec<String> = if st.shells_omitted.is_empty() {
        Vec::new()
    } else {
        vec!["[WARN] 殼省略：其後場景重編號（壓縮寫出序）；寫出集外的既有族 .tour／manifest rows（generator=chain_tour 自有）於重產對帳移除".to_string()]
    };
    let st = st.without_shells();
    let mut gen_warns: Vec<String> = Vec::new();
    let paths = match write_tours(&st, &out_dir, &primary_written, &mut gen_warns) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
    let mut stderr = String::new();
    for w in shell_warns
        .iter()
        .chain(renumber_note.iter())
        .chain(gen_warns.iter())
    {
        stderr.push_str(w);
        stderr.push('\n');
    }
    for p in &paths {
        stdout.push_str(&format!("[OK] chain tour -> {}\n", p.display()));
    }
    // corpus provenance: generator writes the manifest natively
    let out_abs = crate::common::resolve(&out_dir);
    let mroot = crate::tour_manifest::tours_root_of(&out_abs);
    if mroot
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .as_deref()
        != Some(".tours")
    {
        stdout.push_str(&format!(
            "[WARN] manifest skip: out-dir 不在 .tours/ 樹內（resolved root={}）——tour 檔照寫，provenance 不記（暫存/dry-run 目錄零 manifest 副作用）\n",
            mroot.display()
        ));
    } else {
        let mpath = mroot.join("manifest.toml");
        // CR-2 leg-3 F1b: a present-but-unparseable manifest must crash,
        // not fold to default — dump is a full overwrite, so upserting on
        // a corrupted manifest would silently evaporate every existing
        // row. A missing file legitimately folds (load returns default)
        // and stays the legal new-corpus path.
        let mut mdata = match crate::tour_manifest::load(&mpath) {
            Ok(m) => m,
            Err(e) => {
                return ToolOutput::crash(format!(
                    "{e}——manifest 損壞拒絕 upsert／覆寫；修復或刪除重建 manifest 後重跑"
                ))
            }
        };
        if mdata.version.is_none() {
            mdata.version = Some(toml::Value::Integer(1));
        }
        // A: reconcile the owned family on this run's write — rows under
        // this family that name a NUMBERED `.tour` outside the current
        // write set and carry generator=chain_tour are this generator's
        // product: the row leaves the manifest and the file is deleted.
        // Anything else — foreign generator, rowless file, non-`.tour`
        // row (not our format, not ours to police) — is untouched; a
        // shrunk write set (shells / renumbering) must not leave stale
        // tours colliding on ts_key. Data-safety order: rows leave the
        // in-memory manifest and the manifest PERSISTS before any file
        // is deleted — a dump failure crashes with zero deletions and
        // on-disk rows intact; a per-file removal failure is only a
        // WARN (a disclosed rowless orphan file is the tolerated
        // direction).
        let fam_rel = out_abs
            .strip_prefix(&mroot)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let write_nums: std::collections::BTreeSet<u32> = paths
            .iter()
            .filter_map(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .collect();
        let mut removed: Vec<String> = Vec::new();
        for (rel, row) in &mdata.tour {
            let Some(row_fam) = ::std::path::Path::new(rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
            else {
                continue;
            };
            if row_fam != fam_rel || !rel.ends_with(".tour") {
                continue;
            }
            let Some(num) = ::std::path::Path::new(rel)
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if write_nums.contains(&num) {
                continue;
            }
            if row.get("generator").and_then(|v| v.as_str()) == Some("chain_tour") {
                removed.push(rel.clone());
            }
        }
        for rel in &removed {
            mdata.tour.remove(rel);
        }
        let (commit, _w) = crate::tour_manifest::git_head(&repo);
        for p in &paths {
            let rel = crate::common::resolve(p)
                .strip_prefix(&mroot)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            crate::tour_manifest::upsert(
                &mut mdata,
                &rel,
                "chain_tour",
                std::slice::from_ref(&src_rel),
                &commit,
            );
        }
        // dump BEFORE deletions: a failure here leaves every file in
        // place and the on-disk rows untouched
        if let Err(e) = crate::tour_manifest::dump(&mpath, &mdata) {
            return ToolOutput::crash(e);
        }
        let mut failed: Vec<String> = Vec::new();
        for rel in &removed {
            let f = mroot.join(rel);
            if f.exists() {
                if let Err(e) = std::fs::remove_file(&f) {
                    failed.push(rel.clone());
                    stderr.push_str(&format!(
                        "[WARN] 族 {fam_rel} 對帳：{} 刪除失敗（{e}）——row 已移除，殘檔成無 row 孤兒；請手動刪除\n",
                        f.display()
                    ));
                }
            }
        }
        if !removed.is_empty() {
            let mut sorted = removed.clone();
            sorted.sort();
            // counts stay consistent with the per-file WARNs above: the
            // planned removals that failed leave disclosed residue
            let failed_note = if failed.is_empty() {
                String::new()
            } else {
                format!("（{} 項失敗殘留，見 WARN）", failed.len())
            };
            stderr.push_str(&format!(
                "[WARN] 族 {} 重產對帳：移除寫出集外殘留 {} 項{}（chain_tour 自有）：{}\n",
                fam_rel,
                sorted.len(),
                failed_note,
                sorted.join("、")
            ));
        }
        stdout.push_str(&format!(
            "[OK] manifest upsert: {}（{} rows, generator=chain_tour）\n",
            mpath.display(),
            paths.len()
        ));
        // Orphan-family sweep: families whose every source md is gone from
        // disk (renamed/deleted chain md) keep stale-pattern tours in the
        // corpus and fail tour_validate — loud warning, no auto-delete
        // (the corpus is user data; deletion is their call).
        let mut orphan_fams: Vec<String> = Vec::new();
        let mut seen_fams = std::collections::BTreeSet::new();
        for (rel, row) in &mdata.tour {
            let Some(fam) = ::std::path::Path::new(rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
            else {
                continue;
            };
            if !seen_fams.insert(fam.clone()) {
                continue;
            }
            let sources = row
                .get("sources")
                .and_then(|s| s.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !sources.is_empty() && sources.iter().all(|s| !repo_abs.join(s).exists()) {
                orphan_fams.push(fam);
            }
        }
        if !orphan_fams.is_empty() {
            stdout.push_str(&format!(
                "[WARN] manifest 有 {} 個族的 source md 已不存在（疑似改名/刪除）：{:?}——舊族 tour 留在 corpus 且 pattern 會 stale（tour_validate 會 fail）；確認後刪族目錄＋重產或清 manifest 列\n",
                orphan_fams.len(),
                orphan_fams
            ));
        }
    }
    stdout.push_str(&format!(
        "[OK] chain tours: {} 場景 / {} 幀 / {} 步 / skipped {} / shells-omitted {}\n",
        st.tours.len(),
        st.frames,
        st.frames - st.skipped,
        st.skipped,
        st.shells_omitted.len()
    ));
    // CR-17: one stdout WARN per explicit-line trait collision — the
    // candidate lists match the scenario descriptions (same payload),
    // the [LOG] distribution right below carries the same
    // explicit_trait_ambiguous count
    for w in trait_warn_lines(&st.trait_collisions) {
        stdout.push_str(&w);
        stdout.push('\n');
    }
    // CR-18: one stdout WARN per qualifier refusal — the same payload
    // discipline (candidates + reason key); the [LOG] distribution
    // right below carries the matching qualifier_* counts
    for w in qualifier_warn_lines(&st.qualifier_refusals) {
        stdout.push_str(&w);
        stdout.push('\n');
    }
    // C1 (review): fail-closed graph-read frames — disclosed like the
    // trait collisions (the [LOG] distribution right below carries the
    // graph-read-error count)
    if let Some(n) = st.g_counts.get("graph-read-error").filter(|n| **n > 0) {
        stdout.push_str(&format!(
            "[WARN] graph 讀取錯誤 {n} 幀——fail-closed 不錨定\n"
        ));
    }
    stdout.push_str(&format!("[LOG] graph 重錨分佈: {:?}\n", st.g_counts));
    ToolOutput {
        stdout,
        stderr,
        exit_code: 0,
    }
}

/// Duplicate-family decision (pure half of the run() guard):
/// `redirect` = regen should write into the existing family (default
/// out-dir case); `redirect = false` = explicit --out-dir wins, warn
/// only. The existing family's name is returned verbatim — numbered
/// stems (e.g. `01-xxx`) are never renamed.
pub struct DupFamilyDecision {
    pub fam: String,
    pub redirect: bool,
}

pub fn dup_family_decision(
    m: &crate::tour_manifest::Manifest,
    cur_fam: &str,
    src_rel: &str,
    explicit_out_dir: bool,
) -> Option<DupFamilyDecision> {
    for (rel, row) in &m.tour {
        let Some(fam) = ::std::path::Path::new(rel)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
        else {
            continue;
        };
        if fam == cur_fam {
            continue;
        }
        let sources = row
            .get("sources")
            .and_then(|s| s.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if sources.iter().any(|s| *s == src_rel) {
            return Some(DupFamilyDecision {
                fam,
                redirect: !explicit_out_dir,
            });
        }
    }
    None
}
