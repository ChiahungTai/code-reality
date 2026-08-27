# code-reality ZCode plugin

Bundles the code-reality MCP server (stdio — ZCode spawns
`code-reality-mcp --stdio` per session, zero daemon) and the usage
skill. Tools: `refs` / `callers` / `closure` / `audit` — each takes an
explicit `repo_root` absolute path.

## Prerequisite (the binary)

```
cargo install --path ~/Github/code-reality/crates/code-reality
```

(or from a checkout of https://github.com/ctai/code-reality — puts
`code-reality` + `code-reality-mcp` on PATH)

## Install

The repo root doubles as the plugin marketplace (see
`marketplace.json`): Settings → Plugin Management → Discover → `+` →
this repo's local path, Git URL, or GitHub URL. The plugin
appears under Personal → install. The MCP server mounts as
`plugin:code-reality:code-reality`; the skill auto-loads.

An HTTP resident mode also exists (launchd, port 8200) for
multi-harness sharing on one machine — see the repo's `launchd/`.
Not needed for the plugin path.

## Updating the plugin

Installed plugin caches do not pick up content-only changes under
`plugin/` — bump `version` in `marketplace.json` whenever the slice
changes so a marketplace refresh/reinstall is visible, and rerun
`scripts/dist-marketplace.sh` for the directory-source slice.
