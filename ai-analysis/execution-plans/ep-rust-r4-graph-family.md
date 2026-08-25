# EP：R4 graph 家族 Rust 化（foundation＋graph 家族＋scip_refs --audit）

> **ep_type**: implementation
> **parent**: [ep-rust-migration.md](ep-rust-migration.md)（段 R4 **兩段式之①**；繼承 AD-1〜AD-5／雙凍結紀律）
> **分段宣告（master sizing 條款）**：**hub_refs×hazard 整體歸②**——hub_refs 的 callers 方向恆觸發 hazard stage（`hub_refs.py:352`），兩者不可分割交付；hazard 的 AST 差動測試負擔獨立成段。②另衍生 `ep-rust-r4b-hazard-hubrefs.md`（本 EP 不含 hub_refs/hazard 規則引擎任何實作；**profile 的 `hazard_registry` 鍵解析屬①地基面**——S1 必須建模，否則 Python 合法 profile 在 Rust 端 unknown-key crash，見 F7）。master R4 驗收中的「mosaic `hub_refs --hazard` 輸出 cmp＋hazard 差動測試」隨②結案。
> **spec 鏈**: 凍結 Python 即規格（`code_reality/{common,profile,exclusions,snapshot,transition,graph_audit,graph_csv}.py`——逐檔錨點見段落 0）；既有 pytest 套（fd64449＝404 collected）為語意 oracle、`tests/parity/` 為跨語言位元組 oracle（AD-5）——**vacuous 防護／exit 家族／截斷／csv quoting 邊界／git-root assert 等面在既有套無 oracle**，Rust 端新增案例以凍結源碼直讀釘位元組（各段驗證策略標注）
> **parity 面宣告**: 本 EP 全部工具已有 Python 現役實作——**stdout 位元組＋exit codes 為 gate**（stderr 管理面不 gate）；工具間 exit 語意**不統一**（見 S1 決策 D3——逐工具釘死）
> baseline: `fd64449`（＝a88e392＋tests 自足化；crates/ 面兩者等價）

## 實作總覽

| 段 | 內容 | 對應 master |
|----|------|------------|
| S1 | 慣例層 foundation：profile（toml）＋common（EDGE_KINDS/connect_ro WAL/mtime 撕裂守衛/make_meta）＋exclusions＋時間地基 | R4 |
| S2 | `snapshot`：module-edge 匯出＋commit 錨定＋stale 三級＋sidecar JSON（indent=1） | R4 |
| S3 | `transition`：pair 集合差（B1 reversed added-direction）＋EP claims 對照＋md/json 雙輸出 | R4 |
| S4 | `graph_audit`（D1 狀態機＋D2 rust-analyzer 對帳＋`--json` NT 契約）＋`scip_refs --audit` 兩遍式（R2 移轉） | R4 |
| S5 | `graph_csv`：community 多數決＋Python csv quoting＋degree 不變量 | R4 |
| S6 | parity harness 擴充（synthetic crg db fixture 雙跑 cmp＋NT graph_audit 唯讀 gate＋dogfood snapshot smoke）＋收尾 | R4 gate（①部分） |

**繼承硬約束（load-bearing 內嵌）**：共存期既有 Python 檔案零改動；`code-reality` umbrella bin 子命令名＝模組名原樣（relay 契約）；lib 回 `ToolOutput` 不 print/exit（AD-2）；**工具 exit 語意逐工具對齊凍結 Python**（snapshot/transition/graph_csv 的 crash＝uncaught → exit 1＋stdout 空——Rust 以 stderr `[FAIL]` 取代 traceback（stderr 不 gate）、**exit code 必須 1**；graph_audit env 錯＝exit 2）；NT 活體唯讀（graph.db 只 `connect_ro`——immutable/mode=ro URI，絕不寫）。

## EP Review Findings

> 三軌審查 2026-08-25（spec 忠實度／架構一致性／完整度風險——獨立 read-only agent 各軌）。22 錨點＋接線宣稱全數查證吻合；下表為採納回寫項。兩項裁決：T3-8 選 rust-toolchain components 方案；T3-9 dogfood 降為實機手動步驟（gate 不依賴 gitignore 面）。

| ID | 嚴重度 | EP 段落 | 問題 | 處置 | 狀態 |
|----|--------|---------|------|------|------|
| T3-5 | 🔴 | S4/矩陣 | graph_audit `--all`/`--graph` 兩 flag 全 EP 零提及（漏 flag＝載體不完整；`--graph` 是 parity 自選 db 旋鈕） | S4 要點 4 補兩 flag＋R4-P 場景 | implemented |
| F1 | 🟡 | S1 | `ScanRoot.pyi` 型別錯：凍結面是 `pyi: str`（glob 路徑），pseudo code 寫 bool | 改 String＋註記 | implemented |
| T3-7 | 🟡 | S5/S6 | CSV 行尾 CRLF（excel dialect）未釘；CSV 檔案位元組不在 gate——`\n` 分歧會靜默通過 stdout cmp | S5 釘 CRLF＋S6 加 CSV 檔案 cmp | implemented |
| T1-1 | 🟡 | S3 | `extract_baseline` regex 漏 `\*\*baseline\*\*` 粗體錨（字面移植會多匹配） | S3 要點 2 補完整原文 | implemented |
| T1-2 | 🟡 | 段0 D4 | `--json` `missing` 迭代序（first-seen）未釘；most_common 條款在 R4① 零使用 | D4 重釘；most_common 移② | implemented |
| F2 | 🟡 | S2 | umbrella bin `main.rs` 只路由 scip_refs——四新子命令路由無段認領；usage/help 文未裁決 | S2 Context 認領＋裁決文案隨載體更新 | implemented |
| F3 | 🟡 | S4 | 檔面漏 `cache.rs`（audit_targets/missing_refs 兩 face）＋R2 依賴未宣告 | S4 Context 補宣告 | implemented |
| T3-1/2/3/4 | 🟡 | S2-S5 | 四處「直譯」宣稱實無 oracle：git-root assert（被 monkeypatch）、vacuous/exit 家族、quoting 邊界、截斷 | 各段改「新增案例＋凍結源碼直讀釘位元組」 | implemented |
| T3-6 | 🟡 | D3/S6 | argparse 進入面未述（缺參 exit 2／`-h` stdout usage） | D3 補面＋S6 cmp 案例 | implemented |
| T3-8 | 🟡 | S6 | rust-analyzer skip-when-absent 與零 skip 政策相斥 | **裁決**：rust-toolchain.toml 加 `components=["rust-analyzer"]` | implemented |
| T3-9 | 🟡 | S6 | dogfood 依賴本 repo graph.db（gitignore 面）卻列 gate | **裁決**：dogfood 降實機手動步驟；gate＝synthetic＋互通 | implemented |
| T1-3/T3-15 | 🟡 | spec 鏈 | 「422 測試」不可重現（a88e392＝416 defs／fd64449＝404 collected） | 改 404@fd64449 口徑 | implemented |
| F6 | 🟡 | D3 | make_meta git 失敗 exit-1 映射未釘（`ToolOutput::fail` 固定 2） | D3 補充面① | implemented |
| F4/T1-5 | 🟡 | D2 | 原函式不可動措辭＋timespec='auto' 零微秒邊界 | D2 重寫 | implemented |
| F5 | 🟡 | R4-N | assert 面過度宣稱（indent/鍵序非 assert 面） | R4-N 措辭修正 | implemented |
| F8 | 🟡 | D7 | in-process 後 600s 總逾時與 stdout 空/非 JSON 分支消失 | D7 記錄為已接受偏差 | implemented |
| T1-4/T1-6/T3-10/11/12/13/14 | ℹ️ | 段0-S6 | claims_re 空分支 sentinel、DEFAULT_EXCLUDE 定義端漂移、mosaic 殘留句、S1 測試清單補 hazard/scan_roots/bool-depth、glob 風險行、無 communities 案、audit guard 順序 | 對應段落逐項回寫 | implemented |

## UC 盤點

### Backlog 關聯
- 無既有卡（`rust-mcp-server.md` 屬 R6）——本 EP 產出自動建卡 `rust-graph-family.md`（含①②分段註記）

### SYSTEM-MAP 影響
無 SYSTEM-MAP.md（同 R2/R3 慣例——master EP 承載狀態面）。

### 掃描範圍
root AGENTS.md Capabilities（UC-4 完整度治理／UC-6 hub_refs——本 EP 更新 UC-4 載體註記；UC-6 行隨②）、`crates/AGENTS.md`、`.kanban/`。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| UC-4 完整度治理（audit＋`[SRC]`） | ✅（Python） | root AGENTS.md | 載體換軌 | 本 EP 交付 Rust 載體（`--audit` 兩遍式＋graph_audit） |
| snapshot/transition/graph_csv 敘事工具 | ✅（Python） | root AGENTS.md 邊界/敘事行 | 載體換軌 | 本 EP |
| UC-6 hub_refs＋hazard | ✅（Python） | root AGENTS.md | 不動 | ②子 EP |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| UC-4/graph 家族 Rust 載體 | 🟡 | `crates/code-reality`（`profile.rs`/`common.rs`/`snapshot.rs`/`transition.rs`/`graph_audit.rs`/`graph_csv.rs`＋cli 子命令） |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| R4-A | snapshot 正常 | `snapshot --repo <repo>` | `[OK] snapshot: N files, M module edges -> path`＋`[LOG]` | 無 | UC-4 |
| R4-B | graph stale | graph.db 落後 HEAD | `[WARN] CRG graph stale: ...`（stdout）照常輸出 | 無 | UC-4 |
| R4-C | 非 CRG db／db 缺 | graph.db 不在／壞 | crash：exit 1、stdout 空、stderr `[FAIL] ...先跑 uvx code-review-graph build` | 無 | UC-4 |
| R4-D | snapshot 空集合 | 0 邊 | `[WARN] snapshot 空集合（0 files，db raw N 邊）...` | 無 | UC-4 |
| R4-E | transition 邊反轉 | (X,Y)→(Y,X) | reversed＝added 方向（B1）；added/removed 照常含完整三元組 | 無 | UC-4 |
| R4-F | transition 無變化 | 同 snapshot | `## 無結構變化` | 無 | UC-4 |
| R4-G | claims 對照 | `--ep` 有/無 profile | 無 profile→`[WARN] claims 恆 NONE`；EP 缺檔→crash exit 1 | 無 | UC-4 |
| R4-H | graph_audit 缺差 | DB 少於 rust-analyzer | exit 1＋`[WARN] DB 缺差 N 項...`（或 `--json`） | 無 | UC-4 |
| R4-I | graph_audit 環境錯 | rust-analyzer 未裝／db 缺／全零符號 | `[FAIL]` stderr＋**exit 2**（假乾淨防護：`audited and total_ra==0`） | 無 | UC-4 |
| R4-J | graph_audit `--json` | NT 治理鉤子 | 四鍵（risk_files/audited_files/missing/errors）、`ensure_ascii=False, indent=1`、**stdout**——位元組 gate | 無 | UC-4 |
| R4-K | scip_refs --audit | `--audit --repo` | 兩遍式：Rust graph_audit `--json`（in-process lib 呼叫）→ missing → SCIP refs 對照輸出 | 無 | UC-4 |
| R4-L | csv 匯出 | `graph_csv --repo` | `graph-nodes.csv`＋`graph-links.csv`（Python csv quoting；Σdegree==2×links 不變量） | 無 | UC-4 |
| R4-M | WAL 語意 | `-wal` 在/不在 | 無 `-wal`→immutable=1；有→mode=ro（fail→crash 帶修法指引）；讀取前後 mtime 撕裂守衛 | 無 | UC-4 |
| R4-N | 跨語言 artifact 互通 | Rust snapshot → Python transition | `load_snapshot` assert 全過（三元組／`_meta` 結構——indent/鍵序**非** assert 面，歸 S6 雙跑 cmp） | 無 | UC-4 |
| R4-O | hub_refs/hazard | — | **不在本 EP**（②子 EP） | — | UC-6 |
| R4-P | graph_audit scope/override | `--all`／`--graph <db>` | `--all`＝全部 .rs 對帳（vs 風險檔 scope，`graph_audit.py:180`）；`--graph` 覆寫 db 路徑（`:239`——parity 測試自選 db 旋鈕） | 無 | UC-4 |

## 段落 0：全域研究摘要（錨點＋設計決策）

### 凍結 Python 規格錨點（定義端；Rust 為新消費端）

| 錨點 | 定義端 | 移植語意 |
|------|--------|---------|
| `EDGE_KINDS`＝("IMPORTS_FROM","CALLS","INHERITS") | `common.py:18` | 結構邊三 kind 白名單（transition「無結構變化」結論邊界） |
| `anchor_pattern` | `common.py:21-37` | `^[ \t]*`＋escaped line＋`[ \t]*$`（`[ \t]` 非 `\s`——防空行起點） |
| `repo_relative`（resolve 只做 root） | `common.py:40-45` | repo 外→None；symlink 檔案路徑不正規化（行為凍結） |
| `connect_ro` WAL 語意 | `common.py:53-74` | 見 R4-M；rusqlite URI `file:...?immutable=1` / `?mode=ro` |
| `db_mtime_ns`/`assert_db_unchanged` | `common.py:77-90` | ns 精度 mtime 前後比對（撕裂讀兜底）；訊息含「撕裂」 |
| `make_meta` 鍵序 | `common.py:93-115` | repo/commit/created_at(微秒 UTC +00:00)/tool/`**extra`——**插入序＝JSON 序**（byte-parity） |
| `load_profile` crash-only 清單 | `profile.py:53-117` | 缺檔→None；壞 TOML/未知鍵/缺必填/前綴無斜線/depth 非≥1 整數（排除 bool）→assert crash |
| `module_of`（F6 根檔案歸 prefix） | `profile.py:120-138` | 有序首中；prefix 下第 depth 層；段落含 `.`→base |
| `claims_re` | `profile.py:141-153` | 無規則→永不命中；否則 alternation＋`[a-z_0-9]+`（regex crate 可表達，無 look-around）。**空規則分支的 `(?!x)x` 本身含 lookahead——禁字面直譯，以語意等價 sentinel 實作**（永不命中的常數 pattern） |
| `DEFAULT_EXCLUDE`/`is_excluded` | `profile.py:18`／`exclusions.py:13-16` | 目錄粒度前綴（帶斜線）；單一入口禁副本（DEFAULT_EXCLUDE 定義端在 profile.py，exclusions.py:10 只 import） |
| `export_module_edges` | `snapshot.py:53-86` | files＝參與邊的檔案（同模組兩端仍計入）；`src_mod != dst_mod`；sorted() Python 字串序 |
| `detect_stale` 三級 | `snapshot.py:110-141` | sha 直比→`last_updated`（**naive local tz 假設**）→db_mtime |
| `Snapshot.write` | `snapshot.py:196-211` | 檔名 `<repo>-<sha8>.json`（**8** 與 scip `[SRC]` 的 7 不同源）；`json.dumps(indent=1)` |
| `diff_edges`（B1） | `transition.py:72-86` | tuple 集合差；reversed＝added 方向（pair 投影） |
| `render_report`/`render_json` | `transition.py:155-271` | md 結構（截斷 20＋`- ... +N more`）；json 鍵集＋indent=1 |
| D1 狀態機＋regex | `graph_audit.py:55-118` | per-block 去重計數；impl 閉合＝同縮排 `}`；IMPL_RE/FN_RE 直譯（regex crate） |
| D2 `ra_symbols`＋雙層 vacuous 防護 | `graph_audit.py:134-161` | rust-analyzer `symbols` stdin bytes、`check=False`、逾時 None；`file_path` 用 **resolved 絕對路徑** |
| `--json` 四鍵＋exit 家族 | `graph_audit.py:216-297` | R4-J/I；`json.dumps(ensure_ascii=False, indent=1)` 印 stdout |
| audit 兩遍式 | `scip_refs.py:254-316` | R4-K；**雙鍵歸屬**（定義檔, 方法名）——單鍵 216→138 假陽性實證；`repo.resolve()` 正規化 |
| community 多數決 tie-break | `graph_csv.py:70-71` | `(-count, id)` 最小者；`"+".join(sorted(kinds))` |
| csv quoting | `graph_csv.py:136-163` | Python csv QUOTE_MINIMAL；表頭兩行固定 |
| `make_crg_db` fixture schema | `tests/fixtures/crg_db.py:13-121` | Rust 測試直接開同 schema synthetic db（SQL DDL 直譯） |

### 關鍵設計決策（本 EP 定案）

- **D1（JSON 位元組面）**：`serde_json` `preserve_order`（workspace 已啟用）＋**自訂 serializer 層**：`PrettyFormatter::with_indent(b" ")` 對齊 `indent=1`；非 ASCII 不跳脫＝`ensure_ascii=False`（graph_audit/hub_refs 面）；snapshot/transition 的**檔案**輸出（非 stdout gate）同一 serializer——與 Python `ensure_ascii=True` 的跳脫差屬非 gate 面（JSON 語意等價、Python `load_snapshot` 可消費），記錄為已知偏差
- **D2（時間地基）**：`created_at`＝UTC 微秒——**新增 `utc_now_iso_micros()`，原 `utc_now_iso` 不動**（R2 stamp parity 釘 `timespec="seconds"`，改原函式＝破 R2 面）；重現 `isoformat()` 預設 `timespec='auto'`（microsecond==0 時省略小數段——1e-6 機率邊界，記錄為已知偏差面）；`detect_stale` 的 naive-local-tz 語意＝**S1 POC 裁決**（選項：`jiff` crate vs libc `localtime_r` vs shell `date`——位元組面只影響 stale 判定非輸出格式，中風險）
- **D3（exit 語意逐工具）**：snapshot/transition/graph_csv 的 Python crash＝uncaught `AssertionError`→**exit 1、stdout 空**；graph_audit env 錯＝顯式 **exit 2**；scip_refs 家族 0/1/2（R2 既有）。Rust crash 路徑統一 `ToolOutput{stdout:"", stderr:"[FAIL]...", exit_code: 1|2}`——**stdout 空＋exit 對齊**是 gate，stderr 文案 best-effort。**兩個補充面**：① `make_meta` 的 git 失敗（Python `check=True` uncaught）→ **exit 1 手動構造**——`ToolOutput::fail` 固定 exit 2 不可直接用；且 R2 `git_head` 的 Err 是容忍式 [SRC] 文案，複用時須改映射；② **argparse 進入面**：缺參/壞參→exit 2＋stdout 空（stderr usage 不 gate）；`-h`→**stdout** usage＋exit 0——四新子命令 usage 文案逐字移植（R2 `cli.rs` usage 逐字先例），`-h` 輸出列入 S6 cmp 案例
- **D4（排序語意釘死）**：`sorted()`＝Python 字串序——Rust 以 byte 序實作並**記錄非 ASCII 邊界**（BMP 內重合；emoji/surrogate 可分歧——repo 路徑實務 ASCII）；`graph_audit --json` 的 `missing` **陣列迭代序**＝first-seen（scope 檔序 sorted × 每檔 RA 輸出首次出現序＝Counter 插入序——Rust 端 HashMap/BTreeMap 迭代即破位元組，須保存插入序，如 IndexMap 或 Vec 計數）。`Counter.most_common` tie 條款**移除**（R4① 七檔零使用——②hazard 才可能消費，屆時再釘）
- **D5（rg/rust-analyzer/uvx 外部程式）**：全部 shell out 原參數（`rg -n --no-heading -t py -t yaml -t json -t toml {args} . -g !...`——**cwd=root＋路徑 `.`** 是實證產物〔絕對路徑 arg 讓 `-g` 排除靜默失效〕；`rust-analyzer symbols` stdin bytes；②才有 uvx）。**勿以 crate 重寫**（輸出行格式/exit 1 語意/type 選集皆位元組面）
- **D6（新依賴）**：`toml`（profile 載入）＋D2 裁決的時間 crate（若 jiff）——`cargo deny` 驗授權
- **D7（--audit in-process）**：Rust `scip_refs --audit` 的第一遍**直接 lib 呼叫 graph_audit 函數**（非 subprocess——Python 的 subprocess 形態是其凍結實作細節；Rust 載體內部組裝自由，stdout 對齊即可）。`repo.resolve()` 正規化條款照搬。**已接受偏差（隨 subprocess 邊界消失）**：Python audit_mode 的 600s 總預算逾時→FAIL exit 2 與 `proc.stdout` 空/非 JSON 的 FAIL exit 2 分支——in-process 無此邊界（env 錯以 lib Err→exit 2 映射保留；僅剩 per-file `ra_symbols` 60s 逾時）

### 風險假設（本 EP 剩餘）

| 等級 | 假設 | 驗證 |
|------|------|------|
| 中 | serde_json `PrettyFormatter::with_indent(b" ")` 與 Python `json.dumps(indent=1)` 位元組等價（分隔符/巢狀/空物件邊界） | S4 單元測試逐案例（空 list/dict、巢狀、非 ASCII、跳脫字元）＋S6 雙跑 cmp |
| 中 | detect_stale naive-local-tz 語意重現（D2） | S1 POC（三選項實測 `astimezone()` 等價性） |
| 中 | rust-analyzer `symbols` debug 輸出格式耦合（RA_LABEL_RE/RA_KIND_RE）——版本升級可漂 | S4：解析器對 Python 端同輸出逐字對齊；格式漂移→fail loud（雙層 vacuous 防護照搬） |
| 中 | Python `sorted()`×Rust byte 序、`missing` 迭代序（D4） | 單元測試釘；非 ASCII 記錄為已知邊界 |
| 低 | WAL/immutable URI 語意 in rusqlite | S1 cargo 測試（crg_db fixture＋觸發 WAL 檔存在/不存在兩分支） |
| 低 | toml crate 對 `[tool]` 段/[[array]] 解析對齊 Python `tomllib` | S1 單元測試（合法/非法 profile 檔案例族＝既有 test_profile 斷言直譯） |
| 低 | Python `Path.glob`/`rglob` vs Rust glob crate 語意（隱藏檔/`**` 遍歷/symlink/不可讀目錄）——scan_files 兩分支依賴 | S4 單元測試（glob crate `require_literal_leading_dot=false` 對齊 pathlib 行為；sorted 後 dedup 對齊 `graph_audit.py:74-79`） |

## 段落劃分原則

- **依賴序**：S1（地基：profile/common/exclusions＋時間）→ S2（snapshot）→ S3（transition——消費 snapshot 格式）→ S4（graph_audit＋scip --audit——D2 subprocess 與 R2 cli 擴充）→ S5（graph_csv——獨立最小）→ S6（外部驗收）。S5 可與 S4 平行（同 S1 依賴）。
- **垂直切片**：每段收＝該工具 cargo 單元測試（synthetic crg db）＋Python 對應既有測試形態移植；S6 裁決 master R4 gate ①部分。

---

## 段 1：慣例層 foundation

### Context
新模組 `profile.rs`＋`common.rs`（adapter/infra 層）。UC 引用：UC-4 載體換軌的地基。依賴：R2 `engine::git_head`（make_meta 用——Ok＝完整 HEAD sha；Err 是容忍式 [SRC] 文案，make_meta 消費時按 D3 補充面① 改映射 exit 1）。語義約束：**與 S2-S5 共享**：EDGE_KINDS、`module_of`、`is_excluded`、`connect_ro`、時間格式、`ToolOutput` exit 語意（D3）；與②共享 profile/hazard_registries 結構（`HazardRegistry` 四欄位照搬——②消費）。

### 核心實作要點
1. `common.rs`：`EDGE_KINDS`、`anchor_pattern`（regex crate：escape 用 `regex::escape`——語意對齊 Python `re.escape` 的**輸出 pattern 行為**而非字面轉義集）、`repo_relative`（resolve 只做 root）、`graph_db_path`、`connect_ro`（URI 分支＋錯誤訊息含「先 `uvx code-review-graph status` 或 build 後重跑」）、`db_mtime_ns`/`assert_db_unchanged`（訊息含「撕裂」）、`make_meta`（鍵序＋git rev-parse——git 失敗→exit 1 手動構造，見 D3 補充面①）
2. `profile.rs`：`ModuleRule/ScanRoot/HazardRegistry/Profile` struct；`load_profile` crash-only 清單逐條（含 depth 排除 bool——toml 的 integer/boolean 型別分支）；`module_of`（F6）；`claims_re` 編譯；`scan_roots`
3. 時間地基：`utc_now_iso_micros()`；D2 POC（local tz）——`detect_stale_fallback_parse(s) -> Option<i64 epoch>` 介面先行，S2 消費

### Invariant Impact
無行為面（新碼）；風險＝parse 語意漂移（claims_re/module_of）——驗證對齊：S1 單元測試組直譯 `test_profile.py`/`test_common.py` 既有斷言。

### Pseudo Code
```rust
// common.rs
pub const EDGE_KINDS: [&str; 3] = ["IMPORTS_FROM", "CALLS", "INHERITS"];
pub fn anchor_pattern(line: &str) -> String;          // ^[ \t]*{escaped}[ \t]*$
pub fn repo_relative(path: &Path, root: &Path) -> Option<String>;
pub fn graph_db_path(repo_root: &Path) -> PathBuf;
pub fn connect_ro(db: &Path) -> Result<Connection, String>;  // immutable/mode=ro URI
pub fn db_mtime_ns(p: &Path) -> Result<i64, String>;
pub fn assert_db_unchanged(db: &Path, before: i64) -> Result<(), String>;  // 「撕裂」
pub fn make_meta(tool: &str, repo_root: &Path, commit: Option<&str>, extra: Vec<(&str, String)>)
    -> Result<Vec<(String, String)> /*ordered*/, String>;

// profile.rs
pub struct ModuleRule { pub prefix: String, pub depth: i64 }
pub struct ScanRoot { pub path: String, pub pyi: String }  // pyi＝.pyi 合約樹 glob（凍結面是 str 非 bool——profile.py:30）
pub struct HazardRegistry { pub package_prefix: String, pub suffix: String,
                            pub register_fn: String, pub registry: String, pub evidence: String }
pub struct Profile { pub modules: Vec<ModuleRule>, pub exclude: Vec<String>,
                     pub scan_roots: Vec<ScanRoot>, pub hazard_registries: Vec<HazardRegistry> }
pub fn load_profile(repo_root: &Path) -> Result<Option<Profile>, String>;  // crash-only 訊息
pub fn module_of(rel: &str, p: &Profile) -> String;
pub fn claims_regex(p: &Profile) -> regex::Regex;       // 無規則→永不命中
pub fn is_excluded(rel: &str, p: Option<&Profile>) -> bool;
```

### 驗證策略
cargo 單元測試：`make_crg_db` DDL 直譯 helper（Rust 端建 synthetic db——S2-S5 共用）；`test_common.py`/`test_profile.py` 斷言直譯（unknown-key 訊息、prefix 無斜線、depth 非整數〔既有套僅 float 案例——**補 bool 一枚**，bool 排除邏輯在 `profile.py:101-105`〕、module_of F6、claims_re 空 profile 永不命中〔空分支 sentinel——見錨點表〕、connect_ro 兩分支〔有無 `-wal` 檔〕、撕裂守衛訊息、`TestHazardRegistry` 5 案＋scan_roots 兩態——struct/鍵解析屬 S1 移植面，hazard 語意測試歸②）；時間 POC（D2）：三選項對 `astimezone()` 等價實測記錄。已知未覆蓋：真 WAL writer 並發（/live 環境——撕裂守衛語意由 mtime 測試代表）。

## 段 2：snapshot

### Context
`snapshot.rs`。依賴 S1；**首個新子命令——本段認領 `src/bin/code-reality/main.rs` umbrella 路由擴充**（現只路由 scip_refs；usage/help 文案隨載體更新——傘狀 bin 非 Python parity 面，此裁決明文記錄）。語義約束：與 S3 共享 sidecar JSON 格式（跨語言互通 R4-N）；與 S6 共享 dogfood 入口。

### 核心實作要點
1. `export_module_edges(conn, repo_root, profile)`：三 kind 綁參 SQL；qualified→`split("::")[0]`→repo_relative；雙端過濾（repo 外/excluded skip）；同模組邊仍計 files；`sorted()` 邊去重排序；`raw_edge_count` 全 kind COUNT
2. commit 錨定：`git rev-parse HEAD`＋`git log -1 --format=%cI`；`_assert_git_root`（`--show-toplevel` 比對——防外層 repo 靜默錯植）
3. `detect_stale` 三級（S1 時間地基）；`_load_metadata`（空/半套 retry 1s；非 CRG→crash 訊息）
4. `write`：檔名 `<repo>-<sha8>.json`；D1 serializer（indent=1/鍵序/`_meta` 九欄）；冪等覆寫
5. CLI `snapshot`：argparse 模擬（--repo 預設 cwd/--label/--out-dir）；stdout 三形態（stale WARN/空集合 WARN/OK＋LOG）

### Invariant Impact
- 受影響：**commit 錨定正確性**（`_assert_git_root` 漏＝HEAD 靜默錯植外層 repo——下游 transition 全歪）
- 驗證對齊：`test_snapshot.py` 直譯（同模組 files 計入、excluded skip、stale 三級）；**git-root assert 無既有 oracle**（該檔把 `_assert_git_root` monkeypatch 掉、所指 integration 檔已隨自足化刪除）——Rust 新案例：`--repo` 指子目錄→exit 1＋stdout 空（訊息對齊 `snapshot.py:172-185`）

### 驗證策略
cargo：synthetic db（S1 helper）跑 export 斷言；stale 三級注入（metadata 鍵操縱）；`test_snapshot.py` 案例直譯；寫檔後 Python `load_snapshot` 消費斷言（cargo 測試內以 subprocess 呼 Python？——否：**S6 parity 測試做跨語言**，cargo 端只驗格式自洽）。已知未覆蓋：`time.sleep(1.0)` retry 真並發窗口。

## 段 3：transition

### Context
`transition.rs`。依賴 S2（snapshot 格式）。語義約束：B1 pair 集合差語意；md/json 雙輸出（`indent=1`）；`-o` prefix `transition-{a8}..{b8}`。

### 核心實作要點
1. `load_snapshot`（json parse＋三元組 assert 訊息）；`diff_edges`（tuple 差＋reversed added-direction＋kind 變化非 reversed）；`changed_modules`（邊拓撲∪檔案增刪所屬模組）
2. `extract_ep_claims`（claims_regex findall；檔缺→crash「NONE 是檔在但無 mention」）；`extract_baseline`（regex＝`\*\*baseline\*\*:\s*([0-9a-f]{7,40})`——**含粗體 literal**，漏了會多匹配純文字 baseline 寫法；`transition.py:28` 原文）；`compare_claims` 三桶
3. `render_report`（H1/無結構變化/各節/截斷 20/rename 註記逐字）；`render_json`（鍵序）
4. CLI：位置參數×2＋三 flag；stdout `[OK] transition ...`＋baseline `[LOG]`＋尾 `[LOG]`

### 驗證策略
cargo：`test_transition.py` 案例直譯（reversed B1、kind 變化、claims 三桶、無變化）；**截斷無既有 oracle**（baseline 測試無 `+N more` 案例）——新增 >20 邊案例逐字驗 `- ... +{N} more`（`transition.py:140-152`）；snapshot 差集用 S2 寫出的真檔（自產自銷）。已知未覆蓋：非 ASCII 模組名排序邊界（D4 記錄）。

## 段 4：graph_audit＋scip_refs --audit

### Context
`graph_audit.rs`＋`cli.rs`＋**`cache.rs` 擴充**（`audit_targets`/`missing_refs` 需 sqlite/protobuf 兩 face 路徑——對應 Python 模組級 `scip_refs.py:203,225`＋Face 方法 `:495,:550`）。依賴 S1＋R2（`cli.rs` 路由、`engine::fn_tail_name`、`cache::open_face`/Face）。語義約束：**`--json` 四鍵是治理鉤子契約面**（R4-J——本 repo 以 S6 synthetic 雙跑 gate；NT 消費端驗證隨 open-source 政策改道，見 S6 改道注）；exit 家族 0/1/2（D3）；與 R2 cli 的 `--audit` 互斥條款接線（`cli.rs:231` 預留位）。

### 核心實作要點
1. `scan_files`（scan_root glob 優先；否則 rglob `*.rs` 經 exclusions）
2. `risk_scan` D1 狀態機：IMPL_RE/FN_RE 直譯（regex crate——無 look-around 需求）；per-block 去重；閉合規則（`}` 縮排 ≤ impl 縮排）
3. `ra_symbols`：`rust-analyzer symbols` stdin bytes、timeout 60s、`check=false`；RA_LABEL_RE/RA_KIND_RE 解析；`db_functions`（resolved 絕對路徑 file_path；kind IN Function/Test）
4. `audit()`＋exit 語意：env 檢查序（which rust-analyzer→db 存在）；假乾淨防護（audited∧total_ra==0→exit 2）；人讀面（D1/D2 OK 行、缺差 WARN、無缺差 OK）；**CLI flag 全面**：`--repo`（required）、`--json`、**`--all`**（store_true——scope＝全部 .rs 對帳 vs 風險檔，`graph_audit.py:180`）、**`--graph <db>`**（覆寫 db 路徑，`:239`——parity 測試自選 db 旋鈕）
5. `--json`：四鍵 D1 serializer（ensure_ascii=False, indent=1）印 stdout
6. `scip_refs --audit`：D7 in-process 兩遍式——lib 呼叫 audit→missing→`files_by_name`（repo 外 WARN loud）→雙鍵歸屬 `audit_targets`（FN_TAIL 述詞複用 R2 `fn_tail_name`）→`missing_refs`→輸出三段；`repo.resolve()`；互斥條款＋路由接線（FLAGS 增 `--audit` bool——`--h` 縮寫面變化檢查：`--a` 前綴無既有衝突）；**互斥檢查序**：`--audit 與查詢互斥`/`--audit 需 --repo` 兩 guard 在 index 解析**之前**（`scip_refs.py:785-790`）——Rust 現行 final guard 無條件擋缺 query（`cli.rs:315-322`），接線時改條件式（cli.rs 自稱 order mirrors Python——勿破例）

### Invariant Impact
- 受影響：**缺差判定方向**（ra_count vs db_count 反轉＝假警報/假陰性翻轉——D2 對帳的 silent-corruption 面）
- 驗證對齊：`test_graph_audit.py` 直譯（per-block 計數 vs 交集、kind 含 Test）；**vacuous 雙層與 exit 家族無既有 oracle**（該檔無 main/exit 斷言）——新增 RED 案例，位元組以凍結源碼直讀釘（零輸出 WARN `graph_audit.py:194-199`、假乾淨 exit 2 `:251-257`、exit 家族 `:297`）

### 驗證策略
cargo：synthetic db＋fake ra_lookup（測試注入——`ra_symbols` 抽象為 trait/閉包注入，移植 Python `ra_lookup` 注入點）；D1 狀態機案例（kernel.rs 三 impl 形態、inline mod 縮排、無法閉合保守膨脹）；serializer 位元組案例（空 list/巢狀/非 ASCII）＋vacuous/exit RED 案例（見 Invariant）。已知未覆蓋：rust-analyzer 輸出格式版本漂移（fail-loud 面照搬；真 RA 環境由 S6 實機步驟觸）。

## 段 5：graph_csv

### Context
`graph_csv.rs`。依賴 S1。最小獨立段。

### 核心實作要點
1. `load`：兩次全表掃（nodes/qualified→file 映射＋community 投票）；多數決 tie-break `(-count, id)`；File 列（name 欄非 qualified_name）；邊投影 proj（qualified 命中→其檔；否則 `::` 前綴；self-loop skip）；pair 聚合 kinds `"+".join(sorted)`
2. `degrees`（undirected；Σdegree==2×links 不變量）
3. `write_csvs`：**Python csv QUOTE_MINIMAL 直譯**（hand-rolled writer——含逗號/引號/換行欄位加 quote＋`""` 轉義；禁 csv crate 預設〔RFC 差異〕）；表頭固定兩行；**行尾＝CRLF**（Python csv excel dialect `lineterminator="\r\n"`＋檔案 `newline=""` 不轉譯——hand-rolled writer 每行以 `\r\n` 收尾，`graph_csv.py:140-142,158-160`）
4. CLI：`--repo`/`--out-dir`（default `.agent-tmp`——目錄 mkdir parents）；db 缺 crash

### 驗證策略
cargo：`test_graph_csv.py` 直譯（投票 tie-break、投影、不變量）；**quoting 邊界無既有 oracle**（fixture 欄位全無特殊字元）——新增 `,`/`"`/換行欄位案例；**無 communities 投票案**（communities 空→欄位空字串，`graph_csv.py:68-71,151`）；**CSV 檔案位元組 cmp 入 S6**（stdout gate 外——`\n` vs `\r\n` 分歧會靜默通過 stdout cmp）。已知未覆蓋：巨圖效能（量級由 S6 實機步驟記錄）。

## 段 6：parity harness 擴充＋收尾

### Context
master R4 gate ①部分裁決。依賴 S1-S5。語義約束：graph.db 只讀（`connect_ro`——synthetic 或實機一律）。

### 核心實作要點
1. **Python 端 synthetic 雙跑**：`tests/parity/` 新 `test_graph_family_parity.py`——`make_crg_db` 建 synthetic db（tmp），六工具逐案例 Python vs Rust `cmp` stdout＋exit（normalize 只替 tmp 路徑；crash 案例斷言 exit 1＋stdout 空）；`created_at` 類浮動欄位：parity 案例注入固定 commit（`make_meta` 的 commit 參數）＋normalize `created_at`；**`-h`/usage 面 cmp**：四新子命令 `-h` stdout 逐字 cmp（D3 補充面②）；**graph_csv 案例加兩 CSV 檔位元組 cmp**（含 CRLF）
2. **dogfood（本 repo 自身——實機手動步驟，非 committed 測試）**：本 repo graph.db 是 gitignore 面（fresh clone 缺席）——**gate 不依賴它**（gate＝要點 1＋3）。開發機有 graph.db 時執行：Rust `snapshot` 跑通＋凍結 Python `transition` 消費＋Rust/Python `graph_audit --json` 對 `--repo ~/Github/code-reality` 雙跑 cmp（自我相對 parity——兩端吃同一 db，語料漂移兩端同漂＝等價），結果記入 EP 執行報告；rust-analyzer：`rust-toolchain.toml` 加 `components = ["rust-analyzer"]`（rustup 環境自動裝——零 skip 政策的最小 carve-out，`tests/AGENTS.md` 記明），缺席時該步驟記錄未跑；**注意本 repo profile exclude `tests/fixtures/`**——斷言覆蓋面時排除該前綴
3. **跨語言互通（R4-N）**：Rust `snapshot` 寫 sidecar→凍結 Python `transition` 消費（`load_snapshot` assert 過＋diff 結果與 Rust transition 自銷一致）
4. 收尾：root AGENTS.md UC-4 行＋Boundary/export 行註記 Rust 載體（snapshot/transition/graph_csv/graph_audit＋`--audit`）；`crates/AGENTS.md` 模組導航；kanban 卡（In-Progress→階段結算）；master 追蹤卡 R4①進度；②子 EP 指針（`ep-rust-r4b-hazard-hubrefs.md` 待衍生）

> **NT/mosaic 真語料 gate 改道（2026-08-25 user 裁決——open-source 測試政策）**：原 master R4 驗收的「NT `graph_audit --json` cmp（430/233/861 參考值）」不在本 repo gate——測試套必須自足（模式＝CRG/scip-callgraph：合成 fixtures＋inline literal 單元測試＋零外部 repo 依賴；消費者怎麼驗自己的用法是他們的事）。本 repo 以 synthetic 雙跑＋self-dogfood 為 gate；430/233/861 留作歷史參考記錄。legacy NT/mosaic 消費測試依 `tests/AGENTS.md` 政策移除（歷史裁決記錄留在歸檔 EP）。

### 驗證策略（＝master R4 gate ①裁決，open-source 政策形態）
synthetic 雙跑全綠（含 `-h` usage 面、CSV 檔案位元組）＋跨語言互通＋全量回歸（自足套基線零改動＋cargo/clippy/deny/ruff/mypy 綠）；dogfood＝實機手動步驟（best-effort 證據，非 gate）。**已知未覆蓋**：hub_refs×hazard（②）；NT/mosaic 真語料（消費端 handoff）；WAL 真並發撕裂（守衛語意由注入測試代表）。

---

## 整合策略

- **跨段整合點**：S1 地基＝S2-S5 共用（EDGE_KINDS/module_of/connect_ro/serializer）；S2 sidecar 格式＝S3/S6 消費；S4 `--json`＝S6 NT gate；R2 cli 的 `--audit` 預留位接線。
- **baseline**: `fd64449`。
- **回退路徑**：全程 additive（新模組/新子命令/新 parity 檔；R2 既有輸出面零改動——`--audit` 是 R2 明文未實作面）。
- **git**：每段一 commit（user consent gate）。

## Ask First

1. 所有 git commit（consent 規則）
2. D2 時間選項若選 `jiff`（新依賴）——manifest 變更隨段 commit 帶說明
3. 無新增（hazard②子 EP 另衍；graph.db 寫入永不發生）

## 收尾步驟

1. Capabilities：root AGENTS.md UC-4 行（audit 面）＋boundary/narrative 行（snapshot/transition/graph_csv）註記 Rust 載體
2. Kanban：`rust-graph-family` 卡（建卡於 EP 產出時）In-Progress→Done；master 追蹤卡 R4①✅
3. instruction 檔：`crates/AGENTS.md` 補六模組導航（foundation/graph 家族層次＋D3 exit 語意表）；順手修正該檔陳述過時的 NT skip-on-stale 行（相對 tests/AGENTS.md 政策）
4. ②子 EP 指針：master R4 段補「①完成，②→ ep-rust-r4b-hazard-hubrefs.md」
5. /audit-test（手動指定 crates/tests 新檔——vacuous 檢查：parity 雙跑真的兩端都跑）
