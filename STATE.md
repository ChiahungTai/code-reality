# STATE.md — Last session 觀察（occurrence producer EP）

## 卡點／轉向觀察

- **scip-python 本地 build ≠ npm dist**：同 v0.6.6 source，本地 webpack
  build（Node 24 與 Node 16 皆試過）在 mosaic 產生 31 個
  `getVariance` Debug Failure（npm 套件零個）——歸因未解，已由
  indexer emit-skip patch 圍堵（11/552 檔跳過）。下次接手若要收斂
  跳檔數，先查這個（候選：CI 的 npm install 依賴解析 vs 本地）。
- **references 密度語義差距（~20×）**：pyright SCIP emitter 的
  reference-role 遠比 LSP textDocument/references 稀疏。S3/S5 對帳
  必須顯式歸因；REFERENCES 覆蓋率預期顯著低於 lsp golden——
  這是 producer 本質，不是 bug。

## 下次起手點

1. S3 剩餘：F2（occurrence_roles side table＋CALLS 邊推導，graph_db.rs:543
   硬編碼 REFERENCES 處）、F4 碰撞對帳、F5 slot cutover、SM-8/9 接線、
   R2-3 class symbol 量化。F1 已完成（infer_language 前綴表＋unit test）。
2. S5：CALLS pair-set 對帳報告。
3. 產物位置：patch=`scripts/scip-python-mosaic.patch`、issue 草稿=
   `scripts/scip-python-mosaic.patch` 同目錄、全量 probe=
   sidecar `index-occurrence-probe.scip(.db/.fndefs.db)`、
   golden baseline=`~/.mosaic/code-reality/golden/`。
4. scip-python clone 在 `/Users/ctai/Github/scip-python`（v0.6.6＋
   兩處 patch 未 commit；upstream remote 已加）。
