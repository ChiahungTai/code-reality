# [infra] PyPI platform-wheel 分發軸 — 消費端免 cargo 安裝

## 目標

三 crate 以 maturin 打包 `py3-none-<platform>` wheels 上 PyPI
（ruff/pyrefly 模式）：`uv tool install` / `uvx` 消費者路徑、tag `v*`
→CI matrix（macOS×2＋Linux×2）→trusted publishing 發布鏈、版號
首發基準 0.2.0（workspace version 單源）、`.mcp.json` spawn 優先序
翻轉（cargo HEAD 不再蓋 pip stable）。

## 相關

- EP：`ai-analysis/execution-plans/ep-pypi-wheel-distribution.md`
  （baseline `9f25969`）
- Spike 已驗（2026-08-28 本機）：maturin 1.15.0 零配置 bin bindings
  自動偵測；三 wheel 1.4M/8.5M/25M（最大單顆 25MB，遠低於 PyPI
  per-file 上限）；
  五 bin `--version` = `0.1.0+9f25969`（freshness face 穿透 wheel）
- 設計裁決：分發軸收斂 ruff/pyrefly（wheels）＋CRG（setup 薄面，
  另弧）；NT 私有 index 裁 YAGNI
- 前置已落地：plugin CC 化 0.1.3（`9f25969`）

## 驗收標準

1. 三 dist 上 PyPI；`uv tool install`＋`uvx` 陌生路徑實跑可用
2. CI dry-run（workflow_dispatch）12 wheels（4 平台×3 crate）全綠；
   tag push 只在綠 build 發布
3. 首發 workspace version `0.2.0`；版號三層條款生效（EP frozen 段）
4. spawn 優先序翻轉後：ZCode 新 session 雙 server mount＋GUI 無
   PATH 場景可 spawn＋freshness WARN 兩通道各驗一次
5. Linux build kill-gate 有結論（全矩陣或 descope 決議記錄）

## 備註

- NOT：私有 dev index／setup 子命令／CC market 投稿／Windows／
  musllinux／crates.io（EP NOT 段）
- ai-rules handoff：wheels 上線後翻轉 code-reality SKILL.md 安裝段
