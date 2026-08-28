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
| B8 檔案未 walk | 56 | 130 檔分解（2026-08-28 W2 EP probe 實證，推翻「疑 get_ast None」）：**97 個語義空 document**（docstring/import-only `__init__.py`——無可收或目標全 external）＋**33 個 class/variable-only 檔**（全零函式、occurrences 全非 `().` 形，R2-3 fn_tail_name gate 設計行為濾除；例 `config/tw_futures.py` 在 SCIP 面有 18 個 occurrences）。**非 producer bug——列冊歸因結案**；cache build 新增零覆蓋 loud 計數（W2 EP S2） | 列冊歸因（證據：本行＋W2 EP probe 記錄） |
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

## W2 EP 落地結算（2026-08-28，`ep-producer-completion-b7b.md`）

- **B7b ✅ 落地**：偽建構子 mint（`` Cls(). `` call ref＋DEF 補發，
  per-site B7a guard）。mosaic @`24ced017`（s5 同語料）：pseudo
  refs/defs 3,033/377、class-callee missing 4,957→~110（轉換率
  ~97.8%，殘留＝derived-base-with-`__init__` 桶——callee grain 落
  `Base#__init__()`，legacy 期待 `Derived@file`，s5 SM-11 列冊）。
- **gate 實測（`scripts/s5_coverage.py`，全分母＋B7a 正規化；post-build
  review 修正版——首版腳本把 callee kind 誤併 pair key，B7b 真匹配被
  低報）**：`24ced017` 語料 **95.42%**（預測 95.7%，Δ0.28pp＝
  derived-base 殘留桶＋殘餘固有類）；現行 HEAD `0914dedd` 語料
  93.86%（drift-carved 剔 345 條 stale legacy 端點〔lsp_mcp 退役
  刪檔〕後 95.43%——兩語料收斂於 ~95.4%）。
- **度量腳本層次**：raw 83.5%（B7b 真匹配在正規化前即可見）；
  R2-3 剔分母 95.13%（凍結語料）；首版腳本的 raw 71.6-71.9%
  「重現」實為 kind-in-key bug 低報，已修。
- **B8 ✅ 列冊結案**（見上表行）；cache build 新增
  `docs_fully_filtered` loud 計數（stderr WARN 面，stdout parity 不動）。
- **W3 gate 判定素材已齊**：機制落地＋數字重現於凍結語料；嚴格
  ≥95.7% 未達（95.42%，兩語料收斂 ~95.4%）之差距全數歸因列冊桶
  （derived-base ~110＋B5 fixture/builtin ~236＋B4/B6/B1b）——
  退場時點裁決留 user。
