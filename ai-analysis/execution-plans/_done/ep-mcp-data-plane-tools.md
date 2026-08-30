# EP：MCP 資料面四工具——`build`/`snapshot`/`delta_tour`/`project` 進 `mcp_server`

> **ep_type**: implementation

baseline: `62dee2ae519198174aef0f8eca9be33d13bd7589`

## 實作總覽

`mcp_server` 現有 17 個 MCP 工具全屬唯讀查詢面；本 EP 把四個
CLI-only 資料面工具 thin-wrap 進 MCP：`build`（傘形索引重建）、
`snapshot`（邊界快照）、`delta_tour`（快照 diff → CodeTour）、
`project`（投影圖 overlay）。目的：EP/build loop 在 harness session
內直接驅動資料面——ai-rules 端「MCP 優先＋CLI fallback」分層中，
四工具的 CLI fallback 退場條件成立（翻轉一行屬 EP2 範圍，本 EP
只供給能力）。

機械形態＝既有 thin-wrap 慣例：typed params → argv → 各模組
`run(argv)` in-process（同 refs 族走 `cli::run`、graph 族走
`graph_engine::run` 的形態），共用 spawn_blocking＋catch_unwind 的
SM-14 per-request 隔離。四模組 lib 邏輯零改動。

## 設計裁決記錄（凍結——2026-08-29 深夜討論，勿重辯）

1. **MCP 面性格重裁：唯讀查詢 → 含寫副作用**。舊「互動最小面」
   裁決（memory `interface-form-mcp-vs-cli-adjudication`，已補重裁
   更新段）的時空是**查詢驅動**；現在 EP/build loop 要在 session
   內驅動資料面——build 寫 slot/graph.db、snapshot 寫快照檔、
   delta_tour 寫 `.tour`、project 寫 `projections/`——前提變了，
   重裁成立。配套義務：每個工具 description 明示寫副作用 target，
   不留驚喜。
2. **freshness 反對理由收回**：CR 開發機＝CLI dev face、消費者＝
   release+plugin，「新 session 生效」本就是規則——不構成反對
   MCP 化的理由。
3. **分層**：主 session 查詢→MCP 優先；spawned review/verify→
   registry agents（已掛 CR MCP 白名單）。
4. mcp_server.rs 檔頭「snapshot/tours stay CLI; skills
   subprocess-consume them, YAGNI」註解隨本 EP 改寫——該 YAGNI
   判定的前提被 1. 的重裁取代，註解須記錄翻轉而非靜默消失。

## 段落 0：全域研究（主導 session 已完成）

**可複用基礎設施**（全在 `crates/code-reality/src/mcp_server.rs`）：

- SM-14 隔離：`run_refs_like`（:261）＝spawn_blocking＋
  catch_unwind＋`map_tool_output`（:203，exit≠0 → MCP error，
  錯誤文 text 以 stdout 優先——WARN face 印 stdout）
- argv thin-wrap 先例：refs 族→`crate::cli::run`；graph 族→
  `gq`（:388）→`crate::graph_engine::run`；`get_community`（:631）
  直呼 typed lib fn（無 CLI op 時的先例）
- 1MB text cap backstop：`apply_text_cap`（:172）

**四工具 CLI 面（依賴錨點——定義端；消費端＝本 EP 新增的 MCP
handler，第二個 in-tree 消費者是 bin 分發
`src/bin/code-reality/main.rs:31-47`）**：

| 工具 | lib 進入點 | 參數面 | 寫副作用 |
|---|---|---|---|
| build | `build::run`（build.rs:427）→ `build_repo`（:262，roots 可注入） | `--repo`（必）`--producer rust\|python` `--json` | `index.scip` slot、`graph.db`、engine indexes |
| snapshot | `snapshot::run`（snapshot.rs:456）→ `build_snapshot`（:371） | `--repo` `--label` `--out-dir` | snapshots/ TOML |
| delta_tour | `delta_tour::run`（delta_tour.rs:594）→ `build_tour` | positionals `a b`＋`--ep` `--repo` `--task` `--out-dir` | `.tours/delta/<date>-<task>.tour` |
| project | `project::run`（project.rs:498）→ `project_repo`（:208） | `--repo` `--plan`（必）`--json` | `.code-reality/projections/<stem>/` |

**風險假設**：

- rmcp `Option<bool>` 參數（低）：`get_community` 的
  `include_members` 先例在場
- **build 分鐘級長時運行**（中）：MCP 無進度回報、客戶端 timeout
  未知——對策＝description 明示（凍結決策），非協定層進度推送
  （YAGNI）
- **同 repo 並發 build**（低）：與 CLI 面（兩終端同跑）同暴露；
  single-user daemon 姿態不加鎖——known limitation 記錄於此
- **delta_tour 預設 out_dir 是 CWD 相對**（中）：daemon cwd 對
  MCP 呼叫者無意義——MCP 面省略 `out_dir` 時改用
  `<repo_root>/.tours/delta`（in-repo 預設，與 CodeTour 消費端
  〔編輯器開 repo 根〕一致；與 CLI 差異在 description 明示）
- 致命級假設：無（全是既有 lib fn 薄包裝；lib 行為已被 build
  t1-t12 fake-bin 整合測試＋project/delta_tour/snapshot 各自
  suite 覆蓋）

## UC 盤點

### Backlog 關聯

- `.kanban/Backlog/` 已有 sibling session 兩卡（query-time
  self-heal 軸）；本 EP 新建 `ep-mcp-data-plane-tools.md`（EP
  追蹤卡）
- 自動建卡結果：新建 1 張（EP 追蹤卡）；不建獨立能力卡——能力
  落在既有「Unified MCP interface」row 的擴充（更新既有 UC）

### SYSTEM-MAP 影響

- 本 repo 無 SYSTEM-MAP.md（正當跳過）

### 掃描範圍

- root `AGENTS.md` Capabilities（「Unified MCP interface」row）
- `crates/AGENTS.md`（mcp_server 段 :138）
- `plugin/skills/code-reality/SKILL.md`（MCP tools 表 :17＋:215
  措辭——工具事實真相源）

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|---|---|---|---|---|
| Unified MCP interface | ✅ | root AGENTS.md | 更新 | 17→21 工具；面性格含寫副作用（重裁記錄） |
| 新 repo 數據面一鍵準備（build 傘形） | ✅ | root AGENTS.md | 更新 | 入口加 MCP face（`build` 工具） |

### 新增 UC

| 能力 | 狀態 | 實作路徑 |
|---|---|---|
| MCP 資料面驅動（build/snapshot/delta_tour/project 四工具，session 內驅動數據面） | 📋 | `crates/code-reality/src/mcp_server.rs` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|---|---|---|---|---|
| SM-1 | build happy | 有源碼 repo＋producers 在 PATH | 產 index＋graph.db＋[OK] 報告 | 無 | MCP 資料面驅動 |
| SM-2 | build 無語言面 | 空 repo | MCP error「找不到 .py 或 .rs」 | 無 | 同上 |
| SM-3 | build producer 非法 | `producer="go"` | INVALID_PARAMS「rust 或 python」 | 無 | 同上 |
| SM-4 | build 長時運行 UX | 大 repo 呼叫 | description 明示分鐘級、無進度、timeout 自評 | 無 | 同上 |
| SM-5 | snapshot happy | graph.db＋git repo 在場 | 寫 snapshot TOML＋`[OK] snapshot: N files...` 行 | 無 | 同上 |
| SM-6 | snapshot 無 graph.db | 空 repo | MCP error「graph.db 不存在」 | 無 | 同上 |
| SM-7 | delta_tour happy | 兩快照＋repo | 寫 `<repo>/.tours/delta/<date>-<task>.tour` | 無 | 同上 |
| SM-8 | delta_tour 快照檔缺 | 不存在路徑 | MCP error（crash 面） | 無 | 同上 |
| SM-9 | project happy | plan＋sources/＋overlay-gen 在 PATH | `[projected]` graft/claims 報告 | 無 | 同上 |
| SM-10 | project plan 無效 | 不存在 plan 路徑 | MCP error「plan ... 無效」 | 無 | 同上 |
| SM-11 | project 真實 index 缺 | 無 index slot | MCP error「真實 index 不存在——先跑 build」 | 無 | 同上 |
| SM-12 | 並發同 repo build | 兩連線同時 build | 與 CLI 同暴露（不加鎖），known limitation | 無 | — |

## 段落劃分原則

單模組小 EP，兩段＋收尾：S1（impl）→ S2（tests＋L4 stdio 實測）→
收尾（doc-sync＋發行）。跨段語義約束只有一條：**MCP argv 與 CLI
flag 1:1 對應**（delta_tour 的 out_dir 預設差異除外，已在段落 0
記錄）——S1 落地、S2 以斷言釘住。

## S1：四工具 schema＋thin-wrap＋隔離共用

### Context

（UC 引用：實作「MCP 資料面驅動」）mcp_server 是 frontend
adapter（crates/AGENTS.md :138）；四工具已有 CLI 面與 lib 進入點
（段落 0 錨點表）。本段零 lib 改動，只加 adapter 面。

### 核心實作要點

1. **Param structs**（沿既有命名慣例，`repo_root` 必填）：
   - `BuildParams { repo_root, producer: Option<String>, json: Option<bool> }`
   - `SnapshotParams { repo_root, label: Option<String>, out_dir: Option<String> }`
   - `DeltaTourParams { repo_root, snapshot_a, snapshot_b, ep/task/out_dir: Option<String> }`
   - `ProjectParams { repo_root, plan, json: Option<bool> }`
2. **泛隔離 helper**：把 `run_refs_like` 的 spawn_blocking＋
   catch_unwind 抽成 `run_module<F: FnOnce(&[&str]) -> ToolOutput>`
   （refs 族**與 `gq`** 都改 delegate——單一隔離實作，F7）；panic
   訊息統一取 `gq` 形（提取 payload message，較refs 舊形多資訊）；
   錯誤映射仍走 `map_tool_output`
3. **Handlers**（四個 `#[tool]` 方法＋router 註冊）：
   - build：`producer` 非 `rust|python` → INVALID_PARAMS（沿
     `list_communities` 的 algorithm 驗證先例）；`json=true` 推
     `--json`
   - snapshot：`label`/`out_dir` 直傳
   - delta_tour：positionals 在前、flags 在後；`out_dir` 省略時
     傳 `<repo_root>/.tours/delta`（段落 0 裁決）
   - project：`json=true` 推 `--json`；`plan` 路徑原樣直傳
     （description 建議絕對路徑）
   - Param structs 的 Option 欄位一律 `#[serde(default)]`（17 工具
     在線慣例——F8）
4. **Descriptions**（英文，全四工具必含）：寫副作用 target、
   運行時長（build＝minutes-level、無進度回報、timeout 自評；
   其餘秒級）、「Same lib as `code-reality <cmd> ...`」錨
5. **檔頭註解改寫**（mcp_server.rs:1-14）：tool surface v0 的
   read-only 前提→重裁記錄（引本 EP）

### Pseudo Code

```rust
// mcp_server.rs（新增；run_refs_like 改 delegate）
async fn run_module<F>(f: F, argv: Vec<String>) -> Result<CallToolResult, McpError>
where F: FnOnce(&[&str]) -> crate::ToolOutput + Send + 'static {
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        std::panic::catch_unwind(AssertUnwindSafe(|| f(&refs)))
    }).await?   // join 失敗 → INTERNAL_ERROR
     ?           // panic → INTERNAL_ERROR（毒化 sidecar 隔離訊息）
     |> map_tool_output
}

#[tool(description = "One-shot data-plane build ... WRITES ... LONG-RUNNING: minutes-level, no progress reporting ... Same lib as `code-reality build --repo <repo>`")]
pub async fn build(&self, Parameters(BuildParams{..}): ..) -> .. {
    if let Some(p) = &producer { validate(p)? }          // INVALID_PARAMS
    argv = ["build", "--repo", repo_root, (--producer p)?, (--json)?];
    Self::run_module(crate::build::run, argv).await
}
// snapshot / delta_tour / project 同構
```

### 驗證策略

- `cargo build -p code-reality`＋`cargo test -p code-reality
  --test s6_mcp_server`（S1 最低門檻：編譯＋既有測試不破）
- thin-wrap drift 防護＝編譯期（同檔既有論證）

### Invariant Impact

無——純 adapter 層新增；四模組 lib 邏輯與 CLI 面零改動。

## S2：tests＋L4 stdio 實測

### Context

（UC 引用：驗證「MCP 資料面驅動」）沿 `tests/s6_mcp_server.rs`
既有形態：直接 method 呼叫（`Parameters(Params{..})`）＋HTTP smoke
的 tools/list 固定清單。

### 核心實作要點

1. **tools/list 清單 17→21**：既有 `http_server_serves_initialize_
   and_tools_list` 的 names vec 增 `build`/`delta_tour`/`project`/
   `snapshot`（排序）；`tools2.len()` 17→21
2. **錯誤路徑路由**（每工具 loud error 斷言）：SM-2（build 空
   repo）、SM-3（producer 非法）、SM-6（snapshot 無 db）、SM-8
   （delta_tour 缺快照——斷言錨 `讀取失敗`，F5）、SM-10（project
   plan 無效——**fixture 先建 `<repo>/.code-reality/scip/index.scip`
   佔位檔**：project_repo 先查 index 存在才輪到 plan，F3）、
   SM-11（project index 缺）
3. **MCP 寫面 end-to-end**（SM-5＋SM-7 串成一鏈）：git fixture
   （沿 `tests/s2_snapshot.rs` 的 `git()` helper 形態）＋
   `graph_db_fixture` 造 db → `server.snapshot` ×2（兩 commit 間
   加一條 db 邊＝模擬 rebuild）→ 斷言快照檔落地＋`[OK]` 行
   （**contains 斷言**——fixture sha 為 deadbeef 必帶 stale WARN
   但 exit 0，F4）→ `server.delta_tour(a, b, task=..)` → 斷言
   `.tour` 落地 `<repo>/.tours/delta/`
4. **MCP 級 build/project happy 不做**（known uncovered，記錄）：
   lib 面已被 build t1-t12（fake-bin 注入）＋project suite 覆蓋；
   MCP 層增量＝argv 組裝＋驗證，由錯誤路徑測試證明貫通（組裝錯
   → 錯誤訊息不匹配）。MCP build happy 需真實 producer 在
   PATH（run() 用真實 producer_roots，fake-bin 注入只進
   `build_repo` 直呼）——env-coupled，不入 suite
5. **L4 stdio 實測**（EP 驗收項，一次性腳本不進 suite）：驅動
   built bin `--stdio`——JSON-RPC initialize → tools/list（21
   含四新）→ tools/call 四工具（error face 即可證可呼；snapshot/
   delta_tour 用 fixture repo 走 happy）

### 驗證策略

- `cargo test -p code-reality`（全 crate；sibling 未 commit 改動
  在場，以其既有綠為基準——本 EP 不碰那四檔）
- L4：stdio 腳本輸出貼 EP review 區

## 整合策略

- 順序：implement（S1→S2）→ post-build 全鏈（code-review→
  judge→consistency→metadata-sync）→ commit（EP＋impl＋doc-sync
  檔案一起；只 add 本 EP 指名檔，**含 EP 檔與 kanban 卡**——發行
  preflight 見下）→ `scripts/release.sh 0.6.0 --subject "mcp
  data-plane tools (build/snapshot/delta_tour/project)"`（minor
  bump：新工具面；無 --dry-run 呼叫即 commit/tag/push 同意——腳本
  設計）
- **發行前樹淨處置**（F1）：release.sh preflight 的
  `git status --porcelain` **連 untracked 也算**——本 EP 產物
  （EP 檔、kanban 卡）隨 impl commit 帶走；sibling session 的
  四個 tracked-modified＋三個 untracked（`.kanban/Backlog/` 兩卡、
  `ep-index-query-time-self-heal.md`、`poc/`）若仍在：先做
  liveness 確認（rollout mtime＋雙取樣靜止）；靜止則
  `git stash push -u -- <sibling 檔案們>` → release →
  `git stash pop`（不認領、不混入 release commit）；活躍則發行
  暫緩並回報 user
- doc-sync 隨本 EP commit（收尾段清單）

## 收尾步驟

1. Capabilities：root AGENTS.md「Unified MCP interface」row 更新
   （21 工具、含資料面四工具＋寫副作用性格）＋「新 repo 數據面
   一鍵準備」row 補 MCP 入口；kanban EP 卡移 `Done/`
2. `plugin/skills/code-reality/SKILL.md`（工具事實真相源）：
   MCP tools 表 +4 行（含長時運行／寫副作用警示）＋表尾 pointer
   行（graph_query 族）＋:215「MCP face covers the SCIP family」
   措辭翻轉
3. `crates/AGENTS.md` mcp_server 段：thin-wrap 對象從
   `cli::run` 擴為「cli::run＋各模組 run」
4. `plugin/README.md` MCP 工具枚舉補 data-plane family（F2——
   第四文件面）
5. `/audit-test` 對新增測試
6. 發行（release.sh）＋EP 歸檔至 `_done/`

## EP Review

**Reviewer**：code-reviewer agent（fresh eyes，2026-08-30）。
**Verdict：可實施（零 Critical）**。執行驗證：全相關檔讀碼＋rg
釘點掃描（工具數唯一釘點 s6:174）＋git 對帳（baseline=HEAD）。

### Finding Record

| # | 嚴重度 | 摘要 | 裁決 | 落點 |
|---|---|---|---|---|
| F1 | 🟡 | preflight 擋 untracked；stash 不帶 -u 不夠 | ✅ 採納 | 整合策略改寫（EP 產物隨 impl commit；sibling 用 stash -u） |
| F2 | ℹ️ | plugin/README.md 是第四文件面 | ✅ 採納 | 收尾步驟 +item 4 |
| F3 | ℹ️ | project_repo 先查 index 後查 plan——SM-10 fixture 要 index 佔位 | ✅ 採納 | S2 item 2 注記 |
| F4 | ℹ️ | fixture snapshot 必帶 stale WARN（exit 0） | ✅ 採納 | S2 item 3 contains 斷言 |
| F5 | ℹ️ | SM-8 錨 `讀取失敗`（transition.rs:30） | ✅ 採納 | S2 item 2 |
| F6 | ℹ️ | task 空字串可加 schema 擋 | ❌ 不採納 | lib 已 crash-loud（delta_tour.rs:666）；schema 重複驗證非必要 |
| F7 | ℹ️ | refs/gq panic 訊息文字面漂移；「單一」與實態落差 | ✅ 採納（調整） | gq 一併 delegate；panic 訊息統一取 payload 提取形 |
| F8 | ℹ️ | Option 欄位 serde(default) 慣例 | ✅ 採納 | S1 item 3 |

### Post-build dual-context review（2026-08-30）

**Primed 側**（code-reviewer-primed）：意圖高度忠實落地（EP 宣稱
元素逐項對照全 ✅、無 scope creep、架構邊界維持）；findings：
F-A（🟡）SM-3 測試只斷 message 不斷 code——lib 端 exit-2 arm 同
訊息，pre-validation regression 會綠燈通過 → **✅ 採納**：補
`err.code == INVALID_PARAMS` 斷言；F-B（🟡）L4 stdio 待補 →
**✅ 完成**（證據見下）；F-C（ℹ️）project description 缺絕對路徑
建議（EP S1 item 3 指定）→ **✅ 採納**：description 補一句；
F-D（ℹ️）檔頭重裁日期 08-30→08-29 對齊 EP 凍結記錄 → **✅ 採納**；
F-E（ℹ️）SM-4 僅 description 承載無機械釘住 → **❌ 不採納**
（reviewer 自評為正確取捨——釘 description 字串屬脆斷言）。

**L4 stdio 實測證據**（F-B 銷項；`.agent-tmp/l4_stdio.py` 驅動
`target/debug/code-reality-mcp --stdio`，scratch python repo＋真
pyrefly-index producer）：

```
[L4] initialize ok: rmcp
[L4] tools/list: 21 tools（四資料面工具全在）
[L4] build ok: [OK] build: <scratch> [python-face] / graph: 2 nodes / 1 edges /
       producer: pyrefly-index 0.5.1+62dee2a
[L4] snapshot#1 -> <scratch>/.code-reality/snapshots/l4-repo-b0a9b74e.json
[L4] snapshot#2 -> <scratch>/.code-reality/snapshots/l4-repo-a173da53.json
       （第二次 build 前改 source＋commit——sha 後綴互異）
[L4] delta_tour -> <scratch>/.tours/delta/2026-08-30-l4-check.tour（in-repo 預設落地）
[L4] project error face ok: [FAIL] project: plan /nonexistent-plan.toml 無效：... (os error 2)
[L4] PASS — all four data-plane tools callable via stdio
```

過程附帶實證：髒路徑（誤含 `[LOG]` 行）傳入 delta_tour →
JSON-RPC error `工具退出碼 1：...讀取失敗`——error face 穿真
transport 的 SM-8 行為。

**Fresh 側**（code-reviewer）：**可 commit，零 🔴 零 🟡**，7
findings 全 ℹ️。六軸全過（thin-wrap argv 機械比對、隔離委派等
價、錯誤映射、descriptions 逐宣稱對碼、測試品質、[SRC]/[STDERR]
面——軸 6 附帶修正：build freshness WARN 是 bin 啟動面，本就不
在 per-call ToolOutput 內）。裁決：

| # | finding | 裁決 | 理由 |
|---|---|---|---|
| 1 | panic 訊息形態無測試釘住 | ❌ 不採納（記 known test gap） | pre-existing 缺口（HEAD 亦無 panic 測試）；無已知簡單 trigger，為測而測 |
| 2 | get_community 第三份隔離殘留 | ❌ 不採納 | doc comment 已精確限定 argv 族；泛化 run_module＝無消費者抽象（YAGNI） |
| 3 | eprintln 側通道 MCP 面不可見 | ❌ 不採納（情報） | pre-existing lib 介面問題（graph_db parse warns）；lib 回傳值擴充超出 adapter EP 範圍——下游可立項 |
| 4 | e2e 補語義計數斷言 | ✅ 採納 | `2 files`/`3 files`＋steps≥1——證 diff 真實非 plumbing 綠 |
| 5 | git fixture gpgsign/hooks 環境條件 | ❌ 不採納 | 與 s2/s5/build suite 同暴露；先例一致性優先 |
| 6 | description「on PATH」略窄（實際 PATH+~/.local/bin+~/.cargo/bin） | ✅ 採納 | 改「resolvable」；plugin SKILL.md 表同步 |
| 7 | 「since 0.6.0」前向引用 | ✅ 維持 | EP 凍結 0.6.0；發行即閉環 |

**Audit-test 等效性**：新測試品質已由 dual reviewers 覆蓋
（fresh 軸 5「斷言測行為非實作細節、fixture 獨立、21 機械計數
驗證」＋primed 軸 4「無同義反覆、真 sqlite fixture 非 mock、無
mock 假設即 bug」）——等效 /audit-test 審查面，不另 spawn。
