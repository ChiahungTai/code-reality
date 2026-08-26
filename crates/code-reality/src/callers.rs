//! callers — DEF-enc containment attribution + closure BFS (domain, pure
//! functions, zero IO).
//!
//! Mechanism (research §2.1, proven 96.9% attribution coverage): each ref
//! occurrence at (rel_path, line) is attributed to the innermost fn DEF span
//! enclosing it in the same file; that DEF's symbol is the caller, the ref's
//! symbol the callee. Line-granularity known error sources (brace-boundary
//! refs on span start/end lines, same-width ties resolved by scan order) are
//! inherited from the research §7 limitations.
//!
//! Dependency direction (spec clause :194, Rust counterpart): this module
//! imports neither `cli`, `cache`, nor `fndefs` — callers semantics are
//! face-agnostic; face-specific row/span loading is injected by the caller.

use crate::engine::{loc_line, tail, FnSpan};
use std::collections::{BTreeMap, HashMap, HashSet};

/// One attributed caller: the fn DEF symbol whose span enclosed the refs,
/// its defining file, and the call sites (rel_path, 1-based line) in scan
/// order (= the call_edges edge set: caller→callee×site).
pub struct Caller {
    pub symbol: String,
    pub def_path: String,
    pub sites: Vec<(String, i64)>,
}

pub struct CallersResult {
    /// First-site scan order (deterministic; protobuf scan order = sqlite
    /// seq PK insertion order).
    pub callers: Vec<Caller>,
    /// Refs not enclosed by any fn span (use/const/attr layer) — reported,
    /// never silently dropped.
    pub item_level: Vec<(String, i64)>,
}

/// Innermost containment: among the spans in the same file that contain
/// `line` (inclusive bounds), the minimum by `(width, scan seq)` wins —
/// same-width ties go to the first-seen span (EP-canonical rule).
/// Linear scan per row over the same file's spans: bounded by SM-9-scale
/// data (NT passes); revisit with sorted spans + interval search only if
/// pathological indexes show up.
fn innermost(spans: &[FnSpan], line: i64) -> Option<&FnSpan> {
    spans
        .iter()
        .filter(|s| s.start_line <= line && line <= s.end_line)
        // saturating_sub: a hostile-but-fresh sidecar could carry inverted
        // spans; debug builds must not panic on the tie key.
        .min_by_key(|s| (s.end_line.saturating_sub(s.start_line), s.seq))
}

/// Attribute flat scan-ordered ref rows to enclosing fn spans.
/// Rows are `(callee_symbol, rel_path, line)`; the callee field documents
/// provenance (the query face provides it) but does not affect attribution.
pub fn attribute(
    rows: &[(String, String, i64)],
    spans: &BTreeMap<String, Vec<FnSpan>>,
) -> CallersResult {
    let mut result = CallersResult {
        callers: Vec::new(),
        item_level: Vec::new(),
    };
    let mut index: HashMap<String, usize> = HashMap::new();
    for (_callee, rel_path, line) in rows {
        let span = spans.get(rel_path).and_then(|v| innermost(v, *line));
        match span {
            Some(s) => {
                let entry = index.get(&s.symbol);
                match entry {
                    Some(&i) => result.callers[i].sites.push((rel_path.clone(), *line)),
                    None => {
                        index.insert(s.symbol.clone(), result.callers.len());
                        result.callers.push(Caller {
                            symbol: s.symbol.clone(),
                            def_path: s.rel_path.clone(),
                            sites: vec![(rel_path.clone(), *line)],
                        });
                    }
                }
            }
            None => result.item_level.push((rel_path.clone(), *line)),
        }
    }
    result
}

/// One BFS level: newly discovered caller symbols and their per-defining-
/// file aggregation.
pub struct ClosureLevel {
    pub depth: usize,
    pub new_symbols: Vec<String>,
    /// def file → count of new symbols defined there (SM-9 aggregation).
    pub by_file: BTreeMap<String, usize>,
}

pub struct ClosureResult {
    pub levels: Vec<ClosureLevel>,
    /// Expansion results hitting an already-visited symbol (back-edges, self
    /// recursion, and diamond convergences — anything already known).
    pub cycle_reentries: usize,
}

/// BFS over caller edges from seed symbols (already query-resolved). Level
/// k+1 expands level-k symbols via `expand` (exact-symbol callers lookup);
/// anything already visited counts as a cycle re-entry (SM-2).
pub fn closure<F: FnMut(&str) -> CallersResult>(
    seeds: &[String],
    mut expand: F,
    depth: usize,
) -> ClosureResult {
    let mut visited: HashSet<String> = seeds.iter().cloned().collect();
    let mut frontier: Vec<String> = seeds.to_vec();
    let mut result = ClosureResult {
        levels: Vec::new(),
        cycle_reentries: 0,
    };
    for d in 1..=depth {
        let mut new_symbols: Vec<String> = Vec::new();
        let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
        let mut next: Vec<String> = Vec::new();
        for sym in &frontier {
            for caller in expand(sym).callers {
                if visited.contains(&caller.symbol) {
                    result.cycle_reentries += 1;
                    continue;
                }
                visited.insert(caller.symbol.clone());
                *by_file.entry(caller.def_path.clone()).or_default() += 1;
                new_symbols.push(caller.symbol.clone());
                next.push(caller.symbol);
            }
        }
        frontier = next;
        result.levels.push(ClosureLevel {
            depth: d,
            new_symbols,
            by_file,
        });
    }
    result
}

// ---------- output assembly (Rust-native design face; family style) ----------

/// `--callers` report: `[SRC]` first line, one caller line (tail + site
/// count) with its site lines beneath (= the call_edges edge set), then the
/// item-level block (count + list, never silently dropped). Stable shape:
/// a zero-ref DEF still prints the header and a zero item-level line.
/// Sites print in full by design (the edge set IS the product; the query
/// mode's 6-ref truncation does not apply here) — no cap at hub scale.
/// Returns (stdout, exit_code); exit 0 on any DEF hit.
pub fn format_callers(
    query: &str,
    result: &CallersResult,
    src_line: Option<&str>,
) -> (String, i32) {
    let mut out = String::new();
    if let Some(line) = src_line {
        out.push_str(line);
        out.push('\n');
    }
    let sites: usize = result.callers.iter().map(|c| c.sites.len()).sum();
    out.push_str(&format!(
        "[OK] {}：{} callers（{} sites）\n",
        query,
        result.callers.len(),
        sites
    ));
    for c in &result.callers {
        out.push_str(&format!("  {}（{} 處）\n", tail(&c.symbol), c.sites.len()));
        for (path, line) in &c.sites {
            out.push_str(&format!("    {}\n", loc_line(path, *line)));
        }
    }
    out.push_str(&format!(
        "  item-level：{} 處（未歸屬 fn——use/const/屬性層）\n",
        result.item_level.len()
    ));
    for (path, line) in &result.item_level {
        out.push_str(&format!("    {}\n", loc_line(path, *line)));
    }
    (out, 0)
}

/// `--closure` report: per-depth new-caller counts with per-defining-file
/// aggregation, then the cycle-reentry line. Returns (stdout, exit_code);
/// exit 0 on any DEF hit (empty frontiers included).
pub fn format_closure(
    query: &str,
    depth: usize,
    result: &ClosureResult,
    src_line: Option<&str>,
) -> (String, i32) {
    let mut out = String::new();
    if let Some(line) = src_line {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!("[OK] closure：{}（depth={}）\n", query, depth));
    for lvl in &result.levels {
        out.push_str(&format!(
            "  depth {}：{} callers\n",
            lvl.depth,
            lvl.new_symbols.len()
        ));
        for (file, count) in &lvl.by_file {
            out.push_str(&format!("    {}：{} 符號\n", file, count));
        }
    }
    out.push_str(&format!(
        "  cycles：{} 處（frontier 重入已拜訪符號）\n",
        result.cycle_reentries
    ));
    (out, 0)
}
