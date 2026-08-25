//! `code-reality-mcp` — the MCP daemon bin (R6). Streamable-HTTP on
//! 127.0.0.1:8200/mcp; launchd owns the lifecycle (KeepAlive restarts,
//! `cargo install --path` + `launchctl kickstart` upgrades — AD-2).

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("CODE_REALITY_MCP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8200);
    if let Err(e) = code_reality::mcp_server::serve(port).await {
        eprintln!("[FAIL] {e}");
        std::process::exit(2);
    }
}
