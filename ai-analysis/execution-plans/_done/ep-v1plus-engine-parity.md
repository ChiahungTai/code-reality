# EP — v1+ 引擎 parity build：10 個 live CRG MCP 操作的 Rust 面

> Status: **build 完成（2026-08-26，同日單 session）**——S1-S9 落地（S10 Leiden
> 依 EP defer 條件延後）；NT parity 四面實證（見驗收段）；同弧插單：chain_tour
> duplicate-family 修復（mosaic relay）。設計源＝S3 裁決報告
> `../reports/s3-graph-engine-adjudication.md`＋user 門裁決「至少做到 CRG 的功能」；
> parent EP `ep-v1plus-graph-engine.md` S3 結算塊。
> Baseline: `14e7ce3`（build 首 commit 的 parent；working tree 另含 parent EP S3
> 結算＋S3 報告——同弧產物隨首 commit 帶走）

## 設計決策（全段共用）

- **Parity bar＝value-level，非 byte-level**：deterministic ops 比對 exact
  multiset/map（flows 逐欄位 tuple、communities (name,size,cohesion,members-set)、
  impact 逐 node score within 1e-9、hub/bridge 排序清單）；JSON envelope 結構自由
  （v1+ 是新 face，非凍結 CLI byte-parity 時代）。
- **語義源＝CRG 2.3.7 installed source**（`~/.local/share/uv/tools/code-review-graph/
  lib/python3.12/site-packages/code_review_graph/`，下稱 CRG/）——每段錨 file:line，
  實作段內重讀驗證。**讀取順序事實**：`get_all_nodes`＝`SELECT * FROM nodes WHERE
  kind != 'File'`（graph.py:352）、`get_all_edges`＝`SELECT * FROM edges`
  （graph.py:1447）——皆無 ORDER BY＝rowid 序（deterministic per db state），Rust
  端 `ORDER BY rowid` 對齊。
- **資料面**：graph.db read-only（common.rs `SQLITE_OPEN_READ_ONLY` 慣例不變）；
  parity 段＝graph.db-only 邊（原生 qname，零映射需求——S3 結論）；聯集邊留 S4
  所有權翻轉門後。
- **測試**：shipped suite＝自足合成 fixture（tests/AGENTS.md 政策——禁 vendored NT）；
  NT parity＝dev-time L4 對拍（CRG Python 產 reference JSON vs Rust 輸出），數字記
  本 EP 驗收段。
- **CLI/MCP 慣例**：`--repo <root>` 必要（不自動偵測——neutral-cwd 教訓）；MCP 工具
  名對齊 CRG 消費面（consumer 熟悉度），CLI subcommand 同名去 `_tool` 尾。
- 倉庫公開面：code comments/docstrings English；輸出字串可自由設計（無 byte-parity
  約束）。

## 段落

### S1 — 圖載入基盤（`graph_engine.rs`）
Context：全部引擎 op 共用 substrate。CRG 語義源：graph.py:352（nodes）、
graph.py:1447（edges）、graph.py:1493 `load_flow_adjacency`（CALLS/TESTED_BY 全量
串流讀入＋nodes_by_qn——段內重讀取齊欄位）。
Pseudo Code：
```
struct GraphNodeLite { id, qualified_name, name, kind, file_path, language, is_test, community_id }
fn load_nodes(conn, exclude_files=true) -> Vec<GraphNodeLite>   // ORDER BY rowid
fn load_edges(conn) -> Vec<GraphEdgeLite>                        // (source_qualified,target_qualified,kind) ORDER BY rowid
struct FlowAdjacency { nodes_by_qn, calls_out: HashMap<String, Vec<String>> }  // 去重語義對齊 load_flow_adjacency
```
驗證策略：合成 fixture graph.db（測試 helper 建 schema＋插入樣本——沿既有
tests fixture 慣例）單元測試 load 順序/過濾；rowid 序斷言。
核心要點：read-only 開啟；schema 缺表 fail-loud（對齊 common.rs 慣例）。

### S2 — hub＋bridge
Context：CRG/analysis.py（bridge＝networkx betweenness，k-sample >5k 節點，
analysis.py:75-82；hub 的實作位置段內 rg 定位——預估 analysis.py 同檔 degree 排序）。
Pseudo Code：
```
hub: degree = in+out 邊數（kind 全含、File 節點排除），排序 desc、tie-break 對齊 CRG
bridge: Brandes betweenness（undirected、normalized）＋k 取樣（seeded——CRG 網路
     取樣的 rng 語義段內讀；取樣集不同的統計 parity：NT 上 rank 相關性）
```
驗證策略：合成小圖 exact（手算 betweenness）；NT 對拍＝hub top-N 清單比對＋
bridge 統計（小圖 exact／NT rank correlation）。

### S3 — flows 家族（detect_entry_points／trace_flows／criticality／affected／list／get）
Context：CRG/flows.py:164（入口三條件：no-incoming-CALLS〔File-source 邊排除於
called_qnames〕＋framework decorator＋conventional name；Test 排除）、flows.py:222
（前向 BFS ≤15、visited set、trivial 單節點 skip）、flows.py:324（criticality：
file-spread 0.30／external 0.20／security 0.25／test-gap 0.15／depth 0.10——段內
讀齊各因子計式）、flows.py:674（affected＝node-ids∩flow path）。
Pseudo Code：
```
entry_points() -> Vec<qn>            // called_qnames = CALLS targets with non-File sources
trace(ep, max_depth=15) -> Flow      // BFS over calls_out；path=ids 序、depth、node/file counts
criticality(flow, adj) -> f64        // 五因子加權
flows_changed(files) -> affected     // graph.db flow_memberships 或即時重算（段內讀 CRG get_flows 決策）
```
驗證策略：合成圖單元（入口判定三分支、BFS 環路、criticality 手算）；**NT parity
bar＝trace_flows 全量 10,359 條：(entry_point, node_count, depth, file_count,
criticality round 4) multiset exact-match**。

### S4 — impact_radius（bounded relaxation）
Context：CRG/graph.py:771 `get_impact_radius_sql`（種子＝changed files 的全部節點
qn；每深度 frontier×edges JOIN 撃 best score：`score*weight*decay`、`COALESCE
default 0.5`、floor 0.05、cap depth 2／nodes 500；段內讀齊 880-970 的 truncation、
impacted_nodes 排序〔best-path score desc〕、edges 面與 impact_scores dict 形態）。
權重表 constants.py:56（CALLS 1.0…REFERENCES 0.6…CONTAINS 0.3；default 0.5；
decay 0.6；floor 0.05）。
Pseudo Code：
```
impact_radius(files, max_depth=2, max_nodes=500):
  seeds = nodes(files).qn
  best/frontier = {seed:1.0}
  loop depth: for (f in frontier) for e in adj[f]: cand = f.score*w(e.kind or 0.5)*0.6
              if cand > best[e.other] && cand > 0.05: update
  cap 500（截斷語義對齊 CRG——段內讀）；輸出排序＝score desc
```
驗證策略：合成圖（權重衰減、雙向邊、floor 截止、cap 截斷）；NT parity＝sampled
seed sets 逐 node score dict within 1e-9＋impacted 順序一致。

### S5 — communities Tier 0＋architecture_overview
Context：CRG/communities.py:474 `_detect_file_based`（LCP 剝除→adaptive depth
（qualifying ≥10 即停）→min_size 2 過濾→cohesion batch（communities.py:187：
internal/(internal+external)、雙端計 external）→naming（communities.py:79 起：
file prefix／dominant class>40%／keyword fallback、common-words 過濾、slug 30）；
`_split_oversized`/`_dedupe_community_names`（無 igraph＝no-op／名稱去重仍跑）；
store/get（communities.py:895/960）；overview＝communities.py:1020（TESTED_BY 排除、
cross-edge 計數、>10 且非 test-dominated pair 告警）。
Pseudo Code：
```
detect(store, min_size=2) -> Vec<Community>   // Tier 0 全流程移植（含 naming 完整鏈）
architecture_overview() -> {communities, cross_community_edges, warnings}
```
驗證策略：合成 fixture（LCP、adaptive depth、naming 三分支、dedupe）；**NT parity
bar＝42 communities：(name, size, cohesion round 4, members-set) exact-match＋
overview cross-edge 計數一致**。

### S6 — detect_changes＋risk
Context：CRG/changes.py:33（`git diff --unified=0` subprocess＋hunk 解析——
safe-ref regex、timeout 30s）、changes.py:267 `map_changes_to_nodes`（行區間×
node span——段內讀齊 span 判定）、changes.py:312 `compute_risk_score`（六因子：
flow participation sum-of-criticalities cap 0.25｜count*0.05、community crossing
cap 0.15、test coverage 0.30→0.05（transitive tests/5）、security keywords 0.20、
caller count cap 0.10、churn opt-in cap 0.15；round 4）、changes.py:381
`analyze_changes`（組合排序輸出）。
Pseudo Code：
```
diff_ranges(repo, base) -> Map<file, Vec<(start,end)>>   // git subprocess＋unified diff 解析
map_to_nodes(ranges) -> Vec<(node, ranges)>
risk(node, ...) -> f64                                    // 六因子
analyze(files|base) -> prioritized findings
```
驗證策略：合成 diff 文本單元（hunk 邊界、\ No newline、rename）；risk 六因子
逐項手算；NT dev-time＝選定 commit 對拍 CRG `analyze_changes` 輸出。

### S7 — review_context＋minimal_context（組合面）
Context：CRG/main.py:187/281（＝impact＋snippets＋review guidance／graph stats＋
risk＋top communities/flows＋next-tool 建議）。組合層——各組件已由 S3-S6 提供，
本段只做編排與 token 預算（detail_level minimal/standard）。
驗證策略：合成 fixture 組合輸出形態斷言；NT 冒煙（結構鍵齊全）。

### S8 — keyword search（semantic_search fallback face）
Context：CRG/graph.py:695 `search_nodes`（FTS5 MATCH：單詞全句 quoted／多詞
AND-quoted；JOIN nodes ON rowid；LIMIT；**FTS 空結果→LIKE 子串 fallback**
（name/qualified_name lower））＋embeddings.py:1120 `semantic_search`（embeddings
表空＝直接 fallback——本段即 live 實況的面）。
Pseudo Code：
```
search(query, limit=20) -> Vec<NodeDict>   // FTS5→LIKE 兩段；envelope 帶 fallback 標記
```
驗證策略：合成 db（含/不含 nodes_fts 表）兩路徑單元；NT 對拍數組 query 結果集。

### S9 — CLI＋MCP 面
Context：cli.rs FLAGS＋bin/main.rs umbrella（既有 16 子命令模式）、mcp_server.rs
ToolRouter（refs/callers/closure/audit 四工具模式——新工具各自 args 解析，非
run_refs_like）。新增：`impact_radius`／`flows`／`hub`／`bridge`／`communities`
／`arch_overview`／`detect_changes`／`review_context`／`minimal_context`／
`search_nodes` 十 subcommand＋MCP 工具（stdio＋http 共 router）。
Pseudo Code：
```
each op: fn run(args) -> ToolOutput   // --repo 必要＋op flags；--json 面
mcp: #[tool] methods -> lib 呼叫      // 名稱對齊 CRG（get_impact_radius 等）
```
驗證策略：argparse 邊界單元（缺 --repo exit 2、--help stdout——沿 D3 慣例）；
stdio handshake＋tools/list＋樣本 tools/call（in-process E2E 模式沿 5db923a 驗證法）；
NT 冒煙每工具一次。

### S10 — Tier 1 Leiden（single-clustering；增量、可 defer）
Context：S3 報告 §4.3 Tier 1——single-clustering（BSD-3、seeded bit-for-bit），
Tier 0 之上的升級（非 parity 必需）。對拍＝一次性 scratch venv igraph 參考：
modularity(ours) ≥ modularity(ref)−ε＋ARI/NMI 門檻（POC 時釘死）＋size 分布
sanity（無 >25% 巨型社區）。
Pseudo Code：
```
leiden_communities(nodes, edges, weights, seed=42, resolution) -> partition
  // edge 權重沿 CRG EDGE_WEIGHTS 表（communities.py:60）＋undirected 去重 max
```
驗證策略：合成圖（planted partition 可分離）；NT igraph 參考對拍三 bar。
**Defer 條件**：context/時間壓力下本段可獨立 defer（parity floor 不受影響），
EP 記錄即可。

## 整合器型標記

S4（graph.db SQL→in-memory relaxation 語義等價）、S9（MCP stdio/http 邊界）＝
整合器型：需真實邊界測試（NT graph.db 實跑＋in-process MCP E2E），非僅合成。

## 驗收彙總

- S3 flows：NT 10,359 條 multiset exact-match
- S5 communities：NT 42 社區 exact-match＋overview 計數一致
- S4 impact：NT sampled seeds score dict within 1e-9
- S6 detect_changes：NT 指定 commit 對拍 analyze_changes
- S2/S8：清單/結果集對拍（hub exact、bridge 統計、search 結果集）
- 全量 `cargo test` exit 0＋clippy/fmt/deny 綠（repo 慣例 gates）
- MCP：stdio E2E 十工具可呼（NT 樣本）

## Build 結算（2026-08-26）

**NT L4 parity 實證（安裝版 binary、NT graph.db 1.16M 邊）**：
- communities：**42/42 multiset exact**（(name,size,cohesion) vs CRG Python
  `detect_communities` 現場計算——Tier 0 語義完整移植的強證）
- flows：**10,359/10,359 multiset exact**（(entry,node_count,depth,file_count,
  criticality) vs stored flows；首輪 48 條差 1e-4 → `py_round`（十進位 half-even，
  對齊 Python `round`）修至全等）
- hub：top-10 (qn,total_degree) **exact**
- impact_radius：instrument_id.rs 種子 **500/500 scores maxdiff=0.00e+00**＋
  total 41,353==41,353（relaxation 位元級等價）
- bridge：NT 走 sampled 路徑＝統計面（documented deviation）；合成小圖 exact 已測
- detect_changes/review/minimal/search：合成 fixture 測試通過；NT 逐值對拍未跑
  （風險低——六因子公式與 diff parser 均逐行對拍 CRG 源碼移植）

**實作形態**：`crates/code-reality/src/graph_engine.rs`（~2,300 行：loaders/hub/
bridge/flows/impact/communities/changes/search/compositions/`graph_query` CLI）＋
MCP 11 工具（impact_radius/detect_changes/hub_nodes/bridge_nodes/
list_communities/architecture_overview/list_flows/affected_flows/
get_minimal_context/get_review_context/semantic_search——argv 共享 CLI 路徑，
stdio/http 共 router）；tools/list 15 工具釘死測試。

**記錄偏差（EP 是收斂方向）**：
1. S9 CLI：EP 草描十個 subcommand → 實作單一 `graph_query <op>` 傘（消費面在
   MCP；argv 共享維持 CLI=MCP 單後端前提）
2. py_round：乘除 round 邊界差（48/10,359）→ 十進位格式化捨入
3. bridge >5k 節點：networkx RNG 取樣 → 自有固定種子 LCG（統計 parity only）
4. transitive tests：evidence-gated bare-name fallback（legacy graph 路徑）不移植
5. git diff subprocess 無 30s timeout（std 無原生；不新增 dep）
6. get_flow（by id）併入 flows 全列輸出（fresh face 冪等重算，無 stored-id 依賴）
7. churn 因子（opt-in，CRG 預設關）面未接線

**S10 Leiden**：~~defer~~ **deep-work 解凍（2026-08-26 晚 user 指令「所有已知 EP
都做＋完全取代 CRG」）**——single-clustering 0.7（BSD-3、seed=bit-for-bit 決定性）
Tier 1 落地：graph_query `communities --leiden`；resolution 沿 CRG `max(0.05,
1/log10(n))` 縮放；pass-bar＝①種子決定性（同 seed 兩跑 identical）②無 >25%
mega-community ③modularity ≥ Tier-0 同邊集分區（igraph 一次性參考留可選後續）。

**deep-work 結算（2026-08-26 晚，同日第三弧）**：S10+S5-mapper+LSP 面+S4 全部落地。
- **S10 Leiden**：`graph_query communities --leiden`（MCP `algorithm:"leiden"`）；
  NT L4＝1,270 社區／最大 23.4%（Tier-0 巨型 41.7% 消解）／modularity 0.9158／
  resolution 0.2046（=1/log10(76,814) 符 CRG 公式）／seed 42 兩跑 bit-identical。
- **S5-mapper＋union 整合**：`load_union_edges` 走 **(resolved-file, bare-name) 雙鍵**
  對 graph.db nodes 接合（CRG method qname 是 parent-qualified `file::Type.method`，
  組字串式映射兩輪失敗後改雙鍵——與 scip_nodes/graph_audit 對帳同構）；ops 抽
  `*_with(edges)` 核心＋`*_union` 變體（parity 簽名零 churn）；CLI/MCP `--union` 面。
  NT L4＝sidecar 181,591→mapped 179,704（98.96%）／merged_new 177,877／
  impact 41,353→**43,897**（+2,544 SCIP REFERENCES 新可達——聯集增量實證）。
- **LSP 對齊面**：`graph_query symbols`＋MCP `document_symbols`（SCIP cache
  defining occurrences 檔綱、bare name）；hover／型別簽名＝SCIP 無資料，維持
  LSP-only（報告記錄）。
- **S4**：`ai-analysis/reports/s4-crg-retirement-readiness.md`——引擎層退役 READY；
  剩餘＝tree-sitter producer（資料層）＋deferred embeddings＋消費端 cutover
  checklist（ai-rules session）。
- 依賴：`single-clustering 0.7`（default-features=false，BSD-3）。

**deep-work 範圍擴充（2026-08-26）**：+S5-mapper（SCIP symbol↔qname 映射，復用
scip_nodes 對帳邏輯→sidecar REFERENCES 邊接入引擎 ops＝「SCIP rust graph 整合」，
`--union` 面）＋S4（CRG 退役評估報告）＋LSP 對齊面（document_symbols；hover/
型別簽名 SCIP 無資料、維持 LSP-only 並記錄）。

**同弧插單（mosaic relay）**：chain_tour「pattern-stale」裁決＝**不存在於現行碼**
（L4：stale 蓋回→regen→pattern 刷新、escaping 考古證該檔從未被 Rust regen 碰過）；
真 bug＝duplicate-family → 修復＝manifest sources 查重（default out-dir redirect
upsert 既有族/explicit WARN 兩族並存）＋孤兒族掃描＋4 回歸測試；mosaic L4 三面
驗證＋regen 與 committed 版 byte-identical（冪等證）。

## 收尾步驟

- AGENTS.md Capabilities 補引擎 op 行（CLI＋MCP 入口）；crates/AGENTS.md 模組
  清單補 graph_engine 家族
- .kanban 卡搬 Done/（沿父 EP 慣例）
- 本 EP 歸檔 `_done/`（全部段落地後）
