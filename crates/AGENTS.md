# crates/ — Rust carrier

Rust workspace member(s) of the code-reality toolchain. The frozen-Python
parity oracle and harness retired at R7 (2026-08-26); cargo synthetic-repo
tests are the sole gate face.

## code-reality-lsp-bridge (bin crate)

- **role**: LSP↔MCP bridge for the type face (hover / diagnostics /
  edit-recheck, EP ep-type-face-lsp-bridge) — one MCP server process
  (stdio, `--stdio` flag; bin `code-reality-lsp-bridge`), one lazily
  spawned language-server backend (default `pyrefly-lsp`; `--lsp-command`
  overrides — the bridge itself imports NO language-specific crate,
  which is the P2 clause: the Rust type face is this crate with
  rust-analyzer as the backend command).
- **layering**: `framing` (LSP base-protocol Content-Length framing over
  child stdio — headers must end `\r\n`, Content-Length only) /
  `session` (lifecycle + protocol client: `LangSpec` per-language
  profile [languageId, extension gate, hover-retry window, check
  deadline, install hint], lazy spawn + initialize → `initialized`
  handshake, all interactions serialized under one interaction lock
  per session [pyrefly's uris_pending_close assumes a single ordered
  writer], reader thread three-way split [responses → pending slot,
  publishDiagnostics → per-URI diag cache, server→client requests →
  ALWAYS an empty `[]` response — unanswered workspace/configuration
  freezes pyrefly's background indexing], content overlay
  [path → last-sent content+version; LRU eviction didCloses the server
  copy but keeps the overlay, so un-persisted edits survive re-open],
  `sync_open` [no-op when unchanged; didChange full-sync on out-of-band
  disk edits; re-open from overlay after eviction]; full-content
  didChange uses the RANGE form spanning the OLD content — the
  range-elided form is a spec obligation rust-analyzer does not honor,
  probe-verified 2026-08-28]) / `server` (rmcp ToolRouter face +
  `Bridge` extension routing [.py → pyrefly session, .rs →
  rust-analyzer session — two independent lazy backends, SM-7];
  tools `lsp_status` [per-backend lines], `hover` [bounded retry,
  per-backend window; rust-analyzer's transient -32801 "content
  modified" retried like a null hover], `check_file` [convergence =
  cached push carries the overlay's version or newer AND postdates any
  mutation this call AND is per-URI quiesced; push-only model — waiting
  without a mutation would be a guaranteed fake timeout], `edit_file`
  [range-form full-content didChange — see session]; all tools
  spawn_blocking, SM-14 pattern).
- **backend invariants**: rust-analyzer spawns with NO flags (default
  stdio is LSP; it rejects the `--stdio` flag); the initialize request
  advertises ONLY `textDocument.hover` + `publishDiagnostics` — never
  `workspace.configuration` or `didChangeWatchedFiles
  .dynamicRegistration` (advertising them makes pyrefly issue
  server→client requests whose answers gate background work).
- depends on no workspace crate; rmcp hoisted into workspace deps.
  The bin's `stale_binary_warn()` is a LOCAL COPY of
  `code-reality/src/freshness.rs` (the no-workspace-dep clause wins
  over sharing) — change both together.

## pyrefly-producer (bin crate)

- **role**: Rust-native Python occurrence producer (ep-pyrefly-native-
  producer) — links the Pyrefly engine as a git-dep (pinned rev
  `1d64c4b…`; crates.io carries only a placeholder) and emits a SCIP
  protobuf index into the in-repo slot (`<repo>/.code-reality/scip/`).
  SCIP face: the
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
- depends on the code-reality lib for `engine::default_index_path`
  (slot resolution) and `freshness::stale_binary_warn`; the main
  binary stays free of the pyrefly dep tree.
- **bins**: `pyrefly-index` (occurrence index) + `overlay-gen`
  (declarative projection-plan compiler — single-source symbol minting
  via `emit`/`symbol`, declared-edge consistency gate via `py_calls`;
  spawned by the umbrella `project`; never WARN-wired, pyrefly-lsp
  precedent) + `pyrefly-lsp` (thin
  stdio host calling upstream `LspArgs::run` from the same pinned-rev
  engine — the type-face backend the lsp-bridge spawns; engine-version
  parity between the two faces is a lockfile guarantee, not a config).

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
  mirrors cache) / `snapshot` / `transition` (domain module — the
  snapshot-diff domain: load/summarize/claims/json render; its CLI/report
  face retired 2026-08-29, S4 — delta_tour is the sole diff interface) /
  `graph_audit` (graph
  family — one module per frozen Python tool; cargo-test gated) /
  `scip_edges` (v1+ derivation lib — SCIP reference edges, spans-based
  attribution; CLI/inject/sidecar faces retired at v1+ S4, the module
  stays as the derivation oracle `graph_db` tests reconcile against) /
  `graph_db` (v1+ S4 — THE self-owned db face: `build` from any producer
  cache [SCIP or LSP harvest; producer-conditional attribution — spans
  where available, nearest-preceding on the span-less LSP face] into
  `<repo>/.code-reality/graph.db` [symbol-keyed nodes, one-row-per-site
  edges with the CALLS-vs-REFERENCES split derived build-side by
  `py_calls` (ruff parse of referenced files — SCIP carries no call
  role; dunder-collapsed constructor edges match via the symbol's own
  class segment), derived flows/communities materialized, FTS5,
  temp+rename atomicity], and
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
  thin-wrap `cli::run` / `graph_engine::run` / the data-plane module
  `run`s (build/snapshot/delta_tour/project — write side effects,
  ep-mcp-data-plane-tools) through one shared spawn_blocking +
  catch_unwind runner per-request isolation [SM-14]; bin
  `code-reality-mcp`) / `cli` (assembly —
  argv surface, mode routing incl. `--callers`/`--closure`/`--depth`
  1-10000, and the in-process `--audit` two-pass) / `sidecar_migrate`
  (one-shot migration of the retired home sidecar face — leaf on
  argparse/engine; boundary dbs and basename-collision slots are
  knowingly not auto-attributed, EP data-plane-unification) / `build`
  (orchestration leaf — one-shot data-plane bootstrap: detect language
  face → spawn pyrefly-index / `rust-analyzer scip` (sibling bins,
  separate dists) → in-process graph_db build + ensure_indexes; mixed
  repos cat-merge both SCIP indexes into one dual-language graph;
  `BuildError::{Env,Core}` maps fail(2)/crash(1)) / `project`
  (orchestration leaf — projected-graph overlay: spawn overlay-gen,
  cat-merge onto the real index into `.code-reality/projections/<stem>/`,
  protobuf-face graft/HOLE/MISSING report labeled `[projected]`
  (declaration-not-evidence); `ProjectError::{Env,Core}` mirrors the
  build exit semantics; the real slot stays byte-identical).
- **lib API contract**: functions return `ToolOutput {stdout, stderr,
  exit_code}` data — the lib never prints and never exits; the bin owns
  printing/exiting (compile-time premise of CLI/MCP single-backend
  drift-freedom).
- **Exit semantics (D3, per-tool — not uniform)**: snapshot crashes =
  exit 1 + empty stdout (uncaught-Python alignment);
  graph_audit env errors and argparse usage errors = exit 2; `--json`
  faces print with a trailing newline (Python `print`).
- **Subcommand names mirror Python module names verbatim** (`scip_refs`,
  `snapshot`, `graph_audit`; not kebab-case) —
  relay minimal-diff contract. (`transition` left the CLI surface at S4 —
  the module stays as the diff domain delta_tour consumes.)
- **Schema interop**: the derived db keeps the frozen three-table DDL +
  `SCHEMA_VERSION`; extensions (fn_defs, R3) live in separate sidecars —
  never in the shared db (guard would ping-pong rebuilds). All consumer
  modules (audit/chain_tour/hub_refs/hazard/snapshot) read the
  self-owned `.code-reality/graph.db` via `graph_db::consumer_db`
  (missing-db WARN guidance lives there). Tests use
  `tests/graph_db_fixture.rs` (self-owned schema, symbol==qname
  universe). The CRG-era `.code-review-graph/` format and its importer
  were fully removed at W5 (2026-08-28).
- **Parity harness (R7-retired history)**: `tests/parity/` (pytest,
  `parity` marker — deleted with the frozen-Python oracle) drove both
  implementations on identical inputs and `cmp`d stdout + exit codes;
  mutating drills hit fixture copies only. Environment-absent cases
  were valid equivalence (both sides fail loud with the same exit).
  `-h` faces compared with the prog prefix normalized (wrap position
  is prog-length relative; the Rust text is byte-pinned in cargo
  tests).
- Authored content is English; Chinese OUTPUT strings are the byte-parity
  face and exempt.
