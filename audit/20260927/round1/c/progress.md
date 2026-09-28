# Segment C progress

Final worker checkpoint: SOURCE/TEST EDITS FROZEN; ready for main integration and independent review. Accepted EP R0-06/07/10 implementation and targeted verification complete. No delegation, worktree commit/git mutation, installation, or workspace formatting. Baseline evidence remains immutable. Historical intermediate sections below are retained as execution evidence and superseded by the final receipt.

Read: accepted EP, Round 0 findings and real CLI data probe, root/crates AGENTS, TDD/arch-thinking and architecture state/reuse/structure recipes. No graph index exists; impact tracing is source/text-derived, not index-verified.

Ownership: producer stages remain attempt-local; shared publication ownership will cover build publication/stamp/graph and explicit graph input acquisition through derived materialization and rename. Existing heal lock stays outermost. Manual restamps retain identity only with an index-byte binding; only validated production may establish new source provenance. Endpoint validation cannot exclude ABA or edits after the final observation.

Next: targeted regression RED, implementation, GREEN, adapted independent real CLI probe and affected test suites. libc already exists in code-reality dependencies; no dependency or Cargo.lock edit needed.

## Write failure checkpoint

Historical status (resolved): BLOCKED before test/source mutation. The first apply_patch targeting tests/source_identity.rs and tests/graph_db.rs was rejected by PreToolUse marshal admission guard. It reported missing or invalid `.agent-tmp/work-order.json` for this card worktree. Main subsequently issued the credential; worker read it and hooks/work-order-schema.md, then resumed. No self-issued credential or alternate write path was attempted. The subsequent Cargo command in the rejected tool cell did not execute.

At that blocked checkpoint no production changes or tests had been applied; the current status supersedes it below. No commit/git mutation, delegation, install, or workspace formatting was performed.

Prepared regression scope: manual restamp after same-path content edit retains old identity and remains stale; replaced index cannot inherit prior identity; missing canonical graph is torn while projection index remains graph-optional, then ensure_fresh repairs; deterministic producer-time source edit rejects publication and preserves index/meta/graph bytes; four same-process graph writers leave valid nonempty derived tables. Additional real CLI checks remain graph-publication failure recovery, four concurrent processes, lock owner death/release, and bounded busy.

Source findings: build_repo publishes then calls stamp_meta_core then build_from_cache_at; stamp_meta_core currently recomputes current disk identity from document-set equality; graph_db uses shared graph.db.tmp-build and materializes derived tables after rename. Existing libc workspace dependency is already declared in crates/code-reality/Cargo.toml, so this segment requires no dependency coordination edit.

Next action for main: issue a valid scoped work order through the established marshal issuance mechanism, then resume segment C. First run the prepared regression tests against baseline, preserve RED logs, implement accepted ownership/provenance changes, and rerun targeted suites and adapted CLI probe. Planned RED command (NOT RUN): `CARGO_INCREMENTAL=0 TMPDIR=$PWD/audit/20260927/tmp GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp cargo test --locked -p code-reality --test source_identity --test graph_db r0_ -- --nocapture > audit/20260927/round1/c/red.log 2>&1`.

## Current RED and implementation checkpoint

Credential issue resolved. Tests were placed in the narrowly named new `crates/code-reality/tests/publication.rs` instead of modifying the older two suites.

Executed RED command: `CARGO_INCREMENTAL=0 TMPDIR=$PWD/audit/20260927/tmp GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp cargo test --locked -p code-reality --test publication -- --nocapture > audit/20260927/round1/c/red.log 2>&1`.

Exit **101**, **0 passed / 5 failed**, preserved in `red.log`. All failures reached behavioral assertions: manual restamp changed identity after source edit; substituted index inherited prior identity; missing graph reported graph_lags=false; production-time edit was accepted; concurrent graph writers hit SQLite `table metadata already exists`. No fixture/build infrastructure failure.

Changes currently in progress: private `publication_writer.rs` with OS-owned flock, five-second bounded busy and unique attempt temp DB cleanup; `graph_db.rs` lock-aware helper, derived tables before rename, public materialize/index-maintenance ownership; `identity.rs` index byte hashing; `engine.rs` production observation type, canonical missing-graph detection, validated-vs-manual stamp separation. `lib.rs` registers private writer module. Build orchestration wiring is next; GREEN has not yet run. No adapted CLI probe acceptance yet.

### Subsequent execution

- Build wiring completed: capture A with uncached reads before producers, compare postproduction, acquire writer, compare again; pass A to locked stamp; graph/index maintenance use locked helpers. Query provenance now checks index-byte binding; old unbound metadata is legacy, never current-tree.
- `green1.log`: targeted publication exit 101, 4/5 pass; remaining assertion incorrectly expected nonempty CALLS flows from the REFERENCES-only Rust fixture. Corrected to require readable derived tables, nonempty communities and no orphan memberships.
- `red-binding.log`: extra replacement-without-restamp negative, exit 101 (0/1), demonstrated old meta falsely serving current-tree even after explicit graph rebuild. Added bound-index comparison.
- `green2.log`: `cargo test --locked -p code-reality --test publication -- --nocapture` under required env, exit 0, **7 passed / 0 failed**.
- `affected1.log`: affected suites exit 101 after build 33/33 and graph 9/10; old no-temp test counted the deliberately persistent `.writer.lock` as temp. Updated expected persistent artifacts (unlinking lock inode would be unsafe). No production regression was shown by this failure.
- Independent CLI adapter created as `demo_publication.py`; Python standards/output skills read. `probe1.log` exit 1: adapter meta filename typo fixed. `probe2.log` exit 1: production drift/preservation, manual stamp negative, graphfail recovery, four-process graph and mixed writers all passed; death probe killed uv launcher rather than actual Python lock owner. Holder now records its real PID. Original evidence untouched; failed runs retained.
- `probe3.log` and `affected2.log` currently running. All commands use `CARGO_INCREMENTAL=0 TMPDIR=$PWD/audit/20260927/tmp GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp`; CLI driver: `uv run --no-project python audit/20260927/round1/c/demo_publication.py run3`. No dependency/Cargo.lock edits by segment C.

Next: collect both runs, add deterministic wait-before-publication source edit coverage, examine final diff for stale semantic comments and ownership gaps, format only named changed files, then rerun justified targeted verification. Residual ABA/post-check live edits and noncooperating writers remain outside guarantees; abrupt death can leave attempt-owned temp artifacts, but not a held OS lock.

## Final receipt — frozen for integration

Changed production paths (all under `crates/code-reality/src/`): `build.rs`, `engine.rs`, `graph_db.rs`, `identity.rs`, `lib.rs`, `publication_writer.rs` (new private module), `refresh.rs`.

Changed tests: `crates/code-reality/tests/publication.rs` (new, nine integration cases); `crates/code-reality/tests/graph_db.rs` (persistent lock inode is expected infrastructure, not leaked temp).

Audit artifacts: this progress file; `demo_publication.py`; `red.log`, `red-binding.log`, `green1.log`, `green2.log`, `green3.log`, `affected1.log`, `affected2.log`, `final-targeted.log`; `probe1.log` through `probe4.log`; `run1/` through `run4/` retain CLI command/exit/stdout/stderr observations and isolated fixtures. Original Round 0 evidence was not overwritten. Failed intermediate runs are intentionally preserved.

All execution commands below use prefix `CARGO_INCREMENTAL=0 TMPDIR=$PWD/audit/20260927/tmp GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp`:

| Command suffix | Exit and result | Log |
|---|---|---|
| `cargo test --locked -p code-reality --test publication -- --nocapture` (initial RED) | 101; 0 passed, 5 behavioral failures | red.log |
| `cargo test --locked -p code-reality --test publication replaced_index_without_restamp -- --nocapture` | 101; 0 passed, 1 behavioral failure | red-binding.log |
| `cargo test --locked -p code-reality --test publication -- --nocapture` (final expanded regression set) | 0; 9 passed, 0 failed | green3.log |
| `cargo test --locked -p code-reality --test source_identity --test graph_db --test build --test refresh --test js_ts_freshness --test js_ts_build --test js_ts_graph --test js_ts_boundaries` | 0; 112 passed across eight suites, 0 failed/ignored | affected2.log |
| `cargo test --locked -p code-reality --test publication --test refresh --test graph_db` (after named-file formatting and final wording edits) | 0; 35 passed (9+16+10), 0 failed/ignored | final-targeted.log |
| `uv run --no-project python audit/20260927/round1/c/demo_publication.py run4` | 0; seven checked scenarios | probe4.log; run4/commands.json; run4/checks.json |

Formatting command, exit 0: `rustfmt --edition 2021 --config skip_children=true crates/code-reality/src/build.rs crates/code-reality/src/engine.rs crates/code-reality/src/graph_db.rs crates/code-reality/src/identity.rs crates/code-reality/src/publication_writer.rs crates/code-reality/src/refresh.rs crates/code-reality/tests/publication.rs crates/code-reality/tests/graph_db.rs`. No workspace formatter used.

`git diff --check -- crates/code-reality/src/build.rs crates/code-reality/src/engine.rs crates/code-reality/src/graph_db.rs crates/code-reality/src/identity.rs crates/code-reality/src/lib.rs crates/code-reality/src/refresh.rs crates/code-reality/tests/graph_db.rs` exited 0. Git commands were read-only; existing test helpers initialize/commit only their disposable fixture repositories beneath the requested TMPDIR.

Behavior evidence: full uncached source/policy A captured before production; postproduction and writer-held comparison reject drift without changing prior index/meta/graph; source edits while waiting for writer ownership are rejected; A rather than post-hoc disk identity authorizes the stamp. Actual index SHA256 binds retained/consumed provenance. Manual fresh stamp cannot create consumption evidence, and manual restamp of the same index retains the old identity through source drift. Missing canonical graph triggers torn/recovery; alternate projection control is graph-optional. Shared process-death-safe writer ownership covers graph input/cache acquisition through derived materialization and atomic rename, plus umbrella publication/stamp/graph. Five-second lock wait returns explicit busy. `run4` verifies four concurrent graph writers and mixed umbrella/graph writers on 5000 nodes; separate call-chain control verifies four writers preserve **1 flow, 8 memberships, 1 community** and SQLite integrity. Lock owner SIGKILL releases ownership and subsequent graph build succeeds; holder is an independent Python process using the same OS lock, not a killed CR process mid-publication.

Limits/unverified: endpoint equality is not an immutable snapshot/ABA guarantee; live source changes after final observation remain possible. Noncooperating writers are outside the advisory lock contract. Process death can leave attempt-owned temp files (ordinary errors clean only the current attempt); the persistent lock inode deliberately remains. Index-byte validation adds a linear index read to freshness; large-corpus performance was not benchmarked. Old metadata without `index_sha256` remains legacy/unverifiable until rebuilt. Existing query-side source stat-cache precision limitations are unchanged. No full workspace run, independent review, deployment or live-consumer acceptance is claimed. Main owns full suite, root/module/plugin documentation consistency and final review. No unresolved executed-test failure remains.

Next step: main integrates these frozen paths, runs full workspace once all writers are collected, and adjudicates fresh review. No further speculative tests or edits by this worker.
