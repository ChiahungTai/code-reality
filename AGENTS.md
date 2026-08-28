# code-reality

Meta-layer tooling living *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions. Rust carrier
end state (R7, 2026-08-26): the frozen-Python parity oracle and both Python
copies retired after byte-identical acceptance on real corpora (NT
query/graph_audit `--json`/`--audit`, mosaic `hub_refs --json` — gate
record in `ai-analysis/execution-plans/_done/`). Migration history:
`ai-analysis/execution-plans/ep-rust-migration.md` + per-segment child EPs
in `_done/`.

**Repo facts belong to each repo** — the scanned repo's `.code-reality.toml`
profile owns module/exclusion/registry knowledge; the tool layer embeds no
repo-specific special cases. Tool semantics / when-to-run truth source:
ai-rules `skills/code-reality/SKILL.md` (deployed via symlink to four
harnesses).

This repo is public-facing (open source, remote GitHub) — **all authored
content is English**: code comments, docstrings, README, AGENTS.md, commit
messages. (Chinese OUTPUT strings are the frozen CLI byte-parity face,
preserved verbatim.)

## Usage (from any repo cwd)

Freshness face: every bin embeds its build rev (`git describe --always
--dirty --exclude=*` via build.rs) — `--version` prints
`<pkg>+<rev>` (`pyrefly-lsp` keeps its own face with the engine rev;
it never warns — it is a spawned backend). The four WARN-wired bins
(`code-reality`, `code-reality-mcp`, `pyrefly-index`,
`code-reality-lsp-bridge`) each emit one stderr WARN when the CR
checkout's HEAD has moved past the embedded rev or carries uncommitted
`crates/` edits (silent install-lag trap, 2026-08-28). The CR repo's
`.githooks/post-commit` (maintainer layout, opt-in via
`core.hooksPath=.githooks`) background-reinstalls changed crates so the
installed face follows HEAD.

```
code-reality <tool> --repo <repo-root> [args]
```

Installed via `cargo install --path ~/Github/code-reality/crates/code-reality` (→ `~/.cargo/bin/code-reality` + `code-reality-mcp`), `cargo install --path ~/Github/code-reality/crates/pyrefly-producer` (→ `pyrefly-index` + `pyrefly-lsp`), and `cargo install --path ~/Github/code-reality/crates/code-reality-lsp-bridge` (→ `code-reality-lsp-bridge`). Sidecar home:
`~/.mosaic/code-reality/` — per-repo SCIP index slots under `scip/<repo-basename>/`
(generate → `--stamp-meta` → `--build-cache` ordering).

## Module guide

- [crates/AGENTS.md](crates/AGENTS.md) — the Rust carrier: lib layering
  (engine/callers/cache/fndefs/common/profile/argparse + graph/tour/boundary/
  hazard families + mcp_server), exit-semantics table, parity history
- Tool semantics: ai-rules `skills/code-reality/SKILL.md` (the cross-repo
  truth source — this repo no longer duplicates it)

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `code-reality scip_refs <symbol> --repo <repo>` | ✅ |
| Caller-edge query (callers/closure) | `code-reality scip_refs <symbol> --callers/--closure [--depth N] --repo <repo>` | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `code-reality scip_refs --audit --repo` + `code-reality graph_audit --json` | ✅ |
| Deletability safety net (hub_refs/hazard) | `code-reality hub_refs <symbol> --repo <repo> --hazard` | ✅ |
| Boundary / export / narrative tool family | `code-reality <snapshot\|transition\|boundary\|boundary_build\|chain_tour\|delta_tour\|tour_manifest\|tour_validate\|tour_upgrade\|runtime_edges> ...` | ✅ |
| Unified MCP interface | stdio `code-reality-mcp --stdio` (default face: ZCode/Claude plugin in `plugin/`; repo-root `marketplace.json` = installable market) + streamable-http `127.0.0.1:8200/mcp` (launchd plist in `launchd/`, multi-harness sharing) + stdio `code-reality-lsp-bridge --stdio` (type face; separate process — resident LSP state stays out of the stateless main server) | ✅ |
| Self-owned graph db build (producer-keyed schema) | `code-reality graph_db build --repo <repo> [--json]` — any producer cache (rust-analyzer SCIP or LSP harvest) → `.code-reality/graph.db`: symbol-keyed nodes, single edge ontology (one row per call site), derived flows/communities materialized | ✅ |
| Read-chain index maintenance (idempotent) | `code-reality graph_db ensure_indexes --repo <repo> [--json]` — engine indexes (edges endpoints+kind, flow node, nodes anchor); index-only, no row data touched | ✅ |
| Graph-engine family (10 ops + document_symbols, read-only `.code-reality/graph.db`) | `code-reality graph_query <impact_radius\|detect_changes\|hub\|bridge\|communities\|arch_overview\|flows\|affected_flows\|review_context\|minimal_context\|search\|symbols> --repo <repo> [--leiden] [--seed N]` + 12 MCP tools; queries are always full-graph (union materialized at build; `--union` retired) | ✅ (embeddings face deferred by S3 adjudication) |
| Leiden communities tier (seeded deterministic) | `graph_query communities --leiden [--seed N]` — single-clustering 0.7; v1+ S4 new baseline (full-graph edges): NT 1,151 communities, largest 35.0% | ✅ |
| CRG retirement readiness | engine layer READY — `ai-analysis/reports/s4-crg-retirement-readiness.md`; consumer cutover DONE 2026-08-26; format ownership flip DONE 2026-08-27 (`ep-v1plus-own-graph-db.md`); legacy read-path retirement DONE 2026-08-27 (`ep-legacy-db-consumer-cutover.md`: audit/chain_tour/hub_refs+hazard/snapshot all read `.code-reality/graph.db`; graph_csv retired zero-consumer); W3: the legacy importer retired from the refresh chain (2026-08-28, gate 95.42% full-attribution accepted); **W5: the importer fully removed + every consumer repo's `.code-review-graph/` db deleted (2026-08-28; the retired CRG museum repo keeps its own copy by user adjudication)** — pure-producer graph is the served face; data-plane self-management axis W1-W5 complete | ✅ |
| Python symbol truth via LSP harvest | `scripts/lsp_harvest.py` (pyright-langserver → cache three-table db; POC pass-bar 20/20 data-level exact vs LSP) — **golden-oracle generator only** since the pyrefly producer took the production face (row below) | 🟢 (superseded as production face) |
| Rust-native Python occurrence producer (Pyrefly link, SCIP face) | `cargo run --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo> [--out <index.scip>]` — linked Pyrefly engine (git-dep rev `1d64c4b`) emits a SCIP index into the repo-keyed slot; dunder-pair collapse, rel-path module identity, byte-deterministic output; S2 dogfood: defs coverage 99.6% name-normalized vs lsp golden, mosaic full 73s. scip-python fork demoted to fallback (retained, not default). Same crate also ships `pyrefly-lsp` (thin stdio host calling upstream `LspArgs::run` — the type-face backend below; engine-version parity with the producer is a lockfile guarantee). On write the producer invalidates superseded sidecar artifacts beside the slot (stale cache db / stamped meta) — they would otherwise be silently trusted (silent bad-db relay 2026-08-28); `graph_db build`'s lsp fast-path fails loud on the same mtime contradiction as defense-in-depth. B7b (W2 EP): constructor calls resolving to a corpus class (dataclass / object-inherit) mint a pseudo-constructor `Cls().` call ref + one-shot DEF backfill (fn-shaped → passes the fn-tail gate, pairs with legacy class nodes); per-site B7a guard keeps corpus-`__init__` sites in method grain; alias Class-kind display mismatch exempted from the local-binding guard | ✅ |
| Type face via LSP bridge (hover / diagnostics / edit-recheck — Python .py via pyrefly, Rust .rs via rust-analyzer) | `code-reality-lsp-bridge --stdio [--lsp-command <py-cmd>] [--rust-backend <rs-cmd>]` — MCP server routing by file extension; tools `lsp_status` / `hover(file,line,character)` / `check_file(file)` / `edit_file(file,content)`; one lazy backend session per language (independent lifecycles; rust-analyzer spawns with no flags). Consumer scenarios: hover a type signature; check a file's type errors (out-of-band disk edits auto-synced); edit in-memory then recheck (un-persisted edits survive LRU eviction; Rust flycheck/cargo-check runs on disk content). The crate has no language-specific dependencies (the P2 clause is fulfilled: the Rust face reuses the same crate). Equivalence batteries: `tests/equivalence_battery.rs` vs pyright baseline + `tests/ra_equivalence_battery.rs` vs frozen rust-analyzer baseline. Plugin entry live as of plugin `0.1.2` (2026-08-28 version bump — refresh the plugin to mount both servers; remove any stale manual user-level bridge entry after the refresh to avoid double-mounting) | ✅ |
| Binary freshness face | `--version` on any bin (`<pkg>+<git rev>`, embedded via per-crate build.rs) + one-per-process stderr WARN when the CR checkout's HEAD has moved past the embedded rev or carries uncommitted `crates/` edits (`CR_REPO` env or `~/Github/code-reality` fallback; silent on machines without a checkout) + `.githooks/post-commit` background reinstall of changed crates (maintainer layout, opt-in `core.hooksPath=.githooks`) | ✅ |

## Tests

`cargo test`（Rust suites are the sole test face post-R7 — the Python
parity harness retired with the oracle; history in the archived EPs）.
