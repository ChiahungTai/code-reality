# Round 1 remediation plan

> **ep_type**: implementation
> Status: ACCEPTED by main Arbiter after Muse, Codex web/high and GLM-5.3 discussion; implementation authorized by the user's quoted request below.
> Baseline: eaa5d2462ea7821021ad0ea87403484ac0e47538
> Owner: current audit session; durable findings register: ../findings.md; progress: progress.md.

## Intent and authority

User: "Muse codex glm5.3 討論後，你在裁決直接做". Repair the accepted reproduced boundaries and make retained limitations explicit. Local source/test/document changes are authorized after discussion and adjudication. No commit, merge, push, deployment, dependency installation or live configuration change is part of this arc. Existing audit evidence remains immutable. Revert budget is the local scoped diff and disposable test assets.

## Baseline and use cases

Round 0 found ten reproducible issues, with build success and runner-reported 551 passing tests. R0-08 invalidates interpreting that count as fully exercised contracts. See ../findings.md for exact observations and reproductions; design-notes.md records main source readback and ownership.

Existing use cases affected: exact plugin bootstrap; bounded MCP error presentation; MCP job lifecycle disclosure; current LSP verification and bounded teardown; source-aware index freshness; failed-build recovery; simultaneous explicit graph builds; real Rust hover equivalence. No new product feature is proposed. Existing owners are root AGENTS.md, crates/AGENTS.md and plugin/skills/code-reality-tools/SKILL.md. There is no backlog/ directory; this report/EP owns the arc without creating an unrelated board. No SYSTEM-MAP.md was found in the audit inventory.

Memory quick pass found historical plugin uv-bootstrap work, not current completion evidence. Current source and audit probes are authoritative. No memory update is authorized. No graph index exists; positive source tracing is not a complete indexed impact analysis.

## Accepted invariants and design choices

1. A version pin accepts the exact release plus its supported build-revision suffix, not a neighboring release/prerelease.
2. Tool output and INTERNAL_ERROR text follow the same UTF-8-safe bounded payload rule. Parameter-validation errors (INVALID_PARAMS) are outside this bound. The cap is a text bound plus marker/envelope, not a strict serialized JSON frame-size guarantee. Post-build review narrowed the original overbroad wording; the reproduced R0-02 tool-output scope is unchanged.
3. MCP cancellation suppresses delivery; already-started blocking work may finish and write. Transport EOF may await it. No new cancellation/rollback mechanism.
4. Current verification must not accept a cached result after known backend death. Every stdin-write origin must be bounded, and cleanup must not need the mutex held by a stalled write. Initialization failure is terminal for that session.
5. Source identity stamped by orchestrated build must come from a validated production interval, not a fresh hash of whatever disk happens to contain afterward. Manual/head-only restamp must not certify edited source as consumed. Endpoint equality is not a snapshot/ABA guarantee.
6. A required missing main graph is torn. Alternate/projection slots retain their documented optional-graph semantics.
7. Graph writers need controlled ownership covering input acquisition, temporary database work, publication and derived materialization. A private lock-aware internal helper avoids nested acquisition from umbrella build. Existing heal single-flight is a different owner, not reused recursively.
8. A skipped version check is not a passing equivalence test. Real hovers must run against the frozen oracle; source anchor drift is repaired independently of expected values.

## Segment A — wrappers and MCP

Context: R0-01/02/03; plugin/.mcp.json, scripts/test-plugin-wrapper.sh, crates/code-reality/src/mcp_server.rs and its integration tests. Documentation owner remains main session.

Implementation sketch: replace prefix predicate with exact release/build boundary; regression-drive both wrappers including postinstall verification. Apply the existing text cap after forming the complete error message while retaining error code. Document delivery suppression and continuing blocking work in tool-facing descriptions; do not implement subprocess cancellation.

Scenarios: exact release and +rev succeed; neighboring patch and prerelease cannot serve; bad install postcondition fails; long ASCII/multibyte error preserves type and truncation marker; small errors unchanged; real stdio remains usable after failed tool. Cancellation/EOF follow the disclosed retained behavior.

Acceptance: targeted plugin shell/Cargo integration; MCP cap unit boundary cases plus real stdio large-error probe; cancellation/EOF bounded historical probe retained and compared. New behavioral tests first fail on baseline, then pass. No new live installs.

## Segment B — LSP lifecycle and evidence

Context: R0-04/05/08/09; crates/code-reality-lsp-bridge source/tests/README. Shared root documentation main-owned. If libc is necessary, use existing workspace dependency and coordinate Cargo.lock.

Implementation sketch: fail dead checks at entry and before success acceptance; separate child cleanup ownership from serialized pipe writes. Use a coherent bounded transport (deadline-aware nonblocking writes or bounded dedicated writer with independent child kill); cover requests, notifications, initialization and server-request replies. All timeout/partial-frame paths invalidate the session and reap its child. Shutdown clock starts at API entry and cannot wait indefinitely for interaction/writer mutexes. Avoid detached unbounded writer threads. Preserve existing overlay and diagnostic convergence policy. Initialization rollback becomes deterministic terminal/restart-required; no unsafe dead-flag reset. Fix version comparison and initialization trigger in the RA battery; derive hover positions from declaration anchors if framing moves, preserve frozen expected content.

Scenarios: warm cache then backend death errors; initialized child stops reading and receives multi-MiB document; check returns bounded error, concurrent shutdown completes/reaps without external rescue; quiet responsive backend still follows convergence policy; handshake rejection stays dead, a fresh session can succeed; server-originated request replies remain framed; normal Python/Rust/TS behavior unchanged; matching RA executes both frozen cases and unsupported version cannot count pass.

Acceptance: deterministic subprocess regressions (not mocked write_all), wall-clock values and child state recorded, then bridge suite and real RA strict battery. Distinguish transport-write timeout, diagnostic-convergence deadline and shutdown deadline explicitly. Do not claim arbitrary OS/filesystem/kernel hangs are bounded by an application clock.

## Segment C — identity, recovery and writer ownership

Context: R0-06/07/10; code-reality build.rs, engine.rs, graph_db.rs, associated identity/cache plumbing and tests. Root/module/plugin documentation main-owned.

Implementation sketch: capture the governed source identity and policy before producers; compare after production before publication and pass validated provenance into stamp rather than recomputing an authoritative value late. Preserve prior slot on detected production drift. Bind retained provenance to the correct index; manual restamp must preserve valid prior identity or remain unverifiable, never launder content-only drift. Mark missing canonical required graph torn, preserving alternate-slot semantics. Serialize writers with bounded/control-plane busy outcomes and process-death-safe ownership; cover full graph pipeline, including derived tables and cache acquisition. Keep lock order explicit: heal ownership -> data-plane writer; graph helper called by umbrella build must not acquire the same lock twice. Concurrent live source edits remain outside immutable-snapshot guarantees.

Scenarios: stable build fresh; same-path producer-time mutation rejected and old index/meta/graph preserved; postbuild edit+manual stamp stays stale; fresh manual/index-only provenance stays honest; first graph failure -> missing/torn -> refresh recovers; projection optional graph unaffected; same-process and multi-process concurrent writers either serialize success or controlled busy (no incidental SQLite unlink errors); lock released after owner exit; final graph integrity and derived tables valid.

Acceptance: regression tests plus independently adapted Round 0 real CLI probes with outputs under round1/evidence. Failure injection should target the publication boundary rather than rely forever on a specific private temporary filename. Complete affected identity/build/refresh/graph tests before full workspace suite.

## Verification and completion

DEPTH-MIN -> affected suites -> full workspace: CARGO_INCREMENTAL=0, TMPDIR and GIT_CEILING_DIRECTORIES both audit/20260927/tmp. Main owns final full build/test run after writer collection. Do not cargo fmt unrelated files. Independent fresh review plus intent-aware review checks accepted decisions, boundaries and residual limitations; findings are adjudicated and any accepted correction reverified. Root/module/plugin contract descriptions get a consistency pass; no claimed deployment.

Final receipt links decisions, implementation evidence and remaining limitations, with all ten IDs reconciled. No user decision should be requested for routine choices delegated by the user. Material unverified/blocking facts are reported candidly.

## EP Review Findings

| Concern | Decision and incorporated resolution | Status |
|---|---|---|
| R0-01/02 mechanical gates | Adopt exact pin boundary and combined/final error cap; bad main pins should attempt bootstrap, not require zero installs as two reviewers suggested. Existing wrapper's purpose is automatic repair. | verified |
| R0-03 cancellation | Adopt truthful continuing-work/possible-publication and EOF waiting contract; no cancellation/rollback feature. | verified |
| R0-04/09 terminal backend | Adopt entry plus acceptance liveness gates; initialization failure synchronously marks dead, kills/reaps; same-session retry remains unsupported. | verified |
| R0-05 transport choice | Choose dedicated serialized writer with acknowledgement and separate child ownership (Muse/Codex), not GLM's bounded lock-hold approximation. Absolute request/check deadlines include write/lock work; shutdown total budget starts at entry, can kill without writer/interaction locks, reaps and joins transport thread. Bounded queue/no detached blocked writer. Reader replies use same writer path. Preserve framing helper semantics. | verified |
| R0-06 provenance | Capture full uncached identity before production; compare before publication and under writer ownership; stamp captured identity A. Manual/head-sync only preserve valid prior identity bound to index, never hash edited disk into authoritative old-index metadata. Reject GLM's optional/manual trust escape: Codex's negative probe is required. ABA remains disclosed. | verified |
| R0-07 graph role | Missing canonical main graph is torn; alternate/projected slots excluded. Reject global absence rule despite GLM's negative claim; projection face exists in root contract. | verified |
| R0-08 battery | Full version identity, actual initialization/hover not shutdown, hard fail on version mismatch, unchanged frozen oracle. | verified |
| R0-10 writers | Shared data-plane publication ownership across umbrella publication and explicit graph build; bounded controlled busy; lock-aware helper prevents recursion. Derived materialization before database rename (GLM observation accepted as part of publication correctness). Unique temp owns its cleanup. | verified |

Evidence: muse-discussion.md (job-mujox4ak-bh0g02), codex-discussion.md (job-mujox4d8-njeplb, transport=web), glm-discussion.md (job-mujox4r5-2wnjqb, effectiveModel=GLM-5.3). All status=completed, exit=0, nonempty full exported text. Initial failed dispatch attempts remain in progress.md. These source-aware discussions reviewed all ten IDs and the proposed design alternatives; main source readback supplied additional writer/derived-materialization integration analysis. No pending user decision remains for this local scope.
