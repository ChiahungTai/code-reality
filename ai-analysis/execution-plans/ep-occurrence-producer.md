# EP: Occurrence Producer — Python graph data at minutes, not hours

> **ep_type**: implementation
> baseline: a496f030c2a868119b86a63a1dfc25ddc7d41031

## North star

Python repos' graph data is produced by code-reality's own
occurrence-based producer: a full mosaic harvest runs in **minutes**
(scip-python measured pace ≈160× vs lsp_harvest's 6.7h), the graph is
no longer frozen at commit 47033594, and the producer eventually
supplies a **CALLS-edge source of its own** — the precondition for
`import_legacy` retirement (mosaic today: CALLS 78,789 edges 100%
`treesitter-legacy`, LSP face emits only REFERENCES).

Root cause being fixed: `lsp_harvest` per-def references is
N_defs × workspace-scan (1.67s/def × 14,475 defs ≈ 6.7h, 500-def
sample 834s; downstream `graph_db build` is 8.35s — the bottleneck is
purely the producer). An occurrence index inverts this: one pass
yields all references (M, not N×M).

## 裁決已定案（2026-08-27，本 EP 的規格輸入——不重辯）

1. **方向**：occurrence index（外部雙意見共識＋160× 實證），與 Rust 側
   SCIP 對稱，`scip_refs`/`graph_db build` 管線現成。
2. **scip-python 0.6.6 fatal 定性**：`model/utils.py` 上
   `assignClassToProtocol` **無界遞迴**（fork 內嵌舊 pyright）。
   `--stack-size` 繞法死路：NODE_OPTIONS 拒收；直接 node 呼叫
   32MB stack → SIGSEGV（exit 139、零 RangeError）。上游無現成 issue、
   0.6.6 已是最新、pyright 基底舊（`pyright-last-sync` 落後）。
3. **部分輸出可用**：13MB probe（sidecar
   `scip/mosaic_alpha_offline_backtesting/index-probe.scip`）經我方
   vendored protobuf 完整解析——378 docs / 123,642 occurrences /
   20,760 symbols / 21,422 reference-role。
4. **拒絕項**：移除全量 didOpen 會漏 references（pyright#10086）；
   pyright 無磁碟快取共享（cacheDirectory 是 basedpyright）。

## 路線裁定（本 EP 決策，附理由）

- **(a) 最小重現＋上游回報**：S2 並行，不佔關鍵路徑。
- **(b) 修 fork 為主線**（rebase pyright 基底或加 recursion guard）：
  CALLS 級邊需要**語義**解析——自建詞法 exporter 只能給
  REFERENCES 級，無法支撐 import_legacy 退場判準。scip-python 本身
  是 additive fork（"no substantial changes to the pyright
  library"），rebase/patch 工程量可控。
- **(c) 全自建語義 exporter**：等於重寫 pyright，**否決**（記錄否決
  理由防再議）。詞法掃描只作 S4 過渡增量的組件，不作 CALLS 來源。

## UC 盤點

### Backlog 關聯
- `.kanban/Backlog/` 空 → 建卡（EP 追蹤卡；無獨立新增消費能力）

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過）

### 掃描範圍
- root `AGENTS.md` Capabilities（`Python symbol truth via LSP harvest` 行掛「productization follow-up」）、`crates/AGENTS.md`、README

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Self-owned graph db build | ✅ | AGENTS.md Capabilities | 更新 | 新增 occurrence producer 面（SCIP face 原生相容） |
| Python symbol truth via LSP harvest | ✅（follow-up） | AGENTS.md Capabilities | 更新 | 本 EP 交付產品化；lsp_harvest 降為過渡/fallback 面 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Occurrence-based Python producer（分鐘級全量） | 📋 | scip-python（fixed fork）→ sidecar slot → `graph_db build` SCIP face |
| Harvest 增量模式（過渡） | 📋 | `scripts/lsp_harvest.py` 詞法掃描＋dependents 閉包 |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | mosaic 全量 harvest | routine 刷新 | 分鐘級完成（≤10min 門檻），defs/references 對 golden corpus 達標 | 無 | occurrence producer |
| SM-2 | scip-python fatal 檔案在 corpus 內 | model/utils.py 類深 protocol | patched fork 不崩，完成全量索引；或 loud 失敗附最小重現 | S2 | 同上 |
| SM-3 | 程式碼小改動後刷新 | mosaic 日常弧 | 增量模式分鐘級：changed defs 重查＋詞法掃描發現新引用＋dependents 閉包重查——無漏（over-approx 只多查） | S4 | 增量模式 |
| SM-4 | 新增引用外部 def（增量漏洞 D） | 檔 D 新引用既有 def | 詞法掃描 identifiers ∩ defs 名單 → D 進重查集 | S4 | 同上 |
| SM-5 | 語義 invalidation（增量漏洞 C） | 基類改變使第三檔解析漂移 | dependents 閉包（保守 over-approx）納入重查；漏網風險顯式記錄＋週期全量 checkpoint 兜底 | S4 | 同上 |
| SM-6 | scope 對齊 | scip-python 只 index project 608 檔 vs lsp_harvest 1,387（tests/scripts） | 對帳差異清單；indexer 設定補齊或差異顯式接受 | S3 | occurrence producer |
| SM-7 | CALLS 覆蓋率對帳 | 每次全量後 | 新 producer CALLS vs legacy CALLS **pair-set** diff（grain 見 S5/F6）→ 達標門檻列冊 | S5 | import 退場判準 |
| SM-8 | env 解析靜默退化（venv/import 未解→ occurrence 集變小不崩） | 索引完成 | 索引時 assert import-resolution 健康（pyright diagnostics 面或 defs 覆蓋率下限），低於下限 loud | S3 | occurrence producer |
| SM-9 | partial index（fatal/中斷後殘檔） | 索引輸出 | doc 數 vs repo 檔數對帳，不足 loud（與 SM-6 scope 差異分開歸因） | S3 | 同上 |
| SM-10 | slot cutover（舊 lsp cache 在場） | 放入 scip-python index | 明確 evict/migrate 步驟＋meta producer 標記切換；不允許靜默用舊 face | S3 | 同上 |

## 段落 0：全域研究摘要（2026-08-27 實測，路徑已驗證）

- **可複用基礎設施**：`graph_db build` 的 SCIP face（spans 歸屬
  `callers::attribute`）原生吃 SCIP 格式——scip-python 輸出同格式，
  S3 轉接器工程量集中在 symbol 慣例對齊與語言標記，不是新管線。
  `cache.rs` 三表 schema、sidecar slot 機制（basename-keyed）、
  `scip_refs`、`is_test_rel` 現成。
- **golden corpus（對帳 oracle）**：mosaic dogfood cache
  （14,475 defs / 640,976 sites）＋ daily.py 四 execute baseline。
  對帳指標：per-symbol references multiset、defs 覆蓋率。
- **風險假設**：
  - R1（高，S2）：patched fork 的行為等價性——rebase 後 pyright 行為
    差異可能改變 occurrence 集合。以 golden corpus 對帳把關。
  - R2（中，S3）：symbol 慣例——scip-python 的 symbol 形態 vs
    `lsp python <rel> [L<line>] <name>().` 慣例（fn_tail_name 相容
    雙形態）。resolved-name 品質差異（pyright 對 dynamic dispatch 的
    局限）對帳時顯式歸因。
  - R3（中，S4）：詞法掃描 over-approx 的召回/成本曲線（ identifiers
    太common → 重查集爆炸）。
  - R4（低）：scip-python 上游若有後續 release，patch 面收斂——S2 的
    最小重現回報是為此鋪路。
- **非風險**：資料面——probe 已證我方 protobuf 解析、partial index
  資料完整（fatal 前的檔全在）。


## EP Review（雙軸獨立審查 2026-08-27，findings 已回寫）

| # | Finding | 裁決 | 落點 |
|---|---------|------|------|
| F1 🔴 | `infer_language` 只認 `lsp ` 前綴——scip-python symbol（`scip-python python ...`）全被標 **Rust**（graph_db.rs:235-241，probe 實抽 symbol 證形態） | ✅ 採納 | S3 必修項（副語言前綴表） |
| F2 🔴 | 無段交付 CALLS 邊產出路徑——S5 退場判準依賴一個沒人建造的能力（cache 只存 is_def；build 只寫 REFERENCES） | ✅ 採納 | S3 擴 scope：occurrences 加 call-role 欄位＋build 邊 kind 推導 |
| F3 🔴(結構軸) | fn_tail_name `().` 尾濾掉全部 scip-python symbol | ❌ **不採納——被實證否證**：正確性軸抽 probe symbol（`` `pkg`/fn(). ``、`Class#method().`）帶 `().` 尾可通過（engine.rs:93-108）；兩軸衝突以實抽證據裁決 | 記錄（防再議） |
| F4 🟡 | 同檔同名 def 碰撞：scip-python symbol 無 `L<line>` 消歧段（LSP face 有）→ 同 module 同名 def 塌縮為一節點 | ✅ 採納 | S3 對帳項：probe 驗碰撞率，必要時 symbol 加行號消歧（轉接器職責） |
| F5 🟡 | sidecar slot lsp-cache 優先短路（graph_db.rs:414-430）：舊 `index.scip.db` 在場時 scip-python index.scip 被靜默忽略；S4 與 S3 同槽競爭 | ✅ 採納 | S3 明確 cutover 步驟（evict/migrate 舊 lsp cache＋meta 記 producer 切換）；S4 註明槽位策略（增量期 lsp 槽、 occurrence 期 scip 槽，不混用） |
| F6 🟡 | S5 multiset diff grain 未定義：legacy CALLS 54% dangling qname 端點無 symbol 映射、site grain 語義不同——原判準不可證偽 | ✅ 採納 | S5 改判準 grain：**(caller_key, callee_key) pair-set**（legacy qname 經節點 (file,name) 鍵表映射），site multiset 為正規化次要指標 |
| F7 🟡 | S2「additive fork」是上游 README 宣稱未驗證；patch 面可能大到形同重寫；無 timebox/fallback | ✅ 採納 | S2 加 **bounded patch-surface spike**（先 clone＋diff `pyright-last-sync` 基底量測 patch 面，超過門檻即降級主線——fallback＝S4 增量為唯一日常面＋等上游） |
| F8 🟡 | 缺 SM：env 解析靜默退化（occurrence 集變小不崩）、partial index 偵測、slot cutover | ✅ 採納 | 新增 SM-8/9/10 |
| F9 🟡 | S4 cache merge 無冪等設計：occurrences plain INSERT 無 upsert（cache.rs:103-111）→ 增量重查雙計汙染後續 build | ✅ 採納 | S4 設計定案：**partition rewrite**（受影響 (symbol, rel_path) 分區事務重寫，非增量 INSERT）＋移動/刪除的 stale-site 清理範圍顯式化 |
| F10 🟡 | S1 oracle 未套 workspace filter——stdlib/builtins 引用會讓對帳數字無意義 | ✅ 採納 | S1 對帳口徑：先套 scip_edges workspace filter 再比 |
| F11 ℹ️ | `daily.py` baseline 不在本 repo | ✅ 採納 | S1 註明位於 mosaic repo（dogfood 現場） |

## EP Review Round 2（fresh-eyes 獨立審查 2026-08-28，findings 已回寫）

錨點核對：cache.rs:23-31/102-111、graph_db.rs:235-241/414-430、
engine.rs:93-108 全數相符；`build_from_cache_at` 實際在 graph_db.rs:**401**
（EP 原寫 382，已修）。實查修正一條審查前提：**scip_refs.py 已於 R7 退役**
——cache.rs 開頭的 frozen-Python interop contract 現存消費者只剩
`scripts/lsp_harvest.py`（同 DDL 五欄寫入，lsp_harvest.py:216-218）。

| # | Finding | 裁決 | 落點 |
|---|---------|------|------|
| R2-1 🔴 | S3「occurrences 加 call-role 欄位」撞 cache interop contract：lsp_harvest 五欄 positional INSERT 立刻斷；schema 演進策略未規劃 | ✅ 採納 | S3：call-role 走**新 side table**（`occurrence_roles(seq, role)` 以 seq 外鍵）不動原表；schema 變更影響面（lsp_harvest DDL 同步）列 S3 驗收項 |
| R2-2 🔴 | **速度宣稱未實證（user 指示確認）**：160× 是 partial 377 檔外推（fatal 前產物），patched fork 全量時長未驗證、可能超線性；本地無 scip-python clone/安裝 | ✅ 採納 | S2 驗證段強化：全量時長**必須實測**（`time` 全 corpus 索引），≤10min 門檻是量測值非外推；EP 註明 160× 來源為 partial run |
| R2-3 🟡 | class symbol（`` `pkg`/DataFrame# `` 尾 `#` 無 `().`）被 fn_tail_name 閘門在 cache ingest（cache.rs:65,100）與 scan（graph_db.rs:293,313）兩層全濾——constructor call 進不了 S5 CALLS 分母，class-heavy corpus 退場門檻恐不可達且歸因錯誤（lsp_harvest 只收 kind 3/6/12，REFERENCES 面平價，但 CALLS 面缺口） | ✅ 採納 | S3 對帳項加：量化 class-symbol 佔比與 constructor-call 漏損；S5：constructor 邊單獨列冊（不混入覆蓋率分母），gate 延伸（`#` 尾）列為必要時選項 |
| R2-4 🟡 | slot cutover 機制層級不明：cache db 的 `producer` key 只有 lsp_harvest 寫（lsp_harvest.py:240），`cache.rs build_db` 不寫 → scip-python cache 落 `producer_of` fallback "scip"；且 SCIP spans ladder 依賴 fndefs sidecar（graph_db.rs:443）——前置未提 | ✅ 採納 | S3：cutover = evict 舊 slot 檔＋（可選）cache.rs build_db 寫 `producer='scip-python'`；驗證 `producer_of` 歸類；fndefs sidecar 生成列入 S3 步驟 |
| R2-5 🟡 | producer 側新增 CALLS 邊後下游未盤點：snapshot.rs:87 文檔預期失效、graph_audit provenance、MCP/graph_query 第二 CALLS 來源去重 | ✅ 採納 | S3 受影響模組清單（snapshot 文檔、graph_audit、graph_query/MCP 邊過濾），各加回歸驗證 |
| R2-6 🟡 | S4 partition rewrite 違反 scan-order insertion 契約（seq 序）——但 byte-parity 消費者 scip_refs.py 已退役，契約現況縮窄為 lsp_harvest DDL 相容 | ✅ 採納（縮窄版） | S4 設計註記：增量 cache 不承諾 scan-order byte-equivalence；下游若依賴輸出序需確認；隔離增量槽為預設選項 |
| R2-7 ℹ️ | S1 golden corpus 位置未定案；640,976 sites 入 repo 體積可觀 | ✅ 採納 | S1 定案：sidecar（`~/.mosaic` 同域），格式凍結點記錄 |
| R2-8 ℹ️ | S1/S2 腳本與 patch 檔形態未指明（命名/位置/英文/uv run） | ✅ 採納 | S1 對帳腳本入 `scripts/`（`demo_`/tool 命名＋`uv run`）；S2 patch 檔與上游 issue 英文（public repo 約束） |
| R2-9 ℹ️ | `build_from_cache_at` 行號 drift（382→401）；錨點建議 symbol 優先 | ✅ 採納 | S3 依賴錨點已修 |

## EP Validate Findings（2026-08-28 POC 實測）

| ID | 嚴重度 | EP 段落 | 問題(POC 結果) | 建議 | 狀態 |
|----|--------|---------|----------------|------|------|
| V1 | ✅ | S2 | POC#1 通過：真 patch 面＝**45 檔 +1,158/−393**（vendored src vs upstream pyright **1.1.301 tag**，約半數 tests/samples）——「additive fork」宣稱實證成立。注意：`pyright-last-sync` 標記（2022-07 snapshot）**不是**實際同步基底，量 patch 面必須對 upstream version tag | spike 門檻以此為基準（<~50 檔/+1.5k 行） | verified |
| V2 | ✅ | S2 | POC#2 通過：fatal 重現（90s、exit 1、`model/utils.py` RangeError，full stack：typeEvaluator.ts overload → protocols.ts:186 `mroClass` forEach → :175）。vendored 1.1.301 已有 `recursionCount` cap＋`protocolAssignmentStack` 同-(src,dest) 環偵測——崩潰路徑是**經其他 frame 的深遞迴**（native stack 耗盡），非同 pair 環 | S2 patch 設計：mroClass forEach 內成員比較的深度傳遞或全域 depth guard；最小重現輸入已有（stack log 入 `.agent-tmp/mosaic-run2.log`） | verified |
| V3 | ✅ | S2 | POC#3 通過：npm 安裝 `@sourcegraph/scip-python@0.6.6`（**scoped 名**，bare `scip-python` 404）；pace 實測 152/552 檔 @90s ≈ 0.59s/檔 → 全量投影 ≈5.4min（≤10min 門檻內，仍為外推——S2 patched 全量實測為準）。**operational**：workspace 解析以 **cwd** 為準（arg 目錄不作 workspace root，誤用會靜默 index 錯 repo、exit 0）——S2/S3 必須以目標 repo 為 cwd 執行 | S3 接線文件化 cwd 要求；wrapper 腳本加 assert | verified |
| V4 | ⚠️ | S3/SM-9 | 附帶發現：fatal 時 **partial index 照樣寫出**（12.98MB、exit 1）——「exit code 才是失敗訊號，檔案在場不代表成功」；SM-9 的 doc 數對帳必須搭配 exit code 檢查 | S3 增：消費前 assert 產出者 exit 0（包裝層職責） | implemented |
| V5 | ℹ️ | S3/SM-6 | mosaic 實測 project files=**552**（pyrightconfig.json 生效），EP 原記 608——scope 對帳基數以實跑 log 為準 | SM-6 對帳用 552（+tests/scripts 差異說明） | implemented |

## 段落 S1：golden corpus 對帳 harness

**Context**：任何新 producer 的驗收都靠 oracle——先把 dogfood cache
提煉成可機械對帳的 baseline（per-symbol references multiset＋defs
清單），存 `.agent-tmp` 之外的正式位置（`ai-analysis/` 或 sidecar，
S1 段內定）。UC 引用：支撐兩個 📋 能力的驗收。
- 依賴：無（首段）
- 基礎設施：dogfood cache（mosaic repo sidecar slot）、python 腳本形態
- 依賴錨點：cache 三表 schema → `crates/code-reality/src/cache.rs`；
  dogfood cache → `~/.mosaic/code-reality/scip/mosaic_alpha_offline_backtesting/index.scip.db`
- **R2-7 定案**：golden corpus baseline 存 **sidecar**（`~/.mosaic` 同域，
  不入 repo——640,976 sites 體積），格式凍結點（per-symbol references
  multiset＋defs 清單的序列化形態）記入結算段
- **R2-8**：對帳腳本入本 repo `scripts/`（工具命名＋`uv run` 執行；
  public repo 英文內容約束）

**驗證**：對帳腳本跑 dogfood cache 自身（self-consistency）；輸出
差異報告格式凍結（S3/S5 消費）。對帳口徑：先套 scip_edges 的
workspace filter（stdlib/builtins 引用剔除）再比（F10）。
golden corpus 的 daily.py 四 execute baseline 位於 **mosaic repo**
（dogfood 現場，非本 repo——F11）。

## 段落 S2：scip-python fatal 修復（fork patch 主線）＋上游回報

**Context**：(b) 主線，**前置 bounded spike（F7）**：clone scip-python →
diff `pyright-last-sync` 基底量測 patch 面——「additive fork」目前是
上游宣稱、未驗證；patch 面超過門檻（spike 段內量化的 diff 行數）
即降級主線，fallback＝S4 增量為唯一日常面＋等上游修復。spike 通過後：
定位 `assignClassToProtocol` 遞迴（protocols.ts）→ 兩手：
(i) recursion guard/depth cap patch；(ii) 試 pyright 基底 rebase。
任一手通 → mosaic 全量索引實測（≤10min 門檻）。
同段並行：(a) 最小重現（model/utils.py 縮到單檔）＋上游 issue
（fatal 的 stack trace 已在 2026-08-27 實測 log，spike 時入 repo）。
UC 引用：occurrence producer 的前置。
- 依賴：S1 的對帳 harness（驗證 patched 行為等價）
- 風險：R1 行為等價性——patch 後 occurrence 集合對 golden corpus
  對帳，差異逐項歸因（patch 造成 vs 本來就有）
- 產出：patched scip-python（本地 clone＋patch 檔入 repo `scripts/`）
  或 pinned 已修版本；mosaic 全量 index.scip

**驗證（2026-08-28 實測結算）**：
- **fatal 修復達成**：patch 兩件套——(1) protocols.ts 全域
  `protocolAssignDepth` guard（cap 64、bail=false 保守語義；
  **guard 置於 recursionCount early-return 之後**——先於它會在該
  bail 路徑洩漏 depth、單調退化語義，code-review F5 抓到後修正）
  解 RangeError；(2) indexer.ts emit-skip（per-file internal error 記錄＋
  跳過＋續行＋結尾 loud 清單）。patch 檔：
  `scripts/scip-python-mosaic.patch`（vs v0.6.6 tag）；上游 issue 草稿：
  `scripts/scip-python-mosaic-upstream-issue.md`
- **全量時長實測：24.9s**（≤10min 門檻的 1/24；vs lsp_harvest 6.7h ≈ 960×）
- **partial index：11/552 檔 skipped**（loud 列冊；缺口 defs 集中於此）
- **R1 對帳（name-normalized def 覆蓋）**：mosaic_alpha/ 側
  3,511/3,707 = **94.7%**；golden-only 196 高度集中在 skipped 檔
- ⚠️ **references 密度語義差距**：fn-tail gate 後 workspace refs 僅
  ~7.7k vs lsp golden 641k（~20×）——pyright SCIP emitter 的
  reference-role 本質上比 LSP references 稀疏（屬 producer 語義，
  非 patch 造成）；S3/S5 對帳須顯式歸因，REFERENCES 面覆蓋率
  預期顯著低於 lsp golden
- ⚠️ **本地 build 環境差異未解**：同一 v0.6.6 source，本地 webpack
  build 產生 31 個 `getVariance` Debug Failure（npm dist 零個；
  Node16 build 亦然）——已由 emit-skip 圍堵（11 檔），歸因待查
  （CI build vs 本地差異）
- **operational（V3 再確認）**：workspace 以 cwd 解析；產出前必須
  檢查 exit code（fatal 時 partial index 照樣寫出）
- patch 檔與 issue 內文英文（public repo 約束，R2-8）✅

## 段落 S3：occurrence index → graph_db build 接線

**Context**：scip-python 輸出放 sidecar slot → `graph_db build` SCIP
face 消費。工程點（review 後擴）：
- **F1 必修**：`infer_language` 副語言前綴表（`scip-python ` 前綴 →
  Python；現行只認 `lsp ` → 全標 Rust 的 schema 汙染 bug）
- **F2 CALLS 產出路徑**：occurrences 的 call-role 走**新 side table**
  `occurrence_roles(seq, role)`（seq 外鍵關聯，R2-1）——不動 occurrences
  原表（lsp_harvest 五欄 positional INSERT 依賴原 schema）＋ build 邊
  kind 推導（CALLS vs REFERENCES，graph_db.rs:543 硬編碼 REFERENCES 處）
  ——S5 判準的前置，無此則判準不可達。schema 變更影響面
  （lsp_harvest DDL 相容性）列為本段驗收項
- **R2-3 class 符號漏損**：scip-python class symbol 尾 `#` 無 `().`，
  fn_tail_name 閘門（engine.rs:93-108）在 cache ingest（cache.rs:65,100）
  與 scan（graph_db.rs:293,313）兩層全濾——constructor call 進不了
  CALLS 分母。probe 對帳 class-symbol 佔比與 constructor-call 漏損；
  必要時閘門延伸（`#` 尾 class 形態）為顯式選項
- **F4**：同 module 同名 def 碰撞（scip-python symbol 無行號消歧）——
  probe 對帳碰撞率，必要時轉接器加行號消歧段
- **F5 slot cutover（R2-4 具體化）**：evict/migrate 舊 lsp cache
  （`index.scip.db`/`.meta.json`）＋（可選）`cache.rs build_db` 寫
  `producer='scip-python'`（現只寫 head/schema/tool，cache.rs:119-121）；
  驗證 `producer_of`（graph_db.rs:243-258）對新值歸類正確——勿落
  "scip" fallback 誤配 spans ladder；**fndefs sidecar 生成列入本段
  步驟**（spans 歸屬依賴，graph_db.rs:443）
- **R2-5 受影響模組**（producer 新增 CALLS 邊的下游）：snapshot.rs:87
  「REFERENCES-only」文檔預期更新、graph_audit provenance 邏輯、
  graph_query/MCP 邊過濾（與 treesitter-legacy CALLS 共存的去重）——
  各加回歸驗證
- SM-8/9 接線（env 健康 assert、partial index 偵測）
- scope 對齊（608 vs 1,387——indexer 設定或顯式接受）
UC 引用：交付「Occurrence-based Python producer」。
- 依賴：S2 全量 index；S1 對帳
- 語義約束：與既有 SCIP face 共用歸屬邏輯（spans→attribute），
  不 fork 歸屬演算法
- 依賴錨點：`build_from_cache_at` → graph_db.rs:401（R2-9 已修）；SCIP face
  spans 歸屬 → 既有實作

**驗證**：mosaic build＋import 全鏈（與 dogfood baseline 對帳：defs
覆蓋、references multiset 達標門檻 S1 凍結）；`graph_query` 煙霧；
NT（Rust）不受影響回歸。

**進度（2026-08-28）**：F1 ✅（infer_language 前綴表＋unit test
`graph_db::tests::python_prefixes_cover_both_python_producers`）。
F2/F4/F5/SM-8/9/R2-3 未做——接續點見 repo root STATE.md。

## 段落 S4：harvest 增量模式（過渡，可提前獨立）

**Context**：lsp_harvest 加 `--since <ref>` 模式：changed defs 重查＋
詞法掃描（tokenize identifiers ∩ defs 名單→新引用檔進重查集）＋
reverse import dependents 閉包（保守）。over-approx 只多查不漏；
語義 invalidation 漏網風險（基類改變→第三檔漂移）顯式記錄，
週期全量當 checkpoint。UC 引用：交付「Harvest 增量模式」。
- 依賴：無硬依賴（**可被拉前獨立先行**——mosaic 下次大弧前需要時）
- 風險：R3 召回/成本曲線
- 語義約束：**F9 cache merge 定案為 partition rewrite**——受影響
  (symbol, rel_path) 分區事務重寫（非增量 INSERT；occurrences 表
  無唯一鍵，plain INSERT 會雙計汙染後續 build）；def 搬移/刪除的
  stale-site 清理範圍＝分區刪除＋閉包重查；**F5 槽位策略**：增量期
  用 lsp 槽，切 occurrence 後不再混用；**R2-6 scan-order 註記**：
  byte-parity 消費者（scip_refs.py）已隨 R7 退役，partition rewrite
  不再破壞既有契約——但增量 cache 不承諾 scan-order byte-equivalence，
  下游若依賴輸出序需顯式確認；隔離增量槽為預設選項

**驗證**：模擬小改動（改 1-2 檔）→ 增量跑 → 與全量結果對帳（增量
結果 ⊇ 真實變更影響）；時長實測（目標分鐘級）。

## 段落 S5：CALLS 覆蓋率對帳＋import_legacy 退場判準接線

**Context**：全量後產 CALLS 對帳報告（`graph_db` 新 reporting 或
scripts 面）。**判準 grain（F6 凍結）**：兩側正規化為
**(caller_key, callee_key) pair-set**——legacy qname 端點經節點
(file,name) 鍵表映射到 symbol、映射不到的 dangling 端點單獨列冊
（不混入覆蓋率分母）；site multiset 為正規化次要指標（site grain
語義差異：pyright per-site vs tree-sitter lexical callee）。
**退場判準**：pair-set 覆蓋率達標（門檻由 S1/S3 數據定，記入本 EP
結算段）→ `import_legacy` 降級為可選、文檔更新。
**R2-3 constructor 邊**：class constructor call（`DataFrame(...)` 形態）
若 S3 閘門延伸未涵蓋，則**單獨列冊、不混入覆蓋率分母**——避免門檻
被無聲壓低且差距誤歸因「pyright 語義局限」。
mosaic baseline（2026-08-27 實測，入 EP 為判準起點）：CALLS 78,789
條 100% treesitter-legacy、dangling 54% synthesized 懸掛。
- 依賴：S3
- 注意：REFERENCES-only 的 producer 不滿足此判準——CALLS 需要語義
  （路線裁定 (c) 否決的理由）

**驗證**：報告產出＋判準門檻量化；未達標時差距歸因（pyright 語義
局限 vs corpus 特性）。

## 整合策略

- 順序：S1 → S2 → S3 → S5；S4 無硬依賴可平行或提前
- 每段對帳證據寫段末；S3 完成即解除 mosaic graph 凍結（47033594）
- baseline: a496f030c2a868119b86a63a1dfc25ddc7d41031

## 收尾步驟

1. Capabilities：`Python symbol truth` 行改寫（lsp_harvest 過渡面＋
   occurrence producer 主面）＋兩個 📋 能力 ✅；Kanban 搬 Done
2. SYSTEM-MAP：無，跳過
3. instruction 檔：README prerequisites（Python repo 產生流程改寫）、
   crates/AGENTS.md、plugin SKILL
4. /audit-test
