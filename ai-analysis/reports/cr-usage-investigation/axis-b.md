# 軸 B：ZCode subagent transcript 軸 — CR 使用率調查

調查日：2026-09-26。窗口：2026-09-22（含）至 2026-09-26。唯讀調查。

## 0. 資料源現實（重要偏差聲明）

- `~/.zcode/cli/agents/sess_*/agent_*/` 下 **transcript.jsonl 只寫到 2026-09-04**（mtime 直方圖：08-25 起逐日遞減，最後一天 09-04；09-05 之後為 0）。窗口內 369 個 agent 目錄**全部沒有 transcript.jsonl**，只有 `metadata.json`（含 spawn prompt、`profileSnapshot`（systemPrompt＋tools 清單）、`totalToolUseCount`）與 `output.txt`（最終回報）。
- 因此「逐 tool_use 區塊」的原始計畫不可行，改用兩個替代證據面：
  - **證據 A（硬，= 叫用）**：`~/.zcode/cli/log/zcode-2026-09-{22..26}.jsonl` 的 `tool.call.started` / `tool.call.completed` 事件（`core.tool.executor`），欄位帶 `sessionId`（subagent 為 `sess_subagent_agent_<agentId>`）＋`context.toolName`。MCP face 的叫用以此為準。
  - **證據 B（中，= CLI 叫用推定）**：`output.txt` 內含「命令＋實際輸出引述」（如 `→ [FAIL] 預設索引不在：…`、`exit 0`）。spawn 紀律要求回報實跑命令，故視為叫用；但未寫進報告的 CLI 呼叫會被低估（CLI 呼叫在 log 中無 command payload，無法從 log 偵測）。
  - **證據 C（不算使用）**：僅提及工具名／skill 名／crate 名（`cr-freshness` 是 crate 名，不算 CLI 叫用）。
- Bash face 的 `code-reality` CLI 呼叫在 log 無 command 內容（已驗證：log 全檔無 `"command":` 欄位），故 CLI 計數走證據 B。

## 1. 窗口總覽

369 個 agent（`agent_*` 目錄 mtime ≥ 2026-09-22；按 createdAt 分佈 09-22:110、09-23:122、09-24:80、09-25:51、無 createdAt 的 stopped:6）。

| profileId | 數量 | CR MCP 工具在 profile tools 清單 | 備註 |
|---|---|---|---|
| impl-lite | 184 | 無（Read/Write/Edit/Bash/Grep/Glob/WebFetch/WebSearch） | 有 Bash ⇒ CLI 面原則上可達 |
| general-purpose | 64 | 未限定（tools 為 `*` 或空 ⇒ 全工具） | |
| Explore | 57 | 無（唯讀面） | |
| code-reviewer | 38 | **有**（refs/callers/closure/impact_radius） | |
| lite-verify | 13 | **有**（同上四工具） | |
| spec-miner | 5 | **有**（同上四工具） | |
| code-reviewer-primed | 4 | **有**（同上四工具） | |
| cross-verify-investigator | 3 | **無**（Read/Bash/WebFetch/WebSearch） | 但 systemPrompt 指名用 CR（見摩擦 #2） |
| cr-research | 1 | **有**（同上四工具） | |

- tools 清單 distinct 映射錨點：各 profile `metadata.json` `.profileSnapshot.tools`（本調查以 `jq` 全量抽過，見 §5 方法）。

## 2. CR 實際使用（窗口內全體 subagent）

**任何形態用過 CR 的 agent：9 / 369（2.4%）。**

### 2.1 MCP face（證據 A，硬證據）— 6 agents / 18 calls

| # | agentId（前 8 碼） | type | 日期 | repo | 呼叫 | 錨點（log 檔＋時間 UTC） |
|---|---|---|---|---|---|---|
| 1 | ffa2084c | cr-research | 09-22 | code-reality | `callers` ×4 | zcode-2026-09-22.jsonl 21:40:41 / 21:40:56 / 21:41:04 ×2 |
| 2 | 5309e90e | lite-verify | 09-23 | southchariot | `refs` ×7 + `callers` ×1 | zcode-2026-09-23.jsonl 01:01:31–01:06:49 |
| 3 | 26b17ad9 | code-reviewer | 09-24 | delegate-bridge-db-43 | `callers` ×1 | zcode-2026-09-24.jsonl 12:15:54 |
| 4 | 6b0a52da | code-reviewer-primed | 09-24 | delegate-bridge-db-43 | `refs` ×1 | zcode-2026-09-24.jsonl 12:16:07 |
| 5 | 856d07a1 | lite-verify | 09-25 | sc-router | `refs` ×3 | zcode-2026-09-25.jsonl 00:03:05 ×3 |
| 6 | 566d43c4 | lite-verify | 09-25 | ai-guide | `refs` ×1 | zcode-2026-09-25.jsonl 09:14:31 |

subagent 從未呼叫：`closure`、`impact_radius`、`semantic_search`、`get_minimal_context`、`build`、`detect_changes`、`hub_nodes`、`affected_flows`、 lsp-bridge 四工具（hover/check_file/edit_file/lsp_status 在窗口內 8 次呼叫全部來自 main session）。

對照：同期 main session CR MCP 呼叫 62 次／14 個 session（80 次總呼叫中 subagent 僅佔 22.5%）。

### 2.2 CLI face（證據 B）— 3 agents（皆無 MCP 呼叫）

| agentId | type | 日期 | repo | 命令與輸出引述 | 錨點 |
|---|---|---|---|---|---|
| 9c2b6da2 | general-purpose | 09-23 | ai-guide（任務涉 sc-router） | `code-reality scip_refs --repo …/sc-router bus` → `[FAIL] 預設索引不在：…/sc-router/.code-reality/scip/index.scip` | output.txt（「CR 工具 cheap 只讀」節） |
| 64082453 | impl-lite | 09-23 | ai-guide-air-171 | `code-reality freshness --repo /Users/ctai/Github/ai-guide --json` → `"fresh": true … "serves": "current-tree"` | output.txt（「實跑 face 輸出節錄」） |
| 82a9824f | impl-lite | 09-24 | ai-guide | `code-reality delta_tour --help` exit 0 | output.txt（「工具能力查證」節） |

- **freshness/preflight 類呼叫：窗口內 subagent 僅 1 次**（64082453；MCP 本身無 freshness tool，CLI 是唯一面——設計如此）。
- CLI 掃描排出但判定為「提及」（證據 C）者 7 個：ffa2084c（`cr-freshness` crate 名）、a25d1d68、f28cf246、8d3f7f69、74653dae、61ffffa2、0669e05f——皆為文檔/crate 名/他人量測引用，非本 agent 實跑。

## 3. Review 相關樣本分類

- **strict review**：code-reviewer 38 ＋ code-reviewer-primed 4 ＋ cross-verify-investigator 3 ＋ general-purpose 中 prompt 為審查性質者 9（Judge db-35 findings、SC-199 muse 設計審、SC batch-1/AIR-169/AIR-170 fresh-eyes、SC-241/240x/242/236.2 flash second review）＝ **54**。
- **verify 腿（review 鄰接，傘形另計）**：lite-verify 13。
- **CR 自身研究（非 review，但為使用樣本）**：cr-research 1。

### 3.1 strict review 54 個的使用統計

- 用過 CR：**2 / 54（3.7%）**——即 §2.1 的 26b17ad9、6b0a52da（各僅 1 次呼叫）。CLI 亦 0。
- 從未使用：**52 / 54（96.3%）**。
- **工具在場但零使用（reviewer 家族 42 個）：40 / 42（95.2%）**——本調查頭號摩擦數字。
- 工具不在場者：cross-verify-investigator 3（profile 無 CR MCP 工具）＋ general-purpose(review) 9（工具未限定，形式上在場）。

### 3.2 傘形（含 lite-verify 13）67 個

- 用過 CR：5 / 67（7.5%）（2 reviewer ＋ 3 lite-verify）；never 62 / 67（92.5%）。
- profile 有宣告 CR 工具者共 61（42 reviewer＋13 lite-verify＋5 spec-miner＋1 cr-research）：使用 6、**零使用 55（90.2%）**。

## 4. 逐 agent 表

### 4.1 reviewer 家族（38 code-reviewer ＋ 4 primed ＋ 3 cross-verify ＋ 1 cr-research）

| id8 | type | date | repo/cwd | CR tools | CR 呼叫 | 任務 |
|---|---|---|---|---|---|---|
| 30ebaffc | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-167 審查腿 |
| 31120ac6 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-158 settlement 審查 |
| 5a8b4451 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-154 settlement 審查 |
| 5c6454f2 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-155 settlement 審查 |
| 6ee6ebbe | code-reviewer | 09-22 | southchariot | in-list | 0 | 審 SC-186 merge diff（fresh） |
| 746e0e67 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-159 settlement 審查 |
| 9d752f1a | code-reviewer | 09-22 | southchariot | in-list | 0 | 驗收 scbus capability candidate |
| ad518382 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-156 settlement 審查 |
| d181d6b8 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-166 審查腿 |
| d543823e | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-164 品質審查腿 |
| d566de6d | code-reviewer-primed | 09-22 | southchariot | in-list | 0 | 審 SC-186 merge（primed） |
| db5fa4c0 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-162 settlement 審查 |
| dcba50f9 | code-reviewer | 09-22 | ai-guide | in-list | 0 | AIR-157 settlement 審查 |
| ffa2084c | cr-research | 09-22 | code-reality | in-list | **callers×4** | 段落0全域研究：identity 觸面 |
| 11669b6f | code-reviewer-primed | 09-23 | delegate-bridge-db-33 | in-list | 0 | Primed intent-alignment review DB-33 |
| 19cb19a2 | code-reviewer | 09-23 | delegate-bridge-db-35 | in-list | 0 | Review db-35 arc diff |
| 2ecdd9bd | code-reviewer | 09-23 | delegate-bridge-db-33 | in-list | 0 | Fresh-eyes review of DB-33 diff |
| 3a8e86d8 | code-reviewer | 09-23 | ai-guide | in-list | 0 | AIR-174 F1/F2 5.3 judge 審查 |
| 8c1e7beb | code-reviewer | 09-23 | ai-guide | in-list | 0 | AIR-181 批次三 5.3 judge 審查 |
| 9a52b93a | cross-verify-investigator | 09-23 | ai-guide | **not-in-list** | 0 | 查 WT/branch 與主 db |
| d3abfdb3 | cross-verify-investigator | 09-23 | ai-guide | **not-in-list** | 0 | 查 registry.db 與行程面 |
| de2f8f14 | code-reviewer | 09-23 | ai-guide | in-list | 0 | AIR-177 變更 5.3 judge 審查 |
| 18f58bca | code-reviewer | 09-24 | delegate-bridge | in-list | 0 | S2 fresh-eyes code review |
| 1d616c52 | code-reviewer-primed | 09-24 | sc-router | in-list | 0 | 降級補償 primed 審查腿 |
| 26b17ad9 | code-reviewer | 09-24 | delegate-bridge-db-43 | in-list | **callers×1** | Fresh-eyes review S2 diff |
| 43e11c86 | code-reviewer | 09-24 | southchariot | in-list | 0 | sc-199.1 SC 端審查腿（flash 頂替 codex） |
| 4bf1d60a | cross-verify-investigator | 09-24 | sc-router | **not-in-list** | 0 | 資料面取證：未收信案例 |
| 6b0a52da | code-reviewer-primed | 09-24 | delegate-bridge-db-43 | in-list | **refs×1** | Primed review S2 vs EP intent |
| 856ce6eb | code-reviewer | 09-24 | delegate-bridge | in-list | 0 | AIR-196 skill line review |
| af56ee18 | code-reviewer | 09-24 | delegate-bridge | in-list | 0 | S1 fresh-eyes code review |
| c87ad11e | code-reviewer | 09-24 | delegate-bridge-db-43 | in-list | 0 | Fresh-eyes review of S3 diff |
| e678a473 | code-reviewer | 09-24 | sc-router | in-list | 0 | 降級補償 fresh 審查腿 |
| 07b383d8 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | db-49 fresh-eyes verification |
| 2d313435 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | db-48 fresh-eyes verification |
| 3d43caf5 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | DB-44 fresh-eyes review |
| 434ad423 | code-reviewer | 09-25 | ai-guide | in-list | 0 | AIR-187 S1 第二審查腿 |
| 48d6b2b4 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | Fresh-eyes review of W batch |
| 7138fa5e | code-reviewer | 09-25 | ai-guide | in-list | 0 | AIR-202 MCP face 同步快審 |
| 75e04b63 | code-reviewer | 09-25 | ai-guide | in-list | 0 | 202 補第二腿審查 |
| 7c42243b | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | db-45 fresh-eyes review |
| 9b0f8281 | code-reviewer | 09-25 | ai-guide | in-list | 0 | AIR-193 guard 擴充第二審查腿 |
| aaba45de | code-reviewer | 09-25 | ai-guide | in-list | 0 | AIR-197 三軸合議第二腿 |
| c1c1e587 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | db-47 report verification |
| d0b18245 | code-reviewer | 09-25 | ai-guide | in-list | 0 | 199 patrol 腳本補審 |
| e1433198 | code-reviewer | 09-25 | delegate-bridge | in-list | 0 | db-52 fresh-eyes review |
| f1baded3 | code-reviewer | 09-25 | ai-guide | in-list | 0 | AIR-187 全弧 final sweep |

（CR 呼叫欄 0＝log 中該 sessionId 無任何 `mcp__plugin_code-reality*` tool.call 事件；CLI 亦無（output.txt 掃描未中）。）

### 4.2 general-purpose(review) 9 個 — 全部 0 使用（MCP log 無事件；output.txt 無 CLI 實跑）

| id8 | date | repo | 任務 |
|---|---|---|---|
| 3bd6162a | 09-22 | southchariot | SC-199 muse 設計審 |
| 28deec93 | 09-23 | delegate-bridge-db-35 | Judge db-35 review findings |
| f194333e | 09-23 | ai-guide | SC batch-1 fresh-eyes 審查 |
| 8188a269 | 09-23 | ai-guide | AIR-169 fresh-eyes 審查 |
| 405a500d | 09-23 | ai-guide | AIR-170 fresh-eyes 審查 |
| 9b96a55e | 09-23 | southchariot | SC-241 flash second review |
| 84a82fa0 | 09-23 | southchariot | SC-240x flash second review |
| 1f5191ba | 09-23 | southchariot | SC-242 flash second review |
| 0af83708 | 09-23 | southchariot | SC-236.2 flash second review |

### 4.3 lite-verify 13 個（verify 腿）

| id8 | date | repo | CR 呼叫 | 任務 |
|---|---|---|---|---|
| 01cd2439 | 09-22 | ai-guide | 0 | 審計 12 張卡完成度 |
| 5309e90e | 09-23 | southchariot | **refs×7＋callers×1** | 退役完整性 rg 掃描 |
| 1b5ca977 | 09-23 | southchariot | 0 | git show 109e19b 檔案清單 |
| 53edda72 | 09-23 | ai-guide | 0 | Python class-body scope POC |
| 815d11d0 | 09-23 | southchariot | 0 | 查 SC-223 兩顆 commit 檔案清單 |
| bfb7c7e5 | 09-23 | southchariot | 0 | 機查 git stat 與真 DB schema |
| c2f558e5 | 09-23 | southchariot | 0 | 唯讀 git 機查 d705a09 |
| e4d22449 | 09-24 | ai-guide-air-135.1 | 0 | 機驗：跑 scoped pytest |
| 856d07a1 | 09-25 | sc-router | **refs×3** | 查 doctor.py json import 來源 |
| 566d43c4 | 09-25 | ai-guide | **refs×1** | 查注入安全條 git 歷史 |
| 9f5b33bc | 09-25 | ai-guide-air-201 | 0 | 測量 bundle 逐段 bytes |
| cae05a2c | 09-25 | sc-router | 0 | findings 閉環機械驗收 |
| d50d13b8 | 09-25 | sc-router | 0 | pytest 全綠基線確認 |

### 4.4 其餘 CR 使用者（非 review）

| id8 | type | date | 呼叫 | 錨點 |
|---|---|---|---|---|
| 9c2b6da2 | general-purpose | 09-23 | CLI scip_refs（FAIL index 缺） | output.txt |
| 64082453 | impl-lite | 09-23 | CLI freshness `--json`（fresh） | output.txt |
| 82a9824f | impl-lite | 09-24 | CLI delta_tour `--help` | output.txt |

## 5. 方法與可重現性

- MCP 叫用抽取：`jq -r 'select(.event=="tool.call.started" and (.context.toolName|startswith("mcp__plugin_code-reality"))) | [.sessionId,.context.toolName,.timestamp]|@tsv'` 逐日 log（09-22..26），得 80 列；`grep subagent` 得 18 列／6 sessions。完整清單見調查過程檔 `/tmp/cr_axisb_cr_calls.tsv`（session 級錨點已內嵌上表）。
- Agent 盤點：`find … -type d -name "agent_*" -newermt "2026-09-22"` → 369 目錄，逐一目錄 `jq` 讀 `metadata.json` 的 `profileId/createdAt/cwd/description/profileSnapshot.tools`；彙總檔 `/tmp/cr_axisb/agent_rows_all.tsv`。
- CLI 證據掃描：rg pattern `code-reality (scip_refs|graph_query|freshness|graph_db|hub_refs|build|snapshot|delta_tour|project|boundary|chain_tour|audit)|pyrefly-index|overlay-gen|cr-freshness` 對 369 個 output.txt；命中 10，人工分類為實跑 3＋提及 7；再以 `uvx|cargo run/install` 寬 pattern 複掃，無新增實跑。
- prompt/systemPrompt 指引掃描：42 個 reviewer 的 `profileSnapshot.systemPrompt` 全數含「優先使用 code-reality MCP 工具（refs/callers/closure/impact_radius）並核對 provenance/staleness」指引；spawn `.prompt` 層僅 6/42 重申 CR。

## 6. 摩擦觀察（前三大）

1. **Reviewer 家族「在場不用」是主流**：42 個 code-reviewer/-primed 中 40 個（95.2%）全程零 CR 呼叫；唯二使用者（26b17ad9/6b0a52da，db-43 S2 雙腿）也各只叫 1 次，且該 spawn cwd 是無 index 的 worktree（26b17ad9 output.txt 自陳：「CR index 缺席（僅 `.code-reality.toml`，無 `.code-reality/graph.db` / scip index）」）——即使用了也退化為單發試探而非系統性 graph 查證。多數 reviewer 走 Read/Grep 面（6/38 output 明示 rg 工作流）。
2. **指引與配備不對稱**：systemPrompt 層 42/42 reviewer、3/3 cross-verify-investigator、13/13 lite-verify 都載明優先用 CR（cross-verify-investigator 更指名「refs/callers/closure/impact_radius；fallback＝code-reality CLI 經 Bash」，錨點：`agent_d3abfdb3` metadata `profileSnapshot.systemPrompt:19`），但 (a) cross-verify-investigator 的 tools 清單根本沒有 CR MCP 工具——MCP 面物理不可達，3 個全數 0 使用；(b) spawn prompt 只有 6/42 重申，指令強度隨 prompt 層遞減。
3. **Worktree 無 sidecar index＝CR 第一跳即失敗**：`.code-reality/` per-worktree 且 gitignored，spawn cwd 指向 worktree 時 refs/callers/scip_refs 直接 `[FAIL] 預設索引不在`。窗口內至少 4 個 agent 撞上：26b17ad9、6b0a52da（db-43 worktree 無 graph.db）、5309e90e（southchariot-sc-223 無 scip index→自行降級改查 main repo slot，output.txt：「CR MCP 對該 worktree 查證失敗…替代做法＝以 main repo index」）、9c2b6da2（CLI 打 sc-router 無 index）。成功案例（5309e90e 的 8 連發、566d43c4）都靠「繞道 main repo slot」或 spawn 在 main repo。

次級觀察：freshness/preflight（AIR-135.2 要求消費前驗 freshness）在 subagent 面窗口內僅 1 次（64082453，還是 impl-lite 為了接線任務自身需求）；review/verify 腿均未做 preflight。探索型工具（semantic_search/get_minimal_context/impact_radius/closure）subagent 完全未用——用者集中在 main session。

## 7. 統計摘要

| 切面 | 數字 |
|---|---|
| 窗口 agent 總數 | 369 |
| review 相關（strict 54／含 verify 腿 67） | 54 / 67 |
| 任一形態用過 CR | 9 / 369（2.4%）＝ MCP 6＋CLI-only 3 |
| strict review 用過 CR | 2 / 54（3.7%）；never 52（96.3%） |
| reviewer 家族工具在場但零使用 | 40 / 42（95.2%） |
| 全部 profile 宣告在場者零使用 | 55 / 61（90.2%） |
| 工具不在場者 | 244（impl-lite 184＋Explore 57＋cross-verify 3）；其中 2 個 impl-lite 走 CLI |
| freshness/preflight 呼叫 | 1（CLI） |
| subagent 佔全部 CR MCP 呼叫比例 | 18 / 80（22.5%） |
