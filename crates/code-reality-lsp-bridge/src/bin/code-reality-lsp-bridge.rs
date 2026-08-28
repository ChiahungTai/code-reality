//! `code-reality-lsp-bridge` — the type-face MCP server bin (stdio).
//! The AI harness spawns and owns this process over stdin/stdout; it
//! lazily spawns the language-server backend on the first tool call
//! (default `pyrefly-lsp`, override with `--lsp-command <cmd>`).
//! The crate has no language-specific dependencies (the backend is a
//! spawn command — the P2 clause); the current tool face (languageId,
//! .py gate) is Python-specific until P2 parameterizes it.

#[tokio::main]
async fn main() {
    let mut backend = "pyrefly-lsp".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--stdio" => {} // stdio is the only mode; flag accepted for symmetry
            "--lsp-command" => match args.next() {
                Some(v) => backend = v,
                None => {
                    eprintln!("[FAIL] --lsp-command requires a value");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("[FAIL] unrecognized argument {other}");
                std::process::exit(2);
            }
        }
    }
    if let Err(e) = run(backend).await {
        eprintln!("[FAIL] {e}");
        std::process::exit(2);
    }
}

async fn run(backend: String) -> Result<(), String> {
    use rmcp::ServiceExt;
    use std::sync::Arc;

    let session = Arc::new(code_reality_lsp_bridge::LspSession::new(
        &backend,
        std::env::current_dir().unwrap_or_default(),
        500,
    ));
    let server = code_reality_lsp_bridge::server::LspBridgeServer::new(Arc::clone(&session));
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let peer = server
        .serve((stdin, stdout))
        .await
        .map_err(|e| format!("stdio serve failed: {e}"))?;
    // rmcp's serve() returns once the MCP handshake completes; tool
    // calls arrive during waiting(). Graceful backend shutdown belongs
    // AFTER the session ends — before it, lazy spawn means the backend
    // is never up yet and shutdown would be a no-op (fresh F2).
    let wait = peer.waiting().await;
    let _ = session.shutdown();
    wait.map_err(|e| format!("stdio session ended: {e}"))?;
    Ok(())
}
