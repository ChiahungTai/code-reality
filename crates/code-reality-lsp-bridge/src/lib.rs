//! `code-reality-lsp-bridge` — LSP↔MCP bridge for the type face
//! (hover / diagnostics / edit-recheck). One MCP server process, one
//! lazily spawned language-server backend per family (default
//! commands: `pyrefly-lsp` for .py, `rust-analyzer` for .rs,
//! `typescript-language-server --stdio` for the six JS/TS faces;
//! overridable per family). The crate itself has no language-specific
//! dependencies — the P2 clause: any LSP backend is one LangSpec +
//! BackendCommand away.

pub mod framing;
pub mod server;
pub mod session;

pub use session::LspSession;
