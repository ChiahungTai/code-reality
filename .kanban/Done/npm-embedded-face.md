# [infra] npm 內嵌 binary 面 — CC 一鍵全棧（驗證置前）

## 目標

CC 消費者「只裝 plugin、零 `uv tool install`」：五 bin 走 npm 平台
套件（esbuild 模式）隨 plugin 的 npm ci 落地；`.mcp.json` 從
node_modules spawn。**uv/PyPI 主面不動**（疊加面）。ZCode 已實證
無此機制——本面 CC-only，文件明示兩家差異。

## 相關

- EP：`ai-analysis/execution-plans/ep-npm-embedded-face.md`
  （baseline `63b42c8`）
- 前置實證（2026-08-29 probe）：CC npm ci 落地✓／平台挑選生效✓
  （x64-trap 實證）／ZCode 不跑 npm ci✓／未證環節＝
  `.mcp.json` spawn node_modules（S1 kill-gate）
- 先例：esbuild／biome／Turborepo／`@openai/codex`（Rust bin 走 npm）
- 前置 user 手動：npm 帳號＋2FA＋publish 權限（S2 前）

## 驗收標準

1. S1 雙路線 probe 有結論（P2 spawn-from-node_modules 通/不通；
   P1 bin/ 對照；V2 架構變體記錄）——不通則 EP 收案「不可行」
2. （S1 通時）CC 純 plugin 安裝→工具可用，全程零 uv
3. x64-trap 對策條款定案並落地
4. 雙面條款文件化（uv 主面獨立更新／npm 面鎖 plugin 軸）
5. wrapper 探針回歸綠＋文檔/ai-rules handoff 交付

## 備註

- NOT：ZCode npm 面／取代 uv 面／綁 rust-analyzer／Win+Linux 平台
- S1＝獨立 probe 弧（CC session 執行，零 repo 變更）
