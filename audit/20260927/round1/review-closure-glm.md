# Closure-arc judge verdict — GLM-5.3 (sealed)

Bridge job: `job-mukqq28h-jma22f` (family=glm, model=GLM-5.3, mode=plan, status=completed, exit 0; sha256 efcb07088582163e30d72f8a244a39cd6f1a3d7450bcdcfa3fad1ff9351781e4). Dispatched by the ZCode main session after the Muse review; final text below is verbatim.

---

# Arbiter verdict — audit closure arc

**Bottom line: every Muse disposition is upheld; five of six findings ACCEPT outright, one (P3-3) MODIFY for a missing run-provenance record; arc verdict ACCEPT conditional on that single evidence-hygiene amendment. No code changes are required — the `GitCeilingPin` fix is sound and verified.**

All verification below was done read-only against the working tree at baseline `787d29c` (git status confirms exactly the four expected paths — `landing.md` +8, `graph_db.rs` +34, two untracked additions — nothing else).

## Per-finding verdicts

| Finding | Verdict | One-line evidence |
|---|---|---|
| P2-1 Drop guard restores prior ceiling | **ACCEPT** | `graph_db.rs:379-380` saves `var_os` before set; `:385-391` Drop restores Some/None; the outer ceiling is a real condition (`review-adjudication.md:27`: audit prefix sets `GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp`) |
| P2-2 static-mutex serialization | **ACCEPT** | `graph_db.rs:377-378` static `GUARD` with poison-tolerant `unwrap_or_else(\|e\| e.into_inner())` — shape-identical to `churn_env_guard` at `tests/build.rs:1265-1268`, verified side by side |
| P3-1 "Hermetic" comment overstatement | **ACCEPT** | `graph_db.rs:399-402` now reads "bounds ancestor discovery (GIT_DIR/GIT_WORK_TREE overrides stay in effect)" — accurate, no overclaim remains |
| P3-2 verdict rested on out-of-repo evidence | **ACCEPT** | All 22 attachments present; 9/9 `deploy-*.exit` read `0`; smoke JSON `passed=true` for both binaries at `0.9.3+787d29c`; **19/19 comparable files byte-identical (cmp) to the codex-worktree originals** |
| P3-3 "Verified both directions" without logs | **MODIFY** | Logs are attached and both show `build_stamps_snapshot_metadata ... ok / exit=0` — but nothing mechanical records *which TMPDIR each run used* (not in-log, no journal, no shell history; the leftover `audit/20260927/tmp` dir is gone). See amendment below |
| P3-4 backfill verbatim, no action | **ACCEPT** | Correct as designed: the source diff died with the turn (unverifiable by nature), and the apparent tension between `landing.md:21` ("do not yet claim deployment complete") and `landing-closure.md:60` ("complete") is explicit chronology, not contradiction — the closure doc opens by framing itself as the later readback of legs the checkpoint left collecting. Editing backfilled history would falsify it |

## Why P3-3 is MODIFY and not ACCEPT

This was my falsification leg (all-accept is a sycophancy alarm, so I actively tried to break findings — P2's mechanism and P3-2's byte-preservation both held; this one broke). The TMPDIR-in-repo log is the **only empirical demonstration that the pin works in the hostile configuration**: the audit's 574-pass integrated run had the *environment* ceiling set (`findings.md:38`), so it proves the environment ceiling, not the new pin. The in-repo direction run proves the pin — yet its run condition is attested only by the filename and narrative, in a closure record that otherwise holds itself to "all observations mechanical." The gap is exactly the class this arc exists to close.

**Required amendment (cheap, no code change):** rerun the in-repo-direction command once with provenance captured above the cargo output — at minimum `TMPDIR=$TMPDIR` plus `git -C "$TMPDIR" rev-parse --show-toplevel` (proving TMPDIR sits inside a worktree) — and attach that as the direction log (or a companion provenance note). The default-direction log needs the same one-line treatment for symmetry.

## Notes (no action required)

- **P3-2 precision nit:** "the originals live in the codex-side audit worktree" holds for 19/22 attachments; the three closure-generated files (`installed-versions.txt`, both test logs) have no worktree original — the attachment is the sole copy, and their content self-identifies origin. One clarifying clause would perfect the record; it can ride along with the P3-3 amendment.
- **Workspace-run numbers, faithfully:** the prompt narrated "55 ok blocks + one flake"; the actual artifact (`.agent-tmp/workspace-test.log`) shows **62 `test result: ok` rows and exactly one FAILED row** — the `rust_backend` binary, 3 real-rust-analyzer tests (`rust_edit_then_check_native_diagnostics`, `rust_backend_death_leaves_python_alive`, `mixed_language_sessions_are_independent`) in 38.95s under full-workspace compile load. That matches the AGENTS.md-documented cold-load flake class; the `graph_db` binary passed 10/10 inside the same full run, giving the pin a third green observation. The closure record itself makes no numeric claim about this run, so nothing in the deliverable is affected.
- **Scope check on the fix:** `git_head_sha` is asserted only in the fixed test (`graph_db.rs:418-425`); helpers create no `.git` (`:16-26`); no sibling test in the binary is TMPDIR-sensitive, and the ceiling can only affect walks passing through the pinned tempdir — so the process-global mutation window is safe for concurrent tests.

## Final arc verdict

**ACCEPT — conditional on applying the P3-3 run-provenance amendment (plus the optional P3-2 clause) before commit.** The three arc goals are met: the test now enforces its no-ancestor-git assumption instead of inheriting it from the environment, the deployment readback is formally recorded and 19/19 verifiable attachments match their originals byte-for-byte, and the checkpoint backfill preserves history verbatim. Per the judge/reviewer division of labor, the disposition record above is the deliverable; applying the amendment and landing the commit belongs to the calling session.