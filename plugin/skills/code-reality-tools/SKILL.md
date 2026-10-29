---
name: code-reality-tools
description: "Running code-reality tools (symbol truth queries, caller edges, closures, completeness audits, hub/hazard checks) or authoring .code-reality.toml profiles. Use when you need refs/defs for a symbol, who calls it, whether a graph.db is complete, or whether a symbol is safe to delete. Tool availability: repo root has .code-reality.toml, or code-reality --help exits 0."
when_to_use: "Symbol lookup beyond grep (trait disambiguation), caller-edge queries, delete-safety checks, graph completeness audits, .code-reality.toml authoring, or interpreting pyrefly refs density and delta_tour claims output."
license: MIT
---

# code-reality

Structural facts and governance audits for AI coding sessions. Every
MCP tool takes an explicit `repo_root` (absolute path) — repo is a
parameter, not topology.

> Operational facts here mirror tool behavior; update this file and
> version-bump whenever tool behavior changes.

## MCP tools (this plugin)

| Tool | What it answers |
|---|---|
| `refs(symbol, repo_root)` | Where is this symbol defined/referenced (SCIP index; trait disambiguation) |
| `callers(symbol, repo_root)` | Who calls it (sites included; item-level refs noted) |
| `closure(symbol, repo_root, depth?)` | Transitive callers (BFS; default depth 2) |
| `audit(repo_root)` | graph.db completeness gaps × SCIP refs (two-pass). Rust-only completeness oracle — a graph carrying JS/TS nodes returns a partial/non-passing result (capability boundary), never a clean verdict |
| `build(repo_root, producer?, json?)` | One-shot data-plane build (index + graph.db) for any detected mix of Python / Rust / JS-TS. WRITES `.code-reality/`; LONG-RUNNING: minutes-level, no progress reporting |
| `snapshot(repo_root, label?, out_dir?)` | Boundary snapshot of the current graph state; WRITES a snapshot file (needs an existing graph.db) |
| `delta_tour(repo_root, snapshot_a, snapshot_b, ep?, task?, out_dir?)` | Diff two snapshots into a delta-review CodeTour; WRITES `<repo>/.tours/delta/` (in-repo default — pass absolute snapshot paths) |
| `project(repo_root, plan, json?)` | Projected-graph overlay for EP planning; WRITES `.code-reality/projections/<stem>/`, real slot untouched (needs `overlay-gen` resolvable — `uv tool install pyrefly-producer`) |

The graph_query family (impact_radius, detect_changes, hub/bridge,
communities, flows, search, …) is also MCP-exposed under the same
server — 21 tools total.

Responses embed `[SRC]` provenance lines (index version/commit) and a
`[STDERR]` section for management output. Tool output and internal-error text
have a UTF-8-safe byte cap plus an explicit truncation marker. Parameter-validation
errors are outside this bound; it is not a strict serialized JSON frame limit.
Cancellation suppresses response delivery,
but already-started blocking work may continue and publish files. Transport
EOF may await that work; neither cancellation nor EOF implies rollback.

The separate LSP bridge rejects current checks after known backend death,
including before accepting cached diagnostics. Failed initialization is
terminal for that session; restart the bridge to recover. Writes await
bounded acknowledgements; check and shutdown budgets start at their entry
points, with child cleanup independent of the stdin writer lock. Existing
overlay, diagnostic version/time and quiescence checks still apply.

## Prerequisites (per repo)

### SCIP index (Rust repos)

- `refs`/`callers`/`closure`/`audit` need a SCIP index:
  `rust-analyzer scip <repo>` output saved under
  `<repo>/.code-reality/scip/index.scip`
- Multi-root Cargo workspaces: `code-reality build --producer rust`
  resolves one root set (`rust_projects` in `.code-reality.toml` is
  authoritative; absence auto-discovers immediate-child `[workspace]`
  manifests — deep layouts since CR-19, see the next bullet — with
  ambiguity failing loud CR-RUST-ROOT-06/07), stages one
  rust-analyzer scip per root with git-root-relative path
  normalization, pre-concats the partials into one Rust partial, and
  rejects cross-root DEF symbol collisions before the family merge
- Root auto-discovery range (CR-19): when the repo root carries no
  Cargo.toml AND no immediate child does, a bounded recursive scan
  (depth ≤ 3, so `crates/<name>/` and `apps/<x>/src-tauri` layouts are
  found) classifies manifests by pure TOML — `[workspace]` ⇒ root;
  bare `[package]` under a `[workspace]` ancestor ⇒ member; bare
  `[package]` with no ancestor ⇒ standalone root (workspace roots and
  standalone packages may mix — real repos do). The scan skips
  dot-dirs/`target`/`node_modules` and honors the profile `exclude`
  table (excluded paths contribute no candidates — also for the
  config-mode CR-RUST-ROOT-07 inventory gate, which scans the same
  range; the repo-ROOT `[workspace]` included: a config that lists
  other roots while a root workspace exists trips 07 naming the root
  as `.`). Exclude entries are prefix matches, not globs — `poc/`
  rules out the entire subtree under it; excluded subtrees are pruned
  from the scan traversal (an unreadable excluded directory cannot
  abort the scan — out of governance, out of the walk). Fail-loud
  faces: invalid TOML ⇒ CR-RUST-ROOT-04; bare
  package under only non-workspace manifests (no `[workspace]` can own
  it) ⇒ CR-RUST-ROOT-06; nested `[workspace]` roots ⇒ CR-RUST-ROOT-03
  overlap; a 4th-level manifest is out of range and never a candidate.
  If discovery resolves NO root while the governed Rust corpus is
  non-empty, the build fails before any rust-analyzer spawn (exit 2)
  naming the file count and the `.code-reality.toml` `rust_projects`
  remediation — instead of the legacy opaque rust-analyzer
  "no projects" error from feeding the repo root to the producer
- Regenerate by invoking the repo-pinned rust-analyzer binary by its
  full path. The `rust-analyzer` name on PATH is a rustup proxy that
  resolves the toolchain from the current working directory — calling
  it from any cwd outside the repo silently falls back to the default
  toolchain, producing generic-rendering and coverage drift between
  consecutive indexes (NT incident 2026-08-28, under the since-retired
  out-of-repo slot layout: slot cwd resolved default 1.96.0 while the
  repo pinned 1.97.1 — a +322/−5 false diff).

### Python repos (pyrefly producer)

- One-shot: `code-reality build --repo <repo>` runs the whole chain
  (detect → producer → `graph_db build` → `ensure_indexes`); mixed
  repos run all detected producers and merge the partial indexes into
  one multi-language graph (deterministic python→rust→typescript
  order; every leg stages, the live slot publishes once after all legs
  validate — a later-leg failure leaves the previous index intact)
- Generate the same slot with the Rust-native producer: `cargo run
  --release -p pyrefly-producer --bin pyrefly-index -- --repo <repo>`
  (no Node.js, no venv — bundled typeshed), then `code-reality
  scip_refs --repo <repo> --stamp-meta` and `--build-cache`; the Node
  scip-python fork is the retained fallback, not the default face
- Running pyrefly-index alone and going straight to `graph_db build`
  can populate the graph: writing `index.scip` auto-invalidates superseded
  sidecar artifacts beside the slot, and the build side fails loud on
  an lsp cache db older than `index.scip` (mtime gate) — a stale cache
  would otherwise be silently trusted. This manual chain establishes data
  availability, not the source identity provenance of orchestrated `build`.
- Refs density expectation: pyrefly refs counts sit far below the
  LSP-golden baseline (measured ~12.7× at absorption time, 2026-08) —
  expected, not a bug. pyright LSP counts every attribute member
  access, the cache ingest filters non-fn-shaped refs, and constructor
  calls collapse through the dunder into `__init__`. Cross-producer
  reconciliation uses `golden_corpus.py --normalize` (fn_tail
  comparison key)
- scip-python fallback pitfalls (if used): its workspace resolves by
  cwd — indexing from the wrong directory silently indexes the wrong
  repo and still exits 0; on fatal errors a partial index is still
  written, so the exit code is the failure signal

### JavaScript/TypeScript repos (scip-typescript producer)

- First-class structural face for `.js/.jsx/.mjs/.cjs/.ts/.tsx`:
  `code-reality build --repo <repo>` auto-detects the family (one
  producer, two graph-language labels derived from the defining
  document extension); `--producer typescript` forces the leg
- External prerequisites (ship in no wheel, like rust-analyzer):
  `scip-typescript` (structural producer; Node ≥ 18 runtime) and, for
  the type face, `typescript-language-server` + `typescript` via the
  lsp-bridge. Resolution order: repo-local `node_modules/.bin` →
  `CODE_REALITY_NODE_BIN_DIR` → PATH. No `npx`/network at build or
  query time — a missing binary is a loud env failure with install
  guidance
- Existing root `tsconfig.json`/`jsconfig.json` is used ONLY when it
  covers the governed corpus exactly; otherwise (missing config,
  zero-file outcomes, partial coverage) CR writes an ephemeral
  sidecar config with the explicit governed `files` list under
  `.code-reality/` — the target repo's config is never modified, and
  glob inference is never trusted for extension discovery
- Graph semantics: syntactic CALLS are derived by a Tree-sitter
  re-parse (direct/method/optional-chain/constructor calls are CALLS;
  imports and property reads stay REFERENCES; identity is
  line+column+name grain); class/interface `#` symbols are queryable
  as `Type` nodes (bare-name queries); test-path classification
  understands `__tests__/`, `*.spec.*`, `*.test.*`
- Freshness: JS/TS sources participate in the same self-heal (below),
  including add/delete/rename via the source-set fingerprint and
  corpus-policy (profile exclusion) changes; a profile-excluded
  generated tree neither triggers nor appears in heals. If the whole
  corpus ends up excluded, an auto-detect build converges to empty
  (index+graph removed) instead of erroring forever — an explicit
  `--producer <empty face>` still errors loud
- Capability boundaries (S6): `audit`/`graph_audit` completeness is a
  Rust-only oracle — JS/TS-containing graphs get a partial/non-passing
  result with explicit coverage fields (use `refs`/`callers`/
  `graph_query` for JS/TS structural facts); `hub_refs --hazard` marks
  JS/TS targets `hazard_level=unsupported-js-ts` +
  `hazard_supported=false` (static aggregation stays); `project`
  rejects JS/TS planned sources (Python/pyrefly identities only)

### Query-time self-heal & commit-granularity refresh (opt-in)

- scip_refs-family queries (query / --callers / --closure / --audit) on
  a stale slot rebuild it before answering — single-flight across
  concurrent sessions through `.code-reality/scip/.heal.lock`. A rebuild
  that still leaves the slot behind warns once and serves (detection vs
  corpus mismatch never loops). Since the source identity EP the
  rebuild decision is identity-authoritative: a slot whose meta carries
  the identity keys rebuilds on `identity_drift` (content-addressed —
  the mtime-only blind spot, e.g. a same-size content swap under a
  preserved mtime, is closed at this layer), while the torn-plane guard
  (canonical main graph.db missing or older than the slot) stays unconditional;
  alternate and projection indexes are graph-optional. Legacy keyless
  slots keep the baseline mtime/fingerprint criteria. Under an ACTIVE
  writer (a heal that finishes and still finds sources newer than the
  slot) — and equally when the heal fails outright (since CR-5 this
  includes the CR-RUST-ROOT-09 rust coverage gate rejecting an
  ungoverned orphan; before that failure paths armed nothing, so every
  query re-burned the build) — a churn cooldown marker (`.heal-churn`,
  10min) is armed — later
  queries inside the window serve the existing index with a WARN
  instead of re-burning a minutes-scale heal that cannot converge
  anyway; a head drift (commit boundary) always overrides, and a
  converging heal clears the marker.
  `CODE_REALITY_AUTOHEAL=off` reverts to warn-only;
  `CODE_REALITY_HEAL_COOLDOWN_SECS=0` disables the cooldown; explicit
  `--index` paths and the write modes are never
  healed. The manual chain above stays for explicit maintenance.
- `code-reality refresh --repo <repo>` is the post-commit background
  face (full re-produce when sources moved; docs-only commits re-stamp
  provenance only). `code-reality hook install --repo <repo>` wires the
  opt-in `.githooks/post-commit`; install refuses loudly over unmanaged
  hooks, a foreign `core.hooksPath`, or active `.git/hooks/*` entries
  (the local-hooks guard applies only when installing would flip
  `core.hooksPath` — inert leftovers never block a managed rerun),
  and the script pins the resolved absolute bin path (GUI-no-PATH safe),
  preferring the release face when one is installed (`~/.local/bin`
  via uv — the cargo-home dev face is the fallback, so consumer hooks
  stay silent when the CR checkout churns),
  logging to `.code-reality/refresh.log`. The hook debounces event
  bursts (rebase replay, rapid commits) with a trailing quiet window —
  one refresh per burst tail, not one per commit; the runner heartbeats
  `refresh.scheduled` and a dead marker is respawned;
  `CODE_REALITY_REFRESH_QUIET_SECS` overrides the window (default 5s);
  a source-changing lost tail self-heals on the next query, a docs-only
  lost tail re-stamps on the next refresh. Rerunning `hook install`
  upgrades a managed script in place on content diff (byte-identical is
  a no-op) — the one-command migration for template updates; an
  old-format hook still invoking `refresh` gets a log nudge to upgrade.

### Freshness verdict face (source identity)

- `code-reality freshness --repo <repo> [--json]` answers one question
  for a cross-repo consumer: does the indexed source identity equal the
  current source identity, dirty working tree included? Verdict face,
  zero heal (the only write is the identity cache;
  `CODE_REALITY_IDENTITY_CACHE=off` makes every identity computation
  fully read-only). Exits: fresh=0 / stale=1 (a legal answer — stdout
  still carries the verdict) / no-slot, usage, or check-failure=2 with
  loud guidance that names the build command and the empty-terminal
  state (an all-excluded corpus legitimately has NO slot — absence is
  never read as fresh). Head drift never kills fresh; the JSON
  discloses it in `head_drift`.
- JSON contract: `{repo, slot, fresh, stale_reasons, head_drift, faces,
  indexed_source_identity, current_source_identity, identity_algo,
  serves}`. `stale_reasons` vocabulary: `torn-plane` (graph.db older
  than the canonical main slot, or missing), `content-drift` (identity or
  index-byte binding mismatch — includes the
  mtime-preserved swap shape and a newly arrived language face),
  `doc-set-drift`, `policy-drift` (both reported but non-fatal in
  identity mode), `legacy-signals` (keyless meta judged by the baseline
  criteria), `identity-algo-upgrade` (a Rust-face slot whose stamped
  identity_algo predates sha256-v2 — the Rust producer inputs joined
  the identity body, so such a slot never answers fresh; a rebuild
  re-stamps v2). `serves`: `current-tree` (fresh — the consumer may claim
  fresh), `committed-baseline` (stale/torn — graph answers describe the
  committed baseline, the WT delta lives in live LSP), `legacy-signals`
  (keyless meta — interpret by the baseline criteria; the identity
  fields are null there because the current side is not computed).
- **Identity definition** (cross-repo comparable contract, sha256-v2):
  `sha256("cr-identity-v2\0" + Σ sorted-by-rel "{face}\0{rel}\0{size}\0{content_hash}\0" + domain-separated Rust producer inputs (Cargo manifests/locks/toolchain config of the resolved workspace roots) + workspace topology markers)`
  — same content AND same workspace topology ⇒ same identity, so touch /
  stash round-trips / rebase replays are idempotent; mtime gates the
  per-file hash cache only, never the identity body. A v1/legacy
  Rust-face slot answers `identity-algo-upgrade` stale (never fresh);
  non-Rust v1 slots answer `legacy-signals` one-time until rebuilt.
- **Source provenance**: orchestrated `build` captures uncached source
  identity and corpus policy A before producers, compares after production
  and again under publication ownership, then stamps A bound to the index
  bytes (`index_sha256`) when the produced document set matches. Manual and
  head-only restamps preserve prior provenance only for matching index bytes,
  repository and selection. They never recompute current source into trusted
  provenance for an old index. Unbound metadata uses legacy signals, not a
  verified source-identity comparison. Endpoint equality is not an immutable
  snapshot: ABA edits and edits after the final check remain outside the
  guarantee.
- **Writer ownership**: umbrella publication and explicit graph builds share
  a bounded writer guard. Contention returns a controlled busy error; retry
  after the active writer finishes. Explicit graph ownership covers input
  acquisition through derived tables and publication; derived tables are
  completed before temporary database rename. Heal single-flight remains a
  separate outer lock. Non-cooperating writers and live source edits are
  outside this ownership contract.
- **Cache-validity residual risk (disclosed)**: the query-side per-file
  cache reuses a content hash when `(size, mtime[secs+nanos])` match
  exactly — a same-size swap whose mtime is restored to the exact
  nanosecond (a precise forgery; ordinary rsync -a/tar restores and
  same-second formatter rewrites do NOT match to the nanosecond) can
  slip the query-side gate. This is accepted and bounded: the
  orchestrated production fences hash actual source bytes with the cache
  disabled; manual restamps only preserve matching prior provenance. The
  worst query-side outcome is a false
  stale (safe direction — a rebuild purges the cache) or this
  documented residual, and any suspicious cache (corrupt, downgraded
  version, foreign repo binding) is discarded wholesale and recomputed.
- **Naming discrimination**: the `cr-freshness` leaf crate and the
  `tests/freshness.rs` pin file are the BINARY version-freshness axis
  (`--version` rev-mismatch WARN). The `freshness` SUBCOMMAND and
  `tests/source_identity.rs` are the INDEX source-identity axis. They
  share a name and nothing else.

### Slot discipline

- One producer per slot — don't mix producers in the same
  `<repo>/.code-reality/scip/` slot: when an lsp-harvest cache sits
  beside a SCIP index, the SCIP index is preferred and the cache is
  silently ignored
- Manual artifact sequence: generate → `--stamp-meta` → `--build-cache`;
  use orchestrated `build` to establish bound source-identity provenance.
- Legacy `~/.mosaic/code-reality/` slots migrate one-shot via
  `code-reality sidecar_migrate --repo <repo>` (missing-index errors
  auto-suggest this bridge)

### graph.db

- `audit` reads the self-owned db at
  `<repo>/.code-reality/graph.db` — produce it with
  `code-reality graph_db build --repo <repo>` (edges split CALLS vs
  REFERENCES by build-side call detection). The refresh chain is
  purely producer-side (the legacy import face is fully removed)
- Edge kinds are a build-side syntactic derivation: ruff parses each
  `.py` file, and dunder-constructor calls resolve through the
  class-segment fallback

### Profile

- Optional `.code-reality.toml` at repo root declares module rules,
  exclusions, claims prefixes, scan roots — repo facts belong to the
  repo (authoring procedure below)

## Authoring `.code-reality.toml`

Generic shape:

```toml
exclude = ["docs/", "fixtures/", ".venv/"]  # directories end with a slash; a slashless entry is an exact-file path

[[module]]             # ordered, first match wins
prefix = "src/mylib/"  # main code directory, trailing slash
depth = 1              # modules = direct subdirectories; root files belong to the prefix
```

Without a profile: modules fall back to top-level directories, exclude
covers only `.venv/`, claims always read NONE, boundary stays
crash-only, and the hazard-registry rules never fire (the other hazard
rules don't depend on the profile).
The `exclude` table governs the Python walk face and, since CR-5, the
Rust face alike — use it to declare ungoverned orphan Rust paths
(evidence scripts, fixtures outside Cargo targets); an orphan that is
not excluded now fails the build loudly (CR-RUST-ROOT-09) instead of
silently forfeiting the source-identity certification. When the orphan
shares a directory with indexed python evidence, exclude the exact
file path (a slashless entry) — a directory entry would drop the
python face along with it. Feature-gated files rejected by the
coverage gate have a first-class remediation besides exclude: the
`[rust]` profile section (`features = [...]` global, or
`features_by_root` for per-workspace sets, CR-16) passes cargo
features to the rust-analyzer scip leg via a temp `--config-path`
JSON — unknown feature names fail loud pre-spawn, the effective
selection enters the source identity (a features-only change flips
freshness stale), and an absent/empty `[rust]` section keeps argv and
identity byte-identical. Upgrade note:
repos whose exclude table already covers `.rs` paths change identity
input on first build after upgrading — the first freshness verdict
answers `identity_drift` once and one rebuild converges it.

Authoring procedure for a new repo (four fixed steps):

1. **Decide the module rules.** Find the main code directory — the
   layer with implementation logic, not docs/tests/generated
   artifacts. `prefix` is that directory; `depth` says which directory
   level a module is (`depth = 1` = direct subdirectories; root files
   belong to the prefix itself). Multi-root repos write multiple
   `[[module]]` blocks (ordered, first match wins). Rule of thumb: the
   granularity you want module-level comparisons reported at is the
   module layer. Prefixes must end with a slash (profile-load assert).
   Prefix coverage still shapes chain_tour frame
   survival — frames under a module prefix resolve through the
   module segment (exact direct, then suffix/basename/ctx pools);
   frames outside every prefix now fall through to the repo segment
   (repo-relative direct, then a lazily indexed repo-wide filename
   pool), so a missing prefix no longer sends them wholesale into the
   external bucket — but keep covering every layer callstacks pass
   through, including declaration layers (`.pyi` stubs) and examples:
   the repo pool only adopts basename-unique or ctx-majority files, and
   module-pool resolution stays the precise face.
2. **Decide exclude.** List every non-code directory (docs, research
   artifacts, fixtures, stubs, generated `dist`/`node_modules`).
   Directory entries keep the trailing slash: `"docs/"`, not `"docs"`
   — under startswith matching a slashless directory name would also
   hit same-prefixed files like `.venv-setup.py`. A slashless entry is
   an exact-file path (`"audit/lsp/probe.rs"`) — name the full file
   path; it is prefix-matched exactly, never interpreted as a
   directory.
3. **scan_root only for pyo3 reconciliation repos** — a rust-source ×
   `.pyi`-stub boundary scan (boundary_build/boundary). General repos
   don't write it.
4. **Smoke-verify.** Run `code-reality snapshot --repo <repo> --label
   smoke` and judge the module split against intuition (wrong split →
   back to step 1). Later, chain_tour's `not-in-graph` stats signal
   graph freshness and `external`/skip stats signal prefix coverage —
   frames landing wholesale in external means a missing prefix layer
   (back to step 1). The profile file lives at the repo root;
   committing it is that repo's decision.

Repos with a registry auto-discovery mechanism (classes registered by a
scan rather than by direct callers) add one `[[hazard_registry]]`
block. Registration is invisible to caller edges — nobody calls the
constructor directly — so this block teaches the hub_refs hazard layer
to treat matching definitions as referenced:

```toml
[[hazard_registry]]  # file under package_prefix + name suffix => presumed registered
package_prefix = "src/mylib/rules/"
suffix = "Rule"
register_fn = "auto_register_rules"
registry = "RULE_REGISTRY"
evidence = "src/mylib/rules/discovery.py:42"  # optional — display pointer into the registration chain
```

A definition file under `package_prefix` whose name ends with `suffix`
is presumed registered via `register_fn` into `registry`; `evidence`
is optional and display-only.

## Reading claims output (delta_tour)

- The claims regex derives from `[[module]]` prefixes — only path
  mentions under those prefixes are recognized. Non-matching changes
  leave the claims column at NONE, which means "no comparison
  provided" (the single-column edge diff is still usable) — not "the
  compared document makes no claims".
- Relative-path mentions (`adapters/sj/x.py` style) normalize into
  hits via existence checks under the prefix directories when
  repo_root is available.
- Claims are three-state: ⚠ only appears in the comparable state.
  Empty claims (profile not loaded / nothing parseable) → the whole
  block reads "not compared", zero ⚠, plus a stderr WARN. Non-empty
  claims always compare — zero hits faithfully presents real drift
  (⚠/✗) with an observability WARN (real drift or a granularity
  issue).

## Known shape assumptions (boundary)

`boundary_build`'s deep shape (pyi_module segment derivation,
method→class same-crate join, pyclass derive scan) assumes the
NautilusTrader repo structure: scan_root is configurable, but
pyi_module derivation requires the path to contain a `nautilus_trader`
segment. Non-NT layouts crash (loud) by design and the reconciliation
semantics are unverified for them — smoke first (small manual sample)
on any new repo.

## Query-shape notes

- `search` works as a single keyword per query — multi-word queries
  fall through to whole-phrase LIKE matching and typically return
  nothing.
- SCIP DEFs cover functions, methods, and Python classes: a bare Python
  class name resolves to the class DEF — `scip_refs <Class> --callers`
  returns subclass / isinstance / constructor-call sites (end-anchored:
  a nested class matches by its own name only, `Outer` never matches
  `Outer#Inner#`). `Class.method` dot form matches methods; the
  `Class#method()` hash form is the DISPLAY format, not a query key.
  Rust struct/trait/type names are still not resolvable query keys —
  query one of their methods instead; module variables (`NAME.` form)
  remain non-queryable. Dataclass-style classes (constructor call with
  no corpus `__init__`) additionally mint a pseudo-constructor DEF
  (`Class().`), so a bare query then returns both groups (B7b).
- `delta_tour --out-dir` resolves against the executing cwd, not the
  `--repo` root — run it from the repo (or pass an absolute path), or
  the tours land in the caller's `.tours/delta/`. The MCP
  `delta_tour` tool has no cwd trap: omitted `out_dir` defaults to
  `<repo_root>/.tours/delta`.
- `chain_tour` frame grammar + repo-segment path rescue (CR-20): the
  three frame path regexes assemble their extension alternation
  dynamically from `language::JS_TS_EXTS` plus the `py`/`rs` literals
  (single source — no second extension list), longest-first so
  `foo.tsx` never reads as `foo.ts` in the un-anchored any-path scan;
  all eight governed extensions (py/rs/js/jsx/mjs/cjs/ts/tsx) parse in
  the explicit `path:line`, line-less, and stray-second-token shapes,
  and a prose remainder like `see foo.tsx bar.py` stays stray →
  `noref`. Resolution is four-stage: module-exact direct (frozen) →
  repo-relative exact direct (`repo-direct` — a full path like
  `crates/x/src/lib.rs` names the repo file directly, checked
  regular-file + repo containment + exclusion, canonicalized so the
  graph's stored abs path matches) → the frozen module heuristic →
  a lazily built repo-wide filename pool (`repo-suffix` — single-pass
  walk pruning dot-dirs/`node_modules`/`__pycache__`/`venv`/`target`,
  governed-extension files only, profile exclusions applied; suffix
  filter, then basename pool, then ctx majority with a UNIQUE winner —
  a tie refuses as `ambiguous`). A module-segment ambiguity is
  terminal (the repo pool never second-guesses it); `(None, "none")`
  after both segments still reads `external` (the label is unchanged
  this card). Rescues are measurable: `ScenarioTours.repo_rescues` and
  a trailing `[LOG] repo 段救援: {"repo-direct": N, "repo-suffix": M}`
  line in run output. Known residual (honest limits, not fixes): the
  explicit `path:line` scan still has no token-boundary check (a
  `path.ts:12` inside a URL/query matches, the old risk extended to
  the new extensions), and prose frames citing a `.ts` file now
  fail-closed to noref where the py/rs-only grammar let them through.
- `chain_tour` line-less frames (path + symbol, no line number) resolve
  against `graph.db` by exact file + bare name. Multiple same-file
  same-name nodes used to fail closed as an `ambiguous` skip; the
  closed candidate set now narrows through three rules (still
  fail-closed everywhere else):
  - **R1 pseudo-constructor pairing** — the candidate set is exactly
    {Type `X#`, its minted constructor `X().`} with equal `line_start`
    → fills the class line; the step's `x-codeReality.symbol` carries
    the Type symbol (a canonicalization, not an equivalence claim over
    the two SCIP symbols). Python/JS-TS faces only — Rust `Type#`
    nodes never reach the queryable set.
  - **R2 qualified-name alignment** — the frame's dotted token must
    tail-align a candidate's structured descriptor chain
    segment-by-segment (whole-segment equality, case-sensitive):
    equal-length full match first (tier-1), then strictly-shorter doc
    chains (tier-2); exactly one survivor in the winning tier fills.
    A bare (unqualified) token only ever fills via R1 or a tier-1
    exact full-chain match — never through tier-2 (every same-name
    candidate chain ends in the name, so a bare token tail-aligns all
    of them and stays ambiguous). Frames whose symbol remainder
    carries prose around the token (e.g. `see Cls.method`) are not
    eligible for disambiguation — neither R1 nor R2 — and stay
    ambiguous with hints.
  - **R3 candidate hints** — residual ambiguous frames stay skipped
    and the tour description gains one trailing `歧義候選：` line
    listing descriptor + line + kind (≤4 per frame, ≤3 frames detailed
    per scenario, remainder folded into a count). Unambiguous tours
    keep a byte-identical description; the `ambiguous` distribution
    key is unchanged.
  Non-goals (separate arcs): Rust `::` frame grammar BEYOND the
  qualifier form (CR-18 bullet below), and Python overloads. Prose
  frames without a path never
  reach the resolver (`noref`); path-bearing frames whose token misses
  the graph degrade to `not-in-graph` or an ambiguous skip with hints
  — fail-visible both ways.
- `chain_tour` explicit-line trait collision (CR-17): a frame WITH a
  line number whose same-file same-name candidate set contains ≥2
  Rust impl-shaped symbols sharing one inherent type with differing
  trait groups (`impl#[Price][Debug]fmt().` vs
  `impl#[Price][Display]fmt().`; inherent-vs-trait counts as
  differing; generic trait groups like `` `Add<Self>` `` are compared
  verbatim, bracket-depth tracked since CR-18 — CR-17 compared flat
  text up to the first `]`) is a collision. Safe selection: no
  collision → the legacy
  nearest-line pick, byte-identical; collision + the doc line exactly
  hits one candidate → the anchor is kept (`same`) with the candidate
  list attached to the step description for human review; collision +
  drifted line OR no *unique* exact hit (two candidates sharing the
  doc line) → fail-closed, the frame produces NO
  step — nearest-line guessing is refused because it would mint an
  unverified `x-codeReality.symbol` identity that tour_reanchor then
  maintains as truth. Both modes emit one `[WARN] trait 碰撞`
  line per frame — on stdout in the success face; the all-shell
  fail-closed face (exit 1, zero tours written) carries them on
  stderr — listing the first four candidates plus the omitted
  count when truncated (candidate brackets render full-width so the
  lists stay link-inert for tour_validate), add a
  `trait 碰撞候選：` line to the scenario description, and count under
  the independent `explicit_trait_ambiguous` distribution key (never
  folded into `ambiguous`, which keeps its line-less semantics). A
  failed candidate read (prepare/query_map/row decode) fails closed as
  `graph-read-error` — a doc-line step with NO minted identity, no
  nearest-line fallback, one `[WARN] graph 讀取錯誤` line + the same
  `[LOG]` distribution key. R1 /
  R2 / R3 line-less behavior is untouched. CR-18 F6 addendum: a
  malformed impl head in the candidate set trips the collision
  semantics (exact-hit-or-refuse) instead of being skipped silently,
  and trait groups parse with bracket-depth tracking
  (`AsRef<[u8]>` reads whole — CR-17's non-nested `` `Add<Self>` ``
  groups compare byte-identically as before).
- `chain_tour` md trait-qualifier syntax (CR-18): a frame whose symbol
  remainder is `Receiver::method[Trait]` (canonical
  `Price::fmt[Display]`; compatible `Price::fmt()[Display]`; a
  `(path:line)` wrapper after the bracket folds off — a trailing `()`
  after the group is wrapper residue by the tail rule (only the empty
  tail or `()` may follow the bracket group); generic trait
  groups verbatim with bracket-depth — `` Quantity::add[`Add<Self>`] ``,
  `Buf::as_ref[AsRef<[u8]>]`) declares the target's identity. The
  parser produces the `receiver/method/trait` triple directly — never
  `best_ident` (which length-ranks the trait name on the paren-less
  form, the POC-proven trap) — and the shared qualified selector
  matches it against the candidate set's impl heads on ALL THREE axes
  (method = the `name=?` query, receiver = the inherent type group,
  trait = the verbatim trait groups): a trait-only match never selects
  a different type's same-trait impl. Core invariant: once the author
  provides a qualifier, nothing unproven may re-select by line number,
  name similarity, or R1/R2. Four states: exactly one triple-match →
  SELECTED (counts under the legacy `same`/`moved`/`filled`/
  `moved-file` keys; collision semantics bypassed — the declared
  identity is the evidence, no `trait 碰撞` WARN; the step gains a
  `qualifier 命中` note); zero matches → `qualifier_no_hit`; multiple →
  `qualifier_ambiguous` (never R1/R2); a malformed sibling impl head
  that cannot be ruled out (same-or-unparseable receiver) blocks the
  would-be unique hit → `qualifier_unverified` (F6: identity must be
  PROVEN — a MalformedImpl whose receiver parsed as a different type
  is ruled out and does not block). Qualifier-looking syntax that does
  not parse (`Price::fmt[Display`, empty `[]`, trailing junk) →
  `qualifier_malformed` — the frame skips even with an explicit line
  number. A bracket group carrying no ASCII letter (`self::items[0]` —
  the trait slot reads `0`) is indexing prose, not a qualifier shape,
  and stays legacy; the `::`-plus-later-`[` trigger stays deliberately
  wide, so a far-distance prose bracket (`tokio::spawn and items[0]`)
  parses Malformed and skips fail-loud rather than silently degrading
  to `best_ident` (corpus census 2026-10-19: NT `.tours/` titles +
  callstack md carry zero symbol-reachable instances). Every refusal
  state skips the frame (no step, no minted
  `x-codeReality.symbol`) and discloses on the same three faces as
  CR-17 (skip-reason axis, `[LOG]` distribution, one `[WARN]
  qualifier 拒絕` per frame — stdout on the success face, stderr on
  the all-shell face — with the full candidate list, full-width
  brackets). The cross-file moved-file branch (F7) applies the same
  discipline AFTER single-file convergence: the converged file's FULL
  candidate set is read fail-closed (a read error degrades to
  `graph-read-error`, never a silently partial set), the qualifier (or,
  qualifier-less, the CR-17 collision gate — an unresolved collision
  now refuses under `cross_file_trait_ambiguous` instead of the legacy
  silent lowest-line pick; no collision keeps the byte-identical
  lowest-line pick) decides, and `rels > 1` stays
  `moved-file-ambiguous` — a qualifier never selects across files.
  `tour_reanchor --bootstrap-symbols` consumes qualifier titles
  through the SAME parser/selector (a malformed qualifier title is an
  explicit backfill refusal) — the maintenance tool cannot reintroduce
  the ambiguity this syntax removed.
- `chain_tour` all-skip shell gate (CR-11): a scenario whose every
  frame skipped (zero anchored steps) is a shell — it lands no `.tour`
  file and no manifest row; each shell emits a `[WARN]` (family +
  scenario title + frame count + skip-reason distribution), later
  scenarios renumber to the compressed written order, regen
  reconciliation removes chain_tour-owned family files/rows outside the
  write set (foreign-generator, rowless, or non-`.tour` residue
  untouched; rows persist before files are deleted, and a file-removal
  failure only warns — a disclosed rowless orphan is the tolerated
  direction), and the
  summary gains a `shells-omitted` counter. The manifest itself
  persists via an atomic temp-file+fsync+rename (a partial write can
  never truncate it; rename keys on directory permission — a read-only
  target file does not block a dump, and the replace swaps the inode,
  so a pre-existing target's permissions/metadata do not carry over). A
  fully-shell md fails
  closed — exit 1, zero writes, the existing parse-fail face — with the
  pre-existing family's tours/rows disclosed for manual cleanup.
  `--primary` keeps numbering md scenarios (shells included); pointing
  at an omitted shell is the invalid-argument crash. There is no
  frame-count floor: a 1-2-frame real-anchor scenario is legal.

## CLI surface (broader)

The MCP face covers the SCIP family, the graph_query family, and the
data-plane family (build/snapshot/delta_tour/project — write side
effects, re-adjudicated 2026-08-29). The same binary carries the full
toolchain: `code-reality <scip_refs|snapshot|graph_audit|
hub_refs|boundary|boundary_build|build|chain_tour|delta_tour|tour|
tour_manifest|tour_validate|tour_upgrade|tour_reanchor|runtime_edges|
graph_query|graph_db|project|refresh|freshness> --repo <root>`. (Diff consumption runs through
`delta_tour` — the transition CLI retired; snapshot sidecar pairs feed
delta_tour directly. `tour register|materialize` is the intent-level
two-phase materialization face: a manifest `[[delta_arc]]` provenance row
keyed on arcId drives the full recipe — snapshot-pair resolution, stale
gate, EP claims gate — into `.tours/delta/<arcId>.tour`. `tour_reanchor`
re-anchors tour steps by SCIP symbol identity (`x-codeReality.symbol`
via graph.db): dry-run by default (per-step proposals + stats),
`--bootstrap-symbols` backfills symbols onto legacy line/pattern anchors
only for unique, anchor-consistent same-file candidates, `--apply`
writes only verified steps and never touches the manifest; symbol
resolution is freshness-gated — on a stale graph both faces degrade to
legacy file/line/pattern validation with one WARN.)

### Projection plans (`project`)

`code-reality project --repo <root> --plan <plan.toml>` compiles an EP's
declared future into a projected graph (the spawned `overlay-gen` bin
from the `pyrefly-producer` dist does the minting) and reports graft surface,
new-symbol reverse chains, and integration-claim verdicts. The plan
(TOML) declares `[meta]` (`name`, `project`/`version` mirroring the
target repo's pyproject identity — the SCIP symbol prefix keys on it,
plus optional `graph_rev`), `[[symbols]]` (planned defs), `[[edges]]`
(declared call edges: `file` + `needle` locate the call in the planned
source under `<plan dir>/sources/`; the gate fails loud when the
declared edge is not an actual call site), and `[[claims]]`
(integration claims checked against the projection: `[HOLE]` = has DEF
but no call edge from the planned files, `[MISSING]` = symbol absent).
Every line is labeled `[projected]` and the report counts hypothetical
edges — **projected edges are declarations, not evidence**; a wrong
mental model projects a wrong world, so findings feed ep-review as
leads to verify, never as proof. Reruns are idempotent; the real
`scip/index.scip` / `graph.db` are never touched (the projection lives
only under `.code-reality/projections/<stem>/`). Location caveats:
`needle` binds its FIRST occurrence in the planned file (a non-call
first occurrence fails the gate loud), and planned symbol names must
not appear earlier in the file than their def (docstring/comment
mentions would misplace the DEF).

Install (main face): `uv tool install code-reality` (plus
pyrefly-producer and code-reality-lsp-bridge — or let the plugin's
MCP wrapper bootstrap all three on first session). Developer face:
`cargo install --path <this-repo>/crates/code-reality`.
Full docs: the repo README.

### uv discovery contract (single source for wrapper & downstream resolvers)

How `uv` itself is located when the plugin wrapper (and any downstream
resolver, e.g. a dispatcher's MCP-supply preflight) needs it. GUI-launched
harnesses freeze a minimal app PATH that lacks `~/.local/bin` — the uv
installer's default home — so bare-name lookup alone is launch-face
dependent. The contract:

1. Resolve once, in order: first executable `uv` on PATH; only if PATH
   has none, `$HOME/.local/bin/uv` (executable check included). Anything
   else = typed fail-loud (wrapper: exit 127 with install guidance).
2. No pivot after selection: every later invocation (`tool dir --bin`,
   `tool install --force`) uses the SAME selected executable; if its
   `tool dir --bin` fails, fail loud — never retry the next candidate.
   A broken or outdated uv must surface as its own error, not be
   silently masked by the fallback.
3. Deliberately absent: hardcoded Homebrew/brew-prefix candidates. Both
   launch faces carry `/opt/homebrew/bin` (or `/usr/local/bin`) in PATH,
   so step 1 already covers them; hardcoding would override host PATH
   policy and is not cross-platform.
4. Downstream gates untouched: `uv tool dir --bin` stays the sole
   authority for where the tools live; registry pin, plugin-manifest
   version checks, binary `--version`/digest and freshness gates are
   unchanged by discovery. Resolving uv is not a supply approval.

Both `.mcp.json` wrapper shells implement this contract;
`scripts/test-plugin-wrapper.sh` (F-series) pins it: PATH hit / PATH
miss + HOME hit / all miss / non-executable candidate = miss / found-
but-broken = no-pivot, per wrapper string. Cross-repo resolvers mirror
the same observable contract and the same scenario matrix rather than
importing code.
