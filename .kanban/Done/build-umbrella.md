# [tag:code-reality] build 傘形命令——新 repo 數據面一鍵準備（Python＋Rust 兩腿）

## 目標
`code-reality build --repo <path>` 一鍵：偵測語言（--producer rust|python 可覆寫）→
producer index（pyrefly-index／rust-analyzer scip——CLI 已 POC 實證）→ graph_db build →
ensure_indexes → 摘要。零新生產邏輯（純編排層）。雙語言合一 graph deferred。

## 相關
- EP：`ai-analysis/execution-plans/ep-build-umbrella.md`（baseline `a7b240a`）
- 既有能力：graph_db build／ensure_indexes／pyrefly-index producer（本卡為其編排消費者）
- 記憶裁決：cr-refresh-model-vs-crg（「build 傘形＝已裁可行候選、setup 薄面最小形」）

## 驗收標準
- 新 Python repo 一鍵全鏈 `[OK]`；冪等重跑誠實標明全量重產
- Rust-only／空 repo loud fail；pyrefly-index 缺場 fail＋install hint
- cargo test 全綠＋ai-rules dogfood L4

## 備註
前置：npm 退役弧的 9 檔 commit 先落地（兩弧不混樹）。
