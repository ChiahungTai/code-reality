"""code_reality — code reality 薄層工具鏈（dev-only）。

三源證據（LSP／CRG graph／VizTracer runtime）＋跨語言縫（boundary）
＋敘事載體 generator（tour／chain）的可消費機械產物：runtime_edges
（viztracer trace → runtime 呼叫邊表）、snapshot（CRG module-edge 集 →
commit 錨定 sidecar）、transition（兩 snapshot 邊集差異＋EP 宣稱對照）、
hub_refs（CRG callers 按檔聚合＋hazard 分層安全網——AST 級常駐＋
static_prod ≤ 2 觸發 rg 級 dynamic dispatch 偵測，規則在 hazard 模組）、
boundary_build＋boundary（NT pyo3 宣告 ↔ .pyi 合約對照 sidecar——
python 符號 → Rust 真身查詢）、delta_tour／chain_tour（transition/
callchain → CodeTour `.tour`）、graph_csv（graph.db → nodes/links CSV）、
graph_audit（CRG graph.db Rust 完整度稽核——D1 風險掃描＋D2
rust-analyzer 對帳）、scip_refs（rust-analyzer SCIP 索引查詢——CRG
同鍵去重受害符號的 def/refs 真相源 sidecar，與 graph_audit 缺差對帳）、
tour_validate／tour_upgrade／tour_manifest（`.tours` corpus 治理三件）。
repo 結構事實歸各 repo 的 ``.code-reality.toml``（profile 單一源）。

加 __init__.py 是為了 `python -m code_reality.<mod>` 顯式子套件
import（PEP 420 namespace package 在顯式 import 時不可靠——tools/lsp_mcp
前例）。禁 re-export——消費端用完整路徑 import。
"""
