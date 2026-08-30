//! build umbrella integration tests (EP ep-build-umbrella). Fake-bin
//! injection: producers are shell stubs on synthetic roots — the
//! process-global PATH is never mutated (cargo-test parallelism).

use code_reality::build::{
    build_repo, count_sources, ensure_fresh, heal_outcome_after_rebuild_err, BuildError,
    HealOutcome,
};
use code_reality::common::resolve_bin;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");

fn fake_bin(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn mkrepo(t: &tempfile::TempDir, files: &[(&str, &str)]) -> PathBuf {
    let repo = t.path().to_path_buf();
    for (rel, content) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    repo
}

/// Parses `--repo <v>` / `--output <v>` pairs from "$@".
fn arg_parse_sh() -> &'static str {
    r#"prev=''; for a in "$@"; do if [ "$prev" = "--repo" ] || [ "$prev" = "--output" ]; then eval "${prev#--}=\"$a\""; fi; prev="$a"; done"#
}

fn fake_pyrefly(dir: &Path) {
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
mkdir -p \"$repo/.code-reality/scip\"
cp '{FIXTURE}' \"$repo/.code-reality/scip/index.scip\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh()
        ),
    );
}

fn fake_rust_analyzer(dir: &Path, empty: bool) {
    let payload = if empty {
        "printf 'x0123456789x0123456789x0123456789x0123456789x0123456789' > \"$output\"".to_string()
    } else {
        format!("cp '{FIXTURE}' \"$output\"")
    };
    fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi
{}
{payload}
echo '[OK] fake rust-analyzer'
",
            arg_parse_sh()
        ),
    );
}

#[test]
fn t1_count_sources_detection_matrix() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("a.py", "x"),
            ("b.py", "x"),
            ("src/c.rs", "x"),
            ("target/gen.py", "x"), // SKIP_DIRS
            (".hidden/d.py", "x"),  // dot-dir
            ("notes.txt", "x"),
        ],
    );
    assert_eq!(count_sources(&repo).unwrap(), (2, 1));

    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("only.py", "x")]);
    assert_eq!(count_sources(&repo2).unwrap(), (1, 0));

    let t3 = tempfile::tempdir().unwrap();
    let repo3 = mkrepo(&t3, &[("only.rs", "x")]);
    assert_eq!(count_sources(&repo3).unwrap(), (0, 1));
}

#[test]
fn t2_resolve_bin_injection() {
    let t = tempfile::tempdir().unwrap();
    fake_bin(t.path(), "pyrefly-index", "#!/bin/sh\nexit 0\n");
    let roots = vec![t.path().to_path_buf()];
    assert!(resolve_bin("pyrefly-index", &roots, "hint").is_ok());

    // non-executable file is not a hit
    std::fs::write(t.path().join("rust-analyzer"), "not exec").unwrap();
    let miss = resolve_bin("rust-analyzer", &roots, "INSTALL-HINT").unwrap_err();
    assert!(miss.contains("INSTALL-HINT"), "miss={miss}");
    assert!(miss.contains(&t.path().display().to_string()));
}

#[test]
fn t3_run_usage_and_producer_validation() {
    let o = code_reality::build::run(&["build"]);
    assert_eq!(o.exit_code, 2);
    let o = code_reality::build::run(&["build", "--repo"]);
    assert_eq!(o.exit_code, 2);
    let o = code_reality::build::run(&["build", "--repo", "/nonexistent-xyz", "--producer", "go"]);
    assert_eq!(o.exit_code, 2);
    assert!(o.stderr.contains("rust 或 python"));
    let o = code_reality::build::run(&["build", "--help"]);
    assert_eq!(o.exit_code, 0);
    assert!(o.stdout.contains("--producer"));
}

fn git_init(repo: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "x",
        ],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
}

#[test]
fn t4_python_leg_e2e_and_idempotent_rerun() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("python leg");
    assert_eq!(rep.face, "python-face");
    assert!(rep.nodes > 0, "nodes={}", rep.nodes);
    assert!(graph_db_path(&repo).exists());
    assert!(rep.producers.iter().any(|p| p.contains("fake-pyrefly")));
    // umbrella stamps index provenance in-process (relay Finding B)
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "index.scip.meta missing after build"
    );

    let rep2 = build_repo(&repo, None, &roots).expect("rerun");
    assert!(rep2.graph_rebuilt, "second run rebuilds");
    assert_eq!(rep2.indexes_created, 0);
    assert!(rep2.indexes_skipped > 0);
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "meta survives idempotent rerun"
    );
}

#[test]
fn t5_rust_empty_index_guard() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), true);
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("空索引")),
        "{err:?}"
    );
    // failed leg leaves no sibling part behind
    assert!(!repo.join(".code-reality/scip/.rust-part.scip").exists());
}

#[test]
fn t6_rust_leg_e2e() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), false);
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots).expect("rust leg");
    assert_eq!(rep.face, "rust-face");
    assert!(rep.nodes > 0);
    assert!(rep.producers.iter().any(|p| p.contains("fake-ra")));
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "rust path stamps too"
    );
    // single-leg atomicity: the slot is the renamed part (fixture bytes),
    // no sibling residue
    let fixture_len = std::fs::metadata(FIXTURE).unwrap().len();
    assert_eq!(
        std::fs::metadata(slot_of(&repo)).unwrap().len(),
        fixture_len
    );
    assert!(!repo.join(".code-reality/scip/.rust-part.scip").exists());
}

#[test]
fn t7_mixed_repo_unified_graph_concat() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "def g():\n    return 2\n"),
            ("src/lib.rs", "pub fn h() {}\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    fake_rust_analyzer(bindir.path(), false);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("mixed unified");
    assert_eq!(rep.face, "mixed(rust+python)");
    assert!(rep.nodes > 0);
    // cat-merge proof: the slot is exactly python-part ++ rust-part.
    let fixture_len = std::fs::metadata(FIXTURE).unwrap().len();
    let slot_len = std::fs::metadata(slot_of(&repo)).unwrap().len();
    assert_eq!(slot_len, fixture_len * 2, "slot={slot_len}");
    assert!(!rep.repo.join(".code-reality/scip/.rust-part.scip").exists());

    // single-leg override on a mixed repo → note about the other face
    let rep2 = build_repo(&repo, Some("rust"), &roots).expect("producer override");
    assert_eq!(rep2.face, "rust-face");
    assert!(rep2.notes.iter().any(|n| n.contains("未索引：python")));
}

#[test]
fn t8_missing_bin_is_env_fail_with_hint() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x = 1\n")]);
    let empty_roots = vec![PathBuf::from("/nonexistent-bin-dir")];
    let err = build_repo(&repo, None, &empty_roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("uv tool install pyrefly-producer")),
        "{err:?}"
    );
}

#[test]
fn t9_child_failure_maps_env_with_stderr_and_rust_hint() {
    // python leg: producer exits 1 with stderr → Env, child stderr embedded (SM-7)
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x = 1\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 1; fi\necho 'boom-child-stderr' >&2\nexit 1\n",
    );
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("boom-child-stderr")),
        "{err:?}"
    );
    // rust leg: bin missing → hint mentions rustup (SM-14)
    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("src/lib.rs", "pub fn f() {}\n")]);
    let err2 = build_repo(&repo2, None, &roots).unwrap_err();
    assert!(
        matches!(err2, BuildError::Env(ref m) if m.contains("rustup component add rust-analyzer")),
        "{err2:?}"
    );
}

#[test]
fn t10_run_non_dir_fails_loud() {
    let o = code_reality::build::run(&["build", "--repo", "/nonexistent-xyz-abc"]);
    assert_eq!(o.exit_code, 2);
    assert!(o.stderr.contains("不是目錄"), "stderr={}", o.stderr);
}

#[test]
fn t11_empty_repo_env_fail() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("notes.txt"), "no code").unwrap();
    let err = build_repo(t.path(), None, &[]).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("找不到 .py 或 .rs")),
        "{err:?}"
    );
}

#[test]
fn t12_edge_dedupe_doubled_index_converges() {
    // The NT cold-start transient shape: the same index concatenated
    // twice (protobuf same-type merge stacks occurrences). Edge counts
    // must converge — dedupe at the materialization boundary.
    // rich_callers carries attributed refs (rich.scip is defs-only)
    let single = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rich_callers.scip"
    ))
    .unwrap();
    let mk = |bytes: Vec<u8>| {
        let t = tempfile::tempdir().unwrap();
        let slot_dir = t.path().join(".code-reality/scip");
        std::fs::create_dir_all(&slot_dir).unwrap();
        let idx = slot_dir.join("index.scip");
        std::fs::write(&idx, bytes).unwrap();
        (t, idx)
    };
    let (t1, i1) = mk(single.clone());
    let g1 = code_reality::graph_db::build_from_cache_at(t1.path(), &i1).unwrap();
    let (t2, i2) = mk([single.clone(), single].concat());
    let g2 = code_reality::graph_db::build_from_cache_at(t2.path(), &i2).unwrap();
    assert!(g1.edges > 0);
    assert_eq!(
        g1.edges, g2.edges,
        "doubled index must converge: single={} doubled={}",
        g1.edges, g2.edges
    );
    assert_eq!(g1.nodes, g2.nodes);
}

fn graph_db_path(repo: &Path) -> PathBuf {
    repo.join(".code-reality/graph.db")
}

fn slot_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/index.scip")
}

// ---------- S3: query-time heal (ep-index-query-time-self-heal) ----------

fn fake_pyrefly_pre(dir: &Path, pre: &str) {
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
{pre}
mkdir -p \"$repo/.code-reality/scip\"
cp '{FIXTURE}' \"$repo/.code-reality/scip/index.scip\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh()
        ),
    );
}

fn fake_rust_analyzer_counted(dir: &Path, counter: &Path) {
    fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi
{}
echo x >> '{}'
cp '{FIXTURE}' \"$output\"
echo '[OK] fake rust-analyzer'
",
            arg_parse_sh(),
            counter.display()
        ),
    );
}

fn counter_lines(c: &Path) -> usize {
    std::fs::read_to_string(c)
        .map(|t| t.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

fn git_commit(repo: &Path) {
    for args in [
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "x",
        ],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
}

fn lock_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/.heal.lock")
}

#[test]
fn t13_fresh_noop_is_zero_spawn() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_pyrefly_pre(bindir.path(), &format!("echo x >> '{}'", counter.display()));
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("initial build");
    std::fs::remove_file(&counter).unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert_eq!(out, HealOutcome::Fresh);
    assert_eq!(counter_lines(&counter), 0, "steady state spawns nothing");
}

#[test]
fn t14_stale_heals_and_releases_lock() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::Healed { nodes, .. } if nodes > 0),
        "out={out:?}"
    );
    assert!(!lock_of(&repo).exists(), "lock released after heal");
}

#[test]
fn t15_head_drift_only_no_rebuild() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_pyrefly_pre(bindir.path(), &format!("echo x >> '{}'", counter.display()));
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::fs::remove_file(&counter).unwrap();
    std::fs::write(repo.join("docs.md"), "docs only\n").unwrap();
    git_commit(&repo);

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert_eq!(out, HealOutcome::Fresh, "head-drift-only never rebuilds");
    assert_eq!(counter_lines(&counter), 0);
}

#[test]
fn t16_producer_fail_serve_stale_lock_released() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    // swap the producer to fail on run (--version still answers)
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
exit 1
",
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::ServeStale(ref lines) if lines.iter().any(|l| !l.is_empty())),
        "out={out:?}"
    );
    assert!(!lock_of(&repo).exists(), "lock released after failed heal");
}

#[test]
fn t17_half_success_probe() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");

    // index fresh + graph step failed → Healed with a graph note, not
    // mislabeled serve-stale (SM-17)
    let out =
        heal_outcome_after_rebuild_err(&repo, &slot_of(&repo), "graph core boom".into()).unwrap();
    match &out {
        HealOutcome::Healed { notes, .. } => {
            assert!(notes.iter().any(|n| n.contains("graph 未重建")), "{out:?}")
        }
        other => panic!("expected Healed, got {other:?}"),
    }

    // index still stale + rebuild failed → ServeStale carrying the reason
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let out2 =
        heal_outcome_after_rebuild_err(&repo, &slot_of(&repo), "graph core boom".into()).unwrap();
    assert!(
        matches!(out2, HealOutcome::ServeStale(ref l) if l[0].contains("graph core boom")),
        "out={out2:?}"
    );
}

#[test]
fn t18_false_stale_warns_once_no_loop() {
    // Disk-vs-index alignment per the EP fixture spec: rich.scip's docs
    // are crates/a.rs + crates/b.rs, so the repo carries exactly those
    // plus one extra disk file the fake never indexes → persistent
    // missing=1 (the false-stale shape).
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("crates/a.rs", "x"), ("crates/b.rs", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_rust_analyzer_counted(bindir.path(), &counter);
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::fs::remove_file(&counter).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("crates/c.rs"), "x").unwrap();

    let out1 = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out1, HealOutcome::ServeStale(ref l) if l[0].contains("語料不一致")),
        "out={out1:?}"
    );
    // WARN-once semantics: the healed slot is fresh by mtime, the second
    // query rebuilds nothing (SM-9)
    let out2 = ensure_fresh(&repo, &roots).unwrap();
    assert_eq!(out2, HealOutcome::Fresh);
    assert_eq!(
        counter_lines(&counter),
        1,
        "exactly one rebuild across both calls"
    );
}

#[test]
fn t19_lock_escape_and_single_flight() {
    // (a) an abandoned lock (mtime past max age) is stolen
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    std::fs::write(lock_of(&repo), "1 2020-01-01T00:00:00+00:00\n").unwrap();
    let st = std::process::Command::new("touch")
        .args(["-t", "200001010000"])
        .arg(lock_of(&repo))
        .status()
        .unwrap();
    assert!(st.success());
    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "out={out:?}");
    assert!(!lock_of(&repo).exists());

    // (b) concurrent stale hits single-flight through the lock: a slow
    // producer runs exactly once across both healers
    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo2);
    let bindir2 = tempfile::tempdir().unwrap();
    let counter2 = bindir2.path().join("calls");
    fake_pyrefly_pre(
        bindir2.path(),
        &format!("sleep 1; echo x >> '{}'", counter2.display()),
    );
    let roots2 = vec![bindir2.path().to_path_buf()];
    build_repo(&repo2, None, &roots2).expect("build");
    std::fs::remove_file(&counter2).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo2.join("app2.py"), "x = 1\n").unwrap();

    let h1 = {
        let (r, roots) = (repo2.clone(), roots2.clone());
        std::thread::spawn(move || ensure_fresh(&r, &roots))
    };
    let h2 = {
        let (r, roots) = (repo2.clone(), roots2.clone());
        std::thread::spawn(move || ensure_fresh(&r, &roots))
    };
    let o1 = h1.join().unwrap().unwrap();
    let o2 = h2.join().unwrap().unwrap();
    assert!(
        !matches!(o1, HealOutcome::ServeStale(_)) && !matches!(o2, HealOutcome::ServeStale(_)),
        "o1={o1:?} o2={o2:?}"
    );
    assert_eq!(
        counter_lines(&counter2),
        1,
        "single-flight: one producer run"
    );
}

#[test]
fn t20_readonly_slot_dir_serves_stale() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let dir = repo.join(".code-reality/scip");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = ensure_fresh(&repo, &roots).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(out, HealOutcome::ServeStale(ref l) if l[0].contains("heal lock")),
        "out={out:?}"
    );
}

// env var is process-global — guard the off-switch test against the
// parallel heal tests in this file
static HEAL_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn t21_cli_heal_across_query_modes() {
    // e2e through the real cli face: the heal's build_repo resolves the
    // producer from the real roots (PATH) — the L4-shaped path with the
    // actual installed pyrefly-index (machine prerequisite). Holds
    // HEAL_ENV throughout: the env gate is process-global and t22
    // flips it (parallel-test race guard).
    let _guard = HEAL_ENV.lock().unwrap();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let repo_s = repo.display().to_string();
    let stale = |n: &str| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(repo.join(n), "def q():\n    return f()\n").unwrap();
    };

    // query mode: heal fires, the fresh real index answers
    stale("app2.py");
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);
    assert!(out.stdout.contains("f()."), "stdout={}", out.stdout);

    // callers / closure / audit ride the same hook (placed after the
    // final guard, before every mode branch)
    stale("app3.py");
    let out = code_reality::cli::run(&["scip_refs", "--callers", "f", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);

    stale("app4.py");
    let out = code_reality::cli::run(&["scip_refs", "--closure", "f", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);

    // audit mode (graph_audit env prerequisite: real rust-analyzer)
    stale("app5.py");
    let out = code_reality::cli::run(&["scip_refs", "--audit", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);
}

#[test]
fn t23_sm15_staleness_check_err_face() {
    // SM-15: the staleness CHECK itself failing (unreadable subdir) →
    // loud WARN on stderr, query still answers from the existing index
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    let locked = repo.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("z.py"), "x = 1\n").unwrap();
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let repo_s = repo.display().to_string();
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        out.stderr.contains("索引過期檢查失敗"),
        "stderr={}",
        out.stderr
    );
    // still answers: the rich fixture has no f() → no-DEF exit 1 is an answer
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.contains("[WARN] 查無 DEF"));
}

#[test]
fn t22_cli_off_switch_and_non_heal_faces() {
    let _guard = HEAL_ENV.lock().unwrap();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let slot = slot_of(&repo);
    let before = std::fs::read(&slot).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let repo_s = repo.display().to_string();
    let slot_s = slot.display().to_string();

    // env off → old behavior: no heal, stale slot answered as-is
    std::env::set_var("CODE_REALITY_AUTOHEAL", "off");
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    std::env::remove_var("CODE_REALITY_AUTOHEAL");
    assert_eq!(out.exit_code, 1, "rich fixture has no f() → 查無 DEF");
    assert!(
        !out.stderr.contains("index healed"),
        "stderr={}",
        out.stderr
    );
    assert_eq!(std::fs::read(&slot).unwrap(), before, "slot untouched");

    // explicit --index is user-owned: never healed
    let out = code_reality::cli::run(&["scip_refs", "f", "--index", &slot_s, "--repo", &repo_s]);
    assert_eq!(out.exit_code, 1);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(std::fs::read(&slot).unwrap(), before);

    // write modes never heal (stamp / build-cache own their flow)
    let out = code_reality::cli::run(&["scip_refs", "--stamp-meta", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "stamp rewrites meta only"
    );
    let out = code_reality::cli::run(&["scip_refs", "--build-cache", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(std::fs::read(&slot).unwrap(), before);
}
