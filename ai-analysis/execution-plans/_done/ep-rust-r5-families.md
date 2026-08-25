# EP：R5 boundary＋tour＋runtime_edges 家族 Rust 化

> **ep_type**: implementation
> **parent**: [ep-rust-migration.md](ep-rust-migration.md) 段 R5（繼承 AD-1〜AD-5）；依賴 R4① foundation（common/profile/argparse/transition/snapshot 已落地 `990edbc`）
> **spec 鏈**: 凍結 Python 即規格——`boundary.py`（212）＋`boundary_build.py`（952）＋`chain_tour.py`（529）＋`delta_tour.py`（528＝281e07e 吸收版）＋`tour_manifest.py`（169）＋`tour_validate.py`（216）＋`tour_upgrade.py`（191）＋`runtime_edges.py`（244）；對應測試檔（~2449 行）為語意 oracle
> baseline: `9e7eba7`

## 段落（依賴序）

| 段 | 家族 | 內容 |
|----|------|------|
| S1 | tour 治理三件 | `tour_manifest`/`tour_validate`/`tour_upgrade`：TOML manifest 慣例＋`.tour` JSON 治理（dry-run 預設） |
| S2 | runtime_edges | viztracer trace JSON（serde_json）→ (pid,tid) ts-interval enclosure 巢狀歸屬 |
| S3 | boundary 家族 | `boundary_build`（regex 掃 `*.rs` pyo3 宣告↔`.pyi` 對照＋commit-anchored boundary sidecar sqlite）＋`boundary`（查詢面） |
| S4 | chain_tour | callchain markdown 樹框解析＋graph.db re-anchor 三態（same/moved/moved-file——`node_lines` 鍵） |
| S5 | delta_tour | transition diff＋git hunk 錨（`git diff --unified=0` subprocess——D5 shell out 原參數）；**規格＝281e07e 吸收版**（步驟集 git range 單源/claims 三態/語義注入三決策） |
| S6 | parity＋收尾 | 雙跑 cmp（synthetic fixtures；CLI 面）＋AGENTS.md/kanban/master 收尾 |

## 橫切決策

- **D1（迴圈依賴解法）**：`chain_tour`/`delta_tour` 消費 `transition`/`snapshot` lib 介面（已存在）；`boundary_build` sidecar 是獨立 sqlite（schema 照凍結），不碰 CRG graph.db。
- **D2（git subprocess）**：`git diff --unified=0`＋`git rev-parse`/`log` shell out（D5-inherited；絕不引入 git2 crate——輸出行格式＝位元組面）。
- **D3（tour JSON）**：`.tour` 檔＝indent-1 serializer（既有 `to_json_indent1`）；manifest＝toml 讀寫（toml crate）。
- **D4（gate 形態）**：每家族 committed＝鏡像測試（共享 fixture）＋parity 雙跑（CLI 可行面——不依賴 uvx/CRG 的工具全上 byte cmp；graph.db 面用 crg_fixture 合成）；dogfood 手動（`.tours` 語料在本 repo）。
- **D5（分段 commit）**：S1+S2／S3／S4／S5＋S6 各一 commit（deep-work 弧授權）。

## Ask First

1. 任何凍結行為與 EP 推斷衝突時——記錄偏差照凍結面移植
