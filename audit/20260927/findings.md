# Round 0 quality audit

Baseline: `eaa5d2462ea7821021ad0ea87403484ac0e47538` (workspace 0.9.3). Initial working tree clean. This is a read-only audit of production sources; only audit assets and exempt build/test artifacts may be written.

## Findings register

| ID | Severity | Location | Evidence level | Recommended disposition |
|---|---|---|---|---|
| R0-01 | P2 | plugin/.mcp.json:6, :12 | Reproduced | Fix |
| R0-02 | P2 | crates/code-reality/src/mcp_server.rs:255 | Reproduced | Fix |
| R0-03 | P2 | crates/code-reality/src/mcp_server.rs:334 | Reproduced | Narrow contract claim |
| R0-04 | P2 | crates/code-reality-lsp-bridge/src/server.rs:446 | Reproduced | Fix |
| R0-05 | P1 | crates/code-reality-lsp-bridge/src/session.rs:549 | Reproduced | Fix |
| R0-06 | P1 | crates/code-reality/src/engine.rs:1131 | Reproduced | Fix |
| R0-07 | P1 | crates/code-reality/src/engine.rs:890 | Reproduced | Fix |
| R0-08 | P2 | crates/code-reality-lsp-bridge/tests/ra_equivalence_battery.rs:56 | Reproduced | Fix |
| R0-09 | P2 | crates/code-reality-lsp-bridge/src/session.rs:475 | Reproduced | Narrow contract claim |
| R0-10 | P2 | crates/code-reality/src/graph_db.rs:749 | Reproduced | Fix |

## Calibration and evidence boundaries

Existing mechanisms and progress are recorded in [progress.md](progress.md). Graph index absent in this worktree: structural observations are source/text-derived, not index-verified. Runtime evidence takes precedence over inferred defects. Findings are recommendations for user adjudication, not applied changes.

Final register: **10 findings — 3 P1, 7 P2, 0 P3**. All ten have runtime evidence. No unreproduced theory is classified as a defect. The priority is the user-defined scale, not the differing labels in raw reviewer notes. R0-04 is conservatively P2 because the reproduced result hides backend death but does not establish incorrect diagnostics.

### Build and test baseline

- Host: macOS arm64; `rustc 1.96.0 (ac68faa20 2026-05-25)`, `cargo 1.96.0 (30a34c682 2026-05-25)`; locally built CLI/MCP/bridge all report `0.9.3+eaa5d24`. [Version evidence](evidence/version-confirmation.txt).
- `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp" cargo test --workspace --locked`: exit 101, 141 passed / 1 failed before fail-fast stopped the suite. The `graph_db::build_stamps_snapshot_metadata` fixture assumed no ancestor Git repository; audit-local TMPDIR invalidated that assumption. A focused `GIT_CEILING_DIRECTORIES` control passed. This audit-induced failure is **not** a product finding.
- `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp" GIT_CEILING_DIRECTORIES="$PWD/audit/20260927/tmp" cargo test --workspace --locked --no-fail-fast`: exit 0; **runner-reported 551 passed, 0 failed, 0 ignored**, across 60 result blocks including zero-test binary/doc targets. R0-08 proves at least one reported pass skipped its intended assertions. Do not interpret this as 551 fully exercised contracts. [Full baseline log](evidence/cargo-test-isolated.log), [machine-counted totals](evidence/baseline-counts.json).
- `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp" cargo build --workspace --locked`: exit 0. [Build log](evidence/cargo-build-workspace.log).
- Strict Rust equivalence battery: exit 101, 0 passed / 1 failed; default uncaptured control: exit 0 with `[SKIP]`, 1 passed / 0 ignored (R0-08).
- Baseline stderr also includes rust-analyzer shutdown complaints and a backend panic; the Cargo runner still exits 0. These logs are preserved, not summarized as error-free execution. Their independent root cause was not established and is not promoted to an extra finding.

### Coverage and limits

- Calibration read: supplied root AGENTS.md, README.md, crates/AGENTS.md and bridge README. No separate STATE.md or architecture document was found in the repository inventory; architecture guidance lives in the AGENTS files. No backlog state was changed.
- Deep probes: plugin version selection, real stdio MCP failures/cancellation/EOF, LSP session/framing/cache/teardown, source identity stamping, first-build graph recovery and concurrent graph writers. Each finding lists the defenses already present and the precise evidence gap.
- The full test baseline covers the workspace; the adversarial review is bounded to the above cross-component surfaces. This is not exhaustive validation of graph algorithms, every language producer, HTTP cancellation, every operating system, memory leak freedom or all dynamic consumers. JS/TS real external producer integration was not separately provisioned; no dependency installation was performed for the audit.
- Independent source reviewers completed their notes after a usage interruption: [LSP review](evidence/lsp/findings.md), [data-plane review](evidence/data/findings.md). Main repeated the strongest probes and reconciled priorities and claims. These are separate contexts in the same model family, not cross-family validation.
- Failed exploratory fixture `data/run1` is retained but is not accepted evidence. `data/run3` is the no-Git counter-control; identity-stamped findings use `run5` (fixtures inherit the unchanged parent HEAD for read-only Git queries). Fixture source edits and injected failures occur only under the permitted audit directory.
- Unobserved: graph corruption in concurrent-write probes (integrity check passed), incorrect type diagnostics after backend death, client crash due to oversized error, and indefinite blocking. Their absence in these probes is not proof of impossibility.
- Final source scope: baseline HEAD unchanged; tracked `git diff` empty; `git status --short` shows only `?? audit/`. Build/test-generated target and temporary assets are exempt artifacts. No source fix, documentation amendment outside audit, commit, push, package installation or live-service change was performed.

## R0-01 — Exact-version bootstrap gate accepts neighboring releases

- Category: B/D. Severity: P2 (verification-contract gap; no downstream incompatibility claimed).
- Location: `plugin/.mcp.json:6` and `:12`, both `pinned()` shell predicates; regression coverage entry `crates/code-reality/tests/plugin_wrapper.rs:10` / `scripts/test-plugin-wrapper.sh:98`.
- Existing defenses: canonical uv executable directory, three-face conjunction, post-install verification, loud install failures, and tests of stale `0.0.0` versions. These are present and were not overlooked.
- Root cause: `case ... in "$want"*)` treats a textual prefix as release identity. With pin `0.9.3`, both `0.9.30+testrev` and `0.9.3-rc1+testrev` satisfy the gate. The same predicate is reused after installation and in the bridge wait loop.
- Evidence level: **Reproduced** on baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`.
- Reproduction: `uv run python audit/20260927/evidence/demo_wrapper_pin.py` (exit 0). This extracts and runs the actual checked-in wrapper strings against isolated fake uv and executable faces; no package install/network action occurs.
- Observed: both wrappers exit 0 and print `SERVED ... 0.9.30+testrev` / `SERVED ... 0.9.3-rc1+testrev`; `attempted_install=false`, stderr empty. Exact `0.9.3+testrev` is the positive control. All six cases are in [results.json](evidence/wrapper-pin/results.json); command output in [wrapper-pin.log](evidence/wrapper-pin.log).
- Recommendation: **Fix** the release-identity predicate, preserving the supported `+build-rev` suffix, and add neighboring-patch/prerelease negative cases. User may instead explicitly narrow the contract to prefix compatibility, but that would abandon the exact-pin guarantee.
- Deduplication: main MCP precheck/postcheck, bridge precheck/wait and all three checked executable faces are one root cause. This is not evidence that any published `0.9.30` exists or is incompatible.

## R0-02 — MCP error responses bypass the consumer output cap

- Category: B/C. Severity: P2 (reproduced contract asymmetry; client disconnection not observed).
- Location: `crates/code-reality/src/mcp_server.rs:255` (`map_tool_output` error branch); existing cap at `:217` and success application at `:246`.
- Existing defense: successful tool text is capped at 1 MiB with a UTF-8-safe cut and explicit `[TRUNCATED]` marker. The source describes this as a last-resort consumer protection. Nonzero tool exit instead creates `McpError` directly from uncapped stdout/stderr.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. Run `uv run python audit/20260927/evidence/demo_mcp_lifecycle.py` (exit 0), using this checkout's `target/debug/code-reality-mcp --stdio` and a fake producer emitting 1,200,000 stderr bytes before exit 2.
- Observed: real MCP error code `-32603`, message 1,200,087 bytes, whole response 1,200,151 bytes, no `[TRUNCATED]`; server exit 0. [Results](evidence/mcp-lifecycle/results.json), [full response](evidence/mcp-lifecycle/large-error/response.json), [run log](evidence/mcp-lifecycle.log).
- Recommendation: **Fix** the error serialization path to apply an equivalent bounded-text contract; add a real transport error-output test. Alternatively explicitly restrict the cap promise to successful results.
- Deduplication: all module failures routed through `map_tool_output` share this root. No claim of memory exhaustion or client crash is made; those were not observed.

## R0-03 — MCP cancellation and disconnect do not stop an active producer

- Category: A/B. Severity: P2 (lifecycle contract gap requiring a policy decision, not an assertion of unpromised rollback).
- Location: `crates/code-reality/src/mcp_server.rs:334` (`run_module` starts blocking work without a cancellation handoff), `crates/code-reality/src/build.rs:354` (blocking external producer), `crates/code-reality/src/mcp_server.rs:990` (stdio ownership lifecycle).
- Existing defenses: module panics are isolated, blocking work leaves the async runtime responsive, and the tool description explicitly allows long build times. None of those is evidence of producer cancellation.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. Same command/assets as R0-02; `cancel` and `eof` cases use a producer that signals start, sleeps three seconds, writes a completion marker, then exits 2.
- Observed cancellation: after sending `notifications/cancelled`, `tools/list` still responds, the producer nevertheless writes its completion marker, and there is no tool response for the cancelled request. Observed EOF: process remains alive after 500 ms; exits only after approximately 3.051 seconds, with producer completion marker present. Server exits 0 in both cases.
- Recommendation: **Narrow contract claim**: explicitly document cancellation as stopping response delivery while already-started work may continue, including file writes; define whether transport closure is expected to await active producers. If stronger cancellation is desired, scope it as separate work with subprocess ownership and commit-boundary semantics.
- Deduplication: cancelled calls and stdin EOF are two observations of the same missing handoff to blocking work. The probe proves continuation, not indefinite hanging, post-cancellation graph publication, or a working deadline. No rollback guarantee is assumed.

## R0-04 — Dead LSP backend can still yield a successful clean check

- Category: A/D, with B test-boundary gap. Severity: P2 (known backend failure is hidden by a successful cached verification result; incorrect diagnostics were not demonstrated).
- Location: `crates/code-reality-lsp-bridge/src/server.rs:392` and `:446`; `crates/code-reality-lsp-bridge/src/session.rs:649` (unchanged open file bypasses notification/liveness guard).
- Existing defenses: requests and notifications check backend liveness; reader EOF marks the session dead; status exposes `state=dead`; diagnostic version/time/quiescence guards exist. `tests/bridge.rs:78` checks dead-backend requests, while `:465` exercises diagnostic timeout; those are different paths from an unchanged-file cache hit.
- Root cause: `check_file_impl` enters `sync_open`, which may return without touching the dead backend. A previously converged cache entry then satisfies the diagnostic predicates and returns success; no liveness check occurs at that return boundary.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`, both direct library and real stdio MCP.
- Commands from repository root: `audit/20260927/evidence/lsp/probe dead-cache` and `uv run --no-project python audit/20260927/evidence/lsp/demo_mcp.py`; both confirmed exit 0. Rebuild instructions are in [LSP evidence README](evidence/lsp/README.md).
- Observed: initial `count=0`; after killing only the probe-owned backend, `lsp_status` explicitly reports `state=dead`, but another `check_file` returns `isError:false` with the same `count=0`. Direct call returns `Ok("count=0\n")` in 0 ms. Bridge cleanly exits 0. [Direct log](evidence/lsp/dead-cache-confirm.log), [MCP transcript](evidence/lsp/mcp-confirm.log), [source probe](evidence/lsp/probe.rs).
- Recommendation: **Fix** by making cached-result acceptance honor backend death, or explicitly return a labeled historical result that cannot be confused with a successful current check. Add the exact warm-cache → backend-death → repeat-check scenario.
- Scope/deduplication: reproduced on the Python family with a deterministic fake backend. The shared cache path is used by the other families, but their runtime behavior was not separately reproduced. This demonstrates false-success status, not that the unchanged fixture itself contains a missed type error.

## R0-05 — A blocked LSP stdin write bypasses check and shutdown deadlines

- Category: A/B. Severity: P1 (a stalled child prevents both request completion and cleanup).
- Location: `crates/code-reality-lsp-bridge/src/session.rs:549`, `:529`, `:555`; `crates/code-reality-lsp-bridge/src/framing.rs:15`; `crates/code-reality-lsp-bridge/src/server.rs:392` versus deadline creation at `:400`.
- Existing defenses: response receive timeouts, diagnostic convergence deadline (Python 20 seconds), and a ten-second child-exit loop are present. They start after earlier synchronous writes/lock acquisition, so those earlier waits are not bounded by them.
- Root cause: writing a full document holds the backend/interaction locks while blocking on child stdin. `check_file` has not yet entered its deadline loop. Concurrent `shutdown()` cannot acquire the backend lock, so its own kill/reap deadline is unreachable.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. `audit/20260927/evidence/lsp/probe blocked-write` (exit 0). The fake backend completes initialization, acknowledges `stop-reading`, then stops consuming stdin; the probe sends a 4 MiB document and concurrently requests shutdown.
- Observed: after 35,270 ms, the check has not returned despite configured 20,000 ms convergence deadline; shutdown has not returned either. An external kill of the probe-owned child releases the check with `Broken pipe (os error 32)` and shutdown with `Ok(())`. [Confirmation log](evidence/lsp/blocked-write-confirm.log), [probe source](evidence/lsp/probe.rs), [build log](evidence/lsp/probe-build.log).
- Recommendation: **Fix** the transport/ownership path so an overall deadline or cancellation can interrupt a blocked write and reap the child without waiting on the lock held by that write. Add a stalled-reader probe; existing no-diagnostics timeout tests do not exercise pipe backpressure.
- Deduplication: blocked request writes, full-document notifications and blocked shutdown are one synchronous-write/lock root cause. The probe establishes deadline bypass beyond 35 seconds, not an observed infinite hang; child termination is explicit external cleanup, not evidence that the product deadline worked.

## R0-06 — Post-production stamping certifies bytes the index never consumed

- Category: D/A/B. Severity: P1 (stale structural answers labeled current-tree).
- Location: `crates/code-reality/src/engine.rs:1131` (document-set equality gate), `:1146`–`:1164` (hash current disk into indexed identity), called after production at `crates/code-reality/src/build.rs:720`.
- Existing defenses: staged producer outputs, exact document-set comparison, full hash recomputation on stamping, content-aware query checks, and mtime-independent identity are all present. None binds the stamp to the bytes read earlier by the producer.
- Root cause: path-set equality cannot detect a content-only edit between producer read and stamp. The later disk hash is stored as the index's identity, so the stale index and current source appear equal thereafter.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. Run `uv run --no-project python audit/20260927/evidence/data/demo_data_boundaries.py <unused-run-name>` from repo root (recorded `run5`, driver exit 0). It emits a valid SCIP fixture derived from the actual file bytes, then deterministically injects a same-path edit after production and before returning to build.
- Observed: producer read `def old_name()`, disk now contains `def new_name()`, graph still contains `old_name` and not `new_name`; `build` exits 0, `freshness` exits 0 with `fresh:true`, empty stale reasons, equal identities, `serves:"current-tree"`; `refresh` exits 0 without correcting the stale graph. Identity caching is explicitly disabled. [A observation](evidence/data/run5/A-observation.json), [freshness](evidence/data/run5/A-freshness.json), [build](evidence/data/run5/A-build.json), [refresh](evidence/data/run5/A-refresh.json).
- Recommendation: **Fix** the provenance boundary so indexed identity derives from a stable producer input snapshot or a validated before/after production identity; detected mutation must not be stamped as fresh. Add the deterministic in-production edit case to the identity battery.
- Scope: the fake producer provides controlled scheduling and a real valid index, not a claimed defect in Pyrefly's parsing. Fixtures live under this worktree and inherit its readable HEAD; no Git repository was initialized or committed. A separate no-Git control (`run3`, using `GIT_CEILING_DIRECTORIES`) cannot create an identity stamp and correctly falls back to stale legacy signals. Thus this finding specifically covers the Git-resolvable, identity-stamped path, not every build.
- Deduplication: automatic post-build stamp and later freshness/refresh trust are one unbound-provenance root cause. Full hashing and disabling the identity cache do not close this race.

## R0-07 — Missing graph.db is treated as fresh and refresh cannot recover it

- Category: A/D/B. Severity: P1 (failed first-build recovery leaves the data plane unavailable behind a fresh verdict).
- Location: `crates/code-reality/src/engine.rs:890`–`:896`; downstream `crates/code-reality/src/freshness.rs:95`, `crates/code-reality/src/refresh.rs:115`; publication/stamping precedes graph construction at `crates/code-reality/src/build.rs:720` versus `:760`.
- Existing defenses: an existing graph older than the slot forces a rebuild, even with matching source identities. `crates/code-reality/tests/source_identity.rs:321` explicitly tests that older-existing-graph case. It does not exercise missing graph metadata at this predicate.
- Root cause: failed `graph.db.metadata()` becomes `None`, and `is_some_and` returns false. The torn-plane guard consequently handles an old graph but not an absent graph, even when index/meta publication succeeded and graph construction failed.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`; same data probe command as R0-06 (recorded `run5`, driver exit 0). A directory at the temporary graph target injects a first-build graph-open failure, then the obstruction is removed before checking recovery.
- Observed: initial build exits 1 with `unable to open database file`; index/meta exist but graph does not. After removing the obstruction, freshness exits 0 (`fresh:true`, `serves:"current-tree"`), refresh exits 0 without creating the graph, and `graph_query hub` exits 2 with `graph.db 不在`. Explicit `build` subsequently exits 0 and restores the graph. [Failure](evidence/data/run5/B-build-failure.json), [freshness](evidence/data/run5/B-freshness.json), [refresh](evidence/data/run5/B-refresh.json), [missing-graph observation](evidence/data/run5/B-observation.json), [query failure](evidence/data/run5/B-query.json), [recovery control](evidence/data/run5/B-explicit-recovery.json).
- Recommendation: **Fix** missing graph detection and the corresponding recovery decision; distinguish legitimate index-only use if that remains supported. Add failed-first-build → remove obstruction → refresh → graph-query coverage alongside the existing lagging-graph case.
- Scope/deduplication: missing-graph false-fresh and failed automatic recovery are one root cause; the initial injected storage failure itself is expected and loud. No silent graph corruption is claimed. The no-Git control also leaves the graph missing after refresh (with legacy-signals instead of current-tree).

## R0-08 — Rust hover battery misparses the version and reports a skipped check as passed

- Category: B. Severity: P2 (reproduced false-green equivalence evidence).
- Location: `crates/code-reality-lsp-bridge/tests/ra_equivalence_battery.rs:56`–`:69`; frozen oracle `crates/code-reality-lsp-bridge/tests/fixtures/ra_equivalence/ra_hover_baseline.json:3`; server-info composition `crates/code-reality-lsp-bridge/src/session.rs:491`.
- Existing defenses: a frozen baseline, version compatibility guard, and opt-in `BRIDGE_STRICT_BATTERY=1` are present. The problem is the guard's parsing and the default early-return outcome, not lack of an oracle or strict mode.
- Root cause: live version uses the last space-separated token (`2026-05-25)`), while the frozen version uses the first (`1.96.0`). A matching full version therefore looks mismatched. Default execution prints `[SKIP]` and returns normally, which Rust's test runner counts as passed; ordinary capture hides that successful test's stderr.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. `rust-analyzer --version` returns `rust-analyzer 1.96.0 (ac68faa2 2026-05-25)`; the frozen `_version` is `1.96.0 (ac68faa2 2026-05-25)`.
- Reproduce default: `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp" GIT_CEILING_DIRECTORIES="$PWD/audit/20260927/tmp" cargo test --locked -p code-reality-lsp-bridge --test ra_equivalence_battery -- --nocapture` → exit 0, `[SKIP] ... live 2026-05-25) vs frozen 1.96.0`, `1 passed; 0 failed; 0 ignored`. [Default log](evidence/ra-default-battery.log).
- Reproduce strict: same command with `BRIDGE_STRICT_BATTERY=1` → exit 101, `0 passed; 1 failed`, same purported version drift. [Strict log](evidence/ra-strict-battery.log). Both exit files are preserved.
- Recommendation: **Fix** version extraction/comparison and make unavailable equivalence coverage explicit in the acceptance result rather than counting an early return as a passing check. Keep strict-mode validation as a required acceptance check where equivalence is claimed.
- Deduplication: malformed comparison and hidden pass are one gate/coverage root cause. This does not establish that hover behavior differs from the baseline: its comparisons were never reached. At least this one item in the baseline's 551 runner-reported passes is not executed equivalence evidence.

## R0-09 — Initialization rollback promises retry but leaves the session permanently dead

- Category: C/A/B. Severity: P2 (documented recovery claim contradicts reproduced state transition).
- Location: `crates/code-reality-lsp-bridge/src/session.rs:475` says killing the half-initialized backend lets the next call retry; reader EOF sets `dead` at `:390`, and `ensure_spawned` checks liveness at `:357`.
- Existing defenses: initialization errors are loud, the half-installed child is removed/killed, and ordinary backend death deliberately requires a bridge restart. Those are useful defenses; the inconsistency concerns only the initialization-rollback retry promise.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. `audit/20260927/evidence/lsp/probe retry` exits 0. [Confirmation log](evidence/lsp/retry-confirm.log), [Rust probe](evidence/lsp/probe.rs), build recipe in [README](evidence/lsp/README.md).
- Observed: first request returns `initialize handshake failed` from a reject-once backend. After observing EOF, second request on the same session returns `language server backend died ... restart the bridge to recover`, `dead=true`, `pid=None`. A new session using the same now-accepting backend succeeds. This separates stale session state from persistent backend failure.
- Recommendation: **Narrow contract claim** to restart-required behavior after failed initialization. If same-session retry is desired instead, separately authorize generation-safe recovery with reader/child ownership tests; a bare reset of the shared dead flag is not an established fix.
- Deduplication/scope: this differs from R0-04 (acceptance of cached answers after death) and R0-05 (blocked transport), and is not a general demand for automatic restart after all deaths. The initial error itself is expected. Earlier progress provisionally archived this candidate; the subsequently collected reviewer pointed out the explicit source retry promise, which warrants this C finding.

## R0-10 — Concurrent explicit graph builds unlink one another's temporary database

- Category: A/B. Severity: P2 (reproduced concurrent build failures; no final corruption observed).
- Location: `crates/code-reality/src/graph_db.rs:749` fixed `graph.db.tmp-build`, unlink at `:750`, SQLite open at `:758`, final rename at `:900`.
- Existing defenses: temporary sibling plus atomic rename protects publication to readers, SQLite has a busy timeout, and query-time healing has single-flight control. Producer staging has unique attempt names. The explicit graph-build path still shares one temporary pathname; SQLite lock waiting cannot protect a file another invocation unlinks.
- Evidence: **Reproduced**, baseline `eaa5d2462ea7821021ad0ea87403484ac0e47538`. Same data probe command as R0-06, recorded `run5` (driver exit 0). After a serial control, it starts four overlapping `target/debug/code-reality graph_db build --repo <fixture> --json` commands against the same 5,000-definition fixture.
- Observed: serial control exits 0 with 5,000 nodes. Concurrent child exits are `1, 1, 1, 0`: one `attempt to write a readonly database`, two `disk I/O error`, one successful 5,000-node build. Final graph has 5,000 nodes and `PRAGMA integrity_check` returns `ok`. [Exact command/exit records](evidence/data/run5/commands.json), [concise records](evidence/data/concurrency-summary.log), [integrity observation](evidence/data/run5/D-observation.json).
- Recommendation: **Fix** writer ownership at the graph publication boundary (serialize writers or use attempt-owned temporary files with an explicit publication policy). If concurrent explicit builds are intentionally unsupported, the alternative is an explicit, controlled single-writer contract rather than incidental SQLite filesystem errors.
- Deduplication/scope: temp-file unlink/open/rename races are one root cause. Only explicit CLI graph builds were reproduced concurrently; concurrent MCP umbrella builds were not independently exercised. This is not evidence of corrupted output, wrong-generation publication, or a guarantee that every parallel command must succeed.

## User adjudication for Round 1

Select findings by ID. R0-03 and R0-09 specifically require choosing between honest contract narrowing and a new cancellation/retry implementation. R0-10 requires choosing supported concurrent writers versus explicit single-writer behavior. Remaining fix recommendations are scoped to the reproduced predicates/paths; no changes are applied in Round 0.
