# [bug] snapshot 0-files 修復＋lsp-bridge 收斂門修正 — kind 拆集投影＋WARN 歸因＋退化防護＋毒化快取防穿門

## 目標

REFERENCES-only graph.db（scip Rust／lsp-harvest 兩面常態）上
`snapshot` 不再輸出 0 files：S1 止血（空集合 WARN 歸因分流＋
transition 退化 pair 改警示不再誤報「無結構變化」——防護在
transition md/json 兩面，delta_tour `.tour` 面為 allow-list 消費
不透傳）；S2 拆 kind 集（files 面＝全 kind 參與檔案集、
module_edges 維持結構 kind，kind 決策移入 snapshot.rs 自有）＋
`_meta.files_face` 跨面 diff WARN＋清理兩張退化 sidecar
（`22900069`/`63b42c85`）。**S3（2026-08-29 併入，同 session 修）**：
lsp-bridge `check_file` 收斂門修正——毒化 diag-cache 條目（pyrefly
didClose 空 push 晚到落網）不再穿過 fresh/stalled 門（mutation
Instant 跨 call 可見＋stalled 改時間語義）；真實呼叫者正確性，
`lru_evict` 測試＝偵測器（「負載 flake」是誤判——低負載重現 4/35）。
**S4（2026-08-29 第二增量，S3 後執行）**：transition CLI 退役——
delta_tour 成為唯一 diff 介面；退化防護落點移入 `summarize` 層
（`Summary.degenerate`）；`build_tour` allow-list 新增讀取退化標記
注入 tour description（S1 的「delta_tour 零改動」宣稱正式退場）；
ai-rules 三檔回執（transition CLI → delta_tour）。

## 相關

- EP：`ai-analysis/execution-plans/ep-snapshot-zero-files-fix.md`
  （baseline `63b42c8`，疊於未 commit 的 ep-data-plane-unification
  工作樹——其 commit 落地後才開工）
- 案件檔（唯一真相源）：
  `ai-analysis/reports/snapshot-zero-files-case.md`（S1/S2）＋
  `ai-analysis/reports/lsp-bridge-poisoned-cache-case.md`（S3）
- 立案來源：`.kanban/Done/data-plane-unification.md` 驗收 4——
  「沒治獨立立案」結論即本卡（S3 為 2026-08-29 lru_evict 根因
  調查後 user 併入）
- NOT：L3 Rust CALLS 衍生（歸 R4w watch item）／不改 `EDGE_KINDS`
  語義／不動 build 端／sidecar schema 除 `files_face` 外不動／
  上游 pyrefly 修改（回報不修——bridge 免疫後不依賴）

## 驗收標準

1. S1 一個 commit：kind 分布/root/空 db 三分支 WARN（bytes 釘住）＋
   transition 退化 pair 警示（markdown＋json 兩面；delta_tour 零改動
   ——`.tour` 面 allow-list 不透傳為屬實邊界；**S4 後由 build_tour
   主動消費取代——EP 內自洽演化**）
2. S2 一個 commit：files 面全 kind、module_edges 維持結構 kind
   （兩面分離顯式斷言；混合 kind db files=聯集）；`files_face` 欄位
   ＋跨面 diff WARN；兩張退化 sidecar 刪除＋自倉重產
3. 消費端 L4（SM-8 關閉條件）：自倉＋NT＋ai-rules 實跑
   `snapshot` files>0；mosaic ×3 files 增量實測極小——暴增＝設計
   推翻回頭重議
4. S3 一個 commit：毒化條目不穿 fresh/stalled 門（T15/T16）；
   `lru_evict` 連跑 ≥20 次零失敗（T17；修前 4/35）
5. S4 一個 commit：main.rs transition dispatch 0 hits；`summarize`
   帶退化標記（模組測試）；delta_tour 對退化 pair 的 description 含
   「退化快照」警示（新測試）；模組級測試全綠；回執清單交付
   （ai-rules 三檔條文變更）
6. 全套 cargo test 綠；Test Impact Matrix（EP 內 T1-T25）逐項落地；
   兩份案件檔結案

## 備註

- 決策類型：雙向門（S2 files 面可經 `files_face` 欄位識別回退）
- 架構理由：`EDGE_KINDS` 一常數服務兩個需求相反的消費者＝共用
  domain service 外溢——修消費端各取所需，共用層不動

## 完成情境（2026-08-29 結案）

- REFERENCES-only repo（scip Rust／lsp-harvest 面）跑 `snapshot` →
  files>0（自倉 70／NT 1955／ai-rules 7），sidecar 帶
  `_meta.files_face="all-kinds"`；健康 repo（mosaic ×3）同 commit
  增量 +5/+6/+26（參與檔案集幾乎不變）
- 空 db 仍 0 files 時，WARN 如實歸因（kind 分布／root 不符＋profile
  排除／空 db 三分支，bytes 釘住）
- 退化 snapshot pair 過 `delta_tour` → tour description 前置
  「⚠️ 退化快照警示」（＋跨面 files 警示），變化 steps 照渲染——
  不再靜默「無結構變化」
- lsp-bridge `check_file`：毒化 diag-cache 條目（eviction 空 push）
  不再作為收斂答案；停滯時 half-window `force_reopen` 恢復
  （T17 偵測器 20/20 零失敗，修前 4/35）
- transition CLI 退役：`rg '"transition"' main.rs` → 0 hits；
  `summarize` 帶退化標記（任何消費者自動繼承）
