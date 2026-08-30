# [infra] EP 追蹤：主索引 query-time lazy 自癒

## 目標

追蹤 `ai-analysis/execution-plans/ep-index-query-time-self-heal.md` 四段
（S1 producer/build 雙腿原子寫→S2 staleness 偵測層→S3 auto-heal 編排→
S4 commit 粒度背景刷新——user 2026-08-30 save≠commit 裁決）＋收尾段
（Capabilities／crates-AGENTS／SKILL.md+README／audit-test）整體推進。

## 相關

- EP：`ai-analysis/execution-plans/ep-index-query-time-self-heal.md`
  （baseline `62dee2ae`）
- 能力卡：`index-query-time-self-heal.md`（本卡追蹤 EP，能力卡追蹤 UC）

## 驗收標準

- S1-S4 全段落 build＋驗證策略綠
- L4 dogfood：ai-rules incident 形態復現→自動癒合；CR 自倉 hook 合併腳本
  真觸發（在飛三項完成後）
- 收尾五項完成（Capabilities 行、kanban 結算、crates/AGENTS.md 三路徑明文、
  SKILL.md+README、/audit-test）

## 備註

ep_type: implementation；裁決凍結於 EP 段落 0「裁決記錄」。
殘餘：L4 dogfood（ai-rules incident 形態復現＋CR 自倉 mixed heal＋hook 真觸發）
**前置＝CR commit→release（五處鎖步）→ai-rules plugin 刷新 pin**——新 binary
落地後在 ai-rules 下一個查詢即同時完成 remediation 與 dogfood（原「在飛三項」
的人工補救被 self-heal 吸收）；cargo 面全綠（64 條本 EP 測試，含真 producer
的 L4 形 e2e）＋dual-context 審查 22 findings 修正後 16/16 機械驗收通過。
