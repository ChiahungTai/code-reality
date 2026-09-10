---
name: code-reality
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
`[STDERR]` section for management output.

## Prerequisites (per repo)

### SCIP index (Rust repos)

- `refs`/`callers`/`closure`/`audit` need a SCIP index:
  `rust-analyzer scip <repo>` output saved under
  `<repo>/.code-reality/scip/index.scip`
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
  is also safe: writing `index.scip` auto-invalidates superseded
  sidecar artifacts beside the slot, and the build side fails loud on
  an lsp cache db older than `index.scip` (mtime gate) — a stale cache
  would otherwise be silently trusted
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
  corpus mismatch never loops). Under an ACTIVE writer (a heal that
  finishes and still finds sources newer than the slot) a churn
  cooldown marker (`.heal-churn`, 10min) is armed — later queries inside
  the window serve the existing index with a WARN instead of re-burning
  a minutes-scale heal that cannot converge anyway; a head drift (commit
  boundary) always overrides, and a converging heal clears the marker.
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

### Slot discipline

- One producer per slot — don't mix producers in the same
  `<repo>/.code-reality/scip/` slot: when an lsp-harvest cache sits
  beside a SCIP index, the SCIP index is preferred and the cache is
  silently ignored
- Artifact sequence: generate → `--stamp-meta` → `--build-cache`
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
exclude = ["docs/", "fixtures/", ".venv/"]  # directory granularity, trailing slash

[[module]]             # ordered, first match wins
prefix = "src/mylib/"  # main code directory, trailing slash
depth = 1              # modules = direct subdirectories; root files belong to the prefix
```

Without a profile: modules fall back to top-level directories, exclude
covers only `.venv/`, claims always read NONE, boundary stays
crash-only, and the hazard-registry rules never fire (the other hazard
rules don't depend on the profile).

Authoring procedure for a new repo (four fixed steps):

1. **Decide the module rules.** Find the main code directory — the
   layer with implementation logic, not docs/tests/generated
   artifacts. `prefix` is that directory; `depth` says which directory
   level a module is (`depth = 1` = direct subdirectories; root files
   belong to the prefix itself). Multi-root repos write multiple
   `[[module]]` blocks (ordered, first match wins). Rule of thumb: the
   granularity you want module-level comparisons reported at is the
   module layer. Prefixes must end with a slash (profile-load assert,
   same as exclude). Prefix coverage also decides chain_tour frame
   survival — frames whose paths fall under no prefix resolve-fail
   into the external-skip bucket, so cover every layer callstacks pass
   through, including declaration layers (`.pyi` stubs) and examples.
2. **Decide exclude.** List every non-code directory (docs, research
   artifacts, fixtures, stubs, generated `dist`/`node_modules`).
   Always directory-granularity with a trailing slash: `"docs/"`, not
   `"docs"` — the profile-load assert enforces it, and under startswith
   matching a slashless entry would also hit same-prefixed files like
   `.venv-setup.py`.
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

## CLI surface (broader)

The MCP face covers the SCIP family, the graph_query family, and the
data-plane family (build/snapshot/delta_tour/project — write side
effects, re-adjudicated 2026-08-29). The same binary carries the full
toolchain: `code-reality <scip_refs|snapshot|graph_audit|
hub_refs|boundary|boundary_build|build|chain_tour|delta_tour|
tour_manifest|tour_validate|tour_upgrade|runtime_edges|
graph_query|graph_db|project> --repo <root>`. (Diff consumption runs through
`delta_tour` — the transition CLI retired; snapshot sidecar pairs feed
delta_tour directly.)

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
