# Review: ep-w5-import-legacy-removal (uncommitted diff)

> 2026-08-28。W5＝import_legacy 完全拔除（資料面自理軸 W1-W5 終弧）。
> Dual-context 雙審查者（fresh＋primed，背景平行）＋ fold 兩入場發現
> （F-A audit wrapper 漏網、F-B freshness 訊息不自指）。
> 前弧記錄：W3 `3980fe1`、fix relay `2442692`、freshness `929420f`
> （review 記錄隨各自 commit 帶走，全文見 git history）。

## Dual-context findings（雙側零 🔴🟡）

| 來源 | ID | 嚴重度 | 問題 | 決策 | 狀態 |
|----|--------|------|------|------|------|
| primed | P-1 | ℹ️ | EP Changes 缺兩條 doc 清理記帳（graph_db_fixture docstring、s6 註解） | ✅ EP item 8 補列 | verified |
| primed | P-2 | ℹ️ | crates/AGENTS parity-harness bullet 以現在式描述已退役的 `tests/parity/`（同檔開頭自稱 R7 退役——自相矛盾） | ✅ 改「R7-retired history」過去式 | verified |
| 雙側一致 | P-3/F-2 | ℹ️ | README References＋dependency-chain 以現在式描述 code-review-graph 為 runtime dependency（W5 後語義過時；MIT attribution 須保留） | ✅ 改 design-lineage 歸屬語氣（兩處） | verified |
| fresh | F-1 | ℹ️ | 存活 fixture 保留 CRG 時代 API 名（`CrgDbSpec`/`make_crg_db`）——W5 後名稱誤導 | ⏸ 遞延：機械 rename 涉 ~10 測試檔，另開小段（EP 已記錄；fresh 明言不擋本弧） | closed (deferred) |
| judge 自查 | J-1 | ℹ️ | root AGENTS「every repo's db deleted」對 museum repo 不精確（user 裁決保留者；fresh 的 fd spot-check 因 gitignore 漏看該副本） | ✅ 改「every consumer repo's」＋museum 例外限定 | verified |

## Judge 決策與處置

- **F-A（入場發現，非審查者產出）**：`scip_refs --audit` wrapper（cli.rs `audit_mode`）仍傳 `common::graph_db_path` 舊 CRG 路徑——cutover EP 明列該模組已切但只切了 graph_audit 工具面＝call-site 漏網。修＝`graph_db::db_path`；live 證據：ai-rules 修前 FAIL「graph.db 不存在 .code-review-graph」→修後 exit 0 缺差 0 項。fresh 獨立軸查證：wrapper 與 standalone `graph_audit::run` 的 `consumer_db` 解析對齊、cli.rs 無第二同型 drift。**fixed**
- **F-B（user bug report，非審查者產出）**：chain_tour「乾淨樹誤報 uncommitted changes」——查證無 repro（乾淨樹＋rev 吻合實測靜默）＋無 code-path 機制＋時間線（WARN 功能在 `929420f` 18:50 當日最後一筆 commit 才存在）→ 裁決 unscoped diagnostic（並行 session 的 CR checkout 真有未 commit crates/ 編輯、訊息不標主體）。修＝訊息自指 `[WARN] CR checkout {path} ...`（freshness.rs＋bridge bin 副本同步；行為不變）。fresh 驗證：兩副本 format string 逐字相同、`repo` 均在 scope。**fixed**
- 雙側軸驗證（無動作）：removal completeness（`import_legacy`/`crg_fixture`/`graph_db_path`/`--dry-run` 全零殘留；`.code-review-graph` 剩餘 hits 逐條判歷史/歸屬）、測試重寫（consumer_db 兩案 pin 活契約；GraphAnchor probe `SELECT symbol FROM nodes LIMIT 1` 缺欄位即觸發同一錯誤分支＝等價 pin）、over-removal hunting（`connect_ro` 多 caller 非 CRG 專屬、MCP 無 import 路由、`--dry-run` 唯一消費者是 import_legacy、s5_coverage 零消費者）全排除。**verified**

## EP 對照（transition 行）

primed 側 10/10 EP Changes 逐項有 diff hunk 支撐、無 claim 落空；未條列變更僅 P-1 兩條（已補記）。宣稱觸及模組 vs 實際變動：graph_db/cli/common/freshness/hazard/bridge-bin/六測試檔/五文檔/兩 script 檔——與 EP 一致，無 unexplained 差異。

## 機械驗證

- `cargo check --workspace` 零 warning（fresh 獨立複跑：lib＋tests＋bridge 三面綠）
- `cargo test --workspace`：綠，唯 `lru_evict_preserves_overlay_edits`（bridge）全量並行 3/3 次 60s deadline miss——整 target 隔離 15/15 過（2.03s）、單測 0.48s；W3 與 freshness 前弧記錄各有一次同型 flake 史（未觸及 crate：W5 唯一 bridge diff 是 bin 的 stderr 訊息，不編入 `--test bridge`）。**非回歸，獨立修復候選**
- L4：`graph_db -h` 無 import_legacy；op 拒絕 exit 2；`scip_refs --audit --repo ai-rules` exit 0；CR 自身 ra scip 重建（stub 21 節點→899 節點/2277 邊純 producer）→spot-check→7.9M 舊庫刪（W3 gate 順序）→刪後 ensure_indexes/hub 正常
- primed 獨立複驗：owned db 三元組 899/2277/0（read-only sqlite）、W3 EP `_done/` move byte-identical、殘留掃描 exit=1
- `rg import_legacy` 活面（src/tests/scripts/README/AGENTS×2/plugin）→ **0 hits**；歷史檔（ai-analysis/、.kanban/、.review/、_done/）豁免
