# Round 1 adjudication and verification ledger

Baseline: eaa5d2462ea7821021ad0ea87403484ac0e47538. Scope: all ten Round 0 findings; original locations, severity, reproduction commands and baseline evidence remain in ../findings.md. Review profile: boundary (process lifecycle, publication/provenance, consumer acceptance). Three-family discussion, local implementation, full workspace verification and independent fresh/intent review are complete. Accepted plan: ep.md. Review dispositions: review-adjudication.md. Outcome: eight fixes and two honest contract amendments; no commit, push or deployment.

| ID | Title | Decision | Status | Acceptance evidence |
|---|---|---|---|---|
| R0-01 | Exact version bootstrap rejects neighboring releases | Fix | verified locally | a/progress.md: actual wrapper matrix RED 26/8 to GREEN 34/0; both wrappers and post-install checks |
| R0-02 | MCP internal-error text shares bounded tool-output contract | Fix | verified locally | a/progress.md: cap boundaries and real stdio 1.2MB producer error, -32603, cap+marker and subsequent tools/list; parameter-validation excluded |
| R0-03 | Cancellation suppresses delivery, active work can continue | Narrow contract | amended and verified | a/progress.md: real cancel/EOF continuation, four tool descriptions and consumer docs; no cancellation/rollback implementation claimed |
| R0-04 | Known dead LSP backend cannot pass cached verification | Fix | verified locally | b/receipt.md: warm cache then backend SIGKILL rejects check; known-death acceptance gates |
| R0-05 | Stalled LSP write cannot prevent timeout and child cleanup | Fix | verified locally | b/verification.json: lifecycle8; blocked check 402ms/400ms policy, init 3005ms/3s, shutdown about8s/10s, child gone and threads joined without rescue |
| R0-06 | Stamping cannot certify post-production edits as indexed bytes | Fix | verified locally | c/progress.md and run4: producer/lock-wait source drift rejects and preserves prior bytes; manual restamp/index-swap negatives; nine publication regressions |
| R0-07 | Missing required main graph is torn and recoverable | Fix | verified locally | c/run4: injected graph failure, missing/torn, CLI refresh/recovery; alternate-slot graph-optional control |
| R0-08 | Rust hover battery executes its frozen assertions | Fix | verified locally | b/ra-final.log and workspace-test.log: two frozen assertions actually run; full-version negative controls; oracle unchanged |
| R0-09 | Failed initialization is terminal for this session | Narrow contract | amended and verified | b/receipt.md: failed init kills/reaps, same session rejects retry, new session succeeds; retry promise removed |
| R0-10 | Graph writers share controlled publication ownership | Fix | verified locally | c/run4: concurrent graph/mixed writers; integrity plus call-chain control 1flow/8memberships/1community; busy bound and independent OS-lock holder death |

## Discussion evidence and arbitration

- Muse Spark 1.3/xhigh: muse-discussion.md, job-mujox4ak-bh0g02, completed/0.
- Codex chatgpt-web/high: codex-discussion.md, job-mujox4d8-njeplb, transport web, completed/0.
- GLM-5.3: glm-discussion.md, job-mujox4r5-2wnjqb, effective model GLM-5.3, completed/0.

Main compared their suggestions with source and retained the reproduced severity/evidence boundaries. Decisions and rejected weaker alternatives are in ep.md, EP Review Findings. Agreement does not replace post-change evidence. GLM proposed bounded nonblocking write holds; main selected dedicated writer plus independent child ownership to meet the shutdown-from-entry invariant. GLM's manual-stamp trust escape and global absent-graph assumption were rejected in favor of provenance preservation and canonical-main-slot scope. Muse/GLM's zero-install negative pin suggestion was corrected: a stale pin should trigger bootstrap/wait, never immediate service.

## Retained limitations

- Endpoint source hashes do not prove an immutable producer snapshot; transient ABA edits remain outside this repair.
- MCP cancellation/EOF do not kill or roll back an active producer. No overall producer execution deadline is introduced.
- LSP death checks act on observed death; they do not prove atomic knowledge of a process dying at the exact return instant. Cached diagnostic content was not demonstrated wrong in Round 0.
- Controlled writer ownership applies to participating code-reality entry points, not arbitrary external file writers.
- Local source verification is not deployment or installed-plugin acceptance. No commit/push/deployment is included.

## Post-build review

Muse fresh+correctness and GLM intent+correctness reviews completed/exit0; review-muse.md and review-glm.md. The failed native review contributed no acceptance evidence. Both reviewers inspected source and artifacts but executed no probes; main adjudicated all observations in review-adjudication.md. One accepted prose correction explicitly excludes INVALID_PARAMS from the cap; no new reproduced defect was added to the ten-ID register. Low/static observations are archived or disclosed rather than called repaired.

Integrated execution (same isolated TMPDIR/GIT_CEILING_DIRECTORIES as baseline): cargo test --workspace --locked --no-fail-fast exit0, **574 passed / 0 failed / 0 ignored** across63 result rows; cargo build --workspace --locked exit0. Full logs and exit files are workspace-test.* and workspace-build.*. Runtime/test hashes match the independent-review identity; subsequent changes are prose only. Exact final readback is recorded in final-verification.json and progress.md.

Additional limits: INVALID_PARAMS echoes are not capped; idle reader uses2ms polling (CPU unmeasured); edit/hover compose stage budgets rather than one entry-wide budget; writer busy can return graph exit1 versus umbrella exit2; old unbound metadata remains legacy until rebuild; index validation performs a linear byte read with no large-corpus benchmark. Lock-owner death probe kills a Python flock holder, not CR mid-publication. Abrupt process death may leave attempt-owned temporary files. No outstanding user engineering decision is required for the authorized local scope.
