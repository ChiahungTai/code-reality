# EP: W5 — import_legacy full removal (data-plane self-management axis finale)

One-liner: remove `import_legacy` entirely — subcommand + importer +
tests + retired-banner paths + both-end doc residue; fold in the two
live findings surfaced by the entry check; delete the CR repo's own
legacy db. Zero residue on every live face.

## Entry checks (all passed)

1. Six-repo gate: mosaic ×3 (W4), ai-rules, nautilus_trader (W4b,
   producer-face byte-identical proof) — legacy dbs already gone;
   code-reality itself is this arc's batch.
2. CR self test dependency: **zero** — `crg_fixture` /
   `graph_db_fixture` synthesize dbs at arbitrary tempdir paths; no
   test reads the repo's real `.code-review-graph/` (verified against
   every fixture consumer).
3. Old db present: `.code-review-graph/` 7.9M (graph.db Aug 25) —
   delete only after the owned db is rebuilt + spot-checked (W3 EP
   gate ordering: never delete first).

## Entry findings (folded into this arc)

- **F-A (bug, proven live)**: the `scip_refs --audit` wrapper
  (`cli.rs` `audit_mode`) still resolves `common::graph_db_path` —
  the OLD CRG path. The legacy-cutover EP listed `scip_refs --audit`
  among the cut-over modules, but only the `graph_audit` tool face
  was switched; the wrapper face was missed (drift regression). L4
  evidence: `scip_refs --audit --repo ai-rules` FAILs with
  "graph.db 不存在：…/.code-review-graph/graph.db" while the owned
  db exists — the audit face is broken on every legacy-clean repo.
  Fix: resolve `graph_db::db_path` (gate messages unchanged);
  `common::graph_db_path` then has zero callers → delete.
- **F-B (bug report from kickoff)**: chain_tour reported
  `[WARN] uncommitted changes under crates/` during NT W4b with the
  NT tree clean. Investigation: no clean-tree repro (clean tree +
  matching rev verified silent today on this machine); code-path
  analysis finds no clean-tree false-positive mechanism
  (`git status --porcelain` is empty iff the tree is clean incl.
  untracked). The freshness WARN shipped in `929420f` (18:50, the
  day's last CR commit) — at observation time the CR checkout
  carried uncommitted `crates/` edits from the concurrent CR
  session: the warning was accurate about the CR checkout but names
  no repo, reading as a false positive from the NT session whose
  scanned repo was clean. Defect class: unscoped diagnostic. Fix:
  self-identifying message (name the CR checkout path) in
  `freshness.rs` + the bridge's local copy. Behavior unchanged.

## Changes (CR side)

1. `crates/code-reality/src/graph_db.rs`: importer section deleted
   (`ImportLegacyReport` + `import_legacy()`), CLI face (SPEC
   `--dry-run` flag, usage lines, op list, dispatch block), the
   module-doc `import_legacy` sentence.
2. `crates/code-reality/tests/graph_db.rs`: import_legacy test group
   (7 tests) deleted; one attribution comment reworded.
3. `crates/code-reality/tests/crg_fixture.rs` deleted; dead
   `mod crg_fixture;` includes removed from s2_snapshot /
   s4b_hazard_hubrefs / s5_chain_tour;
   `graph_anchor_rejects_legacy_schema_loudly` rebuilt on a minimal
   wrong-schema db (the loud-rejection guard itself stays — `--graph`
   can still be pointed at museum CRG dbs); the consumer_db test's
   case 2 (W3 retirement pin) dropped — its subject is removed.
4. `crates/code-reality/src/common.rs`: `graph_db_path` deleted
   (zero callers after F-A; the frozen-parity oracle retired with
   R7).
5. `crates/code-reality/src/cli.rs`: F-A fix in `audit_mode`.
6. `crates/code-reality/src/freshness.rs` +
   `code-reality-lsp-bridge` local copy: F-B message scoping.
7. `crates/code-reality/src/hazard.rs`: drop the
   `!.code-review-graph/**` ignore entry; root `.gitignore`: drop
   the `.code-review-graph/` line, reword the sibling comment.
8. Docs: root `AGENTS.md` (Legacy-import capability row and S5
   metric row removed; CRG retirement readiness row updated),
   `README.md` (refresh-chain paragraph + References/dependency-chain
   tense), `crates/AGENTS.md` (ops list, Schema interop bullet,
   R7-retired parity-harness bullet rewording),
   `plugin/skills/code-reality/SKILL.md`; doc-residue extras:
   `tests/graph_db_fixture.rs` module docstring (dropped the
   mirrors-crg_fixture sentence), `tests/s6_mcp_server.rs` stale
   `.code-review-graph` comment.
9. `scripts/s5_coverage.py` deleted — purpose-complete (its
   denominator universe, `treesitter-legacy` rows, no longer exists
   after W5; keeping it invites vacuous runs). Root AGENTS.md row
   removed with it.
10. `ep-w3-import-legacy-retirement.md` archived to `_done/` (its
    full-removal gate is executed by this arc).

## Deliberate residuals (documented, not import_legacy wiring)

- `treesitter-legacy` provenance filter in `graph_audit` SQL + s4
  tests: provenance semantics (stray legacy rows must not pollute
  the audit denominator); test-pinned; no writer remains.
- GraphAnchor legacy-schema loud rejection (see 3).
- History: `_done/` EPs, `ai-analysis/reports/`,
  `.kanban/Done/`, `.review/` — archival record, untouched.

## Gates

- `cargo test --workspace` green.
- `rg import_legacy` zero over live faces (src / tests / scripts /
  README / AGENTS.md ×2 / plugin); hits allowed only in history
  (ai-analysis/, .kanban/, .review/).
- L4: `scip_refs --audit --repo ai-rules` passes against the owned
  db; `graph_db -h` shows no import_legacy; CR owned db rebuilt
  (rust chain: build-cache → build) + one graph_query spot-check,
  then `rm -rf .code-review-graph/`; clean-tree WARN silence
  re-verified; `import_legacy` as an op now fails loud (unknown op).
- Post-build dual-context review (fresh + primed).
- Commit gate: user's explicit go (per outward-action consent).

## Settlement record (2026-08-28)

- **L4 all pass**: `graph_db -h` shows `build|ensure_indexes` only;
  `graph_db import_legacy --repo …` → exit 2 unknown-op;
  `scip_refs --audit --repo ai-rules` → exit 0, 缺差 0 項 (the F-A
  fix verified against the pre-fix FAIL). CR self rebuild: the
  slot's index.scip was an 8.4K stub (owned db 21 nodes) —
  regenerated via `rust-analyzer scip` (5.5M, 25.8s) → stamp-meta →
  build-cache → graph_db build → **899 nodes / 2277 edges (CALLS 0 /
  REFERENCES 2277 — expected: the S3-F2 syntactic call split only
  marks .py sources; a Rust repo lands all-REFERENCES), external
  skipped 16,799**. Spot-checks (minimal_context, hub,
  ensure_indexes) exit 0 pre- and post-delete. `.code-review-graph/`
  (7.9M) deleted only after the rebuild + spot-check, per the W3
  gate ordering.
- **Residue sweep**: `rg import_legacy` over live faces = **0 hits**
  (history-only: ai-analysis/, .kanban/, .review/). Remaining
  `.code-review-graph` strings in live faces are history narration
  (AGENTS/README/crates-AGENTS W5 notes), upstream MIT attribution
  (README ×2), and the hub_refs design-rationale comment; the stale
  s6 test comment was reworded.
- **cargo fmt scope discipline**: `cargo fmt --all` swept 10
  untouched files (bridge crate internals, pyrefly lib, freshness
  tests — pure rewrapping); reverted via `git checkout --` to keep
  the W5 diff scoped.
- **Tests**: `cargo check --workspace` clean (one orphan `HashMap`
  import removed). Full `cargo test --workspace`:
  `lru_evict_preserves_overlay_edits` (code-reality-lsp-bridge,
  `--test bridge`) failed at its 60s convergence deadline in every
  full-workspace parallel run this session and passes isolated —
  full bridge target 15/15 in 2.03s, filtered single test 0.48s.
  Identical signature to the W3 EP record (load flake, untouched
  crate — W5's only bridge diff is the bin's stderr message, not
  compiled into the test target). **Pre-existing flake, not a W5
  regression; separate fix candidate** (deadline/retry tuning under
  parallel load). All other workspace suites green (31 test-result
  lines, only this one failure).
- Post-build dual-context review: see below (appended after
  adjudication).

## Post-build dual-context review (adjudicated 2026-08-28)

Fresh (code-reviewer) + primed (code-reviewer-primed) in parallel;
both independent re-verification passes (residue rg sweeps, db
three-tuple 899/2277/0 via read-only sqlite, W3 EP byte-identity
move, cargo check zero-warning) matched the settlement record.

- **Zero 🔴 / zero 🟡 on both sides.**
- ℹ️ adopted: EP Changes bookkeeping for the two doc-residue extras
  (primed); `crates/AGENTS.md` parity-harness bullet marked
  R7-retired history (primed); README References/dependency-chain
  present-tense dependency wording → design-lineage attribution,
  MIT credits kept (fresh + primed converged); root AGENTS.md
  "every repo's" → "every consumer repo's" with the museum-repo
  carve-out (self-correction: the fresh side's fd spot-check missed
  the gitignored museum copy that the W3 EP's user adjudication
  preserves).
- ℹ️ deferred (recorded, not this arc): the surviving self-owned
  fixture keeps the CRG-era API names (`CrgDbSpec` /
  `make_crg_db`) — misleading post-W5; mechanical rename across
  ~10 test files, separate small pass.
- Known flake re-confirmed outside this arc's scope:
  `lru_evict_preserves_overlay_edits` (bridge) fails only under
  full-workspace parallel load (60s convergence deadline), passes
  isolated (15/15 in 2.03s) — identical W3-EP signature, untouched
  crate. Separate fix candidate.
