# Implementation EP: JS/TS freshness, doc-set convergence, and refresh

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S4 — Extend source freshness, doc-set convergence, producer drift, and refresh to JS/TS
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **depends on**: S1 language faces; S2 governed JS/TS corpus; S3 only for final M1 graph acceptance
> **status**: ready-for-implementation

## Implementation overview

Make JS/TS participate in the same query-time self-heal and post-commit refresh lifecycle as Python/Rust, while fixing a pre-existing structural blind spot that becomes an explicit parent acceptance requirement: source deletion/rename cannot be detected reliably from "newest source mtime > index mtime" alone.

The current Stage-A freshness check walks source files and compares only the newest surviving source mtime. A deleted file has no mtime to compare. `doc_set_delta` can detect an indexed file that disappeared, but today it runs only **after a rebuild**; therefore it cannot trigger the rebuild. Post-commit `refresh` sees head drift, calls `ensure_fresh`, gets `Fresh` when no surviving source is newer, and may merely restamp HEAD. That can preserve a deleted document indefinitely.

This EP adds a face-scoped source-set fingerprint to sidecar metadata. Stage A compares the current source-set fingerprint with the stamped fingerprint without parsing SCIP on every query. Add/delete/rename/profile-policy changes can therefore trigger exactly one heal. Existing old metadata remains readable through a compatibility fallback, and current Python/Rust source inclusion rules remain unchanged.

## UC inventory

### Capability updated

**Main-index query-time self-heal + commit-granularity refresh** gains JavaScript/TypeScript and a truthful doc-set drift trigger.

### Invariants

- A source edit/add/delete/rename in an active indexed face eventually produces one converged index or one explicit stale-serving warning; it cannot loop forever.
- A profile-excluded JS/TS file does not trigger freshness.
- Changing the profile policy that defines the JS/TS corpus **does** trigger freshness even if no source file mtime changes.
- An explicit partial producer face is compared only with the faces actually stamped in that index; unrelated source languages do not force self-heal into a broader face.
- Old sidecar metadata without new fingerprint fields remains usable and falls back to baseline freshness behavior until rebuilt/restamped.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | JS edit | modify `.js/.mjs/.cjs/.jsx` | `source_newer` triggers heal | one rebuild |
| SM-2 | TS edit | modify `.ts/.tsx` | same | one rebuild |
| SM-3 | Add source | new JS/TS file | source-set fingerprint differs even if mtimes are awkward | heal + converged doc set |
| SM-4 | Delete source | remove indexed JS/TS file | doc-set fingerprint differs; rebuild removes doc | no stale extra doc |
| SM-5 | Rename source | old→new path | fingerprint differs; rebuilt index has only new path | no duplicate old path |
| SM-6 | Excluded generated edit | change `dist/x.mjs` excluded by profile | no staleness | zero heal |
| SM-7 | Profile change | add/remove `dist/` exclusion | corpus-policy/doc-set fingerprint changes | heal even without source edit |
| SM-8 | Explicit Python-only slot in mixed repo | newer `.ts` file | JS/TS does not stale a Python-only stamped face | face isolation |
| SM-9 | Three-family auto slot | edit any active face | heal selects all detected active producers | S1 atomic build |
| SM-10 | Producer failure during heal | JS/TS tool unavailable | serve prior usable slot with warning | no loop/no torn slot |
| SM-11 | Rebuild still mismatches corpus | filtered producer omits governed doc | warn once + cooldown/stale; do not re-heal loop | missing/extra reported |
| SM-12 | Concurrent healer | two query processes | one build, peer waits/reuses existing lock contract | single-flight |
| SM-13 | Old metadata | no fingerprint keys | baseline mtime/head behavior; no crash | compatibility |
| SM-14 | JS producer version drift | stamped vs installed `scip-typescript` | flagged path warns; drift alone does not create steady-state spawn | version note |
| SM-15 | Docs-only commit | no source/doc-set/policy drift | refresh restamps head only | no producer run |

## Segment 0 — research and design constraints

### Current source anchors

- `crates/code-reality/src/engine.rs:421` — `SourceWalk { py, rs, newest }`.
- `crates/code-reality/src/engine.rs:430` — one-pass source walk handles only `.py/.rs` and special-cases Rust under `target/`.
- `crates/code-reality/src/engine.rs:481` — `StalenessSnapshot` has only `source_newer` + `head_drift`.
- `crates/code-reality/src/engine.rs:489` — Stage A sets `source_newer = walk.newest > slot.mtime`.
- `crates/code-reality/src/engine.rs:525` — `doc_set_delta` is face-scoped only for `.py/.rs` and can calculate indexed-extra/disk-missing paths.
- `crates/code-reality/src/build.rs:477` — producer drift note parses only the pyrefly segment.
- `crates/code-reality/src/build.rs:588` — healer rebuild/convergence path.
- `crates/code-reality/src/build.rs:620` — post-rebuild doc-set delta is calculated, but baseline only treats `missing > 0` as non-converged.
- `crates/code-reality/src/build.rs:708` — `ensure_fresh` returns `Fresh` whenever `source_newer` is false.
- `crates/code-reality/src/refresh.rs:101` — refresh snapshots freshness, then delegates to `ensure_fresh`.
- `crates/code-reality/src/refresh.rs:116` — if `ensure_fresh` says Fresh and HEAD drifted, refresh restamps without rebuilding.

### Newly confirmed baseline blind spot

The existing code path cannot use `doc_set_delta.extra` to trigger a rebuild after a deletion because `doc_set_delta` runs only after a heal was already selected. This is a source-derived finding, not a claim that an existing production incident occurred.

### POC evidence absorbed

- S2 POC: the governed acceptance source set exactly matches 28 SCIP docs after profile exclusion.
- S2 POC: explicit-files config can use the exact same governed set; there is no need for a second TypeScript glob interpretation.
- S1 POC: N-way SCIP merge is viable; S4 can call the standard S1 rebuild instead of inventing incremental merge logic.

### Architecture decisions

1. Preserve Stage-A's zero-SCIP-parse property on the normal query path.
2. Promote S2's JS/TS inclusion predicate into the one-pass source walk rather than performing two filesystem walks per query.
3. Stamp enough sidecar metadata to know the active source faces and source-set fingerprint.
4. Fingerprint **paths**, not mtimes/content. Content changes are already covered by `source_newer`; path fingerprint specifically covers add/delete/rename and profile corpus changes.
5. Profile semantics apply to JS/TS in this arc because S2 filters that producer corpus. Python/Rust keep current corpus rules; do not suddenly apply `.code-reality.toml` exclusions to producer faces that do not currently consume them.
6. Treat both `missing` and `extra` doc-set delta as non-convergence after rebuild.

## Segment 1 — one-pass four-language source inventory

### Context

The steady-state query path must not double-walk the repository. Refactor S2's governed JS/TS predicate into the existing source walk so build and freshness still share one inclusion rule.

### Data shape

```rust
struct SourceWalk {
    py: BTreeSet<String>,
    rs: BTreeSet<String>,
    js: BTreeSet<String>,
    ts: BTreeSet<String>,
    newest_by_face: BTreeMap<LanguageFace, SystemTime>,
}

impl SourceWalk {
    fn paths_for_faces(&self, faces: &BTreeSet<LanguageFace>) -> BTreeSet<&String>;
    fn newest_for_faces(&self, faces: &BTreeSet<LanguageFace>) -> Option<SystemTime>;
    fn fingerprint_for_faces(&self, faces: &BTreeSet<LanguageFace>) -> String;
}
```

One filesystem traversal applies:

- baseline dot-dir / `SKIP_DIRS` rules;
- baseline `target/` rule for Rust;
- S1 extension mapping;
- S2/profile exclusion **only** when language is JavaScript or TypeScript.

S2 producer switches from its standalone walk implementation to `walk_sources(...).js + .ts` or an extracted shared visitor with equivalent one-pass behavior. There must be exactly one JS/TS inclusion predicate after this refactor.

### Fingerprint

Use a deterministic hash over sorted normalized relative paths plus face identifiers, e.g.:

```text
JavaScript\0scripts/a.mjs\0
TypeScript\0src/b.ts\0
```

The exact hash algorithm should reuse an existing project hash helper if present; otherwise choose a stable standard-library/project dependency already in use. The hash is an equality detector, not a security boundary.

### Validation

- One-pass set equality for Python/Rust baseline fixtures.
- Six JS/TS extensions.
- Excluded generated JS/TS never enters set or newest signal.
- Stable fingerprint independent of directory iteration order.

## Segment 2 — stamped active faces, doc-set fingerprint, and profile policy fingerprint

### Context

Stage A needs to compare the current disk corpus with what the index intended to represent without parsing the index each query.

### Metadata additions

At successful build/stamp time, record additive sidecar metadata fields:

```json
{
  "source_faces": ["python", "rust", "javascript", "typescript"],
  "source_set_fingerprint": "...",
  "js_ts_profile_fingerprint": "... or <none>"
}
```

Rules:

- `source_faces` is derived from the **published index documents** or the final selected face contract, so explicit `--producer python` in a mixed repo stamps Python only.
- `source_set_fingerprint` uses current disk paths scoped to those active faces, under the same corpus policy as producer output.
- `js_ts_profile_fingerprint` is present only when the index has JavaScript/TypeScript faces. Hash the relevant profile source/normalized exclusion policy with a stable `<none>` sentinel so creation/deletion/change can be detected.
- Old metadata lacking these keys remains valid.

Do not put a large source list in JSON metadata; store only face names and fingerprints.

### Validation

- Auto three-family build stamps all represented source faces.
- Explicit producer override stamps only its represented faces.
- Profile create/edit/delete changes fingerprint.
- Python/Rust-only stamp does not gain JS profile sensitivity.

## Segment 3 — Stage-A rebuild decision and convergence

### Data model

Extend freshness explicitly:

```rust
struct StalenessSnapshot {
    source_newer: bool,
    doc_set_drift: Option<bool>,      // None for legacy metadata
    corpus_policy_drift: Option<bool>,
    head_drift: Option<bool>,
}

impl StalenessSnapshot {
    fn needs_rebuild(&self) -> bool {
        self.source_newer
            || self.doc_set_drift == Some(true)
            || self.corpus_policy_drift == Some(true)
    }
}
```

`evaluate_staleness`:

1. Read sidecar meta and active faces.
2. Walk source tree once.
3. Compute newest only for active faces when new meta is available; legacy meta uses baseline behavior.
4. Compare source-set fingerprint when stamped.
5. Compare JS/TS profile fingerprint when stamped.
6. Preserve HEAD drift semantics.

Then replace direct `source_newer` gating in:

- `ensure_fresh`;
- `run_heal_locked` convergence check;
- `heal_outcome_after_rebuild_err`;
- peer-wait reevaluation;
- refresh head-sync decision;

with `needs_rebuild()` where the code is deciding whether producer work is required.

### Post-rebuild convergence

After a successful build:

1. Re-evaluate Stage A; `needs_rebuild()` must be false.
2. Load index once on this already-slow path and compute `doc_set_delta` against the face-scoped disk set.
3. `missing > 0 OR extra > 0` is non-converged. Warn with bounded examples and arm existing cooldown; never loop immediately.

The `extra` branch is required for delete/rename correctness.

### Churn semantics

Keep existing single-flight/cooldown behavior. A committed HEAD drift still overrides cooldown as today. An uncommitted doc-set drift may serve stale during an already-active cooldown, then heal after the window; do not create a second lock/cooldown mechanism.

### Validation

Hermetic tests must pin:

- add triggers fingerprint drift;
- delete triggers fingerprint drift even when remaining source mtimes are older than slot;
- rename triggers fingerprint drift;
- post-rebuild extra doc is non-converged;
- old metadata follows baseline path;
- explicit-face isolation ignores unrelated language edits;
- profile-excluded JS edit causes neither `source_newer` nor fingerprint drift;
- profile policy change does trigger rebuild.

## Segment 4 — JS/TS producer drift and refresh integration

### Producer drift

Generalize the flagged-path drift note from a single pyrefly search to stable producer segments:

```rust
fn producer_drift_notes(repo, slot, roots) -> Vec<String> {
    compare stamped "pyrefly-index <v>" with current version when present;
    compare stamped "scip-typescript <v>" with current version when present,
        using S2 node-tool roots;
    keep rust-analyzer excluded from strict compare under the existing floating-toolchain rationale;
}
```

This function is still called only on already-flagged/error/heal paths. The normal fresh query remains zero external producer spawns.

### Refresh

`refresh` may head-sync/restamp only when `needs_rebuild()` is false. Source-set or profile-policy drift must go through normal S1/S2 rebuild, so a commit that deletes a JS/TS file cannot be converted into a metadata-only refresh.

### Validation

- Fake scip-typescript version mismatch warning.
- Missing scip-typescript during a stale heal serves prior slot under current policy.
- Docs-only commit with no path/profile drift remains head-sync only.
- Deleted-file commit rebuilds instead of restamping stale content.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. Refactor the one-pass source inventory while keeping all existing staleness tests green.
2. Add additive metadata/fingerprint fields with legacy fallback.
3. Switch rebuild decision to `needs_rebuild` and add deletion/rename tests.
4. Generalize post-rebuild doc delta to missing+extra.
5. Add scip-typescript drift/refresh paths using S2 tool resolution.
6. Run S1-S4 integrated real dogfood before M1.

### Cross-child contract exported

- S2 and S4 have one JS/TS corpus inclusion predicate after refactor.
- S6 can trust graph/index language faces only after freshness converges.
- S7 acceptance must explicitly exercise edit + add + delete/rename, not edit-only.

## Verification strategy

1. **MIN** — excluded JS edit does not stale; JS edit does; deletion does via fingerprint.
2. **SAMPLE** — add/rename/profile change/explicit-face isolation/legacy metadata/convergence extra.
3. **FULL** — `staleness`, `build`, `refresh` suites plus full `cargo test -p code-reality`, followed by S2 source-only dogfood mutate/revert acceptance on a disposable copy.

The disposable dogfood acceptance should hash the old slot, perform each source operation, invoke a real query/heal, then verify the final doc set and no repeated heal loop.

## Completion / finalization

- Parent S4 evidence records the delete/rename blind-spot fix separately from JS/TS extension support; do not disguise it as pre-existing behavior.
- Update lifecycle docs in S7 after all M1 integration tests pass.
- Run `/audit-test` semantics on freshness tests, especially tests that could falsely pass by touching mtimes instead of proving doc-set drift.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Review must challenge the Stage-A performance budget, legacy metadata fallback, and whether any source deletion can still reach refresh head-sync without rebuilding.
