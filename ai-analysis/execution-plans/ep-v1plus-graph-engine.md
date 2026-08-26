# EP — v1+ 圖引擎裁決弧（B1/B2）：SCIP×CRG 邊集互補、注入與引擎評估

> Status: **draft**（POC 證據已入，未 commit；**S1 注入形態已裁決＝(a) 寫入**（user 2026-08-26，
> 「跟 CRG 目前最像，一開始先參考 CRG 跟 scip-callgraph 作法」）；S3 裁決門未開）
> Source route: `ep-rust-migration.md` v1+ 條款（B1/B2 圖引擎研究→user 裁決；SCIP 邊注入
> graph.db NT 861 缺差→0；CRG MCP 退役）。User 2026-08-26 定調端局設想：
> 「高度整合 rust+python 的 rust 強化版 CRG，利用 CRG＋scip-callgraph」。

## 背景

- R2-R7 完成 code-reality 自家工具鏈 Rust 化；**圖層資料基底仍是 CRG graph.db**
  （snapshot/graph_audit/graph_csv/hub_refs/hazard/chain_tour/common 七模組讀取）。
- ai-rules 端 cutover 評估（`ai-analysis/reports/code-reality-cutover-plan.md`，ai-rules repo）
  結論「分層互賴、不可停 CRG」——本 EP 是把該「依賴」轉成「互補＋逐段自建」的行動弧。

## POC 證據（2026-08-26，NT 語料）

產物：`.agent-tmp/poc-scip-injection/`（example：`crates/code-reality/examples/scip_edge_poc.rs`，
未 commit；compare.py＋三個 TSV）。SCIP 面＝`callers::attribute` 真實歸屬邏輯全量跑
（is_def=0 occurrence×span containment）。

| 面 | 數值 |
|---|---|
| SCIP reference 站點（列） | 677,197（全部 .rs；去重 (file,line) 後 423,672） |
| SCIP 邊（fn 歸屬後） | **393,609**（另 9,831 站點在 fn span 外＝item-level） |
| CRG `CALLS` 邊總數 | 678,829（非 .rs 佔 30,991——CRG 是多語言） |
| CRG .rs CALLS：distinct 邊／distinct 站點 | **412,689**／467,859 |
| CRG .rs 站點被 SCIP 覆蓋 | 68.7%（exact）→ **80.2%（±1 行容忍）** |
| CRG-only 站點 | **92,785（19.8%）** |
| SCIP-only 站點 | 102,396（引用寬度：型別/欄位/attr 參考，非 call） |

**CRG-only 缺口定性**（樣本實查）：macro 重災區——`include_str!` const 初始化、
`criterion_group!`/bench 巨集、多行呼叫引數位置。tree-sitter 見原始語法即記邊；
rust-analyzer 對這些位置不發（或不同形態）occurrence。

**語義事實（不可繞過）**：本語料的 scip crate（0.9.0，rust-protobuf）是**舊 schema**——
`relationships` 只在 `SymbolInformation`（implements/type-def 級），`Occurrence` 無
`symbol_relationships`、無 `is_call_reference` → **call-only 邊在此 index 不可得**。
occurrence＋containment 即 SCIP callgraph 標準做法（scip-callgraph 參考實作同樣不用
relationships）。若未來 rust-analyzer/scip 升級 schema，call-only 面可重評。

**POC 結論**：兩向缺口、互補而非取代——「SCIP 注入補 CRG」成立（節點面 861＋SCIP 邊面），
「SCIP 取代 CRG 解析層」不成立（19.8% CRG-only）。**聯集模型**。

**POC2 引擎面（同日）**：393,609 邊純 std 自建引擎——鄰接表 319ms、closure BFS ≤9ms、
hub 排序 1ms；closure 語義**精確重現** CLI anchor（雙 def 種子 → depth1=15 new＋
reentries 1／depth2=0，逐項命中）。兩個 S3 設計註記：closure 種子面需 callee∪caller
聯集鍵（0-refs trait impl 不在 callee 面）；hub 榜首全是 std/core 符號（unwrap 9,550）
——引擎查詢需 workspace-scoping 過濾外部符號。

## 段落

### S1 — SCIP 邊注入 graph.db（**已裁決：(a) 寫入**）
> User 2026-08-26：「(a) 寫入 graph.db 是不是跟 CRG 目前最像，可能一開始先參考 CRG
> 跟 scip graph 作法」——是：(a) 即 CRG 既有架構形態（單一物化圖、全部下游立即受益）。
> 參考源：CRG 的 upsert 模式（`confidence_tier`/`updated_at` 欄位即為多源寫入設計）、
> scip-callgraph 的邊推導（occurrence＋containment——與本載體已同構，POC 已驗）。

1. example 轉正式工具（`scip_edges` 匯出：caller/callee/站點 TSV 或 sqlite）
2. 注入設計三護欄：
   - **可逆 tier 標記**：SCIP 邊以專屬 `confidence_tier='SCIP'`（或 `kind='SCIP_CALLS'`）
     寫入——回滾＝`DELETE WHERE tier='SCIP'`，不動 CRG 原生邊
   - **先 backup graph.db**（1.6GB；首次注入前完整複製一份）
   - **冪等 upsert＋過期清理**：index 重生後複注入以複合鍵 upsert；離開 index 的舊
     SCIP 邊以 `updated_at` 掃除（對齊 CRG 既有 staleness 慣例）
3. 驗收：注入後 NT `graph_audit --json` missing 861→0（節點面同步補，與 S2 同批）；
   CRG-only 92,785 清單歸檔明示（不在注入範圍）

### S2 — 節點面 861 收斂
graph_audit missing 名單 → SCIP 符號 → graph.db nodes 注入（名稱對映規則＝既有
`scip_refs --audit` 對帳邏輯）。與 S1 同批驗收。

### S3 — B1/B2 圖引擎裁決報告（研究段，不改碼）
- communities：Rust 生態評估（community-detection crate / Leiden port）vs 沿用 CRG Python 計算
- impact radius／flows：自建 BFS on（聯集）邊集的成本（邊集已在 Rust 手上）
- semantic search：embeddings 面缺口（最大未覆蓋項，明確標記）
- 產出：裁決報告＋user 單向門（哪些收進 Rust、哪些永久留 CRG/Python）

### S4 — CRG MCP 退役評估（S1-S3 後；條件式）
僅當 S3 裁決「CRG 獨有面已無消費者或已有替代」才啟動；否則維持分層互賴現況。

## 驗收彙總

- S1/S2：861→0（NT 基準重跑）＋注入形態裁決紀錄
- S3：裁決報告（含 POC 數字與本檔證據段引用）
- 回退：注入採 (a) 時先 backup graph.db；(b) 無需回退
