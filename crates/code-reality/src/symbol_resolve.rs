//! symbol_resolve — the shared SCIP-symbol ↔ graph.db resolution layer.
//!
//! Two faces live here:
//! - the PURE ambiguity-decision core moved verbatim from `chain_tour`
//!   (CR-9 R1/R2/R3): the same-file same-name candidate query, the
//!   pseudo-constructor pair collapse (R1), the qualified-name
//!   scope-chain alignment (R2), and the candidate hint payload (R3).
//!   `chain_tour`'s `fill_anchor` delegates to it — behavior is frozen
//!   (the existing chain_tour suites are the gate).
//! - the symbol-coordinate locator + freshness gate consumed by
//!   `tour_validate` (CR-15: the anchor check consults
//!   `x-codeReality.symbol` before falling back to line/pattern) and
//!   `tour_reanchor`. The gate serves `current-tree` ONLY
//!   ([`crate::freshness::serves_state`]) — a stale graph never drives
//!   symbol-coordinate claims; the callers degrade to the legacy
//!   line/pattern face with one WARN.

use crate::common::connect_ro;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn ident_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]*)*").unwrap())
}

/// Best ident from symbol text: call-position (followed by `(`) first,
/// then longest. Returns (last segment, full dotted form) — CR-9 R2
/// needs the winning token's complete dotted shape (`DataPhaseStep.execute`)
/// for qualified-name alignment; the last-segment face is unchanged.
pub fn best_ident(symbol: &str) -> (String, String) {
    best_ident_filtered(symbol, &|_| false)
}

/// [`best_ident`] with a pre-ranking exclusion (CR-15 M7): tokens the
/// filter rejects (language keywords) never enter the candidate pool, so
/// a keyword cannot win by stable-order precedence and mask the next
/// non-keyword token — paren-less `def run` ranks `def` first
/// unfiltered; filtered, `run` is returned. [`best_ident`] stays the
/// unfiltered face (chain_tour's extraction is behavior-frozen).
pub fn best_ident_filtered(symbol: &str, exclude: &dyn Fn(&str) -> bool) -> (String, String) {
    let mut cands: Vec<(bool, usize, String)> = Vec::new();
    for m in ident_re().find_iter(symbol) {
        if exclude(m.as_str()) {
            continue;
        }
        let nxt = symbol[m.end()..].chars().next();
        cands.push((nxt == Some('('), m.as_str().len(), m.as_str().to_string()));
    }
    if cands.is_empty() {
        return (String::new(), String::new());
    }
    cands.sort_by_key(|c| (std::cmp::Reverse(c.0), std::cmp::Reverse(c.1)));
    let full = cands[0].2.clone();
    let last = full.rsplit('.').next().unwrap_or("").to_string();
    (last, full)
}

pub(crate) type FillRows = [(i64, String, String)];

/// One ambiguous same-name candidate surfaced to the doc author (CR-9
/// R3). `descriptor` is the candidate's descriptor region (scope chain +
/// def, module already cut). Two shapes stay banned in the rendering:
/// markdown-link form `[..](..)` — the shape tour_validate's
/// `file_ref()` actually counts in tour descriptions — and `path.py:NN`
/// form, which chain_tour's own `ref_re` parse face would read back as
/// a file:line anchor.
#[derive(Debug, Clone)]
pub struct FillCandidate {
    pub descriptor: String,
    pub line: i64,
    pub kind: String,
}

/// CR-7 line-less fill outcome: a verified unique anchor or a typed
/// no-anchor reason (machine-readable — the reason feeds the graph
/// distribution axis, while the skip is stamped `noanchor`). CR-9 R3:
/// the ambiguous reason carries the same-file same-name candidates for
/// the tour-description hint line; every other reason carries an empty
/// vector.
pub enum FillOutcome {
    Filled(i64, Option<String>),
    NoAnchor(String, Vec<FillCandidate>),
}

/// Descriptor region of a SCIP symbol: everything after the module /
/// discriminator area. Python faces wrap the module in backticks
/// (`` `pyrefly python p 1.0 `pkg.mod`/Cls#m(). ``), so the boundary is
/// the last `` `/ ``; the Rust/scip faces carry a bare package-or-path
/// prefix (`rust-analyzer cargo p 0.1.0 pkg/impl#[T]m().`), so the
/// boundary falls back to the last `/` of the discriminator-stripped
/// tail. Returns None when no boundary exists — R2 and the R3 hint
/// never guess a chain they could not locate (fail closed).
fn descriptor_region(symbol: &str) -> Option<&str> {
    let s = symbol.trim_end();
    if let Some(i) = s.rfind("`/") {
        return Some(&s[i + 2..]);
    }
    let tail = crate::engine::tail(s);
    let i = tail.rfind('/')?;
    Some(&tail[i + 1..])
}

fn sym_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '%'
}

/// Structured scope chain of a descriptor region: every descriptor name
/// in order (scopes, then the def itself), per the producer grammar —
/// `name().` function, `name#` class, `name.` variable. A Rust trait
/// impl head `impl#[T][Trait]m().` contributes its FIRST bracket group
/// (the inherent type) and skips the trait groups verbatim, so the
/// chain is `[T, m]`. Every bracket group validates like the first
/// (non-empty, all `sym_char`); a malformed group (`impl#[Lab][]m().`)
/// fails the whole chain — the candidate set stays ambiguous instead of
/// silently reading past unverified text. Returns None for anything
/// that does not parse
/// cleanly — a non-ASCII region, an empty segment, or a delimiter
/// outside the producer grammar (`().`/`#`/`.`; qname-style fixture
/// rows). `%` is a name character here (escaped-identifier runs ride
/// through as plain segments), not a rejection trigger. R2 refuses to
/// resolve over an unparseable candidate set.
fn descriptor_chain(region: &str) -> Option<Vec<String>> {
    if !region.is_ascii() {
        return None;
    }
    let mut names: Vec<String> = Vec::new();
    let mut i = 0;
    if let Some(rest) = region.strip_prefix("impl#[") {
        let close = rest.find(']')?;
        let group = &rest[..close];
        if group.is_empty() || !group.chars().all(sym_char) {
            return None;
        }
        names.push(group.to_string());
        i = "impl#[".len() + close + 1;
        while region[i..].starts_with('[') {
            let rest = &region[i + 1..];
            let end = rest.find(']')?;
            let group = &rest[..end];
            if group.is_empty() || !group.chars().all(sym_char) {
                return None;
            }
            i += end + 2;
        }
    }
    let b = region.as_bytes();
    while i < region.len() {
        let start = i;
        while i < region.len() && sym_char(b[i] as char) {
            i += 1;
        }
        if i == start {
            return None;
        }
        names.push(region[start..i].to_string());
        if region[i..].starts_with("().") {
            i += 3;
        } else if region[i..].starts_with('#') || region[i..].starts_with('.') {
            i += 1;
        } else {
            return None;
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

/// CR-17 explicit-line trait-collision reader: the Rust impl head of a
/// descriptor region (`impl#[Type][Trait…]def…`) split into the inherent
/// type group and the trait groups taken VERBATIM up to the first `]`
/// (no bracket-depth tracking — a nested `[` inside a group ends the
/// group there, so `` `Add<Self>` `` rides through as an opaque string).
/// This deliberately does NOT reuse [`descriptor_chain`]'s grammar (which
/// keeps only the first [Type] group and validates every group as
/// `sym_char`-only for R2's chain alignment — frozen for the line-less
/// face): collision detection needs the trait groups, and
/// generic-bearing groups must still count. A malformed group (empty,
/// unterminated bracket) fails the whole read — the candidate then
/// contributes nothing to a collision, which for THIS guard is the
/// fail-open direction (a malformed sibling could hide a real
/// collision); the exposure is bounded by the producer machinery
/// (rust-analyzer mints well-formed bracket groups), and counting
/// malformed impl rows toward collisions is deferred to the CR-18
/// qualifier card. Returns None for non-impl shapes; a non-impl region
/// never contributes to a collision.
pub(crate) fn impl_trait_groups(region: &str) -> Option<(String, Vec<String>)> {
    let rest = region.strip_prefix("impl#[")?;
    let close = rest.find(']')?;
    let ty = &rest[..close];
    if ty.is_empty() {
        return None;
    }
    let mut traits: Vec<String> = Vec::new();
    let mut i = "impl#[".len() + close + 1;
    while region[i..].starts_with('[') {
        let group_rest = &region[i + 1..];
        let end = group_rest.find(']')?;
        let group = &group_rest[..end];
        if group.is_empty() {
            return None;
        }
        traits.push(group.to_string());
        i += end + 2;
    }
    Some((ty.to_string(), traits))
}

/// CR-17: does the explicit-line same-file same-name candidate set carry
/// a trait collision — at least two impl-shaped candidates sharing one
/// inherent type group whose trait-group signatures differ? The name
/// axis is already pinned by the caller's `name=?` query, so
/// (same type, same name, differing trait groups) is exactly the NT
/// shape `impl#[Price][Debug]fmt().` vs `impl#[Price][Display]fmt().`.
/// An inherent impl (zero trait groups) vs a trait impl counts as
/// differing — the guarded hazard is the silent nearest-line pick
/// between semantically distinct same-type same-name impls, the same
/// pair R2 refuses to resolve on the line-less face.
pub(crate) fn explicit_trait_collision(rows: &[(i64, String, String)]) -> bool {
    let mut by_type: std::collections::BTreeMap<String, std::collections::BTreeSet<Vec<String>>> =
        std::collections::BTreeMap::new();
    for (_, sym, _) in rows {
        let Some(region) = descriptor_region(sym) else {
            continue;
        };
        let Some((ty, traits)) = impl_trait_groups(region) else {
            continue;
        };
        by_type.entry(ty).or_default().insert(traits);
    }
    by_type.values().any(|sigs| sigs.len() >= 2)
}

/// CR-17 WARN/hint payload: every candidate of the full same-file
/// same-name candidate set (superset of the colliding group) with its
/// descriptor (region, raw-symbol fallback), line and kind, ordered
/// by (line_start, symbol) — the fill query's stable order, so the list
/// reads identically no matter which anchor line triggered it. Unlike
/// R3's [`fill_candidates`] nothing is dropped: parseability filters are
/// line-less-face concepts; the explicit-line face must disclose every
/// sibling the legacy nearest-line SQL silently picked from.
pub(crate) fn explicit_line_candidates(rows: &[(i64, String, String)]) -> Vec<FillCandidate> {
    let mut ordered: Vec<&(i64, String, String)> = rows.iter().collect();
    ordered.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    ordered
        .into_iter()
        .map(|(line, sym, kind)| FillCandidate {
            descriptor: descriptor_region(sym).unwrap_or(sym).to_string(),
            line: *line,
            kind: kind.clone(),
        })
        .collect()
}

/// Mirror of pyrefly-producer's `pseudo_ctor_symbol` (symbol.rs): the
/// pseudo-constructor form of a class-shaped symbol swaps the trailing
/// `#` for `().`. Local copy — the tool layer embeds no producer dep.
fn pseudo_ctor_symbol(sym: &str) -> Option<String> {
    sym.strip_suffix('#').map(|head| format!("{head}()."))
}

/// R1 (CR-9): the candidate set is EXACTLY the pseudo-constructor
/// self-collision — one Type node `` …X# `` plus its minted
/// constructor-call twin `` …X(). `` (producer B7b pairs one logical
/// definition into two nodes), equal line_start, no other same-name
/// rows. Any unmet condition keeps the set ambiguous. The fill stamps
/// the TYPE symbol — a canonicalization choice, not a claim that the
/// two SCIP symbols are equivalent.
fn r1_pseudo_ctor_pair(rows: &FillRows) -> Option<(i64, Option<String>)> {
    if rows.len() != 2 {
        return None;
    }
    let (a, b) = (&rows[0], &rows[1]);
    let is_type_ctor = |t: &(i64, String, String), c: &(i64, String, String)| {
        t.1.ends_with('#') && t.2 == "Type" && c.2 == "Function"
    };
    let (t, c) = if is_type_ctor(a, b) {
        (a, b)
    } else if is_type_ctor(b, a) {
        (b, a)
    } else {
        return None;
    };
    if pseudo_ctor_symbol(&t.1).as_deref() != Some(c.1.as_str()) {
        return None;
    }
    if t.0 != c.0 {
        return None;
    }
    Some((t.0, Some(t.1.clone())))
}

/// R2 (CR-9): the doc's qualified dotted name uniquely identifies one
/// candidate by structured scope-chain tail alignment. The doc chain
/// (segments of the winning ident token, case-sensitive) is compared
/// against each candidate's descriptor chain with whole-segment
/// equality — never a substring ends_with. Two tiers: equal-length
/// full match first; when no tier-1 candidate survives, a strictly
/// SHORTER doc chain may tail-align into longer candidate chains.
/// Exactly one survivor in the winning tier fills; zero or multiple
/// survivors, an unusable doc chain, or ANY unparseable candidate
/// descriptor keeps the set ambiguous (fail closed).
fn r2_qualified_unique(rows: &FillRows, doc_full: &str) -> Option<(i64, Option<String>)> {
    let doc_chain: Vec<&str> = doc_full.split('.').collect();
    if doc_chain.is_empty() || doc_chain.iter().any(|s| s.is_empty()) {
        return None;
    }
    let chains: Vec<Vec<String>> = rows
        .iter()
        .map(|(_, sym, _)| descriptor_chain(descriptor_region(sym)?))
        .collect::<Option<_>>()?;
    let segs_eq = |chain: &[String]| {
        chain.len() == doc_chain.len()
            && chain.iter().zip(doc_chain.iter()).all(|(a, b)| a == b)
    };
    let tier1: Vec<usize> = chains
        .iter()
        .enumerate()
        .filter(|(_, chain)| segs_eq(chain))
        .map(|(i, _)| i)
        .collect();
    let pick = |survivors: Vec<usize>| {
        if survivors.len() == 1 {
            let (line, sym, _) = &rows[survivors[0]];
            Some((*line, Some(sym.clone())))
        } else {
            None
        }
    };
    if !tier1.is_empty() {
        return pick(tier1);
    }
    let tier2: Vec<usize> = chains
        .iter()
        .enumerate()
        .filter(|(_, chain)| {
            chain.len() > doc_chain.len()
                && chain[chain.len() - doc_chain.len()..]
                    .iter()
                    .zip(doc_chain.iter())
                    .all(|(a, b)| a == b)
        })
        .map(|(i, _)| i)
        .collect();
    pick(tier2)
}

/// R3 hint payload: only candidates whose descriptor parses cleanly
/// render (qname-shaped legacy rows stay out of the hint — the line is
/// advisory; the ambiguous count itself is unaffected).
fn fill_candidates(rows: &FillRows) -> Vec<FillCandidate> {
    rows.iter()
        .filter_map(|(line, sym, kind)| {
            let region = descriptor_region(sym)?;
            descriptor_chain(region)?;
            Some(FillCandidate {
                descriptor: region.to_string(),
                line: *line,
                kind: kind.clone(),
            })
        })
        .collect()
}

/// The row-slice decision core of the same-file fill (moved verbatim
/// from `chain_tour::fill_anchor`): 0 rows → not-in-graph; 1 row →
/// filled; many → R1/R2 collapse under the whole-frame gate, else
/// ambiguous with the R3 hint.
pub(crate) fn resolve_candidates(rows: &FillRows, ident_full: &str, whole: bool) -> FillOutcome {
    match rows {
        [] => FillOutcome::NoAnchor("not-in-graph".into(), Vec::new()),
        [(line, symbol, _)] => FillOutcome::Filled(*line, Some(symbol.clone())),
        many => {
            // whole-frame gate (COD-1): a prose-wrapped remainder
            // carries no disambiguation evidence — skip R1/R2 and
            // stay ambiguous with the R3 hint (fail-visible)
            if !whole {
                return FillOutcome::NoAnchor("ambiguous".into(), fill_candidates(many));
            }
            if let Some(fill) = r1_pseudo_ctor_pair(many) {
                return FillOutcome::Filled(fill.0, fill.1);
            }
            if let Some(fill) = r2_qualified_unique(many, ident_full) {
                return FillOutcome::Filled(fill.0, fill.1);
            }
            FillOutcome::NoAnchor("ambiguous".into(), fill_candidates(many))
        }
    }
}

/// The strict same-file candidate query (moved verbatim from
/// `chain_tour::fill_anchor`): EXACT path+name, line_start present,
/// ordered `line_start, symbol`. SQLite errors propagate loud instead
/// of folding into a miss.
pub(crate) fn same_file_rows(
    conn: &Connection,
    exact_path: &str,
    ident: &str,
) -> Result<Vec<(i64, String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT line_start, symbol, kind FROM nodes \
             WHERE file_path = ?1 AND name = ?2 \
             AND line_start IS NOT NULL AND line_start >= 1 \
             ORDER BY line_start, symbol",
        )
        .map_err(|e| format!("graph.db fill query failed: {e}"))?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map((exact_path, ident), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| format!("graph.db fill query failed: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("graph.db fill query failed: {e}"))?;
    Ok(rows)
}

/// The result of a `nodes.symbol` exact lookup (CR-15 decision 7):
/// strict identity — zero rows is a MISS (never evidence that the
/// definition was deleted; the caller falls back to the legacy
/// line/pattern face), multiple rows sharing one symbol is AMBIGUOUS
/// (defensive: the self-owned schema pins symbol UNIQUE, a violating db
/// fails closed here).
pub enum SymbolLoc {
    Miss,
    Located {
        file_path: String,
        line_start: i64,
    },
    Ambiguous(Vec<(String, Option<i64>)>),
}

/// Read-only symbol-coordinate locator over `.code-reality/graph.db`.
pub struct SymbolLocator {
    conn: Connection,
    repo_root: PathBuf,
}

impl SymbolLocator {
    pub fn open(db: &Path, repo_root: &Path) -> Result<Self, String> {
        let conn = connect_ro(db)?;
        // schema probe (chain_tour review C4 + CR-15 M4): the symbol
        // face reads nodes.symbol/file_path/line_start — a legacy-schema
        // db would silently degrade every lookup to Miss; fail loud
        // instead
        conn.prepare("SELECT symbol, file_path, line_start FROM nodes LIMIT 1").map_err(|_| {
            format!(
                "db 非自有格式（nodes.symbol/file_path/line_start 缺）：{}——重跑 `code-reality graph_db build --repo <repo>`",
                db.display()
            )
        })?;
        Ok(Self {
            conn,
            repo_root: crate::common::resolve(repo_root),
        })
    }

    /// `nodes.symbol` exact match → the definition's current address.
    /// Coordinate-less rows (line_start NULL) stay out of the pool.
    /// SQL and row-decode errors propagate LOUD (CR-15 M4) — a broken
    /// db must never fold into a silent Miss; the callers print one
    /// WARN and degrade to the legacy line/pattern face.
    pub fn locate(&self, symbol: &str) -> Result<SymbolLoc, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT file_path, line_start FROM nodes \
                 WHERE symbol = ?1 AND line_start IS NOT NULL",
            )
            .map_err(|e| format!("graph.db symbol query failed: {e}"))?;
        let rows: Vec<(String, i64)> = stmt
            .query_map((symbol,), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("graph.db symbol query failed: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("graph.db symbol query failed: {e}"))?;
        Ok(match rows.len() {
            0 => SymbolLoc::Miss,
            1 => SymbolLoc::Located {
                file_path: rows[0].0.clone(),
                line_start: rows[0].1,
            },
            _ => SymbolLoc::Ambiguous(rows.into_iter().map(|(f, l)| (f, Some(l))).collect()),
        })
    }

    /// graph.db absolute file_path → repo-relative forward-slash form.
    /// `None` when the path does not live under the repo root (a db
    /// copied across checkouts — the caller degrades, never guesses).
    pub fn rel_file(&self, file_path: &str) -> Option<String> {
        Path::new(file_path)
            .strip_prefix(&self.repo_root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    /// The same-file fill face for bootstrap (CR-15 decision 5): strict
    /// path+name candidates over THIS locator's repo root, resolved by
    /// the shared R1/R2 core. `whole=true` at the call sites — the
    /// bootstrap ident is extracted, never prose-wrapped.
    pub fn same_file_fill(
        &self,
        rel_file: &str,
        ident: &str,
        ident_full: &str,
    ) -> Result<FillOutcome, String> {
        let exact = format!("{}/{}", self.repo_root.display(), rel_file);
        let rows = same_file_rows(&self.conn, &exact, ident)?;
        Ok(resolve_candidates(&rows, ident_full, true))
    }
}

/// Per-corpus symbol-resolution gate (CR-15 decision 2): the symbol
/// coordinate face is served ONLY when graph.db exists AND the freshness
/// verdict says the slot serves `current-tree`. Any other state yields
/// `locator: None` plus the ONE-LINE degrade WARN — the caller's
/// behavior is then exactly the legacy line/pattern face.
pub struct SymbolGate {
    locator: Option<SymbolLocator>,
}

impl SymbolGate {
    /// Production gate. Compute once per tool run (CR-12: one staleness
    /// evaluation per query).
    pub fn for_repo(repo: &Path) -> (SymbolGate, Option<String>) {
        let degraded = |why: String| {
            (
                SymbolGate { locator: None },
                Some(format!("[WARN] {why}——symbol 驗證退化 line/pattern\n")),
            )
        };
        let root = crate::engine::resolve_repo(repo);
        let db = crate::graph_db::db_path(&root);
        if !db.exists() {
            return degraded(format!("無 graph.db（{}）", db.display()));
        }
        let slot = match crate::engine::default_index_path(&root) {
            Ok(s) => s,
            Err(e) => return degraded(format!("slot 路徑解析失敗（{e}）")),
        };
        let snap = match crate::engine::evaluate_staleness(
            &root,
            &slot,
            crate::identity::resolve_policy(crate::identity::IdentityCachePolicy::WriteBack),
        ) {
            Ok(s) => s,
            Err(e) => return degraded(format!("freshness 檢查失敗（{e}）")),
        };
        let serves = crate::freshness::serves_state(&snap);
        if serves != "current-tree" {
            return degraded(format!("graph stale（serves {serves}）"));
        }
        match SymbolLocator::open(&db, &root) {
            Ok(loc) => (SymbolGate { locator: Some(loc) }, None),
            Err(e) => degraded(format!("graph.db 不可讀（{e}）")),
        }
    }

    /// The degraded gate (no symbol face). Callers take [`for_repo`];
    /// this is the explicit disabled value for tests and skips.
    pub fn disabled() -> SymbolGate {
        SymbolGate { locator: None }
    }

    pub fn locator(&self) -> Option<&SymbolLocator> {
        self.locator.as_ref()
    }
}
