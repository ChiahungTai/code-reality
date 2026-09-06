//! R3 S3 CLI tests for the caller-edge modes: mutex family (including the
//! no-query `--build-cache --callers` silent-swallow guard), `--depth`
//! value family, abbreviation surface (`--c` becomes ambiguous), frozen
//! no-query text, and exact stdout shapes for `--callers` / `--closure` on
//! the committed rich_callers.scip fixture (protobuf face, no db/sidecar).

use code_reality::cli::run;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "tests/fixtures/rich_callers.scip";

fn fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(fixture_src(), &dst).unwrap();
    dst
}

fn fixture_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

fn run_fixture(tmp: &tempfile::TempDir, extra: &[&str]) -> code_reality::ToolOutput {
    let idx = fixture_copy(tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let mut argv = vec!["scip_refs"];
    for e in extra {
        argv.push(e);
    }
    argv.push("--index");
    argv.push(&idx_s);
    run(&argv)
}

// ---------- mutex family ----------

#[test]
fn callers_closure_mutex() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--callers", "--closure", "open"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --callers 與 --closure 互斥\n");
}

#[test]
fn build_cache_with_callers_and_no_query_fails_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--build-cache", "--callers"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(
        out.stderr,
        "[FAIL] --build-cache 與 --stamp-meta/--audit/查詢互斥\n"
    );
}

#[test]
fn stamp_meta_with_closure_fails_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let out = run(&[
        "scip_refs",
        "--stamp-meta",
        "--closure",
        "--repo",
        "/tmp/anywhere",
        "--index",
        &idx_s,
    ]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --stamp-meta 與 --audit/查詢互斥\n");
}

#[test]
fn depth_requires_closure() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--depth", "2", "open"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --depth 僅伴 --closure 使用\n");
}

#[test]
fn depth_value_family() {
    let tmp = tempfile::tempdir().unwrap();
    for bad in ["0", "-1", "abc", "2.5", "10001", "18446744073709551615"] {
        let out = run_fixture(&tmp, &["--closure", "--depth", bad, "open"]);
        assert_eq!(out.exit_code, 2, "depth {} must fail", bad);
        assert!(
            out.stderr
                .starts_with(&format!("[FAIL] --depth 需正整數（1-10000）：{}", bad)),
            "{}",
            out.stderr
        );
    }
    for good in ["1", "9", "10000"] {
        let out = run_fixture(&tmp, &["--closure", "--depth", good, "open"]);
        assert_eq!(out.exit_code, 0, "depth {} must pass", good);
    }
}

// ---------- abbreviation surface ----------

#[test]
fn abbreviations_resolve_and_c_is_ambiguous() {
    let tmp = tempfile::tempdir().unwrap();
    // unique prefixes
    let out = run_fixture(&tmp, &["--call", "open"]);
    assert_eq!(out.exit_code, 0, "--call resolves to --callers");
    let out = run_fixture(&tmp, &["--clos", "--dep", "1", "open"]);
    assert_eq!(out.exit_code, 0, "--clos/--dep resolve");
    // `--c` now matches both --callers and --closure → ambiguous (exit 2,
    // stderr face)
    let out = run_fixture(&tmp, &["--c", "open"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("ambiguous"), "{}", out.stderr);
}

#[test]
fn no_query_with_callers_hits_frozen_final_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--callers"]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] 需提供查詢或 --audit\n");
}

// ---------- output shapes (exact pins; protobuf face, no [SRC]) ----------

const EXPECTED_CALLERS: &str = "\
[OK] EventStoreLifecycle.open：8 callers（9 sites）
  kernel/impl#[EventStoreLifecycle]macro_fn().（1 處）
    crates/a.rs:19
  kernel/tests/tie_one().（1 處）
    crates/a.rs:15
  kernel/inner().（1 處）
    crates/a.rs:125
  kernel/outer().（1 處）
    crates/a.rs:110
  kernel/impl#[EventStoreLifecycle][KernelEventStore]delegate().（1 處）
    crates/a.rs:1356
  kernel/tests/t_one().（1 處）
    crates/a.rs:1742
  kernel/tests/t_two().（2 處）
    crates/a.rs:2461
    crates/a.rs:2470
  kernel/cycle_a().（1 處）
    crates/b.rs:408
  item-level：1 處（未歸屬 fn——use/const/屬性層）
    crates/a.rs:999
";

#[test]
fn callers_output_exact_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--callers", "EventStoreLifecycle.open"]);
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stdout, EXPECTED_CALLERS);
}

#[test]
fn callers_no_def_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--callers", "nosuch_method"]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stdout, "[WARN] 查無 DEF：nosuch_method\n");
}

#[test]
fn callers_zero_refs_stable_shape() {
    // outer()/inner()/cycle fns have DEFs but no refs → 0 callers, 0 sites,
    // 0 item-level, exit 0 (R3-K)
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--callers", "inner"]);
    assert_eq!(out.exit_code, 0);
    assert_eq!(
        out.stdout,
        "[OK] inner：0 callers（0 sites）\n  item-level：0 處（未歸屬 fn——use/const/屬性層）\n"
    );
}

#[test]
fn closure_depth2_and_depth3_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(&tmp, &["--closure", "EventStoreLifecycle.open"]);
    assert_eq!(out.exit_code, 0);
    let expected = concat!(
        "[OK] closure：EventStoreLifecycle.open（depth=2）\n",
        "  depth 1：8 callers\n",
        "    crates/a.rs：7 符號\n",
        "    crates/b.rs：1 符號\n",
        "  depth 2：1 callers\n",
        "    crates/b.rs：1 符號\n",
        "  cycles：0 處（frontier 重入已拜訪符號）\n",
    );
    assert_eq!(out.stdout, expected);
    let out = run_fixture(
        &tmp,
        &["--closure", "--depth", "3", "EventStoreLifecycle.open"],
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("depth 3：0 callers"), "{}", out.stdout);
    assert!(
        out.stdout
            .contains("cycles：1 處（frontier 重入已拜訪符號）"),
        "{}",
        out.stdout
    );
}

#[test]
fn closure_depth1_is_callers_set() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_fixture(
        &tmp,
        &["--closure", "--depth", "1", "EventStoreLifecycle.open"],
    );
    assert_eq!(out.exit_code, 0);
    // third source: closure level-1 == the callers set (8 here)
    assert!(out.stdout.contains("depth 1：8 callers"), "{}", out.stdout);
    assert!(
        !out.stdout.contains("depth 2"),
        "depth 1 must truncate: {}",
        out.stdout
    );
}

#[test]
fn callers_mode_answers_python_class_query() {
    // AIR-33 ①: a bare class-name query must reach the caller-edge modes
    // (previously the fn-tail-only matcher answered "查無 DEF" for every
    // `Class#` symbol). Caller attribution rides the CALLER fn's span —
    // the class callee needs no span of its own.
    use protobuf::Message;
    use scip::types::{Document, Index, Occurrence};
    let class = "pyrefly python p 0.1.0 `pkg.mod`/Widget#";
    let fn_sym = "pyrefly python p 0.1.0 `pkg.other`/use_widget().";
    let mk = |symbol: &str, line: i32, def: bool, enc: Option<Vec<i32>>| {
        let mut o = Occurrence::new();
        o.symbol = symbol.to_string();
        o.range = vec![line, 0];
        o.symbol_roles = if def { 1 } else { 0 };
        o.enclosing_range = enc.unwrap_or_default();
        o
    };
    let mut d1 = Document::new();
    d1.relative_path = "pkg/mod.py".to_string();
    d1.occurrences = vec![mk(class, 9, true, None)];
    let mut d2 = Document::new();
    d2.relative_path = "pkg/other.py".to_string();
    d2.occurrences = vec![
        // caller fn spans lines 1-5 (0-based enc [0,0,4,0]); the class
        // reference at 0-based line 3 sits inside it
        mk(fn_sym, 0, true, Some(vec![0, 0, 4, 0])),
        mk(class, 3, false, None),
    ];
    let mut index = Index::new();
    index.documents = vec![d1, d2];

    let tmp = tempfile::tempdir().unwrap();
    let idx = tmp.path().join("class_callers.scip");
    std::fs::write(&idx, index.write_to_bytes().unwrap()).unwrap();
    let idx_s = idx.to_string_lossy().to_string();
    let out = run(&["scip_refs", "--callers", "Widget", "--index", &idx_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(!out.stdout.contains("查無 DEF"), "{}", out.stdout);
    assert!(out.stdout.contains("Widget：1 callers"), "{}", out.stdout);
    assert!(out.stdout.contains("use_widget"), "{}", out.stdout);
    assert!(
        out.stdout.contains("pkg/other.py:4"),
        "site is the class reference line: {}",
        out.stdout
    );
}
