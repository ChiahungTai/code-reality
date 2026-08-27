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

/// import_legacy: nodes merge＋qname-synthesis dual track, edge endpoint
/// resolution, dangling classification, row-count conservation (duplicate
/// source rows land as duplicate site rows), idempotent re-run, and the
/// absent-legacy skip face.
#[test]
fn import_legacy_merges_synthesizes_and_imports_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    // producer face first: src/a.rs carries fn target() (LSP-form cache —
    // the placeholder index keeps the ladder on the sqlite face)
    let index = tmp.path().join("index.scip");
    std::fs::write(
        &index,
        "lsp-harvest placeholder (producer=pyright-langserver)\n",
    )
    .unwrap();
    {
        let c = Connection::open(code_reality::cache::sqlite_path(&index)).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
             (1, 'lsp python src/a.rs target().', 'src/a.rs', 1, 1);",
        )
        .unwrap();
    }
    // real files so path canonicalization is stable on both faces (the
    // legacy db stores repo-absolute paths of existing files, as NT does)
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn target() {}\n").unwrap();
    std::fs::write(repo.join("src/b.rs"), "class Widget:\n    pass\n").unwrap();
    std::fs::write(repo.join("tests/t.rs"), "fn test_x() {}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    // legacy CRG db: Function node mergeable with target(), a legacy-only
    // Class, a legacy-only Test, a File node, and edges incl. a duplicate
    // row and a dangling endpoint
    let legacy_dir = repo.join(".code-review-graph");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    {
        let c = Connection::open(legacy_dir.join("graph.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                name TEXT NOT NULL, qualified_name TEXT NOT NULL UNIQUE,
                file_path TEXT NOT NULL, line_start INTEGER, line_end INTEGER,
                language TEXT, parent_name TEXT, is_test INTEGER DEFAULT 0,
                extra TEXT DEFAULT '{}', updated_at REAL NOT NULL, community_id INTEGER);
             CREATE TABLE edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                source_qualified TEXT NOT NULL, target_qualified TEXT NOT NULL,
                file_path TEXT NOT NULL, line INTEGER DEFAULT 0,
                extra TEXT DEFAULT '{}', confidence REAL DEFAULT 1.0,
                confidence_tier TEXT DEFAULT 'EXTRACTED', updated_at REAL NOT NULL);",
        )
        .unwrap();
        let a = repo.join("src/a.rs");
        let b = repo.join("src/b.rs");
        let t = repo.join("tests/t.rs");
        let qt = format!("{}::target", a.display());
        let qw = format!("{}::Widget", b.display());
        let qx = format!("{}::test_x", t.display());
        let qf = format!("{}::", b.display());
        let ghost = format!("{}::ghost", b.display());
        for (kind, name, qname, file, ls, le, it) in [
            ("Function", "target", &qt, a.display().to_string(), 5, 9, 0),
            ("Class", "Widget", &qw, b.display().to_string(), 1, 40, 0),
            ("Test", "test_x", &qx, t.display().to_string(), 1, 3, 1),
            ("File", "", &qf, b.display().to_string(), -1, -1, 0),
        ] {
            let (ls, le) = if ls < 0 {
                (None, None)
            } else {
                (Some(ls), Some(le))
            };
            c.execute(
                "INSERT INTO nodes (kind, name, qualified_name, file_path, line_start, line_end, language, is_test, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Rust', ?7, 0)",
                rusqlite::params![kind, name, qname, file, ls, le, it],
            )
            .unwrap();
        }
        for (kind, sq, tq, file, line) in [
            ("CALLS", &qt, &qw, a.display().to_string(), 7),
            ("CALLS", &qw, &qx, b.display().to_string(), 2),
            ("CALLS", &qw, &qx, b.display().to_string(), 3),
            ("CONTAINS", &qf, &qw, b.display().to_string(), 0),
            ("CALLS", &qw, &ghost, b.display().to_string(), 9),
        ] {
            c.execute(
                "INSERT INTO edges (kind, source_qualified, target_qualified, file_path, line, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                rusqlite::params![kind, sq, tq, file, line],
            )
            .unwrap();
        }
    }
    let rep = graph_db::import_legacy(&repo, false).unwrap();
    assert!(!rep.skipped);
    assert_eq!(rep.legacy_nodes, 4);
    assert_eq!(
        rep.merged_nodes, 1,
        "Function target() merges onto producer node"
    );
    assert_eq!(
        rep.synthesized_nodes, 3,
        "Class + Test + File mint qname symbols"
    );
    assert_eq!(rep.collision_keys, 0);
    assert_eq!(rep.legacy_edges, 5);
    assert_eq!(rep.mapped_edges, 4, "1 + 2 duplicate + 1 CONTAINS");
    assert_eq!(
        rep.dangling_edges, 1,
        "ghost endpoint absent from legacy nodes"
    );

    let conn = open(&graph_db::db_path(&repo));
    // merged: producer node carries the legacy span
    let (ls, le): (i64, i64) = conn
        .query_row(
            "SELECT line_start, line_end FROM nodes WHERE symbol LIKE '%target%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((ls, le), (5, 9), "legacy span patched onto producer node");
    // synthesized: qname-keyed symbols, provenance stamped
    let synth: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE provenance = 'treesitter-legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(synth, 3);
    let w: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE symbol = ?1 AND kind = 'Class' AND is_test = 0",
            [format!("{}::Widget", repo.join("src/b.rs").display())],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(w, 1);
    // merged edge caller resolves to the PRODUCER symbol
    let merged_caller: String = conn
        .query_row(
            "SELECT caller_symbol FROM edges WHERE provenance = 'treesitter-legacy' \
             AND kind = 'CALLS' AND callee_symbol LIKE '%Widget'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        merged_caller.ends_with("target()."),
        "caller = producer symbol, got {merged_caller}"
    );
    // row-count conservation: the duplicate CALLS pair lands twice
    let dup: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE callee_symbol LIKE '%test_x' AND kind = 'CALLS'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dup, 2);
    // dangling edge lands as-is (endpoint passthrough): BFS/impact skip
    // unknown endpoints by construction, and the criticality external
    // factor counts them — the layer-2 parity bar pinned this
    let ghost: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE callee_symbol LIKE '%ghost'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ghost, 1);
    // CALLS edges present → flows materialized over the imported CALLS
    assert!(
        rep.derived.as_ref().unwrap().flows >= 1,
        "Widget→test_x chain yields flows"
    );

    // idempotent re-run: counts stable, no duplicate rows
    let rep2 = graph_db::import_legacy(&repo, false).unwrap();
    assert_eq!(
        (rep2.merged_nodes, rep2.synthesized_nodes, rep2.mapped_edges),
        (rep.merged_nodes, rep.synthesized_nodes, rep.mapped_edges)
    );
    let total_nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    let rep2_conn = open(&graph_db::db_path(&repo));
    let total_nodes2: i64 = rep2_conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_nodes, total_nodes2, "re-run adds no nodes");
}

#[test]
fn import_legacy_dry_run_is_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    // producer + real legacy import (reuse the merge-test environment in
    // miniature), then snapshot bytes around a dry-run
    let index = tmp.path().join("index.scip");
    std::fs::write(&index, "lsp-harvest placeholder\n").unwrap();
    {
        let c = Connection::open(code_reality::cache::sqlite_path(&index)).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
             (1, 'lsp python src/a.rs target().', 'src/a.rs', 1, 1);",
        )
        .unwrap();
    }
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn target() {}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let legacy_dir = repo.join(".code-review-graph");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    {
        let c = Connection::open(legacy_dir.join("graph.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                name TEXT NOT NULL, qualified_name TEXT NOT NULL UNIQUE, file_path TEXT NOT NULL,
                line_start INTEGER, line_end INTEGER, language TEXT, parent_name TEXT,
                is_test INTEGER DEFAULT 0, extra TEXT DEFAULT '{}', updated_at REAL NOT NULL,
                community_id INTEGER);
             CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                source_qualified TEXT NOT NULL, target_qualified TEXT NOT NULL,
                file_path TEXT NOT NULL, line INTEGER DEFAULT 0, extra TEXT DEFAULT '{}',
                confidence REAL DEFAULT 1.0, confidence_tier TEXT DEFAULT 'EXTRACTED',
                updated_at REAL NOT NULL);",
        )
        .unwrap();
        let a = repo.join("src/a.rs");
        c.execute(
            "INSERT INTO nodes (kind, name, qualified_name, file_path, updated_at) VALUES
             ('Function', 'target', ?1, ?2, 0)",
            (format!("{}::target", a.display()), a.display().to_string()),
        )
        .unwrap();
        c.execute(
            "INSERT INTO edges (kind, source_qualified, target_qualified, file_path, updated_at) VALUES
             ('CALLS', ?1, ?1, ?2, 0)",
            (format!("{}::target", a.display()), a.display().to_string()),
        )
        .unwrap();
    }
    let real = graph_db::import_legacy(&repo, false).unwrap();
    assert!(!real.skipped);
    let db = graph_db::db_path(&repo);
    let before = std::fs::read(&db).unwrap();
    let dry = graph_db::import_legacy(&repo, true).unwrap();
    assert!(dry.dry_run);
    assert_eq!(dry.mapped_edges, real.mapped_edges, "counts still reported");
    let after = std::fs::read(&db).unwrap();
    assert_eq!(before, after, "R1: dry-run must not mutate the db (bytes)");
    // and the legacy data is still there
    let conn = open(&db);
    let kept: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE provenance = 'treesitter-legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(kept >= 1, "R1: dry-run must not sweep imported edges");
}

#[test]
fn import_legacy_skips_when_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let rep = graph_db::import_legacy(&repo, false).unwrap();
    assert!(rep.skipped, "absent legacy db = skip, not error");
}

#[test]
fn import_legacy_requires_new_db() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    std::fs::create_dir_all(repo.join(".code-review-graph")).unwrap();
    Connection::open(repo.join(".code-review-graph/graph.db")).unwrap();
    let err = graph_db::import_legacy(&repo, false).unwrap_err();
    assert!(
        err.contains("graph_db build"),
        "guides to build first: {err}"
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
    // derived materialization: flows stay empty pre-import — the BFS
    // walks CALLS edges and producer refs land as REFERENCES (the
    // documented S2→S3 intermediate state; legacy CALLS arrive with
    // import_legacy). communities are directory-based → one for src/a.py.
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

/// F1 (judge 2026-08-27): after import, the FTS index must cover the
/// synthesized legacy nodes — a stale index silently hides them from
/// search (FTS hit short-circuits the LIKE fallback).
#[test]
fn import_legacy_rebuilds_fts_for_mixed_hits() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let index = tmp.path().join("index.scip");
    std::fs::write(&index, "lsp-harvest placeholder\n").unwrap();
    {
        let c = Connection::open(code_reality::cache::sqlite_path(&index)).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
             (1, 'lsp python src/a.rs target().', 'src/a.rs', 1, 1);",
        )
        .unwrap();
    }
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn target() {}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    // a legacy-only node whose NAME shares a token with the producer node
    let legacy_dir = repo.join(".code-review-graph");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    {
        let c = Connection::open(legacy_dir.join("graph.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                name TEXT NOT NULL, qualified_name TEXT NOT NULL UNIQUE, file_path TEXT NOT NULL,
                line_start INTEGER, line_end INTEGER, language TEXT, parent_name TEXT,
                is_test INTEGER DEFAULT 0, extra TEXT DEFAULT '{}', updated_at REAL NOT NULL,
                community_id INTEGER);
             CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                source_qualified TEXT NOT NULL, target_qualified TEXT NOT NULL,
                file_path TEXT NOT NULL, line INTEGER DEFAULT 0, extra TEXT DEFAULT '{}',
                confidence REAL DEFAULT 1.0, confidence_tier TEXT DEFAULT 'EXTRACTED',
                updated_at REAL NOT NULL);",
        )
        .unwrap();
        let b = repo.join("src/b.rs");
        std::fs::write(&b, "fn target_legacy() {}\n").unwrap();
        c.execute(
            "INSERT INTO nodes (kind, name, qualified_name, file_path, updated_at) VALUES
             ('Function', 'target_legacy', ?1, ?2, 0)",
            (
                format!("{}::target_legacy", b.display()),
                b.display().to_string(),
            ),
        )
        .unwrap();
    }
    graph_db::import_legacy(&repo, false).unwrap();
    // mixed hit: both nodes carry the "target" token — without the
    // post-import FTS rebuild, the legacy node is silently missing
    let conn = open(&graph_db::db_path(&repo));
    let mut stmt = conn
        .prepare("SELECT name FROM nodes_fts WHERE nodes_fts MATCH 'target'")
        .unwrap();
    let hits: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        hits.iter().any(|n| n == "target") && hits.iter().any(|n| n == "target_legacy"),
        "F1: post-import FTS rebuild must surface legacy nodes, got {hits:?}"
    );
}

/// F6 (judge 2026-08-27): the collision defense — a legacy (file, name)
/// hitting multiple producer symbols must count and route to synthesis.
#[test]
fn import_legacy_collision_routes_to_synthesis() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let index = tmp.path().join("index.scip");
    std::fs::write(&index, "lsp-harvest placeholder\n").unwrap();
    {
        let c = Connection::open(code_reality::cache::sqlite_path(&index)).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
        // TWO producer symbols with the same (file, name) — the ambiguous key
        c.execute_batch(
            "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
             (1, 'lsp python src/a.rs dup().', 'src/a.rs', 1, 1),
             (2, 'lsp python2 src/a.rs dup().', 'src/a.rs', 5, 1);",
        )
        .unwrap();
    }
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn dup() {}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let legacy_dir = repo.join(".code-review-graph");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    {
        let c = Connection::open(legacy_dir.join("graph.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                name TEXT NOT NULL, qualified_name TEXT NOT NULL UNIQUE, file_path TEXT NOT NULL,
                line_start INTEGER, line_end INTEGER, language TEXT, parent_name TEXT,
                is_test INTEGER DEFAULT 0, extra TEXT DEFAULT '{}', updated_at REAL NOT NULL,
                community_id INTEGER);
             CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                source_qualified TEXT NOT NULL, target_qualified TEXT NOT NULL,
                file_path TEXT NOT NULL, line INTEGER DEFAULT 0, extra TEXT DEFAULT '{}',
                confidence REAL DEFAULT 1.0, confidence_tier TEXT DEFAULT 'EXTRACTED',
                updated_at REAL NOT NULL);",
        )
        .unwrap();
        let a = repo.join("src/a.rs");
        c.execute(
            "INSERT INTO nodes (kind, name, qualified_name, file_path, updated_at) VALUES
             ('Function', 'dup', ?1, ?2, 0)",
            (format!("{}::dup", a.display()), a.display().to_string()),
        )
        .unwrap();
    }
    let rep = graph_db::import_legacy(&repo, false).unwrap();
    assert_eq!(rep.collision_keys, 1, "ambiguous (file,name) counted");
    assert_eq!(rep.merged_nodes, 0, "conservative: no merge on collision");
    assert_eq!(rep.synthesized_nodes, 1, "routed to qname synthesis");
    let conn = open(&graph_db::db_path(&repo));
    let synth: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE provenance = 'treesitter-legacy' AND symbol LIKE '%::dup'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(synth, 1);
}
