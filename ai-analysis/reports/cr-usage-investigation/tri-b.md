# 軸 B 詰問：對三軸合併 findings 的證據核對與回答

軸 B 樣本：369 zcode subagents（09-22..26）；證據面＝每日 log `tool.call.*` 事件（MCP 硬證據）＋ metadata.json（prompt/profileSnapshot.tools/systemPrompt）＋ output.txt（CLI 實跑引述）。報告：`axis-b.md`。

## Q1 逐點核對

**點 1（codex carrier 無 CR surface）— 無直接證據，同構類比成立。** 軸 B 無 codex carrier；同構案例＝cross-verify-investigator：systemPrompt 有 CR 條款（`agent_d3abfdb3` metadata `profileSnapshot.systemPrompt:19`：「cr | code-reality graph（refs／callers／closure／impact_radius…）| code-reality CLI 經 Bash」）但 tools 清單僅 Read/Bash/WebFetch/WebSearch → 3/3 零使用。「有任務、無 surface」在 subagent 軸同樣成立。

**點 2（worktree 缺 index）— 確認，且比例比合併 findings 更尖銳：attempted→failed＝4/4。** 軸 B 內凡實際對無 index 目標發 CR 呼叫者全數首跳失敗：
- `26b17ad9`（code-reviewer，delegate-bridge-db-43 worktree）output.txt：「CR index 缺席（僅 `.code-reality.toml`，無 `.code-reality/graph.db` / scip index）」；其唯一 callers 呼叫 log 狀態 completed（≠ 結果成功；log 無 result payload，無法區分）。
- `6b0a52da`（primed，同 worktree）同報告。
- `5309e90e`（lite-verify，southchariot-sc-223 worktree）：「CR MCP 對該 worktree 查證失敗：`[FAIL] 預設索引不在：…/southchariot-sc-223/.code-reality/scip/index.scip`」→ 自發降級改查 main repo slot（窗口內唯一自發借用處方者）。
- `9c2b6da2`（general-purpose，CLI）：`scip_refs --repo …/sc-router` → `[FAIL] 預設索引不在`。
結構性面補充：reviewer 家族 spawn cwd 在 worktree（delegate-bridge-db-NN）者 6/42（11669b6f、2ecdd9bd、19cb19a2、26b17ad9、6b0a52da、c87ad11e）——其中 2 個嘗試者 100% 撞牆，其餘 4 個未嘗試（未注入，見點 3/5）。「無 --repo 借用/重建處方」確認：無一個 systemPrompt/模板載明借用 main repo slot；5309e90e 是自行發明。

**點 3（指引不對稱）— 確認，數字即本軸產出。** 42/42 reviewer `profileSnapshot.systemPrompt` 含「優先 code-reality MCP（refs/callers/closure/impact_radius）＋核對 provenance/staleness」；spawn `.prompt` 僅 6/42 重申（26b17ad9、6b0a52da、30ebaffc、de2f8f14、d543823e、d0b18245）。cross-verify-investigator「有條款沒工具」見點 1。

**點 4（graph.db 存在性劇場）— 部分支持（退化同構、機制不同）。** subagent 軸未見逐字 `test -f` 劇場；但同構退化在場：26b17ad9 記下 index 缺席後全審查走 git/grep；6/38 reviewer output 明示 rg 工作流（rg proxy，軟證據）。無人嘗試重建或借用——劇場的 subagent 讀本。

**點 5（派工面無機械閘）— 確認，且本軸有最強因果梯度。** reviewer 家族 CR 使用 2 例全部落在注入子集：注入 6 個中 2 個使用（2/6），未注入 36 個中 0 個（0/36）。profile 級 systemPrompt 42/42 在場卻只產出 2/42 使用——證明「常駐條款無閘無效，spawn 級注入才有效」。

**點 6（glm heal 40.9s）— 無法核對。** 軸 B 無 heal/build 事件可觀察（subagent `build` 呼叫 0 次；16 次 build 全在 main session）。無反駁證據。

**點 7（transcript 停寫）— 確認並加劇。** transcript.jsonl mtime 直方圖止於 09-04（41 檔）；且**每日 log 也輪轉**：`~/.zcode/cli/log/` 現存最早為 09-20（另有一枚 08-26 reset-launch 雜檔）——tool.call 級證據窗僅 ~7 天。稽核基礎建設雙面失效：行為錨點（transcript）停產、事件錨點（log）短留；僅 metadata.json/output.txt 持久。

## Q2 摩擦點按「修掉後使用率升幅」排序

1. **點 5（派工注入/機械閘）**：本軸唯一強相關變量——全部 reviewer 使用落在 6/42 注入子集（2/6 vs 0/36）。加上 78.3% review prompt 零注入（軸 A），機械閘（preflight 必含＋跨檔宣稱至少一次 graph 查證）把使用率下限直接抬到「每 review ≥1 呼叫」，量級遠大於其他點。scip 面已有自動 heal，preflight 成本趨零，阻力主要在 prompt 而非工具。
2. **點 2（worktree index）**：attempted→failed 4/4＝100% 首跳失敗率；只修注入不修此點，注入的 review 會在首跳失敗後退回 grep（點 4 劇場），升幅被吞掉。借用 main repo slot 處方（5309e90e 實證可行）＋派工前 build 是低成本的配套。
3. **點 3（指引不對稱/有條款沒工具）**：升幅第三但近零成本——cross-verify-investigator 補 4 工具（對照 SC-222 judge：有 surface 即自發 24 呼叫）、spawn 模板重申 CR 行（現 6/42）。
（點 4 是 2+5 的下游效應不單列；點 1 限 bridge 軸、點 6 小樣本、點 7 是稽核基建非使用率。）

## Q3 ai-guide 設計改動草案（subagent 軸視角）

1. **spawn 模板「CR preflight 必含段」**（落點：review-engine 派工契約 + code-reviewer/code-reviewer-primed/cross-verify-investigator agent 定義檔的回報格式）：模板加一行「先跑 `code-reality freshness --repo <main-checkout-abs-path> --json`（CLI 即可，不需 MCP）並把 verdict 寫進回報首節；跨檔宣稱（呼叫者/引用/影響面）需 ≥1 次 refs/callers 查證，不可用時 finding 標 `cr:unavailable:<理由>`」。現況失敗證據：freshness preflight 全窗口 subagent 僅 1 次（agent_64082453）；2/6 vs 0/36 注入梯度證明 spawn 級文字才是行為觸發器。
2. **worktree index 處方條款**（落點：cr-query skill「Stale graph check」節 + reviewer systemPrompt）：明文兩條——(i) spawn cwd 在 worktree 時 `.code-reality/` 缺席屬預期，`--repo` 改指 main checkout 絕對路徑借用其 index（5309e90e 實證）；(ii) 需要工作樹粒度時由 marshal 端派工前 `code-reality build --repo <wt>`。現況失敗證據：4/4 首跳失敗、無一處方條款在場、唯一成功 workaround 是 agent 自發發明。
3. **cross-verify-investigator profile 補 surface**：tools 清單加入 refs/callers/closure/impact_radius 四 MCP 工具（fallback CLI 條款已在）。現況失敗證據：systemPrompt:19 有條款、tools 無工具 → 3/3 零使用；正面對照＝SC-222 judge 有 surface 即 24 呼叫。
4. **consumer-boundary 回報閘＋稽核 sink**：reviewer 回報格式必含「CR 呼叫清單或 cr:unavailable 理由」欄（驗收在消費端，符合 AIR-135.3 advisory-startup）；另將每日 log 的 CR `tool.call` 事件定期快照進持久 sink（repo `.agent-tmp/` 或 audit 檔）。現況失敗證據：95.2% 在場不用證明無閘條款無效；transcript 09-04 停寫＋log ~7 天輪轉使任何後續使用率稽核無錨。

## 修正/保留意見（對合併 findings）

- 「reviewer 家族在場零使用 40/42」成立，但應補注：2 個使用者的呼叫結果無法從 log 判定成功（log 只記 completed），且其目標 worktree 無 index——「使用」實為嘗試。
- 「freshness preflight 1/604」與軸 B 一致（subagent 軸 1 次＝64082453）。
- glm 正面鏈（15/16 零注入仍使用）不能外推到 subagent 軸：本軸 42/42 systemPrompt 常駐條款只換來 2/42 使用——常駐指引≠動能，注入＋surface 兩者皆須在場。
