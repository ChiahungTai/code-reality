//! build umbrella integration tests (EP ep-build-umbrella). Fake-bin
//! injection: producers are shell stubs on synthetic roots — the
//! process-global PATH is never mutated (cargo-test parallelism).

use code_reality::build::{build_repo, count_sources, resolve_bin, BuildError};
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
            ("target/gen.py", "x"),   // SKIP_DIRS
            (".hidden/d.py", "x"),    // dot-dir
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

#[test]
fn t4_python_leg_e2e_and_idempotent_rerun() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    // stamp-meta needs a git HEAD — make the fixture a real (tiny) repo
    for args in [
        vec!["init", "-q"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "x"],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
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
}

#[test]
fn t5_rust_empty_index_guard() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), true);
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(matches!(err, BuildError::Env(ref m) if m.contains("空索引")), "{err:?}");
}

#[test]
fn t6_rust_leg_e2e() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), false);
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots).expect("rust leg");
    assert_eq!(rep.face, "rust-face");
    assert!(rep.nodes > 0);
    assert!(rep.producers.iter().any(|p| p.contains("fake-ra")));
}

#[test]
fn t7_mixed_repo_unified_graph_concat() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[("app.py", "def g():\n    return 2\n"), ("src/lib.rs", "pub fn h() {}\n")],
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
    assert!(matches!(err, BuildError::Env(ref m) if m.contains("uv tool install pyrefly-producer")), "{err:?}");
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
    assert!(matches!(err, BuildError::Env(ref m) if m.contains("boom-child-stderr")), "{err:?}");
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
    assert!(matches!(err, BuildError::Env(ref m) if m.contains("找不到 .py 或 .rs")), "{err:?}");
}

fn graph_db_path(repo: &Path) -> PathBuf {
    repo.join(".code-reality/graph.db")
}

fn slot_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/index.scip")
}
