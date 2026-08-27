# EP: Legacy DB Consumer Cutover — retire `.code-review-graph/` reads

> **ep_type**: implementation
> baseline: 94b4afbe6c9e02750c681c95349a80911456b1a9

## North star

Every tool that today opens the CRG-era `.code-review-graph/graph.db`
switches to the self-owned `.code-reality/graph.db`. When the last
consumer flips, the legacy read path retires wholesale:
`common::graph_db_path` legacy opens, `graph_db ensure_indexes`'s legacy
anchor half, `tests/crg_fixture.rs`, and the "frozen reader" doc
language. No transitional dual-read is shipped — old/new dual-run exists
only as a verification gate during build.

Why now (user directive, 2026-08-27): all consumers are our own repos, no
external users, the legacy producer is dead (the db can only grow staler),
and the fresh NT/mosaic parity baselines make this the cheapest moment.

## UC 盤點

### Backlog 關聯
- `.kanban/Backlog/` 目前為空 → 自動建卡（EP 整體追蹤卡 1 張；無獨立新增能力，各段落皆為既有能力的資料源置換）

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（此 repo 無此檔，跳過）

### 掃描範圍
- `AGENTS.md` Capabilities（root）、`crates/AGENTS.md`、`plugin/skills/code-reality/SKILL.md`

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Completeness governance (audit) | ✅ | AGENTS.md Capabilities | 更新 | 資料源舊庫→新庫 |
| Deletability safety net (hub_refs/hazard) | ✅ | AGENTS.md Capabilities | 更新 | 同上 |
| Boundary/narrative tool family | ✅ | AGENTS.md Capabilities | 更新 | chain_tour/snapshot 資料源置換＋graph_csv 退休 |
| Read-chain index maintenance | ✅ | AGENTS.md Capabilities | 更新 | legacy anchor 半邊退場 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| （無新增能力——全為既有能力的資料源置換＋舊路徑退場） | — | — |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 消費端在只有新庫的 repo 跑（如 NT post-cutover） | 正常呼叫 | 全工具從 `.code-reality/graph.db` 讀，行為與舊庫版 parity | 無 | audit/hub/hazard/chain_tour/snapshot |
| SM-2 | 新庫不在 | 任何工具呼叫 | fail-loud，指引 `graph_db build --repo`（必要時 `+ import_legacy`） | 無 | 全部 |
| SM-3 | 舊庫不在但新庫在 | 任何工具呼叫 | 正常運作（舊庫不再是任何路徑的必要條件） | 無 | 全部 |
| SM-6 | NT 真實 corpus 對帳 | snapshot/audit/chain_tour 舊版 vs 新版輸出 | byte/multiset parity（NT 既有 baseline 管線） | 無 | 全部 |

## 段落 0：全域研究摘要（Explore agent 盤點，2026-08-27）

消費端盤點（file:line 已驗證）：

| 模組 | 舊庫開檔點 | SQL 面 | 遷移類別 |
|---|---|---|---|
| chain_tour | chain_tour.rs:319-321（run :704, db :779） | nodes name/file_path LIKE/line_start ×2（:370-372, :397-399） | A 直改名 |
| graph_audit | graph_audit.rs:349（run :409, db :434） | nodes name/file_path/kind IN（:289-291） | A 直改名 |
| scip_refs --audit | cli.rs:713 → 委派 graph_audit | 同上 | 隨 graph_audit |
| hub_refs | hub_refs.rs:253（db :199） | nodes qname/parent_name/name（:257, :263） | A/B（qname 直改，解析宇宙見風險 R2） |
| hazard | hazard.rs:611（db :583） | nodes qname/parent_name/kind（:615, :620） | 同 hub_refs |
| snapshot | snapshot.rs:339/:203（db :308） | edges qname endpoints（:81）、COUNT（:114）、metadata（:205） | B（file 投影改由 edge.file_path/node join 推導） |
| transition | 無直接 DB——diff 兩份 snapshot JSON | — | 只受 S3 輸出格式影響，驗證即可 |
| graph_csv | graph_csv.rs:67-72（run :360, db :386） | 5 條 SQL：File nodes（:155-156）、community majority（:117-118）、qname→file projection（:87-110, :191-204）、edges（:212）、communities（:268） | B＋POC（File nodes 經 import_legacy 進新庫） |

**關鍵事實（已驗證）**：`import_legacy` synthesize 節點攜全欄——kind 含 File、file_path、language、is_test、parent_name（graph_db.rs:787-792）。跑過 build+import 的庫，新庫含完整舊宇宙。原盤點的 class-C gap（File 節點、per-file language/is_test）收斂為「post-import 存在」。

**可複用基礎設施**：新 fixture `tests/graph_db_fixture.rs`；engine 的 `open()`/缺庫指引模式（graph_engine.rs:55 一帶）；`graph_db::db_path`。

**風險假設**：
- R1（高，S4 POC）：graph_csv 的 `proj()`/`::`-fallback（graph_csv.rs:191-204）對 synthesized qname endpoint 的 file 投影語義——SCIP symbol 形態的 endpoint（`lsp python ...`/Rust symbol）不編碼 repo path，投影必須改走 nodes 查表而非字串 split。
- R2（中，S2）：hub_refs/hazard 以 qname 為查詢身份——新庫 universe 含 SCIP symbol（merge 過的節點 qname 為 display 欄），同名解析行為可能變寬。以 NT baseline 對帳裁決。
- R3（中，S3）：snapshot 的 module 推導（repo_rel_qualified `::` split，snapshot.rs:64-66）在 symbol endpoint 上失效——改由 nodes 表 file_path 推導，輸出 module 需與舊版對帳。
- R4（低，S1）：chain_tour anchor 的 LIKE `%/rel` 前導萬用字元——新庫 nodes 有 name/file_path，但 `idx_nodes_name_file_line` 索引不存在於新 schema（引擎索引只有 edges 端點）；S1 驗證量級（新庫節點數遠少於舊庫 80K？NT 新庫 111K nodes——需要時把此索引加進 ENGINE_INDEX_DDL）。

**測試 fixture 雙軌**：`crg_fixture.rs`（舊 schema，S1-S4 遷移期逐步退役）→ `graph_db_fixture.rs`（新 schema）。

## EP Review（雙軸獨立審查 2026-08-27，findings 已回寫）

| # | Finding | 裁決 | 落點 |
|---|---------|------|------|
| F1 🔴 | hub_refs 主讀取路徑是 `crg_query` 子進程（`uvx code-review-graph query`，hub_refs.rs:81-100，`resolve_symbol`:295-296 消費）——僅翻 nodes 面會造成跨宇宙身份錯配；rg `.code-review-graph` 掃不到（無路徑字串） | ✅ 採納 | S2 擴 scope：crg_query 換成新庫 edges 查詢（caller/callee face＋既有索引）；殘留掃描加 `rg "uvx code-review-graph"` |
| F2 🟡 | uniqueness 模型轉變：新庫同名 (name,file) 可多節點（producer＋synthesized 併存）；chain_tour anchor LIMIT 1 無次鍵 tie-break 會漂；graph_audit GROUP BY name 會膨脹 | ✅ 採納 | S1 設計決策：anchor 加 `line_start` 次鍵排序；audit 計數 provenance-aware 去重 |
| F3 🟡 | kind 值域：producer 只寫 `kind='Function'`（test 在 is_test 欄）；legacy `Test` 節點恆 synthesize → 同 name/file 雙計 | ✅ 採納 | S1 設計決策：`kind IN ('Function','Test')` 過濾改為「producer Function（is_test 判 test）優先、排除同 (name,file) 已有 producer 的 synthesized Test」 |
| F4 🟡 | snapshot `raw_edge_count`（COUNT edges 含 REFERENCES）結構性發散；`_meta` 吃 `git_head_sha`/`last_updated`，新庫 metadata 只有 `producer` → staleness 靜默退化 | ✅ 採納 | S3 處置明列：graph_db build/import 補 stamp `git_head_sha`+`last_updated`；raw_edge_count 語義顯式文件化（預期發散項白名單）；module_edges parity 之所以可能＝kind 過濾本就排除 REFERENCES（common.rs:17），寫明 |
| F5 🟡 | mid-cutover 最可能狀態漏場景：legacy 在＋新庫 build 但未 import_legacy → 消費端靜默丟失舊宇宙（非 fail-loud） | ✅ 採納 | 新增 SM-7；決策：消費端偵測「舊庫在場但新庫無 `treesitter-legacy` provenance 節點」→ WARN 指引 import_legacy |
| F6 🟡 | dual-run 機制未指明（單一二進位不可能讀雙 schema） | ✅ 採納 | 各段驗證統一程序：baseline worktree（94b4afb build）vs working-tree build 對同一 repo 對跑 diff；db vintage 差異（舊庫凍結/新庫新鮮）列為假差異歸因項 |
| F7 🟡 | s1_foundation.rs 也掛 crg_fixture——S5 刪 fixture 會編譯炸，遷移無人認領 | ✅ 採納 | S5 precondition：s1_foundation 遷 graph_db_fixture（或 inline schema），列刪除清單前置 |
| F8 ℹ️ | 殘留：s6_mcp_server.rs:151 斷言、per-file fail-loud 文案（`uvx code-review-graph build` ×5 處）、hazard.rs:676 ignore 字串、`--graph` flag 語義 | ✅ 採納 | 全列 S5 sweep 清單 |
| R4 重評 | 新庫 111K nodes＞舊庫 80K，anchor 無 nodes 索引必全表掃——「低」評級樂觀 | ✅ 採納 | S1 直接把 `idx_nodes_name_file_line(name,file_path,line_start)` 加進 ENGINE_INDEX_DDL＋DDL（預期動作，非 contingency） |

| SM-7 | 舊庫在場＋新庫已 build 但未 import_legacy（mid-cutover 預設中間態） | 任何工具呼叫 | WARN：偵測新庫無 `treesitter-legacy` provenance 節點 → 指引 `graph_db import_legacy --repo`（非靜默丟失舊宇宙） | S1 | 全部 |

## 段落 0 補遺（review 後新增事實）

- **第 9 條讀取路徑（F1）**：hub_refs `crg_query` 子進程（hub_refs.rs:81-100，`resolve_symbol`:295-296 消費）讀舊庫——`rg "uvx code-review-graph"` 是唯一掃得到它的 pattern
- **F2/F3 語義漂移**：新庫 (name,file) 可多節點（producer＋synthesized 併存；Rust 同名方法 L<line> 消歧、legacy Test 恆 synthesize）——所有 `name=?` 面的 LIMIT/GROUP BY 都要意識候選集變寬
- **F4**：新庫 metadata 只 stamp `producer`；snapshot staleness 吃 `git_head_sha`/`last_updated`
- **R4 定案**：`idx_nodes_name_file_line(name,file_path,line_start)` 進新 schema DDL＋ENGINE_INDEX_DDL（新庫 111K nodes＞舊庫 80K，無索引必全表掃）

## 段落 S1：chain_tour ＋ graph_audit（＋scip_refs --audit）切新庫

**Context**：A 類直改名＋F2/F3 語義決策。chain_tour anchor 查詢（chain_tour.rs:340-348, :370-372, :397-399）與 graph_audit db_functions（graph_audit.rs:289-291）的述詞欄位在新 schema 全存在。UC 引用：更新「Boundary/narrative tool family」與「Completeness governance」兩個既有能力的資料源。
- 依賴：無（首段）
- 語義約束：與 S5 共享「缺庫 fail-loud 文案 = `graph_db build --repo` 指引」
- 基礎設施：`graph_db::db_path`、engine open 模式
- 依賴錨點：`AnchorDb::new` → 定義 chain_tour.rs:319 / 消費 :779；`db_functions` → graph_audit.rs:289 / 消費 cli.rs:713（--audit 委派）
- Invariant Impact：無（查詢面置換，silent-corruption 風險在 SM-6 parity 門把關）

**要點**：`AnchorDb` 開檔點改 `graph_db::db_path(repo)`；anchor 查詢加 `line_start` 次鍵 tie-break（F2）；graph_audit 計數 provenance-aware 去重（F3：producer Function 優先，排除同 (name,file) 已有 producer 的 synthesized Test）；`idx_nodes_name_file_line` 加進 DDL＋ENGINE_INDEX_DDL（R4 定案）；SM-7 WARN（舊庫在場＋新庫無 treesitter-legacy 節點→指引 import_legacy）。

**驗證**：S1-s5 測試掛 `graph_db_fixture` 建新庫（含 synthesized File/Class 節點）跑既有斷言；NT 實跑 chain_tour（daily corpus）＋`scip_refs --audit --json` 與舊庫版輸出 diff（baseline 2026-08-27）。

## 段落 S2：hub_refs ＋ hazard 切新庫（含 crg_query 子進程取代——F1）

**Context**：兩個面：(a) qname 查詢身份置換（hub_refs.rs:257/:263, hazard.rs:615/:620 → nodes qname/parent_name）；(b) **F1**——hub_refs 主讀取路徑 `crg_query`（hub_refs.rs:81-100 uvx 子進程讀舊庫，`resolve_symbol`:295-296 消費）取代為新庫 edges 查詢（caller/callee face，`idx_edges_caller`/`idx_edges_callee` 已在）；refs 聚合吃的 qname 結果（hub_refs.rs:569-575）改 symbol/qname 對映語義一併裁決。UC 引用：更新「Deletability safety net」資料源；本段吸收原「hub_refs uvx 後續 EP」。
- 依賴：S1 的 fail-loud＋WARN 模式
- 語義約束：與 S1 共享缺庫指引；與 S5 共享 `rg "uvx code-review-graph"` 殘留掃描（路徑字串掃不到它）
- 依賴錨點：`query_nodes_pairs` → hub_refs.rs:253 / `query_nodes` → hazard.rs:611 / `crg_query` → hub_refs.rs:81（消費 :296）

**驗證**：s4b 測試改掛新 fixture；NT baseline worktree 對跑：`hub_refs <symbol> --hazard`（含 refs 聚合面）舊版 vs 新版輸出 diff；crg_query 取代後 `rg "uvx code-review-graph"` 零殘留。

## 段落 S3：snapshot（＋transition 驗證）切新庫

**Context**：B 類——edges 端點 qname→symbol 後，snapshot.rs:81 的 file/module 投影改推導：caller file 由 edges.file_path 自帶；module 由 nodes 表 file_path（endpoint join）取代 `repo_rel_qualified` 字串 split（snapshot.rs:64-66，symbol endpoint 上今日會靜默丟邊）。metadata 面（:205）**F4 處置**：graph_db build/import 補 stamp `git_head_sha`＋`last_updated` 進 metadata；`raw_edge_count`（:113-114）語義顯式化——新庫含 REFERENCES 邊屬預期發散，diff 白名單列冊。transition 無 DB 面但吃 snapshot JSON 的 `_meta`/`module_edges`/`files`（transition.rs:83-105, :115-126）——欄位合約不變。
- 依賴：S1 模式＋F4 的 build 端 metadata stamping（本段實作）
- 語義約束：與 S5 共享「snapshot JSON 欄位為 transition 的合約，不可變」；module_edges parity 之所以可能＝kind 過濾本就排除 REFERENCES（common.rs:17）
- 依賴錨點：snapshot load :68-114 / open :339 / metadata :203 / detect_stale :163-194

**驗證**：s2_snapshot 測試改新 fixture；NT baseline worktree 對跑 snapshot 舊庫版 vs 新庫版 JSON diff（欄位級，`raw_edge_count`/`_meta` 為白名單發散項）；transition 吃新 snapshot 跑 delta 與舊鏈 diff。

## 段落 S4：graph_csv 退休（原「切新庫」——使用證據裁決後改向）

**裁定（2026-08-27，user 決策「修改吧，這輸出人也不會看」）**：graph_csv 是
**相容性殘留非現役工具**——使用證據掃描（跨 mosaic/NT/ai-rules 全 .py/scripts）
零程式消費者讀 `graph-nodes.csv`/`graph-links.csv`；僅有提及＝ai-rules skill 工具
清單表（描述性）＋mosaic 兩份已歸檔 UI 實驗 EP/報告；S4 retirement readiness
report 早歸類「CRG visualization 家族——零現役消費者，ratified parity bar 之外」。
且它是 schema 耦合最重的消費端（File-node 宇宙＋per-file language/is_test＋
community majority vote 只存在於 import_legacy synthesized 節點）——留著等於
強迫每 repo 永遠跑 import_legacy。

**動作**：刪 `src/graph_csv.rs`＋`tests/s5_graph_csv.rs`＋SUBCOMMANDS 條目＋
lib.rs mod；文檔清行（README 工具清單／root+crates AGENTS.md／plugin SKILL——
ai-rules skill 表的 graph_csv 行為跨 repo relay 項）。SM-4/SM-5 場景隨之消解
（R1 風險消失）。

**驗證**：全套 cargo test 綠；rg graph_csv 殘留＝核定白名單（歷史 EP/報告）。

## 段落 S5：舊路徑整體退場（sweep）

**Context**：最後一段，收掉所有过渡面。UC 引用：更新「Read-chain index maintenance」（legacy 半邊刪除）。
- **precondition（F7）**：`tests/s1_foundation.rs` 先遷 graph_db_fixture（或 inline schema），否則刪 crg_fixture 編譯炸
- 刪除：`graph_db ensure_indexes` 的 legacy half＋`LEGACY_ANCHOR_INDEX_DDL`＋usage string 舊庫子句；`common::graph_db_path` 的舊庫語義（若仍被 import_legacy 用則保留為 import 專用並改名註明）；`tests/crg_fixture.rs`（消費測試已全遷）；cli/SUBCOMMANDS 相關殘留
- 殘留掃描（F8）：`rg ".code-review-graph"` ＋ `rg "uvx code-review-graph"`（後者抓路徑字串掃不到的子進程面）；per-file fail-loud 文案（`uvx code-review-graph build` ×5：graph_csv.rs:389, hub_refs.rs:206, snapshot.rs:311/:215, hazard.rs:637）改 `graph_db build` 指引；`tests/s6_mcp_server.rs:151` 斷言更新；hazard.rs:676 ignore 字串；`--graph` flag 語義文檔化
- 文檔：README 舊庫段改寫（「legacy readers」清單刪除）、crates/AGENTS.md、plugin skill prerequisites、root AGENTS.md Capabilities
- 語義約束：`import_legacy` 本身保留（未跑過的既有 CRG repo 還需要一次性匯入）——退場的是「讀取面」不是「匯入面」

**驗證**：全套 cargo test 綠；rg 殘留清單為核定白名單；NT＋mosaic 全工具煙霧（SM-1/SM-3）。

## 整合策略

- 順序：S1 → S2 → S3 → S4 → S5（S1-S3 可獨立 commit；S5 必須最後）
- Parity 閘門：每段附 NT（必要時 mosaic）舊版 vs 新版輸出對帳證據，寫進該段 commit 訊息或 EP 段落
- baseline: 94b4afbe6c9e02750c681c95349a80911456b1a9

## 收尾步驟

1. Capabilities：更新 root AGENTS.md 四行（audit/hub_refs/graph_csv 家族/index maintenance）＋搬 Kanban 卡（EP 卡建於本 EP 產出時）
2. SYSTEM-MAP：無此檔，跳過
3. instruction 檔：crates/AGENTS.md 模組描述（舊庫讀者清單刪除）、plugin SKILL prerequisites、README
4. /audit-test 對遷移後測試套
