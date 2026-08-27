# ep-pyrefly-native-producer

EP: ai-analysis/execution-plans/ep-pyrefly-native-producer.md
(baseline b0ed95a; validate + review findings 全數回寫)

## UC

- Rust 原生 Python occurrence producer（link Pyrefly＋SCIP face 薄 emitter）
- scip-python fork 退役為 fallback（cutover 後文檔降級）

## 狀態

S1 實作中（crates/pyrefly-producer）：api/walk/symbol/emit/lib＋bin＋
fixture 端到端測試＋infer_language 第三前綴。S2（mosaic 對帳）→
S3（cutover）待做。平行制約：occurrence EP F2/F5 先 land、本 EP
S3 後（同 slot）。
