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

Two acquisition faces, one binary layer (PyPI). The MCP spawn wrapper
self-heals the uv face: on every session start it version-checks the
pinned bin (`--version` prints `<ver>+<rev>`, prefix compare) and,
when missing or stale, installs the exact plugin-pinned versions with
`uv tool install --force` — first-session bootstrap, network needed
once (an offline first session fails loud and retries the next one).

1. **PyPI wheels via uv (main face)** — consumer path, no Rust
   toolchain, identical on Claude Code and ZCode. Manual install (or
   skip it entirely on machines with uv — the plugin's wrapper does it
   on first session):

   ```
   uv tool install code-reality              # code-reality + code-reality-mcp
   uv tool install code-reality-lsp-bridge   # code-reality-lsp-bridge
   uv tool install pyrefly-producer          # pyrefly-index + pyrefly-lsp (Python backend)
   rustup component add rust-analyzer        # Rust backend — system dependency, ships in no wheel
   ```

   One-shot use without installing anything: `uvx code-reality <tool>
   ...`; the producer dist needs `uvx --from pyrefly-producer <bin>`.
2. **cargo (developer face)** — build from a checkout of
   https://github.com/ChiahungTai/code-reality (puts bins on PATH; set
   `CODE_REALITY_BOOTSTRAP=off` so the wrapper does not force-install
   the pinned release over your HEAD builds):

   ```
   cargo install --path ~/Github/code-reality/crates/code-reality
   cargo install --path ~/Github/code-reality/crates/pyrefly-producer
   cargo install --path ~/Github/code-reality/crates/code-reality-lsp-bridge
   ```

No uv on the machine: the wrapper fails loud with install guidance
(`curl -LsSf https://astral.sh/uv/install.sh | sh`); installs that
still carry the retired npm-embedded `node_modules` keep working as a
deprecation grace until uv arrives. The npm package
(`code-reality-darwin-arm64`, the embedded face retired 2026-08-29) is
frozen at 0.3.1 and unmaintained — use the uv face.

If a server failed to start right after install and you have since
fixed the environment (e.g. installed uv), restart the harness or
retry the session — failed server spawns are backed off, and a new
session resets that.

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
