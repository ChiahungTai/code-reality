# 能力卡：graph 家族 Rust 載體＝UC-4 換軌（R4①）

[tag:code-reality] [capability]

## 目標
foundation（common/profile/exclusions）＋graph 家族（snapshot/transition/graph_audit/graph_csv）＋`scip_refs --audit` 移植 Rust 載體；byte-parity 契約＝stdout 位元組＋exit codes（stderr 管理面不 gate）。

## 相關
- EP：`ai-analysis/execution-plans/ep-rust-r4-graph-family.md`（①——已過三軌審查 2026-08-25）
- ②子 EP：`ep-rust-r4b-hazard-hubrefs.md`（hazard×hub_refs 恆連動——**不在本卡**；本卡不含 hazard 規則引擎，profile `hazard_registry` 鍵解析屬①地基）
- 父 EP：`ep-rust-migration.md` 段 R4（兩段式之①）

## 驗收標準
S6 gate：synthetic 雙跑全綠（六工具 stdout/exit cmp＋`-h` usage 面＋CSV 檔案位元組）＋跨語言互通（Rust snapshot→Python transition）＋全量回歸零改動；dogfood＝實機手動步驟（graph.db gitignore 面，非 gate）。

## ②完成（2026-08-26）
hazard×hub_refs 全落地（cargo 164／mosaic dogfood 位元組等價）；詳見 `ai-analysis/execution-plans/_done/` 兩份 R4 EP。

## 備註
共存期 Python 零改動；graph.db 只讀（connect_ro）；`--json` 四鍵是治理鉤子契約面（消費端驗證隨 open-source 測試政策改道——見 tests/AGENTS.md）。
