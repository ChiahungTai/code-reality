[tag:crates] cr-freshness 抽取＋freshness WARN 縮窄

## 目標
freshness 邏輯三種消費形態（canonical／path dep／手抄）收斂為零依賴微 crate `cr-freshness`（單一源＋自帶測試）；runtime WARN 縮窄到 dev face（exe 位於 CARGO_HOME/bin）＋crates-relevant 判準（docs-only gap 靜默）。pin 面從此靜默（plugin pin 鏈為唯一權威）、dev 面保留 dirty-crates 守衛（08-28 陷阱）。

## 相關
- EP：`ai-analysis/execution-plans/ep-cr-freshness-extraction.md`（baseline 62dee2a）
- 原 EP 卡：`.kanban/Done/binary-freshness-face.md`
- 更新 UC：Binary freshness face（AGENTS.md:86）

## 驗收標準
- `cargo test` workspace 全綠；四 bin `--version` 版面不變
- SM-1/2/3/4/6 整合測試＋真機 L4 四情境（pin 靜默／dirty 警示／docs-only 靜默／crates 落後警示）
- hook 新增 `crates/cr-freshness/*` arm；文檔同步（AGENTS.md:32+:86、crates/AGENTS.md 條款、STATE.md:17 解銷）
- v0.5.2 出貨（五面鎖步，含共車的 WIRED 修復）

## 備註
凍結裁決（雙面保留／命名 cr-freshness／bridge 條款修訂）與非目標見 EP「實作總覽」。working tree 之並行 session 改動不納入。
