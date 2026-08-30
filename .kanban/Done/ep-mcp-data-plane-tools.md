# [infra] EP 追蹤：MCP 資料面四工具

## 目標

追蹤 `ai-analysis/execution-plans/ep-mcp-data-plane-tools.md`
（S1 四工具 thin-wrap＋S2 tests/L4 stdio 實測＋收尾 doc-sync）
整體推進——`build`/`snapshot`/`delta_tour`/`project` 從 CLI-only
擴進 `mcp_server`，MCP 面性格重裁為含寫副作用（2026-08-29 深夜
凍結決策）。

## 相關

- EP：`ai-analysis/execution-plans/ep-mcp-data-plane-tools.md`
  （baseline `62dee2ae`）
- 既有能力：root AGENTS.md「Unified MCP interface」row（17→21
  工具，本 EP 更新）
- 下游：ai-rules EP2（implement 分層換軌——四工具 CLI fallback
  翻轉的供給端）

## 驗收標準

- 四工具 MCP stdio 實測可呼（L4）
- `cargo test -p code-reality` 全綠
- `scripts/release.sh 0.6.0` 發行成功
- doc-sync 三面（plugin SKILL.md／root AGENTS.md／crates/AGENTS.md）

## 備註

- sibling session（query-time self-heal 軸）共用 Backlog lane；
  兩軸檔案不相交
- 發行前樹淨處置見 EP 整合策略段
