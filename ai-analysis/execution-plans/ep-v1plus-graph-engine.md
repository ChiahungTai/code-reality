# EP — v1+ 圖引擎裁決弧（B1/B2）：SCIP×CRG 邊集互補、注入與引擎評估

> Status: **active**（已入庫 `711d6e8`；S1 邊面形態裁決＝**(A) sidecar**——原 (a)
> 寫入之「下游立即受益」動機經雙審查否證，judge 委任改判 2026-08-26，user 可推翻；
> S3 裁決完成＋user 門①②③全採納（2026-08-26）；**deep-work 全弧終結（同日）**：
> 引擎 parity 子 EP（`_done/ep-v1plus-engine-parity.md`）S1-S10 全落地——含 S10
> Leiden、S5-mapper union 整合（NT impact +2,544）、LSP document_symbols 面；
> **S4 評估完成**＝引擎層退役 READY（`ai-analysis/reports/s4-crg-retirement-readiness.md`；
> 剩 tree-sitter producer＋deferred embeddings＋ai-rules 消費端 cutover）；
> S5 剩餘＝Python producer POC（scip-python）未做）
> **EP Review（2026-08-26，雙獨立審查＋judge 覆核）**：🔴×1＋🟡×10 已回寫＋第二
> 審查補遺 5 項入表（rows 12-16）；REFERENCES 語義 gate 已結（S1 裁決塊）；
> verdict＝**可執行**（S1 邊面 sidecar／S2 節點面照舊／S3/S5 不變）。
> Baseline: `00bcd07`（build 起點；snapshot：
> `~/.mosaic/code-reality/snapshots/code-reality-00bcd07d.json`）

## EP Review Findings

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| 1 | 🔴 必須修正 | S1 | 「全部下游立即受益」vs「對現有讀者隱形」矛盾：code-reality 讀面只認三 kind（common.rs:17）＝隱形且零受益；CRG engine 讀 REFERENCES（constants.py:62 權重 0.6、refactor.py:531 dead-code、communities.py:815,1042 全 kind）＝不隱形且語義漂移未裁決 | **已裁決＝(A) sidecar**（judge 委任裁決 2026-08-26，user 可推翻；S1 裁決塊） | implemented |
| 2 | 🟡 建議 | S1 | 注入執行器本體未指定 | `scip_edges --inject [--dry-run]` 唯一寫入面＋CLI 註冊＋模組清單 | implemented |
| 3 | 🟡 建議 | S1 | qname 映射契約缺失（SCIP scheme ↔ CRG 絕對路徑 qname 不相交；communities qn_to_idx drop 原樣符號） | S1 補映射契約工作項（映射器挪 S5——見 row 20） | implemented（映射→S5） |
| 4 | 🟡 建議 | S2 | nodes 表無 provenance 欄位（schema 實查）→ 節點回滾僅剩 backup | extra 塞 {"tier":"SCIP"} 標記 | implemented |
| 5 | 🟡 建議 | S2 | qualified_name UNIQUE 碰撞策略未定 | 存在即 skip＋碰撞計數報告 | implemented |
| 6 | 🟡 建議 | S1 | 排序契約漏增量面（CRG incremental 按檔 DELETE 刪 SCIP 行） | 契約擴至任何 CRG 寫入後重跑注入 | implemented |
| 7 | 🟡 建議 | 證據段 | 描述漂移（example 已入庫、a1 TSV 已刪、compare.py 需 gunzip） | 同步現況 | implemented |
| 8 | 🟡 建議 | 全域 | AGENTS.md Capabilities 同步＋kanban 卡缺失 | 補收尾步驟段 | implemented |
| 9 | 🟡 建議 | S1 | 無測試計畫（冪等/回滾/過濾/匯出） | 列最小不變項測試 | implemented |
| 10 | 🟡 建議 | 全域 | Scenario Matrix 缺席（含實質寫入步驟） | S1 補最小場景表 | implemented |
| 11 | 🟡 建議 | S5 | POC 無量化 pass bar；ai-rules 無 CRG graph 未註明 | 補判準量化要求＋語料註記 | implemented |
| 12 | 🔴（二審） | S1 | upsert 鍵碰撞：edges 無 UNIQUE 約束（原生 5-tuple 重複 15 組）、CRG `upsert_edge` 比對鍵不含 tier（graph.py:235-266）→ 沿用則同鍵改寫原生行 tier、tier-DELETE 回滾誤刪原生邊 | (A) 裁決下 sidecar 自有 schema 自訂 UNIQUE 鍵（雙名並存：SCIP symbol＋qname——**挪 S5**，見 row 20） | implemented（雙名→S5） |
| 13 | 🟡（二審） | S1 | 前置條件缺：SCIP sidecar 在場＋index 新鮮度（實測 index 8/24 比 graph.db 8/25 舊）＋邊界（graph.db 缺失走 protobuf 面等） | S1 護欄補前置清單 | implemented |
| 14 | 🟡（二審） | S1 | 邊面驗收無數字（外部符號過濾後注入量未知） | sidecar 邊數對帳（COUNT＝匯出面行數＋過濾率） | implemented |
| 15 | ℹ️（二審） | S1 | 「updated_at 掃除＝對齊 CRG 慣例」歸因不精確——CRG 是 file-scoped DELETE＋重插（graph.py:267-270） | 措辭修正為自有設計 | implemented |
| 16 | ℹ️（二審） | S2 | CRG rebuild/incremental 對已注入節點的命運未寫（file-scoped remove 可能清除） | 補「任何 CRG 寫入後重跑節點注入」 | implemented |
| 17 | 🟡（build 審） | S2 | 注入列 file_path/qname 未 resolve——非 canonical 路徑（macOS `/var` symlink）下 analytic residual 與 live re-audit 背離（模型前提未被代碼保證） | 改 `resolve()` 形態（qname＋file_path） | implemented |
| 18 | 🟡（build 審） | S2 | audit errors 捨棄＋無 aggregate guard——RA 全面失效時 missing=0 → 假乾淨「全收斂」 | errors 進報告（JSON 鍵＋`[WARN]` 行） | implemented |
| 19 | 🟡（build 審） | S2 | rollback 僅 lib fn、無 CLI 操作面 | `scip_nodes --rollback`（marker 刪除＋計數） | implemented |
| 20 | 🟡（build 審） | S1 | qname 雙名並存／映射器未實作但 rows 3/12 標 implemented | **裁決：映射挪 S5**（消費端在那裡；sidecar 可重生零遷移債，YAGNI）——本行同步修正 rows 3/12 的記錄 | implemented |
> Source route: `ep-rust-migration.md` v1+ 條款（B1/B2 圖引擎研究→user 裁決；SCIP 邊注入
> graph.db NT 861 缺差→0；CRG MCP 退役）。User 2026-08-26 定調端局設想：
> 「高度整合 rust+python 的 rust 強化版 CRG，利用 CRG＋scip-callgraph」。

## 背景

- R2-R7 完成 code-reality 自家工具鏈 Rust 化；**圖層資料基底仍是 CRG graph.db**
  （snapshot/graph_audit/graph_csv/hub_refs/hazard/chain_tour/common 七模組讀取）。
- ai-rules 端 cutover 評估（`ai-analysis/reports/code-reality-cutover-plan.md`，ai-rules repo）
  結論「分層互賴、不可停 CRG」——本 EP 是把該「依賴」轉成「互補＋逐段自建」的行動弧。

## POC 證據（2026-08-26，NT 語料）

產物：`crates/code-reality/examples/scip_edge_poc.rs`＋`scip_engine_poc.rs`（**已入庫
`711d6e8`**）＋`.agent-tmp/poc-scip-injection/`（現況＝`compare.py`＋
`crg_calls.tsv.gz`〔注入前純淨基線〕＋run.stderr；a1_*.tsv 已刪——example 重生約
一分鐘；`compare.py` 讀未壓縮 `crg_calls.tsv`，重跑前先 `gunzip`）。SCIP 面＝
`callers::attribute` 真實歸屬邏輯全量跑（is_def=0 occurrence×span containment）。

| 面 | 數值 |
|---|---|
| SCIP reference 站點（列） | 677,197（全部 .rs；去重 (file,line) 後 423,672） |
| SCIP 邊（fn 歸屬後） | **393,609**（另 9,831 站點在 fn span 外＝item-level） |
| CRG `CALLS` 邊總數 | 678,829（非 .rs 佔 30,991——CRG 是多語言） |
| CRG .rs CALLS：distinct 邊／distinct 站點 | **412,689**／467,859 |
| CRG .rs 站點被 SCIP 覆蓋 | 68.7%（exact）→ **80.2%（±1 行容忍）** |
| CRG-only 站點 | **92,785（19.8%）** |
| SCIP-only 站點 | 102,396（引用寬度：型別/欄位/attr 參考，非 call） |

**CRG-only 缺口定性**（樣本實查）：macro 重災區——`include_str!` const 初始化、
`criterion_group!`/bench 巨集、多行呼叫引數位置。tree-sitter 見原始語法即記邊；
rust-analyzer 對這些位置不發（或不同形態）occurrence。

**語義事實（不可繞過）**：本語料的 scip crate（0.9.0，rust-protobuf）是**舊 schema**——
`relationships` 只在 `SymbolInformation`（implements/type-def 級），`Occurrence` 無
`symbol_relationships`、無 `is_call_reference` → **call-only 邊在此 index 不可得**。
occurrence＋containment 即 SCIP callgraph 標準做法（scip-callgraph 參考實作同樣不用
relationships）。若未來 rust-analyzer/scip 升級 schema，call-only 面可重評。

**POC 結論**：兩向缺口、互補而非取代——「SCIP 注入補 CRG」成立（節點面 861＋SCIP 邊面），
「SCIP 取代 CRG 解析層」不成立（19.8% CRG-only）。**聯集模型**。

**POC2 引擎面（同日）**：393,609 邊純 std 自建引擎——鄰接表 319ms、closure BFS ≤9ms、
hub 排序 1ms；closure 語義**精確重現** CLI anchor（雙 def 種子 → depth1=15 new＋
reentries 1／depth2=0，逐項命中）。兩個 S3 設計註記：closure 種子面需 callee∪caller
聯集鍵（0-refs trait impl 不在 callee 面）；hub 榜首全是 std/core 符號（unwrap 9,550）
——引擎查詢需 workspace-scoping 過濾外部符號。

## 段落

### S1 — SCIP 邊注入（形態裁決：(a) → **(A) sidecar**；REFERENCES gate 已結）
> **審查修正（🔴 finding 1）**：原句「(a) 即 CRG 既有架構形態（單一物化圖、全部下游
> 立即受益）」不再成立——兩類讀者實況（全數實查）：
> - **code-reality 讀面**：只認 IMPORTS_FROM/CALLS/INHERITS（`common.rs:17`；
>   snapshot/graph_csv 以 kind 過濾、hub_refs/hazard/chain_tour 只查 nodes）——
>   REFERENCES 注入行**隱形＝零受益**（直至聯集引擎面上線）。
> - **CRG engine**：**不隱形**——impact 權重 REFERENCES=0.6（constants.py:62）、
>   dead-code 判決讀 REFERENCES（refactor.py:531；其 REFERENCES 語義＝
>   function-as-value，與 SCIP 全引用面 grain 不匹配）、communities
>   `get_all_edges()` 全 kind 消費（communities.py:815,1042）——全量注入必改變
>   CRG 查詢結果。
>
> **裁決（2026-08-26，judge 委任裁決——user 可推翻；原 ⚠️ 單向門結案）＝(A) 邊面
> 走自有 sidecar**。依據：(a) 動機「全部下游立即受益」雙向否證（零受益＋未裁決
> 漂移）；CRG 查詢無 tier 過濾（實查：`confidence_tier` 全包僅 DDL/寫入/序列化）
> ＝tier 護欄救不了 CRG 讀面；50x 體量（REFERENCES 7,833→~40 萬）＋rebuild 抹除
> ＝CRG 輸出變「有沒有跑注入」的狀態函數＋雙寫者維運稅。(B) 不採——漂移量化是
> 為寫入別人擁有的 db 蒐集污染證據，無現行受益者。已排除：「CRG 不消費的 kind」
> 不存在（communities 全 kind）。**統一 graph.db 端局不變**——移到 S4 所有權翻轉
> 門後（code-reality 擁有 db 時，REFERENCES 語義由自有 engine 設計決定）。

1. example 轉正式工具（`scip_edges` 匯出：caller/callee/站點 TSV 或 sqlite）。
   **寫入面＝`scip_edges --inject [--dry-run]` 唯一入口**（umbrella 路由＋cli.rs
   FLAGS 註冊；新模組 `crates/code-reality/src/scip_edges.rs`；寫入目標＝**sidecar
   union-edge db**（(A) 裁決）——common.rs 讀面恆 `SQLITE_OPEN_READ_ONLY` 不變，
   寫入面與 graph.db 讀面分離＝分層邊界聲明；graph.db 邊面零寫入）
2. **qname 映射契約（最大未列工作項，審查 🟡 補）**：SCIP scheme
   （`rust-analyzer cargo ...`）↔ CRG qname（絕對路徑 `.../file.rs::fn` 形態）——
   原樣注入則 communities `qn_to_idx` 全 drop（隱形成真、受益歸零）；映射器
   雙向（注入用＋聯集查詢用），為 S5 共用面具體化（2026-08-26 build 裁決：S1 僅存 SCIP symbol，映射器與雙名欄位挪 S5——row 20）
3. 注入設計護欄（(A) sidecar 形態；graph.db 邊面零寫入）：
   - **sidecar schema 自主**：union-edge db 放 sidecar home（`~/.mosaic/code-reality/
     scip/<repo>/`，cache/fndefs 機制現成）；自有 UNIQUE 鍵（二審 🔴：CRG edges 無
     UNIQUE 約束、`upsert_edge` 比對鍵不含 tier〔graph.py:235-266〕——自有 schema
     不遷就 CRG upsert 語義）；行存**雙名並存（挪 S5）**（SCIP symbol＋映射 qname——映射
     壓力從「必須無損」降為「查詢便利」）＋`kind='REFERENCES'`（語義軸）＋
     `provenance='SCIP'`
   - 全量注入（非 delta-only）：聯集查詢（S3 引擎／MCP）自取全量
   - 外部符號過濾：兩端點皆 workspace 可解析才注入（std/core 留匯出面——POC2
     hub 榜首全是 std 符號）
   - 冪等 upsert＋`updated_at` 過期掃除（自有設計；CRG 慣例是 file-scoped DELETE
     ＋重插，不沿用）
   - **前置（二審補）**：SCIP sidecar 在場＋新鮮度 gate（沿用 scip_refs WARN 語義；
     實測 index 8/24 比 graph.db 8/25 舊）；邊界情況（graph.db 缺失／cache 缺失）
     沿用讀面失敗模式（邊面新鮮度 gate 由節點面承擔——邊面隨 index 全量重生；節點面 WARN 已落地）
   - 排序契約隨邊面消失（sidecar 單寫入者、免 1.6GB backup）；S2 節點面自守
     CRG 寫入後重跑（見 S2）
4. **S1 場景表（最小）**：
   | 場景 | 觸發 → 預期 |
   |---|---|
   | 首次注入 | `--dry-run` 報行數 → 實注冪等鍵落地 |
   | 複注入 | 零淨增（upsert 冪等） |
   | index 重生 | stale-sweep 掃除離開 index 的舊 SCIP 邊 |
   | 回滾 | 刪 sidecar 檔即回滾——graph.db 全程不受影響 |
   | 注入後 graph_audit | 計數穩定（missing 走 S2，不受邊注入影響；(A) 下結構性成立——scip_edges 不開 graph.db，免測） |
5. **測試（最小不變項）**：冪等複注入零增長；刪 sidecar 重注冪等；外部符號過濾
   （std/core 排除率）；`scip_edges` 匯出正確性（對拍 example 產物）
6. 驗收：NT `graph_audit --json` missing 861→0（節點面走 S2，同批結算）；sidecar
   邊數對帳（COUNT＝匯出面行數，外部符號過濾率進報告）；CRG-only 92,785 不在注入範圍（清單可由 compare.py＋example 從 crg_calls.tsv.gz 重生——見 POC 段）

### S2 — 節點面 861 收斂
graph_audit missing 名單 → SCIP 符號 → graph.db nodes 注入（名稱對映規則＝既有
`scip_refs --audit` 對帳邏輯）。與 S1 同批驗收。
（build 註：原「同步落 id 清單檔」由 extra marker 精確匹配取代——marker 即可
還原 id 集，清單檔冗餘；跨庫監測項：CRG 若對同 qname upsert 改寫 extra 會洗掉
marker——「CRG 寫入後重跑注入」慣例同時恢復 marker 與資料。）

審查補強（🟡）：nodes 表無 provenance 欄位（schema 實查）——
- **回滾**：注入節點 `extra` 塞 `{"tier":"SCIP"}`（回滾＝extra 標記 DELETE，
  注入時同步落 id 清單檔）；backup 仍為最後防線
- **碰撞**：`qualified_name UNIQUE`——存在即 **skip**（保守，不 upsert 原生列），
  碰撞計數進注入報告
- **次序**：邊面已走 sidecar（(A) 裁決）——graph.db 僅節點面，無懸掛邊顧慮；
  sidecar 注入與節點注入同批跑（驗收一次結算）。S2 注入器＝graph.db **唯一
  寫入面**（與 common.rs 讀面分離）
- **rebuild 語義（二審補）**：任何 CRG 寫入（build／incremental file-scoped
  remove）後重跑節點注入——upsert on qualified_name 與原生列共存；被 remove
  清除者重跑冪等恢復

**NT L4 實測（2026-08-26，build 段結算）**：861 → **596**（inserted 330、淨封閉
265 項；analytic residual 與 live re-audit 兩次精確一致）。殘餘 596 為**結構性**
，非注入器缺口——①71 項 mapped-but-qname-occupied（雙鍵命中但 `<abs>::<name>`
節點已存在，如 CRG 的 `Type.method` 形態佔位）；②525 項 index-universe 外
（離線重現精確命中：capnp 生成檔 `data_capnp.rs` 在 index **整檔零 occurrence**
——rust-analyzer SCIP 不含該檔；test cfg 變體、trait-impl 同名多實例如
`from(2/200)`）。qname UNIQUE 的節點模型表達不了同檔同名多實例——**861→0 在
此設計下不可達，驗收修訂為「可封閉集歸零」**（複跑 injected=0 已證封閉集耗盡；
殘餘清單在 `graph_audit --json` missing 面永久可查）。另：backup 改 `VACUUM
INTO`（`fs::copy` 對 WAL-mode db 不健全——實測裸拷開不了 readonly）。

### S3 — B1/B2 圖引擎裁決報告（研究段，不改碼）— **完成（2026-08-26）**
- communities：Rust 生態評估（community-detection crate / Leiden port）vs 沿用 CRG Python 計算
- impact radius／flows：自建 BFS on（聯集）邊集的成本（邊集已在 Rust 手上）
- semantic search：embeddings 面缺口（最大未覆蓋項，明確標記）
- 產出：裁決報告＋user 單向門（哪些收進 Rust、哪些永久留 CRG/Python）

**S3 結算（2026-08-26）**：報告＝`ai-analysis/reports/s3-graph-engine-adjudication.md`
（English，數字全帶 index 版本標籤）。**重框事實**：MCP server 是 base install（uvx，
無 extras）→ igraph／sentence-transformers 從未在場——live communities＝directory
fallback（NT 42 個全是 "Directory-based community" 指紋、33.5k 巨型社區無法分裂）、
semantic search＝FTS keyword fallback（embeddings 0 行）。「Leiden／embeddings 遷移」
實為「採納 CRG 只以 optional extras 形式供貨的功能」。**委任終裁（user 可推翻，報告 §5）**：
①impact／flows／hub／bridge→Rust 自建（純演算法移植，POC2 current-index 重跑 349ms/8ms/1ms
背書；flows 對拍＝NT 10,359 條 exact-match）；②communities→兩層：Tier 0 directory 對拍
（零 crate、exact-match）＋Tier 1 seeded Leiden（single-clustering 優先：BSD-3＋bit-for-bit
seed；leiden-rs 備選；對拍＝modularity≥igraph 參考−ε＋ARI/NMI 門檻，igraph 走一次性 venv）；
③semantic search 拆面——keyword 面 Rust 即刻對拍（live 實況就是 FTS）、embeddings 面
**明確 defer**（本機零使用；cloud-HTTP 為未來廉價路徑）。**閘門**：聯集邊消費全體
（impact/flows/communities-union）依賴 S5 qname 映射；邊集語義留在 S4 所有權翻轉門後
（S1 裁決延續）。**S4 含義**：Doors ①-③ 採納後，CRG 僅餘 graph.db producer（tree-sitter
多語言）＋deferred embeddings——引擎層可退役，S4 範圍＝「引擎退役＋graph.db 所有權翻轉」
而非全面移除。

**User 單向門裁決（2026-08-26）**：「你就至少做到 CRG 的功能」＝**門①②③採納，parity
為底線**（Tier 1 Leiden 與聯集邊升級＝其上增量）。Parity 範圍＝**10 個 live 消費 MCP
操作**（§2 對拍表）；CRG 未被消費的面（visualization/wiki/daemon/refactor/exports）
默認 out of scope。**後續排序**（S4 不是下一步，是最後一關）：
1. **引擎 parity build**（新 session＋子 EP——本 EP 是裁決弧，build 走 execution-plan
   流程）：impact_radius／flows（NT 10,359 exact-match）／hub／bridge／communities
   Tier 0（42 社區 exact-match）＋Tier 1（single-clustering）／keyword search
   （nodes_fts read-only）＋MCP 工具面。**parity 段不依賴 S5**——graph.db-only
   邊原生 qname，無映射需求。
2. **S5**（qname mapper 解鎖聯集邊消費＝超越 CRG 的增量；scip-python POC）。
3. **S4**（parity 落地後）：消費端切換（ai-rules crg-query 等）→ CRG MCP 下線 →
   graph.db 所有權翻轉（REFERENCES 邊語義歸自有 engine）。

### S4 — CRG MCP 退役評估（S1-S3 後；條件式）
僅當 S3 裁決「CRG 獨有面已無消費者或已有替代」才啟動；否則維持分層互賴現況。

### S5 — Python producer（多語言擴展；研究＋POC）
Python repo（mosaic/ai-rules）符號真相目前只有 LSP/pyright——本段補 producer 面，
消費面全共用。內部合約是 row 三元組（defs/occurrences/fn-spans）而非 SCIP
protobuf 本身；SCIP 只是 rust-analyzer 的序列化形態，換 producer 不換管線。

- **P1 主路徑：scip-python**——Sourcegraph fork of pyright emitting SCIP
  （pyright 級解析＋SCIP 輸出＝既有引擎零改動；倉庫未封存，維護節奏未驗）。
  POC：index ai-rules → 引擎解析 → refs 抽查對拍 pyright LSP（R2-R7 parity
  oracle 方法論重演）
- **P2 備援：通用 LSP-harvest adapter**——ty（astral，Rust 原生，已支援
  workspace-wide `textDocument/references`/rename）或 pyright-langserver，
  協議同形、adapter 一份
- **P3 結構邊：ruff_python_parser 原生 indexer**（已在 Cargo workspace
  `0.0.10`，hazard/boundary_build 已用）——CRG tree-sitter 的 Rust 替代，
  服務 S4 退役線；Python 無 macro，CRG-only 缺口主因不存在
- 共用面：engine 解析（scip crate 語言無關）、歸屬/closure/hub、fndefs/cache、
  注入機制（tier/kind/sweep）、qname 映射框架、CLI/MCP 面。不共用者僅 producer
  binary 本身（adapter 層定義）。外部符號判定 Python 較簡（site-packages 路徑
  判定 vs SCIP symbol scheme）
- 審查補強（🟡）：P1 POC 判準須**量化**（refs 抽查樣本數＋一致率門檻，POC 設計時
  釘死——R2-R7 parity oracle 的 pass-bar 慣例）；ai-rules 無 CRG graph（實查），
  POC 對拍走 pyright LSP 面，不涉 CRG

## 驗收彙總

- S1/S2：**可封閉集歸零**（NT 實測 861→596，殘餘結構性——見 S2 L4 段）＋
  sidecar 邊數對帳（182,137＝edges_workspace）＋形態裁決紀錄（(A) sidecar，
  S1 裁決塊）
- S3：裁決報告（含 POC 數字與本檔證據段引用）
- 回退：邊面＝刪 sidecar（graph.db 不受影響）；節點面＝extra 標記 DELETE＋
  backup（首次節點注入前完整複製 graph.db，最後防線）

## 收尾步驟（審查補）

- S1 build 完成 → AGENTS.md Capabilities 加 `scip_edges` 行（匯出面＋`--inject`
  sidecar 寫入語義標註）＋.kanban 建卡（沿用父 EP 慣例）
- crates/AGENTS.md：S2 節點注入器成為 graph.db 唯一寫入面——「CRG graph.db
  reads are read-only」陳述修訂（加注入器例外或改述）
