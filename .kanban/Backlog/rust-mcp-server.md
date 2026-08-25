# 能力卡：單一 MCP 接口（Rust rmcp）＝UC-5

[tag:code-reality] [capability]

## 目標
單一 HTTP 常駐 server（`127.0.0.1:8200`，launchd）支援多 repo：工具 `refs/callers/closure/audit` 皆帶必填 `repo_root`（per-call 參數、無 session 綁定）；rmcp（官方 Tier 1）streamable-http；進程內直連 lib＋per-request 錯誤隔離。

## 相關
- 父 EP：`ai-analysis/execution-plans/ep-rust-migration.md` 段 R6＋架構決策 AD-2/AD-3
- 套路參考：CRG plist 慣例、lsp_mcp 常駐形態

## 驗收標準
V1-V7 閉環（tools/list／17 callers 同 CLI／缺 repo_root loud／毒化隔離 SM-14／KeepAlive）；新 session 工具可見（L6）。

## 備註
Python FastMCP 版永不建（舊 EP S3 由 R6 取代）；v0 工具面=SCIP 家族四件（snapshot/tour 族維持 CLI，YAGNI）。
