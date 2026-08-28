# [infra] 資料面統一 — sidecar 遷入 <repo>/.code-reality/（~/.mosaic 退役）

## 目標

sidecar（SCIP index＋cache db＋snapshots＋boundary）從
`~/.mosaic/code-reality/scip/<basename>/` 遷入
`<repo>/.code-reality/`（與 graph.db 同址）；資料目錄自帶
`.gitignore`（CRG 模式，消費端零 gitignore 設定）；
`~/.mosaic/code-reality` 整目錄退役。basename 撞名/path-hash
議題隨之死亡；staleness 閘門逐支盤點（同樹對齊類全留）。

## 相關

- EP：`ai-analysis/execution-plans/ep-data-plane-unification.md`
  （baseline `e041286`）
- 裁決脈絡：user 2026-08-29 arch-thinking 弧（第三次「資料放哪」
  提問）；反轉先前「sidecar 留集中」——關鍵：path-hash central
  ≡ in-repo（語義等價、共享利益不存在）＋樹污染是邊際成本
  （graph.db 1.5G 早 in-repo、target/ 23G 常態；sidecar 全量
  783M）
- 接觸面：三常數（engine.rs:17／boundary_build.rs:19／
  snapshot.rs:19）＋default_index_path basename 鍵
- CRG 佐證：`.code-review-graph/` 全 per-repo＋
  `_write_data_dir_gitignore`（incremental.py:272）

## 驗收標準

1. 新 slot 落 `<repo>/.code-reality/`＋自帶 `.gitignore`；臨時
   git repo 上 `git status --porcelain` 為空
2. 六個存量 repo 冪等搬遷（NT 602M 同盤 mv）＋搬遷前後查詢/build
   輸出 byte-identical；重跑零動作
3. `~/.mosaic/code-reality/` 退役（含 `__pycache__` 殘留）
4. staleness 閘門盤點表落地（保留/退休＋理由）；snapshot 0-files
   bug 對照實驗有結論（治了收案／沒治獨立立案）
5. 全量測試綠＋README/AGENTS/crates AGENTS/ai-rules handoff 交付

## 備註

- NOT：graph.db 不動／launchd 不動／path-hash 不做（議題死亡）／
  staleness 閘不刪只盤點
- 決策類型：雙向門（資料可搬回）
