---
id: DRAFT-1
title: chain_tour 支援 symbol-only 錨（graph.db 代填行號）——mosaic DRAFT-46 對接
status: Draft
assignee: []
created_date: '2026-10-11 13:10'
labels: []
dependencies: []
---

## 提案設計（源：mosaic DRAFT-46，2026-10-11 傳話）

錨格式放寬：`Symbol path:line # 註解` → `Symbol path # 註解` 也可（顯式行號向後相容）。
行號省略時 chain_tour 按 symbol+path 查 graph.db 代填進 `.tour`——`.tour` 的行號本來就是
regen 刷新的 ephemeral 產物，md 內行號早已是 hint 而非真相源（管線 regen 已在按符號重錨：
上次重產 moved:18＝拿符號＋路徑查 graph.db 自動校正行號），只是解析器目前強制要求行號 token。

## 實驗證據（mosaic 側已機驗）

拔掉 mainline md 全部行號餵 chain_tour：4 場景／23 幀／0 步／skipped 23（noref:23）——
解析器視 `path:line` 為必要 token，缺行號幀全數跳過。

## 觸發條件（deferred-with-triggers）

- mosaic 按族增量遷移其 47 檔 callstack 文檔時
- 行號漂移痛點再現（mosaic research-observe-seeds 已痛）
- 本 repo tour 家族弧順手

## 落點

- 本 repo：tour 解析器（chain_tour parse／anchor resolution；tour_validate 同步放寬）
- ai-guide 側：接線文檔（mosaic 邊界規則會經 ai-guide session 攜 proposed patch——
  但 CR 本體直接擁有解析器，可由 CR 弧直接承做）
- mosaic 側：等管線支援後增量遷移（他們明言按族觸及時順手，不另行承諾）

## 對接

mosaic 側登記＝`mosaic_alpha/backlog/drafts/draft-46 - chain_tour-支援-symbol-only-錨（SCIP-式，免行號漂移）.md`
（空殼卡，實質設計以其 2026-10-11 傳話為準，已全數轉錄於本檔）。

