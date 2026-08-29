# Incident record: NT cold-start first-build edge inflation → construct immunity

2026-08-29, relay-driven (mosaic/NT cold-start verification sessions + CR probes).

## Observation
NT `build` (cold-clear, first run): 66,820 nodes / **466,459** edges; reruns: 66,820 / **234,407**.
Decomposition (exact): 466,459 = 2 × 232,052 (rust) + 2,355 (python) — a duplicated rust edge set; nodes constant.

## Three-way probe (all on-machine, same day)
1. Producers byte-deterministic: `pyrefly-index` p1==p2 (1,622,541 B); `rust-analyzer scip` r1==r2 (280,867,553 B).
2. r1 alone builds a single-copy graph: 63,224 nodes / 232,052 edges.
3. Cold-clear umbrella replay the same evening: 66,820 / 234,407 (single copy) — the transient did not reproduce.

Verdict: unreproducible one-off (state gone; producers exonerated).

## Fix — construct immunity (not root-cause)
`graph_db.rs` edge materialization dedupes on the natural key (kind, caller, callee, file, line)
before INSERT. Whatever duplicates any producer emits, now or later, first build converges.
Regression: `tests/build.rs::t12_edge_dedupe_doubled_index_converges` (doubled index via
protobuf same-type cat-merge = the exact observed shape); CALLS face locked by the existing
`pyrefly-producer` end_to_end exact-count assertion (green post-dedupe).

## Counting-semantics change (L4-verified)
NT canonical edges 234,407 → **232,273** (−2,134): normal producer output legitimately
contains ~0.9% same-key duplicate occurrences (macro-expanded same-site doubles etc.);
per-site counting no longer inflates them (hub degrees stop double-counting).
