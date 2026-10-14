# EP: CR delta-freshness — stale-graph delta disclosure + per-query freshness annotation

> **ep_type**: implementation
> status: accepted（EP Review Cycle 雙腿收斂——review ledger 21 findings 全數裁決，見文末區段）
> baseline: `1b0ba1e62b51efd8a249564e7431f9d113e0a80e`
> date: 2026-10-14
> advisors: muse job-mv7hxe3f-vmzv26 + codex job-mv7hxeai-dcnqio (verdicts: `.agent-tmp/cr-delta-freshness-{muse,codex}-verdict.md`)
> review legs: fresh-eyes（F1-F12）＋ intent-primed（F1-F10），judge 裁決全錄文末
> upstream anchors: bridge codex consult job-mv7ae77q (Amend ruling), bridge proposal `delegate-bridge/.agent-tmp/cr-delta-freshness-proposal.md`, ai-guide AIR-330 (in progress), dossier `.agent-tmp/cr-delta-freshness-dossier.md`

## 實作總覽

Today the freshness face is all-or-nothing: any content drift flips
`serves=committed-baseline` and every consumer (bridge fresh-only supply
gate, cr-query discipline) downgrades wholesale — a docs-only commit skips
the entire graph server even though every structural query would answer
`exact` (real waste: receipt job-mv78cymo-qb4r7n). This EP makes the stale
SET computable and per-query freshness honest:

- **(a)** at build time, persist the governed-corpus per-file manifest
  (base manifest, source files AND rust-input rows) beside the index
  slot, bound to the index bytes by recomposition audit.
- **(b)** the freshness face emits a delta set (changed/added/removed vs
  the base manifest) with explicit completeness semantics.
- **(c)** query faces annotate per-result freshness tier
  (`exact|possibly-stale|no-coverage|indeterminate`) + build-time
  producer version, enforce a **hard negative gate** (a stale inbound
  query can never present an empty result as authoritative absence), and
  expose a read-only `freshness` tool on the MCP server face.

**Scale ruling: full EP** (both advisors agree) — cross-repo public
contract (bridge DB-101 enable gate + ai-guide AIR-330 vocabulary freeze
bind on it), new state shape (manifest sidecar lifecycle + binding), and
a query-evidence boundary decision.

**Cross-repo context (sovereignty fence)**: bridge DB-120/DB-101 and
ai-guide AIR-330 consume this contract but are NOT in scope. This EP
ships the CR-side producer + faces + contract fixtures; external
enablement is gated on their side (S5 hand-off, no code).

## 段落 S1：契約凍結（＝本 EP acceptance 本身，無獨立碼段）

本 EP 的「已裁決設計」（D1-D17）＋TC 表＋詞彙表（D6/D7/D15）在 EP
acceptance 時凍結；此後任何詞彙/語義變更＝本 EP amendment 重開。K2
（跨 repo 詞彙收斂）的 probe 掛 S5（契約 fixture 連同對帳信送 ai-guide）。

## 已裁決設計（勿重辯——advisor convergence＋review 修正，provenance per item）

- **D1 delta truth source** = stamped per-file **manifest diff**; git diff
  is display-only (`base_commit` reference field). Rationale: dirty-at-build
  content, untracked-at-build files, and head-sync `head` drift make any
  commit-anchored diff unsound (muse Q1 killer case; codex Q1).〔both〕
- **D2 manifest storage & binding** = fixed-name sidecar
  `.code-reality/scip/index.scip.manifest.json` (tmp+rename atomic write,
  mirroring `identity-cache.json` :269 precedent), NOT in `meta.json` —
  D4 precedent bars per-file tables from the human-provenance meta, and
  `load_meta` sits on query hot paths (muse Q2). Content:
  `{schema_version, repo, selection, faces, identity_algo, index_sha256,
  source_identity, topology_hash, entries:[{face, rel, size, hash}]}` —
  entries cover governed source files **AND rust-input rows**
  (identity.rs:347-360 same pass already has them — per-file, near-zero
  cost; review F6/intent-F1), no mtime (identity body is mtime-free).
  **Binding audit** (review F1): meta gains `base_manifest_digest`
  pointer (into `preserve_prior_keys` :1390-1398 — pointer ≠ digest-named
  retention, fixed-name ruling unchanged); load-time validation =
  ① meta binding fields equal current meta ② **entries recompose to
  stamped `source_identity`** (catches single-entry tampering — TC-7
  attack 1) ③ digest matches meta pointer. Any mismatch/absence →
  legacy degradation (D15), never a substitute answer.
- **D3 manifest production** = refactor the validated production hash
  pass (`compute_identity_scoped` identity.rs:302-382 /
  `ProductionIdentity::capture` engine.rs:1245-1297) to emit the per-file
  table and the aggregate from the SAME authenticated pass (Full policy,
  stamp path only). The advisors refuted "free byproduct": per-file
  hashes are discarded today; this refactor IS the (a) work. Never
  assemble a stamped manifest from query-side caches.〔both〕
- **D4 delta_complete semantics** = enumeration axis only: walk success +
  manifest binding valid (D2 三條) + every governed file hashed without
  Err. Separate `content_verification` field ∈ `full-rehash | stat-cache`
  discloses the hash trust level (the identity cache stat-gate residual —
  same size + same mtime-nanos forgery — is inherited, never denied).
  Delta scope = **stamped faces ∪ rust-input rows**, not eval_faces;
  detected-only faces bucket as `out_of_scope`; **profile/topology AND
  JS/TS selection/config changes surface as `policy_drift_flags`** (not
  file counts; review F8). Corrupt-cache discard → delta answer must
  equal the no-cache answer (TC-19).
- **D5 new/untracked files** = dual-label: they enter delta `added`
  (making global tier possibly-stale, inbound exhaustive=false) AND a
  direct query against them answers `no-coverage` (absent from the
  indexed generation). Not mutually exclusive.〔both〕
- **D6 tier vocabulary (frozen here; AIR-330 must align)**:
  `exact | possibly-stale | no-coverage | indeterminate`.
  `exact` requires: pinned serving artifact + comparable source identity
  matched + producer/coverage known + the operation's semantics fully
  supported (Rust-only audit precedence carries over). `no-coverage` is
  orthogonal to stale. `indeterminate` = provenance/manifest/generation
  unverifiable.〔codex Q3 + muse Q3〕
- **D7 negative gate (hard, at query boundary)** — **per-face rendering**
  (review fresh-F1; scip query faces are text-only, there is no JSON
  carrier to null):
  - gated set: scip 家族 `callers`/`closure`（empty today = exit-0 成功形
    `0 callers`/empty frontier — flipping to refusal is a visible contract
    change, goldens updated, review fresh-F2）＋ graph_query 內 empty-result
    即完整性宣稱的 op：`search`/`impact_radius`/`affected_flows`
    （intent-F3）；`defs/refs` 空＝not-in-graph 語義（no-coverage 軸）不
    入 negative gate。
  - stale + gated op + empty ⇒ typed refusal: text face＝`[CR_VERDICT]`
    行帶 `negative_verdict=unavailable`＋**exit 1**（值釘死：沿用 1，與
    no-DEF 的 exit-1 以 [CR_VERDICT] 內容區分——review intent-F5；
    crates/AGENTS.md exit 表於 S4 收尾同步）；graph_query＝[CR-META] 帶
    `negative_verdict:unavailable` 且結果欄位 null（無可讀 `[]`）；
    MCP＝`is_error=true`、type 字串 `stale-negative-unavailable` 明載於
    content（map_tool_output :255-274 折疊非零 exit 為 McpError——type
    騎文字面，不新開 transport 面）。
  - stale + gated op + non-empty ⇒ success + `exhaustive:false` +
    baseline semantics（"known within indexed baseline"）。
  - fresh + empty = authoritative zero（`exhaustive:true`；per-face exit
    維持家族現狀並明示契約——no-DEF exit1／0-callers exit0，review
    fresh-F12）。Positive evidence never dressed as current-tree
    completeness.〔muse Q3 null-shape + codex Q3 typed refusal — merged
    per-face〕
- **D8 scip_refs annotation** = `[SRC]` stays line 1; new line 2
  `[CR_VERDICT] v=1 tier=<t> exhaustive=<b> negative_verdict=<v>
  producer_version=<build rev> delta_changed=<n> delta_complete=<b>
  content_verification=<m>` (key=value, frozen vocabulary; producer_version
  enumerated per review intent-F2). Data comes from the verdict
  computation path — **not** `source_line` (hint-only surface,
  engine.rs:1624). Byte-parity goldens updated deliberately in this EP
  (explicit contract decision).
- **D9 graph_query annotation** = header-line system: stdout becomes
  `[OK] graph_query {op}\n[CR-META] {json v=1}\n{body}`. Rejects the
  `_meta`-injection design: `hub/bridge/communities/flows/affected_flows`
  return `Value::Array` (graph_engine.rs:2461-2500) — key injection is
  type-impossible, and an envelope change would break every existing
  consumer. `[CR-META]` schema v=1 enumerated:
  `{tier, producer_version, graph_snapshot_id, exhaustive,
  negative_verdict, delta_summary?}`. Header survives MCP tail-truncation
  and inherits into MCP verbatim (stdout forwarding, mcp_server.rs:240-253).
  Bypasses handled in-scope: `symbols` early-return (graph_engine.rs:
  2413-2428) gets the same header; MCP `get_community` direct library
  call (mcp_server.rs:821-855) routes through the shared annotation
  helper（其 MCP content 由純 JSON 變「頭行＋JSON」＝face 契約變更，入
  golden 與 S5 對照表）. **JSON-parse consumer inventory**（review
  fresh-F10）: `tests/multi_rust_root_e2e.rs:310` 以 `split_once('\n').1`
  parse 第 1 行後全文——D9 落地即破，屬本 EP 明示更新的 golden；S5 對照
  表列 bridge graph client。Contract: **no `[CR-META]` line = unannotated,
  never infer fresh** (incl. `gq_fail` error paths). `review_context`
  的 live source snippets 段加 current-tree 來源標示（review intent-F9）。
- **D10 graph publication provenance** = graph build stamps, in the same
  publication tx: `graph_snapshot_id` (per-build generation id =
  sha256(source_index_sha256 ‖ builder_revision ‖ schema_version) hex),
  `input_index_sha256`, `source_manifest_digest` (S2 manifest present
  ? digest : null), `builder_revision` (build-time CR_BUILD_REV — never
  the query-time binary rev), coverage/selection. Legacy db without keys
  → `graph_snapshot_id:null, tier:indeterminate` (no mtime/git-head
  substitutes — fake bindings are worse than absence).〔muse Q4 + codex
  Q4〕
- **D11 TOCTOU / serving-generation pinning** = annotations derive ONLY
  from the actually-serving artifact: scip path — the loaded index's
  `artifact_hash`（engine.rs:1169 已算——ServingStamp **共享同一值，禁
  雙算**，review fresh-F9）＋ load 當下 meta snapshot 隨查詢攜帶；assembly
  never re-reads paths; binding mismatch → bounded retry once →
  `indeterminate`. graph path — provenance read from the SAME rusqlite
  connection as the results（graph.db 無 WAL pragma——tmp+rename 後舊
  conn 綁舊 inode，前提經 fresh 腿查證）. Test: swap files mid-query,
  assert annotation matches served bytes. TOCTOU ownership＝S4
  （S2 不交付 ServingStamp——review fresh-F3）。ABA 全時刻線性化＝明示
  不宣稱（bounded consistency only）。〔muse Q5 + codex Q5〕
- **D12 MCP freshness tool** = new read-only tool; typed verdict handling:
  fresh/stale (CLI exit 0/1) construct `CallToolResult::success` with
  `is_error=false` explicitly — codex refuted the "route through
  to_tool_result" shortcut (`to_tool_result` sets `is_error=Some(true)`
  on any nonzero exit, mcp_server.rs:240-253); check failures (exit 2)
  stay errors. Stays out of `MANIFEST_WRITE_FACES`. s6 pins 21→22
  (tests/s6_mcp_server.rs:191, :745); tools_digest change is expected ABI
  evolution (bridge wrapper pin alignment flow).〔both + codex refutation〕
- **D13 truncation** = annotations live at head (D8/D9) so tail-truncation
  cannot eat them. Fail-closed trigger（review fresh-F5 收斂定義）＝
  ①標註頭行本身被截（病態小 cap）②refusal envelope 被截——二者轉
  typed `TRUNCATED_UNVERIFIED` error；**stale 大輸出 body 尾截斷而頭行
  在場＝annotated partial（success），不觸發 fail-closed**（exhaustive
  caveat 已隨頭行送達）。Bounded path lists always carry complete counts.
- **D14 producer_version fields** = annotations carry the **build-time**
  stamped producer revision（meta/graph metadata）; the current binary
  rev is disclosed separately where relevant. Query-time CR_BUILD_REV
  must never be presented as the builder version.〔codex Q4〕
- **D15 legacy/unverifiable degradation** = delta/provenance fields always
  present, values null + reason enum:
  `legacy-no-manifest | identity-algo-upgrade | manifest-missing |
  manifest-digest-mismatch | manifest-binding-audit-failed |
  walk-failed | coverage-unknown | budget-exceeded |
  graph-generation-mismatch`. Never absent（bridge 無法區分「舊
  producer」vs「新 producer＋legacy slot」）, never `[]` 冒充無變動.
  **優先序**：walk Err 的 exit-2 fail-loud 面優先於欄位恆在（整份 JSON
  不產出——review fresh-F7）；`budget-exceeded` 的生產者＝S4 per-query
  annotation 成本閘超限（tier 降 indeterminate）.〔muse Q7.2 + codex Q7〕
- **D16 `scripts/lsp_harvest.py` coexistence** = it rewrites
  `index.scip.meta.json` wholesale ({head, producer} — :257-259). The
  sidecar survives, but its placeholder index fails binding validation →
  legacy degradation, no panic, no delta claims. Named in S2 inventory +
  lifecycle test.〔both Q7〕
- **D17 segmentation & commit boundaries** = S1 contract（＝EP acceptance）
  → S2 (a)+(b) manifest + freshness delta → S3 graph publication
  provenance → S4 query gates（[CR_VERDICT] + [CR-META] + negative gate +
  TOCTOU pinning + MCP freshness tool；MCP digest change 獨立尾 commit）
  → S5 cross-repo acceptance hand-off（contract fixtures＋對照表＋信件；
  CR suites green ≠ enable authorization）. No half vocabulary in the
  wild: S2 must not emit S4 fields（tier/exhaustive/negative_verdict）.
 〔muse Q6 + codex Q6 merged〕

## UC 盤點

### Backlog 關聯
- CR-1..CR-9 全 Done——本 EP 無既有卡可復用。
- 自動建卡：追蹤卡 **CR-10**＋bounded children CR-10.1（S2）/CR-10.2
  （S3）/CR-10.3（S4）已建並 commit（chore(backlog)）。
- 去重前置：`backlog search delta/freshness` 零命中同題承諾。

### SYSTEM-MAP 影響
- 本 repo 無 SYSTEM-MAP.md——跳過（正當跳過）。

### 掃描範圍
- `AGENTS.md` Capabilities（root）：freshness face / graph_db build /
  Unified MCP interface / scip_refs 家族 四行受影響。
- `backlog task list --plain`：CR-1..9（全 Done）。

### 同主題 memory 條目（結案蒸餾範圍）
- 本 repo 無 memory 池——結案時照會 ai-guide-marshal 由其池判讀。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Source identity freshness face | ✅ | AGENTS.md Capabilities | 更新 | verdict face 加 delta 輸出（additive） |
| Self-owned graph db build | ✅ | AGENTS.md Capabilities | 更新 | publication 加 generation provenance metadata |
| Unified MCP interface | ✅ | AGENTS.md Capabilities | 更新 | 新增唯讀 freshness tool（digest 演進） |
| scip_refs / graph_query 家族 | ✅ | AGENTS.md Capabilities | 更新 | 查詢輸出加 freshness 標註＋negative gate |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Stale-graph delta disclosure（base manifest＋freshness delta 輸出） | 📋 | crates/code-reality/src/{identity,engine,freshness}.rs |
| Per-query freshness annotation＋negative gate | 📋 | crates/code-reality/src/{engine,callers,graph_engine,mcp_server}.rs |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | docs-only commit 後查詢 | HEAD 前移、identity 不變 | freshness=fresh、tier=exact、delta 空集、manifest 綁定不漂 | exit 0 | delta disclosure |
| SM-2 | dirty-at-build 後 commit 再改 | build 時工作樹內容 ≠ 任何 commit | manifest delta 非空而 git diff 為空 | delta counts >0 | delta disclosure |
| SM-3 | build 後新增 untracked 檔 | governed face 內新檔 | delta.added 含之；該檔查詢=no-coverage；inbound exhaustive=false | 雙標籤 | both |
| SM-4 | stale＋inbound 查詢空結果 | callers/closure/search/impact_radius/affected_flows 回零 | typed refusal（D7 per-face）；MCP is_error=true；CLI exit1＋[CR_VERDICT] | 無可讀 `[]` | annotation+gate |
| SM-5 | stale＋inbound 有命中 | 基準內已知 callers | success＋exhaustive=false＋possibly-stale | 正面證據照供 | annotation+gate |
| SM-6 | stale＋outbound/local 查詢 | 錨點檔未變 vs 變 | 相交=possibly-stale＋delta 摘要；未相交（fresh 才可能 exact）stale 時仍 possibly-stale——交集判定可觀察面＝delta path list | — | annotation |
| SM-7 | 查詢目標不在 index | WT 新檔／被排除檔 | no-coverage（與 stale 正交） | — | annotation |
| SM-8 | legacy slot（無 manifest） | 舊 producer 產出 | 欄位恆在＋null＋reason=legacy-no-manifest；verdict 照舊三態 | 不誤判 fresh | delta disclosure |
| SM-9 | manifest 綁定攻擊 | 竄改 entry／移植 manifest／index 換 manifest 未換 | 重組審計/digest 比對攔截→delta_complete=false＋正確 reason；verdict 不翻 fresh | 綁定攻擊組 | delta disclosure |
| SM-10 | graph 代 mismatch | 查詢釘 G1、slot 已換 S2 | 同 conn metadata→digest 不符→indeterminate；禁拿 M2 補 G1 | TOCTOU 組 | annotation |
| SM-11 | MCP 輸出截斷 | >cap 且頭行在場 vs 被截 | 頭行在場=annotated partial（success）；頭行/refusal 被截=TRUNCATED_UNVERIFIED error | D13 觸發定義 | annotation |
| SM-12 | freshness 經 MCP | 消費端自查 | stale=exit1 → is_error=false（typed verdict）；check-fail=exit2 → error | s6 釘 22 | MCP face |
| SM-13 | lsp_harvest 覆寫 meta | placeholder index | 綁定失敗→legacy 降級，不 panic | 生命週期組 | delta disclosure |
| SM-14 | head-sync re-stamp | docs-only commit＋refresh | meta head 前移、manifest 側檔不動、綁定仍成立 | preserve 契約 | delta disclosure |
| SM-15 | 重 build 生命週期 | Healed 全量重產 | 新 manifest 與新 index 同步出版；舊 graph 代標註 indeterminate | 孤兒側檔=0 | both |
| SM-16 | profile/topology/JS-TS config 變動 | 排除切換／root 新增／tsconfig 改 | policy_drift_flags 獨立旗標，不折進 changed/added/removed | scope 邊界組 | delta disclosure |

## 測試規劃段

author_family: glm（writer 腿經 bridge glm --write-mode edit --wt --card；
RED→GREEN 由主 session judge——same-family precondition 在 implement 消費）。
oracle 全部凍結於本表（advisor 測試矩陣已蒸餾進表——review intent-F7，
禁再引用 .agent-tmp 檔案當 oracle）。

| TC | claim | Given-When | oracle | oracle_source | class | 段 | uncovered |
|---|-------|-----------|--------|---------------|-------|---|-----------|
| TC-1 | docs-only head 前移不產生 delta | build→空 docs commit→freshness --json | `fresh==true && delta.changed==0 && delta_complete==true` | git 客觀史＋manifest 構造 | L2 | S2 | 無 |
| TC-2 | dirty-at-build delta 非空而 git diff 空 | build（含未提交改）→commit→freshness | `delta.changed 含該檔` 且 `git diff --name-only HEAD..` 不含 | manifest vs git 雙源對照 | L2 | S2 | 無 |
| TC-3a | untracked 進 delta.added | build 後新增 governed 檔 | `delta.added 含之` | manifest diff | L2 | S2 | 無 |
| TC-3b | 同檔直接查詢=no-coverage | 同上→查詢該檔 symbol | `tier==no-coverage` | index docs 缺席 | L2 | S4 | 無 |
| TC-4 | stale inbound 空=refusal（per-face） | 造 stale→callers 空／graph_query search 空 | 文字：exit1＋[CR_VERDICT] negative_verdict=unavailable；graph_query：CR-META unavailable＋結果 null；MCP：is_error=true＋type 於 content | D7 契約 | L2 | S4 | 無 |
| TC-5 | stale inbound 有命中=success+non-exhaustive | 同上但 baseline 有命中 | `is_error==false && exhaustive==false && tier==possibly-stale` | D7 | L2 | S4 | 無 |
| TC-6 | fresh inbound 空=權威零 | fresh→callers 空 | `exhaustive==true`＋exit 維持家族現狀（0-callers exit0） | D6/D7 | L2 | S4 | 無 |
| TC-7 | 綁定攻擊三組降級 | 竄改單一 entry hash／移植他 repo manifest／換 index 不換 manifest | 單一 entry 竄改由**重組審計**攔；三者皆 `delta_complete==false`＋正確 reason；verdict 不誤判 fresh | D2 綁定三條 | L2 | S2 | 無 |
| TC-8 | TOCTOU 釘 generation | load 與組裝間換 slot/graph.db | 標註 hash == 實際 serving 本體 artifact_hash（非新檔） | D11 ServingStamp | L3 | S4 | ABA 全時刻線性化 |
| TC-9 | [CR-META] 頭行全 op 覆蓋 | 12 op＋symbols bypass＋Array op＋get_community MCP face | 每輸出第 2 行 parse 得 v=1 CR-META；gq_fail 無行=契約測試 | D9 | L2 | S4 | 無 |
| TC-10 | MCP freshness typed verdict | stale slot→MCP freshness tool | `is_error==false && fresh==false`；破壞 slot→exit2→error | D12 | L3 | S4 | 無 |
| TC-11 | graph generation metadata 冪等與演變 | build→讀 metadata（同 conn）→同內容重 build→改輸入重 build | snapshot_id==sha256(index‖rev‖schema)；**同內容重 build 不變；輸入變更後重 build 改變** | D10 定義式（內容定址冪等——review fresh-F8） | L2 | S3 | 無 |
| TC-12 | 生命週期四組 | head-sync／Healed 重 build／空 corpus 收斂／lsp_harvest 覆寫 meta | 綁定成立／同步出版＋舊代 indeterminate／側檔隨 slot 消失零孤兒／降級不 panic | D2/D15/D16 | L2 | S2 | 無 |
| TC-13 | 截斷契約 | ①頭行在場大 body 尾截②病態小 cap 截頭行③refusal envelope 截 | ①=annotated partial success②③=TRUNCATED_UNVERIFIED error | D13 觸發定義 | L3 | S4 | cap 邊界外行為 |
| TC-14 | 成本預算閘 | NT-scale（本 repo 自食）warm | freshness warm ≤200ms（F-SM-11 基準 40ms 的寬鬆倍數錨定——review fresh-F9/intent-F10）；graph_query 加標註 p50 劣化 <20%；scip_refs 加標註同閘 | 既有 freshness 實測 | L4 | S4 | 冷端絕對值（記錄不設閘） |
| TC-15 | stale outbound tier 判定 | stale→錨點檔未變的 defs 查詢／錨點檔在 delta 的查詢 | 前者=possibly-stale＋delta 摘要（無 intersect 標示）；後者=possibly-stale＋delta 摘要含該檔 | D6/SM-6 | L2 | S4 | 無 |
| TC-16 | legacy 降級形 | 無 manifest slot→freshness --json | `delta 欄位在＋null＋reason=legacy-no-manifest`；verdict 三態照舊 | D15/SM-8 | L2 | S2 | 無 |
| TC-17 | policy/topology 旗標 | profile 排除切換／root 新增／tsconfig 改 | `policy_drift_flags` 含對應旗標；檔案計數不含之 | D4/SM-16 | L2 | S2 | 無 |
| TC-18 | rust-input delta 枚舉 | Cargo.toml-only 變更→freshness | `delta.changed` 含該 row（entries 含 rust-input rows） | D2/D4（review fresh-F6） | L2 | S2 | 無 |
| TC-19 | corrupt-cache discard 等價 | 偽造 cache entry（同 size 同 mtime-nanos）→delta；corrupt discard 後→delta | 與 ReadOnly 全量答案差異＝已知 residual（documented test）；discard 後與無 cache 全等 | D4 | L2 | S2 | stat-gate 偽造全防（明示 residual） |
| TC-20 | ServeStale／peer-heal 標註 | ServeStale 路徑查詢／peer heal 後查詢 | 標註按實際 serving 本體（stale 值不沿用 heal 前樂觀值；peer 產出對應 peer 本體） | D11 | L3 | S4 | 無 |
| TC-21 | graph_query gated ops refusal | stale→impact_radius/affected_flows 空 | CR-META `negative_verdict=unavailable`＋結果 null（D7 gate 集內） | D7 gate 集 | L2 | S4 | hub/bridge/communities（annotate-only，豁免明示） |

TC amendment 附錄：凍結後變更走 amendment（old/new oracle＋reason＋
evidence＋authority）。

## 段落 S2：(a)+(b) manifest 生產＋freshness delta 輸出

### Context

- UC：Stale-graph delta disclosure。依賴：無（首碼段）。
- 語義約束：與 S3 共享 manifest schema（D2）；與 S4 共享 reason enum
  （D15）。**S2 禁輸出 S4 欄位**（tier/exhaustive/negative_verdict）。
- 基礎設施盤點：D3 重構點（identity.rs:302-382、engine.rs:1245-1297——
  fresh 腿驗證 capture 為 manifest 源、build.rs:1242 第三呼叫端僅 cache
  repopulate 非 authoritative）、stamp 單點（engine.rs:1333）、
  validated keys（:1416-1427）、`preserve_prior_keys`（:1390-1398——
  新增 `base_manifest_digest` pointer 進清單）、identity-cache 原子寫
  範式（:269）、測試縫（support::git_init＋source_identity.rs:624-642
  多 commit 先例；多 commit helper 自補）。
- 依賴錨點：`compute_identity_scoped` 定義 identity.rs:302 / 消費
  engine.rs validated 路徑；`stamp_meta_locked` 定義 engine.rs:1333 /
  消費 refresh.rs:123、cli.rs:469；`load_meta` 定義 engine.rs:1439 /
  消費 evaluate_staleness、source_line。
- Invariant Impact：**stamped provenance 完整性**（meta/manifest 與
  index bytes 綁定——silent bad-provenance relay 病灶類）；critical
  path＝stamp 寫入面；驗證對齊＝TC-1/2/7/12/16/17/18/19。

### 核心實作要點

1. **D3 重構**：`compute_identity_scoped` →
   `{ identity, entries（source＋rust-input rows）, topology_hash }`；
   `ProductionIdentity::capture` 同趟收集攜出。Byte-determinism 由
   rust_identity_v2 既有測試釘（K1 spike 首日——kill criteria 見段落 0）。
2. **側檔出版**：validated stamp 路徑原子寫 manifest 側檔（D2 schema；
   schema_version=1）；meta 補 `base_manifest_digest` pointer 進
   preserve 清單。手動 `--stamp-meta`（cli.rs:469）無 validated A 時不
   產 manifest（降級 legacy-no-manifest）。head-sync（refresh.rs:123）
   不觸碰側檔。
3. **freshness delta 輸出**：`evaluate_staleness` 加載並驗證 manifest
   （D2 三條 binding audit）→ delta 計算（stamped faces ∪ rust-input
   rows scope；detected-only→out_of_scope；policy/topology/JS-TS
   config→policy_drift_flags）。JSON additive：`base_commit`（display）
   ＋`producer_version`＋`delta{changed/added/removed 計數＋bounded
   take-50 path list, delta_complete, content_verification,
   out_of_scope_count, policy_drift_flags}`＋`delta_identity`＋
   `delta_unavailable_reason`（D15；欄位恆在——walk Err exit2 面優先）。
   `--delta-full`＝完整枚舉。
4. **降級語義**：manifest 缺席/綁定不符/walk Err → D15 reason。
5. **lsp_harvest 共存**：不改 script；覆寫 meta 後綁定失敗自然降級
   （TC-12）。`\.meta\.json` 外部消費者補掃＝implement 檢查清單
   （fresh 腿已掃：僅 lsp_harvest.py＋tests）。

### 驗證策略

- RED 先行：TC-1/2/3a/7/12/16/17/18/19＋TC-14 之 freshness 面。
- 機驗：`cargo test` 全量綠；`code-reality freshness --repo . --json`
  實跑（本 repo 自食，NT-scale）；additive 欄位對照（舊 keys 零變）。
- POC：K1 spike（首日；budget 2-3 probes；UNKNOWN=顯性裁決不滑入）。

## 段落 S3：graph publication provenance

### Context

- UC：更新 Self-owned graph db build。依賴：S2（manifest 在場才記
  `source_manifest_digest`；缺場記 null＋建號照常）。
- 語義約束：與 S4 共享 graph_snapshot_id 定義（D10）；metadata 讀取
  必同 conn（D11，S4 消費側紀律）。
- 基礎設施盤點：metadata DDL（graph_db.rs:175）、producer 戳（:898）、
  stamp_snapshot_metadata（:401-415）、publication tmp+rename（~:910；
  無 WAL pragma——fresh 腿查證）。
- 依賴錨點：graph_db build 出版路徑 graph_db.rs:757-777,897-915 /
  消費 graph_engine::open:47-56。

### 核心實作要點

1. build（graph_db build＋資料面 build 的 graph 腿）同 tx stamp：
   D10 五欄位。`source_manifest_digest`＝S2 側檔 digest（缺→null）。
2. `git_head_sha` 維持 display 軸不作 provenance 鍵；builder_revision
   來自編譯期常數不受 git 影響。
3. legacy db（無 key）語義由 S4 消費；本段只管出版。

### 驗證策略

- TC-11（冪等＋演變 oracle）＋concurrent generation 對帳（開舊 conn
  後重 build→舊 conn metadata 不變）。

## 段落 S4：(c) 查詢面標註＋negative gate＋MCP freshness tool

### Context

- UC：Per-query freshness annotation＋negative gate。依賴：S2（delta）＋
  S3（graph provenance）。
- 語義約束：詞彙凍結（D6/D7/D15）；標註只從 serving 本體派生（D11）；
  scip_refs/graph_query byte-parity golden 更新＝本段明示契約決策
  （含 multi_rust_root_e2e.rs:310 的第 2 行插入破 parse——明示更新）。
- 基礎設施盤點：三 format 的 src_line 注入點（engine.rs:350-380、
  callers.rs:151-214——**含 format_callers `0 callers` exit-0 成功形與
  format_closure empty-frontier exit-0 兩個零行變體**，review fresh-F2）、
  graph_query choke（graph_engine.rs:2514-2525）、symbols early-return
  （:2413-2428）、get_community MCP 直呼（mcp_server.rs:821-855）、
  to_tool_result/map_tool_output（:240-274）、apply_text_cap（:220-234）、
  evaluate_staleness artifact_hash（engine.rs:1169——共享禁雙算）、s6
  釘數兩處。
- 依賴錨點：`evaluate_staleness` 定義 engine.rs / 消費 freshness.rs:87
  與（本段新增）查詢路徑共享 verdict 計算。

### 核心實作要點

1. **verdict 共享計算**：查詢路徑入口算一次（evaluate＋delta 摘要，
   ServingStamp 共享 artifact_hash）→ 傳入組裝點；per-op 禁重算；
   成本閘超限→tier=indeterminate＋reason=budget-exceeded（D15）。
2. **scip_refs [CR_VERDICT] 行**（D8）：三 format 統一第二行（含兩個
   零行變體）；negative gate（D7 per-face）：stale＋callers/closure 空
   → exit1＋unavailable（**現行 exit-0 成功形翻轉＝明示 contract
   change**，goldens 更新）。
3. **graph_query [CR-META] 頭行**（D9）：choke point 統一＋symbols
   bypass 同點＋get_community 共享 helper（face 契約變更入 golden）；
   gated ops（search/impact_radius/affected_flows）stale＋空 → CR-META
   unavailable＋結果 null；其餘 op annotate-only（豁免明示）；gq_fail
   無行＝不推論 fresh。
4. **MCP freshness tool**（D12）：typed verdict；s6 釘 21→22；digest
   commit 獨立段尾。
5. **truncation**（D13）：頭行制下標註存活；fail-closed 觸發＝頭行被截
   或 refusal envelope 被截。
6. **TOCTOU pinning**（D11）：ServingStamp（共享 artifact_hash）＋同
   conn metadata；mismatch→retry 一次→indeterminate；ServeStale/peer-heal
   按實際本體（TC-20）。`review_context` snippets 段加 current-tree
   來源標示。
7. **收尾面**：crates/AGENTS.md exit-semantics 表同步（refusal exit1）；
   fresh+empty per-face exit 表入契約節。

### 驗證策略

- RED 先行：TC-3b/4/5/6/8/9/10/13/15/20/21＋TC-14 之 graph_query/
  scip_refs 面。
- 機驗：`cargo test` 全量綠；golden 更新以「輸出契約 v1」凍結檔承載；
  MCP 契約組（is_error 對照＋cap 前後 1 byte）。

## 段落 S5：跨 repo acceptance hand-off

### Context

- 依賴：S2-S4 全綠。產出：
  ①契約 fixture（輸出範例＋frozen 詞彙表＋**標註覆蓋 inventory**：
  已標註面=scip refs/callers/closure、graph_query 12 op、freshness
  JSON、MCP freshness tool；**第一期豁免面（unverified）=hub_refs/
  hazard、audit、chain_tour、delta_tour、snapshot、boundary**——review
  intent-F4/fresh-F11）
  ②**跨 repo 消費者對照表**（bridge graph client／DB-120 觀測面／
  AIR-330 措辭——get_community face 變更與 graph_query 第 2 行插入的
  影響面明列）
  ③兩封信草稿（bridge：DB-101 對接面；ai-guide：AIR-330 詞彙對齊
  ＝K2 probe）。
- **寄信=outward，逐次 AUTH**；CR suite 綠 ≠ bridge default enable。

### 驗證策略

- fixture 與實作輸出逐字一致（機驗）；信件經 user AUTH 後寄。

## 段落 0：全域研究（已完成——dossier 為研究記錄）

- 研究記錄：`.agent-tmp/cr-delta-freshness-dossier.md`（§1 全機驗錨點；
  EP Review 雙腿已逐錨驗真——唯一修正：`preserve_prior_keys` 實體在
  engine.rs:1382-1406，dossier 行號略偏）。
- 可複用基礎設施：`engine::git_head`、`IdentityCache` tmp+rename 範式、
  `stale_head_warn` metadata 讀法範本、`run_module` MCP tool 模式、
  `support::git_init`（多 commit helper 自補）。
- 風險假設（kill criteria——AIR-131）：
  - **K1 單趟 hash pass 重構可行性**：fresh 腿 dry-run 已確認 per-file
    `chash` 算後即丟（identity.rs:333-339）且 capture 同趟攜出
    Vec entries 不需第二趟 walk、BTreeMap 序天然 deterministic——風險
    降低但 spike 仍跑（rust_identity_v2 為 oracle）。Kill observation=
    fence 需放寬（A 綁定破壞）或 determinism 破功且修復需第二趟 walk
    且 NT-scale 冷成本 >2× identity pass → 停，re-adjudicate。
  - **K2 詞彙跨 repo 收斂**：probe＝S5 契約 fixture 寄 ai-guide。
    Kill observation=AIR-330 定稿詞彙與 EP 凍結值衝突且讓步會破 D7 硬閘
    → 升級討論（版本化並存），不在 S4 前單方改詞。
- negative 宣稱結構化：dossier §1 錨點經雙腿驗真；`\.meta\.json` 外部
  消費者 fresh 腿補掃完成（僅 lsp_harvest.py＋tests）。
- callstack 菜單積壓：`ai-analysis/blueprint/callstack-plan.md` 不存在
  ——跳過。

## 整合策略

- baseline: `1b0ba1e62b51efd8a249564e7431f9d113e0a80e`
- 收線 scope diff 核對：`git diff baseline..HEAD` 對照授權 scope。
- 段間 commit 邊界＝S2／S3／S4 主體／S4 MCP digest（獨立尾 commit）。
- 下游消費者：bridge DB-120/DB-101、ai-guide AIR-330、plugin
  code-reality-tools SKILL.md（consumer contract 文檔——S4 內同步、
  instruction-writing 紀律）、lsp_harvest.py（不動）。
- author_family: glm（same-family precondition＝RED 前 challenge：
  主 session 以本 TC 表為 frozen oracle 對 writer 測試 blind
  derive→reveal 比對）。

## 收尾步驟

1. AGENTS.md Capabilities：四行更新＋新能力兩行（消費場景自 SM 提煉）；
   plugin skills/code-reality-tools SKILL.md 契約節。
2. 卡結案兩步＋bounded children 逐段結案；弧結案蒸餾照會 ai-guide 池。
3. crates/AGENTS.md exit-semantics 表同步（S4）。
4. /audit-test 對新增測試套件稽核。
5. release 弧（版號鎖步＋plugin pin 對齊）另卡承接。

## non-goals

bridge DB-101/DB-120 契約與詞彙；ai-guide AIR-330 措辭本體；
scope-declaration escape；git tree object 寫入消費者 repo；autoheal
行為變更；cr-freshness leaf crate；舊 graph generation 的 manifest
retention/GC（fixed-name 失配→indeterminate 已 fail-closed）；mosaic
47 檔遷移；pyrefly 上游 panic（另卡追蹤）。

## EP Review 區段（規劃期帳本——boundary profile：fresh＋intent 雙腿）

審查配置：`[EP Review] profile=boundary, context=fresh+intent`。
fresh-eyes（code-reviewer agent）：F1-F12，needs-fix（輕度，無 🔴）；
intent-primed（code-reviewer-primed agent）：F1-F10，needs-fix。
錨點驗真：雙腿逐檔抽查全命中（含 `preserve_prior_keys` 行號偏移修正、
`to_tool_result` is_error=Some(true) 驗真、graph.db 無 WAL 確認）。
正面查證：D3/D9/D11/D12 可行性於源碼成立。

| Finding | 嚴重度 | verdict | disposition |
|---|---|---|---|
| intent-F1 綁定審計缺口（entries 重組==stamped 漏收） | 🟡 | ✅採納 | D2 加重組審計＋`base_manifest_digest` pointer＋per-rust-input rows；TC-7 oracle 同步 |
| intent-F2 producer_version/base_commit 無落點 | 🟡 | ✅採納 | D8 key 列舉＋D9 CR-META schema 枚舉＋S2 freshness JSON 補兩欄 |
| intent-F3 graph_query gate op 集未定 | 🟡 | ✅採納 | D7 gate 集凍結（search/impact_radius/affected_flows＋callers/closure；defs/refs=no-coverage 軸）；TC-21 補 |
| intent-F4 豁免面 inventory 缺 | 🟢 | ✅採納 | S5 fixture 增標註覆蓋 inventory |
| intent-F5 refusal exit 碼未釘 | 🟢 | ✅採納 | D7 釘 exit1＋[CR_VERDICT] 區分；exit 表入 S4 收尾 |
| intent-F6 S1 無段落定義 | 🟢 | ✅採納 | 新增段落 S1（契約凍結＝EP acceptance；K2 probe 掛 S5） |
| intent-F7 advisor 矩陣 oracle 斷點 | 🟡 | ✅採納 | 引用矩陣項全蒸餾入 TC 表（TC-15~21）；TC-12 oracle 自含化 |
| intent-F8 JS/TS config 歸類未定 | 🟢 | ✅採納 | D4 明示歸 policy_drift_flags |
| intent-F9 review_context snippets 混合來源 | 🟢 | ✅採納 | S4 點 6 補 current-tree 來源標示 |
| intent-F10 TC-14 閘值寬鬆 | ℹ️ | ✅採納 | 錨定 F-SM-11 40ms 基準（≤200ms）＋劣化 <20% |
| fresh-F1 JSON-null refusal 無載體 | 🟡 | ✅採納 | D7 改 per-face rendering（text=exit1＋[CR_VERDICT]；graph_query=CR-META＋null；MCP=is_error＋type 於 content） |
| fresh-F2 callers/closure 零行 exit-0 翻轉未明示 | 🟡 | ✅採納 | S4 盤點補兩零行變體；翻轉＝明示 contract change 入 golden |
| fresh-F3 TC-3/TC-8 段落錯置 | 🟡 | ✅採納 | TC-3 拆 3a(S2)/3b(S4)；TC-8 移 S4；D11 ownership=S4 |
| fresh-F4 SM-6/8/16 無 frozen TC | 🟡 | ✅採納 | TC-15/16/17 新增 |
| fresh-F5 TC-13 觸發生產者未定義 | 🟡 | ✅採納 | D13 觸發收斂定義（頭行在場=annotated partial；頭行/refusal 被截才 fail-closed）；TC-13 改三情境 |
| fresh-F6 rust-input 枚舉粒度未定 | 🟡 | ✅採納 | D2 entries 含 rust-input rows；TC-18 新增 |
| fresh-F7 exit-2 優先序＋budget-exceeded 歸屬 | 🟢 | ✅採納 | D15 補兩句 |
| fresh-F8 TC-11 oracle 違反內容定址冪等 | 🟢 | ✅採納 | oracle 改冪等式 |
| fresh-F9 ServingStamp 雙算＋TC-14 數值不一致 | 🟢 | ✅採納 | D11 共享 artifact_hash（engine.rs:1169）；TC-14 錨定＋補 scip_refs 閘 |
| fresh-F10 graph_query JSON-parse 消費者漏盤點 | 🟢 | ✅採納 | D9 consumer inventory（multi_rust_root_e2e:310 明示更新）＋get_community face 變更入 S5 對照表 |
| fresh-F11 unannotated faces 清單缺 | 🟢 | ✅採納 | 併 intent-F4（S5 inventory） |
| fresh-F12 fresh+empty exit 語義未定 | ℹ️ | ✅採納 | D7 契約節補 per-face exit 表（維持家族現狀） |

Judge 總結：22 findings 全數採納（0 駁回、0 待確認）——修正已寫回
D2/D4/D7/D8/D9/D11/D13/D15/D17＋TC 表（TC-3 拆分、TC-8/11/12/13/14
修訂、TC-15~21 新增）＋S2/S4/S5 段落。EP 具 accepted eligibility
（ledger 全 terminal＋修正回寫完成）。
