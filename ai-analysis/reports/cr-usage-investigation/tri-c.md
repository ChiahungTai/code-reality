# 軸 C 詰問：對三軸合併 findings 的條款對照軸裁定

調查日：2026-09-25。證據基礎：axis-c.md（12 case＋條款清單 C1-C11＋BP-1~7）＋本輪補驗。

## 0. 本輪補驗（裁定前的事實修正）

- `brief-judge-sc222-fixverify.md`：code-reality/scip_refs/callers/impact **0 次命中**——Case 11（job-mudcow7p，CR MCP 24 呼叫）的 prompt 與 brief 全鏈零 CR 字樣實錘。自發使用的唯一殘餘驅動＝**carrier 工具清單裡有 CR MCP**（＋carrier 載入的 AGENTS.md）。
- job jsonl 的 `session.updated` **不記錄 offered tools／mcpServers**（job-mucmyie7 驗證：payload 僅 toolCallCount/toolCallId/toolName 等呼叫面欄位）。「CR 工具有沒有被提供」在窗口內無法從 job 檔直接觀測，只能從「呼叫有無」反推——這是所有歸因的共同認知界線，下方裁定據此措辭。
- zcode `general-purpose` profile systemPrompt（metadata.profileSnapshot 實引）：「Your strengths: Searching for code, configurations, and patterns across large codebases…」——**profile 本體以 rg 式搜尋自居、零 CR 提示**；`injectAgentsMd: true` 意味 rules（symbol-query-routing）在 context，但 profile 描述的行為錨更近。

## 1. Q1 裁定：注入不是必要條件；「存取」才是

**裁定**：合併 findings 的歸因框架需要一個修正——把「prompt 注入」從原因層降級到手段層。窗口證據支持的三層模型：

1. **必要條件＝CR 以某種形態「可及」**：(i) MCP 工具在 carrier 清單（glm/zcode face）；(ii) CR 證據已附掛在 material（muse attach → detect_changes ×75）；(iii) CLI 在 shell PATH＋prompt 給了命令（codex face，唯一實例 job-muf55w7g）。三者任一在場，使用就發生。
2. **注入是「無 MCP surface 載體」的必要條件、「有 MCP surface 載體」的放大器**。review-engine:191（我軸條款 C10）「prompt 明示是唯一被實證的觸發形態」在 2026-09-04~06 的實測語境下成立，但窗口內的 15/16＋Case 11 顯示：當工具面在場，GLM carrier 不需注入即自發使用——**C10 需要改寫**，否則它會把修復努力錨在「寫進 prompt」而不是「保證可及」。
3. **「條款在場＋工具在場」自發使用不是普遍穩定路徑，是載體條件路徑**：
   - bridge-glm：穩定（軸 A 15/16＋我軸 Case 11 零注入全鏈使用）。
   - zcode Agent-tool subagents：**不穩定**（軸 A：review 相關 2/54、reviewer 家族在場零使用 40/42）——儘管 `injectAgentsMd: true`（條款在 context）且 tools=["*"]。差異解釋：profile systemPrompt 以「Searching…」行為錨起步、無 repo_root/圖譜慣例指引、review spawn prompt 密集導向 Read/diff，子代理在 context 預算壓力下收斂到最便宜工具。**同一 harness、兩條 spawn 路徑、兩種結果**——「在場」必須細分為「清單在場」＋「行為錨在場」，缺一行為錨即塌縮。
   - codex face：條款與工具雙缺（摩擦 1、3），自發使用不可能。

**我軸 12 case 的再歸因**（誠實修正）：
- Case 1-7、9 原標 (c)「派工 prompt 未注入」——主因判定不變（修復點仍在派工面），但機制描述應改為「**派工面未保證 CR 可及**（清單未掛＋未附掛＋未給 CLI 命令三者皆缺）」。注入與否是表象；job-mucmyie7 與 job-mudcow7p 同 carrier 同 repo 同周，一個用了 CR 一個沒用，差異只能在 spawn 時的工具裝配——而這件事 receipt 不記錄（我軸 BP-1，與摩擦 5 合流）。
- 「是否 offered 但未用」對零呼叫 job 不可觀測（本輪補驗 2）；但對照 15/16 的自發使用率，大面積零呼叫更支持「未裝配」解釋。此為推斷，標 `inferred`。
- Case 8（sc-router 無 graph）、Case 10（worktree 無 sidecar）歸因不變，且與摩擦 2 合併。

**對摩擦點池的個別詰問**：
- 摩擦 1（codex 無 CR MCP）：支持，但補充——codex 有 shell 且 CR CLI 已在機器上（job-muf55w7g codex-face 跑過 `code-reality freshness --json`），「結構性無 MCP」不等於「結構性無 CR」；修法優先走附掛/模板注入而非給 codex 接 MCP。job-mubykur5 路由到 codex 卻叫它用沒有的工具＝派工 resolver 沒有「能力×載體」檢查，這是 model-routing no-silent-downgrade 在 CR 面的缺座。
- 摩擦 4（graph.db 存在性劇場）：支持並升級——`test -f` 半步正是 C8 條款被「複製文字不複製語義」的結果：點 9 寫了兩步（①存在性 ②freshness face），模板抄了①丟了②。條款給了可機械求值的面（`freshness --json`），模板卻停在 shell 存在性——這是 instruction drift 的 code-side 鏡像。
- 摩擦 6（heal 40.9s 後 refs 無 DEF）：支持其「信任損害」效應，且它踩中 cr-query 已知邊界——DEF 只收函式/方法、struct 名不可作查詢鍵（ai-guide code-reality SKILL.md 工具表 scip_refs 行）。carrier 拿符號就查、不知道查詢鍵語法，單點失敗即放棄——工具面條款（plugin skill code-reality-tools）存在但 carrier 不載。歸為「查詢契約未隨工具面傳播」，量級小但對信任侵蝕不成比例。
- 摩擦 7（transcript 09-04 停寫）：與我軸 BP-6 一致，補充——這對 CR 調查本身是二階傷害（未來滲透量測失去 MCP 面錨點），應入治理 backlog 而非僅記錄。
- 軸 A「注入率 78.3% 零 CR」與我軸「74 review jobs 僅 1 prompt 帶 CR」：同向，數字差異是篩選口徑（review 性質 313 vs 我的人工核對 74），無衝突。

## 2. Q2：按「修掉後使用率升幅」排序的前三

**#1 派工面機械閘：review 派工模板強制帶 CR 段＋dispatch receipt 記錄 CR 可用性（摩擦 3、4、5 合併）**
覆蓋 313 review jobs 全載體。理由：(i) muse attach ×75 證明「證據送到面前就被消費」；(ii) codex CLI 可跑（muf55w7g）證明注入即可用，不需新基建；(iii) 15/16 證明 glm 面只要不拆台就自發用。三載體的修復都收斂到「派工生成當下把 CR 段＋可用性行機械化」。升幅估計：review 性質 4.8% → 過半（attach 面給 muse、注入面給 codex、glm 維持自發＋模板保底）。

**#2 review face（`review --base` attach）成為 review 語義派工的唯一路徑（review face 0/604）**
與 #1 是同一閘的兩半，但 attach 面價值獨立：它是唯一「carrier 不需要任何 CR 知識」的通道——機械證據直接進 material，judge 收件驗證步驟（review-engine 四態消費表）契約已寫好，只是從未被餵食。0 → 全量 review 派工的升幅即 #1 的下限保證。

**#3 worktree index 處方（摩擦 2）**
worktree 派工在 bridge 與 agent 面都是常態（db-33、mos-126、sc-234/-264…）。不修此點，#1/#2 的模板與附掛在 worktree job 上首跳即 `[cr:unavailable]`——摩擦池自己的數據（首跳 [FAIL]）。修法便宜（見 Q3-P3），解鎖的是「所有 per-card worktree 審查」這一整類。

不入前三但記錄：存在性劇場（4）併入 #1 的模板修正；指引不對稱（3）併入 #1；heal DEF（6）修 cr-query 查詢鍵指引隨 attach 傳播即可；transcript（7）是觀測性債，不直接升使用率。

## 3. Q3：ai-guide 設計改動草案（條款對照軸視角）

**P1｜改寫 review-engine C10＋把 CR 段從「條款」降為「派工模板的機械輸出」**
- 草案文字：review-engine「spawn prompt 工具紀律」節加一行——「本段由派工產生器機械注入，不由 marshal 記憶重抄；dispatch receipt 必含 `[cr-surface: mcp|attach|cli|absent]` 行」。同步改寫 :191 為雙通道表述：「MCP surface 在場時 GLM 載體零注入即自發使用（2026-09-24 SC-222 實證）；surface 缺席時 prompt 注入 CLI 命令為必要條件（2026-09-04~06 實證）」。
- 為何現況失敗：78.3% 零注入＋我軸 case 1-7（模板化 100 字 prompt）＋job-mubykur5（路由到無該工具的載體）——派工 resolver 沒有「能力×載體」檢查，違反 model-routing no-silent-downgrade 精神。
- 條款出處：review-engine 執行預設點 9＋「spawn prompt 工具紀律」節；rules/model-routing.md。

**P2｜review face 收斂：review 語義派工必經 `review --base`（或 plain task 強制附掛＋receipt 記錄降級）**
- 草案：bridge-dispatch skill 加一條——帶 review 意圖的 `bridge_task`（prompt 含 review/judge/審查關鍵詞或由消費命令標注）不走 review face 時，wrapper 顯式警告＋receipt 記 `[cr-attach: skipped]`；judge 收件驗證步驟（四態 marker 表）已有 fail-closed 語義（marker 缺失照 `[cr:unavailable]` 處置），直接可引用。
- 為何現況失敗：0/604 全走 plain task；消費契約（review-engine「bridge review 的 CR 證據分類與消費」節）寫了但從未被餵食——機制與實際派工路徑脫節。
- 條款出處：review-engine「bridge review 的 CR 證據分類與消費」節；bridge-dispatch skill（MCP face／派工必配回收節）。

**P3｜worktree 處方條款：cr-query「Fallback」節＋review-engine 點 9 各補一段**
- 草案文字：「spawn cwd 為 worktree 且 `.code-reality/` 缺場時，預設以 `code-reality freshness --repo <主checkout絕對路徑> --json` 判讀，並在 findings 標 `serves=committed-baseline`（worktree WT delta 以 live LSP 為準）；禁在工作派工前同步 rebuild」。freshness face 的 `serves` 欄位語義現成（AIR-135.2 判準已定義 committed-baseline 態），不需要新機制。
- 為何現況失敗：`.code-reality/` gitignored → worktree 恆缺場（Case 10 實測 delegate-bridge-db-33）；條款只給 WARN＋fallback，未定義 worktree 情境的 repo 指向，軸 A 首跳 [FAIL]/[cr:unavailable] 即此洞的直接後果。
- 條款出處：cr-query「Fallback — engine absent or stale」節；review-engine 執行預設點 9；rules/symbol-query-routing.md「第一步確認 cr 在場」（在場判定失敗後的處方缺座）。

**P4｜存在性劇場矯正：模板中刪 `test -f graph.db` 半步，freshness face exit code 直接映射消費動作**
- 草案：所有 CR preflight 指引統一為 `code-reality freshness --repo <root> --json`——exit 0（fresh）正常派發／exit 1（stale）降級收據＋背景 rebuild／exit 2（no-slot）依 P3 worktree 條款。GATE WARN 文字由 exit code 機械產生，carrier 無需判斷。同時把 scip_refs 查詢鍵語法（DEF 只收函式/方法）寫進 attach material 尾段固定一行，防摩擦 6 型單點放棄。
- 為何現況失敗：23 codex job 只 test -f 就改 git/grep（摩擦 4）——C8 兩步條款被複製了一步；唯一全鏈正確實踐（job-muf55w7g）恰是 brief 完整攜帶 cr-query 步驟的弧，證明「給對步驟就走對」。
- 條款出處：review-engine 執行預設點 9（兩個誠實前提段）；cr-query「Stale graph check」與「Detect the engine」GATE 節。

## 4. 一句話結論

三軆數據收斂到同一個裁定：**瓶頸不在 reviewer 的意願或條款的理解，在派工層沒有把「CR 可及性」變成機械事實**——把可及性（surface/attach/CLI）做成派工產生器的必填輸出＋receipt 欄位，注入率、存在性劇場、載體路由錯配三個症狀同時消失。
