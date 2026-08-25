# tests — test suite

Self-contained (open-source policy below): every test runs on committed
synthetic fixtures or tmp-dir state — a fresh clone with only the dev
deps passes `uv run pytest` with zero skips and zero environment
dependence. The pytest `integration` marker (registered in
`pyproject.toml`) is reserved for future tmp-repo / fs-git drills.

## Open-source test policy (2026-08-25)

Follows the established pattern of comparable tools (code-review-graph,
scip-callgraph): **the suite is self-contained** — a fresh clone with
only the dev deps passes; no personal paths, no private corpora, no
live external-repo gates.

- Committed synthetic fixtures reproduce corpus *shapes* (enc arity,
  symbol morphology, tie structures, graph.db schemas); tmp-dir repos
  for git/fs integration. No vendored third-party-derived fixtures
  (license provenance; stale-fixture rot).
- Real-corpus evaluation, if ever wanted, is a separate harness run
  against public repos with results committed as artifacts — not a
  test gate (CRG `eval/` precedent).
- How consumers verify the tools against their own repos is the
  consumers' business — docs show usage, nothing more.
- Migration state: the legacy external-consumer tests were removed on
  2026-08-25 under this policy (chain_tour pins, NT scip parity L4 +
  callers pins + SM-9 drill + LSP oracle fixture, and the mosaic
  graph.db integration set incl. the delegation_driver tracing target);
  their historical adjudications stay recorded in the archived EPs.

Run: `uv run pytest` (everything) or `uv run pytest -m "not integration"`
(unit only).

## Conventions

- `conftest.py` puts `tests/fixtures/` on sys.path — fixture helpers are
  imported as plain modules
- Incident regressions are pinned as named tests (e.g. boundary_build
  doc-comment / rename / tuple-struct incidents, graph_audit per-block
  counting, transition reversed-edge reporting) — each encodes a real past
  failure; keep the coverage when refactoring
- Crash-only expectations are asserted directly (invalid input must
  raise), mirroring the package's crash-only philosophy
- Exit-code contracts (found / none / env-error) are tested as CLI
  contracts
- `test_scip_refs` uses a duck-typed fake index — the query-matching
  suite needs no protobuf install
- `test_delta_tour` / `<tool>_integration.py` files are the integration
  counterparts of the `<tool>` unit tests
- rust-analyzer is the one declared carve-out to "zero environment
  dependence": `rust-toolchain.toml` pins it as a toolchain component
  (rustup environments auto-install). When it is absent, graph_audit
  parity cases still pass as fail-loud equivalence (both sides exit 2
  with empty stdout) — never a skip

## fixtures/ (test helpers, not a package)

- `crg_db` — temp CRG-shaped graph.db generator
- `profile_repo` — writes synthetic `.code-reality.toml` into temp repos
- `make_trace` — synthetic viztracer trace JSON
