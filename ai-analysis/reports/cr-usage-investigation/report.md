# CR 使用率調查報告 — code review 面（2026-09-22 ~ 09-26）

**任務**：/at ticket `at-20260926-0011`（user 2026-09-25 排程）——派三個 flash agent 調查近三天 code review 相關 subagent／delegate-bridge 任務的 code-reality（CR）使用率、摩擦、可用未用情境；tri 討論後產出 ai-guide 設計建議。

**方法**：三軸平行調查（A bridge jobs／B ZCode subagents／C ai-guide 條款對照）→ 合併 findings 餵回三 agent tri 詰問輪（歸因確認／否證、摩擦排序、設計草案）→ 本報告綜合。證據檔同目錄：`axis-{a,b,c}.md`、`tri-{a,b,c}.md`、機讀 `window-jobs*.json` 等。

## 方法偏差聲明（誠實揭露）

- `~/.zcode/cli/agents/*/transcript.jsonl` 自 09-04 起不再寫入；每日 log（`~/.zcode/cli/log/`）只留約 7 天（最早 09-20）。軸 B 改用 log `tool.call.started` 事件＋output.txt 命令引述當證據面（後者可能低估 Bash CLI 呼叫）。
- job jsonl 不記 offered tools：零呼叫 job 的「未裝配 vs 未使用」區分標 `inferred`。
- 「使用」口徑：MCP 需有 toolCallId、CLI 需命令頭（軸 A 嚴格口徑）；subagent 的 2 例 review 使用實為「呼叫已發出」（log 記 completed≠結果成功）。
- tri-A 校正：review face 使用實為 5/604（muse 限定）非 0；SC-222 judge 按唯一 toolCallId＝12 呼叫（早前 24 為事件行數）；機械附掛真 attach 僅 delegate-bridge＋db-55 共 8 job（12 筆 marker 含 `[cr:unavailable]`）。

## 1. 量化總覽（tri 校準後）

| 面 | 樣本 | CR 真呼叫 | 率 |
|---|---|---|---|
| bridge jobs 全體 | 604（glm 189／codex 193／muse 222） | 24 | 4.0% |
| bridge review 性質 | 313 | 15 | 4.8% |
| bridge review face（`review --base` 附掛） | 604 | 5（muse 限定） | 0.8% |
| freshness preflight | 604 | 1 | 0.2% |
| ZCode subagents 全體 | 369 | 9 | 2.4% |
| review 相關 subagents（strict） | 54 | 2 | 3.7% |
| reviewer 家族（工具在場） | 42 | 2 | 4.8%（在場零使用 40/42＝95.2%） |

注入面：機械附掛僅 8 job；手寫路由 2 筆；**78.3%（245/313）review prompt 零 CR 字樣**。closure／impact_radius／lsp-bridge／semantic_search 全窗口近零——即使用到也只單發 refs/callers。

## 2. 正面鏈證據（使用何時發生）

「可及」在場就會用，三通道任一即成立：

1. **MCP 在工具清單**：glm 16 筆使用中 15 筆 prompt 零注入（動能＝workspace AGENTS.md＋plugin MCP 在場）；SC-222 judge prompt 零 CR 字樣仍自發 12 呼叫（tri-C 補驗 brief-judge-sc222 實錘）。
2. **證據已附掛**：delegate-bridge 自家 attach→muse 反覆 detect_changes（`[OK]` envelope ×75，job-mudnmu9p-i81525）。
3. **CLI＋prompt 命令**：codex job-muf55w7g。

## 3. 摩擦點清單（tri 詰問後定稿）

1. **派工面無機械閘**（三軸一致第一）：review-engine 硬性必含段（C9）存在但 78.3% prompt 缺席＝規則無閘；reviewer 的 2 例使用全部落在注入子集（注入 2/6 vs 未注入 0/36）——spawn 級注入才觸發行為，常駐 systemPrompt 條款無效。
2. **codex carrier 可及面斷裂**：plugin **已安裝**（config.toml marketplace＋enabled＋approval_mode=approve）但 spawn 面吃不到（193 job 零 MCP surface；job-mubxzkq9 當代證詞「結構上沒有這個能力」）；job-mubykur5 還把 codex 路由到它沒有的 MCP 工具名。codex 無自發通道（無 AGENTS.md 動能），必須與摩擦 1 配對修。
3. **worktree 結構性缺 index**：`.code-reality/.gitignore` 自寫 `*`（by design）→ WT 絕緣；attempted→failed **4/4**（26b17ad9、6b0a52da、5309e90e、9c2b6da2）；零借用處方，5309e90e 唯一自發發明「改查 main slot」。8 repo 約 58 job 結構性零。
4. **graph.db 存在性劇場**：23 個 codex job（16 review）只 `test -f`/`stat` 就退 git/grep；southchariot 的 graph.db 在場也無人查（job-mudn5nd8-tq7g39、job-muct3pl4-peezmt）。
5. **指引/配備不對稱**：reviewer systemPrompt 42/42 指名 CR 但 spawn prompt 僅 6/42 重申；cross-verify-investigator 條款指名 CR、工具清單沒有（3/3 零使用）。
6. **CR 品質摩擦**：glm heal 40.9s 後 refs 仍查無 DEF 即單點放棄（job-mudcow7p-szk8yw；`scheduleForkRevealFlush` 確有定義 sessions.ts:4988＝真 miss 非真空缺）。
7. **稽核錨點雙面失效**：transcript.jsonl 絕跡＋log 約 7 天輪轉——CR 滲透量測（含本次調查）即將無錨。

## 4. 可用未用情境分類（軸 C 12 case 為核；9 歸因派工面、1 引擎缺場、1 worktree 結構）

| 情境 | 實際手段 | 該用 |
|---|---|---|
| 負面宣稱查證（「單一源」「無漏網」） | Grep 枚舉（違 C9 禁令，無 downstream 抓） | refs/callers＋graph_audit |
| review 影響面評估 | git/grep 手工盤點 | impact_radius／detect_changes（正例：SC-222 自發 12 呼叫） |
| review 前 freshness preflight | 無（1/604） | `freshness --json` |
| caller／引用盤點 | rg | callers／closure |
| 符號真值／trait 消歧 | rg／Read | refs |
| worktree 內 review | 首跳 [FAIL] 後放棄 | 借主 checkout index |

## 5. tri 裁定：注入 vs 可及

- **可及是必要條件，注入是 reviewer 族的觸發器**。三通道（清單在場／證據附掛／CLI＋命令）任一在場即發生使用。
- 「條款＋工具在場＝穩定路徑」只在 bridge-glm 成立（AGENTS.md 動能）；Agent-tool subagents 同 harness 卻 2/54——「在場」須細分「清單在場＋行為錨在場」。
- review-engine:191（C10）「prompt 明示是唯一實證觸發形態」須改寫：把修復錨在注入而非可及會修錯方向。

## 6. ai-guide 設計建議（三軸草案收斂，按優先序）

1. **R1 派工面機械閘＋可及性入帳**：review 語義派工由產生器機械注入「CR 段」；ledger/receipt 必填 `crProbe`／`[cr-surface: mcp|attach|cli|absent]`。證據：245/313 零注入閒置庫存、2/6 vs 0/36 梯度、attach→×75 因果鏈。
2. **R2 review face 收斂＋attach 泛化**：review 語義派工必經 review face（現況 5/604 且 muse 限定；`review --base` 是唯一 carrier 零知識通道，四態消費契約已寫好從未被餵食）；attach face 泛化到全 family；plain task wrapper 警告＋receipt 記 `[cr-attach: skipped]`。
3. **R3 family 分流模板**：codex 發 CLI 指令字串、禁發 MCP 工具名（job-mubykur5 錯配實例）；另派 codex spawn 面對照實驗，定位「plugin 已裝、spawn 吃不到」的接線斷點（tri-A D4）。
4. **R4 worktree 借用處方**：first-hop 借主 checkout index——`freshness --repo <主checkout>` 判讀＋`serves=committed-baseline` 姿態消費、禁同步 rebuild、附背景重建一行處方；寫進 cr-query skill＋reviewer systemPrompt。證據：4/4 首跳失敗、58 job 結構性零、`.gitignore` self-`*` by design。
5. **R5 指引/配備對齊（近零成本）**：cross-verify-investigator profile 補 refs/callers/closure/impact_radius 四工具（3/3 零使用 vs SC-222 有 surface 即 12 呼叫）；reviewer spawn 模板重申 CR 段。
6. **R6 條款改寫**：review-engine C10 改「可及性保證＋spawn 級注入」雙通道表述。
7. **R7 劇場清除**：刪 `test -f` 半步，freshness exit code 直接映射消費動作；scip_refs 查詢鍵語法隨 attach material 傳播（治 heal 後單點放棄）。
8. **R8 稽核錨點**：CR tool.call 事件定期落持久 sink（log 輪轉＋transcript 絕跡＝下輪調查無錨）。

## 7. 附錄

- `axis-a.md`／`axis-b.md`／`axis-c.md`：三軸全量證據（逐 job／逐 agent 錨點）
- `tri-a.md`／`tri-b.md`／`tri-c.md`：tri 詰問輪回覆（含否證與校正）
- `window-jobs.json`、`window-jobs-prompts.json`、`usage-detail.json`、`auto-inj.json`、`manual-inj.json`：機讀資料
- 調查窗口的行為證據會隨 log 輪轉消失（約 7 天）——本目錄是唯一留存副本。
