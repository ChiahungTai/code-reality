# 軸 A：delegate-bridge 派工的 CR 注入與使用調查（2026-09-22 ～ 09-26）

調查日期：2026-09-26。唯讀調查；全程未修改任何 repo 既有檔案、未 commit。
調查員：軸 A subagent（ZCode / GLM-5.3-Flash）。

## 0. 資料源與方法

- Ledger：`/Users/ctai/Github/<repo>/.delegate-bridge/jobs.json`（16 repo 全掃；`/Users/ctai/.delegate-bridge/` **不存在**——早前「存在」的記載有誤，glob 錯誤被 `2>/dev/null` 吞掉造成誤判）。
- 逐 job 軌跡：`<repo>/.delegate-bridge/jobs/<jobId>.jsonl`（NDJSON；glm=ZCode session 事件含 `payload.input` 完整 prompt 與 `toolName` 工具呼叫；codex=thread 事件含 `command_execution` 全部 shell 指令；muse=prompt＋stream，工具細節不在 job 檔）。
- muse review-face 完整軌跡：`<repo>/.delegate-bridge/review-<ms>.json`（含全 prompt 與工具事件）。
- codex 完整 session：`~/.codex/sessions/2026/09/*/rollout-*-<thread_id>.jsonl`（由 job NDJSON 首行 thread_id 對映，用於 prompt 還原與 MCP 嘗試反查）。
- 窗口：ledger `timestamp >= 2026-09-21T16:00:00Z`（＝UTC+8 的 09-22 00:00）。
- 機讀全量資料（本調查副產物，供複查）：
  - `window-jobs.json`（604 筆窗口 job 基本欄位）
  - `window-jobs-prompts.json`（592/604 已還原完整 prompt＋nature/injection 標記）
  - `auto-inj.json` / `manual-inj.json` / `usage-detail.json`

## 1. 樣本總量與分類

窗口內共 **604** jobs：glm 189／codex 193／muse 222。

性質分類（依完整 prompt 的角色指派與輸出契約判定；12 筆 prompt 不可還原者用 preview）：

| 性質 | 定義 | 筆數 |
|---|---|---|
| review | prompt 指派 reviewer/judge/challenger/arbiter/驗收角色，或要求 verdict/findings/severity 輸出 | **313**（glm 92／codex 85／muse 136） |
| advisory | 諮詢/合議/研究/立場腿 | 131 |
| task | 實作/工作單/建置 | 160 |

（邊界說明：驗收腿、收線複驗、EP review 均計入 review；「verification receipt」字眼的實作 job 計入 task。分類腳本與逐筆標記在 `window-jobs-prompts.json` 的 `nature` 欄。）

## 2. Prompt 面：CR 注入

注入分三型：
- **auto（機械附掛）**：bridge review face（DB-23/DB-32，`delegate-bridge/rust/crates/bridge-core/src/review.rs:402-462`）在 prompt 組裝前解析 `code-reality` CLI、對有 `.code-reality.toml` 或 `.code-reality/graph.db` 的 repo 跑 `graph_query detect_changes`，成功附 `[OK] graph_query detect_changes` typed envelope，失敗降級 `[cr:unavailable]`／`[cr:empty]`。
- **manual-explicit**：caller 在 task prompt 手寫 CR 工具路由指示。
- **subject-matter**：CR 本身就是審查標的（審 CR 的 EP/碼），CR 字樣自然在場，不算「叫 carrier 用 CR」的注入。

| 注入面 | 筆數 | 佔 review 樣本 | 例證 |
|---|---|---|---|
| auto-attach（全性質） | 12 | review 9/313＝**2.9%** | job-mudnmu9p-i81525（delegate-bridge muse review，`[OK] graph_query detect_changes` envelope ×75）、job-muh587f1-23l835（db-55，`[cr:unavailable]`） |
| manual-explicit 路由 | 2 | — | job-mudddsc6-7efqhg（"If this repo has a code-reality index…" 條件路由）；job-mubykur5-tnnybj（prompt 直接列 `mcp__plugin_code-reality_code-reality__refs` 等工具——但該 job 是 codex，見摩擦 1） |
| prompt 含任何 CR 關鍵詞（含 subject-matter/背景） | 73（review 68） | review **21.7%** | 多為 MOS-126/125、staleness policy、CR-attach 功能審查等 subject-matter |
| 零 CR 字樣 | review 245/313 | **78.3%** | southchariot SC-2xx 八軸 review 全系列、mosaic post-build review 等 |

## 3. Output 面：CR 實際使用

判準：carrier 軌跡中出現 (a) CR MCP 工具呼叫（glm，唯一 toolCallId 計），或 (b) `code-reality <subcommand>` 為 command-head 的 CLI 執行（codex/muse）。單純 `rg` 到字串、prompt 複述、`test -f graph.db` 存在性檢查**不算**使用。

**實際使用共 24/604（4.0%）；review 性質 15/313（4.8%）。**

### 3a. glm（ZCode carrier；自帶使用者 plugin 的 CR MCP）——16 jobs

| jobId | repo | 日期 | 性質 | prompt 注入 | CR 使用（唯一呼叫數） |
|---|---|---|---|---|---|
| job-mudcow7p-szk8yw | southchariot | 09-23 | review | 無 | refs×4、detect_changes×2、semantic_search×3、get_minimal_context、affected_flows、impact_radius |
| job-mudecbb3-0feeey | southchariot | 09-23 | review | 無 | refs×28、callers×4 |
| job-mug6vf95-0sqbd2 | sc-router | 09-25 | review | 無 | check_file×6、refs×12（repo 無 scip slot，見摩擦 4） |
| job-mudqlrv9-uvmbd4 | mosaic_alpha | 09-23 | review | 無 | refs×3、callers×3、semantic_search×3 |
| job-mudgq0h5-3aicyp | southchariot | 09-23 | review | 無 | check_file×3、semantic_search×18 |
| job-mufqqvmt-glrp39 | ai-guide-air-135.1 | 09-24 | review | 無 | check_file×3 |
| job-mudxjluf-1emzyp | ai-guide | 09-23 | review | auto（cr 字樣另有大段） | lsp_status×3、check_file×3 |
| job-mugqmxzh-xd7t4z | ai-guide | 09-25 | review | 無 | refs×4 |
| job-mudgyr0l-9xm07q | mosaic_alpha | 09-23 | review | manual（MOS-126 仲裁工單） | semantic_search×3 |
| job-mude5o9p-gi5ojl | southchariot | 09-23 | review | 無 | check_file×3 |
| job-muem7bh1-fjgtiv | sc-router | 09-23 | advisory | subject（CR freshness 軸） | lsp_status×6、get_minimal_context×3 |
| job-muh230ji-kyfh62 | ai-guide-air-201 | 09-25 | advisory | 無 | check_file×3 |
| job-muff51v8-tq3q4i | ai-guide | 09-24 | advisory | 無 | get_minimal_context×9 |
| job-muflra3f-dd7ea4 | ai-guide-air-135.1 | 09-24 | task | 無 | check_file×3 |
| job-mug6jth5-safoya | ai-guide-air-135.7 | 09-24 | task | 無 | check_file×6 |
| job-mug4ltoc-1rttq2 | ai-guide-air-194 | 09-24 | task | 無 | check_file×6 |

要點：**16 筆中 15 筆 prompt 完全沒有 CR 注入**——使用動能來自 workspace AGENTS.md（symbol-query-routing 任務啟動 gate）＋ZCode 載入的 code-reality plugin MCP，不是來自 bridge prompt。

### 3b. codex（chatgpt-web carrier；僅 shell）——6 jobs 真跑 CLI

| jobId | repo | 日期 | 性質 | prompt 注入 | CR CLI 執行 |
|---|---|---|---|---|---|
| job-mubjspy3-be756g | delegate-bridge | 09-22（02:05） | review | 無（subject：bridge review face 審查） | `code-reality scip_refs resume_model_hint --callers` 等 ×3 |
| job-mude3cza-f1fgxr | delegate-bridge | 09-23 | review | auto | `~/.cargo/bin/code-reality graph_query …` |
| job-mucnha3k-a0mj45 | ai-guide | 09-22 | review | subject | `\| code-reality build`（tri 裁決腿） |
| job-mudgeqnp-4sugsl | ai-guide | 09-23 | advisory | 無 | `\| code-reality freshness` |
| job-muf55w7g-bx5je2 | southchariot | 09-24 | advisory | 無 | `code-reality freshness --repo … --json`＋`graph_query` |
| job-muc1ea6w-tfdwsl | ai-guide | 09-22 | task | 無 | `code-reality scip_refs check_json_face` |

另：**23 個 codex job（review 16）只做了 graph.db 存在性/mtime 檢查**（`test -f`／`ls -l`／`stat .code-reality/graph.db`）後即用 git show/grep 審查，從未查詢——例：job-mudn5nd8-tq7g39、job-muct3pl4-peezmt、job-mucmgbyb-s5zbhk（皆 southchariot review，而該 repo graph.db 在場）。MCP 嘗試：193 個 codex job 的 NDJSON item 型別僅 `command_execution/file_change/agent_message/error`；對映 rollout 全文反查 `mcp__plugin_code-reality` 命中 **0**。

### 3c. muse（review face 有完整 export；其餘 muse job 檔不含工具細節）——2 jobs 真跑

| jobId | repo | 日期 | 性質 | prompt 注入 | CR CLI 執行 |
|---|---|---|---|---|---|
| job-mudnmu9p-i81525 | delegate-bridge | 09-23 | review | auto（`[OK]` envelope ×75） | `graph_query detect_changes` ×9＋`build` |
| job-muddrhmi-i5s30i | delegate-bridge | 09-23 | review | auto | `build`＋`graph_query` ×6 |

同 face 未跑的：job-mudd07zi-1l4v2b（docs-only，`[cr:unavailable]`×3）、job-mudddsc6-7efqhg（有條件路由指示但 0 使用）、db-55 job-muh587f1-23l835（`[cr:unavailable]`：`[FAIL] graph.db 不在：/Users/ctai/Github/delegate-bridge-db-55/.code-reality/graph.db`）。限制：非 review-face 的 muse job（約 215 筆）軌跡不含工具事件，使用狀態不可測（唯一資料源 review-*.json 僅 5 份在窗口內）。

## 4. 交叉統計

- 注入率（review 樣本）：auto 9/313＝2.9%；auto＋manual 11/313＝3.5%；任何 CR 字樣 68/313＝21.7%（多為 subject-matter）。
- 使用率（review 樣本）：15/313＝4.8%（glm 10、codex 3、muse 2）。
- 注入→使用鏈：auto-attach 的 9 筆 review 中 3 筆實際再跑 CR（codex 1、muse 2）；glm 的 16 筆使用中 15 筆來自「無注入」（AGENTS.md 通道）。
- 按 family 的可用面：glm＝CR MCP 在場（使用者 plugin 隨 ZCode 載入）；codex＝**無任何 CR MCP**（0/193），僅 shell CLI；muse review face＝shell CLI（有 attach 機制餵證據）。
- 各 repo CR data-plane 在場（窗口末現況）：graph.db=YES＝ai-guide／ai-lifecycle／code-reality／delegate-bridge／mosaic_alpha／southchariot；僅 `.code-reality.toml`＝air-135.1/135.7/194/195/201、db-55/56、sc-router；無＝Backlog.md、codex-chatgpt-web。

## 5. 摩擦點（依影響排序）

1. **codex carrier 結構性無 CR MCP，且 prompt 曾把 codex 路由到它沒有的 MCP 工具**。193 個 codex job 零 MCP surface；`~/.codex/config.toml` 雖啟用 `code-reality@code-reality-market`（多個工具 `approval_mode = "approve"`），bridge spawn 面吃不到。job-mubykur5-tnnybj 的 review prompt 明列 `mcp__plugin_code-reality_code-reality__refs` 為可用工具——對 codex 是空頭支票。當代證詞：job-mubxzkq9-pp3eke（codex，09-22）：「Bridge code-review 會用 CR 嗎？**目前不會，而且對 bridge forwarder 本身而言，是結構上沒有這個能力。**」
2. **auto-attach 覆蓋極窄且依賴 graph.db 在場**：只接在 delegate-bridge 自家 review face（9/313 review）；六個 air/db WT＋sc-router 僅有 toml 無 graph.db，v1 明文 no auto-build（`review.rs:811`）→ 這些 repo 的審查 100% 拿不到 CR 證據，只看到 `[cr:unavailable]`（job-muh587f1-23l835）。
3. **最大宗 review 派工零注入，codex 退化成「graph.db 存在性劇場」**：southchariot 八軸 review 系列 prompt 零 CR 字樣；23 個 codex job（16 review）只 `test -f`/`stat` graph.db 後改用 git/grep 審查——southchariot 的 graph.db 明明在場也沒人查（例 job-mudn5nd8-tq7g39、job-muct3pl4-peezmt）。審查結論因此無 [SRC]/影響面機驗錨。
4. **glm 自發使用但品質摩擦**：southchariot `refs scheduleForkRevealFlush` 在 heal 40.9s 後仍 `查無 DEF`（exit 1，job-mudcow7p-szk8yw）；sc-router refs×12 落在無 scip slot 的 repo（job-mug6vf95-0sqbd2，`.code-reality/` 目錄現況不存在，heal 未落地或失敗，待查）。

## 6. 正面發現（對照組）

- 注入→使用的鏈條本身是通的：delegate-bridge review face 有 attach 時，muse carrier 會反覆跑 `graph_query detect_changes`（job-mudnmu9p-i81525，[OK] envelope 75 處）。
- glm＋ZCode 的 plugin 通道證明「不靠 prompt 注入也能有使用率」：16 筆使用全來自 AGENTS.md/skills＋MCP 在場，其中 10 筆是 review 性質——這是三家族中唯一有規模的 CR 消費面。

## 7. 調查限制

- muse 非 review-face job 的工具軌跡不落在 job 檔，使用率對 muse 而言是下界。
- 12 筆 prompt 不可還原（10 glm 多為 probe/spawn-error、2 codex），以 200 字 preview 判定注入。
- codex「為何吃不到 plugin MCP」未做 bridge 源碼級定位（僅軌跡反查 0 命中）；approval_mode=approve 是否為成因需 bridge 側確認。
- 使用判定以 command-head/MCP toolCallId 為準；管線中的別名（`cr`、uvx 形態）如有未覆蓋，可能低估 1-2 筆。
