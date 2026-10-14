# EP: CR delta-freshness — stale-graph delta disclosure + per-query freshness annotation

> **ep_type**: implementation
> status: draft (pending EP Review Cycle)
> baseline: `1b0ba1e62b51efd8a249564e7431f9d113e0a80e`
> date: 2026-10-14
> advisors: muse job-mv7hxe3f-vmzv26 + codex job-mv7hxeai-dcnqio (verdicts: `.agent-tmp/cr-delta-freshness-{muse,codex}-verdict.md`)
> upstream anchors: bridge codex consult job-mv7ae77q (Amend ruling), bridge proposal `delegate-bridge/.agent-tmp/cr-delta-freshness-proposal.md`, ai-guide AIR-330 (in progress), dossier `.agent-tmp/cr-delta-freshness-dossier.md`

## 實作總覽

Today the freshness face is all-or-nothing: any content drift flips
`serves=committed-baseline` and every consumer (bridge fresh-only supply
gate, cr-query discipline) downgrades wholesale — a docs-only commit skips
the entire graph server even though every structural query would answer
`exact` (real waste: receipt job-mv78cymo-qb4r7n). This EP makes the stale
SET computable and per-query freshness honest:

- **(a)** at build time, persist the governed-corpus per-file manifest
  (base manifest) beside the index slot, bound to the index bytes.
- **(b)** the freshness face emits a delta set (changed/added/removed vs
  the base manifest) with explicit completeness semantics.
- **(c)** query faces annotate per-result freshness tier
  (`exact|possibly-stale|no-coverage|indeterminate`) + producer version,
  enforce a **hard negative gate** (a stale inbound query can never
  present an empty result as authoritative absence), and expose a
  read-only `freshness` tool on the MCP server face.

**Scale ruling: full EP** (both advisors agree) — cross-repo public
contract (bridge DB-101 enable gate + ai-guide AIR-330 vocabulary freeze
bind on it), new state shape (manifest sidecar lifecycle + binding), and
a query-evidence boundary decision. Two advisory rounds converged the
design; their rulings are absorbed below as decided (勿重辯).

**Cross-repo context (sovereignty fence)**: bridge DB-120/DB-101 and
ai-guide AIR-330 consume this contract but are NOT in scope. This EP
ships the CR-side producer + faces + contract fixtures; external
enablement is gated on their side (S5 hand-off, no code).

## 已裁決設計（勿重辯——advisor convergence，provenance per item）

- **D1 delta truth source** = stamped per-file **manifest diff**; git diff
  is display-only (`base_commit` reference). Rationale: dirty-at-build
  content, untracked-at-build files, and head-sync `head` drift make any
  commit-anchored diff unsound (muse Q1 killer case; codex Q1).〔both〕
- **D2 manifest storage** = fixed-name sidecar
  `.code-reality/scip/index.scip.manifest.json` (tmp+rename atomic write,
  mirroring `identity-cache.json` :269 precedent), NOT in `meta.json` —
  D4 precedent bars per-file tables from the human-provenance meta, and
  `load_meta` sits on query hot paths (muse Q2). Content:
  `{schema_version, repo, selection, faces, identity_algo, index_sha256,
  source_identity, rust_inputs_digest, topology_hash, entries:[{face, rel,
  size, hash}]}` — no mtime (identity body deliberately mtime-free).
  Binding validation on read: repo/selection/identity_algo/index_sha256
  must equal current meta; any mismatch or absence → legacy degradation
  (D15), never a substitute answer.〔muse Q2 / codex Q2; fixed-name over
  digest-named for this arc: old-generation annotation degrades to
  `indeterminate` (fail-closed), digest retention is a future
  optimization〕
- **D3 manifest production** = refactor the validated production hash
  pass (`compute_identity_scoped` identity.rs:302-382 /
  `ProductionIdentity::capture` engine.rs:1245-1297) to emit the per-file
  table and the aggregate from the SAME authenticated pass (Full policy,
  stamp path only). The advisors refuted "free byproduct": per-file
  hashes are discarded today; this refactor IS the (a) work. Never
  assemble a stamped manifest from query-side caches.〔both〕
- **D4 delta_complete semantics** = enumeration axis only: walk success +
  manifest binding valid + every governed file hashed without Err.
  Separate `content_verification` field ∈ `full-rehash | stat-cache`
  discloses the hash trust level (the identity cache stat-gate residual —
  same size + same mtime-nanos forgery — is inherited, never denied).
  Delta scope = **stamped faces ∪ rust-inputs**, not eval_faces;
  detected-only faces bucket as `out_of_scope`; profile/topology drift
  surface as separate flags, not file counts.〔codex Q1 split + muse Q1
  conditions〕
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
- **D7 negative gate (hard, at query boundary)**: stale + inbound-family
  query (refs/callers/closure) + empty result ⇒ typed refusal — JSON:
  result fields `null` + `negative_verdict:"unavailable"` + reason;
  MCP: `is_error=true` typed `stale-negative-unavailable`; CLI text:
  `[CR_VERDICT]` line + nonzero exit. A readable `[]` must not exist in
  that state. Non-empty stale results flow as success with
  `exhaustive:false` + baseline semantics ("known within indexed
  baseline"). Fresh + empty = authoritative zero (`exhaustive:true`).
  Positive evidence never dressed as current-tree completeness.〔muse Q3
  null-shape + codex Q3 typed refusal — merged: empty refuses, positives
  annotate〕
- **D8 scip_refs annotation** = `[SRC]` stays line 1; new line 2
  `[CR_VERDICT] tier=<t> exhaustive=<b> negative_verdict=<v>
  delta_changed=<n> delta_complete=<b> content_verification=<m>`
  (key=value, frozen vocabulary, versioned `v=1`). Data comes from the
  verdict computation path — **not** `source_line` (hint-only surface,
  engine.rs:1624). Byte-parity goldens updated deliberately in this EP
  (explicit contract decision, not incidental).〔muse Q3〕
- **D9 graph_query annotation** = header-line system: stdout becomes
  `[OK] graph_query {op}\n[CR-META] {json v=1}\n{body}`. Rejects the
  `_meta`-injection design: `hub/bridge/communities/flows/affected_flows`
  return `Value::Array` (graph_engine.rs:2461-2500) — key injection is
  type-impossible, and an envelope change would break every existing
  consumer. Header line survives MCP tail-truncation and inherits into
  MCP verbatim (stdout forwarding, mcp_server.rs:240-253). Known bypasses
  handled in-scope: `symbols` early-return (graph_engine.rs:2413-2428)
  gets the same header; MCP `get_community` direct library call
  (mcp_server.rs:821-855) routes through the shared annotation helper.
  Contract: **no `[CR-META]` line = unannotated, never infer fresh**
  (incl. `gq_fail` error paths).〔muse Q4 + codex Q4 bypasses〕
- **D10 graph publication provenance** = graph build stamps, in the same
  publication tx: `graph_snapshot_id` (per-build generation id =
  sha256(source_index_sha256 ‖ builder_revision ‖ schema_version) hex),
  `input_index_sha256`, `source_manifest_digest`, `builder_revision`
  (build-time CR_BUILD_REV — never the query-time binary rev),
  coverage/selection. Legacy db without keys →
  `graph_snapshot_id:null, tier:indeterminate` (no mtime/git-head
  substitutes — fake bindings are worse than absence).〔muse Q4 + codex
  Q4; builder-revision semantics per codex〕
- **D11 TOCTOU / serving-generation pinning** = annotations derive ONLY
  from the actually-serving artifact: scip path — the loaded index
  computes its artifact hash + meta snapshot at load (ServingStamp
  carried with the query); assembly never re-reads paths; binding
  mismatch → bounded retry once → `indeterminate`. graph path —
  provenance read from the SAME rusqlite connection as the results (an
  open conn keeps the old inode across tmp+rename publication; reading
  live paths instead is the mislabel risk). Test: swap files mid-query,
  assert annotation matches served bytes.〔muse Q5 + codex Q5〕
- **D12 MCP freshness tool** = new read-only tool; typed verdict handling:
  fresh/stale (CLI exit 0/1) construct `CallToolResult::success` with
  `is_error=false` explicitly — codex refuted the "route through
  to_tool_result" shortcut (`to_tool_result` sets `is_error=Some(true)`
  on any nonzero exit, mcp_server.rs:240-253); check failures (exit 2)
  stay errors. Stays out of `MANIFEST_WRITE_FACES`. s6 pins 21→22
  (tests/s6_mcp_server.rs:191, :745); tools_digest change is expected ABI
  evolution (bridge wrapper pin alignment flow).〔both + codex refutation〕
- **D13 truncation** = annotations live at head (D8/D9) so tail-truncation
  cannot eat them; data budget controlled before serialization; a
  truncated response that would carry a negative/complete-critical
  conclusion fails closed (typed `TRUNCATED_UNVERIFIED` error), never a
  successful half-JSON. Bounded path lists always carry complete counts.〔codex Q4〕
- **D14 producer_version fields** = annotations carry the **build-time**
  stamped producer revision (from meta/graph metadata); the current
  binary rev is disclosed separately where relevant. Query-time
  CR_BUILD_REV must never be presented as the builder version.〔codex Q4〕
- **D15 legacy/unverifiable degradation** = delta/provenance fields always
  present, values null + reason enum:
  `legacy-no-manifest | identity-algo-upgrade | manifest-missing |
  manifest-digest-mismatch | walk-failed | coverage-unknown |
  budget-exceeded | graph-generation-mismatch`. Never absent (bridges
  can't distinguish "old producer" from "new producer, legacy slot"),
  never `[]` masquerading as "no changes".〔muse Q7.2 + codex Q7〕
- **D16 `scripts/lsp_harvest.py` coexistence** = it rewrites
  `index.scip.meta.json` wholesale ({head, producer} — :257-259). The
  sidecar survives, but its placeholder index fails binding validation →
  legacy degradation, no panic, no delta claims. Named in S2 inventory +
  lifecycle test.〔both Q7〕
- **D17 segmentation & commit boundaries** = S1 contract (this EP frozen
  at acceptance; vocabulary changes reopen it) → S2 (a)+(b) manifest +
  freshness delta → S3 graph publication provenance → S4 query gates
  ([CR_VERDICT] + [CR-META] + negative gate + TOCTOU pinning + MCP
  freshness tool; MCP digest change as an independent final commit) →
  S5 cross-repo acceptance hand-off (contract fixtures + letters; CR
  suites green ≠ enable authorization). No half vocabulary in the wild:
  S2 must not emit S4 fields (muse Q6).〔muse Q6 3-seg + codex Q6 4-seg
  merged〕

## UC 盤點

### Backlog 關聯
- CR-1..CR-9 全 Done（`backlog task list`：無 open 卡）——本 EP 無既有卡可復用。
- 自動建卡：追蹤卡 **CR-10**（本 EP 整體）——EP 落地後掛雙 ref；各段以 bounded child 卡承接（S2/S3/S4 各一張，繼承本 EP 已定決策）。
- 去重前置：`backlog search delta/freshness` 零命中未承諾同題；bridge 側 DB-120/DB-101 為他 repo 主權卡，僅消費本契約。

### SYSTEM-MAP 影響
- 本 repo 無 SYSTEM-MAP.md——跳過（正當跳過，標記理由）。

### 掃描範圍
- `AGENTS.md` Capabilities（root）：freshness face / graph_db build / Unified MCP interface / scip_refs 家族 四行受影響。
- `backlog task list --plain`：CR-1..9（全 Done）。

### 同主題 memory 條目（結案蒸餾範圍）
- 本 repo 無 memory 池（spine 在 ai-guide 側）——跳過；結案時照會 ai-guide-marshal 由其池判讀。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Source identity freshness face | ✅ | AGENTS.md Capabilities | 更新 | verdict face 加 delta 輸出（向後相容 additive） |
| Self-owned graph db build | ✅ | AGENTS.md Capabilities | 更新 | publication 加 generation provenance metadata |
| Unified MCP interface | ✅ | AGENTS.md Capabilities | 更新 | 新增唯讀 freshness tool（digest 演進） |
| scip_refs / graph_query 家族 | ✅ | AGENTS.md Capabilities | 更新 | 查詢輸出加 freshness 標註＋negative gate |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Stale-graph delta disclosure（base manifest＋freshness delta 輸出） | 📋 | crates/code-reality/src/{identity,engine,freshness}.rs |
| Per-query freshness annotation＋negative gate（查詢面三層標註） | 📋 | crates/code-reality/src/{engine,callers,graph_engine,mcp_server}.rs |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | docs-only commit 後查詢 | HEAD 前移、identity 不變 | freshness=fresh、tier=exact、delta 空集、manifest 綁定不漂 | exit 0 | delta disclosure |
| SM-2 | dirty-at-build 後 commit 再改 | build 時工作樹內容 ≠ 任何 commit | manifest delta 非空而 git diff 為空——機械源用 manifest | delta counts >0 | delta disclosure |
| SM-3 | build 後新增 untracked 檔 | governed face 內新檔 | delta.added 含之；對該檔查詢=no-coverage；inbound exhaustive=false | 雙標籤 | both |
| SM-4 | stale＋inbound 查詢空結果 | callers/closure/refs 回零 | typed refusal：null＋negative_verdict=unavailable（MCP is_error=true；CLI 非零 exit＋[CR_VERDICT]） | 無可讀 `[]` | annotation+gate |
| SM-5 | stale＋inbound 有命中 | 基準內已知 callers | success＋exhaustive=false＋possibly-stale | 正面證據照供 | annotation+gate |
| SM-6 | stale＋outbound/local 查詢 | defs/錨點檔未變 | tier 按 delta 相交判定：未相交=possibly-stale（檔面 exact 語義限已定義 index universe） | — | annotation |
| SM-7 | 查詢目標不在 index | WT 新檔／被排除檔 | no-coverage（與 stale 正交） | — | annotation |
| SM-8 | legacy slot（無 manifest） | 舊 producer 產出 | 欄位恆在＋null＋reason=legacy-no-manifest；verdict 照舊三態 | 不誤判 fresh | delta disclosure |
| SM-9 | manifest 綁定不符 | 竄改/移植/index 換 manifest 未換 | delta_complete=false＋reason=manifest-digest-mismatch；freshness verdict 不因此翻 fresh | 綁定攻擊組 | delta disclosure |
| SM-10 | graph 代 mismatch | 查詢釘 G1、slot 已換 S2 | 同 conn 讀 metadata→digest 對不上→indeterminate；禁拿 M2 補 G1 | TOCTOU 組 | annotation |
| SM-11 | MCP 輸出截斷 | >cap 且結論含 negative/complete | typed TRUNCATED_UNVERIFIED error；頭行標註存活 | cap 前後 1 byte | annotation |
| SM-12 | freshness 經 MCP | bridge/消費端自查 | stale=exit1 → is_error=false（typed verdict）；check-fail=exit2 → error | s6 釘 22 | MCP face |
| SM-13 | lsp_harvest 覆寫 meta | placeholder index | manifest 綁定失敗→legacy 降級，不 panic | 生命週期組 | delta disclosure |
| SM-14 | head-sync re-stamp | docs-only commit＋refresh | base payload（head）前移、manifest 側檔不動、綁定仍成立 | preserve 契約 | delta disclosure |
| SM-15 | 重 build 生命週期 | Healed 全量重產 | 新 manifest 與新 index 同步出版；舊 graph 代標註 indeterminate | 孤兒側檔=0 | both |
| SM-16 | profile/topology 變動 | 排除切換／root 新增 | 獨立 policy 旗標，不折進 changed/added/removed | scope 邊界組 | delta disclosure |

## 測試規劃段

author_family: glm（writer 腿經 bridge glm --write-mode edit --wt --card；
RED→GREEN 由主 session judge——same-family precondition 在 implement 消費：
每段 RED 先行、oracle 由本節 TC 凍結供給，writer 不得自證）

凍結 TC（predicate-ID 拆分；oracle 禁指向待測實作）：

| TC | claim | Given-When | oracle | oracle_source | class | uncovered |
|---|-------|-----------|--------|---------------|-------|-----------|
| TC-1 | docs-only head 前移不產生 delta | build→touch HEAD（空 docs commit）→freshness --json | `fresh==true && delta.changed==0 && delta_complete==true` | git 客觀史＋manifest 構造 | L2 | 無 |
| TC-2 | dirty-at-build delta 非空而 git diff 空 | build（含未提交改）→commit→freshness | `delta.changed 含該檔` 且 `git diff --name-only HEAD..` 不含 | manifest entries vs git 雙源對照 | L2 | 無 |
| TC-3 | untracked 雙標籤 | build 後新增 governed 檔 | `delta.added 含之 && 該檔查詢 tier==no-coverage` | manifest diff＋index docs 缺席 | L2 | 無 |
| TC-4 | stale inbound 空=refusal | 改碼造 stale→callers（無 baseline 命中） | JSON 結果 null＋negative_verdict=unavailable；MCP is_error=true；CLI exit≠0 | D7 契約（本 EP 凍結） | L2 | 無 |
| TC-5 | stale inbound 有命中=success+non-exhaustive | 同上但 baseline 有命中 | `is_error==false && exhaustive==false && tier==possibly-stale` | D7 | L2 | 無 |
| TC-6 | fresh inbound 空=權威零 | fresh→callers 空 | `exhaustive==true` | D6/D7 | L2 | 無 |
| TC-7 | 綁定攻擊三組降級 | 竄改 entry hash／移植他 repo manifest／換 index 不換 manifest | `delta_complete==false`＋正確 reason；verdict 不誤判 fresh | D2 綁定欄位 | L2 | 無 |
| TC-8 | TOCTOU 釘 generation | load 與組裝間換 slot/graph.db | 標註 hash == 實際 serving 本體 hash（非新檔） | D11 ServingStamp | L3 | ABA 全時刻線性化（EP 明示不宣稱） |
| TC-9 | [CR-META] 頭行全 op 覆蓋 | 12 op＋symbols bypass＋Array op | 每輸出第 2 行 parse 得 v=1 CR-META；gq_fail 無行=契約測試 | D9 | L2 | 無 |
| TC-10 | MCP freshness typed verdict | stale slot→MCP freshness tool | `is_error==false && fresh==false`；破壞 slot→exit2→error | D12 | L3 | 無 |
| TC-11 | graph generation metadata | build→讀 metadata（同 conn） | snapshot_id==sha256(index_sha256‖builder_rev‖schema) 且重 build 後改變 | D10 定義式 | L2 | 無 |
| TC-12 | 生命週期四組 | head-sync／Healed／空 corpus 收斂／lsp_harvest 覆寫 | TC-14 矩陣（muse 生命週期組）逐項 | D2/D15/D16 | L2 | 無 |
| TC-13 | 截斷 fail-closed | >cap＋negative 結論 | TRUNCATED_UNVERIFIED error；頭行存活 | D13 | L3 | cap 邊界外行為 |
| TC-14 | 成本預算閘 | NT-scale（本 repo 自食）warm/cold | freshness warm < 1s（F-SM-11 既有水準）；graph_query 加標註 warm p50 劣化 < 20% | 既有 freshness 實測基準 | L4 | 冷端絕對值（記錄不設閘） |

TC amendment 附錄：本節凍結後變更走 amendment（old/new oracle＋reason＋evidence＋authority）。

## 段落 0：全域研究（已完成——dossier 為研究記錄）

- 研究記錄：`.agent-tmp/cr-delta-freshness-dossier.md`（§1 全機驗錨點，
  cr-research 代理 CR callers＋rg 雙腿；本 EP 段落直接引用其錨點）。
- 可複用基礎設施：`engine::git_head`（容錯 git）、`IdentityCache`
  tmp+rename 原子寫範式（identity.rs:269）、`stale_head_warn` metadata
  讀法範本（graph_db.rs:421-436）、`run_module` MCP tool 模式
  （mcp_server.rs:583-595 audit 形）、`support::git_init` 測試縫
  （多 commit helper 需自補）。
- 風險假設（kill criteria——AIR-131）：
  - **K1 單趟 hash pass 重構可行性**：Assumption=`compute_identity_scoped`
    可在既有 validated production fence（A 綁定）內重構為同趟輸出
    per-file 表＋aggregate 且保持 byte-determinism。Probe=S2 開工首日
    以 spike 驗證（現有 rust_identity_v2 測試為 oracle）。Kill
    observation=若 fence 需放寬（A 綁定破壞）或 determinism 破功且
    修復需第二趟 walk 且 NT-scale 冷成本 >2× identity pass → 停，
    re-adjudicate（改獨立 manifest 趟並重議成本閘）。Action=S2 內裁決，
    不滑入 implementation。
  - **K2 詞彙跨 repo 收斂**：Assumption=本 EP 凍結詞彙（D6/D7/D15
    reason enum）與 ai-guide AIR-330 移動中的契約可在 S5 前對齊。
    Probe=S1 驗收時將契約節草稿寄 ai-guide（AUTH-gated）。Kill
    observation=若 AIR-330 定稿詞彙與本 EP 凍結值衝突且我方讓步會破
    D7 硬閘語義 → 升級討論（兩契約並存版本化），不在 S4 前單方改詞。
- negative 宣稱結構化：研究階段「唯一寫入點／唯一 git diff 面」類
  宣稱已由 cr-research 以 CR callers＋rg 雙腿取得並附錨點（dossier
  §1）；implement 段再補 `\.meta\.json` 外部拼路徑掃（research 盲區
  聲明在案）。
- callstack 菜單積壓：`ai-analysis/blueprint/callstack-plan.md` 不存在——跳過。

## 段落 S2：(a)+(b) manifest 生產＋freshness delta 輸出

### Context

- UC：Stale-graph delta disclosure。依賴：無（本 EP 首個碼段）。
- 語義約束：與 S3 共享 manifest 側檔 schema（D2）；與 S4 共享 reason
  enum（D15）。S2 不得輸出 S4 欄位（tier/exhaustive）——半套詞彙禁令。
- 基礎設施盤點：D3 重構點（identity.rs:302-382、engine.rs:1245-1297）、
  stamp 單點（engine.rs:1333）、validated keys（:1416-1427）、
  identity-cache 原子寫範式、測試縫（support::git_init＋
  source_identity.rs:624-642 多 commit 先例）。
- 依賴錨點：`compute_identity_scoped` 定義 identity.rs:302 / 消費
  engine.rs（validated 路徑）；`stamp_meta_locked` 定義 engine.rs:1333 /
  消費 refresh.rs:123、cli.rs:469；`load_meta` 定義 engine.rs:1439 /
  消費 evaluate_staleness、source_line。
- 需求邊界繼承：無 spec——「不做」見 non-goals 節。

### 1b. Invariant Impact

- 受影響 invariant：**stamped provenance 完整性**（meta/manifest 與
  index bytes 的綁定——silent bad-provenance relay 是 2026-08-28 已修
  病灶類；preserve 契約＝新事實來源不得被 head-sync 靜默丟棄）。
- critical path：stamp 寫入面（build 出版物＝下游 freshness/查詢的
  信任根）。
- 驗證對齊：TC-1/TC-7/TC-12（綁定攻擊＋生命週期）＝本段 invariant 的
  producer 自證。

### 核心實作要點

1. **D3 重構**：`compute_identity_scoped` 改回傳
   `{ identity: Identity, manifest_entries: Vec<Entry>, rust_inputs_digest,
   topology_hash }`（Entry=face/rel/size/hash；排序＝既有 identity
   輸入序）。`ProductionIdentity::capture` 同趟收集並攜出。
   Byte-determinism 由 rust_identity_v2 既有測試釘。
2. **側檔出版**：validated stamp 路徑在 slot 旁以 tmp+rename 原子寫
   `index.scip.manifest.json`（D2 schema；schema_version=1）。手動
   `--stamp-meta`（cli.rs:469）無 validated A 時不產 manifest（delta
   降級 legacy-no-manifest——restamp 不造假 provenance，對齊既有
  「manual restamp 不重算 current source」原則）。head-sync
  （refresh.rs:123）不觸碰側檔。
3. **freshness delta 輸出**：`evaluate_staleness` 加載並驗證 manifest
   綁定 → delta 計算（stamped faces ∪ rust-inputs scope；detected-only
   → out_of_scope；policy/topology → 旗標）。JSON additive：
   `delta:{changed/added/removed 計數＋bounded path list take 50,
   delta_complete, content_verification, out_of_scope_count,
   policy_drift_flags}`＋`delta_identity`＋`delta_unavailable_reason`
   （D15 enum；欄位恆在）。`--delta-full` flag＝完整枚舉。
4. **降級語義**：manifest 缺席/綁定不符/walk Err → D15 reason；walk
   Err 保持 exit 2（D15 fail-loud——部分枚舉禁冒充完整）。
5. **lsp_harvest 共存**：不修改該 script；其覆寫 meta 後綁定失敗自然
   降級（TC-12）。`\.meta\.json` 外部消費者補掃入 implement 檢查清單。

### 驗證策略

- RED 先行：TC-1/2/3/7/8(scip 側)/12/14(freshness 面)＋muse 測試矩陣
  組 1-4、9、11（綁定攻擊/cache 殘留/scope 邊界/dirty-at-build/生命週期）。
- 機驗：`cargo test` 全量綠；`code-reality freshness --repo . --json`
  實跑（本 repo 自食，NT-scale）；`uvx` 消費端 face 對照不變
  （additive 欄位）。
- POC：K1 spike（首日；budget 2-3 probes；UNKNOWN=顯性裁決不滑入）。

## 段落 S3：graph publication provenance

### Context

- UC：更新 Self-owned graph db build。依賴：S2（manifest 在場才可記
  `source_manifest_digest`；缺場記 null＋建號照常）。
- 語義約束：與 S4 共享 graph_snapshot_id 定義（D10）；metadata 讀取
  必須同 conn（D11）。
- 基礎設施盤點：metadata DDL（graph_db.rs:175）、producer 戳（:898）、
  stamp_snapshot_metadata（:401-415）、publication tmp+rename（~:910）。
- 依賴錨點：`graph_db build` 出版路徑 graph_db.rs:757-777,897-915 /
  消費 graph_engine::open:47-56（現不讀 metadata——本段不改讀，S4 改）。

### 核心實作要點

1. build（含 graph_db build 與資料面 build 的 graph 腿）在同一 tx 內
   stamp：`graph_snapshot_id`（D10 定義式）、`input_index_sha256`、
   `source_manifest_digest`（S2 側檔在場時；否則 null）、
   `builder_revision`（build 當下 CR_BUILD_REV）、`coverage_faces`、
   `selection`。
2. git 失敗不再靜默跳過 head（現 :401-415 `if let Ok`）——builder
   revision 來自編譯期常數不受 git 影響；`git_head_sha` 維持現狀
   （display 軸），不得作 provenance 鍵（D10）。
3. legacy db（無 key）語義由 S4 消費（indeterminate）；本段只管出版。

### 驗證策略

- TC-11＋muse graph 形狀組 8（legacy null）；concurrent old/new
  generation 對帳測試（開舊 conn 後重 build→舊 conn metadata 不變）。

## 段落 S4：(c) 查詢面標註＋negative gate＋MCP freshness tool

### Context

- UC：Per-query freshness annotation＋negative gate。依賴：S2（delta）＋
  S3（graph provenance）。
- 語義約束：詞彙凍結（D6/D7/D15）；標註只從 serving 本體派生（D11）；
  scip_refs byte-parity golden 更新＝本段明示決策。
- 基礎設施盤點：三 format 的 src_line 注入點（engine.rs:350-380、
  callers.rs:151-214）、graph_query choke（graph_engine.rs:2514-2525）、
  symbols early-return（:2413-2428）、get_community MCP 直呼
  （mcp_server.rs:821-855）、to_tool_result/map_tool_output
  （:240-274）、apply_text_cap（:220-234）、s6 釘數兩處。
- 依賴錨點：`evaluate_staleness` 定義 engine.rs / 消費 freshness.rs:87
  與（本段新增）查詢路徑共享 verdict 計算。

### 核心實作要點

1. **verdict 共享計算**：查詢路徑入口算一次（evaluate＋delta 摘要，
   cache 命中近零邊際）→ ServingStamp 傳入組裝點；per-op 禁重算
   （muse Q4 成本條）。graph_query 於 run() 入口取 pinned conn 的
   metadata（D11）。
2. **scip_refs [CR_VERDICT] 行**（D8）：三 format 統一第二行；資料出自
   verdict 計算（非 source_line）。negative gate（D7）：stale inbound
   空 → 結果段落以 refusal 形輸出（文字面：[CR_VERDICT]
   negative_verdict=unavailable＋非零 exit；engine::report 查無 DEF
   既.exit1 語義區分「基準內查無」vs「現行不存在」——golden 更新）。
3. **graph_query [CR-META] 頭行**（D9）：choke point 統一注入＋symbols
   early-return 同點補頭行＋get_community 經共享 helper；gq_fail 無行
   ＝契約（不推論 fresh）。inbound 族 stale＋空 → typed refusal
   envelope（result null；不產可讀 `[]`）。
4. **MCP freshness tool**（D12）：fresh/stale typed success；s6 釘
   21→22；digest 變更 commit 獨立（段尾）。
5. **truncation fail-closed**（D13）：頭行制下保證標註存活；cap 內
   budget 控制序列化；negative/complete 結論被截 → TRUNCATED_UNVERIFIED
   error。
6. **TOCTOU pinning**（D11）：ServingStamp（scip）＋同 conn metadata
   （graph）；binding mismatch → bounded retry 一次 → indeterminate。

### 驗證策略

- TC-4/5/6/9/10/13＋muse 矩陣 5/6/7/10＋codex 矩陣 ⑥⑦⑧⑨⑩。
- scip_refs golden 更新以「輸出契約 v1」凍結檔案承載（契約節引）；
  byte-parity suite 對新 golden 綠。
- MCP 契約組：is_error 對照表（fresh=false/stale=false/fail=true）＋
  cap 前後 1 byte。

## 段落 S5：跨 repo acceptance hand-off

### Context

- 依賴：S2-S4 全綠。產出＝契約 fixture（輸出範例＋詞彙表 frozen 檔）＋
  兩封信草稿（bridge：評估結果＋開卡指針＝本 EP；ai-guide：AIR-330
  詞彙對齊請求）。**寄信=outward，逐次 AUTH**；CR suite 綠 ≠ bridge
  default enable（codex Q6 第 4 段照抄）。
- 驗證：fixture 與實作輸出逐字一致（機驗）；信件經 user AUTH 後寄。

## 整合策略

- baseline: `1b0ba1e62b51efd8a249564e7431f9d113e0a80e`
- 收線 scope diff 核對：`git diff baseline..HEAD` 對照授權 scope
  （S2-S4 檔案清單＋卡 references）；段間 commit 邊界＝S2／S3／S4 主體
  ／S4 MCP digest（獨立尾 commit）。
- 下游消費者：bridge DB-120/DB-101（供給閘）、ai-guide AIR-330（紀律
  措辭）、plugin code-reality-tools SKILL.md（consumer contract 文檔
  ——instruction 面，S4 內同步、走 instruction-writing 紀律）、
  lsp_harvest.py（不動，降級承接）。
- author_family: glm（同 family precondition＝RED 前 challenge：
  主 session 以本 TC 節為 frozen oracle 對 writer 測試做 blind
  derive→reveal 比對）。

## 收尾步驟

1. AGENTS.md Capabilities：四行更新＋新能力兩行（消費場景自 SM 提煉，
   自包含描述）；plugin skills/code-reality-tools SKILL.md 契約節
   （freshness/delta/CR-META/CR_VERDICT 詞彙——與 AIR-330 對齊後措辭）。
2. 卡結案兩步＋bounded child 卡（S2/S3/S4）逐段結案；弧結案蒸餾
   照會 ai-guide 池。
3. instruction 更新：crates/AGENTS.md 若有 family 條目同步。
4. /audit-test 對新增測試套件稽核。
5. release 弧（版號鎖步＋plugin pin 對齊——digest 變更消費者流程）另
   卡承接。

## non-goals

bridge DB-101/DB-120 契約與詞彙；ai-guide AIR-330 措辭本體；
scope-declaration escape（bridge ship order 6）；git tree object 寫入
消費者 repo；autoheal 行為變更；cr-freshness leaf crate（binary 版本
軸——不同面）；舊 graph generation 的 manifest 保留/GC（fixed-name
失配→indeterminate 已 fail-closed；digest retention 留未來優化）；
mosaic 47 檔遷移；pyrefly 上游 panic（另卡追蹤）。

## EP Review 區段（規劃期帳本）

（EP Review Cycle findings 與判決記錄於此——pending）
