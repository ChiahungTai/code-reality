# cr-audit 修正使用方式有效性驗證報告（唯讀查證）

- 日期：2026-09-11 22:4x
- 觀察窗：09-08 ~ 09-11（受限制，見 §5）
- 方法：git show／rg 機械掃描；session 紀錄＝ZCode rollout＋agents output/metadata＋delegate-bridge jobs（ai-rules/code-reality/mosaic×3）
- 全程唯讀；暫存僅本檔

## ① 修正清單落地狀態表

| 修正項 | 內容 | 落地狀態 | 證據 |
|---|---|---|---|
| R1 負面宣稱證據 gate | review-engine＋judge-review 語義 gate | ❌ 未落地（AIR-67 段二 To Do） | backlog/tasks/air-67:6 `status: To Do`；notes 記段二= R1+R3 |
| R2 五步 ladder＋cr-research 角色修正 | 禁教 module-path、ladder 落三檔 | ❌ 未落地 | agents/roles/cr-research.md:23 仍寫「symbol 用 module path（pkg.mod.Symbol）」；skills/cr-query/SKILL.md rg 'ladder\|五步\|Class\.method' 零命中 |
| R3 research.md evidence carrier | EP 同層 references/research.md 條文 | ❌ 未落地（AIR-67 段二） | 同上卡 |
| R4 ai-rules .code-reality.toml | 補 profile＋smoke | ❌ 未落地 | ls .code-reality.toml → No such file（09-11 查） |
| R5 量測迴路換軌 | 事件觸發＋KPI 換軌 | ⚠️ 部分（AIR-66 Done 併流） | backlog/completed/air-66:6 `status: Done`（09-09 22:21）；0db17bc commit 訊息「R8/R5 合流」；cr_usage 窗口內僅被引用審閱（job-mtwh31x2 命中為 docstring 引用）、無實際執行產出；corrections-weekly 輸出 09-08 後 0 份 |
| R6 capability-aware injection | MCP-first 注入塊＋work-order.md:61 修正 | ⚠️ 條文未落地、實務已出現 | mosaic jobs 09-10~11 多檔含注入塊「prefer the code-reality query tools (refs/callers/closure/impact_radius)…fall back to text」「NEVER use write-face during review」（job-mtwonacc-fvfbai、job-mtwm5ik8-gu183q 等）；work-order.md 修正屬 AIR-67 段三 To Do |
| R7 fallback 可見化 | spawn auth 失敗回報＋重試 | ❌ 未 codify（段三 To Do）；窗口內無 cr-research spawn 失敗事件可觀察 | 同上卡 |
| R8 承諾網掃除 | trigger-based 降級＋觀察窗等 | ⚠️ 部分（AIR-66 Done） | project 觀察窗已落 cr-query SKILL.md:76「觀察窗（cr-audit R8）：零正面案例期，不擴接線面」；19 載體全掃除未見 |
| 版號綁定移除 | instruction 內工具版號全拆 | ✅ 落地＋部署 | 89eea7e（09-11 17:36，4 檔）；部署面 ~/.zcode/AGENTS.md rg 'v?0\.[0-9]+\.[0-9]'＝0 命中 |
| lsp-navigation→symbol-query-routing 改名 | 22 處引用對齊 | ✅ 落地＋部署 | 78d3372（09-08）；部署面（~/.zcode/AGENTS.md＋skills/＋.agents/skills/）rg 'lsp-navigation'＝0 命中 |
| 相對 --repo 禁令立→解（MOS-86） | 2806b54 立、be8ae2c v0.6.8 根修後解 | ✅ 落地 | 兩 commit stat 觸及 skills/post-build/SKILL.md；根修宣告在本機 0.6.8+5938aee 驗證 |
| 排程解綁＋launchctl 條款 | 17 處時刻/系統名拆、cr-query 補註 | ✅ 落地＋部署 | f9445b7（09-09）＋cea1e68（09-10）；部署版 ~/.zcode/skills/cr-query/SKILL.md:23「目前未部署、現值以 launchctl 為準」 |
| review agent 層 lite 化 | ep-review/code-review 預設 flash | ✅ 落地 | 1be4780（09-09）；09-11 spawn desc 實例「Primed dual-context 審查（flash）」「R4 primed flash review」（metadata.json） |

## ② 逐項有效性 verdict（行為面）

| 觀察軸 | verdict | 證據 |
|---|---|---|
| 1a. 符號查詢走 CR（ZCode 面） | ✅ 有效 | primed flash review 輸出 output.txt:40（agent_2d216742，09-11）：「MCP `callers`（repo_root=/Users/ctai/Github/mosaic_alpha）＝1 site…rg 互補一致…（MCP 回應附 [WARN]：scip-typescript 缺場、producer 版本錯配、index 落後 HEAD——已揭露非靜默）」——CR-first＋絕對 repo_root＋rg 互補＋誠實降級四點全中。今晚 sess_77327f21 兩個 review 子代理 rollout 亦有 mcp__plugin_code-reality 呼叫 |
| 1b. 禁「rg 掃不到→宣稱零消費者」 | ✅ 有正面樣本／無新違例實錘 | 正面：mosaic job-mtvokwlv-oloocz（09-10）EP 自書「以 current source＋rg 為證據，標成『未 index 驗證』…不能把本 EP 的文字盤點當 repo-wide 完整性證明」＝rules/symbol-query-routing.md:21 行為落地。候選反例：EP review B（agent_f6217e6a，09-09）「唯一 caller…屬實」輸出無 [SRC]——transcript 未持久化無法驗工具面，且 R1 gate 未落地，標 baseline 觀察 |
| 2. --repo 調用形態 | ✅ 有效（MCP 面）／⚠️ CLI 面無法觀察 | MCP 絕對 repo_root 實證見上；jobs 內 `--repo .` 命中 21 處全為 instruction 文本引用（「build --repo .`——stale」語境），非實際執行；v0.6.8 已根修，形態風險降級 |
| 3. 已落地 R 案遵循（R8 觀察窗） | ✅ 有效 | project 觀察窗條文在部署版 cr-query SKILL.md:76；窗口內無 project 常態化濫用跡象 |
| 4. muse 面行為 | ⚠️ 滲透仍低，但 bundle 藉口已消除 | 部署 bundle /Users/ctai/.config/muse/AGENTS.md＝32,040B（≤MUSE_USER_BUDGET 36KiB，deploy_agents.py:139）且含 symbol-query-routing 全文（:522-531）＋測試集機械反查（:502）——「沒載」抗辯不成立；muse-spark jobs 58/96 為主要外部流量，[SRC] 指紋僅 ai-rules 4/96、mosaic 2/39、lab 1/15 檔——低滲透與 cr-audit C2 基線（1.2%）相比無明顯躍升 |
| 5. 殘留訊號 | ✅ 無殘留 | ①版號：09-11 17:40 後 jobs 引用版號為權威＝0（2 個 spawn prompt 版本字串皆合法：base commit 記載/我自身任務引文）②23:20：09-10 後 18 檔命中全為 AIR-52 解綁/schedule-registry A1/A6 分離治理討論本身（job-mtuosbgg、job-mtwat6tv 等），無一例按 23:20 辦事 ③lsp-navigation 舊名部署面 0 命中 |

## ③ 觀察與意外發現

1. **AIR-67（R2+R4 quick-wins「半小時級」）立案 09-09 21:43 至今 To Do 未開工**——cr-audit 最高 CP 值即刻項兩天未動；cr-research.md:23 仍在教錯誤 query 形態，新 spawn 的 cr-research 仍會被引導到「查無 DEF」摩擦。
2. **R6 注入實務領先條文**：mosaic review 工單 09-10 起已自發攜帶 MCP-first＋write-face 禁用注入塊（工作order 治理未 codify 前）——「工單注入有效」的 audit 結論在行為面重現。
3. **muse bundle 32,040B 恰在 C2 報告 32,000 截斷崖上方 40B**——若 muse 自動化批次面 32,000 cap 仍在，bundle 尾段（含 CR 路由？需驗序）仍有靜默截斷風險。cap 語義 unverified，僅標記。
4. **R5 量測迴路仍未證實活著**：cr_usage 窗口內只被引用（甚至出現在 test 檔審閱語境），09-08 後無 corrections 週報產出——audit「殭屍 cron 最貴」警告目前仍成立。
5. **cr-demand demand-pull 零使用**：mosaic backlog 無任何 [cr-demand] 卡（rg 0 命中）——查詢面缺口仍靠人肉觀察回報。
6. **ZCode 主 session rollout 只保留當日 3 檔且快速輪轉**（查證過程中 06647689、a9c2bd05、3dcb2f37 三檔先後消失）——主 session 行為面長期追蹤不可行，agent output.txt/metadata.json 是較穩定的觀察面。

## ④ unverified 清單

- EP review B（f6217e6a）等子代理是否實際呼叫過 CR——per-agent tool transcript 未持久化，僅能驗最終輸出的 [SRC] 引用。
- muse 32,000 untrusted cap 是否仍存在且適用於現行 32,040B bundle。
- tonight sess_77327f21 兩個 review 子代理的 MCP 呼叫細節（rollout 檔輪轉，僅存當下計數 1-2 次）。
- cr_usage.py 自 09-08 是否曾被執行（僅證實被引用）。
- R7（spawn auth 失敗→回報重試）窗口內無觸發事件，無法驗證。
- lite 化的 runtime model face（metadata.json 無 model 欄；rollout 檔已輪轉）——spawn desc 層面（flash 字樣）已證。

## 總計

[OK] 修正落地查證：13 項中 8 落地✅、3 部分⚠️、2 未落地❌（R1/R2/R3/R4 依 AIR-67 To Do）
[OK] 行為面：ZCode 面合規樣本明確（CR-first＋絕對 repo_root＋[WARN] 揭露）；殘留訊號 0；muse 面滲透未躍升但 bundle 藉口已消除
