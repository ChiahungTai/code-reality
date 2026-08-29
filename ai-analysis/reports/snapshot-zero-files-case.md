# Case: snapshot exports 0 files on REFERENCES-only graph dbs

> Status: **closed by `ai-analysis/execution-plans/
> ep-snapshot-zero-files-fix.md`** (S1+S2+S4, 2026-08-29) — files face
> widened to all kinds (`_meta.files_face` marker), empty-set WARN
> attributes its true cause, transition degenerate guard lives at the
> summarize layer with delta_tour the sole diff interface (tour
> description carries the warning). L4: self 70 / NT 1955 / ai-rules 7
> files; mosaic ×3 same-commit increment +5/+6/+26. Originally
> investigated 2026-08-29 by an independent read-only agent; mechanism
> first isolated by the EP session, then independently confirmed and
> extended.

## Symptom

`code-reality snapshot --repo <repo>` returns `0 files` with the WARN
`graph.db 與 --repo 不同 root？` on repositories whose graph.db edges
are (near-)entirely `REFERENCES`. First observed on the code-reality
self repo (0 files / raw 2277 edges).

## Mechanism (confirmed)

`export_module_edges` (`snapshot.rs:121-122`) filters
`WHERE kind IN (IMPORTS_FROM, CALLS, INHERITS)` — `common.rs:17`
`EDGE_KINDS`, frozen at R4 (`990edbc`, 2026-08-25) as the CRG Python
parity face. On a REFERENCES-only db the filter matches **0 rows**, and
`files.insert` (`snapshot.rs:146-147`) only runs inside that loop, so
the file set is necessarily empty while `raw_edge_count` (full-table
count, `:153-155`) still reports thousands — the two numbers in the
WARN line come from different queries; their divergence is the
mechanism's own confession.

The WARN's "different root" attribution is **refuted**: on the self
repo, 899/899 `nodes.file_path` lie under the canonical repo root,
2277/2277 edges join both endpoints to nodes, and `repo_relative`
(`common.rs:58-64`) is correct. The path chain is healthy; the kind
filter is the bottleneck.

## Root cause chain (confirmed)

1. `graph_db build` derives CALLS via `py_calls::call_sites` — a ruff
   **Python** parser (`py_calls.rs:39`) — and its input is pre-filtered
   to `.py` files (`graph_db.rs:485-490`). For a Rust repo (scip
   producer) `call_marks` is always empty and every edge falls to the
   REFERENCES branch (`graph_db.rs:626-631`). REFERENCES-only is the
   **norm** for scip/Rust repos, not an anomaly.
2. The lsp-harvest producer face is explicitly frozen REFERENCES-only
   (`graph_db.rs:477-480`, documented; its CALLS story out of scope) —
   ai-rules inherits the same collapse by design.
3. Historical breakpoint: structural edges (IMPORTS_FROM/CALLS/
   INHERITS) were supplied by the legacy importer (`import_legacy`
   copied kinds verbatim, S4 `9c4d2d8`). W3/W5 retired and removed the
   importer on 2026-08-28 (`3980fe1`, `4a36b22`) — severing the only
   structural-edge source for Rust repos. The sidecar tape shows the
   cliff exactly: snapshots through 2026-08-27 carry 43-47 files / raw
   ~7007; from 2026-08-28 14:24 (sidecar `22900069`) onward: 0 files /
   raw 2277.
4. The cutover EP did reason about the filter —
   `ep-legacy-db-consumer-cutover.md:87` (F4): "module_edges parity is
   possible *because* the kind filter excludes REFERENCES" — treating
   the filter as a parity guarantee. What was missed: on a
   REFERENCES-only db the projection collapses to the empty set, not
   merely diverges in counts.

## Blast radius (measured, 2026-08-29)

| repo | producer | edge kinds | snapshot |
|---|---|---|---|
| code-reality | scip (Rust) | REFERENCES 2277 | **0 files** |
| nautilus_trader | scip (Rust) | REFERENCES 232052 | **0 files** |
| ai-rules | lsp-harvest (Python) | REFERENCES 444 | **0 files** (frozen face) |
| mosaic_alpha ×3 | scip (pyrefly Python) | CALLS present (25.8K-26.3K) | healthy (1202 files measured) |

Downstream:

- `transition.rs:372-382` — diff/new/gone all empty prints
  "無結構變化" with **no empty-pair guard**: two 0-file snapshots
  always produce a false "no structural change" (false negative); the
  healthy→empty direction produces mass gone-files false positives.
- `delta_tour.rs:10,675-699` consumes `transition::load_snapshot` +
  `summarize` — inherits both failure modes.
- `chain_tour` anchors on graph.db directly, does not consume snapshot
  files — unaffected.

## Fix directions (analysis only)

- **(c) WARN attribution split + transition empty-pair guard** —
  minimal, zero semantic risk, immediately stops the misleading
  "different root" guidance and the false "no structural change"
  conclusions. Branch on: raw>0 && projected 0 → kind-distribution
  cause (normal for scip/Rust + lsp-harvest repos); unresolvable
  endpoints → root cause. Pinned bytes: `s2_snapshot.rs:331-358`
  (`cli_empty_set_warn` pins the WARN verbatim — and its fixture
  manufactures the empty set via out-of-root endpoints, i.e. the test
  scenario itself encodes the wrong attribution).
- **(a) widen the projection** — split the kind sets: widen the
  `files` face (restores the participating-file set) while keeping
  `module_edges` structural-only (REFERENCES in module diffs would
  destroy the "no structural change" boundary semantics and drown
  diffs in noise at NT scale). Pinned: `s2_snapshot.rs:86-110`,
  `common.rs:467-470`.
- **(b) build-side Rust CALLS derivation** — semantically cleanest
  (fixes the source), needs a Rust call scanner (syn/tree-sitter);
  already a watch item (`ep-occurrence-producer.md:116` R4w; `:42-47`
  rules out lexical shortcuts). Does **not** help lsp-harvest repos —
  (a)/(c) still needed for coverage.

Recommended sequencing: (c) first as a minimal PR, then the files-only
variant of (a); (b) rides the R4w watch item.

Post-fix hygiene: delete or mark the existing 0-file sidecars (e.g.
self repo `22900069` and later) so transition baselines don't silently
compare against degenerate snapshots.

## Evidence anchors

- `crates/code-reality/src/snapshot.rs:121-122,146-147,153-155,475-484`
- `crates/code-reality/src/common.rs:15-17,58-64,467-470`
- `crates/code-reality/src/graph_db.rs:474-490,626-631`
- `crates/code-reality/src/transition.rs:372-382`;
  `delta_tour.rs:10,675-699`
- `crates/code-reality/tests/s2_snapshot.rs:86-110,331-358`
- `ai-analysis/execution-plans/_done/ep-legacy-db-consumer-cutover.md:87`
- `ai-analysis/execution-plans/_done/ep-occurrence-producer.md:42-47,116,163`
- Sidecar tape: `.code-reality/snapshots/code-reality-*.json` (43-47
  files through 2026-08-27; 0 files from `22900069`, 2026-08-28 14:24)
