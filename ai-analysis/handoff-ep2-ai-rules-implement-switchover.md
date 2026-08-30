# Handoff：EP2——ai-rules implement 全鏈善用 CR（分層換軌＋考古消化＋stale 清理）

> 交付對象：ai-rules session（貼入即開工）。8 欄自足；凍結決策勿重辯。

## 1. 任務一句話

在 ai-rules repo 把 implement/build 全鏈的 CR 消費從「sidecar 接線」升級為「生產路徑」（分層換軌），清掉「spawned agent 未掛 MCP 一律 CLI」的過時宣稱家族，並以 mosaic 兩週考古證據裁決 research spawn 三選一。

## 2. Baseline

- ai-rules @ `683215d`（開工先 `git log` 重驗——已有後續 commit 就對帳避免重疊）
- 供給端：code-reality v0.5.1 → **v0.6.0**（EP1 四 MCP 工具上線：`build`/`snapshot`/`delta_tour`/`project`；stdio 實測可呼；工具 description 內建寫副作用警示＋build 分鐘級長時語義）

## 3. 來源

- CR repo `ai-analysis/execution-plans/ep-mcp-data-plane-tools.md`（EP1 全弧：設計裁決記錄＋EP Review 8 findings）
- 2026-08-29 深夜設計討論（凍結決策出處；記憶錨 `interface-form-mcp-vs-cli-adjudication`、`cr-projected-graph-ep-overlay`）
- mosaic 兩週考古（2026-08-30 seed 挖掘；座標見 §7）

## 4. 已完成（承接不重做）

- Tier-0 接線（ai-rules `8327ae4`：execution-plan 段落 0 spawn prompt 帶 CLI 查詢）
- Tier-1 投影圖 EP 接線（ai-rules `4b0cde5`：段落 0 投影步驟、F3 三態判讀、project 工具表）
- CR MCP 白名單 rollout（ai-rules `8201987`：code-reviewer 族/spec-miner/lite-verify 五檔全名＋MCP-first fallback——「spawned 無 MCP」從此只對 generic Explore 為真）
- CR 側四工具 MCP 化（EP1 v0.6.0）——本 handoff 的供給前提
- brainstorm 報告已在場（`ai-rules/ai-analysis/reports/2026-08-29-skill-agent-rules-improvement-brainstorm.md`）——EP2 消費它，不重做分析

## 5. 已決策（凍結，勿重辯）

1. **分層**：主 session 查詢→MCP 優先；spawned review/verify→registry agents（白名單已掛）；generic Explore→CLI 寫進 spawn prompt
2. EP1 前的過渡條文「四工具 CLI fallback」→ EP1 落地後**翻轉一行**（四工具 MCP face 在場）
3. 誠實界線條文隨接線在場：「CR 全綠≠無 ripple」＋[SRC] 詞彙（互補腿：字串鍵 rg／動態派發）
4. **考古已翻案**：14% 滲透不可重現（一般 EP 消費 0/29；真實滲透 4-5%；機制＝「接線在場但不在生產路徑」——graceful-degrade sidecar 無閉環）。EP2 的換軌設計必須回應這個機制（in-path 必填 vs 再加 sidecar），不是多鋪一層接線

## 6. 下一步（建議順序）

1. **段落 0 決策點：research spawn 三選一**（帶考古證據裁，裁決寫進 EP）：
   - ① 新增掛 CR MCP 的 research agent（白名單機制在場，技術可行）
   - ② 維持 Explore＋CLI 查詢清單（Tier-0 現狀＝`8327ae4` 形態）
   - ③ 複用 spec-miner 形狀
   - 考古證據傾向：②先跑一個 EP 的對照數據（brainstorm T2-1「天然對照」設計：8327ae4 的 CLI-in-prompt 是 sidecar vs in-path 的對照組，觀察窗從下一個 EP 起）再決定升級；③零考古證據；①的舊障礙（MCP 未連線全名 spawn 拒絕）已被 `8201987` 解除但尚無消費數據
2. **implement 分層換軌**（`skills/implement/SKILL.md:166` 區）：spawned review agent 已掛 MCP——callers/impact 查詢改 MCP-first（MCP `callers`／`impact_radius`），CLI fallback 一行留給 generic Explore；同段四工具指示翻轉 MCP 優先
3. **code-review/execution-plan 同步**：`skills/code-review/SKILL.md:127`、`skills/execution-plan/SKILL.md:161` 的「一律 CLI」改分層事實。注意：若決策點選②，execution-plan :161 的表述是「正確但需精確化」（spawn 對象確為 Explore），不是錯誤宣稱
4. **stale 全清**（rg 錨掃描）：`rg -n "未掛 code-reality MCP|一律 CLI|一律以 CLI" skills/`——已知三處（execution-plan:161、implement:166、code-review:127）＋execution-plan「Step 3 平行 Spawn／單一 Agent Prompt」段（~:315-335）的 spawn 敘述一併核對；每處改分層事實，不留「一律」
5. **考古換軌消化**（座標 §7）：挖「CRG 退役後死掉沒搬的使用模式」，逐項判「復活成 CR MCP 形態／死得對」
6. 順手清點：mosaic main worktree 殘留 `.code-review-graph/graph.db`（151KB、mtime 08-28 19:51）與「全刪」宣稱矛盾——回報 mosaic 端處置

## 7. mosaic 兩週考古座標（seed，2026-08-30）

- repo 根 `/Users/ctai/Github/mosaic_alpha`（三 worktree：main／`mosaic_alpha_offline_backtesting`／`mosaic_alpha_trading_lab`；時間窗 2026-08-15~29：322 commits、mtime 口徑 44 個不重複 EP）
- **執行面核心事實**：218 個 `_done/` EP 中工具關鍵詞只命中 6 檔（5 個 CR 自建 EP＋`ep-codebase-sweep-command.md`）；一般工程 EP 0 消費；`scip_refs`/`graph_query`/`project --plan` 全目錄零命中。實際執行證據 100% 集中在 CR 自身開發迴路：
  - `ai-rules/ai-analysis/reports/code-reality-cutover-plan.md`（[SRC] provenance 行＋callers 16 vs 17 對帳）
  - `mosaic_alpha/ai-analysis/reports/code-reality-tools-evaluation.md`（review scoping 省 91% tokens 實測）
- **死掉模式表**：CRG MCP 工具條文（`ep-codebase-sweep-command.md:109-119`→08-26 retired）／skills CRG 查詢面（cutover-plan §3.2「B 保留」標記）／CRG graph.db 作 CR 原料（cutover-plan 引 `tour-bootstrap:20`「退役 CRG＝斷原料」——08-27 起 self-owned）／dependency 三件套＋scan_imports（mosaic `6a7129cd` 08-30 退役，結構事實面改 CR graph）／MCP callers 寫進 EP 條文（`8327ae4^`→`8327ae4` 改 CLI）
- **14% 翻案＋「失敗三分類」三候選源**：brainstorm §2.1（L41 歸因＝sidecar 不在生產路徑）＋cutover-plan §3「A 可切/B 保留/C 修復決策」表＋brainstorm §3.1（sidecar 靜默跳過 vs in-path 有閉環）——「三分類」一詞無逐字出處，EP2 引用候選源時標明
- **research spawn 現行形態**：execution-plan「全域研究」段（Explore＋lite tier＋LSP/rg）；08-29 `8327ae4` 前是「若在場」sidecar 句

## 8. 驗收

- rg 錨掃描 stale 宣稱 0 hits（`rg -n "未掛 code-reality MCP|一律 CLI|一律以 CLI" skills/`）
- implement/code-review/execution-plan 三檔分層條文與白名單事實一致（跨檔無「一律」殘留、決策點裁決與條文方向自洽）
- research spawn 三選一裁決寫進 EP（附考古證據引用）
- 考古換軌結論：死掉模式逐項標「復活（MCP 形態）／死得對」
- post-build 鏈跑畢＋commit（ai-rules 端流程自理）
