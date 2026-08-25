# EP：R3 caller 邊＋closure（Rust 原生）

> **ep_type**: implementation
> **parent**: [ep-rust-migration.md](ep-rust-migration.md)（段 R3 全部；繼承 AD-1〜AD-5／雙凍結紀律，本文僅內嵌 load-bearing 條款）
> **spec 鏈**: ai-rules `ep-code-reality-repo-mcp.md` S2 段（:184-283——能力規格繼承、載體改 Rust；**例外**：spec 要點 3 的 fn_defs 入三表 db＋`SCHEMA_VERSION` bump 設計由 SM-13 **有意取代**為獨立 sidecar——防互毀重建 ping-pong，`cache.rs:3-7` header 已載此定案）；`code-reality-repo-mcp-spec.md`（SM-1/2/6/9，spec :47-60）；研究報告 `rust-precision-ecosystem-research.md` §2/§7（機制已證——本 EP 的規格來源，重寫不搬碼）
> **parity 面宣告**: 新能力無 Python 對象——**既有 R2 輸出面是回歸保護面**（query/stamp/build-cache 的 stdout 位元組與 parity 29 案例零改動）；新 `--callers`/`--closure` 輸出為 Rust 原生設計面（家族風格對齊，無位元組 oracle），驗收走**三源一致**（CLI 輸出＝LSP `incomingCalls`＝closure 起點）
> baseline: `f388b5d`

## 實作總覽

`--callers`/`--closure` 落地在 R2 lib 之上：歸屬語意核心（DEF-enc containment）＋fn_defs 獨立 sidecar（SM-13）＋CLI 新模式＋NT 16 callers 三源驗收。**fn span 的資料載體＝fn DEF occurrence 的 `enclosing_range`**（rust-analyzer 給的是 fn 完整 item span——研究 §2.1 已證，非 DEF occ 自身的 `range`〔那是名稱行〕、亦非 ref occ 的 enc〔那是 callee 回聲，勿用〕）。

| 段 | 內容 | 對應 master |
|----|------|------------|
| S1 | 歸屬語意核心＋protobuf 面存取器（FnSpan/innermost tie/item-level） | R3 |
| S2 | fn_defs sidecar＋sqlite 面 rows（schema/build/stale/ladder；`--build-cache` 擴充） | R3 |
| S3 | callers/closure 輸出組裝＋CLI 新旗標（`--callers`/`--closure`/`--depth`） | R3 |
| S4 | fixture 擴充＋NT L4 三源驗收＋全量回歸 | R3 gate |

**繼承硬約束（load-bearing 內嵌）**：三表 db 與其 `SCHEMA_VERSION` 守衛**零改動**（`scip_refs.py:89,443-445`）——fn_defs 住獨立 sidecar（SM-13，防互毀重建 ping-pong）；共存期既有 Python 檔案零改動；stdout 位元組契約面＝R2 已凍結的輸出（新旗標新增、舊路徑不動——**R2 stdout/exit 面零改動**；`--build-cache` 行為擴充＝sidecar 建置＋stderr 行，stdout/exit 不變）；sidecar home `~/.mosaic/code-reality/` 慣例凍結——**NT 活體 slot 一律唯讀（含 sidecar：不寫入、不重建，測試走 mixed-face）**。

## UC 盤點

### Backlog 關聯
- `rust-caller-edges.md`（本 EP 對應能力卡；build 開始時搬 In-Progress，收尾搬 Done）
- EP 追蹤卡 `ep-rust-migration.md`（In-Progress——R3 完成時補進度行）

### SYSTEM-MAP 影響
無 SYSTEM-MAP.md（本 repo 未建；跨域狀態面由 master EP 承載）。

### 掃描範圍
root AGENTS.md Capabilities（UC-2 行 📋 (EP R3)）、`.kanban/Backlog/rust-caller-edges.md`、`crates/AGENTS.md`、ai-rules `skills/code-reality/SKILL.md`（消費面盤點，**零編輯**——見下）。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| UC-1 符號真相查詢 | ✅ | root AGENTS.md | 無影響 | R2 交付；本 EP 只加不改（回歸面） |
| UC-2 caller 邊查詢 | 📋 | root AGENTS.md＋Backlog 卡 | 新增（✅ 化） | 本 EP 交付 Rust 載體 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| UC-2 caller 邊查詢（callers/closure） | 🟡→✅ | `crates/code-reality`（`callers.rs`＋`fndefs.rs`＋`cli.rs` 新模式） |

### 消費面凍結條款
ai-rules `skills/` 目前 `--callers`/`--closure`/`call_edges` **零出現**（rg -F 全目錄 exit 1，2026-08-25 盤點）——無既寫消費合約需對齊。舊 EP 的 SKILL.md 補寫條款（S2 要點 5）**延至 R7 relay**：共存期 skill 指向 `--project ~/Github/ai-rules` Python 調用形態，Python 無 `--callers`——現在補寫等於廣告不存在的入口。口徑 clause（refs＝所有 non-DEF occ 不可當呼叫數解讀；callers＝歸屬子集）落**本 repo** 文檔（S4 收尾）。

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 查 trait impl 方法引用 | `--callers Type.method` | 兩符號形態消歧（marker impl＋trait decl）＋caller 歸屬＋`[SRC]` 首行 | 無 | UC-2 |
| SM-2 | transitive 影響盤點 | `--closure --depth N` | BFS 過 caller 邊＋環偵測報告＋`[SRC]` | visited set | UC-2 |
| SM-6 | 巨集生成 fn 內呼叫 | 3 元素 single-line enc | 支援（≥35 顆反例不誤殺）——`(sl+1, sl+1)` | 無 | UC-2 |
| SM-9 | hub symbol closure 效能 | 195KB 級 refs | 秒級（sqlite 路徑——tempdir 演練硬 gate）＋按檔聚合 | 無 | UC-2 |
| R3-A | 符號未命中 | `--callers` 無 DEF 查詢 | `[WARN] 查無 DEF：{query}` exit 1（家族語意） | 無 | UC-2 |
| R3-B | 非 fn 形態符號查 callers | 型別/常數查詢 | **兩面一致**：`查無 DEF` exit 1——共用 matcher 宇宙＝`{name}().` 後綴形態（fn-shaped；`engine.rs:29-36` name_pat_match 兩形態皆要求），R2 凍結語意，無面分歧 | 無 | UC-2 |
| R3-C | depth 邊界 | `--depth 0`／負數／非整數 | `[FAIL]` exit 2（需正整數） | 無 | UC-2 |
| R3-D | closure 環 | A↔B 互呼 | visited 環偵測不死循環，報告 cycle 重入數 | visited set | UC-2 |
| R3-E | sidecar 過期 | index 重生成後 mtime 漂移 | stderr WARN＋自動重建；stdout 不變 | 無 | UC-2 |
| R3-F | sidecar 損壞 | 壞 bytes／非 db 檔 | 視同過期→重建；重建失敗→stderr WARN＋protobuf spans 照常答（加速器非依賴） | 無 | UC-2 |
| R3-G | `--build-cache` 擴充 | 明確建置模式 | 同時建三表 db＋fn_defs sidecar；**stdout 位元組凍結（現況單行）**——sidecar 訊息走 stderr | 無 | UC-2 |
| R3-H | 旗標互斥 | `--callers --closure` 同給／`--depth` 無 `--closure`／`--build-cache`/`--stamp-meta` 與 callers/closure 併給（**含無 query positional 形態——互斥條件式擴充含新旗標**，防靜默吞旗標） | `[FAIL]` exit 2 | 無 | UC-2 |
| R3-I | item-level refs | use/const/屬性層 refs | 計數＋清單分離輸出，非靜默丟棄 | 無 | UC-2 |
| R3-J | NT 活體 slot | 測試對 NT 查詢 | 唯讀：三表 db fresh 用 sqlite refs＋sidecar 缺席走 protobuf spans（**不寫入 sidecar**——frozen home 汙染禁令；查詢前後 sidecar 存在性與 mtime 快照不變斷言） | 無 | UC-2 |
| R3-K | DEF 命中零 refs | 死碼 fn 查 callers | `[OK] {query}：0 callers（0 sites）`＋item-level `0 處` 行（穩定輸出形狀）＋exit 0 | 無 | UC-2 |

## 段落 0：全域研究摘要（已證事實＋錨點）

### 機制已證（勿重驗）

- **DEF-enc containment 96.9%**：239,602 顆 workspace fn ref occ 中 232,078 顆成功歸屬；7,524 顆（3.1%）item 層（use/const/屬性），其中 ≥35 顆為巨集生成 fn 真呼叫被 4-element-only 誤殺——**3 元素 span 支援必須**（研究 §2.2；reviewer 獨立重跑 byte-level 複現）。96.9% 是歸屬覆蓋率非全量正確率（§7 限制繼承：行級粒度、巨集展開歸屬＝叫用處所在 fn——語義合理、抽驗非全量）。
- **16 callers 三源基準（2026-08-25 機械重計裁決）**：`EventStoreLifecycle.open`＝18 refs→**16 callers／18 sites**（1 trait impl＋15 test fn；兩 test fn 各 2 sites：2461/2470、2514/2526）。逐筆地面真相＝ai-rules `.agent-tmp/research/_e2e_rerun.out:7-24`（18 行 refs 機械複數＝16 distinct）。**上游文件（研究報告 §2.4 prose「17 callers（1 impl＋16 tests）」／舊 spec :205／master EP gate）與其自身證據檔算術矛盾**——17 callers＋兩雙 site fn ⇒ ≥19 refs ≠ 18；唯一自洽解＝16。LSP 側名單**從未逐筆落盤**（報告僅摘要宣稱），源 2 於 R3 build 階段重取 `incomingCalls` 對帳結案（LSP＝16 → 17 屬謄寫錯；LSP＝17 → 存在 SCIP 缺 1 caller 的實質差，個案調查記錄）。master EP gate 行隨本 EP 修正。
- **span 載體形態**：fn DEF occ 100% 帶 enc（64,164 顆＝4 元素 63,841＋3 元素 323）；4 元素 `[sl,sc,el,ec]`→`(sl+1, el+1)`、3 元素 `[sl,sc,ec]`→`(sl+1, sl+1)`（SCIP 0-based→**FnSpan 1-based inclusive**）。參考實作：`scip_caller_e2e.py`（歸屬演算法——**語意參照，生產碼 Rust 重寫**；其 tie 用「最大 start_line 勝出」變體，**EP 版為準**：`(寬度, 來源序)` 最小）。
- **Rust 資料可讀（2026-08-25 驗）**：`scip` 0.9.0 rust-protobuf `Occurrence` 含 `pub enclosing_range: Vec<i32>`（cargo registry `scip-0.9.0/src/generated/scip.rs:3120`；**上游 proto 已標 deprecated 建議 `typed_enclosing_range`**——rust-analyzer 產物走 legacy 欄位、R2 已釘 scip 0.9，現在可用無風險；升級 scip 時此為已知遷移點）。
- **已知情事（避免誤用）**：ref occ 的 enc＝同檔 callee 定義 span 回聲（勿當歸屬依據）；DEF occ 的 `range`＝名稱行（不含體）；外部符號（`github.com/rust-lang` 前綴）無 workspace DEF→不成 span 候選，對其 refs 的歸屬不受影響。

### 依賴錨點（R2 定義端；R3 為新消費端）

| 錨點 | 定義端（Rust R2） | R3 消費語意 |
|------|-----------------|------------|
| `Query::parse`/`matches_query`（兩符號形態消歧） | `engine.rs:59-90` | 目標解析（SM-1）——callers 查詢與 query 同一 matcher |
| `fn_tail_name`（FN_TAIL 述詞） | `engine.rs:92` | span 候選過濾（fn DEF 判定：`symbol_roles&1`＋FN_TAIL） |
| `loc_line`／`ln` | `engine.rs:122-136` | site 行組裝（ref occ 行＝`range[0]+1`） |
| `find_defs`/`find_refs` 迭代序 | `engine.rs:141-169` | protobuf 面結構化 rows 的同源迭代（掃描序保兩面一致） |
| `source_line`（`[SRC]`） | `engine.rs:380` | callers/closure 首行 |
| `LoadedIndex { index, stderr }`（lib 不 print、WARN 走回傳通道的先例） | `engine.rs:207-211` | `fn_spans` 回傳形狀沿用（見 S1 要點 2） |
| `Face`/`open_face`（面選擇三分支） | `cache.rs:227-287` | refs_rows sqlite 面＋ladder 模式複用 |
| `build_db`（單交易＋tmp+rename） | `cache.rs:61` | sidecar builder 同形 |
| `stale_reason`（四訊號 fail-loud） | `cache.rs:185-216` | sidecar 守衛同形（獨立 meta/schema） |
| `sqlite_path`（`.scip`→`.scip.db`，**全檔名＋`.db`**） | `cache.rs:48-57` | sidecar 同法則：`file_name()`＋`.fndefs.db`→`index.scip.fndefs.db`（釘死單一讀法） |
| `run` 模式路由／`parse_tokens`/FLAGS 表 | `cli.rs:30`（FLAGS）、`cli.rs:69-330` | 新旗標入 FLAGS 表（縮寫機制沿用）；路由插 query 模式組 |
| 互斥條件式（鍵於 query 在場＋stamp） | `cli.rs:201-208`（Python `scip_refs.py:764-790` 同構） | **條件式擴充含新旗標**（S3 要點 1——`--build-cache --callers` 無 query 形態不得靜默通過） |
| parity harness（NT slot guard） | `tests/parity/test_scip_refs_parity.py:225-236` | R3-J 唯讀 guard 沿用＋sidecar 快照斷言 |
| 三表 schema（不可動面） | `scip_refs.py:319-338`（`occurrences` 僅 `seq/symbol/rel_path/line/is_def` 無 range） | fn_defs sidecar 必然性的根據；DDL 零改動 |

### 關鍵設計決策（本 EP 定案）

- **面一致聲明（R3-B 修訂）**：matcher 兩形態（`Type.method` 三條件／裸名單條件）皆要求符號以 `{name}().` 結尾——**fn-shaped 後綴是兩面共同宇宙**（protobuf 面與 sqlite 面〔FN_TAIL 過濾入庫〕在此交集上一致）；非 fn 形態查詢兩面皆 `查無 DEF` exit 1（R2 凍結語意）。「兩面一致」條款以此宇宙為界。
- **tie 規則 EP 版**：innermost＝candidates（`start_line <= line <= end_line`，同檔）取 `(end_line - start_line, 來源序 seq)` 最小；同寬先見者勝。行級粒度誤差源（brace 邊界 ref）docstring 明記（研究 §7 條款繼承）。
- **caller 排序**：first-site 掃描序（決定性、兩面等價——protobuf 掃描序＝sqlite `seq` PK 插入序）；site 行在 caller 行下按掃描序。
- **closure 展開語意**：seed 經使用者 query 解析（多 DEF 符號聯集）；level 1＝callers(seed)；level k+1＝level k 各符號（**精確符號匹配**，非 query pattern——caller 已是編譯器級消歧符號）的 callers 聯集，扣除 visited；cycle＝frontier 命中 visited 的重入計數。per-depth 按檔聚合＝新發現符號按其**定義檔**（span 的 rel_path）分組。
- **sidecar 生命週期**：對齊三表家族規則——查詢時 sidecar 缺席→protobuf spans（**不自動建**，對齊「無 db→protobuf、不建 db」）；過期→stderr WARN＋自動重建（重建需載 protobuf index，一次載入複用）；重建失敗→WARN＋protobuf spans 照常答。`--build-cache` 擴充為建雙 artifacts（stdout 凍結、sidecar 行走 stderr）。
- **輸出風格**：家族對齊（`[SRC]` 首行／`[OK]` 中文標籤／2-space caller 行／4-space site 行）——Rust 原生面無位元組 oracle，設計自由但風格一致。

### 風險假設（本 EP 剩餘）

| 等級 | 假設 | 驗證 |
|------|------|------|
| 中 | **SCIP 16 vs LSP 數字分歧未結**（上游 17 與證據檔矛盾；LSP 名單從未落盤） | S4 LSP 重取對帳（名單級）；分歧→個案調查＋記錄，**gate 以重取結果結案（PENDING 阻斷 EP 結案）** |
| 中 | 新旗標 argv 邊界（`--depth` 值語法／縮寫／互斥）與 R2 argparse 模擬機制整合——無 Python oracle，一致性靠既有機制對齊 | S3 cargo 單元測試（值消費規則對齊 `--index`/`--repo` 既有機制＋正整數 gate；`--c` 縮寫從 unrecognized 變 ambiguous 的行為釘住） |
| 中 | tie 規則 EP 版（寬度,seq）與研究腳本版（最大 start_line）在 NT 名單級可見差 | S4 NT 16 callers 名單級核對（EP 版為準；分歧→記錄個案判讀） |
| 中 | sidecar 重建在 NT 規模（64k spans）耗時 | S4 tempdir 演練量測（frozen home 不可寫——複製 index 到 tempdir 建） |
| 低 | closure 深度放大（hub symbol depth 3+ frontier 膨脹） | SM-9 秒級 gate（tempdir sqlite 路徑硬斷言）；depth 預設 2 |

## 段落劃分原則

- **依賴序**：S1（純語意，零 IO——先釘死歸屬正確性）→ S2（adapter 面：sidecar＋sqlite rows）→ S3（組裝：輸出＋CLI）→ S4（外部驗收＋回歸）。S1/S2 同 crate 內新模組（`callers`／`fndefs`），S3 觸 `cli`，S4 觸 `tests/`。
- **垂直切片**：S1 收＝cargo 單元測試釘 tie/span/item 語意；S2 收＝sidecar 四守衛＋兩面 rows 一致；S3 收＝CLI 全路由＋輸出格式（fixture 上 e2e）；S4＝master R3 gate 裁決（16 callers 名單級三源一致）。

---

## 段 1：歸屬語意核心＋protobuf 面存取器

### Context
新模組 `callers.rs`（domain/use case——純函數零 IO）＋`engine.rs` 增結構化存取器。UC 引用：實作 UC-2（歸屬機制核心）。依賴：R2 `engine` 述詞與迭代序（錨點表）。語義約束：**與 S2 共享 FnSpan 型別與 1-based inclusive 座標**；與 S3 共享 CallersResult 形狀（`callers: Vec<(caller_symbol, sites)>`＋`item_level: Vec<(rel_path, line)>`——site＝掃描序）；**`callers.rs` 不 import `cli`/`cache`/`fndefs`（spec :194「caller_edges 零 scip_refs import、單向」的 Rust 對應——S1 為驗收面）**。

### 基礎設施盤點
`engine.rs` 既有：`fn_tail_name`（span 候選過濾）、`ln`（行轉換）、迭代序先例（`find_defs`/`find_refs` documents→occurrences 內外層）、`LoadedIndex`（WARN 回傳通道先例）。無需外部 crate（BFS＝std collections）。

### 核心實作要點
1. `FnSpan { symbol: String, rel_path: String, start_line: i64, end_line: i64, seq: usize }`（1-based inclusive；seq＝掃描序）
2. `engine::fn_spans(index) -> (BTreeMap<rel_path, Vec<FnSpan>>, Vec<String>)`：fn DEF occ（`symbol_roles&1`＋`fn_tail_name` 命中）的 **`enclosing_range`** 解析——4 元素→`(sl+1, el+1)`、3 元素→`(sl+1, sl+1)`（SM-6）；**其他元素數→跳過該 span ＋回傳通道收集 WARN 行**（fail-loud 不靜默；lib 不 print——`LoadedIndex.stderr` 先例；研究未見其他形態，防禦面）
3. `engine::refs_rows(index, symbols) -> HashMap<symbol, Vec<(rel_path, line)>>`：non-DEF occ、掃描序（`find_refs` 同源迭代，結構化而非 display 字串——禁解析格式化字串）
4. `callers::attribute(rows, spans_by_doc) -> CallersResult`：innermost tie＝`(end-start, seq)` 最小（EP 版）；無候選→item_level。**docstring 明記行級粒度誤差源**（brace 邊界、同寬 tie）
5. `callers::closure(seed_symbols, expand, depth) -> ClosureResult`：BFS＋visited 環計數＋per-depth 按定義檔聚合；`expand`＝注入的「精確符號→CallersResult」查詢包裝（S3 組裝兩面）

### Pseudo Code
```rust
// engine.rs 增列
pub struct FnSpan { pub symbol: String, pub rel_path: String,
                    pub start_line: i64, pub end_line: i64, pub seq: usize }
pub fn fn_spans(index: &Index)
    -> (BTreeMap<String, Vec<FnSpan>>, Vec<String> /*warns*/);
    // per doc: for occ where roles&1 && fn_tail_name(occ.symbol).is_some()
    //   enc = occ.enclosing_range; 4/3 元素解析（0-based→1-based）；seq=掃描計數
    //   其他元素數 → push WARN 行、跳過該 span
pub fn refs_rows(index: &Index, syms: &BTreeSet<String>)
    -> HashMap<String, Vec<(String /*rel_path*/, i64 /*line*/)>>;

// callers.rs（新）
pub struct CallersResult { pub callers: Vec<(String, Vec<(String, i64)>)>,
                           pub item_level: Vec<(String, i64)> }
pub fn attribute(rows: &[(String /*sym*/, String /*path*/, i64 /*line*/)],
                 spans: &BTreeMap<String, Vec<FnSpan>>) -> CallersResult;
    // innermost: candidates = spans[path] where s.start <= line <= s.end
    //            min by (s.end - s.start, s.seq)；None → item_level
pub struct ClosureResult { /* levels: Vec<Level{symbols, by_file}>, cycle_reentries: usize */ }
pub fn closure(seeds: &[String], expand: &dyn Fn(&str) -> CallersResult,
               depth: usize) -> ClosureResult;  // BFS + visited
```

### Invariant Impact
- 受影響 domain invariant：歸屬正確性（tie/span 解析錯＝caller 名單靜默錯誤，下游全錯——本工具的 silent-corruption 面）
- critical path：span 解析（enc 形態判定）＋tie 選取
- 驗證對齊：S1 單元測試組（下）逐項對應；S4 NT 名單級最終裁決

### 驗證策略
cargo 單元測試：4/3 元素 span 解析（含 0-based→1-based 邊界：enc `[10,0,12,5]`→span 11-13）、**其他元素數→WARN 行＋跳過**、巢狀 fn innermost（outer 100-200／inner 120-130／ref@125→inner）、同寬 tie 先見者勝（seq 小者）、邊界行 inclusive（ref@start／ref@end 皆歸屬）、item-level 分離（span 外 ref）、closure **depth=1（單層截斷）**／深度截斷／環偵測（A↔B：cycle 計數且不死循環）／**零 frontier（seed 無 callers→各 depth 空＋cycles 0）**／per-depth 按定義檔聚合。已知未覆蓋：真實巨集展開體內歸屬（NT L4 抽驗承接——研究 §7 條款）。

## 段 2：fn_defs sidecar＋sqlite 面 rows

### Context
新模組 `fndefs.rs`（adapter）。UC 引用：實作 UC-2（sqlite 路徑＝SM-9 秒級的載體）。依賴：S1 FnSpan；R2 `cache.rs` 慣例（builder／守衛／ladder 形態）。語義約束：**SM-13——sidecar 與三表 db 完全獨立**（各自 meta/schema 版本/staleness；Python 永不讀寫 sidecar）；與 S3 共享「spans 來源選擇」介面。

### Invariant Impact
- 受影響 domain invariant：**sidecar↔index 一致性**（過期 sidecar 服務→舊 span→靜默錯歸屬——與三表同級 silent-corruption 面）；**三表 db 不可觸**（schema 互通——任何對三表的寫入都是 SM-13 違規）
- critical path：四守衛＋tmp+rename 原子換入＋腐損 db 偵測（fallible probe 區分開檔/查詢失敗——R2 踩坑：sqlite 延遲報錯）
- 驗證對齊：守衛逐一注入測試（touch index mtime／改 meta.head／改 schema／寫壞 bytes）→過期重建或 WARN 回退

### 核心實作要點
1. Schema（獨立檔 `fndefs_path`＝`file_name()`＋`.fndefs.db`→`index.scip.fndefs.db`——單一讀法釘死）：
   ```sql
   CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
   CREATE TABLE fn_defs (seq INTEGER PRIMARY KEY,  -- VACUUM 不重編——tie 先見者依據（三表 :322-323 前例）
       symbol TEXT NOT NULL, rel_path TEXT NOT NULL,
       start_line INTEGER NOT NULL, end_line INTEGER NOT NULL);
   CREATE INDEX idx_fn_defs_rel_path ON fn_defs(rel_path);
   ```
   meta 三鍵：`head`（sidecar head 快照）／`schema`＝`FNDEFS_SCHEMA_VERSION="1"`（**獨立常數，非三表的 SCHEMA_VERSION**）／`tool`＝`"code-reality.fndefs"`（釘死）
2. builder：單交易插入＋tmp 先清後寫＋`fs::rename`（`build_db` 同形）；stats 行 `[OK] fn_defs sidecar built：{path}（{n} spans）`——**僅 stderr**（`--build-cache` stdout 凍結條款）
3. `stale_sidecar_reason`：四訊號同形（db mtime<index mtime／sidecar head 漂移／schema 版本不符／stat 或 probe 失敗→視同過期——損壞即重建）
4. `Face`（`cache.rs`）增 `refs_rows(symbols)`：`SELECT rel_path, line FROM occurrences WHERE symbol=? AND is_def=0 ORDER BY seq`（既有表零改動）；`fndefs.rs` 提供 `spans(conn) -> BTreeMap<rel_path, Vec<FnSpan>>`（`ORDER BY seq`）
5. **ladder（spans 來源選擇）**：fresh sidecar→sqlite spans；缺席→protobuf spans（不建檔）；過期→WARN＋重建（載 index 一次複用）；重建失敗→WARN＋protobuf spans 照常答

### Pseudo Code
```rust
// fndefs.rs
pub const FNDEFS_SCHEMA_VERSION: &str = "1";
pub fn fndefs_path(index_path: &Path) -> PathBuf;   // file_name() + ".fndefs.db"
pub fn build_sidecar(index: &Index, path: &Path, sidecar_head: &str)
    -> Result<usize /*spans*/, String>;             // 單交易＋tmp+rename
pub fn stale_sidecar_reason(index_path: &Path, p: &Path) -> Option<String>;
pub fn load_spans(p: &Path) -> Result<BTreeMap<String, Vec<FnSpan>>, String>;
// cache.rs Face 增（三表零改動）
impl Face { pub fn refs_rows(&self, syms: &BTreeSet<String>)
    -> Result<HashMap<String, Vec<(String, i64)>>, String>; }
```

### 驗證策略
cargo 測試：守衛四注入（過期→重建訊息＋輸出不變）、壞 bytes→重建、重建失敗注入（改目錄權限形態）→WARN＋protobuf spans 照常、缺席不建檔（查詢後無新檔）、**兩面一致**（同 fixture：protobuf `fn_spans`/`refs_rows` vs sidecar+sqlite rows→attribute 輸出等價）、`--build-cache` 後 stdout 單行位元組不變＋sidecar 行在 stderr、三表 db 檔案位元組在 sidecar 建置前後不變（SM-13 機械斷言）。已知未覆蓋：NT 規模建置耗時（S4 tempdir 演練量測）。

## 段 3：callers/closure 輸出組裝＋CLI 新旗標

### Context
`cli.rs` 模式路由擴充＋`callers.rs` 輸出組裝。UC 引用：實作 UC-2（CLI 面）。依賴：S1/S2。語義約束：**R2 既有路徑的路由序與輸出零改動**（互斥評估序照 Python `:764-831` 家族）；新旗標入 FLAGS 表（argparse 縮寫機制沿用——無歧義時 `--call`→`--callers`；**`--c` 從 unrecognized 變 ambiguous（2 match）——行為釘住（兩者皆 exit 2，stderr 面）**）。

### 核心實作要點
1. 新旗標：`--callers`／`--closure`（query 模式組）；兩者互斥；`--depth` 僅伴 `--closure`（`[FAIL] --depth 僅伴 --closure 使用` exit 2）；值消費對齊 `--index`/`--repo` 既有機制＋正整數 gate（`[FAIL] --depth 需正整數：{v}` exit 2）；`--callers`/`--closure` 無 query positional→既有「需提供查詢或 --audit」FAIL（凍結文案）。**互斥條件式擴充**：既有條件（鍵於 `query.is_some()`＋stamp——`cli.rs:201-208`）加 callers/closure disjunct（`--build-cache --callers`／`--stamp-meta --closure` 等無 query 形態不得靜默吞旗標）；新增組合沿用既有互斥文案語意（callers/closure 屬查詢模式）——**既有輸入行為零改動、文案不動**
2. `--callers` 輸出（家族風格；位元組為 R3 原生設計面）：
   ```
   [SRC] ...
   [OK] {query}：{N} callers（{M} sites）
     {caller tail}（{k} 處）
       {rel_path}:{line}          ← site 行（掃描序；＝call_edges 邊集）
   ...
     item-level：{n} 處（未歸屬 fn——use/const/屬性層）
       {rel_path}:{line}
   ```
   caller 行＝`tail(symbol)`（家族 display 慣例）＋site 數；排序＝first-site 掃描序；**輸出形狀穩定**（零 callers→`0 callers（0 sites）`＋item-level `0 處` 行照印——R3-K）；DEF 未命中→`[WARN] 查無 DEF：{query}` exit 1（家族）
3. `--closure` 輸出：`[OK] closure：{query}（depth={n}）`＋逐 depth `  depth {k}：{x} callers`＋按定義檔聚合行（`    {rel_path}：{y} 符號`）＋末行 `  cycles：{c} 處（frontier 重入已拜訪符號）`；exit 0（DEF 命中即 0——零 callers 亦 0）
4. 路由插點：query 模式組內（互斥檢查之後、既有 query 前）：`--callers`→callers 模式；`--closure`→closure 模式；一般 query 落既有路徑不變
5. `--build-cache` 擴充：建三表後接建 sidecar（同一 index parse 複用）；sidecar 行 stderr

### Pseudo Code
```rust
// cli.rs FLAGS 增 {"--callers", false}, {"--closure", false}, {"--depth", true}
// 互斥條件式擴充（cli.rs:201-208 形態）：
//   build_cache && (stamp || query.is_some() || callers || closure) → 既有文案 FAIL
//   stamp_meta && (query.is_some() || callers || closure)           → 既有文案 FAIL
// 新檢查：
if flag("callers") && flag("closure") { return fail_mutual("--callers", "--closure"); }
if has_value("depth") && !flag("closure") { return fail("--depth 僅伴 --closure 使用"); }
match mode {
    Callers  => { /* resolve defs(query) → rows+spans（面選擇）→ attribute → 組裝 */ }
    Closure  => { /* seeds → closure(seeds, expand, depth) → 組裝 */ }
    _ => /* 既有路徑零改動 */ }
// expand = |sym| 兩面查詢包裝：Face::Sqlite→refs_rows＋sidecar spans；
//           否則 protobuf index 一次載入（rows+spans 同源）→ attribute
```

### 驗證策略
cargo 單元測試：互斥族（callers×closure、depth 無 closure、**`--build-cache --callers`／`--stamp-meta --closure` 無 query 形態**、與既有組合）、depth 值族（0/負/非整數→FAIL；`1`/`9`→OK）、縮寫（`--call`/`--clos`/`--dep 2`／**`--c` ambiguous exit 2**）、無 query→凍結 FAIL 文案、輸出組裝（fixture：caller 行/site 行/item-level 分離/**零 refs 穩定形狀**/closure 逐 depth/cycles 行）。既有 R2 路由回歸：無新旗標時 `run` 行為逐案例不變（既有 cli 測試全綠）。已知未覆蓋：protobuf/sqlite 混合面的 callers 輸出等價（S4 fixture 雙跑承接）。

## 段 4：fixture 擴充＋NT L4 三源驗收＋全量回歸

### Context
master R3 gate 裁決。UC 引用：UC-2 驗收。依賴：S1-S3。語義約束：NT 活體 slot 唯讀（R3-J——sidecar 不寫入 frozen home；測試走 mixed-face）。**測試 harness 落點釘死：pytest `tests/parity/test_scip_refs_parity.py` 內 Rust-only 案例**（無 Python oracle 不雙跑；`nt_fresh` guard 沿用＋擴充「sidecar 預存即 skip」——防側門寫入與假失敗）。

### 核心實作要點
1. **fixture 擴充**（`tests/parity/make_fixture.py` 產 `rich_callers.scip`，commit 進 repo；cargo 測試讀同檔）：巢狀 fn（innermost）、同寬 tie 對、巨集 single-line span（3 元素 enc）、跨 fn 多 site caller（2 sites）、item-level ref（span 外）、closure 環對（A↔B）、外部符號 ref（`github.com/rust-lang` 前綴——歸屬自然行為文檔化）
2. **fixture 雙面等價**：protobuf 面（無 db/sidecar）與 `--build-cache` 後 sqlite 面各跑 `--callers` 與 `--closure`——**stdout 逐位元組相等**（面選擇 ladder/排序路徑的等價裁決）
3. **NT L4（唯讀，Rust-only 案例）**：`--callers EventStoreLifecycle.open --repo <NT>` → **16 callers／18 sites**；**名單級 pin**（測試內嵌全列，不引用 ai-rules 臨時檔）：1 impl variant（`impl#[EventStoreLifecycle][KernelEventStore]open().`）＋15 test fn（symbol 含 `kernel/tests/` 段——**mod tests 內 fn，非獨立 test 檔**）＋18 個 site 行號逐筆（1356/1742/1791/2461/2470/2514/2526/2798/2870/2937/3050/3075/3119/3166/3211/3293/3373/3474）；雙 site fn 兩顆（2461/2470、2514/2526 各屬同 caller）；item-level=0。**pin 隨 NT 索引世代漂移＝顯式再裁決**（非靜默——R2 18-refs NT live pin 同型前例）；`--closure --depth 2` depth-1 集合＝callers 集（三源第三源：closure 起點一致）＋環偵測正常；**查詢前後 sidecar 存在性與 mtime 快照不變**斷言（R3-J）；`nt_fresh` guard 擴 sidecar 預存 skip
4. **三源對帳（LSP，源 2）**：整合測試 probe `127.0.0.1:8000/mcp`——在場→兩步 call hierarchy（`prepareCallHierarchy` @ kernel.rs:544:12→`incomingCalls`）與 CLI 輸出**名單級比對**（fn 名集合相等——LSP 列 item 起始行、SCIP 列呼叫點行，行不可比）；**原始 JSON 回應存檔進本 repo**（`tests/parity/fixtures/lsp_incomingcalls_*.json`——oracle 不依賴 server 常駐）；缺席→顯式 skip＋build 報告記錄——**PENDING 為阻斷結案狀態（gate 不因 PENDING 滿足；補測後結案）**。對帳結果記錄 16/17 裁決（風險表 row 1 結案）
5. **SM-9 硬 gate（sqlite 路徑——tempdir 演練）**：copy NT `index.scip`→tempdir→`--build-cache`（建三表＋sidecar；**量測建置耗時＝風險 row 4 著落**）→sqlite 路徑 `--closure --depth 2`→**wall-clock 斷言 ≤10s**（研究：sqlite refs 查詢次秒級、protobuf 全量 parse 0.9s——10s 為量級界線非緊繃值；CI 環境差異允許本地 heavy 斷言＋記錄分級，但「秒級」必須有數字構成 gate）
6. **回歸**：既有 parity 29 案例全綠（R2 輸出面零改動的機械證明）＋`uv run pytest` 全綠＋`cargo test`/`clippy -D warnings`/`cargo deny` 全綠
7. 收尾：root AGENTS.md UC-2 行 📋→✅（Rust 載體：`code-reality scip_refs --callers/--closure`）＋口徑 clause（refs vs callers——本 repo 文檔）；`crates/AGENTS.md` 補 `callers`/`fndefs` 模組導航；kanban `rust-caller-edges` In-Progress→Done＋master 追蹤卡 R3✅；**ai-rules 零編輯**（消費面凍結條款——R7 relay 才動）；master EP R3 gate 行 16-callers 修正隨本 EP（上游研究報告為歷史文件不改——分歧記錄在本 EP 段落 0）

### 驗證策略（＝master R3 gate 裁決）
NT 名單級 pin 全過＋LSP 對帳完成（**PENDING 阻斷結案**——skip 僅是測試層，build 報告必須補測結案）＋SM-9 tempdir 硬 gate＋fixture 雙面位元組等價＋全量回歸綠。**已知未覆蓋**：巨集展開體內歸屬（抽驗非全量——研究 §7 條款繼承）；96.9% 覆蓋數字的全量重算（研究已證，生產版不做 workspace 全量掃描）。

---

## 整合策略

- **跨段整合點**：S1 FnSpan/CallersResult＝S2 sidecar 與 S3 組裝的共同型別；S2 ladder＝S3 expand 的面選擇；S4 裁決全體（兩面 callers 輸出等價＝fixture 雙跑位元組比對；NT mixed-face＝唯讀事實路徑）。
- **baseline**: `f388b5d`。
- **回退路徑**：全程 additive（新模組/新旗標/新 fixture；R2 stdout/exit 面零改動——`--build-cache` 行為擴充僅 stderr 面）——任何段失敗即停，舊查詢面不受影響（消費端零感知）。
- **git**：每段一 commit（user consent gate）。

## Ask First

1. 所有 git commit（consent 規則）
2. 無新增（hazard/audit/MCP 不在本 EP 範圍；schema 合併評估不早於 R7——SM-13 條款）

## 收尾步驟

1. Capabilities：root AGENTS.md UC-2 行 ✅ 化＋Rust 載體入口；消費場景提煉（Scenario Matrix→自包含描述）
2. Kanban：`rust-caller-edges` Backlog→Done；master 追蹤卡 R3✅ 進度行
3. instruction 檔：`crates/AGENTS.md` 補 `callers`/`fndefs` 模組（歸屬語意／sidecar 慣例／1-based 座標）；root AGENTS.md Module guide 若需補則最小更新
4. 口徑 clause：本 repo 文檔（README 或 AGENTS.md 備註——refs≠呼叫數；callers＝歸屬子集；item-level 分離）
5. /audit-test 對新測試組跑品質稽核——**工具範圍手動指定 `crates/**/tests/*.rs` 逐檔套用同角度**（audit-test 掃描面預設只含 `tests/` 下 `.py`）；vacuous 檢查重點＝名單級斷言真的比對名單而非僅計數（行號 pin 同時抓 span off-by-one）

## EP Review Record

2026-08-25 三軌獨立審查（完整性事實核對／結構合規／驗收覆蓋——fresh eyes Explore agents）。全部 findings 經 judge 逐項裁決：**採納 22 項修入上文、1 項裁決推翻**（驗收軌「已核對項」宣稱 17 名單全對上——未做算術，被地面真相機械重計推翻，見下）。

| 軌 | 關鍵 findings（已修入上文） |
|----|------|
| 完整性 | 🟡P2 **17 vs 16 算術矛盾**（EP/研究報告/spec「17 callers（1 impl＋16 tests）」與證據檔 18 refs 算術不相容——17＋兩雙 site ⇒ ≥19 refs）→ 主 LLM 打 `_e2e_rerun.out:7-24` 機械重計裁決＝**16 callers／18 sites**（1 impl＋15 test fn），EP 全文改 16＋上游矛盾記錄＋LSP 重取條款；🟡P2 R3-B「protobuf 面照常答」錯述（matcher 兩形態皆要求 `{name}().` 後綴——兩面對非 fn 形態**一致**查無 DEF，`engine.rs:29-36`）→ R3-B 改寫；🟢P3：`--build-cache` stdout「三行」→「單行」（`cli.rs:388-397` 實況）；錨點修正（FLAGS `cli.rs:30`／Face 區間 :227-287／parity guard :225-236）；`enclosing_range` 上游已標 deprecated（升級 scip 已知遷移點）；spec 要點 3（fn_defs 入三表）由 SM-13 **有意取代**腳註；S1 補「callers.rs 不 import cli/cache/fndefs」單向條款 |
| 結構合規 | 🔴P1 同 17/16（與完整性軌獨立會師）；🟡P2 `fn_spans` WARN 通道與 lib 不 print 矛盾→簽名改回傳 `(map, Vec<String>)`（`LoadedIndex.stderr` 先例）；🟡P2 **`--build-cache --callers`（無 query）靜默吞旗標洞**（互斥條件式鍵於 `query.is_some()`——`cli.rs:201-208`）→條件式擴充含新旗標＋測試案例；🟡P2 NT L4 harness 落點未釘→釘 pytest Rust-only 案例＋guard 擴 sidecar 預存 skip；🟢P3：sidecar `tool` 鍵值釘死 `"code-reality.fndefs"`；`fndefs_path` 二讀歧義→釘 `file_name()`＋`.fndefs.db`；斷言③「test 檔路徑形態」→「symbol 含 `kernel/tests/` 段」（mod tests 內 fn）；sidecar「不存在」斷言→存在性＋mtime 快照不變；「R2 路徑零改動」限縮為 stdout/exit 面；`--c` 縮寫 ambiguous 測試項 |
| 驗收 | 🔴P1 名單級斷言實為計數＋形態級（錯誤 tie 可空虛通過）→ **16 caller 全列＋18 site 行號逐筆 pin 進測試**（oracle 內嵌，不依賴 ai-rules 臨時檔）；🔴P1 LSP PENDING 條款自相矛盾（要點 3「補測後結案」vs 驗證策略「或 PENDING 明列」）→ 刪後者、**PENDING＝阻斷結案**、整合測試 skip-when-absent＋JSON 存檔進 repo；🟡P2 SM-9「記錄」≠gate 且 sqlite 路徑在 NT 上永不執行→tempdir 演練硬 gate（≤10s）＋建置耗時量測著落；🟡P2 兩面 callers CLI 輸出等價承諾無承接項→S4 fixture 雙面位元組比對；🟡P2 /audit-test 範圍不含 `.rs`→手動指定；🟢P3：R3-K 零 refs 穩定形狀規格＋測試；depth=1／零 frontier 測試項 |

**不採納 0 項**（驗收軌「17 全對上」核對項非建議，屬裁決推翻对象，不計 finding）。

**裁決記錄（2026-08-25，機械重計）**：`_e2e_rerun.out:7-24` 逐行 18 refs——distinct callers＝16（impl 1＋test fn 15；2461/2470 與 2514/2526 兩顆雙 site）。研究報告 §2.4 prose「17 callers（1 impl＋16 tests）」與其自身證據檔算術矛盾（17＋兩雙 site ⇒ ≥19 refs ≠ 18）；上游 spec/master EP 同染。**本 EP 以證據檔為準（16）**；LSP 側名單從未落盤——源 2 於 S4 build 重取結案（16→謄寫錯結案；17→實質缺口的個案調查）。master EP R3 gate 行隨本 EP 修正；上游研究報告為歷史文件不改。
