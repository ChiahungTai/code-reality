//! Shared test helpers (single resolution policy for the backend
//! binary — both test binaries must resolve the same one).

use std::path::PathBuf;

/// Backend binary resolution order (EP R-02): env override → PATH →
/// workspace `target/release`. On a fresh checkout, build first:
/// `cargo build --release -p pyrefly-producer --bin pyrefly-lsp`.
pub fn backend_bin() -> String {
    if let Ok(v) = std::env::var("LSP_BRIDGE_TEST_BIN") {
        return v;
    }
    if std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("pyrefly-lsp"))
                .find(|f| f.exists())
        })
        .is_some()
    {
        return "pyrefly-lsp".to_string();
    }
    let ws_target =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/pyrefly-lsp");
    if ws_target.exists() {
        return ws_target.to_string_lossy().to_string();
    }
    panic!(
        "pyrefly-lsp backend not found — set LSP_BRIDGE_TEST_BIN, put it on PATH, \
         or run: cargo build --release -p pyrefly-producer --bin pyrefly-lsp"
    );
}
