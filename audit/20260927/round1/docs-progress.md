# Round 1 bounded documentation synchronization

Intermediate checkpoint. Owner: segment A worker, reassigned by main to five documentation files only: root AGENTS.md, crates/AGENTS.md, plugin/skills/code-reality-tools/SKILL.md, README.md, plugin/README.md. No source/test changes, commits, installs or delegation.

Instruction-writing skill read before edits. Accepted EP owns the contracts; current B/C source is still changing. This receipt records source alignment only, not runtime acceptance or deployment. Main owns independent review and integrated acceptance.

Source observations:
- engine.rs: ProductionIdentity::capture uses disabled identity cache; stamp_meta_locked uses validated A plus index_sha256, or preserves matching prior provenance only. evaluate_staleness treats missing canonical graph as torn and alternate slots as graph-optional.
- build.rs: captures A before producers, compares post-production and again under WriterGuard, passes A to stamp_meta_locked; no immutable snapshot/ABA guarantee.
- publication_writer.rs: OS-owned shared writer lock, bounded controlled busy; graph_db.rs builds derived data before temp DB rename; explicit graph input acquisition shares ownership.
- bridge session.rs/transport.rs/server.rs: terminal dead/failed-init state, dedicated acknowledged writer, check deadline from entry, independent child cleanup and total shutdown budget.

Unresolved until final source readback: exact timeout constants/names and any B/C review changes to binding/comparability or writer acquisition boundaries. Current source values are not yet advertised as runtime-verified. Preserve main's README cancellation/cap paragraph and plugin README exact-pin paragraph.

Next: remove directly related stale claims, add scoped consumer limits, then re-read B/C completion/source and reconcile exact anchors.

## Draft edits completed; final B/C readback pending

- Root AGENTS: exact pin preserved; graph-build row now states shared ownership/derived-before-rename; affected one-shot build row authored in English, replacing the prior Chinese row only; LSP row adds terminal/current-check/deadline contract; self-heal and freshness rows remove trusted manual recomputation and add index binding/missing-main-graph limits.
- crates/AGENTS: transport owner/lifecycle, production observations and manual stamp distinction, graph writer owner and order, MCP cap/cancellation, strict RA oracle version/declaration anchors.
- Consumer SKILL: MCP cap/cancellation/EOF; LSP lifecycle; manual data-availability distinction; index-bound A/fences, legacy-unbound metadata, graph-optional alternate slots, shared writer ownership and residual cache/ABA limits.
- README: main's MCP paragraph preserved; manual-chain equivalence narrowed to availability; provenance and publication limitations added.
- plugin/README: main's exact-pin paragraph preserved; bounded LSP lifecycle and matching MCP cancellation/cap paragraph added.

Evidence anchors are captured in `docs-source-anchors.txt`; the inspected source-byte snapshot is `docs-source-hashes.txt`. These are read-only source observations, not test evidence. B/C workers still report intermediate runs; exact timeout values/names remain omitted from consumer prose until final readback. No new runtime-verification claim added.

Validation so far: scoped `git diff --check` exit 0. One initial Perl edit expression failed to parse (exit 255, no edit); corrected delimiter and re-read result. All successful edits are within the five owned docs and this audit directory. No instruction policy redesign/version bump or unrelated language-row translation.

Receipt: classification=boundary contract synchronization; review=main-owned pending final fresh/intent review; session-freshness=current source readback in progress; deployment-surfaces=N/A (no landing/deploy authorized).

## Source-aligned documentation handoff

The five-file bounded edit is complete against the current source snapshot. Re-read detected concurrent engine/build changes (hash check exit 1 for those two files); these were comment/remediation-guidance changes consistent with the documented contracts. Refreshed anchors/hashes after readback. The final scoped `git diff --check` exited 0. No executable changes were made during this docs task, so no new runtime tests were run or claimed.

Timeout owner names now resolved in crates/AGENTS: `REQUEST_TIMEOUT`, `HANDSHAKE_TIMEOUT`, `LangSpec.slow_timeout_ms`, `check_file_impl`, and `LspSession::shutdown`. Current source observations: request/handshake constants 30s, check family budgets Python 20s and Rust/TS 30s, shutdown total 10s with 2s reserved for force cleanup. Consumer prose deliberately references entry-based contracts rather than duplicating those tunables. These numbers are source facts, not new runtime acceptance claims.

Unresolved acceptance fields: B affected-suite completion and C final probes/affected suites/final source receipt were still pending at this handoff. Main must compare any later semantic changes against these docs before landing. No unresolved wording choice blocks this source-aligned edit; no claim that the still-running implementations are runtime-verified. Final independent review and deployment remain outside this worker's scope.

Main's original README MCP lifecycle/cap paragraph and plugin README exact-pin paragraph remain intact. Root changed build row is English; unrelated Chinese rows were not translated. No files owned by B/C were modified.

## Accepted postbuild cap-contract narrowing

Read current README.md, plugin/README.md, consumer SKILL.md and crates/AGENTS.md cap passages plus mcp_server.rs. Source anchors: internal_error at 236-237 caps INTERNAL_ERROR; tool-output path at 240-246 caps output; INVALID_PARAMS examples at 446-450, 600-604, 639-643 and 842-846 construct uncapped validation echoes.

Changed those four documents to state that tool output and internal-error text are capped, while parameter-validation errors are outside the bound. Also corrected the identical overclaim in root AGENTS.md's Unified MCP interface row, within existing docs ownership. Preserved lifecycle/exact-pin wording and all runtime/test files. Main owns the audit EP amendment.

Validation: `git diff --check -- README.md plugin/README.md plugin/skills/code-reality-tools/SKILL.md crates/AGENTS.md AGENTS.md` exited 0. Readback found the explicit parameter-validation exclusion in all five files. Docs-only: zero runtime tests run, no new runtime claims. No temporary files, installs, commits or delegation. Edits frozen after this receipt.
