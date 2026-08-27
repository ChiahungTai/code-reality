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
| cache 三表、`occurrence_roles`（S3-F2）、CALLS pair-set 對帳（S5） | occurrence EP——**engine-agnostic，不重做** |
| producer 引擎（scip-python fork） | 本 EP **接替**——fork 降級 fallback |
| golden corpus（sidecar baseline＋`scripts/golden_corpus.py`） | 共用驗收 oracle |

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
defs／references／call sites → 寫 cache 三表 db＋`occurrence_roles`
（與 occurrence EP S3-F2 的 schema 對齊——**先讀該 EP S3 段落**，
確保 side table 形態一致）＋meta `producer='pyrefly-1.3'`。
- 隔離設計：對 Pyrefly 的每一個 import 集中在單一 `engine.rs`；
  其餘代碼不感知 Pyrefly（升版只動一檔）
- git-dep pin：`tag = "v1.3.0"`（鎖版；升級是顯式 commit）
- 語義約束：symbol 形態自選但須過 `fn_tail_name` 閘門（引擎
  `infer_language` 前綴表若需第三前綴，比照 S3-F1 模式擴充）

**驗證**：小 repo（本 repo scripts/）全量 → `golden_corpus --self`
通過；`graph_db build` 消費成功（SCIP face 或直寫面，擇一並記錄）。

## 段落 S2：mosaic 對帳＋CALLS 量化

**Context**：真實語料驗收。跑 mosaic 全量（時長記錄 vs 40s）；
`golden_corpus` 對帳（bar＝library name coverage ≥96.5%、missing
歸因）；CALLS pair-set 用 occurrence EP S5 機制對照 legacy——
**本 EP 的核心價值證明**：自有 CALLS 來源的品質基準線。

**驗證**：對帳報告落 sidecar；差異逐項歸因（P-6）。

## 段落 S3：cutover＋fork 降級

**Context**：sidecar slot 新增 pyrefly 面（比照 F5 cutover 模式：
明確 evict/migrate＋meta producer 標記，不靜默切）；文檔——
README prerequisites、AGENTS.md Capabilities、scip-python fork 與
patch 標記為 fallback（保留，不刪——回退路）。

**驗證**：消費端（mosaic）驗收一弧；NT 零回歸（Rust 面不動）。

## 段落 S4（可選）：上游對話

Pyrefly repo 提 issue/PR：batch occurrence 面（或 SCIP export）
的需求——成功則 git-dep 換官方面、wrapper 再薄一層。

## 整合策略

- 順序：S0 gate → S1 → S2 → S3；S4 任何時點
- **平行性**：occurrence EP 的 S3-F2/S5 照跑（資料面共用）；本 EP
  S1 的 `occurrence_roles` 寫入面須與其設計對齊（先讀後寫）
- baseline: b0ed95a

## 收尾步驟

1. Capabilities：新增 Rust 原生 producer 行＋fork 行改 fallback 狀態
2. crates/AGENTS.md lib 分層更新（pyrefly-producer crate）
3. /audit-test
