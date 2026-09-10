# Implementation EP: truthful language capability boundaries

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S6 — Close truthful capability boundaries for language-specific CR tools
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **depends on**: S3 truthful graph language labels for runtime graph guards
> **blocks**: M1 structural support claim
> **status**: ready-for-implementation

## Implementation overview

Prevent the new JS/TS graph face from making existing Python/Rust-specialized commands look more capable than they are. Core SCIP/graph queries become first-class after S1-S4, but several commands contain language-specific analyzers or symbol-identity assumptions:

- `graph_audit` scans Rust source and reconciles rust-analyzer symbols;
- `scip_refs --audit` calls that same `graph_audit` path before its second pass;
- `hub_refs --hazard` invokes a Python AST/regex dynamic-dispatch safety net;
- `project` invokes `overlay-gen` with pyrefly/Python symbol identity;
- `boundary*` is an explicit NautilusTrader Python↔Rust domain tool.

S6 does not rewrite those analyzers into JS/TS implementations. It adds one explicit support matrix and runtime guards so unsupported/partial coverage cannot produce a clean-looking success. Because the parent M1 claim includes truthful safety boundaries, S6 is an M1 prerequisite rather than a release-only documentation task.

## UC inventory

### Capability added

**Expose truthful per-language capability boundaries instead of false-clean special-tool output.**

### Support matrix frozen by this EP

| Tool face | Python | Rust | JavaScript/TypeScript | Runtime behavior after S6 |
|---|---|---|---|---|
| `scip_refs <sym>` refs/defs | first-class | first-class | first-class | generic SCIP query |
| `scip_refs --callers/--closure` | first-class | first-class | first-class | containment + graph-independent refs |
| `graph_db`, `graph_query` | first-class | first-class | first-class after S3/S4 | unified graph |
| `graph_audit` | existing semantics | Rust completeness oracle | unsupported/partial | never clean-silent when JS/TS present |
| `scip_refs --audit` | existing semantics | wraps Rust audit | unsupported/partial | same guard as graph_audit |
| `hub_refs` static aggregation | existing | existing | usable if graph query resolves | no Python dynamic claim |
| `hub_refs --hazard` / auto-hazard | Python dynamic rules | existing mixed behavior | unsupported dynamic-hazard layer | static result + explicit hazard limitation |
| `project` overlay | Python/pyrefly | real graph may contain Rust | JS/TS planned sources unsupported | reject misleading projection |
| `boundary*` | NT Python side | NT Rust side | outside domain | docs/help state domain; no generic JS claim |

The matrix describes current intended behavior, not a promise that every special tool will eventually become language-neutral.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | Pure JS graph audit | `graph_audit --repo js` | fail/degrade explicitly before a clean verdict | nonzero + message |
| SM-2 | Rust+JS mixed audit | graph contains both | Rust findings may be rendered, but overall coverage marked partial/non-passing | no exit-0 clean |
| SM-3 | Existing Rust audit | Rust-only graph | existing output/exit semantics unchanged | regression |
| SM-4 | `scip_refs --audit` on JS/TS | wrapper mode | same unsupported/partial guard as direct graph_audit | no bypass |
| SM-5 | Static `hub_refs` on JS function | no forced hazard | static caller aggregation works | graph-backed result |
| SM-6 | `hub_refs --hazard` on JS symbol | forced hazard | do not run Python AST as if authoritative; mark hazard layer unsupported | explicit warning/status |
| SM-7 | Auto hazard on low-ref JS symbol | threshold triggers | same limitation, no false "zero hazards" | explicit status |
| SM-8 | Python symbol in mixed JS repo | hazard target resolves Python | existing Python hazard rules still run | target-language scoped |
| SM-9 | JS/TS project overlay | plan/source is `.js/.ts/...` | reject before pyrefly overlay is treated as valid | actionable unsupported |
| SM-10 | Python projection in repo that also has JS | projected sources Python | existing project flow remains usable | no graph-wide overblocking |
| SM-11 | Boundary command in JS-containing repo | explicit NT boundary query | command retains its domain semantics; docs do not imply JS analysis | no generic-support claim |
| SM-12 | CLI/MCP help | user inspects descriptions | same support matrix wording | docs/runtime parity |

## Segment 0 — research and source anchors

### Current specialized paths

- `crates/code-reality/src/graph_audit.rs:147` — default scan pattern is `**/*.rs`.
- `crates/code-reality/src/graph_audit.rs:243` — rust-analyzer is the external audit oracle.
- `crates/code-reality/src/graph_audit.rs:418` — CLI entry point currently has environment/vacuous guards but no JS/TS coverage guard.
- `crates/code-reality/src/cli.rs:761` — `scip_refs --audit` runs `graph_audit::audit`; therefore guarding direct `graph_audit` text alone is insufficient.
- `crates/code-reality/src/hazard.rs:61` — dynamic symbol facts come from Python AST semantics.
- `crates/code-reality/src/hub_refs.rs:438` — hazard stage owns dynamic safety-net execution.
- `crates/code-reality/src/hub_refs.rs:536` — public hub_refs route.
- `crates/code-reality/src/project.rs:257` — project overlay spawns `overlay-gen` from pyrefly producer distribution.
- `crates/code-reality/src/project.rs:295` — overlay identity guard explicitly expects `pyrefly python <project> <version>`.
- `crates/code-reality/src/boundary.rs:1` — boundary contract is Python symbol → Rust truth for NT.

### Parent-review finding absorbed

`graph_audit` against a mixed Rust+JS/TS repo can otherwise report clean while never scanning the JS/TS files. The guard must consider mixed graphs, not only explicit "JS invocation" shapes.

### Architecture decisions

1. Use runtime graph language labels from S3 as the common guard input where a graph is already required.
2. Scope guards to the **capability being invoked**, not the mere presence of JS/TS anywhere in the repo. A Python hazard query in a mixed repo remains valid.
3. Unsupported completeness/safety coverage must be non-passing to automation. A warning on exit 0 is not enough for `graph_audit`/`scip_refs --audit` because those commands are used as gates.
4. Partial mixed audit may still render the Rust results for human value, but the final exit/status is partial/non-passing.
5. Do not broaden `boundary*` merely to make the matrix symmetrical.

## Segment 1 — shared graph language coverage query

### Context

Multiple specialized commands need a reliable answer to "does this graph contain JS/TS nodes?" without each reimplementing SQLite queries.

### Core helper

Add a small graph DB read helper:

```rust
#[derive(Default)]
struct GraphLanguages {
    python: bool,
    rust: bool,
    javascript: bool,
    typescript: bool,
    unknown: BTreeSet<String>,
}

fn graph_languages(db: &Path) -> Result<GraphLanguages, String> {
    SELECT DISTINCT language FROM nodes WHERE kind != 'File';
}
```

Unknown language values are preserved and may force a conservative partial/unsupported result in completeness gates. Do not normalize unknown to Rust.

For symbol-targeted tools, add a query that returns the language(s) of resolved matching node symbols/names. Ambiguity across languages yields a conservative partial hazard result rather than arbitrarily picking one language.

### Validation

- Pure/mixed language fixtures.
- Unknown label preserved.
- Empty graph returns empty coverage and lets existing vacuous guards decide; do not mask them.

## Segment 2 — `graph_audit` and `scip_refs --audit` non-passing partial coverage

### Direct `graph_audit`

Before claiming a clean audit, inspect graph languages:

```rust
let coverage = graph_languages(&graph)?;
let has_js_ts = coverage.javascript || coverage.typescript;
```

Behavior:

- no JS/TS → baseline behavior unchanged;
- pure JS/TS/no Rust audit target → return explicit unsupported environment/capability result, nonzero;
- Rust + JS/TS → run/render Rust audit if useful, append a structured coverage statement, and return a non-passing partial exit code;
- JSON adds additive coverage fields such as `audited_languages` and `unsupported_languages`; do not silently keep the old four-key shape while exit meaning changes.

The exact nonzero code should reuse the project's existing "fixable environment/capability unsupported" convention, preferably `2` rather than pretending the audit found source correctness defects.

### `scip_refs --audit`

The wrapper must call the same coverage preflight/helper before or around `graph_audit::audit`. It cannot bypass the direct command's guard by invoking the internal `audit()` function.

Text should explicitly recommend normal `scip_refs`/callers/graph queries for JS/TS structural facts while saying there is no JS/TS completeness oracle in this arc. Do not recommend `graph_audit` as the fallback.

### Validation

- Pure JS graph cannot produce `[OK] graph_audit ...` + exit 0.
- Rust+JS graph may show Rust findings but exits partial/nonzero and names JS/TS as unaudited.
- Rust-only exact-output fixtures remain unchanged where feasible.
- `scip_refs --audit` mirrors the same behavior.

## Segment 3 — target-language guard for `hub_refs --hazard`

### Context

Static hub aggregation uses graph/ref facts and can remain useful for JS/TS. The hazard layer is the language-specific part. Guard that layer rather than disabling the whole command.

### Core behavior

After resolving the requested symbol, determine its graph language set.

```text
Python target -> existing hazard_stage
JS/TS target -> skip Python AST/dispatch rules; mark dynamic hazard coverage unsupported
ambiguous Python + JS/TS target -> static aggregation allowed, dynamic hazard result partial/ambiguous
Rust target -> preserve existing baseline behavior
```

Both explicit `--hazard` and threshold-triggered auto-hazard must pass through this guard.

JSON must distinguish:

```json
{
  "hazard_level": "unsupported-js-ts",
  "hazard_findings": [],
  "hazard_supported": false
}
```

An empty findings array with `hazard_supported=false` is not a clean hazard result. Text output must say the same.

### Validation

- Forced and auto hazard JS paths.
- Python target in mixed repo still executes current hazard rules.
- Ambiguous target cannot claim supported-clean.
- Static caller counts remain available for JS/TS.

## Segment 4 — projection and boundary capability guards

### `project`

Reject a JS/TS projection before pyrefly-generated symbol identities can be treated as meaningful. Preflight the plan's declared/planned source paths or generated overlay report:

- any planned source extension in the six JS/TS set → explicit unsupported result;
- Python-only planned sources → existing project flow unchanged even when real graph also contains JS/Rust;
- unknown/non-source plans keep existing validation.

The rejection message states that this arc adds real JS/TS graph ingestion, not JS/TS hypothetical overlay symbol minting.

### `boundary*`

No new analyzer is implemented. Update help/docs to identify the command as the NT Python↔Rust boundary domain. Do not add a graph-wide JS guard that prevents valid NT use merely because a repo also contains JS tooling.

### Validation

- JS/TS project source rejected before overlay claim evaluation.
- Python project regression passes in a mixed-language repo fixture.
- Boundary command tests unchanged; only descriptions/support matrix change if needed.

## Segment 5 — one support matrix across CLI, MCP, plugin docs

Update the user-facing descriptions from one source of terminology:

- normal `scip_refs`, callers/closure, graph_db/query: JS/TS first-class after M1;
- audit completeness: Rust-specific/partial on JS/TS;
- dynamic hazard: Python-specific;
- project overlay: Python-only;
- boundary: NT Python↔Rust domain.

Touch at least:

```text
crates/code-reality/src/* command HELP/MCP descriptions
plugin/skills/code-reality/SKILL.md
plugin/README.md
crates/AGENTS.md / root AGENTS.md at finalization timing
```

Instruction file edits obey the repo's instruction-writing rules and land only when runtime behavior has converged.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. S3 must land truthful graph language labels before runtime graph coverage guards can be trusted.
2. Add shared language coverage helper.
3. Guard graph audit + audit wrapper.
4. Guard hazard stage by target language.
5. Guard JS/TS projection; clarify boundary domain.
6. Update descriptions and run cross-command tests.

### Cross-child contract exported

- S7 may call M1 "structurally supported" only after these guards pass.
- S7 documentation uses this matrix verbatim in meaning, even if prose differs.
- A future JS/TS audit/hazard/project extension is a new UC, not hidden inside S6.

## Verification strategy

1. **MIN** — pure JS graph_audit non-passing; JS `--hazard` says unsupported; Rust audit unchanged.
2. **SAMPLE** — Rust+JS partial audit, wrapper audit, Python target in mixed repo, JS projection rejection.
3. **FULL** — affected graph_audit/hub_refs/hazard/project/MCP suites plus full `cargo test -p code-reality` and S7 real dogfood command matrix.

Tests must assert exit/status semantics, not only warning text. A partial audit returning exit 0 is a failed test even if stderr contains a warning.

## Completion / finalization

- Update parent M1 evidence with direct tests for mixed-graph false-clean prevention.
- Documentation/support matrix final publication occurs in S7.
- Run `/audit-test` semantics on capability-boundary tests, especially tests that could pass without actually invoking the guarded path.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Review must try to find a bypass where an internal function call (`scip_refs --audit`, auto-hazard, MCP route) escapes the public CLI guard.
