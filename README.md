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

Sidecar home: `<repo>/.code-reality/` — SCIP index slots under `scip/`
(alongside `graph.db`). The data dir self-ignores via its own single-`*`
`.gitignore` (zero consumer gitignore setup). Legacy `~/.mosaic/
code-reality/` slots migrate with `code-reality sidecar_migrate --repo
<repo>`.

## Quickstart (AI harness enablement)

Consumer install — prebuilt PyPI wheels, no Rust toolchain needed
(macOS arm64 is the published platform; `pipx` / `pip install --user`
work the same, the wheels are pure binaries with no Python ABI
dependency):

```
uv tool install code-reality              # code-reality + code-reality-mcp (structural face)
uv tool install pyrefly-producer          # pyrefly-index + pyrefly-lsp (Python producer + language server)
uv tool install code-reality-lsp-bridge   # code-reality-lsp-bridge (type face: hover/diagnostics MCP, .py + .rs)
rustup component add rust-analyzer        # Rust type-face backend — system dependency, ships in no wheel (skip if you only use the .py face)
```

One-shot use without installing: `uvx code-reality <tool> --repo
<repo-root> [args]` (works where the dist name equals the bin name —
`code-reality`, `code-reality-lsp-bridge`). The producer dist has no
same-named bin, so use `uvx --from pyrefly-producer pyrefly-index
--repo <repo-root>`.

Developer face (builds from a checkout, tracks HEAD):

```
cargo install --path crates/code-reality
cargo install --path crates/pyrefly-producer
cargo install --path crates/code-reality-lsp-bridge
```

Version axes: the PyPI dists follow the workspace version (binary
contract, released on `v*` tags); the plugin manifest and marketplace
entries carry their own 0.1.x version (wiring axis — bumps when
`plugin/` content changes). The two move independently by design; any
binary self-identifies via `--version` (`<pkg>+<git rev>`) regardless
of channel. With both faces installed the CLI name can shadow —
`which code-reality` (or the absolute path) tells them apart.

Every bin answers `--version` with `<pkg>+<git rev>` and warns on
stderr when a local CR checkout has moved past the installed build.
Maintainers can auto-reinstall on commit: `git config core.hooksPath
.githooks` (the shipped post-commit hook assumes the `~/Github/
code-reality` layout and logs under its `.agent-tmp/` — adapt before
opting in on another machine layout).

**ZCode / Claude Code plugin** (stdio MCP + usage skill, zero daemon):
the plugin manifest sits at the Claude Code location
(`plugin/.claude-plugin/plugin.json`); ZCode reads it via its
CC-compatibility fallback, so one manifest serves both harnesses. The
repo root carries both market files — `marketplace.json` (ZCode) and
`.claude-plugin/marketplace.json` (Claude Code). Add this repo as a
plugin marketplace (ZCode: Settings → Plugin Management → Discover →
`+` → Git URL or GitHub URL; Claude Code: `/plugin marketplace add
ChiahungTai/code-reality`), install `code-reality`. Two MCP servers mount per
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

GUI-launched harnesses may lack the install dirs on PATH — give the
absolute path to `code-reality-mcp` there, or reuse the `/bin/sh -c`
wrapper from `plugin/.mcp.json` (resolves via PATH first, then falls
back to `~/.local/bin` and `~/.cargo/bin`).

An HTTP resident mode also exists (`code-reality-mcp`, port 8200,
launchd plist in `launchd/`) for multi-harness sharing on one machine —
not needed for the plugin path.

Per-repo prerequisites for the query tools: a SCIP index
(`rust-analyzer scip <repo>` → `<repo>/.code-reality/scip/index.scip`;
Python repos use the Rust-native pyrefly producer —
`cargo run --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo>`
emits the same slot, then `--stamp-meta`/`--build-cache` as usual; the
Node-based scip-python fork is the retained fallback, not the default
face. All graph-reading
tools (engine, audit, chain_tour, hub_refs/hazard, snapshot) read a
self-owned db at `<repo>/.code-reality/graph.db` — produce it with
`code-reality graph_db build --repo <repo>` (any producer cache); the
refresh chain is purely producer-side (the CRG-era `.code-review-graph/`
import face was fully removed with the W5 legacy-db cleanup).
`graph_db ensure_indexes --repo <repo>` is an idempotent follow-up that
adds the engine read-chain indexes to dbs built before that schema
revision. The `.code-reality/` directory is repo-local derived data
that self-ignores via its own single-`*` `.gitignore` — zero gitignore
setup on the consumer side (a root-level ignore entry also works if a
repo prefers one).

## Tests

```
cargo test --workspace
```

The Rust suites are self-sufficient — fixtures live under
`crates/code-reality/tests/fixtures/`; no external repos or sidecar
artifacts are required to run them.

## References & credits

- **[code-review-graph](https://github.com/tirth8205/code-review-graph)** (MIT, Tirth Kanani) —
  the graph storage & query layer this toolchain originally built on
  (no runtime dependency since the v1+ self-owned-db flip). Its
  SQLite-native design — one graph.db per repo, `qualified_name UNIQUE`
  node collapse — is the storage reference this project's graph.db
  converges on.
- **[scip-callgraph](https://github.com/Beneficial-AI-Foundation/scip-callgraph)** (MIT OR Apache-2.0, Verus team) —
  an independent productization of the DEF-enclosure caller-edge mechanism
  that `scip_refs --callers` (v0 S2) implements; used as a cross-check
  reference, not a dependency.
- **rust-analyzer** — produces the SCIP semantic indexes that `scip_refs`
  consumes (the only compiler-grade precision source for Rust).

## License & notices

This project is licensed under the **MIT License** (see [LICENSE](LICENSE)).

Dependency chain (verified 2026-08-28, no copyleft):

- `rust-analyzer` (MIT/Apache-2.0) — produces the SCIP indexes consumed by `scip_refs`
- `scip.proto` (Apache-2.0, [sourcegraph/scip](https://github.com/sourcegraph/scip)) — vendored at `crates/code-reality/schema/scip.proto`
- `protobuf` (BSD-3) — runtime for the vendored generated bindings
- `pyrefly` (MIT, [facebook/pyrefly](https://github.com/facebook/pyrefly)) — the type-checker engine statically linked into the `pyrefly-producer` bins (pinned git rev)

Distribution requires retaining the upstream license notices.
