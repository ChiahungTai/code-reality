# EP：R2 scip 家族 Rust 化（含 R1 workspace 落地）

> **ep_type**: implementation
> **parent**: [ep-rust-migration.md](ep-rust-migration.md)（段 R2 全部＋**吸收段 R1 全部內容**——master R1 由本 EP 段落 1 執行後標注吸收；繼承其硬約束/AD-1〜AD-5/雙凍結紀律，本文僅內嵌 load-bearing 條款）
> **spec 鏈**: ai-rules `code-reality-repo-mcp-spec.md`（D1-D5 鎖定）；POC 已驗（V1-V5，見 master「EP Validate Findings」）
> **POC**: `poc/r2-byte-identical`（**已驗 byte-identical——本 EP 的規格來源**：matcher/display/[SRC] 語意以此為準；生產化按 lib 分層重寫，不搬 POC 檔案；build+commit 後依生命週期清除）
> baseline: `2eafd8a`（工作樹另含未 commit 的 master EP/instruction/kanban 產物）

## 實作總覽

`crates/code-reality`（lib＋umbrella bin `code-reality`）落地，`scip_refs` 全模式 Rust 化（**--audit 除外——定案：延後 R4**，與 graph_audit 同段；共存期無消費端觸碰 Rust 版，parity harness 不測 audit），每段過 parity gate。

| 段 | 內容 | 對應 master |
|----|------|------------|
| S1 | Cargo workspace＋lib 骨架（吸收 R1：toolchain 檔/deny.toml/`Result<ToolOutput>` API 形狀） | R1 全部 |
| S2 | scip 引擎 lib——protobuf 面（parse/matcher/slot/meta/[SRC]） | R2 |
| S3 | sqlite 面＋builder（三表 schema 互通/三守衛/原子換入/查詢面選擇） | R2 |
| S4 | CLI 子命令 `scip_refs`（argparse 介面對齊/exit 家族/stamp-meta/query/build-cache） | R2 |
| S5 | parity harness＋收尾（NT 真索引 L4＋共用 fixture＋跨語言 db 互通） | R2 gate |

**繼承硬約束（load-bearing 內嵌）**：stdout 位元組＋exit codes（0 命中/1 無 DEF/2 環境錯誤）＝NT 契約面；`[SRC]` 首行（loud error 回應無 `[SRC]` 屬邊界允許）；sidecar home `~/.mosaic/code-reality/` 與 slot/stamp 慣例凍結；**三表 db schema＋`SCHEMA_VERSION` 與 Python 互通**（fn_defs 屬 R3、住獨立 sidecar——SM-13/R3 定案，本 EP 不碰）；共存期**既有 Python 檔案零改動**（`code_reality/`＋ai-rules 副本；新增 `tests/parity/` 與 pyproject marker 註冊屬 master AD-5 授權）；每個 gap＝fail loud——唯一例外見 S3：**衍生面重建失敗的 WARN-loud 回退**（同 Python `:470-478`「壞了不該擋服務」，非靜默：stderr 有 WARN）。

**Scenario Matrix（master 對應）**：SM-3/4/7→S5 裁決；SM-13→S3/S5（跨語言 db 互通）；SM-11/12→全部段落（共存期消費端零感知）；SM-1（trait 消歧）→S2 matcher＋S5 NT 案例。

## UC 引用（master UC 盤點）

- **UC-1 符號真相查詢（refs/defs，trait 消歧）——載體換軌**：能力語意不變，本 EP 交付 Rust 載體
- **UC-4 完整度治理——部分**：query 面＋`[SRC]`；audit 面（`--audit` 兩遍式）延後 R4 與 graph_audit 同段（定案記錄：master 允許的條件條款，本 EP 明文選延後）

## 段落 0：全域研究摘要（POC 已驗事實＋程式碼錨點）

### POC 既證（勿重驗，`poc/r2-byte-identical`）

- **byte-identical 可達**：Rust protobuf-face 掃描 vs Python sqlite-face，`EventStoreLifecycle.open` 查詢 stdout `cmp` 逐位元組相同＋exit 0——matcher 語意、`sorted(defs)`＋掃描序、前 6 refs＋`...共 N 處` 截斷、`tail()` 空格分段、`[SRC]` 組裝全數已驗
- **scip crate 形態**：0.9.0＝**rust-protobuf**（非 prost）——型別 `scip::types::*`、`protobuf::Message::parse_from_bytes(&bytes)`；275MB/2433 docs 解碼 697ms（release）
- **熱路徑**：Python `--build-cache` 9.3s vs Rust 全量 parse 0.9s；sqlite 查詢兩端次秒級

### 關鍵設計決策（POC 之外，本 EP 定案）

- **Rust regex crate 不支援 look-around**（`(?<!\w)` 負向回顧無法表達）→ matcher 與 `FN_TAIL_RE` 一律**手寫字串述詞**（POC 形態已驗），單元測試釘 Python 語意 parity（`my_open`/`reopen` 邊界反例——`scip_refs.py:140` docstring 條款）
- **stderr 不在位元組契約面**（NT 契約＝stdout＋exit codes）：WARN/FAIL 文案盡量對齊（best-effort），parity harness 只 cmp stdout＋exit codes
- **--audit 不掛 flag**：R2 的 clap 不認 `--audit`（未知 flag 天然 loud error）；R4 補齊時加——避免半套行為（掛了 flag 卻回「未實作」的設計味）

### 依賴錨點（Python 定義端；Rust 為新消費端）

| 錨點 | 定義端（Python） | 移植語意 |
|------|-----------------|---------|
| CLI 介面（argparse 全形＋互斥族＋錯誤文案＋exit 2 家族） | `code_reality/scip_refs.py:725-831` | 逐條對齊：`query` 位置參數/`--index`/`--repo`/`--stamp-meta`/`--build-cache`；互斥五條（:764-790）；slot 解析＋legacy 搬遷提示（:792-819） |
| slot 解析（`default_index_path`：repo resolve→basename→slot；空名 exit 2） | `scip_refs.py:575-589` | 同語意（resolve 防 `--repo .` 塌縮） |
| matcher（`_matcher`：`Type.method` 三條件〔名尾+marker 或 trait-decl〕；裸名單條件） | `scip_refs.py:135-159` | POC 手寫述詞（已驗）＋邊界測試 |
| `FN_TAIL_RE`（`(?<!\w)(\w+)\(\)\.$` 捕獲 method 名） | `scip_refs.py:82` | 手寫述詞：字尾 `name().`＋前界非 word＋回捕 name |
| `find_defs`/`find_refs`（DEF=roles&1；掃描序 append） | `scip_refs.py:162-179` | BTreeMap（sorted 迭代）＋掃描序 Vec |
| `ln`/`loc_line`（range[0]+1；<2 元素→-1→`:?`） | `scip_refs.py:116-122` | 直譯 |
| `report`（display 截斷：前 6＋`...共 N 處`；`[WARN] 查無 DEF` exit 1） | `scip_refs.py:182-200` | POC 已驗 |
| `source_line`（`[SRC]` 組裝＋三 WARN 守衛：stamp 舊/repo 不符/drift——stderr） | `scip_refs.py:640-693` | stdout 部分已驗；WARN 三條 best-effort 對齊 |
| `stamp_meta`（meta.json 四鍵 {repo,head,stamped_at,tool}；幂等覆寫；`[OK] meta stamped：...`） | `scip_refs.py:696-722` | 寫回格式釘死：`stamped_at=datetime.now(UTC).isoformat(timespec="seconds")`（`+00:00` 尾綴非 `Z`）、`json.dumps(indent=2)+"\n"`、鍵序 repo/head/stamped_at/tool、tool 值=`"code_reality.scip_refs"`（:398,:714 同值） |
| 三表 schema（meta/occurrences〔seq PK 掃描序〕/symbol_tails）＋FN_TAIL_RE 入庫過濾＋`.tmp` 原子換入＋meta 三鍵（head/schema/tool） | `scip_refs.py:320-405` | rusqlite 同 DDL 同插入序；`os.replace`→`std::fs::rename`（同檔案系統原子性）；stats 行 `[OK] cache built：{path}（{n} symbols/{n} occurrences）` 位元組對齊 |
| 三守衛 `_stale_reason`（db mtime<index mtime／sidecar head 漂移／schema 版本不符／stat 失敗；**損壞 db 視同過期**——重建即治） | `scip_refs.py:422-449` | 同四訊號；查詢內自動重建走 stderr WARN（stdout 位元組不變） |
| 查詢面選擇 `open_face`（**無 db→protobuf 全量、不建 db**；fresh db 優先；過期→自動重建；**重建失敗→WARN＋回退 protobuf 面照常回答**——「衍生面是加速器不是依賴」） | `scip_refs.py:456-480` | 三分支全移植（含 :473-478 回退）；protobuf 缺依賴的 `[FAIL]` exit 2（:777-784）在 Rust 不存在（編譯期依賴） |
| 索引載入家族（**0 docs＝`[FAIL]` exit 2**；<100 docs＝WARN 存疑；解析失敗 `[FAIL]` exit 2） | `scip_refs.py:103-113` | 同門檻分流同文案 |

### 風險假設（本 EP 剩餘）

| 等級 | 假設 | 驗證 |
|------|------|------|
| 中 | `os.replace` 與 `std::fs::rename` 原子性等價（同檔案系統） | S3 崩潰注入測試（殘檔 `.tmp` 清理語意：Python `unlink(missing_ok=True)` 先清——:380） |
| 中 | 手寫述詞與 Python regex 在非 ASCII 符號邊界的行為差（Python `\w` Unicode；Rust `is_alphanumeric` Unicode——理論等價，非 ASCII 符號實務罕見） | S2 單元測試釘 Unicode 邊界案例 |
| 低 | 275MB 索引在 debug build 的解析時間（開發迭代體感） | S5 量測記錄；僅效能非正確性 |

## 段落劃分原則

- **依賴序**：S1（骨架）→ S2（lib 引擎）→ S3（lib sqlite 面，用 S2 的 parse）→ S4（CLI 薄層，組裝 S2/S3）→ S5（外部驗證）。S2/S3 同 crate 內模組邊界（`engine`／`cache`），S4 才碰 clap。
- **垂直切片**：S1 收＝workspace 存在＋Python 零干擾證明；S2-S4 各有 cargo 測試；S5 是 master R2 gate 的完整裁決。
- **POC 整合策略（定案）**：POC 是**規格與語意參照**（已驗行為），生產碼按 lib 分層重寫（POC 的 main.rs 單體不搬）；其 matcher/ln/tail/display 實作可直接對照翻譯。

---

## 段 1：Cargo workspace＋lib 骨架（吸收 master R1）

### Context
master AD-1 結構落地。root `Cargo.toml`：`[workspace] resolver="2" members=["crates/*"]`。`crates/code-reality`：lib（`src/lib.rs`＋空模組架）＋bin `code-reality`（clap，僅 `scip_refs` 子命令骨架）。Toolchain 檔：`rust-toolchain.toml`（釘 1.96 stable channel）、`rustfmt.toml`/`clippy.toml` 最小、`deny.toml`（cargo-deny 授權稽核——MIT 無 copyleft 鏈）。`.gitignore` += `/target`。

**lib API 形狀（master AD-2 前提，本段釘死）**：lib 函數回傳 `Result<ToolOutput, ToolError>`，`ToolOutput = { stdout: String, stderr: String, exit_code: i32 }`——lib 不 print、不 `std::process::exit`；bin 擁有打印與 exit（`std::process::exit(output.exit_code)`）。

依賴錨點：無（新地基）。語義約束：與全部段落共享「Python 原位凍結」；與 R7 共享「子命令名=模組名原樣」。**模組命名映射（master R1 的 domain/use-case/adapter 分層在此具體化）**：`engine`＝domain＋use case（述詞/歸屬/查詢編排）、`cache`＝adapter（sqlite/scip 面）、`cli`＝組裝層。**語言政策**：`crates/` 註解/docstrings/AGENTS.md 一律英文（repo policy）；**輸出字串維持中文原文＝parity 面**（Python 輸出即位元組規格，豁免）。

### Invariant Impact
無行為變更（骨架）；風險面＝Python 干擾（驗證對齊：381 tests 仍綠）。

### 核心實作要點
1. workspace 成員只有 `crates/code-reality`（未來段擴充）；`[workspace.package]`/`[workspace.dependencies]` 集中版本
2. lib 模組架：`engine`（S2）/`cache`（S3）/`cli`（S4 組裝層）空模組＋`ToolOutput`/`ToolError` 型別
3. deny.toml：allow MIT/Apache-2.0/BSD-3/ISC 等；`cargo deny check` 入驗收

### Pseudo Code
```rust
// crates/code-reality/src/lib.rs
pub struct ToolOutput { pub stdout: String, pub stderr: String, pub exit_code: i32 }
pub enum ToolError { Io(String), Decode(String) }  // bin 映射 exit 2
pub fn run(args: &[&str]) -> Result<ToolOutput, ToolError>;  // S4 實作——lib 唯一入口

// src/bin/code-reality/main.rs
fn main() { /* clap parse → code_reality::run → print stdout/stderr → exit_code */ }
```

### 驗證策略
`cargo build`/`cargo test`（骨架級）綠＋`cargo clippy --deny warnings` 綠＋`cargo deny check` 無新引入＋**`uv run pytest` 381 tests 仍綠**（零干擾）＋dogfood snapshot smoke 仍綠。

## 段 2：scip 引擎 lib——protobuf 面

### Context
POC 垂直切片的 lib 化。模組 `engine`：`load_index`（rust-protobuf 解碼＋空索引 WARN＋解析失敗 FAIL）、`matcher`（`Type.method` 三條件＋裸名，手寫述詞）、`fn_tail`（FN_TAIL_RE 述詞＋name 回捕）、`find_defs`/`find_refs`、`ln`/`loc_line`/`tail`、slot 解析＋meta.json 載入＋`git_head`、`source_line`。

依賴：S1。語義約束：與 S3 共享 `find_*` 原語（sqlite 面的語意複檢用同一述詞——Python 側「SQL 只縮候選、語義單一真相源」紀律的 Rust 對應：**述詞單一真相源**）。

### 核心實作要點
1. `matcher`/`fn_tail` 手寫述詞（POC 已驗形態）＋**邊界測試組**：`my_open`/`reopen`（word 邊界反例）、`KernelEventStore#open`（trait-decl 形態）、marker 子字串（`[EventStoreLifecycle]`）、Unicode 邊界（風險表中項）
2. `source_line`：stdout 部分（[SRC]）嚴格對齊；三 WARN（stamp 舊/repo 不符/drift）文案 best-effort 對齊走 stderr
3. 錯誤族：解析失敗/空索引/槽缺失/名解析失敗——全部 `[FAIL]`＋exit 2 路徑（經 `ToolOutput.exit_code`）

### 驗證策略
cargo 單元測試：述詞邊界組、`ln`（0-based→+1、`<2` 元素→-1→`:?`）、`tail`（空格分段 ≤4 段原字串）、slot 解析（`--repo .` resolve 塌縮防護）、meta.json 損壞→WARN+None；**git 失敗三形態**（逾時/git 不在 PATH/rev-parse 失敗→None→`[SRC]` 缺 `repo HEAD` 段——stdout 組成差異，必測）；**`[SRC]` 組成變體**（僅 index 段〔有 meta 無 `--repo`〕／僅 repo 段〔`--repo` 在但無 meta stamp〕／兩段／皆無〔無行〕）。**已知未覆蓋**：protobuf 巨集符號形態的 matcher 邊界（POC 單案例已驗；NT L4 於 S5 全覆）。

## 段 3：sqlite 面＋builder

### Context
模組 `cache`：三表寫入（DDL 直譯：`meta`/`occurrences`〔**seq 顯式 PK＝插入序＝掃描序——次序等價的基礎**〕/`symbol_tails`＋兩索引）、FN_TAIL_RE 入庫過濾（非函數符號不入庫）、`.tmp` 先清後寫＋`fs::rename` 原子換入、meta 三鍵（head=sidecar head 快照/schema=SCHEMA_VERSION/tool）、stats 計數；`stale_reason` 三守衛（mtime/sidecar-head 漂移/schema 版本；**損壞 db 視同過期**）；查詢面選擇三分支（`:456-480` 直譯）：**無 db→protobuf 全量、不建 db**；fresh db 優先→rusqlite 讀；過期→builder 重建＋stderr WARN＋續答；**重建失敗（IO/sqlite error）→stderr WARN＋回退 protobuf 面照常回答**（`:473-478`「壞了不該擋服務」——hard 約束的明示例外，stdout/exit 與 Python 一致）。

依賴：S2（parse/述詞/tail）。語義約束：**與 Python 雙向互通**（SM-13）：Python `--build-cache` 產的 db Rust 查詢面可讀且輸出等價；反向同——本段 DDL/插入序/meta 鍵即互通契約。**fn_defs 不在本次**（R3 獨立 sidecar——SM-13/R3 定案）。

### Invariant Impact
- 受影響 domain invariant：衍生 db 與索引的一致性（過期判定漏掉＝舊資料靜默回應——silent corruption 全下游）
- critical path：三守衛＋原子換入（崩潰殘檔不得讓後續 CREATE 失敗——Python `:380` 先清 `.tmp` 語意）
- 驗證對齊：崩潰注入測試（殘 `.tmp` 後重建成功）；三守衛逐一注入（touch mtime/改 meta.head/改 schema）→ 過期→重建→輸出不變

### 核心實作要點
1. DDL 逐字直譯（含註釋語意）；插入序＝documents→occurrences 掃描序（與 S2 `find_*` 同一迭代次序——兩面輸出等價的結構保證）
2. `[OK] cache built：{path}（{n} symbols/{n} occurrences）` stdout 位元組對齊（NT 治理面消費的潛在腳本風險——對齊免議）
3. 自動重建訊息走 stderr（`[WARN] 衍生 db 過期（{reason}）——自動重建`＋`[OK] 衍生 db 重建完成`）——stdout 位元組不變的硬約束（Python `:357` 條款）；重建失敗回退訊息 `[WARN] 衍生 db 重建失敗——本次查詢改走 protobuf 全量解析：{e}`（:475）同走 stderr
4. sqlite 面 defs 的**非 identifier 後退分支**（:516-523）：method 剝出非 `\w+` 時 `method=?` 超集失效→全候選掃描＋述詞複檢——SQL 只縮候選、語義單一真相源在述詞
5. build 失敗路徑：`[FAIL] 衍生 db 構建失敗：{db_path}：{e}` exit 2（:412-414）——**明示 `--build-cache` 模式的失敗是 exit 2**（查詢內自動重建的失敗才回退——兩者語意不同）

### 驗證策略
cargo 測試：三守衛注入、崩潰殘檔、stats 計數（對 synthetic fixture 斷言 symbols/occurrences 數）、插入序等價（同 fixture 下 protobuf 面與 sqlite 面輸出相同）、**重建失敗回退**（注入 build 失敗→答題照常＋單次解析重用——`:477-478` 性質）、**無 db 不建 db**（查詢後無新檔）。跨語言互通由 S5 裁決。

## 段 4：CLI 子命令 `scip_refs`

### Context
umbrella bin 組裝層。argparse 介面逐條對齊（依賴錨點表）：位置參數 `query`、`--index`/`--repo`/`--stamp-meta`/`--build-cache`；互斥族五條（`--build-cache` 與 stamp/audit/query；`--stamp-meta` 與 audit/query；stamp 需 `--repo`；audit 與 query 互斥——**audit 條款本次不掛**：clap 不認 `--audit`〔R4 補〕，互斥族中 audit 相關兩條隨之省略）；slot 缺失＋legacy 全局 slot 搬遷提示文案（:802-819）；無 protobuf 依賴降級路徑**不存在**（Rust 編譯期依賴）。

依賴：S2/S3。語義約束：bin 零邏輯（組裝＋打印＋exit）；`--help` exit 0（存在性述語契約）。

### 核心實作要點
1. clap 對齊：flag 名/help 意圖、預設值語意（`--index` 省略→slot 解析）；互斥以顯式檢查（錯誤文案對齊 `[FAIL] ...互斥`；**評估序照 Python**：build-cache 組→stamp 組→audit 組〔後者 R4〕）；**宣告差異：clap 不支援 argparse 長選項縮寫**（`--ind`→`--index`）——共存期無消費端用縮寫，接受
2. **檢查/路由全序釘死**（Python `:764-831`）：互斥族→stamp 需 `--repo`→index 解析（slot）→**index 存在性檢查（含 legacy 搬遷提示）→模式路由**（stamp→build-cache→query）——存在性在路由前（stamp 不存在的 index＝exit 2 非成功）
3. **最後防線**：無 query（**含空字串**——Python 互斥檢 `is not None` 但此處檢真值，`""` 會落到這裡）→`[FAIL] 需提供查詢或 --audit` exit 2——**文案原樣保留**（提 `--audit` 的暫時性位元組分歧明示接受：R4 補 flag 前共存期無人觸發〔SM-11〕）
4. `--stamp-meta`：meta.json 冪等覆寫＋`[OK] meta stamped：{sidecar}（{repo_name} @ {short}）`；兩條 exit 2 路徑（`[FAIL] 取不到 repo HEAD——meta 未 stamp` :706-708；`[FAIL] sidecar 寫入失敗` :718-720）
5. 模式路由：全部經 `lib::run` 回 `ToolOutput`

### 驗證策略
cargo 測試：互斥族逐一、缺參錯誤族、`--help` exit 0。CLI 位元組面由 S5 harness 裁決（單體測試只斷路由與 exit）。

## 段 5：parity harness＋收尾

### Context
master AD-5 落地：`tests/parity/test_scip_refs_parity.py`（pytest 編排，`integration` marker 外的新 marker `parity`——本地 NT 索引相依）。雙發執行：`uv run python -m code_reality.scip_refs ...` vs `cargo run --release -p code-reality -- scip_refs ...`（cwd 差異吸收），`cmp` stdout＋exit codes。

### 核心實作要點
1. **共用 synthetic fixture**：Python 端 `tests/parity/make_fixture.py` 用 `scip_pb2` 寫小 .scip——覆蓋面**逐項對齊 `rich_index()` 底稿**（`test_scip_refs.py:749-751`：三符號形態、邊界拒獨〔`my_open`〕、ref-only、非函數形態、空 range `?` 行、>6 refs 截斷、跨檔排序、dash 邊界）；產物 .scip **commit 進 repo**（`tests/parity/fixtures/`）——cargo 測試讀同檔，不依賴 Python 環境
2. **案例組（fixture 上，含所有變異演練）**：query `Type.method`（多符號塊）/裸名/無 DEF（exit 1 對齊）/corrupt index（壞 bytes→兩端 exit 2＋stdout 空）/stamp-meta 冪等（跑兩次輸出等價）/build-cache stats 行/**跨語言 db 互通雙向**（Python 建→Rust 查；Rust 建→Python 查）/自動重建（touch index mtime→兩端各自重建→輸出等價）/SM-3 drift（meta head 暫改→stderr WARN＋stdout 不變）/`--repo` 在但無 meta→`[SRC]` 僅 repo 段/**所有變異演練（touch/暫改/corrupt）一律打 fixture 或 sidecar 副本——絕不碰 NT 活體 slot**（master AD-5 只授權 NT 索引作查詢輸入；變異生產 sidecar＝frozen home 汙染＋與 NT 夜跑競態）
3. **NT L4（真索引，唯讀）**：`EventStoreLifecycle.open`（Type.method，18 refs）＋**裸名 `default_backend_opener`（1 ref，`kernel.rs:129`——已驗基準，研究報告 §2.1 樣本）**兩案例 cmp＋exit codes；SM-4 抽跑（無 slot repo→exit 2＋stdout 空）
4. **pytest 註冊**：pyproject `[tool.pytest.ini_options]` markers += `parity`（pyproject 不在凍結範圍）；NT 相依案例 skip-when-absent（tests/AGENTS.md 慣例）
5. 收尾：master kanban 卡 In-Progress/Done 流轉＋master R1 段標注「absorbed by 本 EP」**＋master R1 的 `ep-rust-r1-workspace.md` 子 EP 指針改指本 EP**＋POC 檔案清除（`poc/r2-byte-identical`、`poc/rust-deps-probe`——行為已被 parity harness 承接）＋root AGENTS.md Capabilities UC-1 行註記 Rust 載體

### 驗證策略（＝master R2 gate 裁決）
全部 parity 案例 cmp stdout＋exit codes 通過；`uv run pytest`（既有 381＋新 parity）綠＋`cargo test` 綠。**已知未覆蓋**：audit 面（R4）；`--help` 位元組（僅 exit 0 存在性述語）；stderr 文案 diff（非契約面，可選報告性比對）。

---

## 整合策略

- **跨段整合點**：S2 述詞＝S3 入庫過濾與語意複檢共用；S3 builder＝S4 自動重建路徑；S5 裁決 S2-S4 全體。
- **baseline**: `2eafd8a`。
- **回退路徑**：全程 additive（crates/ 新增、Python 零改動）——任何段失敗即停，master SM-11/12（消費端零感知）。
- **git**：每段一 commit（user consent gate）。

## Ask First

1. 所有 git commit（consent 規則）
2. 無新增（master 的 R7/hazard/契約演化 gate 不觸及本 EP 範圍）

## 收尾步驟

1. Capabilities：root AGENTS.md UC-1 行「載體：Rust（`code-reality scip_refs`）＋Python 凍結」註記
2. Kanban：master EP 追蹤卡更新（R1✅absorbed/R2✅）；`rust-caller-edges` 卡仍 Backlog（R3）
3. instruction 檔：`crates/AGENTS.md` 建立（lib 分層導航——engine/cache/cli＋ToolOutput 契約）；root AGENTS.md Module guide 補 crates 條目
4. POC 清除（parity harness 已承接行為）
5. /audit-test 對 parity harness＋cargo 測試組跑品質稽核（vacuous 檢查——cmp 型測試是否真的兩端都跑）

## EP Review Record

2026-08-25 三軌獨立審查（完整性事實核對／結構合規／驗收覆蓋——fresh eyes Explore agents，全部 findings 已 judge 全數採納修入上文）。摘要：

| 軌 | 關鍵 findings（已修入上文） |
|----|------|
| 完整性 | 🔴P2 `open_face` 重建失敗→protobuf 回退（`:470-478`）未入規格且與 crash-only 條款衝突——照字面實作會 exit 2＝位元組契約分歧（三軌獨立會師）→ S3 補三分支＋硬約束明示例外＋cargo/S5 測試；P3：無 db 不建 db（:464-465）、最後防線空字串 query 真值檢查（:825-827）、main 檢查全序（存在性在路由前）、stamp 格式釘死（timespec/`+00:00`/indent=2/tool 值）、錨點修（:422-449/:456-480/0-docs FAIL vs <100 WARN）、stdout 組成分支（git fail 缺段/僅 repo 段/build FAIL exit 2/stamp 兩 exit 2）、SqliteFace 非 identifier 後退＋argparse 縮寫差異 |
| 結構合規 | 🔴P2 S5 變異演練未釘作用域——touch mtime/暫改 meta head 可能打 **NT 活體 sidecar**（frozen home 汙染＋與 NT 夜跑競態；master AD-5 只授權唯讀查詢輸入）→ 全部變異釘 fixture/副本、NT L4 唯讀；P3：凍結措辭自相矛盾（tests/parity 授權明示）、`parity` marker 註冊步驟＋skip-when-absent、R1 吸收兩未宣告差異（模組命名映射＋master R1 stale 子 EP 指針）、無參錯誤文案含 `--audit` 的暫時分歧明示、模板缺口（SM 薄映射＋stub）、英文政策釘死（crates 註解英文；中文輸出字串＝parity 面豁免） |
| 驗收 | 🔴P2 第二 NT 案例未釘死（空虛通過風險）→ 釘 `default_backend_opener`（1 ref 基準已驗）；P3：make_fixture 覆蓋對齊 `rich_index()` 底稿（非縮水版）、git-fail 測試組落點、corrupt-index/sqlite-error/empty-query 案、cargo fixture 供給（commit 產物）、S5 缺索引案 wording（文案屬 stderr 非 cmp 面） |

**不採納 0 項**。

## Build Agent Review Record（2026-08-25 /implement 階段 4）

三視角獨立審查（clean／UC-anchored／Correctness——Correctness 軸含實證對抗探測）。**P1×1＋P2×6 全數採納修入＋回歸釘住**；P3 修 12 項、3 項記錄性豁免。Loop 一輪收斂。

| 視角 | 關鍵 findings（已修） |
|------|---------------------|
| clean | P1 `--repo .` stamp 模式 `file_name().unwrap()` panic（Python 答 exit 0）→ lossy+default；P2 engine 錯誤字串預帶 `[FAIL]` 造成雙前綴 → 裸訊息＋邊界統一上標；P2 靜默空 helper → Result 傳播（見 Correctness 同項）；P3：死碼（ToolError/load_stderr/Stats 中繼）移除、`*` 版號釘下限、cargo-deny 自家 crate 補 `license="MIT"` |
| UC-anchored | P2 同 stamp panic；P2 同靜默空 helper；P3：`idx_sha=""` truthiness（Python falsy→未 stamp WARN 分支）、`expand_home` HOME unwrap、`default_index_path("")`、`stamped_at` 型別強制、meta_path/sqlite_path `file_name()` panic 面、`modified()` fail-open→stat 失敗 |
| Correctness | P2 `short()` 位元組切片 panic（短/多位元組 head）→ `chars().take()`；P2 `stamped_at` 型別強制差異 → `py_str_coerced`（None/True/False/數字）；P2 dash 前綴 positional（`-`/`--` 後）被丟 → argparse 模擬；P2 clap 邊界四型（負數 positional/縮寫/重複 flag/額外 positional）→ **拆除 clap：raw-argv 直通＋lib 端 argparse 模擬**（同時消滅雙 parse shim）；P3 `$` 行尾 `\n` 語意、stamp payload JSON 逃逸（serde pretty）、fresh-path `open_ro` 失敗→protobuf 回退 |

**回歸釘住**：parity harness 增 `TestArgparseEdge` 五案例（`-`/`-5`/`-- -weird`/`--ind` 縮寫/額外 positional）——23/23 全綠。

**記錄性偏差（文件化，不修）**：①`--help` 位元組與 argparse 文案不同（契約僅存在性述語 exit 0）；②`git_head` 無 30s timeout（std 無機制；rev-parse 實務即時）——三 WARN 文案已補；③stamp payload 檔案位元組與 Python `ensure_ascii` 不同（非 gate 面，JSON 合法可互通）。**Post-build 補注（2026-08-25 dual-context 收尾鏈）**：fresh-eyes＋primed 雙審查者 18 findings（17 採納修入＋1 記錄豁免），衝突項（`-5.5.5` 負數判定）以本機 oracle 實測裁決＝**Python 3.14 prefix matcher**（`-5.`/`-5x`/`-5.5.5` 皆 positional、`-.` 非）——已按實測語意編碼並以 parity 案例釘住（總數 29）。其餘修正：option-like flag 值拒絕、`--h` 縮寫 help、bool flag `=value` 拒絕、缺席 `stamped_at` 鍵→無日期段（顯式 null→`（None）`）、重建路徑 WARN 保全、build_db 單一交易（NT 規模效能）、stamp 鍵序 preserve_order、互動測試 stderr 斷言（schema 分歧不被 fallback 遮蔽）、`Parsed` enum 重構。

**行為偏差（明示）**：corrupt-but-meta-intact db／fresh-path open_ro 失敗——Python uncaught traceback exit 1，Rust WARN＋protobuf 回覆正確答案（嚴格更好，共存期無消費端觸碰，R4/R7 前維持）。
