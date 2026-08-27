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

0. **新 EP 待寫（lsp_mcp 退役，POC 已過 2026-08-28）**：mosaic_alpha
   `tools/lsp_mcp` 退役、code-reality 統包。POC 實證：完整刷新鏈
   （scip-python index→build-cache→graph_db build→import_legacy）
   =**45.1s 端到端**、exit 0、graph 恢復 753,070 edges/27,133 nodes
   基線；新鮮度偵測 stale WARN 優於現行症狀驅動 reloadWorkspace
   （實查：無 git hook、無 watcher、僅手動 MCP tool）；型別面 ZCode
   走 lsp-python MCP（mosaic 已在允許清單）、CC 走內建。**用
   /execution-plan 正式化**（跨 repo：mosaic 端刪 tools/lsp_mcp＋
   AGENTS.md 改指 code-reality-mcp；前提＝S3-F5 cutover 完成）。
   Pyrefly 評估續攤：讀 pyrefly/lib/lsp/non_wasm/call_hierarchy.rs
   呼叫鏈（batch 枚舉 vs 惰性），clone 已在 ~/Github/pyrefly。
   **已查證（2026-08-28 收尾前）**：①crates.io **未發佈**（`pyrefly
   0.0.1 "Coming soon"`）→ git-dependency 是唯一 link 路；②
   call_hierarchy.rs 的原語＝`find_global_incoming_calls_from_function_
   definition`（per-target 掃 modules 收 call expr）＋`collect_calls_
   from_expr`——**批次化形態已可見**：單 pass 對全部 modules 收 ALL
   resolved calls（collect 的目標參數化），不需 per-def N×M。**剩最後
   一步（主線）**：小 Rust bin＋pyrefly git-dep（需先 cargo build
   pyrefly workspace，分鐘級）＋構築其 semantic state 餵
   find_global_*——成功＝R4w 通關，Rust 原生 producer 立即可規劃。
   API 穩定性：1.x 但 internal lib 無 stability 承諾——git-dep pin
   tag。**✅ S0 spike 已完成（2026-08-28）：GATE GO**——
   `~/Github/pyrefly/pyrefly/examples/batch_calls.rs` 構築鏈跑通
   （關鍵：`transaction.run(&handles, require, None)` 是排程器、
   漏呼叫則 get_ast 恆 None）；mosaic_alpha 全量 27,238 call
   sites／27,605 resolved／debug 3:48.68（release 預期 5-20×）。
   EP 在 `ai-analysis/execution-plans/ep-pyrefly-native-producer.md`
   （未 commit）——下一步：S1 薄 emitter（先 commit EP）。

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
