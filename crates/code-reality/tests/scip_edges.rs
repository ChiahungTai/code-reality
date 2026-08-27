//! v1+ S1 derivation tests (S4-retired faces pruned): derive_edges parity
//! with the R3 callers oracle on the committed rich_callers.scip fixture,
//! workspace filtering (callee must carry a DEF in the index — the NT
//! index holds zero external paths, so DEF membership IS the workspace
//! test). The sidecar inject/export/CLI faces retired with the v1+ S4
//! flip; this module is the derivation oracle `graph_db` reconciles
//! against.

use code_reality::scip_edges::{self, EdgeRow};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const FIXTURE: &str = "tests/fixtures/rich_callers.scip";

// ---------- derive_edges (R3 oracle parity) ----------

#[test]
fn derive_matches_callers_oracle_for_open() {
    let (edges, report, _warns) = scip_edges::derive_edges(Path::new(FIXTURE)).unwrap();
    // R3 callers oracle on this fixture: `--callers EventStoreLifecycle.open`
    // = 8 callers / 9 sites (tests/callers_cli.rs EXPECTED_CALLERS).
    let mut got: BTreeMap<String, usize> = BTreeMap::new();
    for e in &edges {
        if code_reality::engine::fn_tail_name(&e.callee) == Some("open") {
            let name = code_reality::engine::fn_tail_name(&e.caller)
                .unwrap_or(&e.caller)
                .to_string();
            *got.entry(name).or_insert(0) += e.sites;
        }
    }
    let want: BTreeMap<String, usize> = [
        ("macro_fn", 1),
        ("tie_one", 1),
        ("inner", 1),
        ("outer", 1),
        ("delegate", 1),
        ("t_one", 1),
        ("t_two", 2),
        ("cycle_a", 1),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    assert_eq!(
        got, want,
        "caller multiset for callee tail 'open' must match the R3 oracle"
    );
    assert!(
        report.item_level >= 1,
        "the known unattributed site (crates/a.rs:999)"
    );
}

#[test]
fn derive_report_totals_add_up_and_rows_sorted() {
    let (edges, report, _warns) = scip_edges::derive_edges(Path::new(FIXTURE)).unwrap();
    assert_eq!(edges.len(), report.edges_total);
    assert_eq!(
        report.edges_workspace + report.external_skipped,
        report.edges_total
    );
    assert!(report.ref_sites > 0);
    let sorted = edges.windows(2).all(|w| {
        (w[0].caller.as_str(), w[0].callee.as_str()) <= (w[1].caller.as_str(), w[1].callee.as_str())
    });
    assert!(
        sorted,
        "edges must iterate in (caller, callee) order for a deterministic export face"
    );
}

// ---------- workspace filter ----------

#[test]
fn filter_workspace_drops_callees_without_def() {
    let edges = vec![
        EdgeRow {
            caller: "a().".into(),
            callee: "b().".into(),
            sites: 2,
        },
        EdgeRow {
            caller: "a().".into(),
            callee: "std c().".into(),
            sites: 5,
        },
    ];
    let defs: BTreeSet<String> = ["a().".to_string(), "b().".to_string()].into();
    let (kept, skipped) = scip_edges::filter_workspace(edges, &defs);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].callee, "b().");
    assert_eq!(kept[0].sites, 2);
    assert_eq!(skipped, 1);
}

// ---------- injection ----------
