# EP：source identity——dirty-WT/content-aware freshness

> **ep_type**: implementation
> **tier**: full（freshness 語義＝public contract 變更；新 boundary：identity 演算法成為跨 repo 消費端可比對的契約面）
> **date**: 2026-09-23
> **baseline**: `68d763601ca79ec5531737444615934318ab0f9e`
> **author_family**: glm
> **上游**：ai-guide 開題信（scbus message_id `4b130431…`，非 git sha；驗收位置＝雙方 scbus 信箱 receipts＋本 repo `.agent-tmp/scbus-intent-reply-source-identity.md` 備份）→ 意向回執 `6740873b` → 消費端 GO 回執 `b7fa1f7c`；不變量定義源＝ai-guide `ai-development-guide.md`「AIR-135 協作 invariant」條 2：`fresh ⇔ indexed_source_identity == requested_consumer_source_identity`（consumer identity 含 dirty WT/content，HEAD-only ≠ fresh）
> **下游驗收**：ai-guide review-engine preflight 的「宣稱 fresh」消費能力（跨 repo 主權：mutation 歸 CR、acceptance 歸消費端）；錨點＝ai-guide `skills/review-engine/SKILL.md`「CR freshness preflight」
> **研究材料**：[references/research.md](references/research.md)（完整錨點＋negative-claim register＋open questions 裁決）

## 實作總覽

現行 index 軸 staleness＝mtime＋pathset（FNV-1a64 純路徑指紋），內容變更而 mtime 未變（rsync -a／tar 回存／同尺寸原地編輯）時三面訊號全靜默——silent-stale，正是 AIR-135.2 禁止的形態。本 EP 落地 **content-addressed source identity**：

- **身分語義**：`identity = sha256( "cr-identity-v1\0" + Σ sorted by rel "{face}\0{rel}\0{size}\0{content_hash}\0" )`——同內容 ⇒ 同身分（touch/rebase/stash 往返冪等）；mtime **不進**身分本體。
- **cost gate**：per-file content_hash 走 stat-gated incremental cache（`(size, mtime)` 命中即重用）；slot sibling sidecar `identity-cache.json`，atomic-rename 合併寫，損壞即棄置全量重算。**hash 只在 identity 組合點計算（D17 lazy）**——`walk_sources` 本體維持純路徑＋stat（records 於 :630 已 stat 處捕獲 size+mtime，零新增 syscall），路徑面 walk 消費者與 legacy slot 查詢零新增 hash 成本。**stamp/build 路徑全量重 hash**（`IdentityCachePolicy::Full`，D13——indexed identity 永遠從實際 bytes 滿算）。
- **權威翻轉**：stamped slot 的 staleness 決策升級為 identity 比對（`identity_drift`）；torn-plane guard（graph_lags）無條件保留；無 identity keys 的 legacy meta 退化現行判準。決策權威換軸：identity 態下 mtime-newer 不再是 fatal（touch 冪等），identity_drift 取代之——無新 mtime＋identity 漂移＝**真 stale**（rsync 情境）→ 首次查詢即 rebuild；false-stale precise branch 條件本身不變（D18）。
- **消費端 face**：新 CLI 子命令 `code-reality freshness --repo <repo> [--json]`——零 heal 觸發（唯一寫入＝identity cache，D11 政策），exit 0/1/2 三態，輸出 indexed/current identity 對＋`serves` 值域標註（fresh→`current-tree`／stale→`committed-baseline`／legacy→`legacy-signals`）。

成本實測（POC，本 repo＋NT 級語料＋ai-guide；**POC 為同構格式〔無 header/face 欄、排除集略寬〕，格式 pin 歸 TC-1/TC-6，數字代表量級**）：steady-state gate 39.5ms／cold full-hash 495ms／NT 3,418 檔 66.1MB；cache 路徑與全量重算 identity 完全等價。query-time 現行已付 full walk＋stat（research Q1），增量成本僅「changed 檔讀取＋hash」。

## 已決策（勿重辯）

| # | 決策 | provenance |
|---|---|---|
| D1 | 粒度＝content-addressed identity；stat-tuple 降為 cache validity gate；**git machinery 否決**（度量「與 HEAD 距離」非「內容身分」——stash 往返/rebase replay 翻轉 porcelain → false-stale churn；untracked 內容不可見） | 開題信開題先裁項 → 意向回執 6740873b → 消費端接受 b7fa1f7c |
| D2 | hash 演算法＝`sha2` crate（新增 workspace dep），`identity_algo: "sha256-v1"`；FNV-1a64 非抗碰撞不沿用（engine.rs:561-562 自註 NOT a security boundary）；BTreeSet 迭代序保 byte-determinism | research N8＋Q10.6；POC 同演算法實測 |
| D3 | mtime 不進 identity 本體（只進 cache gate）——touch 冪等、同內容遷移目錄冪等 | 消費端接受的「同內容冪等」公式（b7fa1f7c ①）；POC 設計 |
| D4 | cache 落點＝slot sibling sidecar JSON（`<slot_dir>/identity-cache.json`）；sqlite 案否決（query-time 寫打破讀鏈唯讀 face）、meta.json 案否決（per-file 記錄撐爆人讀面） | research Q11 三案取捨 |
| D5 | graph.db metadata keys **不動**（`git_head_sha`/`last_updated` 維持現狀）——identity 單一源＝index stamp 軸；**但 torn-plane guard（graph_lags，engine.rs:821-839，折疊於 source_newer）無條件保留**——identity 態下照樣強制 heal | research Q10.7（單源）＋judge R1（guard 保留） |
| D6 | symlink 不對稱（walk 跳過 vs producer 跟隨）＝**已知邊界不修**——identity 繼承 walk 集；語料定義變更屬另一弧 | research N6 |
| D7 | 消費端 face＝新 CLI 子命令 `freshness`；**MCP 不加**（CLI 權威全量面先例；MCP 膨脹先削工具數） | interface-form MCP vs CLI 裁決（memory: interface-form-mcp-vs-cli-adjudication） |
| D8 | legacy slot 相容：meta 無 identity keys → `identity_drift=None` → 退化現行 mtime+docset 判準（行為零變） | research Q4 `s4_legacy_meta_without_keys_uses_baseline_behavior` 先例同構 |
| D9 | exit semantics：fresh=0／stale=1（verdict face，`tour_validate` FAIL+1 先例）／env·argparse usage=2；無 slot＝fail-loud exit 2＋指引，**禁宣稱 fresh** | crates/AGENTS.md Exit semantics D3（per-tool）；root AGENTS.md「env 類錯 fail(2)」 |
| D10 | stamp 面續寫既有 fingerprint keys（legacy 相容面）；identity keys 為新增權威；staleness 決策 identity 優先、fingerprint 保留為 pathset 快照 | D8 的寫入面對偶；anti-laundering gate 單點繼承（research Q2/Q5） |
| D11 | cache 寫回政策：default-resolved slot 統一寫回（freshness face 與 scip_refs 查詢同政策）；explicit `--index` 查詢與 `CODE_REALITY_IDENTITY_CACHE=off` 完全唯讀（對稱 AUTOHEAL=off 與「explicit --index 永不 heal」） | research Q7 cli.rs:336-339 先例對稱 |
| D12 | identity-cache **不進** producer superseded-sidecar 刪除清單（cache 自描述 version+algo+repo＋hit 只在 stat 全等時，語義自驗）；但空收斂 slot 移除時一併刪除 | research Q8 build.rs:658-662；與 producer 刪除清單的契約精神區辨 |
| D13 | **stamp/build 路徑 identity 計算走 `IdentityCachePolicy::Full`（全量重 hash＋整檔重寫 cache）**——indexed identity 永遠從實際 bytes 滿算；query 端 cache 污染最壞造成 false-stale（安全方向，觸發 rebuild 即淨化）或揭露的 stat-gate 殘餘，**不可能靜默 false-fresh**（build 本就分鐘級，+495ms 為噪聲） | judge R2（intent F2）：S1/S2 原稿 gate 無 caller 區分＝trust anchor 被架空 |
| D14 | **head-drift 不判死**：fresh 判定＝identity（＋legacy 態語料訊號）；head 前移不進 fresh 判定、以獨立欄位 `head_drift` 揭露（對稱既有 `t15_head_drift_only_no_rebuild` 行為；AIR-135.2 LHS 不含 HEAD——內容同則圖事實仍有效）；與 cr-query 現行 HEAD-mismatch 規則的分歧由 ai-guide 線同步（收尾 3） | judge R3；engine t15 pin |
| D15 | **per-file hash 讀取失敗＝fail-loud（Err）**——與目錄級 read-failure 先例（tests/staleness.rs:173 fail-loud pin）及 house crash-only 同向；TOCTOU 暫態（編輯中檔案被換）最壞＝單次「索引過期檢查失敗」WARN 降級。**行為變更揭露**：現行 file-level stat 失敗是靜默跳過（engine.rs:630 `.ok()`），identity 時代收緊為 loud | judge R7 |
| D16 | **cache 硬化**：mtime 記 `(secs, nanos)`（秒級精度下「同秒同尺寸改寫」是日常操作，gate 必須不命中）；cache payload 含 repo canonical path，load 時 mismatch 視同損壞棄置（跨樹移植防護，crash-only 同型） | judge R4/R12 |
| D17 | **hash 計算點＝identity 組合時（lazy）**：`SourceRecord` 不攜 content_hash（walk 記 `(face, rel, size, mtime)`）；per-file hash 與 cache gate 只在 `compute_identity(policy)` 內發生——`walk_sources` 簽名與成本面不變；路徑面 walk 消費者（collect_py_corpus／collect_js_ts_corpus／doc_delta walk／source_line drift probe）零改動零新增成本；legacy slot（無 identity keys → 不計算 identity）查詢零 hash 成本（D8「行為零變」的成本面兌現） | 旗艦審訂 R20（research Q4 插入點草圖的審訂精化——EP 為權威；所有凍結決策 D1-D16 在此形狀下語義不變） |
| D18 | **false-stale precise branch 條件不變**（`!source_newer && needs_rebuild()`＋doc_delta gate，build.rs:1018-1029）——identity 態**亦可達**：producer-omits non-convergence 走 preserve 分支（舊 keys 保留 → identity_drift=true）＋missing/extra>0 → precise「語料不一致」診斷；「僅 legacy 態可達」的是 *fingerprint-only drift 且 identity 相等* 的形態（R18 精確化保留）。不得給 branch 加 legacy-guard（會掛 `s4_nonconverged` 且把 non-convergence 誤述為「heal 期間原始碼又變動」） | 旗艦審訂 R21 |

## UC 盤點

### Backlog 關聯

- 本 repo 無 `backlog/`（`.kanban/` 僅存 Done legacy；board 制已退役）——卡片動作整項跳過（正當跳過：repo 不採 board 制）。
- 上游追蹤：ai-guide 線持開題信與落點表（對方線管轄）；本線以本 EP 為單一追蹤載體。

### SYSTEM-MAP 影響

- 無 SYSTEM-MAP.md（正當跳過：repo 無此檔）。

### 掃描範圍

- root `AGENTS.md` Capabilities 表（16 行）——相關 row：「Self-owned graph db build」「Main-index query-time self-heal + commit-granularity refresh」「Binary freshness face」「Unified MCP interface」。
- `crates/AGENTS.md`（278 行 owner face）——module layering／exit-semantics D3／`walk_sources` 單點敘述。
- `plugin/skills/code-reality-tools/SKILL.md`（工具事實真相源，帶 drift-discipline header）。
- memory 池 rg「freshness|source identity|fingerprint|staleness」——見下節。

### 同主題 memory 條目（結案蒸餾範圍）

- `cr-source-identity-ep-intent`（本弧流水：意向回執＋粒度初裁——弧結案時蒸餾為終態 facts）
- `cr-refresh-model-vs-crg`（freshness 顯式刷新模型決策——durable，本 EP 延伸其語義，結案時補 identity 權威一行）
- `cr-freshness-face-readjudication`（binary 軸 WARN 面——相鄰軸不動，無需蒸餾）
- `cr-freshness-rev-fingerprint-stale`（binary 軸 cargo 指紋釘死——相鄰軸已知缺口，不在本 EP 範圍）
- `interface-form-mcp-vs-cli-adjudication`（face 選型依據——durable）

### 既有 UC 狀態

| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Main-index query-time self-heal + commit-granularity refresh | ✅ | root AGENTS.md Capabilities | 更新 | heal 決策訊號升級 identity 權威（torn-plane guard 無條件保留）；docs-only head-sync 腿語義不變（gate 單點繼承） |
| Self-owned graph db build (producer-keyed schema) | ✅ | root AGENTS.md Capabilities | 更新 | stamp 面新增 identity keys（additive，零 migration） |
| Unified MCP interface | ✅ | root AGENTS.md Capabilities | 無影響 | MCP 不加工具（D7） |

### 新增 UC

| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Source identity freshness face（dirty-WT/content-aware 查詢面：indexed/current identity 對比 verdict，消費端據此可比對宣稱 fresh） | 📋 | `crates/code-reality/src/identity.rs`＋`src/bin/code-reality/main.rs`（`freshness` 子命令）＋stamp/heal 接線（S1-S4） |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 消費端 preflight 問 freshness（語料無變） | `freshness --repo`，identity 相符 | fresh verdict，exit 0，`serves=current-tree` | 無 | Source identity freshness face |
| SM-2 | WT 編輯（mtime 新）後查 freshness | 編輯任一語料檔 | stale verdict，exit 1，stale_reasons 含 content-drift，`serves=committed-baseline` | 無 | 同上 |
| SM-3 | **內容變更 mtime 未變**（rsync -a/tar 回存模擬） | 改內容後 `touch -r` 還原舊 mtime | identity 仍偵測漂移（size 或 content_hash 變）→ stale；heal 鏈走 **rebuild** 非 false-stale serve | rebuild 後收斂 fresh | Source identity face＋self-heal 更新 |
| SM-4 | touch only（mtime 變內容同） | `touch <corpus file>` | cache miss → 重 hash → 內容同 → identity 不變 → **fresh**（無 false rebuild） | 無 | Source identity freshness face |
| SM-5 | 檔案新增／刪除 | 新增/刪除語料檔 | path 集變 → identity 變 → stale（涵蓋現行 doc-set drift 語義） | 無 | 同上 |
| SM-6 | legacy slot（無 identity keys） | 舊 meta.json 消費 | `identity_drift=None` → 現行 mtime+docset 判準，行為零變（D8），`serves=legacy-signals` | 無 | self-heal 更新（相容面） |
| SM-7 | 無 slot 查 freshness | repo 無 `.code-reality/scip/` | fail-loud exit 2＋建槽指引，**禁宣稱 fresh**（D9） | 無 | Source identity freshness face |
| SM-8 | explicit `--index` 查詢 | `scip_refs --index <path>` | **不觸發 identity 計算、不寫 cache、不 heal**（現狀 pin——explicit 面今日即不做 staleness 計算，cli.rs:336-338；D11 唯讀半邊的現狀對稱） | 無 | 同上 |
| SM-9 | cache 檔損壞／半寫 | 手工破壞 identity-cache.json | 棄置 → 全量重算 → identity 正確（crash-only for cache）；atomic-rename 下查詢端讀不到半寫 | 重算後收斂 | Source identity face |
| SM-10 | 並發：查詢遇 heal 進行中 | query 與 heal 競態 | `.heal.lock` 序列化 heal；cache 寫 atomic-rename；查詢端 identity 計算讀不到半寫狀態 | 無 | self-heal 更新 |
| SM-11 | 效能期待 | NT 級語料 steady-state | gate ≤ stat 掃量級（實測 39.5ms）；冷 full-hash 秒級以下（實測 495ms）—— rebuild 觸發才是重活 | POC 數字入 EP | Source identity face |
| SM-12 | docs-only commit 後 refresh | commit 只動文檔 | head-sync re-stamp：head keys 更新、identity keys **值不變**、corpus drift 時 preserve gate 照舊（D10） | 既有 `refresh_docs_only_head_syncs_meta_only` 延伸 | self-heal/refresh 更新 |
| SM-13 | stash 往返／rebase replay（內容不變） | stash push→查（stale）→pop／rebase replay | 內容還原 → identity 回到相等 → 收斂 **fresh**（中途 stale 態合法；git machinery 案在此翻 churn，D1 否決理由的行為面驗證） | pop 後查詢收斂 | Source identity face |
| SM-14 | torn data plane（identity 態） | slot 已發佈、graph.db mtime < slot mtime | `needs_rebuild=true`（graph_lags **無條件**，不因 identity_drift==Some(false) 短路——judge R1） | heal 後 torn 解除 | self-heal 更新 |
| SM-15 | 同秒同尺寸改寫 | formatter/腳本批量寫（同秒內容換） | gate 不命中（(secs,nanos) 精度，D16）→ 重 hash → identity 偵測漂移 → stale | 無 | Source identity face |
| SM-16 | 無實效 exclude 變更 | profile 改 pattern 但不命中任何檔案 | identity 不變 → **fresh**（policy-drift reason 僅 legacy 態可達——訊號降級揭露，judge R14） | 無 | self-heal 更新 |
| SM-17 | 空 terminal repo 查 freshness（全排除語料的合法收斂態） | 語料全數 profile 排除後 slot 缺席 | no-slot exit 2＋stderr 建槽指引——**文案含空終態說明**（指引不得誤導「重建即可得槽」；D9 不變：禁宣稱 fresh） | 無 | Source identity freshness face |

## 測試規劃段（TC 凍結）

> oracle authority 分級：本 EP TC 的 oracle_source 多為 **S**（本 EP 的 identity 定義＝frozen spec＋AIR-135.2 不變量公式；hash 的數學性質）與既有行為 pin 的延伸（authority 同源測試家族）。I/N 不出現——無「以 impl 驗 impl」項。

| TC | claim | Given-When | oracle | oracle_source | evidence class | uncovered |
|----|-------|-----------|--------|---------------|----------------|-----------|
| TC-1 | identity 對等樹決定論：同內容兩樹（不同根目錄）identity 相等，**含 entry 格式（header＋face 欄）pin** | fixture 樹複製兩份→各算 identity | 兩值相等 | S：identity 定義（content-addressing，D3） | L2 單元 | 非 UTF-8 檔名（walk 本身邊界） |
| TC-2 | 內容敏感：任一檔單 byte 變 → identity 變 | 改 1 byte→重算 | 前後不等 | S：identity 定義 | L2 | 無 |
| TC-3 | touch 冪等：mtime 變內容同 → identity 不變 | `touch` 全語料→重算 | 前後相等 | S：D3 | L2 | 無 |
| TC-4 | dirty-WT 偵測：build 後未 commit 編輯 → indexed ≠ current | fixture build→編輯→freshness | verdict stale＋reason content-drift | S：AIR-135.2 公式 | L3 整合 | 無 |
| TC-5 | **mtime 保留偵測**：內容換新＋還原舊 mtime（size 同/異兩形）→ stale 偵測 | 模擬 rsync -a/tar 回存 | verdict stale；heal 鏈走 rebuild 非 false-stale serve | S：AIR-135.2（mtime 禁為唯一判準） | L3 整合（SM-3） | 跨檔案系統 mtime 粒度差 |
| TC-6 | cache 等價：cache 命中路徑 identity ≡ 全量重算 identity（**entry 格式含 face 欄的等價**） | 暖 cache→零變更 recheck／單檔變更 recheck，各自對照 no-cache 全量 | 兩路 identity 相等 | S：incremental 等價原則 | L2 | 無 |
| TC-7 | freshness face 契約：**五態** exit 與 JSON schema 逐鍵逐值 pin——fresh（0）/stale（1）/no-slot（2）/legacy（退化行為＋`serves=legacy-signals` 不分 fresh/stale）/**fresh+head_drift**（exit 0＋`head_drift=true`＋stale_reasons 空） | 五 fixture 態各跑 CLI | exit＋JSON 全鍵 `{repo, slot, fresh, stale_reasons[], head_drift, faces[], indexed_source_identity, current_source_identity, identity_algo, serves}`＋serves 值域（current-tree/committed-baseline/legacy-signals）逐態 | S：D8/D9/D14＋本 EP face 契約 | L3 整合 | 無 |
| TC-8 | anti-laundering 延伸：docs≠disk 時 stamp preserve 擴及 identity keys；docs-only head-sync 不動 identity 值；**stamp 路徑 cache 污染免疫**（污染 cache 後 build → stamped identity 與乾淨全量相等，D13） | 既有 `s4_stamp_preserves…`/`refresh_docs_only…` 情境延伸＋污染注入 | identity keys preserve/不變值/stamped 值不受 cache 影響 | S：既有 anti-laundering 不變量（engine.rs:964-975 design source）延伸＋D13 | L3 整合 | 無 |
| TC-9 | face scoping：python-only explicit slot 不被 .ts 內容變更 staled（identity 版隔離） | 既有 `s4_face_isolation…` 情境 identity 版 | verdict fresh（scope 外） | S：既有 eval scope 語義延續 | L3 | 無 |
| TC-10 | crash-only cache：損壞/降版/**repo mismatch** cache → 全量重算正確答案，零錯誤外洩 | 破壞 identity-cache.json／改 repo 欄→查 freshness | identity 與乾淨路徑相等；stderr 棄置訊息 | S：數據完整性優先（crash-only）＋D16 | L2/L3 | 無 |
| TC-11 | heal 收斂：identity 漂移觸發的真 rebuild 後 fresh，且 cooldown/churn guard 行為不變 | SM-3 情境跑 heal 鏈 | 收斂 fresh；serve-stale/cooldown 線文案相容 | S：self-heal 契約（research Q1 分流點） | L4（真 producer 小語料） | NT 級真語料 heal 時序 |
| TC-12 | 已知邊界 pin：symlink 語料檔對 identity 不可見（walk 語義現狀） | symlinked source→identity/查詢 | 不進 identity（D6 pin，防未來 walk 語義漂移時靜默） | S：D6 | L2 | producer 跟隨側差異（記錄不修） |
| TC-13 | torn-plane 無條件保留：identity-stamped slot＋graph.db 落後 → needs_rebuild true | 既有 `s4_torn_graph_lags_slot_forces_heal`（js_ts_freshness.rs:417）的 identity-stamped 版 | needs_rebuild true（graph_lags 短路免疫，judge R1/SM-14） | S：muse P1-3 torn guard（engine.rs:821-839 doc comment） | L3 整合 | 無 |
| TC-14 | 同秒同尺寸改寫偵測：gate 不命中（nanos 精度） | 同秒內換內容保尺寸→查 | stale（TC-5 的精度形，D16） | S：D16 | L2 | 檔案系統無 nanos mtime 的极端 FS |
| TC-15 | stash 往返收斂：push→（stale）→pop → 收斂 fresh | SM-13 情境 | 終態 identity 相等、fresh | S：D1/D3 冪等 | L3 整合 | 無 |
| TC-16 | no-op exclude 變更 → fresh（訊號降級 pin，SM-16） | 改 profile pattern 不命中→查 | fresh；policy-drift reason 不可達（identity 態） | S：content-addressed 語義＋judge R14 揭露 | L2 | 無 |

**pre-RED challenge**：full 檔＋silent-corruption path 命中（freshness 宣稱錯誤＝靜默信任錯誤證據，同 silent-corruption 級）→ 觸發。challenge 形態＝fresh context 對 **TC-5／TC-6／TC-10／TC-13** 四條 blind derive oracle（先自行推「什麼輸入會讓 mtime-gate 失效／cache 路徑與全量路徑何況分岔／stamp 何時可能信任 cache／torn guard 何時可被短路」再對照 TC），falsifiable 探針＝故障注入清單：同尺寸同 mtime 內容替換、**同秒同尺寸改寫**、cache 部分 entry 污染、cache 版本降級、**cache repo 欄竄改**、**identity-stamped slot＋graph.db 落後**。

**same-family precondition**：`author_family: glm`——若實作 session 同為 glm 家族，RED 前須完成上述 challenge 或顯式記錄 degraded。

**amendment 附錄**：（TC 凍結於實作前，變更走本區記錄 old/new oracle＋reason＋authority）

| TC | old oracle | new oracle | reason | authority |
|----|-----------|-----------|--------|-----------|
| TC-7 | 五 fixture 態各跑 CLI；exit＋JSON 全鍵 `{repo, slot, fresh, stale_reasons[], head_drift, faces[], indexed_source_identity, current_source_identity, identity_algo, serves}`＋serves 值域逐態 | **四個 JSON-producing 態**（fresh／stale／legacy／fresh+head_drift）逐鍵逐值 pin；**no-slot 態**＝exit 2＋stderr 建槽指引（含空終態說明，SM-17）、不產 JSON。逐態欄值補釘：legacy 態 `indexed/current_source_identity`=null（current **不計算**——無可比對面）、`identity_algo` 恆 `"sha256-v1"`、`head_drift`＝`snap.head_drift == Some(true)`（None→false）、`faces`＝eval scope faces 的 `meta_name()` | no-slot 態無資料可填全鍵——原 oracle 對該態不可滿足；legacy 態 identity 欄值未釘則斷言不可機械判準 | 旗艦審訂（2026-09-23，ledger R24；D9/D8 的 face 契約補全，非語義變更） |

## 段落 0 研究摘要（全文＝[references/research.md](references/research.md)）

**可複用基礎設施**：
- `walk_sources`（engine.rs:595-668）——唯一 walk 實作，staleness/corpus/stamp/heal 四面共用；records 捕獲點＝`ent.metadata()` 收集處（:630，已 stat——size+mtime 零新增 syscall；**hash 計算在 identity 組合點，非 walk 內**，D17）。
- `fingerprint_for_faces`（engine.rs:565-588）——face-scoped 指紋先例，identity 同構（face 進 entry）。
- `stamp_meta_core` fresh/preserve 分支（engine.rs:1009/:977-991）——anti-laundering 單點，identity keys 自動繼承保護（refresh docs-only 腿零改動，research Q5）。
- `acquire_heal_lock`（build.rs:803-837）——single-flight；atomic-rename 寫先例 `write_is_atomic_no_tmp_residue`（end_to_end.rs:65）。
- `SOURCE_FACES`/`parse_stamped_faces`（engine.rs:718-735）——face scoping 與 legacy 相容先例（D8 同構）。
- 新子命令註冊：main.rs:28-93 `route()`＋`SUBCOMMANDS: [&str; 20]`；自製 argparse（禁 clap）。

**依賴關係與關鍵約束**（工具輸出引用見 research.md 逐條）：
- query-time 現行成本＝full walk＋N stat＋meta 讀＋1 git spawn（engine.rs:737-739 自述＋`t13` pin 的範圍澄清）——identity 增量僅 changed 檔 hash。
- identity 計算須與 producer 語料同一規則（AD-11，js_ts_corpus.rs:3-6）——走 `walk_sources` 單點即滿足。
- explicit `--index` 唯讀姿態與 `CODE_REALITY_AUTOHEAL=off` 逃生（cli.rs:336-339）——D11 對稱設計的錨。
- **torn-plane guard 折疊點**：`source_newer: source_newer || graph_lags`（engine.rs:839；muse P1-3 doc comment :821-823）——S3 改寫的無條件保留項（judge R1 親證）。

**negative 宣稱**：research.md (a) 節 N1-N9 全數成立（雙腿）；本 EP 依賴的關鍵 negative＝N1（現無 content hash——S1 是新建非改寫）與 N4（walk 無快取——cache 是新增層非替換）。

**風險假設與 kill criteria（AIR-131）**：

| # | Assumption | Probe | Kill observation | Action |
|---|---|---|---|---|
| KC-1 | stat-gated content identity 的 query-time 成本在 NT 級語料可用（steady-state ≤500ms、冷 hash ≤60s） | **已執行（POC，`poc/poc_identity_cost.py`）**：NT 3,418 檔/66.1MB → gate 39.5ms、cold 495ms、CR 自倉 gate 2.0ms；cache≡full 三語料等價 | steady-state >500ms 或冷 hash >60s（未觸發—— retire） | 已 retire（probe 通過，數字如上） |
| KC-2 | identity 跨等價樹可重現（路徑正規化/編碼不洩漏揮發性輸入） | POC 預證（雙路徑等價）＋TC-1/TC-6 implementation pin | 等價樹 identity 不等（正規化修復後仍分岔） | S1 內 pivot：隔離揮發性輸入；仍不可修 → 粒度改 git-machinery 加速器混合案（research arc，重開題） |

其餘中風險假設（並發競態、cache 污染殘餘、大語料、Windows 正規化）不入 kill 名單（不會殺死方案方向），對應驗證散入 S1-S3 驗證策略與 TC-6/TC-9/TC-10。

**Cache validity 殘餘風險揭露（research Q10.2 的政策裁決）**：`(size, mtime[secs+nanos])` 全等即重用 cached content_hash——同尺寸同 nanos-mtime 的內容替換（精確偽造情境）會逃過 gate。政策＝**接受並揭露，且由 D13 兜底**：①stamp/build 路徑全量重 hash（D13）——indexed identity 永遠真實；②query 端 gate 逃逸的最壞後果＝false-stale（安全方向，rebuild 即淨化）或 stat-gate 同類殘餘；③crash-only 語義（cache 任何可疑——損壞/降版/repo mismatch——即棄置重算，永不帶病服務）；④本揭露寫入消費端 SKILL.md 工具事實面。identity 的「content-aware」宣稱精度以此為準。

## 段落劃分原則

四段依資料流切：S1 身分計算層（新 module，零消費者）→ S2 寫入面（stamp/JSON face）→ S3 決策面（heal/refresh 消費 identity）→ S4 讀出面（CLI face）。S2/S3/S4 依賴 S1 的 identity API；S3/S4 可並行。每段自帶驗證策略與 TC 引用。

---

## S1：identity engine（`crates/code-reality/src/identity.rs` 新 module）

### Context

- 實作「Source identity freshness face」的計算層。依賴：無（S1 是 leaf）；被 S2/S3/S4 消費。
- 語義約束：與 S2 共享 identity entry 格式（`"{face}\0{rel}\0{size}\0{content_hash}\0"`，face 字串＝`LanguageFace::meta_name()`——與 `source_faces`/`parse_stamped_faces` 同語彙）與 `identity_algo` 字串；與 S3 共享 `identity_drift` 三態語義與 `IdentityCachePolicy`。
- 基礎設施盤點：`walk_sources`（唯一 walk）、`SKIP_DIRS`（producer 反向 import 單一源——**不可搬移**，producer import 路徑保持）、`normalize_rel`（js_ts_corpus.rs:55-58，identity 鍵沿用同正規化）。lib.rs 註冊 `pub mod identity;`（hub_refs 與 js_ts_calls 之間）。
- 技術選型：`sha2` workspace dep（D2）；serde_json（既有）cache 序列化；**streaming hash**（`Sha256::update` 分塊餵）為實作裁量——多 GB 單檔不整檔入記憶體（judge R16）。
- 成功標準：TC-1/2/3/6/10/12/14 綠；POC 數字量級不退化。

### 1b. Invariant Impact

- 受影響 domain invariant：**producer freshness 不變量（AIR-135.2）本體**——identity 計算是新真相源；anti-laundering（drift 保持可見）在 S2 接線，S1 計算層自身須無隱藏揮發性輸入（正規化單一源）＋**stamp 路徑不可信任 cache**（D13 的計算層前提：caller 明示 policy，S1 不自判）。
- critical path 觸及：silent-corruption 鄰接面（freshness 誤宣稱＝靜默信任錯誤證據）——TC-5/TC-6/TC-10/TC-13 為其驗證對齊（producer 自證守住）。

### 核心實作要點

- `SourceRecord { face: LanguageFace, rel: String, size: u64, mtime: (i64 secs, u32 nanos) }`——**不含 content_hash**（D17 lazy：hash 在 identity 組合點計算）；`SourceWalk` per-face `BTreeSet<String>` 升格為 records。**投影介面不變**（`paths_for_faces`/`newest_for_faces` 照舊——rel 投影）；**raw per-face 欄位的既有生產消費者五處機械改寫**（編譯期可見，judge R10/R11＋旗艦 R23）：js_ts_corpus.rs:34-39（iter chain）、build.rs:243-248 `collect_py_corpus`（多行 `walk\n.py\n.iter()`——行內 grep 搜不到，勿漏）、evaluate_staleness detected-face 探測 engine.rs:768-779（`is_empty()`）、fingerprint_for_faces engine.rs:574-585（欄位 match）、doc_set_delta engine.rs:872-883（`disk.extend(walk.*)`）——另有測試側斷言（tests/staleness.rs 的 `w.py`/`w.rs` membership/equality）同步機械改寫。
- `compute_identity(root, records, faces, policy: IdentityCachePolicy, cache) -> Result<Identity, String>`：rel 鍵 `normalize_rel` 正規化 → entry 組裝 → sha256。policy 三態語義：**WriteBack**＝gate 命中重用＋miss 實讀＋寫回；**Full**（stamp/build 端）＝**繞過 gate 全量實讀**＋寫回（D13——indexed identity 永遠從實際 bytes 滿算；污染 entries 被 scope 內真值覆寫）；**ReadOnly**（`CODE_REALITY_IDENTITY_CACHE=off`）＝繞過 gate 實讀、**不讀不寫 cache**（off 的語義＝不信任 cache，非「只讀 cache」）。
- per-file hash 讀取失敗 → **Err 傳播**（D15）；streaming hash（`Sha256::update` 由 `io::copy` 餵）。
- `IdentityCache`：load（version/algo 不符或 parse 失敗或 **repo canonical mismatch** → None，D16）；gate（`(size, mtime(secs,nanos))` 全等）；store＝atomic tmp+rename 的**合併寫**（保留仍屬現 walk 集的既有 entries、新算值覆寫同鍵、prune 離集 entries；寫失敗 WARN 不擋——t20 唯讀 slot dir 態相容）。Full 與 WriteBack 共用同一 store；差異僅在 gate 繞過（「整檔重寫」＝atomic 全檔寫的機制描述，合併語義見前）。
- **正規化邊界 pin（跨單位語義換算防線）**：walk rel 維持原樣供既有比較面（stamp 的 `disk == docs`、doc_set_delta、fingerprint）；`normalize_rel` 只在 identity entry 與 cache key 邊界施作——**不得**把 walk 的 rel 正規化「統一」進 walk_sources（會改動 disk==docs 對 producer rel 的比較面，silent 破壞 anti-laundering gate）。
- **D6 的可觀測推論（記錄免修）**：語料含 symlink 的 repo（producer 跟隨、walk 跳過）恆 `disk != docs` → stamp 恆走 preserve 分支 → **identity keys 永不蓋章＝恆 legacy 態**——安全降級（現行判準照常服務），非缺陷；TC-12 的邊界 pin 即涵蓋此形的不可見性。
- env 逃生 `CODE_REALITY_IDENTITY_CACHE=off`（對稱 AUTOHEAL；policy 降級 ReadOnly）。

### Pseudo Code

```rust
// identity.rs（新檔，code-reality crate 內，engine 的鄰接模組；lib.rs 註冊）
pub const IDENTITY_ALGO: &str = "sha256-v1";
const IDENTITY_VERSION: u32 = 1;

pub enum IdentityCachePolicy { Full, WriteBack, ReadOnly }

pub struct SourceRecord { pub face: LanguageFace, pub rel: String,
                          pub size: u64, pub mtime: (i64, u32) }   // 無 content_hash（D17）

pub struct Identity { pub algo: &'static str, pub value: String }

// hash 只在組合時發生（D17）——walk_sources 本體維持純路徑＋stat，簽名不變。
// records＝現 walk 的記錄集（BTreeMap<rel, SourceRecord>，迭代序＝byte-determinism）
pub fn compute_identity(root: &Path, records: &BTreeMap<String, SourceRecord>,
                        faces: &[LanguageFace], policy: IdentityCachePolicy,
                        cache: &mut IdentityCache) -> Result<Identity, String> {
    let mut h = Sha256::new();
    h.update(b"cr-identity-v1\0");
    for (rel, rec) in records {                        // sorted by rel
        if !faces.contains(&rec.face) { continue; }
        let key = normalize_rel(rel);
        let chash = content_hash_of(&root.join(&rec.rel), &key,
                                    (rec.size, rec.mtime), policy, cache)?;  // D15：Err 傳播
        h.update(format!("{}\0{}\0{}\0{}\0",
            rec.face.meta_name(), key, rec.size, chash));
    }
    if !matches!(policy, IdentityCachePolicy::ReadOnly) {
        cache.store();    // atomic 合併寫（prune 離集；寫失敗 WARN 不擋）
    }
    Ok(Identity { algo: IDENTITY_ALGO, value: hex(h.finalize()) })
}

fn content_hash_of(path: &Path, key: &str, stat: (u64, (i64, u32)),
                   policy: IdentityCachePolicy, cache: &mut IdentityCache)
                   -> Result<String, String> {
    if let IdentityCachePolicy::WriteBack = policy {
        if let Some(hash) = cache.hit(key, stat) { return Ok(hash); }
    }
    // Full（stamp/build）與 ReadOnly（env-off）一律實讀——D15：讀取失敗 Err 傳播（fail-loud）
    let mut h = Sha256::new();
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;   // streaming
    Ok(hex(h.finalize()))
}
```

Call stack：`walk_sources`（既有，簽名不變）→ **需要 identity 的 caller** 呼叫 `compute_identity`（D17：路徑面 caller 不呼）→ S2 stamp（Full）／S3 evaluate_staleness drift 比對（WriteBack；stamped identity keys 存在才計算）／S4 freshness face（同 S3）。

### 驗證策略

- TC-1（決定論＋格式 pin）、TC-2（敏感）、TC-3（touch 冪等）、TC-6（cache 等價）、TC-10（crash-only＋repo mismatch）、TC-12（symlink pin）、TC-14（nanos 精度）——單元層（fixture 樹）。
- **測試落點**：單元測試 identity.rs 內嵌 `#[cfg(test)]`（js_ts_corpus.rs 先例——fixture 樹 tempdir）；CLI/整合測試新檔 `tests/source_identity.rs`。**既有 `tests/freshness.rs` 是 binary 版面 pin（version-face，EP ep-binary-freshness-face）——同名近鄰，勿混入 index 軸測試**（對稱收尾 1 的 cr-freshness 命名辨識行）。
- 效能回歸：不進 CI（環境耦合）；POC 數字固化於本 EP 為基準，SM-11 量級驗證在 /ep-validate 抽查。
- 前期 POC：已執行（KC-1 retire；`poc/poc_identity_cost.py` 保留至 S1 build 承接後清除）。

## S2：stamp face＋anti-laundering 延伸（engine.rs `stamp_meta_core`＋build.rs render）

### Context

- 實作能力：更新「Self-owned graph db build」。依賴：S1。與 S3 共享 identity keys 命名。
- 語義約束：fresh/preserve 分支結構不變（`disk == docs` gate 單點，research Q2/Q5——refresh docs-only 腿**零改動**自動繼承）。**stamped identity 覆蓋 faces＝本次 stamp 的 faces 集**（auto 場＝當時全部偵測面）——與 S3 eval scope 對鏡（judge R11）。
- 依賴錨點：`stamp_meta_core` → 定義 `engine.rs:921` / 消費 `build.rs:711`、`refresh.rs:119`、`cli.rs:469`；`preserve_prior_keys` → 定義 `engine.rs:977` / 消費同函數內（延伸斷言點 `tests/js_ts_freshness.rs:254`）。

### 核心實作要點

- **stamp 路徑 identity 計算用 `IdentityCachePolicy::Full`**（D13）：繞過 gate 全量實讀＋合併寫回——build/heal 本就分鐘級，+495ms 為噪聲；此行是「rebuild 淨化 cache」承諾的機制落點。
- fresh pair 追加寫 `source_identity`（value）＋`identity_algo`（兩鍵；`disk == docs` gate 內——identity 只在語料與 index 一致時蓋章）。**compute Err → 落 preserve 分支**（`stamped_fresh_keys` 保持 false＋WARN）——對齊既有「mismatch **或 recompute 失敗** → preserve」語義（engine.rs:969-975 註解），不以 `?` 使 stamp 整體失敗（publish 後 stamp 失敗會讓 meta 停留舊 head，劣於 preserve；build 端 stamp 失敗本就只是 note，build.rs:726-729）。
- `preserve_prior_keys` 擴為五鍵（＋兩 identity keys）＋WARN 文案更新（identity 並列）。
- umbrella `build --json` additive key `source_identity`（build.rs:1165-1176 render）。
- 空收斂點（build.rs:658-662）移除 identity-cache.json（D12 後半）。

### Pseudo Code

```rust
// stamp_meta_core 內（現有 engine.rs:1009 if disk == docs 分支）
match compute_identity(&repo, &walk_records, &faces_vec,
                       IdentityCachePolicy::Full, &mut cache) {      // D13
    Ok(identity) => {
        payload["source_identity"] = identity.value;   // 新增
        payload["identity_algo"]   = identity.algo;    // 新增
    }
    Err(_) => { /* stamped_fresh_keys 維持 false → 走既有 preserve 分支（上方要點） */ }
}
// preserve closure（engine.rs:977-991）擴五鍵：三既有 + source_identity + identity_algo
```

### 驗證策略

- TC-8（preserve 延伸＋docs-only head-sync identity 值不變＋**stamp 路徑 cache 污染免疫**）——延伸既有兩測試的斷言面＋污染注入測試。
- **新 stamp keys 斷言＝逐鍵枚舉**（judge R13——非計數）：基底六鍵（repo/head/stamped_at/tool/producer/selection，engine.rs:956-963）＋fresh 分支既有 2-3 鍵（source_faces/source_set_fingerprint/〔JS/TS〕js_ts_profile_fingerprint）＋新增 2 鍵（source_identity/identity_algo）；frozen dict order 契約僅約束**凍結 Python 序五鍵**（repo/head/stamped_at/tool/producer——engine.rs:915-920 doc 註；`selection` 為 carrier 增鍵不在凍結序內，identity keys 同為 additive 排尾）。
- legacy meta（無 identity keys）讀取退化在 S3 驗證。

## S3：heal/refresh 決策接入（engine.rs `evaluate_staleness`＋build.rs 分流）

### Context

- 實作能力：更新「Main-index query-time self-heal」。依賴：S1（S2 的 stamped keys 是其輸入）。
- 語義約束：與 S2 共享 keys；與 S4 共享 stale_reasons 詞彙。**最關鍵語義翻轉＝needs_rebuild 決策權威換軸**：identity 態下 mtime-newer 退出 fatal 集（touch 冪等）、`identity_drift` 取代之——rsync 形態（無新 mtime＋identity 漂移）首次查詢即 rebuild。false-stale precise branch（build.rs:1018-1029）條件本身**不變**（D18——它是 rebuild 後的重評估診斷面，非查詢入口閘）。
- 依賴錨點：`evaluate_staleness` → 定義 `engine.rs:750` / 消費 `build.rs:999,1133`、`refresh.rs:101`；`needs_rebuild` → 定義 `engine.rs:684` / 消費 `build.rs:1140`；false-stale 分流 → `build.rs:1018-1029` / 測試 `t18_false_stale_warns_once_no_loop`（tests/build.rs:902）；**graph_lags 折疊點 → engine.rs:821-839** / 既有測試 `s4_torn_graph_lags_slot_forces_heal`（js_ts_freshness.rs:417）。

### 1b. Invariant Impact

- 受影響 invariant：producer freshness（權威翻轉本體）；false-stale 語義（漏報→誤報的光譜移動，須雙向測試）；**torn-plane guard（muse P1-3）無條件保留**。
- critical path：heal 分流——驗證對齊＝TC-5（rsync 情境走 rebuild）＋TC-13（torn guard identity 版）＋`t18` 家族回歸（真 false-stale——legacy slot——仍 serve-stale 不 loop）。

### 核心實作要點

- `StalenessSnapshot`＋`identity_drift: Option<bool>`（judge R6 的穿線載體：`evaluate_staleness(repo, slot, policy)` 簽名擴充）。**五個既有 caller 一律 `WriteBack`**（default-slot 語義，D11）：heal 內重評估 build.rs:999、`heal_outcome_after_rebuild_err` build.rs:957、`wait_peer_and_reevaluate` build.rs:1086、`ensure_fresh` build.rs:1133、refresh 自存快照 refresh.rs:101；`CODE_REALITY_IDENTITY_CACHE=off` 在 identity 模組內降級 ReadOnly（對稱 AUTOHEAL 的內部解析）；**Full 僅 stamp 端**（S2/D13）。原分配表「build.rs:1008＝Full」係 walk-time hashing 形狀的產物——D17 下該 walk（doc_delta）是路徑面消費者，不涉 policy。
- **identity 計算條件與 scope（旗艦 R22）**：meta 須同時攜 `source_faces`＋`source_identity`＋`identity_algo` 才計算（任一缺 → `identity_drift=None` 退化 legacy，不 bomb——meta 不一致視同 legacy）。**current-side identity 的 face scope＝eval_faces**——auto＝stamped∪detected、explicit＝pinned，與 `source_newer` 的 eval scope 完全同鏡（judge R11 的 S2 對鏡落到此處）：新語言面到達 → current identity 增 entry → drift → heal（`s4_auto_index_heals_on_new_language_arrival` 的 identity 態對應——mtime 已退出 identity 態決策，scope 鏡像是該行為的唯一承載）。`identity_drift = compute_identity(eval_faces).value != stamped_value`。legacy slot 不計算（零 hash 成本，D8）。
- **`needs_rebuild` 改寫（torn guard 無條件，judge R1）**：

```rust
// StalenessSnapshot 增欄：graph_lags 自 source_newer 拆出為獨立欄（engine.rs:839 拆折疊）
self.identity_drift == Some(true)
    || self.graph_lags                                   // 無條件——muse P1-3，TC-13
    || (self.identity_drift.is_none() && legacy 三訊號)   // D8 legacy 態
```

  identity 態下 `doc_set_drift`/`corpus_policy_drift` **照算不拆**（非 fatal，但餵 S4 stale_reasons 與 SM-16 降級揭露）。
- **false-stale precise branch 條件不變**（D18，旗艦 R21）：`!snap.source_newer && snap.needs_rebuild()`＋doc_delta gate（build.rs:1018-1029）。identity 態的可達形態＝**non-convergence 診斷**：producer 持續漏產 → rebuild 後 disk≠docs → stamp 走 preserve 分支（舊 identity keys 保留）→ `identity_drift=true`＋missing/extra>0 → precise「語料不一致」ServeStale＋churn marker（`s4_nonconverged_heal_arms_cooldown_not_fresh` 釘住此文案）。legacy-only 的是「fingerprint-only drift 且 identity 相同」形態（judge R18：path-set 變必 identity 變——identity 態不可達）。**不得**給 branch 加 `identity_drift.is_none()` guard（會掛前述測試，且把 non-convergence 誤述為「heal 期間原始碼又變動」）。
- docs-only head-sync 腿：零改動（gate 在 stamp，S2 已接）——驗證測試釘住（TC-8 後半）。
- churn cooldown/cooldown override/churn marker：行為不變（identity 驅動的 heal 同 guard）——既有 t13-t22 家族回歸。
- **訊號降級揭露（judge R14）**：無實效的 exclude 變更（pattern 不命中）在 identity 態下 ⇒ fresh（policy-drift reason 僅 legacy 態可達）——content-addressed 語義的自洽推論，TC-16 釘死。

### 驗證策略

- TC-4（dirty-WT）、TC-5（mtime 保留→rebuild 非 serve——**SM-3 核心翻轉**）、TC-9（face scoping identity 版）、TC-11（heal 收斂＋guard 不變，L4 真 producer）、TC-13（**torn-plane 無條件**）、TC-15（stash 往返收斂）、TC-16（no-op policy→fresh）、TC-6 延伸（gate 決策消費 identity）。
- 回歸：staleness.rs 8 條（legacy 語義）、js_ts_freshness.rs 12 條（含 s4_torn_graph_lags identity-stamped 版—— TC-13）、build.rs t13-t22（分流/guard/鎖；`t18_false_stale_warns_once_no_loop`＝tests/build.rs:902）全數保持綠。**`s4_auto_index_heals_on_new_language_arrival`（js_ts_freshness.rs:395）在 identity 態的對應＝identity eval scope 鏡像 source_newer union（核心要點第二條）——此測試保持綠即釘住該語義**。

## S4：消費端 freshness face（main.rs `freshness` 子命令）

### Context

- 實作能力：新增「Source identity freshness face」的查詢出口。依賴：S1/S3。
- 語義約束：**零 heal 觸發**（verdict face——heal 屬 scip_refs 查詢路徑與 refresh；唯一寫入＝identity cache，D11 政策）；exit 三態（D9）；MCP 不加（D7）。
- 依賴錨點：`route()` → 定義 `main.rs:28-70` / `SUBCOMMANDS` → `main.rs:72-93`；heal gate 對稱錨 `cli.rs:336-339`（explicit 永不 heal / AUTOHEAL off）。

### 核心實作要點

- `freshness --repo <repo> [--json]`：evaluate_staleness 同路徑（policy=WriteBack——default slot 含 cache 寫回；D11）。
- **stale verdict 直接建構 `ToolOutput`**（`exit_code: 1`）——`ToolOutput::fail` 是 exit 2、`crash` 是 exit 1＋`[FAIL]`（lib.rs 既有契約），皆非 verdict face；stale 是合法答案不是錯誤，stdout 照出 verdict（human／JSON）。
- **檢查失敗（walk/stat/hash Err——D15 傳播）→ exit 2 fail-loud**：verdict face 不得抄 scip_refs 的「以現存索引作答」WARN 降級樣式（此處沒有可退的作答面）；無 slot 同理 exit 2＋建槽指引（D9），**指引文案含空終態說明**（全排除語料的合法收斂態＝slot 缺席——SM-17，勿誤導重建）。
- **fresh 定義式（D14，judge R3/R5 釘死）**：`fresh = !snap.needs_rebuild()`——head 前移**不判死**（`t15` 既有行為對稱；AIR-135.2 LHS 不含 HEAD）；head 前移以獨立欄位 `head_drift: bool` 揭露、**不進** `stale_reasons`（該欄只列 fatal reasons）。
- `--json` schema（TC-7 逐鍵逐值；四個 JSON-producing 態——no-slot 不產 JSON，見 amendment）：

```json
{ "repo": "...", "slot": "...", "fresh": true|false,
  "stale_reasons": ["content-drift"|"doc-set-drift"|"policy-drift"|"legacy-signals"|"torn-plane"],
  "head_drift": true|false, "faces": ["python","rust"],
  "indexed_source_identity": "..."|null, "current_source_identity": "..."|null,
  "identity_algo": "sha256-v1",
  "serves": "current-tree"|"committed-baseline"|"legacy-signals" }
```

- **逐態欄值（機械判準，旗艦 R24）**：`head_drift`＝`snap.head_drift == Some(true)`（None→false——無 HEAD 資訊即無 drift 可報）；`faces`＝eval scope faces 的 `meta_name()`（BTreeSet 序）；legacy 態 `indexed/current_source_identity`＝**null**（current 不計算——無可比對面）、`identity_algo` 恆 `"sha256-v1"`（工具演算法 id，非當次計算聲明）。
- `serves` 值域語義（judge R5）：fresh→`current-tree`（消費端可宣稱 fresh）；stale→`committed-baseline`（graph 答案是 committed baseline、WT delta 走 live LSP——誠實降級標註）；legacy→`legacy-signals`（D8 態，消費端按現行判準解讀）。
- stale_reasons 詞彙：`content-drift`（含 mtime 未變情境——SM-3 是賣點；亦涵蓋 auto 場 scope 增長）、`doc-set-drift`、`policy-drift`（identity 態非 fatal——fresh 時不出現）、`legacy-signals`（D8 態）、`torn-plane`（graph_lags）。**映射規則（序固定，旗艦 R24）**：`torn-plane`←graph_lags；`content-drift`←`identity_drift==Some(true)`；`doc-set-drift`←`doc_set_drift==Some(true)`；`policy-drift`←`corpus_policy_drift==Some(true)`；`legacy-signals`←`identity_drift.is_none() && source_newer`（mtime 訊號僅 legacy 態 fatal）；`head_drift` 恆不入（D14）。
- exit：fresh 0／stale 1／no-slot·env·usage·檢查失敗 2（fail-loud＋建槽指引）。
- human face：verdict 行＋reasons；註冊面：`route()` 新臂＋`SUBCOMMANDS: [&str; 20]`→**21**（長度字面值同步；--help 清單由陣列帶入）。
- **命名辨識（雙近鄰，judge R16＋旗艦 R24）**：`freshness` 子命令（index 軸）與既有 crate `cr-freshness`（binary 軸）；且**既有 `tests/freshness.rs` 是 binary 版面 pin**——index 軸測試落 `tests/source_identity.rs`，SKILL.md 工具事實面加辨識行。

### Pseudo Code

```rust
// main.rs route() 新臂
"freshness" => freshness::run(tail),            // 新檔 freshness.rs（argv→lib）
// lib API：ToolOutput 契約（lib 不印不 exit——crates/AGENTS.md lib API contract）
pub fn freshness(repo: &Path, json: bool) -> ToolOutput {
    // no slot → env-class fail: exit 2 + 指引（含空終態說明；禁宣稱 fresh，D9/SM-17）
    // snap = evaluate_staleness(repo, slot, WriteBack) — Err → exit 2 fail-loud（verdict 無降級作答面）
    // fresh = !snap.needs_rebuild()      // head 不判死（D14；head_drift 獨立欄位）
    // stale → ToolOutput { exit_code: 1, .. } 直接建構（fail()=2/crash()=1+[FAIL] 皆非 verdict face）
    // serves = match (snap.identity_drift, fresh) {
    //     (None, _)              => "legacy-signals",      // legacy 態不分 fresh/stale（快審觀察1）
    //     (Some(false), true)    => "current-tree",
    //     (Some(_), _)           => "committed-baseline",  // 含 torn（identity 同但 graph 落後）
    // }
}
```

### 驗證策略

- TC-7（五態 exit＋JSON schema 逐鍵逐值——含 fresh+head_drift 態；**oracle 範圍見 amendment 附錄**：no-slot 態＝exit 2＋stderr 指引、不產 JSON）、TC-12（face 級 symlink pin 併入）。測試落點 `tests/source_identity.rs`（tests/freshness.rs 為 binary 版面 pin，勿混入）。
- explicit `--index` 現狀 pin：explicit 查詢不進 staleness 計算（SM-8——零寫半邊為現狀不變；不做「explicit 計照走」的新接線，YAGNI，judge 採 fresh F2 建議 (a)）。
- 消費端演練（L4）：ai-guide preflight 模擬——stale 時 JSON 可判讀、fresh 時 exit 0；真消費驗收歸 ai-guide（下游驗收面）。

---

## 整合策略

- `baseline: 68d763601ca79ec5531737444615934318ab0f9e`
- `author_family: glm`（same-family RED 前置＝測試規劃段 pre-RED challenge）
- 段落執行序：S1 → S2 → S3 → S4（S3/S4 理論可並行，同 workspace crate 編譯單元——仍序列，鎖步簡單）。
- 向下相容：legacy slot 全程可用（D8/D10）；identity keys 未蓋章前現行判準照舊——deploy 後首次 build/heal 自然補章。
- 消費端接線（ai-guide 線，非本 EP 範圍）：review-engine preflight 消費 `freshness --json`；**兩點同步註記**（judge R18/R3）：①head 語義分歧——本 face head 不判死 vs cr-query 現行 HEAD-mismatch→regenerate，ai-guide 同步時擇一面收斂；②冷 full-hash 首跑可破 1s（一次性，KC-1 容忍內）——preflight 預算敘述宜含。落地後回執 scbus 信箱，驗收歸消費端。
- 發行：workspace version bump＋plugin pin 鎖步（五處 guard v3）隨收尾；PyPI/plugin 消費面更新＝release SOP（另行觸發，不綁本 EP commit）。

## 收尾步驟

1. **模組 instruction 檔＋Capabilities**：root AGENTS.md 新 row「Source identity freshness face」＋更新 self-heal row 敘述（identity 權威＋torn guard 保留）；`crates/AGENTS.md` exit-semantics 表補 freshness 三態＋module 段 identity.rs 條目；`plugin/skills/code-reality-tools/SKILL.md` 工具事實更新（freshness face＋cache validity 殘餘風險揭露＋cr-freshness 命名辨識行）＋plugin version bump（drift-discipline header 契約）。卡結案：無 board（正當跳過）。消費場景提煉：SM-1/2/3/7/11 自包含一句話入 Capabilities 備註。
2. **SYSTEM-MAP**：不存在，跳過。
3. **instruction 檔一致性**：ai-guide 端 SKILL.md 接線條文（cr freshness preflight 消費面＋head 語義同步＋冷 hash 註記）由 ai-guide 線自行同步（跨 repo 主權）；本 repo 側只保證 `freshness --json` 契約與本 EP TC-7 一致。
4. **/audit-test**：新增/延伸測試全量稽核（五證據域）；TC amendment 區結算；POC `poc/poc_identity_cost.py` 於 S1 build 承接後清除（量測數字已固化 EP＋對應行為 TC-6 釘死）；memory 結案蒸餾（第三動）。

---

## EP review（規劃期帳本）

> profile=boundary（fresh＋intent 雙腿）；judge＝主 session（Arbiter seat）；兩條 🔴 經獨立親證（engine.rs:839 sed 實測／EP 自身文本對照）。R1-R18 全 ✅ 採納、apply 完成後 status=implemented。**旗艦面審訂（2026-09-23，決策腿全權）**：R20-R25 追加、R6/R10/R18 附加 revision note、D17/D18 增補、TC-7 amendment——修改直接落檔為定稿。

| # | 來源 | 嚴重度 | finding | 裁決 | status |
|---|---|---|---|---|---|
| R1 | fresh F1＋intent F1（收斂） | 🔴 | needs_rebuild 改寫靜默關閉 torn-plane guard（graph_lags 折疊於 source_newer，Some(false) 短路；違 D5＋回歸測試必掛） | ✅ graph_lags 拆獨立欄＋needs_rebuild 無條件項＋SM-14/TC-13＋stale_reasons 補 torn-plane | implemented |
| R2 | intent F2 | 🔴 | 「rebuild 淨化」未接線——stamp/build 同吃 cache gate，stamp 與 eval 同源同污 ⇒ drift 恆 false | ✅ D13（stamp 路徑 `IdentityCachePolicy::Full` 全量＋整檔重寫）＋TC-8 污染免疫 pin＋揭露段改指向 D13 | implemented |
| R3 | fresh F4＋intent F5（收斂） | 🟡 | head_drift fresh 語義未決（pseudo 殘留「？」） | ✅ D14 head-not-fatal＋獨立欄位＋TC-7 第五態＋ai-guide 同步註記 | implemented |
| R4 | intent F3 | 🟡 | cache mtime 精度未釘死（秒級則同秒改寫靜默 stale） | ✅ D16 (secs,nanos)＋TC-14＋pre-RED 探針補 | implemented |
| R5 | intent F4＋F7 | 🟡 | serves 語義歧義＋faces 欄位缺＋TC-7 鍵集漂移 | ✅ 值域列舉三態＋faces 欄位＋TC-7 逐鍵逐值 | implemented |
| R6 | fresh F5 | 🟡 | cache 政策 API 穿線未規格化（evaluate_staleness 無 cache 參數） | ✅ IdentityCachePolicy 三態 enum＋五 caller 分配表（S3）；revision（旗艦 R20）：分配表精確化——五 caller 一律 WriteBack（ReadOnly 由 env-off 內部降級、Full 僅 stamp 端）；原表「build.rs:1008＝Full」係 walk-time hashing 形狀產物，D17 lazy 下該 walk 為路徑面消費者、無 policy | implemented |
| R7 | fresh F6 | 🟡 | per-file hash 讀取失敗政策未定（TOCTOU/permission） | ✅ D15 fail-loud（Err）＋行為變更揭露＋:173 先例對齊 | implemented |
| R8 | intent F6 | 🟡 | stash/rebase 場景行缺 | ✅ SM-13＋TC-15 | implemented |
| R9 | fresh F3＋intent F8（收斂） | 🟡 | 「零副作用」vs D11 寫回矛盾 | ✅ 總覽改「零 heal 觸發；唯一寫入＝identity cache」 | implemented |
| R10 | fresh F7 | 🟢 | 「零破壞」過度宣稱（raw per-face 欄位三處消費者） | ✅ S1 改「投影介面不變；raw 消費者機械改寫」＋列三處錨點；revision（旗艦 R23）：消費者實為**五處**（＋collect_py_corpus build.rs:243-248〔多行 `walk\n.py\n.iter()`，行內 grep 搜不到〕、doc_set_delta engine.rs:872-883）＋測試側斷言——S1 枚舉已擴 | implemented |
| R11 | fresh F8 | 🟢 | face_name() 型別錯＋stamped identity face scope 未寫 | ✅ meta_name() 修正＋S2 stamped scope=本次 stamp faces 集 | implemented |
| R12 | fresh F9 | 🟢 | cache 無 repo 綁定（跨樹移植重放） | ✅ D16 repo 欄＋mismatch 棄置＋TC-10 延伸 | implemented |
| R13 | fresh F10 | 🟢 | 「七鍵斷言」算術對不上（實為六鍵基底） | ✅ S2 改逐鍵枚舉斷言 | implemented |
| R14 | fresh F11 | 🟢 | no-op exclude 變更訊號降級未揭露 | ✅ S3 揭露＋SM-16/TC-16 | implemented |
| R15 | fresh F12 | ℹ️ | POC 格式異於 EP 格式（無 header/face；排除集略寬） | ✅ 總覽註記「同構格式；格式 pin 歸 TC-1/TC-6」 | implemented |
| R16 | fresh F13 | ℹ️ | streaming hash／Windows disk==docs 既有行為／freshness.rs 命名近鄰 | ✅ S1 註記＋收尾 SKILL.md 辨識行（disk==docs 為既有行為繼承，非新風險） | implemented |
| R17 | intent F9 | ℹ️ | 上游 provenance 三 ID 不可 git 解析 | ✅ header 標注 scbus message_id 類型＋驗收位置 | implemented |
| R18 | intent F10＋錨點抽查 | ℹ️ | 正面確認（意圖對齊/粒度/覆蓋/oracle 階層）＋false-stale 超集措辭＋冷 hash 註記＋t18 行號 902 | ✅ S3 精確化＋整合策略同步註記＋行號修正；revision（旗艦 R21）：「僅 legacy 態可達」的 false-stale 表述有誤——precise branch 在 identity 態亦可達（non-convergence 診斷）；legacy-only 的是「fingerprint drift 且 identity 相等」形態（D18＋S3 修正） | implemented |
| R19 | 快速重審（fresh context 修正驗收） | ℹ️ | 四點全 PASS＝ACCEPT；觀察1：serves match 臂序 legacy×stale 交集空窗；觀察2：D13「cache=None」速記 vs API 名 Full | ✅ 觀察1＝臂序改 legacy 判別優先（不分 fresh/stale）＋TC-7 釘值；觀察2＝術語統一 Full（D13/總覽/ledger R2 三處） | implemented |
| R20 | 旗艦審訂 F1（決策腿全權） | 🔴 | walk-time hashing 形狀：SourceRecord 內嵌 content_hash 使**所有** walk_sources 呼叫者付 hash 成本；四個路徑面 caller（source_line drift probe engine.rs:1228／collect_py_corpus build.rs:243／collect_js_ts_corpus js_ts_corpus.rs:29／doc_delta walk build.rs:1008）policy 未分配（編譯牆下 flash 必即興）；legacy slot 每查詢付無消費面的 hash（D8「行為零變」的成本面破功） | ✅ D17 lazy-hash：records 僅 (face, rel, size, mtime)，hash＋gate 移入 compute_identity（policy 落點）；walk_sources 簽名不變；S3 分配表隨之精確化（R6 note）；凍結決策 D1-D16 語義全數保持 | implemented |
| R21 | 旗艦審訂 F2 | 🔴 | S3「false-stale 僅 legacy 態可達」不成立：producer-omits non-convergence 在 identity 態走 preserve（舊 keys 保留 → identity_drift=true）＋doc_delta>0 進 precise branch——`s4_nonconverged`（js_ts_freshness.rs:352）釘「語料不一致」文案；若按原表述加 legacy-guard 必掛該測試且誤述病因 | ✅ D18：branch 條件不變；S3 Context/要點表述修正＋R18 附加 note | implemented |
| R22 | 旗艦審訂 F3 | 🟡 | identity eval scope 未明文：current-side 若只算 stamped faces，新語言面到達在 identity 態**不觸發 rebuild**（mtime 已退出決策），`s4_auto_index_heals_on_new_language_arrival`（js_ts_freshness.rs:395）必掛——「與 S3 eval scope 對鏡」僅為暗示，flash 易漏 | ✅ S3 明文：current identity scope＝eval_faces（auto＝stamped∪detected；explicit＝pinned）；計算前提＝meta 三鍵齊（否則退化 legacy 不 bomb）；legacy 零計算 | implemented |
| R23 | 旗艦審訂 F4 | 🟡 | raw per-face 欄位消費者枚舉漏二：collect_py_corpus（build.rs:243-248，多行鏈式、行內 grep 搜不到）與 doc_set_delta（engine.rs:872-883）；測試側另有斷言 | ✅ S1 枚舉擴五處＋測試側註記（R10 note）；全數編譯期可見、機械改寫 | implemented |
| R24 | 旗艦審訂 F5 | 🟡 | S4 執行面漏釘六項：stale=1 需直接建構 ToolOutput（fail()=2／crash()=1+[FAIL]，lib.rs 契約）；stale_reasons↔snapshot 映射未定義（TC-7 不可機械判準）；legacy 態 identity 欄值未釘；no-slot 態與「JSON 全鍵」矛盾；檢查失敗可能被抄 scip_refs serve-stale 樣式；SUBCOMMANDS 長度字面值與 tests/freshness.rs 既有佔名（binary 版面 pin）未提 | ✅ S4 全面補釘＋reasons 映射＋逐態欄值＋SM-17；TC-7 走 amendment 附錄（old/new oracle＋authority）；測試落點 tests/source_identity.rs | implemented |
| R25 | 旗艦審訂 F6 | 🟢 | S2 stamp 端 pseudo 以 `?` 傳播 compute Err——publish 後 stamp 整體失敗會讓 meta 停留舊 head（build 端 stamp 失敗僅 note，build.rs:726-729），劣於既有「failed recompute→preserve」語義（engine.rs:969-975 註解明載） | ✅ S2：Err→preserve 分支（stamped_fresh_keys 維持 false＋WARN），pseudo 改 match | implemented |

（ledger 全 terminal——accepted eligibility 成立；R20-R25＝旗艦面審訂 2026-09-23，全數 implemented 於本修訂稿）
