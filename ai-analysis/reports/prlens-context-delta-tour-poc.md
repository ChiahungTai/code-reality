# PRLens Context Selection → Code Reality Delta Tour POC

> Status: **POC implemented and regression-tested** (2026-09-17).  
> Scope: Code Reality producer only; South Chariot consumer/UI unchanged.  
> Source inspiration: local `/Users/ctai/Github/prlens`, especially its codebase-context selection model (co-change history, paired test discovery, context prioritization, immutable SHA pinning).

## 1. Why this investigation mattered

South Chariot / Code Reality already has a strong Code Tour foundation:

- delta tours are historical snapshots pinned to the `after` commit;
- Anchor v2 / graph-aware re-anchoring is stronger than filename+line-only navigation;
- `chain_tour` already carries namespaced `x-codeReality` metadata;
- South Chariot's `.tour` parser preserves unknown metadata, so additive producer-side experiments do not require an immediate schema/UI migration.

The remaining gap is not primarily navigation. `delta_tour` is good at answering:

> What changed, and where should I jump?

PRLens contributes a different capability:

> What nearby context is worth reading with this change?

Its most useful ideas are therefore **context selection signals**, not its review UI or GitHub integration.

## 2. What was learned from PRLens

The useful PRLens mechanisms are:

1. **Historical co-change** — files repeatedly modified in the same commits can reveal coupling not represented by imports/calls.
2. **Paired test discovery** — a likely test/spec file is often the highest-value companion to a changed production file.
3. **Immutable context basis** — all contextual evidence is fetched from the same head SHA as the reviewed diff.
4. **Evidence prioritization** — context is budgeted by usefulness rather than by blindly including nearby files.

The main design decision for Code Reality is to keep historical co-change as a **separate evidence axis** from the semantic graph. Co-change is correlation, not dependency, and should not be silently promoted into CALLS/import edges.

## 3. POC boundary

The first implementation deliberately changes only the Code Reality producer:

- file: `crates/code-reality/src/delta_tour.rs`
- tests: `crates/code-reality/tests/s5_delta_tour.rs`
- no South Chariot changes;
- no new Code Tour steps;
- no UI behavior changes;
- no change to existing A/R/M/D step-count semantics.

Context is attached additively to normal added/renamed/modified steps under:

```json
{
  "x-codeReality": {
    "context": {
      "basisRef": "<after commit>",
      "pairedTest": {},
      "cochanged": {}
    }
  }
}
```

This follows the existing `x-codeReality` precedent already used by `chain_tour`.

## 4. Metadata contract

Example shape:

```json
{
  "x-codeReality": {
    "context": {
      "basisRef": "4fed46165ec7e4b8a11673d3822128ac1d016511",
      "pairedTest": {
        "state": "available",
        "file": "pkg/test_mod.py"
      },
      "cochanged": {
        "state": "available",
        "lookback": 10,
        "sourceTouches": 5,
        "candidates": [
          {
            "file": "pkg/helper.py",
            "jointTouches": 3,
            "conditional": 0.6
          }
        ]
      }
    }
  }
}
```

`state` distinguishes evidence states instead of collapsing all failures into "nothing found":

- `available` — evidence was successfully collected and candidates exist;
- `empty` — evidence collection succeeded but found no candidate;
- `unavailable` — evidence could not be collected, with a reason preserved.

This is important because Code Reality treats unknown/unavailable as different from verified-empty.

## 5. Snapshot discipline

All context is resolved against the tour's existing `after` commit, never against the mutable working tree.

The implementation uses git object data such as:

- `git ls-tree ... <after>` for tracked files;
- `git log ... <after> -- <source>` for touching commits;
- `git show <commit>` for commit file sets.

This preserves the same historical-snapshot invariant already used by delta tours: opening an older tour should not silently project today's repository state onto yesterday's change.

### Regression proof

The synthetic test deliberately:

1. commits `pkg/test_mod.py` into the `after` snapshot;
2. records `after`;
3. deletes `pkg/test_mod.py` from the working tree without committing;
4. builds the tour;
5. asserts the metadata still resolves `pkg/test_mod.py`.

The test passes, proving paired-test selection is actually ref-pinned rather than accidentally reading disk state.

## 6. Paired-test selection

The POC supports common conventions derived from the PRLens idea:

- `test_<stem><suffix>`
- `<stem>_test<suffix>`
- `<stem>.test<suffix>`
- `<stem>.spec<suffix>`
- `<stem>_spec<suffix>`

Candidate ordering improves on PRLens's first-basename-hit behavior:

1. same directory first;
2. then greater common parent-path prefix;
3. deterministic lexical tie-break.

Files excluded by the Code Reality profile, `.kanban/`, or `.tours/` are not emitted as context.

Test files themselves do not recursively receive another paired-test suggestion.

## 7. Co-change selection

PRLens uses recent co-change frequency. The POC keeps that useful idea but exposes a normalized measure as well.

Current policy:

- inspect the most recent **10 commits touching the source file**;
- skip sweeping commits changing more than **50 files**;
- consider only files that still exist at `basisRef`;
- filter profile exclusions plus `.kanban/` / `.tours/`;
- rank by `jointTouches`, deterministic filename tie-break;
- retain top **3** candidates;
- expose:

```text
conditional = jointTouches / sourceTouches
```

Example from the regression fixture:

```text
sourceTouches = 5
helper jointTouches = 3
conditional = 3 / 5 = 0.6
```

This is intentionally still a lightweight heuristic. It is evidence of historical association, not a claim of architectural dependency.

## 8. Why metadata-first instead of companion steps

The POC intentionally does **not** convert paired tests or co-changed files into Code Tour steps yet.

Without a budget, a change with 12 primary files could quickly become dozens of contextual steps and turn the tour into a generic code browser.

Metadata-first provides a reversible boundary:

```text
Code Reality
  └─ produces contextual evidence
       └─ x-codeReality.context

South Chariot
  └─ later decides presentation
       ├─ badges
       ├─ Related Context links
       ├─ expandable panel
       └─ selectively promoted companion steps
```

This keeps evidence generation owned by Code Reality and UX policy owned by the consumer.

## 9. TDD evidence

A new regression test was added:

```text
changed_step_carries_ref_pinned_context_metadata
```

The test was first run before production implementation and failed at:

```text
context.basisRef == Null
```

That established a real RED state rather than adding a test that was already satisfied by existing behavior.

After implementation, the targeted test passed and verified:

- `basisRef == after`;
- same-directory paired test wins over another basename match;
- working-tree deletion does not affect the pinned result;
- co-change state is `available`;
- lookback is 10;
- source touch count is 5;
- the strongest candidate is `pkg/helper.py`;
- `jointTouches == 3`;
- `conditional == 0.6`.

## 10. Regression results

Focused gates:

```text
s5_delta_tour          8/8 passed
s8_tour_materialize   12/12 passed
```

Full Code Reality package gate:

```text
cargo test -p code-reality
```

passed across the full package test suite, including delta/chain tour, materialization, MCP data-plane, graph, snapshot, refresh, transition, hazard, and related regression suites.

Formatting / diff hygiene:

```text
cargo fmt --all -- --check   passed
git diff --check             passed
```

At report time, the implementation working tree contains only:

```text
M crates/code-reality/src/delta_tour.rs
M crates/code-reality/tests/s5_delta_tour.rs
```

The repository was already `main...origin/main [ahead 2]` before this POC; no commit was created as part of this work.

## 11. What should be evaluated next

The next useful step is not more implementation. It is evidence collection over real arcs.

Run the enriched `delta_tour` over roughly 10 representative work arcs and evaluate at least:

1. **Useful-novel relation rate** — how often co-change finds context that the semantic graph does not already make obvious.
2. **Noise rate** — how often candidates are formatting/bulk-edit/process artifacts rather than useful system context.
3. **Paired-test precision** — whether the selected test is actually the test a reviewer would want to inspect.
4. **Ranking quality** — whether top-1/top-3 ordering matches human judgment.
5. **Consumer usefulness** — whether metadata is useful enough to justify Related Context UI before promoting any companion into actual tour steps.

Only after this validation should South Chariot decide whether to render the metadata.

## 12. Current conclusion

The PRLens investigation produced one high-value architectural insight:

> Code Tour should combine **semantic graph relations** with a separate **historical co-change evidence axis**.

The current POC establishes the producer contract for that idea without expanding the tour, changing the consumer, or weakening snapshot provenance.

The promising path is therefore:

```text
changed file
  ├─ semantic graph evidence
  ├─ paired-test evidence
  └─ historical co-change evidence
          ↓
   attention / presentation policy
          ↓
      human Code Tour
```

This is a plausible path from a "changed-file navigator" toward a "system-understanding navigator", but the co-change signal still needs real-arc precision/noise validation before it should affect user-facing navigation.
