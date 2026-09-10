# Implementation EP: JS/TS graph language, symbols, and CALLS semantics

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S3 — Make graph materialization language-correct and call-aware for JS/TS
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **depends on**: S1 language mapping; S2 staged JS/TS SCIP corpus for real acceptance
> **status**: ready-for-implementation

## Implementation overview

Make current producer-neutral SCIP ingestion truthful for JavaScript/TypeScript. The existing engine already reads `scip-typescript` indexes and can attribute references to enclosing functions, but three baseline assumptions are Python/Rust-specific:

1. every non-Python graph node is labeled `Rust`;
2. only `name().` function-shaped symbols survive graph materialization, and only Python `name#` symbols are admitted to the `scip_refs` cache;
3. syntactic CALLS classification runs only on `.py`, leaving every JS/TS graph edge as `REFERENCES`.

This EP fixes those semantics without adding a new edge ontology. It introduces a document-aware queryable-symbol policy, reuses S1's extension→language mapping, adds a Tree-sitter JS/TS call collector, and unifies test-file classification. Type/class-like JS/TS nodes use a non-entrypoint `Type` node kind so they become queryable/connected without being misrepresented as functions.

Planning POCs materially de-risk this work: source-only `delegate-bridge` has 192/192 function definitions with `enclosing_range`, current caller attribution finds real cross-file callers, Tree-sitter classified the six extension fixture exactly, and real `scip-typescript` emitted `Named#` / `Greeter#` definitions that current CR drops.

## UC inventory

### Capabilities updated

| Capability | Baseline | This EP end state |
|---|---|---|
| `scip_refs` symbol truth | JS/TS functions partly usable; class/type symbols dropped | Functions + selected class/interface/type `#` symbols queryable |
| Graph node language | non-Python → Rust | path-derived Python/Rust/JavaScript/TypeScript |
| Graph node set | function-shaped only | functions + JS/TS/Python queryable type-like nodes; Rust `Type#` policy unchanged |
| Graph edge semantics | JS/TS references only | syntax-validated `CALLS` + remaining `REFERENCES` |
| Test classification | DB and query regex disagree | one shared path policy |

### New UC

**Use unified graph queries on JS/TS without false language metadata or references-only call structure.**

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | JS function definition | `.js/.mjs/.cjs/.jsx` | Node language `JavaScript`, kind `Function` | graph DB row |
| SM-2 | TS function definition | `.ts/.tsx` | Node language `TypeScript`, kind `Function` | graph DB row |
| SM-3 | JS/TS class/interface/type-like symbol | real `.../Name#` | Queryable by bare name and materialized kind `Type` | `scip_refs` + graph DB |
| SM-4 | Rust `Type#` | `.rs` symbol | Remains outside bare-class query policy | regression guard |
| SM-5 | Direct call | `foo()` | `CALLS` edge | parser/site fixture |
| SM-6 | Method call | `obj.foo()` | `CALLS` edge to resolved function symbol | same |
| SM-7 | Optional chain | `obj?.foo?.()` | `CALLS` edge | same |
| SM-8 | Constructor | `new Foo()` | CALLS classification can match type/class symbol via class segment | same |
| SM-9 | Pure property reference | `const x = obj.foo` | Remains `REFERENCES` | negative fixture |
| SM-10 | JSX/TSX embedded call | `{foo()}` | `CALLS` edge | six-extension fixture |
| SM-11 | Syntax-error source during live edit | Tree-sitter parse error | Warn/degrade that file to references; graph build stays explicit | warning fixture |
| SM-12 | Missing enclosing range | producer DEF has no span | Reference falls to item-level; coverage metric exposes degradation | acceptance gate |
| SM-13 | JS test path | `__tests__/`, `*.test.js`, `*.spec.tsx` | DB `is_test` and graph filter agree | shared helper |
| SM-14 | Existing Python/Rust graph | current fixtures | Languages/CALLS/queryable policy unchanged | full regression |
| SM-15 | Real source-only dogfood | `mapExitCode` and peers | Non-vacuous CALLS/review/impact results traceable to source | M1 acceptance |

## Segment 0 — research and POC results

### Current source anchors

- `crates/code-reality/src/engine.rs:103` — `python_face` gates Python-specific class querying.
- `crates/code-reality/src/engine.rs:110` — `fn_tail_name` recognizes `name().`.
- `crates/code-reality/src/engine.rs:235` — `fn_spans` uses SCIP `enclosing_range`; empty spans are silently unavailable for function containment.
- `crates/code-reality/src/cache.rs:72` — cache build's queryable-symbol gate; baseline lines 80-85 allow class tail only when `python_face`.
- `crates/code-reality/src/graph_db.rs:223` — DB `is_test_rel` handles `tests/` and Python `test_` shapes.
- `crates/code-reality/src/graph_db.rs:239` — `infer_language` returns Python for three prefixes and Rust for everything else.
- `crates/code-reality/src/graph_db.rs:291` — graph scan filters every non-`fn_tail_name` occurrence.
- `crates/code-reality/src/graph_db.rs:500` — call marks are built only from `.py` sources.
- `crates/code-reality/src/graph_db.rs:595` — inserted graph nodes are hard-coded `Function`.
- `crates/code-reality/src/graph_db.rs:651` — CALLS matching already supports function tail + class segment, useful for constructors.
- `crates/code-reality/src/graph_engine.rs:468` — query-side JS test regex already knows `__tests__`, `.spec.[jt]s(x)`, `.test.[jt]s(x)`.
- `crates/code-reality/src/graph_engine.rs:196` — `CALLS` is already a first-class weighted edge; no ontology addition is needed.

### POC evidence absorbed

From `../../poc/results.md`:

- Current source-only JS graph: `Rust=192`, `REFERENCES=293`, `CALLS=0`.
- `scip_refs mapExitCode --callers` still finds three real callers: containment path is reusable.
- Dogfood function-span coverage is `192/192 = 100%`.
- Six-extension fixture span coverage is `10/10 = 100%`.
- `Greeter#` and `Named#` are real scip-typescript type-like DEFs; current `scip_refs Greeter` fails.
- Tree-sitter POC exact counts passed for direct/method/optional/new/JSX/TSX/non-call cases.

### Frozen acceptance bars

- Hermetic function fixture: **100%** queryable function definitions must have usable `enclosing_range`.
- Real `delegate-bridge` acceptance corpus: **>=99%** queryable function DEF coverage, and every frozen acceptance symbol must have a span. Current observed baseline is 100%; the 99% floor prevents one exotic producer omission from forcing an architectural rewrite while still rejecting broad degradation.
- CALLS fixture: exact expected site set, not merely `CALLS > 0`.
- Non-call fixture: exact negative set for property/import/type-only references.

## Segment 1 — document-aware queryable symbol and language policy

### Context

The same symbol-shape policy must drive cache ingestion and graph materialization. Duplicating a Python-vs-other gate in each path is how current JS/TS class/type definitions disappear.

### Core design

Add a shared internal helper, preferably beside existing symbol-tail parsing in `engine.rs`:

```rust
enum QueryableKind {
    Function,
    Type,
}

struct QueryableSymbol<'a> {
    name: &'a str,
    kind: QueryableKind,
}

fn queryable_symbol(symbol: &str, rel_path: &str) -> Option<QueryableSymbol<'_>> {
    if let Some(name) = fn_tail_name(symbol) {
        return Some(Function(name));
    }

    let type_name = class_tail_name(symbol)?;
    let lang = LanguageFace::from_path(Path::new(rel_path));
    if python_face(symbol) || matches!(lang, Some(JavaScript | TypeScript)) {
        return Some(Type(type_name));
    }
    None
}
```

This intentionally leaves Rust `Type#` outside the bare-class policy until a separately reviewed Rust contract says otherwise.

Use `queryable_symbol` in both:

- `cache::build_db` tail/name collection;
- `graph_db::scan` for protobuf and SQLite arms.

`ScanRows` must retain enough kind information for node insertion, either as a per-symbol map or a richer DEF row. Do not infer node kind again from name strings later.

### Graph node insertion

Replace the hard-coded `Function` node kind with the queryable kind:

```text
Function-shaped symbol -> kind = "Function"
admitted `#` symbol     -> kind = "Type"
```

`Type` is intentionally broad because the `#` suffix alone does not distinguish class vs interface vs type alias reliably. Do not label every `#` as `Class` without a SymbolInformation-based proof.

Graph language is derived from the defining document path using S1's `LanguageFace::from_path`:

```text
.py                 -> Python
.rs                 -> Rust
.js/.jsx/.mjs/.cjs  -> JavaScript
.ts/.tsx             -> TypeScript
```

For legacy/unknown document extensions, retain the old symbol-prefix fallback rather than silently relabeling unknown producers.

### Validation

- Real-shaped scip-typescript symbols `Greeter#`, `Named#`, `helper().`.
- Python class query regression remains enabled.
- Rust `Type#` remains filtered.
- Graph DB stores `Type` nodes but entrypoint inference still considers only Function/Test as before.
- Language fixtures for every supported extension.

## Segment 2 — Tree-sitter JS/TS call-site classifier

### Context

SCIP occurrence roles do not say "this reference is a call". Current Python support re-parses source to build `(file,line,callee-name)` call marks. JS/TS needs the same role with a syntax-aware parser.

### Dependencies

Add and lock through Cargo.lock after implementation validation:

```toml
tree-sitter = "0.25"
tree-sitter-javascript = "0.25.0"
tree-sitter-typescript = "0.23.2"
```

The exact versions may advance if the implementation session proves a newer compatible set; the POC baseline above is known-good.

### Files

```text
crates/code-reality/src/
├── js_ts_calls.rs       # new syntax-aware collector
├── py_calls.rs          # unchanged behavior; optional shared tiny types only
└── graph_db.rs          # combine Python + JS/TS marks
```

### Call collector pseudo code

```rust
fn call_sites(repo: &Path, rels: &BTreeSet<String>) -> (CallSiteSet, Vec<String>) {
    for rel in rels {
        let language = match extension {
            js|jsx|mjs|cjs => tree_sitter_javascript::LANGUAGE,
            ts             => LANGUAGE_TYPESCRIPT,
            tsx            => LANGUAGE_TSX,
            _              => continue,
        };
        parse source;
        if root.has_error() {
            warn once for file;
            // no speculative CALLS marks from a broken parse
        }
        walk syntax tree:
            call_expression -> static_callee_tail(function child)
            new_expression  -> static_callee_tail(constructor child)
            if a stable identifier/property tail exists:
                marks.insert((rel, start_line + 1, tail))
    }
}
```

`static_callee_tail` supports:

- identifier calls: `foo()` → `foo`;
- member calls: `obj.foo()` → `foo`;
- optional-chain calls: `obj?.foo?.()` → `foo`;
- constructor calls: `new Foo()` → `Foo`;
- parenthesized/static forms only when the parser tree still exposes one unambiguous terminal identifier.

Computed/dynamic forms such as `obj[key]()` do not mint a guessed name. Their SCIP references remain `REFERENCES` unless a later semantic mechanism can prove the target.

### Integration with graph materialization

Build two mark sets independently:

```rust
py_marks = py_calls::call_sites(... .py rels ...)
js_ts_marks = js_ts_calls::call_sites(... six JS/TS rels ...)
call_marks = union(py_marks, js_ts_marks)
```

Rust behavior stays as today. LSP-harvest remains REFERENCES-only according to its existing contract.

The edge writer continues to use existing `fn_tail_name(callee)` and `class_segment(callee)` matching. Constructor references to `Foo#` can therefore become CALLS without adding a constructor edge kind.

### Validation

Parser unit fixtures must assert exact site sets for:

- direct/method/optional/new;
- JS + MJS + CJS;
- JSX + TSX embedded calls;
- TS generic/type syntax;
- property-only access;
- import/export/type-only reference;
- computed dynamic call (no guessed mark);
- syntactically broken file (warning + zero marks for ambiguous portion).

Graph integration fixture then verifies exact `CALLS` vs `REFERENCES` rows, not only parser counts.

## Segment 3 — containment coverage and test-path single source

### Containment coverage

Add a small measured statistic during graph build or acceptance tooling: queryable function DEF count vs function DEFs with usable `enclosing_range`. It does not need to become noisy normal CLI output if current report shape would regress; JSON/build diagnostics or test helper evidence is sufficient.

Rules:

- empty `enclosing_range` remains a legal producer condition and falls back to item-level attribution;
- malformed non-empty ranges keep the existing fail-loud warning;
- acceptance gates use the denominator, so a few passing CALLS cannot hide widespread span loss.

### Test path single source

Move path classification into one shared helper consumed by both graph DB insertion and graph engine filtering. The union policy is:

```text
path component tests/
Python test_*.py
path component __tests__/
*.spec.js|jsx|ts|tsx
*.test.js|jsx|ts|tsx
```

Preserve current Rust/Python `tests/` behavior. Do not make filename-only `.test.*` logic apply to unrelated extensions.

### Validation

- `is_test` DB value and graph query `is_test_file` agree on the same table of paths.
- Existing Rust/Python test classification fixtures unchanged.
- Dogfood span coverage recorded against the predeclared >=99% bar.

## Segment 4 — graph-query acceptance on real JS/TS

### Context

Unit correctness does not prove graph queries use the new CALLS structure. This segment is the S3 L3/L4 consumer-path gate.

### Acceptance corpus

Use S2's source-only `delegate-bridge` graph and freeze several symbols at the child implementation baseline, including `mapExitCode` and functions selected in S7.

For each frozen symbol:

1. `scip_refs <symbol> --callers` gives source sites.
2. Direct `rg`/source inspection confirms a representative call site.
3. Graph DB contains the corresponding `CALLS` edge when syntax is a call.
4. `graph_query review_context` and `impact_radius` return non-vacuous results where applicable.
5. Imports/constant/property references stay item-level or `REFERENCES` rather than being promoted.

Acceptance also includes a class/type symbol (`Greeter`-shape hermetic fixture or a real dogfood type if one exists) to prove queryable `#` ingestion.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. Land shared queryable/language/test-path helpers with graph DB/cache regression tests.
2. Land Tree-sitter collector and exact parser tests.
3. Connect marks to graph materialization and run exact edge tests.
4. Measure span coverage and run S2 real-dogfood graph queries.
5. S6 capability guards must still land before M1 support is advertised.

### Cross-child contract exported

- S4 can determine JS/TS index faces from document extensions using the same `LanguageFace` mapping.
- S6 can inspect graph `language` values reliably after this segment.
- S7 may use graph-query outputs as JS/TS structural evidence only after this S3 acceptance passes.

## Verification strategy

1. **MIN** — language labels, `Greeter#` queryability, direct/non-call parser fixture.
2. **SAMPLE** — six extension syntax matrix, constructor, test paths, exact edge rows.
3. **FULL** — affected cache/caller/graph_db/graph_engine tests, then real S2 dogfood and full `cargo test -p code-reality` before M1 integration.

No npm invocation belongs in default S3 unit tests. The parser is linked Rust code; producer-specific symbol shapes are frozen as tiny fixtures/constants and cross-checked in the real acceptance lane.

## Completion / finalization

- Record Tree-sitter dependency choice and measured dogfood span coverage in parent evidence.
- Update root/plugin support claims only in S7.
- Run `/audit-test` semantics on parser/graph tests.
- Do not change `EDGE_KINDS` or CALLS weighting as part of this arc.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Review must specifically challenge the `Type` node-kind choice, Rust `Type#` non-regression, and call-site false-positive paths.
