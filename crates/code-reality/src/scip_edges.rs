//! `scip_edges` — the SCIP reference-edge derivation library (v1+ S1;
//! CLI/inject sidecar faces retired with the v1+ S4 flip — the union
//! plane now materializes inside `.code-reality/graph.db` via
//! `graph_db build`). This module stays as the derivation oracle the
//! `graph_db` tests reconcile against.
//!
//! Edge semantics = the `scip_refs --callers` face: every is_def=0
//! occurrence attributed to the innermost enclosing fn span — reference
//! edges, NOT call-only (old-schema index has no is_call_reference).
//! Derivation keeps `kind='REFERENCES'` on the semantic axis.
//!
//! Workspace filter: the callee must carry a DEF in the index (the NT
//! corpus index holds zero external paths — referenced std/core symbols
//! simply have no DEF occurrence, so DEF membership IS the test). The
//! caller side needs no check by construction — callers come from
//! fn-span DEFs, a subset of the DEF universe (invariant holds only
//! while spans derive from DEFs; revisit if spans_source ever admits
//! non-DEF sources). Skipped external edges stay visible in the report.

use crate::cache::{self, Face};
use crate::engine::{fn_tail_name, ln};
use crate::fndefs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// (symbol, rel_path, line) — one scan-ordered occurrence row.
type OccRow = (String, String, i64);

pub struct EdgeRow {
    pub caller: String,
    pub callee: String,
    pub sites: usize,
}

pub struct DeriveReport {
    pub ref_sites: usize,
    pub item_level: usize,
    pub edges_total: usize,
    pub edges_workspace: usize,
    pub external_skipped: usize,
}

/// Keep only edges whose callee has a DEF in the index.
pub fn filter_workspace(edges: Vec<EdgeRow>, defs: &BTreeSet<String>) -> (Vec<EdgeRow>, usize) {
    let mut kept = Vec::new();
    let mut skipped = 0usize;
    for e in edges {
        if defs.contains(&e.callee) {
            kept.push(e);
        } else {
            skipped += 1;
        }
    }
    (kept, skipped)
}

/// Ref rows + DEF symbols via the family face ladder (fresh sqlite cache
/// → protobuf in-memory; never builds the cache on miss). The sqlite face
/// stores only fn-tailed symbols' occurrences (cache::build_db filter) —
/// the protobuf branch mirrors that (`fn_tail_name` gate) so both faces
/// carry the same fn-callee universe.
fn scan_rows_and_defs(face: &Face) -> Result<(Vec<OccRow>, BTreeSet<String>), String> {
    match face {
        Face::Sqlite(conn) => {
            let mut rows = Vec::new();
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT symbol, rel_path, line FROM occurrences \
                         WHERE is_def = 0 ORDER BY seq",
                    )
                    .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
                let it = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })
                    .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
                for row in it {
                    rows.push(row.map_err(|e| format!("scip_edges 掃描失敗：{e}"))?);
                }
            }
            let mut defs = BTreeSet::new();
            let mut stmt = conn
                .prepare("SELECT DISTINCT symbol FROM occurrences WHERE is_def = 1")
                .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
            let it = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
            for s in it {
                defs.insert(s.map_err(|e| format!("scip_edges 掃描失敗：{e}"))?);
            }
            Ok((rows, defs))
        }
        Face::Protobuf { index } => {
            let mut rows = Vec::new();
            let mut defs = BTreeSet::new();
            for d in &index.documents {
                for occ in &d.occurrences {
                    if fn_tail_name(&occ.symbol).is_none() {
                        continue;
                    }
                    if occ.symbol_roles & 1 != 0 {
                        defs.insert(occ.symbol.clone());
                    } else {
                        rows.push((occ.symbol.clone(), d.relative_path.clone(), ln(occ)));
                    }
                }
            }
            Ok((rows, defs))
        }
    }
}

/// Full derivation (POC A1 semantics through the real lib ladder).
/// Returns ALL edges (external included) in (caller, callee) order; the
/// report carries the workspace split.
fn derive_internal(
    index_path: &Path,
) -> Result<(Vec<EdgeRow>, BTreeSet<String>, DeriveReport, Vec<String>), String> {
    #[allow(clippy::type_complexity)] // derivation aggregate, self-describing in context
    let (face, mut warns) = cache::open_face(index_path)?;
    let (rows, defs) = scan_rows_and_defs(&face)?;
    // Mixed-face cost note: a fresh sqlite cache + missing fndefs sidecar
    // makes the Sqlite arm re-parse the full protobuf for spans (accepted;
    // the sidecar ladder rebuilds itself on first touch).
    let spans_result = match &face {
        Face::Protobuf { index } => fndefs::spans_source(index_path, Some(index)),
        Face::Sqlite(_) => fndefs::spans_source(index_path, None),
    };
    let (spans, span_warns) = spans_result?;
    warns.extend(span_warns);

    let ref_sites = rows.len();
    let mut by_callee: BTreeMap<String, Vec<OccRow>> = BTreeMap::new();
    for (sym, rel, line) in rows {
        by_callee
            .entry(sym.clone())
            .or_default()
            .push((sym, rel, line));
    }
    let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut item_level = 0usize;
    for (callee, group) in &by_callee {
        let res = crate::callers::attribute(group, &spans);
        item_level += res.item_level.len();
        for c in &res.callers {
            *edges.entry((c.symbol.clone(), callee.clone())).or_insert(0) += c.sites.len();
        }
    }
    let all: Vec<EdgeRow> = edges
        .into_iter()
        .map(|((caller, callee), sites)| EdgeRow {
            caller,
            callee,
            sites,
        })
        .collect();
    let edges_workspace = all.iter().filter(|e| defs.contains(&e.callee)).count();
    let report = DeriveReport {
        ref_sites,
        item_level,
        edges_total: all.len(),
        edges_workspace,
        external_skipped: all.len() - edges_workspace,
    };
    Ok((all, defs, report, warns))
}

/// Public derive: ALL edges (external included) + report + ladder WARNs.
/// The `graph_db build` test face reconciles its site multiset against
/// this (same spans-based attribution).
pub fn derive_edges(
    index_path: &Path,
) -> Result<(Vec<EdgeRow>, DeriveReport, Vec<String>), String> {
    let (all, _defs, report, warns) = derive_internal(index_path)?;
    Ok((all, report, warns))
}
