//! S1 foundation integration tests — connect_ro WAL branches, the tear
//! guard, make_meta ordering, and the D2 time oracle (recorded from the
//! live Python `astimezone()` probe).

mod crg_fixture;

use code_reality::common::{
    assert_db_unchanged, connect_ro, db_mtime_ns, make_meta, to_json_indent1,
};
use serde_json::json;

fn crash(out: code_reality::ToolOutput) -> (String, String, i32) {
    (out.stdout, out.stderr, out.exit_code)
}

#[test]
fn connect_ro_immutable_branch_without_wal() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("graph.db");
    let mut spec = crg_fixture::CrgDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "abc".into()));
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    let conn = connect_ro(&db).expect("immutable branch opens");
    let v: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key='git_head_sha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "abc");
}

#[test]
fn connect_ro_mode_ro_branch_with_wal_file() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("graph.db");
    let mut spec = crg_fixture::CrgDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "abc".into()));
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    // empty -wal file: existence flips connect_ro to mode=ro (a live
    // writer scenario; an empty wal reads as no frames)
    std::fs::write(db.with_file_name("graph.db-wal"), b"").unwrap();
    let conn = connect_ro(&db).expect("mode=ro branch opens with -wal present");
    let v: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key='git_head_sha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "abc");
}

#[test]
fn tear_guard_detects_mid_read_rewrite() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("graph.db");
    crg_fixture::make_crg_db(&db, &crg_fixture::CrgDbSpec::default()).unwrap();
    let m0 = db_mtime_ns(&db).unwrap();
    assert!(assert_db_unchanged(&db, m0).is_ok());
    std::fs::write(&db, b"x").unwrap();
    let err = assert_db_unchanged(&db, m0).unwrap_err();
    assert!(err.contains("撕裂"), "{err}");
}

#[test]
fn make_meta_key_order_and_injected_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    std::fs::create_dir(&repo).unwrap();
    let meta = make_meta(
        "code_reality.snapshot",
        &repo,
        Some("0123456789abcdef"),
        vec![("label", json!(null)), ("n", json!(7))],
    )
    .unwrap();
    let keys: Vec<&String> = meta.keys().collect();
    assert_eq!(
        keys,
        vec!["repo", "commit", "created_at", "tool", "label", "n"]
    );
    assert_eq!(meta["repo"], json!("myrepo"));
    assert_eq!(meta["commit"], json!("0123456789abcdef"));
    let created = meta["created_at"].as_str().unwrap();
    assert!(created.ends_with("+00:00"));
    // timespec auto: 25 chars (no frac) or 32 (with micros)
    assert!(created.len() == 25 || created.len() == 32, "{created}");
}

#[test]
fn make_meta_git_failure_is_err() {
    // a plain dir with no git repo: rev-parse must fail (crash family)
    let tmp = tempfile::tempdir().unwrap();
    let out = make_meta("t", tmp.path(), None, vec![]);
    assert!(out.is_err());
}

#[test]
fn d2_time_oracle_pinned() {
    // recorded from the live Python probe (datetime.fromisoformat(s)
    // .astimezone().timestamp()) on this machine — the libc construction
    // reproduces the naive-local assumption exactly (D2 POC decision).
    for (s, epoch) in [
        ("2026-08-25T12:00:00", 1_787_630_400i64),
        ("2026-01-15T03:30:45", 1_768_419_045),
        ("1970-06-01T00:00:00", 13_017_600),
        ("2027-03-14T23:59:59", 1_805_039_999),
    ] {
        assert_eq!(
            code_reality::common::parse_iso_to_epoch(s),
            Some(epoch),
            "{s}"
        );
    }
}

#[test]
fn tooloutput_fail_shape() {
    let (stdout, stderr, code) = crash(code_reality::ToolOutput::fail("boom"));
    assert!(stdout.is_empty());
    assert!(stderr.starts_with("[FAIL] boom"));
    assert_eq!(code, 2);
}

#[test]
fn json_indent1_pretty_shapes() {
    let v = json!({"_meta": {"repo": "r", "n": 1}, "files": [], "edges": [["a","b","CALLS"]]});
    let s = to_json_indent1(&v);
    assert!(
        s.starts_with("{\n \"_meta\": {\n  \"repo\": \"r\",\n  \"n\": 1\n },\n \"files\": [],\n")
    );
}
