//! R3 S2 fndefs sidecar tests: path convention, build/load roundtrip with
//! protobuf-face equivalence, the four staleness guards, the accelerator
//! ladder (absent → protobuf, no build; stale → WARN + rebuild; rebuild
//! failure → WARN + protobuf), --build-cache stdout freeze (sidecar message
//! on stderr only), and the SM-13 three-table-bytes-untouched assertion.

use code_reality::cache::sqlite_path;
use code_reality::engine::{fn_spans, load_index};
use code_reality::fndefs::{
    build_sidecar, fndefs_path, load_spans, spans_source, stale_sidecar_reason,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FIXTURE: &str = "tests/fixtures/rich.scip";
// enc-bearing fixture (R3): rich.scip predates enclosing_range data, so
// span-content assertions against it are vacuous (empty span map).
const CALLERS_FIXTURE: &str = "tests/fixtures/rich_callers.scip";

fn fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(fixture_src(), &dst).unwrap();
    dst
}

fn callers_fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(CALLERS_FIXTURE),
        &dst,
    )
    .unwrap();
    dst
}

fn fixture_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

fn touch_index(index_path: &Path) {
    std::thread::sleep(Duration::from_millis(20));
    let bytes = std::fs::read(index_path).unwrap();
    std::fs::write(index_path, bytes).unwrap();
}

fn key(s: &code_reality::engine::FnSpan) -> (String, String, i64, i64, usize) {
    (
        s.symbol.clone(),
        s.rel_path.clone(),
        s.start_line,
        s.end_line,
        s.seq,
    )
}

/// Minimal index-side meta.json so the head guard matches the build head.
fn stamp_head(index_path: &Path, head: &str) {
    let meta = code_reality::engine::meta_path(index_path);
    std::fs::write(&meta, format!("{{\"head\": \"{}\"}}\n", head)).unwrap();
}

/// In-memory enc-bearing index (the R2 rich.scip fixture predates R3 and
/// carries no enclosing_range data).
fn enc_index() -> scip::types::Index {
    use scip::types::{Document, Occurrence};
    let o = |symbol: &str, roles: i32, range: Vec<i32>, enc: Option<Vec<i32>>| {
        let mut x = Occurrence::new();
        x.symbol = symbol.to_string();
        x.symbol_roles = roles;
        x.range = range;
        if let Some(e) = enc {
            x.enclosing_range = e;
        }
        x
    };
    // document order deliberately differs from the BTreeMap sort order
    // (z.rs first): the sidecar must store the ORIGINAL protobuf scan seq —
    // an enumerate-over-sorted-flatten regression would renumber and fail
    // the roundtrip below.
    let mut d1 = Document::new();
    d1.relative_path = "z.rs".to_string();
    d1.occurrences = vec![o(
        "cargo x b/helper().",
        1,
        vec![3, 0],
        Some(vec![3, 0, 8, 1]),
    )];
    let mut d2 = Document::new();
    d2.relative_path = "a.rs".to_string();
    d2.occurrences = vec![
        o("cargo x a/outer().", 1, vec![9, 0], Some(vec![9, 0, 11, 5])),
        o(
            "cargo x a/macro_fn().",
            1,
            vec![19, 2],
            Some(vec![19, 2, 44]),
        ),
        o("cargo x a/inner().", 1, vec![0, 0], Some(vec![0, 0, 5, 9])),
    ];
    let mut i = scip::types::Index::new();
    i.documents = vec![d1, d2];
    i
}

#[test]
fn fndefs_path_is_full_filename_plus_suffix() {
    let p = fndefs_path(Path::new("/slot/nautilus_trader/index.scip"));
    assert!(p.ends_with("index.scip.fndefs.db"));
    assert_eq!(p.parent().unwrap().file_name().unwrap(), "nautilus_trader");
}

#[test]
fn sidecar_roundtrip_matches_protobuf_spans() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = enc_index();
    let (pb_spans, warns) = fn_spans(&idx);
    assert!(warns.is_empty());

    let side = tmp.path().join("index.scip.fndefs.db");
    let (n, build_warns) = build_sidecar(&idx, &side, "headsha").unwrap();
    assert!(build_warns.is_empty());
    let sq_spans = load_spans(&side).unwrap();

    assert_eq!(n, pb_spans.values().map(Vec::len).sum::<usize>());
    for (doc, pb_list) in &pb_spans {
        let sq_list = sq_spans.get(doc).expect("doc missing in sidecar");
        assert_eq!(
            pb_list.iter().map(key).collect::<Vec<_>>(),
            sq_list.iter().map(key).collect::<Vec<_>>(),
            "span divergence in {}",
            doc
        );
    }
}

#[test]
fn stale_guards_index_mtime_schema_and_head() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    stamp_head(&idx, "headsha");
    let loaded = load_index(&idx).unwrap();
    let side = fndefs_path(&idx);
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    assert!(
        stale_sidecar_reason(&idx, &side).is_none(),
        "fresh after build"
    );

    // index regenerated later → stale
    touch_index(&idx);
    assert!(stale_sidecar_reason(&idx, &side).is_some());

    // rebuild fresh, then corrupt schema → stale
    let loaded = load_index(&idx).unwrap();
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    {
        let conn = rusqlite::Connection::open(&side).unwrap();
        conn.execute("UPDATE meta SET value = '9' WHERE key = 'schema'", [])
            .unwrap();
    }
    assert!(stale_sidecar_reason(&idx, &side)
        .unwrap()
        .contains("schema 版本不符"));

    // rebuild fresh, then head drift → stale
    let loaded = load_index(&idx).unwrap();
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    {
        let conn = rusqlite::Connection::open(&side).unwrap();
        conn.execute("UPDATE meta SET value = 'other' WHERE key = 'head'", [])
            .unwrap();
    }
    assert!(stale_sidecar_reason(&idx, &side)
        .unwrap()
        .contains("head 變動"));

    // rebuild fresh, then corrupt bytes → stale (corrupt counts as stale)
    let loaded = load_index(&idx).unwrap();
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    std::fs::write(&side, b"not a sqlite file at all").unwrap();
    assert!(stale_sidecar_reason(&idx, &side).unwrap().contains("損壞"));
}

#[test]
fn absent_sidecar_uses_protobuf_and_never_builds() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = callers_fixture_copy(&tmp);
    let (spans, stderr) = spans_source(&idx, None).unwrap();
    let loaded = load_index(&idx).unwrap();
    let (pb_spans, _) = fn_spans(&loaded.index);
    assert!(!pb_spans.is_empty(), "fixture must carry enc spans");
    for (doc, pb_list) in &pb_spans {
        assert_eq!(
            pb_list.iter().map(key).collect::<Vec<_>>(),
            spans[doc].iter().map(key).collect::<Vec<_>>()
        );
    }
    assert!(!stderr.iter().any(|l| l.contains("重建")));
    assert!(
        !fndefs_path(&idx).exists(),
        "miss must not build the sidecar"
    );
}

#[test]
fn stale_sidecar_warns_and_rebuilds() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = callers_fixture_copy(&tmp);
    stamp_head(&idx, "headsha");
    let loaded = load_index(&idx).unwrap();
    let side = fndefs_path(&idx);
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    touch_index(&idx);

    let (spans, stderr) = spans_source(&idx, None).unwrap();
    let joined = stderr.concat();
    assert!(joined.contains("[WARN] fn_defs sidecar 過期"), "{}", joined);
    assert!(joined.contains("重建完成"), "{}", joined);
    let (pb_spans, _) = fn_spans(&loaded.index);
    for (doc, pb_list) in &pb_spans {
        assert_eq!(
            pb_list.iter().map(key).collect::<Vec<_>>(),
            spans[doc].iter().map(key).collect::<Vec<_>>(),
            "rebuilt spans must serve"
        );
    }
}

#[test]
fn rebuild_failure_falls_back_to_protobuf_spans() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = callers_fixture_copy(&tmp);
    stamp_head(&idx, "headsha");
    let loaded = load_index(&idx).unwrap();
    let side = fndefs_path(&idx);
    build_sidecar(&loaded.index, &side, "headsha").unwrap();
    touch_index(&idx);

    // read-only parent dir → rebuild cannot even create the tmp file;
    // catch_unwind so the permission restore runs before TempDir's drop
    // (a failed test would otherwise leave the dir unremovable). No-op
    // under root (root ignores mode bits) — acceptable for a dev-box suite.
    let dir = tmp.path();
    let mut perms = std::fs::metadata(dir).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o500);
    std::fs::set_permissions(dir, perms).unwrap();
    let result = std::panic::catch_unwind(|| spans_source(&idx, None));
    let mut perms = std::fs::metadata(dir).unwrap().permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(dir, perms).unwrap();

    let (spans, stderr) = result.expect("ladder must answer, not fail").unwrap();
    let joined = stderr.concat();
    assert!(
        joined.contains("重建失敗——本次查詢改用 protobuf spans"),
        "{}",
        joined
    );
    let (pb_spans, _) = fn_spans(&loaded.index);
    for (doc, pb_list) in &pb_spans {
        assert_eq!(
            pb_list.iter().map(key).collect::<Vec<_>>(),
            spans[doc].iter().map(key).collect::<Vec<_>>(),
            "fallback must serve protobuf spans"
        );
    }
}

#[test]
fn build_cache_stdout_frozen_and_three_table_bytes_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();

    // counts come from make_fixture.py build_index() (= rich_index()
    // coverage, see tests/test_scip_refs.py)
    let out = code_reality::cli::run(&["scip_refs", "--build-cache", "--index", &idx_s]);
    assert_eq!(out.exit_code, 0);
    // stdout stays the frozen single line (parity face)
    assert_eq!(
        out.stdout,
        format!(
            "[OK] cache built：{}（7 symbols/17 occurrences）\n",
            sqlite_path(&idx).display()
        )
    );
    assert!(
        out.stderr.contains("fn_defs sidecar built"),
        "{}",
        out.stderr
    );
    assert!(fndefs_path(&idx).exists());

    // SM-13: sidecar build leaves the three-table db bytes untouched
    let db = sqlite_path(&idx);
    let bytes_after_both = std::fs::read(&db).unwrap();
    // rebuild ONLY the sidecar over a stale copy: delete sidecar, corrupt
    // freshness via direct build_sidecar call, compare db bytes again
    std::fs::remove_file(fndefs_path(&idx)).unwrap();
    let loaded = load_index(&idx).unwrap();
    build_sidecar(&loaded.index, &fndefs_path(&idx), "headsha").unwrap();
    assert_eq!(
        bytes_after_both,
        std::fs::read(&db).unwrap(),
        "sidecar build must not touch the three-table db"
    );
}
