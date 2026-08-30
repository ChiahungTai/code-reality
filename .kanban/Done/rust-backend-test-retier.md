[tag:crates] rust_backend 測試 re-tier——驗收級測試退出 always-on gate

## 目標
rust_backend.rs 六測試中五個各自 spawn 真 rust-analyzer 冷載整個 workspace（harness 並行 → 6-8 顆 ra 自殘競爭）→ edit→check 的 30s 收斂 deadline 偶爾越界（STATE.md 既有 flake）。Re-tier：刪兩顆（latency 結算 artifact、hover 與 battery 重疊）、#[ignore] 兩顆（觸發式：動 server/session 時 --ignored 跑）、always-on 留 route+death（2 顆 ra）。

## 相關
- EP：ai-analysis/execution-plans/ep-rust-backend-test-retier.md（baseline f99f0f0）
- 更新 UC：Type face via LSP bridge（AGENTS.md:91 測試面敘述）

## 驗收標準
- default 跑 4 passed＋2 ignored、計時顯著下降、pgrep 並行 ra ≤2
- --ignored 兩顆綠；全量 cargo test 綠
- crates/AGENTS.md 觸發紀律＋STATE.md 追蹤項解銷

## 備註
test-only＋docs 變更、不出 release（wheel 不變）。凍結裁決見 EP。

〔2026-08-30 追裁：user 選「每次都跑＋RA_SERIAL 序列化」（+~20s 相對並行形、52.48s 實測）——`#[ignore]` 形出貨（`4d109d1`）後即翻；兩顆重測試的驗證價值經論證與跨測試並行無關，flake 條件同樣移除（並行冷載→一次一顆）。詳 EP Build Record 追裁段。〕
