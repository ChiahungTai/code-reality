//! Version-face pins (EP ep-binary-freshness-face): the `--version`
//! output shape (umbrella route arm + the mcp bin). The rev-mismatch
//! predicate and the WARN signal decision moved to the zero-dep
//! `cr-freshness` leaf crate's own tests (EP ep-cr-freshness-extraction
//! — a spawned test bin can never assert the WARN: the S2 dev-face exe
//! gate silences any target/debug binary). Spawned bins run with
//! CR_REPO pointed at a nonexistent dir so the freshness check is
//! silent and the stdout assertion is deterministic.

use std::process::Command;

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
