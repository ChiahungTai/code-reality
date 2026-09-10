# JS/TS language-face planning POC results

These are planning-time feasibility checks for the parent blueprint and its seven implementation EPs. They do not modify code-reality product source or the real `delegate-bridge` checkout. Temporary executable fixtures lived under `.agent-tmp/js-ts-poc-1789031719/`; the durable facts needed by later sessions are recorded here.

## Environment and external tool faces

- Node: `v24.20.0`.
- `@sourcegraph/scip-typescript`: `0.4.0`; `scip-typescript --version` prints the plain version string `0.4.0`.
- `typescript-language-server`: `6.0.0`; `--stdio` is an explicit required CLI flag.
- Tree-sitter parser POC: `tree-sitter-javascript 0.25.0` + `tree-sitter-typescript 0.23.2` on `tree-sitter 0.25`.
- Neither `scip-typescript` nor `typescript-language-server` was installed on the normal PATH; POCs invoked fixed npm versions through `npx -y`. Production build/heal remains forbidden from doing implicit downloads.

## P1 — existing config presence is not usability

Acceptance corpus copy: `/Users/ctai/Github/delegate-bridge`.

The repository's root `tsconfig.json` is `{}`. Calling `scip-typescript index` with that config returned exit `1` and:

```text
error: no files got indexed
```

Using a CR-derived JS config with `allowJs`, `NodeNext`, and a source-only corpus returned exit `0` and a `1,376,364` byte SCIP index. This confirms that S2 needs a usability/postcondition gate; `tsconfig.json` existence is insufficient.

## P2 — governed corpus and source/dist boundary

The filesystem contains 42 `.mjs` files, of which 28 remain when `dist/` is excluded. A derived source-only config produced exactly 28 SCIP documents. An exact sorted set comparison between the expected source files and SCIP `Document.relative_path` values passed with no diff.

The same 28-document SCIP produced the following graph under current CR:

```text
nodes=192
edges=293
calls_edges=0
item_level_refs=953
non_fn_defs_skipped=14863
node language: Rust=192
edge kind: REFERENCES=293
```

`scip_refs mapExitCode --callers` still found three real source-side callers (`runTask`, `runReview`, `runBgWorker`) plus item-level references. This confirms that the existing SCIP/caller-attribution layer is reusable while graph language/CALLS semantics are not yet truthful.

## P3 — function containment coverage

On the source-only `delegate-bridge` SCIP:

```text
documents=28
function definitions=192
function definitions with enclosing_range=192
coverage=100.00%
```

On the six-extension synthetic fixture, coverage was also `10/10 = 100%` after all six documents were included. S3 may therefore use current containment attribution as the baseline, while retaining a measured coverage gate so future producer changes cannot silently degrade it.

## P4 — six-extension config construction

A synthetic fixture contained one file for each face: `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, `.tsx`.

A conventional derived config using `include=["src/**/*"]` produced only five SCIP documents: `.jsx` was omitted by the TypeScript project file selection in this configuration. `tsc --listFiles` showed the same omission, so this was not a CR parsing issue.

Replacing the glob with an explicit `files` list produced all six files under both TypeScript and `scip-typescript`:

```text
documents=6
src/lib.ts
src/view.tsx
src/plain.js
src/plain.jsx
src/esm.mjs
src/common.cjs
```

The same explicit absolute `files` list also worked when the derived `tsconfig.json` lived under the CR-owned `.code-reality/scip/...` sidecar instead of the repository root. This is the preferred fallback shape: the target repository is not mutated, and producer input can be derived from the same governed source set that freshness later consumes.

## P5 — current queryable-symbol gap

The six-extension SCIP contained two class-like `#` definitions:

```text
.../lib.ts/Named#
.../lib.ts/Greeter#
```

Current `scip_refs Greeter` returned `查無 DEF` / exit `1`, while `scip_refs helper --callers` found three callers. This validates S3's requirement to make the queryable-symbol policy document/language-aware without accidentally enabling every Rust `Type#`.

## P6 — syntax-aware CALLS classifier candidate

A standalone Rust POC parsed all six extensions using Tree-sitter. Frozen source cases contained direct calls, method calls, optional-chain calls, constructor `new`, JSX/TSX embedded calls, and pure property reads.

Observed counts matched the expected syntax exactly:

```text
js:  calls=3 new=1
jsx: calls=1 new=0
mjs: calls=3 new=1
cjs: calls=3 new=1
ts:  calls=3 new=1
tsx: calls=1 new=0
```

The property-only reference was not counted as a call. This clears the parser-feasibility assumption and gives S3 a concrete dependency candidate; the implementation still needs real-corpus reconciliation and performance measurement before acceptance.

## P7 — TypeScript LSP compatibility with current bridge semantics

A raw JSON-RPC POC spawned `typescript-language-server 6.0.0 --stdio` with TypeScript `5.9.2` and deliberately mirrored the current bridge's protocol behavior:

- server-to-client requests received `result: []`;
- `didOpen` used language-specific IDs;
- edits used the bridge's range-form full-document replacement;
- hover retried until non-null;
- diagnostics were observed through `publishDiagnostics`.

Hover returned non-null results for all six extensions. A `.ts` edit that changed a declared `string` return to a number produced two diagnostics, and restoring the original content converged back to zero diagnostics.

One additional implementation constraint was confirmed from source: `LspSession` currently executes `Command::new(backend_cmd)` with no argument vector, while `typescript-language-server` requires `--stdio`. S5 must therefore add backend arguments (or an equivalent typed command spec); a plain third command string is insufficient.

## P8 — N-way SCIP concatenation feasibility

Raw concatenation of three valid SCIP protobuf messages (28-document JS index + 6-document mixed JS/TS index + 28-document JS index) parsed successfully as one `Index` with 62 documents and 394 function definitions. All 394 function definitions retained `enclosing_range`.

This clears the N-way protobuf feasibility assumption. S1 still must stage every producer leg and publish the live slot only after all requested legs validate; current `python_leg` writes the live slot directly, so existing two-leg behavior is not the required atomicity proof.

## Planning consequences frozen for child EPs

1. JS/TS is one producer family but two graph-language labels.
2. Fallback config uses a CR-owned sidecar config with an explicit governed `files` list; no target `tsconfig.json` mutation.
3. Existing project configs are accepted only when their producer result is usable; a zero-file result can fall back to the governed derived config, while unrelated producer/config errors remain loud failures.
4. Profile exclusions and freshness must consume the same JS/TS source-set function.
5. Tree-sitter is the concrete CALLS-classifier candidate; regex is not a fallback.
6. TypeScript LSP support requires a third independent session, per-extension language IDs, and an executable-plus-args backend spec so `--stdio` is represented explicitly.
7. M1 cannot be advertised before S6 guards language-specific tools against false-clean output.
