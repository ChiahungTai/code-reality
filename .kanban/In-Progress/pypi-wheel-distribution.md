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

1. 三 dist 上 PyPI ✅ DONE（2026-08-28 v0.2.0，run `33186412174`：
   `uvx` 陌生路徑＋PyPI 直裝 venv 五 bin 全 `0.2.0+aacebd6`；本機
   `uv tool install` 實裝隨 S4 spawn 翻轉後）
2. CI dry-run ✅ DONE（2026-08-28 run `33182529285`：四腿全綠＝
   縮編超集、publish gate skipped 正確、12 顆 artifacts tag 全驗；
   Linux 編譯可行＝免費情報）；tag push 只在綠 build 發布
3. 首發 workspace version `0.2.0` ✅ DONE（版號條款生效：tag→綠
   build→publish；版號面盤點通過——PyPI＝workspace＝wheel＝--version）
4. spawn 優先序翻轉後：ZCode 新 session 雙 server mount＋GUI 無
   PATH 場景可 spawn＋freshness WARN 兩通道各驗一次
5. Linux 腿 deferred（2026-08-28 縮編）；in-flight 全矩陣 dry-run
   `33182529285` 的 Linux 結果作未來重啟情報記錄

## 備註

- NOT：私有 dev index／setup 子命令／CC market 投稿／Windows／
  musllinux／crates.io（EP NOT 段）
- ai-rules handoff：wheels 上線後翻轉 code-reality SKILL.md 安裝段
