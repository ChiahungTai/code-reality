# Round 0 data-plane findings

Reviewer evidence only; final disposition belongs to the main collection owner.
Baseline: `eaa5d2462ea7821021ad0ea87403484ac0e47538`.
Runtime: main-built `target/debug/code-reality`, `--version` exit 0,
`0.9.3+eaa5d24`. All three findings below are reproduced at this baseline,
not attributed to an uncommitted change. Source relationships are text-derived,
not index verified. No repository graph was built/healed. No cargo commands ran.

Read first: root `AGENTS.md`, `README.md`, `crates/AGENTS.md`. Inspected the
relevant source and tests with multiple text patterns and direct body reads.
Existing test results are not claimed: main owns the baseline test run.

## Reproduction and assets

Working directory: `/Users/ctai/.codex/worktrees/5967/code-reality`.
Executed harness command (exit **0**):

```sh
uv run --no-project python audit/20260927/evidence/data/demo_data_boundaries.py
```

This completed run is `run2/`. To reproduce without overwriting it, pass a new
directory name as the script's first argument. The harness creates ordinary
fixture directories, derives SCIP definitions from fixture source, and invokes
the baseline binary. It supplies `CODE_REALITY_IDENTITY_CACHE=off` and prefixes
PATH with `run2/bin` to select its deriving fake producer. Exact absolute child
commands, exit codes, stdout, stderr, and environment overrides are in
`run2/commands.json`; concurrent children inherit the same environment.

**Git context limitation:** fixtures are inside this worktree, have no `.git`,
and inherit the parent HEAD for the tool's internal Git reads. No `git init`
or Git mutation was performed. A/B therefore have `head_drift=false` throughout.
Their conclusions concern content identity and graph absence, respectively;
neither depends on HEAD changing or a standalone fixture repository. These
probes do not establish behavior outside a Git ancestor. Main owns the separate
`build_stamps_snapshot_metadata` baseline failure investigation.

Initial `run1/` was an unsuccessful fixture attempt: its tiny producer output
hit the existing `<128 bytes` guard (build exit 2), then the harness failed to
open a nonexistent graph (exit 1). It is retained and is not finding evidence.
`run2/` uses two valid function definitions and passed that guard.

## A — Producer-time source mutation is stamped as indexed content

**Critical; confirmed by runtime reproduction.**

Source: `crates/code-reality/src/engine.rs:1131` compares document path sets;
`:1146` computes identity from current disk bytes and `:1164` stores it as
the indexed identity. `src/build.rs:687` publishes the producer output before
the stamp call at `:720`. `src/engine.rs:746` makes matching identity override
source mtime-newness in the rebuild decision.

The fixture producer reads `old_name`, emits its valid SCIP, then changes the
same source path to `new_name` before returning. The document set is unchanged.
Stamping therefore certifies the later bytes rather than the bytes represented
by the index. This is a controlled producer-read/edit interleaving, not a claim
that the real producer itself edits source.

Observed commands (absolute forms in `run2/commands.json`):

- `target/debug/code-reality build --repo <run2/A-in-build-mutation> --json`
  exits **0**, nodes=2, no stderr.
- `target/debug/code-reality freshness --repo <same> --json` exits **0**,
  `fresh=true`, `stale_reasons=[]`, `serves="current-tree"`; indexed/current
  identities both `fde44f1a507daa153c058b82bae4e4f2f676b4223eb7463390e20e5eea4d3e72`.
- `target/debug/code-reality refresh --repo <same>` exits **0**, empty output;
  graph still contains `old_name`, while source defines `new_name`.

Assets: `run2/A-observation.json`, `run2/A-build.json`,
`run2/A-freshness.json`, `run2/A-refresh.json`, and the full fixture directory
including `producer-input.txt`, current `app.py`, SCIP, meta and graph.

Existing defenses considered: Full-policy hashing prevents poisoned-cache
stamps; mismatch preservation prevents document deletion laundering. Neither
ties a same-path content stamp to producer input. Tests
`source_identity.rs:81` and `:104` check consistent stamping/cache poisoning,
`:164` checks deletion preservation; `build.rs:1170` checks churn using a
source touch and an additional document. Those inspected scenarios do not
exercise this content-changing, same-document-set interleaving.

## B — Failed first graph build becomes fresh and refresh does not recover it

**Critical; confirmed by runtime reproduction.**

Source: `crates/code-reality/src/engine.rs:890`–`:896` maps absent graph
metadata to `None` and then `false` for `graph_lags`. The index/stamp can already
exist when `src/build.rs:760` returns a graph-build error. Matching identity
then produces a fresh verdict. `src/refresh.rs:117`'s Fresh branch performs
only an optional HEAD stamp, not graph recovery.

Fault injection: an empty directory at `.code-reality/graph.db.tmp-build`
forces the initial graph open to fail. The harness removes that directory
before freshness/refresh, so the failure condition no longer exists.

- `target/debug/code-reality build --repo <run2/B-first-build-failure> --json`
  exits **1**: `graph.db 建立失敗：unable to open database file`.
- `target/debug/code-reality freshness --repo <same> --json` then exits **0**,
  `fresh=true`, `stale_reasons=[]`, `serves="current-tree"`.
- `target/debug/code-reality refresh --repo <same>` exits **0**, empty output;
  observation records `graph_exists_after_refresh=false`, `slot_exists=true`.
- Control: an explicit `build` after these observations exits **0**, nodes=2,
  proving the fixture and repaired directory permit a successful graph build.

Assets: `run2/B-build-failure.json`, `run2/B-freshness.json`,
`run2/B-refresh.json`, `run2/B-observation.json`,
`run2/B-explicit-recovery.json`. The retained B fixture is the recovered state;
the observation file records graph absence before that explicit recovery.

Existing defenses considered: unconditional torn-plane comparison catches an
existing graph older than the slot (`source_identity.rs:321` directly tests
that case). `build.rs:870` tests the half-success reporting helper after a
successful build; it does not inject a first graph failure. Producer staging
failure tests at `js_ts_build.rs:246` and `:279` protect the index before
publication. None of those inspected defenses catches the reproduced missing
graph state. This finding is specifically failed graph recovery, not a claim
that SCIP-only queries cannot work without a graph.

## D — Concurrent graph writers collide on the same temporary SQLite file

**Important; confirmed runtime failure, no demonstrated final corruption.**

Source: `crates/code-reality/src/graph_db.rs:749` uses the fixed path
`graph.db.tmp-build`; `:750` unlinks it, `:758` opens it, and `:900` renames
that shared pathname. Concurrent invocations do not own separate graph temp
names. SQLite's timeout at `:759` does not protect the unlink/name lifecycle.

Command for both control and concurrent children:
`target/debug/code-reality graph_db build --repo <run2/D-concurrent-graph> --json`.
Control exits **0**, nodes=5000. Four overlapping children give:

- child 0: **1**, `graph.db schema 建立失敗：attempt to write a readonly database`;
- child 1: **1**, `graph.db schema 建立失敗：disk I/O error`;
- child 2: **1**, `graph.db schema 建立失敗：disk I/O error`;
- child 3: **0**, nodes=5000.

Assets: `run2/commands.json` entries `D-serial-control` and `D-concurrent-*`,
`run2/D-observation.json`, fixture SCIP/source and final graph. Final graph has
5000 nodes and SQLite `integrity_check="ok"`. The supported claim is concurrent
build failure only; wrong-generation publication/corruption is not established.

Existing defenses considered: per-attempt producer staging (`build.rs:509`)
and temp+rename graph publication protect other boundaries. Heal single-flight
is tested at `build.rs:941`; the reproduction uses explicit graph build.
`graph_db.rs:170` tests sequential idempotence, while
`js_ts_build.rs:423` staggers a slow TS leg against a Python-only build and
asserts complete SCIP output. That staging test does not establish graph
writer isolation under the simultaneous graph phase reproduced here.

## Handoff limits

No source/documentation/shared-progress edits; only this evidence subtree was
written. `git diff --stat` was empty and `git status --short` showed only
`?? audit/` before this report. No further delegation. No additional candidates
or speculative fixes are proposed. Main retains collection and disposition.
