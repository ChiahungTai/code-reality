//! S3 cache tests: three-table interop, guard injections, face selection
//! branches (no-db / fresh / rebuild / fallback), crash-leftover cleanup,
//! and protobuf↔sqlite face equivalence on the shared rich.scip fixture.

use code_reality::cache::{build_db, open_face, sqlite_path, stale_reason, Face};
use code_reality::engine::{load_index, Query};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FIXTURE: &str = "tests/fixtures/rich.scip";

fn fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(fixture_src(), &dst).unwrap();
    dst
}

fn fixture_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

fn rebuild_newer_index(index_path: &Path) {
    std::thread::sleep(Duration::from_millis(20));
    let bytes = std::fs::read(index_path).unwrap();
    std::fs::write(index_path, bytes).unwrap();
}

#[test]
fn build_stats_and_ingest_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    let stats = build_db(&loaded.index, &db, "headsha").unwrap();
    // 7 fn-shaped symbols ingested; NON_FN excluded
    assert_eq!(stats.symbols, 7);
    // doc a: IMPL/OTHER/MY_OPEN/MY_OPEN_DASH DEFs (4) — NON_FN dropped
    // doc b: IMPL 8 refs + 1 empty-range ref, TRAIT_IMPL DEF, TRAIT_DECL 2, REF_ONLY 1 (13)
    assert_eq!(stats.occurrences, 17);
}

#[test]
fn protobuf_and_sqlite_faces_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    build_db(&loaded.index, &db, "headsha").unwrap();
    let Face::Sqlite(conn) = open_face(&idx).unwrap().0 else {
        panic!("fresh db must select the sqlite face");
    };

    for q in ["EventStoreLifecycle.open", "open", "my-open", "X.my_open", "run"] {
        let query = Query::parse(q);
        let pb_defs = code_reality::engine::find_defs(&loaded.index, &query);
        let sq_defs = code_reality::cache::sqlite_defs(&conn, &query).unwrap();
        assert_eq!(pb_defs, sq_defs, "defs divergence for {}", q);
        let set: BTreeSet<String> = pb_defs.keys().cloned().collect();
        let pb_refs = code_reality::engine::find_refs(&loaded.index, &set);
        let sq_refs = code_reality::cache::sqlite_refs(&conn, &set).unwrap();
        assert_eq!(pb_refs, sq_refs, "refs divergence for {}", q);
    }

    // spot-check rich_index semantics through the sqlite face
    let q = Query::parse("open");
    let defs = code_reality::cache::sqlite_defs(&conn, &q).unwrap();
    // 5 defs: IMPL, TRAIT_IMPL, TRAIT_DECL, OTHER_TYPE, plus MY_OPEN_DASH —
    // FN_TAIL captures "open" from "my-open()." (documented method=? superset
    // boundary in rich_index); bare-name matcher accepts it (dash boundary)
    assert_eq!(defs.len(), 5);
    let refs = code_reality::cache::sqlite_refs(&conn, &defs.keys().cloned().collect()).unwrap();
    let impl_symbol = defs
        .keys()
        .find(|s| s.contains("impl#[EventStoreLifecycle]open") && !s.contains("[EventStore]"))
        .unwrap();
    let impl_refs = refs.get(impl_symbol).unwrap();
    assert!(impl_refs.contains(&"crates/b.rs:?".to_string())); // empty range → "?"
    assert_eq!(impl_refs.len(), 9); // 8 numbered + 1 empty-range
}

#[test]
fn stale_guards_all_four_signals() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    // realistic setup: stamp the sidecar first, build with the same head
    std::fs::write(
        code_reality::engine::meta_path(&idx),
        r#"{"repo": "/x", "head": "headsha", "stamped_at": "2026-08-24T13:45:02+00:00", "tool": "code_reality.scip_refs"}"#,
    )
    .unwrap();
    build_db(&loaded.index, &db, "headsha").unwrap();
    assert_eq!(stale_reason(&idx, &db), None, "fresh build must be fresh");

    // 1. db older than index
    rebuild_newer_index(&idx);
    assert_eq!(stale_reason(&idx, &db).as_deref(), Some("db 比索引檔舊"));
    build_db(&loaded.index, &db, "headsha").unwrap();

    // 2. schema version mismatch
    tamper(&db, "UPDATE meta SET value = '9' WHERE key = 'schema'");
    let reason = stale_reason(&idx, &db).unwrap();
    assert!(reason.contains("schema 版本不符（9 ≠ 1）"), "got: {}", reason);
    build_db(&loaded.index, &db, "headsha").unwrap();

    // 3. sidecar head drift (meta.json appears with a different head)
    std::fs::write(
        code_reality::engine::meta_path(&idx),
        r#"{"repo": "/x", "head": "othersha", "stamped_at": "2026-08-24T13:45:02+00:00", "tool": "t"}"#,
    )
    .unwrap();
    // db meta head is "headsha" but sidecar now says "othersha" → drift
    let reason = stale_reason(&idx, &db).unwrap();
    assert!(reason.contains("sidecar head 變動"), "got: {}", reason);

    // 4. corrupt db bytes
    std::fs::write(&db, b"not sqlite at all").unwrap();
    let reason = stale_reason(&idx, &db).unwrap();
    assert!(reason.contains("db 損壞"), "got: {}", reason);
}

fn tamper(db: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(sql, []).unwrap();
}

#[test]
fn open_face_no_db_never_builds() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let (face, _) = open_face(&idx).unwrap();
    assert!(matches!(face, Face::Protobuf { .. }));
    assert!(!sqlite_path(&idx).exists(), "query miss must not create a db");
}

#[test]
fn open_face_stale_rebuilds_and_serves_sqlite() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    build_db(&loaded.index, &db, "headsha").unwrap();
    rebuild_newer_index(&idx);
    let (face, stderr) = open_face(&idx).unwrap();
    assert!(matches!(face, Face::Sqlite(_)));
    let joined: String = stderr.concat();
    assert!(joined.contains("衍生 db 過期（db 比索引檔舊）——自動重建"));
    assert!(joined.contains("[OK] 衍生 db 重建完成"));
}

#[test]
fn open_face_rebuild_failure_falls_back_to_protobuf() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    build_db(&loaded.index, &db, "headsha").unwrap();
    rebuild_newer_index(&idx);
    // db path occupied by a directory → ro-open fails (db 損壞) AND the
    // rename-into-place rebuild fails → WARN + protobuf fallback
    std::fs::remove_file(&db).unwrap();
    std::fs::create_dir(&db).unwrap();
    let (face, stderr) = open_face(&idx).unwrap();
    assert!(matches!(face, Face::Protobuf { .. }));
    let joined: String = stderr.concat();
    assert!(joined.contains("衍生 db 重建失敗——本次查詢改走 protobuf 全量解析"));
    std::fs::remove_dir(&db).unwrap();
}

#[test]
fn crash_leftover_tmp_is_cleaned_by_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let loaded = load_index(&idx).unwrap();
    let db = sqlite_path(&idx);
    let tmpdb = db.with_file_name("index.scip.db.tmp");
    std::fs::write(&tmpdb, b"junk from a crashed build").unwrap();
    build_db(&loaded.index, &db, "headsha").unwrap();
    assert!(!tmpdb.exists(), "leftover tmp must be removed before CREATE");
    assert!(db.exists());
}
