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

```
cargo install --path ~/Github/code-reality/crates/code-reality              # code-reality + code-reality-mcp
cargo install --path ~/Github/code-reality/crates/pyrefly-producer          # pyrefly-index + pyrefly-lsp (Python backend)
cargo install --path ~/Github/code-reality/crates/code-reality-lsp-bridge   # code-reality-lsp-bridge
rustup component add rust-analyzer                                           # Rust backend
```

(or from a checkout of https://github.com/ctai/code-reality — the
first three put their bins on PATH; missing backends surface as loud
tool errors with install guidance)

Freshness: `--version` on any bin prints `<pkg>+<git rev>`; when a CR
checkout is present on the machine, invocations warn once on stderr if
the installed binary lags it (stale HEAD or uncommitted `crates/`
edits).

## Install

The repo root doubles as the plugin marketplace (see
`marketplace.json`): Settings → Plugin Management → Discover → `+` →
this repo's local path, Git URL, or GitHub URL. The plugin
appears under Personal → install. Both MCP servers mount under
`plugin:code-reality:*` and the skill auto-loads. The
type-face entry ships with this plugin's next version bump —
already-installed caches are version-keyed (see Updating below);
fresh installs get both servers.

An HTTP resident mode also exists (launchd, port 8200) for
multi-harness sharing on one machine — see the repo's `launchd/`.
Not needed for the plugin path.

## Updating the plugin

Installed plugin caches do not pick up content-only changes under
`plugin/` — bump `version` in `marketplace.json` whenever the slice
changes so a marketplace refresh/reinstall is visible, and rerun
`scripts/dist-marketplace.sh` for the directory-source slice.
