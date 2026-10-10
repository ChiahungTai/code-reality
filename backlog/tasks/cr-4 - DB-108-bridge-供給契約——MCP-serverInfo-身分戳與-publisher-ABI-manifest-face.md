---
id: CR-4
title: DB-108 bridge 供給契約——MCP serverInfo 身分戳與 publisher ABI manifest face
status: Done
assignee: []
created_date: '2026-10-10 03:06'
updated_date: '2026-10-10 04:41'
labels:
  - feature
  - bridge-request
dependencies: []
references:
  - ai-analysis/2026-10/10-10-db108-supply-contract/advisory-codex-result.md
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
delegate-bridge 來信（dutymail db108-cr-supply-contract-001）：bridge 想把 CR graph+LSP 真正供給給 writer 腿，要 CR 側回答六項契約。六項中四項有現成面（安裝登記／lsp-bridge／freshness／工具清單），兩項缺口由本卡補上：

**做什麼**：(1) 兩個 MCP server 的 initialize 身分面現自查成 rmcp/3.5.1（rmcp crate 的 from_build_env 在 rmcp 內展開）——bridge 無法用 serverInfo 當版本錨；蓋上真名真版。(2) 新增 `--manifest --json` publisher 自宣告面：版本、進入點形態、協議協商、工具清單＋schema digest——bridge 相容性驗證的錨，binary 自帶隨版出貨、免第二 pin。

**不做什麼**：不改 bridge 側（那是對方 repo 的弧）；graph_query per-answer stale 標記本卡不做（組合式答覆：bridge 供給前呼叫 freshness face——exit code 機器可判；per-answer 標記列後續選項）。

```mermaid
flowchart LR
    B["bridge supply v2"] --> P{"供給前驗證"}
    P --> M["code-reality-mcp --manifest --json（本卡新增）"]
    P --> F["code-reality freshness（既有 exit code oracle）"]
    M --> A["ABI 錨：版本＋工具 schema digest"]
    F --> S{"fresh / stale?"}
    S -->|fresh| G["供給 graph+LSP faces"]
    S -->|stale| R["typed stale refusal（bridge 側）"]
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 - MCP initialize serverInfo 身分戳：code-reality-mcp 與 code-reality-lsp-bridge 的 initialize 回應 serverInfo.name＝各自 binary 名、version＝CARGO_PKG_VERSION（非 rmcp/3.5.1）——live probe 測試斷言（s6 http smoke peer_info＋stdio probe）
- [ ] #2 #2 - code-reality-mcp --manifest 印 cr-mcp-manifest/1 JSON：crate_version、build_rev、entrypoints（stdio/streamable-http）、protocol.latest、tools（name/description/input_schema）21 隻、tools_digest＝canonical（遞迴排序鍵）sha256 hex、write_faces 宣告⊆tools——exit 0；跑兩次 byte-identical
- [ ] #3 #3 - code-reality-lsp-bridge --manifest 同 schema：4 工具、index_dependency:false、backend_prereqs 三 family、write_faces 含 edit_file（in-memory overlay）——exit 0
- [ ] #4 #4 - 雙 bin 未知參數仍 fail loud exit 2；--manifest 不破壞 --stdio/HTTP 兩模式路由（s6 既有 http smoke 全綠）
- [ ] #5 #5 - cargo test workspace 全綠零 warning；安裝面 uv 灌新版後 stdio probe 複驗 serverInfo 與 manifest（live驗收）
- [ ] #6 #6 - 回執：dutymail 六項逐項答覆寄 delegate-bridge-marshal（body ≤8KB、landing 指針、in_reply_to db108-cr-supply-contract-001-1791659500）
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
〔baseline：code-reality cf0dc74（v0.10.0）〕

〔已決策勿重辯：①範圍＝(a) 兩 MCP server get_info 身分戳（真名真版——serverInfo 現自查 rmcp/3.5.1，env! 在 rmcp crate 內展開，實測重現）＋(b) code-reality-mcp --manifest --json publisher ABI 自宣告面（version+build rev、進入點 faces stdio/http、protocolVersion 協商面、tools 名稱+input schema＋canonical sha256 digest；lsp-bridge bin 同 face 獨立 digest）——codex advisory 腿 adversarial 驗證後定案（.agent-tmp/db108-supply-contract/codex-result.md，job-mv1syt54-k9owju，chatgpt-web/high）②graph_query per-answer stale 標記本卡不做——組合式 v1 答覆（bridge 供給前呼 freshness exit-code oracle）；per-answer meta 列後續選項（medium）③digest 口徑＝canonicalized tool names+input schemas（codex 裁決：count/name-hash 不足；protocolVersion 表協商面非不變單值）④唯讀消費指引隨回執交付：CODE_REALITY_IDENTITY_CACHE=off（freshness 預設 WriteBack 落 identity cache）、CODE_REALITY_AUTOHEAL=off（scip autoheal 屬 mutating；heal 失敗 best-effort 照答非保證）⑤不動 bridge 側、不動安裝 wrapper、工具行為零變更——manifest 是加面非改面〕

〔範圍：crates/code-reality/src/bin/code-reality-mcp/main.rs（--manifest arm）、src/mcp_server.rs（get_info 身分＋工具枚舉可達面）、crates/code-reality-lsp-bridge/src/bin＋server.rs（同兩面）；tests/s6_mcp_server.rs（工具名凍結）延伸 manifest 斷言。回執＝dutymail 六項逐項寄 delegate-bridge-marshal（envelope body ≤8KB＋landing 指針）。規模分級：standard——card Planning Contract 承載，無 standalone EP。〕
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
實作定案（F5 deviation，reviewer 確認）：face＝`--manifest` 單 flag、輸出恆 JSON（終端面語義同 --version）——卡文原寫 `--manifest --json`，`--json` companion 未做（在後被吞、在前 exit 2，與 --version 同性質，接受不作）。digest 口徑 reviewer 獨立重算證實＝name+input_schema（tools_digest_scope 宣告為真）。

跟催：build 後 slot 無 identity 認證（freshness 恆 stale）＝前於本弧既存狀態，已開卡追蹤。
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
DB-108 六項供給契約收斂：四項現成面（plugin pin 安裝登記／lsp-bridge 獨立面／freshness exit-code oracle／tools-list 測試凍結）逐項答覆＋兩缺口實作落地——initialize serverInfo 真身分戳（原自查 rmcp/3.5.1）＋雙 bin `--manifest` publisher ABI face（cr-mcp-manifest/1：tools+schema canonical sha256 digest、write_faces 誠實標記、mutating_notes/index_dependency 揭露）。審查鏈：codex adversarial advisory（job-mv1syt54-k9owju）＋fresh（needs-fix→全修）＋primed（accept）＋re-review（pass）四腿；全倉 cargo test 672/0、安裝面 live probe 過。回執已寄 delegate-bridge-marshal（db108-cr-six-item-reply-001，accepted seq 64）。

```mermaid
flowchart LR
    S["db108 solicit 六項契約"] --> V["codex adversarial 驗證腿"]
    V --> A{"裁決"}
    A -->|現成面 ①③④⑥| R1["逐項 face 名答覆"]
    A -->|缺口 ②| I["serverInfo 身分戳＋--manifest ABI face"]
    A -->|⑤ 組合式| R2["freshness exit-code oracle 為 v1；per-answer meta 列後續"]
    I --> T["cargo test 672/0＋fresh+primed+re-review 收斂"]
    R1 --> M["main @ 929e373＋回執寄出 seq 64"]
    T --> M
    R2 --> M
    M --> W["mail waiter 掛哨——新信喚醒"]
```
<!-- SECTION:FINAL_SUMMARY:END -->
