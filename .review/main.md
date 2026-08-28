# Review: ep-binary-freshness-face (uncommitted diff)

> 2026-08-28。EP v1 雙軸審查 NEEDS-REWORK（結構 10＋技術 11 findings）
> → v2 全數採納重寫（Review Record 在 EP 文末）→ 實作後 code-diff
> fresh 審查（背景，收斂後回填本檔）。
> 前弧記錄：W3 `3980fe1`、fix relay `2442692`（review 記錄隨各自
> commit 帶走，全文見 git history）。

## EP v1→v2 judge 摘要（關鍵採納）

- 🔴 rerun-if-changed 只指 `.git/HEAD`：同 branch commit 只動 branch
  ref——改三檔 watch（HEAD＋resolved loose ref＋packed-refs）；POC 改
  real-commit/ref-touch 形態（v1 的 touch-HEAD POC 是自驗盲區）
- 🔴 OnceLock guard 無效寫法→第一行 `if WARNED.set(()).is_err()`
- 錨點重定位：umbrella route() 既有 `--version` arm 拆分（cli.rs 是
  per-tool 層）；WARN 接四 bin main 早段（15 子命令＋MCP 面）
- 動機缺口：uncommitted-edit 窗口加第二訊號（SM-8）
- ripgrep 歸屬更正（實為 rev-parse --short=10）；prefix 比對對
  abbrev 漂移免疫；freshness.rs 獨立模組（common.rs 是 parity 契約）

## 實作驗證（live）

- S1 POC：touch branch ref→build script 重跑＋crate 全鏈重編
- SM-1 四 bin `0.1.0+2442692-dirty`；SM-2 假 checkout 三 bin WARN
  （per-crate 重裝路徑正確）；SM-3 `CR_REPO=/nonexistent` 零輸出；
  SM-7 mcp bin 啟動 WARN；SM-8 may-lag 訊號（動機場景閉環）
- byte-determinism：mosaic `--out` 隔離重產＝BYTE-IDENTICAL（rev 不進 index 產出）
- hook dry-run（對 fix relay commit diff）：code-reality＋producer
  背景安裝收斂；**安裝面 binary 自帶新版本面**（自我託管）
- R1 修復 probe：HOOK-DEAD 場景（Cargo.lock＋lowercase 路徑）→三 crate 全觸發

## Code-diff fresh 審查（NEEDS-FIX→judge 全收斂）

| ID | 嚴重度 | 問題 | 決策 | 狀態 |
|----|--------|------|------|------|
| R1 | 🔴 | hook `*Cargo.toml` POSIX case 端錨定——root manifest 與 lowercase 路徑同 commit 時永不 match（審查者機械重現 HOOK-DEAD） | ✅ 重寫 per-line `while read` 匹配；probe 重現原場景→三 crate 全觸發 | fixed |
| R2 | 🟡 | `~/.mosaic` 缺席時 redirection 失敗→`cargo install` 靜默不跑 | ✅ `mkdir -p` 前置 | fixed |
| R3 | 🟡 | linked worktree 的 branch ref/packed-refs 在 common dir，`--git-dir` 解不到→rerun trigger 掉兩個 | ✅ `--path-format=absolute --git-common-dir` 解析（fallback gitdir）；主 checkout 輸出驗證 HEAD+refs 齊 | fixed |
| R4 | 🟡 | README（Quickstart 受眾）零 hook 註記，EP S4 指名項靜默掉單 | ✅ Quickstart 補 freshness 段（opt-in＋路徑自改警示） | fixed |
| R5 | 🟡 | bridge/pyrefly-lsp version 面＋WARN 發射路徑零測試 | ✅ 補 `umbrella_warns_when_checkout_head_differs`（temp git fixture 斷言 WARN）＋bridge `version_face`＋`pyrefly_lsp_version_face` | fixed |
| R6 | 🟢 | bridge local copy 漏抄 crates-dir guard（非 CR repo 的 CR_REPO 會誤 WARN） | ✅ 補 `crates/code-reality-lsp-bridge` is_dir guard | fixed |
| R7 | 🟢 | 文檔「every bin warns」過廣（pyrefly-lsp by design 不 WARN） | ✅ root AGENTS.md 限縮四 WARN-wired bins＋pyrefly-lsp 面形註記 | fixed |
| R8 | 🟢 | version 斷言不容 git-less pkg-only 形態 | ✅ 三測試改雙形態（pkg 或 pkg+rev） | fixed |
| R9 | 🟢 | 連發 commit 背景安裝互撞／case 重複觸發 | 記錄（cargo target-dir lock 序列化＋inst 冪等，無害） | documented |

審查者驗證通報：EP v2 條款逐項落碼（packed-refs 條件 emit、guard 第一行、prefix 比對、SM-3 零 spawn、mcp/bridge warn 無 await 競態）；`describe --exclude=*` hash-only 保證以 annotated-tag scratch probe 機械成立。

## 最終測試狀態

- freshness 3（含 WARN fixture）＋end_to_end 6（含兩 version 面）＋bridge version_face 1 全綠
- 全量 workspace：綠＋1 件 `rust_edit_then_check_native_diagnostics` 全量並行 flake（rust-analyzer worker teardown SendError；隔離 21.8s 過）——未觸及 crate，非回歸





## Judge 決策與處置

| 來源 | ID | 嚴重度 | 問題 | 決策 | 狀態 |
|----|--------|------|------|------|------|
| 雙側一致 | F-01/F1 | 🟡 | `graph_engine.rs:51` open 錯誤仍指引「舊庫在場再加 import_legacy」——live code、graph_query CLI＋12 MCP 工具的入口面 | ✅ 採納（user 初判「不重要」，經查證 push back：MCP graph_query 家族正是此錯誤的消費者） | fixed |
| 雙側一致 | F-02/F5 | 🟡🟢 | 退役 banner／`retired:true` 零測試釘住 | ✅ 採納：新增 `import_legacy_cli_face_carries_retirement_banner`（text skip ＋ json skip 兩態斷言） | fixed |
| primed F2 | 🟡 | EP 驗收證據來自編輯前 binary | 部分成立：WARN/chain_tour spot-check 已用重裝後 binary 重驗（build 語義未動，純 db 有效）；EP 已記錄此限制 | documented（EP Note 段） |
| 雙側 | F-04/F3 | ℹ️ | s5_chain_tour 測試名與新語義相反；HELP 跨行 literal 脆弱 | ✅ 採納：改名 `..._no_import_guidance`；HELP 改單行顯式 `\n` | fixed |
| 雙側 | F-05/F6 | ℹ️ | `.review/main.md` 前弧記錄混在 working tree | 本檔即處置：改寫為 W3 記錄隨 commit 帶走 | fixed |
| fresh 逐軸 | — | — | consumer_db 5 呼叫端僅消費二元組；`connect_ro`/`graph_db_path` 非 dead code；docs 全淨；四輸出路 banner 一致 | 查證通過，無動作 | verified |

## 機械驗證

- `cargo test -p code-reality --test graph_db --test s5_chain_tour`：16＋13 全綠
- `cargo test --workspace`：綠（唯 `lru_evict_preserves_overlay_edits` 全量並行下 61s deadline flaky，單獨重跑 0.48s 過——lsp-bridge crate 未觸及，非回歸）
- `rg import_legacy crates/code-reality/src/graph_engine.rs` → 0 hits
- 最終 binary（含全部修正）重裝後雙面 smoke：WARN banner ✓、`graph_query hub` 於 mosaic 純 db ✓
- Primed claim 核對：legacy 邊種加總 136,218 ✓；節點 14,517+12,410=26,927 ✓；邊 27,410+136,218=163,628 ✓
