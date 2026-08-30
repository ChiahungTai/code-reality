# EP: rust_backend 測試 re-tier——驗收級測試退出 always-on gate

> **ep_type**: implementation
> **baseline**: `f99f0f09dd219b26453133953c5f1f7e0ea8b120`（v0.6.2）
> **revision**: v1（2026-08-30）

## 實作總覽

**問題**（2026-08-30 診斷定案；數字經 review F2/F7 修正）：`tests/rust_backend.rs` 六個測試中**四個**（hover/edit/mixed/latency）各自 spawn 真 rust-analyzer 冷載整個 CR workspace（route 的 `session_for` 只回預建 session、backend lazy 不 spawn——`server.rs:72-86`＋`session.rs:170-186` 實證）；cargo test harness 預設**並行跑同 binary 內所有測試** → binary 內並行 ra 上限 4 顆（pgrep 觀察值 6-8 顆，含風熄期重疊——觀察非結構事實）。`rust_edit_then_check_native_diagnostics` 的收斂 deadline 固定 30s（`session.rs:97 slow_timeout_ms=30_000`），在此自殘競爭下餘裕貼邊（隔離總時長 30-37s）——偶爾越界＝STATE.md 既有追蹤的「負載餓死 flake」（當日全量跑現身一次：`cache=(None,0)`、`overlay_ver=1`＝reopen 已走、分析推送從未到達；隔離重跑綠）。**cargo test 對 test binary 是序列執行**（失敗輪 39/53 suite 後 fail-fast）——競爭者就是套件自己，全量只是改變了 page cache／記憶體狀態的抽籤。

**根因層級**：驗收／結算級測試被當 **always-on regression gate**——bridge 是低頻變更的穩定面（server.rs 自毒化修復後未動），不動它時這些測試零資訊，卻每次無關變更（如 tour guard）都付 30-40s＋flake 風險。

### 凍結裁決（2026-08-30 對話，勿重辯）

1. **re-tier（分級）**，非序列化止痛（static Mutex）、非拉 deadline（對不可控輸入釘時間＝環境耦合設計債）。
2. **刪兩顆**：
   - `rust_latency_budget_numbers`（`:196`）——P2 EP 結算 artifact：bounds 寬到實際不會紅（cold<60s）、`println!` 被 capture 平常不可見、結算目的已消費。
   - `rust_hover_function_signatures`（`:58`）——**`ra_equivalence_battery.rs` 超集覆蓋**（實證：同 `src/framing.rs`、同位置族含 `(11,10)`、frozen JSON baseline 比對＋ra 版本釘＋drift-skip；battery 更嚴格）。
3. **`#[ignore]` 兩顆**（觸發式：動 `server.rs`/`session.rs` 時跑 `cargo test -p code-reality-lsp-bridge -- --ignored`）：
   - `rust_edit_then_check_native_diagnostics`（SM-8——毒化快取弧證明會真壞的收斂門）；
   - `mixed_language_sessions_are_independent`（SM-9 互動鎖）。
4. **always-on 留兩顆**：`route_by_extension`（MCP 副檔路由契約——**不 spawn 任何 backend**，F2 實證）、`rust_backend_death_leaves_python_alive`（crash-only 真路徑）。always-on 並行 ra **6-8 顆 → 1 顆**（僅 death；另 1 顆 pyrefly）。
5. **非目標**：不 de-spawn 化 route（over-engineering）、不動 `ra_equivalence_battery`（已有 drift-skip 自律）、不動任何產品碼（server/session/framing 零改）、**不出 release**（test-only＋docs 變更，wheel 內容不變——隨下次自然 release 帶上）。

## UC 盤點

### 掃描範圍
`AGENTS.md:96`（Type face via LSP bridge 行，測試面敘述）、`crates/AGENTS.md` bridge 段、`.kanban/`（Backlog 空）。

### Backlog 關聯
新建 1 張 EP 追蹤卡 `.kanban/Backlog/rust-backend-test-retier.md`。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Type face via LSP bridge | ✅ | AGENTS.md:96 | 更新 | 測試面敘述補 re-tier 觸發紀律（always-on 兩顆＋ignored 觸發指令） |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | 對應能力 |
|---|------|------|---------|---------|
| SM-1 | default gate 跑 | `cargo test -p code-reality-lsp-bridge --test rust_backend` | **2 passed＋2 ignored**（route+death 跑、edit+mixed 跳）；總時長顯著下降（30-37s → 預期 ≤15s）；**並行 ra ≤1 顆**（僅 death；pgrep 驗證——F2 修正後的上界） | bridge 測試面 |
| SM-2 | 觸發式跑 | `-- --ignored` | 重測試兩顆在低競爭下綠 | 收斂門＋互動鎖 |
| SM-3 | 全量 workspace | `cargo test` | 綠；flake 存在條件（6-8 顆並行）移除 | — |
| SM-4 | bridge 變更時 | 動 server.rs/session.rs 的 EP/commit | 開發者依 crates/AGENTS.md 指引跑 `--ignored`（文檔接線） | 收斂門覆蓋時機 |
| SM-5 | ra 版本漂移 | PATH 的 rust-analyzer 換版 | battery skip-on-drift 生效——**drift 期間 default gate 無 ra-hover 冒煙**（F5 如實記錄：可接受，drift 本身已是需人為重生成 baseline 的狀態） | battery |

## 段落 S1：測試檔 re-tier

### Context
- **UC 引用**：更新「Type face via LSP bridge」測試面。
- **依賴錨點**：六測試位置（`:42 route`／`:58 hover`／`:70 edit`／`:110 mixed`／`:144 death`／`:196 latency`）；battery 超集錨（`tests/ra_equivalence_battery.rs:73` framing.rs、`:79` (11,10)、frozen baseline 比對）。
- **語義約束**：`#[ignore]` 帶理由字串（觸發條件寫在 attribute 裡——`cargo test -- --ignored` 或 `--include-ignored` 時可見）。
- **成功標準**：SM-1/2/3 全綠＋pgraft ≤2 實證。

### Invariant Impact
無（純測試面重組，產品碼零改動）。

### 核心實作要點
1. 刪 `rust_hover_function_signatures` 與 `rust_latency_budget_numbers` 兩個 `#[test]` 區塊；`WRITE_MSG` 常數**無條件隨 latency 刪除**（全檔唯一使用點＝latency `:203`——F3 實證；hover 用的是 `READ_MSG`）；`rust_session()` 仍被 edit 使用，保留。
2. `rust_edit_then_check_native_diagnostics` 與 `mixed_language_sessions_are_independent` 加：
   ```rust
   #[ignore = "heavy real-ra cold load ×2 self-contention flaked the 30s convergence deadline — run on server.rs/session.rs changes: cargo test -p code-reality-lsp-bridge -- --ignored"]
   ```
3. 檔頭 doc comment 更新：always-on＝route+death（便宜、真契約）；ignored 兩顆＋觸發指令＋flake 診斷一句（指向 STATE.md 弧）。

### Pseudo Code
```
tests/rust_backend.rs:
  - delete: fn rust_hover_function_signatures（battery 超集）
  - delete: fn rust_latency_budget_numbers（結算 artifact 已消費）
  - annotate: #[ignore = "<觸發條件>"] on rust_edit_then_check_native_diagnostics
  - annotate: #[ignore = "<觸發條件>"] on mixed_language_sessions_are_independent
  - header doc: always-on 面 + ignored 觸發紀律
  （route / death 不動）
```

### 驗證策略
- `cargo test -p code-reality-lsp-bridge`：4 passed、2 ignored、**計時對比**（記錄 before/after）。
- 跑期間 `pgrep -f rust-analyzer | wc -l` 採樣 ≤2（flake 條件移除的機械證據）。
- `cargo test -p code-reality-lsp-bridge -- --ignored`：2 passed。
- 全量 `cargo test`：綠。

## 段落 S2：文檔＋STATE＋結算

### Context
- **依賴錨點**：`crates/AGENTS.md` bridge 段（測試面敘述）；root `AGENTS.md:96`（capabilities 行測試面子句）；`STATE.md` 追蹤項行。

### 核心實作要點
1. `crates/AGENTS.md` bridge 段補一行：rust_backend 測試面 re-tier——always-on 只跑 route+death；`--ignored`（edit→check convergence＋mixed independence）於動 server.rs/session.rs 時跑（2026-08-30 flake 診斷：6-8 顆並行 ra 冷載×30s deadline 的自殘競爭）。
2. `AGENTS.md:96` 行內測試面子句補 `rust_backend.rs` 分級一句。
3. `STATE.md`：「rust_backend starvation-gate（負載餓死 flake）」追蹤項解銷（resolved-by-retier：根因＝驗收級測試在 always-on gate 自殘競爭；re-tier 移除存在條件）；**順帶處理同行既有「`rust_backend.rs:112` unused var 預存項」**——刪 hover 區塊後行號漂移，實作時查證所指（候選：mixed 的 `_dir`）並解銷或更新錨點（F4）。

### 驗證策略
- rg 對帳：`rg -n 'starvation|rust_backend' crates/AGENTS.md AGENTS.md STATE.md` 語義一致。
- 全量綠即收。

## Review Record

> 審查者：獨立 Explore agent（fresh eyes，2026-08-30，逐項實證非採信 EP 敘述）。judge：主 session。**7/7 全採納**（全為事實修正，方案本體經實證站得住）。

| ID | 嚴重度 | 摘要 | 處置 |
|----|--------|------|------|
| F1 | 🟡 | EP 全文引 `AGENTS.md:91`，實際「Type face via LSP bridge」行在 **:96**（:91 是 graph-engine 行）——照字面會改錯表格行 | ✅ 全文改 :96 |
| F2 | 🟡 | spawn 數字不符：route 的 `session_for` 不 spawn（`server.rs:72-86` 回預建 session、`session.rs:170-186` backend lazy）；真 spawn ra 的是**四顆**非五顆；always-on 並行 ra＝**1 顆**（僅 death）非 2 顆 | ✅ 問題段/裁決 4/SM-1 修正（結論更好） |
| F3 | 🟡 | `WRITE_MSG` 僅 latency `:203` 使用（hover 用 READ_MSG）——改無條件隨 latency 刪除，條件句事實錯誤 | ✅ S1 要點 1 改寫 |
| F4 | 🟡 | STATE.md:17 既有「`rust_backend.rs:112` unused var 預存項」錨點將因刪行漂移——S2 順帶查證解銷或更新 | ✅ S2 要點 3 補 |
| F5 | 🟡 | battery 超集在版本釘住前提成立（斷言面經 frozen 精確比對藴含）；drift skip 期間 default gate 無 ra-hover 冒煙——如實記錄（可接受：drift 本身需人為重生成 baseline） | ✅ SM-5 補註 |
| F6 | 🟢 | SM-1「4 passed＋2 ignored」scope 錯——package 全跑遠多於此；正確數＝`--test rust_backend` 單 binary **2 passed＋2 ignored** | ✅ SM-1/S1 驗證改單 binary scope |
| F7 | 🟢 | 「6-8 顆同時」為 pgrep 觀察值（含風熄重疊）非結構事實——binary 內上限 4；「cargo test binary 序列」敘述正確 | ✅ 問題段措辭軟化 |

查證無 finding 的軸：刪除外部引用面乾淨（rg 全 repo 含 CI 無引用；唯一 workflow 無 cargo test）；`#[ignore = "reason"]` 機制正確（toolchain 1.96.0；crate 現有 ignore 數零→`--ignored` 恰跑新增 2 顆）；六測試/battery/session 錨點全命中；crates/AGENTS.md bridge 段現無測試面字樣（S2 為純新增）。

判定：**GO-WITH-CONDITIONS → 條件全數回寫後 GO**。

---

## 整合策略

- **baseline**: `f99f0f09dd219b26453133953c5f1f7e0ea8b120`
- **commit**（user 確認後）：單一 commit（測試＋文檔＋STATE＋EP 歸檔一起——量級小）。
- **無出貨**：wheel 內容不變（test-only）；版號面不動。
- **回退**：單 commit revert。

## 收尾步驟

1. AGENTS.md:96 行更新＋kanban 卡 → Done＋EP 歸檔 `_done/`。
2. SYSTEM-MAP 不存在，跳過。
3. /audit-test：以驗證策略的機械證據（計時對比＋pgrep 採樣＋ignored 跑）取代獨立執行（簡化記錄）。

---

## Build Record（2026-08-30 implement）

- **SM-1**：`--test rust_backend` 2 passed＋2 ignored、exit 0、零 warnings（WRITE_MSG 刪除乾淨）；**pgrep 並行 ra 峰值＝1 顆**（death 的）——F2 上界精準命中。
- **SM-2**：`-- --ignored` 2 passed（24.03s，2-way 並行綠）。
- **SM-3**：全量 cargo test **53 suites 0 failed exit 0**。
- **誠實修正——EP 預測錯誤**：SM-1「總時長顯著下降（≤15s）」不成立——實測 30.30s，floor 由**單顆** ra 冷載本 workspace（~25-30s）決定；本 EP 的改善面是**並行度（4-6 並行→1）與 flake 存在條件**，不是時長。要壓時長得另案（共享 session fixture——非目標）。
- 文檔三處落地（STATE.md 三項解銷、crates/AGENTS.md tiering bullet、AGENTS.md:96 行）；`:112` 預存項結案（`_dir` 底線前綴綁定＝刻意非警告）。
- post-build 縮編：build diff 審查併入 EP review（7 findings 已對實際檔案逐項實證；test-only＋docs、零產品碼變更）——如實記載的降級。
