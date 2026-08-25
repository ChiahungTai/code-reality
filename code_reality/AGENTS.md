# code_reality — tool package

Flat, single-level package (no subpackages). Every consumer-facing module
is a standalone CLI (`python -m code_reality.<tool> --repo <repo-root>`);
`common` / `profile` / `exclusions` / `hazard` are library-only, imported
by the tools. Consumption model is subprocess CLI from other repos'
sessions — this package is not imported into consumer codebases.

## Architecture position

The toolchain aggregates three evidence sources and never writes to any
producer:

- **SCIP semantic indexes** (rust-analyzer) — compiler-grade symbol truth
  for Rust (`scip_refs`)
- **CRG graph.db** (code-review-graph) — structural graph, read-only via
  `common.connect_ro` (WAL-aware; torn-read guard `db_mtime_ns` /
  `assert_db_unchanged` because CRG may rebuild concurrently)
- **viztracer traces** — runtime call edges (`runtime_edges`)

plus the cross-language seam: `boundary_build` / `boundary` bridge PyO3
declarations to `.pyi` contract stubs.

All derived artifacts are **commit-anchored sidecars** under
`~/.mosaic/code-reality/` (snapshot JSON, boundary SQLite, scip cache):
sha in the filename → idempotent per commit, staleness detectable,
HEAD-matching file preferred over newest.

Repo-specific knowledge (module prefixes, exclusions, hazard registries)
lives in each scanned repo's `.code-reality.toml`, loaded by `profile` —
the tool layer embeds no repo-specific special cases. This repo profiles
itself via its own `.code-reality.toml` (dogfood).

## Module boundaries

Internal layering (absolute imports; `__init__.py` is docstring-only and
re-exports are forbidden — import full paths):

- Foundation: `profile` (most-imported module; `.code-reality.toml` single
  source with crash-only schema validation, `module_of` / `claims_re` /
  `scan_roots`), `exclusions` (single predicate over profile excludes),
  `common` (edge-kind whitelist, sqlite RO + torn-read guard, CodeTour
  anchor pattern, commit-anchored `_meta` block)
- Pure detection layer: `hazard` (dynamic-dispatch hazard patterns) —
  consumed only by `hub_refs`
- Composition edges: `boundary` → `boundary_build`; `delta_tour` →
  `transition`; `hub_refs` → `hazard`; `tour_upgrade` → `tour_manifest` +
  `tour_validate`; `chain_tour` → `tour_manifest`
- Standalone: `scip_refs` (imports only `scip_pb2`), `runtime_edges`

The package depends on no producer code (CRG, rust-analyzer) — only their
on-disk artifacts — and nothing here imports from `tests/`.

## Tool families (navigation seeds)

### Graph.db consumers (CRG side)

- `snapshot` — module-edge set export → commit-anchored sidecar JSON
- `transition` — diff of two snapshot edge sets + EP plan-claims
  comparison (reversed edges reported in the added direction)
- `hub_refs` — CRG callers/callees aggregation per file with prod/test
  split; bare-symbol resolution via nodes-table exact match; integrates
  the hazard stage (AST-level resident always, rg-level gated at
  `static_prod <= 2`) — the deletability safety net against
  "zero refs → safe to delete" misjudgment
- `graph_audit` — graph.db Rust completeness audit: D1 same-name-method
  risk scan + D2 rust-analyzer symbols reconciliation; `--json` contract
- `graph_csv` — graph.db → file-level nodes/links CSV (Cosmograph input)

### SCIP truth query

- `scip_refs` — def/refs truth source for symbols CRG's same-key dedup
  silently drops; modes: symbol query / `--audit` / `--stamp-meta` /
  `--build-cache`; exit codes 0=found / 1=none / 2=env error
- `scip_pb2.py` + `scip.proto` — vendored gencode/schema (Apache-2.0,
  sourcegraph/scip); protobuf is the sole third-party runtime dep; do not
  hand-edit the gencode

### PyO3 boundary family

- `boundary_build` — regex scan of `*.rs` for pyo3 declarations,
  cross-referenced against `.pyi` stubs, written to a commit-anchored
  boundary sidecar SQLite
- `boundary` — read-only query CLI over that sidecar: Python symbol →
  Rust ground truth (path:line), `--rs` reverse lookup, stale detection

### Tour family (narrative generators + corpus governance)

- `chain_tour` — callchain markdown docs (tree-shaped frames) → one
  CodeTour per scenario, frame line numbers re-anchored via graph.db
- `delta_tour` — transition diff + git hunk anchors → delta-review tour
- `tour_manifest` — `.tours/manifest.toml` provenance (derived vs
  curated); generator inferred from filename conventions
- `tour_validate` — mechanical validation of the `.tours` corpus (link
  keys, anchor three-states, manifest source)
- `tour_upgrade` — old-format tour migration; dry-run by default
  (curated-corpus protection)

### Runtime

- `runtime_edges` — viztracer trace JSON → runtime call-edge table
  (nesting by (pid, tid) ts-interval enclosure)

## Design invariants

- **Crash-only**: invalid input crashes (e.g. profile schema violations);
  no repair-through paths. Tests assert crashes directly.
- **Exit-code contracts**: query/audit CLIs encode machine-readable
  outcomes (found / none / env-error) — consumed as gates by callers.
- **Producer artifacts are read-only**: graph.db and SCIP indexes are
  never written; derived caches go to separate sidecar files.
- **Language policy**: authored content is English; migrated docstrings
  keep their original language (zero-change migration constraint).
