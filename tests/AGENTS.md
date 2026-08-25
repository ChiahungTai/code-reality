# tests — test suite

Unit tests run on synthetic fixtures only (no network, no real repos).
Integration tests carry the pytest `integration` marker (registered in
`pyproject.toml`), consume real repos and sidecar artifacts outside this
repo, and **skip** — not fail — when those are absent.

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
- `<tool>_integration.py` files are the integration counterparts of the
  `<tool>` unit tests

## fixtures/ (test helpers, not a package)

- `crg_db` — temp CRG-shaped graph.db generator
- `profile_repo` — writes synthetic `.code-reality.toml` into temp repos
- `make_trace` — synthetic viztracer trace JSON
- `delegation_driver` — viztracer tracing target, executed via subprocess
  (its golden delegation edge is asserted by
  `test_runtime_edges_integration`)
