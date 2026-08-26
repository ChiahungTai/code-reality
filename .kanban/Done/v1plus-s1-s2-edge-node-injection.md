# 能力卡：SCIP 邊匯出＋sidecar 注入／graph_audit 節點注入（v1+ S1/S2）

[tag:code-reality] [capability]

## 目標
`code-reality scip_edges --repo <repo> [--inject [--dry-run|--json]]`：SCIP
reference 邊匯出（occurrence 歸屬面）＋union sidecar（`<index-stem>.union.db`）
冪等注入——(A) 裁決：graph.db 邊面零寫入（CRG engine 無過濾消費 REFERENCES，
寫入＝未裁決漂移）。

`code-reality scip_nodes --repo <repo> [--dry-run] [--rollback] [--json]`：
graph_audit missing → 雙鍵對帳（無 DEF 不注入）→ graph.db 節點注入——家族唯一
graph.db 寫入面；`extra {"tier":"SCIP"}` marker 回滾＋`VACUUM INTO` 首注備份。

## 相關
- EP：`ai-analysis/execution-plans/ep-v1plus-graph-engine.md` S1/S2（(A) sidecar
  裁決塊；EP Review rows 17-20 build 審紀錄）
- NT L4 實測：export 393,609 三數字重現／inject 182,137 冪等／861→596
  （可封閉集歸零——殘餘結構性：71 qname-occupied＋525 index-universe 外）

## 驗收標準
- 冪等複注入零增長；刪 sidecar 重注穩定；dry-run 不落地——`tests/scip_edges.rs`
- marker 回滾只刪標記節點；UNIQUE 碰撞 skip＋殘餘如實；rollback 圖徑獨立＋互斥
  guard——`tests/scip_nodes.rs`
