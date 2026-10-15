# Output Contract v1 — CR-10 delta-freshness frozen fixture

> S5 hand-off artifact (EP `2026-10-14-cr-delta-freshness` §S5 ①②).
> Every output block below is **captured verbatim** from the landed
> implementation (`code-reality 0.13.0+85c520e`, built at the S4 landing
> commit) — machine-verified, not hand-written. Consumers (bridge graph
> client DB-120/DB-101, ai-guide AIR-330) MUST parse against these
> byte shapes; this document is the comparison-table source of truth.
> Vocabulary is frozen at EP D6/D7/D15 — renaming anything here is an
> EP reopening, not a compatibility note.

## 1. Frozen vocabulary

| axis | values | notes |
|---|---|---|
| `tier` | `exact` \| `possibly-stale` \| `no-coverage` \| `indeterminate` | D6. Never inferred from absence — `no [CR-…‑] line = unannotated`. |
| `negative_verdict` | `null` \| `unavailable` | D7. `unavailable` = stale + gated-empty ⇒ typed refusal (authority-less emptiness). MCP folded-refusal match key: **`stale-negative-unavailable`** (minted verbatim in the folded content; consumers match this literal). |
| `exhaustive` | `true` \| `false` | `true` only on fresh authoritative answers (incl. fresh+empty authoritative zero). Partial execution (R4 `Degraded`) forces `false`. |
| `delta_complete` / `content_verification` | bool / `stat-cache` \| `full-rehash` \| `null` | D2/D4. `null` pair rides `delta_unavailable`. |
| `delta_unavailable` reasons | `manifest-binding-audit-failed` \| `legacy-no-manifest` \| `manifest-missing` \| … (D15 enum) | reason always present, `null` when N/A. |
| graph `graph_snapshot_id` | 64-hex \| `null` | D10/AM-3: `sha256("cr-graph-snapshot-v1\0{input_index_sha256}\0{builder_revision}\0{schema_version}\0")` — trailing NUL in place; cross-tool recomputation MUST use this exact byte format. `builder_revision` SQL NULL ⇒ empty string inside the id. `null` in an annotation = not a graph-served answer (scip-face/symbols bypass) — N/A, not "missing". |
| lsp-harvest face | AM-3 ② residual | golden-only face: `input_index_sha256` binds the placeholder bytes; consumers must never derive fresh/delta claims from an lsp-face-served graph. |

## 2. Verbatim output examples

### 2.1 scip_refs — fresh, authoritative exact (multi-face producer string; lab: single-face)

```
[SRC] scip index @ 4059f3c（2026-10-14） · repo HEAD @ 4059f3c
[CR_VERDICT] v=1 tier=exact exhaustive=true negative_verdict=null producer_version="pyrefly-index 0.13.0+85c520e-dirty; rust-analyzer rust-analyzer 1.96.0 (ac68faa2 2026-05-25) [roots: .]" delta_changed=0 delta_complete=true content_verification=stat-cache delta_unavailable=null delta_paths_total=0
[OK] graph_db/build_from_cache_at().
  DEF  crates/code-reality/src/graph_db.rs:688
```

Line 1 `[SRC]` unchanged (pre-existing contract). Line 2 `[CR_VERDICT]` is
the D8 annotation — **always the second line** across all three scip
output formats, including the two zero-row variants.

### 2.2 scip_refs — stale, possibly-stale with delta summary

```
[SRC] scip index @ 499ea47（2026-10-14） · repo HEAD @ 499ea47
[CR_VERDICT] v=1 tier=possibly-stale exhaustive=false negative_verdict=null producer_version="pyrefly-index 0.13.0+85c520e-dirty" delta_changed=1 delta_complete=true content_verification=full-rehash delta_unavailable=null delta_paths=src/lab.py delta_paths_total=1
[OK] `src.lab`/greet().
```

`delta_paths` (comma-joined) is **display-only, non-normative** — the
authoritative delta is the `freshness --json` face (§2.5). The complete
union size rides `delta_paths_total`; `delta_paths_truncated=true` rides
exactly when the 50-entry cap bit (D13 count-pairing).

### 2.3 scip_refs — D7 hard negative gate (stale + inbound-empty)

```
[SRC] scip index @ 499ea47（2026-10-14） · repo HEAD @ 499ea47
[CR_VERDICT] v=1 tier=possibly-stale exhaustive=false negative_verdict=unavailable producer_version="pyrefly-index 0.13.0+85c520e-dirty" delta_changed=1 delta_complete=true content_verification=stat-cache delta_unavailable=null delta_paths=src/lab.py delta_paths_total=1
[REFUSED] call_greet：0 callers（0 sites）
  item-level：0 處（未歸屬 fn——use/const/屬性層）
```

**exit 1.** The gate is bound to `needs_rebuild()` (staleness truth) and
fires regardless of delta availability; when the delta did not answer
the tier degrades to `indeterminate` while the refusal still fires.
MCP face folds this into `is_error=true` with the literal
`stale-negative-unavailable` in the content (§3).

### 2.4 scip_refs — no-coverage (symbol absent from the graph; unchanged exit-1 family face)

```
[SRC] scip index @ 499ea47（2026-10-14） · repo HEAD @ 499ea47
[CR_VERDICT] v=1 tier=no-coverage exhaustive=false negative_verdict=null producer_version="pyrefly-index 0.13.0+85c520e-dirty" delta_changed=1 delta_complete=true content_verification=stat-cache delta_unavailable=null delta_paths=src/lab.py delta_paths_total=1
[WARN] 查無 DEF：nonexistent_sym
```

### 2.5 freshness JSON (CLI `freshness --json` == MCP `freshness` tool payload; authoritative delta)

```json
{
 "repo": "…/lab",
 "slot": "…/lab/.code-reality/scip/index.scip",
 "fresh": false,
 "stale_reasons": ["content-drift"],
 "head_drift": false,
 "faces": ["python"],
 "indexed_source_identity": "16737ebe…",
 "current_source_identity": "1096e409…",
 "identity_algo": "sha256-v2",
 "serves": "committed-baseline",
 "base_commit": "499ea47…",
 "producer_version": "pyrefly-index 0.13.0+85c520e-dirty",
 "delta": {
  "changed": {"count": 1, "paths": ["src/lab.py"]},
  "added": {"count": 0, "paths": []},
  "removed": {"count": 0, "paths": []}
 }
}
```

`count`+`paths` pairing is the D13 normative shape; `serves` ∈
`current-tree | committed-baseline | legacy-signals`. MCP `freshness`
tool: same JSON, **`is_error=false` for both fresh and stale** (a stale
verdict is a legitimate typed answer, exit-0-class); a broken slot is
the error face. The tool may write the identity cache
(`CODE_REALITY_IDENTITY_CACHE=off` reverts to read-only) — disclosed in
the manifest `mutating_notes`, not a `write_faces` member.

### 2.6 graph_query — `[CR-META]` header (all 12 ops; line 1 `[OK]`, line 2 JSON)

```
[OK] graph_query search
[CR-META] {"v":1,"tier":"exact","producer_version":"85c520e","graph_snapshot_id":"50453561…","exhaustive":true,"negative_verdict":null,"delta_summary":{"changed":0,"added":0,"removed":0,"delta_complete":true,"content_verification":"stat-cache","delta_unavailable":null}}
```

`graph_snapshot_id`/`producer_version` here describe the **graph.db
generation** (D10: `producer_version` = the graph build-time
`builder_revision`, never a query-time binary rev). Gated ops
(`search` / `impact_radius` / `affected_flows`): stale + empty ⇒ header
`negative_verdict:"unavailable"` + result fields `null` (never `[]`).
`symbols` bypass and `get_community` carry the same header (symbols:
graph fields read `null` — scip-cache-served, scip binding governs).
`gq_fail` paths print **no header** — header absence never means fresh.

### 2.7 MCP `get_community` (face changed vs pre-S4 — consumers take note)

```
isError: false
[OK] graph_query get_community
[CR-META] {"v":1,"tier":"exact","producer_version":"85c520e","graph_snapshot_id":"50453561…","exhaustive":true,"negative_verdict":null,"delta_summary":{…}}
{
 "name": "data-send",
 "size": 15, …
}
```

**Impact for line-oriented consumers: `graph_query` MCP results gained a
second line** — skip/parse `[CR-META]` before JSON (v2 manifests
`get_community`/`list_communities` et al. all carry it).

### 2.8 MCP manifest identity (supply-side anchor)

```
tools: 22 (was 21; new read-only `freshness` tool)
tools_digest: sha256-f1d3a1ddf27af… (algorithm: sorted-keys-compact-json+sha256, scope: name+input_schema)
crate_version: 0.13.0 · build_rev: 85c520e
write_faces: ["build","snapshot","delta_tour","project"]  (freshness NOT a write face)
```

## 3. Value-escaping rule (frozen; R7)

`[CR_VERDICT]` is space-separated `key=value` tokens. Any value
containing whitespace, a comma, or a double quote is wrapped in double
quotes with inner `"` escaped as `\"` (e.g.
`producer_version="pyrefly-index 0.13.0+85c520e-dirty"` — note the
space inside). Parsers split on spaces outside quotes and keep
quotes/escapes verbatim in the value. Enumerated values (tier,
negative_verdict, delta_complete, content_verification,
delta_unavailable) never trigger quoting.

## 4. Annotation coverage inventory

- **Annotated**: scip_refs (`refs`/`callers`/`closure` — three formats ×
  zero-row variants), graph_query 12 ops (+ symbols bypass + get_community
  MCP face), freshness JSON (CLI + MCP tool), MCP freshness typed verdict.
- **Exempt in period 1 (unverified, no header — absence ≠ fresh)**:
  hub_refs/hazard, audit, chain_tour, delta_tour, snapshot, boundary.
- Fail-closed truncation: a truncated `[CR-…]` header line ⇒
  `TRUNCATED_UNVERIFIED` error, never a partial success.

## 5. Cross-repo consumer comparison table

| consumer face | pre-S4 contract | post-S4 change | action for consumers |
|---|---|---|---|
| bridge graph client (graph_query) | line 1 = `[OK]`, then JSON | **line 2 = `[CR-META]` JSON header inserted** | skip/parse line 2 before JSON; read `graph_snapshot_id` for serving-generation pinning (D11: metadata came from the same connection as the results) |
| bridge DB-120 observation face (scip family) | line 1 = `[SRC]`, then content | **line 2 = `[CR_VERDICT]` key=value line inserted** | parse `tier`/`exhaustive`/`negative_verdict`; on `exit 1` + `negative_verdict=unavailable` treat emptiness as refusal, not absence |
| MCP `get_community`/`list_communities` | content-only | header inserted (§2.7) | same line-2 handling |
| MCP tools surface | 21 tools | 22 tools (+`freshness`), new `tools_digest` | refresh pinned digest; `freshness` is read-only-typed (§2.5) |
| AIR-330 vocabulary | draft wording | frozen values §1 | ai-guide final wording must match §1 or reopen the EP (K2 probe — this document is the probe payload) |

## 6. Provenance of this fixture

Captured 2026-10-14 by the marshal session from
`code-reality 0.13.0+85c520e` (the S4 landing commit) — lab corpus
(single-module Python, pyrefly face) for stale/refusal faces and the
code-reality repo itself (multi-face) for the fresh dogfood faces.
Re-verification command shape: rebuild any repo with
`code-reality build --repo <repo>` and diff the faces against §2.

---

# Output Contract v1.1 — CR-13 per-face stale-base supplement (2026-10-15)

> Supplemental section appended by CR-13 (R11 scip stale reason-set
> per-face split, plan A). **§1–§6 above are immutable** — none of their
> bytes were revised. This section is **NOT purely additive**: it
> revises one semantic contract (the v1 §2.3 gate binding) and declares
> the revision honestly below. The frozen §1 vocabulary does NOT
> change — no new word is minted; `tier` / `negative_verdict` /
> `stale_reasons` keep their v1 values.
>
> Captured from `code-reality 0.14.0+6044ca8-dirty`, 2026-10-15;
> re-verification: rebuild a lab of the §9 shape and diff the live
> faces against §9.

## 7. Honest semantic-revision disclosure (NOT purely additive)

v1 §2.3 states: "The gate is bound to `needs_rebuild()` (staleness
truth)". **CR-13 revision**: the scip-face gate is now bound to
`needs_source_rebuild()` — the scip query faces (refs/defs/callers/
closure) never consume graph.db (cli refs/defs/callers read the SCIP
slot; only `--audit` reads graph.db), so a torn plane whose ONLY fatal
signal is `graph_lags` (canonical graph.db missing or older than the
slot; source identity fresh) is no longer scip-face stale. v1 §2.3's
text stands above as the historical v1 record; consumers parsing
scip-face `[CR_VERDICT]` lines and refusal envelopes MUST use the
per-face table (§8) instead of the single v1 predicate.

Consumer-visible deltas, spelled out:

| consumer-visible fact | v1 | v1.1 (CR-13) |
|---|---|---|
| scip-face refusal trigger set | any `needs_rebuild()` staleness, incl. a graph-only torn plane | **narrowed** to source-side staleness only |
| `negative_verdict=unavailable` literal | frozen | **unchanged** — same literal, narrower trigger set |
| refusal envelope (exit 1; MCP fold `stale-negative-unavailable`) | frozen | **unchanged error handling** — fewer inputs reach it, nothing about the envelope moved |
| graph-only torn-plane scip query (inbound-empty) | false refusal (availability loss, not correctness) | source-exact answer (`tier=exact`) while `freshness --json` still answers repo-stale (§9) |
| public `stale_reasons` vocabulary (§2.5) | frozen | **unchanged** — diagnostic reasons keep being reported verbatim; only the fatal predicate became per-face |
| graph-served faces, freshness repo verdict, heal trigger | — | **unchanged** (`needs_rebuild()` still includes the unconditional torn-plane guard) |

## 8. Per-face stale-base table (normative from v1.1)

| face | served artifact | stale base (fatal predicate) | graph-only torn plane (`graph_lags=true`, identity fresh) |
|---|---|---|---|
| scip_refs `refs`/`defs`/`callers`/`closure` (`[CR_VERDICT]` + D7 gate) | scip slot | `needs_source_rebuild()` — identity-algo-upgrade ∨ `identity_drift` ∨ (legacy mode: mtime-newer ∨ doc-set ∨ policy) | **not stale** — answers source-exact (`tier=exact`), never refuses |
| graph_query graph-served ops (12 ops + `get_community`, `[CR-META]`) | graph.db | `needs_rebuild()` = `needs_source_rebuild()` ∨ `graph_lags` (unchanged) | stale — gated-op negative verdict may fire on gated-empty |
| graph_query `symbols` bypass (`graph_snapshot_id: null`) | scip slot | `needs_source_rebuild()` (CR-13 alignment — same base as scip_refs; previously the full `needs_rebuild()`) | **not stale** |
| `freshness --json` repo-level verdict (CLI + MCP tool) | whole data plane | `needs_rebuild()` (unchanged from v1) | stale: `stale_reasons:["torn-plane"]`, `serves:"committed-baseline"`, exit 1 |

## 9. Graph-only torn-plane verbatim outputs (machine-captured)

Lab: single-module Python repo (`mod_a.py`, fixture producer
`pyrefly-index 0.4.0-fixture`), built with
`code-reality build --repo <repo>`; then the slot bytes rewritten in
place (content unchanged — stamped identity and the binding audit stay
valid) with the meta sidecar mtime kept in lockstep, leaving graph.db
older than the slot — the "slot published, graph build failed" torn
plane (muse P1-3). Scip face shown on the AUTOHEAL=off face; the
explicit `--index` face behaves identically (both test-verified).

### 9.1 scip_refs over the graph-only torn plane — source-exact, no refusal

```
[SRC] scip index @ 9b7f572（2026-10-15） · repo HEAD @ 9b7f572
[CR_VERDICT] v=1 tier=exact exhaustive=true negative_verdict=null producer_version="pyrefly-index 0.4.0-fixture" delta_changed=0 delta_complete=true content_verification=stat-cache delta_unavailable=null delta_paths_total=0
[OK] mod_afn1：0 callers（0 sites）
  item-level：0 處（未歸屬 fn——use/const/屬性層）
```

**exit 0.** Contrast with v1 §2.3: on this same state the v1 face
answered `tier=possibly-stale exhaustive=false
negative_verdict=unavailable` + `[REFUSED]` + exit 1. A source-side
drift on this lab still produces exactly the v1 §2.3 refusal face
(unchanged — test-verified contrast arm).

### 9.2 `freshness --json` on the same lab — repo-level verdict unchanged

```json
{
 "repo": "…/lab/repo",
 "slot": "…/lab/repo/.code-reality/scip/index.scip",
 "fresh": false,
 "stale_reasons": ["torn-plane"],
 "head_drift": false,
 "faces": ["python"],
 "indexed_source_identity": "f9c07b7bec0d4531fdeffa3071174ff862d3ab53bfe6bb65613b7620ef54cf4f",
 "current_source_identity": "f9c07b7bec0d4531fdeffa3071174ff862d3ab53bfe6bb65613b7620ef54cf4f",
 "identity_algo": "sha256-v2",
 "serves": "committed-baseline",
 "base_commit": "9b7f572c21db99bba30be334fe5ee623c66280fe",
 "producer_version": "pyrefly-index 0.4.0-fixture",
 "delta": {
  "changed": {"count": 0, "paths": []},
  "added": {"count": 0, "paths": []},
  "removed": {"count": 0, "paths": []},
  "delta_complete": true,
  "content_verification": "stat-cache",
  "out_of_scope_count": 0,
  "policy_drift_flags": []
 },
 "delta_identity": "f9c07b7bec0d4531fdeffa3071174ff862d3ab53bfe6bb65613b7620ef54cf4f",
 "delta_unavailable_reason": null
}
```

**exit 1.** The repo-level verdict still reports the torn plane — the
split is per-face, not a weakening of the heal obligation: the
query-time auto-heal and `refresh` still rebuild a torn plane
(`needs_rebuild()` unchanged), so a served graph-only-lag scip answer
implies the heal path was escaped (`AUTOHEAL=off` / explicit
`--index`) or failed. (`repo`/`slot` paths elided to `…/lab/…` —
machine temp dir; every other byte verbatim.)
