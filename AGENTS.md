# code-reality

Meta-layer tooling living *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions. Rust carrier
end state (R7, 2026-08-26): the frozen-Python parity oracle and both Python
copies retired after byte-identical acceptance on real corpora (NT
query/graph_audit `--json`/`--audit`, mosaic `hub_refs --json` — gate
record in `ai-analysis/execution-plans/_done/`). Migration history:
`ai-analysis/execution-plans/ep-rust-migration.md` + per-segment child EPs
in `_done/`.

**Repo facts belong to each repo** — the scanned repo's `.code-reality.toml`
profile owns module/exclusion/registry knowledge; the tool layer embeds no
repo-specific special cases. Tool semantics / when-to-run truth source:
ai-rules `skills/code-reality/SKILL.md` (deployed via symlink to four
harnesses).

This repo is public-facing (open source, remote GitHub) — **all authored
content is English**: code comments, docstrings, README, AGENTS.md, commit
messages. (Chinese OUTPUT strings are the frozen CLI byte-parity face,
preserved verbatim.)

## Usage (from any repo cwd)

```
code-reality <tool> --repo <repo-root> [args]
```

Installed via `cargo install --path ~/Github/code-reality/crates/code-reality`
(→ `~/.cargo/bin/code-reality` + `code-reality-mcp`). Sidecar home:
`~/.mosaic/code-reality/` — per-repo SCIP index slots under `scip/<repo-basename>/`
(generate → `--stamp-meta` → `--build-cache` ordering).

## Module guide

- [crates/AGENTS.md](crates/AGENTS.md) — the Rust carrier: lib layering
  (engine/callers/cache/fndefs/common/profile/argparse + graph/tour/boundary/
  hazard families + mcp_server), exit-semantics table, parity history
- Tool semantics: ai-rules `skills/code-reality/SKILL.md` (the cross-repo
  truth source — this repo no longer duplicates it)

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `code-reality scip_refs <symbol> --repo <repo>` | ✅ |
| Caller-edge query (callers/closure) | `code-reality scip_refs <symbol> --callers/--closure [--depth N] --repo <repo>` | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `code-reality scip_refs --audit --repo` + `code-reality graph_audit --json` | ✅ |
| Deletability safety net (hub_refs/hazard) | `code-reality hub_refs <symbol> --repo <repo> --hazard` | ✅ |
| Boundary / export / narrative tool family | `code-reality <snapshot\|transition\|graph_csv\|boundary\|boundary_build\|chain_tour\|delta_tour\|tour_manifest\|tour_validate\|tour_upgrade\|runtime_edges> ...` | ✅ |
| Unified MCP interface | stdio `code-reality-mcp --stdio` (default face: ZCode/Claude plugin in `plugin/`; repo-root `marketplace.json` = installable market) + streamable-http `127.0.0.1:8200/mcp` (launchd plist in `launchd/`, multi-harness sharing) | ✅ |
| SCIP reference-edge export + union sidecar injection | `code-reality scip_edges --repo <repo> [--inject [--dry-run\|--json]]` (edge plane lands in the index-sibling `index.union.db`, never CRG graph.db) | ✅ |
| graph_audit missing → graph.db node injection | `code-reality scip_nodes --repo <repo> [--dry-run] [--rollback] [--json]` (the sole graph.db write face; `extra {"tier":"SCIP"}` marker rollback + `VACUUM INTO` backup) | ✅ |

## Tests

`cargo test`（Rust suites are the sole test face post-R7 — the Python
parity harness retired with the oracle; history in the archived EPs）.
