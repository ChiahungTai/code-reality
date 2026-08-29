# EP: Projected Graph Overlay — `code-reality project` 工具生產化

> **ep_type**: implementation
> **baseline**: ff6dafa

把「EP 構想期的投影圖」從 POC（2026-08-29 全綠）生產化：宣告式 projection plan（TOML）→ 單一源鑄 overlay SCIP → cat 真實 index → 投影 slot 查詢 → `[projected]` 標籤報告（graft surface / 新符號反向鏈 / 零邊洞檢測）。消費者：EP 作者 session（段落 0 ripple 證據）與 ep-review（F3 兜底假設路徑驗證機械化）。

## EP Review Findings

雙 agent 審查（F1+F3+dry-run／F2+F4+F5）2026-08-29；judge 全採納（26 findings，含 2 項 judge 裁定設計決策）。

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| 1 | 🔴 | S2 查詢節 | `scip_face::callers` 是虛構函式（callers 實作入口＝cli.rs:640 私有 `callers_mode`） | 改指名既有 pub 原語組合：`cache::open_face`（cache.rs:278）＋`engine::{find_defs,refs_rows,fn_spans}`＋`fndefs::spans_source`＋`callers::attribute`——cli.rs:640-676 主體在 project.rs 重組 | implemented |
| 2 | 🟡 | S2 步驟 5 | cat 先例 `concat_scip`（build.rs:252-258）私有未列 | 錨點補列；升 pub（build 傘形與 project 共用） | implemented |
| 3 | 🟡 | A1 | 否決腿無改動地圖 | 補 graph_db.rs:562/68/747/767 四錨點＋「db 目標必須 thread 至 slot 否則污染真實 graph.db」 | implemented |
| 4 | 🟡 | S2 pseudo | `read_toml` 違 bounded context（code-reality 無法 import producer parser——cycle） | **judge 裁定**：S2 零 TOML 解析——slot 目錄名＝plan 檔名 stem（S2 端 sanitization）；內容（minted defs/edges 去重、claims、meta）全消費 S1 `--report` JSON | implemented |
| 5 | 🟡 | S1/S2 | plan name 無 sanitization（路徑穿越風險，撞非污染 invariant） | name 規則 `^[A-Za-z0-9_-]+$`（slot stem 端驗證）、違反 exit 2；SM-4 補案例 | implemented |
| 6 | 🟡 | S1 pseudo | 逐 symbol `start_module` 會對同檔產 duplicate documents | group by rel_path（每檔一次 start_module）；測試斷言同檔多 symbol→單 document | implemented |
| 7 | 🟡 | A1 | 機制描述與代碼相反：family rule＝「no db → protobuf face（never build on miss）」（cache.rs:273-277） | A1 改寫＋probe 記錄走哪個 face 與 class 符號可達性；SM-6 斷言＝stdout 報告（剝離 stderr WARN）＋slot 重建清除 sidecar | implemented |
| 8 | ℹ️ | 多處 | 錨點行號漂移（cli.rs flags :34-35/--index :261-263；SKILL.md :215-219）＋SKILL.md CLI 列舉現況缺 `build` | 刷新；S3 順手補 build | implemented |
| 9 | ℹ️ | S1 gate | `call_sites` 回傳 warns 被 pseudo 丟棄（parse 失敗會偽裝成 gate 不命中） | warns 非空併入錯誤輸出 | implemented |
| 10 | ℹ️ | S2 | lib.rs 註冊點無行號 | 補（lib.rs:54-82 字母序） | implemented |
| 11 | 🔴 | S3 | root AGENTS.md 三處 bin 列舉會過時（:47-48 兩 bin→三、:88 five→six、:33-34 視 WARN 決策） | S3 更新點清單補齊 | implemented |
| 12 | 🟡 | S3 | `crates/pyrefly-producer/pyproject.toml:8` description 列舉兩 bin | 補第三 bin | implemented |
| 13 | 🟡 | S1 SPEC | overlay-gen 的 `--version` face 與 WARN-wiring 決策未記錄 | **judge 裁定**：帶 `--version`（every-bin 條文 AGENTS.md:29-31）；**不** WARN-wire（spawned backend，pyrefly-lsp 前例）→ :33-34 列舉不動 | implemented |
| 14 | 🟡 | Deferred | 舊 projection slot 累積無處置 | 進 Deferred＋報告尾行印 slot 路徑 | implemented |
| 15 | ℹ️ | A3 | toml 依賴 | workspace 繼承（root Cargo.toml:15 `toml = "0.9"`） | implemented |
| 16 | 🟡 | Deferred | MCP 不做未聲明 | Deferred 第五條：CLI-only（session 內消費；MCP 工具化待跨 session 需求實證） | implemented |
| 17 | 🟡 | S2 驗證 | `rich.scip` defs-only，graft surface 斷言退化 | 真實腿改 `tests/fixtures/rich_callers.scip`（tests/build.rs:277 註明攜 attributed refs） | implemented |
| 18 | 🟡 | S2 claims | to_symbol 不存在時 callers 是 exit 1「查無 DEF」非空集——HOLE 誤報 | 詞彙二分：`[projected][MISSING]`（無 DEF）vs `[projected][HOLE]`（有 DEF 無 from-site）；SM 補場景 | implemented |
| 19 | ℹ️ | S1 needle | schema「首次出現」與 pseudo `nth()` 漂移 | pseudo 改 first_occurrence＋「needle 兩次出現→取首次」測試 | implemented |
| 20 | ℹ️ | SM | 空 plan 無場景 | 補：空 plan → 僅 meta 的空 overlay＋空報告，exit 0 | implemented |
| 21 | ℹ️ | SM | 兩 projection 並存無場景 | 補：兩 name 並存→兩 slot 獨立零干擾 | implemented |
| 22 | ℹ️ | S3 版本 | 版本同步點三處 | root Cargo.toml:7、plugin/.claude-plugin/plugin.json:3、plugin/.mcp.json:6（`want=` 最易漏） | implemented |
| 23 | ℹ️ | S2 | F-02 關聯：`concat_scip` 可見性 | 升 pub crate 內共用（build.rs:252） | implemented |

---

## UC 盤點

### Backlog 關聯
- 掃描 `.kanban/Backlog/`：空（無既有卡）
- 自動建卡結果：新建 2 張——`code-reality-project-overlay.md`（能力卡）＋ `ep-tracking-ep-projected-graph-overlay.md`（EP 整體卡）

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（meta-tooling repo，正當跳過）

### 掃描範圍
- root `AGENTS.md` Capabilities 表（無投影相關行）；`crates/AGENTS.md` lib layering；`.kanban/Backlog/`

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Unified in-repo data plane | ✅ | AGENTS.md Capabilities | 更新 | projections/ 子目錄擴充 slot 慣例（`.code-reality/.gitignore` single-`*` 自動覆蓋——engine.rs:355-369 `write_data_dir_gitignore`，零消費端 setup） |
| Symbol truth query (scip_refs) | ✅ | AGENTS.md Capabilities | 無影響 | `--index` 覆寫（cli.rs:261-263）為既有能力，本 EP 純消費 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| EP 投影圖（projected graph）：宣告式 plan → overlay scip → 投影查詢 + `[projected]` 報告 | 📋 | `crates/code-reality/src/project.rs`（orchestrator）＋ `crates/pyrefly-producer/src/bin/overlay-gen.rs`（鑄造 bin） |

---

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | Happy path：EP 帶 plan+planned sources 跑投影 | `code-reality project --repo R --plan P` | 報告含 graft surface（touched symbol 的 real-only vs +projected callers diff）、新符號 projected callers、`[projected]` 標籤＋假想邊數；尾行印 slot 路徑 | 無 | EP 投影圖 |
| SM-2 | 零邊洞：plan 宣稱整合既有符號但 code 沒接線 | claims 查詢無 from-site 且 to 有 DEF | 報告列 `[projected][HOLE]` finding——**不是錯誤** | 無 | EP 投影圖 |
| SM-2b | claim 指向不存在的符號 | claims 的 to_symbol 在 merged index 無 DEF | `[projected][MISSING]` 行（「宣稱的符號不在 index」）——非 HOLE、非 error | 無 | EP 投影圖 |
| SM-3 | 一致性 gate 失敗：宣告邊與 planned source 不符 | `[[edges]]` needle 在 source 中非 call 行（或 call_sites warns 非空——parse 失敗透傳） | exit 2，逐條列出不符邊＋py_calls warns——fail-loud，不產出半套 overlay | 重修 plan 或 source 再跑 | EP 投影圖 |
| SM-4 | plan 格式錯誤（TOML parse / schema 違反 / 非法 name） | 壞 TOML、缺欄位、未知 kind、name/stem 不符 `^[A-Za-z0-9_-]+$` | exit 2，帶 plan 行號或具體欄位的錯誤訊息 | 無 | EP 投影圖 |
| SM-5 | 前置缺失：真實 index 不存在 / overlay-gen bin 缺 | repo 無 index；PATH 無 bin | exit 2 ＋安裝/前置指引（仿 build 傘行為） | 先跑 `code-reality build` | EP 投影圖 |
| SM-6 | 重跑冪等 | 同 plan 連跑兩次 | 投影 slot 重建（先清 sidecar 再產）、**stdout 報告** byte-identical（stderr WARN 如 face 提示允許差異）；真實 slot 零變動 | 無 | EP 投影圖 |
| SM-7 | graph 世代對照 | plan `meta.graph_rev` ≠ 真實 index stamped meta | 報告 WARN 行——不擋執行 | 重跑投影 | EP 投影圖 |
| SM-8 | 效能期待 | mosaic 級（9.9MB index） | 鑄造秒級；cat 秒級；查詢＝per-symbol protobuf/cache face 讀取（index 只 open 一次——見 S2 查詢節）；全程不需 graph_db build（A1 probe 確認） | 無 | EP 投影圖 |
| SM-9 | 空 plan | 零 symbols/edges/claims | 僅 meta 的空 overlay＋空報告骨架，exit 0 | 無 | EP 投影圖 |
| SM-10 | 兩 projection 並存 | 同 repo 兩個不同 plan 檔 | 兩 slot 獨立、彼此與真實 slot 零干擾 | 無 | EP 投影圖 |

---

## 段落 0：全域研究摘要（POC 2026-08-29 全綠＋結構偵察＋雙 agent 審查查證）

### 可複用基礎設施（全部實證）
- **鑄造 API（producer 側，in-process）**：`IndexEmitter`（`crates/pyrefly-producer/src/emit.rs:16`，`start_module`:39 / `push_def`:69 / `push_call_reference`:104 / `write`:118）；`symbol::{discriminator,def_symbol,target_symbol,pseudo_ctor_symbol}`（symbol.rs:30-95）；`module_of_rel`（producer lib.rs:233）；`walk::{DefKind,DefSite,ScopeEntry}`（walk.rs:9-59）。POC `crates/pyrefly-producer/examples/overlay_gen.rs`（未 commit）——本 EP 畢業為正式 bin 後刪除 example。
- **call-site 語法推導（gate 重用）**：`code_reality::py_calls::call_sites(repo_root, rels)`（py_calls.rs:29）——回傳 `(CallSiteSet, Vec<String>)`，**warns 必須透傳**；producer 已 path-dep 依賴 code-reality（producer Cargo.toml:14）。
- **查詢原語組合（S2 查詢節，Finding 1）**：`cache::open_face`（cache.rs:278）＋`engine::{find_defs,refs_rows,fn_spans}`（engine.rs:140/236/194）＋`fndefs::spans_source`（fndefs.rs:169）＋`callers::attribute`（callers.rs:54）——即 cli.rs:640-676 `callers_mode` 私有主體的 pub 原語版。
- **cat-merge**：`concat_scip`（build.rs:252-258，protobuf 同型訊息串接＋tmp-sibling 原子 rename；mixed-repo 呼叫先例 build.rs:330-337）——目前私有，S2 升 pub crate 內共用。
- **bin spawn 邊界**：`resolve_bin`（build.rs:142-154）＋`producer_roots`（build.rs:156-167）——「process spawn is the only legal coupling」（build.rs:4-6、crates/AGENTS.md:75-77）。
- **測試 fixture**：`tests/fixtures/rich_callers.scip`（攜 attributed refs——graft surface 斷言用）；`tests/fixtures/rich.scip`（defs-only，僅供其他斷言）。

### 依賴關係和關鍵約束（附證據）
- **循環依賴硬約束**：producer→code-reality path dep（Cargo.toml:14）；反向即 cargo cycle。⇒ 鑄造住 producer bin、orchestrator spawn 消費；**plan schema 知識全封 S1**，S2 零 TOML 解析（Finding 4 裁定）。
- **SCIP 無 call role**：CALLS 拆分靠讀源碼 (file, 1-based line, callee name)（py_calls.rs 檔頭）⇒ gate 免費。
- **Symbol ID 單一源**：一律經 symbol.rs 構造，禁手寫。
- **B7b 配對必要**（POC 實證）：ctor call ref 無 pseudo-ctor DEF 過不了 def-symbol gate。
- **cache face 語意**（cache.rs:273-277）：no sidecar → protobuf 全量 face（never build on miss）；sidecar 存在且 stale → 自動重建寫出（cache.rs:309-319）；sqlite face 有 fn-tail gate（cache.rs:64-65）而 protobuf 全量——class/ctor 查詢行為 face 有別，A1 probe 記錄。

### 風險假設
| ID | 假設 | 等級 | 驗證 |
|----|------|------|------|
| A1 | merged index 不經 graph_db build，`scip_refs --index` 直接可查 | 高（S2 go/no-go） | **S1 前置 probe**：POC 產物複製到淨 tempdir（無 sidecar）跑 callers 查詢；記錄①走哪個 face（stderr）②checkpoint graft site 在場③class/pseudo-ctor 可達性④sidecar 是否被自動寫出。**方向已由 cache.rs:273-277 family rule 支持（protobuf face on miss）**；probe 為行為釘死。若否決→S2 加 graph 腿：`build_from_cache_at`（graph_db.rs:444）＋db-path threading 四點（graph_db.rs:562 寫入/:68 materialize/:747 ensure/:767 consumer_db）——**db 必須 thread 至 slot 否則污染真實 graph.db** |
| A2 | overlay-gen 輸出 byte-deterministic | 中 | S1 測試（兩次鑄造 bytes 相等；IndexEmitter 順序由 Vec push 決定無 HashMap 洩漏） |
| A3 | producer 需 toml 依賴 | 低（已證） | `toml = { workspace = true }` 繼承（root Cargo.toml:15 `toml = "0.9"`） |
| A4 | `[projected]` 被誤讀為證據 | 中 | 協定（標籤＋假想邊計數行）＋S2 測試斷言標籤在場＋skill 措辭（S3） |

### 死路假設嫌疑
- 無（POC 已全鏈實證；A1 是範圍變數非死路）。

---

## 段落劃分原則

垂直切片：S1（producer 側鑄造＋gate）→ S2（orchestrator＋查詢＋報告，消費 S1 bin 與 `--report`）→ S3（文檔＋收尾＋版本 staging）。語義約束：plan TOML schema 與 report JSON 格式由 S1 單一定義；`[projected]` 詞彙（graft/HOLE/MISSING/假想邊數）S1 report→S2 輸出→S3 文檔三處一致。

---

## S1: `overlay-gen` bin（pyrefly-producer crate）

### Context
- **UC 引用**：實作「EP 投影圖」鑄造半邊。
- **依賴關係**：S2 以 bin spawn 消費（含 `--report` JSON）；本段不依賴 S2。
- **語義約束**：report JSON 欄位（minted defs/edges、dedup 的 to_* 清單、claims 透傳、meta）＝S2 消費契約。
- **基礎設施盤點**：見段落 0；新增依賴僅 `toml`（workspace 繼承）。
- **依賴錨點**：
  - `IndexEmitter` → 定義 `crates/pyrefly-producer/src/emit.rs:16` / 消費＝新 bin（example overlay_gen.rs:14/110 吸收）
  - `symbol::def_symbol` → `symbol.rs:49` / 消費＝新 bin
  - `py_calls::call_sites` → `crates/code-reality/src/py_calls.rs:29` / 消費＝新 bin（gate；warns 透傳）
  - `report_stale_binary` → 對照組 `src/bin/pyrefly-index.rs:13`（overlay-gen **不**用——spawned backend 前例 `pyrefly-lsp`）
  - Cargo.toml path dep → `crates/pyrefly-producer/Cargo.toml:14`
- **技術選型**：bin `overlay-gen.rs`（與 pyrefly-index.rs/pyrefly-lsp.rs 命名一致）；plan 格式 TOML；`--version` face（every-bin 條文）。
- **成功標準**：POC 場景產出語義等價 scip；gate 不符 exit 2 逐條列；`--version` 印 `<pkg>+<rev>`。

### Invariant Impact
無（meta-tooling fail-loud）。

### 核心實作要點
- 檔案：`crates/pyrefly-producer/src/bin/overlay-gen.rs`（單檔）
- Plan TOML schema v1（單一源＝本 bin 的 parser 結構體）：
  ```toml
  [meta]
  name = "checkpoint-coordinator"      # 報告標籤用（非 slot 名）
  graph_rev = "<real index stamped rev>"  # 缺省 = "unstamped"

  [[symbols]]
  rel_path = "mosaic_alpha/structure/checkpoint_coordinator.py"
  kind = "class" | "function"
  name = "CheckpointCoordinator"
  scope = []                           # 外→內 enclosing 名（class 語意由 S1 判定為 is_class）

  [[edges]]
  file = "<rel_path>"
  needle = "aggregate_trend_runs(legs)"   # source 中首次出現定位
  to_module = "mosaic_alpha.structure.trend_run"
  to_kind = "function" | "class"           # class → B7b pseudo-ctor 配對
  to_name = "aggregate_trend_runs"

  [[claims]]
  to_module = "..."
  to_kind = "..."
  to_name = "..."
  note = "..."
  ```
- 鑄造：**group by rel_path**——每檔一次 `start_module`，檔內逐 symbol `push_def`（Finding 6）；sym ID 經 `def_symbol`/`target_symbol` 單一源；ctor 邊自動 pseudo-ctor DEF+REF 配對。
- **一致性 gate**：`call_sites(sources_dir, rels)`；每條 edge 以 **first_occurrence**(needle) 定位 → 必須命中 (file, line, to_name)；**warns 非空 → 併入 exit 2 輸出**（防 parse 失敗偽裝 gate 不命中）；gate 不通過**不寫 out**。
- SPEC：`--plan <toml> --sources <dir> --out <scip> [--report <json>] [-h] [--version]`（無 WARN-wire）。
- Report JSON：minted defs/edges 計數、edges 的 to_* 去重清單（供 S2 graft 查詢）、symbols 清單（供反向鏈）、claims 透傳、meta。

### Pseudo Code
```text
main(argv):
  SPEC: --plan --sources --out [--report] [-h] [--version]
  plan = toml parse（錯 → fail(2) 帶行號；name 非 ^[A-Za-z0-9_-]+$ → fail(2)）
  srcs = load 涉及 rel_path 檔（缺 → fail(2)）
  (sites, warns) = py_calls::call_sites(sources_dir, rels)
  for edge in plan.edges:
      pos = first_occurrence(source_of(edge.file), edge.needle)   # 找不到 → fail(2)
      require (edge.file, line_of(pos), edge.to_name) ∈ sites    # gate
  if gate fails or warns non-empty: exit 2 列宣告 vs 實際＋warns（不寫 out）
  em = IndexEmitter::new()
  for (rel, syms) in group_by(plan.symbols, rel_path):           # 每檔一次 start_module
      em.start_module(rel, srcs[rel]); for s in syms: em.push_def(def_symbol(...))
  for edge: em.push_call_reference(target_symbol(...))            # ctor → pseudo 配對
  em.write(out); write report json
  print "[OK] overlay-gen: N defs, M edges, gate P/P -> out"
```

### 驗證策略
- **前置 probe（A1）**：見風險假設表——結論（face/可達性/sidecar 行為）回寫本 EP 後 S2 才動工。
- 單元測試（bin 內 `#[cfg(test)]` 或抽 lib 模組供測）：schema 錯誤帶行號；非法 name；gate 失敗輸出（含 warns 透傳）；同檔多 symbol → 單 document；needle 兩次出現取首次；B7b 配對；byte-determinism。
- 整合（`crates/pyrefly-producer/tests/overlay_gen.rs`，底線命名隨 end_to_end.rs 慣例）：tempdir sources＋plan → 讀回 scip 斷言 symbol IDs（POC ID 為 golden）。
- Example `examples/overlay_gen.rs` 刪除（吸收）。
- 已知未覆蓋：rust 語言 plan。

---

## S2: `code-reality project` orchestrator＋查詢＋`[projected]` 報告

### Context
- **UC 引用**：實作「EP 投影圖」編排與報告半邊。
- **依賴關係**：spawn S1 bin（`--report` 為內容契約）；查詢用 in-process pub 原語組合。
- **語義約束**：與 S1 共享 report JSON 契約與 `[projected]` 詞彙；S2 **零 TOML 解析**（slot 名＝plan 檔名 stem，`^[A-Za-z0-9_-]+$` 驗證，非法 → fail(2)）。
- **基礎設施盤點**：`resolve_bin`/`producer_roots`（build.rs:142-167）；`engine::default_index_path`（engine.rs:332）；`engine::meta_path`（engine.rs:339）；`concat_scip`（build.rs:252，升 pub）；查詢原語組合（段落 0）。
- **依賴錨點**：
  - `route()` → `crates/code-reality/src/bin/code-reality/main.rs:28-66` / 消費＝新增 arm
  - `SUBCOMMANDS` → `main.rs:68-85`（`[&str; 16]`→17）
  - `pub mod project` → `crates/code-reality/src/lib.rs:54-82`（字母序插入）
  - `default_index_path` → `engine.rs:332` / 消費＝project.rs
  - `resolve_bin` → `build.rs:142` / 消費＝project.rs
  - `concat_scip` → `build.rs:252` / 消費＝project.rs（升 pub，build.rs 自身呼叫點 :330 不動）
  - 查詢原語 → `cache.rs:278`、`engine.rs:140/194/236`、`fndefs.rs:169`、`callers.rs:54` / 消費＝project.rs（重組 cli.rs:640-676 callers_mode 主體；**index 每 face 只 open 一次**，per-symbol 重用 handles）
- **技術選型**：slot `<repo>/.code-reality/projections/<plan-stem>/`；plan root＝`--plan` 檔所在目錄（`sources/` 子目錄）。
- **成功標準**：SM-1/2/2b/5/6/7/9/10 全過；mosaic 級端到端。

### Invariant Impact
無（會計/風控/silent-corruption 不觸及）。**非污染 invariant**：真實 slot（index.scip/graph.db/sidecar）執行前後 bytes 不變＋slot name sanitization 防路徑穿越——測試釘死。

### 核心實作要點
- 檔案：`crates/code-reality/src/project.rs`（SPEC/HELP/run/`project_repo` 可注入核心）＋三點註冊。
- Pipeline：
  1. SPEC：`--repo R --plan P [--json]`
  2. 真實 index 缺 → fail(2)＋「先跑 code-reality build」指引
  3. slot＝`projections/<plan stem>`（stem 驗證）；**重建＝先清 slot 內 sidecar/產物再產**（冪等）
  4. spawn `overlay-gen --plan --sources <plan_root>/sources --out slot/overlay.scip --report slot/overlay-report.json`（bin 缺 → fail(2)＋指引）
  5. `concat_scip(real_index, overlay.scip) → slot/index.scip`（pub 化重用）
  6. **查詢（A1 probe 已確認可行；原語組合）**：對 real index 與 slot index 各 open face **一次**；per symbol 查 callers：touched real symbols（report 的 to_* 去重）＝graft surface；plan symbols＝反向鏈；claims＝HOLE/MISSING 二分（to 無 DEF → MISSING；有 DEF 無 from-site → HOLE——MISSING 需先 `find_defs` 判存）
  7. 報告：graft surface（real-only vs +projected diff，各附 file:line）、新符號反向鏈、`[projected][HOLE]`/`[projected][MISSING]` 行、`[projected]` 前綴＋「假想邊 M 條（宣告，非證據）」計數、graph rev 對照 WARN（`engine::meta_path`）、**尾行印 slot 路徑**（自清提示）
- 錯誤語意：Env fail(2)；Core crash(1)。

### Pseudo Code
```text
run(argv): SPEC --repo --plan [--json]
project_repo(repo, plan_path):
  real = default_index_path(repo)?                 # 缺 → Env fail(2)
  stem = sanitize(plan_stem)                       # 非 ^[A-Za-z0-9_-]+$ → fail(2)
  slot = repo/.code-reality/projections/<stem>/    # 清 sidecar 後重建
  spawn overlay-gen(plan, plan_dir/sources, slot/overlay.scip, slot/overlay-report.json)
  concat_scip(real, slot/overlay.scip) -> slot/index.scip
  rep = json(slots/overlay-report.json)            # minted/to_*/symbols/claims/meta
  real_face = open_face(real); proj_face = open_face(slot/index.scip)   # 各一次
  report = ProjectedReport::new(rep.meta)
  for to in rep.touched_symbols:                   # graft surface
      report.graft(to, diff(callers(real_face, to), callers(proj_face, to)))
  for sym in rep.symbols: report.reverse(sym, callers(proj_face, sym))
  for claim in rep.claims:
      if find_defs(proj_face, claim.to) empty: report.missing(claim)
      elif no from-site: report.hole(claim)
  report.rev_check(rep.meta.graph_rev, stamped_meta(real))
  print report; print "slot: <slot path>"
```

### 驗證策略
- 整合測試 `crates/code-reality/tests/project.rs`（仿 tests/build.rs：tempdir mini-repo＋checked-in fixture）：
  - **真實腿用 `tests/fixtures/rich_callers.scip`**（attributed refs——graft surface 斷言有效）＋`tests/fixtures/proj-plan/`（plan.toml＋sources/）
  - 斷言：graft 欄位、`[projected]` 標籤在場、HOLE 與 MISSING 行（兩案例都造）、usage/exit-code、非污染（真實 slot bytes 前後相等）、冪等（stdout 剝離 stderr 後相等）、兩 name 並存零干擾、bin 缺 → fail(2) 指引
- 已知未覆蓋：graph face（deferred）；duplicate rel_path（deferred）。

---

## S3: 文檔＋收尾＋版本 staging

### Context
- **UC 引用**：收斂「EP 投影圖」UC（Capabilities ✅ 行）。
- **依賴關係**：S1/S2 完成後。
- **語義約束**：skill 措辭＝S2 報告詞彙。
- **基礎設施盤點**（更新點全清單）：
  - `plugin/skills/code-reality/SKILL.md`：CLI surface 節（:215-219）列舉加 `project`＋**順手補現況缺的 `build`**；新增 projection 小節（plan 格式、`[projected]`=宣告非證據的洗衣陷阱警告）
  - `crates/AGENTS.md`：lib layering 加 `project`（orchestration leaf，仿 build 條目）＋producer 條目提 overlay-gen
  - `root AGENTS.md`：Capabilities 新行＋**安裝段 :47-48（pyrefly-producer → 三 bin）＋bootstrap 行 :88（five→six bins）**（:33-34 WARN-wired 四 bin 列舉不動——overlay-gen 不 WARN-wire）
  - `crates/pyrefly-producer/pyproject.toml:8`：description 補 overlay-gen
- **成功標準**：文檔零漂移列舉；kanban Done；版本 staged。

### Invariant Impact
無。

### 核心實作要點
- Capabilities 行：`| EP 投影圖（projected graph）| code-reality project --repo <repo> --plan <plan.toml> | ✅ |`
- 版本三同步點：root `Cargo.toml:7`（workspace 0.4.1→0.5.0）、`plugin/.claude-plugin/plugin.json:3`、`plugin/.mcp.json:6`（`want=`）；dist 再生。**commit/tag 待 user 確認**（outward gate）。
- kanban 兩卡 Backlog→Done；記憶回寫 cr-projected-graph-ep-overlay.md。

### Pseudo Code
（文檔段——修改要點如上）

### 驗證策略
- `rg "project"` 三文檔命中語義一致；`rg "overlay-gen"` 文檔/描述命中；`rg "\[projected\]"` 三處詞彙一致（S1 report/S2 輸出/S3 文檔）。
- `/audit-test` 於 S1/S2 測試套。

---

## 明確 Deferred（負空間，防 scope creep）
1. **既有檔內計畫 call site**（duplicate rel_path）：cat 兩份同路徑 document 的 build 行為未驗（POC 開放縫）。零邊洞已從反側覆蓋宣稱面；正向模擬另案。
2. **graph face 查詢**（impact_radius/flows on projection）：檔案面需絕對路徑且價值弱於 graft surface；MVP scip face 已足。
3. **ai-rules 接線**（layer 旗標觸發、ep-review F3 消費 `[projected]` findings）：ai-rules 側另案 handoff。
4. **rust 語言 plan**：Python face only。
5. **MCP 工具面**：project 不進 MCP（session 內 CLI 消費；MCP 工具化待跨 session 需求實證——CLI/MCP 單源慣例下的明示偏離）。
6. **projection slot GC**：舊 slot 累積手動自清（報告尾行已印路徑）；`--prune` 待需求實證。

## 整合策略
- 整合點＝S2 pipeline 全鏈（S1 bin↔orchestrator↔查詢原語）；S3 收斂文檔。
- 整合測試：tests/project.rs 端到端（fixture 真實腿＋spawn workspace 內建的真 overlay-gen bin）。
- baseline: ff6dafa（implement 階段 1 重驗）。

## 實作紀錄（build session 2026-08-29 深夜）

**A1 probe 結論**：全綠——淨目錄（無 sidecar）merged index 直接可查（protobuf face on miss, never build）；graft site 在場；class/pseudo-ctor 可達；**零 sidecar 自動寫出**。S2 定案零 graph_db 依賴（db-path threading 不需要）。

**偏差與發現（EP 是收斂方向非合約）**：
1. **[重大發現] 相對 `--repo` 靜默丟光跨模組 refs**——pyrefly engine 回傳絕對 module path，producer 的 corpus `strip_prefix` 對相對 root 必敗→全部掉 external。這解開了 POC 期「複製品解析全失」八實驗之謎（全用相對路徑；真實 mosaic 一直用絕對路徑所以正常）。修復＝`emit()` 入口 canonicalize（pyrefly-producer lib.rs），regression 釘在 `pyrefly_index_relative_repo_resolves_cross_module_refs`。**POC 期對 pyrefly「深黑盒環境耦合」的判決要修正：根因是參數形態，不是 venv/MAPPING。**
2. plan `[meta]` 補 `project`/`version` 欄位（EP schema 漏列——symbol ID 前綴需要 pyproject identity；POC 已知但 schema 未帶入）。
3. `scope` 條目形態定為 table `{name, class}`（描述子 join 需要 class-ness）。
4. ctor 邊（to_kind=class）限 planned class（B7b 配對需要 DEF 在 overlay 內）——真實 class ctor 邊列為 MVP 邊界，parse 期 fail-loud。
5. S2 查詢層用 `engine::load_index`＋`engine::fn_spans`（純 protobuf face）而非 cli 的 face ladder——規避 sqlite fn-tail gate 對 class DEF 的行為差異＋保證零 sidecar 寫出（非污染 invariant）；`concat_scip`/`producer_roots` 升 `pub(crate)`。
6. report 格式 TOML（雙邊 toml crate，零 serde derive）；`overlay_files` 欄位新增（HOLE 判定需要計畫檔集合）。
7. 測試分層：producer 側 real-engine e2e（CARGO_BIN_EXE 兩 bin）；code-reality 側 fake-bin 注入（tests/build.rs 慣例）＋guarded real-bin e2e（target/release 在場才跑——env-coupled skip-on-drift）。
8. 順手修（預先存在）：無。`build.rs BuildError::msg` dead 為 HEAD 既有（標記不刪）。

## Post-Build 審查回寫（2026-08-29 雙 agent：fresh＋primed）

無 🔴。採納並落地：CR-1 report 攜 module＋callers 查詢加 `` `module`/ `` 前綴過濾（同名符號跨模組污染修正）；CR-2 [[claims]] 拒絕 class 目標（B7b 限制下無法 WIRED，parse 期 fail-loud）；CR-3 symbols 宣告順序單調驗證（underflow panic → plan 錯誤）；CR-7 B7b 配對加 module 一致；CR-6 graft/reverse 的 MISSING verdict 進 JSON＋rev_warn 欄位；CR-11 JSON 風格對齊 common::to_json_indent1＋shadow 修正＋dedup 清理。文檔：producer/root README 補 overlay-gen、SKILL.md 命名鑄造 bin＋「real slot 不動」措辭精確化＋needle/symbol 定位 caveat（CR-8/9）。Fixture 溯源（R-1/CR-4）：corpus 同步 untouched_helper＋fixtures/proj-plan/README.md 重生指令＋三 fixture 重生。不採納：CR-5（guarded e2e 的行為斷言已是 loud 防線）。補測（R-2）：needle 首 now occurrence gate、document 計數、空 plan、rev WARN、spawn 失敗腿、run() arg faces。

記錄修正：F-17 實際採用新鑄 `proj_real_leg.scip`（真 pyrefly engine 產物、attributed refs 意圖達成）而非 `rich_callers.scip`（rust-scheme 不符 python overlay 別名需求）——原 EP 未記此替換，本節補記。語義約束「`[projected]` 三處一致」實為兩處（S1 report 不攜 verdict，判定全在 S2）——措辭修正。

## 收尾步驟
1. Capabilities ✅ 行＋kanban Done（原子操作）
2. SYSTEM-MAP：N/A
3. instruction 檔同步（S3 已含）
4. `/audit-test`
