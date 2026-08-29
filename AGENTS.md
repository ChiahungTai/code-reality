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
repo-specific special cases. Tool facts & pitfalls:
`plugin/skills/code-reality/SKILL.md` (versioned with the plugin,
standalone-consumer face); wiring / when-to-run: ai-rules
`skills/code-reality/SKILL.md` (deployed via symlink to four
harnesses).

This repo is public-facing (open source, remote GitHub) — **all authored
content is English**: code comments, docstrings, README, AGENTS.md, commit
messages. (Chinese OUTPUT strings are the frozen CLI byte-parity face,
preserved verbatim. Exempt per user adjudication 2026-08-29: `ai-analysis/`
— EPs, reports — and `.kanban/` are internal non-published working docs;
Chinese body is the convention there.)

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

Installed from PyPI wheels (consumer face, no Rust toolchain; macOS
arm64): `uv tool install code-reality` (→ `code-reality` +
`code-reality-mcp`), `uv tool install pyrefly-producer` (→
`pyrefly-index` + `pyrefly-lsp`), `uv tool install
code-reality-lsp-bridge` — or from a checkout via `cargo install
--path ~/Github/code-reality/crates/<crate>` (developer face, →
`~/.cargo/bin`). Sidecar home:
`<repo>/.code-reality/` — SCIP index slot under `scip/` with the
self-contained single-`*` `.gitignore` (generate → `--stamp-meta` →
`--build-cache` ordering); legacy `~/.mosaic/code-reality/` slots migrate
via `code-reality sidecar_migrate --repo <repo>`.

## Module guide

- [crates/AGENTS.md](crates/AGENTS.md) — the Rust carrier: lib layering
  (engine/callers/cache/fndefs/common/profile/argparse + graph/tour/boundary/
  hazard families + sidecar_migrate + mcp_server), exit-semantics table,
  parity history
- Tool semantics split: standalone tool facts & pitfalls live in
  `plugin/skills/code-reality/SKILL.md` (versioned with the plugin —
  carries the drift-discipline header); consumer-ecosystem wiring
  ("when to run") stays in ai-rules `skills/code-reality/SKILL.md`

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `code-reality scip_refs <symbol> --repo <repo>` (slot resolves in-repo since the data-plane unification) | ✅ |
| Caller-edge query (callers/closure) | `code-reality scip_refs <symbol> --callers/--closure [--depth N] --repo <repo>` | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `code-reality scip_refs --audit --repo` + `code-reality graph_audit --json` | ✅ |
| Deletability safety net (hub_refs/hazard) | `code-reality hub_refs <symbol> --repo <repo> --hazard` | ✅ |
| Boundary / export / narrative tool family | `code-reality <snapshot\|boundary\|boundary_build\|chain_tour\|delta_tour\|tour_manifest\|tour_validate\|tour_upgrade\|runtime_edges> ...` — snapshot's files face is all-kind (`_meta.files_face` marker; module_edges stay structural-kind, the empty-set WARN attributes kind-distribution vs root vs empty-db); transition is the snapshot-diff DOMAIN (load/summarize/claims/json render — the CLI/report face retired 2026-08-29 S4, delta_tour is the sole diff interface and carries the degenerate/cross-face guards via summarize + tour description) | ✅ |
| Unified MCP interface | stdio `code-reality-mcp --stdio` (default face: ZCode/Claude Code plugin in `plugin/` — CC-compatible manifest `plugin/.claude-plugin/plugin.json` single-sources both harnesses; root `marketplace.json` ZCode market + `.claude-plugin/marketplace.json` CC market) + streamable-http `127.0.0.1:8200/mcp` (launchd plist in `launchd/`, multi-harness sharing) + stdio `code-reality-lsp-bridge --stdio` (type face; separate process — resident LSP state stays out of the stateless main server); plugin spawn wrapper (plugin `0.3.0`+) resolves via PATH first (PATH present ⇒ PATH order wins), then the plugin's `node_modules/.bin` (CC embedded face — prepended to the child PATH, so the lsp-bridge backend resolves the embedded `pyrefly-lsp`; unset `${CLAUDE_PLUGIN_ROOT}` degrades gracefully for ZCode), then falls back to `~/.local/bin` and `~/.cargo/bin` with the fallback dir prepended to the child PATH (GUI-no-PATH safe for servers *and* backends); wheels face beats the dev checkout only in the fallback branch | ✅ |
| Self-owned graph db build (producer-keyed schema) | `code-reality graph_db build --repo <repo> [--json]` — any producer cache (rust-analyzer SCIP or LSP harvest, read from the in-repo slot) → `.code-reality/graph.db`: symbol-keyed nodes, single edge ontology (one row per call site), derived flows/communities materialized | ✅ |
| Read-chain index maintenance (idempotent) | `code-reality graph_db ensure_indexes --repo <repo> [--json]` — engine indexes (edges endpoints+kind, flow node, nodes anchor); index-only, no row data touched | ✅ |
| Graph-engine family (10 ops + document_symbols, read-only `.code-reality/graph.db`) | `code-reality graph_query <impact_radius\|detect_changes\|hub\|bridge\|communities\|arch_overview\|flows\|affected_flows\|review_context\|minimal_context\|search\|symbols> --repo <repo> [--leiden] [--seed N]` + 12 MCP tools; queries are always full-graph (union materialized at build; `--union` retired) | ✅ (embeddings face deferred by S3 adjudication) |
| Leiden communities tier (seeded deterministic) | `graph_query communities --leiden [--seed N]` — single-clustering 0.7; v1+ S4 new baseline (full-graph edges): NT 1,151 communities, largest 35.0% | ✅ |
| CRG retirement readiness | engine layer READY — `ai-analysis/reports/s4-crg-retirement-readiness.md`; consumer cutover DONE 2026-08-26; format ownership flip DONE 2026-08-27 (`ep-v1plus-own-graph-db.md`); legacy read-path retirement DONE 2026-08-27 (`ep-legacy-db-consumer-cutover.md`: audit/chain_tour/hub_refs+hazard/snapshot all read `.code-reality/graph.db`; graph_csv retired zero-consumer); W3: the legacy importer retired from the refresh chain (2026-08-28, gate 95.42% full-attribution accepted); **W5: the importer fully removed + every consumer repo's `.code-review-graph/` db deleted (2026-08-28; the retired CRG museum repo keeps its own copy by user adjudication)** — pure-producer graph is the served face; data-plane self-management axis W1-W5 complete | ✅ |
| Python symbol truth via LSP harvest | `scripts/lsp_harvest.py` (pyright-langserver → cache three-table db; POC pass-bar 20/20 data-level exact vs LSP) — **golden-oracle generator only** since the pyrefly producer took the production face (row below) | 🟢 (superseded as production face) |
| Rust-native Python occurrence producer (Pyrefly link, SCIP face) | `cargo run --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo> [--out <index.scip>]` — linked Pyrefly engine (git-dep rev `1d64c4b`) emits a SCIP index into the in-repo slot (`<repo>/.code-reality/scip/`); dunder-pair collapse, rel-path module identity, byte-deterministic output; S2 dogfood: defs coverage 99.6% name-normalized vs lsp golden, mosaic full 73s. scip-python fork demoted to fallback (retained, not default). Same crate also ships `pyrefly-lsp` (thin stdio host calling upstream `LspArgs::run` — the type-face backend below; engine-version parity with the producer is a lockfile guarantee). On write the producer invalidates superseded sidecar artifacts beside the slot (stale cache db / stamped meta) — they would otherwise be silently trusted (silent bad-db relay 2026-08-28); `graph_db build`'s lsp fast-path fails loud on the same mtime contradiction as defense-in-depth. B7b (W2 EP): constructor calls resolving to a corpus class (dataclass / object-inherit) mint a pseudo-constructor `Cls().` call ref + one-shot DEF backfill (fn-shaped → passes the fn-tail gate, pairs with legacy class nodes); per-site B7a guard keeps corpus-`__init__` sites in method grain; alias Class-kind display mismatch exempted from the local-binding guard | ✅ |
| Type face via LSP bridge (hover / diagnostics / edit-recheck — Python .py via pyrefly, Rust .rs via rust-analyzer) | `code-reality-lsp-bridge --stdio [--lsp-command <py-cmd>] [--rust-backend <rs-cmd>]` — MCP server routing by file extension; tools `lsp_status` / `hover(file,line,character)` / `check_file(file)` / `edit_file(file,content)`; one lazy backend session per language (independent lifecycles; rust-analyzer spawns with no flags). Consumer scenarios: hover a type signature; check a file's type errors (out-of-band disk edits auto-synced); edit in-memory then recheck (un-persisted edits survive LRU eviction; Rust flycheck/cargo-check runs on disk content). Convergence gates harden since the poisoned-cache fix (2026-08-29): the freshness basis is the newer of this call's mutation and the overlay's `last_mutation` (stamped at every mutation origin), and the stall test is time-based — a poisoned eviction push (version+1, empty) can no longer pass as the converged answer. The crate has no language-specific dependencies (the P2 clause is fulfilled: the Rust face reuses the same crate). Equivalence batteries: `tests/equivalence_battery.rs` vs pyright baseline + `tests/ra_equivalence_battery.rs` vs frozen rust-analyzer baseline + `tests/lsp_status_availability.rs` (missing-backend availability pin). Plugin entry live since plugin `0.1.2` | ✅ |
| Binary freshness face | `--version` on any bin (`<pkg>+<git rev>`, embedded via per-crate build.rs) + one-per-process stderr WARN when the CR checkout's HEAD has moved past the embedded rev or carries uncommitted `crates/` edits (`CR_REPO` env or `~/Github/code-reality` fallback; silent on machines without a checkout) + `.githooks/post-commit` background reinstall of changed crates (maintainer layout, opt-in `core.hooksPath=.githooks`) | ✅ |
| PyPI platform-wheel distribution (cargo-free consumer install) | `uv tool install code-reality` / `uv tool install pyrefly-producer` / `uv tool install code-reality-lsp-bridge` — wheels on PyPI (macOS arm64; v0.2.0 first release 2026-08-28); release path: `.github/workflows/release-wheels.yml` on `v*` tags via trusted publishing (three per-dist GitHub environments). One-shot use: `uvx code-reality <tool> ...` where dist name = bin name; the producer dist needs `uvx --from pyrefly-producer <bin>`. rust-analyzer stays a system dependency (`rustup component add rust-analyzer`); `lsp_status` reports missing backends as `state=unavailable` with install guidance. **Main face** — the npm embedded face (row below) is additive and version-locked to the plugin axis | ✅ |
| CC one-click full stack (npm embedded face) | `claude plugin install code-reality@code-reality-market` — the plugin's `package.json` `optionalDependencies` pull npm `code-reality-darwin-arm64` (the five bins assembled verbatim from the same-tag PyPI wheels; os/cpu-gated); CC runs `npm ci` on install and the servers spawn from `node_modules/.bin` via the wrapper chain (row above). macOS arm64 with arm64 npm only — an x64 npm (Rosetta node) skips the package and the wrapper falls through to the uv face; ZCode has no npm mechanism (uv face, README-documented). Release: `release-wheels.yml` npm-pack/npm-publish jobs (version guard vs workspace; single `release-npm` environment, `NPM_TOKEN`) — 0.3.0 bootstrapped from the PyPI wheels (registry live); github-marketplace install verified end-to-end (npm ci in cache, root parity; both servers connected) | ✅ |
| Unified in-repo data plane (sidecar home retired) | default slots `<repo>/.code-reality/{scip,boundary,snapshots}/` resolved by `engine::default_index_path` / the `default_out_dir` family; the data dir self-writes a single-`*` `.gitignore` (zero consumer gitignore setup); legacy `~/.mosaic/code-reality/` slots migrate one-shot via `code-reality sidecar_migrate --repo <repo>` (retired 2026-08-29; five repos migrated byte-identical) | ✅ |

## Tests

`cargo test`（Rust suites are the sole test face post-R7 — the Python
parity harness retired with the oracle; history in the archived EPs）.
