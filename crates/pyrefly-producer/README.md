# pyrefly-producer

The Rust-native Python occurrence producer for code-reality: a linked
[Pyrefly](https://github.com/facebook/pyrefly) engine (pinned git rev)
emitting repo-keyed SCIP indexes, plus `pyrefly-lsp` — the same engine
hosted as a stdio language server (the Python backend of the
code-reality-lsp-bridge type face).

Ships two binaries: `pyrefly-index` (index producer → sidecar SCIP
slot; invalidates superseded sidecar artifacts on write) and
`pyrefly-lsp` (LSP host). Byte-deterministic output; the scip-python
fork remains only as a retained fallback.

See the [repository README](../../README.md) for the refresh chain
(generate → `--stamp-meta` → `--build-cache`).
