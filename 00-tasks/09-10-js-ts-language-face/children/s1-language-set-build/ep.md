# Implementation EP: language-set build orchestration

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S1 — Generalize build orchestration from `RepoKind` to ordered language faces
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **status**: ready-for-implementation

## Implementation overview

Replace the two-language `RepoKind::{Python,Rust,Mixed}` build state with a deterministic producer-family set that can represent Python, Rust, and JavaScript/TypeScript without combinatorial enum growth. Every selected producer writes to a staged partial SCIP file; the live in-repo slot is replaced only after every requested leg validates and the final N-way merge is parseable.

This EP is deliberately limited to orchestration. It exposes the `TypeScript` producer family as a selectable/detectable face and defines the staging contract, but S2 owns the actual `scip-typescript` invocation/configuration. S3 owns language labels and CALLS semantics. S4 owns JS/TS freshness. S6 must still land before M1 is advertised.

The parent POC proved that raw concatenation of three valid SCIP messages remains parseable (`62` documents, `394` function definitions), so this segment can preserve the existing protobuf concatenation mechanism while fixing the current atomicity hole: `python_leg` presently writes the live slot directly.

## UC inventory

### Existing capabilities updated

| Capability | Current | This EP changes |
|---|---|---|
| One-shot build umbrella | Python/Rust `RepoKind` | Ordered producer-family set with a third `typescript` family |
| Mixed-repo build | Python+Rust two-branch flow | Any subset of Python/Rust/TypeScript, one deterministic loop |
| Live SCIP publication | Python may write live slot before Rust completes | All legs stage; one final atomic publish |
| CLI/MCP producer override | `rust|python` | `rust|python|typescript`; omitted detected faces are explicit |

### New UC

**Build any detected producer-family combination without exposing a partial live index.**

Primary consumer: `code-reality build --repo <repo>` and MCP `build`.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | Python only | `.py` corpus | Select Python once; stage, validate, publish | previous slot untouched until publish |
| SM-2 | Rust only | `.rs` corpus | Select Rust once; existing semantics preserved | same |
| SM-3 | JS/TS only | any of six JS/TS extensions | Select TypeScript producer family once | S2 supplies leg implementation |
| SM-4 | Python+Rust | both existing faces | Same semantic result as baseline, through generic loop | byte/semantic regression fixture |
| SM-5 | Python+JS/TS | two families | Stable order; one final merge | no torn slot |
| SM-6 | Rust+JS/TS | two families | Stable order; one final merge | no torn slot |
| SM-7 | Three families | all detected | Python → Rust → TypeScript staged merge order | deterministic merged bytes for fixed partials |
| SM-8 | Explicit override | `--producer typescript` | Run only TypeScript; report other detected families as omitted | live slot represents requested override only |
| SM-9 | Later leg fails | Python succeeds, TypeScript/Rust fails | Return environment/core failure; pre-build slot byte-identical | failure receipt |
| SM-10 | Partial invalid | producer returns missing/tiny/unparseable SCIP | Reject before merge/publish | old slot survives |
| SM-11 | Empty repo | no supported sources | Keep current loud no-source behavior, updated wording | no producer spawn |
| SM-12 | MCP producer value invalid | unsupported value | validation failure lists three legal values | no build side effect |

## Segment 0 — research and frozen constraints

### Current source anchors

- `crates/code-reality/src/build.rs:64` — `RepoKind` encodes only Python/Rust/Mixed.
- `crates/code-reality/src/build.rs:102` — `count_sources` returns the fixed `(py, rs)` pair.
- `crates/code-reality/src/build.rs:133` — `detect_kind` maps that pair to `RepoKind`.
- `crates/code-reality/src/build.rs:158` — `python_leg` invokes `pyrefly-index` with no `--out`; baseline lines 168-169 explicitly say it writes the live in-repo slot itself.
- `crates/code-reality/src/build.rs:186` — `rust_leg` already accepts an explicit output path and validates the staged file.
- `crates/code-reality/src/build.rs:237` — `concat_scip` performs protobuf same-message concatenation through a sibling temp + rename.
- `crates/code-reality/src/build.rs:248` — `build_repo` owns detection, producer selection, merge, stamp, graph build.
- `crates/code-reality/src/build.rs:805` — CLI producer validation is still `rust|python`.
- `crates/code-reality/src/mcp_server.rs:858` — MCP forwards the optional producer value to the build CLI face.

### POC evidence absorbed

- `../../poc/results.md` P8: three-way raw SCIP concatenation parsed correctly as a single index.
- The POC does **not** establish atomic publication. Source inspection proves the opposite for the Python-first mixed path, so atomicity is an implementation invariant, not inherited behavior.

### Architecture decisions

1. Use two concepts, not one overloaded enum:
   - `LanguageFace`: source/document language (`Python`, `Rust`, `JavaScript`, `TypeScript`), defined in a small internal module usable later by S3/S4.
   - `ProducerFamily`: executable producer leg (`Python`, `Rust`, `TypeScript`), where JS + TS map to the same family.
2. Producer order is an explicit total order: Python, Rust, TypeScript. Never depend on hash/map iteration order.
3. `--producer typescript` is the stable override spelling. It selects the shared JS/TS producer family; `javascript` is not a second producer value.
4. Every producer leg accepts a staged output path. S1 changes the Python call to `pyrefly-index --out <part>` using the already documented producer capability.
5. Raw SCIP concatenation remains the merge mechanism in this segment. CR product code does not consume `Index.metadata`; CR's sidecar stamp/report remains the provenance authority. The merge order is nevertheless frozen and tested because protobuf singular fields are last-value-wins.
6. One-family builds also stage first. There is no special fast path that lets a producer write the live slot.

## Segment 1 — language and producer-family model

### Context

Implement the representation used by build detection and later child EPs. Keep it small: extension mapping and producer-family mapping belong here; producer command details do not.

### Files

```text
crates/code-reality/src/
├── language.rs          # new internal source-language / producer mapping
├── lib.rs               # register internal module as needed
└── build.rs             # consume ProducerFamily set
```

### Core implementation

Add an internal language module with explicit extension mapping:

```rust
enum LanguageFace {
    Python,
    Rust,
    JavaScript,
    TypeScript,
}

enum ProducerFamily {
    Python,
    Rust,
    TypeScript,
}

impl LanguageFace {
    fn from_path(path: &Path) -> Option<Self> {
        match extension(path) {
            "py" => Python,
            "rs" => Rust,
            "js" | "jsx" | "mjs" | "cjs" => JavaScript,
            "ts" | "tsx" => TypeScript,
            _ => None,
        }
    }

    fn producer(self) -> ProducerFamily {
        match self {
            Python => ProducerFamily::Python,
            Rust => ProducerFamily::Rust,
            JavaScript | TypeScript => ProducerFamily::TypeScript,
        }
    }
}

impl ProducerFamily {
    const ORDERED: [Self; 3] = [Python, Rust, TypeScript];
    fn cli_name(self) -> &'static str { ... }
}
```

Refactor source counting/detection to return a structured inventory/set rather than `(usize, usize)` and `RepoKind`. Preserve existing directory skip behavior, including the Rust-under-`target/` exception already encoded in source walking. Do not apply new profile semantics to Python/Rust in this segment.

### Validation

- Unit table for all eight relevant extensions plus unsupported extensions.
- Detection fixtures for every producer-family combination in SM-1 through SM-7.
- Existing Python/Rust detection tests must retain their expected outcomes.
- `cargo test -p code-reality --test build` after the RED/GREEN loop for changed behavior.

## Segment 2 — all-leg staging and atomic publication

### Context

This is the correctness-critical part of S1. The invariant is simple: before the final publish, the old live slot is byte-identical. A producer failure may leave CR-owned staged files for cleanup/retry, but it must never leave a one-leg live index that claims to represent a mixed repo.

### Core implementation

Introduce an internal staged-leg result:

```rust
struct StagedLeg {
    family: ProducerFamily,
    path: PathBuf,
    producer_version: String,
}

fn stage_python(repo, stage_path, report, roots) -> Result<StagedLeg, BuildError> {
    run pyrefly-index --repo repo --out stage_path;
    validate_partial(stage_path)?;
    ...
}

fn stage_rust(...) -> Result<StagedLeg, BuildError> { ...existing rust shape... }

fn stage_typescript(...) -> Result<StagedLeg, BuildError> {
    return S2-owned implementation or explicit not-implemented hook until S2 lands;
}

fn merge_staged(parts: &[StagedLeg], publish_tmp: &Path) -> Result<(), BuildError> {
    create/truncate publish_tmp;
    for family in ProducerFamily::ORDERED {
        append bytes for matching part;
    }
    parse publish_tmp as SCIP Index;
}

fn publish_staged(publish_tmp, live_slot) {
    atomic rename sibling temp -> live_slot;
}
```

Implementation details:

- Stage names must be unique to one build attempt under the live slot's directory; do not reuse a shared `.rust-part.scip` name across concurrent/failed attempts.
- All staged paths live under CR-owned `.code-reality/scip/`; they are not target source files.
- Preserve the pre-build slot until **all** selected legs have returned successfully and the merged candidate parses.
- Build graph/cache only from the published candidate after rename, matching current data-plane ordering. If downstream graph build fails after publication, retain the existing `heal_outcome_after_rebuild_err` semantics; do not disguise a graph-only failure as producer failure.
- Cleanup of this build's own staged parts is best-effort and scoped by unique names. Never delete unrelated sidecar files.

### Failure tests

The critical fixture seeds the live slot with sentinel bytes/index A, then uses fake producers:

1. Python staged success.
2. Rust or TypeScript staged failure.
3. Assert build fails.
4. Assert live slot hash/bytes still equal index A.
5. Assert graph build did not consume a partial candidate.

Also test malformed/tiny later part and final merged parse failure.

### Regression tests

- Existing Python-only and Rust-only build behavior.
- Existing Python+Rust mixed graph counts/queries on the hermetic fake fixture.
- Determinism: same ordered partial bytes produce the same merged bytes across repeated runs.

## Segment 3 — CLI/MCP producer contract and reporting

### Context

The generic internal representation is incomplete if public validation/help still encodes two producers.

### Core implementation

Update build CLI help/validation and the MCP tool schema/description:

```text
--producer rust|python|typescript
```

`build_repo` accepts `Option<ProducerFamily>` after parsing at the boundary rather than passing arbitrary strings deep into orchestration.

When override is present in a repo with other detected faces, report omitted families in deterministic order. Preserve the existing semantic meaning: override deliberately builds a partial language face; it is not an error. S6 later prevents language-specific safety tools from treating that partial face as a full completeness proof.

Report producer versions in selected-family order. S2 will supply the TypeScript version string.

### Validation

- CLI: legal `python`, `rust`, `typescript`; illegal value returns usage/env error and lists all legal values.
- MCP build: schema accepts/forwards `typescript`; invalid value remains loud.
- JSON and text reports preserve existing Python/Rust fields and add the third producer without shape drift beyond the value set.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

Implementation order:

1. Segment 1 lands representation/detection with no third producer execution.
2. Segment 2 converts Python/Rust to all-leg staging and proves old-slot invariance.
3. Segment 3 exposes the third override only when the S2 hook is wired in the same implementation arc; do not ship a CLI value that can only panic or return an internal placeholder.

### Cross-child contract exported

- S2 receives `ProducerFamily::TypeScript` and a staged output path; it must never publish directly.
- S3/S4 may reuse `LanguageFace::from_path` instead of duplicating extension lists.
- S6 can inspect build report/graph languages without reintroducing `RepoKind`.

## Verification strategy

Use progressive validation:

1. **MIN** — extension mapping + Python-only/Rust-only + Python-success/later-failure old-slot invariant.
2. **SAMPLE** — all seven producer-family combinations with fake producers, explicit overrides, malformed part.
3. **FULL** — `cargo test -p code-reality --test build`, affected MCP build tests, then `cargo test -p code-reality` after S2 integration.

No npm/network dependency belongs in S1 tests. TypeScript producer execution is faked here; real external behavior belongs to S2 acceptance.

## Completion / finalization

- Update parent S1 status/evidence when implementation is accepted.
- Do not mark M1 structurally supported from S1 alone.
- Update `crates/AGENTS.md` build architecture only after the implementation and review converge; root Capabilities stays at parent-arc finalization.
- Run `/audit-test` semantics on new/modified build tests.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Any accepted cross-child contract correction must be applied here before `/implement`.
