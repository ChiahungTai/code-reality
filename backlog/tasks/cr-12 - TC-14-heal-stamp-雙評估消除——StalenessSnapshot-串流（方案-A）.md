---
id: CR-12
title: TC-14 heal/stamp 雙評估消除——StalenessSnapshot 串流（方案 A）
status: In Progress
assignee: []
created_date: '2026-10-15 09:10'
updated_date: '2026-10-14 23:17'
labels: []
dependencies: []
references:
  - ai-analysis/execution-plans/_done/2026-10-14-cr-delta-freshness/ep.md
  - .agent-tmp/cr-followup-design-brief.md
ordinal: 12000
---

# CR-12 — TC-14 heal/stamp 雙評估消除（方案 A：串流快照）

## Problem

Default 查詢路徑上同一份 staleness 評估跑兩遍：cli.rs heal 檢查（~:330-361，
內含完整 evaluate_staleness → needs_rebuild 決策）之後 `serving_stamp`
（~:392）再跑一次完整評估（全 source walk＋identity＋delta＋identity-cache
WriteBack）。TC-14 實測：NT 本語料 warm 86ms 及格，但加標註劣化 >20%。

**歸因修正（codex 討論反證，已採納）**：graph_query 不走 cli.rs heal 流程
（main.rs:31 → graph_engine::run 自建 stamp，graph_engine.rs ~:2514，無
ensure_fresh）——本卡只消除 **scip_refs fresh path 的重複求值**；
graph_query 標註成本是單次求值的固有成本，不在本卡 scope（見 Out of scope）。

## Plan A（方向已由 user 裁定；設計經 codex 討論 refine，2026-10-15）

- **②① plumbing**：load-face 返回值形——`HealCheck { outcome: HealOutcome,
  reusable_snapshot: Option<StalenessSnapshot> }`（HealOutcome 四態已封裝於
  build.rs ~:1275）；stamp 側新增可接受 snapshot 的建構入口。
  **None ＝ 重算，禁解讀為 fresh**（HealOutcome::Fresh 涵蓋 slot 缺席）。
- **②② per-outcome 快照語義**：Fresh → 原快照可重用；Healed →
  run_heal_locked 的 post-build 重算結果（build.rs ~:1491 已存在）；
  HealedByPeer → lock 釋放後重算結果（build.rs ~:1611 已存在）；
  ServeStale → 保守重算除非 slot 可證未變；Err → 無快照（D15 降級流程不變）。
  stamp-time 與 post-load binding 檢查全保留（freshness.rs ~:300/~:398）。
  D11 bounded consistency 不變（禁宣稱 ABA linearization）。
- **②③ single-flight**：等待 peer 後用 post-wait 重算快照，禁沿用等待前
  快照；逾時/接管按實際結果。

## AC（驗證協議）

- 閘（NT 本 repo 自食實測，S2 AM-2 慣例）：freshness warm ≤200ms；
  **scip_refs 標註 p50 劣化 <20%（修正後）**；fresh query 恰一次完整
  staleness 求值（求值次數計數證據）；heal/peer-heal stamp 綁實際 serving
  generation；cold 記錄數值不設門檻。
- 量測協議：固定 corpus／index hash／release binary／identity-cache warm，
  交錯 baseline/candidate 各 ≥50 次，記 p50/p95＋求值次數。
- graph_query 標註：分開量測歸因（單次求值為固有成本），本卡不設 <20% 閘。

## Out of scope

evaluate_staleness 本身的成本優化；graph_query 標註路徑提速（若量測後仍
劣化屬 disclosed residual，另議卡）。

## Scope fence

cli.rs（heal 塊＋兩查詢入口）、freshness.rs、heal 模組（build.rs）簽名串接；
graph_engine.rs 僅在快照串接必要處觸碰。契約面（fixture）不在本卡。
