# code-reality-darwin-arm64

Native binaries for [code-reality](https://github.com/ChiahungTai/code-reality)
on macOS arm64 — the five Rust bins (`code-reality`, `code-reality-mcp`,
`pyrefly-index`, `pyrefly-lsp`, `code-reality-lsp-bridge`), each embedding
its build rev for the freshness face.

**Not for direct installation.** This package exists as the
`optionalDependencies` target of the code-reality Claude Code plugin: on
install, Claude Code runs `npm ci`, npm resolves the platform package
matching the machine, and the plugin's `.mcp.json` spawns the servers from
`node_modules/.bin`. npm selects or skips this package via its `os`/`cpu`
fields — on a mismatched platform the install succeeds with the package
skipped and the plugin falls back to its PATH/uv resolution chain.

## Install faces

- **Claude Code users**: install the plugin from the marketplace — the
  binaries arrive via this package automatically (zero uv).
- **Everyone else / CLI-heavy workflows**: the main distribution face is
  PyPI wheels — `uv tool install code-reality` (plus
  `pyrefly-producer`, `code-reality-lsp-bridge`).

Both faces are macOS arm64 only.

## Provenance

The binaries are extracted verbatim from the same-tag PyPI wheels
(`<pkg>-<ver>.data/scripts/*`) by the release workflow — identical bytes,
identical embedded revs, no separate build.
