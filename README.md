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
`boundary` `delta_tour` `chain_tour` `graph_csv` `tour_validate` `tour_upgrade`
`tour_manifest` `graph_audit` `scip_refs`

Sidecar home (frozen at migration): `~/.mosaic/code-reality/` — including the
per-repo SCIP index slots under `scip/<repo-basename>/`.

## Quickstart (AI harness enablement)

```
cargo install --path crates/code-reality        # code-reality + code-reality-mcp on PATH
```

**ZCode / Claude Code plugin** (stdio MCP + usage skill, zero daemon):
add this repo as a plugin marketplace (Settings → Plugins → Add
marketplace → local path or GitHub URL — the repo root carries
`marketplace.json`), install `code-reality`. The MCP server mounts as
`plugin:code-reality:code-reality` and spawns per session.

**Any MCP-capable harness** (generic): point your MCP config at

```json
{"type": "stdio", "command": "code-reality-mcp", "args": ["--stdio"]}
```

An HTTP resident mode also exists (`code-reality-mcp`, port 8200,
launchd plist in `launchd/`) for multi-harness sharing on one machine —
not needed for the plugin path.

Per-repo prerequisites for the query tools: a SCIP index
(`rust-analyzer scip <repo>` → `~/.mosaic/code-reality/scip/<basename>/index.scip`)
and, for audits, a CRG graph (`uvx code-review-graph build`).

## Tests

```
cargo test --workspace
```

Tests marked `integration` consume real repos and sidecar artifacts outside
this repo.

## References & credits

- **[code-review-graph](https://github.com/tirth8205/code-review-graph)** (MIT, Tirth Kanani) —
  the graph storage & query layer this toolchain builds on. `snapshot` /
  `transition` / `hub_refs` / `graph_audit` / `graph_csv` consume its per-repo
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
- `scip.proto` (Apache-2.0, [sourcegraph/scip](https://github.com/sourcegraph/scip)) — vendored at `code_reality/scip.proto`
- `protobuf` (BSD-3) — runtime for the vendored generated bindings
- `code-review-graph` (MIT, Copyright Tirth Kanani) — graph.db producer audited by `graph_audit`

Distribution requires retaining the upstream license notices.
