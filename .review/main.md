# Review: ep-w3-import-legacy-retirement (uncommitted diff, dual-context)

> 2026-08-28 post-build 階段 1-3。fresh（code-reviewer）＋primed（code-reviewer-primed）雙審查者。
> 模式：uncommitted diff（baseline = HEAD `ef58b61`）。
> （前弧 ep-type-face-lsp-bridge 的 review 記錄已隨該 EP commit 帶走）
> W3 已 commit `3980fe1`。以下為同日 fix relay（ep-sidecar-invalidation）記錄。

## Fix relay（sidecar 失效缺口）審查處置

雙審查者（fresh Approved 5🟢 / primed PASS-with-findings 3 項）。Judge 裁決與落地：

| 來源 | ID | 問題 | 決策 | 狀態 |
|----|--------|------|------|------|
| fresh R1 | 🟢 | remove_file TOCTOU：並發移同檔 NotFound 變 hard error（index 已寫好的事實被吞） | ✅ 採納：NotFound＝目標狀態容忍；真失敗訊息附「index 已寫入」 | fixed |
| fresh R3 | 🟢 | 失效清單漏第三 sibling `index.scip.fndefs.db`（讀時有 guard、非 silent，屬對稱性） | ✅ 採納：`fndefs::fndefs_path` 入清單（測試斷言 3 檔） | fixed |
| fresh R2 | 🟢 | gate 對 placeholder stat 失敗 hard error | ❌ 維持 fail-loud：harvest 恆寫 placeholder、缺席＝異常（`stale_reason` stat-fail=loud 先例）；記 EP | documented |
| fresh R4 | 🟢 | 嚴格 `<` 在粗粒度 mtime 上漏抓（相等 pass） | 記 EP residual（APFS ns 粒度近理論值；雙邊契約本取 `>=`） | documented |
| fresh R5 | 🟢 | unchanged repo 重跑 emit 仍失效有效 cache（重建成本） | ❌ 不採（digest-keyed skip 複雜度>收益）；記 EP | documented |
| primed F1 | Low | `document_symbols_at` 無條件信 sidecar（outline 面 silent 過期） | 記 EP 已知過渡邊界（不建 db、producer fix 阻斷未來形成） | documented |
| primed F2 | Info | .review 記 lsp trusted-path 測試 4 件→實為 5 件 | 本表修正 | fixed |

覆蓋面自檢（沿用）：正路徑＝既有 lsp trusted-path 測試（5 件）＋producer 全量 end_to_end 未動全過；`--out` probe path keying 零互踩（fresh 驗證）；半失效狀態收斂 loud-or-safe（fresh 軸向 1）。



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
