//! `code-reality-mcp` — the MCP bin. Two modes:
//! - `--stdio` (plugin default): the AI harness spawns and owns this
//!   process over stdin/stdout — zero daemon, zero port, any OS.
//! - HTTP resident (default): streamable-http on 127.0.0.1:8200/mcp;
//!   launchd/systemd owns the lifecycle (multi-harness sharing).

#[tokio::main]
async fn main() {
    code_reality::freshness::stale_binary_warn("code-reality");
    let stdio = std::env::args_os().any(|a| a.to_string_lossy() == "--stdio");
    let result = if stdio {
        code_reality::mcp_server::serve_stdio().await
    } else {
        let port: u16 = std::env::var("CODE_REALITY_MCP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8200);
        code_reality::mcp_server::serve(port).await
    };
    if let Err(e) = result {
        eprintln!("[FAIL] {e}");
        std::process::exit(2);
    }
}
