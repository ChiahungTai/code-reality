# EP 追蹤卡：code-reality Rust-based 遷移

[tag:code-reality] [ep]

## 目標
v0 Python 凍結為 parity oracle；Rust workspace 逐工具族遷移（byte-identical parity gate），終局一次 relay 切換消費端並刪除兩份 Python（本 repo＋ai-rules 舊副本）。

## 相關
- EP: `ai-analysis/execution-plans/ep-rust-migration.md`（blueprint；R1-R7 逐段衍生子 EP）
- spec/research/前 EP：ai-rules `ai-analysis/`（spec D1-D5 鎖定不重辯）
- 能力卡：`rust-caller-edges.md`、`rust-mcp-server.md`

## 驗收標準
- 各段 parity gate 綠（NT 三面 byte-identical＋mosaic hazard＋fixtures cmp）
- R7：單次 relay 完成、兩 repo 零殘留、NT 契約面最終 byte-compare

## 備註
baseline `2eafd8a`；共存期雙凍結（兩份 Python 零改動）；舊 EP S2/S3 Python 版永不建、S4 吸收進 R7。

**進度**：R1 ✅（被 R2 子 EP 吸收執行）；R2 ✅（2026-08-25 build 完成——parity 23/23＋NT L4 byte-identical＋pytest 411 全綠；audit 面 R4 接手）。
