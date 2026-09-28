# Adjudication note — ZCode main session (2026-09-27 06:5x)

Written by the ZCode main session after the Round-0 auditor (codex session
`01a0dfe2-ae98-7762-998b-216b2212fd03`) and both reviewer subagents died on
`usage_limit_exceeded` at ~06:48 (quota resumes 11:10 AM). Purpose: prevent
re-hunting on resume. Evidence below was independently reproduced in the MAIN
repo (`/Users/ctai/Github/code-reality`, same baseline `eaa5d24`, clean tree).

## 1. R0-01 (prefix-match pin) — CONFIRMED, stands as written

`plugin/.mcp.json:6` and `:12` both use `case "$("$ub/$x" --version ...)" in
"$want"*)` — glob prefix match. Pin `0.9.3` accepts `0.9.30+rev` /
`0.9.3-rc1+rev`. Textually confirmed in the main repo; repro assets in
`evidence/wrapper-pin/`. Disposition decision (fix predicate vs narrow
contract) stays with the user.

## 2. Baseline `cargo test` exit 101 — NOT a product red; audit-environment artifact

Failing test: `build_stamps_snapshot_metadata`
(`crates/code-reality/tests/graph_db.rs:389`, `git_head_sha` count 1 ≠ 0).

Root cause (mechanically proven both directions in the main repo):

- The auditor ran `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp"
  cargo test --workspace --locked` (rollout tool-call #15) — TMPDIR was
  redirected into the repo to satisfy the workorder's temp-file routing rule.
- `tempfile::tempdir()` then lands inside the git repo, so the stamping call
  `git -C <tmp>/repo rev-parse HEAD` (`common.rs:133`, discovery walks UP)
  finds the worktree HEAD and stamps `git_head_sha` → assert fails.
- Main repo, default TMPDIR → `ok. 1 passed`. Main repo,
  `TMPDIR="$PWD/.agent-tmp/tmpred"` → identical FAILED.

Residual genuine finding (new, suggest R0-02, P3 test-robustness):

- `build_stamps_snapshot_metadata` silently depends on TMPDIR not living
  inside a git repo. Fix candidate: set `GIT_CEILING_DIRECTORIES` (or an
  explicit `GIT_DIR` pointing nowhere) inside the test, so the non-git-repo
  assumption is enforced rather than inherited from the environment.
- Process-side note: the workorder's "暫存集中 audit/<日期>/tmp" rule should
  not be applied as a TMPDIR export for `cargo test` runs (command temp files
  ≠ test tempdirs).

## 3. State of the in-flight probes at death

- `evidence/mcp-lifecycle/results.json`: COMPLETE (cancel / eof /
  large-error, exit 0; eof exit ~3.05s after EOF). progress.md's "in
  progress" line is stale relative to this evidence.
- `evidence/wrapper-pin/`: complete (6 cases, results.json + log).
- `evidence/lsp/` (Archimedes) and `evidence/data/` (Meitner): partial
  probes only; both subagents errored on the same usage limit before
  reporting findings. These two legs are the main unfinished work.

## Resume instruction (per the workorder's own resume clause)

Read findings.md + progress.md, skip R0-01 (done), record item 2 above as
adjudicated (do NOT re-hunt the baseline failure; optionally register the
P3 test-robustness finding), then re-dispatch / continue the LSP and
data-plane reviewer legs from their partial evidence, then emit the bounded
receipt.
