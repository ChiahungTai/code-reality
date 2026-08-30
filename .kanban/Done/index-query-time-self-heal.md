# [infra] 主索引 query-time 過期偵測＋lazy 自癒 — scip_refs 家族 stale 命中自動重建

## 目標

主索引（`<repo>/.code-reality/scip/index.scip`）與原始碼之間建立 query-time
守衛：scip_refs 家族查詢時偵測 index↔source 漂移（Stage A：walk+mtime；
Stage B：doc-set 差異），實質漂移→自動重產（single-flight，重用 build
傘形）＋heal 後驗證防迴圈；僅 HEAD／producer 版本漂移→WARN-only。生產者
寫入改 tmp+rename 原子化（並發前提）。**commit 粒度背景刷新**（user
2026-08-30 裁決修正，save≠commit）：opt-in `hook install` → post-commit
背景 `refresh`（source 變→全量重產；僅 head 移動→re-stamp-only），
query-time heal 降級為安全網。

## 相關

- EP：`ai-analysis/execution-plans/ep-index-query-time-self-heal.md`
  （baseline `62dee2ae`）
- 動機實案：2026-08-30 ai-rules dogfood incident（index 落後 19 commits、
  stamp 缺失、producer 落後 2 版；WARN 有印但 session 仍消費 stale 資料）
- 裁決：反 save-time（watcher/daemon 永不做）；graph face 自癒 defer

## 驗收標準

- stale 查詢（缺檔／既有檔編輯）自動癒合且以新索引作答（SM-2/3）
- heal 失敗 serve stale＋loud WARN，查詢不擋死（SM-6）
- 並發 stale 命中 single-flight，一重建一等待重用（SM-7/8）
- false-stale（偵測≠producer 語料）heal 後 WARN-once 不迴圈（SM-9）
- `CODE_REALITY_AUTOHEAL=off` 完全退出行為（SM-10）
- 小 repo WARN-2 噪音消除：healthy 靜默、缺檔精確訊息（SM-14）
- commit → 背景刷新（SM-19）；docs-only commit 只 re-stamp 不重產；
  hook 缺席時查詢自癒接手（SM-20）

## 備註

實作排序在在飛三項（producer 重建→stamp→ai-rules index 刷新）之後。
