[tag:crates] EP: legacy DB consumer cutover

## 目標
8 個仍讀 `.code-review-graph/graph.db` 的工具模組（chain_tour、graph_audit、scip_refs --audit、hub_refs、hazard、snapshot、graph_csv；transition 間接）切到自有 `.code-reality/graph.db`，完成後舊庫讀取面整體退場（含 ensure_indexes legacy 半邊、crg_fixture）。

## 相關
- EP: ai-analysis/execution-plans/ep-legacy-db-consumer-cutover.md
- 盤點基礎: ai-analysis/reports/s4-crg-retirement-readiness.md
- Parity baseline: NT 2026-08-27（graph_query 四 op EXACT＋byte-parity 管線）

## 驗收標準
- 全工具從新庫讀，NT 舊版 vs 新版輸出對帳通過（每段）
- rg `.code-review-graph` 殘留＝核定白名單（import_legacy 源＋歷史文檔）
- 全套 cargo test 綠

## 備註
無新增能力——全為既有能力資料源置換＋舊路徑退場。
