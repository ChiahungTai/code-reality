# [bug] snapshot 0-files 修復 — kind 拆集投影＋WARN 歸因＋transition 退化防護

## 目標

REFERENCES-only graph.db（scip Rust／lsp-harvest 兩面常態）上
`snapshot` 不再輸出 0 files：S1 止血（空集合 WARN 歸因分流＋
transition 兩側空集合改警示「退化快照」不再誤報「無結構變化」，
delta_tour 自動繼承）；S2 拆 kind 集（files 面＝全 kind 參與檔案集、
module_edges 維持結構 kind，kind 決策移入 snapshot.rs 自有）＋
`_meta.files_face` 跨面 diff WARN＋清理兩張退化 sidecar
（`22900069`/`63b42c85`）。

## 相關

- EP：`ai-analysis/execution-plans/ep-snapshot-zero-files-fix.md`
  （baseline `63b42c8`，疊於未 commit 的 ep-data-plane-unification
  工作樹——其 commit 落地後才開工）
- 案件檔（唯一真相源）：
  `ai-analysis/reports/snapshot-zero-files-case.md`
- 立案來源：`.kanban/Done/data-plane-unification.md` 驗收 4——
  「沒治獨立立案」結論即本卡
- NOT：L3 Rust CALLS 衍生（歸 R4w watch item）／不改 `EDGE_KINDS`
  語義／不動 build 端／sidecar schema 除 `files_face` 外不動

## 驗收標準

1. S1 一個 commit：kind 分布/root 兩分支 WARN（bytes 釘住）＋
   transition 退化 pair 警示（markdown＋json 兩面；delta_tour 零改動
   繼承）
2. S2 一個 commit：files 面全 kind、module_edges 維持結構 kind
   （兩面分離顯式斷言；混合 kind db files=聯集）；`files_face` 欄位
   ＋跨面 diff WARN；兩張退化 sidecar 刪除＋自倉重產
3. 消費端 L4（SM-8 關閉條件）：自倉＋NT＋ai-rules 實跑
   `snapshot` files>0；mosaic ×3 files 增量實測極小——暴增＝設計
   推翻回頭重議
4. 全套 cargo test 綠；Test Impact Matrix（EP 內 T1-T11）逐項落地；
   案件檔結案

## 備註

- 決策類型：雙向門（S2 files 面可經 `files_face` 欄位識別回退）
- 架構理由：`EDGE_KINDS` 一常數服務兩個需求相反的消費者＝共用
  domain service 外溢——修消費端各取所需，共用層不動
