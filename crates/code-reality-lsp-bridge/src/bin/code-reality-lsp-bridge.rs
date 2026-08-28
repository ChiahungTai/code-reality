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
    stale_binary_warn();
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

/// Local copy of the freshness warn (the crate depends on no workspace
/// crate — the P2 independence clause; keep in sync with
/// code-reality/src/freshness.rs). One stderr line per process when the
/// installed binary lags the CR checkout.
fn stale_binary_warn() {
    static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if WARNED.set(()).is_err() {
        return;
    }
    let Some(embedded) = option_env!("CR_BUILD_REV") else {
        return;
    };
    let repo = std::env::var_os("CR_REPO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join("Github/code-reality")
        });
    if !repo.is_dir() {
        return;
    }
    if !repo.join("crates/code-reality-lsp-bridge").is_dir() {
        return;
    }
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if let Some(head) = head {
        let base = embedded.strip_suffix("-dirty").unwrap_or(embedded);
        if !base.is_empty() && base != "unknown" && !head.starts_with(base) {
            let short = &head[..head.len().min(7)];
            eprintln!(
                "[WARN] installed binary {embedded} != repo HEAD {short} — rerun: cargo install --path {}/crates/code-reality-lsp-bridge",
                repo.display()
            );
            return;
        }
    }
    let dirty = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["status", "--porcelain", "--", "crates/"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);
    if dirty {
        eprintln!(
            "[WARN] CR checkout {} has uncommitted changes under crates/ — installed binary may lag (commit triggers auto-reinstall)",
            repo.display()
        );
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
