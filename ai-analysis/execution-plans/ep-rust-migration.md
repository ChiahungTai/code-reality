# EP：code-reality Rust-based 遷移（共存後刪除舊的）

> **ep_type**: blueprint（七段皆中型變更，逐段衍生 implementation 子 EP）
> **spec**: [ai-rules ai-analysis/specs/code-reality-repo-mcp-spec.md](../../../ai-rules/ai-analysis/specs/code-reality-repo-mcp-spec.md)（D1-D5／UC×7／SM×10——**已鎖決策本 EP 不重辯**；本 EP 改寫的只有**實作路線**：Python → Rust，user 2026-08-25 拍板「重新寫 ep 因為要改成 rust based」＋「參考 nt v1 怎樣做的，達到共存後刪除舊的」＋「mcp 要支援開啟一個 http 支援多個 repo」；**D3 消費形態隨 Rust 同步**：`uv run --project` → `cargo install --path`〔仍 git-path 本地形態——路線必然結果，非重辯〕）
> **研究背景**: [ai-rules ai-analysis/reports/rust-precision-ecosystem-research.md](../../../ai-rules/ai-analysis/reports/rust-precision-ecosystem-research.md)（caller 邊機制 96.9% E2E＋生態掃描＋§7 限制）
> **前 EP**: [ai-rules ai-analysis/execution-plans/ep-code-reality-repo-mcp.md](../../../ai-rules/ai-analysis/execution-plans/ep-code-reality-repo-mcp.md)（S1 ✅ 完成於 baseline `2eafd8a`；**S2/S3 由本 EP R3/R6 Rust 原生取代——Python 版永不建**；**S4 吸收進 R7**）
> baseline: `2eafd8a`（code-reality v0 initial import）

## 實作總覽

v0 Python（6,208 行＋381 tests）為**凍結的 parity oracle**；Rust workspace 落地後逐工具族遷移，每族過 byte-identical parity gate，終局一次 relay 切換消費端並**刪除兩份 Python**（code-reality 本體＋ai-rules 舊副本）。

| 段 | 內容 | 取代/吸收 | UC |
|----|------|----------|-----|
| R1 | Rust workspace 落地（repo 結構調整，Python 零改動） | — | 基礎設施 |
| R2 | scip 家族 Rust 化（解析/cache/查詢/audit） | scip_refs 重寫 | UC-1/UC-4 載體換軌 |
| R3 | caller 邊＋closure（Rust 原生） | 舊 EP S2（Python 版永不建） | UC-2（核心） |
| R4 | graph.db 家族＋hazard | snapshot/transition/hub_refs/hazard/graph_audit/graph_csv 重寫 | UC-4/UC-6 載體換軌 |
| R5 | boundary＋tour＋runtime_edges 家族 | 五工具重寫 | 隨遷工具族載體換軌 |
| R6 | MCP server（單一 HTTP、多 repo） | 舊 EP S3（rmcp 取代 FastMCP） | UC-5（核心） |
| R7 | 終局 relay＋雙副本刪除＋文檔收斂 | 舊 EP S4 吸收擴大 | 全部收斂 |

## 架構決策（深思考結論，子 EP 繼承）

### AD-1 repo 結構：nt_v1 共存模式

root 落 `Cargo.toml`（workspace members=`crates/*`）＋Rust 工具鏈檔；**Python 套件留在 `code_reality/` 原 position 不動**（canonical import path `python -m code_reality.<tool>` 全程有效——消費端零破壞）；R7 一次刪除。參考實證：`~/Github/nt_v1` 一個 repo 內完整展示三態並存（root Cargo workspace＋`crates/`、root `nautilus_trader/` 舊態、`python/nautilus_trader/` 新態殼）。

**與 NT 的關鍵差異**：NT 是 Python library（使用者策略碼 import）→ 端局必須留 PyO3 殼；code-reality 消費形態是 **subprocess CLI＋MCP 工具**（消費者只依賴 process 邊界：argv/stdout/exit codes）→ **端局純 Rust、Python 可全刪、免綁定層**。這是本遷移比 NT 簡單的根本原因。

### AD-2 lib-first：MCP 進程內直連，CLI 與 MCP 共享同一 lib

Rust crate 佈局＝一個 lib（domain 純函數＋query 編排＋adapter：rusqlite/scip/toml/fs）＋兩個薄 frontend（umbrella CLI bin `code-reality <sub>`、MCP bin）。**「CLI＝MCP 唯一後端」的語義從 process 邊界（舊 S3 spawn 薄殼）升級為 lib 邊界**——兩 frontend 呼叫同一 lib 格式化函數，drift 在編譯期不可能。

- 舊 spawn 決策是 **Python 形態的補償**（editable freshness＋~1s venv 啟動成本）；Rust 二進位無此問題。
- **代價（必須刻意設計）**：spawn 形態的 per-call fault isolation 是免費的（子進程崩=隔離）；進程內形態必須以 **per-request `Result` 邊界**重建——工具 handler 回 Err＝該請求 loud error，daemon 存活；process 級 crash 保留給真 invariant 違反。子 EP R6 必測（單 repo 髒 sidecar 不毒化其他請求）。
- SM-10 保鮮策略改寫：**install＋kickstart 升級流程**（`cargo install --path` → `launchctl kickstart`；launchd KeepAlive 既有的 V7 形態沿用），取代 spawn 免重啟。
- **panic 語義**：資料驅動 panic（髒 artifact 上的 unwrap/索引越界）不是 `Err`——handler 邊界以 `catch_unwind`（或 per-request task）把 panic 映射為請求級 loud error，daemon 存活；KeepAlive 是後備非設計（SM-14 延伸兩案）。

### AD-3 單一 HTTP 常駐、per-call 多 repo（原始設計，rmcp 載體）

**當初這樣設計的理由**（理由 1/5/6＝spec 原文；理由 2/3/4＝前 EP 段落 0／環境事實考古——出處錨定非杜撰；子 EP 不重辯）：

1. **路由負擔內化**（spec 痛點原文：「三接口把路由負擔壓在 LLM 認知上」）——單一 server 讓 repo 成為**參數**而非**拓撲**，LLM 不選 server、只給 `repo_root`；缺參 loud error（SM-5）。
2. **solo 單機運維經濟**——一個 launchd daemon/port/plist/log（CRG 前例 port 5555、124MB log 教訓的 ThrottleInterval/log 慣例）；per-repo server = N 份生命週期管理，不成立。
3. **工具面預算**——MCP tools 是 per-session 快照注入，單 server × 少工具 vs 多 server × 各自工具清單；新 session 才見新工具，server 越少 re-mount 越少。
4. **工作現實是跨 repo**——同一天同一 session 問 NT/mosaic/ai-rules/code-reality 自己；repo 無狀態查詢（讀 on-disk artifacts）天然不需要 workspace 綁定。
5. **與 lsp_mcp 的本質分界**（D4 維持）——lsp_mcp 持 workspace 常駐狀態必須獨立；code-reality 是無狀態查詢服務，單 server 多 repo 零損失。
6. **UC-7 鋪路**——「可寫進 plugin manifest 的一條命令」天然要求一個命令服務全部 repo。

**rmcp 帶來的更好作法（採用）**：AD-2 進程內直連＋tokio 並發（跨 repo 請求天然並行；rusqlite `Connection` 為 `!Sync` → per-call 連線或 pool，子 EP 設計）＋內建 streamable-http session 管理。**考慮過且拒絕**：MCP protocol `roots` 機制（客戶端通告 workspace 給 server）——客戶端支援不一致、單 root 假設承載不了同 session 多 repo 現實、隱式綁定製造靜默查錯 repo 風險；維持 `repo_root` 顯式必填。

**HTTP 而非 stdio**：launchd 常駐跨 session 共用＋ZCode/Claude 同 URL 多客戶端（user-level mount 形態 `{"type":"http","url":...}`）；stdio＝每客戶端起一進程，無常駐共享。

### AD-4 單次 relay（不執行舊 S4 的中繼）

消費端現況：**ai-rules 舊副本仍在且是現役服務面**（skills 指 `--project ~/Github/ai-rules`，機械驗證 rg）。Rust 遷移期間兩份 Python 全凍結（v0=parity oracle、ai-rules 副本=現役面，S1 已驗 byte-identical）→ 消費端**一次**切到 Rust binary（R7），不先切 code-reality Python。理由：兩次 relay=兩輪 NT 側 byte-compare＋三 repo 文檔面改寫；中繼路徑教會消費者一個即將再變的路徑。**逃生口**：若 Rust 時程滑落過久，舊 EP S4（ai-rules→code-reality Python relay）可獨立執行（規格仍在 ai-rules 前 EP），R7 改為第二次 relay——屆時 Ask First 重確認。**中繼 wording re-base**：逃生口若執行，SM-11 與 R7 步驟②的「ai-rules 舊路徑」表述改指 code-reality Python 面。

### AD-5 parity harness＝第一級遷移基礎設施

`tests/parity/`（pytest 編排，R2 建立、R3-R5 擴充）：同一輸入跑 Python `uv run python -m ...` vs `cargo run -- ...`，`cmp` stdout 位元組＋exit codes。輸入集＝既有 synthetic fixtures＋NT 真索引（18 refs 兩形態／audit／`graph_audit --json`）＋mosaic 真圖（hazard 差動）。**gate 判準＝與本地 Python 輸出 `cmp` 位元組相同＋exit codes 一致（自我相對）**；138/861/430/233/861 為 NT 機器參考值（本地 graph.db 缺席時兩版同 fail-loud 亦為有效等價——前 EP 條款）。**Python 測試套（381）是語意 oracle，parity harness 是跨語言位元組 oracle；R7 隨 Python 一起退役**（退役後 Rust cargo tests 為唯一測試面）。

## 硬約束（spec 繼承＋Rust 化增補）

- **NT 治理鉤子 CLI 契約**：`--json` 鍵／exit codes（0 命中／1 未命中／2 環境錯誤家族）／stdout 位元組——跨語言 parity gate 逐段驗證，**永不靜默破壞**；刻意演化（如有）走 relay＋NT 側同步（Ask First）。
- **`[SRC]` 溯源**：有 stamp 必帶；loud error 回應本質無 `[SRC]` 屬邊界允許（spec 條款）。
- **sidecar home 凍結**：`~/.mosaic/code-reality/`（slot/stamp/三表 sqlite schema＋`SCHEMA_VERSION` 三守衛）；Rust 寫的衍生 cache 必須與 Python 互通（共存期雙向可讀，schema 演化走既有 stale-重建語義）。
- **repo 事實歸 repo**：`.code-reality.toml` schema 凍結（toml crate 解析＋crash-only 驗證語意對齊）；工具層零 repo 特例。
- **存在性述語**：ai-rules skills 以 `.code-reality.toml` 在場 **OR** 工具 `--help` exit 0 判可用——Rust umbrella bin 維持 `--help` exit 0 家族。
- **lsp_mcp／CRG MCP（PyPI @2.3.8）不動**（D4／雙活條款）；v1+ 展望不變。
- 每回應帶 `[SRC]`（MCP 面）；crash-only 哲學在 Rust 的對應：**per-request loud error**（AD-2），輸入無效即拒，無修復路徑。

## UC 盤點

### 掃描範圍
- 本 repo `AGENTS.md` Capabilities（6 條）；`.kanban/` 不存在（本 EP 產出時建立）；`SYSTEM-MAP.md` 不存在（不適用）；ai-rules 側 `.kanban/`（前 EP 卡：追蹤卡已移 In-Progress、兩能力卡在 Backlog——R7 relay 時收斂，非本 repo 職權）

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| 符號真相查詢（refs/defs，trait 消歧）＝UC-1 | ✅（Python v0） | root AGENTS.md Capabilities | **載體換軌** | R2 Rust 化；能力語意不變，parity gate 把關 |
| 完整度治理（audit＋`[SRC]`）＝UC-4 | ✅ | 同上 | 載體換軌 | R2（scip_refs --audit）＋R4（graph_audit） |
| 可刪判斷安全網（hub_refs/hazard）＝UC-6 | ✅ | 同上 | 載體換軌 | R4；hazard AST port＝最高風險項（見風險清單） |
| boundary／export／narrative 工具族 | ✅ | 同上 | 載體換軌 | R5 |
| caller 邊查詢（callers/closure）＝UC-2 | 📋（舊 S2） | 同上 | **實作路徑改 Rust** | R3；Python 版永不建 |
| 單一 MCP 接口＝UC-5 | 📋（舊 S3） | 同上 | **實作路徑改 Rust** | R6（rmcp）；架構決策 AD-2/AD-3 |
| 圖級查詢（impact/communities/hub/dead-code）＝UC-3 | 📋 v1+ | spec UC 定位 | 無影響（遞延不變） | v1+ 展望承載（B 裁決＋SCIP 注入後上 MCP；UC-6 資料源升級同段） |

### 新增 UC
無（能力集不變——本 EP 是實作路線換軌；parity harness 為內部基礎設施，記 Kanban 不入 Capabilities）。

### Backlog 關聯＋建卡結果
自動建卡（本 EP 產出時執行）：EP 追蹤卡 `ep-rust-migration.md`＋能力卡 `rust-caller-edges.md`（UC-2）＋`rust-mcp-server.md`（UC-5），共 3 張新建於本 repo `.kanban/Backlog/`。

## Scenario Matrix

**能力面 SM×10 繼承 spec 原文**（SM-1 trait 消歧 caller／SM-2 closure BFS 環偵測／SM-3 索引過期 drift WARN／SM-4 無索引 loud error exit 2／SM-5 缺 repo_root loud error／SM-6 single-line span 巨集 fn／SM-7 NT 位元組級不變／SM-8 Python repo 圖查詢 v1／SM-9 closure 秒級按檔聚合／SM-10 保鮮——**語意全數不變**；對應段：SM-1→R2/R3、SM-2/6→R3、SM-3/4→R2、SM-5→R6、SM-7→R2-R5 parity＋R7、SM-8→R4（mosaic 驗收）、SM-9→R3、SM-10→R6；SM-10 策略改寫見 AD-2）。**遷移面新增**：

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應段 |
|---|------|------|---------|------------|--------|
| SM-11 | 共存期消費端夜跑 | 遷移中 NT 治理鉤子執行 | 走 ai-rules 舊路徑不變（兩份 Python 凍結；Rust 僅 parity harness 觸碰） | 無 | 全段共用 |
| SM-12 | parity fail | Rust 輸出與 Python `cmp` 有差 | 該段不得收——gate 擋、fail loud 修 Rust；消費端零感知（仍走舊路徑） | 段驗收 gate | R2-R5 |
| SM-13 | 跨語言 cache 互通 | Rust-built 衍生 db 由 Python 讀（或反向） | 三表 schema/`SCHEMA_VERSION` 互通；**Rust 專屬擴充（fn_defs）住獨立 sidecar——Python 三表 db 不動**（防 SCHEMA_VERSION 互毀重建 ping-pong）；不符→既有 stale-重建語義，不自動修 | 三守衛測試移植 | R2/R3 |
| SM-14 | MCP 單請求毒化 | 某 repo sidecar 髒/缺失／lib panic（髒 artifact 上 unwrap） | 該請求 Err（loud、帶指引）或 panic 經 handler 邊界 `catch_unwind` 映射為請求級 loud error——daemon 存活、其他 repo 請求不受影響 | per-request 隔離測試（Err 與 panic 兩案） | R6 |
| SM-15 | 刪除後回退 | R7 後發現問題 | R7 gate 前全程雙活可瞬間回頭；gate 後回退=git revert（雙向門） | 無 | R7 |

## 段落 0：全域研究摘要

### 可複用基礎設施
- **Python v0 全家＝parity oracle**（凍結）：`scip_refs.py` face 層（`ProtobufFace`/`SqliteFace`、`refs_rows` 現況為格式化字串——Rust 側結構化重取）、兩符號形態 matcher、slot 解析、`[SRC]` stamping、三守衛；`hazard.py` 六規則純函數；`profile.py`/`common.py`/`exclusions.py` 慣例層；381 tests 語意 oracle。
- **結構模板**：`~/Github/nt_v1`（root Cargo workspace＋`crates/*`＋Python 套件原位並存——機械驗證）；`~/Github/nautilus_trader`（v2 端局：`python/` 薄殼——本 EP 不需要此形態，見 AD-1 差異）。
- **Rust 生態選型**（本地已驗 2026-08-25，`poc/rust-deps-probe`）：`scip` 0.9.0（**rust-protobuf 形態——型別 `scip::types::*`**，研究報告 §4.1）；`scip-callgraph`（caller 邊參考實作，MIT/Apache）；`rmcp` **3.1.4**（官方 Rust MCP SDK、Tier 1 conformance 2026-08-21 PR #3287；本地編譯過——`streamable_http_server` 在核心、無獨立 feature 旗標；tokio）；`rusqlite` bundled（sqlite 3.53.2 本地編譯過）；hazard port 候選＝`ruff_python_parser`（`rustpython-parser` 已 superseded——astral-sh/ruff#286 脈絡，支援最新 grammar；編譯＋parse 已驗，**差動測試仍必須**）。
- **launchd/MCP 套路**（repo 外唯讀參考）：CRG plist（KeepAlive/ThrottleInterval 10/`FASTMCP_LOG_LEVEL=WARNING`）；lsp_mcp 常駐形態（port 8000）；port 8200 空閒（前 EP 已查）。

### 依賴關係與關鍵約束
- 消費端三面（現役=ai-rules 路徑，機械驗證）：NT 治理鉤子（`graph_audit --json`＋`scip_refs` query/audit）；mosaic（`hub_refs --hazard`）；ai-rules skills（subprocess＋存在性述語＋7 檔路徑引用——R7 relay 清單）。
- 本機工具鏈：cargo/rustc 1.96.0 在場（已驗）。
- 環境事實：sidecar home 含 NT 真索引（S1 已歸位 repo-keyed slot）＋衍生 db；code-reality 自身 dogfood（CRG graph.db＋snapshot sidecar 錨 `2eafd8a3`）。

### 風險假設清單

| 等級 | 假設 | 驗證 |
|------|------|------|
| **致命｜✅已驗（2026-08-25 POC）** | 跨語言 byte-identical stdout 可達（Rust 格式化＝Python 位元組） | `poc/r2-byte-identical` 實證：Rust（scip 0.9.0 protobuf-face 掃描）vs Python（sqlite face）對 `EventStoreLifecycle.open`——stdout `cmp` 逐位元組相同（[SRC]/兩符號塊/18 refs/截斷行）＋exit codes 同 0。**跨 face 等價一併證實**。熱路徑實測：Python `--build-cache` 9.3s vs Rust 全量 parse 0.9s（release，275MB/2433 docs）——約 10x；sqlite 查詢路徑兩端同為次秒級 |
| 高｜✅已驗（同 POC） | `scip` crate 對 ra 產出索引的 protobuf 相容（研究報告未本地跑過） | POC 實證：scip 0.9.0 完整解 NT 真索引。**形態勘誤：scip crate 走 rust-protobuf（非 prost）——型別在 `scip::types::*`、經 `protobuf::Message::parse_from_bytes` 解碼；R2 crate 選型照此** |
| 高 | hazard AST 語意漂移（`ruff_python_parser` vs CPython `ast`） | 依賴面已驗（`poc/rust-deps-probe`：編譯＋parse getattr/importlib 形態 OK）；**語意漂移風險不變**——R4 差動測試（mosaic 真語料＋synthetic）；**fallback 階梯**：修到一致 → Python holdout（最後一個 Python 工具，刪除延期）→ 判讀差異 user 簽核（Ask First） |
| 高 | MCP 進程內隔離（AD-2 代價） | R6 SM-14 必測 |
| 中｜部分已驗 | rmcp streamable-http＋launchd 常駐本地跑通 | 依賴面已驗（`poc/rust-deps-probe`）：rmcp **3.1.4** 編譯過（server feature）；`streamable_http_server` 在核心（tower `StreamableHttpService`）——**無 `transport-streamable-http` feature 旗標**。常駐/掛接 V1-V6 仍屬 R6 驗證 |
| 中 | 381 tests 語意覆蓋轉譯損耗（cargo tests 重寫時的語意漏失） | parity harness 位元組級把關＋子 EP 逐段測試計畫 |
| 中 | umbrella bin invocation 對消費端文檔面的改寫量 | R7 relay 包文檔面同步清單（前 EP S4 同型） |

### callstack 菜單積壓
無（本 repo 無 `ai-analysis/blueprint/callstack-plan.md`）。

## 段落劃分原則

- **依賴序**：R1（workspace 存在）→ R2（scip 引擎面=NT 契約最重面先證）→ R3（長在 R2 上）；R4 在 R2 後可展開（graph 家族）；**R5 依賴 R4**（foundation 層＋`transition` 是 delta_tour/chain_tour/boundary 系的原料——import 圖實證）；**R6 僅需 R2+R3**（audit 面=R2 組裝或 R4 `graph_audit`——可與 R4/R5 平行展開）；R7（全部 parity 綠後）。
- **垂直切片**：每段獨立 parity gate 可驗收；共存期任何一段失敗都不影響現役消費面（SM-11/12）。
- **blueprint 紀律**：每段 → 衍生 implementation 子 EP（`parent:` 本檔＋繼承該段 Context/吸收範圍）；子 EP 攜帶自己的 Pseudo Code/測試計畫。
- **雙凍結紀律**：遷移期 code-reality `code_reality/` 與 ai-rules 副本零改動（防 parity 基準漂移）；修 bug 走「兩份同步修＋parity 重跑」或「只修 Rust 側＋基準更新說明」——子 EP 視情況釘。

---

## 段 R1：Rust workspace 落地（repo 結構調整）——✅ 已被 R2 子 EP 吸收執行

> 本段全部交付項由 [ep-rust-r2-scip-family.md](ep-rust-r2-scip-family.md) 段 1 執行（2026-08-25 build 完成：workspace/toolchain/deny/`Result<ToolOutput>`/umbrella bin/`.gitignore` `/target`；Python 測試零干擾驗證——實測 388 passed〔=遷移時點基準，EP 原文 381 為舊 EP 時點計數，差額為 parity-era 前置測試成長〕）。

### Context（原始規格，供對照）
AD-1 結構決策落地：root `Cargo.toml`（`[workspace] members=["crates/*"]`、`resolver="2"`）＋`crates/code-reality`（lib 骨架：domain/use-case/adapter 分層目錄，空實作）＋umbrella CLI bin 骨架（clap；子命令名=現行模組名原樣，僅 `--help` exit 0）＋`rust-toolchain.toml` 釘版＋`rustfmt.toml`/`clippy.toml` 最小＋`deny.toml`（cargo-deny 授權稽核——MIT repo 無 copyleft 鏈維持，對齊 spec 授權結論）＋`.gitignore` += `/target`。**Python 側零改動**。

依賴：無前置。語義約束：與全部段落共享「Python 原位凍結」；與 R7 共享「子命令名=模組名原樣」（relay 最小 diff）。**lib API 形狀釘死（AD-2 前提）**：lib 函數回傳 `Result<ToolOutput, ToolError>`（`ToolOutput = {stdout, stderr, exit_code}` 資料形態）——lib 不 print、不 `std::process::exit`；兩個 bin 擁有打印與 exit。這是「drift 編譯期不可能」與 per-request `Result` 邊界的成立前提。

### 吸收範圍與驗收
不改任何行為。驗收＝`cargo build`/`cargo test`（骨架級）綠＋`cargo clippy --deny warnings` 綠＋**Python 381 tests 仍綠**（零干擾證明）＋dogfood snapshot smoke 仍綠＋`cargo deny check` 授權面無新引入。

（原衍生子 EP 指針 `ep-rust-r1-workspace.md` 已廢——由 R2 子 EP 吸收執行。）

---

## 段 R2：scip 家族 Rust 化

### Context
最重契約面先證（致命假設的載體）。`crates/code-reality` 內實作：SCIP 索引解析（`scip` crate）→ 結構化資料（對應 Python face 層語意）→ 衍生 sqlite cache builder（**三表 schema＋`SCHEMA_VERSION` 與 Python 互通**，SM-13）→ `scip_refs` 查詢 CLI 子命令（兩符號形態 matcher、`[SRC]` stamping、slot 解析、drift WARN、exit 0/1/2）＋`--audit` 兩遍式（subprocess 呼叫 graph_audit 的既有形態在 Rust 內重演或延後至 R4 組裝——子 EP 釘）。parity harness（AD-5）本段建立。

吸收範圍：`scip_refs.py`（835 行）＋`scip.proto`/`scip_pb2.py`（vendored gencode——Rust 側 `scip` crate 取代，proto 檔保留為 schema 參照）。UC-1/UC-4 載體換軌。**致命先驗已完成**（2026-08-25 `poc/r2-byte-identical`：byte-identical 實證＋scip 0.9.0 相容性＋熱路徑數字，見風險清單）——子 EP 直接以 POC 為起點擴充（matcher 全形態、audit、cache builder）。

依賴：R1。語義約束：與 R3 共享結構化 face 存取器（refs rows/fn spans——R3 caller 歸屬的原料）；與 R4 共享 exit 家族與 slot 慣例。

### 驗收（parity gate）
NT 真索引：query 兩形態 18 refs＋`--audit`——與 Python 版 `cmp` 逐位元組相同＋exit codes 一致（自我相對判準見 AD-5；**audit 兩遍式組裝若延後至 R4，本項 gate 移轉 R4 驗收——R2 子 EP 必須在定義 gate 前定案組裝時點**）；衍生 db 雙向互通（Python 讀 Rust-built、反向）；三守衛語義移植測試；SM-3/4 抽跑。**POC 先驗在前**（風險清單致命項）。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r2-scip-family.md`

---

## 段 R3：caller 邊＋closure（Rust 原生）

### Context
UC-2（舊 S2 的能力規格全繼承、載體改 Rust）：DEF-enc containment 歸屬（96.9% 機制已證）＋innermost tie 規則＋**3 元素 single-line span 支援**（SM-6，≥35 顆巨集 fn 反例）＋item-level remainder 分離輸出＋closure BFS（visited 環偵測、depth、按檔聚合）。`scip-callgraph` 為參考實作（非依賴）。CLI：`scip_refs <symbol> --callers/--closure [--depth N]`（site 行=call_edges 邊集，承接條款照舊 EP S2）。

吸收範圍：新能力（Python 版從未存在——無 parity 對象，驗收走**三源一致**而非 cmp）。依賴：R2 結構化存取器。語義約束：與 R6 共享「callers/closure 上 MCP 工具面」。**fn_defs 落位（SM-13）**：closure sqlite 路徑需要的 fn span 表住**獨立 Rust-owned sidecar**——Python 三表 db 與其 `SCHEMA_VERSION` 守衛（`scip_refs.py:89,443-445` 實證）不動，防互毀重建 ping-pong；schema 合併只在不早於 R7 評估。

### 驗收
`EventStoreLifecycle.open` → **16 callers／18 sites**（2026-08-25 機械重計裁決：上游「17 callers（1 impl＋16 tests）」與證據檔 18 refs 算術矛盾——`_e2e_rerun.out` 逐行複數＝1 impl＋15 test fn；裁決記錄見 R3 子 EP）＝LSP `incomingCalls`（curl 127.0.0.1:8000/mcp 對帳——**名單重取結案**，PENDING 阻斷）＝closure 起點（三源一致，spec 成功條件②）；closure 秒級（sqlite 路徑——tempdir 演練硬 gate）；單元測試釘 tie/single-span/item-level 語意（舊 EP S2 測試計畫繼承、語言改寫）。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r3-caller-edges.md`

---

## 段 R4：graph.db 家族＋hazard

### Context
rusqlite 讀 CRG graph.db（`connect_ro` WAL 語意＋torn-read guard 的 Rust 對應）：`snapshot`（module-edge sidecar JSON——commit-anchored 慣例不變）、`transition`（邊集 diff＋EP claims 對照＋reversed-edge added-direction 語意）、`hub_refs`（callers/callees 聚合＋prod/test split＋nodes-table 符號解析）、`graph_audit`（D1 風險掃描＋D2 對帳——**`--json` 鍵=NT 契約面**）、`graph_csv`（Cosmograph 匯出）。**hazard**：六規則 AST 偵測（`ruff_python_parser`）＋`static_prod ≤ 2` gating——最高風險項，差動測試（mosaic 真語料＋synthetic fixtures 全集）＋fallback 階梯（風險清單）。

吸收範圍：`snapshot.py`/`transition.py`/`hub_refs.py`/`hazard.py`/`graph_audit.py`/`graph_csv.py`＋`profile.py`/`common.py`/`exclusions.py` 慣例層（toml profile 載入＋crash-only 驗證＋`module_of`/`claims_re`/`scan_roots`；EDGE_KINDS 白名單；`anchor_pattern`）。UC-4/UC-6 載體換軌。

依賴：R1；（`scip_refs --audit` 的兩遍式組裝若 R2 延後，本段補——並進本段驗收）。**子 EP 兩段式**：①foundation＋graph 家族、②hazard（差動測試負擔獨立成段；sizing 需要時 hazard 可獨立衍生子 EP，不重編本 EP 段號）。語義約束：sidecar 慣例凍結；dogfood 本段起可用 Rust 工具掃自己。

### 驗收（parity gate）
NT `graph_audit --json` `cmp` 位元組相同（430/233/861 為 NT 機器參考值，自我相對判準見 AD-5）＋`scip_refs --audit` cmp（若自 R2 移轉）；mosaic `hub_refs --hazard` 輸出 cmp＋hazard 差動測試全綠（或 fallback 階梯觸發→Ask First）；snapshot sidecar 由 Python `transition` 消費（跨語言 artifact 互通）；dogfood snapshot smoke。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r4-graph-family.md`

---

## 段 R5：boundary＋tour＋runtime_edges 家族

### Context
- `boundary_build`/`boundary`：regex 掃 `*.rs` pyo3 宣告↔`.pyi` 對照（regex crate；commit-anchored boundary sidecar sqlite 慣例不變；NT repo 真掃描整合測試繼承）。
- tour 族：`chain_tour`（callchain markdown 樹框解析＋graph.db re-anchor 三態）、`delta_tour`（transition diff＋git hunk 錨；`git diff --unified=0` subprocess 或 git2——子 EP 選型）、`tour_manifest`/`tour_validate`/`tour_upgrade`（TOML/`.tour` JSON 治理三件；dry-run 預設語意）。
- `runtime_edges`：viztracer trace JSON（serde_json）→ (pid,tid) ts-interval enclosure 巢狀歸屬。

吸收範圍：`boundary*.py`/`chain_tour.py`/`delta_tour.py`/`tour_*.py`/`runtime_edges.py`。依賴：R4（delta_tour 消費 transition；chain_tour 消費 graph.db face）。

### 驗收（parity gate）
各族 synthetic fixtures cmp（incident regression 全攜帶——boundary_build 7 個 regression 案、delta_tour 步序、chain_tour 錨三態）；NT 邊界掃描整合（`test_boundary_integration` 形態）對齊；`.tours` 語料治理工具在本 repo dogfood 上驗證。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r5-families.md`

---

## 段 R6：MCP server（單一 HTTP、多 repo）

### Context
AD-2/AD-3 落地：`rmcp`（官方 SDK、Tier 1）streamable-http 常駐 `127.0.0.1:8200`；**進程內直連 lib**（R2-R5 產物）＋per-request `Result` 隔離（SM-14）＋tokio 並發（rusqlite per-call 連線/pool——子 EP 設計）。工具面 v0＝SCIP 家族四件：`refs(symbol, repo_root)`/`callers(symbol, repo_root)`/`closure(symbol, repo_root, depth=2)`/`audit(repo_root)`——**`repo_root` 必填**（SM-5 loud error 帶修正指引；callers 輸出含 site 行＝call_edges 邊集——承接條款照舊 S2）；snapshot/transition/tour 族維持 CLI（skills subprocess 消費，YAGNI 條款照舊）。`[SRC]` 透傳＋`[STDERR]` 段（管理訊息可見性）。launchd plist（CRG 慣例）＋ZCode user-level mount（`{"type":"http","url":"http://127.0.0.1:8200/mcp","timeoutMs":60000}`，子樹 merge 紀律）；Claude 端 optional 同 URL。升級流程=install＋kickstart（AD-2）。

吸收範圍：新 frontend（取代舊 S3 的 `mcp_server.py` 規格——Python 版永不建）。依賴：**R2+R3**（audit 面=R2 組裝或 R4 `graph_audit`——可與 R4/R5 平行展開）。**sync-in-tokio**：rusqlite 阻塞 I/O 在 handler 內走 `spawn_blocking`（或明示接受單人負載容忍，子 EP 釘）；**跨請求零共享 mutable cache**（未來引入 repo 鍵快取須明示＋測試）。

### 驗收
V1-V7 形態（CRG 閉環）：tools/list 四工具；refs 18 refs＋callers 16 callers 與 CLI 同輸出（數字隨 R3 gate 裁決——見段 R3）；缺 repo_root loud；SM-14 毒化隔離（Err 與 panic 兩案）；audit `[STDERR]` WARN 可見；SM-10：改碼→`cargo install --path`→`launchctl kickstart`→下一呼叫反映新版本（kickstart 版 V4）；KeepAlive 自起；跨 session 新 session 工具可見（L6 user 抽查）。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r6-mcp-server.md`

---

## 段 R7：終局 relay＋雙副本刪除＋文檔收斂

### Context
AD-4 單次 relay。前提：R2-R6 全綠＋user gate。安裝形態：`cargo install --path ~/Github/code-reality/crates/code-reality`（**workspace root 是 virtual manifest 無 package——`--path` 必須指 crate 目錄**；MCP server 為同 crate bin target 一併安裝）→ `~/.cargo/bin/code-reality`；消費端命令 `uv run --project ~/Github/ai-rules python -m code_reality.X` → `code-reality X`（子命令名原樣）。

要點：relay 三包一次打包（NT：治理鉤子命令＋文檔面；mosaic：`hub_refs --hazard`＋文檔面；ai-rules：7 檔 skills 路徑＋AGENTS.md＋SKILL.md 消費形態模板/存在性述語）——各含自足驗收命令（舊路徑先存基準→新路徑→`cmp`）。刪除（gate 後）：**hazard holdout 生效時**——刪除範圍排除 hazard 面＋其 foundation 依賴＋parity harness 保留至 holdout 解除（mosaic relay 對應延期）。①本 repo `code_reality/`＋`tests/`（**刪前定案 fixture generators 去向**：移植為 Rust test helpers 或語料凍結靜態資料）＋`pyproject.toml`/`uv.lock`（Cargo-only 端局；parity harness 隨 Python 退役）＋**scip.proto 先搬 `crates/`（或 docs/）再刪**（R2 保留條款兌現）＋`.code-reality.toml` module prefix 改 crates 形態＋instruction 體系全面改寫（root AGENTS.md/README/crates AGENTS.md）＋`.gitignore` 清理＋實體殘留清理（`code_reality.egg-info/`、`.venv/`、`.pytest_cache/`）；②ai-rules `git rm -r code_reality/ tests/`＋pyproject 清理＋`uv lock` 重生（前 EP S4 要點全繼承）＋kanban 卡收斂（追蹤卡＋兩能力卡搬 Done）＋**部署面同步**（`~/.zcode/skills/code-reality/`、`~/.agents/skills/code-reality/` 等四 harness 副本——實體檔非 symlink〔diff 已證 byte-identical〕，relay 包涵蓋或其同步機制一併交代）。零殘留掃：兩 repo 路徑導向 rg（豁免清單繼承前 EP：zcode-session-query 借 venv 條目）＋**殘留掃範圍擴及 NT/mosaic 文檔面**（NT `AGENTS.md`/`callstack-plan.md` 命令字串、mosaic `tools/AGENTS.md`/kanban 卡——或列豁免清單）＋consistency formal gate。

### 驗收
NT 三面 byte-identical（新路徑）＋**exit codes 0/1/2 一致**；mosaic hazard 命中；兩 repo 零殘留掃 0（豁免核對）＋NT/mosaic 文檔面殘留掃（或豁免清單）；本 repo Python 殘留掃 0（檔案系統層含 egg-info/.venv/.pytest_cache）；`/consistency` formal。

→ 衍生子 EP：`ai-analysis/execution-plans/ep-rust-r7-relay-deletion.md`

---

## 整合策略

- **跨段整合點**：R2 結構化存取器=R3 原料；R2-R5 lib=R6 唯一後端；R1 umbrella bin 子命令=R7 relay 命令面；parity harness 逐段擴充至全工具族。
- **baseline**: `2eafd8a`。
- **回退路徑**：R7 gate 前全程雙活（現役面不變）；gate 後回退=git revert（**本 repo 刪除 commit＋NT/mosaic/ai-rules 三面 relay commits——跨 repo revert 清單**）。段內失敗=段不收（SM-12），無部分態外洩。
- **雙凍結**：見段落劃分原則。

## Ask First 清單

1. R7 relay＋雙刪前：NT/mosaic 驗收回報＋user 確認（D1 gate 精神）
2. hazard 差動測試若實質漂移且修不齊：fallback 階梯裁決（Python holdout vs 判讀差異簽核）
3. 任何 NT 契約面刻意演化（契約版本 bump 需 NT 側同步）
4. 所有 git commit（consent 規則）
5. （繼承 spec）B1/B2 圖引擎裁決（v1+）；CRG MCP 退役時點；lsp_mcp home 遷移

## v1+ 展望（另行 EP）

Rust 化使既有 v1+ 項目全部更便宜但時點不變：S5 B1/B2 圖引擎研究→user 裁決（**2026-08-25 user 方向表態：漸進演化、端局純 Rust 引擎——NT 模式**〔Python 全程活著服務消費端、Rust 逐塊取代、最後才刪，同 AD-1〕——R2 三表 cache＋R3 fn_defs sidecar 即自建圖層種子；路線＝逐段把圖能力收進 Rust，CRG 的最後角色〔Python indexer、communities 計算〕有 Rust 替代時退役；正式 D5 裁決仍走 v1+ 研究報告＋單向門）；SCIP 邊注入 graph.db（NT 861 缺差→0）；CRG MCP 退役；lsp_mcp home 遷移（D4 維持進程分離）；UC-7 發佈（cargo crate/binaries＋plugin market——umbrella binary 一條命令天然滿足）。**新增候選**：Rust 化後索引生成自有化評估（ra SCIP 產出環節的自動化——`--refresh` 一條龍從「可選小卡」升格）。

## 收尾步驟（各段 build 階段 5 執行）

1. **Capabilities＋Kanban**：R2-R5 段完成→root AGENTS.md 對應行註記載體（Rust）＋kanban 卡 In-Progress/Done 流轉；R3/R6 完成→caller 邊/MCP 兩行 📋→✅；R7→整表改寫（Rust 端局）。
2. **SYSTEM-MAP.md**：不適用（本 repo 無此檔）。
3. **instruction 檔**：R7 全面改寫；**導航指針例外**——EP 建立時已最小更新 root AGENTS.md（EP 指向新檔＋📋 行 EP 代號改 R3/R6），防遷移期 session 被舊 EP 誤導；每段子 EP 收尾核對 parity harness 文檔。
4. **/audit-test**：對 parity harness 與各段 cargo tests 跑品質稽核（mock 健康度——rusqlite/scip face 的測試雙床是否 vacuous）。

## EP Review Record

2026-08-25 四軌獨立審查（結構／完整性遺漏／UC 覆蓋／合規兜底——fresh eyes Explore agents，全部 findings 已 judge）。**0 P1／6 P2／19 P3；全數採納修入上文**（S5 取最小形態）。摘要：

| 軌 | 關鍵 findings（已修入上文） |
|----|------|
| 結構 | 🔴P2 R4/R5「可平行」與 R5 依賴 R4 foundation（import 圖實證）矛盾 → 依賴序改為 R5 依賴 R4、R6 僅需 R2+R3；P2 fn_defs 與凍結三表 schema 的 SCHEMA_VERSION 互毀重建 ping-pong（`scip_refs.py:89,443-445` 實證）→ fn_defs 獨立 Rust-owned sidecar（SM-13/R3）；P3：R2 audit gate 條件化、lib API 形狀釘死（`Result<ToolOutput>`——lib 不 print/exit）、panic→`catch_unwind` 請求級映射（SM-14 兩案）、rusqlite `spawn_blocking`、跨請求零共享 cache、NT 基準數字自我相對化、`cargo install --path` 必須指 crate 目錄（virtual manifest root）、AD-4 逃生口 wording re-base |
| 完整性 | P2 scip.proto「保留」與 R7 全刪矛盾 → 先搬 `crates/` 再刪；P2 四 harness 部署面（`~/.zcode`/`~/.agents` skills 副本——實體檔非 symlink）在 relay 範圍外 → 納入 R7 relay 包；P3：boundary_build regression 7 案（非四案）、fixture generators 去向（移植或凍結語料）、實體殘留（egg-info/.venv/.pytest_cache）＋ai-rules `uv lock` 重生、殘留掃擴及 NT/mosaic 文檔面 |
| UC 覆蓋 | P2 UC-3 遞延在盤點表缺席 → 補列（v1+ 展望承載）；P2 SM-10 kickstart 化後無驗證落點 → R6 驗收補 kickstart 版 V4；P3：AD-3 理由出處標注（1/5/6＝spec；2/3/4＝前 EP/環境）、SM×10 補 per-SM 對應段、R6 補 call_edges 承接條款 |
| 合規 | P2 R4 gate 未涵蓋 R2 延後的 `scip_refs --audit` 面 → 條件條款移轉；P3：R7 驗收補 exit codes 0/1/2、hazard holdout 與 R7 無條件刪除矛盾 → 條件分支、AGENTS.md 遷移期指向舊 EP → 最小指針更新即刻執行（見收尾 3）、D3 消費形態同步明文化、跨 repo revert 清單 |

**不採納 0 項**；hazard 拆段取最小形態（R4 子 EP 兩段式，不重編段號——避免 kanban 卡與既有引用 churn）。

## EP Validate Findings

2026-08-25 POC 驗證（`poc/r2-byte-identical`＋`poc/rust-deps-probe`；POC 存活至 R2 build+commit 後清除）：

| ID | 嚴重度 | EP 段落 | 問題（POC 結果） | 建議 | 狀態 |
|----|--------|---------|----------------|------|------|
| V1 | 🔴→✅ | R2（致命） | 跨語言 byte-identical 實證通過：Rust protobuf-face vs Python sqlite-face，stdout `cmp` 逐位元組相同＋exit 0；跨 face 等價一併證實 | 致命假設解除；R2 子 EP 以 POC 為起點 | verified |
| V2 | 🟡→✅ | R2 | `scip` crate 相容性通過；**形態勘誤：rust-protobuf（非 prost）**，型別 `scip::types::*`；275MB/2433 docs 解碼 697ms（release） | R2 crate 選型按 rust-protobuf 形態；段落 0 已修 | verified |
| V3 | 🟡 | R6 | rmcp 本地編譯通過；**版號勘誤：3.1.4（非 0.5.0）**；`streamable_http_server` 在核心、無 feature 旗標（tower `StreamableHttpService`）；launchd 常駐仍未驗（R6 V1-V6） | R6 子 EP 按 3.x API；段落 0 已修 | verified（依賴面） |
| V4 | 🟠 | R4 | `ruff_python_parser` 編譯＋parse 通過（getattr/importlib 形態）；**語意漂移風險不變**——差動測試仍是 R4 硬 gate | 維持 fallback 階梯不變 | verified（依賴面） |
| V5 | ⚪ | R2 | 熱路徑實測：Python `--build-cache` 9.3s（68k symbols/741k occs）vs Rust 全量 parse 0.9s——約 10x（非想像中的 50x+）；Python sqlite 查詢 0.156s（次秒級兩端一致） | 效能敘事按實測口徑；「互動式重生索引」價值成立 | verified |

**結論**：致命假設解除，EP 路線成立——可直接進 R1/R2 子 EP 衍生。
