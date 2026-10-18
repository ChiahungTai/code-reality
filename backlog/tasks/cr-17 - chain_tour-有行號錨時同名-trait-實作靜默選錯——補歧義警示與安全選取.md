---
id: CR-17
title: chain_tour 有行號錨時同名 trait 實作靜默選錯——補歧義警示與安全選取
status: In Progress
assignee: []
created_date: '2026-10-18 12:36'
updated_date: '2026-10-18 12:38'
labels:
  - chain-tour
  - graph-anchor
dependencies: []
references:
  - CR-17
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
nautilus_trader 回報：tour 裡寫 `Price::fmt() price.rs:776`（語意是 Display 格式化），重產時被錨到 839 行的 `impl Debug for Price` 而非 845 行的 `impl Display for Price`——同檔兩個同名 `fmt`，工具挑了行號近的那個、一句警示都沒有；validate 也抓不到（兩處 pattern 都命中）。

**做什麼**
- 有行號的錨在「同檔＋同名＋同型別、多個 trait 實作」候選時：WARN 列出全部候選（如 [Debug]fmt 839 / [Display]fmt 845），重產報告加獨立計數
- 行號已漂移（非精確命中）且遇上述碰撞：不再猜最近行號——該幀拒絕錨定（fail-closed）
- 行號精確命中或沒有 trait 碰撞：維持現行行為

**不做什麼**
- 不做 `Price::fmt[Display]` 消歧語法（獨立下一卡）
- 不動 graph.db schema、Rust producer、CR-9 的 R1/R2/R3 語義

```mermaid
flowchart LR
    frame["md 幀：Price::fmt price.rs:776"] --> q["同檔同名 fmt 候選"]
    q --> two["839 Debug 與 845 Display"]
    two --> old["現況：取最近行號"]
    old --> bad["靜默錨 839，錯誤身分寫進 tour"]
    two --> fix["本卡：WARN 列候選＋獨立計數"]
    fix --> drift["行號漂移＋碰撞＝拒絕猜測"]
    fix --> exact["精確命中或無碰撞＝照舊"]
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 AC-1 NT 原始重現：fixture Price::fmt() price.rs:776＋graph 兩節點（839 impl#[Price][Debug]fmt / 845 impl#[Price][Display]fmt）→ 不得無警示輸出 line=839/symbol=Debug/graph=moved；歧義須揭露且依安全策略拒絕無證據的 nearest 選取
- [ ] #2 AC-2 明確座標仍可解析：Price::fmt() price.rs:845 → line=845、symbol=impl#[Price][Display]fmt().；同時保留 trait collision 資訊供人工辨識
- [ ] #3 AC-3 line-less R1/R2/R3 不回歸：既有 s5_chain_tour.rs 語義保留（雙候選 ambiguous＋候選清單＋不產生錯誤 step）
- [ ] #4 AC-4 一般 moved anchor 不受影響：不同名或無 trait 碰撞的行號漂移維持最近行號選取；既有 nearest tie-break 測試行為保留
- [ ] #5 AC-5 報告一致：CLI 報告與 scenario description 的候選清單、explicit_trait_ambiguous 計數一致；L4 以 NT 真實 graph.db（唯讀）驗證原始錨 776 → 警示/拒絕、修正錨 845 → Display
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
〔baseline：code-reality 2f76918（v0.16.0）〕

〔已決策勿重辯：
①根因（codex 確認腿 job-mvdsprge-dprgzj 與主 session 自查雙源一致）＝ explicit-line 路徑 GraphAnchor::anchor（chain_tour.rs）的 SQL `WHERE name=? ORDER BY ABS(line_start-文檔錨行) LIMIT 1` 同名多候選靜默取最近；NT 實例＝原錨 776，|839−776|=63 < |845−776|=69 → 必選 Debug。CR-9 的 fail-closed（resolve_candidates）只守 line-less fill 分支，anchor 分支不在守備範圍；R2 的 descriptor_chain 對兩個 impl 都產 ["Price","fmt"]（Trait 段忽略），即使進 R2 也無法消歧。
②安全選取策略：行號精確命中唯一候選 → 保留該錨＋揭露碰撞資訊；行號漂移（非精確命中）＋同型別同名多 trait 候選 → fail-closed 拒絕 nearest 猜測；無 trait 碰撞 → 維持現行 moved 行為。
③獨立計數 explicit_trait_ambiguous（scenario description 與 CLI 報告），不混入既有 g_counts["ambiguous"]（語義不同：後者＝line-less 無法填入）。
④NT 案 2（Price::fmt[Display] qualifier 語法）＝下一張獨立卡——須處理 SCIP descriptor grammar（含泛型 trait 如 Add<Self>），不與本卡耦合；NT 案 3（description 自由文字 lint）不做——無法機械驗收。
⑤修改範圍＝chain_tour.rs／symbol_resolve.rs（共用候選查詢與 Rust impl descriptor 辨識）／tests/s5_chain_tour.rs／plugin/skills/code-reality-tools/SKILL.md；明確不動：graph.db schema、Rust SCIP producer、R1 pseudo-constructor pairing、既有 R2 dotted-scope 對齊語義、tour_validate 自由文字推斷、MCP 新工具或 CLI 新旗標。〕

範圍：上述四檔。descriptor_chain 既有語義（保留第一個 [Type]、忽略其餘 Trait bracket）不改——碰撞辨識走新的 descriptor 判讀，不動 R2。
<!-- SECTION:PLAN:END -->
