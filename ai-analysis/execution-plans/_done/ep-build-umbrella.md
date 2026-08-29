# EP: build 傘形命令——新 repo 數據面一鍵準備（Python 腿 MVP）

> **ep_type**: implementation
> baseline: `a7b240a`
> **前置條件**：working tree 有 npm 退役弧的未 commit 變更（retirement）＋本弧的 EP/kanban
> untracked——`/implement` 開始前 retirement commit 必須先落地（兩弧不混樹）。

## 實作總覽

`code-reality build --repo <path>` 一鍵完成新 repo 的數據面準備：偵測 repo 語言形態 →
跑 producer index（**兩腿**：Python＝subprocess `pyrefly-index`；Rust＝subprocess
`rust-analyzer scip`——CLI 形狀已 POC 實證，見段落 0）→ in-process `graph_db build` →
`graph_db ensure_indexes` → 印 state-transition 摘要。**零新生產邏輯**——本 EP 是既有
組件的編排層（傘形），與記憶裁決「setup 薄面最小形」對齊。`--producer rust|python`
可覆寫偵測（混合 repo 預設走副檔名多數腿，摘要明示未索引語言；雙語言合一 graph＝
deferred 設計題，見文末）。

分發語境：binary 面已由 plugin wrapper 的 first-session uv bootstrap 解決（npm 退役弧）；
`build` 補上數據面最後一哩——新機器/新 repo 的完整 onboarding 收斂為
「plugin 裝好 → wrapper 裝五 bin → `code-reality build` 一次」。

## UC 盤點

### 掃描範圍
- `AGENTS.md` Capabilities（rows 78/79/84）＋ `.kanban/{Backlog,In-Progress,Done}`（Backlog 空）

### Backlog 關聯
- 卡已隨 EP 建立：`.kanban/Backlog/build-umbrella.md`（EP 整體追蹤＋新 UC；S3 收尾搬 Done，不重複建卡）

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（repo 無此檔——正當跳過）

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Self-owned graph db build | ✅ | AGENTS.md:78 | 無影響 | build 是其新消費者（in-process 呼叫） |
| Read-chain index maintenance | ✅ | AGENTS.md:79 | 無影響 | 同上 |
| Python occurrence producer（pyrefly-index） | ✅ | AGENTS.md:84 | 無影響 | build 以 subprocess 消費其 bin |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| 新 repo 數據面一鍵準備（build 傘形） | 📋 | `crates/code-reality/src/build.rs` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 新 Python repo 首次 build | `build --repo <py-repo>` | 偵測 python → pyrefly-index 落 slot → graph.db 建 → indexes → `[OK]` 摘要 | 無 | 新 UC |
| SM-2 | Rust-only repo | `build --repo <rs-repo>` | 偵測 rust → scip 腿全鏈（SM-12 同型） | 無 | 新 UC |
| SM-3 | 混合 repo（py+rs） | `build --repo <mixed>` | 主語言腿執行；摘要明示未指數的另一語言 | 無 | 新 UC |
| SM-4 | 無原始碼 repo | 空 repo / 無 .py 無 .rs | loud fail（不是靜默成功） | 無 | — |
| SM-5 | pyrefly-index bin 缺場 | PATH 無 bin 且 fallback 目錄無 | fail＋install hint（`uv tool install pyrefly-producer`）＋已試過的解析路徑 | 裝好後重跑 | 新 UC |
| SM-6 | 冪等重跑 | 對已 build 過的 repo 再跑 | 全鏈重跑成功；摘要誠實標明「全量重產」（pyrefly-index 每次刪 sidecar 重建） | 無 | 新 UC |
| SM-7 | producer 中途失敗 | pyrefly-index exit≠0（實際路徑：repo 無 .py／engine 錯；no-AST 只是 WARN＋exit 0） | fail(2)＋嵌 child stderr（`Command::output()` 取得）；**不**繼續 graph build | 修源碼重跑 | — |
| SM-8 | GUI no-PATH 環境 | PATH 缺 `~/.local/bin` | fallback 解析鏈找到 `~/.local/bin/pyrefly-index` | 無 | 新 UC |
| SM-9 | 既有 graph.db 重建 | slot 已有舊 graph.db | 原子替換（temp sibling＋rename，既有行為）；摘要報 rebuilt | 無 | — |
| SM-10 | --repo 不存在／非目錄 | 路徑不存在或非目錄 | loud fail「不是目錄——不建立目錄」（sidecar_migrate.rs:84-88 慣例） | 無 | — |
| SM-11 | slot 已有 rust SCIP 跑 python 腿 | detect=python＋slot 有舊 rust 索引 | pyrefly emit 覆蓋 index.scip＋刪 stale sidecars（producer lib.rs:203-222）；摘要 notes 明示「既有 rust 索引已被覆蓋」 | 無 | — |
| SM-12 | Rust repo 一鍵（NT 形） | `build --repo <rs-repo>`（detect rust） | `rust-analyzer scip <repo-dir> --output slot`（dir 形式＋current_dir）→ graph build → indexes → `[OK]` | 無 | 新 UC |
| SM-13 | producer 靜默空輸出 | scip 產物 <1KB（誤用 Cargo.toml 形式／workspace 載入失敗） | loud fail「空索引」；**不**放行 graph build | 無 | — |
| SM-14 | rust-analyzer 缺場 | PATH＋fallback 皆無 bin | fail(2)＋hint（`rustup component add rust-analyzer`） | 裝好重跑 | 新 UC |
| SM-15 | --producer 覆寫 | 混合 repo＋`--producer rust` | 跑 rust 腿（覆蓋偵測）；摘要注明 python 未索引 | 無 | 新 UC |
| SM-16 | 混合 repo 合一 | detect=Mixed（無 --producer） | 兩腿各產 temp scip → 串接寫 slot → 單一 graph.db 雙語言可查（POC 已證：cat-merge 合法＋graph_db 零改動＋三查詢面綠） | 無 | 新 UC |

## 段落 0：全域研究摘要（Explore agent 2026-08-29，錨點已驗）

**可複用基礎設施**：
- 子命令接線：`main.rs:28-45` route() exact-match on argv.first()；`SUBCOMMANDS: [&str; 15]`
  main.rs:67-83 同時餵 --help 與 usage。新增 `build`＝route arm＋SUBCOMMANDS＋長度改 16。
- 模組形態先例＝`sidecar_migrate.rs`：`const SPEC`（:48-64）＋`HELP`（:66-69）＋純核心
  `migrate_repo() -> Result<Report, String>`（:82，可注入測試）＋薄 `run(argv)`（:261-317，
  冪等「無動作」訊息 :291-297）。Report 形（moved/dropped/warnings）是摘要最佳先例。
- lib 註冊：`lib.rs:54-81` pub mod 清單；`ToolOutput{stdout,stderr,exit_code}`（lib.rs:15-44，
  fail=2/crash=1；lib 永不 print、永不 process::exit）。
- graph_db in-process 呼叫：`build_from_cache(repo)`（graph_db.rs:702-705，CLI 面）與核心
  `build_from_cache_at(repo, index_path)`（:444）；`ensure_indexes(repo)`（:728-746，冪等、
  db 缺席→Err「先 build」:732-737、report 有 created/skipped :716-720）。
- spawn 先例（同 crate shell-out）：`graph_audit.rs:251` `Command::new("rust-analyzer")`（60s
  timeout、spawn 失敗=crash 家族）；`hazard.rs:674` rg runner；`session.rs:237` 兄弟 bin
  `pyrefly-lsp` lazy spawn＋`install_hint`（session.rs:75-99，Python hint 即
  「uv tool install pyrefly-producer」）。
- slot：`engine::default_index_path`＝`<repo>/.code-reality/scip/index.scip`（engine.rs:332-336）；
  首寫自動補單 `*` gitignore（pyrefly-producer lib.rs:74-78）。

**關鍵約束**：
- **跨 crate 只能 spawn**：`pyrefly-producer → code-reality` 單向 path dep（反向=workspace
  循環＋破壞三 dist 分發）；code-reality bin 呼 pyrefly-index 必須 process spawn＋PATH 解析。
  crate 內無現成 which()/PATH+fallback helper——S2 新增。
- **slot 單檔**：兩種 producer 後跑覆蓋先跑（風險 3）→ MVP「單一主語言」策略＋摘要明示。
- pyrefly-index 對 Python 腿可省 stamp-meta/build-cache（cache::open_face 無 db 走 protobuf 面、
  stale 自動重建——README.md:114-122、cache.rs:278-289）。

**Rust SCIP 現況＋POC 實證（2026-08-29，user 擴 scope 後現場驗）**：
- **CLI 形狀已驗**：`rust-analyzer scip <repo-dir> --output <path>`（目錄形式）。
  workspace 根目錄形式＝`scip .` → code-reality 全 workspace **5.77MB／9.2s**；單 crate
  目錄 `scip crates/code-reality` → 4.98MB。exit 0＋stderr 有非致命 ERROR 雜訊
  （`enclosing definition with no name`——成功跑也有，忽略）。
- **致命陷阱（POC 抓到）**：**Cargo.toml 檔案形式＝靜默空輸出**——`scip Cargo.toml
  --output x` exit 0、`Generating SCIP finished`，但產物只有 102-122 bytes（純 metadata
  空索引）。silent-failure 形狀 → Rust 腿必須（a）用目錄形式、（b）**empty-index guard**
  （產物 < 1KB → loud fail「producer 產出空索引——workspace 載入可能失敗」）。
- **toolchain proxy**：本機 default==repo pin 無法示差，但設計守衛不變——spawn 一律
  `.current_dir(repo)`（proxy 依 cwd 解 toolchain，SKILL.md:35-43 陷阱）。
- **NT 地面真相**：`~/Github/nautilus_trader/.code-reality/scip/`＝280MB index.scip
  （2026-08-28）＋cache db＋fndefs＋meta.json（`tool: code_reality.scip_refs`——手動
  鏈 producer→stamp-meta→build-cache 的產物）。NT＝Rust-dominant 混合（~2614 .rs）＝
  Rust 腿 dogfood 標的；build 腿重跑等價重現此鏈（stamp/cache 為可選加速，MVP 不含）。

**語言偵測**：無現成 repo 級邏輯；可組合根 `Cargo.toml`/`pyproject.toml` 存在性＋副檔名計數
（新寫短走訪；SKIP_DIRS 參考 pyrefly-producer lib.rs:355-371）。

**風險假設**（等級）：pyrefly-index 缺場行為（高→SM-5）、版本歪斜三 dist 各自 uv 安裝
（中→摘要記錄 `--version` 輸出，不強制）、混合 repo（中→SM-3）、exit code 映射 child 1→
fail(2)（低）、長跑 timeout 政策（低→不設硬 timeout，大 repo 全量即目的）。
無致命等級假設（全部組件已存在且實證過）。

## EP Review Findings

獨立五維度審查 2026-08-29（Explore agent；judge 全採納；錨點驗證近全命中）。

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| 1 | 🔴 必須修正 | S2 | `piped().status()` pipe-deadlock：不讀 pipe，stderr>64KB 阻塞＋無 timeout＝掛死；且取不到 child stderr 違 SM-7 | 改 `Command::output()`（併發排空＋直接取 stderr）；UX 註明 WARN 建完後一次呈現 | implemented |
| 2 | 🟡 建議 | S2 | `ensure_indexes` 回 `EnsureIndexesReport{db,created,skipped}` 非 Vec | BuildReport 改 created/skipped 欄位 | implemented |
| 3 | 🟡 建議 | S1 | run 簽名 `&[String]` 與 route 傳 `&[&str]` 矛盾 | 統一 `&[&str]` | implemented |
| 4 | 🟡 建議 | S2 | exit 映射未明示（兄弟先例 graph 段→crash(1)） | 三分法：usage→fail2；環境類→fail2（有意）；graph 核心→crash1 | implemented |
| 5 | 🟡 建議 | S2 驗證 | cargo test 並行 PATH set_var 污染 | resolver 注入化（roots 參數），測試不動 PATH | implemented |
| 6 | 🟡 建議 | S3 | 文檔漏 plugin/skills/code-reality/SKILL.md python 手動段 | S3 補 Python 段一行（rust 段不動） | implemented |
| 7 | 🟡 建議 | SM | 缺 SM-10（非目錄）/SM-11（rust-slot 覆蓋）；SM-7 觸發例不準 | 補兩列＋修觸發例 | implemented |
| 8 | ℹ️ | 多處 | 檔數漂移、kanban 卡已建、graph_rebuilt 填充法、default_index_path Result、空-AST 庫語義未驗證、detect profile-blind、fixture 極小化、MCP 面 non-goal、hint 帶 cargo alt、S1 測試僅 usage/detect 綠 | 全數併入對應段落與注意事項 | implemented |

## 段落劃分原則

垂直切片：S1 接線+偵測（無副作用、可獨立驗證）→ S2 Python 腿編排 → S3 Rust 腿編排
（共享 S2 骨架）→ S4 dogfood+文檔收尾。依賴：S2←S1（Report/detection）；S3←S1
（共用 resolve/exit 骨架）；S4←S2+S3。語義約束：S1-S3 共享 `RepoKind` enum、
`BuildReport` struct 與 exit 三分法（S1 定義、S2/S3 填充）。

---

### S1: 子命令接線＋語言偵測＋報告骨架

**Context**
- UC 引用：實作「新 repo 數據面一鍵準備」的骨架（偵測與接線）。
- 依賴錨點：`main.rs:28-45`（route arm）、`main.rs:67-83`（SUBCOMMANDS）、`lib.rs:54-81`
  （pub mod build）、`sidecar_migrate.rs:48-82`（SPEC/HELP/Report/run 形態）。
- 語義約束：與 S2 共享 `RepoKind{Python,Rust,Mixed(py_dominant|rs_dominant),Empty}` 與
  `BuildReport`（欄位見 pseudo code）。
- 基礎設施盤點：`argparse.rs` `ToolSpec/parse()`（:38-43,:112）＋`required()`（:252-261）。
- 技術選型：偵測＝根 `Cargo.toml`/`pyproject.toml` 存在性＋walk 副檔名計數（跳
  SKIP_DIRS：dot-dirs/`__pycache__`/`venv`/`node_modules`/`target`）；成功標準＝SM-1~4 的
  偵測面全對＋`--help` 列出 build＋usage 錯誤含 build。

**核心實作要點**
- `crates/code-reality/src/build.rs`：`detect_kind(repo) -> RepoKind`（純函式）、
  `SPEC`（--repo required）、`HELP`、`BuildReport`、`run(argv)`（S1 版本：只偵測＋印
  偵測結果與「S2 未接」訊息？——否：S1 連同 S2 一次接線，S1 的 run 直接進 S2 邏輯，
  EP 分段是實作順序非交付順序）。
- main.rs route 加 `Some("build") => build::run(argv)` arm＋SUBCOMMANDS 加 "build"。

**Pseudo Code**
```rust
// crates/code-reality/src/build.rs
pub enum RepoKind { Python, Rust, Mixed { py: usize, rs: usize }, Empty }
pub struct BuildReport {
    repo: PathBuf, kind: RepoKind,
    producer_version: Option<String>,      // S2: pyrefly-index --version 輸出
    index_path: Option<PathBuf>,           // S2: slot 落點
    graph_built: bool, graph_rebuilt: bool,// S2: 首建/原子替換
    indexes: Vec<String>,                  // S2: created/skipped 明細
    notes: Vec<String>,                    // 混合 repo 未指數語言、全量重產聲明等
}
fn detect_kind(repo: &Path) -> Result<RepoKind, String> {
    // walk（手刻短走訪，SKIP_DIRS 同 pyrefly-producer 慣例）計 .py/.rs；
    // 根 Cargo.toml/pyproject.toml 存在性作為加權訊號（非決定性——計數決定）
    // 0+0 → Empty；只有一種 → 單語言；兩種 → Mixed{..}（多數決在 run 端）
}
pub fn run(argv: &[&str]) -> ToolOutput { /* SPEC parse → S2 核心 → 渲染（main.rs route 傳 &[&str]） */ }
```

**驗證策略**
- 單元測試（`crates/code-reality/tests/build.rs`）：detect_kind 各案例——純 py fixture、
  純 rs fixture、混合、空、dot-dir 排除；`run(["build","--repo",fixture])` 的 usage 錯誤
  （缺 --repo）；--help 含 build。
- 已知未覆蓋：S1 不驗 spawn/graph（S2 範圍）。

---

### S2: Python 腿編排鏈（spawn＋in-process graph）

**Context**
- UC 引用：完成「新 repo 數據面一鍵準備」的 Python 腿。
- 依賴錨點：spawn 先例 `graph_audit.rs:243-292`；install_hint 先例 `session.rs:75-99,:237-252`；
  in-process `graph_db.rs:444`（build_from_cache_at）、`:728-746`（ensure_indexes）、
  slot `engine.rs:332-336`；pyrefly-index CLI `pyrefly-index.rs:21-46`（--repo/--out/--version，
  成功印 `[OK] ... in {secs}s`，失敗 exit 1）。
- 語義約束：沿用 S1 的 `BuildReport`；exit 映射——child exit≠0 → `fail(2)` 嵌 child
  stderr；spawn 目標失敗（bin 不存在）→ `fail(2)`＋install hint。
- 基礎設施盤點：bin 解析鏈語義抄 `plugin/.mcp.json` wrapper（PATH → `~/.local/bin` →
  `~/.cargo/bin`）；Report 渲染抄 sidecar_migrate（冪等/明細/notes）。

**核心實作要點**
- `resolve_producer_bin(search_roots) -> Result<(PathBuf, Vec<String>), String>`：**注入化**
  （search roots 參數化——cargo test 並行下不碰 process 全域 PATH，F4-1）；生產端 roots＝
  PATH 各段＋`~/.local/bin`＋`~/.cargo/bin`。失敗訊息含已試路徑＋雙 hint
  （`uv tool install pyrefly-producer` or `cargo install --path crates/pyrefly-producer`，
  對齊 session.rs:88 先例）。
- **spawn 選型（EP review 🔴 修正）**：用 `Command::output()`——內建雙 pipe 併發排空
  （禁 `piped().status()`：不讀 pipe，child stderr 超過 pipe buffer 即永久阻塞，與
  「不設 timeout」組合＝掛死；graph_audit.rs:258-264 的 drain-thread 註解即此陷阱），
  且直接取得 child stderr 供 SM-7 fail 訊息嵌入。UX 註明：capture 模式下 producer 的
  `[WARN]`（如 skipped_no_ast）在鏈完成後一次呈現。
- **exit code 三分法（F2-1）**：argparse usage → `fail(2)`；環境類（bin 缺場、child
  exit≠0）→ `fail(2)`（有意選擇：可修復環境問題）；detect/graph 核心錯 → `crash(1)`
  （對齊 graph_db.rs:897 與 sidecar_migrate.rs:315 慣例）。
- `run` 主鏈：detect → Rust/Empty/非目錄分流（SM-2/4/10 loud fail）→ resolve bin →
  `--version` 記錄（觀測不強制）→ spawn（output()）→ 成功後 in-process
  `build_from_cache_at(repo, default_index_path(repo)?)`（先記 `db_path.exists()` 填
  `graph_rebuilt`）→ `ensure_indexes(repo)` → 渲染 Report。
- `BuildReport.indexes` 欄位改 `created: usize, skipped: usize`（接
  `EnsureIndexesReport{db,created,skipped}` graph_db.rs:716-720，F1-2）。

**Pseudo Code**
```rust
fn build_python_leg(repo:&Path, rep:&mut BuildReport) -> Result<(),String> {
    let (bin, tried) = resolve_producer_bin(roots())?;              // SM-5/8
    if let Ok(v) = run_capture(&bin, &["--version"]) { rep.producer_version = Some(v); }
    let out = Command::new(&bin).arg("--repo").arg(repo).output()   // output(): 併發排空
        .map_err(|e| format!("spawn {bin:?}: {e} — {INSTALL_HINT}"))?;
    if !out.status.success() {                                      // SM-7
        return Err(format!("pyrefly-index failed ({}):\n{}",
            out.status, String::from_utf8_lossy(&out.stderr)));
    }
    rep.graph_rebuilt = graph_db::db_path(repo).exists();           // 原子替換前快照
    let idx = engine::default_index_path(repo)?;
    graph_db::build_from_cache_at(repo, &idx)?;                     // in-process, SM-9
    let ir = graph_db::ensure_indexes(repo)?;                       // EnsureIndexesReport
    rep.indexes_created = ir.created; rep.indexes_skipped = ir.skipped;
    Ok(())
}
```

**驗證策略**
- 整合測試（沿用 `tests/graph_db.rs:12-37` fixture-scip 手法）：fixture slot → 直接呼叫
  graph 段（不 spawn 真 producer）→ Report 欄位斷言。
- fake-bin 測試：tmp dir 放假 `pyrefly-index`（shell 腳本：`--version` 印一行、正常路徑
  copy 預置 .scip 到 slot 或 exit 0）→ PATH/env 注入跑 `run()` → 斷言解析、版本記錄、
  child 失敗映射（假 bin exit 1 → fail(2)＋stderr 嵌入）。
- 冪等：同一 fixture 跑兩次 → 第二次成功＋notes 含全量重產聲明。
- 已知未覆蓋：真 pyrefly-index 全量跑（開發機 L4 於 S3 dogfood 涵蓋）。

---

### S3: Rust 腿＋混合合一編排（scip spawn＋空索引守衛＋cat-merge）

**Context**
- UC 引用：完成「新 repo 數據面一鍵準備」的 Rust 腿＋混合 repo 雙語言合一。
- 依賴錨點：POC 實證介面 `rust-analyzer scip <repo-dir> --output <path>`（段落 0：
  workspace 根目錄形式 5.77MB/9.2s；**Cargo.toml 檔案形式＝靜默空輸出 102-122 bytes**）；
  **合一 POC 實錄（/tmp/mixedpoc）**：`cat rs.scip py.scip`＝合法 Index →
  `graph_db build` 零改動 → `scip_refs rust_greet`（src/lib.rs:1）＋
  `scip_refs py_greet`（app.py:1）＋`graph_query search` 三面同 db 全綠；
  spawn 形態同 S2（`Command::output()`＋`.current_dir(repo)`）。
- 語義約束：與 S2 共享 `BuildReport`／resolve／exit 三分法骨架；混合路徑＝
  **兩腿產物 cat 串接後寫 slot**（protobuf 同型訊息串接＝合法合併，repeated 疊加；
  跨語言 symbol scheme 不同（rust-analyzer vs pyrefly 命名空間）不易撞——同名碰撞
  為已知未測邊界，撞上時查詢面自然呈現兩個符號）。

**核心實作要點**
- `rust_leg(repo, out_path)`：resolve `rust-analyzer`（PATH→`~/.cargo/bin`；缺場
  fail(2)＋`rustup component add rust-analyzer` hint）→ spawn `scip <repo>
  --output <out>`、`.current_dir(repo)`、`output()` → **空索引守衛**（<128 bytes
  → fail「producer 產出空索引——傳 repo 目錄而非 Cargo.toml；workspace 載入可能
  失敗」；閾值 128 非 1024——POC 迷你 crate 合法索引 725B，1KB 會誤殺）。
- **混合 repo（detect=Mixed 或 `--producer both`）**：兩腿各產出 temp scip →
  `std::fs::write(slot, rs ++ py)`（Rust 端讀兩檔串接，非 shell cat）→ 既有
  `build_from_cache_at`＋`ensure_indexes`。單腿路徑（純 rust／純 python／
  `--producer rust|python` 覆寫）行為不變。
- rust 腿無 sidecar 刪除邏輯（pyrefly producer 的職責）；混合路徑摘要 notes 明示
  「雙語言合一 graph（rust N docs＋python M docs）」。

**Pseudo Code**
```rust
fn rust_leg(repo:&Path, rep:&mut BuildReport) -> Result<(),String> {
    let bin = resolve_ra_bin()?;                                   // SM-14
    let slot = engine::default_index_path(repo)?;
    let out = Command::new(&bin).arg("scip").arg(repo).arg("--output").arg(&slot)
        .current_dir(repo).output()                                // proxy: cwd=repo
        .map_err(|e| format!("spawn {bin:?}: {e} — rustup component add rust-analyzer"))?;
    if !out.status.success() {
        return Err(format!("rust-analyzer scip failed ({}):\n{}",
            out.status, String::from_utf8_lossy(&out.stderr)));    // 同 SM-7 映射
    }
    if fs::metadata(&slot).map(|m| m.len()).unwrap_or(0) < 1024 {  // SM-13 空索引守衛
        return Err("producer wrote an empty index — pass the repo DIR (not Cargo.toml); workspace load may have failed".into());
    }
    graph_db::build_from_cache_at(repo, &slot)?;                   // protobuf face
    /* ensure_indexes 同 S2 */
    Ok(())
}
```

**驗證策略**
- fake-bin 測試（同 S2 手法）：假 rust-analyzer 三態（--version／寫 >1KB 假 scip／
  寫 <1KB 檔）→ 守衛兩側＋exit≠0 映射斷言。
- 自倉快速 L4：code-reality 自身＝Rust workspace——真跑 scip 腿（≈9s/5.77MB 級）＋
  graph build 綠（S4 NT dogfood 前的自證）。
- 已知未覆蓋：NT 規模的耗時（280MB 級）留 S4 dogfood 實測。

---

### S4: dogfood＋文檔＋收尾

**Context**
- UC 引用：結算「新 repo 數據面一鍵準備」。
- 依賴錨點：README.md:114-122（現行手動兩步文檔——改寫為 build 一鍵＋手動作為進階）；
  AGENTS.md Capabilities 表（新 row）；`.kanban/Backlog/build-umbrella.md`（搬 Done）。
- 技術選型：dogfood 標的＝ai-rules repo（python 腿：純 Python、小）＋**nautilus_trader**
  （rust 腿：Rust-dominant 混合、~2614 .rs、既有 280MB 手動鏈產物可對照）；成功標準＝
  兩腿 `[OK] build` 全鏈＋`.code-reality/{scip/index.scip,graph.db}` 落地＋冪等重跑。

**核心實作要點**
- README Quickstart 段新增 `code-reality build --repo <path>` 一鍵（手動鏈改為進階註記）。
- `plugin/skills/code-reality/SKILL.md` Python 段補一行（build 一鍵取代手動
  producer→stamp→build-cache 鏈；rust 段不動——腿未上線）。
- AGENTS.md 新 capability row（入口＋狀態 ✅）＋kanban 搬 Done。
- 非目標明示：build 不加 mcp_server tool（CLI-only，同 sidecar_migrate 先例）。

**驗證策略**
- L4（python 腿）：`code-reality build --repo ~/Github/ai-rules` 真跑全鏈＋
  `graph_query search` 冒煙（確認建出的庫可用）＋冪等重跑。
- L4（rust 腿）：`code-reality build --repo ~/Github/nautilus_trader`——NT 為
  Rust-dominant 混合 repo；跑畢 `scip_refs`/`callers` 冒煙＋與既有 Aug-28 手動鏈產物
  重現等價（覆蓋 280MB slot 屬預期行為 SM-11 型）。耗時以分鐘計。
- L4（混合合一）：`code-reality build --repo ~/Github/code-reality`（自倉＝混合
  repo：crates/*.rs＋scripts/*.py）→ 單一 graph.db 雙語言查詢（rust 符號＋
  python 符號各至少一查）。
- 文檔驗證：rg 殘留（「手動兩步」敘述已改）、Capabilities row 與實測入口一致。

---

## 合一已驗證（原 deferred 項）；殘餘邊界註記

- **合一 graph 已 POC 驗證並納入 S3**（cat-merge＋graph_db 零改動＋三查詢面綠，
  /tmp/mixedpoc 實錄）——「多 slot＋graph merge」的原始設計題**不需要**。
- 殘餘已知邊界：跨語言**同名**符號碰撞未測（兩 producer symbol scheme 命名空間不同，
  撞上時查詢面自然呈現兩個符號——非靜默丟失）；`--producer both` 與偵測 Mixed
  的優先序＝顯式 flag 蓋偵測。

## 整合策略

- `cargo test -p code-reality` 全綠（新 tests/build.rs＋既有套件零回歸）。
- S4 dogfood L4 三標的：ai-rules（python 腿）＋nautilus_trader（rust 腿，分鐘級）＋
  code-reality 自倉（混合合一）＋冪等重跑。
- 文檔（README/AGENTS/kanban/plugin SKILL.md）與實作同步。
- **發布規劃（user 裁決 2026-08-29）**：本 EP 結案後走 **v0.4**（新能力 minor bump：
  workspace＋plugin 版號＋wrapper pin 五面一顆 commit＋tag v0.4 → CI PyPI×3 →
  消費者 plugin 更新→wrapper `--force` 全 bin 換版）。

## 收尾步驟

1. Capabilities 新 row：「新 repo 數據面一鍵準備」＋入口 `code-reality build --repo <path>` ✅；
   kanban 卡搬 Done/。從 Scenario Matrix 提煉消費場景（SM-1/3/5/6/8 的自包含描述）寫入 row 備註。
2. SYSTEM-MAP：不存在，跳過。
3. instruction 檔：AGENTS.md row（上）；crates/AGENTS.md 若列模組清單則補 build 模組一行。
4. `/audit-test` 對 tests/build.rs 稽核。
