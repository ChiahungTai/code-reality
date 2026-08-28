//! v1+ S4 tests: `graph_db build` — producer-keyed schema (symbol as the
//! node key, qname a display column), site-grain edges whose (caller,
//! callee) site multiset matches the scip_edges derivation face (same
//! spans-based attribution — the sidecar-era algorithm), language inferred
//! per producer (no bootstrap 'Python' hardcode), idempotent rebuild over
//! an existing db, and the derived tables present (materialization wiring
//! lands with S2 loaders).

use code_reality::graph_db;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "tests/fixtures/rich_callers.scip";

fn slot(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(FIXTURE, &dst).unwrap();
    dst
}

fn repo_dir(tmp: &tempfile::TempDir) -> PathBuf {
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    repo
}

fn open(db: &Path) -> Connection {
    Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

#[test]
fn build_creates_producer_keyed_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let rep = graph_db::build_from_cache_at(&repo, &index).unwrap();
    assert!(rep.nodes > 0, "fixture carries fn defs");
    assert!(rep.edges > 0, "fixture carries attributed refs");
    let db = graph_db::db_path(&repo);
    assert!(db.exists(), "db lands under .code-reality/");
    let conn = open(&db);
    // node key face: symbol populated, qname present as display column
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE symbol IS NOT NULL AND symbol != '' \
             AND qname IS NOT NULL AND qname != '' AND kind = 'Function'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n as usize, rep.nodes);
    // language inferred, never the bootstrap 'Python' hardcode (rust fixture)
    let langs: Vec<String> = conn
        .prepare("SELECT DISTINCT language FROM nodes")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(langs, vec!["Rust".to_string()]);
    // provenance stamped
    let provs: Vec<String> = conn
        .prepare("SELECT DISTINCT provenance FROM nodes")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(provs, vec!["scip".to_string()]);
    // edge face: REFERENCES site rows with location + confidence columns
    let bad: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE kind != 'REFERENCES' \
             OR provenance != 'scip' OR file_path = '' OR line <= 0 \
             OR confidence IS NULL OR confidence_tier IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad, 0, "every edge row is a located REFERENCES site");
    // workspace filter: every edge endpoint is a node symbol
    let dangling: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e WHERE \
             NOT EXISTS (SELECT 1 FROM nodes n WHERE n.symbol = e.caller_symbol) \
             OR NOT EXISTS (SELECT 1 FROM nodes n WHERE n.symbol = e.callee_symbol)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dangling, 0);
    // derived tables exist (empty until S2 wires materialization)
    for t in ["communities", "flows", "flow_memberships"] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap_or(-1);
        assert!(n >= 0, "{t} table must exist");
    }
}

#[test]
fn edge_sites_match_scip_edges_derivation() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open(&graph_db::db_path(&repo));
    let mut got: BTreeMap<(String, String), i64> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT caller_symbol, callee_symbol FROM edges")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        for r in rows {
            let (c, t) = r.unwrap();
            *got.entry((c, t)).or_insert(0) += 1;
        }
    }
    // oracle: scip_edges derive on the same fixture, workspace-filtered,
    // sites summed — same spans-based attribution, so the multiset must
    // match exactly (an S4-layer-2 prerequisite).
    let (all, _report, _warns) = code_reality::scip_edges::derive_edges(&index).unwrap();
    let (face, _w) = code_reality::cache::open_face(&index).unwrap();
    let defs: std::collections::BTreeSet<String> = {
        use code_reality::cache::Face;
        match &face {
            Face::Sqlite(conn) => {
                let mut stmt = conn
                    .prepare("SELECT DISTINCT symbol FROM occurrences WHERE is_def = 1")
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap();
                rows
            }
            Face::Protobuf { index } => {
                let mut s = std::collections::BTreeSet::new();
                for d in &index.documents {
                    for occ in &d.occurrences {
                        if code_reality::engine::fn_tail_name(&occ.symbol).is_some()
                            && occ.symbol_roles & 1 != 0
                        {
                            s.insert(occ.symbol.clone());
                        }
                    }
                }
                s
            }
        }
    };
    let mut want: BTreeMap<(String, String), i64> = BTreeMap::new();
    for e in &all {
        // build skips self-refs (self_ref_skipped) — the oracle must
        // mirror that, or a fixture with recursion breaks confusingly
        if defs.contains(&e.callee) && e.caller != e.callee {
            *want
                .entry((e.caller.clone(), e.callee.clone()))
                .or_insert(0) += e.sites as i64;
        }
    }
    assert_eq!(got, want, "site multiset must equal the derivation face");
}

#[test]
fn build_is_idempotent_and_leaves_no_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let r1 = graph_db::build_from_cache_at(&repo, &index).unwrap();
    let r2 = graph_db::build_from_cache_at(&repo, &index).unwrap();
    assert_eq!((r1.nodes, r1.edges), (r2.nodes, r2.edges));
    let dir = graph_db::db_path(&repo)
        .parent()
        .unwrap()
        .read_dir()
        .unwrap();
    let files: Vec<_> = dir.filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
    assert_eq!(files.len(), 1, "no temp leftover: {files:?}");
}

#[test]
fn is_test_rel_covers_rust_and_python_conventions() {
    assert!(graph_db::is_test_rel("tests/graph_engine.rs"));
    assert!(graph_db::is_test_rel("crates/x/tests/common/mod.rs"));
    assert!(graph_db::is_test_rel("test_utils.py"));
    assert!(graph_db::is_test_rel("pkg/test_helper.py"));
    assert!(!graph_db::is_test_rel("src/lib.rs"));
    assert!(!graph_db::is_test_rel(
        "crates/code-reality/src/graph_engine.rs"
    ));
}

#[test]
fn missing_cache_is_loud() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let index = tmp.path().join("nonexistent.scip");
    let err = graph_db::build_from_cache_at(&repo, &index).unwrap_err();
    assert!(err.contains("不在"), "loud error face: {err}");
}

#[test]
fn build_fails_loud_on_stale_lsp_sidecar_superseded_by_newer_index() {
    // Silent bad-db relay (2026-08-28, W4 mosaic cleanup): a stale
    // lsp-harvest cache db left beside a freshly written index.scip was
    // silently trusted (CALLS 0 / REFERENCES-only db, no WARN). The lsp
    // fast-path now fails loud on the mtime contradiction.
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.py"), "def target():\n    pass\n").unwrap();
    let index = tmp.path().join("index.scip");
    std::fs::write(&index, "lsp-harvest placeholder\n").unwrap();
    let cache_db = code_reality::cache::sqlite_path(&index);
    {
        let c = Connection::open(&cache_db).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
             (1, 'lsp python src/a.py target().', 'src/a.py', 1, 1);",
        )
        .unwrap();
    }
    // fresh producer run rewrote the index AFTER the cache was built
    std::fs::write(&index, "fresh producer index bytes\n").unwrap();
    let err = graph_db::build_from_cache_at(&repo, &index).unwrap_err();
    assert!(
        err.contains("已被較新的 index.scip 取代"),
        "fail-loud on superseded lsp sidecar: {err}"
    );
}

#[test]
fn lsp_cache_builds_with_nearest_preceding_attribution() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let index = tmp.path().join("index.scip");
    std::fs::write(
        &index,
        "lsp-harvest placeholder (producer=pyright-langserver)\n",
    )
    .unwrap();
    let cache_db = code_reality::cache::sqlite_path(&index);
    {
        let c = Connection::open(&cache_db).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        // two defs in one file; a ref AFTER the second def must attribute
        // to the second (nearest preceding), not the first
        let rows: Vec<(&str, &str, i64, i64)> = vec![
            ("lsp python src/a.py first().", "src/a.py", 1, 1),
            ("lsp python src/a.py second().", "src/a.py", 10, 1),
            ("lsp python src/a.py first().", "src/a.py", 20, 0),
        ];
        for (i, (s, r, l, d)) in rows.iter().enumerate() {
            c.execute(
                "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![(i + 1) as i64, s, r, l, d],
            )
            .unwrap();
        }
    }
    let rep = graph_db::build_from_cache_at(&repo, &index).unwrap();
    assert_eq!(rep.nodes, 2);
    assert_eq!(rep.edges, 1, "one ref site → one edge row");
    // derived materialization: flows stay empty — the BFS walks CALLS
    // edges and this lsp-harvest face lands refs as REFERENCES.
    // communities are directory-based → one for src/a.py.
    assert_eq!(rep.flows, 0);
    assert_eq!(rep.communities, 1);
    let conn = open(&graph_db::db_path(&repo));
    let (caller, kind, prov, lang): (String, String, String, String) = conn
        .query_row(
            "SELECT e.caller_symbol, e.kind, e.provenance, n.language \
             FROM edges e JOIN nodes n ON n.symbol = e.caller_symbol",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        caller, "lsp python src/a.py second().",
        "nearest preceding def wins (not the first)"
    );
    assert_eq!(kind, "REFERENCES");
    assert_eq!(prov, "lsp-harvest");
    assert_eq!(lang, "Python", "lsp-prefixed symbols infer Python");
}

#[test]
fn build_materializes_engine_read_chain_indexes() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open(&graph_db::db_path(&repo));
    for idx in [
        "idx_edges_caller",
        "idx_edges_callee",
        "idx_flow_memberships_node",
        "idx_nodes_name_file_line",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
                [idx],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{idx} present on fresh build");
    }
}

#[test]
fn ensure_indexes_is_idempotent_on_built_db() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    // fresh build already materialized the engine indexes via DDL —
    // ensure is a no-op reporting all skipped (for dbs built before the
    // index DDL revision it would create them)
    let first = graph_db::ensure_indexes(&repo).unwrap();
    assert_eq!(first.created, 0);
    assert_eq!(first.skipped, 4);
    let second = graph_db::ensure_indexes(&repo).unwrap();
    assert_eq!(second.created, 0, "idempotent");
    assert_eq!(second.skipped, 4);
    let conn = open(&graph_db::db_path(&repo));
    let plan: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT line_start FROM nodes \
             WHERE file_path LIKE '%/a.rs' AND name='target' \
             AND line_start IS NOT NULL \
             ORDER BY ABS(line_start-10), line_start, symbol LIMIT 1",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("idx_nodes_name_file_line"),
        "anchor query uses the covering index, got: {plan}"
    );
}

#[test]
fn build_stamps_snapshot_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open(&graph_db::db_path(&repo));
    let last_updated: Option<String> = conn
        .query_row(
            "SELECT value FROM metadata WHERE key='last_updated'",
            [],
            |r| r.get(0),
        )
        .ok();
    assert!(
        last_updated.is_some(),
        "last_updated stamped for staleness face"
    );
    // no git repo here -> git_head_sha absent (stamping skips silently)
    let sha: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM metadata WHERE key='git_head_sha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sha, 0);
}
