//! Version-face pin for the bridge bin (EP ep-binary-freshness-face
//! review R5). The WARN signal logic lives in the shared `cr-freshness`
//! leaf crate (tested there since ep-cr-freshness-extraction; the
//! former local copy is retired). CR_REPO points at a nonexistent dir
//! so the freshness check is silent and the stdout assertion is
//! deterministic.

use std::process::Command;

#[test]
fn version_face_carries_rev() {
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-lsp-bridge"))
        .arg("--version")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn bridge bin");
    assert!(out.status.success());
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        line == env!("CARGO_PKG_VERSION")
            || line.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+")),
        "pkg or pkg+rev face: {line}"
    );
    assert!(!line.contains(' '), "no spaces: {line}");
}
