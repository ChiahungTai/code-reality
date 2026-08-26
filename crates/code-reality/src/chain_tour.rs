//! `chain_tour` — the frozen `code_reality/chain_tour.py` contract:
//! callchain markdown → one CodeTour `.tour` per scenario. Tree-frame
//! DFS order = step order; step lines re-anchored against graph.db
//! (same/moved/moved-file); unanchorable frames (no anchor / external /
//! name collision) are skipped with the real reason recorded in the tour
//! description.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{anchor_pattern, assert_db_unchanged, connect_ro, db_mtime_ns, graph_db_path};
use crate::profile::{is_excluded, load_profile, Profile};
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

fn ref_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"([\w./\-]+\.(?:py|rs)):(\d+)").unwrap())
}

fn ident_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]*)*").unwrap())
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

/// Best ident from symbol text: call-position (followed by `(`) first,
/// then longest; last segment.
pub fn best_ident(symbol: &str) -> String {
    let mut cands: Vec<(bool, usize, String)> = Vec::new();
    for m in ident_re().find_iter(symbol) {
        let nxt = symbol[m.end()..].chars().next();
        cands.push((nxt == Some('('), m.as_str().len(), m.as_str().to_string()));
    }
    if cands.is_empty() {
        return String::new();
    }
    cands.sort_by_key(|c| (std::cmp::Reverse(c.0), std::cmp::Reverse(c.1)));
    cands[0].2.rsplit('.').next().unwrap_or("").to_string()
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub depth: usize,
    pub parent: Option<usize>,
    pub symbol: String,
    pub ident: String,
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
        let (mut content2, note) = if content.contains("  # ") || content.starts_with('#') {
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
            path = Some(m[1].to_string());
            line_no = Some(m[2].parse().unwrap_or(0));
            let range = ref_re().find(&content2).map(|m| m.range());
            if let Some(r) = range {
                content2 = format!("{}{}", &content2[..r.start], &content2[r.end..])
                    .trim()
                    .to_string();
            }
        }
        let ident = best_ident(&content2);
        frames.push(Frame {
            depth,
            parent,
            symbol: content2.clone(),
            ident,
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

/// graph.db re-anchoring: same-file nearest line_start → cross-file move
/// detection → not-in-graph. Cross-file gate (POC discipline): ident
/// must be substring-missing AND length ≥ 4.
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
}

impl GraphAnchor {
    pub fn new(db: &Path, repo_root: &Path) -> Result<Self, String> {
        Ok(Self {
            conn: connect_ro(db)?,
            repo_root: crate::common::resolve(repo_root),
        })
    }

    pub fn anchor(
        &self,
        abs_path: &Path,
        line_no: i64,
        ident: &str,
        substring_status: &str,
    ) -> GraphHit {
        let rel = abs_path
            .strip_prefix(&self.repo_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let like = format!("%/{}", like_escape_chain(&rel));
        let hit: Option<i64> = self
            .conn
            .query_row(
                "SELECT line_start FROM nodes \
                 WHERE file_path LIKE ?1 ESCAPE '\\' AND name=?2 \
                 AND line_start IS NOT NULL \
                 ORDER BY ABS(line_start-?3) LIMIT 1",
                (&like, ident, line_no),
                |r| r.get(0),
            )
            .ok();
        if let Some(g_line) = hit {
            let delta = g_line - line_no;
            return GraphHit {
                g: if delta == 0 {
                    "same".into()
                } else {
                    "moved".into()
                },
                g_line: Some(g_line),
                g_file: None,
                g_files: vec![],
            };
        }
        if substring_status != "missing" || ident.chars().count() < 4 {
            return GraphHit {
                g: "not-in-graph".into(),
                ..Default::default()
            };
        }
        let prefix = like_escape_chain(&format!("{}/", self.repo_root.display()));
        let rels: Vec<String> = {
            let Ok(mut stmt) = self.conn.prepare(
                "SELECT DISTINCT substr(file_path, length(?1)+2) FROM nodes \
                 WHERE name=?2 AND file_path LIKE ?3 ESCAPE '\\' LIMIT 6",
            ) else {
                return GraphHit {
                    g: "not-in-graph".into(),
                    ..Default::default()
                };
            };
            let Ok(rows) = stmt.query_map(
                (
                    self.repo_root.display().to_string(),
                    ident,
                    format!("{prefix}%"),
                ),
                |r| r.get::<_, Option<String>>(0),
            ) else {
                return GraphHit {
                    g: "not-in-graph".into(),
                    ..Default::default()
                };
            };
            rows.filter_map(|r| r.ok().flatten()).collect()
        };
        if rels.len() == 1 {
            let ln: Option<i64> = self
                .conn
                .query_row(
                    "SELECT MIN(line_start) FROM nodes WHERE name=?1 \
                     AND file_path LIKE ?2 ESCAPE '\\'",
                    (ident, format!("{prefix}{}", like_escape_chain(&rels[0]))),
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            return GraphHit {
                g: "moved-file".into(),
                g_line: ln,
                g_file: Some(rels[0].clone()),
                g_files: vec![],
            };
        }
        if rels.len() > 1 {
            return GraphHit {
                g: "moved-file-ambiguous".into(),
                g_files: rels.into_iter().take(4).collect(),
                ..Default::default()
            };
        }
        GraphHit {
            g: "not-in-graph".into(),
            ..Default::default()
        }
    }
}

pub struct ScenarioTours {
    pub tours: Vec<serde_json::Value>,
    pub frames: usize,
    pub skipped: usize,
    pub g_counts: BTreeMap<String, i64>,
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
        }
        "moved-file-ambiguous" => {
            parts.push(format!(
                "同名多檔無法自動判定：{}",
                py_list_inline(&g.g_files)
            ));
            parts.push(anchor_disp);
        }
        _ if matches!(status, "drift" | "drift-far" | "missing") => {
            parts.push(format!("graph 未索引；substring 判定 {status}"));
            parts.push(anchor_disp);
        }
        _ => parts.push(anchor_disp),
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
    step
}

fn py_list_inline(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
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
    for (heading, lines) in &blocks {
        let frames = parse_frames(lines);
        let mut steps: Vec<serde_json::Value> = Vec::new();
        let mut skipped = 0usize;
        let mut skip_examples: Vec<String> = Vec::new();
        let mut skip_reasons: BTreeMap<String, i64> = BTreeMap::new();
        let mut scen_g: BTreeMap<String, i64> = BTreeMap::new();
        for f in &frames {
            let mut status = "noref".to_string();
            let mut g = GraphHit {
                g: "noref".into(),
                ..Default::default()
            };
            let mut abs_path: Option<PathBuf> = None;
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
                        status = check_anchor(&ap, f.line.unwrap_or(0), &f.ident);
                        if let Some(ga) = &ga {
                            if !f.ident.is_empty() {
                                g = ga.anchor(&ap, f.line.unwrap_or(0), &f.ident, &status);
                            }
                        }
                        abs_path = Some(ap);
                    }
                }
            }
            *g_counts.entry(g.g.clone()).or_insert(0) += 1;
            *scen_g.entry(g.g.clone()).or_insert(0) += 1;
            let Some(ap) = abs_path else {
                skipped += 1;
                *skip_reasons.entry(status.clone()).or_insert(0) += 1;
                if skip_examples.len() < 3 {
                    skip_examples.push(f.symbol.chars().take(30).collect());
                }
                continue;
            };
            steps.push(step_of(
                &f.path,
                f.line,
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
        tours.push(serde_json::json!({
            "title": heading,
            "description": format!(
                "{} 幀 → {} 步；{skipped} 幀跳過（{reasons}——例：{examples}）。\ngraph 重錨分佈：{}",
                frames.len(),
                steps.len(),
                dist.join(" ")
            ),
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
    })
}

/// Write tours: `{NN}.tour` pure sequence numbers, JSON title prefixed
/// `NN - ` (upstream chain-parseable); primary members carry isPrimary.
pub fn write_tours(
    st: &ScenarioTours,
    out_dir: &Path,
    primary: &std::collections::BTreeSet<usize>,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{} 建立失敗：{e}", out_dir.display()))?;
    let mut paths = Vec::new();
    for (i, tour) in st.tours.iter().enumerate() {
        let n = i + 1;
        let p = out_dir.join(format!("{n:02}.tour"));
        let mut emitted = tour.clone();
        emitted.as_object_mut().unwrap().insert(
            "title".into(),
            serde_json::Value::String(format!("{n:02} - {}", tour["title"].as_str().unwrap_or(""))),
        );
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
        eprintln!(
            "[WARN] 舊檔名格式殘留 {} 檔（chain-*.tour）——新舊同 title 會使 player 撞鍵靜默落第一條（corpus 靜默雙份）；重錨過渡＝刪舊檔＋manifest 重建（rm {}/chain-*.tour 後重產或 init-scan）",
            legacy_sorted.len(),
            out_dir.display()
        );
    }
    Ok(paths)
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
                    "  --graph GRAPH         CRG graph.db（預設 <repo>/.code-review-graph/graph.db；不存在則退化純文檔錨）\n",
                    "  --repo REPO           repo 根\n",
                    "  --out-dir OUT_DIR     輸出目錄（預設 .tours/arch/<文檔 stem>/——stem 即 subgroup label）\n",
                    "  --primary PRIMARY     標 isPrimary 的場景編號（1-based 逗號分隔）；預設不標\n",
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
            let default_db = graph_db_path(&repo);
            if default_db.exists() {
                Some(default_db)
            } else {
                stdout.push_str(&format!(
                    "[WARN] graph.db 不存在（{}）——退化純文檔錨，無重錨\n",
                    default_db.display()
                ));
                None
            }
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
        let mpre =
            crate::tour_manifest::load(&guard_root.join("manifest.toml")).unwrap_or_default();
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
    let valid: std::collections::BTreeSet<usize> = (1..=st.tours.len()).collect();
    let out_of_range: Vec<usize> = primary.difference(&valid).copied().collect();
    if !out_of_range.is_empty() {
        return ToolOutput::crash(format!(
            "--primary 越界（共 {} 場景）：{:?}——輸入錯誤要大聲",
            st.tours.len(),
            out_of_range
        ));
    }
    let paths = match write_tours(&st, &out_dir, &primary) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
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
        let mut mdata = crate::tour_manifest::load(&mpath).unwrap_or_default();
        if mdata.version.is_none() {
            mdata.version = Some(toml::Value::Integer(1));
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
        if let Err(e) = crate::tour_manifest::dump(&mpath, &mdata) {
            return ToolOutput::crash(e);
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
        "[OK] chain tours: {} 場景 / {} 幀 / {} 步 / skipped {}\n",
        st.tours.len(),
        st.frames,
        st.frames - st.skipped,
        st.skipped
    ));
    stdout.push_str(&format!("[LOG] graph 重錨分佈: {:?}\n", st.g_counts));
    ToolOutput {
        stdout,
        stderr: String::new(),
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
