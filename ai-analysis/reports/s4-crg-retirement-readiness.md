# S4 Follow-up — CRG Retirement Readiness (deep-work close, 2026-08-26)

> Addendum to `s3-graph-engine-adjudication.md` (its §6 anticipated this).
> Trigger: user door ratification + deep-work directive ("完全取代 CRG").

## What is now replaced (this repo, engine layer)

All ten live-consumed CRG MCP operations have a Rust face with NT parity
evidence (`graph_query` CLI + 16-tool MCP server):

| CRG op | Rust face | NT evidence |
|---|---|---|
| impact_radius (+ union tier) | `graph_query impact_radius [--union]` | 500/500 scores exact; union +2,544 reachable |
| detect_changes + risk | `graph_query detect_changes` | synthetic exact (six factors hand-checked) |
| hub_nodes (+ union) | `graph_query hub [--union]` | top-10 exact |
| bridge_nodes (+ union) | `graph_query bridge [--union]` | synthetic exact; NT statistical (sampled, documented) |
| list/get communities | `graph_query communities [--leiden\|--union]` | Tier-0 42/42 multiset exact; Leiden tier: 1,270 communities, largest 23.4% (Tier-0 giant 41.7% dissolved), modularity 0.9158, seeded deterministic |
| architecture_overview | `graph_query arch_overview` | cross-edge counts exact (synthetic + shares Tier-0 core) |
| list/get/affected flows | `graph_query flows / affected_flows` | 10,359/10,359 multiset exact |
| semantic_search (keyword face) | `graph_query search` | FTS5→LIKE both paths; embeddings face deferred (S3 §4.5) |
| review_context / minimal_context | `graph_query review_context / minimal_context` | structural keys (EP S7 bar) |
| (LSP-aligned bonus) document_symbols | `graph_query symbols` | SCIP-cache outline; hover/type signatures remain LSP-only (not in SCIP data) |

Union tier (S5-mapper): SCIP sidecar REFERENCES edges map into engine
queries via the (file, name) double-key join — NT 179,704/181,591 edges
mapped (98.96%), 177,877 new after dedup, impact reach +2,544 nodes.
graph.db stays read-only; the edge plane lives in the sidecar (S1 (A)
adjudication intact).

## What still needs CRG (the honest remainder)

1. **graph.db PRODUCER** — tree-sitter parsing builds graph.db itself
   (multi-language). Rust corpora have SCIP truth (S1/S2), but the db
   build for non-Rust (Python/TS/...) is still `uvx code-review-graph
   build`. Replacement path = S5 P3 (ruff_python_parser indexer) — not
   built.
2. **Embeddings face** — never ran on this machine (0 rows); deferred by
   S3 door 3. Cloud-HTTP is the cheap future path; not a retirement
   blocker on current usage.
3. **CRG-internal features with zero live consumers** (visualization,
   wiki, daemon/watch, refactor dead-code, exports) — out of scope by
   the ratified parity bar; not blockers.

## Cutover checklist (consumer side — ai-rules / harness, separate session)

1. ai-rules `crg-query` skill consumers: switch the 10-op tool calls to
   `code-reality` MCP (tool names already match; args: repo_root
   required, no auto-detect).
2. `launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.user.crg-mcp.plist`
   once no consumer remains; keep the plist file for rollback.
3. graph.db ownership flip (S1 gate): with engine queries on the union
   plane, REFERENCES semantics become ours to define; revisit whether
   the remaining CRG-producer writes still warrant sidecar isolation.
4. Rollback: re-bootstrap the CRG launchd plist; the Rust faces are
   additive (no destructive step anywhere).

**Verdict**: engine-layer retirement is READY per this repo; full CRG
process retirement waits on the consumer cutover (ai-rules session) and
remains bounded by the tree-sitter producer (data layer).
