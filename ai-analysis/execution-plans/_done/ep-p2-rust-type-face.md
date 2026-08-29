# EP: P2 Rust 型別面——bridge crate 的 rust-analyzer backend

> **ep_type**: implementation
> baseline: 42c6150
> 上游：ai-rules umbrella P2 row（findings #7 gate 規格＋#3 NT 檢查點）；
> P1 EP `ep-type-face-lsp-bridge.md`（crate／審查紀錄／電池方法論）

## North star

P1 決策 2（backend 參數化＝P2 邊際成本塌縮）的兌現：`code-reality-lsp-bridge`
加入 rust-analyzer backend，補 ZCode 的 Rust hover/diagnostics 真空。
分工條文翻轉後最後一塊：U9 全語言綠。

## Spike 已驗證（2026-08-28 本 session，`.agent-tmp/lsp-spike/ra_spike.py`）

- **rust-analyzer 1.96.0 spawn 無參數**（預設 stdio 即 LSP——歷史教訓
  「不收 `--stdio`」吻合：不傳任何 flag）→ initialize 0.02s
- **hover 完整往返**：`read_message` 得 module path＋完整簽名＋trait
  實作訊息；std 方法（`trim_end_matches`）同樣完整
- **diagnostics push 模型**與 pyrefly 同形（didOpen 後推播）
- **首次 hover 冷載入 749ms~9.5s**（workspace/cargo metadata 載入），
  熱 hover 25ms——P1 的 hover 有界重試 500ms 窗對 ra 不足，須
  per-backend 參數
- position hit-test 邊界：ra 對部分位置回空（與 pyright 類似）——
  電池位置取符號中段＋`cat -n` 機械驗證（P1 三次行號事故教訓）

## 裁決已定案

1. **同 crate 語言無關形狀**——rust-analyzer 是 backend 參數不是新
   crate（P1 決策 2）；resident state 不進 code-reality-mcp（沿用）
2. **backend 選擇機制（本 EP 裁決）：per-call 副檔名自動路由**——
   `.py`→`pyrefly-lsp`、`.rs`→`rust-analyzer`。理由：單一 MCP entry
   形態保持（plugin/user-level config 零變更）；AI 呼叫端零認知
   負擔（`file` 參數必給，副檔即路由鍵）；雙 backend 各自 lazy
   spawn（只用一種語言的 session 不拉另一個進程）。內部形態：
   per-language `LspSession`（互不相干）
3. **P2 gate＝bridge 內外往返一致性＋延遲預算**（findings #7——
   oracle＝同引擎對拍：bridge hover 輸出 vs 固化的直連 ra baseline；
   跨引擎等值無獨立對象，ra 自身是 incumbent）
4. NT session 掛接檢查點（findings #3）＝消費端實證（user/NT 端
   新 session hover `crates/event_store/src/kernel.rs`）——非本
   repo CI 測試範圍，EP 結算段記錄交接

## EP Review Findings（2026-08-28 兩軸獨立審查——12 findings 全採納回寫）

| ID | 嚴重度 | 處置 |
|----|--------|------|
| S-F-1 🟡 缺並發 mixed-call 場景 | 回寫 SM-9（rs 冷載入佔其 session 鎖期間 py hover 不受影響——per-session 鎖結構證據 session.rs:63，測試補行為釘） |
| S-F-2 🟡 tool description/schema 文案 Python 硬編 | 回寫 S2：雙語言化＋收尾補 README 兩份 |
| S-F-3 🟢 spawn 錯誤指引硬編 pyrefly 安裝 | 回寫 S1：LangSpec 加 `install_hint` 欄 |
| S-F-4 🟢 bin 單 session shutdown | 回寫 S1：`Bridge::shutdown_all()` |
| S-F-5 🟢 副檔比對大小寫 | 定案：case-sensitive（與 P1 `ensure_py` 一致），pseudo 註明 |
| C-F-01 🟡 didChange fallback 不必要且自帶 F6 風險 | 回寫 S2：**刪除 fallback**（spec 明定 range 省略＝全文替換是協議義務；實測仍保留，失敗才回報） |
| C-F-02 🔴 version 放寬式未定義 | 回寫 S2：放寣式＝`version_ok = e.version.map(\|v\| v >= overlay_v).unwrap_or(fresh)`——None 僅在 `last_push > mutation_at` 時可接受（fresh 天然防 F6 過時空推）；預期 ra 實測直接帶 version（vfs 版本），放寬是後備；補 F6 回歸測試移植到 rs backend |
| C-F-03 🟡 ra hover retry 10s 餘裕不足（實測上緣 9.5s） | 回寫 S1：`hover_retry_ms` ra＝30_000（重試迴圈不連續持 interaction 鎖——無阻塞代價）；SM-4 註明窗盡＝軟失敗可重試 |
| C-F-04 🟡 detached-file 退化未記載 | 回寫 S2：root 外 .rs 的 hover 形狀（無專案依賴解析、簽名殘缺非 error）記載＋行為測試 |
| C-F-05 🟡 diagnostics 三個 ra 特性 | 回寫 S2：flycheck（cargo check）基於**磁碟**——overlay 編輯只有 ra 原生診斷（mismatched-types 等），完整 cargo 診斷集需落盤（SM-8 註明）；多波推播（語法/語義/flycheck）與 severity=4 hint 雜訊記載進延遲預算 |
| C-F-06 🟡 正規化三個不穩定源 | 回寫 S3：baseline JSON 記 `serverInfo.version`＋電池開頭斷言（不一致→skip＋loud 警示）；電池加暖場 hover（載入期 module path 行可能缺席）；明文避開 type 符號（`// size = N` 行隨架構變不可正規化） |
| C-F-07 ℹ️ server→client request 回 `[]` 對 ra 型別不匹配 | 混語言測試涵蓋；ra 容忍未答 request（spike 實證），`[]` 風險低，抱怨再改 `null` |

## UC 盤點

### Backlog 關聯
- 新建 1 張能力卡（Rust 型別面）

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過）

### 掃描範圍
- AGENTS.md Capabilities、crates/AGENTS.md、.kanban/

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Python type face via LSP bridge | ✅ | AGENTS.md | 更新 | 描述從 Python-specific 改為雙語言（副檔路由） |
| Unified MCP interface | ✅ | AGENTS.md | 無 | entry 不變（單 server 雙 backend） |

### 新增 UC

| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Rust 型別面（hover/diagnostics/edit-recheck，同 bridge tools） | 📋 | `crates/code-reality-lsp-bridge`（擴充） |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | AI hover Rust 符號型別 | `hover(file.rs, line, ch)` | markdown 簽名（ra 原文，含 module path） | 無 | Rust 型別面 |
| SM-2 | AI 查 Rust 檔錯誤 | `check_file(file.rs)` | diagnostics（ra push；同 P1 收斂機制） | 無 | Rust 型別面 |
| SM-3 | 混語言 session | 先 hover .py 再 hover .rs | 各自 backend lazy spawn；互不影響 | 無 | 雙 backend |
| SM-4 | 首次 hover 冷載入 | ra workspace 載入期間 hover | per-backend 重試窗（ra 10s）覆蓋；窗盡回 no hover | 無 | Rust 型別面 |
| SM-5 | 非 py/rs 檔 | `.txt` | 明確錯誤（副檔路由拒絕——雙語言版 SM-15） | 無 | 路由 |
| SM-6 | ra 不在 PATH | 僅裝 pyrefly 的機器 hover .rs | loud error＋安裝指引（rustup component add rust-analyzer） | 裝 ra | Rust 型別面 |
| SM-7 | ra backend 死亡 | kill ra 進程 | 該 backend dead＋loud error；**pyrefly backend 不受影響**（獨立 session） | 無 | 雙 backend |
| SM-8 | 編輯後重查（Rust） | `edit_file(file.rs)` → check | ra 原生診斷反映 overlay 編輯（**flycheck＝cargo check 基於磁碟——完整 cargo 診斷集需落盤**，C-F-05 語義限制註明） | 無 | Rust 型別面 |
| SM-9 | 並發 mixed-call | rs 冷載入（30s retry 窗）期間同時 hover .py | py 立即回（per-session 鎖——rs 的載入不佔 py 的 interaction lock） | 無 | 雙 backend |

## 段落劃分原則

S1 session 語言參數化＋路由層 → S2 tools 接線＋過濾＋測試 →
S3 ra 電池（往返一致性）＋延遲預算＋文檔收尾。每段 integration
test 打真實 backend（ra 全綠 cargo workspace 在場——測試目錄即
本 crate 自身）。

## S1：LspSession 語言參數化＋backend 路由

### Context

UC 引用：Rust 型別面（📋）。裁決 2（副檔路由）的載體。

- 依賴關係：P1 的 `LspSession` 泛化（現在 languageId="python"、
  retry 500ms 硬編碼）
- 語義約束：per-language session 完全獨立（鎖/cache/overlay/生命
  週期各自一套——ra 死不死不影響 pyrefly）；與 S2 共享路由結果
  （`BackendKind { Python, Rust }`）
- 基礎設施盤點：P1 session.rs 全套（sync_open/overlay/diag_cache
  ——機制本身語言無關，僅參數注入差異）

### 核心實作要點

- `LangSpec`：`{ language_id, extension, hover_retry_ms, install_hint }`
  ——Python `{python, py, 500, cargo install .../pyrefly-producer}`、
  Rust `{rust, rs, 30_000, rustup component add rust-analyzer}`（C-F-03）
- `LspSession::new(backend_cmd, root, quiesce_ms, lang: LangSpec)`：
  didOpen 的 languageId、ensure 副檔（case-sensitive——S-F-5 定案）、
  hover retry 窗、spawn 失敗的安裝指引全走 LangSpec（S-F-3）
- `Bridge`（server 層）：`py: Arc<LspSession>`＋`rs: Arc<LspSession>`
  （各自 lazy）＋`route(file) -> Result<(&LspSession, PathBuf), String>`
  （副檔判別；未知副檔 loud error 列出支援面）＋`shutdown_all()`（S-F-4）
- `lsp_status`：雙 backend 行（各自 cmd/server_info/open_files/state）
- bin 參數：`--rust-backend <cmd>`（預設 `rust-analyzer`——無參數
  spawn）、`--lsp-command` 保留為 Python backend 覆寫（P1 相容）；
  結尾 `shutdown_all()`

### Pseudo Code

```
LangSpec { language_id: &'static str, ext: &'static str, hover_retry_ms: u64 }
PY = LangSpec{python, py, 500}; RS = LangSpec{rust, rs, 10_000}

Bridge::route(file):
    match ext(file):
      "py" -> Ok(&self.py)
      "rs" -> Ok(&self.rs)
      other -> Err("unsupported file type .{other} — this bridge serves .py (pyrefly) and .rs (rust-analyzer)")
```

### 驗證策略

- 混語言測試（SM-3）：同 session 先 py check 後 rs hover——兩
  backend 都 lazy 起來且行為正確
- SM-7：kill ra（backend_pid 測試 hook）→ rs 工具 loud error＋
  **py hover 仍正常**（獨立性釘）
- `.txt` 路由拒絕（SM-5）

## S2：tools 接線＋didChange 相容驗證

### Context

- 語義約束：四 tools 的 ensure/路由統一走 `Bridge::route`；
  check_file 收斂機制語言無關直接沿用（per-URI cache＋version
  雙條件——ra 推播帶 version 待實測，不帶則 version_ok 的 Some
  要求要放寬為 per-backend 行為）
- 基礎設施盤點：P1 server.rs 的 impl 函式簽名帶 `&LspSession`——
  改為帶 `&Bridge`（route 後傳 session）

### 核心實作要點

- tools 進 route；**tool description/schema 文案雙語言化**（S-F-2：
  "Hover a Python symbol"→"Hover a Python or Rust symbol" 等）
- **didChange range 省略形**：spec 明定 range 眡略＝全文替換是協議
  義務（INCREMENTAL 只約束「有 range 時」），無 fallback 路徑
  （C-F-01——fallback 自帶 F6 形態風險）；integration test 實測
  edit→check 反映新內容即證，實測失敗才回報
- **version 收斂**：預期 ra 對 client didOpen 過的檔案推播帶 version
  （vfs 版本）；後備放寬式＝`unwrap_or(fresh)`（None 僅在
  `last_push > mutation_at` 時接受——C-F-02，fresh 天然防 F6 過時
  空推）；P1 的 F6 回歸測試移植到 rs backend
- diagnostics 格式：`format_diags` 沿用（ra 的 code 缺席/flycheck
  E0xxx 均相容）；**flycheck 基於磁碟**的語義限制＋多波推播＋
  severity=4 hint 雜訊記載（C-F-05，SM-8 註明）；detached-file
  退化（root 外 .rs——hover 可用但無專案依賴解析）行為測試
  （C-F-04）

### 驗證策略

- rs hover 三型（fn/type/method——用本 crate framing.rs 固定符號）
- rs edit→check（SM-8）：注入型別錯誤（如把 `usize` 用成字串
  context）→ check 收斂出新錯——range 省略形相容性一併證
- rs 非 cargo 環境退化：單檔無 Cargo.toml 目錄——ra 仍服務
  語法級 hover（行為記錄，不阻塞）

## S3：ra 電池（往返一致性）＋延遲預算＋收尾

### Context

- **P2 gate（findings #7）**：bridge 內外往返一致性＝bridge hover
  對同符號輸出與直連 ra client 一致——測試形態：固化 ra baseline
  （sidecar JSON，生成器入庫——同 P1 電池模式；更新時需 ra 在場）
  ＋runtime bridge hover 對拍（正規化：ra hover 的 ```rust 圍籬＋
  module path 行＋簽名——**ra→ra 同引擎，正規化只需剝格式雜訊**
  〔前導換行、trait 附註段〕，無跨引擎 kind 前綴問題）
- 延遲預算記錄（EP 結算段）：initialize／冷首 hover／熱 hover／
  check 收斂典型值（本 crate workspace 實測）

### 核心實作要點

- `tests/fixtures/ra_equivalence/`：baseline 生成器（直連 ra 抓
  hover）＋固化 JSON（framing.rs 三符號：`write_message` fn、
  `read_message` fn、`len` local——**刻意不含 type 符號**：`// size = N`
  行隨目標架構變不可正規化，C-F-06）；baseline 記錄
  `serverInfo.version`，電池開頭斷言（不一致→skip＋loud 警示）
- `tests/ra_equivalence_battery.rs`：**暖場 hover**（丟棄結果，
  等 workspace 載入完成——載入期 module path 行可能缺席）→
  bridge hover → 正規化（剝前導空白＋取首 ```rust 圍籬）→ vs
  baseline
- 收尾：AGENTS.md 型別面行改雙語言描述＋Capabilities 新 Rust 行
  （或併入同一行——雙語言一行）＋crates/AGENTS.md 補路由層＋
  kanban Done＋**ai-rules 翻轉交接**（Rust 型別面→bridge——
  relay 回執隨收尾報告）

### 驗證策略

- 電池綠（三符號往返一致）
- `cargo test` 全 workspace 綠＋`cargo install --path` bridge
  crate 重裝＋PATH 驗證（中性 cwd）
- 延遲預算數字記錄進結算段（NT 大 repo 的數字屬消費端實證——
  user/NT session 補）

## 結算段（2026-08-28 build 完成）

**Build 中實測推翻審查判斷的記錄**（spec 知識 ≠ backend 實作現實）：

1. **C-F-01 被推翻**：審查依 LSP spec 認定 range 省略形＝協議義務、
   fallback 不必要——**ra 實渗零反應**（didChange 後 15s 零推播）；
   range 形（start {0,0}→end {old_lines,0}）正常。已改 range 形
   全量替換（end span **舊內容**——首版 end 用新文行數是語義錯誤
   只替換前 N 行）。pyrefly 對 range 形相容（py 測試群全綠）
2. **ra 靜默丟棄 didChange**（hover 暖場後立即 change）：check 收斂
   迴圈加**停滯重發**（deadline 過半且 cache version 落後→
   didClose+didOpen overlay 內容恢復）
3. **ra 暫態 `-32801 content modified`**（hover 撞 file-change 處理）
   ——retry 視同暫態 null
4. **正規化取首圍籬不夠**：ra hover＝module path 圍籬＋簽名圍籬——
   改**合併所有 ```rust 圍籬**（首圍籬無符號區別力）
5. **check deadline per-backend**：ra 波次（syntax/semantic/flycheck）
   ＋並行測試負載——py 20s／rs 30s（`slow_timeout_ms`）

**P2 gate 證據**：
- **內外往返一致性** ✅——`ra_equivalence_battery` 綠：bridge hover
  vs 直連 ra baseline（兩符號：module path＋完整簽名＋where 子句
  逐字等值；版本釘：live rust-analyzer ≠ frozen 版本→loud skip）
- **延遲預算**（本 workspace 實測，測試輸出）：initialize 0.02s；
  冷首 hover 749ms~9.5s（workspace 載入）；熱 hover 25ms~1ms；
  check 收斂秒級（didOpen 波）/停滯重發場景 +半 deadline。NT 大
  repo 數字屬消費端實證
- **SM-7 後端獨立性** ✅——kill ra → rs loud error、py hover 不受
  影響（測試釘）

**測試帳**：bridge crate 22 tests（bridge 15＋py 電池 1＋rust_backend
6＋ra 電池 1）。

**Post-Build Findings（2026-08-28 fresh review，9 findings：8 採納
1 回滾）**：F1 並發 mixed-call 測試補上（thread::spawn：rs 冷載入
期間 py hover 計時通過）；F2 `LangSpec.extension` 接線進路由（原
dead field）；F3 timeout 測試注入 2s deadline（原燒生產 20s）；
F7 空 old 內容 end {0,0}；F9 `BRIDGE_STRICT_BATTERY=1` 把版本 drift
skip 轉 fail；F6 空 hover 重試行為記載（下方已知限制）。**F4
回滾**：收緊 reissue 於 `overlay_version > 1` 破壞 lru 場景（evict
re-open 歸 version 1 也需要恢復路徑——pyrefly 洪峰後同樣會丟
re-open 推播，60s 實測證實）——spurious reissue 代價（一次多餘
close+re-open 且仍收斂）小於必要性，取捨記錄於 server.rs 註解。
F5（force_reopen 併發原子性）不採納：單 MCP client 實務＋審查自
標 inferred，多 client 時再議。EP spec 段 drift 同步（兩符號非三、
ra retry 30s、LangSpec 含 slow_timeout_ms）。

**Post-Build 補充：rust-analyzer 上游 panic 觀察**——workspace 全量
測試（數十 LS 進程並行）壓力下 ra 自身偶發 panic（reload.rs
SendError），屬上游資源壓力不穩定非 bridge 因素；單 crate 測試
（23 tests）穩定全綠。

**工具面清單（不變）**：`lsp_status`／`hover`／`check_file`／
`edit_file`——schema 文案雙語言化（"Python or Rust"）；bin 參數
`--lsp-command`（Python backend 覆寫）＋`--rust-backend`（預設
`rust-analyzer` 無參數 spawn）。

**已知限制（列冊）**：
- Rust flycheck（cargo check）基於磁碟——overlay 編輯只有 ra 原生
  診斷（SM-8 語義限制，tool description 已註明）
- detached-file 退化（root 外 .rs）：hover 可用但無專案依賴解析
  （C-F-04 記載）
- ra 版本漂移（rustup 自動升級）→ 電池 loud skip＋baseline 重生

**NT 掛接檢查點（消費端實證，user/NT session）**：新 session 掛
bridge（plugin entry 或手動 config）→ hover
`crates/event_store/src/kernel.rs` 任一符號＋`check_file` 該檔——
大 repo 冷載入延遲與 hover 品質的實地數字回填本段。

## 收尾步驟

1. AGENTS.md：型別面行雙語言化（tools 清單＋路由說明＋ra 安裝
   前提）
2. crates/AGENTS.md：LangSpec/路由層描述
3. .kanban 卡 Done
4. /audit-test 新增測試
5. relay 回執：commit hash＋往返一致性證據＋延遲預算＋backend
   路由定案＋tools 清單——供 ai-rules 翻「Rust 型別面→bridge」
   ＋roadmap P2 打勾＋產 W2 handoff（kickoff prompt user 已持有）
