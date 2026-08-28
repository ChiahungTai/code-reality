# S5 覆蓋天花板分析（import_legacy 退場判準規格）

> 2026-08-28，agent 全量分類（5,805 missing pairs 逐一比對 cache/
> fndefs/源碼，非抽樣）。資料：mosaic graph.db＋slot cache @ 24ced017。
> 數字重現全命中（72.33%/15,172/5,805）。

## 關鍵發現：missing 的 84% 是建構子語義，非解析品質

| 桶 | pairs | 診斷 | 處置 |
|---|---|---|---|
| B7a 建構子 grain 錯位 | 2,642 | producer 已出邊，callee=`__init__@file` vs legacy `Class@file`——度量端正規化即可收割 | 度量端（`class_segment` 已存在） |
| B7b 無 corpus `__init__` 建構子 | 2,254 | dataclass/繼承 object——mint 判 external 丟棄、邊不存在（`RiskGuardConfig` 等） | producer 補完 EP（mint 偽建構子 `Cls().`＋補 def occurrence） |
| B5 legacy 偽陽性 | 368 | 83 builtin-like 名碰撞＋285 fixture/stub 誤綁（部分真缺） | 偽陽性列冊；真缺（~200-350）高風險上游依賴 |
| B4 module-level call | 165 | item-level 略過；legacy caller=File | Module pseudo-node 或度量端剔 File-caller |
| B6 行漂移/mock | 162 | legacy 行號漂移（含超 EOF）＋mock receiver 未解析 | 不修 producer（artifact） |
| B1b super() | 115 | producer 邊正確（父類 `__init__`）；legacy 誤綁同檔 | legacy artifact |
| B8 檔案未 walk | 56 | **130 個 in-scope .py 零 occurrences**（`config/tw_futures.py`、`data/*/types.py`——真模組，疑 get_ast None） | **producer bug 調查** |
| B7c/B2/B3 | 50 | 繼承邊角/self-ref 政策略過/decorator 歸屬差 | 記錄 |

## 判準裁決

- **EP R2-3 凍結條款（constructor 邊剔出分母）在 2c44534 結算時未套用**
  ——正確套用：Class-callee legacy pairs（4,962，與 producer 交集
  結構性為 0）剔除 → **15,172/16,015＝94.7%，即刻達標 ≥90%**
- 再剔 File-caller（220）→ 95.8%
- producer 補完路徑：B7a（度量收割）→ 84.9%；**+B7b → 95.7%
  （把剔分母變真匹配的決定性槓桿）**；理論天花板（剔盡 legacy
  偽陽性）≈98.4%
- B5/B6 硬骨頭（receiver 型別推導）**不需要動**即可過門檻

## 附帶發現

- legacy 邊 import 時有被 re-target（callee 端 12,796 條指向 scip
  節點、5,331 雙端 scip）——import_legacy 的合併比帳面深
- B8 的 130 檔未 walk 是 def 宇宙缺口（不只 56 pairs）

## 建議路線

1. 判準修正記錄（94.7% 達標）＋B7a 度量收割入同弧
2. B7b＋B8＝producer 補完 EP（與 bridge EP 排序由 user 定）
3. B7b 落地後啟動 import_legacy 退場鏈（刷鏈去步驟→純 producer
   graph→消費端驗收→mosaic 刪 .code-review-graph/）
