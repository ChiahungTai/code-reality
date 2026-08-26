# crates/ — Rust carrier (coexistence)

Rust workspace member(s) of the code-reality toolchain. The frozen Python
package (`code_reality/`) stays the parity oracle until the R7 relay deletes
it; every Rust tool must reproduce Python stdout bytes + exit codes exactly
(gated by `tests/parity/`).

## code-reality (lib + umbrella bin)

- **lib layering**: `common` (foundation — EDGE_KINDS, anchor pattern,
  repo relativization, CRG `connect_ro` WAL semantics [immutable=1 /
  mode=ro], mtime tear guard, ordered `make_meta`, the D1 JSON serializer
  [`to_json_indent1` = `json.dumps(indent=1)` byte face], time foundation
  [libc `localtime_r` for the naive-`astimezone()` equivalence — D2 POC])
  / `profile` (foundation — `.code-reality.toml` crash-only loader incl.
  `hazard_registry` parsing [①-foundation, rules engine is R4b],
  `module_of` F6, claims regex with a never-match sentinel) / `argparse`
  (shared argparse mimic — abbreviations, negative-number positionals,
  `--` separator, `-h`; all graph-family CLIs grow from here) / `engine`
  (domain/use case — SCIP parsing via the `scip` crate [rust-protobuf
  form, types at `scip::types::*`], symbol predicates [hand-rolled — the
  `regex` crate has no look-around], scan, `[SRC]` assembly, structured
  caller-edge accessors [`FnSpan`/`fn_spans` from DEF
  `enclosing_range`, flat `refs_rows`]) / `callers` (domain — DEF-enc
  containment attribution with the `(width, seq)` innermost tie, closure
  BFS, mode output assembly; imports neither cli/cache/fndefs) / `cache`
  (adapter — derived sqlite three-table cache with cross-language schema
  interop, stale guards, face selection with protobuf fallback, audit
  target double-key attribution) / `fndefs` (adapter — fn-span sidecar
  `*.fndefs.db`, the sqlite carrier for callers/closure spans; ladder
  mirrors cache) / `snapshot` / `transition` / `graph_audit` / `graph_csv`
  (graph family — one module per frozen Python tool; byte-parity gated
  by `tests/parity/test_graph_family_parity.py`) / `scip_edges` (v1+ S1
  write face — SCIP reference edges into the index-sibling
  `<index-stem>.union.db` sidecar (default slot: `index.union.db`): own schema, PK (caller, callee),
  provenance='SCIP', idempotent upsert + `updated_at` sweep; CRG
  graph.db stays edge-write-free per the (A) adjudication) /
  `scip_nodes` (v1+ S2 write face — graph_audit missing → double-key
  reconciliation → graph.db nodes; THE only graph.db writer in the
  family: `extra {"tier":"SCIP"}` marker rollback, `VACUUM INTO`
  first-inject backup, UNIQUE-collision skip + structural-residual
  reporting) / `graph_engine` (v1+ engine parity — the ten live CRG
  MCP ops re-implemented read-only over graph.db: loaders with rowid
  ordering parity, hub/bridge (sampled Brandes, own LCG — statistical
  parity above 5k nodes), flows family, impact relaxation, communities
  Tier 0 (directory grouping — igraph never present in base CRG), risk
  six-factor, FTS5→LIKE search, compositions; `py_round` for Python
  `round()` decimal parity; CLI umbrella `graph_query <op>` (incl. `--union`
  sidecar join + `--leiden` tier + `symbols` outline) and the 12
  MCP tools share the argv path) / `mcp_server` (frontend adapter — rmcp streamable-http on 8200, tools
  thin-wrap `cli::run` in-process with spawn_blocking + catch_unwind
  per-request isolation [SM-14]; bin `code-reality-mcp`) / `cli` (assembly —
  argv surface, mode routing incl. `--callers`/`--closure`/`--depth`
  1-10000, and the in-process `--audit` two-pass).
- **lib API contract**: functions return `ToolOutput {stdout, stderr,
  exit_code}` data — the lib never prints and never exits; the bin owns
  printing/exiting (compile-time premise of CLI/MCP single-backend
  drift-freedom).
- **Exit semantics (D3, per-tool — not uniform)**: snapshot/transition/
  graph_csv crashes = exit 1 + empty stdout (uncaught-Python alignment);
  graph_audit env errors and argparse usage errors = exit 2; `--json`
  faces print with a trailing newline (Python `print`).
- **Subcommand names mirror Python module names verbatim** (`scip_refs`,
  `snapshot`, `transition`, `graph_audit`, `graph_csv`; not kebab-case) —
  relay minimal-diff contract.
- **Schema interop**: the derived db keeps the frozen three-table DDL +
  `SCHEMA_VERSION`; extensions (fn_defs, R3) live in separate sidecars —
  never in the shared db (guard would ping-pong rebuilds). CRG graph.db
  reads are read-only (`connect_ro`) for every module except the
  `scip_nodes` injector (sole write face, marker-tagged). Synthetic-db
  tests share `tests/crg_fixture.rs` (production-shape CRG DDL).
- **Parity harness**: `tests/parity/` (pytest, `parity` marker) drives
  both implementations on identical inputs and `cmp`s stdout + exit
  codes; mutating drills hit fixture copies only. Environment-absent
  cases are valid equivalence (both sides fail loud with the same exit).
  `-h` faces compare with the prog prefix normalized (wrap position is
  prog-length relative; the Rust text is byte-pinned in cargo tests).
- Authored content is English; Chinese OUTPUT strings are the byte-parity
  face and exempt.
