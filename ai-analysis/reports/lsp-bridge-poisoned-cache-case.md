# Case: lsp-bridge check_file convergence gates pass poisoned diag-cache entries

> Status: **closed by `ai-analysis/execution-plans/
> ep-snapshot-zero-files-fix.md` segment S3** (2026-08-29) — F1
> (mutation Instant stamped at every origin, basis = newer of both) +
> F2 (stalled recast in time semantics) landed; T15/T16 deterministic
> poison-injection tests green; the detector (`lru_evict`) 20/20
> consecutive runs zero failures (pre-fix 4/35). Declared residuals:
> late-landing poison (push after basis) and force_reopen's own
> self-poisoning race — pre-existing, documented in the EP. Originally
> folded into the EP as S3 (user adjudication 2026-08-29); investigated
> 2026-08-29 by an independent read-only agent with reproduction (4
> failures in 35 isolated runs, one at 1-min load 5.74 — the race
> window is resident, load only amplifies).

## Symptom

`lru_evict_preserves_overlay_edits` (lsp-bridge `tests/bridge.rs:212`)
fails intermittently after ~60.2-60.9s with

```
overlay edits lost (overlay=Some(2) cache=Some((Some(3), 0))): count=0
```

Historically triaged as load starvation (isolated reruns pass — the
~11% rate made that look causal). The 2026-08-29 investigation
reproduced it at low load and decoded the mechanism: **a real bridge
correctness defect**, not a timing-only flake.

## Mechanism (code-verified chain)

1. **Poisoned entry**: LRU eviction sends didClose
   (`session.rs:526-534`, cap 8); pyrefly's didClose handler publishes
   `version + 1` **empty** diagnostics unconditionally (pinned pyrefly
   checkout `server.rs:4022-4026`); that async push lands after the
   bridge's own `diag_cache.remove` (`session.rs:533`) → the cache
   re-acquires a poisoned `(Some(3), 0)` entry.
2. **No real push**: the eviction re-open didOpen is processed with
   `has_subsequent_mutation=true` (the evicting didClose is already
   queued, microseconds later) → pyrefly skips the synchronous
   validation/publish (checkout `server.rs:3679`); a batch whose last
   mutation is didClose never re-runs the deferred open-files
   validation (checkout `:1866-1870`). Publication is left to the
   async recheck wave; when it stalls, nothing arrives within 60s.
3. **F1 — fresh gate defect (root)**: `edit_file_impl` discards the
   mutation `Instant` of the applied edit (`server.rs:386-394`). The
   nudge retry's `check_file_impl` therefore runs with
   `mutation_at=None` and `fresh` resolves `unwrap_or(true)`
   (`server.rs:317`) — unconditionally fresh. The poisoned entry
   (version 3 ≥ 2, 60s-old = "quiesced") is returned instantly as the
   answer: `count=0`. The retry gate added in `f82b274` (for the
   starvation signature) is defeated by exactly this path.
4. **F2 — stalled-gate defect**: the half-deadline recovery compares
   versions (`server.rs:333-338`); the poisoned version+1 always ≥ the
   re-open-reset overlay version (`session.rs:510-516` resets to 1),
   so `force_reopen` never fires and check 1 burns the full 60s
   deadline (`bridge.rs:225` `slow_timeout_ms = 60_000` →
   `server.rs:300-301`).

Consumed durations 60.19-60.89s = 60.0s deadline + ε, matching the
signature. `overlay=Some(2)` never loses to disk semantics (no 8:13
diagnostic ever appears) — this is a poisoned-cache read, not an
overlay-semantics failure.

## Ruled out

- `~/.mosaic` retirement: zero touch in the lsp-bridge test chain
  (only HOME use is the bin dev-notice, inert in test builds).
- Backend drift: `backend_bin()` resolves `~/.cargo/bin/pyrefly-lsp`
  (pinned engine rev `1d64c4b`), constant across all runs.

## Consumer impact (why this is not just a test problem)

The same gates serve real `check_file` callers: after an eviction +
edit sequence the bridge can silently return stale empty diagnostics
as a converged answer. F1/F2 fix the product path; the test is the
detector.

## Fix directions (no new dependencies)

1. **F1**: make the last LSP mutation `Instant` visible across calls —
   e.g. `OverlayEntry.last_mutation`, recorded by `edit_file_impl`;
   `check_file_impl`'s freshness basis = the newer of its own mutation
   and `entry.last_mutation`. The nudge then cannot converge on the
   poisoned entry and the existing retry gate becomes effective.
2. **F2**: recast `stalled` in time semantics — no push newer than the
   mutation basis past half-deadline ⇒ stalled ⇒ `force_reopen`,
   decoupled from version numbers (poisoned version+1 defeats any
   version comparison).
3. Upstream pyrefly (batch-tail didClose not re-running deferred
   validation): report, not required — the bridge becomes immune via
   1+2.

## Evidence anchors

- Bridge: `crates/code-reality-lsp-bridge/src/server.rs:300-301,317,
  333-338,386-394`; `src/session.rs:145-149,510-516,526-534,533`
- Test: `crates/code-reality-lsp-bridge/tests/bridge.rs:212-270`
  (quiesce 300ms, `slow_timeout_ms=60_000`, nudge retry `:262-265`,
  assertion `:270`)
- History: `e524866` (test born, quiesce misuse) → `decdb75` (P2:
  quiesce 300 + dedicated 60s slow timeout, "flaked at 10s and 20s")
  → `f82b274` (nudge retry gate, starvation signature — defeated by F1)
- Reproduction table (4/35 failures, loads 3.8-40.6): investigation
  session 2026-08-29; durations 60.19-60.89s all matching deadline+ε
