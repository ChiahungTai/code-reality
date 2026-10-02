# 軸 C：情境對照——「按條款該用 CR、實際走 rg/Grep/Read」調查

窗口：2026-09-22 ～ 2026-09-25。調查日：2026-09-25。
調查員：軸 C（情境對照軸）。唯讀調查，僅產出本報告檔。

## 0. 證據面與方法限制（重要）

- 任務指定的 `/Users/ctai/.zcode/cli/agents/sess_*/agent_*/transcript.jsonl` 在窗口內**已絕跡**：52 個近期 session、369 個 agent dir 全數無 transcript.jsonl（新 layout 只留 `metadata.json`＋`output.txt`；rollout model-io 僅暫態殘留 3 檔，屬本次調查自身家族）。**窗口內 spawned agent 的逐步工具行為已無法從 agent dir 回溯**——這本身記為治理斷點 BP-6。
- 因此實際行為樣本改用兩個有完整行為紀錄的穩定面：
  1. **delegate-bridge job jsonl**（`/Users/ctai/Github/*/.delegate-bridge/jobs/job-*.jsonl`，窗口內 604 個）：zcode-face job 記 `tool.updated`（toolName＋input），codex-face job 記 `item.command_execution`（完整 shell 命令）。
  2. **zcode agent metadata.json**（spawn prompt 全文＋description＋profileSnapshot）＋output.txt（最終報告）。
- 偵測「實呼 CR」用嚴格口徑：命令欄位含 `code-reality <subcommand>`（cr-query「滲透量測」建議的 subcommand 錨）；`aggregated_output` 內的 skill 文字 echo 不計。

## 1. 「該用 CR」條款清單（判定準據）

| # | 條款原文（節錄） | 出處 |
|---|---|---|
| C1 | 「搜尋前分清符號/引用/呼叫鏈與字串/config：**符號優先 code-reality（refs/callers/closure**，核對 [SRC] provenance/stale）」 | ai-guide `rules/symbol-query-routing.md:9` |
| C2 | 「涉及依賴/引用/fan-in/消費者/呼叫鏈/跨域/…查詢，**第一步確認 cr 在場**（MCP 或 `.code-reality/graph.db`）」 | 同檔 `:13`（任務啟動 gate） |
| C3 | 「index 缺/過期且不可重建→LSP；LSP 亦缺才 rg…**禁把未查到斷言為不存在**」 | 同檔 `:17` |
| C4 | 「Symbol callers (direct) → code-reality MCP `callers`…；Transitive blast radius → `impact_radius`（LSP can't do transitive efficiently）」 | ai-guide `skills/cr-query/SKILL.md:47,51`（分工表） |
| C5 | 「A review/planning command that expects the engine …and finds it absent must emit a one-line `[WARN] graph not available`…**Silent fallback = the user gets a worse review without knowing why.**」 | cr-query `SKILL.md:27`（GATE） |
| C6 | 「claim 必須查證…**依賴順序與 dead code 宣稱 → 依查詢路由取得 import／引用／呼叫鏈證據**」 | ai-guide `skills/review-engine/SKILL.md:62` |
| C7 | 真實案例：「審查者 rg 稱『無建構點』→ LSP findReferences 立刻列出…**rg pattern 失誤，把『自己沒查到』誤判為『程式碼不存在』**」 | review-engine `SKILL.md:88` |
| C8 | 「**CR freshness preflight**：風險 profile 判定後、review 腿 spawn 前跑 <1s 機械檢查——①graph.db 存在性；②`code-reality freshness --repo <root> --json`」 | review-engine `SKILL.md:182`（執行預設點 9） |
| C9 | 「**CR 接線查證段（硬性必含）**：diff 含 callable 新增/修改且命中觸發面…spawn prompt 必含接線查證指令…**negative verdict（零消費者/唯一 caller/可刪）永遠不可用 rg 宣稱**」 | review-engine `SKILL.md:190` |
| C10 | 「實測（2026-09-04～06）…spawn prompt 未明示時 reviewer 幾乎不觸發（rg-only）；spawn prompt 明示 CR 步驟的 agent 則穩定重度使用——**prompt 明示是唯一被實證的觸發形態**」 | review-engine `SKILL.md:191` |
| C11 | 「delegate-bridge review material（`review --base` **自動附掛 code-reality graph-checked 機械證據段**…material 尾段恆以四態 marker 之一收尾 `[cr:present]/[cr:empty]/[cr:unavailable]/[cr:skipped]`」 | review-engine `SKILL.md:198-217` |

## 2. 量化概況（窗口內）

- 近期 bridge jobs：604（southchariot 256、ai-guide 151、delegate-bridge 98、mosaic_alpha 43、sc-router 24…）。
- 文字提及 code-reality 的 jobs：273；**實際呼叫 CR CLI（嚴格口徑）：3**（zcode-face 無 shell 面，另計）。
- review/judge 性質 jobs（prompt 關鍵詞篩選後人工核對）：74；其中**有 CR 消費行為的僅 3**：
  - `job-mudcow7p-szk8yw`（SC-222 judge）：CR MCP 24 呼叫（refs×8、detect_changes×4、semantic_search×6、get_minimal_context×2、impact_radius×2、affected_flows×2）
  - `job-mudgq0h5-3aicyp`（judge）：CR MCP semantic_search×12
  - `job-muf55w7g-bx5je2`（advisory 弧）：CLI `freshness --repo … --json`＋`graph_query impact_radius --files …`
- **bridge review 四態 marker 實附掛：0/604**（所有 `[cr:*]` 字樣皆為 prompt/回應引用 review-engine 條文文字，非 material 尾段實附）。
- **freshness preflight（C8）實跑：窗口內僅上述 1 例**；另有 1 job 只做了 `test -f graph.db` 存在性半步（`job-muh65o1c-qb3mbj`，SC-264 R1）。
- CR 設施在場性（審查當時）：southchariot graph.db 10.7MB（09-25 15:35）、mosaic_alpha（09-25 23:25）、ai-guide（09-25 17:14）皆在場；**sc-router 無 graph.db**；**delegate-bridge 主 repo 有、其 worktree（delegate-bridge-db-33 等）無**（`.code-reality/` gitignored 不隨 worktree 產生）。

## 3. Case 清單（12 案例；「該用」判定逐條掛 C# 條款）

### Case 1 — SC-234 review：Grep 盤點「單一源 predicate」消費面
- 錨點：southchariot `job-mue76u0i-grhjmp.jsonl`（prompt 100 字、GLM carrier @ api.z.ai anthropic）
- 問題：驗證 worker 宣稱「`isEffectivelyPinned()`（sessions.ts:2816）單一源 valid-pin predicate」、menu contextValue 窮舉「無漏網」——典型 fan-in/negative verdict（觸 C9 觸發面：單一源宣稱＋介面變更）。
- 手段：174 Read＋32 Grep。pattern 原文：`refreshDbPinnedRows|dbPinnedRows`、`extraPinnedIds`、`isEffectivelyPinned|pinVerdictOf|freshPinVerdictOf|已釘選`、`'southChariot.archiveSelected'`。
- 該用：MCP `refs`/`callers`（C1、C4、C9）。graph.db 在場。
- 歸因：**(c)** prompt 未注入 CR 接線查證段（C10 預言的形態）。

### Case 2 — SC EP 審查：Grep 手畫 import graph
- 錨點：southchariot `job-mue5ia7a-q54ci2.jsonl`
- 問題：`from "./paths|from "../`、`^import|^} from` 人工盤點模組依賴與 migration 掛載。
- 手段：86 Grep＋36 Read＋16 Glob。
- 該用：`affected_flows`／graph 模組邊（C1）；EP 審查屬 cr-query when_to_use 明列場景。
- 歸因：**(c)**。

### Case 3 — SC review：跨模組符號鏈人工追
- 錨點：southchariot `job-mudzkzin-y1nnrw.jsonl`
- 問題：`carrierTwinKeys|sessionKeyOf`、`bridgeSessionKeys(|bridgeKeys`、`clusterByHarness` 跨模組結構。
- 手段：160 Read＋8 Grep。
- 該用：`callers`＋跨模組加 `impact_radius`（C9 原文「跨模組變更加 impact_radius」）。
- 歸因：**(c)**。

### Case 4 — sc-263 Judge：finding 查證全靠 Grep
- 錨點：southchariot `job-muh3es3o-e9avk0.jsonl`
- 問題：judge 驗 findings——`buildLiveBridgeRows|bridgeRunningCount`、`async pollOnce|pollOnce(`、`const bgPoll|bgInFlight = true`。
- 手段：44 Grep＋16 Read。
- 該用：`refs`/`callers`（C6 查證路由）。對照組：同 repo 同 carrier 的 SC-222 judge（Case 11）證明此面 CR 可用。
- 歸因：**(c)**。

### Case 5 — SC judge：查證義務進了 prompt、查證路由沒進
- 錨點：southchariot `job-mucplv90-qq9p55.jsonl`；prompt 原文「唯讀核對 file:line——**每條 Important 必須實際讀源碼驗證**」
- 手段：58 Read＋20 Grep＋8 Glob。
- 該用：fan-in 查證 → CR（C6/C9）。
- 歸因：**(c)** 變形——C6 的義務半句被轉譯進 prompt，C9 的路由半句沒有；結果「必須實際讀源碼」被執行成「Read＋Grep」。

### Case 6 — SC READ-ONLY review（88 Read＋18 Grep）
- 錨點：southchariot `job-mudxti9h-zcxhjc.jsonl`。prompt 同 Case 1（模板化 100 字）。歸因 **(c)**。

### Case 7 — SC READ-ONLY review（76 Read＋10 Grep）
- 錨點：southchariot `job-mue1itb4-iwhoh8.jsonl`。歸因 **(c)**。

### Case 8 — sc-router Arbiter：無 graph 場的靜默降級
- 錨點：sc-router `job-mug6vf95-0sqbd2.jsonl`
- 問題：`def (transfer|release|reclaim)`、`DELIVERABILITY_VALUES` 消費面裁決。
- 手段：42 Read＋18 Grep。sc-router **無 graph.db**——降級本身合法（C3），但 C5 GATE 要求的 `[WARN] graph not available` 未由 carrier 發出（job 內 WARN 字樣僅為 cat 進來的 skill 文檔內容，非主動警示）。
- 歸因：**(d)** 主因（引擎確實缺場）＋**C5 GATE 未實現於 carrier 面**（設計斷點 BP-3）。

### Case 9 — mosaic MOS-129 Judge：Python repo（CR 最強面）照樣 Grep
- 錨點：mosaic_alpha `job-mudqlrv9-uvmbd4.jsonl`
- 問題：`bsr|broker|nightly_bsr`、`nightly-sequence Op5|Op5 22:00` 引用面裁決。
- 手段：32 Read＋22 Glob＋18 Grep。mosaic graph.db 在場（09-25 23:25，Python＝pyrefly 生產面）。
- 歸因：**(c)**。

### Case 10 — worktree fresh-eyes review：engine 結構性缺席
- 錨點：zcode agent `agent_2ecdd9bd`（sess_2bfc6f55，DB-33 fresh-eyes review of delegate-bridge-db-33 worktree）；對應 bridge 系列 job 在 `delegate-bridge-db-33/`。
- 事實：spawn prompt CR mention=0；`delegate-bridge-db-33/.code-reality/` **不存在**（主 repo `delegate-bridge/.code-reality/graph.db` 在場——`.code-reality/` gitignored，worktree 不繼承）。
- 該用：review 腿按 C2 第一步「確認 cr 在場」→ 在 worktree 恆為否；按 C5 應 WARN＋考慮 `--repo` 指向主 checkout（條款未定義 worktree 場景）。
- 歸因：**(a) 結構變形**——非「agent 沒有工具」而是「派工拓撲使工具恆缺席」且條款對此場景無處方（設計斷點 BP-2）。

### Case 11（正向對照）— SC-222 judge：CR 在 inventory 就會被用
- 錨點：southchariot `job-mudcow7p-szk8yw.jsonl`；prompt CR mention=0（brief-judge-sc222 亦無強制）。
- 行為：CR MCP 全套 24 呼叫＋Grep×28 併用——refs×8、detect_changes×4、semantic_search×6、impact_radius×2、affected_flows×2、get_minimal_context×2。
- 義：反證「carrier 不會自發用 CR」——**GLM carrier 在 CR MCP 出現在工具清單時會自發重度使用**；window 內多數 review job 沒用它，是因為該 spawn 的工具 inventory 裡沒有 CR（同一 carrier、同一 API endpoint，可用性隨 spawn 浮動——BP-1）。

### Case 12（正向對照）— advisory 弧：brief 有 CR 指引就全鏈消費
- 錨點：southchariot `job-muf55w7g-bx5je2.jsonl`＋`.agent-tmp/mermaid-panzoom/advisory-brief.md`
- 行為：cat cr-query SKILL.md → `test -f graph.db` → `code-reality freshness --repo … --json` → `graph_query impact_radius --files …`——C8 preflight＋C9 查證的完整實踐，全窗口唯一。
- 義：與 C10「prompt 明示是唯一被實證的觸發形態」一致；brief 寫了 CR 步驟，carrier 就照跑。

## 4. 歸因分布（12 案例）

| 歸因 | case 數 | cases |
|---|---|---|
| (c) 派工 prompt 未注入 CR 指引 | 8 | 1-7、9 |
| (c) 變形：查證義務注入、路由沒注入 | 1 | 5 |
| (d) 引擎缺場（合法降級，但 GATE WARN 未發） | 1 | 8 |
| (a) 結構變形：worktree 派工 engine 恆缺席 | 1 | 10 |
| 正向對照（不計入摩擦） | 2 | 11、12 |

- (b)「有工具但沒想到」在 bridge 面幾乎不成立：Case 11 顯示工具在場即被使用。
- (e)「主動判斷 rg 更快」未發現明確證據（無 job 在比較後棄 CR）。
- (f)「試過 CR 但失敗」窗口內未發現。

## 5. 設計斷點清單

### BP-1｜Carrier 工具 inventory 浮動，CR 可用性成擲骰，receipt 不記錄
同一 GLM carrier（api.z.ai anthropic endpoint）：`job-mudcow7p` 有 CR MCP（108 次字串出現、24 呼叫），`job-mucmyie7`／`job-mue76u0i` 等 60+ review jobs 完全無 `mcp__plugin_code-reality` 字樣（MCP 一般面如 `mcp__node_repl__js` 卻在場）。CR plugin 是否掛進某次 carrier spawn，在 dispatch receipt／job descriptor 無任何記錄。消費面後果：review-engine 點 9 的「fresh=true → 正常派發」前提在派工面不可觀測。

### BP-2｜Worktree 派工使 engine 結構性缺席，條款無處方
`.code-reality/` 是 repo 內 gitignored sidecar（單一 `*` .gitignore）；bridge 派工與 agent spawn 高度使用 per-card worktree（db-33、mos-126、southchariot-sc-234/-264…），worktree 內恆無 graph.db/scip。C2「第一步確認 cr 在場」與 C8 preflight 對「在場判定失敗後怎麼辦」只給了 WARN＋fallback，未定義「以 `--repo <主 checkout>` 借用 graph（identity 口徑如何記）」或「worktree 內重建」的擇一。實證：Case 10＋Case 8。

### BP-3｜cr-query GATE（C5）只約束讀 skill 的 LLM，carrier 沒有機械 WARN
`[WARN] graph not available` 落點是「review/planning command」的行為，但 bridge carrier（codex/glm/muse runtime）不載 ai-guide skills 時，GATE 無宿主。Case 8 中 skill 文字被 cat 進來（被動知曉），carrier 仍未發 WARN——條款無 carrier-side 機械載體（類似 AIR-135.3「startup hook 只 advisory；correctness gate 在 consumer boundary」的洞：consumer boundary 上沒有 gate）。

### BP-4｜「硬性必含」的 C9 接線查證段在派工面無機械閘
窗口 74 個 review/judge jobs，prompt 含 CR 指引者 1（且是 meta 工作——`job-mudnho34-ur96kw`，其 prompt 本身在修 bridge review CR-attach 機制）。SC 系列 R1-PROMPT.md／backfill-review brief 模板（如 `.agent-tmp/sc264-review/R1-PROMPT.md`，code-reality mention=0）不含接線查證段。條款寫「硬性必含」，但派工模板產生器（marshal/main session）無 preflight gate 強制；C10 早已自證「prompt 未明示 → rg-only」——設計已知解方，接線未閉合。

### BP-5｜bridge review 自動附掛面（C11）零使用
604 jobs 中 `[cr:present]` 等 marker 實附掛為 0（全部命中皆為條文引用文字）。派工實際都走 plain task＋promptFile；`review --base` 的 CR material attach 在窗口內未發生過。機制存在、消費契約完備（judge 收件驗證步驟也寫了），但沒有任何一條真實 review 走過這條路——機制與實際派工路徑脫節。

### BP-6｜窗口內行為證據面消失，治理無法回溯稽核
agent transcript.jsonl 自 2026-09-22 前後停寫（新 layout 只留 metadata＋output），rollout model-io 為暫態。後果：本軸被迫以 bridge job jsonl 為主證據面；zcode-face spawned review agent（AIR 系列審查腿、db-33 fresh-eyes 等 40+ 個）的逐步工具行為永久不可考。CR 滲透量測（cr-query「滲透量測」節）的 MCP 面計數在未來窗口將只能依賴 bridge jobs——若 bridge 派工也換面，量測失去錨點。

### BP-7｜freshness preflight（C8）無機械化，事實上靠記憶
點 9 要求「風險 profile 判定後、review 腿 spawn 前」跑兩步檢查；窗口內唯一真實執行在 Case 12（advisory 弧，brief 有寫才跑）。主 session 側（ZCode）無 hook／gate 在派工前強制此檢查；且主 session transcript 已不可考（BP-6），連「有沒有跑過」都無法稽核。

## 6. 結論一句話

條款把 CR 定為 review 的 A 級機械證據來源，但窗口內 74 個 review/judge jobs 只有 3 個碰過 CR；摩擦主因不是 agent 不聽話（Case 11 證明工具在場即自發使用），而是**派工層沒有把 CR 工具與 CR 指令穩定送到 review 腿面前**（inventory 浮動、worktree 缺席、prompt 模板未含接線查證段、attach 面零使用），且這條鏈上沒有任何機械 gate 會在缺場時發聲。
