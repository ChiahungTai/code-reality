# STATE.md — Last session 觀察（pyrefly＋occurrence 雙弧收斂，2026-08-28）

## 卡點／轉向觀察

- **S5 覆蓋率 72.3% 的歸因結構**：lexical（treesitter-legacy）vs
  semantic（pyrefly）的 CALLS 差不是均質缺口——673 個 callee 是
  「字面看見、型別解析不至 corpus def」的固有類，404 個是歸屬面
  （unresolved/item-level/unchained）。**下次要拉覆蓋率時先攻
  404 類**（item-level 1,996 筆 module-level call 略過是最大單一
  可修項），不要把 673 類當 bug 修。
- **F2 機制教訓**：EP 原設計「side table 承接 call-role」建立在
  index 端攜帶 call 資訊的前提上——實測 scip-python 完全不標
  （146,867 筆 non-def 全無標記、SCIP proto 無 call bit）。**資料
  面設計前先驗證上游真的攜帶該資訊**。

## 下次起手點

0. **型別面 EP（已裁決 B，待開）**：橋官方 `pyrefly lsp`（獨立
   LS 進程＋薄 MCP 橋），host 在 code-reality repo 新小 crate。
   先薄 spike（spawn pyrefly lsp → didOpen → hover 往返）。驗收
   三條在 mosaic relay（hover 對照 pyright／diagnostics .py 過濾
   〔SM-15 教訓〕／串流＋ZCode entry 形態）。mosaic 端 lsp_mcp
   已全刪（user 裁決）——ZCode hover/diagnostics 真空中，此 EP
   是補口。
1. **import_legacy 過渡邊**：退場門檻已記錄（resolved-legacy
   ≥90% 且 missing 全歸因語義固有類）——現 72.3% 未達；拉法見
   上「404 類」。
2. **pyrefly-index bin 尚未 cargo install**（~/.cargo/bin 無）——
   下次裝機時 `cargo install --path crates/pyrefly-producer`。
3. ai-rules 端 SKILL.md 改寫＋drift 同步 5 處完成，**ai-rules
   commit 在本地等 user 授權**（那邊的紅線）。
