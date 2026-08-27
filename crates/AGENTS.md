# crates/ — Rust carrier

Rust workspace member(s) of the code-reality toolchain. The frozen-Python
parity oracle and harness retired at R7 (2026-08-26); cargo synthetic-repo
tests are the sole gate face.

## pyrefly-producer (bin crate)

- **role**: Rust-native Python occurrence producer (ep-pyrefly-native-
  producer) — links the Pyrefly engine as a git-dep (pinned rev
  `1d64c4b…`; crates.io carries only a placeholder) and emits a SCIP
  protobuf index into the repo-keyed sidecar slot. SCIP face: the
  existing `--stamp-meta` → `--build-cache` → `graph_db build` pipeline
  consumes it unchanged — no cache-schema writes.
- **layering**: `api.rs` is the single-point isolation of every Pyrefly
  import (rev upgrades touch only this file); `walk.rs` pure ruff-AST
  collectors (callee-name positions, not receiver starts); `symbol.rs`
  scip-python-mirroring symbol forms (`pyrefly python <proj> <ver>`
  discriminator — infer_language's third Python prefix) + dunder-pair
  collapse; `emit.rs` protobuf assembly (DEF occurrences carry the full
  node range in `enclosing_range` — engine::fn_spans builds caller
  attribution from it); `lib.rs` orchestration (module identity derived
  from rel paths on both defs and targets — pyrefly handle naming is
  fallback-shaped and would split symbol identity; local-binding guard
  drops refs whose display name ≠ the innermost def).
- depends on the code-reality lib only for `engine::default_index_path`
  (slot resolution); the main binary stays free of the pyrefly dep tree.

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
  mirrors cache) / `snapshot` / `transition` / `graph_audit` (graph
  family — one module per frozen Python tool; cargo-test gated) /
  `scip_edges` (v1+ derivation lib — SCIP reference edges, spans-based
  attribution; CLI/inject/sidecar faces retired at v1+ S4, the module
  stays as the derivation oracle `graph_db` tests reconcile against) /
  `graph_db` (v1+ S4 — THE self-owned db face: `build` from any producer
  cache [SCIP or LSP harvest; producer-conditional attribution — spans
  where available, nearest-preceding on the span-less LSP face] into
  `<repo>/.code-reality/graph.db` [symbol-keyed nodes, one-row-per-site
  edges, derived flows/communities materialized, FTS5, temp+rename
  atomicity], `import_legacy` [merge onto producer symbols where
  (file, name) resolves uniquely, qname-minted symbols otherwise,
  dangling endpoints passthrough — legacy db read-only], and
  `ensure_indexes` [idempotent IF NOT EXISTS: engine read-chain indexes
  (edges caller/callee+kind, flow_memberships node_id, nodes anchor
  name/file/line) for dbs built before that DDL revision]) /
  `graph_engine` (v1+ engine parity — the ten live ops read-only over
  the self-owned db: symbol-keyed loaders with rowid ordering parity,
  hub/bridge (sampled Brandes, own LCG — statistical parity above 5k
  nodes), flows family, impact relaxation, communities Tier 0
  (directory grouping) + Leiden tier, risk six-factor, FTS5→LIKE
  search, compositions; `py_round` for Python `round()` decimal parity;
  CLI umbrella `graph_query <op>` (`--union` retired — build-time
  materialization) and the 12 MCP tools share the argv path) /
  `mcp_server` (frontend adapter — rmcp streamable-http on 8200, tools
  thin-wrap `cli::run` in-process with spawn_blocking + catch_unwind
  per-request isolation [SM-14]; bin `code-reality-mcp`) / `cli` (assembly —
  argv surface, mode routing incl. `--callers`/`--closure`/`--depth`
  1-10000, and the in-process `--audit` two-pass).
- **lib API contract**: functions return `ToolOutput {stdout, stderr,
  exit_code}` data — the lib never prints and never exits; the bin owns
  printing/exiting (compile-time premise of CLI/MCP single-backend
  drift-freedom).
- **Exit semantics (D3, per-tool — not uniform)**: snapshot/transition
  crashes = exit 1 + empty stdout (uncaught-Python alignment);
  graph_audit env errors and argparse usage errors = exit 2; `--json`
  faces print with a trailing newline (Python `print`).
- **Subcommand names mirror Python module names verbatim** (`scip_refs`,
  `snapshot`, `transition`, `graph_audit`; not kebab-case) —
  relay minimal-diff contract.
- **Schema interop**: the derived db keeps the frozen three-table DDL +
  `SCHEMA_VERSION`; extensions (fn_defs, R3) live in separate sidecars —
  never in the shared db (guard would ping-pong rebuilds). The legacy
  `.code-review-graph/` db is read-only everywhere (`connect_ro`) — it
  is only ever an `import_legacy` source. All consumer modules
  (audit/chain_tour/hub_refs/hazard/snapshot) read the self-owned
  `.code-reality/graph.db` via `graph_db::consumer_db` (missing-db and
  un-imported-legacy WARN guidance live there). Tests use
  `tests/graph_db_fixture.rs` (self-owned schema, symbol==qname
  universe); `tests/crg_fixture.rs` (production-shape CRG DDL) feeds
  the import_legacy/ensure test universe only.
- **Parity harness**: `tests/parity/` (pytest, `parity` marker) drives
  both implementations on identical inputs and `cmp`s stdout + exit
  codes; mutating drills hit fixture copies only. Environment-absent
  cases are valid equivalence (both sides fail loud with the same exit).
  `-h` faces compare with the prog prefix normalized (wrap position is
  prog-length relative; the Rust text is byte-pinned in cargo tests).
- Authored content is English; Chinese OUTPUT strings are the byte-parity
  face and exempt.
