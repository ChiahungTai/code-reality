# EP: snapshot zero-files fix — kind-split file projection, WARN attribution, transition degenerate guard

> **ep_type**: implementation
> baseline: 63b42c85aa85d078ddd051d301b9ab4272552582
>
> Stacking note: this baseline sits on an **uncommitted** working tree
> (the ep-data-plane-unification changeset — `snapshot.rs`, `graph_db.rs`,
> tests, sidecar relocation). This EP's code changes must not conflict
> with those files' pending state; **work may only start after that EP's
> commit lands**. All anchors below were verified against the working
> tree (the state this EP builds on), not the bare baseline commit.

## North star

`code-reality snapshot --repo <repo>` reports a non-empty participating
file set on every producer face — including REFERENCES-only dbs (scip
Rust repos, lsp-harvest Python repos), today's norm since the legacy
importer retirement. When a snapshot still degenerates to 0 files, the
WARN names the true cause (edge-kind distribution vs unresolvable
endpoints), and `transition` refuses to convert two degenerate
snapshots into a false "無結構變化" conclusion. Truth source:
`ai-analysis/reports/snapshot-zero-files-case.md` (symptom, mechanism,
root-cause chain, blast radius, evidence anchors — all re-verified for
this EP, see Segment 0).

Design already adjudicated by the user (do not re-litigate): layered
fix — S1 = WARN attribution split + transition degenerate guard (one
commit, bleeding-stop); S2 = files/module_edges kind-set split with the
kind decision moved into `snapshot.rs` (one commit). L3 (Rust CALLS
derivation) is out of scope (see NOT).

## EP Review Findings

> 2026-08-29 獨立 agent 五維度審查（17 錨點獨立複核零 drift）＋主
> session 對 critical 親證（delta_tour allow-list 消費——`delta_tour.rs:
> 260-262`/`:267-271`/`:514-518`）後回寫。

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| F3-1 | 🔴 必須修正 | S1/T4/SM-8/段落0 | 「delta_tour 零改動自動繼承」被 `build_tour` 的 allow-list 消費反駁——警示欄位在 `.tour` 面被靜默丟棄，T4 兩子句矛盾不可實現 | T4 降級為 transition json 面；`.tour` 面不透傳＝allow-list 邊界屬實記錄（option a） | implemented |
| F3-2 | 🟡 重要 | S1/S2 | `kind_edge_count` 計數點 S1→S2 漂移（迴圈頂 vs 閘內解析後）——T1 釘的 root 分支會在 S2 二次翻轉 | 計數點入口化（kind 匹配即計、不論可解析）；T1 加 S2 後不再翻轉的回歸註記 | implemented |
| F4-1 | 🟡 重要 | S1 Context | 「健康→退化 mass gone-files 假陽性」被 Context 列為止血對象但 degenerate_pair 只防兩側皆空 | degenerate_pair 擴為任一側空＋文案區分何側（option a） | implemented |
| F1-1 | 🟡 重要 | T-matrix | 漏列 `cli_ok_line_and_sidecar_schema`（`:244-263` 釘死 `_meta` 完整 key 清單——`files_face` 必炸） | 增列 T12：更新（keys 尾補 `files_face`） | implemented |
| F1-2 | 🟡 重要 | T-matrix | 漏列 `truncation_over_twenty_appends_more_line`（既有退化 pair fixture） | 增列 T13＋渲染契約（警示附加段、變化段照渲染） | implemented |
| F5-1 | ℹ️ 提醒 | S1 WARN 分流 | 兩分支對 `raw==0` 全空 db 不互斥完備——else 誤歸因「不同 root？」 | else-if 補空 db 分支 | implemented |
| F4-2 | ℹ️ 提醒 | S2 sidecar 清理 | 外部 repo（NT、ai-rules）退化 sidecar 未在清單也未明示 out of scope | L4 實跑順手刪同 repo 退化檔（跨 repo 不代刪） | implemented |
| F4-3 | ℹ️ 提醒 | S2 文檔清單 | `common.rs:15-16` EDGE_KINDS doc comment 的「projection filter in snapshot」半句 S2 後過時 | S2 要點 4 補此項 | implemented |
| F1-3 | ℹ️ 提醒 | T-matrix | 兩個退化 pair 相鄰測試未列冊 | 增列 T14（不動＋理由） | implemented |
| F2-1 | ℹ️ 提醒 | EP 語言 | EP 中英混合與「全英」裁決表述張力（root AGENTS.md English scope 未含 ai-analysis；既有 EP 慣例中文主體） | user 裁決 ai-analysis/ 語言豁免與否 | needs-confirmation |

## UC 盤點

### Backlog 關聯
- `.kanban/Backlog/snapshot-zero-files-fix.md` — EP tracking card
  (created with this EP)
- Lineage: `.kanban/Done/data-plane-unification.md` acceptance item 4
  ("snapshot 0-files bug 對照實驗有結論——治了收案／沒治獨立立案")
  concluded **獨立立案** → this EP is that case

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（此 repo 無此檔，跳過）

### 掃描範圍
- root `AGENTS.md` Capabilities（Boundary/narrative tool family 行）、
  `crates/AGENTS.md`、`ai-analysis/reports/snapshot-zero-files-case.md`、
  `_done/ep-legacy-db-consumer-cutover.md`（F4 濾波器論證）、
  `_done/ep-occurrence-producer.md`（R4w watch item）

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Boundary / export / narrative tool family | ✅ | AGENTS.md Capabilities | 更新 | snapshot files 面語義修復＋transition/delta_tour 退化防護（能力恢復，非新能力） |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| （無新增能力——既有 snapshot/transition 行為修復） | — | — |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | REFERENCES-only db（scip Rust producer：自倉、NT）跑 snapshot | `snapshot --repo` | S2 後 files>0（全 kind 參與檔案集）；S1 僅止血——WARN 歸因 kind 分布而非「不同 root」 | 無 | narrative family |
| SM-2 | REFERENCES-only db（lsp-harvest Python producer：ai-rules）跑 snapshot | 同上 | 同 SM-1（lsp-harvest 面凍結 REFERENCES-only——`graph_db.rs:477-480`，by design） | 無 | 同上 |
| SM-3 | 健康 repo 回歸（mosaic ×3，CALLS 在場） | S2 後跑 snapshot | files 增量極小（REFERENCES 參與檔案 ⊇ CALLS 參與檔案）；**增量暴增＝設計被推翻**（falsification） | 無 | 同上 |
| SM-4 | 混合 kind db（CALLS＋REFERENCES） | S2 後 export | files＝跨 kind 聯集；module_edges＝結構 kind only | 無 | 同上 |
| SM-5 | 跨 face-version transition diff（舊 sidecar 無 `files_face` vs 新 sidecar 有） | `transition a.json b.json` | WARN：files 面不可比；module_edges 仍可比（kind 集不變） | 無 | 同上 |
| SM-6 | 既有 0-file sidecar 清理後 | 刪 `22900069`/`63b42c85` 兩張退化 sidecar＋重產自倉 snapshot | transition 不再對退化 baseline diff；新 sidecar files>0＋帶 `files_face` | 無 | 同上 |
| SM-7 | kind 匹配但端點不可解析（root 外端點 fixture） | fixture 製造 | root 分支 WARN（歸因保留「不同 root？」——此分支內屬實） | 無 | 同上 |
| SM-8 | 任一側 files 空的 transition pair（退化 snapshot） | `transition`/`delta_tour` | WARN「退化快照」（文案區分何側——單側退化＝該側檔案清單不可信），**兩側皆空才抑制「無結構變化」結論**；防護在 transition 的 md/json 兩面生效——delta_tour 的 `.tour` 面是 allow-list 消費，警示不透傳（review F3-1 屬實記錄） | 無 | 同上 |

> Lineage note: the parent scenario id **SM-8** in
> ep-data-plane-unification (snapshot 0-files) is closed by this EP's
> SM-1/SM-2 + S2's L4 acceptance (real-corpus `files > 0`), not by a
> single row here.

## 段落 0：全域研究摘要（anchors re-verified against working tree）

Mechanism (case file, confirmed): `export_module_edges` filters
`WHERE kind IN (IMPORTS_FROM, CALLS, INHERITS)`
(`snapshot.rs:121-122`, `EDGE_KINDS` at `common.rs:17`); on a
REFERENCES-only db the filter matches 0 rows and `files.insert`
(`snapshot.rs:146-147`) only runs inside that loop, while
`raw_edge_count` (`snapshot.rs:153-155`) counts the full table — the
two numbers in today's WARN (`snapshot.rs:475-484`) come from different
queries and their divergence is the mechanism's own confession.
REFERENCES-only is the **norm**: `py_calls::call_sites` is a ruff
**Python** parser fed only `.py` sources (`graph_db.rs:474-490`), so
Rust repos take the REFERENCES branch (`graph_db.rs:626-631`); the
lsp-harvest face is frozen REFERENCES-only by design
(`graph_db.rs:477-480`). The legacy importer was the only structural
kind source for Rust repos until its removal severed it (case file
root-cause chain 3).

Verified anchor table (definition end / consumption end):

| Anchor | Definition | Consumption / note |
|---|---|---|
| `EDGE_KINDS` | `common.rs:17` | `snapshot.rs:122`（S2 起消費端自查）；語義＝transition「無結構變化」邊界 |
| `export_module_edges` | `snapshot.rs:100` | `snapshot.rs:382`（`build_snapshot`） |
| 空集合 WARN | `snapshot.rs:475-484` | 文案歸因「不同 root？」——S1a 分流點 |
| `load_snapshot` / `LoadedSnapshot` | `transition.rs:68` / `:61-66`（攜 `meta`——face-version 欄位可讀） | transition 報告路徑＋`delta_tour.rs:675/:679` |
| `summarize` | `transition.rs:172` | `transition.rs:594` 一帶＋`delta_tour.rs:699` |
| 「無結構變化」分支（無空對防護） | `transition.rs:372-382` | S1b 插入點 |
| `render_json_value` | `transition.rs:459` | `delta_tour.rs:699-700` 讀 json——但 `build_tour` 為 **allow-list 消費**（`:260-262` 只讀 `_meta.before/after`、`:267-271` `ep_claims`、輸出僅 title/description/steps `:514-518`）：警示欄位不透傳到 `.tour` 面（review F3-1；防護面＝transition md/json 兩面） |
| `make_meta` | `common.rs:161` | `snapshot.rs:386`——`files_face` 欄位經同一 meta 機制 |
| `cli_empty_set_warn` | `tests/s2_snapshot.rs:331-358` | fixture 以 root 外端點（`/elsewhere/…`）製造空集合——測試情境本身編碼了錯誤歸因（case file 判讀成立） |
| `export_counts_same_module_files_and_skips_excluded` | `tests/s2_snapshot.rs:86-110` | REFERENCES 邊兩端點已透過結構邊進 files——現斷言**測不出** S2 變化，須補 REFERENCES-only 參與檔（見 Test Impact Matrix） |
| `edge_kinds_pinned` | `common.rs:467-470` | 不動（EDGE_KINDS 原名原義） |
| `report_no_change_and_claims_faces` | `tests/s3_transition.rs:184-220` | fixture 有真實內容（`files=["f.py"]`）——不動 |
| R4w watch item | `_done/ep-occurrence-producer.md:116`（詞法捷徑排除 `:42-47`；CALLS 下游盤點 `:163`） | L3 歸屬 |

Sidecar tape (re-enumerated): exactly **two** degenerate sidecars —
`code-reality-22900069.json` and `code-reality-63b42c85.json` (both 0
files / raw 2277); every earlier sidecar carries 43-47 files / raw
7007. Matches the case file's cliff description. (Note: current file
mtimes show 08-28 22:24 for `22900069`, not the case file's 14:24 —
mtimes were reset by the data-plane sidecar relocation; the cliff
boundary itself agrees.)

**可複用基礎設施**：`graph_db_fixture::make_graph_db`（兩個 s2 測試已用）；
`tests/s5_delta_tour.rs`（json 面繼承驗證落點）；`make_meta` extra-pairs
機制（`files_face` 免新通道）。

**風險假設**：
- R1（中，S2 falsification）：mosaic files 增量「極小」是推斷（REFERENCES
  參與檔案 ⊇ CALLS 參與檔案）——SM-3 實測裁决；暴增＝設計推翻。
- R2（低，S1）：kind-matched-but-empty 的殘餘歸因——kind 匹配列全數被
  profile exclusion 排除時也會落 root 分支；root 分支文案保留問號語氣
  （「不同 root？」是提示非斷言）。`raw==0` 空 db 第三態已補分支
  （review F5-1）；profile-exclusion 殘餘不另擴分支（超出已裁決範圍）。
- R3（低，S2）：`files_face` 缺席的舊 sidecar 判讀為 structural-only 面
  ——`LoadedSnapshot.meta` 已攜全 meta（`transition.rs:63/:126`），無 schema
  改動。

## Test Impact Matrix（pre-declared——作法改掉就逐項判測試刪/改/補）

| # | 測試 | 錨點 | 動作 | 段落 |
|---|------|------|------|------|
| T1 | `cli_empty_set_warn` | `tests/s2_snapshot.rs:331-358` | **更新**：fixture 不變（root 外端點＝root 分支案例），釘住的 WARN bytes 改為 root 分支文案；S2 後 root 分支 bytes **不再翻轉**（計數點入口化——F3-2 的回歸確認） | S1 |
| T2 | kind 分布分支 WARN（新） | s2_snapshot.rs 新增 | **新增**：REFERENCES-only fixture（root 內端點）→ kind 分布分支新文案 | S1 |
| T3 | transition 兩側空集合 WARN（新） | s3_transition.rs 新增 | **新增**：兩張 0-file snapshot → WARN「退化快照」face，不出現「無結構變化」結論 | S1 |
| T4 | transition json 面退化警示（新） | s3_transition.rs 新增 | **新增**：退化 pair → **transition json 輸出**帶 degenerate 警示欄位（delta_tour `.tour` 面 allow-list 不透傳——邊界屬實記錄，非測試標的；review F3-1） | S1 |
| T5 | `export_counts_same_module_files_and_skips_excluded` | `tests/s2_snapshot.rs:86-110` | **更新**：顯式兩面分離斷言——補一個**僅經 REFERENCES 參與**的檔案，斷言其**進 files**、**不進 module_edges**（現 fixture 兩端點已被結構邊覆蓋，測不出分離） | S2 |
| T6 | `edge_kinds_pinned` | `common.rs:467-470` | **不動**（EDGE_KINDS 原名原義；共用層零語義變更的釘住） | — |
| T7 | snapshot 側 files-kind 集 pin（新） | snapshot.rs 側新增 | **新增**：snapshot.rs 自有 kind 決策釘住（全 kind 參與 files 面） | S2 |
| T8 | `report_no_change_and_claims_faces` | `tests/s3_transition.rs:184-220` | **不動**（fixture 有真實內容——已驗證 `files=["f.py"]`） | — |
| T9 | 混合 kind 兩面（新） | s2_snapshot.rs 新增 | **新增**：CALLS＋REFERENCES db → files＝聯集、module_edges＝CALLS only | S2 |
| T10 | face-version 欄位 pin（新） | s2/s3 新增 | **新增**：`_meta.files_face` 在場 pin；跨面 diff → transition WARN（s3） | S2 |
| T11 | 消費端 L4 驗收 | 自倉＋NT 實跑 | **新增**（非 cargo test）：`snapshot` files>0——**SM-8 真正關閉條件**；含 mosaic ×3 增量實測（SM-3 falsification）；L4 實跑時順手刪**同 repo** 退化 sidecar（NT／ai-rules 各一張，review F4-2；跨 repo 不代刪） | S2 |
| T12 | `cli_ok_line_and_sidecar_schema` | `tests/s2_snapshot.rs:180-266` | **更新**：`_meta` key 清單釘樁（`:244-263`）尾部補 `files_face`（review F1-1）；`crg_raw_edges`/files 計數斷言不變 | S2 |
| T13 | `truncation_over_twenty_appends_more_line` | `tests/s3_transition.rs:265-286` | **更新**：既有退化 pair fixture（兩側空 files＋sb 帶 25 邊）——渲染契約：警示＝附加段、變化段照渲染、兩側皆空才抑制結論（review F1-2） | S1 |
| T14 | `cli_baseline_log_and_profileless_warn` / `cli_missing_ep_crashes_exit_1` | `tests/s3_transition.rs:336-363` / `:365-388` | **不動**（前者 stdout-only 斷言、後者渲染前 crash——已核不受影響；review F1-3 列冊） | — |

## 段落 S1：止血——WARN 歸因分流＋transition 退化防護（一個 commit）

**Context**：症狀的兩個誤導面同時止血——(a) snapshot 空集合 WARN 把
kind 分布造成的空集合錯誤歸因「不同 root？」；(b) transition 對兩張退化
snapshot 必然輸出假陰性「無結構變化」（`transition.rs:372-382` 無空對
防護），健康→退化方向則 mass gone-files 假陽性。UC 引用：更新「Boundary
/ export / narrative tool family」。零語義變更：files/module_edges
輸出不變。
- 依賴：無（首段；S2 的獨立前置）
- 語義約束：與 S2 共享「`EdgeExport` 欄位擴充只增不改」——S1 引入的
  `kind_edge_count` 在 S2 後語義為結構 kind 匹配列數，欄位名即此義
- 基礎設施：`graph_db_fixture`（WARN 測試）、`tests/s5_delta_tour.rs`
- 依賴錨點：見段落 0 表（空集合 WARN / 無結構變化分支 / render_json_value 三行）
- Invariant Impact：**觸及 silent-conclusion path**——transition 是
  review claims 對照面，假「無結構變化」結論靜默污染審查判讀（非會計/
  風控，但同屬 bug 不 crash 而污染下游）。驗證對齊：T3/T4 直接斷言 WARN
  在場＋結論缺席。

**要點**：
1. `EdgeExport`（`snapshot.rs:69-73`）增 `kind_edge_count: usize`——
   kind 匹配列數，**迴圈入口計數**（kind 匹配即計、不論端點可解析
   ——S2 平移後語義不變，review F3-2）。
2. 空集合 WARN（`snapshot.rs:475-484`）分流：
   - `files 空 && kind_edge_count == 0 && raw_edge_count > 0` →
     **kind 分布分支**：如實陳述「結構 kind（IMPORTS_FROM/CALLS/
     INHERITS）匹配 0 邊、raw 全數其他 kind——scip Rust 與
     lsp-harvest repo 的常態」，不再指控 root。
   - `files 空 && kind_edge_count > 0` → **root 分支**：保留「不同
     root？」歸因——此分支內（kind 匹配列存在但全部無法解析進 repo
     root）該歸因屬實（R2 的 profile-exclusion 殘餘由問號語氣涵蓋）。
   - `files 空 && raw_edge_count == 0` → **空 db 分支**：「db 零邊
     （空 build？）——先 `graph_db build`」（review F5-1 第三態；
     建過的 db 幾乎必有 REFERENCES，罕見但可構造）。
3. transition 增共享 helper `degenerate_pair(&sa, &sb) -> Option<String>`
   —— **任一側** `files` 空即退化（文案區分何側——review F4-1：單側
   退化＝該側 added/gone 檔案清單不可信）。`render_report`（markdown
   面，改寫 `:372-382` 的結論路徑）與 `render_json_value`（json 面，
   `:459`）兩面消費。渲染契約：退化警示＝**附加段**、變化段照渲染
   （files 面 degenerate 不否定 module_edges 面——review F1-2）；僅
   **兩側皆空**時抑制「無結構變化」結論。delta_tour（`delta_tour.rs:
   675-699`）經 json 面讀 summarize，但 `build_tour` 為 allow-list
   消費——`.tour` 面不透傳警示（屬實記錄，review F3-1；零
   delta_tour.rs 改動維持）。

**Pseudo Code**：
```rust
// snapshot.rs — EdgeExport 擴充 + WARN 分流
pub struct EdgeExport { files, module_edges, raw_edge_count,
                        kind_edge_count: usize }  // NEW: kind-matched rows
for row in kind_filtered_rows { kind_edge_count += 1; /* …不變… */ }
// run() 空集合分支：
if snap.files.is_empty() {
    let raw = snap.meta["crg_raw_edges"]; let kind_n = exported.kind_edge_count;
    if kind_n == 0 && raw > 0 { push!(kind 分布分支 WARN) }   // 常態歸因
    else if raw == 0          { push!(空 db 分支 WARN) }      // 零邊 db（F5-1）
    else                       { push!(root 分支 WARN) }       // 保留原歸因
}

// transition.rs — 退化 pair 防護（兩 render 面共享；任一側空即退化）
fn degenerate_pair(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> Option<String> {
    match (sa.files.is_empty(), sb.files.is_empty()) {
        (true, true) => Some("兩側 snapshot files 皆空（退化快照）——diff 無意義，勿下「無結構變化」結論".into()),
        (true, false) => Some("before 側 snapshot files 空（退化）——gone-files 清單不可信".into()),
        (false, true) => Some("after 側 snapshot files 空（退化）——added-files 清單不可信".into()),
        _ => None,
    }
}
// render_report：警示＝附加段、變化段照渲染；兩側皆空才抑制「無結構變化」結論
// render_json_value：同一 helper → json 警示欄位（.tour 面不透傳——allow-list 邊界）
```

**驗證策略**：
- DEPTH-MIN：T1（更新 root 分支 bytes）/ T2 / T3 / T4——fixture 層全綠。
- DEPTH-SAMPLE（L4 消費端）：自倉實跑 `snapshot`——仍 0 files（S2 前屬
  預期）但 WARN 歸因 kind 分布；用兩張既有退化 sidecar 實跑
  `transition` → 退化警示 face（同時是 SM-8 行為的實景演示）。
- 全套 `cargo test` 綠；S1 commit 訊息附自倉實跑輸出。

## 段落 S2：files 面放寬——kind 集決策移入 snapshot.rs＋face-version＋sidecar 清理（一個 commit）

**Context**：`EDGE_KINDS` 一個常數服務兩個需求相反的消費者——transition
要結構邊界（「無結構變化」的語義基礎）、snapshot files 面要參與檔案集宇
宙。這是共用 domain service 外溢陷阱：一個常數被迫同時定義「什麼算結構
變化」與「什麼檔案算參與」，REFERENCES-only 常態下後者塌縮為空。修法＝
**消費端各取所需**：`common.rs` 的 `EDGE_KINDS` 保持原名原義（transition
邊界），snapshot.rs 自有 kind 決策（bounded context——files 面全 kind、
module_edges 面維持 `EDGE_KINDS`）。UC 引用：更新「Boundary / export /
narrative tool family」。mosaic 參考基準：files 增量應極小（SM-3
falsification）。
- 依賴：S1 的 `kind_edge_count` 欄位（語義平移為結構 kind 匹配數）
- 語義約束：與 S1 共享 `EdgeExport` 欄位契約；`common.rs:17`
  `EDGE_KINDS` 與 `common.rs:467-470` pin 測試零改動；transition 的
  module_edges 消費面（kind 集不變）零改動
- 基礎設施：`graph_db_fixture`（混合 kind db）、`make_meta` extra-pairs
- 依賴錨點：見段落 0 表（EDGE_KINDS / export_module_edges / make_meta /
  load_snapshot(meta) 四行）
- Invariant Impact：files 面語義變更（結構 kind 檔案集 → 全 kind 檔案
  集）——下游 transition files diff 語義隨之變。驗證對齊：T5/T7/T9
  釘兩面分離；T10 釘 face-version 閘；SM-3 實測把「增量極小」從推斷
  升為證據；跨面 diff 語義保護由 SM-5 WARN 承擔。

**要點**：
1. `export_module_edges` 單趟查詢去 kind 過濾（`SELECT kind, caller,
   callee FROM edges`）；`files` 對每條可解析、非排除端點收錄（**全
   kind**）；module edge insert 加消費端閘 `EDGE_KINDS.contains(&kind)`
   ——`snapshot.rs:94-99` 文檔註解改寫為兩面語義。
2. `_meta` 增 `files_face: "all-kinds"`（缺席 ≡ 舊 structural-only
   面）。transition 兩 render 面在 `degenerate_pair` 之外另查
   `sa/sb.meta["files_face"]` 不一致 → WARN（files 面跨面不可比；
   module_edges 仍可比——kind 集不變）。
3. Sidecar 清理（operational——`.code-reality/` 自帶 gitignore，非
   commit 內容）：刪兩張已驗證退化 sidecar（`code-reality-22900069.json`
   、`code-reality-63b42c85.json`）；重產自倉 snapshot（files>0＋帶
   `files_face`），作為 SM-8 關閉的自倉證據。L4 實跑時順手刪**同
   repo** 的退化 sidecar（NT／ai-rules 各有一張——review F4-2；跨
   repo 不代刪）。
4. 文檔：`snapshot.rs` 模組註解、root AGENTS.md narrative family 行、
   `crates/AGENTS.md`（若述及 files 面語義）、**`common.rs:15-16`
   `EDGE_KINDS` doc comment**（「the projection filter in snapshot」
   半句過時——改寫為 transition 邊界＋module_edges 面閘，review F4-3）。

**Pseudo Code**：
```rust
// snapshot.rs — bounded-context kind 決策（common.rs 不動）
let sql = "SELECT kind, caller_symbol, callee_symbol FROM edges";  // 無 WHERE
for (kind, src_q, dst_q) in rows {
    let structural = EDGE_KINDS.contains(&kind.as_str());   // 消費端閘
    if structural { kind_edge_count += 1 }                   // 入口計數（S1 語義：不論可解析——F3-2）
    let (Some(src_rel), Some(dst_rel)) = (endpoint_rel(src_q), endpoint_rel(dst_q)) else { continue };
    if excluded { continue }
    files.insert(src_rel); files.insert(dst_rel);           // 全 kind 參與
    if structural && module_of(src) != module_of(dst) { module_edges.insert(..) }
}
// build_snapshot：meta.insert("files_face", "all-kinds")
// transition render 面們：files_face 不一致 → WARN（files 面不可比；module_edges 可比）
```

**驗證策略**：
- DEPTH-MIN：T5（兩面分離，含 REFERENCES-only 參與檔）/ T7 / T9 / T10
  ——fixture 層全綠。
- DEPTH-SAMPLE（L4）：自倉＋ai-rules（兩種 REFERENCES-only producer
  face）實跑 `snapshot` → files>0（SM-1/SM-2）。
- DEPTH-FULL（L4）：NT 實跑 `snapshot` → files>0（232,052 REFERENCES
  邊的量級面）；mosaic ×3 實跑 → files 增量實測，記錄於 EP/commit 證據
  ——**增量暴增＝設計推翻，回頭重議**（R1 falsification，如實呈報不
  硬拗）。
- 跨面 diff：舊（無欄位）vs 新 sidecar 實跑 `transition` → WARN 在場
  （SM-5）。
- 全套 `cargo test` 綠；S2 commit 訊息附四 repo 實跑 files 數＋mosaic
  增量。

## NOT（out of scope——防 scope creep 再議）

- **L3 Rust CALLS 衍生**（build-side Rust call scanner）——歸 R4w watch
  item（`_done/ep-occurrence-producer.md:116`；詞法捷徑已被 `:42-47`
  排除；CALLS 下游盤點見 `:163`）。本 EP 的 S2 使 snapshot 不依賴 CALLS
  在場——L3 未來落地改變的是 module_edges 密度，非 files 面存亡。
- **不改 `EDGE_KINDS` 語義/名稱**——`common.rs:17` 與
  `common.rs:467-470` pin 測試不動；transition 結構邊界不變。
- **不動 graph_db build 端**——REFERENCES-only 常態的成因鏈（Python-only
  call scanner＋凍結 lsp-harvest 面）不在本 EP 觸及範圍。
- **snapshot sidecar schema 除 `files_face` 欄位外不動**——
  `files`/`module_edges`/`_meta` 既有欄位合約不變（transition 欄位消費
  面零改動）。

## 整合策略

- 順序：S1 → S2（S1 先行止血、零語義風險；S2 依賴 S1 的
  `kind_edge_count` 欄位）。各自獨立 commit（已裁決）。
- 開工前置：ep-data-plane-unification 的 commit 先落地（baseline 疊加
  註記）；本 EP code 變更與其未 commit 檔案（`snapshot.rs` 等）同檔修
  改須在其 commit 之後進行。
- 證據閘門：每段 commit 訊息附 L4 實跑證據（S1：WARN 歸因實景；S2：
  四 repo files 數＋mosaic 增量）。
- baseline: 63b42c85aa85d078ddd051d301b9ab4272552582

## 收尾步驟

1. Capabilities＋Kanban：root `AGENTS.md` narrative family 行補 files
   面語義註記（全 kind 參與＋退化防護）；`.kanban/Backlog/
   snapshot-zero-files-fix.md` 搬 `.kanban/Done/`（原子操作）；從
   Scenario Matrix 提煉自包含消費場景句入卡片描述
2. SYSTEM-MAP：無此檔，跳過
3. instruction 檔：`crates/AGENTS.md` 若述及 snapshot files 面語義則同步
   （兩面分離＋`files_face`）；`snapshot.rs` 模組註解已是行內文檔面
4. `/audit-test` 對 T1-T10 測試組（更新＋新增面）；案件檔
   `ai-analysis/reports/snapshot-zero-files-case.md` 補結案狀態行
   （Status: open → closed by this EP）
