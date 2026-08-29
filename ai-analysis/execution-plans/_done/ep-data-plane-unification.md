# EP: data-plane unification — sidecar 遷入 `<repo>/.code-reality/`（~/.mosaic 退役）

> **ep_type**: implementation
> baseline: e041286

Source: user 設計討論 2026-08-29（第三次問「資料到底放哪」後的
arch-thinking 弧）。裁決反轉先前「sidecar 留集中」立場——查證後
in-repo 勝出，理由鏈：

1. **鍵與位置的一致性**：index/cache 是 per-checkout 派生數據（鍵＝
   工作樹＋rev），資料應與鍵同住。現行 `basename` 鍵＋home 位置＝
   鍵錯＋分離雙重不一致；freshness/mtime/sidecar-invalidation 機械
   全是在為分離買保險。
2. **path-hash central ≡ in-repo（語義等價）**：修好撞名後 central
   也是 per-checkout——「跨 checkout 共享」利益在正確鍵下不存在。
   等價下選 staleness 偵測最簡（同樹同 fs）、code 最少（零鍵計算）、
   rm checkout 即清者＝in-repo。
3. **樹污染是邊際成本**：graph.db 早已在樹內（2026-08-29 實測 NT
   178M；1.5G 為歷史峰值）、`target/`
   23G 在樹內是 Rust 常態；sidecar 全量 783M（NT 602M＋offline
   106M＋mosaic×2 各 32M＋自倉 11M＋ai-rules 212K）相對小。
4. CRG 同構佐證：`.code-review-graph/` 全 per-repo＋自帶 .gitignore。

**決策類型**：雙向門（資料可搬回、git 可回）。

## 現況盤點（2026-08-29 實查）

- **接觸面三常數**：`engine.rs:17 DEFAULT_INDEX_ROOT`（scip）、
  `boundary_build.rs:19`（boundary）、`snapshot.rs:19`（snapshots）
  ＋鍵邏輯 `engine.rs:322-342 default_index_path`（basename 鍵）；
  pyrefly-producer 側有對稱的 slot 落點（emit 面）。
- **slot 盤點**：NT 602M（index.scip 268M＋cache db 232M＋union 83M
  ＋fndefs 18M）、offline_backtesting 106M、mosaic_alpha 32M、
  trading_lab 32M、code-reality 11M、ai-rules 212K、snapshots 1.3M。
  home 根層另有 review 盤點出的殘留：`golden/`、`transition-*`
  六對（transition 預設寫 CWD 的歷史產物）、`scip/scip_pb2.py`、
  `scip/stderr.log`、`install.log`（hook 所屬）、`__pycache__`。
- **review 補列的接觸面**（EP 初稿漏）：`scripts/lsp_harvest.py:22,128`
  （Python 側自帶 `SIDECAR_HOME` 常數寫 home slot）、
  `.githooks/post-commit:11-13`（每 commit `mkdir -p` 重建 home
  目錄寫 install.log）、`cli.rs:286-296`（legacy 全域 slot 搬遷
  提示——常數翻轉後成錯誤路徑）。
- 開著的 bug：snapshot 對自建 graph 回 **0 files**——**review 讀碼
  後歸因降級**：比較鏈＝graph.db 內 build-time 絕對 file_path
  （`graph_db.rs:575` resolve 寫入）對 snapshot-time canonical root
  做 strip_prefix（`snapshot.rs:72-81`＋`common.rs:58-64`），
  sidecar home 不在鏈上、graph.db 早 in-repo——遷移本身不治此
  bug（SM-8 改假設檢驗）。

## EP Review Findings

> 2026-08-29 獨立 agent 五維度審查（F1-F5＋深層思考）＋主 session
> 對 critical/load-bearing findings 親證後全數回寫；無待確認項。

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| F5-1 | 🔴 必須修正 | S1/SM-7/驗收1 | `.gitignore` 內容 `*`＋`!.gitignore` 錯引 CRG——`!.gitignore` re-include 自身成 untracked，`git status --porcelain` 必現 `?? .code-reality/`；CRG `_write_data_dir_gitignore` docstring 實證 single `*`；NT 現存 inner gitignore（`*`＋註解、root 無條目）porcelain 乾淨，check-ignore 實證 inner `*` 連 .gitignore 自身都 ignore | 改單一 `*`（可帶註解檔頭），禁 `!.gitignore` | implemented |
| F4-1 | 🟡 建議採納 | S1 | `scripts/lsp_harvest.py:22,128` 自帶 Python 常數獨立寫 home slot——接觸面遺漏；退役後再跑會重建 home | 列入 S1 檔案清單同步翻轉 | implemented |
| F4-2 | 🟡 建議採納 | S2 | `.githooks/post-commit:11-13` 每 commit `mkdir -p ~/.mosaic/code-reality` 寫 install.log——直接擊敗驗收 3 | hook log_dir 改 `.agent-tmp/`（root .gitignore 既有）；驗收加「退役後 commit 一次不重建」 | implemented |
| F4-3 | 🟡 建議採納 | S1 | `cli.rs:286-296` legacy 全域 slot 提示用 `expand_home(DEFAULT_INDEX_ROOT)`——常數翻轉後對非 `~/` 字串 no-op 成錯誤路徑 | S1 一併退休；缺索引錯誤改提示搬遷 | implemented |
| F2-1 | 🟡 建議採納 | S4 | 漏 `plugin/skills/code-reality/SKILL.md:28-31`（Prerequisites 寫死 home 路徑，隨 plugin 出貨、變更需版本 bump 才傳播）；`examples/scip_edge_poc.rs:25` 同類 | S4 清單補＋plugin 版本 bump；順手改 example | implemented |
| F3-1 | 🟡 建議採納 | SM-8/S3 | 0-files 歸因不成立（見現況盤點降級段）——sidecar home 不在比較鏈上 | SM-8 改假設檢驗；實驗前先比對 db file_path 與 resolve(--repo) | implemented |
| F5-2 | 🟡 建議採納 | 整合策略/S2 | S1→S2 過渡窗口全查詢 miss＋PyPI 舊版 binary 續寫 home（混版本雙寫）；「兩邊都在」裁決基準未定義 | S1 翻轉與 S2 搬遷機制同一 release 出貨；雙存在時不覆寫、WARN 列兩邊 mtime 供人裁決 | implemented |
| F4-4 | ℹ️ 提醒 | S2 | home 退役盤點漏 `golden/`、根層 `transition-*` 六對、`scip_pb2.py`、`stderr.log`、`install.log` | 驗收 3 加 `test ! -e ~/.mosaic/code-reality` 斷言 | implemented |
| F4-5 | ℹ️ 提醒 | S2 | NT `index.union.db`（83M）無讀者死 artifact（v1+ S4 已物化進 graph.db，僅 tests 引用） | 搬遷排除、直接刪 | implemented |
| F1-1 | ℹ️ 提醒 | S1 | 兩個 DEFAULT_OUT_DIR 是 repo-join 重構非常數替換（三個消費站 flat `expand_home` 無 repo context；boundary sha-鍵共享→per-repo 語義變化） | 改 `fn default_out_dir(repo)`；附註語義變化 | implemented |
| F5-3 | ℹ️ 提醒 | S1 驗證 | `tests/s2_engine.rs:197-213` 斷言 in-repo 路徑時 macOS canonicalize `/var`→`/private/var` 陷阱 | 斷言先 canonicalize tmp.path()；測試名去 basename-keyed 字樣 | implemented |
| F3-2 | ℹ️ 提醒 | SM-6/S2 | 「雙源期讀 in-repo 優先」空集合語義（全 codebase 無 dual-source reader） | 改寫為「home 僅搬遷命令觸碰」 | implemented |
| F5-4 | ℹ️ 提醒 | S2 | 搬遷應 slot 目錄級 rename（原子；slot 四檔 mtime 關係不被中斷拆散）＋EXDEV fallback；「graph.db 1.5G」數字未驗（今日 NT 實測 178M） | 機制約束寫入 S2；樹污染論證數字附實測 | implemented |

## EP Validate Findings

> 2026-08-29 POC 實跑。原型改動（engine.rs resolver 重寫＋gitignore
> helper、producer 預設分支、graph_db 建點）僅存活於驗證期間、已
> 還原——行為由 /implement 以生產標準重寫。driver：
> `poc/poc_inrepo_thin_slice.py`＋`poc/poc_migration_stability.py`
> （build 提煉行為為正式測試，commit 時清除）。

| ID | 嚴重度 | EP 段落 | 問題(POC 結果) | 建議 | 狀態 |
|----|--------|---------|----------------|------|------|
| V1 | ✅ 通過 | S1 | thin slice 10/10：producer→stamp→build-cache→query→graph_db build 全鏈在 tempdir git repo 落 `<repo>/.code-reality/`、單一 `*` gitignore（註解檔頭）、`git status --porcelain` 空、零 home 寫入；macOS canonicalize `/var`→`/private/var` 實測在場 | 可進 build | verified |
| V2 | ✅ 通過 | S2 | 四檔 slot（index/db/fndefs/meta）mv roundtrip mtime_ns/size 全保；scip_refs 與 graph_db build stdout 重跑 byte-identical；輸出不嵌 slot/home 絕對路徑——byte-identical 驗收可達 | 可進 build | verified |
| V3 | ℹ️ 提醒 | S1 | `--stamp-meta`/`--build-cache` 是**無 symbol 模式**（與查詢互斥，帶 symbol 報錯）；stamp 對無 commit 的 git repo fail-loud | build/驗收腳本注意 CLI 模式差異 | implemented |
| V4 | ℹ️ 提醒 | S1 | 原型 gitignore writer 靜默失敗（`let _ =`）會讓 porcelain 髒了無聲 | implement 須 fail-loud（Result 傳播） | implemented |

## UC 盤點

### Backlog 關聯
- 自動建卡：本 EP 追蹤卡（Backlog）＋新 UC 卡合一（單卡）。

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過）。

### 掃描範圍
- root AGENTS.md Capabilities／crates/AGENTS.md／.kanban/。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Symbol truth query 家族 | ✅ | AGENTS.md | 更新 | slot 解析改 in-repo（消費者 CLI 面不變——仍 `--repo`） |
| Self-owned graph db | ✅ | AGENTS.md | 更新 | build 源頭改讀 in-repo slot |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| 統一 in-repo 資料面（sidecar＋自帶 .gitignore；~/.mosaic 退役） | 📋 | `crates/*/src` 三常數＋鍵邏輯＋搬遷工具 |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 既有 repo 首次跑新工具 | 任一查詢/producer | 新 slot 落 `<repo>/.code-reality/scip/`＋自帶 `.gitignore`；`git status` 乾淨 | 無 | 新增 UC |
| SM-2 | 舊 home slot 在場 | 搬遷命令 | 冪等搬遷（同盤 mv 瞬移）；重跑=零動作 | 無 | 新增 UC |
| SM-3 | 兩個同名 repo／同 repo 多 worktree | 各自 producer | 各自 `<repo>/.code-reality/` 天然隔離（basename 撞名類死亡） | 無 | 新增 UC |
| SM-4 | rm -rf checkout | 使用者 | 資料隨樹消失，home 無孤兒 | 無 | 新增 UC |
| SM-5 | index 比 cache 新（同樹內） | producer 重跑後 build | mtime 閘/lsp fast-path fail-loud **保留作動**（同樹對齊仍需） | S3 | graph db |
| SM-6 | 搬遷中斷 | Ctrl-C | 同盤 rename 路徑：冪等重跑收斂；**EXDEV fallback 中斷**＝in-repo 留半套→dual-presence WARN 停損由人工裁決（agent review R-1 註記）；雙源期 home 僅搬遷命令觸碰（無 reader），搬遷後 in-repo 為唯一來源 | 無 | 新增 UC |
| SM-7 | 消費端 repo 不想加 .gitignore | 任意 | **零設定**——`.code-reality/.gitignore`（單一 `*`，CRG 實證模式）工具自寫 | 無 | 新增 UC |
| SM-8 | snapshot 0-files bug | 對自建 graph 跑 snapshot | 假設檢驗：歸因疑在 build/snapshot root 不一致（非 sidecar 分離）；先比對 db 內 file_path 與 resolve(--repo)，沒治則獨立立案 | S3 | 觀察項 |
| SM-9 | S1 上線後、S2 搬遷前／PyPI 舊版 binary 在場 | 任一查詢 | in-repo slot 全面 miss→fail-loud 缺索引指引（偵測條件＝home slot 的 index.scip 在場，agent review U-3 註記；僅 cache 殘骸不觸發）；舊版續寫 home 不污染 in-repo | 無 | 新增 UC |

## 段落劃分原則

S1（常數＋鍵＋自帶 gitignore）→ S2（冪等搬遷＋退役驗證）→
S3（閘門盤點＋snapshot bug 驗證）→ S4（文檔/ai-rules 翻轉）。
S1 是行為變更核心；S2 依賴 S1；S3/S4 在 S2 驗證後。

---

## S1: slot 解析改 in-repo＋自帶 .gitignore

**Context**
UC 引用：實作「新增 UC：統一 in-repo 資料面」。改動面刻意最小：
三常數＋`default_index_path`＋producer 對稱落點。
- 依賴錨點：`engine.rs:17`＋`engine.rs:322-342 default_index_path`
  （定義端）→ 消費端 `scip_refs`/`graph_db build`/producer emit
  （rg `default_index_path|DEFAULT_INDEX_ROOT` 全列）；
  `boundary_build.rs:19`（消費 `:1269`）、`snapshot.rs:19`（消費
  `:445`）＋`boundary.rs:369` 同型。
- review 補列檔案：`scripts/lsp_harvest.py:22,128`（Python 側
  `SIDECAR_HOME` 同步翻轉，否則退役後再跑即重建 home）；
  `cli.rs:286-296`（legacy 全域 slot 提示退休——常數翻轉後
  `expand_home` 對非 `~/` 字串 no-op，計出錯誤路徑）。
- 語義約束：與 S2 共享「新位置＝`<repo>/.code-reality/{scip,boundary,
  snapshots}/`」；`meta.json` 的 repo 欄位語義不變。
- 顯式 `--index-root`/`--out` 覆蓋路徑（若在場）行為不變——只改
  default。

**要點**
- `default_index_path` 改為 `<resolved-repo>/.code-reality/scip/
  index.scip`（basename 鍵邏輯整段刪除）。
- 兩個 DEFAULT_OUT_DIR 是 **repo-join 重構非常數替換**（review
  F1-1）：三個消費站 `boundary_build.rs:1265-1269`、
  `boundary.rs:365-369`、`snapshot.rs:441-445` 都是 flat
  `expand_home` 無 repo context——改為
  `fn default_out_dir(repo) -> <repo>/.code-reality/{boundary,
  snapshots}/`，三站各自接 `--repo`。附註：boundary db 以 NT
  short-sha 為鍵，今日跨 repo 共享一顆、in-repo 後 per-repo 各自
  持有（reader 以 nt_commit 匹配＋glob `*.db`，語義相容，僅磁碟
  小量重複）。
- `.code-reality/` 建立時自寫 `.gitignore`（內容**單一 `*`**，可帶
  註解檔頭——CRG `_write_data_dir_gitignore` docstring 實證；
  **禁 `!.gitignore`**：re-include 自身會讓 porcelain 出現
  `?? .code-reality/` 擊敗 SM-1，review F5-1。寫在自己的資料目錄
  內，不觸消費端 repo config，plugin-stance 相容）。writer 失敗
  **須 fail-loud**（POC V4：靜默失敗＝porcelain 髒了無聲）。
- `cli.rs:286-296` legacy 全域 slot 提示隨鍵邏輯退休；缺索引
  錯誤在舊 home slot 仍存在時改提示搬遷路徑（與 S2 機制對齊，
  review F4-3/SM-9）。
- 空 basename 防護邏輯隨鍵邏輯退休。

**驗證策略**
- 既有測試全綠（fixtures 走顯式路徑/tempdir，預期零改或小改——
  釘 home 路徑者僅 `tests/s2_engine.rs:197-213` 一處，斷言改
  `tmp.path().canonicalize().unwrap().join(".code-reality/scip/
  index.scip")`（macOS `/var`→`/private/var` canonicalize 陷阱，
  review F5-3），測試名同步去 basename-keyed 字樣）。
- 新增回歸：producer→查詢→build 全鏈在 tempdir repo 上走一次，
  斷言 slot 落 in-repo＋`.gitignore` 存在＋`git status --porcelain`
  為空（臨 git repo fixture）。
- 效能期待：無（路徑解析次序不變）。
- POC 實測注意（V3）：`--stamp-meta`/`--build-cache` 是無 symbol
  模式（與查詢互斥）；stamp 對無 commit 的 git repo fail-loud。

## S2: 冪等搬遷＋`~/.mosaic/code-reality` 退役

**Context**
UC 引用：完成「新增 UC」的存量面。搬遷對象=S1 前的 home slots。
- 語義約束：與 S3 共享「雙源期（home 未刪）讀 in-repo 優先」；
  搬遷後 sidecar 檔 mtime 保持（mv 語義）——mtime 閘依賴它。

**要點**
- 搬遷機制定案（「build 時決」→已決）：獨立 `code-reality
  sidecar_migrate --repo <repo> [--home <root>]` 命令（cli 缺索引
  錯誤偵測舊 home slot 存在時提示它）；lazy 首跑搬遷否決——讀取
  路徑內 602M mv 是隱藏副作用。行為：home slot 存在且 in-repo 缺
  →**slot 目錄級 rename**（同 volume 原子；producer invalidation
  假設 slot 檔案組一致，逐檔 mv 中斷會拆散 mtime 關係——review
  F5-4）；偵測 EXDEV→`cp -p`（保 mtime）→驗證 size→刪源；兩邊
  都在→**不覆寫**＋WARN 列兩邊路徑供人裁決（數據完整性優先，
  review F5-2）；重跑=零動作（冪等）。
- **build 偏離記錄**：(a) boundary 面不進通用 migrate——home 的
  sha 鍵 db 無 repo 歸屬資訊（工具層不嵌 repo 特例）；NT 兩顆
  （61590e48/9133b899）以一次性 `mv` 落 NT in-repo boundary。
  (b) ai-rules slot 內容經查為 `.agent-tmp/repo-poc/` 舊 Python
  code-reality 的 lsp 收成（placeholder index＋437 occurrences）
  ——basename 撞名污染，搬入樹＝種外來數據；不搬，隨 home 退役
  清除，ai-rules 下次使用重新生成。
- NT slot 的 `index.union.db`（83M）為無讀者死 artifact（v1+ S4
  已物化進 graph.db，僅 tests 引用）——搬遷排除、直接刪
  （review F4-5）。
- 逐 repo 驗收後刪 home slot；全清後 `~/.mosaic/code-reality/`
  整目錄退役（含 `golden/`、根層 `transition-*`、`scip_pb2.py`、
  `stderr.log`、`install.log`、`__pycache__`，review F4-4）；驗收
  斷言 `test ! -e ~/.mosaic/code-reality`。
- `.githooks/post-commit:11-13` 的 log_dir 改 `.agent-tmp/`（root
  .gitignore 既有條目）——否則每次 commit 重建 home 目錄，直接
  擊敗退役（review F4-2）；驗收含「退役後跑一次 commit，目錄
  不重建」。

**驗證策略**
- 每個 repo：搬遷→`scip_refs` 一查＋`graph_db build` 重建→
  與搬遷前輸出 byte-identical（資料面零損失）。
- 重跑冪等斷言（第二次=零動作）；中斷重跑收斂。

**S2 結算（2026-08-29）**

五 repo 搬遷全過（ai-rules 依偏離記錄跳過）。每 repo：
`scip_refs` 查詢 stdout 搬遷前後 byte-identical（diff 空）＋
`graph_db build` 重建計數對齊搬遷前基準（NT 63224|232052、
mosaic_alpha 14517|27410、offline 14517|27410、trading_lab
14805|27832、自倉 899|2277）＋NT 四檔 sha256 逐位對帳＋boundary
兩顆 mtime/size 逐位保留（61590e48/9133b899 → NT in-repo）＋
offline golden 落 `.code-reality/golden/`。dogfood 抓到回報 bug
（dual-presence 跳過檔仍列 moved）當場修正＋補測試
（`dual_presence_file_reports_no_false_move`）。`~/.mosaic/
code-reality/` 已退役（`test ! -e` 斷言通過；查詢＋snapshot 後
不重建；`~/.mosaic/` 母目錄其他專案目錄未觸）。RED 階段殘骸
（`pyrefly-inrepo-<pid>` slot，舊 code＋`emit(repo, None)` 產物）
隨退役清除。

## S3: staleness 閘門盤點＋snapshot bug 驗證

**Context**
語義約束：**逐閘評估、非一刀刪**——同樹內 index vs cache 的
mtime 對齊（SM-5）、lsp fast-path mtime 閘、producer sidecar
失效（㊶ 弧）**全保留**；可退休的是「跨 root 漂移」類的防禦性
假設（若有）。

**要點**
- 盤點清單：freshness face（binary↔checkout——不動）、graph_db
  build mtime 閘（留）、producer invalidation（留）、snapshot
  "不同 root" WARN 路徑（重驗）。
- **snapshot 0-files bug 對照實驗（假設檢驗，review F3-1）**：
  實驗前先 `SELECT file_path FROM nodes LIMIT 5` 與
  `resolve(--repo)` 比對——若 root cause 是 build/snapshot root
  不一致（checkout 搬移或路徑別名），遷移不治、直接走獨立立案
  分支；治了才收案記錄（不擴本 EP scope）。

**驗證策略**
- 每個保留閘的既有測試仍綠；退休項列明理由表。

**S3 結算（2026-08-29）**

閘門盤點表：

| 閘門 | 錨點 | 處置 | 理由 |
|------|------|------|------|
| binary freshness WARN（checkout↔installed） | freshness.rs | 保留 | 與 sidecar 位置無關（binary↔checkout 軸） |
| graph_db build mtime 閘（index vs cache，同樹） | cache.rs:192-203 | 保留 | SM-5——搬遷（目錄級 rename 保 mtime）後同樹對齊仍需 |
| lsp fast-path mtime fail-loud | graph_db.rs:448-463 | 保留 | ㊶ 雙修之一，同樹語義 |
| producer sidecar invalidation | pyrefly-producer lib.rs:199-215 | 保留 | slot 檔案組一致性契約 |
| snapshot stale meta（sha/last_updated/mtime） | snapshot.rs:196-214 | 保留 | 獨立閘，僅記錄 stale 到 meta、不 gate export |
| cli legacy 全域 slot 提示（跨 root 時代遺物） | cli.rs 舊 :286-296 | **已退休（S1）** | 常數翻轉後 `expand_home` 成錯誤路徑（review F4-3）；改 sidecar_migrate 過渡橋 |
| basename 撞名假設 | engine.rs 舊鍵邏輯 | **已死亡（S1）** | in-repo 以 checkout 為鍵（SM-3） |

SM-8 對照實驗結論：**未治，獨立立案**。實證鏈：自倉 db 2277 邊
全為 `REFERENCES`；snapshot 投影過濾 `EDGE_KINDS=[IMPORTS_FROM,
CALLS, INHERITS]`（common.rs:17，凍結 CRG 結構種類）選出 0 列
→ 0 files；WARN 的「不同 root」推測是**錯誤歸因**——反證：
899/899 nodes 在 canonical root 下、2277/2277 邊端點可在 nodes
表解析、`repo_relative`（common.rs:58-64）邏輯正確。in-repo
遷移前後行為相同（遷移不觸及 kind 本體）。獨立調查（agent）
已結案：成因＝build 端 CALLS 判定只餵 `.py` 給 ruff parser
（Rust repo 全 REFERENCES 是 scip producer 常態）＋lsp-harvest
面凍結 REFERENCES-only＋2026-08-28 W3/W5 退休 import_legacy 後
結構邊斷源（sidecar 磁帶 47 files→0 files 斷崖）；爆炸半徑
3/6 repo（code-reality、NT 232K 邊、ai-rules），transition 空
pair 假陰性無防護。案件檔：
`ai-analysis/reports/snapshot-zero-files-case.md`（含修法分層
建議）。

## S4: 文檔／ai-rules 翻轉

**要點**
- repo README「Sidecar home」段改 in-repo 敘述＋消費端零 gitignore
  設定說明；AGENTS.md Usage 段同步；`crates/AGENTS.md`；
  `plugin/skills/code-reality/SKILL.md:28-31`（Prerequisites 寫死
  home 路徑——隨 plugin/marketplace 出貨，內容變更需版本 bump 才
  傳播到已裝快取，review F2-1）；`examples/scip_edge_poc.rs:25`
  硬編 home 路徑順手改。
- ai-rules handoff：code-reality SKILL.md 若提 sidecar 路徑同步
  翻轉（觸發條件＝本 EP build 完成）。

**S4 結算（2026-08-29）**

README（Sidecar home 段＋hook 段＋prerequisites 段＋`.gitignore`
建議段）、root AGENTS.md（Usage＋Capabilities 新行＋兩行附註＋module
guide）、crates/AGENTS.md（producer slot＋layering 補 sidecar_migrate）、
plugin/skills SKILL.md、example、pyrefly-producer README＋pyproject
description 全翻轉。ai-rules handoff prompt 隨 build 完成報告交付
（跨 repo，user 自行執行）。**plugin 版本 bump 義務（review F2-1）
移交 plugin 軸**（user 裁決 2026-08-29：CC session 實驗 plugin 一次
搞定作法，本樹不動）——SKILL.md 內容變更的傳播隨該軸出貨；F5-2
同版本出貨約束在下次 release 切 tag 時兌現（勿在部分 commit 狀態
切 tag）。

## NOT（scope boundary）

- **不動** graph.db 位置（已在正確位置）。
- **不動** launchd/HTTP 面。
- **不做** path-hash 鍵（議題隨 in-repo 死亡）。
- **不刪** staleness 閘（只盤點退休候選，同樹對齊類全留）。
- Windows 語義不對齊的 `#[cfg(not(unix))]` stub 不處理（EP NOT 沿襲）。

## 整合策略

- baseline: `e041286`。
- S1+S2 可同 session（S1 code＋S2 逐 repo 搬遷驗收）；S3+S4 收尾。
- **同版本出貨約束**（review F5-2/SM-9）：S1 default 翻轉與 S2
  搬遷機制必須同一 release 進 PyPI——否則已裝舊版 binary 續寫
  home、新版讀 in-repo，混版本雙寫。S1 上線→S2 搬遷間的開發機
  窗口內查詢全面 miss 是預期（fail-loud 指引兜底）。
- 全 repo 驗收清單：nautilus_trader、mosaic_alpha、
  mosaic_alpha_offline_backtesting、mosaic_alpha_trading_lab、
  ai-rules、code-reality（自倉 dogfood）。

## 收尾步驟

1. Capabilities：新增「統一 in-repo 資料面」行；既有兩行附註。
   Kanban 搬 Done＋EP 歸檔。
2. 無 SYSTEM-MAP.md——跳過。
3. instruction 檔：AGENTS.md（root＋crates）Usage/Capabilities 同步。
4. /audit-test：S1 有 callable 變更（default_index_path 語義改）——
   正常跑；新增回歸測試在稽核範圍。
5. ai-rules handoff prompt 交付（sidecar 路徑翻轉，觸發＝build 完成）。
