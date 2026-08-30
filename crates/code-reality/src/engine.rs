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

/// Symbol matches query (name-tail AND (marker OR trait-decl) for Type.method).
pub fn matches_query(symbol: &str, query: &Query) -> bool {
    match query {
        Query::TypeMethod { type_name, method } => {
            name_pat_match(symbol, method)
                && (symbol.contains(&format!("[{}]", type_name))
                    || trait_decl_match(symbol, type_name))
        }
        Query::Bare { name } => name_pat_match(symbol, name),
    }
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
            if occ.symbol_roles & 1 != 0 && matches_query(&occ.symbol, query) {
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

/// One disk walk feeding both staleness signals: the per-language file
/// sets (doc-set comparison) and the newest source mtime (cheap trigger).
#[derive(Debug, Default)]
pub struct SourceWalk {
    pub py: BTreeSet<String>,
    pub rs: BTreeSet<String>,
    pub newest: Option<std::time::SystemTime>,
}

/// Walk `repo` for source files, mirroring the producer's corpus rules
/// (dot-dirs + [`SKIP_DIRS`]). Fail-loud: a walk error is an error, never
/// a silent empty set posing as fresh.
pub fn walk_sources(repo: &Path) -> Result<SourceWalk, String> {
    let root = resolve_repo(repo);
    let mut out = SourceWalk::default();
    // (dir, under_target): `target/` stays walkable for the python face
    // (pyrefly indexes .py under it) but its `.rs` are cargo OUT_DIR
    // artifacts — never in the rust-analyzer corpus — so the rust face
    // and the newest-mtime signal both skip them. Post-build fresh-eyes
    // finding: this repo carries 49 such .rs; counting them made every
    // heal report missing>=49 and killed the Healed path.
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
                let is_py = name.ends_with(".py");
                if !(is_py || (name.ends_with(".rs") && !under_target)) {
                    continue;
                }
                let rel = ent
                    .path()
                    .strip_prefix(&root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| name.clone());
                if is_py {
                    out.py.insert(rel);
                } else {
                    out.rs.insert(rel);
                }
                if let Ok(m) = ent.metadata().and_then(|md| md.modified()) {
                    if out.newest.is_none_or(|n| m > n) {
                        out.newest = Some(m);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The two index↔source staleness signals. `head_drift=None` means "no
/// head information" (unstamped meta, or git absent — SM-16): the
/// unstamped WARN is source_line's single source, never duplicated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalenessSnapshot {
    pub source_newer: bool,
    pub head_drift: Option<bool>,
}

/// Cheap staleness evaluation (Stage A): walk + stats + meta read. Zero
/// producer spawns (a stamped meta adds one `git rev-parse`) and zero
/// protobuf parses — the steady-state query cost.
pub fn evaluate_staleness(repo: &Path, slot: &Path) -> Result<StalenessSnapshot, String> {
    let walk = walk_sources(repo)?;
    let slot_m = slot
        .metadata()
        .and_then(|m| m.modified())
        .map_err(|e| format!("stat {} 失敗：{e}", slot.display()))?;
    let source_newer = walk.newest.is_some_and(|n| n > slot_m);
    let stamped = load_meta(slot)
        .0
        .and_then(|m| m["head"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty());
    let head_drift = match stamped {
        None => None,
        Some(idx) => match git_head(repo) {
            Ok(head) => Some(idx != head),
            Err(_) => None,
        },
    };
    Ok(StalenessSnapshot {
        source_newer,
        head_drift,
    })
}

/// Doc-set delta between a loaded index and the disk walk. Face-scoped:
/// only extensions already present in the index are compared, so a
/// python-face index never goes false-stale over stray .rs files (the
/// reverse blind spot — index language ≠ repo language — is a recorded
/// v1 boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocDelta {
    pub missing: usize,
    pub examples: Vec<String>,
    pub extra: usize,
}

pub fn doc_set_delta(docs: &BTreeSet<String>, walk: &SourceWalk) -> DocDelta {
    let has_py = docs.iter().any(|d| d.ends_with(".py"));
    let has_rs = docs.iter().any(|d| d.ends_with(".rs"));
    let disk: BTreeSet<&String> = walk
        .py
        .iter()
        .chain(walk.rs.iter())
        .filter(|p| (has_py && p.ends_with(".py")) || (has_rs && p.ends_with(".rs")))
        .collect();
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
    let sidecar = meta_path(index_path);
    let payload = serde_json::json!({
        "repo": repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()).display().to_string(),
        "head": head,
        "stamped_at": utc_now_iso(),
        "tool": "code_reality.scip_refs",
        "producer": producer,
    });
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
