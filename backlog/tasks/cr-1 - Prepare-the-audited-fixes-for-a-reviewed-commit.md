---
id: CR-1
title: Prepare the audited fixes for a reviewed commit
status: In Progress
assignee: []
created_date: '2026-09-27 23:22'
updated_date: '2026-09-28 03:43'
labels:
  - audit
  - handoff
dependencies: []
references:
  - audit/20260927/round1/handoff/ep.md
  - audit/20260927/round1/handoff/index.html
  - audit/20260927/round1/landing.md
type: chore
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The quality audit and local remediation are complete: eight fixes and two clarified contracts, with passing workspace verification and independent review. This card lets another LLM recover the exact dirty tree and prepare a concrete commit proposal. Commit, integration and deployment need their own authorization.

```mermaid
flowchart LR
  A["Completed fixes and evidence"] --> B["Reconcile current files"]
  B --> C["Prepare commit manifest and message"]
  C --> D["Request named landing authorization"]
```

Keep the ten completed findings closed. Do not add speculative fixes or stage generated audit fixtures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Record current baseline and every final-verification.json file hash; explain all additional or changed paths without overwriting unknown work.
- [ ] #2 Produce an exact proposed commit manifest separating reviewed product files, selected durable evidence and excluded generated fixtures; preserve original audit evidence.
- [ ] #3 Provide an English commit message, successful build/test/review evidence pointers and all retained limitations.
- [ ] #4 Record effective hook side effects and exact pending landing action; execute no commit, merge, push, install or deployment without its explicit authorization.
- [ ] #5 Update card Notes with a self-contained next action and references; distinguish prepared from committed, integrated and deployed.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Baseline
Workspace: /Users/ctai/.codex/worktrees/5967/code-reality. Baseline: eaa5d2462ea7821021ad0ea87403484ac0e47538, detached HEAD with 29 reviewed product files dirty/untracked. Final content hashes: audit/20260927/round1/final-verification.json. Tests: 574 passed, 0 failed, 0 ignored; build exit 0.

## Decisions already made
Parent EP: audit/20260927/round1/ep.md, EP Review Findings. Eight fixes accepted; R0-03 and R0-09 are honest contract amendments. Review dispositions: audit/20260927/round1/review-adjudication.md. INVALID_PARAMS excluded from output cap. No new feature, speculative repair or repeated Round0 audit.

## Scope
Follow audit/20260927/round1/handoff/ep.md. Reconcile source identity, prepare exact commit manifest and English message, preserve evidence. Product source/tests/config and root/module AGENTS/consumer SKILL remain read-only; instruction synchronization already complete. Update only this card and handoff evidence. No commit, branch mutation, merge, push, install or deployment without action-specific authorization.

## Scenarios
Matching identity: reuse valid evidence. Drift or missing evidence: record exact discrepancy and verify affected scope. Generated fixture proposed for staging: exclude from manifest while retaining original locally. Missing authorization: deliver proposal and stop before outward action.

## Integration
Consumers: next LLM and user reviewing a landing proposal. Inspect effective hooks before commit authorization because post-commit may reinstall tools. Existing parent EP and final-verification.json remain authoritative. Do not stage audit/ wholesale.

## Verification
Acceptance Criteria below are the only card acceptance checklist. Use SHA256 comparison, git status/diff/show/log and linked successful command logs. Any changed product file needs delta verification/review; no gratuitous whole-suite rerun on identical bytes.

## Final Summary preview
MOCKUP - expected report structure, not executed landing: identity reconciled; exact proposed paths; commit message; reused or refreshed evidence; pending named authorization. User-visible surfaces: landing manifest, English commit message, and bounded receipt.

## Authorized continuation
User explicitly authorized commit, integration and deployment, then asked this session to finish directly. Preparation-only restrictions above are superseded for this arc by audit/20260927/round1/landing.md. Scope is local main fast-forward, local installed tool/plugin deployment, and verification; no remote push or public registry publication.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Handoff requested by user. No implementation worker remains active. Resume from the handoff EP Segment0; do not restart Round0. Card and Backlog configuration are local/uncommitted; no source changes in this planning turn. The native broad review failed quota historically; completed external reviews and later bounded docs followup are the accepted evidence.

Independent handoff EP review completed PASS across F1-F5. One stale four-versus-five prose-delta count was corrected. Plan accepted for preparation only. Board initialized with auto_commit=false, remote_operations=false and active-branch checking enabled; no AGENTS injection. Start with the exact owning worktree named in Plan.

AUTH: user said "commit／整合／部署 你可以做" and asked to finish directly. The preparation-only stop is superseded for this arc. Main owns commit, local main integration and deployment verification. Public registry publication is not inferred from local deployment.
<!-- SECTION:NOTES:END -->
