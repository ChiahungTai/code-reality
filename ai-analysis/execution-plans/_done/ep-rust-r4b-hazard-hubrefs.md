# EP：R4② hazard×hub_refs Rust 化（可刪判斷安全網）

> **ep_type**: implementation
> **parent**: [ep-rust-migration.md](ep-rust-migration.md)（段 R4 兩段式之②；繼承 AD-1〜AD-5／雙凍結紀律——①已於 `990edbc` 完成）
> **spec 鏈**: 凍結 Python 即規格——`code_reality/hazard.py`（565 行：六規則判定層純函數＋rg runner＋nodes 解析）＋`code_reality/hub_refs.py`（408 行：CRG query subprocess＋nodes 解析＋目錄聚合＋hazard stage＋`--json`）；既有 `tests/test_hazard.py`（556 行）＋`tests/test_hub_refs.py`（438 行）為語意 oracle（案例族全可注入——monkeypatch crg/rg）
> baseline: `990edbc`

## 實作總覽

| 段 | 內容 |
|----|------|
| S1 | `hazard.rs`：SymbolFacts/HazardFinding＋`parse_symbol_facts`（ruff_python_parser）＋pattern builders＋`classify_rg_lines`＋六 detectors（RgRunner 注入）＋resident/full/gate_warning＋`make_rg_runner`（rg subprocess 原參數）＋`symbol_facts`（nodes 解析） |
| S2 | `hub_refs.rs`：`crg_query`（uvx subprocess）＋`resolve_qualified`（nodes 精確匹配）＋`aggregate`（first-seen Counter×top）＋`caller_files_of`＋`hazard_stage`＋`json_payload`（compact separators）＋CLI；main.rs 路由 `hub_refs` |
| S3 | 測試：`test_hazard.py`/`test_hub_refs.py` 案例族鏡像（共享 fixture 斷言＝committed 差動）＋serializer 釘＋parity（lib 面可行部分）＋收尾 docs |

**繼承硬約束**：lib 回 ToolOutput；rg/uvx shell out 原參數（D5-inherited——`rg -n --no-heading -t py -t yaml -t json -t toml {args} . -g !.venv/** …` cwd=root＋路徑 `.`）；`uvx code-review-graph query <pattern> <target>` 120s timeout；exit 語意：`SystemExit(str)` 訊息→stderr＋exit 1；not-found 路徑 stdout 空；**`_require_ok`／ambiguous 路徑 stdout 含 `[FAIL]`＋候選[:10] 行**（print 先於 raise——防假陰性輸出設計）；argparse `-h`＝exit 0、usage error/壞值＝exit 2。

## 段落 0：錨點與決策

### 凍結錨點

| 錨點 | 定義端 | 移植語意 |
|------|--------|---------|
| `parse_symbol_facts` | `hazard.py:88-114` | AST walk ClassDef（name==symbol）；bases＝Name.id／dotted Attribute；`STR_ENUM_BASES={StrEnum,BaseStrEnum}`＋`str+Enum` 組合；`PROTOCOL_BASES={Protocol}`；SyntaxError→空 facts |
| `_extract_str_members` | `hazard.py:128-140` | class body 頂層 `NAME = "value"` 單目標 Constant str |
| `method_name` | `hazard.py:143-146` | `::` 剝前綴→`.` 取尾；裸 class→None |
| pattern builders | `hazard.py:149-161` | getattr `getattr\(\s*<id>\s*,\s*["']<sym>["']`（regex escape）；strentenum `"<v>"` rg -F；importlib `import_module\(\s*["']<mod>["']` |
| `classify_rg_lines` | `hazard.py:164-183` | `tests/` 前綴→test；`is_excluded`→excluded；其餘 prod |
| 六 detectors | `hazard.py:186-356` | strentenum/getattr/gap 排除定義檔自身；gap 兩形態（裸 `\bSym\(`／`.method\(`）＋差集；count/summary/detail 逐字。evidence 組成各異：strentenum/protocol＝`prod[:5]`、getattr/importlib＝`(prod+test)[:5]`、gap＝`sorted(missing)[:5]`；人讀面 evidence 切 `[:3]`（hub_refs.py:397） |
| `resident_findings`/`full_findings`/`hazard_gate_warning` | `hazard.py:359-445` | 常駐=存在性（count=0）；full=六規則含計數；gate：`static_prod ≤ 2`＋findings→WARN 行 |
| `symbol_facts` | `hazard.py:453-503` | nodes 查 `name[+parent_name]`→唯一 in-repo 非 excluded→AST parse（**corrupt db→AssertionError fail-loud**，非降級；非唯一→name-only facts；AST parse 失敗仍設 rel_path/module/kind） |
| `make_rg_runner` | `hazard.py:513-565` | rg subprocess 原參數＋`.` 路徑＋`./` 剝前綴＋exit∉{0,1}→crash |
| `crg_query` | `hub_refs.py:70-99` | uvx subprocess 120s；returncode≠0→crash；stdout 非 JSON→crash |
| `_require_ok` | `hub_refs.py:102-114` | status≠ok→`[FAIL] CRG {status}: …`＋候選[:10]→exit 1（stdout 空） |
| `resolve_qualified` | `hub_refs.py:117-168` | `::` 直通；nodes 精確匹配（`.`→name+parent）；唯一→qname；多→`[FAIL]`＋清單＋exit 1；零→`symbol not found` exit 1 |
| `aggregate` | `hub_refs.py:184-219` | 目錄（去檔名）Counter；`is_test or tests/ 前綴`→test；`most_common(top)`＝**first-seen 序**（count desc 穩定）；outside 計數 |
| `hazard_stage` | `hub_refs.py:242-288` | callers 方向恆跑（觸發式：static_prod≤2 或 force）；callees 僅 force 且 gap 跳過；level resident/full |
| `json_payload` | `hub_refs.py:291-319` | **八鍵**（symbol/target/direction/results_omitted/aggregate/hazard_findings/hazard_level/hazard_gate）；`json.dumps(ensure_ascii=False)` **無 indent**＝separators `(", ", ": ")` |
| CLI 面 | `hub_refs.py:322-405` | positional symbol（required）＋`--repo`/`--direction {callers,callees}`/`--top int=20`/`--hazard`/`--json`；人讀面（[OK] 行/prod:/test: 目錄行/⚠ hazards/註腳 WARN） |

### 決策（本 EP 定案）

- **D1（AST 載體）**：`ruff_python_parser`（master POC 已驗編譯＋parse；語意差動＝S3 共享 fixture 鏡像＋dogfood 手動）。SyntaxError→空 facts 語意：ruff parse 錯誤回 Err→同 Python 容錯
- **D2（aggregate 次序）**：Counter＝first-seen——Rust 以 Vec+HashMap 保存插入序（`most_common(top)`＝前 top 個照插入序 count desc；count 相同保持首見先）
- **D3（hub_refs `--json`）**：compact＋`ensure_ascii=False`＋separators `(", ", ": ")`——`common::to_json_py_compact()`（**本 EP 新建**，照 `to_json_indent1` 先例——serde 自訂 formatter 覆寫 `begin_array_value`/`begin_object_key`〔`, `〕＋`begin_object_value`〔`: `〕）＋單行＋尾換行（`print`）
- **D4（gate 形態）**：committed＝雙語言鏡像案例族（同一 fixture 斷言）；CLI 位元組 parity 依賴 uvx+CRG＝外部依賴，走 dogfood 手動步驟（mosaic `hub_refs --hazard` cmp——master R4 gate 承接；env 缺席＝兩端同 fail-loud 亦有效）
- **D5（ruff 依賴面）**：crates.io `ruff_python_parser`＋`ruff_python_ast`；deny 驗授權（MIT）

### 風險

| 等級 | 假設 | 驗證 |
|------|------|------|
| 高 | ruff_python_parser AST 語意 vs CPython ast（ClassDef walk/bases/Assign-Constant-str） | S3 鏡像 test_hazard.py 全案例族（strentenum 兩形態/protocol/syntax-error 安全）＋dogfood 差動 |
| 中 | most_common first-seen 序重現 | 單元釘（count 同值序） |
| 中 | compact separators 位元組 | 序列化釘案例 |
| 低 | uvx/rg subprocess 形態 | timeout/exit 家族鏡像測試 |

## 段 1：hazard.rs

### Context
判定層純函數（六規則）＋rg runner＋nodes 解析。依賴：S1① 既有 `common`/`profile`。語義約束：detectors 介面 `RgRunner = Fn(&[&str]) -> Result<Vec<String>, String>`（測試注入——鏡像 Python RgRunner Callable）。

### 核心實作要點
1. `SymbolFacts`/`HazardFinding` struct（HazardFinding 的 detail＝`Vec<(String, i64)>` 保序——Python dict 插入序）
2. `parse_symbol_facts(source, symbol)`：ruff parse→visit ClassDef；bases／StrEnum／Protocol／str members
3. builders＋`classify_rg_lines`＋六 detectors＋`resident_findings`/`full_findings`/`hazard_gate_warning`（summary 文案逐字）
4. `symbol_facts(symbol, repo_root, profile)`：nodes 查詢（`name[+parent_name]`）→repo_relative＋excluded 過濾→唯一才 AST；module＝rel 去 .py 換 `.`
5. `make_rg_runner(repo_root)`：rg subprocess（原參數；`./` 剝前綴；exit∉{0,1}→Err）

### 驗證策略
cargo 鏡像 `test_hazard.py` 案例族：parse_symbol_facts 六測例（strentenum／str+Enum comma／protocol／plain〔頂層成員**會**被捕〕／missing／syntax-error）／builders／method_name 四態／classify／六 detectors（注入 rg 行集——**args-aware**）／resident vs full／gate 閾值 2（inclusive）／gap 兩形態與差集/symbol_facts 經 crg_fixture db（唯一/多個/缺 db/corrupt-db fail-loud）＋**TestMakeRgRunner 鏡像**（真 rg subprocess：`./` 前綴剝除＋排除 glob＋三 builder 相容——Python 套件同賴 rg 在場，依賴姿態對齊）。

## 段 2：hub_refs.rs＋CLI

### Context
CRG query subprocess＋聚合＋hazard stage＋CLI。依賴 S1(hazard)+S1①(common/profile/argparse)。

### 核心實作要點
1. `crg_query(pattern, target, repo_root)`：`uvx code-review-graph query …` cwd=root、120s（spawn+try_wait 迴圈）、returncode≠0→Err（crash 文案含 stderr 尾 500）；stdout serde_json 解析（非 JSON→Err）
2. `_require_ok`/`resolve_qualified`/`resolve_symbol`：nodes 精確匹配（`.`→name+parent_name）；status≠ok→`[FAIL]`+候選（stdout 空、exit 1）；ambiguous/not-found 文案逐字
3. `aggregate(results, repo_root, top)`：目錄計數（first-seen 序）；`is_test || tests/ 前綴`；excluded/outside 計數
4. `caller_files_of`＋`hazard_stage`（觸發邏輯：force OR (callers && total_prod≤2)；callees 僅 force 且 baseline=None）＋`json_payload`（D3 serializer）
5. CLI：argparse SPEC（symbol positional required＋五 flag；`--top` 值驗證：Python `type=int` 壞值→argparse error exit 2）；人讀面逐行對齊（[OK]/prod:/test:/⚠/WARN 註腳）

### 驗證策略
cargo 鏡像 `test_hub_refs.py`：aggregate 四案例（含 tie first-seen）／crg_query（uv 缺席/exit≠0/非 JSON/逾時——Err 家族；成功解析形狀）／resolve_qualified（直通/精確/parent/ambiguous/not-found/excluded-only）／**resolve_symbol 轉發面**（not_found/ambiguous 的 `[FAIL]`＋候選 stdout 契約——require_ok 面）／hazard_stage 觸發矩陣（0/2 inclusive/3 no/force/callees）/json_payload 八鍵形狀＋空 hazard；CLI（-h exit 0／usage exit 2／--direction invalid choice／--top 壞值——新增，無 Python 鏡像）。parity（lib 可行面）：json_payload 序列化位元組對 Python `json.dumps` 釘案例。

## 段 3：收尾

1. AGENTS.md：UC-6 行註記 Rust 載體
2. kanban：`rust-graph-family.md` Done 卡補②完成註記（或建 `rust-hazard-hubrefs.md`——採前者，①②同卡）
3. master EP R4 段補「②完成」
4. dogfood 手動步驟（mosaic `hub_refs --hazard`＋`--json` 雙跑 cmp——記錄 EP 執行報告）
5. /audit-test：detectors 注入面非 vacuous 檢查

## 整合策略

- 回退：全程 additive（兩新模組＋main.rs 路由一行）；R2-R4① 輸出面零改動
- git：單一 commit（user deep-work 弧授權——AUTH：user said「你應該要全部做完做到Ｒ７」）

## EP Review Findings（2026-08-25，spec 忠實度單軌）

| ID | 嚴重度 | 處置 | 狀態 |
|----|--------|------|------|
| F-01 | 🔴 | 「空 stdout」錯述——實作已正確（讀源時按 print-先-raise 移植）；EP 三處文字修正 | implemented |
| F-02 | 🟡 | symbol_facts corrupt-db 語意補正（fail-loud 非降級）——實作正確；補 EP 文字＋測試案例 | implemented |
| F-03 | 🟡 | 九鍵→八鍵——實作已八鍵；EP 文字修正 | implemented |
| F-04 | 🟡 | 補 TestMakeRgRunner 鏡像（真 rg） | implemented |
| F-05/06/07/08/09 | ℹ️ | helper 新建註記／evidence 組成逐 detector／行號 149-161／-h exit 0 措辭／案例清單對齊 | implemented |
| F-10 | 🟡 | 補 require_ok 轉發面測試 | implemented |

## Build Record（2026-08-26）

- **AST 差動 dogfood（R4b 硬 gate）**：七案例（strentenum/comma 形態/protocol/nested+dotted base/Multi bases+AnnAssign 排除/syntax error/相鄰字串拼接 `"xy"`）——ruff 0.0.10 vs CPython ast **逐欄位全等**。
- **真語料 dogfood（mosaic）**：`hub_refs Interval --json`（resident 面）與 `AlphaCondition --hazard`（ambiguous FAIL 面）**位元組全等**；force 全規則面（qualified Interval --hazard）發現**凍結工具自身的非確定性**——rg 多 `-e` pattern 並行輸出序洩漏進 `evidence[:5]` 切片，Python 與 Rust 皆 run-to-run 翻動（**counts/summary/結構穩定**、僅 evidence 行序賽跑）。非移植缺陷；parity 判準＝計數與結構位元組穩定＋evidence 統計等價。**記錄為已知面**（未來刻意演化候選：rg `--sort path`——需 relay 同步）。
- **實作勘誤**：`to_json_py_compact` 初版漏覆寫 `begin_object_key`＋first 值誤寫 `[`（測試抓出後修）；`query_nodes_pairs` 誤直開 sqlite 改走 `connect_ro`；fixtures 需 canonical 先行（/var symlink 陷阱第三次出現——已入 cargo 測試慣例）。
- 閘門：cargo 測試 s4b 26 案例＋全套件回歸；clippy/ruff/mypy/deny 見收尾報告。

## Ask First

1. ruff 差動若實質漂移修不齊→fallback 階梯（master 風險條款）——dogfood 步驟發現時停
2. 無新增
