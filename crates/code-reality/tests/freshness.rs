//! Freshness face pins (EP ep-binary-freshness-face): the rev-mismatch
//! predicate (table-driven) and the `--version` output shape (umbrella
//! route arm + the mcp bin). Spawned bins run with CR_REPO pointed at a
//! nonexistent dir so the freshness check is silent and the stdout
//! assertion is deterministic.

use std::process::Command;

const HEAD: &str = "2442692582edb2031f07a820da94b4b921f84888";

#[test]
fn rev_mismatch_table() {
    // same commit: abbreviated embedded prefixes the full head hash
    assert!(!code_reality::freshness::rev_mismatch("2442692", HEAD));
    // dirty install is not a stale one
    assert!(!code_reality::freshness::rev_mismatch(
        "2442692-dirty",
        HEAD
    ));
    // abbreviation-length drift (8-char embed of the same commit)
    assert!(!code_reality::freshness::rev_mismatch("24426925", HEAD));
    // different commit
    assert!(code_reality::freshness::rev_mismatch("3980fe1", HEAD));
    assert!(code_reality::freshness::rev_mismatch("3980fe1-dirty", HEAD));
    // absent/unusable embedded face never warns
    assert!(!code_reality::freshness::rev_mismatch("", HEAD));
    assert!(!code_reality::freshness::rev_mismatch("unknown", HEAD));
}

#[test]
fn umbrella_version_face_carries_rev() {
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality"))
        .arg("--version")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.trim();
    assert!(
        line == env!("CARGO_PKG_VERSION")
            || line.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+")),
        "pkg or pkg+rev face (git-less builds fall back to pkg-only): {line}"
    );
    assert!(!line.contains(' '), "no spaces in the version face: {line}");
}

#[test]
fn mcp_version_face_carries_rev() {
    // Root cause pin (2026-08-28): the mcp bin had no --version arm —
    // every unknown flag fell into the HTTP resident default, so this
    // spawn used to hang on a listener instead of answering.
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .arg("--version")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.trim();
    assert!(
        line == env!("CARGO_PKG_VERSION")
            || line.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+")),
        "pkg or pkg+rev face (git-less builds fall back to pkg-only): {line}"
    );
    assert!(!line.contains(' '), "no spaces in the version face: {line}");
}

#[test]
fn mcp_rejects_unknown_arg_loudly() {
    // The same trap class: a typo'd flag must fail loud, never silently
    // start the HTTP resident mode.
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .arg("--bogus")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown argument"),
        "loud usage error: {stderr}"
    );
}

#[test]
fn mcp_help_face_answers_on_stdout() {
    // Review R-1: explicit --help is stdout + exit 0 (umbrella/CLI
    // convention), not stderr.
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .arg("--help")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--stdio") && stdout.contains("8200"),
        "help describes both modes on stdout: {stdout}"
    );
}

#[test]
fn mcp_arg_priority_is_ordered() {
    // Review R-2: the per-arg loop is ordered — the first unexpected
    // argument rejects, so a typo is never swallowed by a later
    // diagnostic flag; `--stdio --version` answers as a version probe.
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .args(["--bogus", "--version"])
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert_eq!(
        out.status.code(),
        Some(2),
        "typo before --version must reject, not answer"
    );
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .args(["--stdio", "--version"])
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert!(out.status.success());
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        line == env!("CARGO_PKG_VERSION")
            || line.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+")),
        "version wins the combination: {line}"
    );
}

#[test]
fn umbrella_warns_when_checkout_head_differs() {
    // Fixture repo with a crates/code-reality/ layout and its own HEAD —
    // the spawned bin's embedded rev can never prefix-match it (review
    // R5: the WARN emission path had no automated test).
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("cr");
    std::fs::create_dir_all(repo.join("crates/code-reality")).unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .expect("git fixture")
            .status
            .success()
    };
    assert!(git(&["init", "-q", "."]));
    assert!(git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "x"
    ]));
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality"))
        .args(["scip_refs", "-h"])
        .env("CR_REPO", &repo)
        .output()
        .expect("spawn code-reality bin");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("!= repo HEAD"),
        "mismatch WARN emitted: {stderr}"
    );
}
