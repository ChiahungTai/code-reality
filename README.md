# code-reality

Meta-layer tooling that lives *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions.

Migrated from `ai-rules` (2026-08-25). Repo-specific knowledge stays in each
consumed repo's `.code-reality.toml` profile — the tool layer embeds no
repo-specific special cases.

## Usage (from any repo cwd)

```
code-reality <tool> --repo <repo-root> [args]
```

Tools: `snapshot` `transition` `hub_refs` `runtime_edges` `boundary_build`
`boundary` `delta_tour` `chain_tour` `tour_validate` `tour_upgrade`
`tour_manifest` `graph_audit` `scip_refs` `graph_query` `graph_db`

Sidecar home (frozen at migration): `~/.mosaic/code-reality/` — including the
per-repo SCIP index slots under `scip/<repo-basename>/`.

## Quickstart (AI harness enablement)

```
cargo install --path crates/code-reality              # code-reality + code-reality-mcp (structural face)
cargo install --path crates/pyrefly-producer          # pyrefly-index + pyrefly-lsp (Python producer + language server)
cargo install --path crates/code-reality-lsp-bridge   # code-reality-lsp-bridge (type face: hover/diagnostics MCP, .py + .rs)
rustup component add rust-analyzer                    # the Rust type-face backend (spawns with no flags)
```

Every bin answers `--version` with `<pkg>+<git rev>` and warns on
stderr when a local CR checkout has moved past the installed build.
Maintainers can auto-reinstall on commit: `git config core.hooksPath
.githooks` (the shipped post-commit hook uses `~/Github/code-reality`
and `~/.mosaic` paths — adapt before opting in on another layout).

**ZCode / Claude Code plugin** (stdio MCP + usage skill, zero daemon):
add this repo as a plugin marketplace (Settings → Plugin
Management → Discover → `+` → Git URL or GitHub URL —
the repo root carries
`marketplace.json`), install `code-reality`. Two MCP servers mount per
session: `plugin:code-reality:code-reality` (structural face) and
`plugin:code-reality:code-reality-lsp-bridge` (Python type face —
hover / check_file / edit_file / lsp_status tools; spawns the
`pyrefly-lsp` backend lazily on the first tool call). Already-installed
plugin caches are version-keyed: a content-only change under `plugin/`
stays inert until `marketplace.json`/`plugin.json` are version-bumped
and the plugin is refreshed — fresh installs get both servers directly.
For local-path marketplaces, point at a clean slice
(`scripts/dist-marketplace.sh` → `dist/marketplace`), never the repo
root — directory sources mirror the whole tree, and a built `target/`
pollutes the plugin cache with gigabytes of build artifacts.

**Any MCP-capable harness** (generic): point your MCP config at

```json
{"type": "stdio", "command": "code-reality-mcp", "args": ["--stdio"]}
```

and, for the Python type face,

```json
{"type": "stdio", "command": "code-reality-lsp-bridge", "args": ["--stdio"]}
```

GUI-launched harnesses may lack `~/.cargo/bin` on PATH — give the
absolute path to `code-reality-mcp` there (or reuse the `/bin/sh -c`
wrapper from `plugin/.mcp.json`).

An HTTP resident mode also exists (`code-reality-mcp`, port 8200,
launchd plist in `launchd/`) for multi-harness sharing on one machine —
not needed for the plugin path.

Per-repo prerequisites for the query tools: a SCIP index
(`rust-analyzer scip <repo>` → `~/.mosaic/code-reality/scip/<basename>/index.scip`;
Python repos use the Rust-native pyrefly producer —
`cargo run --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo>`
emits the same slot, then `--stamp-meta`/`--build-cache` as usual; the
Node-based scip-python fork is the retained fallback, not the default
face. All graph-reading
tools (engine, audit, chain_tour, hub_refs/hazard, snapshot) read a
self-owned db at `<repo>/.code-reality/graph.db` — produce it with
`code-reality graph_db build --repo <repo>` (any producer cache); the
refresh chain is purely producer-side since the W3 import_legacy
retirement. `code-reality graph_db import_legacy --repo <repo>` remains
functional but retired (WARN on every invocation) — recovery/migration
use only, until W4 removes the CRG-era `.code-review-graph/` dbs.
`graph_db ensure_indexes --repo <repo>` is an idempotent follow-up that
adds the engine read-chain indexes to dbs built before that schema
revision. The `.code-reality/` directory is repo-local data — add it to
the repo's `.gitignore` if you don't want it tracked (that choice
belongs to each repo).

## Tests

```
cargo test --workspace
```

The Rust suites are self-sufficient — fixtures live under
`crates/code-reality/tests/fixtures/`; no external repos or sidecar
artifacts are required to run them.

## References & credits

- **[code-review-graph](https://github.com/tirth8205/code-review-graph)** (MIT, Tirth Kanani) —
  the graph storage & query layer this toolchain builds on. `snapshot` /
  `transition` / `hub_refs` / `graph_audit` consume its per-repo
  SQLite `graph.db` (nodes / edges / communities / flows / FTS5). Its
  SQLite-native design — one graph.db per repo, `qualified_name UNIQUE` node
  collapse — is the storage reference this project's derived sqlite cache
  converges on; the v1 roadmap evaluates adopting it as the internal graph
  engine (SCIP edge injection into graph.db).
- **[scip-callgraph](https://github.com/Beneficial-AI-Foundation/scip-callgraph)** (MIT OR Apache-2.0, Verus team) —
  an independent productization of the DEF-enclosure caller-edge mechanism
  that `scip_refs --callers` (v0 S2) implements; used as a cross-check
  reference, not a dependency.
- **rust-analyzer** — produces the SCIP semantic indexes that `scip_refs`
  consumes (the only compiler-grade precision source for Rust).

## License & notices

This project is licensed under the **MIT License** (see [LICENSE](LICENSE)).

Dependency chain (verified 2026-08-25, no copyleft):

- `rust-analyzer` (MIT/Apache-2.0) — produces the SCIP indexes consumed by `scip_refs`
- `scip.proto` (Apache-2.0, [sourcegraph/scip](https://github.com/sourcegraph/scip)) — vendored at `crates/code-reality/schema/scip.proto`
- `protobuf` (BSD-3) — runtime for the vendored generated bindings
- `code-review-graph` (MIT, Copyright Tirth Kanani) — graph.db producer audited by `graph_audit`

Distribution requires retaining the upstream license notices.
