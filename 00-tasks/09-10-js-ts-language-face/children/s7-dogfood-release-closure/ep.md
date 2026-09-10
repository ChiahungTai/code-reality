# Implementation EP: JS/TS dogfood, distribution, documentation, and release closure

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S7 — Distribution, dogfood acceptance, documentation, and capability finalization
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **dependencies**: M1 requires S1 + S2 + S3 + S4 + S6; M2 requires S5; this EP consumes both gates but keeps them independently observable
> **status**: ready-for-implementation

## Implementation overview

Close the JS/TS language-face arc with real-corpus acceptance, consumer-facing prerequisite truth, help/MCP/documentation synchronization, and release-preparation consistency checks. This child does not introduce new graph semantics. It proves that the contracts delivered by S1-S6 compose into a usable consumer face and prevents distribution/documentation from advertising capabilities that have not passed their corresponding gate.

The release model remains the current PyPI single-binary layer. `code-reality`, `code-reality-lsp-bridge`, and `pyrefly-producer` remain the plugin-managed CR distributions. `scip-typescript`, `typescript-language-server`, TypeScript, and Node are external JS/TS prerequisites resolved by the runtime contracts from S2/S5. Production startup, build, query-time heal, and default tests must never run `npx -y` or silently install npm packages.

The primary M1 dogfood target is `/Users/ctai/Github/delegate-bridge`. At this planning baseline it contains 42 total `.mjs` files and 28 source-side `.mjs` files when its generated `dist/` mirror is excluded. The source-only denominator is the acceptance baseline. The exclusion itself belongs to `delegate-bridge` through its own `.code-reality.toml` change arc; code-reality must not hard-code `dist/`.

## UC inventory

### Capabilities finalized

| Capability | Gate | Closure responsibility |
|---|---|---|
| JS/TS structural build/query face | M1 | Real dogfood build, exact governed doc set, truthful language/CALLS, refs/callers/impact, freshness mutation matrix |
| JS/TS type face | M2 | Six-extension routing, real `.mjs` + typed fixture hover/diagnostics/edit-recheck, missing-backend isolation |
| Consumer installation/prerequisites | M1/M2 | Document external Node/npm tools and the actual resolver contract without adding implicit installers |
| Capability/support matrix | M1/M2 | Synchronize CLI/MCP/plugin/root/module docs after runtime acceptance |
| Release readiness | both | Version-face consistency and packaging gates only; no tag/push/publish without separate user authorization |

### Backlog / SYSTEM-MAP state

- This repository has no `backlog/` directory at the planning baseline, so there is no Backlog.md card action in this implementation child unless the repository adopts one before execution.
- No root `SYSTEM-MAP.md` exists at the planning baseline. Do not invent one solely for this arc.
- Root `AGENTS.md` and `crates/AGENTS.md` are capability/navigation sources and update only after the corresponding runtime gate passes.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Gate |
|---|---|---|---|---|
| SM-1 | Source-only delegate-bridge build | repo profile excludes generated mirror | exactly the governed source corpus reaches the final index; no target tsconfig mutation | M1 |
| SM-2 | Empty `{}` repo tsconfig | normal build | S2 derived explicit-files sidecar path succeeds; repo config remains byte-identical | M1 |
| SM-3 | Frozen caller query | selected real cross-file symbols | refs/callers are non-vacuous and reconcile to source; generated copies do not duplicate results | M1 |
| SM-4 | Graph semantic query | `review_context` / `impact_radius` on frozen dogfood symbols | result contains real structural relationships backed by JS `CALLS`, not references-only fallback | M1 |
| SM-5 | Language truth | JS-only dogfood plus TS fixture | JS nodes report JavaScript, TS nodes TypeScript; no JS/TS node reports Rust through fallback | M1 |
| SM-6 | Included source edit | edit source `.mjs` | stale detection heals once and converges | M1 |
| SM-7 | Included add/delete/rename | mutate governed source set | S4 corpus identity detects the path-set change even when mtimes alone would miss delete/rename | M1 |
| SM-8 | Excluded generated edit | edit `dist/...` excluded by repo policy | no heal loop and no doc-set mismatch | M1 |
| SM-9 | Mixed-specialized command | graph contains Rust + JS/TS | S6 returns explicit non-passing partial/unsupported status for Rust-only completeness coverage | M1 |
| SM-10 | Type hover by extension | `.js/.jsx/.mjs/.cjs/.ts/.tsx` | every extension routes to the JS/TS LSP family with its correct languageId | M2 |
| SM-11 | Type diagnostics/edit | bad then corrected TS/JS content | diagnostics appear and converge back to zero under existing overlay semantics | M2 |
| SM-12 | Type backend missing | TLS unavailable | only JS/TS type face is unavailable; M1 commands remain usable | M2 boundary |
| SM-13 | GUI-launched plugin | npm tools reachable only through documented resolver roots | behavior matches S2/S5 resolver; docs do not assume interactive-shell PATH | distribution |
| SM-14 | Default test run offline | `cargo test` with no npm/network | default suites remain hermetic; external npm tools belong to explicit acceptance lanes | regression |
| SM-15 | Release preparation | behavior changes accepted | all CR version faces can be bumped consistently and wrapper/release gates pass | release prep |

## Segment 0 — research and frozen closure constraints

### Current source anchors

- `plugin/.mcp.json` — current first-session bootstrap installs only exact-pinned PyPI `code-reality`, `code-reality-lsp-bridge`, and `pyrefly-producer`; its `node_modules/.bin` path is a retired embedded-face grace path, not a general npm dependency installer.
- `scripts/test-plugin-wrapper.sh` — enforces wrapper pin == plugin manifest == workspace version == both marketplace listings and validates the existing three-distribution uv bootstrap behavior.
- `scripts/release.sh` — owns the multi-face version bump/release-preparation workflow.
- `.github/workflows/release-wheels.yml` — publishes the three PyPI distributions; the npm embedded face is retired.
- `Cargo.toml`, `plugin/.claude-plugin/plugin.json`, `marketplace.json`, `.claude-plugin/marketplace.json`, `plugin/.mcp.json` — current version consistency faces.
- `plugin/README.md` and `plugin/skills/code-reality/SKILL.md` — consumer installation and tool semantics.
- root `AGENTS.md`, `crates/AGENTS.md` — capability and module truth.
- Parent POC evidence: `00-tasks/09-10-js-ts-language-face/poc/results.md`.

### Frozen planning evidence

1. `/Users/ctai/Github/delegate-bridge` has 42 total `.mjs` files and 28 source-side files when its generated `dist/` mirror is excluded at this baseline.
2. scip-typescript accepted all 28 governed source files when driven by an explicit CR-owned `files` config; an empty `{}` config and `--infer-tsconfig` did not cover the real corpus.
3. The six-extension fixture proved that glob/include inference can omit `.jsx`, while a CR-owned sidecar config with explicit governed files indexes `.js/.jsx/.mjs/.cjs/.ts/.tsx` exactly.
4. Current graph ingestion already recovers real function callers but requires S3 for truthful JavaScript/TypeScript labels and syntactic `CALLS` edges.
5. `enclosing_range` coverage was 192/192 function definitions on source-only `delegate-bridge` and 10/10 on the six-extension fixture.
6. The Tree-sitter JS/TS call-classification POC distinguished direct/method/optional-chain calls, constructors, and pure property reads across all six extensions.
7. `typescript-language-server 6.0.0 --stdio` + TypeScript 5.9.2 passed hover across all six extensions and diagnostics/edit convergence; S5 must represent the backend as executable + argv because `--stdio` is required.
8. Freshness source inspection found a general delete/rename blind spot in mtime-only Stage A; S4 closes it with active-face/source-set/profile corpus identity.

### Closure decisions

- **No implicit npm acquisition.** Do not extend `plugin/.mcp.json`, build, heal, or the LSP bridge into an npm installer in this arc. Consumer docs may show explicit user installation commands, but runtime resolution remains deterministic and no-network.
- **Repo-local configuration stays repo-local.** The `delegate-bridge` `.code-reality.toml` exclusion is a separate consumer-repo change. CR implementation/tests may use fixtures or a disposable copy but do not write that external repository as part of this EP unless its own session explicitly authorizes the change.
- **M1 and M2 remain separable.** Structural support can ship as accepted even when the optional JS/TS type backend is unavailable, provided docs/status say so. If implementation chooses to release both together, both gates must pass before the combined capability claim.
- **Version bump is required before a release candidate because tool behavior changes.** The implementation may prepare the version change and run consistency gates only within the user's authorized implementation scope. Tagging, pushing, publishing, or marketplace release remains a separate outward action.

## Segment 1 — freeze real-corpus dogfood baseline and oracle

### Context

Real dogfood must catch integration mistakes that hermetic fixtures can miss: generated-source duplication, config discovery, path normalization, real symbol descriptors, and graph-query semantics. At the same time, acceptance names must not become stale blueprint constants if `delegate-bridge` evolves before implementation.

### Baseline capture

At the start of S7 implementation, record a small machine-readable or textual acceptance manifest under this task's implementation evidence area containing:

```text
delegate_bridge_head = <git HEAD observed at implementation start>
governed_js_ts_doc_count = <count after repo profile>
unfiltered_js_ts_doc_count = <diagnostic count only>
frozen_symbols = [<3-6 real cross-file function symbols>]
source_locations = [<path:line anchors>]
expected_caller_relationships = [<caller -> callee examples>]
```

Use the planning examples (`mapExitCode`, `runTask`, `runReview`, `getAdapter`, resume/session-resolution functions) as discovery seeds only. Re-resolve them against the implementation-start checkout and freeze symbols that actually exist and have cross-file behavior. If a named seed disappeared, select a replacement from the same real use case and record why.

The planning denominator of 28 is a baseline fact, not a permanent invariant. If the external repo changes before implementation, record both the old planning denominator and the new implementation-start governed denominator. Acceptance is exact equality between S2's governed source set and the SCIP document set at the captured revision.

### Validation

- The acceptance manifest contains the external repo HEAD and exact governed doc set denominator.
- Frozen symbols resolve to source definitions and at least one has a cross-file function caller.
- Generated `dist/` duplicates are absent only because the consumer profile excludes them, not because CR globally filters `dist/`.

## Segment 2 — M1 real structural acceptance

### Build and corpus checks

Run the production `code-reality build` path against a disposable copy or explicitly prepared `delegate-bridge` checkout whose repo-owned profile excludes the generated tree. Before and after build, hash the target repo's existing `tsconfig.json` (currently `{}` at planning time) or record absence. The file must remain unchanged.

Verify all of the following from the published index/graph:

1. Final SCIP document paths equal the governed source corpus exactly.
2. The producer reports the installed/resolved scip-typescript face and does not perform implicit acquisition.
3. JavaScript documents/nodes carry JavaScript language; typed fixture nodes carry TypeScript.
4. The S3 acceptance threshold for `enclosing_range` coverage is met.
5. Frozen call-site examples produce `CALLS`; imports, property reads, declarations, and other non-call refs remain `REFERENCES`.
6. Queryable non-function JS/TS symbols chosen by S3 survive cache + graph ingestion without broadening Rust type policy.

### Query checks

For the frozen real symbols, capture and reconcile:

```text
code-reality scip_refs <symbol> --repo <delegate-bridge> --callers
code-reality graph_query review_context --repo <delegate-bridge> ...
code-reality graph_query impact_radius --repo <delegate-bridge> ...
```

Acceptance requires non-vacuous structural output traceable to real source call sites. A successful exit with only item-level references or zero impact edges is insufficient.

### Freshness mutation lane

Use a disposable dogfood copy and exercise the actual query-time heal/refresh path for:

- included source content edit;
- add;
- delete;
- rename;
- excluded generated edit;
- profile exclusion change.

The delete/rename cases specifically prove the S4 source-set/profile fingerprint path. For a deleted file, arrange the surviving source mtimes so the old mtime-only rule would not accidentally rescue the test. Acceptance requires the removed path to disappear from the rebuilt SCIP doc set.

### Capability-boundary checks

Exercise at least one direct and one wrapped/internal route from S6:

- mixed Rust+JS/TS `graph_audit` cannot return a clean passing result for uncovered JS/TS completeness;
- `scip_refs --audit` follows the same non-passing partial contract;
- JS/TS-targeted `hub_refs --hazard` does not report empty-clean Python dynamic coverage;
- JS/TS projection is rejected explicitly while Python projection remains valid;
- `boundary*` remains usable for its Python/Rust NT domain and is not rejected merely because unrelated JS exists in the repository.

### M1 acceptance record

M1 passes only if S1-S4 + S6 behavior is integrated in one runtime state and the real-corpus checks above pass. Record command, corpus revision, governed denominator, frozen symbols, and result evidence. Do not infer M1 from individual child unit suites.

## Segment 3 — M2 JS/TS type-face acceptance

### Hermetic routing matrix

Use the S5 fake backend to prove all extension/languageId pairs and three-backend death isolation:

| Extension | languageId |
|---|---|
| `.js` | `javascript` |
| `.jsx` | `javascriptreact` |
| `.mjs` | `javascript` |
| `.cjs` | `javascript` |
| `.ts` | `typescript` |
| `.tsx` | `typescriptreact` |

The resolver used by `lsp_status` availability checks and the resolver used for actual spawn must resolve the same executable. The JS/TS command always includes the fixed `--stdio` argument supplied by the typed backend spec, including when `--typescript-backend <executable>` overrides only the program.

### Real backend lane

With an explicitly installed `typescript-language-server` + TypeScript:

- run hover on real `.mjs` from the dogfood corpus;
- run hover across the six-extension fixture;
- introduce and then correct a typed error and verify diagnostics converge;
- kill/restart only the JS/TS backend and verify Python/Rust sessions remain healthy.

Do not require initialize `serverInfo` for correctness; planning POC observed it as unknown. Treat protocol behavior and tool results as the contract.

### Missing-backend lane

Run the bridge without a resolvable TLS executable. `lsp_status` must report only the JS/TS family as unavailable with deterministic installation/resolution guidance. M1 structural commands must remain usable because they depend on `scip-typescript`, not the live LSP backend.

## Segment 4 — distribution and consumer prerequisite truth

### Keep the CR distribution layer unchanged

Do not add scip-typescript/TLS npm installation to the current uv bootstrap. `plugin/.mcp.json` continues to own only the three CR PyPI distributions. The retired npm embedded path remains a deprecation-grace path and must not be repurposed as the JS toolchain installation model.

Consumer documentation must distinguish:

```text
CR binaries:
  code-reality / code-reality-mcp
  pyrefly-index / pyrefly-lsp
  code-reality-lsp-bridge

External JS/TS prerequisites:
  Node runtime within the supported policy
  scip-typescript for M1 structural production
  typescript-language-server + TypeScript for M2 type face
```

### Resolver documentation

Document the exact resolver behavior implemented by S2/S5, including:

- explicit executable override where public;
- repo-local `node_modules/.bin` when supported;
- `CODE_REALITY_NODE_BIN_DIR`;
- PATH / existing fallback roots that the implementation actually uses;
- GUI-launch implications;
- missing-tool failure guidance.

Do not document `npx -y` as the production execution path. It may remain in the planning POC record because that was an explicit one-shot feasibility tool.

### Runtime support wording

Node 24 is only planning-time observed-compatible. Consumer docs must use the runtime policy frozen from upstream/package compatibility evidence in S2/S5 and avoid turning this workstation observation into a support guarantee.

## Segment 5 — help, MCP, plugin skill, and instruction synchronization

Only after M1/M2 behavior reaches its acceptance gate, update all affected public descriptions from Python/Rust shorthand to the truthful support matrix:

```text
root AGENTS.md Capabilities
crates/AGENTS.md module/build/type-face guidance
plugin/README.md prerequisites and examples
plugin/skills/code-reality/SKILL.md standalone tool facts/pitfalls
CLI --help text for build / producer / lsp bridge flags
MCP tool descriptions for build/query/type faces and specialized-tool boundaries
```

Required semantic claims:

- `build`, normal `scip_refs` callers/closure, graph DB/query family: JS/TS first-class after M1.
- LSP hover/check/edit: JS/TS first-class only when M2 backend prerequisites resolve.
- `graph_audit`: Rust completeness oracle; JS/TS coverage is unsupported/partial and non-passing when relevant.
- `scip_refs --audit`: same completeness boundary as the wrapped audit.
- static `hub_refs`: generic graph aggregation where applicable; `--hazard` remains Python-specific dynamic safety for JS/TS targets.
- `project`: current Python/pyrefly projection only for this arc.
- `boundary*`: current NT Python/Rust domain.

Instruction edits must follow the repository instruction-writing rule and update the single source plus dependent references in one change arc.

## Segment 6 — regression, test audit, and release preparation

### Progressive verification

1. **MIN** — targeted S1-S6 integration suites, source-only dogfood build, one frozen caller/impact example, delete freshness case, one JS/TS hover/diagnostic case.
2. **SAMPLE** — full M1 mutation matrix, all six LSP extensions, specialized-tool guard routes, missing-backend path, GUI-safe resolver fixtures.
3. **FULL** — full workspace `cargo test`, release/wrapper consistency checks, complete dogfood command matrix, documentation/help parity checks.

The final default suite must be hermetic. Tests that invoke real npm packages or external consumer repositories are explicit acceptance lanes and must skip/fail with a clear prerequisite rather than downloading dependencies.

### Required gates

At minimum run:

```text
cargo test
bash scripts/test-plugin-wrapper.sh
```

Run the relevant release-script dry-run/preflight contract if available at implementation time. Verify all version faces are consistent before and after any prepared bump. Do not run a real release, tag, push, trusted-publishing job, or marketplace publication from this EP without explicit user authorization.

Run `/audit-test` semantics on the new JS/TS tests. Pay special attention to:

- fake producer tests that could pass without enforcing exact corpus filtering;
- CALLS tests that could accept any non-zero call count instead of exact sites;
- freshness tests whose mtimes accidentally trigger the old path rather than the new delete/rename fingerprint;
- type-backend fakes that mirror implementation data rather than protocol behavior;
- guard tests that assert warning text but ignore exit/status semantics.

### Version preparation

Because the public tool behavior changes, prepare the next version through the repository's single release workflow rather than manually editing one face. The preparation must keep `Cargo.toml`, plugin manifest, both marketplace listings, and wrapper pin in lockstep and must satisfy `scripts/test-plugin-wrapper.sh`.

Actual release publication is outside this implementation plan's automatic actions.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. Revalidate S1-S6 implementation evidence and dependency order at the implementation-start HEAD.
2. Capture the real `delegate-bridge` acceptance manifest and consumer-repo profile assumption.
3. Run M1 dogfood only after S1-S4 + S6 are integrated.
4. Run M2 independently after S5; a missing type backend cannot invalidate M1.
5. Synchronize help/MCP/plugin/instruction claims only after the corresponding gate passes.
6. Run full regression, test-quality audit, wrapper/release consistency, and prepare the version bump.

### Cross-child contracts consumed

- **S1**: selected producer legs stage and publish atomically; explicit `typescript` route is real, not a placeholder.
- **S2**: one governed six-extension corpus helper, explicit-files derived sidecar config, deterministic external producer resolution.
- **S3**: truthful JS/TS node language, queryable non-function policy, exact syntactic CALLS classifier, frozen `enclosing_range` acceptance bar.
- **S4**: active-face/source-set/profile identity participates in rebuild decisions, covering delete/rename/profile changes.
- **S5**: typed program+argv backend with fixed `--stdio`, six extension language IDs, independent third session and shared resolver for status/spawn.
- **S6**: specialized-tool support matrix and non-passing false-clean guards across direct/internal/MCP routes.

## Verification strategy

The implementation completion record must separate:

- hermetic L1-L3 suite evidence;
- real external producer/LSP acceptance evidence;
- real dogfood source/graph reconciliation evidence;
- release-preparation consistency evidence;
- documentation/help parity evidence.

No single green `cargo test`, successful build exit, or non-empty graph is sufficient to claim the full arc.

## Completion / finalization

- Parent M1 status can advance only after the S7 M1 acceptance record exists.
- Parent M2 status can advance only after the S7 M2 acceptance record exists, or the project explicitly splits M2 into a later approved arc and advertises structural-only support.
- Update root/module/plugin documentation only to the capability level actually accepted.
- No Backlog.md action unless `backlog/` exists by implementation time.
- No SYSTEM-MAP action unless a root `SYSTEM-MAP.md` exists by implementation time.
- Do not archive the parent blueprint until its final advertised capability and regression evidence converge.
- Do not commit, tag, push, or publish solely because this plan reaches the release-preparation step; those outward actions follow the active session's authorization rules.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Review must challenge release-boundary creep, dogfood denominator drift, M1/M2 gate independence, npm/network hermeticity, and whether real-corpus checks prove graph semantics rather than merely successful command exits.
