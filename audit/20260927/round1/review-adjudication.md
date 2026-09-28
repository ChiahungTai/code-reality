# Post-build adjudication

Baseline: eaa5d2462ea7821021ad0ea87403484ac0e47538. Reviewed source identity: review-identity.json (29 files); main SHA256 readback found zero drift before the documentation-only correction. Review profile: boundary; independent fresh and intent contexts, both with explicit correctness/process/publication checks. Structural conclusions are source-derived, not index-verified.

Muse job-mujy90px-cq8siy and GLM-5.3 job-mujy91cg-8dtjns both completed/exit0, nonempty outputs sealed in review-muse.md and review-glm.md. The native review failed quota and is not accepted evidence. Reviewer claims are advisory: Muse's header says 32 identities, but the actual manifest contains 29; GLM's opening phrase that all evidence is verified does not mean it executed runtime probes. Neither reviewer ran commands. Main owns the executed suite and worker-evidence acceptance.

| Review item | Main disposition | Evidence and boundary |
|---|---|---|
| GLM F1: uncapped INVALID_PARAMS | Accept contract narrowing; P3 theoretical path, not a reproduced defect or new Round0 ID | Read mcp_server.rs internal_error and all four parameter echoes. The latter are pre-existing and outside R0-02's reproduced tool-output path. Narrow EP and four consumer docs; do not claim a code repair for this path. |
| Muse F1: sidecar reader atomicity | Archive as pre-existing static observation | fs::write/load_meta path can expose legacy state, but no concurrent probe establishes runtime incidence. Do not inherit reviewer's broader safe-direction proof; a legacy verdict is not current-tree authority. No claim of reader-atomic sidecar publication. |
| Muse F2: MCP exit-category collapse | Archive, pre-existing and compatible | Internal error remains -32603; CLI exit distinction was never promised by MCP. |
| Muse F3: edit/hover composed budgets | Record limitation; theoretical, no fix | check/shutdown have entry budgets; edit sync-open/apply-edit and hover stages compose separate budgets. No claim of one entry-wide edit/hover deadline. |
| Muse F4: contention terminates session | Accept as documented operational tradeoff | The interaction-timeout regression deliberately proves termination/reaping; caller timeout can invalidate an otherwise healthy but blocked session. |
| Muse F5 + GLM F3: reply queue head-of-line delay | Merge and archive as within-contract static observation | Same bounded writer owns reader replies; shutdown-under-backpressure is exercised. Concurrent caller latency not separately demonstrated; fixed reply deadline is not caller's deadline. |
| Muse F6 + GLM F6: writer busy/hold/exit faces | Record limits, no defect upgrade | Five-second busy is reproduced and explicit; graph face exit1 versus umbrella Env exit2 remains. Large-corpus lock hold unmeasured. No promise every concurrent writer succeeds. |
| GLM F2: idle reader polling | Record mechanism, reject unmeasured numeric CPU estimate | transport.rs read WouldBlock sleeps 2ms. No CPU benchmark; the review's 0.1–0.5% estimate is not accepted evidence. |
| GLM F4: duplicated version negative-control literal | Archive as P3 maintenance observation | Authoritative positive battery loads unchanged frozen oracle and executes both cases; literal duplication does not create a passing skip. |
| GLM F5: retired strict env switch | Archive, intended | Version mismatch now fails in default mode; the old environment variable is no longer necessary. |
| GLM F7: old metadata migration | Retain disclosed limitation | No index-byte binding means legacy-signals until orchestrated rebuild. No old slot is newly certified current-tree. |

No reproduced new defect was reported. Original ten findings retain their original evidence/severity; secondary theoretical observations are not inflated into additional defect counts. Eight fixes and two contract amendments are the accepted local outcome.

Final followup: Gauss corrected five documentation surfaces (including the same overclaim in root AGENTS.md, within existing ownership). Erdos independently read all five cap paragraphs, EP invariant2 and mcp_server.rs cap/INVALID_PARAMS paths; returned PASS with anchors AGENTS.md:94, crates/AGENTS.md:266, README.md:116, plugin/README.md:27, consumer SKILL.md:35, EP:23. This later bounded followup completed successfully and is distinct from the earlier quota-failed broad review. No execution claim by that reviewer. Final SHA256 comparison confirms only those five prose files differ from the review snapshot; all runtime/test files remain identical. final-verification.json seals this readback and diff-check exit0.

## Integrated verification

Environment prefix: CARGO_INCREMENTAL=0 TMPDIR=$PWD/audit/20260927/tmp GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp.

- cargo test --workspace --locked --no-fail-fast: exit0; 574 passed, 0 failed, 0 ignored, summed from 63 test-result rows. workspace-test.log and workspace-test.exit.
- cargo build --workspace --locked: exit0. workspace-build.log and workspace-build.exit.
- git diff --check: exit0 before final documentation correction.
- Frozen runtime/test files: review-identity.json SHA256 comparison, zero drift before documentation correction. Final readback allows only the five named prose deltas.

The original baseline runner had 551 passing tests, including the falsely passing RA gate. The final count is not evidence of coverage by itself; the process/stdio/publication probes and real frozen RA assertions supply the boundary evidence. No installed consumer, deployment, commit, push, large-corpus performance, immutable source snapshot or arbitrary OS-hang guarantee is claimed.
