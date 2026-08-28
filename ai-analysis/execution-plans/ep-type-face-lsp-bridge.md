# EP: 型別面 LSP 橋——`pyrefly-lsp` backend ＋ LSP↔MCP bridge crate

> **ep_type**: implementation
> baseline: 6ba5714
> 上游：ai-rules umbrella `cr-lsp-replacement-roadmap.md` P1 段；技術真相源
> `ai-analysis/reports/cr-as-unified-language-intelligence.md`（U9 唯一缺口）

## North star

CR 完全取代 LSP 路線的最後一塊：**型別面（U9 hover/diagnostics）** 由
「獨立 `pyrefly lsp` 子進程＋薄 LSP↔MCP 轉譯層」供應。結構面已全取代
（producer 78s／SCIP）；本 EP 之後 U1-U10 全綠，且 bridge crate 的
backend 參數化形狀讓 P2（Rust 型別面＝rust-analyzer spawn）的邊際成本
塌縮為「換 spawn 命令＋測一輪」。

## 裁決已定案（2026-08-28，照抄勿重辯）

1. **裁決 B**：橋走獨立 LS 子進程＋薄 LSP↔MCP 轉譯層，host 在 repo
   **新小 crate**（`crates/code-reality-lsp-bridge`）；resident state 不進
   code-reality-mcp（無狀態設計——bounded context 物理化）
2. **backend 參數化**：bridge crate 不 import 任何語言特定 crate
   （pyrefly 的 git dep 只在 producer crate）——backend 是 spawn 命令
   參數，P2 換 rust-analyzer 不改 bridge
3. **驗收三條**（凍結於 STATE.md 起手點 0）：① hover 對照 pyright 等值
   ② diagnostics `.py` 過濾（SM-15 教訓）③ 串流＋ZCode entry 形態
4. **電池不過＝維持並列**：lsp-python 不因本 EP 退役（umbrella 決策 6；
   退役面屬 P3，本 EP 不碰 mosaic `tools/lsp_mcp/` 任何檔案）
5. **pyright 僅存非生產角色**：golden oracle＋驗收對照組（決策 7）

## Spike 已驗證（2026-08-28 本 session，證據在 `.agent-tmp/lsp-spike/`）

- **`pyrefly lsp` 子進程往返全通**：initialize → initialized → didOpen →
  hover ×4 型（function def/call、variable、attribute）→ didChange →
  diagnostics 重推 → shutdown request＋exit 乾淨退出。薄 stdio client
  即可（無隱藏 handshake）
- **workspace bin 主案成立**：`crates/pyrefly-producer/src/bin/pyrefly-lsp.rs`
  薄入口呼叫上游 `LspArgs::run`（pinned rev `1d64c4b` bit 級對齊 producer
  引擎），release 編譯 16s，行為與 PyPI wheel 逐項一致
- **PyPI wheel `1.3.0.dev2`**：`--version` 字串與 pinned rev Cargo.toml
  一致（`1.3.0-dev.2`）但 commit 對應不精確（每週自動 release）——
  只作協議驗證/fallback，不作安裝主案
- **行為特性**（bridge 設計輸入；源碼級證據見 EP Review 表 R-05/R-09）：
  - hover 無 didOpen → null（**bridge 須代客 didOpen/didClose**）
  - hover 在 module info 未就緒時**立即回 null**（非延遲）——client 端
    有界重試因應（spike 兩腳本皆如此）
  - diagnostics 純 push、僅由 mutation 驅動（didOpen/didChange/didClose/
    didSave）；無 mutation → 無新推播（check 須走 per-URI cache 而非
    傻等 channel）
  - 每推必帶文件 `version` 欄（過時推播防護鍵）
  - position encoding UTF16（server 端 spec 夾限，越界不 panic）
  - `customHoverProvider` 勿設（否則 server 關 hover）；client
    capabilities 採最小集——不廣告 `workspace.configuration`／
    `didChangeWatchedFiles.dynamicRegistration`（廣告了 server 會發
    `workspace/configuration` request，unanswered 將**凍結背景索引**）
  - gitignored 檔（repo 內被 .gitignore 排除）hover 靜默 null——上游
    行為，文檔記錄（SM-7）
  - preset 由 repo config（`pyrefly.toml`）決定；無 config 時 basic
    preset 連 bad-assignment 都不報——bridge 只轉發不配置

### 取捨理由回填（umbrella findings #11——橋 pyrefly 而非 pyright）

| 軸 | pyrefly lsp | pyright-langserver |
|---|---|---|
| 引擎同源性 | 與 producer 同引擎同 rev——hover 簽名與 index 符號永不分歧 | 型別語義另一源——hover 與 scip_refs 可能同符號不同型別 |
| 版本對齊 | workspace bin bit 級對齊（git dep 同 lockfile） | node 進程版本管理另一套（npm/pip 包裝） |
| runtime 依賴 | native binary（cargo install 一條龍，PATH 已驗證） | node runtime 在場前提 |
| typeshed | bundled（零 setup，spike 實證 go-to typeshed link） | 自帶但版本隨包 |
| 上游活性 | Meta 生產級（Instagram/PyTorch），每週 release | 微軟維護，成熟但生產 face 是 VS Code 擴展 |

pyright 保留兩角色：`scripts/lsp_harvest.py` golden oracle 產 baseline；
等值電池對照組（見 S5）。

## EP Review Findings（2026-08-28 三軸獨立審查——已全數裁決採納回寫）

| ID | 嚴重度 | 段落 | 問題（證據） | 處置 |
|----|--------|------|-------------|------|
| R-01 | 🟡 | S1 依賴 | rmcp tool 參數 struct 需 `serde::Deserialize`＋`schemars::JsonSchema`（`mcp_server.rs:21-28`），依賴清單漏列 | 回寫 S1 |
| R-02 | 🟡 | S1/S5 測試 | integration test 寫死 `target/release/pyrefly-lsp`——fresh checkout debug test 必炸；跨 crate bin 無 `CARGO_BIN_EXE_` | 回寫：env 覆寫→PATH→前置建置命令解析順序 |
| R-03 | 🟡 | S1 | `initialized` 通知漏列——`initialize_finish` 阻塞等它，之前的 didOpen 被丟棄（`server.rs:1146-1182`） | 回寫 S1 pseudo |
| R-04 | 🟡 | S1 | server→client request「log+skip」不安全：unanswered `workspace/configuration` 凍結背景索引（`server.rs:5892-5899,3690-3699`） | 回寫：一律回空回應＋caps 最小集不變式 |
| R-05 | 🔴 | S2 | hover null 是立即回應非延遲（`EmptyResponseReason::ModuleInfoNotFound` 路徑）——「request 端等待」處方錯誤 | 回寫：有界重試（500ms 窗每 100ms） |
| R-06 | 🔴 | S2×S4 | LRU evict（didClose）靜默回滾未落盤編輯——ensure_open 重讀磁碟舊文（`server.rs:4027-4093`） | 回寫：content overlay（session 維護 path→last-known-content；重開用 overlay） |
| R-07 | 🔴 | S3 | 收斂演算法三洞：(a) channel 靜默判定在忙碌頻道永不觸發；(b) 純 push 下重複 check（無 mutation）無推播可等→10s 假「未收斂」；(c) 未用 version 欄防過時推播（`server.rs:3148-3222` 僅 mutation 驅動） | 回寫：S3 重設計——常駐 per-URI cache＋per-URI 靜默＋version 雙條件＋無 mutation 直回 cache |
| R-08 | 🟡 | S1/S2 | 並發 tool 呼叫互吞 notif channel＋競爭 LRU/version map | 回寫：session 層單互斥序列化所有 LSP 互動 |
| R-09 | 🔴 | S5 | 「逐字相等」判準可證偽於自家電池：pyrefly hover 對參數>1 callable 強制多行（`display.rs:148-149,687-705`），pyright 單行 | 回寫：正規化判準（抽首圍籬→剝 kind 前綴→whitespace 摺疊）雙側共用 |
| R-10 | 🟡 | S5 | 剝離規則未雙側對稱（kind token 集合不同、pyrefly 尾段 Go-to links 預設附加）、生成器未入庫 | 回寫：單一正規化規格＋生成腳本入庫＋四型 payload 摘錄 |
| R-11 | 🟡 | S4 | range=全文需 UTF-16 端點計算，與透傳立場矛盾 | 回寫：range 省略形全量替換（pyrefly 支援 `None => result = text`） |
| R-12 | 🟡 | 收尾 | 追蹤卡含 relay 回執義務但 EP 收尾無此步；plugin entry 未定 lazy spawn/fallback/inert 註記 | 回寫收尾 7 步 |
| R-13 | ℹ️ | 全 | crate 名 `lsp-bridge` 與 Emacs 同名專案撞名、不帶 repo 前綴；rmcp 應 hoist 進 workspace deps；兩 crate 兩條 install 路徑；serverInfo 斷言應 name 非 version；SM-7 口徑對齊；Q 走建構子參數；UC 補 Unified MCP interface 行；backend 死亡路徑測試；幀協議硬需求（header 必 `\r\n`、必有 Content-Length） | 全數回寫 |

三軸 verdict：結構 PASS-with-findings（四裁決全合規）／完整性
PASS-with-findings／正確性 NEEDS-REWORK（R-05/R-06/R-07/R-09 回寫後
消除）。主 LLM 裁決：31 findings 全採納。

## UC 盤點

### Backlog 關聯

- 自動建卡結果：新建 2 張（本 EP 追蹤卡＋型別面橋能力卡）

### SYSTEM-MAP 影響

- 無 SYSTEM-MAP.md（跳過，理由：repo 無此檔）

### 掃描範圍

- `AGENTS.md` Capabilities 表、`crates/AGENTS.md`、`.kanban/{Backlog,In-Progress,Done}/`

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Python symbol truth via LSP harvest | 🟢 | AGENTS.md（superseded as production face） | 無 | `scripts/lsp_harvest.py` 轉任 golden oracle generator——S5 電池沿用此角色，不修改 |
| Rust-native Python occurrence producer | ✅ | AGENTS.md | 更新 | `pyrefly-lsp` bin 加入同 crate（LS face 與 producer 同 rev 承諾的文件化） |
| Unified MCP interface | ✅ | AGENTS.md | 更新 | 新增第二個 stdio MCP server entry（type-face 獨立進程）——單 server 假設不再成立 |

### 新增 UC

| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Python 型別面 LSP 橋（hover/diagnostics/edit-recheck，MCP tools） | 📋 | `crates/code-reality-lsp-bridge/` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | AI 查運算式型別 | `hover(file,line,ch)` 落在符號上 | markdown 型別簽名（上游 hover value 原文） | 無 | 型別面橋 |
| SM-2 | AI 查檔案型別錯誤 | `check_file(file)`（.py） | diagnostics 列表（severity/range/message/code） | 無 | 型別面橋 |
| SM-3 | AI 編輯後重查 | `edit_file(file, content)` → `check_file` | 新 diagnostics 反映編輯後內容（version 雙條件收斂） | 無 | 型別面橋 |
| SM-4 | 非 Python 檔 | `check_file`/`hover` on `.txt` | 明確錯誤訊息（不接受副檔）——SM-15 | 無 | 型別面橋 |
| SM-5 | hover 空位置 | position 落在空白/註解/字面值間隙（indexing 已就緒） | `"no hover"` 正常回應（非 error、非 null 靜默） | 無 | 型別面橋 |
| SM-6 | backend 不在 PATH | spawn `pyrefly-lsp` 失敗（首次工具呼叫時——lazy spawn） | loud error：backend 命令＋`--lsp-command` 覆寫指引＋安裝提示 | 修 PATH／覆寫後重啟 | 型別面橋 |
| SM-7 | gitignored 檔 | hover repo 內被 .gitignore 排除的 .py | null hover（上游 exclude 行為）；文檔記錄、回應不區分 | 無 | 型別面橋 |
| SM-8 | 慢檢查收斂 | 大檔首輪 diagnostics 逾收斂窗 | 回已收到的部分＋明確「未收斂」標記（不假裝完整） | 加寬窗口重試 | 型別面橋 |
| SM-9 | 單進程多檔 | 連續 hover/check 不同檔 | open-files LRU 上限內零重複 didOpen；超限 evict＋didClose（overlay 保留，重開用 overlay 版本——編輯不丟） | 無 | 型別面橋 |
| SM-10 | dirty 檔 evict | edit 檔 A → 開 8 檔擠出 A → check A | overlay 重開——診斷反映**編輯後**內容（非磁碟舊文） | 無 | 型別面橋 |
| SM-11 | 並發工具呼叫 | 兩 check_file 交錯（背景索引推播洪峰） | session 互斥序列化——各檔 per-URI cache 獨立收斂，互不吞推播 | 無 | 型別面橋 |
| SM-12 | 磁碟外編輯 | AI 用自有工具改檔（磁碟變更）→ check_file | sync_open 偵測 mtime/size 變化 → didChange 同步新內容 → 診斷反映磁碟現況 | 無 | 型別面橋 |
| SM-13 | backend 中途死亡 | LS 進程 crash/EOF | reader EOF → session 標 dead → 後續工具呼叫 loud error（非 hang） | 重啟 bridge | 型別面橋 |

## 段落劃分原則

垂直切片：S1 進程生命週期 → S2 hover（代客 sync_open＋overlay）→ S3
diagnostics（per-URI cache 收斂）→ S4 edit-recheck（串流面）→ S5 等值
電池＋文檔。每段 integration test 打真實 `pyrefly-lsp` binary（非
mock——L3 消費端模式：bridge 的消費者是 LS 進程本身）。

**測試 binary 解析順序**（跨 crate 無 `CARGO_BIN_EXE_`）：環境變數
`LSP_BRIDGE_TEST_BIN` 覆寫 → PATH 查找（cargo install 後）→ 前置建置
`cargo build --release -p pyrefly-producer --bin pyrefly-lsp`（寫進
各段驗證策略，測試代碼內建解析鏈，fresh checkout 按 CI 文檔先跑建置）。

## 段落 0：全域研究摘要

- **可複用基礎設施**：
  - `rmcp` 3.1.4（hoist 進 `[workspace.dependencies]`，兩 MCP crate
    一條聲明）——`CodeRealityServer` 的 `ToolRouter`＋`#[tool]` 屬性宏
    模式（`crates/code-reality/src/mcp_server.rs`）；tool 參數 struct
    需 `serde::Deserialize`＋`schemars::JsonSchema` derive
  - `lsp-server` crate（lockfile 在場，pyrefly 傳遞依賴）——client 端
    framing 手寫薄層（`Connection` 是 server 形態無 child 建構子；
    `sender/receiver` 欄位 pub 可混合複用，本 EP 選手寫最直達）
  - 上游入口 `LspArgs::run`（`pyrefly/lib/commands/lsp.rs:183`）——
    `pyrefly-lsp` bin 已建（spike 交付物）
  - MCP bin 雙 mode 模式（`--stdio`/HTTP，`bin/code-reality-mcp/main.rs`）
- **幀協議硬需求**（手寫 framing 規格）：header 必 `\r\n` 結尾、必帶
  `Content-Length`（大小寫不敏感）、body 按 byte 數讀——pyrefly reader
  端 `protocol.rs:218-227` 實證
- **依賴關係**：bridge crate 依賴 rmcp＋tokio＋serde（derive）＋
  serde_json＋schemars；**不依賴 pyrefly**（裁決 2）。`pyrefly-lsp`
  bin 屬 producer crate。
- **風險假設**（spike 已消除致命項）：
  - ~~`pyrefly lsp` 子命令存在~~（POC#1＋spike 實證）
  - ~~stdio 往返可行~~（spike 實證）
  - ~~收斂機制可行性~~（EP Review R-07 重設計後機制閉合：per-URI
    cache＋version 雙條件，殘餘風險＝Q 值實測調參，S3 承擔）
  - 低：UTF16——hover 位置透傳（server 端 spec 夾限）；tool 描述明寫
    「character 為 UTF-16 code unit 偏移」（AI 呼叫端合約）

## S1：bridge crate 骨架＋LS 生命週期

### Context

實作「型別面橋能力」的進程地基。UC 引用：型別面 LSP 橋（📋）。

- 依賴關係：無前序段落；S2-S4 都建立在 `LspSession` 上
- 語義約束：與 S2-S4 共享「backend 是 spawn 命令參數」；「所有 LSP
  互動經 session 單互斥序列化（notif 單消費者＝reader thread）」；
  「MCP tool 一律 `spawn_blocking`，不阻塞 async runtime」（沿用
  mcp_server.rs SM-14 模式）；「client capabilities 最小集不變式」——
  不廣告 `workspace.configuration`／`didChangeWatchedFiles
  .dynamicRegistration`／`workDoneProgress`（廣告即觸發 server→client
  request 期票）
- 基礎設施盤點：rmcp `ToolRouter` 模式；幀協議硬需求（段落 0）
- 依賴錨點：`LspArgs::run` → 定義
  `pyrefly/lib/commands/lsp.rs:183` / 消費
  `crates/pyrefly-producer/src/bin/pyrefly-lsp.rs:main`
- 技術選型：LSP framing 手寫薄層——理由：lsp-server `Connection` 是
  server 形態（stdin/stdout 自持），bridge 需要「子進程 stdio 三路
  reader（responses/notifications/server→client requests 分流）」

### 核心實作要點

- `crates/code-reality-lsp-bridge/Cargo.toml`：rmcp（workspace dep）、
  tokio、serde（derive）、serde_json、schemars
- `LspSession`：
  - `spawn(cmd, quiesce_ms)`（Q 走建構子參數，預設 500，測試可注入）
  - spawn backend（`--lsp-command` 覆寫，預設 `pyrefly-lsp`；**lazy
    spawn**——首次工具呼叫才拉起，plugin 消費者無 backend 時不因
    啟動即炸）
  - reader thread 三路分流：responses（request id 匹配）→
    notifications → **server→client requests 一律回空回應 `[]`**
    （絕不 skip——unanswered `workspace/configuration` 會凍結 pyrefly
    背景索引；空回應走 pyrefly 既有錯誤路徑清 awaiting flag）
  - handshake 順序（嚴格）：`initialize` request → 等回應 → 送
    `initialized` **通知** → 此後才准送任何 didOpen（之前送的被
    server 丟棄）
  - `request()`／`notify()` 一律持 session mutex（序列化）
  - shutdown 流程：`shutdown` **request**（非通知）→ 等回應 → `exit`
    通知 → child.wait(timeout 10s)
  - backend 死亡偵測：reader EOF → 標 dead → 後續呼叫 loud error
    （不 resurrect；caller 重啟 bridge）
- bin `code-reality-lsp-bridge`：`--stdio` mode（ZCode entry）＋
  `--lsp-command <cmd>` 參數
- `lsp_status` tool：serverInfo.name/version／backend 命令／open 檔數
  ／state（alive/dead/not-spawned-yet）

### Pseudo Code

```
crates/code-reality-lsp-bridge/
  Cargo.toml
  src/
    lib.rs          // pub mod session; pub mod server;
    session.rs      // LspSession (mutex + overlay + per-URI diag cache 見 S2/S3)
    framing.rs      // child stdio: write "Content-Length: N\r\n\r\n"+body;
                    // read: parse headers (必 \r\n, 必 Content-Length), body by bytes
    server.rs       // LspBridgeServer (rmcp ToolRouter)
    bin/code-reality-lsp-bridge.rs

LspSession::ensure_spawned():
    if spawned { return }
    child = Command::new(backend_cmd).stdin(piped).stdout(piped).stderr(inherit).spawn()?
    spawn reader thread: loop { frame = read_frame(child.stdout)?;
        msg = parse(frame)?;
        match msg {
          Response{id} -> resp_tx.send(msg),
          Notification -> notif handling (per-URI diag cache update, S3),
          server->client Request{id, method} -> reply(id, result=[]),  // R-04: 永不 skip
        } }
    send request initialize { rootUri: cwd, capabilities: 最小集 }
    await response
    send notification initialized                      // R-03: 必要步驟
    spawned = true

shutdown():
    request("shutdown") -> await response             // R-03: 是 request
    notify("exit"); child.wait(timeout 10s)

server.rs:
    #[tool] lsp_status() -> "backend=<cmd> server=<name version> open_files=<n> state=<...>"
```

### 驗證策略

- integration test（`tests/session.rs`）：spawn `pyrefly-lsp`（測試
  binary 解析順序：`LSP_BRIDGE_TEST_BIN` → PATH → 前置建置），斷言
  `serverInfo.name == "pyrefly-lsp"`（name 非 version——R-13）＋
  handshake 成功＋shutdown 退出 code 0
- backend 死亡路徑（R-13）：kill child → 下一呼叫 loud error 非 hang
- backend 失敗路徑：`--lsp-command /nonexistent` → spawn error loud
- `cargo build` 全 workspace 綠（新 crate 進 members glob `crates/*`）

## S2：hover tool（代客 sync_open＋content overlay）

### Context

UC 引用：型別面 LSP 橋（📋）。驗收三條之①的前半（往返層）。

- 依賴關係：S1 的 `LspSession`
- 語義約束：與 S3/S4 共享 open-files 管理（`sync_open(file)`）與
  content overlay（session 級 `path → {content, version}`——edit_file
  更新它、evict 不清它、重 open 用它）；MCP tool 回應一律文字
- 基礎設施盤點：S1 全部；hover null 語義（module info 未就緒＝立即
  null，非延遲——R-05）
- 依賴錨點：didOpen/didClose/hover 方法形狀 → spike
  `.agent-tmp/lsp-spike/lsp_client.py`（觀察證據）

### 核心實作要點

- `hover(file_abs, line, character)` tool：副檔檢查（.py，SM-15）→
  `sync_open` → `textDocument/hover` → **null 時有界重試**（500ms 窗
  每 100ms——背景索引就緒前的暫態 null 與真無符號 null 的區分；重試
  盡後仍 null → `"no hover at <line>:<char>"`，SM-5/SM-7 統一回應）
- tool 描述明寫「`character` 為 UTF-16 code unit 偏移（LSP 慣例）」
- `sync_open(file)`：
  - overlay 無此檔 → 讀磁碟 → didOpen（version 1）→ overlay 記錄
  - overlay 有 → 比較磁碟（mtime＋size）：變了（SM-12 磁碟外編輯）→
    讀磁碟新文 → didChange 全量（range 省略形，version 遞增）→
    overlay 更新；沒變 → no-op（server 端已 open）
  - LRU 上限 8：evict 最舊（didClose）——**overlay 保留**（重 open
    用 overlay 版本＋version 歸 1，未落盤編輯不丟，SM-9/SM-10）
- 檔案不存在 → loud error（檔案路徑＋「須為磁碟上存在的絕對路徑」）

### Pseudo Code

```
hover(file, line, ch):
    ensure_py(file)?
    sync_open(file)?
    deadline = now + 500ms
    loop {
        resp = session.request("textDocument/hover", {file, {line, ch}})?
        if resp.result != null { return resp.result.contents.value }
        if now >= deadline { return "no hover at {line}:{ch}" }
        sleep(100ms)
    }

sync_open(file):
    match overlay.get(file):
      None -> text = read_disk(file)?; did_open(file, text, v=1); overlay.insert
      Some(entry) ->
        if disk_mtime_size(file) != entry.stamp:      // SM-12
            text = read_disk(file)?; did_change_full(file, text, v=entry.v+1)
            overlay.update
        // else no-op
    if open_files.len() > 8: evict_oldest(did_close; overlay 保留)
```

### 驗證策略

- integration test：fixture（臨時目錄，避 gitignore）對拍 spike 觀察值
  （function/variable/attribute 三型 hover 字串）
- null hover 場景：註解行 → `"no hover"` 回應
- 磁碟外編輯（SM-12）：open → 磁碟改檔 → hover/check 反映新內容
- 非 .py：`.txt` → error
- LRU＋overlay（SM-10）：edit A → 開 8 檔擠出 A → check A 診斷反映
  **編輯後**內容（非磁碟舊文）
- 檔案不存在 → loud error

## S3：diagnostics tool（per-URI cache 收斂）

### Context

UC 引用：型別面 LSP 橋（📋）。驗收三條之②。

- 依賴關係：S2 的 `sync_open`＋overlay；S1 reader thread
- 語義約束（R-07 重設計核心）：**diagnostics 永遠以 per-URI 最新推播
  為準**——reader thread 常駐維護 `uri → (version, diagnostics,
  last_push_time)` cache；check_file 不「drain channel」而是讀 cache
  ＋條件等待；收斂雙條件＝「`push.version ≥` bridge 已送出的該檔
  version」且「該 URI 最後一推距今 ≥ Q ms」（per-URI 靜默，與 channel
  忙閒無關）；無未完成 mutation → 直接回 cache（零等待）
- 基礎設施盤點：S1 notif 分流；上游推播語義（僅 mutation 驅動；每推
  帶 version；`data: "committing-transaction"` 標記可作輔助訊號）

### 核心實作要點

- `check_file(file)` tool：副檔檢查 → `sync_open`（可能觸發 didOpen/
  didChange＝mutation）→ 判定：
  - 該檔無待收斂 mutation（sync_open no-op 且 cache 有條目）→
    **直接回 cache**（純 push 模型下無 mutation 就沒有新推播——傻等
    是 10s 假逾時的根因）
  - 有 mutation → 等雙條件收斂（version 對齊＋per-URI 靜默 ≥Q）；
    逾時（10s）→ 回 cache 現值＋`[WARN] not converged`（SM-8）
- 回應格式：`count=N`＋每條一行 `sev=N code=... line:col message`
- `.py` 過濾雙層：tool 入口副檔檢查（拒非 .py）＋回應組裝時若上游
  推播含非 .py URI 條目（防禦）跳過並計數標記
- evict 的 didClose 推播噪音（對被 evict URI 推空診斷＋觸發其餘 open
  檔重推）為已知上游行為——cache 模型天然吸收（per-URI keyed，噪音
  不影響其他檔的收斂判定）

### Pseudo Code

```
reader thread (常駐): Notification{publishDiagnostics{uri, version, diags}} ->
    diag_cache[uri] = (version, diags, now)

check_file(file):
    ensure_py(file)?
    mutation = sync_open(file)?          // true 若觸發 didOpen/didChange
    uri = file_uri(file)
    entry = diag_cache.get(uri)
    if !mutation && entry.is_some(): return format(entry)      // R-07b: 直回 cache
    deadline = now + 10s
    loop {
        (v, diags, t) = diag_cache[uri]?
        if v >= overlay[file].version && now - t >= Q: return format(diags)  // R-07c: version 雙條件
        if now >= deadline: return format(diags) + "[WARN] not converged"
        sleep(50ms)
    }
```

### 驗證策略

- integration test：strict fixture（`pyrefly.toml` preset=strict＋兩處
  bad-assignment）→ 2 條列表行；乾淨檔 → `count=0`
- **重複 check 無編輯**（R-07b 回歸釘）：check → check（不 edit）→
  第二次立即回且內容一致（非 `[WARN] not converged`）
- 多檔忙碌頻道（R-07a）：A 檔 check 收斂期間對 B 檔 didChange 洪峰
  ——A 的收斂不受 B 推播影響（per-URI keyed）
- 逾時路徑：Q 建構注入大值觸發 → `[WARN]` 標記在場
- 非 .py → error

## S4：edit-recheck tool（串流面）

### Context

UC 引用：型別面 LSP 橋（📋）。驗收三條之③（與 S5 的 ZCode entry demo
合併驗收）。

- 依賴關係：S2 overlay（edit 更新 overlay＋version）、S3 cache 收斂
- 語義約束：didChange 採 **range 省略形全量替換**
  （`contentChanges: [{text}]` 無 range 鍵）——pyrefly 明確支援
  （`None => result = text`）；range=Some 全文形需 UTF-16 端點計算，
  與「位置透傳」立場矛盾，棄用（R-11）

### 核心實作要點

- `edit_file(file, content)` tool：副檔檢查 → `sync_open` → didChange
  （range 省略形、version 遞增）→ overlay 更新（content＋version）→
  回 `"edited, {n} bytes, run check_file for diagnostics"`
- 編輯不直接回 diagnostics（收斂非同步——AI 自然兩段式：edit →
  check；避免 tool 內隱式等待疊加收斂窗與編輯檢查雙重延遲）

### Pseudo Code

```
edit_file(file, content):
    ensure_py(file)?; sync_open(file)?
    entry = overlay[file]; entry.v += 1
    session.notify("textDocument/didChange", {textDocument: {uri, version: entry.v},
        contentChanges: [{text: content}]})       // R-11: range 省略形
    overlay[file] = {content, v: entry.v, stamp: disk_stamp(file)}  // stamp 同步: 內容即磁碟將有的
```

### 驗證策略

- integration test：spike P2 劇本重現——open（2 errors）→ edit 引入
  return-type 違規 → check 收斂後含新 error 且 hover 反映新簽名
  （`-> int`）——version 雙條件保證不吃編輯前舊推（R-07c 回歸釘）

## S5：等值電池＋文檔收尾

### Context

UC 引用：型別面 LSP 橋（📋）＋更新 Rust-native occurrence producer
（pyrefly-lsp bin 文件化）＋更新 Unified MCP interface。驗收三條之①
（對照 pyright）與歸宿定案。

- 依賴關係：S2-S4 tools 全在場
- 電池歸宿定案（umbrella findings #5）：**CR repo integration test＋
  sidecar baseline fixture**（golden_corpus 模式）——pyright 輸出
  固化為 baseline 檔（更新時才需 pyright 在場），runtime 電池只跑
  pyrefly 端對拍 baseline，零 node 依賴
- 語義約束：fixture 用**顯式 annotations**（`x: int = ...`）——推論
  差異（Literal vs int）排除在等值判準外（兩引擎語義差異容許域，
  電池判定剝離；此為判準設計而非寬鬆豁免）
- **正規化判準**（R-09/R-10——單一共用規格，baseline 生成與 runtime
  電池跑同一份）：
  1. 抽 hover markdown 的**第一個 python 圍籬**內容（剝 ```python…
     ``` 外殼；兩家的 kind 前綴與 pyrefly 尾段 Go-to links／Type
     source 皆在圍籬外或後段，取首圍籬即天然剝離）
  2. 剝行首 kind 前綴 `^\([a-z ]+\)\s*`（`(variable)`/`(function)`/
     `(class)` 兩家 token 集合差異吸收）
  3. whitespace 正規化：換行摺疊為單空格＋連續空白摺疊（pyrefly 對
     參數>1 callable 強制多行，pyright 單行——格式差吸收）
  4. 正規化後**逐字相等**

### 核心實作要點

- `tests/fixtures/equivalence/`：annotation 直給的 hover 電池檔
  （variable/function/attribute/class 四型）＋baseline JSON
  （`pyright_hover_baseline.json`：position → 正規化後期望字串）＋
  **生成腳本入庫**（`gen_baseline.py`——lsp_harvest 同款 client 打
  pyright-langserver，套用同一正規化函數後固化；檔頭記錄生成命令）
- 正規化函數單一實作：Rust 端（電池測試內）＋ Python 端（生成腳本）
  各一份，規格以 EP 本段為準（兩側簡單 textual 操作，drift 面小）
- `tests/equivalence_battery.rs`：bridge hover 四型 → 正規化 → vs
  baseline 等值
- baseline 生成完成後，EP 結算段附**四型原始 payload 摘錄**（兩家各
  一行——防止正規化規格與實例脫鉤，R-10/F-14）
- diagnostics 過濾實測紀錄（S3 測試輸出摘錄）＋串流 demo 紀錄（S4
  測試輸出摘錄）進 EP 結算段
- 收尾（詳下方收尾步驟）：AGENTS.md Capabilities 三行（型別面橋 ✅
  ＋producer 行補 `pyrefly-lsp`＋Unified MCP interface 行補第二
  server）；crates/AGENTS.md 提 bridge crate；.kanban 卡 Done；plugin
  manifest 加 bridge entry——**lazy spawn**（首工具呼叫才拉 backend，
  無 backend 的消費者啟動不炸、`lsp_status` 明示 not-spawned-yet）、
  沿用 `/bin/sh` fallback 指令模式、註記 entry 在下次發版 bump 前
  inert（ZCode plugin cache 以版本為 key）——實際 ZCode 掛載驗證屬
  消費端弧（mosaic relay／user 端）

### Pseudo Code

```
tests/fixtures/equivalence/battery.py:
    count: int = 0
    def scale(v: int, k: int) -> int: return v * k
    class Box: size: int = 0
positions: [count(var), scale(func), Box.size(attr), Box(class)]
gen_baseline.py: pyright hover -> normalize -> {"count...": "count: int", "scale...": "scale v: int, k: int -> int", ...}
test: bridge.hover(each) -> normalize -> assert == baseline
```

### 驗證策略

- `tests/equivalence_battery.rs` 全綠（四型正規化等值）
- `cargo test` 全 workspace 綠（新 crate 全套；fresh checkout 按
  測試 binary 解析順序文檔前置建置）
- 安裝驗證：`cargo install --path crates/code-reality` ＋
  `--path crates/pyrefly-producer` ＋ `--path crates/code-reality-lsp-bridge`
  （三 crate 三命令，AGENTS.md 文檔寫明）後 PATH 驗證（純 PATH＋
  中性 cwd，零 checkout）

## 整合策略

- 段落順序 S1→S5 嚴格線性（每段站在前段 tools 上）
- 每段完成即 `cargo test -p code-reality-lsp-bridge`（段級）＋全
  workspace build
- S5 收尾跑全量 `cargo test`＋安裝驗證
- baseline: 6ba5714

## 收尾步驟

1. AGENTS.md Capabilities：`Python 型別面 LSP 橋` 行（入口
   `code-reality-lsp-bridge --stdio`＋tools 清單）＋producer 行補
   `pyrefly-lsp`＋Unified MCP interface 行補第二 stdio server entry
   （安裝路徑三條列明）
2. crates/AGENTS.md：bridge crate 分層描述（rmcp server／LSP client
   session／backend 參數化——P2 條款；caps 最小集不變式）
3. .kanban：追蹤卡＋能力卡 → Done/
4. plugin manifest：`plugin/.mcp.json` 補 bridge entry（lazy spawn＋
   `/bin/sh` fallback 模式＋inert-until-bump 註記；版本 bump 紀律屬
   發版弧，本 EP 不發版）
5. Scenario Matrix「消費場景」提煉進 Capabilities 備註
6. /audit-test 對新增 integration tests
7. **relay 回執交接**（追蹤卡自帶義務，R-12）：完成報告附
   commit hash＋電池證據摘要＋工具面清單（名稱＋參數形態），供
   user 貼回 ai-rules session（翻分工條文＋roadmap P1 打勾＋W2
   handoff 產生）

## 結算段（2026-08-28 build 完成）

**驗收三條證據**：

1. **hover 對照 pyright 等值** ✅——`tests/equivalence_battery.rs` 綠。
   逐字 parity 三型（variable `int`／function
   `def scale( v: int, k: int ) -> int`／attribute `int`——正規化規格：
   抽首圍籬→剝 kind 前綴→剝 `name: ` 前綴→剝 `: ...` 尾→whitespace
   摺疊，雙端同一份規格）；class 型 pyrefly 單側斷言
   （`(class) Box` kind 在場）——**兩家顯示深度本質不同**
   （pyrefly 給 constructor 簽名 `def Box() -> Box`、pyright 只給
   名字 `Box`），屬語義差非格式差，逐字吸收即過度規格化，記錄排除。
   baseline 固化 sidecar：`tests/fixtures/equivalence/
   pyright_hover_baseline.json`（生成器入庫 `gen_baseline.py`，
   pyright 僅更新 baseline 時需在場）。
2. **diagnostics `.py` 過濾** ✅——`check_non_python_rejected`＋tool
   入口副檔檢查＋回應組裝防禦層；strict fixture 實渗
   `count=2`（8:13＋9:19 兩條 bad-assignment，與 spike 觀察逐位一致）。
3. **串流＋ZCode entry 形態** ✅——`edit_then_check_reflects_new_content`
   （edit `-> str`→`-> int` → check 收斂出 bad-return 且 hover 顯示
   新簽名——version 雙條件擋住編輯前舊推）；entry 形態＝`--stdio`
   spawn＋`plugin/.mcp.json` bridge entry（inert-until-bump）＋五 bin
   PATH 驗證（中性 cwd /tmp、純 PATH、零 checkout）。實際 ZCode
   session 掛載驗證屬發版弧（工具清單新 session 才固定）。

**測試帳**：bridge crate 18 tests（17 lifecycle/tools 含三個 review 回歸
釘：CJK URI、busy-channel 分離、timeout WARN ＋1 電池）全綠——review
修正前 15、修正後 18，重裝 backend 後再驗一輪；全 workspace
`cargo test` exit 0；`cargo install --path` 三 crate exit 0。

**Build 中抓到的設計修正**（回寫知識）：
- check_file 初版「無 mutation 直回 cache」在 edit 後 serving 舊診斷
  ——收斂條件改為 version 對齊（`cache.version ≥ overlay.version`）
  ＋fresh（mutation 後）＋per-URI quiesce 三條件
- hover null 的有界重試必要（module info 未就緒＝立即 null 非延遲）
- 電池位置必須 `cat -n` 機械驗證：本 EP 三次行號 off-by-one 全部
  是人工心算翻車（spike fixture、edit 測試 count 預期、電池 parity
  位置）；pyright 對空行/邊界 hit-test 寬鬆（回鄰近符號）會掩蓋
  錯位，pyrefly 嚴格回 null——**寬鬆端的「成功」不可作為位置正確
  的證據**

**已知風險（列冊非阻斷）**：
- 10s 收斂 deadline 在重負載（14 測試進程冷啟動）下曾假逾時
  （首跑 flaky、邏輯修正後三連綠）——SM-8 場景，生產單 bridge 單
  backend 負載遠低於此
- gitignored 檔 hover 靜默 null（上游 exclude 行為，SM-7 文檔記錄）

**消費場景提煉**（自 Scenario Matrix）：hover 查型別簽名／check_file
查型別錯誤（磁碟外編輯自動同步）／edit_file 記憶體編輯後 recheck
（未落盤編輯 LRU evict 不丟）／非 .py 明確拒絕。

## Post-Build Findings（2026-08-28 dual-context 審查，judge 全採納已修）

fresh（NEEDS-FIX→修後 18/18 綠）＋primed（PASS-with-findings），
全部採納落地：

| ID | 處置 |
|----|------|
| fresh F1 🔴 URI percent-encode mismatch（CJK 檔名 check_file 全壞，POC 實證） | `file_uri` 對齊 url crate PATH encode set（非 ASCII byte 逐位編碼）＋CJK 回歸測試 `check_file_cjk_filename_converges` |
| fresh F2/primed F-01 bin shutdown 排序（serve 返回時 backend 必未 spawn） | shutdown 移到 `peer.waiting()` 之後 |
| fresh F3 handshake 失敗半初始化態（didOpen 全被丟棄的 silent 卡死） | initialize 失敗 kill child＋清 slot（下次呼叫重試） |
| fresh F4/primed F-07 測試解析鏈順序不一致 | `tests/common/mod.rs` 單一 policy（env→PATH→target/release） |
| fresh F5 language-agnostic 措辭 vs Python 硬編碼 | doc 精確化：**imports 層** agnostic（P2 條款）、**行為面**（languageId/.py gate）Python-specific until P2 |
| fresh F6 version=None 空推誤判收斂（並發 evict 競態） | `version_ok` 要求 `Some(v) ≥`（上游 open push 必帶 version） |
| fresh F7-F12 | pyrefly-lsp argv loud 拒絕＋--version；Content-Length 64MiB 上限；MCP 錯誤碼分流（ensure_py→INVALID_PARAMS 前置、其餘 INTERNAL_ERROR）；mutation instant 前移 notify 前；didClose 清 diag_cache 條目；unused import 刪 |
| primed F-02 R-07a/SM-8 兩測試缺 | `busy_channel_does_not_break_other_file_convergence`＋`check_file_timeout_path_warns` |
| primed F-03/08 | AGENTS.md Usage 三條 install＋場景句併入型別面行 |
| primed F-04/05/06/11 | EP 文檔同步（見下方結算段修訂） |
| primed F-09/10/12 | 測試名改 `hover_function_def`；hover description 補 gitignore 行為；錯誤訊息去內部編號 |
| primed F-13 | s5-ceiling-analysis.md 隨 commit 帶走（kickoff 明示）；其餘 git add 只指名檔案 |

**結算段修訂**（primed F-04/F-05/F-06/F-11 回寫）：
- `.py` 過濾第二層由 per-URI keyed lookup **結構性吸收**（R-07
  重設計後只 get 目標 URI，非 .py 條目結構性進不了回應）——原設計
  的「計數標記」機制不存在也不需要
- 正規化規格為**五步**（S5 正文三步為舊版）：抽首圍籬→剝 kind
  前綴→剝 `name: ` 前綴→剝 `: ...` 尾→whitespace 摺疊
- 四型兩家原始 payload 對照（R-10 義務）：

| 型 | pyright 原始 | pyrefly 原始 |
|---|---|---|
| variable | `(variable) count: int` | `count: int`（無 kind 前綴） |
| function | `(function) def scale(\n    v: int,\n    k: int\n) -> int` | `(function) scale: def scale(\n    v: int,\n    k: int\n) -> int: ...` |
| class | `(class) Box` | `(class) Box: def Box() -> Box: ...` |
| attribute | `(variable) size: int` | `(attribute) size: int` |

- `lsp_status` state 欄兩態（alive/dead）；not-spawned-yet 由
  `server=not-spawned-yet` 欄承載——資訊等價、欄位映射如上
