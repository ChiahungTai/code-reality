# Review: sweep batch — fixture rename + flake retry + card/EP cleanup (uncommitted)

> 2026-08-28（發版 bump 弧的小修批次）。前弧：mcp fix `89ff971`、W5
> `4a36b22`（全文見 git history）。

## 內容

1. fixture rename：`make_crg_db`→`make_graph_db`、`CrgDbSpec`→
   `GraphDbSpec`（W5 遞延項落地；7 測試檔——首輪漏 3 檔〔s1_foundation/
   graph_engine/s4_graph_audit〕由全量 build 抓回，教訓＝rename 盤點
   必須全目錄 rg 重掃不可憑記憶列舉）。
2. flake 修：`lru_evict_preserves_overlay_edits`（全 workspace 並行
   3/3 敗、隔離必過）——retry-on-starvation 設計。
3. `.kanban/In-Progress/` 兩張處理好的卡刪除；rust-migration 主 EP
   歸檔 `_done/`（byte-identical 純搬移，子 EP 全在 `_done/`）。

## Fresh 審查判讀（F1-F3 全採納）

- **F1 🟡 ✅（核心）**：首版 retry gate 是 content-shaped
  （`!contains("bad-return")`）——把「starved」與「wrong answer」混為
  一談：eviction 路徑丟 overlay 的 signature 恰是首查 disk-state 答案
  （8:13 在、bad-return 不在、無 WARN），re-edit 從普通 edit 路徑重建
  overlay 會「治癒」它＝**遮蔽本測試全 suite 獨家釘住的 EP R-06 契約**
  ；同時漏掉「WARN partial 已含 bad-return」的 starvation 模式。
  修＝gate 改 starvation signature：`[WARN] not converged ||
  starts_with("count=0")`（strict fixture 合法答案永 ≥1 error，count=0
  只能是 starvation）——wrong-content 首查直接踩 assert 大聲失敗。
- **F2 ✅**：assert 訊息隨 F1 準確（只在 retry 真跑過的路徑成立）。
- **F3 ✅**：s2_snapshot/graph_engine 檔頭 doc comment 的 CRG-era
  語義殘留改寫（識別字殘留原本已零——歷史 EP 豁免）。
- 審查者另證實：re-edit 是實質 nudge（apply_edit 無條件 range-form
  didChange＋version bump，非 sync_open no-op）；retry 有界（≤2×60s）；
  無 LRU/lock 不良互動；bump JSON lockstep 0.1.2、全 repo 無 0.1.1 殘留。

## 機械驗證

- 全 workspace：**41 suites 全綠**（flake 測試並行負載下 ok——當日原
  3/3 敗；gate 修正後 bridge target 15/15 隔離綠）
- `rg "CrgDbSpec|make_crg_db"` 全 repo＝0（僅 `_done/` 歷史 EP）
- `cargo check --workspace --tests` 綠（唯一 warning＝rust_backend 既有
  unused `dir`，未觸碰檔、標記不動）
