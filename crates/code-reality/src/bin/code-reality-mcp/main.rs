//! `code-reality-mcp` — the MCP bin. Two modes:
//! - `--stdio` (plugin default): the AI harness spawns and owns this
//!   process over stdin/stdout — zero daemon, zero port, any OS.
//! - HTTP resident (default): streamable-http on 127.0.0.1:8200/mcp;
//!   launchd/systemd owns the lifecycle (multi-harness sharing).
//!
//! `--version`/`--help` answer and exit before any mode starts, and any
//! other argument fails loud — the membership-test parse used to route
//! EVERY unknown flag (including `--version`) into the HTTP resident
//! default, so a version probe silently started a listener and hung
//! (2026-08-28). The per-arg loop is ordered (lsp-bridge shape): the
//! first unexpected argument rejects, so `--bogus --version` never
//! masquerades as a successful version probe.

#[tokio::main]
async fn main() {
    code_reality::freshness::stale_binary_warn("code-reality");
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut stdio = false;
    for a in &args {
        match a.as_str() {
            "--stdio" => stdio = true,
            "--version" | "-V" => {
                println!("{}", code_reality::freshness::version_face());
                return;
            }
            "--help" | "-h" => {
                println!(
                    "code-reality-mcp — MCP server: --stdio (harness-owned) | no args: streamable-http 127.0.0.1:8200/mcp (launchd-owned)"
                );
                return;
            }
            bad => {
                eprintln!(
                    "[FAIL] unknown argument: {bad} (supported: --stdio; no args = HTTP resident mode)"
                );
                std::process::exit(2);
            }
        }
    }
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
