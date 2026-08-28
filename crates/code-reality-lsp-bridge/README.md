# code-reality-lsp-bridge

The type-face MCP bridge: routes by file extension — `.py` tool calls
(hover / check_file / edit_file / lsp_status) go to a lazily spawned
`pyrefly-lsp` backend, `.rs` calls to `rust-analyzer`. One backend
session per language, independent lifecycles; the bridge itself does
no type analysis, it speaks LSP to the backends.

Backends are system-level installs (`pyrefly-lsp` ships in the
pyrefly-producer distribution; `rustup component add rust-analyzer`
for the Rust face) — missing backends surface as loud tool errors on
first use.

See the [repository README](../../README.md) for the tool semantics.
