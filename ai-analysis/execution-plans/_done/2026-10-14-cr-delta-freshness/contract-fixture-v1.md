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
