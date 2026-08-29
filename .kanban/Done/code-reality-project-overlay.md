# [tag:crates] EP 投影圖 overlay（code-reality project）

## 目標
宣告式 projection plan → overlay SCIP（單一源鑄造）→ cat 真實 index → 投影 slot 查詢 → `[projected]` 報告（graft surface / 新符號反向鏈 / HOLE / MISSING）。EP 構想期與 ep-review 的機械 ripple 證據。

## 相關
- EP：ai-analysis/execution-plans/ep-projected-graph-overlay.md
- 設計記憶：cr-projected-graph-ep-overlay（POC 全綠 2026-08-29）

## 驗收標準
- `code-reality project --repo R --plan P` 端到端（SM-1..10）
- 一致性 gate fail-loud（宣告 vs 程式碼）
- 真實 slot 零污染（bytes 不變）
- `[projected]` 標籤＋假想邊計數在場

## 備註
Deferred 清單見 EP（duplicate rel_path / graph face / ai-rules 接線 / rust plan / MCP / slot GC）。
