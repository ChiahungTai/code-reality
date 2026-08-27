[tag:scripts] EP: occurrence producer — Python graph data at minutes

## 目標
lsp_harvest 6.7h 全量 → 分鐘級 occurrence producer（scip-python patched fork 主線）；harvest 增量模式（過渡，可提前）；CALLS 覆蓋率對帳接線 import_legacy 退場判準。

## 相關
- EP: ai-analysis/execution-plans/ep-occurrence-producer.md
- 裁決輸入: mosaic dogfood 效能 relay（2026-08-27）＋ scip-python 實測（fatal＝無界遞迴、160× 速度）
- Golden corpus: mosaic dogfood cache（14,475 defs / 640,976 sites）＋ daily.py baseline

## 驗收標準
- mosaic 全量 harvest ≤10min 且 defs/references 對 golden corpus 達標
- 增量模式：模擬小改動分鐘級、結果 ⊇ 真實影響
- CALLS 覆蓋率報告產出、退場門檻量化

## 備註
新增能力 2項（occurrence producer、增量模式）＋更新 2 行 Capabilities。
