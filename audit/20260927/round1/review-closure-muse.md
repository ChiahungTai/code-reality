# Closure-arc code review — Muse Spark 1.3 (sealed)

Bridge job: `job-mukqfcyp-pauibi` (family=muse, model=muse-spark-1.3,
effort=xhigh, status=completed, exit 0). Dispatched by the ZCode main
session for the audit-closure arc (test hardening + landing-closure
record + checkpoint backfill). Final text below is verbatim.

---

# Code Review — audit closure arc (read-only)

Scope reviewed: `git diff` (2 files) + new untracked `audit/20260927/round1/landing-closure.md`. No files modified. No P1 (product-behavior) issues — the diff is test-only plus a docs checkpoint. Findings below are P2/P3.

Checked and **no issue**: edition 2021 `set_var` safety (workspace `Cargo.toml:6` = `edition = "2021"`, `crates/code-reality/Cargo.toml:4` inherits it — `set_var` is still a safe fn here); ceiling direction (ceiling=`tmp.path()`, fixture repo=`tmp.path()/repo` per `crates/code-reality/tests/graph_db.rs:16-26`, so the walk from the fixture hits the ceiling before any enclosing checkout); all new prose is English.

## P2

### P2-1 — Env mutation is panic-unsafe and discards a pre-existing outer ceiling
`crates/code-reality/tests/graph_db.rs:374-376`

```rust
std::env::set_var("GIT_CEILING_DIRECTORIES", tmp.path());
graph_db::build_from_cache_at(&repo, &index).unwrap();
std::env::remove_var("GIT_CEILING_DIRECTORIES");
```

- If `build_from_cache_at(...).unwrap()` panics, `remove_var` never runs; the ceiling leaks for the rest of the test-binary process.
- `remove_var` does not restore: the audit baseline runs with an outer `GIT_CEILING_DIRECTORIES=$PWD/audit/20260927/tmp` (`audit/20260927/findings.md:30`, `audit/20260927/round1/review-adjudication.md:27`, `ep.md:63`). After this test runs, that outer protection is gone for every later test in the same binary. In the audit env this test's tmpdir is a *descendant* of the outer ceiling, so overwriting is harmless for itself, but clearing is strictly wider than before.
- Fix: save `std::env::var_os` beforehand and restore it (preferably in a `Drop` guard), not bare set/remove.

### P2-2 — Process-global env mutated without the workspace's own serialization pattern
`crates/code-reality/tests/graph_db.rs:374-376`

- Cargo runs test threads in the same binary in parallel; env vars are process-global. The workspace already documents this and serializes env-mutating tests with a static mutex (`crates/code-reality/tests/build.rs:1263-1268`: "process-global; cargo runs test threads in parallel" + `churn_env_guard()` used at `:1171, :1228, :1272, :1296`).
- The new test follows only the *unguarded* precedent (`tests/build.rs:1135`, `tests/js_ts_build.rs:405`) and skips the guard pattern.
- Practical blast radius is small and worth stating: each `tests/*.rs` file is its own binary (separate process), so cross-file contamination is impossible; within the `graph_db` binary (6 tests) the ceiling value is a unique sibling tempdir, so other tests' upward walks never pass through it and tests with their own `.git` stop before the ceiling matters. The pattern is still unsound — it relies on path disjointness rather than synchronization — and a future test in this binary that depends on enclosing-repo discovery would flake depending on thread timing.
- Fix: reuse a static-`Mutex` guard (same shape as `churn_env_guard`) or mark the test serial.

## P3

### P3-1 — "Hermetic" comment overstates the guarantee
`crates/code-reality/tests/graph_db.rs:370-373`, re-asserted in `audit/20260927/round1/landing-closure.md:36-41`

- The ceiling does enforce the stated assumption (no `.git` in the `tempfile` fixture; `git -C <repo> rev-parse HEAD` in `crates/code-reality/src/common.rs:133-148` respects `GIT_CEILING_DIRECTORIES`). It does not guard `GIT_DIR` / `GIT_WORK_TREE` overrides, `safe.directory`/ownership failures, or a `.git` placed inside the fixture. "Discovery stops at the ceiling, making the fixture hermetic" should be scoped to ancestor discovery.

### P3-2 — Closure's deployment verdict rests on evidence not in scope
`audit/20260927/round1/landing-closure.md:10-32,49-54`

- `:12` "All nine `deploy-*` exit files in this directory read `0`": this checkout's `audit/20260927/round1/` contains **no** `deploy-*` files (verified `ls`; only `workspace-*.exit` plus segment `a/b/c` exits). `find` over the directory shows no `deploy-*`, `installed-smoke-*`, `plugin-deployment.json`, or `integration.exit`; `landing-evidence-paths.json` references none of those names. "In this directory" presumably means the codex-side audit worktree, not this repo snapshot — ambiguous as written.
- `:19-24` version readbacks (`0.9.3+787d29c` on six faces) and `:25-32` smoke rerun (`passed=true`, `installed-smoke-zcode-closure.json` "in the audit worktree's round1 directory") are likewise not anchored by any file in this diff. The "byte-copied snapshot … intentionally not duplicated" sentence explains the absence but leaves the reviewer with author attestation only.
- Not calling these claims false — the hash `787d29c8ff2343526614434441b86ed709980b38` does match this repo's HEAD (`git log`), and the "public distribution unchanged" scoping is correct. But as a *formal acceptance record*, the verdict "complete" (`:51-52`) is unverifiable from the reviewed artifacts. Recommend attaching the nine exit files + version/smoke logs, or scoping the claim to the external worktree path explicitly.

### P3-3 — "Verified both directions" has no in-diff log
`audit/20260927/round1/landing-closure.md:40-41`

- TMPDIR-in-worktree (previously FAILED) and default-TMPDIR both passing is asserted in prose; the diff contains the fix but no test logs. Acceptable as author attestation for a P3, but not mechanically checkable here.

### P3-4 — Backfill: no drift introduced, one inherited evidence gap
`audit/20260927/round1/landing.md:15-21` vs `audit/20260927/round1/landing-closure.md:42-44`

- "Verbatim" cannot be checked (the dying turn's text is not in scope), but internal consistency passes: the backfilled checkpoint ends with "do not yet claim deployment complete" (`landing.md:21`), which is temporally consistent with the closure later declaring completion after readback — not a contradiction.
- The backfilled paragraph re-asserts "all nine files match committed source (plugin-deployment.json)" and the whitespace gate-sequencing narrative with no in-scope evidence files. Since it is a backfill it should stay verbatim; the fix belongs in closure evidence (P3-2), not by editing the backfill. No overclaim is *introduced* by this arc beyond inheriting the prior arc's unanchored references.

## Summary

- Test fix direction is correct and fixes the reported TMPDIR-in-worktree failure mode for this fixture.
- Before calling it done, the test should (a) restore rather than remove the env var under a `Drop` guard, and (b) serialize with the workspace's existing mutex pattern.
- The deployment-acceptance verdict is coherent and correctly scoped (local only, no registry/PyPI change), but its nine-exit-file / version / smoke evidence lives outside the reviewed snapshot — attach it or name the external path explicitly.
