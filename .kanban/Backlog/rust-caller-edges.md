# 能力卡：caller 邊查詢（Rust 原生）＝UC-2

[tag:code-reality] [capability]

## 目標
`code-reality scip_refs <symbol> --callers/--closure`：DEF-enc containment 歸屬（96.9% 機制已證）＋single-line span（SM-6）＋item-level 分離＋closure BFS（環偵測/depth/按檔聚合）。

## 相關
- 父 EP：`ai-analysis/execution-plans/ep-rust-migration.md` 段 R3
- 研究背景：ai-rules 研究報告 §2（17 callers LSP 交叉驗證）

## 驗收標準
`EventStoreLifecycle.open` callers=17＝LSP `incomingCalls`＝closure 起點（三源一致）；closure 秒級。

## 備註
Python 版永不建（舊 EP S2 由 R3 取代）；無 parity 對象（新能力）——驗收走三源一致。
