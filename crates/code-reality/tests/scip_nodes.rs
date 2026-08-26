//! v1+ S2 tests: graph_audit missing → SCIP reconciliation → graph.db
//! node injection. Synthetic end-to-end (fake ra_lookup — no
//! rust-analyzer shell; synthetic .scip index + CRG-shape graph.db via
//! the shared crg_fixture DDL): inject/rollback marker, UNIQUE collision
//! skip, dup-name residual, unmapped guard, dry-run no-write, backup,
//! and the CLI faces.

mod crg_fixture;

use code_reality::graph_audit::{OrderedCounter, RaLookup};
use code_reality::scip_nodes;
use crg_fixture::{make_crg_db, CrgDbSpec, NodeSeed};
use protobuf::Message;
use scip::types::{Document, Index, Occurrence};
use std::path::{Path, PathBuf};

fn occ_def(symbol: &str) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = 1; // DEF
    o.range = vec![4, 0]; // 1-based line 5
    o
}

fn write_index(path: &Path, docs: Vec<Document>) {
    let mut i = Index::new();
    i.documents = docs;
    std::fs::write(path, i.write_to_bytes().unwrap()).unwrap();
}

struct Fixture {
    tmp: tempfile::TempDir,
    repo: PathBuf,
    graph: PathBuf,
    index: PathBuf,
}

fn fixture(defs: &[&str]) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn a() {}\n").unwrap();
    let mut doc = Document::new();
    doc.relative_path = "src/a.rs".to_string();
    for sym in defs {
        doc.occurrences.push(occ_def(sym));
    }
    let docs = vec![doc];
    let index = tmp.path().join("index.scip");
    write_index(&index, docs);
    let graph = tmp.path().join("graph.db");
    make_crg_db(
        &graph,
        &CrgDbSpec {
            nodes: vec![NodeSeed {
                name: "existing_fn".into(),
                qname: format!("{}::existing_fn", repo.join("src/a.rs").display()),
                file_path: repo.join("src/a.rs").display().to_string(),
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .unwrap();
    Fixture {
        tmp,
        repo,
        graph,
        index,
    }
}

fn lookup_with(
    counts: &[(&str, usize)],
) -> Box<dyn for<'a> Fn(&'a Path) -> Result<Option<OrderedCounter>, String>> {
    let counts: Vec<(String, usize)> = counts.iter().map(|(n, c)| (n.to_string(), *c)).collect();
    Box::new(move |_p: &Path| {
        let mut c = OrderedCounter::default();
        for (name, n) in &counts {
            for _ in 0..*n {
                c.bump(name);
            }
        }
        Ok(Some(c))
    })
}

fn node_count(graph: &Path, like: &str) -> i64 {
    let conn = rusqlite::Connection::open(graph).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE qualified_name LIKE ?1",
        [format!("%{like}%")],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn inject_maps_inserts_marks_and_backs_up() {
    let f = fixture(&["x src/missing_fn()."]);
    let lookup = lookup_with(&[("missing_fn", 1)]);
    let rep =
        scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
            .unwrap();
    assert_eq!(rep.missing_total, 1);
    assert_eq!(rep.mapped, 1);
    assert_eq!(rep.inserted, 1);
    assert_eq!(rep.collision_skipped, 0);
    assert_eq!(rep.unmapped, 0);
    assert_eq!(rep.residual_missing, 0, "count-1 gap fully closes");
    // backup created beside the db — and it is a SOUND single-file sqlite
    // (VACUUM INTO, not a WAL-sidecar-orphaned fs::copy)
    let bak = f.graph.with_file_name("graph.db.bak-scip-inject");
    assert!(bak.exists());
    let bconn =
        rusqlite::Connection::open_with_flags(&bak, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let n: i64 = bconn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    assert!(n >= 1, "backup carries the pre-inject node set");
    // marker + shape in the db
    let conn = rusqlite::Connection::open(&f.graph).unwrap();
    let (extra, kind, language): (String, String, String) = conn
        .query_row(
            "SELECT extra, kind, language FROM nodes WHERE qualified_name LIKE '%missing_fn'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(extra, "{\"tier\":\"SCIP\"}");
    assert_eq!(kind, "Function");
    assert_eq!(language, "Rust");
}

#[test]
fn re_inject_collides_and_residuals_on_dup_names() {
    let f = fixture(&["x src/missing_fn()."]);
    // ra sees the same fn name twice in one file; one qname can only
    // carry one node → 1 insert, residual 1
    let lookup = lookup_with(&[("missing_fn", 2)]);
    let rep =
        scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
            .unwrap();
    assert_eq!(rep.inserted, 1);
    assert_eq!(
        rep.residual_missing, 1,
        "dup-name gap beyond one qname is reported, not fabricated"
    );
    // second run: the node exists now → collision skip
    let rep2 =
        scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
            .unwrap();
    assert_eq!(rep2.inserted, 0);
    assert_eq!(rep2.collision_skipped, 1);
    assert_eq!(rep2.residual_missing, 1);
}

#[test]
fn unmapped_missing_is_not_injected() {
    let f = fixture(&["x src/missing_fn()."]);
    // ghost_fn has no SCIP DEF → unmapped, never inserted
    let lookup = lookup_with(&[("missing_fn", 1), ("ghost_fn", 1)]);
    let rep =
        scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
            .unwrap();
    assert_eq!(rep.mapped, 1);
    assert_eq!(rep.unmapped, 1);
    assert_eq!(rep.inserted, 1);
    assert_eq!(node_count(&f.graph, "ghost_fn"), 0);
}

#[test]
fn dry_run_writes_nothing() {
    let f = fixture(&["x src/missing_fn()."]);
    let lookup = lookup_with(&[("missing_fn", 1)]);
    let rep =
        scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, true, true, Some(&*lookup))
            .unwrap();
    assert!(rep.dry_run);
    assert_eq!(rep.inserted, 1, "dry-run still reports what WOULD land");
    assert_eq!(node_count(&f.graph, "missing_fn"), 0);
    assert!(!f.graph.with_file_name("graph.db.bak-scip-inject").exists());
}

#[test]
fn rollback_deletes_only_marked_nodes() {
    let f = fixture(&["x src/missing_fn()."]);
    let lookup = lookup_with(&[("missing_fn", 1)]);
    scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
        .unwrap();
    assert_eq!(node_count(&f.graph, "missing_fn"), 1);
    let n = scip_nodes::rollback_nodes(&f.graph).unwrap();
    assert_eq!(n, 1);
    assert_eq!(node_count(&f.graph, "missing_fn"), 0);
    // the native node survives
    assert_eq!(node_count(&f.graph, "existing_fn"), 1);
}

// ---------- CLI faces ----------

#[test]
fn cli_help_and_missing_repo_fail() {
    let h = code_reality::scip_nodes::run(&["scip_nodes", "-h"]);
    assert_eq!(h.exit_code, 0);
    assert!(h.stdout.contains("usage: scip_nodes"));
    let out = code_reality::scip_nodes::run(&["scip_nodes"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("[FAIL]"));
}

#[test]
fn cli_dry_run_json_on_synthetic() {
    // The CLI production path shells rust-analyzer (RA presence is
    // machine-dependent — same convention as s4_graph_audit).
    let ra = std::process::Command::new("rust-analyzer")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ra {
        eprintln!("skip: rust-analyzer absent");
        return;
    }
    let f = fixture(&["x src/missing_fn()."]);
    // CLI production path uses the live rust-analyzer lookup — point it
    // at the synthetic repo via --graph/--index and assert only the
    // guard/JSON plumbing (missing detection itself is lib-tested above).
    let out = code_reality::scip_nodes::run(&[
        "scip_nodes",
        "--repo",
        f.repo.to_str().unwrap(),
        "--graph",
        f.graph.to_str().unwrap(),
        "--index",
        f.index.to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);
    assert_eq!(out.exit_code, 0);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
}

#[test]
fn cli_rollback_graph_only_and_mutex_guards() {
    let f = fixture(&["x src/missing_fn()."]);
    let lookup = lookup_with(&[("missing_fn", 1)]);
    scip_nodes::inject_nodes_with(&f.repo, &f.graph, &f.index, false, true, Some(&*lookup))
        .unwrap();
    assert_eq!(node_count(&f.graph, "missing_fn"), 1);
    // graph-only rollback: no --repo, no index in sight
    let out = code_reality::scip_nodes::run(&[
        "scip_nodes",
        "--graph",
        f.graph.to_str().unwrap(),
        "--rollback",
    ]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("rollback：刪除 1"), "{}", out.stdout);
    assert_eq!(node_count(&f.graph, "missing_fn"), 0);
    // mutex guard: --dry-run --rollback must fail loud, never delete
    let out = code_reality::scip_nodes::run(&[
        "scip_nodes",
        "--graph",
        f.graph.to_str().unwrap(),
        "--rollback",
        "--dry-run",
    ]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --dry-run 與 --rollback 互斥\n");
}
