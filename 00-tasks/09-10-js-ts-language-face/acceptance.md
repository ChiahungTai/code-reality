# JS/TS language-face arc — implementation acceptance record

Captured 2026-09-10 by the implementation session (deep-work). This is
the S7 child-EP acceptance manifest plus the M1/M2 evidence summary.

## Dogfood corpus baseline (S7 segment 1)

```text
delegate_bridge_head   = 6312fa249f52f26b6d798e9c362ab93e4ffd2db5
                       (disposable copy at /tmp/cr-dogfood; the real repo
                        was never written — its own arc owns the profile)
unfiltered_mjs_docs    = 50 (14 under the generated dist/ mirror)
governed_mjs_docs      = 36   ← acceptance denominator (planning was 42/28;
                                the repo moved — both recorded per EP)
root tsconfig.json     = ABSENT at implementation time (planning saw `{}`)
                         → the derived-config path is the ONLY production
                         path for this corpus (SM-3 proven on real data)
frozen_symbols         = mapExitCode (3 cross-file callers:
                         scripts/bridge/core/task.mjs:458 + 2 more),
                         runTask, tsGreet-family fixtures for type lanes
profile (disposable)   = exclude = ["dist/"]  — repo-owned policy, never
                         a tool-layer special case
acceptance toolchain   = scip-typescript 0.4.0, typescript-language-server
                         6.0.0, TypeScript 5.9.2, Node v24.20.0 (locally
                         installed prefix + CODE_REALITY_NODE_BIN_DIR —
                         observed-compatible, not an upstream guarantee)
```

## M1 structural acceptance (real producer, disposable copy)

| Check | Result |
|---|---|
| Auto-detection → typescript-face, derived config | PASS — "derived config、36 governed 文檔" |
| Governed doc set exactness (derived mode enforces exact convergence) | PASS — 36/36, dist/ mirror absent |
| Language truth | PASS — 256/256 nodes JavaScript, zero Rust mislabels |
| `enclosing_range` coverage (S3 bar ≥99%) | PASS — 256/256 = 100% |
| Syntactic CALLS (column-grained identity) | PASS — 340 CALLS / 4 REFERENCES on real syntax |
| mapExitCode `--callers` | PASS — 3 real cross-file callers w/ sites |
| `graph_query review_context` (task.mjs) | PASS — 10 changed / 44 impacted nodes in 10 files |
| `graph_query impact_radius` (status.mjs) | PASS — real JS nodes (mapExitCode et al.) |
| No repo mutation (tsconfig absent stays absent; ephemeral derived config cleaned) | PASS |
| Freshness: content edit / add / delete (backdated mtime — fingerprint-only) / rename / excluded-edit / profile drop+restore | ALL PASS — each mutation heals-and-converges; excluded edit does NOT heal |
| graph_audit on JS-only graph | PASS — exit 2 capability-unsupported (never clean) |
| scip_refs --audit wrapper | PASS — same boundary, exit 2 |
| hub_refs --hazard JS target | PASS — hazard_level=unsupported-js-ts, hazard_supported=false, static aggregation intact |

## M2 type-face acceptance (real TLS through the bridge)

| Check | Result |
|---|---|
| lsp_status three families | PASS (py/rs/ts) |
| Hover all six extensions (.js/.jsx/.mjs/.cjs/.ts/.tsx) | PASS — real signatures |
| Real delegate-bridge .mjs hover (mapExitCode) | PASS |
| Type error surfaces (TS2322) → correction converges to zero | PASS — no deadline WARN after the diag_versions fix |
| Missing JS/TS backend | PASS — per-family `unavailable`, bridge alive |

## Review waves absorbed (post-implementation)

1. In-harness mid-build reviewer: 1×🟡 (existing-config partial corpus) + 7×🟢 — adopted (F1 escalated into codex P0-2; F2 stamp WARN superseded by the preserve-keys fix; F6/F7/F8 landed).
2. Codex bridge review (job-mtvge4wn-7yv8vu): 4×P0 + 4×P1 — P0-1 per-attempt staging nonce ✅; P0-2 existing-config full-coverage-or-fallback ✅; P0-3 subsumed by P0-2 ✅; P0-4 churn-marker arming + guard reorder ✅; P1-5 CALLS column identity ✅; P1-6/7/8 regression tests ✅.
3. User-directed codex architecture review: blocker 1 (policy-empty convergence — index+graph removed, next check Fresh) ✅; blocker 2 (stamp preserves prior identity keys — drift stays visible) ✅; LSP test isolation (env race + compile-once) ✅; test-support consolidation (tests/support/ + production-logic-backed deriving fixture, FNV/python mirrors deleted) ✅; pub-visibility reverts ✅.
4. muse bridge final review (job-mtvk5t7y-0nej1i, 1×P0 + 2×P1 + 8×P2): P0-1 auto-index new-language heal (selection stamp `auto`/`explicit` + union-with-detected eval scope) ✅; P1-2 qualified `::` JS-target hazard bypass (three-prong language resolution incl. embedded defining-file extension) ✅; P1-3 torn index/graph pair (graph.db older than slot forces heal) ✅; P2-4 lsp_harvest schema col ✅; P2-5 empty-branch fndefs cleanup + graph_rebuilt=false ✅; P2-6 zero-node graph audit = Unsupported ✅; P2-7 project guard recursive ✅; P2-9 SKILL.md wording ✅; P2-10 corpus normalization at collection ✅; P2-8 rejected (coverage denominators already pinned by `s3_coverage_denominators_recorded` — hand-built Index with an enc-less variant, not the deriving fixture); P2-11 rejected (pre-existing, reviewer demanded no change).

Final verification state after all four waves: full workspace `cargo test` exit 0 (504/0), wrapper gate 14/14, M1 re-verified ALL PASS (340 CALLS preserved through the SiteEdge column refactor), M2 ALL PASS.

Deferred (explicitly out of this arc, per review): production module decomposition of engine.rs/build.rs (separate refactor); migration of the OLD test files (build.rs/refresh.rs) onto tests/support/.
