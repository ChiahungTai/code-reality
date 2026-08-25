# code-reality

Meta-layer tooling living *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions. Migrated
big-bang from `ai-rules` (2026-08-25; migration EP lives in ai-rules at
`ai-analysis/execution-plans/ep-code-reality-repo-mcp.md`). Current route EP
— Rust-based migration with coexistence-then-delete, superseding the
ai-rules EP's S2-S4 — lives in this repo at
`ai-analysis/execution-plans/ep-rust-migration.md`.

**Repo facts belong to each repo** — the scanned repo's `.code-reality.toml`
profile owns module/exclusion/registry knowledge; the tool layer embeds no
repo-specific special cases. Tool semantics / when-to-run truth source:
ai-rules `skills/code-reality/SKILL.md` (deployed via symlink to four
harnesses).

This repo is public-facing (open source, remote GitHub) — **all authored
content is English**: code comments, docstrings, README, AGENTS.md, commit
messages. Migrated code keeps its original docstrings (zero-change migration
constraint).

## Usage (from any repo cwd)

```
uv run --project ~/Github/code-reality python -m code_reality.<tool> --repo <repo-root> [args]
```

Sidecar home (frozen at migration): `~/.mosaic/code-reality/` — including
per-repo SCIP index slots under `scip/<repo-basename>/`
(generate → `--stamp-meta` → `--build-cache` ordering).

## Module guide

- [crates/AGENTS.md](crates/AGENTS.md) — Rust carrier (coexistence with the
  frozen Python until the R7 relay): lib layering, byte-parity contract,
  schema interop rules
- [code_reality/AGENTS.md](code_reality/AGENTS.md) — the tool package:
  foundation modules, tool families, sidecar conventions, internal layering
- [tests/AGENTS.md](tests/AGENTS.md) — unit vs integration split, fixture
  helpers, test conventions

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `python -m code_reality.scip_refs <symbol> --repo <repo>`; Rust carrier: `cargo run -p code-reality -- scip_refs ...` (byte-parity gated) | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `scip_refs --audit --repo` + `graph_audit --json`; Rust carrier: `code-reality scip_refs --audit ...` / `code-reality graph_audit ...` (byte-parity gated, `--audit` first pass in-process) | ✅ |
| Deletability safety net (hub_refs/hazard) | `hub_refs <symbol> --repo <repo> --hazard`；Rust carrier: `code-reality hub_refs ...`（byte-parity gated——AST face via ruff_python_parser, differential-verified） | ✅ |
| Boundary / export / narrative tool family | snapshot / transition / boundary family / tour family / runtime_edges / graph_csv; Rust carrier for snapshot / transition / graph_csv / graph_audit: `code-reality <sub> ...` (byte-parity gated; boundary/tour/runtime_edges stay R5) | ✅ |
| Caller-edge query (callers/closure) | Rust carrier: `code-reality scip_refs <symbol> --callers/--closure [--depth N] --repo <repo>` (`--depth` 1-10000, default 2; item-level refs = refs not enclosed by any fn — refs are not call counts) | ✅ (Rust-native; Python carrier never built — R3 superseded it) |
| Unified MCP interface | `python -m code_reality.mcp_server` | 📋 (EP R6) |

## Tests

`uv run pytest` — self-contained (synthetic fixtures + tmp-dir state
only; zero environment dependence — open-source test policy in
[tests/AGENTS.md](tests/AGENTS.md)).
