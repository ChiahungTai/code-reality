# code-reality ZCode plugin

Bundles two stdio MCP servers (ZCode spawns each per session, zero
daemon) and the usage skill:

- `code-reality` — the structural face: `refs` / `callers` / `closure`
  / `audit` + the graph_query family; every tool takes an explicit
  `repo_root` absolute path.
- `code-reality-lsp-bridge` — the Python type face: `hover` /
  `check_file` / `edit_file` / `lsp_status`. It spawns the
  `pyrefly-lsp` language server lazily on the first tool call.

## Prerequisites (the binaries)

```
cargo install --path ~/Github/code-reality/crates/code-reality              # code-reality + code-reality-mcp
cargo install --path ~/Github/code-reality/crates/pyrefly-producer          # pyrefly-index + pyrefly-lsp (bridge backend)
cargo install --path ~/Github/code-reality/crates/code-reality-lsp-bridge   # code-reality-lsp-bridge
```

(or from a checkout of https://github.com/ctai/code-reality — all three
put their bins on PATH; the type-face server needs `pyrefly-lsp` from
the second command, missing binaries surface as loud tool errors with
install guidance)

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
