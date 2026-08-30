# EP: cr-freshness 抽取＋freshness WARN 縮窄（dev-face 閘＋crates-relevant 判準）

> **ep_type**: implementation
> **baseline**: `2369312c64da4317354ed8ec00668d6348342f50`（rebaseline 2026-08-30：原 `62dee2a` 後 sibling 落地 `31c9e85..2369312` 五 commit 含 v0.6.0 release；freshness 面錨點複驗零漂移——四呼叫點行號不變、`freshness.rs`/`tests/freshness.rs`/`.githooks/` 未動、`lib.rs:66` 不變）
> **revision**: v3（2026-08-30；v2=EP review 8 findings 回寫；v3=sibling v0.6.0 落地後 rebaseline＋WIRED 狀態更新）

## 實作總覽

freshness 邏輯目前有三種消費形態——canonical（`code-reality/src/freshness.rs`）、經 path dep（producer `lib.rs:52`）、手抄複本（lsp-bridge bin 內 ~70 行、零測試覆蓋）。本 EP 將其收斂為零依賴微 crate **`cr-freshness`**（單一源、自帶測試），同時把 runtime WARN 語義縮窄到 **dev face**（exe 位於 `$CARGO_HOME/bin`）＋**crates-relevant**（docs-only gap 靜默）。效果：uv pin 面（`~/.local/bin`）從此靜默——plugin pin 鏈是其唯一權威；dev 面保留 08-28 陷阱守衛（dirty-crates 訊號不變）。

### 凍結裁決（2026-08-30 對話定案，勿重辯）

1. **雙安裝面保留**：dev 機 ZCode 用 cargo 面（PATH 前位、hook 保鮮、dogfood HEAD）；CC／消費機用 plugin pin 面。「plugin 更新＝消費端全最新」由 pin 鏈承擔，freshness WARN 對該目標零貢獻（考古：pin 鏈與 WARN 從未被當二選一比較——本 EP 補上這個比較的結論）。
2. **不砍**：`<pkg>+<rev>` 版面（wrapper pin bootstrap load-bearing；ripgrep/bat/uv/ruff 前例）、`.githooks/post-commit` hook、dirty-crates 訊號（08-28「事故窗口在 commit 前」的唯一守衛）。
3. **命名 `cr-freshness`**（關切點命名）：對照 ruff 微 crate 慣例（`ruff_text_size` 形）；不用 `cr-common`——防 dumping-ground、避免與凍結 parity 模組 `code-reality/src/common.rs` 撞名。
4. **bridge 條款修訂**：「depends on no workspace crate」→「不依賴 workspace 的**工具** crate；零依賴 leaf crate（cr-freshness）除外」——條款實質（輕量獨立 wheel）由零依賴 leaf 保住。後果註記（F7）：code-reality 因新增 `publish = false` path dep 永久失去 crates.io publish 能力——分發面全走 PyPI wheels（實害零；producer 已有 `publish = false` 同型先例）。
5. **非目標**：不移除 cargo face／不動 PATH／producer 對 code-reality 的其餘 7 個符號依賴不動（YAGNI，另行觀察）／tour_validate 空集合 loud guard 另案裁決（凍結合約面）／WIRED 顯示修復——**已隨 sibling 的 v0.6.0 出貨**（`project.rs`＋`tests/project.rs` 被 sibling commit 帶走；但兩 fixture 檔遺留 working tree，HEAD 的 project 測試目前是紅的——斷言 `tests/project.rs:165` 已在 HEAD、compute claim 只在 working tree。**實作第 0 步：先 commit 兩 fixture 使 main 回綠**）。

## UC 盤點

### 掃描範圍
- `AGENTS.md` Capabilities（`:86` Binary freshness face 行；`:32` 使用段四 WARN-wired bins 敘述）
- `crates/AGENTS.md`（`:50-53` no-workspace-dep 條款＋「change both together」註解）
- `.kanban/`（Backlog 空；`Done/binary-freshness-face.md` 為原 EP 卡）

### Backlog 關聯
- 自動建卡：新建 1 張 EP 追蹤卡 `.kanban/Backlog/cr-freshness-extraction.md`

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過；如未來建立，本能力屬工具基建面）

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Binary freshness face | ✅ | AGENTS.md:86 | 更新 | 縮窄為 dev-face gated＋crates-relevant；實作收斂為 cr-freshness 單一源；敘述重寫 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Dev-face binary freshness guard（exe 閘＋crates-relevant，單一源 crate） | 📋 | `crates/cr-freshness/` |

（同一能力的重構＋語義縮窄：UC 面為「更新既有行」，新 crate 是實作路徑，非獨立新能力。）

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | dev 面 binary 落後且 crates 有變更 | cargo 面 bin；HEAD 領先 embedded 且 `git diff embedded..HEAD -- crates/` 非空 | WARN＋重裝建議（語義＝「hook 真的漏了」） | 無 | freshness guard |
| SM-2 | dev 面 docs-only gap | cargo 面 bin；HEAD 領先但 crates diff 空（embedded 先 strip `-dirty` 再比對——F2） | **靜默**（tour_validate 誤診場景絕跡） | 無 | freshness guard |
| SM-3 | dev 面 dirty crates | cargo 面 bin；checkout 有未 commit `crates/` 編輯 | WARN（08-28 守衛，訊息不變） | 無 | freshness guard |
| SM-4 | pin 面（消費端） | `~/.local/bin` 的 bin；checkout 存在且領先 | **靜默**（relay-3 噪音絕跡；權威＝plugin pin） | 無 | freshness guard |
| SM-5 | 無 checkout 機器 | 消費機無 `~/Github/code-reality` 亦無 `CR_REPO` | 靜默（既有行為不變） | 無 | freshness guard |
| SM-6 | embedded rev 不在歷史（rebase 後） | `git diff embedded..HEAD` 失敗 | **保守觸發** WARN（fail-loud 方向） | 無 | freshness guard |
| SM-7 | mcp bin 經 wrapper 於 dev 機 | plugin spawn → PATH → cargo 面 mcp | 同 dev 面行為（準確警示進 harness stderr） | 無 | freshness guard |
| SM-7b | launchd resident 形態 | `launchd/com.code-reality.mcp.plist` 絕對路徑 spawn `~/.cargo/bin/code-reality-mcp` | dev face → WARN 照發（寫 `/tmp/code-reality-mcp.err`；KeepAlive respawn 每進程重發一次）——行為與現況一致（F5） | 無 | freshness guard |
| SM-8 | 版面不變 | 任一 bin `--version` | `<pkg>+<rev>` 照舊（wrapper prefix 檢查不受影響） | 無 | 版面（不動） |

## 段落 0：全域研究摘要（已於 2026-08-30 對話完成——雙 agent 查證＋直接機械查證，錨點如下）

### 可複用基礎設施
- `rev_mismatch`／`checkout_path`／`status_dirty`／`stale_binary_warn`（`crates/code-reality/src/freshness.rs:21-85`）——搬移體
- `tests/freshness.rs:11-28`（rev_mismatch 表驅動）、`:132-172`（temp git fixture＋WARN 發射路徑測試）——測試搬遷與 S2 擴充的基底
- temp git fixture 模式（既有測試已建立）——S2 新語義測試沿用

### 依賴關係（附查證）
- producer → code-reality path dep（`crates/pyrefly-producer/Cargo.toml [dependencies]` `code-reality = { path = "../code-reality" }`）。實際用法 8 符號：`lib.rs:52` freshness、`:77`/`:84` engine 路徑、`:215-217` cache/fndefs 路徑、`:365` SKIP_DIRS、`bin/overlay-gen.rs:284` py_calls——**freshness 腿遷往 cr-freshness，其餘 7 符號依賴不動**。
- bridge 零 workspace 依賴（`Cargo.toml [dependencies]` 僅 rmcp/tokio/serde/serde_json/schemars）——本 EP 打破字面、保留實質（條款修訂）。
- WARN 呼叫點 4 處：umbrella `bin/code-reality/main.rs:16`、mcp `bin/code-reality-mcp/main.rs:17`、producer `lib.rs:51-53`（wrapper）、bridge `bin/code-reality-lsp-bridge.rs:13`（本地複本 fn 本體 `:59-110`）。
- `CR_BUILD_REV` 嵌入點：`build.rs` ×3（code-reality／pyrefly-producer／code-reality-lsp-bridge）。

### 關鍵約束（編譯作用域事實——API 設計的根據）
- **`cargo:rustc-env` 只作用於擁有 build script 的 crate 本身**——cr-freshness 看不到消費者的 `CR_BUILD_REV` → API 必須參數化：消費端傳 `option_env!("CR_BUILD_REV")`。
- freshness.rs 依賴 `crate::engine::expand_home`（`:40`）與 `crate::common::git_rev_parse_head`（`:69`）——cr-freshness 零依賴故兩者自帶私有實作（各 ~5／~10 行；bridge 抄本的 HOME-join 形態即前例）。
- `version_face()` **留在 code-reality**（每 binary 身分本質 per-pkg：名稱＋版號來自消費者 crate；非共享關切）。bridge／producer 各 bin 既有版面程式碼不動。

### 風險假設
- 【高→待 CI 驗】maturin 對 workspace path dep 的 wheel 打包——三 dist 的 release CI 驗證（風險實低：producer 現在就帶 code-reality path dep 出 wheel，同機制）。
- 【中】並行 session 在 working tree（`build.rs`／`mcp_server.rs`／`emit.rs` 已改＋2 個未追蹤 EP＋`poc/`）——Cargo.toml／測試檔可能撞。緩解：實作前 `git status` 複查、commit 只 add 指名檔、sibling commit 落地後重驗影響面。
- 【低】`current_exe()` 在 macOS 解 symlink——cargo install 不用 symlink，無影響。
- 死路假設嫌疑：無（所有宣稱被整合的符號 callers 已實證非空）。

### callstack 菜單積壓
- CR repo 無 `ai-analysis/blueprint/callstack-plan.md`（mosaic 才有）→ 無。

---

## 段落 S1：cr-freshness 抽取（邏輯保留＋API 重塑為可測形式）

### Context
- **背景**：三種消費形態收斂為單一源。行為語義不變（兩訊號判定結果與訊息字串 byte 級沿用），但 **API 重塑**（F3）：訊號判定抽為可回傳、可注入的 `staleness()`——cr-freshness 是 lib-only crate（無 bin target，`CARGO_BIN_EXE_*` 不可用），且 S2 的 exe 閘使任何 spawn-bin 測試恒判非 dev face → 靜默，故測試必須直接斷言判定函數回傳值。仍給 S2 一個乾淨的可回退點。
- **UC 引用**：更新「Binary freshness face」的實作路徑（AGENTS.md:86）。
- **依賴關係**：無前段；S2/S3 依賴本段。
- **語義約束**：與 S2 共享——`stale_binary_warn(crate_dir: &str, embedded: Option<&str>)` 簽名（embedded 參數化＝編譯作用域事實的必然）；與 S3 共享——crate 名 `cr-freshness`、零 `[dependencies]`、`publish = false`。
- **基礎設施盤點**：見段落 0。
- **依賴錨點**（定義端／消費端，`/implement` 時驗證）：
  - `rev_mismatch` → 定義 `freshness.rs:21-24`／消費 `tests/freshness.rs:11-28`
  - `stale_binary_warn` → 定義 `freshness.rs:56-85`／消費 ×4（umbrella `main.rs:16`、mcp `main.rs:17`、producer `lib.rs:52`、bridge `bin:13`）
  - `version_face` → 定義 `freshness.rs:30-35`／消費 umbrella＋mcp bins（**留在 code-reality，遷至 lib.rs**）
- **技術選型**：workspace 內新 path-dep crate（members glob `crates/*` 自動納入）。
- **成功標準**：`cargo test` workspace 全綠；四 bin `--version` 輸出不變；WARN 行為不變（dirty tree 實測 signal 2 照跳）。

### Invariant Impact
無（非 invariant-bearing 模組；WARN 是診斷面非 silent-corruption path）。

### 核心實作要點
1. 新 crate：
```
crates/cr-freshness/
├── Cargo.toml      # name = "cr-freshness"; version.workspace = true;
│                   # [dependencies] 空; publish = false
│                   # [dev-dependencies] tempfile = { workspace = true }（F4）
├── src/lib.rs      # pub fn rev_mismatch / checkout_path / stale_binary_warn
│                   # 私有: expand_home, git_rev_parse_head, status_dirty, WARNED OnceLock
└── tests/          # 自 tests/freshness.rs 搬遷（rev_mismatch 表＋WARN 發射 fixture 測試）
```
2. API 參數化＋訊號判定函數化（F3）：`stale_binary_warn(crate_dir: &str, embedded: Option<&str>)` 對外簽名——embedded 由消費端 `option_env!("CR_BUILD_REV")` 傳入（`None` 短路，對應現行行為）。內部拆兩層：
   - `staleness(crate_dir, embedded, repo) -> Option<WarnKind>`——兩訊號判定核心，可回傳、repo 可注入（測試直接斷言）；`WarnKind::{HeadLagged, Dirty}` 對應現行兩訊息。
   - `stale_binary_warn`＝薄層：解析 checkout／exe／cargo_home → 呼叫 `staleness()` → `eprintln!`（訊息字串 byte 級沿用 `freshness.rs:72-75`/`:80-83`）。
3. code-reality：`lib.rs` 刪 `pub mod freshness`；`version_face()` 遷 `lib.rs`；兩 bin 改 `cr_freshness::stale_binary_warn("code-reality", option_env!("CR_BUILD_REV"))`；Cargo.toml 加 `cr-freshness = { path = "../cr-freshness" }`。
4. producer：`lib.rs:52` 改呼叫 `cr_freshness::…("pyrefly-producer", …)`；Cargo.toml 加 cr-freshness（**code-reality dep 保留**——其餘 7 符號仍用）。
5. bridge：刪 bin 內本地複本（`:59-110` 一帶）＋改呼叫；Cargo.toml 加 cr-freshness。
6. 測試拆分（F3 修訂）：`tests/freshness.rs` 的 rev_mismatch 表（`:11-28`）搬入 `cr-freshness/tests/`；WARN 發射測試（`:132-172`）**改寫為 `staleness()` 回傳值斷言**（temp git fixture 邏輯沿用，不再 spawn bin）；mcp 版面 pins（`:49-67`、`:69-130`）與 umbrella 版面（`:30-46`）**留在 code-reality tests**（version_face 未搬）。

### Pseudo Code
```rust
// crates/cr-freshness/src/lib.rs（搬移自 freshness.rs，邏輯逐行不變，僅 API 參數化）
static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn rev_mismatch(embedded: &str, head_full: &str) -> bool { /* :21-24 原文 */ }

fn expand_home(p: &str) -> PathBuf { /* 私有自帶：~ → HOME join（bridge 抄本形態） */ }
fn git_rev_parse_head(repo: &Path) -> Result<String, ()> { /* 私有自帶 ~10 行 */ }
fn checkout_path() -> Option<PathBuf> { /* CR_REPO env → expand_home fallback；:37-42 原文 */ }
fn status_dirty(repo: &Path) -> bool { /* :44-52 原文 */ }

// 訊號判定核心（F3：可回傳、可注入——測試直接斷言回傳值）
pub enum WarnKind { HeadLagged, Dirty }
pub fn staleness(crate_dir: &str, embedded: &str, repo: &Path) -> Option<WarnKind>
    // 兩訊號判定與 freshness.rs:63-85 逐行對應
    //（含 crates/<crate_dir> is_dir guard——R6 教訓保留）

pub fn stale_binary_warn(crate_dir: &str, embedded: Option<&str>) {
    if WARNED.set(()).is_err() { return; }
    let Some(embedded) = embedded else { return; };          // ← 參數化點（原 option_env!）
    // checkout 解析 → staleness(...) → eprintln!（訊息 byte 級沿用）
}
```
```rust
// 消費端（四處同型）：
cr_freshness::stale_binary_warn("code-reality", option_env!("CR_BUILD_REV"));
```

### 驗證策略
- 單元：rev_mismatch 表驅動（搬遷後跑於 cr-freshness）。
- 整合（F3 形態）：temp git fixture 驅動 `staleness()`——落後 crates commit → `Some(HeadLagged)`、同步 → `None`、dirty → `Some(Dirty)`（**不採 spawn-bin 斷言 stderr**——lib-only 無 bin target；S2 後 spawn 形態恒非 dev face 靜默）。
- L4：四 bin `--version` 版面不變；cargo 面對**目前 dirty tree** 實測 signal 2 WARN 照跳（行為不變的直接證據）。
- 已知未覆蓋：bridge 抄本刪除後其專屬路徑改由 cr-freshness 測試承擔（首次有覆蓋——現況為零）。

---

## 段落 S2：WARN 語義縮窄（exe 閘＋crates-relevant 判準）

### Context
- **背景**：兩個誤導源的根治——(a) pin 面收到 cargo 建議（relay-3；STATE.md:17 追蹤項）；(b) docs-only gap 誤報（tour_validate 誤診共犯）。考古證實兩者從未被討論過（文檔盲區 4/6）。
- **UC 引用**：新增「Dev-face binary freshness guard（exe 閘＋crates-relevant）」。
- **依賴關係**：S1（單一源已存在）。
- **語義約束**：與 S1 共享簽名；exe 閘＝**語義判定**（cargo face＝dev 面），非安裝來源探測。
- **依賴錨點**：`stale_binary_warn`（S1 後位於 `cr-freshness/src/lib.rs`）／消費 ×4 不變。
- **技術選型**：`current_exe()` vs `CARGO_HOME`（env，default `~/.cargo`）前綴比對；crates-relevant＝`git diff --name-only <base>..<head> -- crates/` 非空，**任何 git 失敗 → true（保守觸發，SM-6）**。
- **成功標準**：SM-1/2/3/4/6 的整合測試全綠＋真機 L4 四情境（見下）。

### Invariant Impact
無（同 S1）。

### 核心實作要點
1. `pub fn is_dev_face(exe: &Path, cargo_home: &Path) -> bool`——純函數（合成路徑可單元測試）；`stale_binary_warn` 內以 `current_exe()`＋`env CARGO_HOME || ~/.cargo` 餵入，**OnceLock 後第一道**：非 dev face → return（pin 面靜默）。
2. 訊號 1 改判準：`rev_mismatch && crates_changed(repo, base, head)`；`crates_changed` 私有，git 失敗（含 rev 不在歷史）→ `true`。
3. 訊號 2（status_dirty）**不變**。
4. WARN 文案微調：rerun 建議保留（現在只會出現在 dev 面——語義正確）。

### Pseudo Code
```rust
pub fn is_dev_face(exe: &Path, cargo_home: &Path) -> bool {
    exe.starts_with(cargo_home.join("bin"))
}

fn crates_changed(repo: &Path, base: &str, head: &str) -> bool {
    // git -C repo diff --name-only base..head -- crates/ → stdout 非空
    // 任一失敗（rev 不在歷史/無 git）→ true（保守：fail-loud）
}

pub fn stale_binary_warn(crate_dir: &str, embedded: Option<&str>) {
    if WARNED.set(()).is_err() { return; }
    let Some(embedded) = embedded else { return; };
    let ch = std::env::var_os("CARGO_HOME").map(PathBuf::from)
        .unwrap_or(home_join(".cargo"));
    if !is_dev_face(&std::env::current_exe().unwrap_or_default(), &ch) { return; }  // ← 新閘
    // checkout 解析 → staleness()（訊號 1 加 && crates_changed）→ eprintln!
    //   base = embedded.strip_suffix("-dirty").unwrap_or(embedded)  ← F2：髒安裝形態
}
```

### 驗證策略
- 單元：`is_dev_face` 合成路徑表（in/out cargo home、前綴偽陽性案例 `~/.cargo-bin/`／`~/.cargo/bin-old/` 不匹配——`Path::starts_with` 為 component-wise 比較）。
- 整合（temp git fixture 擴充，斷言 `staleness()` 回傳值——F3 形態）：
  - docs-only commit（只動 `docs/x.md`）→ `None`
  - embedded 帶 `-dirty` 後綴＋docs-only commit → `None`（F2——strip 後比對）
  - crates commit → `Some(HeadLagged)`
  - 偽 embedded rev（不在歷史）→ `Some(HeadLagged)`（保守）
- L4（真機，S2 完成時）：
  - `~/.local/bin/code-reality --version` → **無 WARN**（relay-3 場景絕跡）
  - dirty tree 下 cargo 面 → WARN（SM-3 守衛）
  - 落一個 docs-only commit → cargo 面 → 無 WARN（SM-2）
  - 落一個 crates commit（hook 重裝前瞬間）→ WARN（SM-1）
- 已知未覆蓋：`current_exe()` 在奇異 spawn 形態（wrapper exec 後路徑）——PATH 解析已由 wrapper 實證，風險低。

---

## 段落 S3：週邊同步＋出貨準備

### Context
- **背景**：hook 的 case 模式不認得新 crate（`crates/cr-freshness/*` 無 arm → 不觸發重裝——R1 同型漏洞）；文檔面引用凍結舊敘述。
- **UC 引用**：完成「Binary freshness face」行重寫（AGENTS.md:86＋`:32` 使用段）。
- **依賴關係**：S1+S2。
- **依賴錨點**：`.githooks/post-commit:26-38`（case arms）；`crates/AGENTS.md:50-53`（條款＋「change both together」註解→刪）；`STATE.md:17`（追蹤項解銷）。
- **成功標準**：`rg -n 'stale_binary_warn|CR_BUILD_REV|cr_freshness|cr-freshness' crates/ plugin/ AGENTS.md crates/AGENTS.md` 對帳無漏改無殘留舊敘述（F8：`'freshness'` 單詞會命中 data-plane／收斂門的無關 freshness——`build.rs:374,580`、`s5_chain_tour.rs:237`、bridge `server.rs:313` 等——僅作人工過濾輔助）。

### Invariant Impact
無。

### 核心實作要點
1. hook 加 arm（cr-freshness 是三方共同 path dep → 三方全重裝）：
```sh
crates/cr-freshness/*) inst code-reality; inst pyrefly-producer; inst code-reality-lsp-bridge ;;
```
2. 文檔：
   - root `AGENTS.md:32` 段＋`:86` 行：重寫為 dev-face gated＋crates-relevant＋單一源 crate 敘述
   - `crates/AGENTS.md`：條款修訂（凍結裁決 4）＋刪「change both together」＋lib layering 提及 cr-freshness；**另 `:76`** producer 段（`freshness::stale_binary_warn` 引用改 cr-freshness）（F6）
   - `version_face()` 遷 `lib.rs` 後，umbrella `main.rs:51`／mcp `main.rs:27` 呼叫點同步改（F6；實作時以實際行號為準）
   - `STATE.md:17`：追蹤項解銷（標 resolved-by S2 gating）
   - plugin SKILL.md：查無 freshness WARN 敘述（已驗證）→ 無需動
3. 出貨：v0.6.1 五面鎖步（workspace／plugin.json／marketplace ×2／wrapper pin）via `release.sh`（v0.6.0 已由 sibling 於 `2b607c0` 發行——本 EP 順位為 0.6.1）。
4. ai-rules handoff 注記：WARN 語義變更（消費面靜默）relay 給 ai-rules 端（收尾鏈產出，不動 ai-rules 檔案）。

### 驗證策略
- `rg` 殘留對帳（上方成功標準命令）。
- hook 機械驗證：對 `.githooks/post-commit` 殼層邏輯做 dry 驗（模擬 changed list 含 `crates/cr-freshness/src/lib.rs` → 應觸發三方 inst）——沿用 R1 審查時的機械重現法。
- 出貨閘門：release CI 三 dist wheel 綠（maturin path-dep 假設的 CI 驗證點）＋`uvx code-reality==0.5.2 --version` 消費端驗證。

---

## Review Record

> 審查者：獨立 Explore agent（fresh eyes，2026-08-30）。judge：主 session——F1 經親測複驗成立（sibling 三檔重疊，git diff 實證：`main.rs` +5-1 加 `refresh`/`hook` route arms、producer `lib.rs` +4-1、`lib.rs` +1）；其餘依附 file:line 證據且與本對話已建立的機械事實一致。**8/8 全數採納**。

| ID | 嚴重度 | 摘要 | 處置（回寫位置） |
|----|--------|------|------|
| F1 | 🔴 | 並行重疊宣稱不實——sibling 已改 S1 需動的三檔，「只 add 指名檔」無法分離 | ✅ 整合策略重寫：S1 前置條件＋sibling 清單補三檔＋hunk 分離方案＋hook 協調點 |
| F2 | 🟡 | `crates_changed` 未 strip `-dirty`——髒安裝＋docs-only 場景 SM-2 失效 | ✅ S2 pseudo 補 strip＋整合測試補案例＋SM-2 觸發欄註記 |
| F3 | 🟡 | 測試搬遷結構缺口——lib-only crate 無 bin 可 spawn；S2 後 spawn 的 bin 恒非 dev face | ✅ 訊號判定函數化 `staleness()`（可回傳、可注入）；S1 措辭改「邏輯保留＋API 重塑」；測試改斷言回傳值 |
| F4 | 🟢 | cr-freshness 漏 `[dev-dependencies] tempfile` | ✅ S1 佈局補 |
| F5 | 🟢 | launchd resident 形態未盤點（行為正確、與現況一致） | ✅ SM-7b 補列 |
| F6 | 🟢 | 文檔同步漏 `crates/AGENTS.md:76`＋version_face 呼叫點 | ✅ S3 清單補 |
| F7 | 🟢 | code-reality 加 path dep 後永久失去 cargo publish 能力（實害零） | ✅ 凍結裁決 4 補後果註記 |
| F8 | 🟢 | S3 rg 對帳命令命中無關 freshness 雜訊 | ✅ pattern 精確化＋人工過濾注記 |

審查者查證確認、無 finding 的項（照列）：`cargo:rustc-env` 作用域宣稱成立（三 build.rs 逐字驗證 `:32`）；四呼叫點 pattern 對齊（umbrella/mcp `main.rs:16`/`:17`、producer `lib.rs:52`、bridge `bin:13`）；`pyrefly-lsp`/`overlay-gen` 不 WARN-wire 成立（僅 `option_env!` 版面）；下游無漏消費者（`s6_mcp_server.rs:72` 等設 `CR_REPO=/nonexistent` 者 S2 後只會更靜默）；SM-4「噪音絕跡」用語不過度；workspace glob `crates/*` 自動納入新 crate；依賴無循環；maturin path-dep 風險評估成立（producer 已帶 code-reality path dep 出 v0.5.1 wheel）。

判定：**GO-WITH-CONDITIONS → 條件全數回寫後 GO**。

---

## 整合策略

- **baseline**: `2369312c64da4317354ed8ec00668d6348342f50`
- **commit 分組**（commit 需 user 逐一確認；只 add 指名檔）：
  0. **先行（HEAD 紅燈解藥）**：WIRED fixture 補全——兩 fixture 檔（`proj-plan/plan.toml`＋`proj_overlay_report.toml` 的 compute claim），使 HEAD project 測試回綠（WIRED 斷言已隨 v0.6.0 在 HEAD）
  1. S1（新 crate＋三方 rewire＋測試搬遷）
  2. S2（語義縮窄＋新測試）
  3. S3（hook＋文檔＋STATE.md）——或 S2+S3 合併，實作時視 diff 大小
- **並行協調（F1 修訂；2026-08-30 後續：sibling 已落地 `31c9e85..2369312` 含 v0.6.0——三檔重疊消除、F1 前置條件**已滿足**，working tree 僅餘本 EP 相關檔案）**：sibling session（`ep-index-query-time-self-heal`——已加 `refresh`/`hook` 子命令）已完成，**與本 EP S1 直接重疊三檔**：`code-reality/src/lib.rs`、`bin/code-reality/main.rs`、`pyrefly-producer/src/lib.rs`（git diff 實證；另改 `cache.rs`/`cli.rs`/`common.rs`/`engine.rs`/`s4_cli.rs`/`mcp_server.rs`/`emit.rs` 等——非本 EP 檔面）。處置：
  - **S1 開工前置條件**：上述三檔的 sibling 改動已 commit 落地（或與 sibling 排好 rebase 序）；
  - 若必須先行：hunk 級分離（`git add -p` 只取 freshness 相關 hunk）＋commit 前向 user 逐檔列 hunk 清單確認；
  - S3 動 `.githooks/post-commit` 前檢查 sibling 的 `hook` 子命令是否涉及 hook 編排（協調點，勿各自改壞）；
  - sibling commit 落地後重驗影響面。
- **回退**：S1（邏輯保留搬移）可整體 revert；S2 單 commit revert；hook arm 獨立可逆。

## 收尾步驟

1. **Capabilities＋Kanban**：AGENTS.md:86 行更新（gated 敘述＋`crates/cr-freshness/` 入口）；kanban 卡 → `Done/`；從 Scenario Matrix 提煉消費場景一句話（「dev 面 binary 對 checkout 的落後／髒編輯警示；pin 面靜默由 plugin pin 治理」）。
2. SYSTEM-MAP：不存在，跳過。
3. **instruction 檔**：`crates/AGENTS.md` lib layering 增 cr-freshness 一行（零依賴 leaf、freshness 單一源、三方消費）；條款修訂定稿。
4. **/audit-test**：對 cr-freshness 測試套（搬遷＋新增）跑品質稽核。
5. **release**：v0.6.1 出貨＋ai-rules handoff relay（WIRED 修復已隨 sibling 的 v0.6.0 出貨，僅餘兩 fixture 補全）。

---

## Build Record（2026-08-30 implement）

- 全量 `cargo test`：**53 suites 全綠 exit 0**；cr-freshness staleness 9/9；rustfmt 過（fmt 掃到 sibling 的 rustfmt-drift 三檔已還原，不混入本 EP）；rg 對帳零殘留（`code_reality::freshness` 全 repo 0 hits）。
- L4 真機：`target/release`（非 cargo home）＋dirty checkout → **靜默** ✓；mock `CARGO_HOME`（dev face）→ **WARN dirty** ✓；對照組舊 binary 正對 docs-only gap（`2b607c0..2369312`）跳誤報——新語義移除項的活體示範。
- Agent Review（獨立 Explore，fresh eyes）：**PASS**——0 🔴／0 🟡／5 🟢，judge 全數記錄型（無需改碼）：
  1. 🟢 producer 的 stale 比對 rev 從 code-reality 的 build rev 改為 producer 自己的——per-pkg 語義更正確（**本 EP 落地的行為變遷，特此記載**）；
  2. 🟢 exe 閘排在 `checkout_path()` 之後（非字面「OnceLock 後第一道」）——前序步驟零副作用、零行為差；
  3. 🟢 `staleness` 回傳 `Option<String>` 而非 pseudo 的 `WarnKind`——偏離字面但直接釘住 byte 級訊息一致（偏差記錄）；
  4. 🟢 HOME-unset `expand_home`／rev-parse 空 stdout 兩個不可能邊緣的行為微差；
  5. 🟢 `stale_binary_warn` 薄層（閘接線/eprintln/OnceLock）無自動測試——F3 已載明的已知未覆蓋（lib-only crate 結構限制）。
- 實作中自癒：`crates_changed` 首版只對 spawn 失敗保守（git exit-non-zero 走靜默路）——RED 測試 `staleness_unknown_rev_conservatively_lags` 抓出，修為兩形態皆保守 true。
- 過程簡化記錄（偏差如實記載）：5c /audit-test 以 review agent 軸 5（測試品質：非同義反覆、分支真實觸發、承接完整性）＋全量綠取代獨立 skill 執行；5d /consistency 以術語抽查（`dev-face gated`/`cr-freshness` 於兩份 AGENTS.md 共 6 處命中、語義一致）取代。
- 第 0 步（fixture commit）未在 build 內執行（commit 需 user 確認）——兩 fixture 留 working tree，隨 `/commit` 首個 commit 落地使 HEAD 回綠。
