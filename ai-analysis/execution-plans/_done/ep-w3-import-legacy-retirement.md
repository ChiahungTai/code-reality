# EP: W3 — import_legacy retirement from the refresh chain

One-liner: retire `import_legacy` from the consumer refresh chain —
refresh = pure producer graph; mosaic consumer acceptance on the
rebuilt pure-producer db.

## Entry checks (all passed)

1. W2 commit `ef58b61` on HEAD (B7b mint + W1 metric + B8 closure).
2. Gate: **95.42%** (frozen-corpus full-attribution version) — user
   adjudicated 2026-08-28 as W3 gate met (strict 95.7% was the
   prediction; Δ fully attributed: derived-base ~110 + B5 ~236 +
   B4/B6/B1b inherent classes, listed in
   `ai-analysis/reports/s5-ceiling-analysis.md` W2 settlement).
3. B8: listed-attribution closure (not a bug).

## Adjudication: retirement form

**CLI retained, marked retired** (WARN on every invocation,
`"retired": true` in `--json`; usage text flagged). Rationale:

- The recovery path until W4 (rerun `import_legacy`, minutes-idempotent)
  is the stated insurance in the kickoff — removing the subcommand now
  would break it.
- W4 (mosaic `.code-review-graph/` cleanup) is the removal point;
  until then the legacy db remains the one-shot import source.
- The refresh-chain semantics flip is carried by **removing the
  guidance that pushed users to import** (`consumer_db` SM-7 warn +
  missing-db warn tail), not by deleting the subcommand.

## Changes (CR side)

- `crates/code-reality/src/graph_db.rs`
  - `consumer_db`: un-imported-legacy guidance removed — a db with zero
    `treesitter-legacy` nodes is the normal pure-producer state;
    missing-db warn no longer mentions `import_legacy`.
  - `import_legacy` CLI face: `[WARN] import_legacy 已退役（W3）...`
    prefix on every text output; `retired: true` in json outputs
    (skipped and report faces).
  - Usage text: `import_legacy` flagged 退役/恢復用.
- `crates/code-reality/src/graph_engine.rs`: `open` missing-db error no
  longer appends the import_legacy clause (review F-01 — graph_query
  CLI + 12 MCP tools are this error's consumers; guidance removal must
  cover the engine entry face too).
- `crates/code-reality/tests/graph_db.rs`:
  `import_legacy_cli_face_carries_retirement_banner` pins the `[WARN]
  已退役` prefix and `"retired": true` marker (review F-02).
- `crates/code-reality/tests/s5_chain_tour.rs`: consumer_db case 2
  flipped — asserts NO import_legacy guidance; test renamed to
  `..._no_import_guidance`.

Post-build dual-context review (fresh + primed): all directional
findings applied; record in `.review/main.md`.

## Full-removal gate (subcommand deletion)

Per-repo safety gate (same everywhere): rebuild owned db pure-producer
(`graph_db build`, no import_legacy) → one graph_query spot-check →
only then `rm -rf .code-review-graph/`. Never delete first — the
legacy db is the recovery source until the owned db is verified.

| Batch | Repo(s) | State (2026-08-28) | Action |
|---|---|---|---|
| W4 (kickoff-scheduled) | mosaic_alpha | pure db verified PASS today | delete legacy dir directly |
| W4 extension (fold into relay) | mosaic_alpha_offline_backtesting, _trading_lab (worktrees) | offline has stale owned db; trading_lab none | rebuild → spot-check → delete |
| 2nd | ai-rules, code-reality | owned dbs present | rebuild → delete |
| 3rd (heaviest) | nautilus_trader | owned db present; legacy db 1.4G | rust-analyzer SCIP regen → stamp → cache → build → delete |
| user-adjudicated | code-review-graph (retired CRG repo) | no owned db | keep as-is (museum; user 2026-08-28) — excluded from the deletion gate |

Once the last repo is cleaned: remove the subcommand +
`import_legacy()` + its test group + the banner test + `crg_fixture.rs`;
root AGENTS.md capability 🟢 → ❌ removed.
- Docs flipped: root `AGENTS.md` (capabilities row → 🟢 retired
  recovery face; CRG retirement readiness row), `README.md`,
  `crates/AGENTS.md`, `plugin/skills/code-reality/SKILL.md`.

## Mosaic consumer acceptance

Baseline (mixed-form db, legacy nodes = 12,410 / nodes 26,927 / edges
163,628; backed up to `.agent-tmp/w3/graph.db.before-mixed`):

| Query | Baseline |
|---|---|
| `graph_query hub` | captured (before.hub.txt) |
| `graph_query communities` | captured (before.communities.txt) |
| `graph_query impact_radius` (test_trajectory_viewer.py) | total_impacted 7905 |
| `chain_tour` margin-accounting-chain | 6 場景 / 50 幀 / 50 步; 重錨 {moved 6, not-in-graph 2, same 42} |

Rebuild (no import_legacy): `pyrefly-index` → `--stamp-meta` →
`--build-cache` → `graph_db build` → same query set.

### After-rebuild comparison

Rebuilt pure-producer db: **14,517 nodes / 27,410 edges**
(CALLS 25,875 / REFERENCES 1,535), `treesitter-legacy` = **0**. The
producer face is byte-count identical before/after (scip nodes 14,517,
scip edges 27,410) — the entire delta is the retired legacy universe:
edges TESTED_BY 33,117 + CALLS 74,651 + CONTAINS 16,583 +
IMPORTS_FROM 9,813 + REFERENCES 1,681 + INHERITS 373; nodes 12,410
(qname-minted legacy symbols).

| Query | Before (mixed) | After (pure) | Attribution |
|---|---|---|---|
| `graph_query hub` | top = create_test_df (deg 342) | top = run_parity_test (deg 118) | legacy TESTED_BY/CALLS inflated in-degree; ranking re-normalizes |
| `graph_query communities` | sizes/cohesion incl. legacy nodes | smaller communities, higher cohesion | legacy-only members (class/dataclass qname nodes) gone; "python"→"Python" casing from node provenance source |
| `graph_query impact_radius` (test_trajectory_viewer.py) | total_impacted 7,905 | total_impacted 275 | TESTED_BY edges carried test→source impact; call-graph reachability remains |
| `chain_tour` margin-accounting-chain | 6 場景/50 幀/50 步; {same 42, moved 6, not-in-graph 2} | **identical** | reanchor anchors are producer symbols |

All queries exit 0, deterministic. Live-face checks with the retired
binary: `import_legacy --dry-run` prints the `[WARN] import_legacy 已退役（W3）`
banner (dry-run report merged=5369 synthesized=12410 mapped=63825/136218
— recovery path verified functional); `chain_tour` on the pure db emits
no import_legacy guidance despite `.code-review-graph/` still present.

**Acceptance: PASS** — pure-producer graph serves the consumer face;
every delta attributes to the deliberately retired legacy universe.

Tests: full `cargo test --workspace` green except
`code-reality-lsp-bridge::bridge::lru_evict_preserves_overlay_edits`,
which failed only under full-workspace parallel load (61s deadline) and
passes in 0.48s isolated — load flake, untouched crate, not a W3
regression.

Note: the mosaic rebuild used the first-reinstall binary (W2 producer +
W3-unmodified build path); W3 diff touches only `consumer_db` guidance
strings and the `import_legacy` output face, not build semantics.

## Out of scope (per kickoff)

- ai-rules doc flips (skills import_legacy mention, crg-query fallback
  line, smell-detector refresh line) — ai-rules session with the回執.
- derived-base residual bucket (~110 pairs) — improvement item.
- W4 mosaic `.code-review-graph/` cleanup — separate relay.
