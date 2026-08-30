# EP：主索引 query-time lazy 自癒（staleness 偵測＋auto-heal）

> **ep_type**: implementation
> baseline: `62dee2ae519198174aef0f8eca9be33d13bd7589`
> 來源：2026-08-30 裁決評估（handoff：ai-rules dogfood incident——index 落後 19
> commits＋stamp 缺失＋安裝版 producer 落後 2 版；WARN 有印但 session 仍消費
> stale 資料，refs 行號漂移 `:135`→`:154`、新檔 `mine_corrections` 假陰性）。
> 裁決（凍結，不重辯）：**反對 save-time**（watcher／daemon／hook 三論據成立，
> 見段落 0「裁決記錄」）；採 **query-time lazy 自癒**，且**觸發分級**——原始碼
> 實質漂移→自動重建；僅 HEAD 漂移→WARN-only 不重建（source_line 單一源）。
> **已過 EP Review Cycle**（三 agents F1/F2/F3+4+5，findings 全數採納回寫——
> 見下表；judge 抽樣查證 5/5 成立）。

## EP Review Findings

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| F1-1 | 🔴 | S3 | `ensure_fresh` 簽名（回 `HealOutcome`）與偽碼 `?` 傳播矛盾；walk 失敗行為未定義 | 改 `Result<HealOutcome, String>`；hook 端 Err→WARN＋續查詢（open_face 哲學）；補 SM-15 | implemented |
| F1-2 | 🔴 | S3 | 「fresh no-op 零 spawn」與 Stage A 讀現裝 producer 版本（必 spawn）互斥 | producer 版本探測移出穩態——僅 flagged/heal 路徑探測；穩態＝walk+stats 零 spawn | implemented |
| F1-3 | 🟡 | S3 | false-stale 測試 fixture 形狀未指明：`fake_pyrefly` 無條件複製 rust 形 `rich.scip`，面限定下漏收測試恐構造出走不到 ServeStale 的空測試 | S3 補 fixture 規格（磁碟檔名對齊 index rel_paths＋partial fake）；面限定盲點入 §1b/風險表 | implemented |
| F1-4 | 🟡 | S3 | 非 git repo（`git_head` Err）行為未指定 | `head_drift=None`（無 head 資訊；不重複 WARN——source_line 已有一條）；heal 仍靠 `source_newer`；補 SM-16 | implemented |
| F1-5 | 🟡 | S2+S3 | producer 探測雙報（source_line 每查詢一次＋Stage A 又一次） | 單一探測點（S3 flagged 路徑）；source_line 不加 producer WARN | implemented |
| F1-6 | 🟡 | S3 | SM-9「至多一次重建」缺直接釘死測試 | 補「false-stale fixture 連續兩次 → 第二次 Fresh 且 fake 計數不增」 | implemented |
| F1-7 | 🟡 | S2 | SKIP_DIRS「單一源」未達成：producer `lib.rs:362` 自帶 3 項私有清單 | engine const 內容＝producer 現值（3 項）；producer 改 import（path-dep 既有）；build 側組合＋`target`；明示決策 | implemented |
| F1-8..F1-13 | ℹ️ | 段落0/S1/S2/UC | 錨點與命名修正：`git_init` 實在 `tests/build.rs:123`（非 :11-70）；型別名 `IndexEmitter`（非 `Emitter`）；`first_output_line` 私有／`producer_roots` pub(crate) 需 visibility 調整；`open_face` 第三消費端 `cli.rs:780`；`load_index` 19/24 計數為 b30af73 世代查詢（本地 rg src+tests 約 29 sites）；UC「freshness face 無影響（freshness.rs）」與 S2 子進程 `--version` 設計不符 | 全數修正／標註 | implemented |
| F1-14 | ℹ️ | S1 | 「並發最後者勝」成功標準不可測試 | 改「rename(2) 結構性論證記錄＋code review 確認 rename 路徑」 | implemented |
| F2-2 | 🟡 | S3/收尾 | heal 內部把 `BuildError::{Env,Core}` 降級為 serve-stale WARN（查詢仍 exit 0）——`crates/AGENTS.md` build 段的 Env→2/Core→1 映射變路徑相依，收尾未要求寫明 | 收尾步驟 3 明列「explicit build keeps Env→2/Core→1；heal-internal 兩者皆 serve-stale WARN（open_face 哲學）」 | implemented |
| F2-4 | 🟡 | 全段 | pseudo code 中文註解是最強抄寫源，無「落地註解英文」防護 | 加 authoring guard 行 | implemented |
| F2-5 | 🟡 | S2/收尾 | (a) cli 段 instruction 更新取捨未辯護；(b) SKIP_DIRS「單一源」宣稱過度（見 F1-7） | (a) 收尾明示「cli 段不動，理由：掛點是單一 if 委派非 argv 面」；(b) 併 F1-7 處理 | implemented |
| F2-7 | ℹ️ | S1/S2 | tmp 慣例引用不精確（cache.rs 為無點形）；`doc_set_delta` §2/§3 簽名不一致 | tmp 引用改 `build.rs:255` dot 形（walk 跳 dotfile 有利）；簽名釘死兩參（exts 內部推導） | implemented |
| F3-1 | 🔴 | S2/S3 | 偵測原語未收斂：head/producer 漂移邏輯在 source_line（S2）與 stage_a（S3）雙份實作；source_line 讀版本需 build 零件→engine→build 倒置（engine 現況零 `use crate::`）；穩態每查詢兩次 spawn | S2 交付 `engine::evaluate_staleness` 單一原語（spawn-free）；`resolve_bin`/`first_output_line` 遷 `common`（build 改 import，engine→common 合法向下）；版本探測僅 S3 flagged 路徑 | implemented |
| F3-2 | 🟡 | S1 | `IndexEmitter::write` 非唯一消費端：`overlay-gen.rs:442` 同寫（projections 面）——爆炸半徑陳述錯誤＋無回歸 | 錨點補第二消費端（影響良性：overlay 寫入連帶原子化）；S1 加 overlay-gen 既有測試全綠 | implemented |
| F3-3 | ℹ️ | S2 | meta `producer` 格式描述自相矛盾（`pyrefly-index <ver>+<rev>` vs 首行原樣） | 統一「`--version` 首行原樣（`<ver>+<rev>`）」 | implemented |
| F4-1 | 🟡 | S1 | **rust-face slot 寫入游離於原子性外**：`rust_leg` 把 slot 直傳 `rust-analyzer scip --output`（`build.rs:219-223`）——rust-only repo 並發 heal 可見 torn index，且 protobuf 欄位邊界截斷可**靜默**解析為較短有效索引（mixed 面反有 concat_scip rename 保護） | rust_leg 一律寫 `.rust-part.scip` sibling；單腿→rename 至 slot；mixed→既有 concat。S1 範圍擴為「雙腿寫入原子性」 | implemented |
| F4-2 | 🟡 | S2 | (a) SKIP_DIRS 跨 crate 兩份（見 F1-7）；(b) `.code-reality.toml` profile exclusion 面（root AGENTS.md 宣稱 owns exclusion knowledge）與 producer 語料／偵測 walk 三者關係未述 | (a) 併 F1-7；(b) 明記：profile exclude 只餵 scan/hazard graph 面、不進 scip 語料（producer 不讀 profile），偵測 walk 對稱不讀——pyrefly config 排除差異由 SM-9 迴圈防護兜底（out-of-scope 記錄） | implemented |
| F4-3 | 🟡 | 收尾 | `plugin/skills/code-reality/SKILL.md`（standalone 工具事實真相源，:54/:79 記載手動刷新鏈）與 `README.md:114-132` 記載本 EP 改變的行為，未列入收尾 | 收尾增兩項：SKILL.md（heal 行為＋env opt-out＋手動鏈註記）＋README（一句） | implemented |
| F4-4 | ℹ️ | S3 | env var 不進 `--help`（byte-pinned face）的 tradeoff 未記 | 收尾明記刻意不進（決策記錄） | implemented |
| F4-5 | ℹ️ | 段落0 | producer/lsp-bridge 對 lib 變更的同步分析缺（審查者代驗：零同步） | 補一行分析結論 | implemented |
| F4-6 | ℹ️ | S3 | 測試小缺口：四模式 heal 回歸；SM-12 唯讀 slot 對應測試；poll-timeout 不可測（接受） | 補齊；timeout 標接受 | implemented |
| F5-2 | ℹ️ | SM | 遺漏情境：非 git（併 F1-4）；`--repo` 缺席／`--index` 顯式；heal 中第三查詢（SM-7 語義延伸）；**producer 半成功**（index+stamp 已新但 graph build Err——原設計誤標 ServeStale「以現存索引作答」）；並發人工 build（良性，S1 後 last-writer-wins）＋寫入窗口 race（新 index+舊 meta→誤標 head-drift，窗口窄接受） | 補 SM-17（半成功：build_repo Err 後重評 Stage A，乾淨→`Healed`＋graph 未重建 note）／SM-18（不 heal 情境）；其餘入風險表／註記 | implemented |
| V-1 | 🟡 user 方向 | 全 EP | ep-validate 期間 user 裁決修正：原評估把「存檔就增量更新」讀成 save-time 並將 commit 觸發外推否決——save 與 commit 是不同事件（頻率／狀態 coherent 性／機制：watcher vs git 原生 hook）；commit 粒度 opt-in hook 改採納 | 新增 S4＋SM-19/20＋裁決記錄 2/6 修訂；「增量」語義校準＝commit 觸發採納、更新形態維持全量冪等重產 | implemented |
| R-F1-1 | 🔴 | S4 | 終審：S4 pseudo 落點架構錯——`cli.rs` 是 scip_refs 專屬 tool module（`cli.rs:194-197`），子命令路由＋`SUBCOMMANDS` 在 umbrella bin `main.rs:28-87`，EP 漏列 bin；模組歸屬未裁決 | S4 全段重寫：新 leaf `src/refresh.rs`（ToolSpec，build/project 先例）＋bin 兩 route 臂＋`SUBCOMMANDS` 增項（非 parity face，零斷言）；UC 行補檔案 | implemented |
| R-F1-2/3/5 | 🟡 | S4 | `Ok(Fresh) if head_drift` 不可實現（unit variant）；hook-fired e2e 在 cargo 不可行（tests 不 mutate PATH）；`stamp_meta_mode` 寫 stdout 與「refresh stdout 空」矛盾且為 cli-private | refresh 自呼 `evaluate_staleness` 取 snapshot；e2e 改 bytes 斷言＋函數直測（真 hook→L4）；stamp 寫入核心抽共用 fn（cli 印 stdout／refresh 印 stderr） | implemented |
| R-F2-1/2 | 🟡 | 收尾3/裁決5 | 「cli 段不動」辯護被兩個新 argv 子命令打破；exit 第三路（refresh 恆 exit 0）未裁決 | 收尾 3 改三路徑明文＋cli 段補掛點句＋subcommand 命名 carrier-native 例外＋Usage 段補行；裁決 5 改三路徑 | implemented |
| R-F4-1/2 | 🟡 | S4 | installer 對既有 hook／既有 hooksPath（husky）行為未定義、remove 不還原舊值、hooksPath 停用 `.git/hooks/*` 副作用未記；hook 腳本 PATH 脆弱（GUI no-PATH 已知陷阱） | installer loud 拒絕語義（無 marker hook／外值 hooksPath）＋副作用警示＋僅-own-unset；腳本嵌絕對路徑；風險表補行 | implemented |
| R-F5-1 | 🟡 | SM-20 | checkpoint「hook 失敗→SM-20」但觸發欄無「裝了但失敗」 | SM-20 觸發欄補「或 hook 執行失敗（bin 不可解析含 GUI no-PATH）」 | implemented |
| R-ℹ️ | ℹ️ | S4/風險表 | §1b 兩斷言點、byte-deterministic 措辭（rust 面無依據）、遞迴 call-chain 證據行、re-stamp 不取鎖收窄、env gate 範圍、gitignore 姿態、--help bytes 變更註記、MCP 無 refresh/hook、worktree／index 缺失互動、**EP 追蹤卡滯後（缺 S4／收尾第 4 項）** | 全數併入 S4 重寫＋風險表＋追蹤卡同步 | implemented |

## EP Validate Findings

| POC | EP 段落 | 問題(POC 結果) | 建議 | 狀態 |
|-----|---------|----------------|------|------|
| POC-1 | S2 | ✅ producer 語料實證：dot-dirs＋`__pycache__`/venv/node_modules 全排除、a.py/sub/g.py 收錄、**target/ 內 .py 也收錄**——偵測 walk（同規則）與 python 面語料**精確一致**（超集原則以等式成立） | 風險表「false-stale」行收窄：剩餘面＝pyrefly config 排除（未測）＋rust 面（rust-analyzer 自行發現）；loop-guard 維持 | verified |
| POC-2 | S3/S4 | ✅ `O_CREAT\|O_EXCL` 搶鎖 20 輪恰一勝者；mtime age>10min 可偷＋偷後可重建；fresh lock 二次 acquire 正確拒絕。首跑失敗＝**POC harness bug**（fork child uncaught exception→child 端 TemporaryDirectory finalizer rmtree 共享 tmpdir；child 一律 `finally: os._exit` 後全綠）——非鎖語義失敗 | Rust 端對應紀律已內建（LockGuard Drop 釋鎖）；fork 教訓與 Rust 實作無關 | verified |
| POC-3 | — | ⬇️ dropped（user 2026-08-30：量已知的事——readdir+stat bounded ≈2 syscalls/檔；標的另涉被動 repo hermes-agent 非工作面） | 風險表 walk 成本行已改論證＋退路（快訊號短路） | dropped |
| POC-4 | S2/S3/S4 | ✅ 真 producer＋真 git e2e 5/5：git commit 不動未變更檔 mtime；docs-only commit→`source_newer=False`（SM-4＋S4 re-stamp-only 的機械基礎）；編輯既有檔→True（SM-3）；新增檔→True（SM-2） | 無——核心觸發語義全部實證 | verified |

## 實作總覽

主索引（`<repo>/.code-reality/scip/index.scip`）與原始碼之間目前**沒有任何
query-time 守衛**：既有五守衛只守 cache↔index
（`crates/code-reality/src/cache.rs:192-226`），head 守衛需 stamp 才有
idx_sha（`engine.rs:573-579`），`documents < 100` 閾值又把「缺檔」誤報成
「截斷」（`engine.rs:298-304`）。本 EP 把 cache 層已戰測的 lazy 自癒模式
（`open_face`，`cache.rs:278-338`：stale→WARN＋自動重建→驗證→fallback）
**往上推一層**到 index↔source：

- **S1**：寫入原子性（python `IndexEmitter::write`＋rust leg——F4-1 修正後
  雙腿覆蓋）——auto-heal 使並發 spawn 常態化的正確性前置
- **S2**：staleness 偵測層——`evaluate_staleness` 單一原語（F3-1 收斂）＋
  `walk_sources`／`doc_set_delta` 原語＋`common` 零件遷移＋meta `producer`
  欄位＋WARN-2 退役
- **S3**：auto-heal 編排——`ensure_fresh`（single-flight lock＋重用
  `build_repo`＋heal 後驗證防迴圈＋半成功探針）、cli `scip_refs` 掛點、
  `CODE_REALITY_AUTOHEAL=off` opt-out
- **S4**：commit 粒度背景刷新（user 2026-08-30 裁決修正——save≠commit；
  opt-in `hook install` → post-commit 背景 `refresh`）——S3 降級為安全網

heal 範圍＝**scip_refs 工具家族**（query／audit／callers／closure——掛點在
mode 分支前）。graph face（graph_query 家族讀 graph.db）**不在本 EP**——
graph_engine 無 staleness 守衛（rg 證實零 `stale|mtime` 命中），留顯式
`build`（defer，見裁決記錄）。

**Authoring guard（F2-4）**：本 EP 各段 pseudo code 的中文註解僅供 EP 內部
說明；落地 code comments／docstrings／commit messages **一律英文**——中文
僅限輸出字串字面值（stdout/stderr 訊息，byte-parity face 豁免）。

---

## UC 盤點

### 掃描範圍

- root `AGENTS.md` Capabilities 表（repo root）
- `.kanban/{Backlog,In-Progress,Done}`（Backlog 原為空）
- `SYSTEM-MAP.md`：**不存在**——本 repo 為工具 repo，無跨域功能狀態面，
  暫不建立（skill 提醒已記；user 可覆蓋）

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Symbol truth query（scip_refs 家族） | ✅ | root AGENTS.md Capabilities | 更新 | 查詢路徑新增 pre-query heal hook（行為面變更） |
| build 傘形（data-plane bootstrap） | ✅ | 同上 | 更新 | `build_repo` 被 heal 重用（介面不變）；rust leg 寫入路徑改 sibling+rename；`ensure_fresh` 新鄰居 |
| Binary freshness face | ✅ | 同上 | 無影響 | meta `producer` 欄位取自**子進程 `pyrefly-index --version` 首行**（build 零件，非 `freshness.rs::version_face`——那是 code-reality 自家 bin 面；F1-13 修正） |

### 新增 UC

| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| 主索引 query-time 過期偵測＋lazy 自癒（scip_refs 家族自動觸發；env opt-out）＋commit 粒度背景刷新（opt-in `hook install`／`refresh`） | 📋 | `crates/code-reality/src/{common,engine,build,cli,refresh}.rs`＋`src/bin/code-reality/main.rs`（routing）＋`crates/pyrefly-producer/src/{emit.rs,lib.rs}` |

### Backlog 關聯

- Backlog 原空 → 已建卡：能力卡 `index-query-time-self-heal.md`＋EP 追蹤卡
  `ep-index-query-time-self-heal.md`（`[infra]` 標籤，沿 Done 卡慣例）

---

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 新鮮索引查詢（穩態） | docs／mtime／head 全綠 | no-op、無 WARN、正常延遲——Stage A＝`evaluate_staleness`（walk+stats+meta 讀，**零 spawn、零 protobuf parse**；F1-2/F3-1 修正後 producer 版本不探測） | 無 | 主索引 lazy 自癒 |
| SM-2 | 缺檔漂移（incident 形態） | 新增 .py 後直接查詢 | Stage A 旗標→auto-heal（重產+stamp）→查詢以新索引作答＋`[OK]` heal 行 | heal 失敗→SM-6 | 同上 |
| SM-3 | 既有檔編輯（行號漂移） | 檔案 mtime > index mtime、doc-set 不變 | auto-heal（incident 的 `:135`→`:154` 形態） | 同 SM-2 | 同上 |
| SM-4 | 僅 HEAD 移動（docs-only commit） | stamp head ≠ repo HEAD、source_newer=false | WARN-only（**source_line 單一源**，`engine.rs:573-579` 既有行）不重建、零 spawn | 無 | 同上 |
| SM-5 | producer 版本錯配 | meta `producer` ≠ 現裝（**僅在已進入 flagged/heal 路徑時探測**——F1-2/F1-5 修正：穩態零 spawn；無實質漂移時不觸發，binary freshness face 的 per-process WARN 另有覆蓋） | heal 結果附帶 WARN 行（升級訊號——incident 根因一 loud 化） | 升級安裝後消失 | 同上 |
| SM-6 | heal 失敗（producer 不在 PATH／spawn exit≠0） | flagged 路徑 build_repo Err 且重評仍 stale | serve stale＋loud WARN，**查詢仍回答**（open_face 哲學） | 裝 producer 後自癒 | 同上 |
| SM-7 | 並發 stale 命中 | 兩（或多）session 同時查同一 stale slot | single-flight：一者重建、其餘持鎖等待→釋放後重評→直接用（HealedByPeer）；第三查詢走同一 lock-poll 路徑 | 無 | 同上 |
| SM-8 | lock 殘留（healer crash） | `.heal.lock` 存在但 owner 死亡 | age 逃逸（mtime > 10min → steal）後正常 heal | 無 | 同上 |
| SM-9 | false-stale（偵測≠producer 語料） | SKIP_DIRS walk 與 pyrefly 語料不一致（含 pyrefly config 排除目錄形態） | heal 後重評仍旗標→**WARN-once serve，不迴圈**；連續第二次查詢→Fresh、零重建 | 修偵測語義 | 同上 |
| SM-10 | 明確 opt-out | `CODE_REALITY_AUTOHEAL=off` | 舊行為（不 heal、零 spawn）——CI／唯讀場景 | 無 | 同上 |
| SM-11 | index 缺失（首跑未 build） | slot 不存在 | 現行 FAIL＋提示**不變**（v1 不做 bootstrap-on-miss——保留 sidecar_migrate 提示流） | 跑 `build` | — |
| SM-12 | 唯讀 slot／寫入失敗 | heal 寫入拋錯（chmod 555 slot dir／寫入失敗 fake） | serve stale＋WARN（自然退化，無特例） | 修權限 | 同上 |
| SM-13 | MCP 面 stale 查詢 | MCP tool → cli::run（`mcp_server.rs:267`） | 與 CLI 同行為（單一後端）；**已知風險**：mosaic 級 heal ~78s 可能觸發 client timeout（env opt-out 為解；budget 機制 defer） | 無 | 同上 |
| SM-14 | 小 repo WARN-2 噪音 | healthy 小 repo（17 檔全收錄） | `documents < 100` 啟發式退役→靜默；缺檔時改精確訊息（磁碟 M vs 索引 D） | 無 | 同上 |
| SM-15 | staleness 檢查失敗 | walk/stat 失敗（`evaluate_staleness` Err——F1-1） | `[WARN] 索引過期檢查失敗（…）——本次查詢以現存索引作答`，查詢續行 exit 0 | 修權限後自癒 | 同上 |
| SM-16 | 非 git repo／git_head 失敗 | `git_head` Err（engine.rs:433-449） | `head_drift=None`（無 head 資訊，不額外 WARN——source_line 已有一條）；heal 仍由 `source_newer` 驅動；stamp 缺席（`--stamp-meta` 恆 FAIL）→ head/producer 面永久缺席屬既有行為 | git init | 同上 |
| SM-17 | producer 半成功 | `build_repo` 在 index+stamp 完成後、graph build 失敗（`build.rs:374` Core Err——F5-2d） | **重評 Stage A**：乾淨→`Healed`＋note「graph 未重建（{e}）」（不誤標 serve-stale）；仍旗標→ServeStale | 顯式 build 補 graph | 同上 |
| SM-18 | 不 heal 的查詢形態 | `--repo` 缺席（cli.rs:263-266 先 FAIL）／`--index` 顯式路徑（`default_resolved=false`） | 掛點 guard 跳過，現行為 | 無 | — |
| SM-19 | commit 邊界背景刷新（hook 已裝，S4） | post-commit hook → 背景 `code-reality refresh --repo` | source 變→全量重產（下次查詢 Fresh、零 heal 延遲——SM-13 timeout 風險大減）；僅 head 移動（docs-only commit）→**只 re-stamp**（[SRC] 保新、不付重產） | hook 失敗→SM-20 | 主索引 lazy 自癒 |
| SM-20 | hook 缺席**或失敗**（未裝／`--no-verify`／新 clone／bin 不可解析含 GUI no-PATH 殘餘） | commit 後直接查詢 | S3 接手（既有 SM-2/3 行為）——安全網語義（R-F5-1：觸發欄涵蓋「裝了但失敗」） | 裝 hook／修 bin | 主索引 lazy 自癒 |

效能期待差異：ai-rules 規模全量 producer 實測 **0.87s**（17 files／211 defs，
2026-08-30 probe，外部數據未由審查複驗）；mosaic 級 ~78s（S2 dogfood 既有
數字）——heal 代價＝每次實質漂移付一次，single-flight 防 stampede。

---

## 段落劃分原則

依賴鏈：S1（producer＋build 雙腿寫入，獨立）→ S2（偵測層，engine／common／
cli）→ S3（編排，依賴 S1 原子性＋S2 評估原語）→ S4（commit 粒度刷新，
依賴 S3 `ensure_fresh`＋S2 `evaluate_staleness`）。垂直切片：每段獨立可
驗證、可 commit。收尾段最後。

---

## 段落 0：全域研究摘要

### 可複用基礎設施

- **lazy 自癒模板**：`open_face`（`cache.rs:278-338`）——no-db→protobuf
  （miss 不建）、fresh→sqlite、stale→WARN＋自動重建→驗證→失敗回退；五守衛
  `stale_reason`（`cache.rs:192-226`）。S3 直接照此形上推一層。
- **`build_repo`**（`build.rs:262-381`）：語言偵測→spawn producer→in-process
  stamp（`:347-363`）→graph_db build＋ensure_indexes。heal 重用整條（單一
  已測路徑）。
- **bin 解析＋版本讀取**：`resolve_bin`（`build.rs:142`，pub）／
  `producer_roots`（`build.rs:156`，pub(crate)）／`first_output_line`
  （`build.rs:169`，**私有**——S2 遷 common 時補 pub，F1-10）——S2/S3 的
  producer 版本零件。
- **rev-embed 面**：`pyrefly-index --version` 已印 `<pkg>+<rev>`
  （`crates/pyrefly-producer/src/bin/pyrefly-index.rs:26-34`，
  `CR_BUILD_REV`）；`freshness.rs:21-56`（code-reality 自家 bin 面）。
- **tmp+rename 原子寫先例**：`cache.rs:81-139`（無點 `<name>.tmp`）與
  `build.rs:252-258`（dot 形 `.merge-tmp.scip`）——S1 取 **dot 形**（walk
  跳 dotfile，`build.rs:111`；引用改以此為準——F2-7a 修正）。
- **測試基礎設施**：`tests/build.rs`——`fake_bin`@:11／`mkrepo`@:18／
  `fake_pyrefly`@:31（**無條件複製 `tests/fixtures/rich.scip`**——rust 形，
  documents 為 `crates/a.rs`／`crates/b.rs`；`--version` 回
  `fake-pyrefly 9.9.9`）／`fake_rust_analyzer`@:50／`git_init`@:123（F1-8
  錨點修正）；t1-t12 模式。
- **meta 斷言形態**：substring 斷言（`tests/s4_cli.rs:172-175` 皆
  `contains`）——meta 加 `producer` 鍵不破壞既有測試（已驗證）。
- **producer／lsp-bridge 同步面**（F4-5，審查代驗）：producer crate 對 lib
  的依賴＝`engine::{default_index_path,meta_path}`＋`cache::sqlite_path`＋
  `fndefs::fndefs_path`（`pyrefly-producer/src/lib.rs:215-217`＋Cargo.toml
  path-dep）——S2 engine 變更全為加法，零同步需求；lsp-bridge 的
  `stale_binary_warn` 本地副本條款不觸發（S2 不動 freshness.rs）。

### 依賴錨點（定義端／消費端）

| symbol | 定義 | 消費 |
|---|---|---|
| `load_index` | `engine.rs:290` | `cli.rs:413,471,591,661`；`cache.rs:282,299,311`；本地 rg src+tests 約 29 sites（CR 圖查詢 19/24 為 b30af73 世代——F1-12 標註） |
| `open_face` | `cache.rs:278` | `cli.rs:383`（query）；`:578`（callers ladder）；`:780`（audit 兩段式面——F1-11 補） |
| `stale_reason` | `cache.rs:192` | `cache.rs:291`（S2 模式源，不直接呼叫） |
| `source_line` | `engine.rs:504` | `cli.rs:336,358`（WARN 面） |
| `stamp_meta_mode` | `cli.rs:421` | `cli.rs:314-315`；`build.rs:352`（in-process stamp） |
| `build_repo` | `build.rs:262` | `build.rs:452`（run）；S3 `ensure_fresh`（新） |
| `python_leg`／`rust_leg` | `build.rs:182,210` | `build_repo`（rust_leg 寫入路徑 S1 改造） |
| `resolve_bin`／`producer_roots`／`first_output_line` | `build.rs:142,156,169`（S2 前 xxx 遷 common——F3-1） | legs＋tests（t2 import 路徑隨遷） |
| `count_sources`／`SKIP_DIRS` | `build.rs:101,60` | `build.rs:128`——S2 遷 engine 並跨 crate 單一源（見 S2 要點） |
| `IndexEmitter::write` | `emit.rs:118`（型別名 `IndexEmitter`——F1-9） | `pyrefly-producer/src/lib.rs:208`＋`overlay-gen.rs:442`（**第二消費端**，F3-2——projections 面，連帶原子化屬良性） |
| `default_index_path` | `engine.rs:332` | cli／build／producer 三方 |
| `git_head` | `engine.rs:433` | `source_line`；`stamp_meta_mode` |

### 裁決記錄（凍結，實作時不重辯）

1. **反 save-time 三論據**（2026-08-30 裁決評估全驗）：(a) 治錯藥——incident
   是偵測缺口非速度缺口，WARN-only 已被證實會被 session 忽略且
   `engine.rs:550-553` 補救指引誤導（過期 index 上指向 re-stamp）；(b) partial
   增量正確性——occurrence 索引全程式視野，跨世代混存＝`009e6a1` identity
   guard 剛清除的靜默 join 失效物種，且全庫無增量 merge 機械（唯二 merge 是
   同世代 cat-merge 與帶 guard 的 overlay）；(c) daemon 同型於已清除的失敗
   ——CRG 常駐類別退役史＋user 已否決 hook 侵入＋worktree 增殖。
2. **永不做**：per-repo watcher／daemon／任何 **per-save** 常駐面；**靜默**
   auto-install hook 進消費端（明示 opt-in `hook install` 於 S4 採納——
   user 2026-08-30 指示覆蓋舊否決；舊否決針對靜默侵入形）。
3. **Defer**：graph face query-time 自癒（graph_engine 無 staleness 守衛，
   消費場景重流程，留顯式 build）；heal 瘦身（producer-only 不重建 graph——
   v1 用 `build_repo` 單一路徑，graph 重建成本隨同次漂移一次付清）；MCP heal
   timeout budget；bootstrap-on-miss（SM-11 現行 FAIL 保留）。
4. **觸發分級**：`source_newer`（實質漂移）→auto-heal；head-drift 單獨出現
   →WARN-only（source_line 單一源）；producer-drift→flagged 路徑附帶 WARN
   （穩態零 spawn——F1-2/F1-5 修正）。
5. **exit-semantics 三路徑**（F2-2＋R-F2-2）：explicit `build` 保持
   Env→fail(2)／Core→crash(1)；heal-internal 兩者皆降級 serve-stale WARN
   （查詢仍 exit 0，open_face「answering beats a traceback」哲學）；
   **S4 `refresh`＝背景面設計，恆 exit 0**（補救＝stderr＋SM-20 網；
   前景手動使用者讀 stderr）——收尾步驟 3 必須把三路徑寫進
   `crates/AGENTS.md` build 段。
6. **commit ≠ save**（user 2026-08-30，ep-validate 期間方向修正）：三論據
   瞄準 save-watcher/daemon 類；commit 粒度＝低頻 coherent checkpoint＋git
   原生事件（hook 事件驅動、零常駐）——S4 opt-in hook **採納**。論據重新
   適用：治錯藥在有 S2/S3 偵測層下失效（hook＝預熱最佳化，偵測網仍在）；
   partial-merge 論據與時點無關（更新形態維持全量冪等重產，per-file 拼接
   任何觸發時點都排除）；daemon 論據不適用（無常駐進程）。

### 風險假設

| 等級 | 假設 | 處置 |
|---|---|---|
| 高 | doc-diff 偵測 walk 與 producer 語料（pyrefly `default_config_finder`→`Handles::all`，`api.rs:77-84`；producer 自 walk 亦跳 dot-dirs＋3 skip dirs，`lib.rs:361-371`）不一致 → false-stale | 面限定比較＋heal 後重評 WARN-once 不迴圈（SM-9）＋**偵測 walk 必須是 producer 語料的超集**（skip 較少→false-stale 安全側；絕不 skip 更多→false-fresh 危險側）；ep-validate POC：構造 pyrefly 排除目錄的 repo |
| 高 | std-only O_EXCL lock 的 steal 語義（crash 殘留、竊取競態） | age 逃逸（>10min）＋S3 測試覆蓋（殘留 lock／並發搶鎖） |
| 中 | **rust-face torn index**（F4-1 原始風險）：`rust_leg` 直寫 slot＋protobuf 欄位邊界截斷可靜默解析為較短有效索引 | S1 擴範修正（sibling+rename）——風險關閉；S1 驗證含 rust-leg 原子性測試 |
| 中 | **面限定盲點**（F1-3）：index 語言面與 repo 語言錯配（python repo＋rust 形 index）時 doc-set 恆空差異→誤判 Healed | 已知 v1 邊界（§1b 註記）；ep-validate 以 CR 自倉 mixed 形態 L4 覆蓋正確面；錯配形態靠 head-drift WARN 兜底 |
| 中 | MCP 長時 heal（mosaic ~78s）觸發 client timeout | SM-13 已知風險標注＋env opt-out；budget 機制 defer |
| 中 | NT 規模 Stage A walk 成本 | readdir+stat only、零 spawn、零 parse；bounded ≈ 2 syscalls/檔（量測 POC 砍——user 2026-08-30：量已知的事）；若實測過慢的退路＝head/mtime 快訊號短路 |
| 中 | mtime 回撥（checkout 舊版檔案帶舊 mtime）→偽 fresh | v1 接受；stamp 在場時 head-drift WARN 兜底（[SRC] 不說謊） |
| 低 | meta 加 `producer` 欄位破壞測試 | substring 斷言已驗證不破壞 |
| 低 | `可能截斷` WARN 退役衝擊測試 | rg 證實僅 `engine.rs:301` 一處、零測試引用 |
| 低 | 並發人工 `build` 與 heal 同跑（人工 build 不取鎖——F5-2e） | S1 原子性後＝last-writer-wins＋graph_db temp+rename，良性；記錄接受 |
| 低 | 寫入窗口 race：查詢落在 producer 寫 index 後、sidecar 失效/stamp 前（`lib.rs:208-232` 窗口，F5-2f） | 新 index+舊 meta→誤標 head-drift WarnOnly；窗口窄（毫秒級），記錄接受 |
| 低 | hook 預熱使 flagged-path 的 producer-drift 附註更少出現（S4 副作用） | 版本錯配主要由 binary freshness face（per-process WARN）承擔；S4 已註記 |
| 低 | hook 與查詢 heal／人工 build 併發（S4） | **rebuild 分支**走 `ensure_fresh` 同鎖單飛；head-sync re-stamp 分支不取鎖（meta last-writer-wins，與人工 build 同類 benign——R-F3-3）；人工 build 不取鎖（S1 後 last-writer-wins，既有 benign 行） |
| 低 | GUI git client 觸發 hook 時 bin 不可解析（已知陷阱類：lsp-bridge 為此設計純解析鏈，root AGENTS.md:77） | S4 hook 腳本嵌 install 時**絕對路徑**消除 PATH 面（R-F4-2）；殘餘失效（bin 缺失）＝silent no-op→SM-20 網接手 |

---

## S1：寫入原子性（python emit＋rust leg——F4-1 擴範）

### Context

**背景**：python 面 `IndexEmitter::write` 是裸 `std::fs::write`
（`emit.rs:118-128`）；**rust 面更危險**——`rust_leg` 把 slot 直傳
`rust-analyzer scip --output`（`build.rs:219-223`），且 protobuf 欄位邊界
截斷可**靜默**解析為較短有效索引（mixed 面反有 `concat_scip` tmp+rename
保護）。今天兩者只由人工 `build` 觸發所以窗口小；S3 讓 auto-heal 在查詢路徑
spawn producer 後，多 session 並發寫同一 slot 成為常態。

**UC 引用**：實作「主索引 query-time 過期偵測＋lazy 自癒」的正確性地基。

**依賴關係**：無上游；S3 依賴本段（原子性是並發 heal 的前提）。

**語義約束**：與 S3 共享——tmp 檔名 dot 形 `.{name}.tmp`
（`build.rs:255` `.merge-tmp.scip` 形；dot 前綴使 walk 跳過——S2
`walk_sources` 同靠 dot-skip）；寫入失敗清理 tmp（crash 殘留由下次寫入前
防禦性 remove 處理，`cache.rs:85` 同形）。

**基礎設施盤點**：tmp+rename 先例×2（`cache.rs:81-139`、`build.rs:252-258`）
——直接對齊；無需新依賴。

**依賴錨點**：`IndexEmitter::write` → 定義 `emit.rs:118`／消費
`pyrefly-producer/src/lib.rs:208`＋`overlay-gen.rs:442`（第二消費端——
projections 面，連帶原子化屬良性）；`rust_leg` → 定義 `build.rs:210`／消費
`build_repo`（`:319` 單腿直寫 slot——本段改造點）。

**技術選型**：temp-sibling＋`fs::rename`（rename(2) 不可分割——讀者要嘛全
舊要嘛全新）。**成功標準**（F1-14 修正）：三條寫入路徑（python emit／rust
單腿／mixed concat）皆 tmp+rename；成功後無 `.tmp` 殘留、slot 可 parse；
「並發寫最後者完整勝出」為 rename(2) 結構性論證——記錄於此＋code review
確認三條路徑皆走 rename，不做不可靠的 race 測試。

### 1b. Invariant Impact

- **受影響 domain invariant**：index 檔案完整性（讀者永不見半寫狀態——
  特別是 rust 面「截斷但可解析」的靜默形態）。
- **critical path 觸及**：slot 寫入路徑（python producer 面＋rust leg＋
  overlay 附帶）。
- **驗證對齊**：S1 驗證「三路徑 rename 斷言＋無殘留＋可 parse」。

### 2. 核心實作要點

- `IndexEmitter::write()`（emit.rs）：encode→寫 `.{name}.tmp` sibling→
  `rename` 至目標；rename 失敗清理 tmp 再回 Err。簽名／輸出／錯誤字串形態
  不變（凍結面零變更；overlay-gen 第二消費端連帶受益）。
- `rust_leg`（`build.rs:210-247`）：`--output` 一律指向
  `.code-reality/scip/.rust-part.scip` sibling；寫入成功＋空索引守衛通過後
  ——單腿（Rust repo）：`rename(part, slot)`；mixed：走既有
  `concat_scip`（`build.rs:252-258`，本身已 tmp+rename）。失敗路徑 remove
  part（既有 `build.rs:333` 同形）。
- 三條路徑共用一個 4-行 helper（`atomic_rename(tmp, dst)`）或各自內聯——
  實作時擇一，避免第三種寫法（衝突寫法禁止混合）。

### 3. Pseudo Code

```
crates/pyrefly-producer/src/emit.rs
  pub fn write(&self, path) -> Result<(), String> {
      mkdir parent; bytes = encode()?;
      let tmp = path.with_file_name(format!(".{}.tmp", file_name));
      let _ = fs::remove_file(&tmp);            // crash-leftover defense (cache.rs:85 form)
      fs::write(&tmp, bytes)?;
      match fs::rename(&tmp, path) { Ok(()) => Ok(()), Err(e) => { cleanup(tmp); Err(...) } }
  }

crates/code-reality/src/build.rs  rust_leg()
  let rs_part = slot_dir.join(".rust-part.scip");
  run rust-analyzer scip --output rs_part ...;   // always the sibling, never the slot
  empty-index guard (<128B) on rs_part;          // unchanged
  if mixed { concat_scip(&slot, &rs_part)? }     // existing tmp+rename path
  else      { fs::rename(&rs_part, &slot)? }     // NEW: single-leg atomicity
  let _ = fs::remove_file(&rs_part);             // no-op on single-leg, cleans on mixed
```

Call Stack 不變：`pyrefly_producer::emit()`→`IndexEmitter::write()`→sidecar
失效迴圈（`lib.rs:214-232`）；`build_repo`→`rust_leg`→rename。

### 4. 驗證策略

- 單元（`crates/pyrefly-producer/tests/end_to_end.rs` 增）：write 成功後無
  `.tmp` 殘留、slot 可 `Index::parse_from_bytes`；目標已有舊內容時覆蓋正確。
- `tests/build.rs` 增：rust 單腿 e2e（t6 形態擴充）——slot 由 rename 落地、
  無 `.rust-part.scip` 殘留；失敗路徑（fake_rust_analyzer exit 1）不留 part。
- **overlay-gen 回歸**（F3-2）：`tests/project.rs` 既有測試全綠（第二消費端
  行為不變＋連帶原子化）。
- 已知未覆蓋：跨檔案系統 rename（tmp 與目標同目錄，不可能觸發）；並發
  last-writer-wins 為結構性論證（見成功標準）。

---

## S2：staleness 偵測層（evaluate_staleness 單一原語＋WARN 面＋meta producer 欄位）

### Context

**背景**：incident 的偵測缺口：(1) `documents < 100`（`engine.rs:298-304`）
在小 repo 永存噪音且措辭誤導（真截斷本來就會在 `load_index` parse loudly
失敗，`engine.rs:292-294`）；(2) 未 stamp 時 `engine.rs:550-553` 的補救指引
在 index 已過期時是錯誤指引；(3) producer 版本無記錄。審查修正（F3-1）：
偵測必須收斂為 **engine 內單一原語**——原設計讓 source_line 呼叫 build 的
版本零件會引入 engine→build 倒置（engine 現況零 `use crate::` 匯入），且
每查詢雙 spawn。

**UC 引用**：實作「主索引 query-time 過期偵測」（能力的前半：偵測＋可見性）。

**依賴關係**：上游無；S3 消費本段原語
（`evaluate_staleness`／`doc_set_delta`／`walk_sources`）。

**語義約束**：與 S3 共享——`StalenessSnapshot` 結構形態；面限定比較語義
（只比 index 已收錄副檔名集合）；meta `producer` 欄位格式＝**`--version`
首行原樣（`<ver>+<rev>`）**（F3-3）；**穩態零 spawn 原則**——
`evaluate_staleness` 絕不 spawn 子進程（版本探測是 S3 flagged 路徑的職責）。

**基礎設施盤點**：`resolve_bin`／`first_output_line`（`build.rs:142,169`）
遷 `common.rs`（build 改 import；`tests/build.rs` t2 import 路徑隨遷——
F3-1）；`SKIP_DIRS` 跨 crate 單一源（見要點）；`load_meta`／`git_head`
（engine 既有）。

**依賴錨點**：`SKIP_DIRS` → 定義 `build.rs:60`＋producer `lib.rs:362`（兩份
現況）／消費 `build.rs:111`＋producer `collect_py_files` `lib.rs:370`——本
段統一至 `engine::SKIP_DIRS`；`stamp_meta_mode` → 定義 `cli.rs:421`／消費
`cli.rs:314`＋`build.rs:352`；`source_line` → 定義 `engine.rs:504`／消費
`cli.rs:336,358`。

**技術選型**：Stage A（`evaluate_staleness`：walk＋stats＋meta 讀——零
spawn 零 parse）與 Stage B（`doc_set_delta`：需 loaded index）分離——穩態
查詢只付 Stage A，Stage B 由 heal 路徑在旗標後執行。**成功標準**：healthy
小 repo 靜默（SM-14）；incident 形態（新增檔）被 Stage A 旗標；穩態 scip_refs
查詢零子進程 spawn（fake-bin 計數器可證）。

### 1b. Invariant Impact

- **受影響 domain invariant**：[SRC] provenance 誠實性——WARN 面不得把
  stale 資料標成新鮮；補救指引必須指向正確動作；偵測原語單一源（雙份實作
  ＝ drift 溫床）。
- **critical path 觸及**：silent-corruption path（stale 索引被消費而無警示
  ——本 EP 的存在理由）。
- **驗證對齊**：S2 驗證「WARN 面逐情境斷言」＋`evaluate_staleness` 單元
  矩陣（fresh／缺檔／編輯／head-drift／非 git——SM-1/2/3/4/16 對應斷言）。

### 2. 核心實作要點

- **`common.rs` 遷入**（F3-1）：`resolve_bin`（原 `build.rs:142`）、
  `first_output_line`（原 `:169`，補 pub）；新增
  `pub fn producer_version(name: &str, roots: &[PathBuf]) -> Option<String>`
  （resolve＋`--version` 首行；找不到→None）。build.rs 改
  `use crate::common::{resolve_bin, first_output_line}`；engine 不反向依賴
  build（分層保持：common foundation ← engine domain ← build orchestration）。
- **`engine.rs` 新增**：
  - `pub const SKIP_DIRS: &[&str] = &["__pycache__", "venv", "node_modules"];`
    ——**內容＝producer `lib.rs:362` 現值**（跨 crate 單一源，行為零變更）；
    producer 改 `use code_reality::engine::SKIP_DIRS` 刪本地常數（path-dep
    既有）；`build.rs count_sources` 改用 engine 版＋本地補 `"target"`
    （rust build 噪音，現行 4 項行為保留）。**超集原則**：偵測 walk 的 skip
    集必須 ⊆ producer 的 skip 集（skip 較少→false-stale 安全側；絕不 skip
    更多→false-fresh 危險側）。dot-dirs 兩側都跳（producer
    `lib.rs:370`／build `build.rs:111` 同形）。
  - `pub struct SourceWalk { pub py: BTreeSet<String> /*rel*/, pub rs: BTreeSet<String>, pub newest: Option<SystemTime> }`
  - `pub fn walk_sources(repo: &Path) -> Result<SourceWalk, String>`（一次
    walk 收集集合＋最新 mtime；dot-dirs＋SKIP_DIRS 跳過；Err＝fail-loud）
  - `pub struct StalenessSnapshot { pub source_newer: bool, pub head_drift: Option<bool> }`
  - `pub fn evaluate_staleness(repo: &Path, slot: &Path) -> Result<StalenessSnapshot, String>`
    ——`walk_sources` newest vs slot mtime；`load_meta` head vs `git_head`
    （**head_drift=None**＝meta 未 stamp 或 git_head Err——SM-16；不產生
    WARN，source_line 已有單一源 WARN）。**零 spawn／零 protobuf parse**。
  - `pub fn doc_set_delta(docs: &BTreeSet<String> /*rel*/, walk: &SourceWalk) -> DocDelta`
    （兩參簽名釘死——F2-7b；exts 由 docs 內部推導＝面限定）；
    `DocDelta { missing: usize, examples: Vec<String>, extra: usize }`
- **`engine.rs` WARN 面**：
  - **退役** `documents < 100` 啟發式（`engine.rs:298-304` 刪；`LoadedIndex.stderr`
    欄位保留）——精確缺檔訊息由 S3 heal 路徑產生（磁碟 M vs 索引 D）
  - `engine.rs:550-553` 補救指引分流：未 stamp 時以 `walk_sources` cheapest
    檢查（newest > index mtime？）——漂移→「索引過期——跑 code-reality
    build」；未漂移→原 `--stamp-meta` 指引
  - source_line **不加** producer-drift WARN（F1-5：單一探測點在 S3 flagged
    路徑）
- **`cli.rs` `stamp_meta_mode`**：payload 增
  `"producer": <pyrefly-index --version 首行>`（`common::producer_version`；
  None→`"<unresolved>"`——provenance 盡力而不 crash）；stdout 面
  （`meta stamped：...`）不變。

### 3. Pseudo Code

```
crates/code-reality/src/common.rs   // moved from build.rs (F3-1)
  pub fn resolve_bin(...)                      // unchanged body
  pub fn first_output_line(...)                // +pub
  pub fn producer_version(name, roots) -> Option<String>

crates/code-reality/src/engine.rs
  pub const SKIP_DIRS: &[&str] = &["__pycache__", "venv", "node_modules"];
  pub fn walk_sources(repo) -> Result<SourceWalk, String>
      // readdir loop (count_sources form) + per-file mtime; dot-dirs + SKIP_DIRS skipped
  pub fn evaluate_staleness(repo, slot) -> Result<StalenessSnapshot, String>
      // walk.newest > slot mtime → source_newer
      // meta.head vs git_head → head_drift: Option<bool>   (None = unstamped | git absent)
      // ZERO spawn, ZERO protobuf parse
  pub fn doc_set_delta(docs: &BTreeSet<String>, walk) -> DocDelta
      // exts = extensions present in docs (face-scoped); disk set filtered to same exts
      // missing = disk - docs; extra = docs - disk

crates/code-reality/src/cli.rs  stamp_meta_mode()
  payload += { "producer": producer_version("pyrefly-index", roots).unwrap_or("<unresolved>") }

crates/code-reality/src/engine.rs  source_line()   // remediation split only
  unstamped && walk newer → "index stale — run code-reality build"
  unstamped && !newer     → existing "--stamp-meta" guidance
```

Call Stack：`cli run()`→`source_line()`（指引分流，零 spawn）；stamp 模式→
`common::producer_version`（spawn 一次，僅 stamp 時）。

### 4. 驗證策略

- 單元（`tests/s2_engine.rs` 增）：`walk_sources` fixture（SKIP_DIRS／dot-dir
  ／深層／`newest` 取最大／read 失敗→Err）；`evaluate_staleness` 矩陣——
  fresh／新增檔（source_newer）／編輯（mtime）／head-drift（stamp 後
  commit）／非 git（head_drift None）；`doc_set_delta`——缺檔／多檔／面
  限定（docs 僅 .py 時 .rs 不計）／改名（missing+extra 同時）。
- `tests/build.rs` t2：`resolve_bin` import 路徑隨遷更新（行為不變）。
- `tests/s4_cli.rs` 增：stamp 後 meta 含 `"producer"`；指引分流兩分支訊息
  斷言；`documents < 100` 退役（rg 已證實零測試引用，若 s2/s4 有殘留斷言
  刪除）。
- 已知未覆蓋：`<unresolved>` 分支（producer 不在 PATH 的 stamp）——手動 L4
  記錄；walk symlink 行為（std `file_type` 不跟隨，記錄）。

---

## S3：auto-heal 編排（ensure_fresh＋single-flight＋cli 掛點）

### Context

**背景**：偵測（S2）只給可見性；incident 證明 WARN-only 不足（session 看到
WARN 仍消費 stale 資料）。本段把 `open_face` 模板（`cache.rs:278-338`）上推
一層：stale 命中→auto-rebuild→驗證；失敗→serve stale＋loud WARN（查詢永不
因 heal 失敗而擋死——現行為的嚴格改善）。

**UC 引用**：完成「主索引 query-time 過期偵測＋lazy 自癒」。

**依賴關係**：S1（原子寫：並發 spawn 常態化的前提）＋S2（
`evaluate_staleness`／`walk_sources`／`doc_set_delta`／
`common::producer_version`）；重用 `build_repo` 全鏈。

**語義約束**：與 S2 共享——面限定比較、meta `producer` 格式、穩態零 spawn；
lock 檔 `.code-reality/scip/.heal.lock`（O_EXCL create，內容
`pid iso-time`）；env `CODE_REALITY_AUTOHEAL=off`（命名對齊
`CODE_REALITY_BOOTSTRAP`）；**遞迴防護**——heal 掛點必須在寫入模式
early-return（`cli.rs:314-319`）**之後**（`build_repo` 經 `cli::run` 呼叫
`--stamp-meta` 分支（`build.rs:352`→`cli.rs:314-316`），該分支在掛點前
返回→無遞迴；F3-7 經三 agent 獨立 call-chain 實查成立）；**WARN 單一源**——
head-drift WARN 屬 source_line，ensure_fresh 不重複印；producer-drift WARN
僅本段 flagged 路徑產生。

**基礎設施盤點**：`build_repo`（`build.rs:262`）；`open_face` 模板形；
`tests/build.rs` fake-bin 注入（`fake_bin`／`fake_pyrefly`——**注意
fixture 形狀**：無條件複製 rust 形 `rich.scip`（documents＝
`crates/a.rs`／`crates/b.rs`），`--version` 回 `fake-pyrefly 9.9.9`——
F1-3）。

**依賴錨點**：`build_repo` → 定義 `build.rs:262`／消費 `build.rs:452`＋S3
`ensure_fresh`（新）；`scip_refs run()` 寫入模式分支 → `cli.rs:314-319`
（掛點緊鄰其後）；`mcp_server.rs:267` `cli::run` thin-wrap——MCP 面自動
覆蓋（零改動）。

**技術選型**：single-flight 用 std-only O_EXCL lockfile（無新依賴；age 逃逸
處理 crash 殘留）；heal 重用 `build_repo` 整鏈（單一已測路徑；graph 重建
成本隨同次漂移一次付清——瘦身變體 defer）。**成功標準**：SM-1/2/3/6/7/8/9/
10/15/16/17 全綠；ai-rules 規模 heal 體感秒級（0.87s 實測基準）；穩態查詢
零子進程 spawn。

### 1b. Invariant Impact

- **受影響 domain invariant**：(1) 查詢答案與其宣稱世代一致——heal 成功後
  必須以新索引作答（不允許「heal 了但回答舊資料」）；(2) heal 不得讓 stale
  偽裝新鮮——false-stale 時 WARN-once＋如實標示；(3) 半成功不誤標（SM-17：
  index 已新鮮時不說「以現存（舊）索引作答」）。
- **critical path 觸及**：silent-corruption path（stale 消費）＋並發寫入面。
- **驗證對齊**：S3 驗證「healed-then-answered」「false-stale WARN-once＋
  第二次零重建」「single-flight」「半成功→Healed+note」四組測試。
- **面限定盲點**（F1-3 註記）：index 語言面與 repo 語言錯配時 doc-set 恆空
  差異→誤判 Healed——已知 v1 邊界（風險表中級行）。

### 2. 核心實作要點

- `build.rs` 新增（orchestration leaf，與 `build_repo` 同居）：
  - `pub enum HealOutcome { Fresh, Healed { secs: f64, nodes: usize, edges: usize, notes: Vec<String> }, HealedByPeer { waited_secs: f64 }, ServeStale (Vec<String>) }`
    （F1-2 設計後 WarnOnly 變體退役——producer-drift 是 flagged 路徑的
    `notes` 行，不是獨立結局；head-drift 歸 source_line 單一源）
  - `pub fn ensure_fresh(repo: &Path, roots: &[PathBuf]) -> Result<HealOutcome, String>`
    （F1-1：Err＝staleness 檢查本身失敗——hook 端 WARN＋續查詢）：
    1. slot 缺失 → `Ok(Fresh)`（caller 的既有缺索引 FAIL 路徑不變——SM-11）
    2. `engine::evaluate_staleness(repo, &slot)?`（零 spawn）；`!source_newer`
       → `Ok(Fresh)`（head-drift WARN 由 source_line 印）
    3. **flagged 路徑**：搶 `.heal.lock`（O_EXCL；held 且 age ≤10min→poll
       ≤120s（200ms 步進）→重評→`Fresh`/`HealedByPeer`；timeout→
       `ServeStale`；age >10min→steal）
    4. `build_repo(repo, None, roots)`：`Err(e)` → **半成功探針**（SM-17）：
       重評 `evaluate_staleness`——乾淨→`Ok(Healed{ notes: vec![「graph
       未重建（{e}）——下次顯式 build 補」] })`；仍旗標→`ServeStale([e])`
       （LockGuard Drop 釋鎖）
    5. **驗證（迴圈防護）**：重評 `evaluate_staleness`＋`load_index(&slot)`
       →`doc_set_delta`——仍旗標或 missing>0→`ServeStale`（false-stale
       WARN——SM-9）；乾淨→`Healed`
    6. **producer-drift 附註**（僅 flagged 路徑——F1-2）：meta `producer`
       在場且 ≠ `common::producer_version("pyrefly-index", roots)` →
       notes／ServeStale 訊息加一行升級提示
- `cli.rs` `scip_refs run()` 掛點（`cli.rs:319` 之後、mode 分支之前）：
  `repo` 在場＋`default_resolved`＋env ≠ off →
  `match crate::build::ensure_fresh(...)`：
  `Ok(Fresh)`無聲；`Ok(Healed{..})`→`[OK] index healed（{secs:.1}s，{nodes}
  nodes）——查詢以重建後索引作答`＋notes 行；`Ok(HealedByPeer)`→`[OK] index
  healed（同 slot 並發 heal，等待後重用）`；`Ok(ServeStale(ws))`→ws 逐行＋
  `[WARN] 本次查詢以現存索引作答`；`Err(e)`→`[WARN] 索引過期檢查失敗（{e}）
  ——本次查詢以現存索引作答`（SM-15）。寫入模式（stamp／build-cache）與
  `--index` 顯式路徑／`--repo` 缺席**不 heal**（SM-18）。
- env：`CODE_REALITY_AUTOHEAL=off` 跳過整個 hook（零 spawn——CI／唯讀場景）；
  **刻意不進 `--help`**（byte-pinned face，F4-4——SKILL.md／README 承載
  可見性）。

### 3. Pseudo Code

```
crates/code-reality/src/build.rs
  struct LockGuard(PathBuf);            // Drop → remove_file (panic-safe)
  fn acquire_heal_lock(slot_dir, max_age=10min) -> Option<LockGuard>
      // O_EXCL create fails: read existing lock mtime →
      //   age > max_age → steal ; age ≤ max_age → None(held)

  pub fn ensure_fresh(repo, roots) -> Result<HealOutcome, String> {
      let slot = default_index_path(repo); if !slot.exists() { return Ok(Fresh) }
      let snap = engine::evaluate_staleness(repo, &slot)?;
      if !snap.source_newer { return Ok(Fresh) }        // head-drift WARN: source_line owns it
      let Some(lock) = acquire_heal_lock(...) else {
          return Ok(wait_peer_and_reevaluate(repo, &slot, 120s));   // poll 200ms → Fresh | HealedByPeer
      };
      match build_repo(repo, None, roots) {
          Err(e) => {
              let snap2 = engine::evaluate_staleness(repo, &slot)?;
              if !snap2.source_newer { Ok(Healed{ notes: vec![format!("graph not rebuilt ({e}) — run build")], ..}) }
              else { Ok(ServeStale(vec![e])) }           // LockGuard Drop releases
          }
          Ok(rep) => {
              let snap2 = engine::evaluate_staleness(repo, &slot)?;
              let delta = load_index(&slot) → doc rel paths → doc_set_delta(..)?;
              if snap2.source_newer || delta.missing > 0 {
                  Ok(ServeStale(vec!["detection vs producer corpus mismatch (false-stale)"]))
              } else { Ok(Healed{ secs, nodes: rep.nodes, edges: rep.edges, notes: producer_drift_note() }) }
          }
      }
  }

crates/code-reality/src/cli.rs  scip_refs run()
  // after :319 (stamp/build-cache early-returns), before mode branches:
  if repo.is_some() && default_resolved && env CODE_REALITY_AUTOHEAL != "off" {
      match crate::build::ensure_fresh(repo_path, &producer_roots()) { ...push stderr lines... }
  }
```

Call Stack：MCP `code-reality-mcp`（`mcp_server.rs:267` catch_unwind）→
`cli::run` → `ensure_fresh` → lock → `build_repo` → spawn `pyrefly-index`
（S1 原子寫）→ in-process stamp（`build.rs:352`，遞迴終止於 stamp 分支
early-return——F3-7 實查）→ graph_db build → 驗證 → 回查詢主流程（
`open_face` 見新 index＋無 cache db→protobuf face 直接答，或下查詢自動建
cache——既有 lazy 鏈接手，零下游接線）。

### 4. 驗證策略

- `tests/build.rs` 增（fake-bin 注入模式，t13+）：
  - stale→Healed：fixture 建索引→touch 新 .py→`ensure_fresh`→slot 更新＋
    outcome `Healed`＋lock 釋放
  - fresh no-op：無漂移→`Fresh`＋**零 spawn**（fake-bin 呼叫計數器斷言——
    計 `--version` 以外的 producer 調用；F1-2 修正後穩態本就不探測版本）
  - head-drift only：git commit（docs 檔）→`Fresh`＋零 spawn（WARN 屬
    source_line，本段不印——由 s4_cli e2e 斷言 stderr 有該 WARN）
  - producer fail：fake_pyrefly exit 1→`ServeStale`＋lock 釋放
  - **半成功**（SM-17）：fake 正常寫 index＋stamp 但 graph 步驟注入失敗
    （fixture 直建 graph.db 缺失情境／注入 BuildError::Core）→重評乾淨→
    `Healed`＋graph note（不誤標 ServeStale）
  - **false-stale（F1-3 fixture 規格）**：磁碟檔名必須與 index documents
    的 rel_paths **對齊**——用 rust-leg 形（repo 放 `crates/a.rs`＋
    `crates/b.rs`，`fake_rust_analyzer` 變體只輸出 `crates/a.rs` 的
    partial index——新增 `fake_rust_analyzer_omit(dir, omit)` 或對齊
    rich.scip 檔名的 content-aware fake）→heal 後仍 missing>0→`ServeStale`
    ＋WARN-once訊息（**迴圈防護的釘死測試**——斷言出口真被走到）
  - **SM-9 第二次零重建**（F1-6）：false-stale fixture 連續兩次
    `ensure_fresh`→第二次 `Fresh`（heal 已更新 mtime）且 fake 計數不增
  - lock 逃逸：手寫舊 mtime lock→steal 成功 heal；live lock＋slow
    fake→第二呼叫 `HealedByPeer`
  - SM-12：chmod 555 slot dir→heal 寫入失敗→`ServeStale`（還原權限於
    teardown）
- `tests/s4_cli.rs` 增（e2e）：stale fixture 上 `scip_refs <sym> --repo`→
  stdout 正確答案＋stderr heal 行；**四模式 heal 回歸**（F4-6——query／
  audit／callers／closure 各一條 stale→healed 斷言）；`--stamp-meta`／
  `--build-cache` 模式零 heal（遞迴防護）；`CODE_REALITY_AUTOHEAL=off` 零
  spawn；`--index` 顯式路徑零 heal（SM-18）
- 整合（L4，**在飛三項完成後**——見整合策略）：ai-rules 真實 slot 復現
  incident（9/17 形態）→查詢自動癒合＋`mine_corrections` DEF 可查；CR 自倉
  mixed-repo heal（雙腿＋cat-merge 路徑）
- 已知未覆蓋：MCP client timeout（SM-13——外部 harness 行為，標注＋env
  解）；poll-timeout 分支（120s 等待不可測——接受）；跨機 NFS mtime 語義
  （單機工具，接受）。

---

## S4：commit 粒度背景刷新（opt-in post-commit hook＋`refresh`／`hook` 子命令）

### Context

**背景**：user 2026-08-30 裁決修正（V-1）——**save ≠ commit**：三論據瞄準
的 save-watcher/daemon 類維持永不做；commit 粒度是不同設計點（低頻、
開發者宣告的 coherent checkpoint、git 原生事件驅動、零常駐進程）。先前的
「post-commit hook 否決」記錄針對**靜默 auto-install 侵入消費端**；本段改
為**明示 opt-in 安裝**（user 執行、可逆）。與 S3 互補：hook＝commit 邊界
主動預熱（首次查詢零 heal 延遲，SM-13 timeout 風險大減）；S3 query-time
heal＝安全網（hook 缺席或失敗：`--no-verify`／未裝／新 clone／bin 不可
解析／工作樹未 commit 編輯）。「增量」語義校準：commit 觸發＝採納；更新
形態＝**全量冪等重產**（per-file 拼接與觸發時點無關地排除——論據二）。

**UC 引用**：完成「主索引 query-time 過期偵測＋lazy 自癒」的 commit 邊界
面。

**依賴關係**：S3（`ensure_fresh`＋`.heal.lock`）＋S2（
`evaluate_staleness`——refresh 自取 snapshot，R-F1-2）。

**語義約束**：與 S3 共享鎖與 ensure_fresh（**rebuild 分支**同鎖單飛；
head-sync re-stamp 分支不取鎖——meta last-writer-wins，與人工 build 同類
benign，R-F3-3）；hook 目錄 `.githooks/`（CR 自倉同形）；`refresh` 直接呼
lib 面（**不進** scip_refs 掛點——re-entry 唯一入口是 `build_repo` 的
in-process stamp（`build.rs:352`→`cli.rs:314-316` early-return 早於掛點
`:319`），head-sync 直呼 stamp core 亦不經 `cli::run`——無遞迴，R-F3-2）；
**模組歸屬（R-F1-1/R-F3-4）**：新 orchestration leaf `src/refresh.rs`
（own `argparse::ToolSpec`，沿 `build`／`project` 先例）——`cli.rs` 是
scip_refs 專屬 tool module（`cli.rs:194-197`），子命令路由在 umbrella bin。

**基礎設施盤點**：umbrella routing `bin/code-reality/main.rs:28-67`（
`Some(&"build") => code_reality::build::run(argv)` @ `:45` 先例）＋
`SUBCOMMANDS` 清單 `:69-87`（餵 `--help`／usage bytes——bin docstring
`:11-13` 明言非 parity face，rg 零斷言，reviewer 已驗——增項合法）；
CR 自倉 `.githooks/post-commit` binary-重裝 hook 先例（tracked、本地已設
`core.hooksPath`）；`ensure_fresh`／`resolve_bin`（絕對路徑嵌入用）。

**依賴錨點**：`ensure_fresh` → 定義 build.rs（S3）／S4 `refresh.rs` 消費；
routing → `main.rs:28-87`（route 兩臂＋`SUBCOMMANDS` 增 `refresh`/
`hook`）；`stamp_meta_mode` → 定義 `cli.rs:421`／S4 抽共用 core（見要點）。

**技術選型**：`refresh`＝lib `ensure_fresh` 的 argv 薄面＋head-sync；
installer 對既有 hook／既有 config **loud 拒絕不覆蓋**（R-F4-1）；hook
腳本嵌 install 時解析的**絕對路徑**（GUI no-PATH 消除，R-F4-2）。
**成功標準**：SM-19/20；安裝／移除可逆（含舊值還原指引）；docs-only
commit 不付重產只 re-stamp；rebuild 分支與查詢 heal 單飛。

### 1b. Invariant Impact

- **受影響 domain invariant**：同 S3 (1)(2)；＋ hook 不得在 commit 路徑
  同步阻塞（背景唯一形——腳本形狀斷言）；installer 不靜默改消費端 git
  設定（拒絕語義＋復原法 stderr 印出斷言——R-F1-4 兩個對齊點入驗證）。
- **驗證對齊**：S4 驗證「installer bytes／可逆」「docs-only→re-stamp-only
  （index bytes 不變——**不需重產因 source 未變**；重產 byte 等價僅
  python 面保證，rust 面無 determinism 依據，設計不依賴該等價——R-F1-6
  措辭）」「rebuild 分支單飛」三組。

### 2. 核心實作要點

- **`src/refresh.rs` 新 leaf**（R-F1-1）：`pub fn run(argv) -> ToolOutput`
  處理 `refresh` 與 `hook install|remove` 兩個子命令（ToolSpec：
  `--repo`／hook 的位置參數）；bin `main.rs` 增兩個 route 臂＋
  `SUBCOMMANDS` 兩項。
- **`refresh` 流**（R-F1-2）：自呼 `engine::evaluate_staleness` 取
  snapshot（`Fresh` 是 unit variant 給不出 head_drift——不動 S3 凍結面）
  → `build::ensure_fresh` → `Ok(Fresh) && snap.head_drift == Some(true)`
  → head-sync stamp；`Err(e)`→WARN 行、**恆 exit 0**（背景面設計，裁決
  5 第三路）；輸出全走 stderr（stdout 空——背景面）。
- **stamp core 抽取**（R-F1-5）：`cli.rs stamp_meta_mode` 的 payload 寫入
  核心抽成共用 fn（cli 模式照舊印 stdout 凍結面；refresh 面 render 到
  stderr；cli-private 可見性隨調）。
- **installer 語義**（R-F4-1）：(a) `.githooks/post-commit` 已存在且無
  code-reality marker → **loud 拒絕**（印內容摘要＋手動合併指示——CR
  自倉 dogfood 即此形：binary-重裝 hook 在場，走手動合併單腳本）；
  (b) `core.hooksPath` 已設非 `.githooks` 值（husky 等）→ **loud 拒絕**
  不覆寫；(c) install 印副作用警示（設定 hooksPath 會停用
  `.git/hooks/*`）＋復原法（含舊值）；remove 僅在現值仍是我們設的時
  unset，印還原指引。
- **hook 腳本**（R-F4-2）：install 時嵌 `resolve_bin("code-reality")`
  的**絕對路徑**：`#!/bin/sh`＋marker 行＋
  `nohup <abs-path> refresh --repo "$(git rev-parse --show-toplevel)" >/dev/null 2>&1 &`
  ——GUI git client 無 PATH 的已知陷阱（lsp-bridge wrapper 同源設計）由
  絕對路徑消除；殘餘失效面（bin 缺失 silent no-op）→SM-20 網。
- **env gate 範圍**（R-F3-5）：refresh/hook＝顯式指令，**不受**
  `CODE_REALITY_AUTOHEAL=off`（同 explicit build 姿態）；gate 只管查詢
  路徑的自動行為。
- **gitignore 姿態**（R-F4-3）：installer 不改消費端 gitignore（不替
  消費端寫設定）；hook 檔 tracked 與否＝消費端決定（文檔記兩形態：
  untracked→status 常駐提醒；committed→worktree 共享）；CR dogfood＝
  committed 同現況。

### 3. Pseudo Code

```
crates/code-reality/src/refresh.rs          // NEW leaf (ToolSpec; build/project precedent)
  refresh:
    snap = engine::evaluate_staleness(repo, &slot)?        // own S2 primitive (R-F1-2)
    match build::ensure_fresh(repo, roots)? {
        Ok(Fresh) if snap.head_drift == Some(true) => stamp_core(repo, &slot)  // stderr-rendered
        Ok(other)  => render outcome lines (stderr)
        Err(e)     => WARN line                             // exit 0 — background face (ruling 5)
    }
  hook install: refusals first (unmarked existing hook / foreign hooksPath) →
      write .githooks/post-commit (absolute bin path + marker) →
      git config core.hooksPath .githooks → print side-effect + revert guidance
  hook remove: unset only if value still ours → remove file → print restore guidance

crates/code-reality/src/bin/code-reality/main.rs            // routing (R-F1-1)
  Some("refresh") => code_reality::refresh::run(argv)
  Some("hook")    => code_reality::refresh::run(argv)
  SUBCOMMANDS += ["refresh", "hook"]          // help/usage bytes change — legal non-parity face

git chain: git commit → .githooks/post-commit (abs path) → nohup refresh →
  ensure_fresh (lock on rebuild branch) → build_repo | stamp-only → next query Fresh
```

### 4. 驗證策略

- 單元（`tests/refresh.rs` 或 `tests/build.rs` 增）：installer 寫檔 bytes
  （絕對路徑＋marker＋`nohup … &` 形狀——R-F1-4 兩斷言）／marker 冪等／
  remove 反向＋僅-own-unset；既有無 marker hook 拒絕／外值 hooksPath 拒絕；
  refresh 直測（fake roots 注入；head-sync 分支＝stamp core 被呼且 index
  bytes 不變）；rebuild 分支與 `ensure_fresh` 併發→單飛（HealedByPeer 形）。
- **hook-fired e2e 不入 cargo**（R-F1-3：`tests/build.rs:1-3` 明言不
  mutate PATH＋hook 嵌絕對路徑）→ L4 dogfood：CR 自倉手動合併腳本（
  binary-重裝＋refresh 併存）＋真 commit 觸發觀察。
- 已知未覆蓋：GUI git client 觸發（絕對路徑消除 PATH 面；殘餘＝bin 缺失
  silent no-op→SM-20）；worktree 相對 hooksPath 逐工作樹解析（行為正確：
  各刷各 slot，R-F5-2）；index 缺失＋hook→refresh no-op→SM-11 一致
  （R-F5-3）；`--no-verify`（SM-20）。
- MCP：refresh/hook **無** MCP 暴露（維護指令非查詢面——R-F4-5 補記）。

---

## 整合策略

- 段落間整合測試＝S3 的 e2e 組（依賴 S1 原子寫＋S2 原語同時在場）。
- **排序約束**：實作排序在在飛三項（producer 重建→stamp→ai-rules index
  刷新）之後——heal 的 L4 驗收需要 ai-rules 的新鮮基線；CR 側 S1-S3 可先
  行（cargo 測試自足）。
- baseline: `62dee2ae519198174aef0f8eca9be33d13bd7589`（下游
  /post-build／/code-review 任務弧審查範圍邊界）。

## 收尾步驟

1. **Capabilities＋Kanban**：root `AGENTS.md` Capabilities 新增一行（主索引
   query-time 過期偵測＋lazy 自癒＋commit 粒度背景刷新｜`code-reality
   scip_refs ... --repo <repo>`（自動觸發；`CODE_REALITY_AUTOHEAL=off`
   退出）＋`code-reality refresh`／`hook install`（opt-in）｜✅）＋Symbol
   truth query 行補充說明；`.kanban/` 卡片 In-Progress→Done（能力卡＋EP 追蹤卡）；從
   Scenario Matrix 提煉消費場景（SM-2/3/6/7/9 自包含描述）寫入卡片。
   **效能數字與 SM 編號不入 AGENTS.md**（元資訊禁止——F2-6）。
2. **SYSTEM-MAP**：不存在——略（UC 盤點已記）。
3. **instruction 檔**：
   - `crates/AGENTS.md` engine 段補 `walk_sources`／`evaluate_staleness`／
     `doc_set_delta`／`SKIP_DIRS` 跨 crate 單一源職責；common 段補
     `resolve_bin`/`first_output_line`/`producer_version` 遷入；build 段補
     `ensure_fresh` orchestration leaf＋**exit-semantics 三路徑明文**
     （explicit build Env→2/Core→1；heal-internal serve-stale WARN；
     refresh 背景面恆 exit 0——R-F2-2）＋rust leg sibling+rename；
     pyrefly-producer 段補 emit 原子寫＋SKIP_DIRS 改 import；**新增
     `refresh.rs` leaf 段**（S4）；cli 段補一句 scip_refs heal 掛點；
     subcommand 命名段補 carrier-native 例外（`refresh`／`hook`，同
     `build` 先例——R-F2-1）。
   - root `AGENTS.md` Usage 段補一行 refresh/hook（**heal 對使用者透明；
     refresh/hook 是明示 opt-in 動作**，非透明——Capabilities 行＋
     SKILL.md 承載，R-F2-1）。
4. **consumer 真相源**（F4-3）：`plugin/skills/code-reality/SKILL.md`——
   heal 行為（scip_refs 家族自動觸發＋觸發分級＋serve-stale 語義）、
   `CODE_REALITY_AUTOHEAL=off`、手動刷新鏈（:54/:79 段）註記「查詢時自動
   癒合＋opt-in commit hook 預熱，手動鏈保留給顯式維護」＋`hook install`
   opt-in 用法；`README.md` data-plane 段（:114-132）補一句 heal／hook
   語義。
5. **/audit-test**：對 S1-S3 新增測試跑品質稽核（覆蓋對稱性：Fresh／
   Healed／HealedByPeer／ServeStale／Err 五出口各有斷言；mock 健康度：
   fake-bin 是真執行檔非 in-process mock；false-stale 測試斷言出口真被
   走到——F1-3）。
