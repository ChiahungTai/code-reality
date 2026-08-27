# EP — v1+ S4 所有權翻轉：自有格式 graph.db（`.code-reality/`）

> Status: **reviewed（2026-08-27 ep-review 五維度審查回寫；設計方向與數字引用全部查證屬實，schema 消費面/範圍切割/S4 可達性已按 findings 修訂）**
> **北極星（user 定調 2026-08-26）：self-contained**——完成後 code-reality
> 與 CRG 零依賴（進程已退、格式義務本 EP 清除、重建自產）；殘留物僅剩惰性
> 資料（舊 .code-review-graph/ 目錄＝oracle＋回滾）與可解除安裝的 uv tool。
> **審查修訂（F4-1/F4-2）**：「與 CRG 零依賴」的最終達成＝本 EP（格式義務清除
> ＋graph_engine 讀鏈自產）＋後續消費端 EP（見「後續 EP 劃界」）——本 EP 範圍
> 內 hub_refs 等九個舊 db 消費端仍讀舊庫、uvx 依賴仍在（誠實盤點見
> Self-contained 清單）。
> 設計源：本檔「證據與設計」段＝arch-thinking 深層思考分析（user 提案「改個名、
> 設計正確的 db」）＋ delta 審查 F2（union double-key collision 實證）。
> Baseline: `3150e11`（build 首 commit 的 parent；立 EP 時點 `e9ffa9d`，中間夾
> 無關的 tour_validate 測試 commit）。

## EP Review Findings（2026-08-27，證據：原始碼 path:line＋NT oracle 實測）

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| 1 | 🔴 | S1/S2 | nodes schema 缺 ops 消費欄位（is_test/parent_name/extra/community_id；GraphNodeLite 直讀 graph_engine.rs:52，測試入口判定 :565/:1798） | schema 增列＋定義 is_test 判定來源（見 S1） | implemented |
| 2 | 🔴 | S2/開放1 | edges schema 缺 impact edges-among SQL 直讀欄位（file_path/line/confidence/confidence_tier，graph_engine.rs:930）；detect_changes 硬查 flows JOIN flow_memberships（:1589） | edges 沿 site-grain 補欄位；derived 表裁決為「存」＋定義 writer（見 S1/S2） | implemented |
| 3 | 🔴 | S4 | parity 四 bar 在新節點宇宙上結構性不可達（新 nodes=cache 63,833 vs 舊 80,312；Tier-0 communities 是節點分組函數；hub degree 明計重複行 :195-197 而新 PK 去重） | S4 改三層驗收（對帳/引擎無損夾具/新基準歸因，見 S4） | implemented |
| 4 | 🔴 | S3 | provenance 枚舉含 'treesitter-legacy' 但 S3 只匯邊——矛盾；只匯邊＝flows 入口全滅（Test 節點 22,229/is_test/File 節點都在舊 nodes）且反解保留率實測僅 ~16%（CALLS 678,829 中雙端在舊 nodes 者僅 133,527） | 裁決：legacy nodes 也入庫（merge＋qname 合成雙軌，見 S3） | implemented |
| 5 | 🔴 | S2 | 「27-test 改 fixture」字面執行會炸全量 gates——crg_fixture.rs 被 8 個測試目標共用（含 s1_foundation 等舊 schema 消費端 7 目標） | graph_engine 測試改掛新 fixture 檔，共用檔不動（見 S2） | implemented |
| 6 | 🔴 | 全 EP | 舊 db 有九個 Rust 消費端（graph_csv/chain_tour/graph_audit/hub_refs/hazard/snapshot/cli scip_refs --audit/scip_nodes＋common 定義）讀 `graph_db_path`，EP 零著墨 | 劃界後續 EP＋S2 loaders 做成共享 library 面（見「後續 EP 劃界」） | implemented |
| 7 | 🔴 | 清單 | hub_refs 活路徑殼出 `uvx code-review-graph query`（hub_refs.rs:82-91）——「零依賴/uv tool 惰性物」宣稱為假 | 清單誠實化：uvx=活依賴，後續段落處理（見清單表） | implemented |
| 8 | 🟡 | S1/S3 | build 中斷半成品擋重跑（bootstrap graph.exists() guard）；import 重跑冪等未定義 | temp+atomic rename；import 先清 treesitter-legacy 再寫 | implemented |
| 9 | 🟡 | S1 | 「405 節點/20 邊」是 cache 狀態函數（ai-rules cache 現已 424 defs/54 refs） | bar 改「graph_db build ≡ scip_nodes --bootstrap 同 cache 輸出一致」 | implemented |
| 10 | 🟡 | S5 | instruction 同步清單不足（AGENTS.md Capabilities 多行/52-58、crates/AGENTS.md:33-48、plugin skill audit 指引、crg-query --union 敘述） | S5 改逐檔清單 | implemented |
| 11 | 🟡 | S1/S5 | 「收斂成單一 build 面」語義未閉合（舊命令退休？19 個 scip_nodes/scip_edges 測試命運？） | 裁決：CLI 面退休、library 吸收、測試隨改（見 S1/S5） | implemented |
| 12 | 🟡 | 驗收 | 「刪 .code-reality/ 即回」不成立（loaders 已指新庫，missing db=loud error） | 回滾＝git revert S2 commit＋刪目錄 | implemented |
| 13 | 🟡 | 全 EP | 無 Scenario Matrix；parity bar 是 happy-path 不能兼職 | 補場景矩陣（見「場景矩陣」） | implemented |
| 14 | ℹ️ | S1 | `import-legacy` 連字號違反 repo snake_case op 慣例 | 改 `import_legacy` | implemented |
| 15 | ℹ️ | S1 | bootstrap 現硬編 `language='Python'`（scip_nodes.rs:443）——Rust cache 節點被錯標 | build_from_cache 修：language 從來源推導 | implemented |
| 16 | 🟡 | S3/S4 | 效能（1.16M 邊 import 耗時）與 freshness gate（cache/db 新舊錯配）未規劃 | 串流批次寫＋沿 mtime 警告機制 | implemented |
| 17 | ℹ️ | S2-S4 間 | 中間態：S2 後 S3 前 NT 查詢只有 REFERENCES 邊（flows/hub 空轉） | EP 記錄為已知中間態（開發窗可接受） | implemented |
| 18 | 🟡 | F3-5 | graph_db.rs 與 scip_nodes::bootstrap 邊界、common::graph_db_path 歸屬未定 | 見 S1 模組邊界段 | implemented |

三個大裁決（A：legacy nodes 入庫；B：derived 表存；D：消費端劃界）為委任裁決，
user-override 可翻案；翻案則 S3/S4/範圍隨之重寫。

## 證據與設計

**為什麼現在**：CRG 相容約束的存在理由（CRG 在線上是第二讀者）已消失
（cutover 2026-08-26：ai-rules 消費端切換＋com.user.crg-mcp bootout）。留著相容
格式＝為不存在的讀者付永久稅：查詢時 double-key join（collision 已被審查證實）、
qname parent-qualified 慣例、雙檔（graph.db＋union sidecar）。

**目標 schema**（`.code-reality/graph.db`，取代 `.code-review-graph/` 的角色；
欄位集＝GraphNodeLite/GraphEdgeLite 消費面全集，沿舊 schema 欄位語義）：

```
nodes  (symbol TEXT PRIMARY KEY,      -- producer 原生鍵（SCIP/LSP symbol；legacy qname 合成）
        name, kind, file_path, line_start, line_end, language,
        parent_name, is_test INTEGER, extra TEXT,   -- extra=JSON（decorators 等）
        community_id,                -- communities 物化時回寫（loader 直讀）
        provenance,                  -- 'scip' | 'lsp-harvest' | 'treesitter-legacy'
        qname, updated_at)           -- qname 為 display 欄位（非鍵）
edges  (caller_symbol, callee_symbol, kind, provenance,
        file_path, line, confidence, confidence_tier)   -- 一行一 call site（無合成 PK）
       -- SCIP REFERENCES + LSP harvest + legacy tree-sitter 邊同表共居
       -- 聯集在 build 時物化，查詢零 join；hub degree 沿舊語義數 ALL 行
derived: flows/flow_memberships/communities —— build 後置物化（writer=graph_db
       build 呼叫引擎 ops 算完寫表；detect_changes 硬依賴這些表，get_minimal_context
       軟讀）
```

**設計決策**：
1. 節點鍵＝producer symbol——查詢面 double-key join 與其 collision 類消失
   （import 面的一次性反解 join 不可避免，計數入報告）
2. 邊住單一本體（sidecar 退休）——`--union` flag 退休（永遠全量）；**邊粒度沿
   舊 grain（一行一 call site）**，不引入 (caller,callee,kind) 合成 PK：hub degree
   明計重複行（graph_engine.rs:195-197）、edges-among SQL 直讀 file_path/line，
   去重會同時砍掉兩個消費面語義＋NT 159,283 個重複鍵的資訊
3. 舊 `.code-review-graph/` **原地不動**＝parity oracle＋回滾（單向門成本低）
4. legacy **nodes＋edges 皆 import**（審查裁決 A）：nodes merge（可反解者補
   is_test/parent_name/extra 進 producer 節點）＋qname 合成（不可反解者以 qname
   為 symbol 入庫）——Test/File 節點與 is_test 旗標只存在於舊庫，不 import 則
   flows 入口偵測全滅
5. derived 表**存**（審查裁決 B）：detect_changes 硬查 flows JOIN
   flow_memberships（prepare 失敗＝error），「ops 不動」承諾下唯一路徑是物化
6. 已知 regression 記錄：未來 Rust rebuild 無 tree-sitter producer（macro/多行
   19.8% 互補面只在 legacy import 裡）——未來選項：tree-sitter-rust producer 或接受
7. 演算法層（graph_engine ops）不動——BFS/relaxation/Leiden 與儲存層已分離

## 段落

### S1 — schema＋build 面（`.code-reality/graph.db`）✅ done（2026-08-27）
Context：`scip_nodes --bootstrap` 與 `scip_edges --inject` 收斂成單一 build 面
（審查裁決 C：兩者的 **CLI 子命令退休**、library 邏輯由 graph_db.rs 吸收或
引用；對應測試隨 CLI 退休改寫——被吸收邏輯由 tests/graph_db.rs 承接）。
Pseudo Code：
```
new module graph_db.rs:
  fn db_path(repo) = repo/.code-reality/graph.db     -- 歸屬 graph_db.rs
  fn build_from_cache(repo) -> Report      -- cache index（任何 producer）→ nodes+edges
                                           -- language 從來源推導（修 :443 硬編 'Python'）
                                           -- is_test：producer 面沿路徑判定；
                                           --   Rust 節點＝tests/ 目錄＋python test_ 前綴
                                           -- 寫 temp 檔成功後 atomic rename（半成品不擋重跑）
  fn materialize_derived(repo)            -- build 後置：引擎 ops 算 flows/communities 寫表
                                           -- ＋UPDATE nodes.community_id 回寫
CLI: code-reality graph_db build --repo R [--json]
```
驗證策略：tests/graph_db.rs 合成 fixture（兩 producer symbol 形態＋冪等重跑＋
半成品清理單元）；L4 bar＝**`graph_db build` 輸出 ≡ `scip_nodes --bootstrap`
同 cache 輸出一致**（nodes/edges 內容級對照；bootstrap 對同 cache 是現行 oracle，
固定數字如 405/20 是 cache 狀態函數不作 bar）。
**Build 記錄（偏差 2 條）**：
1. 歸屬實作為 **producer 條件**（非 EP 原文的統一 nearest-preceding）：SCIP 面
   有 spans → spans-based innermost（scip_edges inject 同算法，S4 層2 對 sidecar
   資料面的必要對齊）；LSP-harvest cache 無 end_line（無 spans）且其
   index.scip 為 placeholder（protobuf ladder 必炸）→ nearest-preceding（沿
   bootstrap）＋直連 cache face。L4 實證：ai-rules 上 nodes 405/edges 33
   sites/item-level 0 與 bootstrap 全對齊；rust fixture 上 site multiset 與
   scip_edges derivation exact（測試釘住）。
2. self-ref（ref 歸屬到 callee 自身 def）從 bootstrap 的靜默丟棄改為
   `self_ref_skipped` 顯式計數（never-silently-dropped 原則）；L4 的 21 筆
   即此類。
materialize_derived 的呼叫接通在 S2（trace_flows 直讀 conn——需 loaders 先指向
新庫）。

### S2 — graph_engine loaders 重寫 ✅ done（2026-08-27）
Context：`load_nodes`/`load_edges`（＋FlowAdjacency）指向新 db；symbol 鍵直通
（GraphEdgeLite 改帶 symbol 端點，qname 惰性衍生）。ops 簽名不改（`*_with(edges)`
核心本來就吃注入邊）。CRG 舊庫讀取器保留為 S3 的 import 源（`load_edges_crg`；
common::graph_db_path 保留指舊路徑——九個舊消費端沿用，
見「後續 EP 劃界」）。loaders 做成 graph_db.rs 共享 library 函數（後續消費端
遷移＝換呼叫，非換 SQL）。
驗證策略：既有 27-test graph_engine 套件**改掛新 fixture 檔**
（tests/graph_db_fixture.rs，新 schema 形態）；共用 crg_fixture.rs **不動**
（服務 s1_foundation/s2_snapshot/s4_graph_audit/s4b_hazard_hubrefs/s5_chain_tour/
s5_graph_csv/scip_nodes 七個舊 schema 消費端目標）；演算法不動＝測試值不變。
`--union` flag 退休（graph_query CLI＋mcp_server `use_union` 參數 deprecate：
回覆附指引「union 已物化，新語義=預設全量」）。
**Build 記錄**：
1. 鍵面落實＝GraphNodeLite 加 `symbol`（qualified_name 保留為 display 欄位，
   輸出 JSON 面不變）；GraphEdgeLite 端點正名 `caller_symbol`/`callee_symbol`
   （SQL 字串欄位名與 Rust 欄位同步的全檔 identifier rename，41 點一次正確）；
   FlowAdjacency `nodes_by_qn`→`nodes_by_key`；bridge/hub/flows BFS/relaxation/
   Leiden/detect_changes/transitive_test_count 的內部鍵配全部改 symbol。
   bridge/modularity 的跨社區邊輸出端點顯示 symbol（層2 夾具 symbol==qname
   一致；層3 新形態，記錄）。
2. `--union` 退休落實：CLI 面旗標帶退休指引錯誤、四個 `_union` ops＋
   load_union_edges_at mapper 刪除、MCP `use_union` 參數保留但 no-op＋
   deprecated 描述；`union_mapper_maps_sidecar_to_qnames` 測試隨面刪
   （27→26）。
3. materialize_derived 接通（build 尾聲）：flows/communities 寫表＋
   nodes.community_id 回寫（qname 鍵 UPDATE——同 qname 同檔本就同組）。
4. flows 面語義確認：producer REFERENCES 邊不驅動 flows BFS（CALLS 邊在
   legacy import 進來）——即場景矩陣的 S2→S3 中間態；ai-rules 合成庫
   實測 flows=0/communities=1。
5. 全量 cargo test 綠（26+6 graph 面；s1-s5/s4b 等舊消費端目標全過——
   crg_fixture 未動）。

### S3 — legacy importer（nodes＋edges）✅ done（2026-08-27）
Context：舊 CRG graph.db 一次性 import（NT：80,312 nodes/1,155,668 edges 實測）。
**汙染防護紀律（user 提醒 2026-08-27）**：db 本體皆 repo-relative、測試全走
tempdir；index slot（~/.mosaic/...）為全域讀面——import_legacy 對舊庫**唯讀**
（connect_ro），寫入只落 `.code-reality/graph.db`；S4 層2 重建夾具寫 temp
不落 NT repo；NT 舊庫全程不可寫（oracle）。
Pseudo Code：
```
fn import_legacy(repo) -> Report
  -- 讀 .code-review-graph/graph.db（在場才跑；不在＝skip 非 error）
  -- nodes: (file,name)→producer symbol 反查（碰撞計數入報告）
  --   可反解 → merge 進既有節點（補 is_test/parent_name/extra 等 producer 缺欄位）
  --   不可反解 → 以 qname 為 symbol 直接入庫（provenance='treesitter-legacy'）
  -- edges: 全 kind；雙端可解析者走 symbol 映射（producer symbol 優先、legacy
  --   合成 qname 次之）；懸空端點（不在舊 nodes——外部符號）qname 直通入庫
  --   （edges 無 FK；BFS/impact/hub 對未知端點天然跳過；criticality 的
  --   external factor 依賴其計數——S4 層2 實證：砍懸空邊使 criticality
  --   系統性低 external 項）
  -- 冪等：先 DELETE WHERE provenance='treesitter-legacy' 再寫（merge 為 upsert）
  -- 串流批次寫（1.16M 邊分段 txn）
CLI: code-reality graph_db import_legacy --repo R [--dry-run|--json]
```
驗證策略：NT L4＝遷移計數對帳：mapped/unmapped/collision 三數＋**懸空端點
分類計數**＋**可映射上限預估**（審查實測上限：全 kind 雙端在舊 nodes=188,630、
CALLS=133,527、舊庫含 15,476 distinct 懸空 target——對帳數對齊預估表即過）＋
**行數守恆**（導入邊行數≡mapped 邊的源行數）＋抽樣 20 邊人核。
**Build 記錄（NT L4 實測，2026-08-27）**：nodes 80,312 守恆（merged 32,430＋
synthesized 47,882；collision_keys 10,148 走保守 synthesis；symbol_collision 0）；
edges 1,155,668 全量（**mapped 307,720 ＝ 層2 夾具 SQL EXISTS 直測的 307,720
完全一致**——import 的 qname 解析與獨立 SQL 直測兩法同值；審查報告的 188,630
口徑已不可重現，以雙法一致的 307,720 為準）；dangling 847,948 照入
（dangling-through 語義，見 EP 審查回寫段）；**行數守恆成立**（mapped 走
symbol 映射＋dangling qname 直通＝源全量）。

### S4 — NT parity 重驗（遷移正確性的唯一驗收；三層結構）✅ done（2026-08-27）
Context：CRG-era parity 數字的邊集語境＝舊庫 tree-sitter 邊；新庫節點宇宙
（producer symbol＋legacy 合成）與舊不同，硬對數字必然失敗——分層驗收：
- **層1 遷移完整性**：S3 對帳全過（上列三數＋分類＋上限預估＋行數守恆）。
- **層2 引擎無損（exact bar）**：在「舊節點宇宙＋導入邊」重建夾具上（從 NT
  舊庫 nodes＋import_legacy 產物建臨時 fixture，schema 同新庫）重跑四 op 對
  CRG-era 舊值：flows **10,359/10,359** multiset、communities **42/42**、impact
  instrument_id.rs 500 scores **maxdiff=0**、hub top-10 exact——隔離節點宇宙
  變因，驗 loader/鍵直通無 bug。
- **層3 新基準（記錄非 gate-exact）**：新庫上全 op 跑新數字，差異逐條歸因
  （節點宇宙變更 vs 邊損失 vs 語義變更）；Leiden seed 重現（1,270 社區/最大
  23.4%/modularity 0.9158 為舊基準，新數字記錄）。
**層1/層2 任一不符＝遷移有缺口，不得宣稱完成**（防靜默缺邊）；層3 缺歸因
記錄亦不得宣稱完成。
**Build 記錄（2026-08-27，V_old＝baseline `3150e11` binary 對 NT 舊庫實跑）**：
- **層1 ✅**（S3 記錄：mapped 307,720 雙法同值、nodes/edges 守恆）。
- **層2 ✅ EXACT 四面**：層2 夾具（temp；nodes=舊庫 80,312 原樣 qname-symbol、
  edges=1,155,668 全量）上新 binary——**flows 10,359/10,359 multiset exact**
  （含 criticality；中途抓到並修正懸空邊語義：砍懸空邊使 criticality 系統性
  低 external 項）、**communities 42/42 multiset exact**（含 cohesion——R2 修
  symbol 鍵前 cohesion 恆 0 的撕裂已修）、**impact 500/500 maxdiff=0.0**
  （keys-equal＋impacted_files equal）、**hub top-10 逐位 exact**
  （top1 Data.clone 7083）。
- **層3 新基準（記錄＋歸因）**：NT 新庫（build 63,224 producer＋import 全量）
  flows **10,051**（−308 歸因：merge 折疊 32,430 個 legacy 節點到 producer
  symbol——入口偵測與 BFS 可達性隨節點宇宙微變）、communities **42**（數同
  組成不同：members 107,938/總節點 111,106，producer file_path 集合參與分組）、
  impact **500 scores**（與 CRG-era 交集 265 個 **maxdiff=0**；235 個差異＝
  種子/可達宇宙變更；impacted_files 38→49）、hub top-10 面貌改變（SCIP
  REFERENCES 邊墊高通用 trait fn——`from`×3/`to_pyvalue_err` 冒出，`clone`
  7086 保持 top1；資訊正確：這些確實到處被引用）、**Leiden seed 42**：
  1,151 社群/最大 35.0%（舊基準 1,270/23.4%——REFERENCES 權重改變分群面貌，
  記為新基準）。

### S5 — 消費端同步＋收尾（逐檔清單）✅ done（2026-08-27）
- repo `AGENTS.md` Capabilities：scip_edges/scip_nodes/bootstrap/Union edge
  plane/--union 行全部改寫為 graph_db build/import_legacy＋graph_query
  full-graph 語義＋Leiden 新基線（1,151/35.0%）；CRG retirement 行補
  format ownership flip DONE
- `crates/AGENTS.md`：lib layering 更新（scip_nodes 模組刪除、scip_edges
  改純 derivation lib、graph_db 新模組描述、graph_engine 讀鏈敘述、
  fixture 雙軌說明、寫面=graph_db、開頭 R7 過時段順手修正）
- `plugin/skills/code-reality/SKILL.md`：audit 前置改為事實陳述（audit 仍讀
  舊庫=凍結讀者；uvx code-review-graph build 是死指引——明示勿用）
- ai-rules `skills/crg-query/SKILL.md`：路徑行 `.code-review-graph/`→
  `.code-reality/`（description/判定/正文四處）＋fallback 段（--union 退休
  敘述＋新庫缺→build、舊庫在→import_legacy）＋CLI 清單
- ai-rules `skills/CLAUDE.md` crg-query 行同步（.code-reality/graph.db）
- **CLI 退休（F1 裁決，本段執行）**：`scip_nodes` 模組整刪（bootstrap 已被
  graph_db 吸收、inject/rollback 無消費者）；`scip_edges` CLI/inject/sidecar
  面刪（derivation lib 保留——graph_db 測試的 oracle）；main.rs route＋
  SUBCOMMANDS 同步；對應測試面裁剪（scip_nodes 8 tests 刪、scip_edges 留
  derive/filter 3 tests）
- 舊 `.code-review-graph/` 不刪（oracle＋回滾）；清理＝獨立後續決定
**偏差記錄**：ai-rules `tour-bootstrap` 路徑行**未同步**——chain_tour 仍讀舊庫
（F4-1 劃界的未遷移消費端），該行描述的是事實，改了反而說謊；待後續 EP
遷移 chain_tour 時一併更新。

### 場景矩陣

| 場景 | 觸發 | 預期行為 |
|---|---|---|
| build happy path | cache 在場 | nodes+edges+derived 物化，report 計數 |
| cache 缺 | cache db 不在 | loud error（沿 bootstrap 現行為） |
| build 中斷 | 半途失敗 | temp+rename 保護，重跑不受擋 |
| build 重跑 | cache 未變 | 冪等重建（內容一致） |
| import：舊庫在 | .code-review-graph/ 在場 | nodes merge/合成＋edges import＋對帳報告 |
| import：舊庫不在 | 目錄缺 | skip（非 error） |
| import 重跑 | 二次執行 | 冪等（先清 treesitter-legacy 再寫） |
| 反解碰撞 | (file,name) 多義 | 計數入報告，保守處理（merge 跳過） |
| 中間態查詢 | S2 後 S3 前 | NT graph_query 只有 REFERENCES 邊（已知退化，S4 前閉合） |
| 下游舊消費端 | graph_csv/chain_tour 等 | 行為不變（仍讀舊庫；劃界後續 EP） |
| freshness 錯配 | cache 新於 db | ~~警告~~ **裁掉**（F2 裁決：build 冪等秒級，重跑即最新——not-implemented by design） |

## 驗收彙總

- S4 層1＋層2 全過；層3 新基準與歸因記錄齊 ✅（2026-08-27）
- 全量 cargo gates 綠 ✅（全 suites passed、clippy unused 歸零）；ai-rules
  多檔同步 ✅（crg-query/CLAUDE.md；tour-bootstrap 偏差記錄見 S5）
- 回退：**git revert S2 loaders commit＋刪 `.code-reality/`**（loaders 已指新
  庫，僅刪目錄會讓 graph_query 全面 loud fail；舊庫全程未動）

## Self-contained 清單（本 EP 完成後的依賴盤點）

| 項 | 狀態 |
|---|---|
| CRG 進程（MCP server） | ✅ 已退（bootout 2026-08-26） |
| CRG 格式義務（schema/qname/sidecar 分離） | ✅ 本 EP 清除（graph_engine 讀鏈） |
| CRG db 格式讀取（八消費端：graph_csv/chain_tour/graph_audit/hub_refs/hazard/snapshot/cli `scip_refs --audit`＋common::graph_db_path） | ⚠️ 劃界後續 EP（仍讀舊庫凍結面；scip_nodes 已隨 S5 CLI 退休刪除） |
| `uvx code-review-graph` 殼出（hub_refs resolve_symbol 路徑） | ⚠️ **活依賴**非惰性物——後續 EP 處理 |
| 重建自產——Rust | ✅ rust-analyzer SCIP＋`graph_db build` |
| 重建自產——Python | ✅ pyright LSP＋`scripts/lsp_harvest.py`；**產品化**（subcommand 化，脫離 scripts/ 形態）＝本 EP 外的後續小段 |
| tree-sitter 邊 producer | ❌ 隨 CRG 退出——macro 邊只在 legacy import（已知 regression，記錄） |
| 殘留惰性物 | 舊 db 目錄（oracle/回滾）、launchd plist——穩定後手動清除 |

## 後續 EP 劃界（審查裁決 D，user-override 可翻案）

本 EP 只翻轉 graph_engine 讀鏈。九個舊 db 消費端
（graph_csv/chain_tour/graph_audit/hub_refs/hazard/snapshot/cli `scip_refs
--audit`/scip_nodes 舊面＋common::graph_db_path）＝**後續 EP**：統一遷移到
graph_db.rs 共享 loader（S2 已鋪路：library 函數化）；hub_refs 的 uvx 殼出
路徑（resolve_symbol → scip_refs Rust 面）同段處理；各消費端錯誤訊息中
`uvx code-review-graph build` 指引同步改寫。

## Dual-context 審查回寫（2026-08-27，judge 裁決 12 修＋6 記錄）

**R1（已修）**：`import_legacy --dry-run` 的冪等清掃（DELETE treesitter-legacy）
與 `tx.commit()` 原未受 dry_run 保護——dry-run 會刪光既有匯入還 commit。修：
dry-run 全程唯讀（不掃、rollback 不 commit）；測試 `import_legacy_dry_run_is_read_only`
釘死（前後 db bytes unchanged＋legacy 邊仍在）。
**R2（已修）**：community 鍵空間撕裂——`detect_communities_with` 的
member_sets/members 輸出與 `dedupe_community_names`、`materialize_derived`
回寫原本走 qname，與邊端點（symbol）不同空間 → symbol≠qname 時 cohesion 恆 0、
cross-edge 恆空（層2 parity 的 42/42 是夾具 symbol==qname 宇宙的正確值，非假綠；
新庫層3 需要 symbol 鍵）。修：四處統一 symbol；測試
`communities_cohesion_and_cross_edges_survive_symbol_ne_qname`（symbol≠qname
fixture：cohesion>0＋members 全 symbol）。
**🟡 六項**：MCP use_union 改 loud deprecated（`gq_union_deprecated` 前綴、
刪 leiden+union 過時特判、description 更新）；scip_nodes 錯誤指引改指
graph_db build；HELP 補 build/import_legacy＋--dry-run；F1/F2 見下。
**🟢 六項**：build 文字面補 self_ref_skipped；刪 BATCH no-op；temp 清除失敗
帶原因 warn；scan Sqlite arm 補 fn-tail 對稱過濾（non_fn_defs_skipped 入
report）；nodes_fts FTS5 external-content 表＋build/import 後 rebuild＋fixture
對稱；`graph_engine::open()` 缺庫錯誤附 graph_db build 指引。

## Judge 修正迴圈（2026-08-27 第二輪：本地 dual＋外部 relay 三眼合併）

修正落地（10 項）：F1 import 後 FTS rebuild（legacy 節點＝NT 新庫 ~43% 對
search 靜默不可見——混合命中回歸測試釘住）；F2 dry-run 的 merge key 查詢排除
treesitter-legacy provenance（前次合成節點不再污染計數）；F3 build 對
--dry-run fail-loud（原靜默吞 flag）；F4 SUBCOMMANDS 移除已刪的 scip_edges；
F5 R2 測試 cross 斷言改頂層 `arch["cross_community_edges"]`（原找錯層＝死斷言）；
F6 collision 防禦補雙同名 def 回歸測試；F7 stale-temp 清除 ENOENT 靜默（乾淨
build 不再假 WARN）；F8 protobuf 臂 non-fn 計數對稱；F9 merge extra 改為僅填
空值（不覆蓋 producer 標記）；F10 import skip 路徑 --json 面。primed#5 順手：
`*_with` 註解更新（union face 已死）。

記錄不修（4 項）：F11 materialize 失敗窗口（build 重跑即復原——冪等性吸收）；
F12 檔案不存在時路徑解析不對稱（symlink 場景，測試環境已規避、實務零案例）；
F13 deprecated 前綴插 content[0]（opt-in 才觸發，可接受）；F14 fixture 函式名
`make_crg_db` 漂移（graph_db_fixture 內部命名，無消費歧義）。primed#6 單 tx
vs EP「串流批次」：**偏差申報**——實作為單一 transaction 全量寫（SQLite 單 tx
即批量、分段 commit 更慢且冪等 sweep 語義複雜化，良性偏離）。

## Fresh-eyes 第二輪修正（2026-08-27，本地 agent 對修正後狀態）

初 snapshot 的 critical（FTS/dry-run ignore/SUBCOMMANDS 殘留）已被上輪修正
消除（agent 逐項驗證在場）；對最終狀態 8 個 ℹ️：**6 修**（F1 `graph_db -h`
不帶 op 攔截顯示 HELP；F2 site-multiset oracle 端補 self-ref 過濾——fixture
無遞迴故原綠燈成立，防未來再生炸；F3 merge UPDATE 移除死寫的 community_id
（materialize 必重算）；F4 本 repo .gitignore 補 `.code-reality/`——自身
dogfood 對稱舊目錄先例）；F5 README 舊庫讀者面列全（7 模組指向
crates/AGENTS.md 劃界）；F6 materialize 熱迴圈改 prepared stmt（NT 量級
~300K rows））。**2 記錄**（F7 build 記憶體峰值——refs 全量駐留＋by_callee
複製，繼承 scip_edges 同構設計、EP 已接受，corpus 翻倍再考慮串流化；F8
Test kind 永不 merge 的雙節點宇宙微觀成因——同一 test fn 在新庫呈 producer
Function＋qname-keyed Test 兩節點，hub degree/search 見重複，層3 hub 面貌
變化的微觀機制）。

## Build 偏差與裁決記錄（F4/F5/F6/I1）

- **F4（模組歸屬）**：loaders 實住 `graph_engine.rs`（非新模組）；import 自帶
  SQL 而非呼叫 scip_nodes——歸屬以「誰消費誰持有」記錄，不為形式搬 code。
- **F5（schema 實作 vs 草稿差異）**：nodes/edges 保留 `id INTEGER PRIMARY KEY
  AUTOINCREMENT`（rowid 面：ops 的 nodes_by_id/flows.entry_point_id 消費）；
  symbol UNIQUE；metadata 表沿舊形。
- **F6（confidence 裁決）**：producer 邊 confidence=1.0/tier='EXTRACTED' 寫死
  （occurrence 歸屬＝最高確定面；舊庫 confidence 是 CRG 產物，新 producer 無源）。
- **I1（中間態）**：build-only（import 前）flows=空——REFERENCES 不驅動 BFS，
  CALLS 在 legacy import 進來；已釘測試（S2→S3 間 NT 查詢退化為已知態）。
- **F1（CLI 退休時點）**：scip_nodes/scip_edges 的 CLI 退休**延後至 S5 收尾**
  （本 EP 內完成，非後續 EP）——S2-S4 期間 bootstrap/inject 面仍可用於
  對照與回滾；S5 一併移除 CLI 註冊與 Capabilities 行。
- **F2（freshness 場景）**：cache 新於 db 的警告**裁掉**（build 秒級冪等重跑
  即最新——warning 的價值被冪等性取代；場景矩陣該行標記 not-implemented by
  design）。

## 開放問題（build 時裁）


1. ~~communities/flows derived 表要不要存~~ → **已裁（finding 2）：存**，
   build 後置物化（detect_changes 硬依賴）
2. `graph_db build` 是否順手做 incremental（沿 cache mtime）——傾向 v1 全量
   （build 秒級，YAGNI）；freshness 警告（非 gate）仍做
3. SCIP/LSP 邊的 confidence 语义（舊庫 confidence 為 CRG 產物；新 producer
   邊沿 scip_edges sidecar 現行值）——S2 build 時對 sidecar schema 定
