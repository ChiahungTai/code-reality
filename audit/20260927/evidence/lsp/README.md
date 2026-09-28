# LSP lifecycle probes

Baseline: `eaa5d2462ea7821021ad0ea87403484ac0e47538`. Run all commands from the repository root. Only the probe's own child PIDs are killed. The fake backend uses the real library framing and session implementation; the MCP confirmation uses the real bridge binary.

The initial workspace `cargo test --workspace --locked` built the libraries and binaries. The preserved probe can be run directly. Recompile against the exact artifacts from that build with:

```sh
rustc --edition=2021 audit/20260927/evidence/lsp/probe.rs \
  -L dependency=target/debug/deps \
  --extern code_reality_lsp_bridge=target/debug/deps/libcode_reality_lsp_bridge-5776dae1339d352e.rlib \
  --extern serde_json=target/debug/deps/libserde_json-dfd3d69bd5424347.rlib \
  -o audit/20260927/evidence/lsp/probe-rebuilt
```

This command was executed successfully, exit 0 (`probe-build.log`, `probe-build.exit`). Cargo artifact suffixes are build-specific; retain the matching library pair or rebuild the workspace before adapting them. No production Rust file was modified.

```sh
audit/20260927/evidence/lsp/probe dead-cache
uv run --no-project python audit/20260927/evidence/lsp/demo_mcp.py
audit/20260927/evidence/lsp/probe blocked-write
```

Each exits 0 when it observes the reported defect. `dead-cache-confirm.exit`, `mcp-confirm.exit` and `blocked-write-confirm.exit` preserve the rerun exit statuses. The blocked-write case deliberately waits about 35 seconds, then kills/reaps its own child and joins both waiting threads. An external watchdog may be used if adapting the probe.

`demo_mcp.py` invokes `backend.sh`, which references this worktree's absolute evidence path. Adapt that wrapper when moving the assets. `cache-fixture.py` is created by the direct `dead-cache` command and must exist before the MCP command.

`retry.log` and `retry-confirm.log` record rejected initialization followed by permanent dead-session behavior. The final collected source review identified the explicit next-call retry promise at `session.rs:475`; the main report therefore includes R0-09 as a contract-narrowing recommendation. Ordinary backend death still explicitly requires bridge restart; no general automatic-restart requirement is inferred.
