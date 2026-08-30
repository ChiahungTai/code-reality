//! code-reality — Rust carrier of the code-reality toolchain.
//!
//! Layering (AD-2, ep-rust-migration.md): one lib, two thin frontends
//! (umbrella CLI bin, future MCP bin) sharing the same lib. Lib functions
//! return [`ToolOutput`] data — the lib never prints and never calls
//! `std::process::exit`; bins own printing and exiting. This is the
//! compile-time premise of "CLI = MCP single backend" drift-freedom.
//!
//! Module taxonomy maps the blueprint's domain/use-case/adapter layers:
//! - engine: domain + use case (symbol predicates, attribution, query
//!   orchestration over the SCIP protobuf face)
//! - cache: adapter (derived sqlite three-table cache + face selection)
//! - cli: assembly (argument surface, mode routing)

/// Completed tool run: everything a bin needs to print/exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ToolOutput {
    /// Environment-level loud failure (`[FAIL]` on stderr, exit 2).
    pub fn fail(stderr_msg: impl Into<String>) -> Self {
        Self {
            stdout: String::new(),
            stderr: msg_line("FAIL", &stderr_msg.into()),
            exit_code: 2,
        }
    }

    /// Uncaught-Python crash face (D3): empty stdout, exit 1, `[FAIL]` on
    /// stderr (best-effort — Python prints a traceback). Callers that
    /// already accumulated gated stdout (e.g. transition's WARN-before-
    /// crash) keep it by building the struct directly.
    pub fn crash(stderr_msg: impl Into<String>) -> Self {
        Self {
            stdout: String::new(),
            stderr: msg_line("FAIL", &stderr_msg.into()),
            exit_code: 1,
        }
    }
}

// Catastrophic ToolError was folded into ToolOutput (env failures are exit 2,
// not exceptional) — documented deviation from the EP sketch.

/// `[TAG] message` + trailing newline (Python output-convention shape).
pub fn msg_line(tag: &str, message: &str) -> String {
    format!("[{}] {}\n", tag, message)
}

pub mod argparse;
pub mod boundary;
pub mod boundary_build;
pub mod build;
pub mod cache;
pub mod callers;
pub mod chain_tour;
pub mod cli;
pub mod common;
pub mod delta_tour;
pub mod engine;
pub mod fndefs;
pub mod freshness;
pub mod graph_audit;
pub mod graph_db;
pub mod graph_engine;
pub mod hazard;
pub mod hub_refs;
pub mod mcp_server;
pub mod profile;
pub mod project;
pub mod py_calls;
pub mod refresh;
pub mod runtime_edges;
pub mod scip_edges;
pub mod sidecar_migrate;
pub mod snapshot;
pub mod tour_manifest;
pub mod tour_upgrade;
pub mod tour_validate;
pub mod transition;
