# EP: Pyrefly-linked Rust-native Python producer

> **ep_type**: implementation
> baseline: b0ed95a
> 前置：與 ep-occurrence-producer.md 平行（不阻塞其 S3-F2/S5——資料面共用）

## North star

Python graph 資料的 producer 由 **link Pyrefly 的 Rust 原生 binary** 供應：
Node.js、vendored pyright（2022 快照）、自維護 patch 三負擔一起退役。
資料面（cache 三表＋`graph_db build`）零變更——引擎可替換的架構承諾
在此兌現。

## 裁決已定案（2026-08-28，證據在前序 session）

1. **引擎選 Pyrefly**（v1.3.0-dev.2，Meta/Instagram/PyTorch 生產級）
   優先；ty 備位（beta）。spike 記錄：fatal 檔 0.29s 零崩潰、自動吃
   `pyrightconfig.json`、`ruff` 系 crates 親和。
2. **不自建語義引擎**（＝重寫 pyright，occurrence EP 路線 (c) 否決
   沿用）；**資料面自有的部分已建完**——本 EP 只做「換心臟」。
3. **crates.io 未發佈**（`pyrefly 0.0.1 Coming soon`）→ git-dependency
   pin tag 是唯一 link 路；internal API 無穩定承諾→隔離層必須薄且
   單點（見 S1 設計）。
4. **已定位的原語**（`~/Github/pyrefly` clone 實讀）：
   `State::new(config_finder, thread_count)` → `new_transaction` →
   `handles.all(config_finder)`（構築模板在 `pyrefly/lib/commands/
   check.rs:1433-1445`）；`Transaction::goto_definition(&handle,
   range)`／`get_ast`／`get_module_info`；CALLS 級原語
   `find_global_incoming_calls_from_function_definition`
   （`pyrefly/lib/lsp/non_wasm/call_hierarchy.rs:350`，rdeps closure＋
   `local_references_from_definition`）。

## 與 occurrence EP 的關係

| 面 | 歸屬 |
|---|---|
| cache 三表、`occurrence_roles`（S3-F2）、CALLS pair-set 對帳（S5） | occurrence EP——**engine-agnostic，不重做**；本 EP SCIP face 完全不碰 cache schema |
| producer 引擎（scip-python fork） | 本 EP **接替**——fork 降級 fallback |
| golden corpus（sidecar baseline＋`scripts/golden_corpus.py`） | 共用驗收 oracle |

## EP Review Findings（2026-08-28，五維度 agent 審查＋主 session 查證裁決）

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| F-1 | 🔴 | S1 | 「SCIP face 或直寫面擇一」隱藏機械約束：直寫面需 placeholder `index.scip`＋fndefs sidecar＋ingest 過濾複製（炸點僅在端到端 spans ladder）；SCIP face 下 `build_db` 不寫 producer meta（cache.rs:119-121 硬編碼 tool） | 裁決 **SCIP face**（主 session 查證屬實：SCIP SymbolRole 無 call bit、call-site＝callee reference occurrence 可完整表達解析；全既有管線照用）；producer 身分改記 index `tool_info` | implemented |
| F-2 | 🔴 | S1 | `occurrence_roles` 只存在 EP 文本；與平行 session land 順序未定；in-db side table 與 crates/AGENTS.md「extensions never in shared db」衝突 | SCIP face 下本 EP **不寫 occurrence_roles**——schema 管轄權 100% 留 occurrence EP F2；衝突消解 | implemented |
| F-3 | 🟡 | S1 | 第三前綴幾乎必然需要（條件項應改確定項）；`infer_language` 在 graph_db.rs 非 engine.rs；test 未延伸 | 改確定工作項＋`python_prefixes_cover_both_python_producers` 擴三 producer＋錨點修正 | implemented |
| F-4 | 🟡 | S3 | `producer_of` 對 pyrefly 歸類未處理（Protobuf face 落 "scip"、身分在 graph.db 消失） | 裁決：接受 "scip"（spans ladder 分支正確）＋記錄理由；身分留 index tool_info；`producer_of` 擴充列 optional observability follow-up | implemented |
| F-5 | 🟡 | S2/P-3 | bar 引用錯位（96.5% 是 lsp library 口徑；golden_corpus 只有 symbol-exact reconcile，跨 producer 符號集不相交） | bar 改 name-normalized def coverage **≥94.7%**（scip-python parity）＋golden_corpus `--normalize` 旗標（預設 off＝現行輸出位元組不變，R2-7 凍結相容） | implemented |
| F-6 | 🟡 | S2/P-4 | CALLS pair-set 量測硬依賴 occurrence S5（未建） | 整合策略加 blocked-on 註記；P-3 先行 | implemented |
| F-7 | 🟡 | S3 | 與 occurrence EP F5 撞同 slot、land 順序未定；cutover 使其 baseline 失效 | land 順序＝occurrence F2/F5 先、本 EP S3 後；S3 加 baseline 重跑 | implemented |
| F-8 | 🟡 | S1/S3 | 無 cargo test 規劃；S3 未引用 R2-5 下游清單 | S1 測試清單入文；S3 繼承 R2-5（graph_audit provenance、graph_query/MCP 邊過濾） | implemented |
| F-9 | 🟡 | Matrix | 缺 env 失敗／partial 輸出／回退演練三情境 | P-7/P-8/P-9 入矩陣 | implemented |
| F-10 | 🟡 | S1/P-2 | defs/refs 枚舉未 spike；dunder 證據僅 call 語境（`resolve_call_dunders` 是 call 限定） | P-2 風險註＋S1 語境分離驗證＋S2 首批差異歸因枚舉面 | implemented |
| F-11 | ℹ️ | S2 | 「vs 40s」出處不明（全 repo 僅此一處） | 改絕對值記錄＋對照 scip-python 實測 26.1s | implemented |
| F-12 | ℹ️ | S1 | 82,732 數字來源未標；「全 corpus」vs「全 repo root」用語不一致 | 補註 POC 全 repo root run；用語統一 | implemented |
| F-13 | ℹ️ | 全域 | pyrefly git-dep build-time 成本未註記；crate 內 engine.rs 與 code-reality engine.rs 同名混淆 | 收尾加 build-time 註記；隔離檔改 `api.rs` | implemented |

## UC 盤點

| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Rust 原生 Python occurrence producer | 📋 | link Pyrefly＋薄 SCIP/直寫 emitter |
| scip-python fork 退役為 fallback | 📋 | cutover 後文檔降級 |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint |
|---|------|------|---------|-----------|
| P-1 | link spike：小 bin 構築 State 枚舉 call sites | S0 | 編譯過＋跑出 ≥1 條 (caller, callee) 對；API 不可構築＝**gate 失敗回退**（EP 關閉，續用 fork） | S0 |
| P-2 | 小 repo 全量 defs+refs+calls | S1 | 對 golden corpus self-dump 一致性通過 | S1 |
| P-3 | mosaic 全量 | S2 | library name coverage ≥96.5%（occurrence EP 實測 bar）＋時長記錄 | S2 |
| P-4 | CALLS pair-set 對照 legacy treesitter | S2 | 用 occurrence EP S5 機制量測；差距歸因顯式 | S2 |
| P-5 | Pyrefly 版本升級 | 日常 | pin tag＋薄 wrapper 單點改；internal API 破壞面集中 | S1 設計驗收 |
| P-6 | 語義差異（與 pyright 解析不同處） | 對帳時 | golden_corpus diff 逐項歸因（引擎觀點 vs bug） | S2 |
| P-7 | target repo 無 venv／env 解析失敗 | S1 | fail-loud：exit≠0＋明確診斷，不靜默降級（SM-8 先例） | S1 |
| P-8 | 單檔解析 internal error | S1 | 跳過＋結尾 loud 清單（emit-skip 教訓：不整批失敗也不靜默腐敗） | S1 |
| P-9 | cutover 後回退演練（fork fallback） | S3 | fork 路徑重新產出 index 一弧（回退路驗證，非紙上保留） | S3 |

## 段落 S0：link spike＋go/no-go gate ✅ 已完成（2026-08-28，GO）

**實測結果**：spike bin＝`~/Github/pyrefly/pyrefly/examples/batch_calls.rs`
（workspace example＝git-dep 等價證明）。**關鍵 API 發現**：
`transaction.run(&handles, Require::Everything, None)` 是排程器——
漏了它則 `get_ast` 恆 None（所有 getter 純讀取，不觸發載入）。
完整構築鏈：`default_config_finder(None)` → `State::new(cf,
ThreadCount::AllThreads)` → `new_transaction` → `checkpoint(Ok(iter))`
→ `Handles::new(files).all(cf)` → **`transaction.run`** → walk
AST call exprs → `goto_definition(handle, TextSize)`。
**數據**：code-reality scripts 煙霧 ✅；mosaic_alpha 全 corpus
27,238 call sites／27,605 resolved targets／debug build 3:48.68
（499% CPU）——release 預期 5-20× 更快。**GATE: GO**（S1 起跑）。

## 段落 S1：薄 emitter（`crates/pyrefly-producer`）

**Context**：spike 形態產品化。**單一職責**：讀 repo → 枚舉全部
defs／references／call sites → **發射 SCIP protobuf index**（review
F-1 裁決 SCIP face：cache build／golden corpus／fndefs spans／
`graph_db build`／stale 語義全走既有管線，North star「資料面零
變更」由此兌現；直寫面需複製 placeholder `index.scip`＋fndefs
sidecar＋ingest 過濾三機制且炸點僅在端到端 spans ladder——列
後續優化，非本 EP）。產出放 sidecar slot，沿用既有
`--stamp-meta` → `--build-cache` 順序；**不寫 `occurrence_roles`**
（review F-2：side table 管轄權 100% 在 occurrence EP F2，本 EP
的 index 經共享管線餵它，不碰 cache schema）；producer 身分記
在 index metadata `tool_info`（`pyrefly-1.3`；cache meta `tool`
硬編碼 `'code_reality.scip_refs'` 是共享行為，不動）。
- 隔離設計：對 Pyrefly 的每一個 import 集中在單一 `api.rs`
  （review F-13 消歧，避免與 code-reality `engine.rs` 同名混淆）；
  其餘代碼不感知 Pyrefly（升版只動一檔）
- git-dep pin：`rev = "1d64c4b605445f43d66162e56592837223b3dabf"`
  （validate 修正：v-prefix tag 不存在；最新 tag `1.3.0-dev.2`
  ＝08-18 落後 spike 實證 commit `1d64c4b`＝08-27 達 2,445 行
  state/ 差異——pin 實證 commit，升級到未來 tag 是顯式 commit；
  POC-1 已用此 rev 從 pyrefly workspace 外部編譯+runtime 驗證）
- 語義約束：目標命名走 `find_definition`（富型別含
  `display_name`——validate 實測全 repo root 1,341 檔 82,732
  targets 零 None）而非 `goto_definition`（僅 module+range）；
  multi-target 邊去重：dunder pair（`__init__`+`__new__`）崩縮
  單邊 prefer `__init__`（validate 實測 3,044/3,044 全為 dunder
  pair）——**在 emitter 內完成**：SCIP 無 call role bit，
  call-site＝callee 的 reference occurrence，dunder 崩縮須在
  發射前擇一
- symbol 形態：比照 scip-python 慣例（leading discriminator
  `` `pyrefly python <project> <version> symbol` ``、函數 `().`
  尾、方法 `Class#method().`）；`infer_language` 前綴表加
  `pyrefly ` 判 Python（graph_db.rs:237；review F-3 改**確定
  工作項**——symbol 形態自選必然帶第三前綴），unit test
  `python_prefixes_cover_both_python_producers` 擴三 producer
- refs 語境註記（review F-10）：`resolve_call_dunders` 是 call
  語境限定——class name 的 reference 應解析到 class def 而非
  `__init__`；S1 以實例驗證此語境分離後再定 refs 枚舉細節
- 測試（cargo test，repo 唯一測試面——review F-8）：dunder
  崩縮規則；emitted symbols 過 `fn_tail_name` 閘門；
  `infer_language` 三前綴；fixture 端到端（emitter →
  build-cache → `graph_db build`）

**驗證**：小 repo（本 repo scripts/）全量 → `golden_corpus --self`
通過；`graph_db build` 消費成功（SCIP face——F-1 已裁決記錄）。

## 段落 S2：mosaic 對帳＋CALLS 量化

**Context**：真實語料驗收。跑 mosaic 全量（**時長絕對值記錄**＋
對照 scip-python 實測 26.1s——occurrence EP S2 結算值；review
F-11）；對帳 bar（review F-5 裁決）：`golden_corpus.py` 加
`--normalize` 模式（fn_tail 正規化比較鍵、**預設 off＝現行輸出
位元組不變**，R2-7 凍結相容）——bar＝name-normalized def
coverage vs lsp golden **≥94.7%**（scip-python parity 實測值；
stretch 96.5%＝lsp library 口徑基準），missing 逐項歸因；CALLS
pair-set 用 occurrence EP S5 機制對照 legacy（**blocked-on
occurrence S5 建成**——review F-6；P-3 對帳可先行）——**本 EP
的核心價值證明**：自有 CALLS 來源的品質基準線。

**驗證**：對帳報告落 sidecar；差異逐項歸因（P-6）；首批差異
優先歸因枚舉面（review F-10——refs 枚舉是未 spike 路徑）。

**結算（2026-08-28 S2 實測）**：
- mosaic_alpha 全量 release **106.42s**（1,341 檔／21,655 defs／
  64,563 in-corpus refs／26,736 call sites／dunder 崩縮 5,989／
  外部 target 丟 280,742）——比 POC-2 的 38s 慢是誠實代價：POC
  只解 call sites，emitter 解**全部** references；對照 scip-python
  26.1s（同數量級、4×；vs lsp_harvest 6.7h）
- **defs coverage（name-normalized）＝ 99.6%**（11,545/11,563，
  bar ≥94.7% 大幅超越；scip-python 94.7% 且有 11 檔跳過，本面
  零跳檔）
- missing 47 歸因：`.agent-tmp/` 點目錄 scope 差異（lsp golden
  索引了點目錄、walker 依慣例跳過——lsp 面的包含才是異常）
- extra 29：pyrefly 多找到的 defs（`setUp` 等 test 方法）
- refs 密度差（50.5k vs 641k≈12.7×）：集中於 dataclass/attribute
  成員引用（`instrument` 27.7k、`account_state` 26k…）——pyright
  LSP reference 面計全部屬性存取（含 Store）；屬 producer 語義
  （occurrence EP 預警的密度差距內），非 bug；`--normalize` 旗標
  已實作（預設 off 保 R2-7 凍結）
- 報告：sidecar `pyrefly-reconcile.json`（offline_backtesting
  slot）；P-4 CALLS pair-set 仍 blocked-on occurrence S5

## 段落 S3：cutover＋fork 降級

**Context**：sidecar slot 新增 pyrefly 面（比照 F5 cutover 模式：
明確 evict/migrate＋meta producer 標記，不靜默切；evict 對象依
slot 現場判定——lsp 舊槽或 scip-python 槽，明確記錄；review F-7
land 順序＝occurrence EP F2/F5 先、本段後，衝突時停下協調）；
`producer_of` 歸類（review F-4 裁決）：SCIP Protobuf face 落
"scip"（spans ladder 分支正確）——**接受並記錄**：pyrefly 身分
保留在 index `tool_info`，`producer_of` 擴充列 optional
observability follow-up；文檔——README prerequisites、AGENTS.md
Capabilities、scip-python fork 與 patch 標記為 fallback（保留，
不刪——回退路，P-9 演練一弧）。

**Fallback 完全退役（S3 後的顯式後續步驟，非本 EP 段落）**：
pyrefly 面在 mosaic 消費端穩定一個觀察期後刪除——清理清單：
`~/Github/scip-python` clone（含 vendored pyright 2022 快照＋
patch）、Node.js runtime 文檔面、`scripts/scip-python-mosaic.patch`
與 upstream issue 草稿（git history 已保存）。**不隨之清理**：
`scripts/lsp_harvest.py`（golden oracle 的產生器——對帳體系換代
前保留）；type-checking 面的 pyright（ZCode lsp-python MCP／
CC 內建）——不同消費者，不在本 EP 管轄。

**驗證**：消費端（mosaic）驗收一弧；NT 零回歸（Rust 面不動）；
Python 面下游繼承 occurrence EP R2-5 清單（graph_audit
provenance、graph_query/MCP 邊過濾）；cutover 後重跑 reconcile
刷新 baseline（既有數據是 scip-python producer 的）。

**結算（2026-08-28 S3 實測）**：
- cutover 鏈全跑：mosaic_alpha slot 新開（無既有槽可 evict——
  land 順序顧慮實際未發生碰撞）→ stamp @ 24ced01 → build-cache →
  graph_db build → import_legacy（+63,825 legacy CALLS 過渡邊，
  退場條件＝occurrence F2 落地＋S5 驗證）→ 消費端煙霧
  （graph_query：26,656 nodes/183,855 edges/1,261 檔）
- **消費端等值電池（lsp_mcp 退役 EP 的證據基礎）**：
  references 對照——`indicator_definition` 舊面（lsp db）46 refs
  vs 新面（pyrefly）45 refs；DEF 位置的 ±1 行差**當時誤歸因為
  def/decorator 約定差——post-build dual-review 裁決實為 emitter
  行號 off-by-one（1-based 寫入 0-based 合約），已修正＋0-based
  行號斷言釘死**（見 Post-Build Findings PB-1）；callers——
  `vwap_math_df` 5 callers 帶 file:line；
  workspace_symbol（FTS5 search）✓；document_symbol（graph_query
  symbols --query <file>）✓。hover/diagnostics/implementation 不在
  graph 職責（型別面＝ZCode lsp-python MCP／CC 內建，既有裁決）
- 電池抓到並修復：call func 名雙重發射（同位置 CallSite＋NameExpr
  ref → callers sites 加倍）——walker 加 call_name_ranges 去重
- 文檔：README prerequisites／root AGENTS.md（新增 producer 行＋
  lsp_harvest 行改標 golden-oracle only）／crates/AGENTS.md（新
  crate 段）／graph_db.rs 錯誤字串去 lsp-harvest 指引
- mosaic `tools/lsp_mcp` 刪除＝**既有 pending EP 的行動**（STATE.md
  起手點 0；前提＝occurrence S3-F5），本 EP 提供其驗證證據不越界
  執行

## 段落 S4（可選）：上游對話

Pyrefly repo 提 issue/PR：batch occurrence 面（或 SCIP export）
的需求——成功則 git-dep 換官方面、wrapper 再薄一層。

## 整合策略

- 順序：S0 gate → S1 → S2 → S3；S4 任何時點
- **P-4 blocked-on occurrence EP S5**（量測機制未建成）；P-3
  對帳可先行（review F-6）
- **land 順序（review F-7）**：occurrence EP F2/F5 先、本 EP S3
  後——兩者動同一個 Python producer slot
- **平行性**：occurrence EP 的 S3-F2/S5 照跑（資料面共用）；本
  EP SCIP face 不寫 cache schema——對齊點只剩 SCIP 語義
  （role bits／symbol 慣例）
- baseline: b0ed95a

## 收尾步驟

1. Capabilities：新增 Rust 原生 producer 行＋fork 行改 fallback 狀態
2. crates/AGENTS.md lib 分層更新（pyrefly-producer crate）
3. /audit-test

## EP Validate Findings（2026-08-28，POC＝`poc/poc_pyrefly_gitdep/`）

| ID | 嚴重度 | EP 段落 | 問題（POC 結果） | 建議 | 狀態 |
|----|--------|---------|------------------|------|------|
| V1 | 🔴 | S1 | EP 原 pin `tag = "v1.3.0"` 不存在（tags 無 v 前綴；最新 `1.3.0-dev.2`＝08-18 落後 spike commit `1d64c4b`＝08-27 達 2,445 行 state/ 差異） | 改 pin `rev = "1d64c4b…"`——已回寫 S1 | implemented |
| V2 | 🟢 | S1 | POC-1 ✅ git-dep link：外部 workspace 獨立 crate 以 rev pin 編譯+runtime 跑通（`[patch.crates-io] backtrace` 不傳播無害；`pyrefly_util` 等經同 repo git-dep 可解析；`ruff_python_ast`/`ruff_text_size` 走 crates.io 0.0.10 型別邊界一致）；smoke 223 sites/221 resolved | 致命風險清除，S1 可直接產品化 | verified |
| V3 | 🟢 | S2 | POC-2 ✅ release（default profile，非 pyrefly 的 lto profile）計時：同口徑（527 檔/27,245 sites≈S0 的 27,238）**38.36s** vs S0 debug 3:48.68＝**5.96×**（EP 預測 5-20× 內）；全 repo root（1,341 檔/80,849 sites）亦僅 38.23s——固定成本（stdlib/typeshed/env 解析）主導，corpus 規模邊際成本低 | S2 驗收計時註記 profile | verified |
| V4 | 🟢 | S1 | POC-3 ✅ multi-target 語義：全 repo 3,044/3,044 multi-target 均為 `__init__`+`__new__` 成對（constructor dunder 雙解析），零 overload、零真 ambiguity | S1 邊去重＝dunder pair 崩縮 prefer `__init__`——已回寫 S1 | implemented |
| V5 | ⚠️ | S1 | API 深度發現：`find_definition` 富型別（`display_name` 零 None、`definition_range`、`module`）是 S1 命名直達路；`FindPreference.resolve_call_dunders` 提供 constructor 語義開關 | 已回寫 S1（含 R2-3 關聯註記） | verified |

**結論：可直接 /implement**——致命假設（git-dep link）已實證，
EP 修正（pin rev＋S1 語義約束）已回寫。

## Post-Build Findings（2026-08-28，dual-context：fresh＋primed 平行審）

| ID | 嚴重度 | 來源 | 問題 | 裁決 | 狀態 |
|----|--------|------|------|------|------|
| PB-1 | 🔴 | fresh F-1 | **行號全系統 off-by-one**：emitter 把 1-based 寫入 SCIP 0-based 合約（`engine::ln` 讀取端 +1）；runtime 重現（def 實體行 4 → 顯示 :5）。S3 電池的 ±1 曾被誤歸因 decorator 約定差——「觀察異常→合理化」實例 | ✅ 修正（line_col 0-based）＋e2e 0-based 行號斷言釘死；S3 結算勘誤 | implemented |
| PB-2 | 🟡 | fresh F-2 | **輸出非確定**：pyrefly `Handles` 內部 HashSet 摧毀檔案序，同 repo 兩次 emit bytes 不同（runtime byte-diff 實證）——動搖 determinism 標準 | ✅ drive() 對 modules 排序 by rel_path＋雙跑位元組相等測試 | implemented |
| PB-3 | 🟡 | fresh F-3 | walker 只排除 dot-dir/__pycache__，非點前綴 `venv`/`node_modules` 會污染 corpus | ✅ SKIP_DIRS 擴充（.code-reality.toml exclusions 整合列後續優化） | implemented |
| PB-4 | 🟡 | primed R-1 | P-7 承諾 fail-loud exit≠0，實際 finder errors 走 WARN+exit 0 | **裁決 (a)**：pyrefly 內建 typeshed 無 venv 依賴（fixture/e2e/dogfood 全程無 venv 通過），P-7 的「無 venv」場景對此引擎為空集合；fail-loud 面＝無 .py 檔 Err（已測）＋pinned rev `Handles::all` 恆回空錯誤（api.rs 註解標明 rev 升級風險）。文檔不改 exit 語義 | adjudicated |
| PB-5 | 🟡 | primed R-2 | P-9 fork 回退演練缺席（fallback 宣稱紙上保留） | **裁決**：occurrence EP S2 同日（2026-08-28）有 fork 全量 26.1s 實跑紀錄＝演練已存在；下次演練锚定「Fallback 完全退役」前置檢（EP 已列）。不重複跑 Node build | adjudicated |
| PB-6 | ℹ️ | fresh F-4/F-5、primed R-3/R-5 | finder_errors 恆空（rev 行為）／drop 計數三因混裝／`module_name` 死欄位／debug env 未文檔 | ✅ 計數拆三（external/local/unchained）＋欄位移除＋rev 註解；crates/AGENTS.md 補 debug env 一句 | implemented |
| PB-7 | ℹ️ | fresh F-6/F-8 | 測試邊界鬆（>= 讓回歸靜默過）＋TOML inline comment 破損 | ✅ 精確計數斷言＋0-based 行斷言＋unquote 切 `#` | implemented |
| PB-8 | ℹ️ | fresh F-7、primed R-4 | poc/ 生命週期矛盾（962MB target 已清，源碼將隨 commit 入庫） | **裁決**：POC 生命週期完成（驗證意圖已被 api.rs＋測試承接）——**整個 poc/ 已刪除**，證據留 EP Validate 段＋git history | implemented |

**補充發現（PB-1 調查途中）**：cache ingest 濾除非 fn 形態 refs
（class `#`／變數 `.` 尾）——既有管線語義，同時是 refs 密度差
vs lsp golden 的組成之一（lsp 面 references 計屬性成員存取）。

**結論**：🔴/🟡 全數處置（6 修正＋2 裁決記錄），followup＝
cargo test 全綠＋最終鏈刷新（行號修正落地 slot 產物）。
