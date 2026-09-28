# Round 0 progress

- Calibration: baseline eaa5d2462ea7821021ad0ea87403484ac0e47538, initial git status --short empty (clean). Workspace version 0.9.3. rustc 1.96.0 (ac68faa20 2026-05-25), cargo 1.96.0 (30a34c682 2026-05-25).
- Read root AGENTS.md (provided), README.md, crates/AGENTS.md and bridge README. No STATE.md / standalone architecture document surfaced in the repository file inventory.
- Existing defenses: staged multi-producer publication; corpus-policy/source-identity freshness; torn-plane detection; fail-loud unsupported language faces; LSP per-family lifecycle, interaction serialization, overlay retention and diagnostic version/time/quiescence convergence; pinned bootstrap postcondition checks.
- Structural evidence degraded: no .code-reality/graph.db in this checkout. No index rebuild authorized outside audit directory. Source reads, text-derived references and runtime probes will be labeled accordingly; no claims of complete dynamic-consumer coverage.
- In progress: baseline cargo test/build, bounded lifecycle and data-plane investigation.
- Blockers: none yet.
- Next: collect baseline exit codes/counts, then reproduce candidate findings.

## Dispatch checkpoint

- Local owner: baseline builds/tests, MCP transport/cancellation and plugin bootstrap contract probes.
- LSP reviewer handle: 01a0dfe4-2e0a-7550-b23b-4239c30ba305 (Archimedes), scope bridge lifecycle/diagnostic evidence; sink evidence/lsp/.
- Data-plane reviewer handle: 01a0dfe4-2e93-7700-8584-ea6f23745bb7 (Meitner), scope staged publication/freshness; sink evidence/data/.
- Both use native explorer / inherit model and effort, Reviewer/findings authority, no further delegation, no production edits. Parent owns collection via these handles. CR surface unavailable for this checkout (missing index); read-only source/text fallback declared.
- Baseline cargo test running, process session 51876; log evidence/cargo-test-workspace.log. All command temp files routed into audit/20260927/tmp.

## Checkpoint: R0-01 complete

- Accepted R0-01 (P2, B/D): both real bootstrap wrappers accept 0.9.30 and 0.9.3-rc1 under the 0.9.3 pin. Reproduced with isolated executables; no actual installations performed.
- Evidence: evidence/demo_wrapper_pin.py, evidence/wrapper-pin/results.json, evidence/wrapper-pin.log. Findings register/detail appended.
- Baseline: compilation reached test execution; binaries now available in target/debug/.
- In progress: MCP cancellation/disconnection probe; independent LSP and data-plane reviews.
- Blockers: none. Next: collect baseline tests and run live stdio MCP probe.

## Resume / R0-02 and R0-03 complete

- Both independent reviewers stopped with authoritative usage-limit errors. Their evidence assets survived; completion reports did not. Resume was attempted once and returned the same error; no silent replacement or further redispatch. Parent will validate and integrate their surviving probes; independent final review remains incomplete.
- Accepted R0-02 (P2 B/C): MCP nonzero-exit errors bypass 1 MiB text cap; actual response 1,200,151 bytes, no truncation marker.
- Accepted R0-03 (P2 A/B): real MCP cancellation and EOF leave external producer running. Recommendation is contract amendment; no claim of indefinite hang or rollback failure.
- Baseline first invocation exited 101 at graph_db::build_stamps_snapshot_metadata, 1 vs 0 git_head_sha rows. Root cause: audit-local TMPDIR sits inside a Git worktree; Git discovers its parent. This is an audit-environment artifact, not accepted as a product defect. Focused control with GIT_CEILING_DIRECTORIES at the audit tmp root passed (1 test).
- Full baseline continuing with explicit Git discovery ceiling and --no-fail-fast, session 50138; evidence/cargo-test-isolated.log. This preserves the permitted write root while isolating fixture Git discovery.
- Next: validate surviving LSP/data probes and collect baseline counts.

## Checkpoint: R0-04 and R0-05 complete

- Accepted R0-04 (P1 A/D/B): dead-backend warm-cache check still returns successful count=0; independently re-ran direct library and real MCP probes, both exit 0.
- Accepted R0-05 (P1 A/B): stalled LSP stdin blocks check and shutdown beyond 35 seconds; external child kill releases both. Re-ran with exit 0. This is deadline bypass, not a claim of infinite observed hang.
- Recompiled the preserved LSP Rust probe against baseline libraries, exit 0; exact command and assets documented in evidence/lsp/README.md.
- Initialization retry behavior archived as an observation, not a defect: existing restart-bridge instruction is explicit.
- In progress: data-plane stamp/recovery evidence and full test baseline. No production changes.

## Checkpoint: R0-06 and R0-07 complete

- Accepted R0-06 (P1 D/A/B): source changes after producer read but before stamp; graph old_name, disk new_name, freshness current-tree, refresh no-op. Repeat run5 confirms with identity cache off. No-Git run3 is a negative control because stamp requires Git; report limits the claim to the identity-stamped path.
- Accepted R0-07 (P1 A/D/B): failed first graph build leaves published index/meta, missing graph is not torn, refresh no-op, graph query exit 2; explicit build recovers. Run5 adds the consumer query before recovery.
- Full baseline runner reports 551 passed / 0 failed / 0 ignored across 60 result blocks, exit 0; separate workspace build exit 0. Counts saved in evidence/baseline-counts.json.
- Additional evidence check: ra_equivalence_battery has a version-drift early return that Cargo may count as passed. Running its existing BRIDGE_STRICT_BATTERY=1 mode before interpreting the reported pass total as actual battery coverage.
- Remaining: collect strict-battery result, final limitations/coverage and report consistency. Sources remain unchanged.

## Checkpoint: R0-08 complete

- Accepted R0-08 (P2 B): version guard compares live trailing date against frozen leading semver; identical full versions still skip. Default --nocapture confirms [SKIP] plus 1 passed/0 ignored, exit 0. Strict mode fails, exit 101. Versions recorded in evidence/version-confirmation.txt.
- Corrected baseline interpretation: 551 is Cargo's pass count, not proof all planned assertions ran; at least one is a verified early-return skip. No claim of full equivalence validation.
- Findings now R0-01 through R0-08, four P1 and four P2. No P3 padding; no theoretical risk promoted to defect.
- Remaining: final report calibration/coverage, asset and scope verification, terminal agent collection.

## Final checkpoint — Round 0 complete

- Both resumed reviewer handles have now returned completed reports (evidence/lsp/findings.md and evidence/data/findings.md). Earlier usage-limit status is superseded; parent collected both terminal results. No missing independent report remains.
- Accepted R0-09 (P2 C/A/B) after source review exposed the explicit initialization next-call retry promise; main repeated retry probe, exit 0. This supersedes the provisional archived observation above. Recommended contract narrowing, not assumed general restart support.
- Accepted R0-10 (P2 A/B): concurrent explicit graph writers share/unlink one temp pathname; serial succeeds, concurrent exits 1/1/1/0, final integrity is ok. No corruption claim.
- Severity reconciliation: R0-04 reduced from provisional P1 to P2 because false-success/death nondisclosure is reproduced but incorrect diagnostics are not. Final total: 10 findings, 3 P1, 7 P2, 0 P3.
- Report includes baseline commands/results, existing defenses, evidence limitations, all reproduction paths, and Round 1 decisions. Automated report check found 10 matching IDs, zero broken artifact links and zero invalid source line anchors (evidence/report-check.json).
- Final git HEAD is still eaa5d2462ea7821021ad0ea87403484ac0e47538. Both unstaged and staged tracked diffs empty; status only ?? audit/. Probe/process inventory found no remaining owned audit child or rust-analyzer process. Build/test artifacts retained per authorization.
- Completed scope: A/B/C/D calibrated, probed and deduplicated. Blockers: none for this bounded Round 0. Pending user adjudication: select Round 1 IDs; cancellation/retry semantics (R0-03/R0-09), concurrent-writer policy (R0-10), and acceptance of remaining fix recommendations. No fixes or outward actions performed.
- Resume point: read findings.md and this final checkpoint; do not rescan completed findings. Continue only the user-selected Round 1 scope.
