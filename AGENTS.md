# code-reality

Meta-layer tooling living *above* repositories: structural facts, governance
audits, and narrative artifacts consumed by AI coding sessions. Rust carrier
end state (R7, 2026-08-26): the frozen-Python parity oracle and both Python
copies retired after byte-identical acceptance on real corpora (NT
query/graph_audit `--json`/`--audit`, mosaic `hub_refs --json` — gate
record in `ai-analysis/execution-plans/_done/`). Migration history:
`ai-analysis/execution-plans/_done/ep-rust-migration.md` + per-segment
child EPs in `_done/`.

**Repo facts belong to each repo** — the scanned repo's `.code-reality.toml`
profile owns module/exclusion/registry knowledge; the tool layer embeds no
repo-specific special cases. Tool facts & pitfalls:
`plugin/skills/code-reality-tools/SKILL.md` (versioned with the plugin,
standalone-consumer face; skill id `code-reality-tools` — deliberately
distinct from ai-guide's `code-reality` skill so both can coexist in one
session's skill registry without dedup shadowing); wiring / when-to-run: ai-guide
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
`code-reality-lsp-bridge`) call the zero-dep `cr-freshness` leaf
crate (single source since ep-cr-freshness-extraction): one stderr
WARN per process, **dev-face gated** — active only when the running
exe lives under `$CARGO_HOME/bin` (pin-driven `~/.local/bin`/uvx
installs are silent; the plugin pin is their version authority) — and
**crates-relevant** — a docs-only HEAD gap does not warn, a
git-unresolvable rev conservatively does, and uncommitted `crates/`
edits always warn on the dev face. The CR repo's
`.githooks/post-commit` (maintainer layout, opt-in via
`core.hooksPath=.githooks`) background-reinstalls changed crates so the
installed face follows HEAD, plus a hand-merged refresh leg (burst
debounce → release-face `refresh`, re-stamp on docs-only commits) —
deliberately unmanaged (no HOOK_MARKER) so `hook install` keeps
refusing rather than dropping the reinstall block; template changes
must be hand-ported (header comment in the hook).

```
code-reality <tool> --repo <repo-root> [args]
```

Installed from PyPI wheels (consumer face, no Rust toolchain; macOS
arm64): `uv tool install code-reality` (→ `code-reality` +
`code-reality-mcp`), `uv tool install pyrefly-producer` (→
`pyrefly-index` + `pyrefly-lsp` + `overlay-gen`), `uv tool install
code-reality-lsp-bridge` — or from a checkout via `cargo install
--path ~/Github/code-reality/crates/<crate>` (developer face, →
`~/.cargo/bin`). Sidecar home:
`<repo>/.code-reality/` — SCIP index slot under `scip/` with the
self-contained single-`*` `.gitignore` (generate → `--stamp-meta` →
`--build-cache` ordering); legacy `~/.mosaic/code-reality/` slots migrate
via `code-reality sidecar_migrate --repo <repo>`. scip_refs-family
queries self-heal a stale slot before answering (single-flight;
`CODE_REALITY_AUTOHEAL=off` reverts to warn-only); `code-reality refresh
--repo <repo>` is the post-commit background face and `code-reality hook
install|remove --repo <repo>` the opt-in `.githooks/post-commit` wiring
(loud refusals over unmanaged hooks / foreign `core.hooksPath` / active
`.git/hooks/*`).

## Module guide

- [crates/AGENTS.md](crates/AGENTS.md) — the Rust carrier: lib layering
  (engine/callers/cache/fndefs/common/profile/argparse + graph/tour/boundary/
  hazard families + sidecar_migrate + build + mcp_server), exit-semantics table,
  parity history
- Tool semantics split: standalone tool facts & pitfalls live in
  `plugin/skills/code-reality-tools/SKILL.md` (versioned with the plugin —
  carries the drift-discipline header); consumer-ecosystem wiring
  ("when to run") stays in ai-guide `skills/code-reality/SKILL.md`

## Capabilities

| Capability | Entry | Status |
|---|---|---|
| Symbol truth query (refs/defs, trait disambiguation) | `code-reality scip_refs <symbol> --repo <repo>` (slot resolves in-repo since the data-plane unification) | ✅ |
| Caller-edge query (callers/closure) | `code-reality scip_refs <symbol> --callers/--closure [--depth N] --repo <repo>` | ✅ |
| Completeness governance (audit + `[SRC]` provenance) | `code-reality scip_refs --audit --repo` + `code-reality graph_audit --json` | ✅ |
| Deletability safety net (hub_refs/hazard) | `code-reality hub_refs <symbol> --repo <repo> --hazard` | ✅ |
| Boundary / export / narrative tool family | `code-reality <snapshot\|boundary\|boundary_build\|chain_tour\|delta_tour\|tour\|tour_manifest\|tour_validate\|tour_upgrade\|runtime_edges> ...` — snapshot's files face is all-kind (`_meta.files_face` marker; module_edges stay structural-kind, the empty-set WARN attributes kind-distribution vs root vs empty-db); transition is the snapshot-diff DOMAIN (load/summarize/claims/json render — the CLI/report face retired 2026-08-29 S4, delta_tour is the sole diff interface and carries the degenerate/cross-face/cross-generation guards via summarize/render + tour description); `tour register\|materialize` is the intent-level two-phase materialization face (AIR-80 — manifest `[[delta_arc]]` provenance rows keyed on arcId: pending row on register, commit-ish rev-parse + snapshot-pair + stale gate + EP claims gate on materialize, row upserts `tourPath`/quality) | ✅ |
| Unified MCP interface | stdio `code-reality-mcp --stdio` (default face: ZCode/Claude Code plugin in `plugin/` — CC-compatible manifest `plugin/.claude-plugin/plugin.json` single-sources both harnesses; root `marketplace.json` ZCode market + `.claude-plugin/marketplace.json` CC market) + streamable-http `127.0.0.1:8200/mcp` (launchd plist in `launchd/`, multi-harness sharing) + stdio `code-reality-lsp-bridge --stdio` (type face; separate process — resident LSP state stays out of the stateless main server) + the data-plane four (`build`/`snapshot`/`delta_tour`/`project`, since 0.6.0 — write side effects re-adjudicated 2026-08-29: descriptions carry write targets, build's minutes-level no-progress note, delta_tour's in-repo out_dir default); plugin spawn wrapper resolves the canonical uv executable directory with `uv tool dir --bin`, version-checks the pinned `code-reality-mcp` / `code-reality-lsp-bridge` / `pyrefly-index` faces, bootstraps all three distributions via `uv tool install --force` when any face is missing/stale, and rechecks the postcondition before exec; `CODE_REALITY_BOOTSTRAP=off` escapes dev cargo-HEAD setups; the lsp-bridge wrapper resolves the pinned bridge only from that same uv directory with a bounded first-session wait for the twin bootstrap | ✅ |
| Self-owned graph db build (producer-keyed schema) | `code-reality graph_db build --repo <repo> [--json]` — any producer cache (rust-analyzer SCIP or LSP harvest, read from the in-repo slot) → `.code-reality/graph.db`: symbol-keyed nodes, single edge ontology (one row per call site), derived flows/communities materialized | ✅ |
| Read-chain index maintenance (idempotent) | `code-reality graph_db ensure_indexes --repo <repo> [--json]` — engine indexes (edges endpoints+kind, flow node, nodes anchor); index-only, no row data touched | ✅ |
| Graph-engine family (10 ops + document_symbols, read-only `.code-reality/graph.db`) | `code-reality graph_query <impact_radius\|detect_changes\|hub\|bridge\|communities\|arch_overview\|flows\|affected_flows\|review_context\|minimal_context\|search\|symbols> --repo <repo> [--leiden] [--seed N]` + 12 MCP tools; queries are always full-graph (union materialized at build; `--union` retired) | ✅ (embeddings face deferred by S3 adjudication) |
| Leiden communities tier (seeded deterministic) | `graph_query communities --leiden [--seed N]` — single-clustering 0.7; v1+ S4 new baseline (full-graph edges): NT 1,151 communities, largest 35.0% | ✅ |
| CRG retirement readiness | engine layer READY — `ai-analysis/reports/s4-crg-retirement-readiness.md`; consumer cutover DONE 2026-08-26; format ownership flip DONE 2026-08-27 (`ep-v1plus-own-graph-db.md`); legacy read-path retirement DONE 2026-08-27 (`ep-legacy-db-consumer-cutover.md`: audit/chain_tour/hub_refs+hazard/snapshot all read `.code-reality/graph.db`; graph_csv retired zero-consumer); W3: the legacy importer retired from the refresh chain (2026-08-28, gate 95.42% full-attribution accepted); **W5: the importer fully removed + every consumer repo's `.code-review-graph/` db deleted (2026-08-28; the retired CRG museum repo keeps its own copy by user adjudication)** — pure-producer graph is the served face; data-plane self-management axis W1-W5 complete | ✅ |
| Python symbol truth via LSP harvest | `scripts/lsp_harvest.py` (pyright-langserver → cache three-table db; POC pass-bar 20/20 data-level exact vs LSP) — **golden-oracle generator only** since the pyrefly producer took the production face (row below) | 🟢 (superseded as production face) |
| Rust-native Python occurrence producer (Pyrefly link, SCIP face) | `cargo run --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo> [--out <index.scip>]` — linked Pyrefly engine (git-dep rev `1d64c4b`) emits a SCIP index into the in-repo slot (`<repo>/.code-reality/scip/`); dunder-pair collapse, rel-path module identity, byte-deterministic output; S2 dogfood: defs coverage 99.6% name-normalized vs lsp golden, mosaic full 73s. scip-python fork demoted to fallback (retained, not default). Same crate also ships `pyrefly-lsp` (thin stdio host calling upstream `LspArgs::run` — the type-face backend below; engine-version parity with the producer is a lockfile guarantee). On write the producer invalidates superseded sidecar artifacts beside the slot (stale cache db / stamped meta) — they would otherwise be silently trusted (silent bad-db relay 2026-08-28); `graph_db build`'s lsp fast-path fails loud on the same mtime contradiction as defense-in-depth. B7b (W2 EP): constructor calls resolving to a corpus class (dataclass / object-inherit) mint a pseudo-constructor `Cls().` call ref + one-shot DEF backfill (fn-shaped → passes the fn-tail gate, pairs with legacy class nodes); per-site B7a guard keeps corpus-`__init__` sites in method grain; alias Class-kind display mismatch exempted from the local-binding guard | ✅ |
| Type face via LSP bridge (hover / diagnostics / edit-recheck — Python .py via pyrefly, Rust .rs via rust-analyzer, JS/TS via typescript-language-server) | `code-reality-lsp-bridge --stdio [--lsp-command <py-cmd>] [--rust-backend <rs-cmd>] [--typescript-backend <ts-cmd>]` — MCP server routing by file extension; tools `lsp_status` / `hover(file,line,character)` / `check_file(file)` / `edit_file(file,content)`; one lazy backend session per family (independent lifecycles; rust-analyzer spawns with no flags; the JS/TS family spawns `typescript-language-server --stdio` via the typed `BackendCommand` — argv, never a shell string; the override flag changes only the program). The six JS/TS extensions route with per-extension languageIds (js/mjs/cjs→javascript, jsx→javascriptreact, ts→typescript, tsx→typescriptreact); backend resolution is override → repo-local `node_modules/.bin` → `CODE_REALITY_NODE_BIN_DIR` → PATH (one resolver for status and spawn; npm tools are external prerequisites like rust-analyzer — no wheel shipping, no npx/network at runtime). Diagnostic convergence is policy-per-family (`LangSpec.diag_versions`, measured): pyrefly/ra pushes carry `version` (version gate + time basis + quiesce); typescript-language-server 6.0.0 never stamps `version` and ignores the LSP `versionSupport` capability, so that family converges on time basis + quiesce. Consumer scenarios: hover a type signature; check a file's type errors (out-of-band disk edits auto-synced); edit in-memory then recheck (un-persisted edits survive LRU eviction; Rust flycheck/cargo-check runs on disk content). Convergence gates harden since the poisoned-cache fix (2026-08-29): the freshness basis is the newer of this call's mutation and the overlay's `last_mutation` (stamped at every mutation origin), and the stall test is time-based — a poisoned eviction push (version+1, empty) can no longer pass as the converged answer. The crate has no language-specific dependencies (the P2 clause is fulfilled: the Rust face reuses the same crate). Equivalence batteries: `tests/equivalence_battery.rs` vs pyright baseline + `tests/ra_equivalence_battery.rs` vs frozen rust-analyzer baseline + `tests/lsp_status_availability.rs` (missing-backend availability pin) + `tests/ts_backend.rs` (hermetic fake-TLS family: routing/languageIds, argv isolation, diagnostics, death isolation both directions). `tests/rust_backend.rs` runs all four tests always-on; the three real-ra tests serialize on a test-file mutex (one workspace cold load at a time — concurrent cold loads flaked the 30s convergence deadline). Plugin entry live since plugin `0.1.2` | ✅ |
| Binary freshness face | `--version` on any bin (`<pkg>+<git rev>`, embedded via per-crate build.rs) + one-per-process stderr WARN via the zero-dep `cr-freshness` leaf crate (single source for all four WARN-wired bins, dev-face gated): active only when the running exe is under `$CARGO_HOME/bin` (pin installs stay silent — the plugin pin is their authority) and crates-relevant — warns when the checkout's HEAD carries `crates/` changes past the embedded rev (docs-only gaps silent; git-unresolvable revs warn conservatively) or uncommitted `crates/` edits (`CR_REPO` env or `~/Github/code-reality` fallback; silent on machines without a checkout) + `.githooks/post-commit` background reinstall of changed crates (maintainer layout, opt-in `core.hooksPath=.githooks`) | ✅ |
| PyPI platform-wheel distribution (cargo-free consumer install) | `uv tool install code-reality` / `uv tool install pyrefly-producer` / `uv tool install code-reality-lsp-bridge` — wheels on PyPI (macOS arm64; v0.2.0 first release 2026-08-28); release path: `.github/workflows/release-wheels.yml` on `v*` tags via trusted publishing (three per-dist GitHub environments). One-shot use: `uvx code-reality <tool> ...` where dist name = bin name; the producer dist needs `uvx --from pyrefly-producer <bin>`. rust-analyzer stays a system dependency (`rustup component add rust-analyzer`); `lsp_status` reports missing backends as `state=unavailable` with install guidance. **Single binary layer** — the npm platform-package face is retired (2026-08-29; registry entry frozen at 0.3.1, deprecation pending) and the plugin wrapper bootstraps the exact plugin-pinned version from PyPI (row below); the wrapper test enforces both wrapper pins == plugin == workspace == both marketplace listings | ✅ |
| First-session uv bootstrap (both harnesses; npm embedded face retired) | `plugin/.mcp.json` spawn wrapper: `uv tool dir --bin` defines the consumer executable face; `code-reality-mcp`, `code-reality-lsp-bridge`, and `pyrefly-index` must all prefix-match the plugin pin before the MCP server starts. A missing/stale face triggers exact-pin `uv tool install --force` for all three distributions, followed by the same direct-path verification; an exit-0 install that leaves any face stale fails loud. `CODE_REALITY_BOOTSTRAP=off` restores developer PATH/cargo resolution; no uv → loud 127 + guidance, with the retired embedded `node_modules` as a deprecation-grace rescue; the lsp-bridge wrapper waits for the pinned bridge in the same uv bin dir. Supersedes the npm face & the a1bce6b wait-for-parity stance (2026-08-29: npm face was CC-only + darwin-arm64-only + a second-registry lockstep burden, agent-verified; npm package frozen at 0.3.1, `npm deprecate` pending) | ✅ |
| Unified in-repo data plane (sidecar home retired) | default slots `<repo>/.code-reality/{scip,boundary,snapshots}/` resolved by `engine::default_index_path` / the `default_out_dir` family; the data dir self-writes a single-`*` `.gitignore` (zero consumer gitignore setup); legacy `~/.mosaic/code-reality/` slots migrate one-shot via `code-reality sidecar_migrate --repo <repo>` (retired 2026-08-29; five repos migrated byte-identical) | ✅ |
| 新 repo 數據面一鍵準備（build 傘形；language-set 編排＋JS/TS 第一類面） | `code-reality build --repo <repo> [--producer rust\|python\|typescript] [--json]`（MCP face：`build` tool）— 偵測語言面集（`LanguageFace` 副檔名單一源：.py/.rs/六 JS-TS 副檔名）→ 有序 producer 腿（python→rust→typescript；`pyrefly-index --out`／`rust-analyzer scip <repo-dir>`＋`current_dir` toolchain pin／`scip-typescript` 外部 npm 前置——repo-local `node_modules/.bin` → `CODE_REALITY_NODE_BIN_DIR` → PATH 解析，無 npx/網路）→ **每腿 staged、全部驗證後一次原子發佈**（後腿失敗＝pre-build slot byte-identical；attempt 唯一名，同 process 併發 build 不互踩）→ N-way protobuf 串接 → in-process `graph_db build`＋`ensure_indexes`；**JS/TS 語料**：existing tsconfig/jsconfig 僅在完整覆蓋 governed corpus 時採用，否則 CR-owned 暫態 sidecar config（顯式絕對 `files`、絕不改 target repo 設定）且 derived 模式強制精確收斂；governed corpus＝repo profile 排除後的六副檔名集（`js_ts_corpus` 單一源，producer 與 freshness 同政策）；全語料被 profile 排除＝合法空終態（index/graph 移除、收斂 Fresh）；env 類錯 `fail(2)`／graph 核心 `crash(1)`；index provenance in-process stamp（[SRC] 全行）；L4 dogfood：delegate-bridge 拋棄式副本 36 governed docs 精確、derived config 路徑（repo tsconfig 不存在）、256 nodes 全 JavaScript、coverage 100%、CALLS 340 真語法標記、mutation matrix（edit/add/delete/rename/excluded/profile 變更）全收斂；NT 混合 66,820 nodes 雙語言合一（歷史 L4）；**計數語義（L4 實證）**：edge 物化自然鍵去重（kind＋caller＋callee＋file＋line）——任何 producer 側重複均無害 | ✅ |
| JS/TS 圖語義（第一類結構查詢） | `scip_refs`／`--callers`／`--closure`／`graph_query` 家族對 `.js/.jsx/.mjs/.cjs/.ts/.tsx` 第一類：節點語言從**定義文件副檔名**推導（JS vs TS 分標，非 symbol 前綴）；class/interface `#` 符號可查（`Type` 節點，bare-name 查詢；Rust `Type#` 維持不可查）；**CALLS 語法派生**＝Tree-sitter 再剖析（direct/method/optional-chain/constructor 為 CALLS，import/property read 留 REFERENCES；identity＝line+column+name——同列同名 plain load 不污染）；test-path 分類單一源（`__tests__`/`.spec.`/`.test.`）；`enclosing_range` 覆蓋率分母進 build 報告（dogfood 100%，驗收門檻 ≥99%） | ✅ |
| 語言能力邊界（truthful boundaries，防 false-clean） | `graph_audit`/`scip_refs --audit`：完整度 oracle 僅 Rust——純 JS/TS graph＝exit 2 capability-unsupported；Rust+JS/TS 混合＝結果照印但 exit 2 partial＋`[PARTIAL]` 覆蓋聲明（JSON additive `audited_languages`/`unsupported_languages`；wrapper 與直呼同守衛）。`hub_refs --hazard`：hazard 動態層僅 Python 語義——JS/TS 目標＝`hazard_level=unsupported-js-ts`＋`hazard_supported=false`（靜態聚合照常；ambiguous 同名保守處理）。`project` overlay：Python/pyrefly 符號鑄造 only——planned sources 含 JS/TS 即拒（明示訊息）。`boundary*`：維持 NT Python↔Rust 領域 | ✅ |
| Projected-graph overlay for EP planning | `code-reality project --repo <repo> --plan <plan.toml>` — compiles a declarative projection plan (planned symbols/edges/claims, TOML) via the spawned `overlay-gen` (single-source symbol minting + declared-edge-vs-source consistency gate), cat-merges the overlay onto the real index into `.code-reality/projections/<plan-stem>/`, and reports graft surface (real vs projected caller sites), new-symbol reverse chains, and claim verdicts (`[projected][HOLE]` claimed-but-unwired / `[projected][MISSING]` absent symbol). Every output carries the `[projected]` label + hypothetical-edge count — declarations, not evidence. The real slot stays byte-identical (protobuf-face queries only, zero sidecar writes) | ✅ |
| Main-index query-time self-heal + commit-granularity refresh | scip_refs-family queries auto-heal a stale slot before answering (single-flight via `.code-reality/scip/.heal.lock`; a rebuild that still lags warns once and serves — corpus mismatch never loops; `CODE_REALITY_AUTOHEAL=off` reverts to warn-only). The rebuild decision is `needs_rebuild()` — **identity-authoritative since the source identity EP**: a comparable slot (meta carries `source_faces`+`source_identity`+`identity_algo`) rebuilds on `identity_drift` (content-addressed — mtime-newness is no longer fatal, touch is idempotent), the torn-plane guard (graph.db older than the slot, muse P1-3) stays UNCONDITIONAL in both modes, and a legacy keyless slot keeps the baseline trio (mtime-newer OR source-set fingerprint drift OR JS/TS corpus-policy drift — reported but non-fatal in identity mode); faces are scoped from the stamped `source_faces` unioned with detected faces on auto (pinned on explicit — a newly arrived language drifts the current-side identity, the scope mirror that keeps the heal honest), and the fingerprint/identity keys are stamped only on a build whose doc set equals the disk corpus (on mismatch — or a failed identity recompute — a restamp PRESERVES the prior keys, they describe the index, keeping drift visible instead of laundering it into mtime-only freshness; the stamp path always recomputes identity from actual bytes, never a cache) + `code-reality refresh --repo <repo>` (post-commit background face: full idempotent re-produce when sources moved, head-sync re-stamp on docs-only commits — never restamping over corpus drift, identity values unchanged) + `code-reality hook install\|remove --repo <repo>` (opt-in `.githooks/post-commit`; script pins the resolved absolute bin path — GUI-no-PATH safe, release face preferred over the cargo-home dev face so consumer hooks stay silent when the CR checkout churns; logs to `.code-reality/refresh.log`; trailing burst debounce — rebase replays and rapid commits coalesce into one tail refresh via `refresh.pending`/`refresh.scheduled` heartbeat markers, `CODE_REALITY_REFRESH_QUIET_SECS` window default 5s, dead-runner respawn, source-changing lost tails self-heal on the next query, docs-only tails re-stamp on the next refresh; rerunning `hook install` upgrades managed scripts in place on content diff and `refresh` nudges old-format hooks) | ✅ |
| Source identity freshness face (dirty-WT/content-aware verdict for cross-repo consumers) | `code-reality freshness --repo <repo> [--json]` — verdict face, zero heal (only write = the identity cache, `CODE_REALITY_IDENTITY_CACHE=off` reverts fully read-only); exits fresh=0 / stale=1 / no-slot·usage·check-failure=2 (never claims fresh without a slot; the guidance names the empty-terminal state for all-excluded corpora); JSON `{repo, slot, fresh, stale_reasons[torn-plane\|content-drift\|doc-set-drift\|policy-drift\|legacy-signals], head_drift, faces, indexed_source_identity, current_source_identity, identity_algo, serves}` with `serves` ∈ `current-tree` (fresh, consumer may claim fresh) / `committed-baseline` (stale/torn — graph answers are the committed baseline, WT delta lives in live LSP) / `legacy-signals` (keyless meta — read by the baseline criteria, identities null, current NOT computed); head drift is disclosed (`head_drift`) and never fatal (AIR-135.2's freshness has no HEAD on its left-hand side). Identity = `sha256("cr-identity-v1\0" + Σ sorted-by-rel "{face}\0{rel}\0{size}\0{content_hash}\0")` — same content ⇒ same identity (touch / stash round-trip / rebase replay idempotent; mtime only gates the per-file hash cache). Consumer scenarios: preflight a "claimed fresh" index before review (SM-1) · a dirty WT answers stale with `content-drift` (SM-2) · a same-size content swap under a preserved mtime is still caught — the mtime-only blind spot is closed at the identity layer (SM-3) · a missing slot fails loud with build guidance (SM-7) · NT-scale steady-state gate ≈ 40 ms, cold full-hash < 1 s (SM-11; the documented residual: an exact-nanos same-size forged swap can slip the query-side stat gate — bounded by the always-full stamp recompute, disclosed in the consumer SKILL). MCP carries no freshness tool (CLI is the authoritative full face) | ✅ |

## Tests

`cargo test`（Rust suites are the sole test face post-R7 — the Python
parity harness retired with the oracle; history in the archived EPs）.

## Build artifact hygiene

`target/` only grows — cargo never reaps artifacts orphaned by toolchain
or profile changes (measured 2026-09-18: 36.4GiB accumulated vs 4.1GiB
for a fresh full rebuild of the same sources). Cold rebuild is cheap on
the dev machine (debug ~85s, release ~2m36s), so keep it lean by
discipline:

- `cargo clean` when `target/` exceeds ~10GiB or monthly — total
  regeneration cost is the two measurements above; no sweep tooling or
  scheduling needed.
- One-shot builds (agent-dispatched / CI — they never reuse incremental
  state) prefix `CARGO_INCREMENTAL=0`: the incremental cache
  (`target/debug/incremental/`, ~12GiB at its worst) only pays off for
  interactive edit-compile loops (local dev, rust-analyzer flycheck) —
  keep it enabled there.
