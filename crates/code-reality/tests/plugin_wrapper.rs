// Wrapper regression mount (EP: ep-npm-embedded-face S3): cargo test is
// the repo's sole test face, so the plugin wrapper checks are mounted
// here by running scripts/test-plugin-wrapper.sh — the quadrant +
// precedence assertions over the REAL .mcp.json strings (jq-extracted).
// Requires jq and /bin/sh on PATH (maintainer machines and CI runners
// have both); a missing jq fails loud rather than skipping.
use std::process::Command;

#[test]
fn plugin_wrapper_regression() {
    // Anchor via the compile-time manifest dir — the runtime CWD of a
    // test binary is not guaranteed to be the package root.
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/test-plugin-wrapper.sh");
    let out = Command::new(&script)
        .output()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", script.display()));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "wrapper regression failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains(", 0 failed"),
        "wrapper regression reported failures: {stdout}"
    );
}
