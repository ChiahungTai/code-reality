# [producer] B7b pseudo-constructor mint + W1 settlement revision + B8 closure

## 目標
mosaic resolved-legacy coverage 94.7%（記錄版）→ ≥95.7%（真匹配）：
B7b 偽建構子 mint（`Cls().`＋DEF occurrence）、W1 度量腳本
（R2-3 條款＋B7a 正規化）、B8 列冊歸因（probe 實證：非 bug）。

## 相關
- EP: ai-analysis/execution-plans/ep-producer-completion-b7b.md
- 規格: ai-analysis/reports/s5-ceiling-analysis.md
- roadmap: ai-rules cr-lsp-replacement-roadmap.md W2

## 驗收標準
- S3 gate：B7a 正規化真匹配 ≥95.7%（scripts/s5_coverage.py）
- B7a 類（corpus `__init__`）行為不變
- B8：33＋97 證據寫回 s5-ceiling-analysis.md
- cargo test 全綠；comments/commits 全英文

## 備註
W3（import_legacy 退場鏈）gate＝本卡 B7b 落地。
