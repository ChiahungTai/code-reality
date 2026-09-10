# Blueprint: JavaScript / TypeScript first-class language faces

> **ep_type**: blueprint
> **status**: reviewed-final
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **task family**: `00-tasks/09-10-js-ts-language-face/`
> **primary acceptance corpus**: `/Users/ctai/Github/delegate-bridge`

## Implementation overview

Extend code-reality from its current Python + Rust production data plane to a first-class JavaScript / TypeScript family without creating a second graph format or embedding repository-specific rules in the tool layer.

The end state is deliberately split into two milestones:

1. **M1 — production structural face**: `code-reality build`, query-time self-heal / refresh, `scip_refs` refs/callers/closure, `graph_db`, and the graph-query family work on `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, and `.tsx`, including mixed Python/Rust/JS/TS repositories. Node language attribution is correct and JS/TS call sites produce real `CALLS` edges rather than a references-only graph.
2. **M2 — production type face**: `code-reality-lsp-bridge` routes the same six extensions to a JS/TS LSP backend for hover, diagnostics, and in-memory edit/recheck while keeping Python and Rust sessions independent.

This blueprint does **not** redefine Python-specific or NT-specific tools as magically language-neutral. `hub_refs --hazard`, `graph_audit`'s rust-analyzer reconciliation, `boundary*`, and the current pyrefly-keyed `project` overlay must expose an explicit support boundary on JS/TS until a dedicated extension exists. A JS/TS caller must never receive a false-clean result from a Python/Rust-only safety check.

### Architecture decisions frozen by this blueprint

- **AD-1 — SCIP remains the interchange format.** `scip-typescript` is the producer candidate; no new CR index schema and no custom JS/TS reference engine.
- **AD-2 — replace binary `RepoKind` branching with a language-set model.** The third language family is the forcing function to remove combinatorial `MixedPyRust`-style states. Build orchestration iterates an ordered set of detected producer faces and merges successful partial SCIP indexes deterministically.
- **AD-3 — one producer family, two source-language labels.** JavaScript and TypeScript share the `scip-typescript` producer, while graph nodes derive `JavaScript` vs `TypeScript` from the defining document extension rather than from the SCIP symbol prefix.
- **AD-4 — every producer leg is staged; the live slot is published once.** Python, Rust, and JS/TS each write a validated partial index. No producer may write the live slot during a multi-leg build. Requested partials merge in deterministic face order and the final validated aggregate replaces the live slot with one atomic rename. Failure of any later leg leaves the pre-build slot byte-identical.
- **AD-5 — no implicit network download on build/self-heal.** `build` resolves an installed `scip-typescript` binary and fails loud with install guidance when absent. Do not hide `npx -y` downloads inside query-time healing. This preserves offline determinism and avoids surprise network work on a read query.
- **AD-6 — JS-only config synthesis is derived data, never a repository edit.** If no usable TS project config exists, CR generates a CR-owned sidecar config whose `files` list is the exact governed JS/TS source set (absolute paths are accepted by the proven producer path). It must not create/modify the target repo's `tsconfig.json`, and it must not rely on TypeScript glob/include inference to discover all six supported extensions.
- **AD-7 — repo corpus facts remain repo-owned.** Existing `.code-reality.toml` exclusions are the CR-level source of exclusions. JS/TS integration must not hard-code `delegate-bridge`, and must not globally assume that a directory named `dist/` is generated. The producer/normalization path must have a way to remove profile-excluded documents from the final partial index.
- **AD-8 — `CALLS` is a semantic requirement, not an optional polish.** SCIP itself does not encode a call-role bit. JS/TS gets a syntax-aware call-site classifier analogous to Python `py_calls`; regex-only classification is rejected. The child EP must POC a maintained Rust parser before locking the dependency.
- **AD-9 — structural producer and type/LSP backend remain separate lifecycles.** `scip-typescript` is the batch index producer. A JS/TS LSP backend is a separate process owned by `code-reality-lsp-bridge`.
- **AD-10 — partial language support must fail loud.** Language-specific commands that are not extended in this arc detect JS/TS inputs/graphs and return an explicit unsupported/degraded statement instead of silently running a Python/Rust analysis.
- **AD-11 — producer corpus and freshness corpus share one effective policy.** Repo-owned profile exclusions and language-extension rules are normalized once and consumed by both producer-output filtering and staleness/doc-set walking. A file cannot be excluded from the SCIP face while remaining freshness-relevant, or vice versa.

## UC inventory

### Backlog relation

- This repository has no `backlog/` directory at planning time, so automatic Backlog.md card creation is skipped.
- Do not create legacy `.kanban/` state. If this repository adopts Backlog.md later, initialize it through the current `backlog init --agent-instructions none` contract before creating a tracking card.

### SYSTEM-MAP impact

- No root `SYSTEM-MAP.md` exists, so there is no SYSTEM-MAP lifecycle entry to update.

### Scan scope

- Root `AGENTS.md` Capabilities table.
- `crates/AGENTS.md` build/data-plane and LSP-bridge module guidance.
- `plugin/skills/code-reality/SKILL.md` consumer-facing prerequisites and language semantics.
- `crates/code-reality/src/build.rs`, `engine.rs`, `graph_db.rs`, `graph_engine.rs`, `mcp_server.rs` and their integration tests.
- `crates/code-reality-lsp-bridge/src/session.rs`, `server.rs`, binary wiring and equivalence batteries.
- `plugin/.mcp.json`, `plugin/README.md` for dependency/bootstrap truth.
- `/Users/ctai/Github/delegate-bridge` as an external read-only dogfood corpus during planning.

### Same-topic project memory entries

- No project memory pool (`.agents/memory`, project-local Claude memory, or equivalent) exists in this checkout; skip memory distillation registration for this arc.

### Existing UC state

| Capability | Current state | Source | Impact | Change |
|---|---|---|---|---|
| Symbol truth refs/defs | ✅ Python/Rust | root `AGENTS.md` | update | Add JS/TS documents and symbols to the production SCIP face. |
| Caller / closure query | ✅ Python/Rust | root `AGENTS.md` | update | JS/TS callers must resolve across files and imports. |
| Self-owned graph.db | ✅ producer-neutral SCIP ingestion with language assumptions | root `AGENTS.md`; `graph_db.rs` | update | Correct language attribution and JS/TS `CALLS` classification. |
| Graph query family | ✅ on materialized graph | root `AGENTS.md` | update | JS/TS graph must have enough semantic edges for impact/review/flows rather than references-only degradation. |
| One-shot build umbrella | ✅ Python/Rust only | root `AGENTS.md`; `build.rs` | update | Third producer family, language-set orchestration, N-way merge. |
| Query-time self-heal / refresh | ✅ Python/Rust source walk | root `AGENTS.md`; `engine.rs` / `build.rs` | update | Track all JS/TS source extensions and producer drift. |
| LSP type face | ✅ `.py` + `.rs` | root `AGENTS.md`; lsp-bridge | update | Route six JS/TS extensions to an independent third backend. |
| Completeness / hazard / projection special tools | ✅ but language-specific | root `AGENTS.md` | boundary clarification | Fail loud or explicitly degrade on JS/TS until dedicated semantics exist. |

### New UC

| Capability | State | Implementation family |
|---|---|---|
| Build and maintain a first-class JS/TS structural graph | 📋 | S1-S4 |
| Query JS/TS refs, callers, impact, review context, and graph flows from the unified graph | 📋 | S3-S4 |
| Hover / diagnose / in-memory recheck JS/TS through the CR type face | 📋 | S5 |
| Expose truthful per-language capability boundaries instead of false-clean special-tool output | 📋 | S6 |

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint | Capability |
|---|---|---|---|---|---|
| SM-1 | TypeScript repo with a usable `tsconfig.json` | `code-reality build --repo <ts-repo>` | Validate the config covers the detected JS/TS corpus, then run installed `scip-typescript`, build graph, and stamp producer provenance. | old slot remains usable until new partial validates | structural graph |
| SM-2 | Conventional JavaScript repo without tsconfig | `.js/.jsx` + `package.json` | Use derived JS project config; never edit repository config. | ephemeral config removed/ignored | structural graph |
| SM-3 | Pure ESM `.mjs` repo | `delegate-bridge`-shape repo | Index `.mjs` successfully even though upstream `--infer-tsconfig` misses this corpus. | prior slot survives producer/config failure | structural graph |
| SM-4 | CommonJS repo | `.cjs` files | Include `.cjs`, preserve cross-file references. | same as SM-3 | structural graph |
| SM-5 | Mixed JSX/TSX application | `.jsx/.tsx` | Both extensions are part of source/freshness corpus and graph correctly. | no false-fresh state | structural graph |
| SM-6 | Python + Rust + JS/TS monorepo | all producer families detected | Run each selected producer once, deterministically merge partial indexes, build one graph. | no torn live slot; merge order stable | structural graph |
| SM-7 | Producer override in mixed repo | `--producer typescript` (final spelling fixed in child EP) | Run only JS/TS leg and report omitted detected faces. | explicit omission note | build control |
| SM-8 | Missing JS/TS producer | `scip-typescript` unresolved | Exit as environment failure with exact install guidance; do not invoke network or corrupt current slot. | previous slot untouched | build safety |
| SM-9 | Producer returns zero indexed files | bad project config / unsupported corpus | Fail loud rather than accepting metadata-only/empty output. | previous slot untouched | build safety |
| SM-10 | Repo profile excludes generated tree | `.code-reality.toml exclude=["dist/"]` | Excluded documents do not appear in the JS/TS partial/final graph and are excluded by the same effective freshness corpus. | source and staleness corpus agree | corpus governance |
| SM-11 | JS/TS edit after a fresh build | edit/add/remove/rename source | Query-time freshness sees source drift and heals once under existing single-flight/cooldown rules. | heal lock / stale-serving semantics unchanged | self-heal |
| SM-12 | JS/TS producer version changes | stamped producer differs from installed | Surface a producer-drift warning on the flagged path; no silent mismatch. | no rebuild loop | lifecycle |
| SM-13 | Cross-file JS call query | `scip_refs mapExitCode --callers`-shape | Return true function callers and sites; import/const/item-level refs remain distinguishable. | caller set reconciled to source/LSP oracle | query |
| SM-14 | Graph-language attribution | mixed `.mjs` + `.ts` | `.mjs/.js/.jsx/.cjs` nodes say JavaScript; `.ts/.tsx` say TypeScript; Python/Rust unchanged. | graph DB query | graph semantics |
| SM-15 | Graph structural edges | JS/TS function calls | Materialization emits non-zero `CALLS` where syntax is a call; non-call refs remain `REFERENCES`. | call-site golden set | graph semantics |
| SM-16 | Small JS repo performance | `delegate-bridge` dogfood | Build is seconds-scale and records timing; no hard SLA until child EP validation. Measure both the current 42-document unfiltered POC corpus and the source-only profile-filtered corpus instead of treating generated mirrors as source. | timing report | performance |
| SM-17 | Large TS workspace / memory pressure | workspace build | Supported project/workspace discovery works or fails with actionable guidance; child EP evaluates `--no-global-caches`/memory behavior. | bounded failure, no partial slot | performance |
| SM-18 | LSP JS/TS hover | any `.js/.jsx/.mjs/.cjs/.ts/.tsx` tool call | Route to the JS/TS backend and return upstream hover under one family policy. | third backend lazy-spawn | type face |
| SM-19 | LSP JS/TS diagnostics/edit | `check_file` / `edit_file` | Preserve current full-content overlay semantics and convergence guarantees. | Python/Rust backend remains alive | type face |
| SM-20 | JS/TS LSP backend unavailable | missing binary/runtime | `lsp_status` reports JS/TS unavailable with install guidance; structural graph remains usable. | per-backend independence | type face |
| SM-21 | Python/Rust-only special tool on JS/TS | `graph_audit`, `hub_refs --hazard`, or pyrefly-keyed `project` against JS/TS | Fail loud or return explicitly marked unsupported/degraded output; never imply a clean audit. Mixed repositories are included: a Rust-only scanner may not report clean-silent while JS/TS nodes sit outside its scanned corpus. | capability guard | truthful boundary |
| SM-22 | Existing Python/Rust consumer | normal build/query/test | Byte/semantic behavior remains unchanged except generalized internal representation. | existing Rust suites + dogfood | regression safety |
| SM-23 | Empty or unusable project config | repo has `tsconfig.json` such as `{}` but it covers no detected JS/TS source | Treat it as unusable and take the derived-config path; config existence alone is not authority. | no target-repo mutation | structural graph |
| SM-24 | Excluded generated edit after fresh build | modify `dist/.../*.mjs` excluded by repo profile | Do not trigger heal and do not create a permanent doc-set delta; source edits outside the exclusion still trigger normally. | no heal loop / no false-fresh | lifecycle |
| SM-25 | JS/TS non-function symbol and test file | class/interface-like symbol in `*.test.mjs` or `__tests__/...` | Queryable symbol forms chosen for JS/TS survive cache/graph ingestion and test classification agrees with graph-query test filtering. | cache + graph DB rows | graph semantics |

## Segment 0 — global research summary

### Method boundary

This detached code-reality worktree has no `.code-reality/graph.db` or SCIP slot. Therefore planning-time dependency discovery used direct source inspection plus `rg` over actual consumers; no claim in this EP treats CR graph output as current-worktree evidence. Rebuilding CR's own graph solely to write this EP was intentionally avoided.

### Reusable infrastructure

- `build.rs`: existing producer resolution, version probes, environment/core error families, atomic partial-slot pattern, protobuf same-message concatenation, producer provenance stamping, single-flight healing.
- `engine.rs`: shared source walk, doc-set delta, staleness evaluation, metadata stamping.
- `graph_db.rs`: producer-neutral SCIP protobuf reader, span-based caller attribution, single edge ontology, edge natural-key dedupe, derived graph materialization.
- `common::resolve_bin` / `producer_version`: reusable external-backend resolution and version observation.
- `profile.rs`: repo-owned exclusion prefixes; this must be reused rather than embedding JS repository special cases.
- `code-reality-lsp-bridge`: generic `LspSession` already separates protocol/lifecycle from `LangSpec`; adding a third language should reuse that session engine rather than fork it.

### Planning-time POCs completed 2026-09-10

The persistent command/results record is `00-tasks/09-10-js-ts-language-face/poc/results.md`. The summary below keeps only the decisions that constrain implementation.

#### POC-A — upstream JS auto-inference is insufficient for the real acceptance corpus

Environment: Node `v24.20.0`, `@sourcegraph/scip-typescript@0.4.0`, current `/Users/ctai/Github/delegate-bridge` (42 total `.mjs` documents, zero `.js/.ts`; 28 `.mjs` remain when the generated `dist/` tree is excluded at the current corpus baseline).

Command shape:

```text
npx -y @sourcegraph/scip-typescript@0.4.0 index \
  --cwd /Users/ctai/Github/delegate-bridge \
  --infer-tsconfig --no-progress-bar --output <tmp>
```

Result: exit 1, `error: no files got indexed`. Upstream documentation recommends `--infer-tsconfig` for JavaScript, but Sourcegraph also has a closed-as-not-planned issue documenting cases where inferred JS config indexes zero files. Therefore `--infer-tsconfig` alone is rejected as CR's fallback contract.

#### POC-B — explicit governed-files config indexes the `.mjs` corpus on the user's Node 24 runtime

A temporary copy received a generated config with `allowJs=true`, `module/moduleResolution=NodeNext`, and an explicit governed `files` list. No repository file was edited.

Result: exit 0. The unfiltered feasibility run indexed all 42 `.mjs` documents, including generated `dist/` mirrors; the governed source-only run indexed exactly the 28 profile-selected source documents. The source-only index was ~1.38 MiB. Therefore 42 is only an unfiltered feasibility/performance observation; 28 is the planning-baseline acceptance denominator. This clears the fatal feasibility question for `.mjs` indexing. It does **not** turn Node 24 into an upstream-supported version: Node 24 remains an observed-compatible runtime rather than a guaranteed upstream contract.

#### POC-C — existing CR already parses scip-typescript output

The governed 28-document SCIP was fed to current `graph_db build` in a temporary copy.

Result:

```text
nodes=192
edges=293
CALLS=0
edge kinds: REFERENCES=293
node language distribution: Rust=192
item-level refs=953
non_fn_defs_skipped=14863
external_skipped=436
```

`scip_refs mapExitCode --callers` found three real function callers plus item-level references in the source-only corpus. This proves the current SCIP parser/caller-attribution machinery is reusable. It also pinpoints the semantic gaps that block production support: language attribution defaults every non-Python symbol to Rust, graph materialization has no JS/TS syntactic call marks, and a large class/type-like definition population is filtered by the current function-oriented queryable-symbol policy.

#### POC-D — corpus boundary matters

The unfiltered acceptance index contains both `scripts/...` and generated `dist/marketplace/plugin/scripts/...`, producing duplicate logical definitions such as `mapExitCode`. CR must therefore reuse repo-owned exclusions or an equivalent producer-normalization step. The tool layer must not hard-code `dist/` as excluded.

#### POC-E — explicit governed `files` is required for the six-extension face

A hermetic fixture containing `.js/.jsx/.mjs/.cjs/.ts/.tsx` exposed a TypeScript discovery trap: the initial glob/include form produced only five SCIP documents and omitted `.jsx`. `tsc --listFiles` and `scip-typescript` both covered all six when CR generated a sidecar project with an explicit `files` list derived from the governed source set. The sidecar lived under CR-owned data and used absolute source paths, so the target repository remained untouched.

Decision: usable repository configs may remain authoritative when they actually cover the governed corpus; the fallback path must use an exact governed `files` list rather than glob inference.

#### POC-F — syntax-aware JS/TS call classification is feasible

Maintained Rust Tree-sitter grammars (`tree-sitter-javascript`, `tree-sitter-typescript`) were exercised across all six extensions. The POC distinguished direct calls, method calls, optional-chain calls, constructors, and pure property reads with the expected fixture counts. S3 may use these as concrete implementation candidates, subject to real-corpus reconciliation and graph-build performance checks.

#### POC-G — current LSP protocol works, but the backend command shape must change

`typescript-language-server 6.0.0 --stdio` + TypeScript `5.9.2` returned hover for all six extensions. A deliberately bad `.ts` produced two diagnostics and a corrected full-document edit converged to zero; a server-to-client request answered with `[]` did not freeze the session. `serverInfo` was not useful and is not part of the correctness contract.

Source inspection found the concrete integration blocker: current `LspSession` spawns `Command::new(&backend_cmd)` without argv, while TLS requires `--stdio`. S5 therefore owns a typed program+argv backend command model; `--typescript-backend <executable>` changes the executable only and the JS/TS family retains the required fixed arg.

#### POC-H — N-way merge and containment data are strong; freshness needs corpus identity

Raw concatenation of three valid SCIP protobuf messages parsed successfully as one `Index` with 62 documents and 394 function definitions. Function-definition `enclosing_range` coverage was 192/192 on source-only `delegate-bridge` and 10/10 on the six-extension fixture.

Source inspection also found a pre-existing freshness blind spot: Stage-A staleness compares newest surviving source mtime to index mtime. Delete/rename can therefore leave no newer surviving source, allowing `refresh` to restamp HEAD without rebuilding while a deleted document remains indexed. S4 must add active-face/doc-set/profile corpus identity (fingerprint or equivalent) to the rebuild decision; mtime-only logic is insufficient for the M1 delete/rename claim.

### External tool facts verified 2026-09-10

- Sourcegraph documents `scip-typescript` as the TypeScript/JavaScript SCIP indexer with definition/reference/cross-file support. Current npm package observed: `@sourcegraph/scip-typescript` 0.4.0.
- Upstream quick-start: TS expects a project config; JS offers `--infer-tsconfig`; Node 18/20 are the documented supported runtimes.
- `typescript-language-server` is the S5 backend choice after the planning POC: npm 6.0.0 exposes the required `--stdio`, and the POC passed hover on all six extensions plus diagnostic/edit convergence. The implementation still treats version/runtime support as an explicit external-prerequisite policy rather than silently acquiring whatever npm serves.

### Critical/high-risk assumptions remaining

| Level | Assumption | Required validation |
|---|---|---|
| **fatal, cleared** | `scip-typescript` can index the actual `.mjs` acceptance corpus | POC-B passed. |
| **high, feasibility cleared** | A maintained Rust JS/TS parser can classify call expressions across all six extensions without making graph build unacceptably heavy | Tree-sitter POC passed the six-extension semantic fixture. S3 still must reconcile real-corpus sites and measure graph-build overhead before dependency lock. Regex is not fallback. |
| **high** | Profile exclusions can be applied without leaving producer-vs-staleness corpus disagreement or dangling self-owned symbols | S2/S4 child EP integration fixture with excluded generated tree. |
| **high, merge feasibility cleared** | N-way SCIP merge can replace current 2-leg branch logic while preserving Python/Rust byte/semantic behavior | Raw three-index concat parsed successfully; S1 still owns all-leg staging, deterministic ordering, and Python/Rust regression. |
| **high, protocol feasibility cleared** | Modern TypeScript LSP preserves current bridge convergence/overlay semantics and supports all six extensions | TLS 6.0.0 POC passed hover/diagnostics/edit convergence; S5 still owns program+argv wiring, resolver/status/spawn identity, and death isolation. |
| **high, coverage feasibility cleared** | `scip-typescript` function definitions carry enough `enclosing_range` data for containment-based caller attribution to remain complete enough for production use | Planning coverage is 192/192 dogfood and 10/10 fixture. S3 freezes the production acceptance bar and fails M1 if implementation evidence drops below it. |
| **medium** | scip-typescript behaves acceptably on pnpm/yarn/tsconfig-reference workspaces | S2 child EP workspace fixtures / external corpus. |
| **medium** | Node 24 remains operational despite upstream declaring only 18/20 | Record as observed pass; do not encode a false support claim. Test CI/consumer policy against declared minimum separately. |
| **medium** | Existing `producer_version(<bin> --version)` semantics match `scip-typescript`'s actual CLI and can be made GUI-safe for npm-installed binaries | S2 child EP POC version output and binary-resolution behavior before wiring producer drift. |

### Similar implementations / anchors

- Python producer orchestration: `crates/code-reality/src/build.rs::python_leg`.
- Rust staged partial + atomic merge: `crates/code-reality/src/build.rs::rust_leg`, `concat_scip`, and mixed branch in `build_repo`.
- Python call classification: `crates/code-reality/src/py_calls.rs` feeding `graph_db.rs` `call_marks`.
- Generic LSP protocol/session engine: `crates/code-reality-lsp-bridge/src/session.rs::LspSession` with language-specific `LangSpec`.

### Projection note

The EP projection tool is intentionally not used here. The current worktree lacks a real index, and the present `project` implementation mints pyrefly/Python symbol identities. Using it to justify the design of a new JS/TS language face would be circular and could launder a Python-only projection into evidence.

## Segment partitioning and dependency graph

This is a blueprint because seven medium-sized concerns must move together to avoid a misleading half-supported language face. Each segment below derives its own implementation EP before `/implement`.

Dependency shape:

```text
S1 language-set core
  ├─> S2 scip-typescript producer/corpus
  │      └─> S4 freshness + lifecycle
  └─> S3 graph semantics (language + CALLS)
          └─> S6 capability-boundary integration
S2 + S3 + S4 + S6 ─> M1 structural acceptance
S1 ─> S5 JS/TS LSP type face ─> M2 type acceptance
M1 + M2 ─> S7 dogfood/docs/distribution closure
```

---

## S1 — Generalize build orchestration from `RepoKind` to ordered language faces

### Context

Implements the architectural prerequisite for **Build and maintain a first-class JS/TS structural graph**. The existing `RepoKind::{Python,Rust,Mixed}` and `(run_py, run_rs)` match are a two-language encoding that becomes combinatorial at language three.

### Dependency anchors

- Definition: `crates/code-reality/src/build.rs::RepoKind`, `count_sources`, `detect_kind`, `build_repo`.
- Consumers: `crates/code-reality/tests/build.rs` detection/mixed/override tests; `crates/code-reality/src/mcp_server.rs` build producer argument validation; query-time heal in `build.rs::run_heal_locked` calls `build_repo`.

### Absorb scope

- Introduce a small internal `LanguageFace` / detected-language-set model with deterministic ordering.
- Source counting detects `.py`, `.rs`, and the six JS/TS extensions while preserving existing skip semantics.
- Replace boolean two-leg orchestration with a producer-leg loop / staged partial collection. Refactor the current Python leg so it also writes an explicit partial rather than mutating the live slot before other legs finish.
- Generalize `--producer` validation and report face/omitted-language notes without breaking existing `rust|python` values.
- Publish exactly once after every requested leg validates; failed later legs leave the pre-build live slot byte-identical.
- Define stable merge order and verify any singular protobuf metadata last-wins behavior is deterministic before relying on raw same-message concatenation.
- Preserve error families, provenance stamp, graph build, and index creation semantics.

### Shared semantic constraints

- Python and Rust existing command behavior is a regression contract, not a rewrite opportunity.
- S1 is orchestration-only: it must not change graph edge ontology, caller semantics, or language-specific query behavior.
- JS + TS are one producer family for orchestration but retain per-document graph language labels later in S3.
- Partial indexes are merged in a stable order and the live slot changes only after all requested producer legs validate.

### Success criteria

- Synthetic Python-only, Rust-only, JS/TS-only, Python+Rust, Python+JS/TS, Rust+JS/TS, and three-family repositories select exactly the expected producer set.
- Existing build integration tests remain green with no Python/Rust behavior drift.
- A Python-first build followed by a failing Rust or JS/TS leg leaves the pre-build live index byte-identical; the same invariant holds for every deterministic leg order.
- CLI/MCP producer validation exposes JS/TS without copy-pasted family-specific branching.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s1-language-set-build/ep.md`

---

## S2 — Add the `scip-typescript` producer leg and governed JS/TS corpus construction

### Context

Implements the producer half of **Build and maintain a first-class JS/TS structural graph**. POC-A proves upstream JS auto-inference is not enough for `.mjs`-only repositories; POC-B/E prove an explicit CR-owned config driven by the governed `files` set works across the acceptance corpus and all six extensions.

### Dependency anchors

- Existing external-bin contract: `crates/code-reality/src/common.rs::resolve_bin`, producer version helpers.
- Existing staged producer shape: `crates/code-reality/src/build.rs::rust_leg`.
- Repo-owned exclusions: `crates/code-reality/src/profile.rs::load_profile`, `is_excluded`.
- Consumer docs: `plugin/README.md`, `plugin/skills/code-reality/SKILL.md`.

### Absorb scope

- Resolve and version-probe an installed `scip-typescript` executable; missing binary/runtime is an environment failure with exact install guidance. The child EP must define a GUI-safe discovery contract for npm-installed binaries or an explicit PATH prerequisite rather than assuming an interactive shell PATH.
- POC the real `scip-typescript --version` shape before reusing the generic producer-drift comparison.
- Define a **usable-config predicate** before preferring an existing TS project configuration: it must parse and cover the detected JS/TS corpus through `files`/`include`/project references or equivalent compiler semantics. Mere existence is insufficient; an empty `{}` config on the acceptance corpus is unusable.
- For JS-only/unusable-config repos, generate a CR-owned sidecar config whose explicit `files` list is derived from the governed six-extension corpus; configure JS/module semantics as needed, never write target configuration, and do not rely on TypeScript glob/include inference for extension discovery.
- Define TS/TSX and mixed JS+TS handling; child EP validates workspace flags/project discovery rather than assuming a single root tsconfig.
- Apply the shared effective corpus policy from AD-11 to the produced partial corpus without hard-coded repo names or global `dist/` policy.
- Validate non-empty/parseable output before it can participate in live-slot merge.
- Add fake-producer tests plus real, non-always-on acceptance POC on `delegate-bridge`.

### Shared semantic constraints

- No `npx -y` auto-download inside production `build`/heal paths.
- The exact supported upstream version policy must be explicit: pin/minimum/range is decided in the child EP from compatibility evidence, not from whatever global npm happens to return.
- Node is a production runtime prerequisite for both `scip-typescript` and the candidate type backend. The child EP must freeze the supported Node policy from current upstream/package evidence; Node 24 is recorded as POC-compatible only and documentation must not extrapolate unsupported majors from that observation.

### Success criteria

- Pure `.mjs` `delegate-bridge`-shape fixture produces a non-empty SCIP index whose document set equals the governed corpus exactly.
- A repo with an empty `{}` `tsconfig.json` takes the derived-config path and succeeds without editing that file.
- The six-extension derived-config fixture indexes `.js/.jsx/.mjs/.cjs/.ts/.tsx` exactly; the earlier glob/include form that omitted `.jsx` is a regression case.
- Profile-excluded generated paths are absent from the partial index.
- Missing producer and zero-file output are loud, actionable failures and preserve the old live slot.
- Real `delegate-bridge` dogfood produces source-level refs for frozen symbols without source/dist duplication once its repo profile declares the generated tree.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s2-scip-typescript-producer/ep.md`

---

## S3 — Make graph materialization language-correct and call-aware for JS/TS

### Context

Implements **Query JS/TS refs, callers, impact, review context, and graph flows from the unified graph**. POC-C shows current CR already reads scip-typescript and `scip_refs --callers` can attribute function contexts, but graph materialization lies about language (`Rust`) and emits only `REFERENCES`.

### Dependency anchors

- Language attribution: `crates/code-reality/src/graph_db.rs::infer_language` and node insert path.
- Queryable-symbol filters: `crates/code-reality/src/cache.rs::build_db` and `graph_db.rs::scan`.
- Call split: `graph_db.rs` `call_marks` + `CALLS`/`REFERENCES` decision.
- Function containment coverage: `crates/code-reality/src/engine.rs::fn_spans` and SCIP `enclosing_range`.
- Test classification: `graph_db.rs::is_test_rel` vs `graph_engine.rs::TEST_FILE_RE`.
- Python precedent: `crates/code-reality/src/py_calls.rs`.
- Graph semantics consumers: `graph_engine.rs` queries that specifically weight/filter `CALLS` (entrypoint inference, review context, affected flows, community crossing, etc.).

### Absorb scope

- Derive graph language from definition document extension, not a Python-vs-other symbol-prefix binary.
- Replace Python-only/non-function queryable-symbol gates with a document/language-aware policy for the scip-typescript face. Measure `non_fn_skipped` / fully-filtered-document rates and explicitly pin which JS/TS class/interface/type symbol forms are queryable; do not accidentally make every Rust `Type#` queryable.
- Introduce a syntax-aware JS/TS call-site collector for all six extensions.
- POC maintained Rust parser candidates in the child EP and lock one only after fixtures prove direct call, method call, optional chaining, constructor/new, JSX/TSX, and non-call reference distinctions.
- Feed JS/TS call marks through the existing natural-key edge materializer; preserve Python classifier and Rust semantics.
- Record `enclosing_range` coverage over queryable JS/TS function definitions on dogfood and the fixture matrix. The child EP must set a minimum acceptance bar before implementation so a small non-zero `CALLS` sample cannot mask widespread containment fallback.
- Align DB `is_test` classification with existing JS/TS query conventions (`__tests__`, `.spec.*`, `.test.*`) or explicitly redesign the single source; Python/Rust behavior remains unchanged.
- Add language/edge-kind tests using a small checked-in SCIP fixture or hermetic fake producer output; do not require npm for the unit suite.

### Shared semantic constraints

- `scip_refs --callers` attribution and graph `CALLS` are related but not identical: callers can be derived from reference containment, while graph structural semantics require actual syntactic calls.
- Never upgrade all references inside a function to `CALLS` merely because caller attribution succeeded.
- Existing graph edge weighting and structural `EDGE_KINDS` semantics remain stable; the change is improved classification, not a new edge ontology.

### Success criteria

- JS and TS fixture nodes carry correct `language` values; Python/Rust fixtures unchanged.
- Frozen JS/TS class/interface/type fixtures chosen as queryable survive both `scip_refs` cache ingestion and graph DB materialization; non-queryable variables remain filtered.
- Frozen JS/TS fixtures emit expected `CALLS` sites and retain imports/property reads/non-call uses as `REFERENCES`.
- The recorded `enclosing_range` coverage meets the child EP's predeclared production bar; falling below it fails M1 rather than silently degrading caller attribution.
- JS test-file fixtures produce consistent `is_test` values and graph-query filtering.
- `graph_query review_context` / `impact_radius` on the dogfood corpus has non-vacuous structural results traceable to real call sites.
- `mapExitCode`-shape caller results reconcile against direct source/LSP references; item-level imports are not reported as function calls.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s3-js-ts-graph-semantics/ep.md`

---

## S4 — Extend source freshness, doc-set convergence, producer drift, and refresh to JS/TS

### Context

Completes M1 lifecycle semantics. A one-time JS/TS build is not production support if `.mjs/.ts` edits never trigger staleness, if doc-set convergence compares only Python/Rust, or if delete/rename can remain falsely fresh because no surviving source mtime became newer than the index.

### Dependency anchors

- `crates/code-reality/src/engine.rs::SourceWalk`, `walk_sources`, `evaluate_staleness`, `doc_set_delta`.
- `crates/code-reality/src/build.rs::producer_drift_note`, `ensure_fresh`, `run_heal_locked`, refresh path.
- Tests: `crates/code-reality/tests/staleness.rs`, `build.rs`, `refresh.rs`.

### Absorb scope

- Generalize `SourceWalk` from hard-coded `py/rs` sets to a language-aware corpus representation while preserving safe superset-walk rules, and make its freshness-facing path consume the same effective corpus policy as producer filtering.
- Track all JS/TS extensions in newest-source and doc-set signals.
- Apply one normalized profile/language corpus policy on both sides of the producer-vs-disk comparison so excluded documents neither force false-stale loops nor appear false-fresh.
- Stamp active language faces plus a stable face-scoped source-set/profile-policy identity (fingerprint or equivalent) and include identity drift in Stage-A rebuild decisions. This must detect add/delete/rename and relevant exclusion-policy changes even when newest-source mtime alone is unchanged.
- Extend face-scoped doc-set comparison to JS/TS and mixed indexes.
- Extend producer-drift observation to the JS/TS producer without spawning external tools on the steady-state query path beyond existing flagged behavior.
- Keep single-flight lock, churn cooldown, peer-heal, and serve-stale-on-failure contracts unchanged.

### Success criteria

- Edit/add/delete/rename for each JS/TS extension exercises the same freshness state machine as Python/Rust.
- Delete/rename tests explicitly construct the case where surviving source mtimes are not newer than the index, proving corpus identity rather than mtime accidentally triggers the rebuild.
- A profile-excluded generated `.mjs` edit does not trigger an endless false-stale heal.
- Editing only a profile-excluded `.mjs` leaves the freshness state stable; editing a nearby included `.mjs` triggers exactly the expected stale/heal transition, proving the filter is not over-broad.
- JS/TS producer failure during heal serves the prior index with a loud warning under existing policy.
- Three-family synthetic doc-set convergence reaches `Healed`, not a language-mismatch loop.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s4-js-ts-freshness/ep.md`

---

## S5 — Add JavaScript/TypeScript to the LSP type face

### Context

Implements **Hover / diagnose / in-memory recheck JS/TS through the CR type face** after the structural path is independent and stable. The protocol client is already mostly generic; the current bridge object and `LangSpec.extension` are still two-language/single-extension shaped, and the current backend command representation cannot express the required TLS `--stdio` argv.

### Dependency anchors

- `crates/code-reality-lsp-bridge/src/session.rs::LangSpec`, `LspSession`.
- `crates/code-reality-lsp-bridge/src/server.rs::Bridge::new`, `session_for`, status/help descriptions.
- Binary flags in `crates/code-reality-lsp-bridge/src/bin/code-reality-lsp-bridge.rs`.
- Existing Python/Rust equivalence and backend-death batteries.

### Absorb scope

- Generalize one-extension `LangSpec` routing to an extension set or matcher.
- Generalize the backend command from a bare executable string to a typed executable + argv form. Keep shell execution out of the bridge.
- Add a third independent LSP session serving `.js/.jsx/.mjs/.cjs/.ts/.tsx`.
- Add a public `--typescript-backend <executable>` command flag for that JS/TS family while preserving the existing Python/Rust flags. The flag overrides only the executable; the family command spec retains fixed `--stdio`. `session_for` must route all six extensions through the family matcher and `lsp_status` must expose a third backend line.
- Consume the completed `typescript-language-server 6.0.0 --stdio` POC: all six extensions returned hover, diagnostics converged after edit correction, and the current server-to-client request response strategy did not freeze. Do not depend on initialize `serverInfo`.
- Define `LangSpec` policy values from measured backend behavior (hover retry, convergence timeout, language id/config), not by copying Python/Rust defaults.
- Define backend command/version/config discovery and unavailable-state install guidance, including the same Node runtime and GUI-PATH considerations as S2.
- Preserve overlay, LRU, out-of-band disk sync, convergence deadlines, and backend-death independence.
- Add JS/TS hover/diagnostic/edit-recheck batteries plus mixed-backend liveness tests.

### Shared semantic constraints

- The structural JS/TS producer must remain usable when the LSP backend is absent.
- Do not add TypeScript-specific protocol branches inside generic `LspSession` unless the POC proves an unavoidable upstream difference; prefer `LangSpec` policy data.

### Success criteria

- All six supported extensions route correctly; representative JS and TS/JSX/TSX hover fixtures return expected signatures.
- Actual spawn includes the required `--stdio` argv, and availability/status resolution uses the same executable resolver as spawn.
- Introduced syntax/type errors appear after in-memory `edit_file` and clear after correction.
- Killing JS/TS backend leaves Python/Rust sessions live, and vice versa.
- `lsp_status` reports three backend families and missing JS/TS tooling as `unavailable`, not a server-wide failure.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s5-js-ts-lsp/ep.md`

---

## S6 — Close truthful capability boundaries for language-specific CR tools

### Context

Implements **Expose truthful per-language capability boundaries instead of false-clean special-tool output**. Adding JS/TS to `build` makes existing commands reachable on new graph content; Python/Rust-only tools must not silently imply parity.

### Dependency anchors

- `graph_audit.rs`: rust-analyzer symbol reconciliation.
- `hazard.rs` / `hub_refs.rs`: Python AST/dynamic-dispatch safety net.
- `project.rs` / `overlay-gen`: pyrefly/Python symbol identity assumptions.
- `boundary.rs`: Python/Rust NT boundary semantics.
- Consumer tool descriptions in MCP server and plugin skill.

### Absorb scope

- Produce a per-tool language-support matrix in code/docs and enforce any dangerous unsupported path at runtime.
- `scip_refs`, callers/closure, graph_db, graph_query: first-class JS/TS after M1.
- `hub_refs` static graph aggregation may remain usable, but `--hazard` must identify its Python-specific limitation on JS/TS rather than returning false-clean dynamic safety.
- `graph_audit` remains its documented Rust reconciliation unless a separate JS/TS completeness oracle is explicitly implemented. It must detect when the unified graph contains JS/TS nodes outside its Rust scan corpus and return an explicit partial/degraded result even in a mixed repository; direct JS invocation is not the only guarded case.
- Current `project` overlay remains Python/pyrefly-only in this blueprint unless its child EP demonstrates a producer-neutral symbol minting design; at minimum, reject misleading JS/TS projection.
- `boundary*` remains its NT Python/Rust domain and is not broadened merely because the repo contains JS/TS.

### Success criteria

- Every public CLI/MCP description matches runtime behavior for JS/TS.
- Unsupported language-specific paths have direct tests proving they fail/degrade explicitly.
- A mixed Rust+JS/TS fixture cannot produce a clean-silent `graph_audit` result when JS/TS content is outside the scanner's coverage.
- No acceptance report uses a Python/Rust-only tool as evidence of JS/TS completeness.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s6-language-capability-boundaries/ep.md`

---

## S7 — Distribution, dogfood acceptance, documentation, and capability finalization

### Context

Closes M1/M2 into a consumer-usable release without resurrecting the retired npm distribution face. The CR wheel remains the product binary; JS/TS producer/LSP executables are external prerequisites analogous to rust-analyzer unless a child EP explicitly proves a better deterministic acquisition model.

### Dependency anchors

- `plugin/.mcp.json` first-session uv bootstrap.
- `plugin/README.md` prerequisites and type-face description.
- `plugin/skills/code-reality/SKILL.md` source-of-truth usage semantics.
- root `AGENTS.md` Capabilities and `crates/AGENTS.md` module contracts.
- release/marketplace pin consistency tests.

### Absorb scope

- Document JS/TS structural producer prerequisite and type-backend prerequisite with exact fail-loud guidance.
- Document the supported Node runtime policy and the executable-discovery/PATH contract for both JS/TS binaries; include GUI-launched plugin behavior in acceptance so an interactive-shell-only install is not presented as generally available.
- Keep PyPI bootstrap single-binary-layer semantics; do not silently add npm package installation to session startup without a separate user-approved policy change.
- Dogfood M1 on `delegate-bridge`: add/consume a repo-owned CR profile for generated-tree exclusions in that repository's own change arc, build a real graph, and validate frozen refs/callers/impact examples.
- Dogfood M2 on representative `.mjs` and `.ts` corpora.
- Update Capabilities/help/MCP descriptions from Python/Rust to the truthful support matrix.
- Run full cargo suites, relevant release consistency gates, and audit newly modified tests.

### Acceptance examples on `delegate-bridge`

At minimum freeze and reconcile a source-only set around several cross-file symbols such as `mapExitCode`, `runTask`, `runReview`, `getAdapter`, and resume/session resolution functions. At the current planning baseline there are 42 total `.mjs` files and 28 after excluding the repo's generated `dist/` mirror; the S7 golden denominator must use the repo-owned profile-filtered corpus, not the unfiltered 42. The exact golden list is captured by the S7 child EP at its baseline so later code churn does not turn names in this blueprint into brittle test constants.

### Success criteria

- `code-reality build --repo /Users/ctai/Github/delegate-bridge` succeeds without repository tsconfig mutation and produces a graph whose source corpus excludes generated copies according to repo profile.
- `scip_refs --callers` and graph review/impact queries return source-level, non-vacuous results for frozen dogfood symbols.
- JS/TS edits participate in self-heal; LSP type face works when its backend is installed and degrades independently when absent.
- Full Python/Rust regression suite is green.
- Root/module/plugin instructions advertise exactly what is and is not supported.

→ Derived implementation EP: `00-tasks/09-10-js-ts-language-face/children/s7-dogfood-release-closure/ep.md`

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

### Delivery order

1. S1 language-set core.
2. S2 producer and S3 graph semantics may proceed after S1 with coordinated contracts; S4 depends on S2's corpus definition.
3. S6 capability guards integrate after S3 identifies the language-bearing graph semantics.
4. M1 gate: S1-S4 + S6 integrated and real `delegate-bridge` structural dogfood accepted.
5. S5 type face can proceed in parallel after S1 but must not block structural support.
6. S7 runs full acceptance and documentation/release closure after M1 and M2.

### M1 structural acceptance gate

The arc may call JS/TS **structurally supported** only when all are true:

- auto-detection + explicit producer route work for the six extensions;
- JS-only `.mjs` fallback is proven on real dogfood;
- mixed language merge is atomic and deterministic;
- an unusable/empty existing config falls back to derived config without mutating the target repo;
- graph language attribution is correct;
- intended JS/TS non-function symbol forms survive query/cache materialization without broadening Rust symbol policy;
- JS/TS syntax produces validated `CALLS`, not just `REFERENCES`;
- caller-attribution `enclosing_range` coverage meets the S3 child EP's predeclared bar;
- source freshness/self-heal covers JS/TS and converges for content edit, add, delete, rename, and relevant profile-corpus changes; delete/rename acceptance must prove a face/doc-set/profile identity signal rather than relying on surviving-source mtime;
- `scip_refs` and core graph queries are non-vacuous on real symbols;
- unsupported specialized tools are already guarded against false-clean output.

### M2 type acceptance gate

- `.js/.jsx/.mjs/.cjs/.ts/.tsx` routing is covered and representative JS, JSX, TS, and TSX hover + diagnostic + edit-recheck fixtures pass;
- backend liveness is isolated across Python/Rust/JS-TS;
- missing JS/TS backend is an explicit `unavailable` state and does not damage M1.

### Regression gate

- `cargo test` full workspace.
- Targeted `code-reality` build/staleness/refresh/graph DB suites during S1-S4.
- Targeted `code-reality-lsp-bridge` suites during S5.
- Existing Python/Rust dogfood paths from root `AGENTS.md` remain valid.
- New tests must be audited for hermeticity: npm/network-dependent tests are acceptance/POC lanes, not default unit-test dependencies.

## Finalization requirements

Because this is a blueprint, finalization occurs after all child implementation EPs and post-build acceptance converge:

1. Update root `AGENTS.md` Capabilities for JS/TS structural and type faces, with explicit special-tool boundaries.
2. Update `crates/AGENTS.md`, `plugin/skills/code-reality/SKILL.md`, `plugin/README.md`, CLI/MCP descriptions and prerequisites.
3. No SYSTEM-MAP action unless one exists by closure time.
4. No Backlog.md action unless the repository adopts `backlog/` before implementation; if adopted, create/update the tracking card under the current kanban contract.
5. Run `/audit-test` semantics on new/modified tests and record the result in implementation completion evidence.
6. Archive/close this blueprint only after M1 and M2 acceptance, or explicitly split M2 into a separately approved follow-up and change the advertised capability accordingly.

## EP review record

Independent reviewer: Muse via `delegate-bridge` job `job-mtvamiae-ekou2e` (`muse-spark-1.3`, xhigh). Reviewer verdict was **CHANGES REQUIRED**. The main LLM rechecked the cited source/config facts before adjudication; all findings below were accepted, with F8 narrowed to an explicit discovery/runtime contract rather than presupposing one npm installation mechanism. G4 was already partly present and is recorded as a scope-strengthening correction rather than a new segment.

| Finding | Decision | Evidence rechecked | Absorbed into blueprint |
|---|---|---|---|
| F1 existing config can be unusable | ✅ adopted | `delegate-bridge/tsconfig.json` is `{}` while the `.mjs` inferred-config POC indexed zero files | S2 usable-config predicate; SM-23 |
| F2 42-file denominator included generated output | ✅ adopted | current corpus: 42 total `.mjs`, 28 with `dist/` excluded | POC-A/B wording; SM-16; S7 source-only acceptance |
| F3 producer exclusion vs freshness mismatch | ✅ adopted | `engine.rs::walk_sources/doc_set_delta` have no profile input today | AD-11; S4 shared effective corpus policy; SM-24 |
| F4 current Python leg violates all-leg atomic staging | ✅ adopted | `build.rs::python_leg` writes the live in-repo slot directly; Rust already accepts an output path | AD-4; S1 stages every leg and tests later-leg failure |
| F5 JS/TS non-function symbols are filtered | ✅ adopted | `cache.rs` class arm is Python-only; both `graph_db` scan arms gate on `fn_tail_name` | S3 queryable-symbol policy + measured filter rates |
| F6 M1 dependency omitted S6 | ✅ adopted | M1 text already required false-clean guards while dependency graph placed S6 after M1 | dependency graph + delivery order corrected |
| F7 mixed `graph_audit` can be silently partial | ✅ adopted | default scanner is `**/*.rs` | S6 mixed-graph degraded/guard requirement |
| F8 external runtime/discovery/version contract incomplete | ✅ adopted with mechanism left open | producer roots are PATH + `~/.local/bin` + `~/.cargo/bin`; generic version helper assumes `--version` | S2/S5/S7 must validate version shape, Node policy, and GUI-safe discovery/PATH contract |
| G1 `CALLS>0` does not prove containment coverage | ✅ adopted | `fn_spans` silently skips definitions with empty `enclosing_range` | S3 records denominator and freezes acceptance bar before implementation |
| G2 JS test classification differs by layer | ✅ adopted | `graph_db::is_test_rel` lacks `__tests__` / `.spec` / `.test`; graph engine regex has them | S3 aligns or single-sources classification |
| G3 M2 backend contract under-specified | ✅ adopted | `LangSpec` is single-extension, `Bridge` has two sessions, CLI has no JS/TS flag | S5 names `--typescript-backend`, all-six routing/status, measured LangSpec policy and protocol POC |
| G4 S1 abstraction scope needs a guard | ✅ adopted as strengthening | current orchestration is combinatorial at language three; edge ontology constraint already existed in S3 | S1 explicitly stays orchestration-only and preserves Python/Rust behavior |

No reviewer finding remains pending. Child implementation EPs must treat this reviewed blueprint as the parent contract and revalidate any drifted source anchor before implementation.
