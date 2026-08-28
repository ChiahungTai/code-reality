# EP: binary freshness face — rev embedding, `--version`, stale WARN, auto-reinstall

> **ep_type**: implementation
> **baseline**: `2442692`
> **revision**: v2（dual-axis review NEEDS-REWORK 全數採納重寫；審查記錄見文末 Review Record）

任務一句話：讓「安裝面 binary 與 repo 現況靜默漂移」變 loud——rev
嵌入、`--version` 面、消費時 stale WARN（**兩訊號：HEAD 落後＋
working tree 未 commit**）、post-commit 背景重裝。

動機（2026-08-28 實案）：W3 與 fix relay 弧中，同一 session 三次踩到
`cargo install` 早於程式碼編輯 → live L4 驗證跑舊 binary。**關鍵：事故
窗口在 commit 之前**（編輯未 commit、embedded==HEAD）——單比 HEAD
抓不到動機場景，故 S3 需雙訊號。資料殘留面（sidecar）已由 `2442692`
修復；本 EP 處理 binary 面。

## 慣行定位（查證 2026-08-28，v2 修正歸屬）

- **Rev 嵌入＋`--version`＝Rust 慣行**：vergen 生態（build.rs
  emitter→`cargo:rustc-env`→`env!`）與 ripgrep 手捲 build.rs 兩條
  正規路線。**更正（v2）**：ripgrep 實際用 `git rev-parse
  --short=10 HEAD`（無 describe/dirty/exclude）；本 EP 採 `git
  describe --always --dirty --exclude=*` 的理由是**hash-only 形態
  保證**（`--exclude=*` 排除全部 tag 候選→`--always` 必退 hash；
  本 repo 0 tags 實測輸出 `<hash>[-dirty]`），不是 ripgrep 形態。
  git 缺席時 fallback：不 emit rev（`option_env!` → None → 面
  停用）。
- **Post-commit 自動重裝 hook＝repo 自有工作流自動化，非 Rust 生態
  norm**（如實標示，不冒充）。

## UC 盤點

（v1 內容不變：Backlog 卡已建 `.kanban/Backlog/binary-freshness-face.md`；
無 SYSTEM-MAP；既有 UC 三行「更新」；新增 UC 兩行——freshness face、
安裝面自動收口。掃描範圍：根 AGENTS.md Capabilities、crates/AGENTS.md
bin 清單與依賴行、plugin/README.md。）

**v2 更正**：既有版本面現況＝`pyrefly-lsp --version` 已存在（pkg
version＋engine rev）、umbrella `code-reality --version` 已被 route()
攔截（印 usage、無 rev）——非 v1 宣稱的零存量。

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | 對應能力 |
|---|------|------|---------|---------|
| SM-1 | 消費者問版本 | 任一 bin `--version` | 印 `<pkg>+<hash>[-dirty]`（如 `0.1.0+2442692`；rev 缺席→只印 pkg） | freshness face |
| SM-2 | 安裝面落後 repo HEAD | CR checkout 在場且 HEAD 全長 hash 不以 embedded（剝 `-dirty`）為前綴 | stderr 一行 WARN（每進程一次，兩訊號合計） | freshness face |
| SM-3 | 消費機器無 CR checkout | `CR_REPO` env 缺＋預設路徑不存在 | 零輸出零 spawn | freshness face |
| SM-4 | repo dirty | embedded 带 `-dirty` | `--version` 如實；WARN 比對剝 dirty 後綴（dirty≠過期） | freshness face |
| SM-5 | git 缺席／tarball 建構 | build 時無 git | `option_env!`→None；版本面退 pkg-only；WARN 停用；build log 一行 `cargo:warning` | freshness face |
| SM-6 | commit 後自動收口 | CR repo `git commit` | hook 背景重裝**含依賴傳遞**（code-reality 變更→重裝 producer）；前景零延遲 | 自動收口 |
| SM-7 | MCP 面消費 | `code-reality-mcp` 啟動 | WARN 於 bin main 早段（每進程一次；stdio 協議面只走 stderr）。launchd 常駐面**已知未覆蓋**：重裝不重啟，手動 `launchctl kickstart` | freshness face |
| SM-8 | **動機場景：編輯未 commit**（v2 新增） | CR checkout 在場且 `git status --porcelain -- crates/` 非空 | stderr 一行「working tree has uncommitted changes; installed binary may lag」 | freshness face |

## 段落 0：全域研究摘要（v2 更正）

- **既有版本面**：`pyrefly-lsp.rs:24-30` 已有 `--version`；umbrella
  `src/bin/code-reality/main.rs` route() 已有 `--help|-h|--version`
  共用 arm（印 usage）；`CARGO_PKG_VERSION` 已在 producer 使用。
- **可重用 helper**（v1 漏列）：`common::git_rev_parse_head`
  （common.rs:136，回傳**全長** hash——S3 比對用它）、
  `engine::expand_home`（engine.rs:312）、`engine::git_head`
  （engine.rs:418，tolerant face）。
- **依賴**：`pyrefly-producer`→`code-reality` path dep（Cargo.toml
  實證；crates/AGENTS.md:71 記「only for engine::default_index_path」
  ——S3 後需修訂該行）；bridge 零 workspace dep（依據＝crates/
  AGENTS.md:50「depends on no workspace crate」——v2 更正引用權威）。
- **插入點實況**（v2 更正）：umbrella `--version` 在 route() 層被攔
  （cli.rs 的 `--help` 是 per-tool token 層，看不到 umbrella flag）；
  umbrella 有 15 個子命令各自 dispatch——WARN 接 `bin/code-reality/
  main.rs` main() 早段才蓋全部；MCP bin 另有 `src/bin/code-reality-mcp/
  main.rs` 進入點；bridge bin 是 `src/bin/code-reality-lsp-bridge.rs`
  的 `#[tokio::main] async fn main()` 回傳 `()`（early exit 用
  `std::process::exit(0)`）。
- **bin 清單**：5 bins（`~/.cargo/bin` 實存）。
- **風險假設**（v2 更新）：
  - 〔高→已驗證〕rerun-if-changed 觸發面：同 branch commit 只動
    `refs/heads/<branch>` 不動 `HEAD` 檔（mtime 實證：本 repo HEAD
    mtime 落後 48 commits）——設計已改為三檔 rerun（見 S1）。
  - 〔中〕`cargo install --path` 共用 workspace target fingerprints
    → build.rs 不重跑時 rev 沿用舊值（cargo 源碼實證）——S1 三檔
    rerun 為解；S1 POC 以 real commit 驗證。
  - 〔中〕git spawn 成本：修復 guard 後每進程一次（10-15ms）+
    前置 exists 檢查；MCP 長活進程=session 一次，可忽略。
  - 〔低〕hook 與手動重裝互撞：`cargo install` 冪等（上游源碼
    "always rebuilt"＋替換語義）。
- **callstack 菜單積壓**：無 plan 檔——跳過。

## S1：rev 嵌入（build.rs ×3，v2 重寫）

### Context
- UC 引用：實作「Binary freshness face」嵌入層。
- 語義約束：env 名 `CR_BUILD_REV`（值形態 `<hash>[-dirty]`；git 缺席
  **不 emit**→`option_env!` 為 None）；與 S2/S3 共享。
- 依賴錨點：新檔 `crates/<crate>/build.rs` ×3（定義端）；消費端
  S2/S3。**三檔 rerun**：`<gitdir>/HEAD`＋`<gitdir>/refs/heads/
  <branch>`（HEAD 為 symref 且檔存在時）＋`<gitdir>/packed-refs`
  （存在時）；gitdir 以 `git rev-parse --git-dir` 從 manifest dir
  定位（worktree 下 `../../.git` 是 file 非 dir，路徑 probe 不可靠）。
- 技術選型：`git describe --always --dirty --exclude=*`（hash-only
  保證；非 ripgrep 形態——ripgrep 用 rev-parse --short=10 無 dirty）。
- 成功標準：三 crate 建構後 `option_env!("CR_BUILD_REV")` 有值；
  **real commit 後 `cargo build` build.rs 重跑**（POC）。

### Pseudo Code
```rust
// crates/<crate>/build.rs（三份同體）
use std::path::PathBuf;
fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let gitdir = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"]).current_dir(&manifest).output();
    let describe = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--exclude=*"])
        .current_dir(&manifest).output();
    let (gitdir, rev) = match (gitdir, describe) {
        (Ok(g), Ok(d)) if g.status.success() && d.status.success() => (
            PathBuf::from(String::from_utf8_lossy(&g.stdout).trim()),
            String::from_utf8_lossy(&d.stdout).trim().to_string(),
        ),
        _ => {
            println!("cargo:warning=git metadata absent — CR_BUILD_REV not emitted");
            return; // option_env! → None；S2/S3 面自動停用
        }
    };
    println!("cargo:rustc-env=CR_BUILD_REV={rev}");
    println!("cargo:rerun-if-changed={}", gitdir.join("HEAD").display());
    // same-branch commits update the branch ref, not HEAD — watch it too
    if let Ok(head) = std::fs::read_to_string(gitdir.join("HEAD")) {
        if let Some(branch) = head.strip_prefix("ref: ") {
            let r = gitdir.join(branch.trim());
            if r.exists() { println!("cargo:rerun-if-changed={}", r.display()); }
        }
    }
    let packed = gitdir.join("packed-refs");
    if packed.exists() { println!("cargo:rerun-if-changed={}", packed.display()); }
}
```

### 驗證策略
- POC（**real commit 形態，禁用 touch HEAD**）：`git commit
  --allow-empty -m revbump` → `cargo build -p code-reality -v 2>&1 |
  rg "Running.*build-script"` 確認 build.rs 重跑；還原 commit
  （`git reset --hard HEAD~1`）。次形態：touch
  `.git/refs/heads/main` 亦可（等價觸發面）。
- fallback：`GIT_DIR=/nonexistent cargo build` 一過（warning 路徑，
  手驗）。
- 已知未覆蓋：crates.io tarball 分發（無此面）。

### Invariant Impact
無（rev 只進 binary；`emit_is_byte_deterministic` 為 in-process
emit×2 byte-compare，producer src 不讀 build 時序 env——審查實證）。

## S2：`--version` 面（v2 錨點重定位）

### Context
- UC 引用：freshness face 查詢面（SM-1）。
- 語義約束：輸出 `<CARGO_PKG_VERSION>+<CR_BUILD_REV>`（rev 缺席→
  pkg-only）；**無空格、無 `g` prefix**（describe hash-only 形態）。
- 依賴錨點（v2 更正）：
  - code-reality：**改 `src/bin/code-reality/main.rs` route() 既有
    `--help|-h|--version` arm**——從共用 usage 拆出 `--version` 專屬
    輸出（tests 無 golden 釘住，審查實證）。
  - `pyrefly-index`：`src/bin/pyrefly-index.rs:21` 既有 `--help`
    分支同層加 `--version`。
  - bridge：`src/bin/code-reality-lsp-bridge.rs` argv loop 加
    `--version`→`std::process::exit(0)`（async main 回傳 ()）。
  - `pyrefly-lsp`：既有 `--version` 對齊（pkg 後追加 `+<rev>`）。
- 成功標準：四 bin `--version` 印 rev；exit 0。

### Pseudo Code
```rust
// 共通形態（各 bin 的 argv 早段 / route() 拆 arm）
let rev = option_env!("CR_BUILD_REV");
let face = match rev { Some(r) => format!("{}+{}", env!("CARGO_PKG_VERSION"), r), None => env!("CARGO_PKG_VERSION").to_string() };
println!("{face}");
// code-reality bin：return；pyrefly-index：return ExitCode::SUCCESS；bridge：std::process::exit(0)
```

### 驗證策略
- 三＋一 bin 實跑 `--version`（rev 形態斷言用 `rg -o '\+<hex8,>'`，
  不釘死值）；各 bin 一件回歸測試（umbrella 用 `std::process::
  Command` 對 `cargo run --bin` 或 lib 形態函式）。
- 已知未覆蓋：無。

## S3：消費時 stale WARN——雙訊號（v2 重寫）

### Context
- UC 引用：freshness face 警示面（SM-2/3/4/7/8）。
- 語義約束：**每進程至多一行**（單一 `OnceLock` 蓋兩訊號；guard＝
  第一行 `if WARNED.set(()).is_err() { return; }`）；只走 stderr；
  訊號 1（HEAD 落後）＝checkout 全長 hash（`common::
  git_rev_parse_head`）不以 embedded（剝 `-dirty`）為**前綴**
  （prefix 比對：abbrev 長度漂移免疫）；訊號 2（未 commit 編輯，
  動機場景）＝`git status --porcelain -- crates/`（於 checkout root）
  非空→一句「binary may lag」；checkout 偵測＝`CR_REPO` env→fallback
  `~/Github/code-reality`（`engine::expand_home` 展開；存在才比）；
  失敗（無 git/非 repo）→靜默 return。
- 基礎設施盤點（v2 更正）：helper 放**新模組 `crates/code-reality/
  src/freshness.rs`**（common.rs 是 frozen byte-parity 契約模組，
  非 parity 面不入）；重用 `common::git_rev_parse_head`＋
  `engine::expand_home`。producer 直接呼叫（既有 path dep）；
  **bridge 本地 ~12 行複製**（依據＝crates/AGENTS.md:50「depends on
  no workspace crate」；同步義務寫 crates/AGENTS.md）。
- 依賴錨點（v2 更正）：
  - `src/bin/code-reality/main.rs` main() 早段（**一次蓋 15 子命令
    ＋umbrella**——v1 的 cli.rs 錨點只蓋 scip_refs，錯）
  - `src/bin/code-reality-mcp/main.rs` main() 早段（SM-7；不得
    panic——stdio 協議面）
  - `src/bin/pyrefly-index.rs` main() 早段
  - bridge `src/bin/code-reality-lsp-bridge.rs` main() 早段（本地
    複製版 helper）
- 成功標準：安裝面落後→任一 bin 呼叫一行 WARN；編輯未 commit→
  「may lag」一行；無 checkout 機器零輸出。

### 核心實作要點
- 純函式 `pub fn rev_mismatch(embedded: &str, head_full: &str) ->
  bool`：`head_full.starts_with(embedded.strip_suffix("-dirty").
  unwrap_or(embedded))` 的反相＋`unknown`/空值短路 false（表驅動
  測試釘）。
- 訊息（英文，bin face 慣例自覺標註）：
  `[WARN] installed binary {embedded} != repo HEAD {short} — rerun cargo install --path <CR_REPO>/crates/<crate>`
  `[WARN] working tree has uncommitted changes under crates/ — installed binary may lag (commit triggers auto-reinstall)`

### Pseudo Code
```rust
// crates/code-reality/src/freshness.rs
static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
pub fn stale_binary_warn(crate_dir: &str) {
    if WARNED.set(()).is_err() { return; }   // v2: guard 必須擋後續
    let Some(repo) = checkout_path() else { return };          // CR_REPO || ~/Github/code-reality (expand_home)，exists 檢查
    let Some(embedded) = option_env!("CR_BUILD_REV") else { return };
    if !repo.join("crates").join(crate_dir).exists() { return; }
    if let Some(head) = crate::common::git_rev_parse_head(&repo) {
        if rev_mismatch(embedded, &head) { eprintln!(...); return; }
    }
    if status_porcelain_nonempty(&repo) { eprintln!(...may lag...); }
}
```
（`status_porcelain_nonempty`：`git status --porcelain -- crates/`
output 非空；spawn 失敗→false。）

### 驗證策略
- 單元：`rev_mismatch` 表驅動（`2442692`/`2442692-dirty` 對全長
  同 commit 前綴＝false；不同 commit＝true；`unknown`/空＝false；
  長度不等同前綴對）。
- L4（真機）：① 安裝舊版→`git commit --allow-empty`→跑 umbrella
  `code-reality scip_refs -h`→一行 WARN；② 不 reset、直接編輯
  `crates/code-reality/src` 一行→再跑→「may lag」訊號（動機場景
  重現）；③ `CR_REPO=/nonexistent`→零輸出（SM-3）；④ default
  path（不設 env）正向案例（v1 漏）。
- 已知未覆蓋：MCP stderr 在各 harness Errors tab 可見性（記錄）；
  launchd 常駐面重啟（SM-7 註記）。

### Invariant Impact
無。

## S4：post-commit hook＋文檔（v2 修訂）

### Context
- UC 引用：安裝面自動收口（SM-6）。定位＝repo 自有自動化。
- 語義約束：前景零延遲（背景 `&`）；**依賴傳遞**：`crates/
  code-reality/*` 變更→重裝 code-reality **＋pyrefly-producer**
  （path dep）；root `Cargo.toml`/`Cargo.lock` 變更→三 crate 全裝；
  merge commit（`HEAD~1` 只 diff 第一 parent）＝已知未覆蓋；初始
  commit 邊界 guard（`git rev-parse --verify HEAD~1` 失敗→全裝）。
- 依賴錨點：新檔 `.githooks/post-commit`；`git config core.hooksPath
  .githooks`（一次性；README 註明 **maintainer-layout 專用**——
  hook 內絕對路徑 `~/Github/code-reality` 是本機佈局，外部使用者
  opt-in 前需自改）。
- pattern（v2 放寬）：`*crates/code-reality/*`（含 src/build.rs/
  Cargo.toml——v1 的 `/src*` 漏 build.rs 與 manifest）、
  `*crates/pyrefly-producer/*`、`*crates/code-reality-lsp-bridge/*`。

### Pseudo Code
```sh
#!/bin/sh
log="$HOME/.mosaic/code-reality/install.log"
changed=$(git diff --name-only HEAD~1 HEAD 2>/dev/null) || changed="all"
inst() { echo "$(date '+%F %T') install $1" >> "$log"; \
  cargo install --path "$HOME/Github/code-reality/crates/$1" >> "$log" 2>&1; }
(
  [ "$changed" = all ] && { inst code-reality; inst pyrefly-producer; inst code-reality-lsp-bridge; exit 0; }
  case "$changed" in *crates/code-reality-lsp-bridge/*) inst code-reality-lsp-bridge;; esac
  case "$changed" in *crates/code-reality/*) inst code-reality; inst pyrefly-producer;; esac
  case "$changed" in *crates/pyrefly-producer/*) inst pyrefly-producer;; esac
  case "$changed" in *Cargo.toml|*Cargo.lock) inst code-reality; inst pyrefly-producer; inst code-reality-lsp-bridge;; esac
  echo "$(date '+%F %T') done" >> "$log"
) > /dev/null 2>&1 &
```

### 驗證策略
- L4：`core.hooksPath` 設定→小 commit→install.log 有 install 行、
  `--version` rev 前進、前景 commit <1s；**從 repo 子目錄 commit**
  一次（相對 hooksPath 解析，v1 漏）；跨 crate 傳遞案例（僅動
  `crates/code-reality/src`→producer 也重裝）。
- 已知未覆蓋：merge commit diff 面；launchd 常駐面重啟。

### Invariant Impact
無。

## 整合策略

段序 S1→（S2∥S3）→S4。最終整合劇（v2 更新）：乾淨樹 commit→hook
重裝→`--version` 前進→WARN 消失；**編輯不 commit**→「may lag」訊號
（動機場景閉環）；mosaic slot 重跑 pyrefly-index byte-compare（rev
只進 binary 不進 index 產出）。

## Review Record（v2）

| 軸 | 判定 | 採納 |
|----|------|------|
| 結構/合規 | NEEDS-REWORK（10 findings） | 全數（F1 rerun 三檔＋POC real-commit、F2 錨點/現況更正、F3 SM-7 接線 mcp bin、F4 latency 記錄（guard 修復後可忽略）、F5 hook 依賴傳遞＋寬 pattern、F6 動機缺口→SM-8 雙訊號、F7 格式、F8 typo、F9 權威改 crates/AGENTS.md:50＋:71 修訂入 S4、F10 launchd 已知未覆蓋） |
| 技術正確性 | NEEDS-REWORK（11 findings） | 全數（F1 同上＋worktree `--git-dir`、F2 OnceLock guard 第一行、F3 typo＋expand_home、F4 錨點/段落0 更正、F5 15 子命令＋mcp、F6 ripgrep 歸屬更正、F7 prefix 比對＋full hash＋死邏輯刪、F8 prose 對齊、F9 freshness.rs 新模組、F10 pattern/初始commit/子目錄/maintainer 註記、F11 bridge exit 形態＋檔名） |

## 收尾步驟

1. Capabilities：根 AGENTS.md 加「Binary freshness face ✅」行＋
   kanban 卡入 Done/。
2. 文檔：plugin/README.md（Prerequisites 補 freshness 一行）、
   crates/AGENTS.md（bridge helper 複製同步義務＋:71 修訂「producer
   depends on code-reality lib for default_index_path and
   freshness helper」）、README.md hook maintainer 註記。
3. 記憶：弧線加註（freshness 弧；EP v2 重寫教訓＝rerun-if-changed
   的 git 觸發面要先實證 mtime 語義——結構審查抓到設計級錯誤）。
4. `/audit-test`：S2/S3 測試品質稽核。

## 結算更正（v3，2026-08-28——W5 後續審查發現）

**SM-1／S2 對 mcp bin 不實（已修補）**：本 EP 的「任一 bin `--version`」
（SM-1）與 S2 anchors 對 mcp bin 不成立——`code-reality-mcp` 的 argv
是成員測試式 parse，一切非 `--stdio` flag 靜默落進 HTTP 常駐預設
（`--version` 探測＝掛住一個 listener）。「已知未覆蓋：無」對此不實；
S3（WARN 接線）有涵蓋 mcp 而 S2 漏列＝同檔自相矛盾。修補：W5 後續
微修（ordered per-arg loop＋version/help early-exit＋未知參 exit 2＋
`freshness::version_face()` 兩 bin 共用），回歸釘四面落
`tests/freshness.rs`（version／unknown-arg／組合序／help）。SM-1 自此
真為 any-bin。收尾步驟 1 的 Capabilities 行已於 `929420f` 落地；本弧
無 kanban 卡。EP 隨修補 commit 歸檔。
