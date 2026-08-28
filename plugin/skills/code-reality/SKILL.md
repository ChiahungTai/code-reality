---
name: code-reality
description: "Running code-reality tools (symbol truth queries, caller edges, closures, completeness audits, hub/hazard checks) or authoring .code-reality.toml profiles. Use when you need refs/defs for a symbol, who calls it, whether a graph.db is complete, or whether a symbol is safe to delete. Tool availability: repo root has .code-reality.toml, or code-reality --help exits 0."
when_to_use: "Symbol lookup beyond grep (trait disambiguation), caller-edge queries, delete-safety checks, CRG graph completeness audits."
license: MIT
---

# code-reality

Structural facts and governance audits for AI coding sessions. Every
MCP tool takes an explicit `repo_root` (absolute path) — repo is a
parameter, not topology.

## MCP tools (this plugin)

| Tool | What it answers |
|---|---|
| `refs(symbol, repo_root)` | Where is this symbol defined/referenced (SCIP index; trait disambiguation) |
| `callers(symbol, repo_root)` | Who calls it (sites included; item-level refs noted) |
| `closure(symbol, repo_root, depth?)` | Transitive callers (BFS; default depth 2) |
| `audit(repo_root)` | graph.db completeness gaps × SCIP refs (two-pass) |

Responses embed `[SRC]` provenance lines (index version/commit) and a
`[STDERR]` section for management output.

## Prerequisites (per repo)

- `refs`/`callers`/`closure`/`audit` need a SCIP index:
  `rust-analyzer scip <repo>` output saved under
  `~/.mosaic/code-reality/scip/<repo-basename>/index.scip`
- Python repos generate the same slot with the Rust-native pyrefly
  producer: `cargo run --release -p pyrefly-producer --bin
  pyrefly-index -- --repo <repo>` (no Node.js, no venv — bundled
  typeshed), then `code-reality scip_refs --repo <repo> --stamp-meta`
  and `--build-cache` as usual; the Node scip-python fork is the
  retained fallback, not the default face
- `audit` reads the self-owned db at
  `<repo>/.code-reality/graph.db` — produce it with
  `code-reality graph_db build --repo <repo>` (edges split CALLS vs
  REFERENCES by build-side call detection). The refresh chain is purely
  producer-side; `graph_db import_legacy` is retired (W3) — functional
  but WARN-flagged, recovery/migration use only
- Optional `.code-reality.toml` at repo root declares module rules,
  exclusions, claims prefixes, scan roots — repo facts belong to the repo.

## CLI surface (broader)

The MCP face covers the SCIP family. The same binary carries the full
toolchain: `code-reality <scip_refs|snapshot|transition|graph_audit|
hub_refs|boundary|boundary_build|chain_tour|delta_tour|
tour_manifest|tour_validate|tour_upgrade|runtime_edges|
graph_query|graph_db> --repo <root>`. 

Install/upgrade: `cargo install --path <this-repo>/crates/code-reality`.
Full docs: the repo README.
