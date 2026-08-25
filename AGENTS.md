# code-reality

Meta-layer tooling living *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions. Migrated
big-bang from `ai-rules` (2026-08-25; migration EP lives in ai-rules at
`ai-analysis/execution-plans/ep-code-reality-repo-mcp.md`).

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

- [code_reality/AGENTS.md](code_reality/AGENTS.md) — the tool package:
  foundation modules, tool families, sidecar conventions, internal layering
- [tests/AGENTS.md](tests/AGENTS.md) — unit vs integration split, fixture
  helpers, test conventions

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `python -m code_reality.scip_refs <symbol> --repo <repo>` | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `scip_refs --audit --repo` + `graph_audit --json` | ✅ |
| Deletability safety net (hub_refs/hazard) | `hub_refs <symbol> --repo <repo> --hazard` | ✅ |
| Boundary / export / narrative tool family | snapshot / transition / boundary family / tour family / runtime_edges / graph_csv | ✅ |
| Caller-edge query (callers/closure) | scip_refs `--callers` / `--closure` | 📋 (EP S2) |
| Unified MCP interface | `python -m code_reality.mcp_server` | 📋 (EP S3) |

## Tests

`uv run pytest` (tests marked `integration` consume real repos and sidecar
artifacts outside this repo).
