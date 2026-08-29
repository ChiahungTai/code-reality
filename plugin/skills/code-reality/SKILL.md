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
| `audit(repo_root)` | graph.db completeness gaps × SCIP refs (two-pass) |

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

## CLI surface (broader)

The MCP face covers the SCIP family. The same binary carries the full
toolchain: `code-reality <scip_refs|snapshot|graph_audit|
hub_refs|boundary|boundary_build|chain_tour|delta_tour|
tour_manifest|tour_validate|tour_upgrade|runtime_edges|
graph_query|graph_db> --repo <root>`. (Diff consumption runs through
`delta_tour` — the transition CLI retired; snapshot sidecar pairs feed
delta_tour directly.)

Install/upgrade: `cargo install --path <this-repo>/crates/code-reality`.
Full docs: the repo README.
