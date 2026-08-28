//! `code-reality-lsp-bridge` — LSP↔MCP bridge for the type face
//! (hover / diagnostics / edit-recheck). One MCP server process, one
//! spawned language-server backend (default `pyrefly-lsp`; override
//! with `--lsp-command`). The crate itself has no language-specific
//! dependencies — the P2 clause is that the Rust type face reuses this
//! crate with a rust-analyzer backend command; until then the tool
//! face (languageId, .py gate) is Python-specific.

pub mod framing;
pub mod server;
pub mod session;

pub use session::LspSession;
