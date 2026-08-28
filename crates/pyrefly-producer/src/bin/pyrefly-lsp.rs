//! `pyrefly-lsp` — pinned-rev host for the Pyrefly language server.
//!
//! Thin stdio entrypoint that calls the upstream `LspArgs::run` from the
//! same git-rev engine the occurrence producer links against, so the
//! type-face server and the structural-face index can never drift apart
//! in engine version. Installed alongside `pyrefly-index` by
//! `cargo install --path`; spawned as the default backend by the
//! lsp-bridge MCP server.

use std::process::ExitCode;
use std::sync::Arc;

use pyrefly::commands::lsp::IndexingMode;
use pyrefly::commands::lsp::LspArgs;
use pyrefly::lsp::non_wasm::external_provider::NoExternalProvider;
use pyrefly_util::telemetry::NoTelemetry;
use pyrefly_util::thread_pool::ThreadCount;

fn main() -> ExitCode {
    // No flags: this bin is spawned by code-reality-lsp-bridge. Reject
    // anything else loudly (a swallowed --help would hang as a server).
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--version" => {
                let rev = option_env!("CR_BUILD_REV");
                let suffix = rev.map(|r| format!("+{r}")).unwrap_or_default();
                println!(
                    "pyrefly-lsp {}{suffix} (engine: pinned git rev 1d64c4b)",
                    env!("CARGO_PKG_VERSION")
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!(
                    "error: unrecognized argument {other} — pyrefly-lsp takes no arguments \
                     (it is spawned by code-reality-lsp-bridge)"
                );
                return ExitCode::FAILURE;
            }
        }
    }
    // Mirror `pyrefly lsp` defaults: lazy non-blocking background indexing,
    // 2000-file workspace limit (the non-fbcode default).
    let args = LspArgs {
        indexing_mode: IndexingMode::default(),
        workspace_indexing_limit: 2000,
        build_system_blocking: false,
    };
    let version = concat!("pyrefly-lsp ", env!("CARGO_PKG_VERSION"));
    match args.run(
        version,
        None,
        None,
        &NoTelemetry,
        Arc::new(NoExternalProvider),
        None,
        ThreadCount::default(),
    ) {
        Ok(status) => status.to_exit_code(),
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
