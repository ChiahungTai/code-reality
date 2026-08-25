# crates/ — Rust carrier (coexistence)

Rust workspace member(s) of the code-reality toolchain. The frozen Python
package (`code_reality/`) stays the parity oracle until the R7 relay deletes
it; every Rust tool must reproduce Python stdout bytes + exit codes exactly
(gated by `tests/parity/`).

## code-reality (lib + umbrella bin)

- **lib layering**: `engine` (domain/use case — SCIP parsing via the `scip`
  crate [rust-protobuf form, types at `scip::types::*`], symbol predicates
  [hand-rolled — the `regex` crate has no look-around], scan, `[SRC]`
  assembly, structured caller-edge accessors [`FnSpan`/`fn_spans` from DEF
  `enclosing_range`, flat `refs_rows`]) / `callers` (domain — DEF-enc
  containment attribution with the `(width, seq)` innermost tie, closure
  BFS, mode output assembly; imports neither cli/cache/fndefs) / `cache`
  (adapter — derived sqlite three-table cache with cross-language schema
  interop, stale guards, face selection with protobuf fallback) / `fndefs`
  (adapter — fn-span sidecar `*.fndefs.db`, the sqlite carrier for
  callers/closure spans; ladder mirrors cache) / `cli` (assembly — argv
  surface, mode routing incl. `--callers`/`--closure`/`--depth` 1-10000).
- **lib API contract**: functions return `ToolOutput {stdout, stderr,
  exit_code}` data — the lib never prints and never exits; the bin owns
  printing/exiting (compile-time premise of CLI/MCP single-backend
  drift-freedom).
- **Subcommand names mirror Python module names verbatim** (`scip_refs`,
  not kebab-case) — relay minimal-diff contract.
- **Schema interop**: the derived db keeps the frozen three-table DDL +
  `SCHEMA_VERSION`; extensions (fn_defs, R3) live in separate sidecars —
  never in the shared db (guard would ping-pong rebuilds).
- **Parity harness**: `tests/parity/` (pytest, `parity` marker) drives both
  implementations on identical inputs and `cmp`s stdout + exit codes;
  mutating drills hit fixture copies only, NT real-index cases are
  read-only (skip-on-stale guard).
- Authored content is English; Chinese OUTPUT strings are the byte-parity
  face and exempt.
