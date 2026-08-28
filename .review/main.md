# Review: ep-w3-import-legacy-retirement (uncommitted diff, dual-context)

> 2026-08-28 post-build 階段 1-3。fresh（code-reviewer）＋primed（code-reviewer-primed）雙審查者。
> 模式：uncommitted diff（baseline = HEAD `ef58b61`）。
> （前弧 ep-type-face-lsp-bridge 的 review 記錄已隨該 EP commit 帶走）

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
