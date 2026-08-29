# EP: pyrefly-index sidecar invalidation + build-side staleness gate (fix relay)

Source: ai-rules fix relay (2026-08-28, from W4 mosaic cleanup live
testing) — offline_backtesting's first `graph_db build` silently read an
8/27 lsp-harvest sidecar (CALLS 0 / REFERENCES 610,554 bad db, no WARN)
because `pyrefly-index` rewrote `index.scip` without invalidating the
old cache db + stamped meta.

## Root cause

`build_from_cache_at`'s lsp fast-path (graph_db.rs) trusts an existing
lsp-producer cache db unconditionally — it bypasses `open_face`'s
staleness ladder on purpose (placeholder index.scip would fail protobuf
parsing), but that also bypassed any freshness check against the index
file itself.

## Fix (both, adjudicated belt + braces)

1. **Producer-side invalidation (root fix)**: `pyrefly_producer::emit`
   removes the sidecar artifacts derived from the PREVIOUS index
   (`cache::sqlite_path` + `engine::meta_path`) after a successful
   write; removed paths are reported (`[OK] invalidated stale sidecar:`
   lines, `EmitReport.invalidated_sidecars`). Covers every downstream
   consumer, not just build.
2. **Build-side fail-loud gate (defense-in-depth)**: the lsp fast-path
   errors when the cache db mtime < index.scip mtime — an index newer
   than the lsp cache means a producer run has superseded the sidecar.
   Only the mtime contradiction is checked; schema/head checks stay out
   (deliberate bypass for lsp slots, unchanged). Error guides recovery:
   rerun `pyrefly-index` (auto-invalidates) then build.

## Acceptance (relay scenario, live binary)

1. Slot seeded with lsp-flavored db + stamped meta + placeholder index →
   `pyrefly-index` → both sidecar files removed, reported.
2. Direct `graph_db build` (no stamp/build-cache, the relay's exact
   path) → builds from the fresh index: CALLS 1 / REFERENCES 0 on the
   mini fixture — correct call split, not the bad REFERENCES-only shape.
3. Gate: lsp db re-seeded + index.scip touched newer → build exits 1
   with the superseded-sidecar message.

Regression pins: `emit_invalidates_stale_sidecar_artifacts`
(pyrefly-producer end_to_end), `build_fails_loud_on_stale_lsp_sidecar_
superseded_by_newer_index` (graph_db). Existing lsp trusted-path tests
pass unchanged (no false positive on the legitimate harvest order).

## Notes

- Reinstall-order trap hit three times this session: `cargo install`
  must rerun AFTER code edits — live L4 checks with a stale installed
  binary contradict the source tree.
- NT pre-W4b note from the relay holds: NT's S4-era sidecar gets the
  same protection on its next producer run; until then the build gate
  fires loud instead of silently trusting it.

## Review residuals (dual-context, post-adjudication)

- **mtime granularity**: the gate is strict `cache_m < idx_m` (fail-open
  on equality, matching the `db >= idx` harvest contract); on coarse-
  mtime filesystems a legacy slot landing on the same tick as a new
  index escapes the gate — APFS ns granularity makes this near-
  theoretical here (review R4).
- **`document_symbols_at` (graph_engine) still trusts the sidecar
  without staleness check** (primed F1): outline face only, no db
  build; a legacy stale slot serves a stale outline silently until the
  next producer run removes it. Known transitional edge, out of this
  EP's adjudicated scope.
- **Stat failure on a missing placeholder index.scip stays a hard
  error** (review R2): harvest always writes the placeholder, so
  cache-without-index is anomalous — consistent with the
  `stale_reason` stat-fail=loud precedent.
- **Unconditional invalidation re-stamps on unchanged repos** (review
  R5): accepted cost (fail-safe direction); digest-keyed skip judged
  complexity-over-benefit.
- Producer invalidation covers all three slot artifacts (cache db,
  stamped meta, fn-defs sidecar — review R3) and tolerates a concurrent
  remove of the same file (NotFound = target state, review R1); a real
  remove failure reports that the index itself was already written.
