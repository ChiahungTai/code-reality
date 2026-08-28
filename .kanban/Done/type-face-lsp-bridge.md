# 能力卡：Python 型別面 LSP 橋（hover/diagnostics/edit-recheck MCP tools）

[tag:code-reality] [capability]

## 目標
`code-reality-lsp-bridge --stdio [--lsp-command <cmd>]`：獨立 MCP server，
spawn `pyrefly-lsp`（producer crate 的 pinned-rev LS bin）子進程，薄
LSP↔MCP 轉譯——tools：`lsp_status`／`hover(file,line,ch)`（代客
didOpen）／`check_file(file)`（diagnostics latest-wins 收斂＋.py 過濾）／
`edit_file(file,content)`（didChange 全量＋recheck 串流面）。backend
參數化（`--lsp-command`）＝P2 Rust 型別面（rust-analyzer）邊際成本塌縮。

驗收三條（凍結）：hover 對照 pyright 等值（sidecar baseline）／
diagnostics .py 過濾（SM-15）／串流＋ZCode entry（`--stdio` spawn 形態）。

## 相關
- EP：`ai-analysis/execution-plans/ep-type-face-lsp-bridge.md`
- umbrella：ai-rules `cr-lsp-replacement-roadmap.md` P1
- 電池不過＝維持並列（lsp-python 退役屬 P3，本卡不碰）
