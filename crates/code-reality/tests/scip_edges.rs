//! v1+ S1 tests: derive_edges parity with the R3 callers oracle on the
//! committed rich_callers.scip fixture, workspace filtering (callee must
//! carry a DEF in the index — the NT index holds zero external paths, so
//! DEF membership IS the workspace test), sidecar union-db injection
//! (idempotent upsert + stale sweep + dry-run no-write), and the CLI
//! faces (export TSV / inject report / guards).

use code_reality::scip_edges::{self, EdgeRow};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const FIXTURE: &str = "tests/fixtures/rich_callers.scip";

fn fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(FIXTURE, &dst).unwrap();
    dst
}

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

#[test]
fn dry_run_creates_no_db() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let rep = scip_edges::inject(&idx, true).unwrap();
    assert!(rep.dry_run);
    assert!(rep.report.edges_workspace > 0);
    assert!(!scip_edges::union_db_path(&idx).exists());
}

#[test]
fn inject_is_idempotent_and_counts_reconcile() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let rep1 = scip_edges::inject(&idx, false).unwrap();
    assert_eq!(rep1.swept, 0);
    assert_eq!(rep1.db_rows, rep1.report.edges_workspace);
    let rep2 = scip_edges::inject(&idx, false).unwrap();
    assert_eq!(rep2.swept, 0, "unchanged index: nothing goes stale");
    assert_eq!(
        rep2.db_rows, rep1.db_rows,
        "idempotent re-inject: zero net growth"
    );
}

#[test]
fn inject_sweeps_stale_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    scip_edges::inject(&idx, false).unwrap();
    // a row that left the index = absent from the new set + old updated_at
    let db = scip_edges::union_db_path(&idx);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO edges (caller_symbol, callee_symbol, sites, updated_at) \
         VALUES ('gone().', 'gone().', 1, 1.0)",
        [],
    )
    .unwrap();
    drop(conn);
    let rep = scip_edges::inject(&idx, false).unwrap();
    assert_eq!(rep.swept, 1);
    assert_eq!(rep.db_rows, rep.report.edges_workspace);
    let conn = rusqlite::Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE caller_symbol = 'gone().'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn delete_sidecar_reinject_is_stable() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let rep1 = scip_edges::inject(&idx, false).unwrap();
    std::fs::remove_file(scip_edges::union_db_path(&idx)).unwrap();
    let rep2 = scip_edges::inject(&idx, false).unwrap();
    assert_eq!(rep2.db_rows, rep1.db_rows);
    assert_eq!(rep2.swept, 0);
}

#[test]
fn union_db_is_index_sibling() {
    let p = scip_edges::union_db_path(Path::new("/x/index.scip"));
    assert_eq!(p, PathBuf::from("/x/index.union.db"));
}

// ---------- CLI faces ----------

fn run_cli(index: &str, extra: &[&str]) -> code_reality::ToolOutput {
    let mut argv: Vec<&str> = vec!["scip_edges", "--index", index];
    argv.extend_from_slice(extra);
    code_reality::scip_edges::run(&argv)
}

#[test]
fn cli_help_and_guards() {
    let h = code_reality::scip_edges::run(&["scip_edges", "-h"]);
    assert_eq!(h.exit_code, 0);
    assert!(h.stdout.contains("usage: scip_edges"));

    let out = code_reality::scip_edges::run(&["scip_edges", "--dry-run"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --dry-run 僅伴 --inject 使用\n");

    let out = code_reality::scip_edges::run(&["scip_edges", "--json"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --json 僅伴 --inject 使用\n");

    let out = code_reality::scip_edges::run(&["scip_edges"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("[FAIL]"));

    let out = code_reality::scip_edges::run(&["scip_edges", "--index", "/nope/none.scip"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("索引不在"));
}

#[test]
fn cli_export_tsv_matches_derive() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let out = run_cli(idx.to_str().unwrap(), &[]);
    assert_eq!(out.exit_code, 0);
    let (_edges, report, _warns) = scip_edges::derive_edges(Path::new(FIXTURE)).unwrap();
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.len(),
        report.edges_total,
        "export face carries the FULL edge set (external included) — one TSV line per pair"
    );
    let cols: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(cols.len(), 3, "caller \\t callee \\t sites");
    assert!(out.stderr.contains("[OK] scip_edges:"));
}

#[test]
fn cli_inject_dry_run_and_json() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let out = run_cli(idx.to_str().unwrap(), &["--inject", "--dry-run", "--json"]);
    assert_eq!(out.exit_code, 0);
    let v: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert!(v["edges_workspace"].as_u64().unwrap() > 0);
    assert!(!scip_edges::union_db_path(&idx).exists());

    let out2 = run_cli(idx.to_str().unwrap(), &["--inject", "--json"]);
    assert_eq!(out2.exit_code, 0);
    let v2: Value = serde_json::from_str(&out2.stdout).unwrap();
    assert_eq!(v2["dry_run"], false);
    assert_eq!(v2["db_rows"], v2["edges_workspace"]);
    assert_eq!(v2["swept"], 0);
}
