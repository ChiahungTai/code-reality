//! engine — SCIP parsing, symbol predicates, query orchestration (protobuf face).
//!
//! Semantics ported 1:1 from the frozen Python `code_reality/scip_refs.py`
//! (line anchors in `ai-analysis/execution-plans/ep-rust-r2-scip-family.md`);
//! display assembly is byte-identical (proven by `poc/r2-byte-identical`).
//! Predicates are hand-rolled string functions because the `regex` crate has
//! no look-around; boundary tests pin parity with the Python regexes
//! (`my_open`/`reopen` cases from the Python matcher docstring).

use protobuf::Message;
use scip::types::{Index, Occurrence};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const META_SUFFIX: &str = ".meta.json";

// ---------- predicates ----------

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Python `(?<!\w)<name>\(\)\.$` — trailing `<name>().` whose preceding char
/// is not a word char (or string start). Python `$` also matches just before
/// a trailing newline — strip it first.
pub fn name_pat_match(s: &str, name: &str) -> bool {
    let s = s.strip_suffix('\n').unwrap_or(s);
    let needle = format!("{}().", name);
    match s.strip_suffix(&needle) {
        Some(before) => before.chars().next_back().is_none_or(|c| !is_word(c)),
        None => false,
    }
}

/// Python `(?<![\w#])<Type>#` — `<Type>#` whose preceding char is neither a
/// word char nor `#` (or string start).
fn trait_decl_match(s: &str, type_name: &str) -> bool {
    let pat = format!("{}#", type_name);
    let mut from = 0usize;
    while let Some(pos) = s[from..].find(&pat) {
        let abs = from + pos;
        let ok = abs == 0 || {
            let c = s[..abs].chars().next_back().unwrap();
            !is_word(c) && c != '#'
        };
        if ok {
            return true;
        }
        from = abs + pat.len();
    }
    false
}

/// Query shape (`Type.method` vs bare name) — mirrors `_matcher` (scip_refs.py:135).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    TypeMethod { type_name: String, method: String },
    Bare { name: String },
}

impl Query {
    pub fn parse(q: &str) -> Query {
        match q.rsplit_once('.') {
            Some((type_name, method)) => Query::TypeMethod {
                type_name: type_name.to_string(),
                method: method.to_string(),
            },
            None => Query::Bare {
                name: q.to_string(),
            },
        }
    }
}

/// Queryable-symbol kinds (S3): `#`-tailed class/type-like symbols are
/// queryable as `Type` nodes on the Python faces (AIR-33) and the JS/TS
/// faces (scip-typescript emits real `Name#` defs); Rust `Type#` stays
/// non-queryable by bare name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryableKind {
    Function,
    Type,
}

pub struct QueryableSymbol<'a> {
    pub name: &'a str,
    pub kind: QueryableKind,
}

/// Single ingest/query gate for queryable symbol shapes, document-aware
/// (S3): fn-shaped symbols are always queryable functions; `#`-tailed
/// symbols are queryable types only on the Python faces (symbol prefix)
/// or when the DEFINING document is JS/TS (scip-typescript carries no
/// language discriminator in the symbol — AD-3 derives it from the
/// document extension). This one predicate drives both the scip_refs
/// cache ingest and graph materialization — duplicating the gate per
/// path is how JS/TS class definitions used to disappear.
pub fn queryable_symbol<'a>(symbol: &'a str, rel_path: &str) -> Option<QueryableSymbol<'a>> {
    if let Some(name) = fn_tail_name(symbol) {
        return Some(QueryableSymbol {
            name,
            kind: QueryableKind::Function,
        });
    }
    let type_name = class_tail_name(symbol)?;
    let js_ts = matches!(
        crate::language::LanguageFace::from_path(Path::new(rel_path)),
        Some(crate::language::LanguageFace::JavaScript)
            | Some(crate::language::LanguageFace::TypeScript)
    );
    (python_face(symbol) || js_ts).then_some(QueryableSymbol {
        name: type_name,
        kind: QueryableKind::Type,
    })
}

/// Class-arm membership for query matching: Python faces by symbol
/// prefix, JS/TS by defining document. `rel_path: None` = no document
/// context (legacy call sites — python-prefix rule only).
fn class_arm_matches(symbol: &str, rel_path: Option<&str>) -> bool {
    if python_face(symbol) {
        return true;
    }
    matches!(
        rel_path.and_then(|rel| crate::language::LanguageFace::from_path(Path::new(rel))),
        Some(crate::language::LanguageFace::JavaScript)
            | Some(crate::language::LanguageFace::TypeScript)
    )
}

/// Symbol matches query (name-tail AND (marker OR trait-decl) for
/// Type.method). Bare matches the fn tail `<name>().` OR the class tail
/// `<name>#` on the python faces / JS-or-TS documents (AIR-33 + S3:
/// end-anchored, so a mid-chain class in `Outer#Inner#` never matches
/// an `Outer` query; Rust `Type#` DEFs share the tail shape but stay
/// non-queryable by bare name — see [`python_face`]). `rel_path` is the
/// DEFINING document of the occurrence when known.
pub fn matches_query(symbol: &str, rel_path: Option<&str>, query: &Query) -> bool {
    match query {
        Query::TypeMethod { type_name, method } => {
            name_pat_match(symbol, method)
                && (symbol.contains(&format!("[{type_name}]"))
                    || trait_decl_match(symbol, type_name))
        }
        Query::Bare { name } => {
            name_pat_match(symbol, name)
                || (class_arm_matches(symbol, rel_path)
                    && class_tail_name(symbol).is_some_and(|n| n == name))
        }
    }
}

/// Python-face discriminator: the symbol's second space-separated token
/// is `python` (`pyrefly python …`, `scip-python python …`). Gates the
/// bare-class query arm — AIR-33 adjudicated Python classes; rust
/// `Type#` DEF symbols (same tail shape, e.g. `build/BuildError#`)
/// stay non-queryable by bare name, preserving the documented rust-face
/// contract (probe-confirmed on this repo's own index).
pub fn python_face(symbol: &str) -> bool {
    symbol.split(' ').nth(1) == Some("python")
}

/// Python `FN_TAIL_RE` = `(?<!\w)(\w+)\(\)\.$` — capture the trailing function
/// identifier of a fn-shaped symbol (`…/<name>().` → `<name>`); a trailing
/// newline is tolerated (Python `$` semantics).
pub fn fn_tail_name(symbol: &str) -> Option<&str> {
    let body = symbol.strip_suffix('\n').unwrap_or(symbol);
    let body = body.strip_suffix("().")?;
    let start = body
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if start == body.len() {
        return None; // no word chars before "()."
    }
    let name = &body[start..];
    let before_ok = start == 0 || !is_word(body[..start].chars().next_back().unwrap());
    before_ok.then_some(name)
}

/// Trailing class identifier of a `#`-suffixed symbol (`…/Class#` →
/// `Class`, `…/Outer#Inner#` → `Inner`) — the class form of the
/// pyrefly/scip-python face. Tail-anchored like `fn_tail_name`; unlike
/// `trait_decl_match` (mid-string search, the Type.method qualifier arm)
/// it never matches a class sitting mid-chain.
pub fn class_tail_name(symbol: &str) -> Option<&str> {
    let body = symbol.strip_suffix('\n').unwrap_or(symbol);
    let body = body.strip_suffix('#')?;
    let start = body
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if start == body.len() {
        return None; // no word chars before "#"
    }
    let name = &body[start..];
    let before_ok = start == 0 || !is_word(body[..start].chars().next_back().unwrap());
    before_ok.then_some(name)
}

// ---------- display ----------

/// `tail()` — symbol.split(" ") with >4 parts takes the last (scip_refs.py:129).
pub fn tail(symbol: &str) -> &str {
    let parts: Vec<&str> = symbol.split(' ').collect();
    if parts.len() > 4 {
        parts[parts.len() - 1]
    } else {
        symbol
    }
}

/// `ln()` — SCIP ranges are 0-based; return the 1-based start line, -1 if absent.
pub fn ln(occ: &Occurrence) -> i64 {
    if occ.range.len() >= 2 {
        occ.range[0] as i64 + 1
    } else {
        -1
    }
}

pub fn loc_line(rel_path: &str, line: i64) -> String {
    if line <= 0 {
        format!("{}:?", rel_path)
    } else {
        format!("{}:{}", rel_path, line)
    }
}

// ---------- protobuf-face scan ----------

/// DEF occurrences matching the query, per symbol, in scan order (:162).
pub fn find_defs(index: &Index, query: &Query) -> BTreeMap<String, Vec<String>> {
    let mut defs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 != 0
                && matches_query(&occ.symbol, Some(&d.relative_path), query)
            {
                defs.entry(occ.symbol.clone())
                    .or_default()
                    .push(loc_line(&d.relative_path, ln(occ)));
            }
        }
    }
    defs
}

/// Non-DEF occurrences of the given symbols, in scan order (:173).
pub fn find_refs(index: &Index, symbols: &BTreeSet<String>) -> HashMap<String, Vec<String>> {
    let mut refs: HashMap<String, Vec<String>> =
        symbols.iter().map(|s| (s.clone(), Vec::new())).collect();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 == 0 && symbols.contains(&occ.symbol) {
                refs.entry(occ.symbol.clone())
                    .or_default()
                    .push(loc_line(&d.relative_path, ln(occ)));
            }
        }
    }
    refs
}

// ---------- caller-edge accessors (R3) ----------

/// A fn DEF's enclosing span, 1-based inclusive lines. `seq` is the DEF's
/// scan order — the same-width tie-break basis (first-seen wins). Only the
/// per-file relative order matters to the tie rule; both faces preserve it
/// (protobuf scan order; sqlite `ORDER BY seq` insertion order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSpan {
    pub symbol: String,
    pub rel_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub seq: usize,
}

/// fn DEF spans from `enclosing_range` (the fn's full item span — NOT the
/// DEF occurrence's own `range`, which is the name line; NOT a ref's enc,
/// which is a callee echo). 4-element enc `[sl,sc,el,ec]` → `(sl+1, el+1)`;
/// 3-element single-line enc `[sl,sc,ec]` → `(sl+1, sl+1)` (SM-6, ≥35
/// macro-generated fns in NT). An unset enc (len 0) is a legal "no
/// enclosing data" state — skipped silently (rust-analyzer covers fn DEFs
/// 100%, so this only fires on enc-less indexers; refs fall to item-level).
/// Any other non-empty arity is skipped with a WARN line (fail-loud
/// defensive face — unseen in research data).
pub fn fn_spans(index: &Index) -> (BTreeMap<String, Vec<FnSpan>>, Vec<String>) {
    let mut spans: BTreeMap<String, Vec<FnSpan>> = BTreeMap::new();
    let mut warns: Vec<String> = Vec::new();
    let mut seq = 0usize;
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 == 0 || fn_tail_name(&occ.symbol).is_none() {
                continue;
            }
            let enc = &occ.enclosing_range;
            let (start_line, end_line) = match enc.len() {
                0 => continue, // legal absent enc — no span, no warn
                4 => (enc[0] as i64 + 1, enc[2] as i64 + 1),
                3 => (enc[0] as i64 + 1, enc[0] as i64 + 1),
                n => {
                    warns.push(format!(
                        "[WARN] fn span enc 元素數 {}（預期 4 或 3）——跳過：{}\n",
                        n, occ.symbol
                    ));
                    continue;
                }
            };
            spans
                .entry(d.relative_path.clone())
                .or_default()
                .push(FnSpan {
                    symbol: occ.symbol.clone(),
                    rel_path: d.relative_path.clone(),
                    start_line,
                    end_line,
                    seq,
                });
            seq += 1;
        }
    }
    (spans, warns)
}

/// Structured non-DEF rows for the given symbols, flat in global scan order
/// (the interleaved order across symbols that caller-first-site ordering and
/// per-caller site ordering depend on). Row = (symbol, rel_path, 1-based
/// line). Structured data, never parsed back from display strings.
pub fn refs_rows(index: &Index, symbols: &BTreeSet<String>) -> Vec<(String, String, i64)> {
    let mut rows: Vec<(String, String, i64)> = Vec::new();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 == 0 && symbols.contains(&occ.symbol) {
                rows.push((occ.symbol.clone(), d.relative_path.clone(), ln(occ)));
            }
        }
    }
    rows
}

/// Byte-identical query report (report(), scip_refs.py:182). Returns
/// (stdout, exit_code); defs empty → `[WARN] 查無 DEF` + exit 1.
pub fn report(
    defs: &BTreeMap<String, Vec<String>>,
    refs: &HashMap<String, Vec<String>>,
    src_line: Option<&str>,
    query: &str,
) -> (String, i32) {
    let mut out = String::new();
    if let Some(line) = src_line {
        out.push_str(line);
        out.push('\n');
    }
    if defs.is_empty() {
        out.push_str(&format!("[WARN] 查無 DEF：{}\n", query));
        return (out, 1);
    }
    for symbol in defs.keys() {
        let r_list = refs.get(symbol).map(Vec::as_slice).unwrap_or(&[]);
        out.push_str(&format!("[OK] {}\n", tail(symbol)));
        for loc_str in &defs[symbol] {
            out.push_str(&format!("  DEF  {}\n", loc_str));
        }
        out.push_str(&format!("  refs: {} 處（跨檔）\n", r_list.len()));
        for r in r_list.iter().take(6) {
            out.push_str(&format!("    {}\n", r));
        }
        if r_list.len() > 6 {
            out.push_str(&format!("    ...共 {} 處\n", r_list.len()));
        }
    }
    (out, 0)
}

// ---------- index loading (load_index, scip_refs.py:96-113) ----------

pub struct LoadedIndex {
    pub index: Index,
}

pub fn load_index(path: &Path) -> Result<LoadedIndex, String> {
    // Bare messages — the [FAIL] tag is applied once at the ToolOutput::fail boundary.
    let bytes = std::fs::read(path).map_err(|e| format!("索引解析失敗（損壞/截斷？）：{}", e))?;
    let index = Index::parse_from_bytes(&bytes)
        .map_err(|e| format!("索引解析失敗（損壞/截斷？）：{}", e))?;
    if index.documents.is_empty() {
        return Err("索引 0 文檔——空或損壞".to_string());
    }
    // The old `documents < 100` "possible truncation" heuristic retired
    // (S2): it misattributed missing files as truncation on small repos
    // while real truncation fails loudly in the parses above; the precise
    // missing-files signal is doc_set_delta on the heal path.
    Ok(LoadedIndex { index })
}

// ---------- slot / meta / git / [SRC] ----------

pub fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(p),
    }
}

/// Canonical repo root for in-repo data-plane resolution: relative
/// `--repo .`/`--repo ""` resolves against cwd; canonicalize is
/// best-effort (non-existent paths pass through unchanged).
pub fn resolve_repo(repo: &Path) -> PathBuf {
    if repo.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| repo.to_path_buf())
    } else {
        repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf())
    }
}

/// In-repo slot: `<repo>/.code-reality/scip/index.scip` — derived data
/// lives with its key (the checkout). The home basename-keyed slot and
/// its empty-name guard retired with the key. Currently infallible;
/// the `Result` shape stays for call-site stability.
pub fn default_index_path(repo: &Path) -> Result<PathBuf, String> {
    Ok(resolve_repo(repo)
        .join(".code-reality")
        .join("scip")
        .join("index.scip"))
}

pub fn meta_path(index_path: &Path) -> PathBuf {
    let name = index_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    index_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{}{}", name, META_SUFFIX))
}

/// Self-contained ignore for the in-repo data dir: a single `*` (CRG
/// `_write_data_dir_gitignore` mode). A `!.gitignore` negation would
/// re-include the file itself and dirty `git status --porcelain`.
/// Pre-existing content is never rewritten; failures are loud (a silent
/// miss leaves the data dir committable).
pub fn write_data_dir_gitignore(data_root: &Path) -> Result<(), String> {
    let g = data_root.join(".gitignore");
    if g.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(data_root)
        .map_err(|e| format!("資料目錄建立失敗（{}）：{e}", data_root.display()))?;
    std::fs::write(
        &g,
        "# Auto-generated by code-reality — derived data, do not commit.\n*\n",
    )
    .map_err(|e| format!(".gitignore 寫入失敗（{}）：{e}", g.display()))?;
    Ok(())
}

// ---------- index↔source staleness primitives (S2) ----------

/// Corpus skip dirs — single source shared with the producer crate (its
/// `collect_py_files` consumed the same 3-item list). The detection walk
/// must be a SUPERSET walk relative to the producer corpus: skipping
/// fewer dirs degrades to false-stale (safe — the heal's loop guard
/// catches it); skipping more would go false-fresh (the dangerous side).
/// `target` is NOT in this list: the python face keeps walking it
/// (pyrefly indexes .py under it) while `walk_sources` skips its `.rs`
/// (rust-analyzer's OUT_DIR artifacts never enter the corpus).
pub const SKIP_DIRS: [&str; 3] = ["__pycache__", "venv", "node_modules"];

/// Shared test-file classification (S3 single source): consumed by BOTH
/// graph DB insertion (`is_test` column) and graph-engine query
/// filtering, which previously disagreed (`__tests__`/`.spec`/`.test`
/// known only to the query regex). Union policy — `tests/` directory
/// and the `test_` prefix keep their historical any-extension behavior
/// (Rust/Python fixtures pin it); `__tests__/` and the `.spec.`/`.test.`
/// suffixes apply to the six JS/TS extensions only, never to unrelated
/// extensions.
pub fn is_test_path(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    if p.starts_with("tests/") || p.contains("/tests/") {
        return true;
    }
    if p.starts_with("test_") || p.contains("/test_") {
        return true;
    }
    if p.starts_with("__tests__/") || p.contains("/__tests__/") {
        return true;
    }
    let name = p.rsplit('/').next().unwrap_or("");
    for stem in [".spec", ".test"] {
        for ext in [".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx"] {
            if name.ends_with(&format!("{stem}{ext}")) {
                return true;
            }
        }
    }
    false
}

/// One disk walk feeding every staleness signal: the per-language file
/// sets (doc-set comparison), the newest source mtime (cheap trigger),
/// and the face-scoped source-set fingerprint (S4 add/delete/rename
/// detection — mtime alone cannot see a deletion). Python and JS/TS
/// files are profile-excluded HERE (AD-11: producer corpus and freshness
/// corpus share one effective policy); Rust keeps its historical corpus
/// rules (no profile filter — rust-analyzer is workspace-scoped with no
/// per-file corpus input, so a walk-side filter would desync freshness
/// from what the producer actually indexes; recorded exemption).
#[derive(Debug, Default)]
pub struct SourceWalk {
    pub py: BTreeMap<String, crate::identity::SourceRecord>,
    pub rs: BTreeMap<String, crate::identity::SourceRecord>,
    pub js: BTreeMap<String, crate::identity::SourceRecord>,
    pub ts: BTreeMap<String, crate::identity::SourceRecord>,
    /// Newest across ALL walked faces — the legacy staleness basis for
    /// indexes without stamped face metadata.
    pub newest: Option<std::time::SystemTime>,
    /// Per-face newest for face-scoped comparisons (S4 face isolation:
    /// an explicit python-only slot is not staled by a newer `.ts`).
    pub newest_by_face: BTreeMap<crate::language::LanguageFace, std::time::SystemTime>,
}

impl SourceWalk {
    /// All records merged across faces, keyed by rel (globally unique —
    /// the face is a function of the extension). BTreeMap iteration is
    /// rel-sorted: [`crate::identity::compute_identity`]'s
    /// byte-determinism basis. Records carry no content hash (D17) —
    /// hashing happens only at the identity assembly point.
    pub fn records(&self) -> BTreeMap<String, crate::identity::SourceRecord> {
        self.py
            .iter()
            .chain(self.rs.iter())
            .chain(self.js.iter())
            .chain(self.ts.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Disk paths for the given faces (Python/JS/TS sets are already
    /// governed — profile-excluded at walk time; Rust is unfiltered).
    pub fn paths_for_faces(
        &self,
        faces: &BTreeSet<crate::language::LanguageFace>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for face in faces {
            match face {
                crate::language::LanguageFace::Python => out.extend(self.py.keys().cloned()),
                crate::language::LanguageFace::Rust => out.extend(self.rs.keys().cloned()),
                crate::language::LanguageFace::JavaScript => out.extend(self.js.keys().cloned()),
                crate::language::LanguageFace::TypeScript => out.extend(self.ts.keys().cloned()),
            }
        }
        out
    }

    /// Newest mtime among the given faces only (None when none present).
    pub fn newest_for_faces(
        &self,
        faces: &BTreeSet<crate::language::LanguageFace>,
    ) -> Option<std::time::SystemTime> {
        faces
            .iter()
            .filter_map(|f| self.newest_by_face.get(f).copied())
            .max()
    }

    /// Stable fingerprint of the sorted (face, path) membership for the
    /// given faces — an equality detector for add/delete/rename and
    /// corpus-policy changes, NOT a security boundary. FNV-1a 64 over
    /// `"<face>\0<path>\0"` pairs; BTreeSet iteration is already sorted,
    /// so directory traversal order cannot leak into the hash.
    pub fn fingerprint_for_faces(&self, faces: &BTreeSet<crate::language::LanguageFace>) -> String {
        fn fnv1a64(bytes: &mut u64, s: &str) {
            for b in s.as_bytes() {
                *bytes ^= *b as u64;
                *bytes = bytes.wrapping_mul(0x100000001b3);
            }
        }
        let mut h: u64 = 0xcbf29ce484222325;
        for face in faces {
            let paths = match face {
                crate::language::LanguageFace::Python => &self.py,
                crate::language::LanguageFace::Rust => &self.rs,
                crate::language::LanguageFace::JavaScript => &self.js,
                crate::language::LanguageFace::TypeScript => &self.ts,
            };
            for p in paths.keys() {
                fnv1a64(&mut h, face.meta_name());
                fnv1a64(&mut h, "\0");
                fnv1a64(&mut h, p);
                fnv1a64(&mut h, "\0");
            }
        }
        format!("{h:016x}")
    }
}

/// Walk `repo` for source files, mirroring the producer's corpus rules
/// (dot-dirs + [`SKIP_DIRS`]) plus the repo-owned profile exclusions for
/// the Python/JS/TS faces (Rust exempt — see [`SourceWalk`]). Fail-loud:
/// a walk error is an error, never a silent empty set posing as fresh.
pub fn walk_sources(repo: &Path) -> Result<SourceWalk, String> {
    let root = resolve_repo(repo);
    // JS/TS corpus policy: repo-owned profile exclusions (loaded once
    // per walk — the same effective policy the producer applies, AD-11).
    let profile = crate::profile::load_profile(&root)?;
    let mut out = SourceWalk::default();
    // (dir, under_target): `target/` stays walkable for the python face
    // (pyrefly indexes .py under it) but its `.rs` are cargo OUT_DIR
    // artifacts — never in the rust-analyzer corpus — so the rust face,
    // the newest-mtime signal, and the JS/TS faces (build detection
    // skips `target` entirely; one corpus policy) all skip them.
    // Post-build fresh-eyes finding: this repo carries 49 such .rs;
    // counting them made every heal report missing>=49 and killed the
    // Healed path.
    let mut stack = vec![(root.clone(), false)];
    while let Some((dir, under_target)) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("讀取 {} 失敗：{e}", dir.display()))?;
        for ent in entries.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            let name = ent.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                stack.push((ent.path(), under_target || name == "target"));
            } else if ft.is_file() {
                let Some(face) = crate::language::LanguageFace::from_path(Path::new(&name)) else {
                    continue;
                };
                let rel = ent
                    .path()
                    .strip_prefix(&root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| name.clone());
                // The record capture point (D17): one stat, already paid
                // — size+mtime feed the identity cache gate; the content
                // hash itself happens only inside compute_identity.
                let md = ent.metadata().ok();
                let (size, mtime) = match &md {
                    Some(md) => (
                        md.len(),
                        crate::identity::systemtime_tuple(md.modified().ok()),
                    ),
                    None => (0, (0, 0)),
                };
                let rec = crate::identity::SourceRecord {
                    face,
                    rel: rel.clone(),
                    size,
                    mtime,
                };
                let m = md.and_then(|md| md.modified().ok());
                match face {
                    crate::language::LanguageFace::Python => {
                        if crate::profile::is_excluded(&rel, profile.as_ref()) {
                            continue;
                        }
                        out.py.insert(rel, rec)
                    }
                    crate::language::LanguageFace::Rust => {
                        if under_target {
                            continue;
                        }
                        out.rs.insert(rel, rec)
                    }
                    crate::language::LanguageFace::JavaScript
                    | crate::language::LanguageFace::TypeScript => {
                        if under_target || crate::profile::is_excluded(&rel, profile.as_ref()) {
                            continue;
                        }
                        if face == crate::language::LanguageFace::JavaScript {
                            out.js.insert(rel, rec)
                        } else {
                            out.ts.insert(rel, rec)
                        }
                    }
                };
                if let Some(m) = m {
                    if out.newest.is_none_or(|n| m > n) {
                        out.newest = Some(m);
                    }
                    if out.newest_by_face.get(&face).is_none_or(|n| m > *n) {
                        out.newest_by_face.insert(face, m);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The index↔source staleness signals. `head_drift=None` means "no
/// head information" (unstamped meta, or git absent — SM-16): the
/// unstamped WARN is source_line's single source, never duplicated here.
/// `doc_set_drift`/`corpus_policy_drift=None` = legacy metadata without
/// the S4 fingerprint keys (baseline mtime behavior applies).
///
/// Source identity EP: `identity_drift=None` = the meta carries no
/// comparable identity pair (legacy slot — zero hash cost, D8), and the
/// mtime/fingerprint signals keep their baseline authority. When
/// `identity_drift` is `Some`, the decision flips to the
/// content-addressed axis: mtime-newness is no longer fatal (touch is
/// idempotent, D3) and `identity_drift` replaces it, while
/// `doc_set_drift`/`corpus_policy_drift` keep being REPORTED (feeding
/// the freshness reasons and the SM-16 degradation disclosure) without
/// being fatal. `graph_lags` is split out of `source_newer` and stays
/// an UNCONDITIONAL rebuild trigger (muse P1-3 torn-plane guard, judge
/// R1 — never short-circuited by a matching identity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalenessSnapshot {
    /// Raw mtime-newness of the scoped corpus (no graph_lags folding).
    pub source_newer: bool,
    /// graph.db older than the slot — torn data plane (muse P1-3).
    pub graph_lags: bool,
    pub doc_set_drift: Option<bool>,
    pub corpus_policy_drift: Option<bool>,
    /// `Some(current != stamped)` when the meta carries a comparable
    /// identity pair; `None` = legacy (not computed).
    pub identity_drift: Option<bool>,
    pub head_drift: Option<bool>,
    /// Faces the evaluation was scoped to (stamped ∪ detected on auto,
    /// pinned on explicit) — the freshness face's `faces` field.
    pub eval_faces: BTreeSet<crate::language::LanguageFace>,
    /// Stamped identity value (identity mode only; None = legacy).
    pub stamped_identity: Option<String>,
    /// Current-side identity value (identity mode only; None = legacy —
    /// not computed at all, D8 zero-cost face).
    pub current_identity: Option<String>,
}

impl StalenessSnapshot {
    /// The rebuild decision — identity-authoritative (source identity
    /// EP): in identity mode a content drift replaces mtime-newness as
    /// the fatal signal; in legacy mode the baseline trio applies. The
    /// torn-plane guard fires in BOTH modes, unconditionally.
    pub fn needs_rebuild(&self) -> bool {
        self.identity_drift == Some(true)
            || self.graph_lags
            || (self.identity_drift.is_none()
                && (self.source_newer
                    || self.doc_set_drift == Some(true)
                    || self.corpus_policy_drift == Some(true)))
    }
}

/// Fingerprint of the JS/TS-relevant exclusion policy: `<none>` sentinel
/// when the repo carries no profile (creation of one is then a visible
/// drift), otherwise a stable hash over the sorted `exclude` prefixes —
/// the only profile section that shapes the JS/TS corpus (AD-11).
pub fn js_ts_profile_fingerprint(repo: &Path) -> String {
    let Ok(profile) = crate::profile::load_profile(repo) else {
        return "<none>".to_string();
    };
    let Some(p) = profile else {
        return "<none>".to_string();
    };
    let mut h: u64 = 0xcbf29ce484222325;
    for prefix in &p.exclude {
        for b in prefix.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn parse_stamped_faces(
    meta: &serde_json::Value,
) -> Option<BTreeSet<crate::language::LanguageFace>> {
    let arr = meta.get("source_faces")?.as_array()?;
    let mut out = BTreeSet::new();
    for v in arr {
        let name = v.as_str()?;
        let face = match name {
            "python" => crate::language::LanguageFace::Python,
            "rust" => crate::language::LanguageFace::Rust,
            "javascript" => crate::language::LanguageFace::JavaScript,
            "typescript" => crate::language::LanguageFace::TypeScript,
            _ => return None, // unknown face name — treat as legacy meta
        };
        out.insert(face);
    }
    Some(out)
}

/// Cheap staleness evaluation (Stage A): walk + stats + meta read. Zero
/// producer spawns (a stamped meta adds one `git rev-parse`) and zero
/// protobuf parses — the steady-state query cost. With S4 face metadata
/// present, the mtime signal is face-scoped (an explicit python-only
/// slot is not staled by a newer `.ts`) and the two fingerprint
/// comparisons cover add/delete/rename and JS/TS corpus-policy changes
/// that mtime alone cannot see. For AUTO-selected indexes the eval
/// scope is the union of stamped faces and currently-detected disk
/// faces — a newly arrived language (or a profile un-exclusion that
/// reveals one) must trigger the heal that would then rebuild all
/// faces; an EXPLICIT producer override keeps its pinned faces (muse
/// P0-1). A graph.db older than the slot is a torn data plane (index
/// published, graph build failed) and forces a heal (muse P1-3).
pub fn evaluate_staleness(
    repo: &Path,
    slot: &Path,
    policy: crate::identity::IdentityCachePolicy,
) -> Result<StalenessSnapshot, String> {
    let root = resolve_repo(repo);
    let walk = walk_sources(repo)?;
    let slot_m = slot
        .metadata()
        .and_then(|m| m.modified())
        .map_err(|e| format!("stat {} 失敗：{e}", slot.display()))?;
    let meta = load_meta(slot).0;
    let stamped_faces = meta.as_ref().and_then(parse_stamped_faces);
    // selection mode: "explicit" = operator-forced single family (pinned
    // face scope); "auto" or absent (legacy) = union with detected faces
    let explicit_selection = meta
        .as_ref()
        .and_then(|m| m["selection"].as_str().map(str::to_string))
        .is_some_and(|s| s == "explicit");
    let detected_faces: Option<BTreeSet<crate::language::LanguageFace>> = if explicit_selection {
        None
    } else {
        let mut f = BTreeSet::new();
        if !walk.py.is_empty() {
            f.insert(crate::language::LanguageFace::Python);
        }
        if !walk.rs.is_empty() {
            f.insert(crate::language::LanguageFace::Rust);
        }
        if !walk.js.is_empty() {
            f.insert(crate::language::LanguageFace::JavaScript);
        }
        if !walk.ts.is_empty() {
            f.insert(crate::language::LanguageFace::TypeScript);
        }
        Some(f)
    };
    let eval_faces: Option<BTreeSet<crate::language::LanguageFace>> = stamped_faces
        .as_ref()
        .map(|stamped| {
            let mut union = stamped.clone();
            if let Some(detected) = &detected_faces {
                union.extend(detected.iter().copied());
            }
            union
        })
        .or(detected_faces);
    let source_newer = match &eval_faces {
        Some(faces) => walk.newest_for_faces(faces).is_some_and(|n| n > slot_m),
        None => walk.newest.is_some_and(|n| n > slot_m),
    };
    // The fingerprint compares the STAMPED faces' paths against disk —
    // the union adds mtime coverage; cross-face doc-set drift for a
    // newly arrived face is carried by source_newer (its file is new,
    // hence newer than the slot).
    let fp = meta
        .as_ref()
        .and_then(|m| m["source_set_fingerprint"].as_str().map(str::to_string));
    let doc_set_drift = match (&stamped_faces, &fp) {
        (Some(faces), Some(stamped_fp)) => Some(walk.fingerprint_for_faces(faces) != *stamped_fp),
        _ => None,
    };
    let has_js_ts = stamped_faces.as_ref().is_some_and(|f| {
        f.contains(&crate::language::LanguageFace::JavaScript)
            || f.contains(&crate::language::LanguageFace::TypeScript)
    });
    let policy_fp = meta
        .as_ref()
        .and_then(|m| m["js_ts_profile_fingerprint"].as_str().map(str::to_string));
    let corpus_policy_drift = match (has_js_ts, policy_fp) {
        (true, Some(stamped)) => Some(js_ts_profile_fingerprint(repo) != stamped),
        _ => None,
    };
    // Torn data plane: the slot published but the graph build failed —
    // the prior graph lags the slot forever unless this forces the heal
    // (muse P1-3; graph normally lands milliseconds after the slot).
    // SPLIT out of source_newer (source identity EP): needs_rebuild
    // consumes it unconditionally — identity equality must never
    // short-circuit the torn-plane guard (judge R1).
    let graph_lags = slot
        .parent()
        .map(|d| d.parent().map(|p| p.join("graph.db")))
        .flatten()
        .and_then(|g| g.metadata().ok())
        .and_then(|m| m.modified().ok())
        .is_some_and(|gm| gm < slot_m);
    let stamped = meta
        .as_ref()
        .and_then(|m| m["head"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty());
    let head_drift = match stamped {
        None => None,
        Some(idx) => match git_head(repo) {
            Ok(head) => Some(idx != head),
            Err(_) => None,
        },
    };
    // Identity axis (source identity EP): computed ONLY when the meta
    // carries the full comparable triple (source_faces + source_identity
    // + identity_algo) — any absence or algo mismatch is a legacy slot
    // and pays ZERO hash cost (D8/R22: no partial triple ever bombs).
    // The current side is scoped to eval_faces — the exact mirror of
    // the mtime scope (auto = stamped ∪ detected: a newly arrived
    // language must drift; explicit = pinned). Read failure is
    // fail-loud (D15) — the caller owns the degradation face.
    let stamped_identity_val = meta
        .as_ref()
        .and_then(|m| m["source_identity"].as_str().map(str::to_string));
    let stamped_algo = meta
        .as_ref()
        .and_then(|m| m["identity_algo"].as_str().map(str::to_string));
    let mut identity_drift = None;
    let mut current_identity = None;
    if let (Some(eval), Some(stamped_val)) = (&eval_faces, &stamped_identity_val) {
        if stamped_algo.as_deref() == Some(crate::identity::IDENTITY_ALGO) {
            let policy = crate::identity::resolve_policy(policy);
            let mut cache = crate::identity::IdentityCache::load(
                crate::identity::cache_path_for_slot(slot),
                &root,
            );
            let current = crate::identity::compute_identity(
                &root,
                &walk.records(),
                eval,
                policy,
                &mut cache,
            )?;
            identity_drift = Some(current.value != *stamped_val);
            current_identity = Some(current.value);
        }
    }
    Ok(StalenessSnapshot {
        source_newer,
        graph_lags,
        doc_set_drift,
        corpus_policy_drift,
        identity_drift,
        head_drift,
        eval_faces: eval_faces.unwrap_or_default(),
        stamped_identity: if identity_drift.is_some() {
            stamped_identity_val
        } else {
            None
        },
        current_identity,
    })
}

/// Doc-set delta between a loaded index and the disk walk. Face-scoped:
/// only extensions already present in the index are compared, so a
/// python-face index never goes false-stale over stray .rs files (the
/// reverse blind spot — index language ≠ repo language — is a recorded
/// v1 boundary). Python/JS/TS compare against the governed
/// (profile-filtered) walk sets — the same corpus policy the producer
/// applies; Rust compares unfiltered (recorded exemption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocDelta {
    pub missing: usize,
    pub examples: Vec<String>,
    pub extra: usize,
}

pub fn doc_set_delta(docs: &BTreeSet<String>, walk: &SourceWalk) -> DocDelta {
    let has_py = docs.iter().any(|d| d.ends_with(".py"));
    let has_rs = docs.iter().any(|d| d.ends_with(".rs"));
    let has_js = docs.iter().any(|d| {
        [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|e| d.ends_with(e))
    });
    let has_ts = docs
        .iter()
        .any(|d| d.ends_with(".ts") || d.ends_with(".tsx"));
    let mut disk: BTreeSet<&String> = BTreeSet::new();
    if has_py {
        disk.extend(walk.py.keys());
    }
    if has_rs {
        disk.extend(walk.rs.keys());
    }
    if has_js {
        disk.extend(walk.js.keys());
    }
    if has_ts {
        disk.extend(walk.ts.keys());
    }
    let missing_list: Vec<&String> = disk
        .iter()
        .copied()
        .filter(|p| !docs.contains(*p))
        .collect();
    DocDelta {
        missing: missing_list.len(),
        examples: missing_list.iter().take(3).map(|s| s.to_string()).collect(),
        extra: docs.iter().filter(|d| !disk.contains(d)).count(),
    }
}

// ---------- stamp core (S4) ----------

/// Stamp-write failure split so the cli `--stamp-meta` mode keeps its two
/// frozen faces (git-warn + HEAD fail; write fail) while the refresh
/// head-sync renders both to stderr.
#[derive(Debug)]
pub enum StampError {
    Git(String),
    Write(String),
}

impl std::fmt::Display for StampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StampError::Git(w) | StampError::Write(w) => write!(f, "{}", w.trim_end()),
        }
    }
}

/// Stamp-write core shared by the umbrella build (face-accurate producer
/// string from the legs it ran), the refresh head-sync (preserves the
/// existing producer value), and the cli `--stamp-meta` mode (frozen
/// stdout face; resolve-on-fresh). Returns the stamped head; the payload
/// key order (repo/head/stamped_at/tool/producer) is the frozen Python
/// dict order.
pub fn stamp_meta_core(
    repo: &Path,
    index_path: &Path,
    roots: &[PathBuf],
    producer: Option<&str>,
    selection: Option<&str>,
) -> Result<String, StampError> {
    let head = git_head(repo).map_err(StampError::Git)?;
    // Face-accurate provenance: explicit wins; otherwise preserve an
    // existing stamp (head-sync) and only resolve on a fresh stamp —
    // never blanket-stamp pyrefly onto a rust-face index.
    let producer = match producer {
        Some(p) => p.to_string(),
        None => load_meta(index_path)
            .0
            .and_then(|m| m["producer"].as_str().map(str::to_string))
            .filter(|s| !s.is_empty() && s != "<unresolved>")
            .unwrap_or_else(|| {
                crate::common::producer_version("pyrefly-index", roots)
                    .unwrap_or("<unresolved>".to_string())
            }),
    };
    // Selection mode ("auto" | "explicit"): drives the freshness scope —
    // auto unions stamped faces with detected disk faces (a newly
    // arrived language must heal, muse P0-1); explicit keeps the pinned
    // face (operator-owned partial build). Preserved on head-sync like
    // the producer string.
    let selection = match selection {
        Some(s) => s.to_string(),
        None => load_meta(index_path)
            .0
            .and_then(|m| m["selection"].as_str().map(str::to_string))
            .unwrap_or_else(|| "auto".to_string()),
    };
    let sidecar = meta_path(index_path);
    let mut payload = serde_json::json!({
        "repo": repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()).display().to_string(),
        "head": head,
        "stamped_at": utc_now_iso(),
        "tool": "code_reality.scip_refs",
        "producer": producer,
        "selection": selection,
    });
    // S4 identity keys (the source-set fingerprint family) describe the
    // INDEX's corpus contract, not the disk. Fresh pairs are stamped
    // ONLY when the index document set equals the current disk corpus
    // for the index's faces (one walk — stamping a fingerprint of
    // drifted disk state over an old index would launder a delete into
    // freshness). On a mismatch (or failed recompute) the PRIOR keys are
    // PRESERVED verbatim: they still truthfully describe this unchanged
    // index, so the next staleness evaluation compares them against
    // drifted disk and the delete/rename stays visible (codex blocker —
    // the earlier drop-the-keys design degraded to mtime-only). Never
    // fabricate keys: a legacy keyless meta stays keyless until a real
    // rebuild stamps a consistent pair.
    let prior = load_meta(index_path).0;
    let preserve_prior_keys = |payload: &mut serde_json::Value| {
        if let Some(m) = &prior {
            for key in [
                "source_faces",
                "source_set_fingerprint",
                "js_ts_profile_fingerprint",
                "source_identity",
                "identity_algo",
            ] {
                if let Some(v) = m.get(key) {
                    if !v.is_null() {
                        payload[key] = v.clone();
                    }
                }
            }
        }
    };
    let mut stamped_fresh_keys = false;
    if let Ok(loaded) = load_index(index_path) {
        let faces: BTreeSet<crate::language::LanguageFace> = loaded
            .index
            .documents
            .iter()
            .filter_map(|d| crate::language::LanguageFace::from_path(Path::new(&d.relative_path)))
            .collect();
        if !faces.is_empty() {
            if let Ok(walk) = walk_sources(repo) {
                let disk = walk.paths_for_faces(&faces);
                let docs: BTreeSet<String> = loaded
                    .index
                    .documents
                    .iter()
                    .map(|d| d.relative_path.clone())
                    .collect();
                if disk == docs {
                    // Identity first (source identity EP): its recompute
                    // must succeed before ANY fresh key lands — a
                    // failure here keeps stamped_fresh_keys false and
                    // the preserve branch below stays truthful (failed
                    // recompute ≡ mismatch; a partial fresh stamp would
                    // pair a new fingerprint with a stale identity).
                    // D13: Full policy — the indexed identity always
                    // comes from actual bytes, never a query-side cache.
                    let policy =
                        crate::identity::resolve_policy(crate::identity::IdentityCachePolicy::Full);
                    let mut cache = crate::identity::IdentityCache::load(
                        crate::identity::cache_path_for_slot(index_path),
                        repo,
                    );
                    match crate::identity::compute_identity(
                        &resolve_repo(repo),
                        &walk.records(),
                        &faces,
                        policy,
                        &mut cache,
                    ) {
                        Ok(identity) => {
                            let names: Vec<&str> = faces.iter().map(|f| f.meta_name()).collect();
                            payload["source_faces"] = serde_json::json!(names);
                            payload["source_set_fingerprint"] =
                                serde_json::json!(walk.fingerprint_for_faces(&faces));
                            if faces.contains(&crate::language::LanguageFace::JavaScript)
                                || faces.contains(&crate::language::LanguageFace::TypeScript)
                            {
                                payload["js_ts_profile_fingerprint"] =
                                    serde_json::json!(js_ts_profile_fingerprint(repo));
                            }
                            payload["source_identity"] = serde_json::json!(identity.value);
                            payload["identity_algo"] = serde_json::json!(identity.algo);
                            stamped_fresh_keys = true;
                        }
                        Err(_) => {} // → preserve branch
                    }
                }
            }
        }
    }
    let prior_had_keys = prior.as_ref().is_some_and(|m| {
        m.get("source_set_fingerprint")
            .is_some_and(|v| !v.is_null())
            || m.get("source_identity").is_some_and(|v| !v.is_null())
    });
    if !stamped_fresh_keys {
        preserve_prior_keys(&mut payload);
        if prior_had_keys {
            eprintln!(
                "[WARN] stamp-meta：索引文檔集與磁碟語料不一致——保留既有 source-set fingerprint／source identity（drift 保持可見；重跑 build 產出一致配對）\n"
            );
        }
    }
    let text = format!("{}\n", serde_json::to_string_pretty(&payload).unwrap());
    std::fs::write(&sidecar, &text)
        .map_err(|e| StampError::Write(format!("sidecar 寫入失敗：{}", e)))?;
    Ok(head)
}

/// Load stamp sidecar; corrupt/missing shapes → WARN + None (:596).
pub fn load_meta(index_path: &Path) -> (Option<serde_json::Value>, Vec<String>) {
    let p = meta_path(index_path);
    if !p.exists() {
        return (None, Vec::new());
    }
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) => {
            return (
                None,
                vec![format!(
                    "[WARN] index meta 損壞（[SRC] 缺 index 版本）：{}\n",
                    e
                )],
            )
        }
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Err(e) => (
            None,
            vec![format!(
                "[WARN] index meta 損壞（[SRC] 缺 index 版本）：{}\n",
                e
            )],
        ),
        Ok(v) if v.is_object() && v["head"].is_string() => (Some(v), Vec::new()),
        Ok(_) => (
            None,
            vec!["[WARN] index meta 形狀非預期（[SRC] 缺 index 版本）\n".to_string()],
        ),
    }
}

/// Live repo HEAD via `git rev-parse` (:611); Err carries the verbatim WARN
/// line (git missing / rev-parse failure — timeout is a documented deviation:
/// std Command has no timeout, rev-parse returns instantly in practice).
fn sidecar_head_of(index_path: &Path) -> String {
    load_meta(index_path)
        .0
        .and_then(|m| m["head"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Stamped meta head of an index ("" when absent) — shared by both derived
/// artifacts' staleness guards.
pub fn stamped_head(index_path: &Path) -> String {
    sidecar_head_of(index_path)
}

/// No-DEF report fragment (frozen family text): `[SRC]` first when present,
/// then `[WARN] 查無 DEF` with exit 1 (byte-identical to report()'s no-DEF
/// branch; used by the R3 modes).
pub fn no_def_lines(src_line: Option<&str>, query: &str) -> (String, i32) {
    let mut out = String::new();
    if let Some(line) = src_line {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!("[WARN] 查無 DEF：{}\n", query));
    (out, 1)
}

pub fn git_head(repo: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|_| "[WARN] git 不在 PATH——[SRC] 略過 repo HEAD\n".to_string())?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !s.is_empty() {
        return Ok(s);
    }
    Err(format!(
        "[WARN] git rev-parse 失敗——[SRC] 略過 repo HEAD：{}\n",
        String::from_utf8_lossy(&out.stderr).trim_end()
    ))
}

/// UTC now as `YYYY-MM-DDTHH:MM:SS+00:00` — parity with Python
/// `datetime.now(UTC).isoformat(timespec="seconds")` (scip_refs.py:710).
/// Hand-rolled civil-from-days (Hinnant) to avoid a chrono dependency.
pub fn utc_now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// Python `str(value)[:10]` coercion for the `stamped_at` sidecar field
/// (scip_refs.py:667): numbers/bools/null stringify (None/True/False), not "".
fn py_str_coerced(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// `[SRC]` assembly + the three stderr WARN guards (source_line, :640).
/// Returns ([SRC] stdout line or None, stderr WARN lines).
pub fn source_line(index_path: &Path, repo: Option<&Path>) -> (Option<String>, Vec<String>) {
    let mut warns: Vec<String> = Vec::new();
    let stale_stamp = match (meta_path(index_path).metadata(), index_path.metadata()) {
        (Ok(m), Ok(i)) => match (m.modified(), i.modified()) {
            (Ok(mt), Ok(it)) => mt < it,
            _ => false,
        },
        _ => false,
    };
    let (meta, meta_warns) = load_meta(index_path);
    warns.extend(meta_warns);
    let idx_sha: Option<String> = meta
        .as_ref()
        .and_then(|m| m["head"].as_str())
        .map(str::to_string);
    let repo_sha: Option<String> = repo.and_then(|r| match git_head(r) {
        Ok(head) => Some(head),
        Err(warn) => {
            warns.push(warn);
            None
        }
    });
    if idx_sha.is_none() && repo_sha.is_none() {
        return (None, warns);
    }
    if stale_stamp && meta.is_some() {
        warns.push(
            "[WARN] stamp 比索引檔舊——索引重生成後未重 stamp（跑 --stamp-meta）\n".to_string(),
        );
    }
    let mut parts: Vec<String> = Vec::new();
    match idx_sha.as_deref().filter(|s| !s.is_empty()) {
        Some(sha) => {
            // Python `str(meta.get("stamped_at", ""))[:10]`: absent key → ""
            // (no date part); present null → "None"; numbers/bools stringify.
            let stamped = meta
                .as_ref()
                .and_then(|m| m.get("stamped_at").map(py_str_coerced))
                .unwrap_or_default();
            let date: String = stamped.chars().take(10).collect();
            if date.is_empty() {
                parts.push(format!("scip index @ {}", short(sha)));
            } else {
                parts.push(format!("scip index @ {}（{}）", short(sha), date));
            }
        }
        _ => {
            // Divergence-aware remediation (S2): stamping a stale index
            // formalizes it — when sources moved past the slot, point at
            // rebuild instead of re-stamp.
            let drift = repo
                .and_then(|r| walk_sources(r).ok())
                .and_then(|w| w.newest)
                .zip(index_path.metadata().and_then(|m| m.modified()).ok())
                .is_some_and(|(n, s)| n > s);
            warns.push(
                if drift {
                    "[WARN] index meta 未 stamp 且原始碼已較新——索引過期：跑 code-reality build\n"
                } else {
                    "[WARN] index meta 未 stamp（生成後跑 --stamp-meta）——[SRC] 缺 index 版本\n"
                }
                .to_string(),
            );
        }
    }
    if let Some(sha) = &repo_sha {
        parts.push(format!("repo HEAD @ {}", short(sha)));
    }
    if let (Some(idx), Some(rs)) = (&idx_sha, &repo_sha) {
        if let Some(m) = &meta {
            let stamped_repo = m["repo"].as_str().unwrap_or("");
            if let Some(r) = repo {
                if !stamped_repo.is_empty() {
                    let resolved = r.canonicalize().unwrap_or_else(|_| r.to_path_buf());
                    if stamped_repo != resolved.to_string_lossy() {
                        warns.push(format!(
                            "[WARN] stamp 的 repo（{}）與 --repo 不符——index sha 歸屬可能錯（同名 basename？改用顯式 --index）\n",
                            stamped_repo
                        ));
                    }
                }
            }
        }
        if idx != rs {
            warns.push(format!(
                "[WARN] repo HEAD 已離開 index 生成點（index @ {} vs HEAD @ {}）——重生索引並重跑 --stamp-meta後再查\n",
                short(idx),
                short(rs)
            ));
        }
    }
    (Some(format!("[SRC] {}", parts.join(" · "))), warns)
}
