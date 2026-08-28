# 能力卡：Rust 型別面（bridge 同 crate rust-analyzer backend）

[tag:code-reality] [capability]

## 目標
`code-reality-lsp-bridge` 副檔名自動路由（.py→pyrefly-lsp、.rs→
rust-analyzer 無參數 spawn）：同四 tools 服務雙語言，per-backend
lazy session（獨立生命週期＋hover retry 窗——ra 冷載入 10s）。
P2 gate＝bridge 內外往返一致性（同引擎 baseline 對拍）＋延遲預算
記錄；NT session 掛接屬消費端實證。

## 相關
- EP：`ai-analysis/execution-plans/ep-p2-rust-type-face.md`
- umbrella：ai-rules roadmap P2（findings #7/#3）
