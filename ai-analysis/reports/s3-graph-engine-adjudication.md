# S3 Graph-Engine Adjudication — Rust self-build vs keep CRG Python

> EP: `ai-analysis/execution-plans/ep-v1plus-graph-engine.md` (segment S3, research — no
> code changed). Baseline `14e7ce3`. Corpus: nautilus_trader (NT). Date: 2026-08-26.
> This is the formal answer sheet for "can the CRG engine layer retire" (S4 gate).
> Adjudication is delegated (user said they cannot judge directly); every door in §5
> carries a recommendation the user may override.

## 1. The finding that reframes the whole question

Two of the three "hard to migrate" CRG faces **never actually run on this machine**.
The migration question is not "port Leiden + port embeddings" — it is "adopt features
CRG only ever shipped as uninstalled optional extras".

Evidence (all probed 2026-08-26):

- The CRG MCP server is launched by launchd as `uvx code-review-graph@2.3.8` with no
  extras (`~/Library/LaunchAgents/com.user.crg-mcp.plist`). Package metadata declares
  `igraph` under extra `communities`/`all` and `sentence-transformers` under extra
  `embeddings`/`all` — a base install has neither. Import probe in the installed tool
  env (2.3.7, same extras layout): `IGRAPH_AVAILABLE = False`.
- NT `graph.db` communities table: 42 rows, **every** description is
  `"Directory-based community: ..."` — the fingerprint of
  `_detect_file_based` (communities.py:474), the fallback that runs when igraph is
  absent. The Leiden path would emit `"Community of N nodes"` (communities.py:460).
  Largest community: 33,584 nodes (`crates/adapters`) — and the oversized-community
  splitter that would break it up is itself igraph-gated and no-ops
  (communities.py:577). So live NT communities are directory grouping with an
  unresolved mega-community.
- NT `graph.db` embeddings table: **0 rows**. The local embedding provider requires
  sentence-transformers (embeddings.py:749 `_check_available`); without it
  `get_provider` returns None and semantic search serves the FTS5/BM25 keyword
  fallback (search.py). On this machine, "semantic search" has only ever been
  keyword search.

## 2. What the ten live-consumed CRG MCP operations actually are

(Consumer inventory per ai-rules cutover audit; implementations read at
`~/.local/share/uv/tools/code-review-graph/lib/python3.12/site-packages/code_review_graph/`.)

| Operation | CRG implementation | External dep | Live behavior on NT |
|---|---|---|---|
| detect_changes | git diff subprocess → line ranges → node-span mapping → risk score (changes.py:381 `analyze_changes`) | git CLI | works |
| impact_radius | bounded best-score relaxation in SQLite temp tables (graph.py:771); weights CALLS 1.0 … REFERENCES 0.6, decay 0.6/depth, floor 0.05, default depth ≤2, cap 500 nodes (constants.py:42-73) | none | works |
| hub_nodes | degree ranking over edges | none | works |
| bridge_nodes | networkx betweenness centrality, k-sampled above 5k nodes (analysis.py:75-82) | networkx (base dep) | works |
| list/get communities, architecture_overview, minimal_context | igraph Leiden when available, **else directory grouping** (communities.py:798); overview = communities + cross-edge counts (communities.py:1020) | igraph (extra, **absent**) | **directory fallback**: 42 communities, one 33.5k-node giant |
| list/get flows, affected_flows | entry-point heuristics (no-incoming-CALLS ∪ decorator ∪ name pattern, flows.py:164) → forward BFS ≤15 over CALLS adjacency (flows.py:222) → criticality weights 0.30/0.20/0.25/0.15/0.10 (flows.py:324) | none | works; 10,359 flows stored |
| semantic_search | local sentence-transformers / cloud HTTP / **else FTS5 keyword fallback** (embeddings.py:649, search.py:181) | sentence-transformers (extra, **absent**) | **keyword-only** (embeddings table empty) |

Structural fact: every operation above is either pure stdlib algorithmics (BFS,
degree, heuristics, SQL) or an optional ML dep that is not installed. Nothing on the
live face requires porting a Python-ecosystem algorithm *except the aspirationally*
Leiden path.

## 3. Rust-side engine facts

- Union sidecar (`~/.mosaic/code-reality/scip/nautilus_trader/index.union.db`):
  181,591 workspace REFERENCES edges (PK caller_symbol/callee_symbol,
  provenance='SCIP'). EP S1 acceptance recorded 182,137 at build time — the sidecar
  is a function of the index state at injection; current row count is the
  authoritative figure and both are index-version-labeled facts.
- POC2 engine re-run on the current index (2026-08-26, 412,337 full edges —
  this is the fresh-index count; the EP POC table's 393,609 was the 2026-08-24
  index):

  | Metric | 8/24 index (EP POC) | current index (re-run) |
  |---|---|---|
  | SCIP reference edges (full) | 393,609 | **412,337** |
  | adjacency build | 319 ms | **349 ms** |
  | closure BFS (seed `EventStoreLifecycle]open`) | ≤9 ms | **8 ms** (depth1=16 new, depth2=0) |
  | hub degree ranking | 1 ms | **1 ms** |

  Sub-second cold adjacency, millisecond traversals at 400k edges. The hub top-10 is
  again dominated by std/core symbols (`unwrap` 9,771 references) — confirming the
  workspace-scoping filter already noted in the EP as a design requirement.

- Existing substrate in code-reality: `engine.rs` (SCIP adjacency engine, POC2
  lineage), `delta_tour.rs` (diff parsing), `common.rs` read-only graph.db layer.
  No FTS5 usage yet (CRG's `nodes_fts` virtual table is readable read-only when
  needed).

### Leiden crate survey (for the communities door)

| Crate | License | Weighted | Seeded determinism | Notes |
|---|---|---|---|---|
| single-clustering 0.7.0 (2026-08-04) | BSD-3-Clause | yes | **yes — "fixed seed gives bit-for-bit identical results"** | Leiden incl. refinement phase; RB + CPM quality functions; active; README warns of API churn in 0.x |
| leiden-rs 0.8.1 (2026-05-15) | MIT OR Apache-2.0 | yes | undocumented (rand 0.9 dep) | 4 quality functions (Modularity/CPM/RB/RBER), ships ARI/NMI/VI evaluation metrics, petgraph/gryf adapters; young (created 2026-04) but ~13k recent downloads |
| fa-leiden-cd 0.1.0 (2025-08-27) | unverified (registry reports non-standard) | yes | unknown | single release, effectively dormant; excluded from recommendation pending license verification |

The EP's hypothesized `community-detection` crate does not exist on the registry
under that name. No crate is needed at all for the directory-grouping parity tier.

## 4. Adjudication by family

### 4.1 Impact radius → adopt Rust (self-build)

CRG semantics are fully specified: per-kind weights with REFERENCES=0.6, per-depth
decay 0.6, score floor 0.05, bounded depth/nodes (constants.py:56-73) over a
best-score relaxation (graph.py:859-877). Porting is a weight table plus a
Dijkstra-shaped loop over an adjacency that POC2 already builds in 349 ms. Rust also
gains what CRG structurally cannot have: running on the **union** edge set (sidecar
REFERENCES joined with graph.db CALLS), which is the actual quality upgrade — CRG
reads graph.db only.

- Gate: S5 qname mapper (changed-file seeds are qnames; sidecar edges are SCIP
  symbols).
- Cross-check protocol: on a graph.db-only edge set, per-node best-score match vs
  CRG within float ε on a sampled seed set; the union run is then additive by
  construction (new edges only).

### 4.2 Flows → adopt Rust (self-build)

Entry-point detection is a heuristic (no incoming CALLS ∪ framework decorator ∪
conventional name, flows.py:164-214); tracing is forward BFS ≤15 over CALLS
adjacency (flows.py:222-281); criticality is a fixed 5-factor weighted score
(flows.py:324). All deterministic — exact-match cross-check against CRG on NT's
stored 10,359 flows is feasible and is the right pass-bar. Design note: keep flows
CALLS-only initially (the entry-point rule keys on CALLS targets, so sidecar
REFERENCES edges do not perturb parity); traversing REFERENCES is a union-semantics
decision that belongs behind the S4 ownership flip per the S1 ruling.

### 4.3 Communities → adopt Rust, two-tier (the one real algorithmic door)

The fact base removes the usual port-blocking argument: there is **no Leiden ground
truth to preserve** — CRG's live output is directory grouping. "Keep CRG Python"
would preserve directory grouping plus a resident Python process, and can never see
the union edge set (CRG consumes `get_all_edges()` from graph.db only,
communities.py:815).

- **Tier 0 — directory-grouping parity (floor, no crate):** CRG's fallback is a
  deterministic pure function of node file paths plus an adaptive grouping depth
  (communities.py:474-557). Exact-match cross-check on NT (42 communities) is the
  pass-bar. Cheap, zero algorithm risk, and already strictly equal to live quality.
- **Tier 1 — seeded Leiden (upgrade, crate adoption):** fixes the 33.5k-node
  mega-community that live CRG cannot split (its splitter is igraph-gated).
  Primary candidate **single-clustering** (BSD-3, bit-for-bit seeded determinism —
  matching CRG's own seed-42 reproducibility intent, communities.py:16-20);
  alternate **leiden-rs** (permissive, richer metrics, seed control unverified —
  verify before use). Pin the exact version either way.
- **Cross-check protocol for Tier 1:** exact partition match across Leiden
  implementations is neither possible nor meaningful (RNG iteration order differs).
  Sound bar: (a) modularity of our partition ≥ igraph reference − ε on the same
  weighted edge set; (b) structural agreement (ARI/NMI) vs the igraph reference
  above a fixed threshold fixed at POC design time (R2-R7 pass-bar convention);
  (c) size-distribution sanity — no community above the split threshold. The
  igraph reference is a **one-off scratch venv** (install igraph extra), not a
  permanent dependency.
- Gates: S5 qname mapper (community naming needs file_path/language/kind node
  metadata; union edges need symbol↔qname join); edge-set choice (graph.db-only vs
  union) is a REFERENCES-semantics decision → behind the S4 ownership flip per the
  S1 ruling. Tier 0 can land on graph.db-only edges with no S5 dependency.
- Risk register: both candidate crates are young (0.x, API churn warnings).
  Mitigation: Tier 0 floor is always available; the crate is an isolated leaf dep;
  version-pinned.

### 4.4 hub / bridge → adopt Rust (self-build)

Hub ranking is proven (POC2: 1 ms) — needs only the workspace-scoping filter.
Bridge is Brandes betweenness with k-sampling above 5k nodes in networkx
(analysis.py:75-82); a hand-rolled sampled Brandes in Rust is ~150 lines,
deterministic under a fixed sampling seed. Cross-check: exact match vs networkx on
a small repo; statistical match (rank correlation) on NT.

### 4.5 Semantic search → split the face; embeddings deferred as an explicit gap

- **Keyword face → Rust now.** On NT the live "semantic" search *is* the FTS5/BM25
  fallback (embeddings table empty, provider uninstalled). Reading CRG's existing
  `nodes_fts` virtual table read-only (or a simple token search over nodes) is
  instant parity with what has actually been running.
- **Embeddings face → defer, do not adopt now.** Options if it ever materializes:
  (i) cloud HTTP providers — CRG itself calls them with plain HTTP (embeddings.py
  OpenAI-compatible path); a Rust `reqwest` client is cheap **when needed**;
  (ii) local ONNX MiniLM via candle/ort — a heavy new dependency (~90 MB model,
  ML runtime) for a face with zero observed usage (0 rows, no provider keys ever
  configured) — over-engineering today; (iii) keep CRG Python permanently for
  embeddings — the only option that blocks full S4 retirement.
  Recommendation: mark the gap explicitly in capabilities ("keyword parity;
  embeddings not adopted"), revisit only on demonstrated demand.

## 5. User one-way doors (decision list)

| # | Door | Recommendation | Override cost if reversed later |
|---|---|---|---|
| 1 | Adopt Rust engine for impact radius, flows, hub, bridge (parity on graph.db-only edges first, union after S4) | **Yes** — pure-algorithm ports, POC2-proven cost, reversible (CRG MCP stays until parity verified) | Low: keep both faces during parity window |
| 2 | Communities: Tier 0 directory parity now **+** Tier 1 seeded Leiden via single-clustering (leiden-rs alternate; verify seed API before use) | **Yes, both tiers** — conservative alternative: Tier 0 only, defer Tier 1 until crates mature | Medium: Tier 1 is the only new algorithmic dependency in the plan |
| 3 | Semantic search: keyword face to Rust; embeddings face **deferred as explicit gap** (alternatives: cloud-HTTP now / local ONNX / permanent CRG) | **Defer** — zero observed usage on this machine | Low: any option can be added later; only "permanent CRG" constrains S4 |
| 4 | Union-edge semantics for engine queries (which edges feed impact/flows/communities) | **No new decision here** — already gated behind the S4 ownership flip by the S1 ruling; this report only records engine readiness | — |

## 6. S4 implication (what remains for CRG if Doors 1-3 are adopted)

Every live-consumed CRG MCP operation acquires a Rust replacement **except
true-embedding semantic search** — a face that has never run on this machine.
CRG's remaining role shrinks to: (a) graph.db **producer** (tree-sitter parsing,
multi-language — until S5/P3 replaces the Python producer face), and (b) the
deferred embeddings face if it ever activates. In the layered-replacement language
of the cutover audit: the **engine layer** becomes replaceable; the **data layer**
stays CRG-produced for non-Rust corpora (Rust corpora already have SCIP truth via
S1/S2). S4 can therefore be scoped as "engine retirement + graph.db ownership
flip", not "full CRG removal".

## 7. Number appendix (index-version labels mandatory)

- SCIP edge face, 2026-08-24 index (EP POC table): 677,197 reference sites →
  393,609 edges (9,831 item-level sites). POC2: adjacency 319 ms, closure ≤9 ms,
  hub 1 ms.
- SCIP edge face, current index (re-run 2026-08-26): 693,013 sites → **412,337**
  edges (10,961 item-level). POC2: adjacency **349 ms**, closure **8 ms**, hub
  **1 ms**; closure seed `EventStoreLifecycle]open` → depth1=16 new / depth2=0
  (prior index: depth1=15 + 1 reentry — same convergence shape, count drift from
  index regeneration).
- Union sidecar workspace edges: 181,591 (current COUNT; EP S1 build-time record
  182,137 — index-state function, see §3).
- NT graph.db (built 2026-08-24, schema 9): 80,312 nodes (76,814 community-tagged);
  1,155,668 edges — CALLS 678,829 / TESTED_BY 333,915 / CONTAINS 84,227 /
  IMPORTS_FROM 43,651 / REFERENCES 7,833 / IMPLEMENTS 6,984 / INHERITS 229;
  42 directory-based communities (largest 33,584); 10,359 flows; **0 embeddings**.

Sources: CRG sources cited inline (file:line under the installed 2.3.7 package);
registry metadata from crates.io API (fetched 2026-08-26 with UA); POC re-run log
`.agent-tmp/poc-scip-injection/rerun-20260826.log`; EP evidence section
`ep-v1plus-graph-engine.md` §POC.
