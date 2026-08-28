# EP — Producer Completion (B7b pseudo-constructor mint) + W1 settlement revision

> **ep_type**: implementation
> baseline: decdb75c2feb63f0b83d7351a5b7975920b6748e
> Spec source: `ai-analysis/reports/s5-ceiling-analysis.md` (agent full
> classification of 5,805 missing pairs; W1-W4 road map) + ai-rules
> `cr-lsp-replacement-roadmap.md` 資料面自理軸 (user 2026-08-28 ordering:
> P1→P2→this arc; W3 gate = B7b landing).

## 實作總覽

Raise the mosaic resolved-legacy coverage from the 94.7%
(denominator-carve accounting fix) to the **95.7% true-match** level by
minting pseudo-constructor call edges for corpus classes without a
corpus `__init__` (B7b, 2,254 pairs), and close the B8 investigation
(130 zero-occurrence files) with evidence-based disposition. W1 (S5
settlement revision — R2-3 clause record + B7a metric harvest) opens
this EP as its first segment per the user's merge decision.

**Probe-verified facts this EP is built on** (2026-08-28, this session):

1. **B7b mechanism (live probe, `PYREFLY_PRODUCER_DEBUG` on a
   dataclass + plain-class fixture)**: pyrefly resolves
   `RiskGuardConfig()` / `PlainHolder()` constructor calls **to the
   corpus Class target**, not to an external `__init__`. The mint in
   `lib.rs::mint_targets` then produces a class-shaped symbol
   (`` `pkg.mod`/Cls# ``) which the cache ingest gate
   (`cache.rs:100` `fn_tail_name`) filters out — the edge never lands.
   The s5 report's "mint 判 external 丟棄" attribution is corrected by
   this probe: the dominant mechanism is *class-shaped mint dropped at
   the ingest gate* (an external-drop subset may coexist for classes
   whose `__init__` lives in an external base; the fix below does not
   depend on distinguishing them).
2. **B8 root cause (disproves "疑 get_ast None")**: fresh
   `pyrefly-index` run on mosaic_alpha emits **zero**
   `skipped_no_ast` WARNs; all 1,320 files yield an AST. The 130
   zero-occurrence files decompose into **97 semantically empty SCIP
   documents** (docstring/import-only `__init__.py` — nothing to
   collect or all targets external) and **33 documents whose every
   occurrence is class/variable-shaped** and therefore dropped by the
   frozen R2-3 clause at cache ingest (`fn_tail_name` gate). All 33
   files verified to contain **zero function definitions** (regex
   `^\s*(async )?def ` over each file). B8 is a **designed filter, not
   a producer bug** — disposition is record-and-close with this
   evidence.
3. **graph_db edge gate**: `graph_db.rs:592`
   (`if !def_symbols.contains(callee) { continue; }`) — a call
   reference to a minted symbol with no DEF row is dropped. The B7b
   mint MUST therefore emit a DEF occurrence for the pseudo-constructor
   symbol, or the edge dies one stage later.

## UC 盤點

### Backlog 關聯
- `.kanban/Backlog/` empty at EP creation — auto-created card below
  (EP-integral tracking card).

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（本 repo 無此檔）。

### 掃描範圍
- `AGENTS.md` Capabilities（root）、`crates/AGENTS.md`、
  `.kanban/Backlog/`。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Rust-native Python occurrence producer (pyrefly-index) | ✅ | AGENTS.md Capabilities | 更新 | B7b mint 擴充 constructor-call 邊產出；report 計數器新增 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| S5 resolved-legacy coverage metric（可重現驗收數字） | 📋 | `scripts/s5_coverage.py` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | dataclass / object-inherit 建構子呼叫 | `RiskGuardConfig()` in corpus | producer 出 `Cls().` call ref + DEF occurrence；cache/graph 兩面成 CALLS 邊 | — | producer 更新 |
| SM-2 | 帶 corpus `__init__` 的建構子（B7a 類） | `WithInit()` | 行為不變（callee=`Cls#__init__().`）——B7a 屬度量端正規化，非本弧 producer 變更 | 度量腳本 class_segment 正規化 | W1 metric |
| SM-3 | class 名稱的純 load（非呼叫） | `isinstance(x, Cls)` / 型別註解 | class-shaped reference 照舊被 ingest gate 濾除——不 mint、不回歸 | 測試斷言 | producer 更新 |
| SM-4 | 同 class 多處建構 | 同檔/跨檔多次 `Cls()` | 多條 call refs、DEF occurrence 恰一筆（冪等） | 測試斷言 | producer 更新 |
| SM-5 | 從未被建構的 class | corpus class 無任何 constructor call | 不 mint DEF（two-pass：只為被呼叫的 class 補 def）——避免 node 宇宙膨脹 | 測試斷言 | producer 更新 |
| SM-6 | 覆蓋率量測（W1+B7b 驗收） | `uv run python scripts/s5_coverage.py --db <graph.db>` | 套用 R2-3 剔分母 + B7a 正規化後輸出分層數字；B7b 前重現 94.7%（記錄版）、後 ≥95.7%（真匹配） | — | S5 metric UC |
| SM-7 | byte-determinism | 連跑兩次 pyrefly-index | index 逐位相同（B7b mint 不引入非確定性）；**禁止 iterate `called_classes` 集合發射**（HashSet 序不得進輸出——DEF 補發內嵌在 defs 迴圈） | 既有 determinism 測試延伸 | producer 更新 |
| SM-8 | B8 列冊 | s5-ceiling-analysis.md B8 行 | 歸因改寫為實證版（33+97、非 get_ast）；凍結第二條成立 | — | — |
| SM-9 | alias 建構呼叫（R-F1） | `from m import Cls as C; C()` | mark=`C`、tail=`Cls` → REFERENCES 邊（非 CALLS）——列冊殘差桶，W1 腳本輸出診斷計數 | W1 診斷層 | — |
| SM-10 | mixed target set（Class＋corpus `__init__` 同現） | pyrefly 對某 class 回 `[Class, __init__, __new__]` | **per-call-site 排除條件**：該 site 有 corpus `__init__` target 時不 pseudo-mint（B7a 路徑不變）；fixture 斷言帶 `__init__` class 的 targets 不含 Class（含則記錄實況） | 測試斷言 | producer 更新 |
| SM-11 | 繼承 corpus base 且 base 帶 `__init__` | `class D(B): pass`（B 有 `__init__`） | callee grain＝`Base#__init__()` 非 `Derived` → B7b 後仍 missing——列冊殘差桶（S3 歸因清單預列，防誤歸 B5/B6） | S3 歸因 | — |
| SM-12 | nested class／attribute ctor／同名 class+function | `Outer.Inner()`、`a.b.Cls()`、同 module `class Cls`+`def Cls` | chain 形 `Outer#Inner().` 過 fn_tail_name（tail=`Inner`）各一斷言；同名碰撞＝symbol 字串同、nodes OR IGNORE 合併——已知邊角列冊 | fixture 斷言 | producer 更新 |

## 段落劃分原則

垂直切片：先立度量尺（W1）→ 再改 producer（S1）→ 補 producer 診斷計數
（S2，B8 的 loud-list 收尾）→ 全鏈重跑驗收（S3）。W1 先行使其成為
S1/S3 的同一把尺（同度量面不重寫）。

---

## 段落 W1：S5 結算修訂（R2-3 條款套用記錄＋B7a 度量收割）

### Context

- **UC 引用**：實作「S5 resolved-legacy coverage metric（可重現驗收數字）」。
- s5 結算（`2c44534`）漏套 EP R2-3 凍結條款：Class-callee legacy
  pairs（4,962）與 producer 交集結構性為 0，剔出分母後
  15,172/16,015＝**94.7%**，即刻達標 ≥90%。s5-ceiling-analysis.md 已
  推導完成——本段只做「數字記錄進 EP＋把度量做成可重現腳本」，
  **不重跑舊結算**（kickoff 承接條款）。
- B7a（2,642 pairs，producer 已出邊但 callee grain 是
  `Cls#__init__().` vs legacy `Class@file`）由度量端 `class_segment`
  正規化收割——`graph_db.rs:255` 已有同名邏輯（dunder 崩潰邊回退用），
  腳本複用同語義。

### 依賴錨點
- `class_segment` → 定義 `crates/code-reality/src/graph_db.rs:255` /
  腳本端等價實作 `scripts/s5_coverage.py`（消費端＝本腳本，Python 等價
  正規化，非跨語言引用）。
- 資料源：`<repo>/.code-reality/graph.db` 的 `edges` 表（provenance
  `scip` vs `treesitter-legacy`、kind `CALLS`）＋ `nodes` 表
  （node-key = name@file basename 正規化，F6 凍結 grain）。

### 核心實作要點
- `scripts/s5_coverage.py`：讀 graph.db → 建 producer / legacy-resolved
  兩個 pair set（caller key × callee key）→ 四層輸出：
  1. raw（重現 72.3% 對帳面）
  2. R2-3 剔分母 → 94.7% 記錄版。**carve 判據寫死 SQL 語義
     （R-F5）**：legacy edge 的 callee_symbol join `nodes` 命中
     `provenance='treesitter-legacy' AND kind='Class'`（非 symbol
     字串猜測）。
  3. B7a 正規化（producer method-callee `__init__` → own-class key，
     `graph_db.rs:255` class_segment 同語義；**L 端永不正規化**——
     legacy callee 無 `__init__` 尾）＋剔分母診斷版
  4. **gate 層＝全分母（不剔）＋B7a 正規化**——B7b 前重現 s5 預測
     84.9%（B7a 收割），B7b 後 ≥95.7% 退場 gate。
- 診斷計數（R-F1/R-F2）：basename 碰撞 pair 數（同名 (name,basename)
  對應多個全路徑）；producer REFERENCES-kind 邊被 legacy CALLS 命中數
  （alias 殘差桶，SM-9）。
- node-key grain 維持 (name, file basename)（與 s5-pairset.json 對帳
  同口徑）；F6 原文是 (file,name) 鍵表映射，basename 是本腳本選擇
  ——碰撞量由診斷計數列冊監督。
- B7b 前置驗收：在**現有** graph.db（未含 B7b）上跑，第 2 層數字與
  94.7% 對帳（容差 ±0.3pp，超出即停下歸因——實測 93.85%，差 0.85pp，
  歸因＝graph.db 已過 s5 pairset 之後的重產〔mosaic working tree 前進
  至 `0914dedd`〕＋腳本 keying 微差；gate 層 84.44% vs s5 預測 84.9%
  同因，記錄不阻擋）。

### Pseudo Code

```
scripts/s5_coverage.py
  --db <path> [--json]
  load edges(kind=CALLS): producer = provenance 'scip'; legacy = 'treesitter-legacy'
    legacy-resolved = 兩端 symbol 皆存在於 nodes（synthesized 端點剔，F6 列冊）
  node_key(symbol) -> (name, file basename)   # F6 frozen grain
  b7a_normalize(callee_key): if callee symbol tail == '__init__'
                             and class_segment(symbol) is Some(c): key -> (c, file)
  report:
    raw: |P ∩ L| / |L|
    r2_3: |P ∩ L'| / |L'| where L' = L minus Class-callee pairs (callee 無 () 尾/Class 形)
    b7a: with normalization on P side
  寫 sidecar json（同 s5-pairset.json 鍵名，附各層數字）
```

### 驗證策略
- 對帳測試：現有 mosaic graph.db 上第 2 層 = 94.7%（±0.3pp）；raw 層
  72.3% 對帳（同容差）。fail-loud：對帳失敗印分母構造差異明細。
  （實測記錄：93.85%/71.62%——超容差已歸因〔graph.db 重產於
  mosaic `0914dedd`、s5 pairset 產於 `24ced017` 資料〕，結構四層全
  重現：carve 4,957≈4,962、B7a 收割 84.44%≈84.9%。）
- 純 Python 腳本，`uv run python scripts/s5_coverage.py` 實跑（must-execute）。

---

## 段落 S1：B7b 偽建構子 mint（producer）

### Context

- **UC 引用**：更新「Rust-native Python occurrence producer」。
- probe 事實（見總覽）：class-kind call target 目前 mint 成
  `` Cls# `` → ingest gate 濾除 → 邊不存在（2,254 pairs 的機制）。
- 修法：call target 的 innermost kind 為 `DefKind::Class` 時，改 mint
  偽建構子符號 `` `pkg.mod`/Cls(). ``（`symbol.rs` 現有 descriptor 體系
  下＝Class descriptor 去 `#` 加 `().`，或等效構造）＋**唯一一筆** DEF
  occurrence（name range＝class name、node range＝class stmt range），
  使 cache ingest（`fn_tail_name` 過閘）與 graph_db
  （`def_symbols.contains` 過閘）兩面成立，且 tail name `Cls` 對上
  legacy `Class@file` node-key → 真匹配。
- 語義約束：與 W1 共享「B7a 類（corpus `__init__`）行為不變」——
  dunder-collapsed `Cls#__init__().` mint 路徑零改動。

### 依賴錨點
- `mint_targets` → 定義 `crates/pyrefly-producer/src/lib.rs:139` /
  消費 `lib.rs:102-113`（calls 迴圈）。
- `fn_tail_name` gate → 定義 `crates/code-reality/src/engine.rs`（ingest
  濾除點：tails 建立決定性第一道 `cache.rs:65`＋occurrences 收錄
  `cache.rs:100`）——本段不動它，靠 `().` 尾通過。
- `IndexEmitter::push_def` → `crates/pyrefly-producer/src/emit.rs:69`。

### 核心實作要點
- **結構（R-F3/R-F4 裁決）**：pass 1 是**無副作用資料掃描**（只建
  `called_classes: HashSet<symbol>`，不 push 任何 occurrence）；發射
  維持既有單一 per-module 迴圈與 defs→refs→calls 原序，DEF 補發內嵌
  在 defs 迴圈（禁 iterate 集合發射——HashSet 序不得進輸出）。
- `mint_targets` 回傳 target 加 kind 資訊：call site 的 target
  innermost kind 為 `DefKind::Class` 時改 mint
  `` `pkg.mod`/Cls(). ``（`target_symbol` 體系下等效構造）。
- **per-call-site 排除條件（SM-10）**：同一 call site 有任何 corpus
  `__init__` target 被保留時，Class target 不 pseudo-mint——
  B7a（帶 corpus `__init__`）行為保證不變。
- DEF 補發：defs 迴圈對 class def 追加 `` /Cls(). `` DEF occurrence
  （name/node range 同 class def），僅當該 symbol 在
  `called_classes`。冪等：集合去重。
- `EmitReport` 新增 `minted_pseudo_ctor_refs` 與
  `minted_pseudo_ctor_defs` 兩欄（SM-4 冪等機器可查：refs ≥ defs，
  defs＝被呼叫 class 數），bin 端 `[OK]` 行納入。
- `symbol.rs` 文檔註解更新：pseudo-constructor 形（`Cls().`）加入符號
  形式表；附兩句語義記錄——graph 節點 kind 以 `Function` 呈現
  （`graph_db.rs` hardcode，tail-name 面統一的已知取捨）；同名
  class+function 字串碰撞由 nodes OR IGNORE 合併（Python 遮蔽語義）。
- **參考面不動**：非 call 的 class reference（SM-3）維持 class-shaped
  mint（ingest 照濾）。

### Pseudo Code

```
// lib.rs — pass 1: pure data scan, no emission
let mut called_classes: HashSet<String> = HashSet::new();
for m in &driven.modules {
    for c in &m.calls {
        let targets = resolve_mint_kinds(...);           // (symbol, kind)
        let has_corpus_init = targets.iter().any(is_corpus_init);
        if !has_corpus_init {
            for (sym, Class) in targets { called_classes.insert(pseudo_form(sym)); }
        }
    }
}
// pass 2: existing single per-module loop, original order
for d in &m.defs {
    emit def as today;
    if d.kind == Class { let s = pseudo form; if called_classes.contains(&s) {
        emitter.push_def(&s, d.name_range, d.node_range); report.pseudo_ctor_defs += 1; } }
}
for c in &m.calls { /* same has_corpus_init guard; Class target -> pseudo form ref */ }
```

### 驗證策略
- fixture 擴充（`tests/fixtures/mini`）：dataclass class、無 `__init__`
  class、帶 `__init__` class（B7a 對照組）、跨檔 constructor call、
  純 load 對照、nested class（`Outer.Inner()`）、attribute ctor
  （`a.b.Cls()`）、alias call（`import X as Y; Y()`，SM-9 列冊）。
- `tests/end_to_end.rs` 新斷言：
  - `Cls().` DEF occurrence 存在（恰一筆）＋call ref 成 CALLS 邊
    （graph_db 查詢）；
  - 帶 `__init__` class 的 callee 仍為 `Cls#__init__().` 且其 call
    targets **不含 Class kind**（SM-10，probe 斷言——含則記錄實況
    再裁決）；
  - 純 load 無 pseudo mint；nested/attribute 各一 chain 形斷言。
- determinism 電池：兩次 emit 逐位相同（既有測試延伸）。
- `cargo test -p pyrefly-producer` 全綠；release 實跑
  `pyrefly-index --repo fixtures/mini` 觀察 `[OK]` 行新計數
  （refs ≥ defs）。

---

## 段落 S2：B8 列冊歸因（證據寫回）＋cache ingest 零覆蓋 loud 計數

### Context

- **UC 引用**：無新 UC（文檔＋診斷計數）。
- B8 裁定：非 producer bug（probe 證據見總覽）。處置＝
  s5-ceiling-analysis.md B8 行改寫為實證歸因（97 空 document＋33
  class/variable-only 檔，R2-3 gate 設計行為）——凍結第二條
  （missing 全歸因語義固有類）自此成立。
- 附帶防護：`cache.rs` build 統計新增「SCIP 有 occurrences 但全被
  fn-gate 濾除的 document 數」（loud list 原則——未來 audit 不需重做
  這次的人工挖掘）。Stats/print 納入；不改 schema、不改行為。

### 依賴錨點
- `build_db` → 定義 `crates/code-reality/src/cache.rs:61` / 消費
  `cli.rs`（`--build-cache` 路徑）。
- `s5-ceiling-analysis.md` B8 行 → `ai-analysis/reports/`（文檔錨點）。

### 驗證策略
- `cargo test -p code-reality`（cache 統計不破壞既有測試）；既有測試
  加 mini index（含 class-only document）斷言新計數 ≥1（R-F11，
  loud-list 有 regression cover）。
- mosaic 實跑 `--build-cache` 觀察新計數＝33＋（B7b 後 §S3 重跑時该數
  應下降——被建構的 class 檔案獲得 `().` 形 def）。

---

## 段落 S3：全鏈重跑＋95.7% gate 驗收

### Context

- **UC 引用**：驗收「S5 metric」＋「producer 更新」。
- 依賴：S1、S2 完成後執行。

### 核心實作要點
- mosaic_alpha 全鏈：`pyrefly-index --repo ~/Github/mosaic_alpha`（寫
  slot）→ `code-reality scip_refs --stamp-meta` → `--build-cache` →
  `graph_db build`（既有 ordering）。
- `scripts/s5_coverage.py` gate 層（全分母＋B7a 正規化真匹配）
  **≥95.7%** 為 gate；低於門檻 → 列出殘餘 missing 的 top 分類（**預列
  殘差桶：alias→REFERENCES〔SM-9〕、derived-from-corpus-base
  〔SM-11〕、B5/B6 固有類**），回 S1 歸因（不硬調門檻）。
- 對照組：`tests/equivalence_battery.rs` 及既有 code-reality suites
  （`cargo test`）全綠——CALLS 邊增加不得改變既有查詢面行為語義。

### 驗證策略
- gate 數字寫進完成報告（coverage 三層數字＋minted_pseudo_ctors）。
- 電池不過＝維持並列原則沿用（不觸發 P3）。

---

## 整合策略

- 段落間資料流：W1 腳本是 S3 的驗收尺；S1 mint 改變 SCIP 面，S2 計數
  是 S1 的側面觀察。S1 與 W1 可並行實作，S3 收斂。
- 全 EP 完成判定：SM-1~SM-8 全數可指向證據；S3 gate ≥95.7%。

## EP Review Record（dual-axis, 2026-08-28 — findings 全數採納）

| # | Finding（軸） | 裁決 | 落點 |
|---|---|---|---|
| F1 | alias 建構呼叫落 REFERENCES、侵蝕 gate（結構） | ✅ 列冊殘差桶＋W1 診斷計數＋fixture | SM-9／W1／S3 |
| F2 | basename grain 冒稱 F6 凍結、碰撞未量測（結構） | ✅ 診斷計數列冊監督（維持 basename＝對帳同口徑） | W1 |
| F3 | two-pass 破 document 發射順序（雙軸同報） | ✅ pass 1 改無副作用掃描、發射維持單迴圈原序 | S1 |
| F4 | HashSet iteration 序入輸出風險（正確） | ✅ 明文禁 iterate 集合發射 | SM-7／S1 |
| F5 | carve 判據未定義（正確） | ✅ 寫死 SQL 語義（nodes kind='Class' join） | W1 |
| F6 | nested class 正規化對帳缺案例（正確） | ✅ fixture＋L 端永不正規化明文 | W1／S1 |
| F7 | graph 節點 kind=Function 取捨未記錄（雙軸） | ✅ symbol.rs 文檔記錄 | S1 |
| F8 | 同名 class+function 撞名（雙軸） | ✅ 已知邊角列冊 | SM-12 |
| F9 | minted_pseudo_ctors 單計數無法驗 SM-4（正確） | ✅ 拆 refs/defs 兩欄 | S1 |
| F10 | nested/attribute/alias fixture 缺（正確） | ✅ 全數列入 | S1 |
| F11 | S2 loud counter 無 regression cover（正確） | ✅ mini index 斷言測試 | S2 |
| F12 | mixed target set／derived-base 殘留（正確） | ✅ per-site 排除條件＋殘差桶預列 | SM-10／SM-11 |

## 結算（2026-08-28 build 完成）

- **W1 ✅**：`scripts/s5_coverage.py` 四層輸出＋兩項診斷；對帳記錄
  （raw 71.62%／gate pre-B7b 84.44% 於重產後 graph.db——超容差已歸因
  corpus drift，見 S3）。
- **S1 ✅**：mint 實作＋per-site B7a guard＋alias Class-kind
  local-binding 豁免（probe 實證：alias 建構呼叫死於 local-binding
  guard 而非 REFERENCES 判定——review F1 機制修訂）；fixture 擴充
  （nested/attribute/alias/pure-load）＋end_to_end 釘樁（defs 14、
  refs 7、call_sites 9、pseudo 3/2、CALLS 8／REFERENCES 2、
  B7a no-pseudo 斷言）。
- **S2 ✅**：s5 報告 B8 行改寫＋`docs_fully_filtered` loud 計數
  （**stdout parity 不動**——fndefs frozen-stdout 測試抓到首版
  污染，改走 stderr WARN 面）＋s3_cache regression 測試。
- **S3 ✅（數字）**：mosaic 全鏈重跑（slot cache＋graph_db＋
  import_legacy）。`24ced017` 凍結語料（臨時 worktree 全鏈重現）
  gate **95.42%**（預測 95.7%；Δ0.28pp＝derived-base 殘留桶
  ~110 pairs＋B5/B4/B6/B1b 固有類，B7b 範圍外列冊）；現行 HEAD
  `0914dedd` 93.86%（drift-carved 95.43%——345 stale legacy 端點指向
  lsp_mcp 退役刪檔；兩語料收斂 ~95.4%）。pseudo refs/defs 3,033/377；
  fn-gate 全濾檔 33→26。**post-build review 修正**：首版度量腳本把
  callee kind 誤併 pair key（B7b 真匹配被低報，raw「重現 72%」實為
  bug 副作用）——修正版如上。**嚴格 ≥95.7% 未達——退場時點裁決留
  user**（機制落地＋差距全歸因）。
- 電池：`cargo test --release --workspace` 全綠（見 commit 前重跑）。

## 收尾步驟

1. Capabilities 更新：root AGENTS.md producer 行（B7b mint 註記）＋
   S5 metric 腳本入口；Kanban 卡（Backlog 自動建）搬 Done/。
2. `s5-ceiling-analysis.md` B8 行已於 S2 改寫；EP 歸檔 `_done/`。
3. instruction 檔：crates/AGENTS.md 若有 producer 符號形式描述則同步。
4. `/audit-test` 對新增測試。
5. 完成報告回執素材：commit hash＋三層覆蓋數字（94.7% 記錄版重現＋
   ≥95.7% 真匹配）＋B8 處置證據——user 貼回 ai-rules（roadmap W2
   打勾、產 W3 handoff）。
