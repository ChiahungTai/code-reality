//! CR-10 S3 — graph publication generation provenance (EP
//! `ai-analysis/execution-plans/2026-10-14-cr-delta-freshness`; frozen
//! TC row TC-11 plus the D11 old-inode reconciliation premise).
//!
//! Contract under test (D10, publication side only — the S4 consumption
//! vocabulary is out of scope by the D17 fence):
//! - `graph_db build` stamps, inside the SAME write tx as the row data
//!   (publication atomicity): `graph_snapshot_id` (per-build generation
//!   id — content-addressed sha256 over the consumed slot artifact
//!   hash, the compile-time builder revision, and the schema version),
//!   `input_index_sha256`, `source_manifest_digest` (S2 sidecar present
//!   ? digest of its exact bytes : null), `builder_revision` (build-time
//!   CR_BUILD_REV — never a query-time binary rev), and the slot meta's
//!   `coverage_faces` / `selection` (null when the meta doesn't carry
//!   them);
//! - same-content rebuilds keep the generation id (content-addressed
//!   idempotence, review fresh-F8); an input change moves it;
//! - tmp+rename publication leaves an already-open connection pinned to
//!   its own generation's metadata (the D11 same-conn read premise S4
//!   will consume — no WAL, the old inode stays readable);
//! - a legacy db without the keys reads back null, and no face other
//!   than publication ever retro-stamps them.
//!
//! The `‖` concatenation of D10 is encoded as the domain-prefixed,
//! NUL-separated byte string pinned in `expected_snapshot_id` below.
//! The tests recompute the id from raw parts (never through a lib
//! helper), so the byte format itself is under test.

mod graph_db_fixture;

use code_reality::graph_db;
use code_reality::identity::sha256_hex;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "tests/fixtures/rich_callers.scip";
const FIXTURE_ALT: &str = "tests/fixtures/rich.scip";

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

fn open_ro(db: &Path) -> Connection {
    Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

/// The legacy read shape S4 consumes: a missing key or a SQL NULL both
/// read back `None` — never an error, never a fabricated value.
fn meta_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |r| {
        r.get::<_, Option<String>>(0)
    })
    .ok()
    .flatten()
}

/// The D10 formula `sha256(source_index_sha256 ‖ builder_revision ‖
/// schema_version)` with the concatenation pinned byte-for-byte.
fn expected_snapshot_id(input: &str, rev: &str, schema: u32) -> String {
    sha256_hex(format!("cr-graph-snapshot-v1\0{input}\0{rev}\0{schema}\0").as_bytes())
}

// ---------- TC-11: the D10 stamp, formula and null axes ----------

#[test]
fn tc11_publication_stamps_generation_provenance() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));

    // input identity: the exact slot artifact bytes the build consumed
    let input = sha256_hex(&std::fs::read(&index).unwrap());
    assert_eq!(
        meta_value(&conn, "input_index_sha256").as_deref(),
        Some(input.as_str())
    );

    // generation id: recomputed here from raw parts (D10 formula)
    let rev = option_env!("CR_BUILD_REV").unwrap_or("");
    assert_eq!(
        meta_value(&conn, "graph_snapshot_id").as_deref(),
        Some(expected_snapshot_id(&input, rev, graph_db::SCHEMA_VERSION).as_str())
    );

    // schema version rides the db header, single-sourced with the hash
    // input (a schema revision must move every generation id)
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap() as u32,
        graph_db::SCHEMA_VERSION
    );

    // builder_revision is the compile-time constant verbatim (SQL NULL
    // when the compiling toolchain had no git metadata) — never a
    // query-time binary rev
    assert_eq!(
        meta_value(&conn, "builder_revision"),
        option_env!("CR_BUILD_REV").map(str::to_string)
    );

    // no manifest sidecar, no slot meta → the soft axes read null
    assert_eq!(
        meta_value(&conn, "source_manifest_digest"),
        None,
        "absent S2 sidecar digests null"
    );
    assert_eq!(meta_value(&conn, "coverage_faces"), None);
    assert_eq!(meta_value(&conn, "selection"), None);
}

// ---------- TC-11: source_manifest_digest follows the sidecar bytes ----------

#[test]
fn tc11_source_manifest_digest_follows_sidecar_bytes_or_null() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let mpath = code_reality::identity::manifest_path_for_slot(&index);

    // gen with sidecar v1: the digest binds the exact bytes on disk
    std::fs::write(&mpath, b"{\"gen\": 1}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));
    assert_eq!(
        meta_value(&conn, "source_manifest_digest").as_deref(),
        Some(sha256_hex(b"{\"gen\": 1}\n").as_str())
    );

    // sidecar bytes change → the next publication's digest follows
    std::fs::write(&mpath, b"{\"gen\": 2}\n").unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));
    assert_eq!(
        meta_value(&conn, "source_manifest_digest").as_deref(),
        Some(sha256_hex(b"{\"gen\": 2}\n").as_str())
    );

    // sidecar vanishes → back to null (presence-only semantics, D10 —
    // binding validation is S2/S4 consumption, never re-derived here)
    std::fs::remove_file(&mpath).unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));
    assert_eq!(meta_value(&conn, "source_manifest_digest"), None);
}

// ---------- TC-11: coverage_faces / selection mirror the slot meta ----------

#[test]
fn tc11_coverage_and_selection_mirror_slot_meta_or_null() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let meta_file = code_reality::engine::meta_path(&index);

    // no meta → both null (already covered gen-1 in tc11_publication…;
    // here the meta EXISTS but predates the fields)
    std::fs::write(&meta_file, r#"{"head": "h", "producer": "p"}"#).unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));
    assert_eq!(meta_value(&conn, "coverage_faces"), None);
    assert_eq!(meta_value(&conn, "selection"), None);

    // S2 stamp shape: the faces array and selection carry over verbatim
    std::fs::write(
        &meta_file,
        r#"{"head": "h", "producer": "p", "source_faces": ["rust"], "selection": "auto"}"#,
    )
    .unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&graph_db::db_path(&repo));
    assert_eq!(
        meta_value(&conn, "coverage_faces").as_deref(),
        Some(r#"["rust"]"#)
    );
    assert_eq!(meta_value(&conn, "selection").as_deref(), Some("auto"));
}

// ---------- TC-11: content-addressed idempotence + evolution ----------

#[test]
fn tc11_same_content_rebuild_keeps_generation_input_change_moves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let db_path = graph_db::db_path(&repo);

    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let id1 = meta_value(&open_ro(&db_path), "graph_snapshot_id").unwrap();
    let in1 = meta_value(&open_ro(&db_path), "input_index_sha256").unwrap();

    // same bytes → same generation (fresh-F8): last_updated re-stamps,
    // the content-addressed id must not move
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&db_path);
    assert_eq!(meta_value(&conn, "graph_snapshot_id").unwrap(), id1);
    assert_eq!(meta_value(&conn, "input_index_sha256").unwrap(), in1);

    // input change → new generation id, new input hash, formula intact
    std::fs::copy(FIXTURE_ALT, &index).unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();
    let conn = open_ro(&db_path);
    let id2 = meta_value(&conn, "graph_snapshot_id").unwrap();
    let in2 = meta_value(&conn, "input_index_sha256").unwrap();
    assert_ne!(id2, id1, "an input change must move the generation id");
    assert_ne!(in2, in1);
    assert_eq!(
        id2,
        expected_snapshot_id(
            &in2,
            option_env!("CR_BUILD_REV").unwrap_or(""),
            graph_db::SCHEMA_VERSION
        )
    );
}

// ---------- D11 premise: old connection keeps its generation ----------

#[test]
fn d11_old_connection_reads_its_own_generation_across_republication() {
    let tmp = tempfile::tempdir().unwrap();
    let index = slot(&tmp);
    let repo = repo_dir(&tmp);
    let db_path = graph_db::db_path(&repo);

    graph_db::build_from_cache_at(&repo, &index).unwrap();
    // open BEFORE the second publication; the first read materializes
    // the fd on the current inode (SQLite opens the file lazily)
    let old = open_ro(&db_path);
    let id1 = meta_value(&old, "graph_snapshot_id").unwrap();

    std::fs::copy(FIXTURE_ALT, &index).unwrap();
    graph_db::build_from_cache_at(&repo, &index).unwrap();

    // tmp+rename swapped the directory entry: the old connection still
    // reads ITS generation's metadata (old inode view — no WAL)
    assert_eq!(
        meta_value(&old, "graph_snapshot_id").unwrap(),
        id1,
        "an already-open connection must keep its generation's metadata"
    );

    // a fresh connection sees the new generation, and it differs
    let fresh = open_ro(&db_path);
    let id2 = meta_value(&fresh, "graph_snapshot_id").unwrap();
    assert_ne!(id2, id1, "premise: the rebuild moved the generation");
}

// ---------- legacy db: keys absent read null, no retro-stamp ----------

#[test]
fn legacy_db_without_keys_reads_null_and_stays_unstamped() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_dir(&tmp);
    let db_path = graph_db::db_path(&repo);
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    // a seeded probe key proves the read path is live — the six null
    // assertions below are then real query results, not swallowed
    // errors (review fresh-F4)
    let spec = graph_db_fixture::GraphDbSpec {
        metadata: vec![("probe_key".to_string(), "1".to_string())],
        ..Default::default()
    };
    graph_db_fixture::make_graph_db(&db_path, &spec).unwrap();

    const KEYS: [&str; 6] = [
        "graph_snapshot_id",
        "input_index_sha256",
        "source_manifest_digest",
        "builder_revision",
        "coverage_faces",
        "selection",
    ];
    let assert_all_null = |conn: &Connection| {
        for k in KEYS {
            assert_eq!(meta_value(conn, k), None, "legacy {k} must read null");
        }
    };
    assert_eq!(
        meta_value(&open_ro(&db_path), "probe_key").as_deref(),
        Some("1"),
        "the read path must be live for the null assertions to mean anything"
    );
    assert_all_null(&open_ro(&db_path));

    // the only other write face (ensure_indexes) stays index-only —
    // provenance is minted at publication and nowhere else
    let rep = graph_db::ensure_indexes(&repo).unwrap();
    assert_eq!(rep.created, 4);
    assert_all_null(&open_ro(&db_path));
}
