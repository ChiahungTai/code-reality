# code-reality ZCode plugin

Bundles two stdio MCP servers (ZCode spawns each per session, zero
daemon) and the usage skill:

- `code-reality` — the structural face: `refs` / `callers` / `closure`
  / `audit` + the graph_query family; every tool takes an explicit
  `repo_root` absolute path.
- `code-reality-lsp-bridge` — the type face, routed by file extension:
  `.py` → pyrefly (`hover` / `check_file` / `edit_file` / `lsp_status`),
  `.rs` → rust-analyzer (same tools). Each backend spawns lazily and
  independently.

## Prerequisites (the binaries)

Three acquisition faces; the MCP servers resolve them through a
candidate chain (PATH → the plugin's `node_modules/.bin` →
`~/.local/bin` → `~/.cargo/bin` → fail-loud guidance), so any face
works and better-installed faces win:

1. **Claude Code plugin (embedded face, zero uv)** — the plugin ships a
   `package.json` whose `optionalDependencies` pull
   `code-reality-darwin-arm64` from npm. On install Claude Code runs
   `npm ci` and the five binaries land in the plugin's
   `node_modules/.bin` — no uv, no Rust toolchain. **macOS arm64 with
   an arm64 npm only** (on an x64 npm — e.g. Rosetta Homebrew node —
   npm skips the package and the chain falls through to the faces
   below). Both distribution faces (npm and PyPI wheels) are macOS
   arm64 only.
2. **PyPI wheels (main face)** — consumer path, no Rust toolchain,
   works everywhere the plugin runs (required on ZCode, which has no
   npm-install mechanism):

   ```
   uv tool install code-reality              # code-reality + code-reality-mcp
   uv tool install pyrefly-producer          # pyrefly-index + pyrefly-lsp (Python backend)
   uv tool install code-reality-lsp-bridge   # code-reality-lsp-bridge
   rustup component add rust-analyzer        # Rust backend — system dependency, ships in no wheel
   ```

   The embedded face covers the MCP tool surface; CLI-heavy workflows
   (`graph_db build`, `snapshot`, `pyrefly-index` production) stay on
   this face.
3. **cargo (developer face)** — build from a checkout of
   https://github.com/ChiahungTai/code-reality (puts bins on PATH;
   missing backends surface as loud tool errors with install guidance):

   ```
   cargo install --path ~/Github/code-reality/crates/code-reality
   cargo install --path ~/Github/code-reality/crates/pyrefly-producer
   cargo install --path ~/Github/code-reality/crates/code-reality-lsp-bridge
   ```

If a server failed to start right after install and you have since
fixed the environment (e.g. installed the uv face), reinstall the
plugin — Claude Code backs off retrying servers that failed to spawn,
and a reinstall resets that.

Freshness: `--version` on any bin prints `<pkg>+<git rev>`; when a CR
checkout is present on the machine, invocations warn once on stderr if
the installed binary lags it (stale HEAD or uncommitted `crates/`
edits).

## Install

The plugin manifest lives at `plugin/.claude-plugin/plugin.json` — the
Claude Code location. ZCode reads it through its CC-compatibility
fallback (lookup order `.zcode-plugin/` → `.claude-plugin/`), so one
manifest serves both harnesses; there is deliberately no
`.zcode-plugin/` copy to drift against it.

Two market files point at the same `./plugin` slice: the repo-root
`marketplace.json` (ZCode format) and `.claude-plugin/marketplace.json`
(Claude Code format, adds the required `owner` field). Register either
market (ZCode: Settings → Plugin Management → Discover → `+` → this
repo's local path, Git URL, or GitHub URL; Claude Code: `/plugin
marketplace add ChiahungTai/code-reality`). The plugin appears under
Personal → install. Both MCP servers mount under
`plugin:code-reality:*` and the skill auto-loads.

An HTTP resident mode also exists (launchd, port 8200) for
multi-harness sharing on one machine — see the repo's `launchd/`.
Not needed for the plugin path.

## Updating the plugin

Installed plugin caches do not pick up content-only changes under
`plugin/` — bump `version` in all three places whenever the slice
changes (the plugin manifest, the ZCode `marketplace.json` entry, the
CC `.claude-plugin/marketplace.json` entry) so a marketplace
refresh/reinstall is visible, and rerun
`scripts/dist-marketplace.sh` for the directory-source slice. Version
comparison reads the marketplace entry as "latest" and the plugin
manifest as "installed" — a stale entry silently suppresses the update
prompt.
