//! `code-reality-lsp-bridge` — the type-face MCP server bin (stdio).
//! The AI harness spawns and owns this process over stdin/stdout; it
//! lazily spawns one language-server backend per language on the first
//! routed tool call (.py → `pyrefly-lsp`, .rs → `rust-analyzer`;
//! overrides: `--lsp-command <cmd>` for the Python backend,
//! `--rust-backend <cmd>` for the Rust backend — rust-analyzer spawns
//! with NO flags; its default stdio mode is LSP).
//! The crate has no language-specific dependencies (the P2 clause);
//! the tool face routes by file extension.

#[tokio::main]
async fn main() {
    cr_freshness::stale_binary_warn("code-reality-lsp-bridge", option_env!("CR_BUILD_REV"));
    let mut py_backend = "pyrefly-lsp".to_string();
    let mut rs_backend = "rust-analyzer".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--stdio" => {} // stdio is the only mode; flag accepted for symmetry
            "--version" | "-V" => {
                let rev = option_env!("CR_BUILD_REV");
                let face = match rev {
                    Some(r) => format!("{}+{}", env!("CARGO_PKG_VERSION"), r),
                    None => env!("CARGO_PKG_VERSION").to_string(),
                };
                println!("{face}");
                std::process::exit(0);
            }
            "--lsp-command" => match args.next() {
                Some(v) => py_backend = v,
                None => {
                    eprintln!("[FAIL] --lsp-command requires a value");
                    std::process::exit(2);
                }
            },
            "--rust-backend" => match args.next() {
                Some(v) => rs_backend = v,
                None => {
                    eprintln!("[FAIL] --rust-backend requires a value");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("[FAIL] unrecognized argument {other}");
                std::process::exit(2);
            }
        }
    }
    if let Err(e) = run(py_backend, rs_backend).await {
        eprintln!("[FAIL] {e}");
        std::process::exit(2);
    }
}

async fn run(py_backend: String, rs_backend: String) -> Result<(), String> {
    use rmcp::ServiceExt;
    use std::sync::Arc;

    let bridge = Arc::new(code_reality_lsp_bridge::server::Bridge::new(
        &py_backend,
        &rs_backend,
        std::env::current_dir().unwrap_or_default(),
    ));
    let server = code_reality_lsp_bridge::server::LspBridgeServer::new(Arc::clone(&bridge));
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let peer = server
        .serve((stdin, stdout))
        .await
        .map_err(|e| format!("stdio serve failed: {e}"))?;
    // rmcp's serve() returns once the MCP handshake completes; tool
    // calls arrive during waiting(). Graceful backend shutdown belongs
    // AFTER the session ends — before it, lazy spawn means the backends
    // are never up yet and shutdown would be a no-op (P1 fresh F2).
    let wait = peer.waiting().await;
    bridge.shutdown_all();
    wait.map_err(|e| format!("stdio session ended: {e}"))?;
    Ok(())
}
