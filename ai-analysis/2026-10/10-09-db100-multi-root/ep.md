# EP: multi-Cargo-workspace root graph — SCIP producer composition ＋ identity v2（DB-100 責任信採納）

> **ep_type**: implementation
> **baseline**: `25fd932f6b1cbe0fca3cfa95c4c26975c2bf9abf`（v0.9.5）
> **revision**: v2（2026-10-09；v1→v2＝EP Review boundary 雙腿 findings 回寫：1 P0＋4 P1 必改裁決＋14 P2；review ledger 見文末「EP Review 區段」）
> **card**: CR-3（backlog/tasks/cr-3）；來源信：dutymail `db100-cr-root-graph-solicit-001`＋supplement（寄 code-reality-marshal，2026-10-09）
> **研究材料**: `references/research.md`（收斂裁決＋錨點＋風險＋kill criteria）；詳證四份在 `references/`

## 實作總覽

CR 的 Rust producer 假設「一 repo 一 Cargo workspace」：`stage_rust` 把 repo 根目錄交給 `rust-analyzer scip`，dual-workspace repo（如 delegate-bridge 的 `rust/`＋`dutymail/`）觸發 "more than one project" exit 2，root graph 對第二 workspace 永久 stale。本 EP 讓 CR 原生支援**一 git repo 內多個 Cargo workspace**：Rust leg 逐 workspace 產生 partial，每個 partial 的 SCIP document paths 正規化為 git-root-relative 後在 rust family 內部合流（pre-concat），再走**零改動**的既有 `merge_and_publish`；同時 identity 升 v2——Cargo manifests/locks/workspace topology 進入 freshness trust boundary（修掉「Cargo 變更→假 fresh」的既有缺陷），v1 Rust stamp 強制 stale。

四條凍結邊界（與來信 AC 對齊）：
1. Cargo workspace 邊界是 architecture invariant（DB-80 凍結）；CR composite graph 是 observation model——root graph 出現跨 project references 不推斷架構違規，no-cross-path-dep gate 照舊。
2. `merge_and_publish`/`StagedLeg` 契約零改動（pre-concat 承載多 root）；單 workspace 行為完全不變。**Supersession 聲明**：CR-3 卡 Plan 範圍段的「StagedLeg 擴 (family, project_root)＋merge_and_publish 同族 N-way」（來源＝codex advisory 原案）由本 EP Q3 pre-concat 案**取代**——收斂依據＝`references/research.md`（改 `find(family)` 波及全 family 與 fast path）；開工 card amendment 同步改寫範圍句。
3. `--stamp-meta` 反洗白語義不動：手動 restamp 永不認證 current sources。
4. **向後相容性邊界（EP review 裁決）**：identity 升 v2 對既有 v1 非 Rust slot 造成**一次性可觀測退化**（identity-mode → legacy-signals、identities 欄 → null，直到該 repo 下次 rebuild restamp v2）——接受並揭露（legacy-signals 本非 fresh，不說謊；雙軌 face-scoped algo 的複雜度大於收益）。見 SM-13／TC-9。

### 凍結裁決（雙腿 advisory＋雙腿設計＋caller 兩次一手實測收斂，勿重辯）

- **Q1（發現）**：Combined——`.code-reality.toml` 新增 `rust_projects = ["rel/root", ...]`（權威，`VALID_KEYS` 擴充）；缺省時保守 auto（repo 根無 Cargo.toml → 枚舉直系子目錄）；歧義（多個 implicit package 或形態混合）fail-loud `CR-RUST-ROOT-06` 要求顯式 config。`--rust-root` CLI flag 不做。
- **Q2（path normalize）**：每個 Rust partial 在 stage 後、合流前 parse SCIP protobuf，rewrite 每 `Document.relative_path` 前綴 project-root-relative；symbol ID 永不改寫。**`"."`-root 特例**：root 為 `.` 時不加前綴（沿用 `js_ts_corpus::normalize_rel` 語義）——防 `./` 前綴破壞 doc-set 逐位元組相等閘。
- **Q3（合流粒度）**：Pre-concat——rust family 內逐 root 產生→驗證→normalize→依 sorted root order `concat_scip` 成單一 rust partial；跨 root DEF collision gate 在合流前執行。**取代** advisory 原案 staged-multi-leg（見凍結邊界 2）。
- **Q4（identity v2）**：`IDENTITY_ALGO` → `sha256-v2`（domain header `cr-identity-v2\0`）；**`rust_inputs` 走 `compute_identity` 獨立參數，不進 `records()`**——faces／detected_faces／`fingerprint_for_faces`／`paths_for_faces` 一律只看源碼記錄（防「Cargo-present、rs-absent」repo 被蓋假 rust face）；topology descriptor（sorted canonical roots＋discovery mode＋member manifests/locks＋toolchain config＋present/absent markers）進 identity 與 Observation A 比較；v1/legacy Rust stamp 回報 `identity-algo-upgrade` 強制 stale。**config bytes 不逐字入 identity 的聲明**：config 的效果面（discovery mode＋canonical roots）由 topology descriptor 承載——roots/mode 變動必變 digest、config 外觀編輯不假 stale（TC-5 reorder 冪等覆蓋），優於逐 byte 雜湊。
- **Q5（分段）**：S1 root resolver → S2 freshness v2 → S3 Rust composition＋collision gate → S4 E2E acceptance。
- **Q6（zero-roots fallback——review 裁決）**：「Rust face 被 detection 偵測但 resolve 出零 roots」（既有 fixture 形態：全 repo 無任何 Cargo.toml）＝**legacy parity fallback**——整 repo 視為單一 root `.`、走現行單 leg 路徑、不啟用 coverage gate；行為與現狀逐位元組同形。
- **Q7（coverage gate 範圍——review 裁決）**：coverage gate（fatal）僅在 **resolved roots > 1 或 config 模式**啟用；單 root（含 fallback `.`）維持現行 doc-set WARN 語義——RA 合法漏產文檔（非 member 散落 .rs、fixture、產生檔）不對已服務的單 workspace repo 形成新錯誤面。
- **Q8（resolve 兩層設計——review 裁決）**：`resolve_structural`（純 TOML：`[workspace]` 表在場即 root 候選；**query/freshness 面唯一來源**，永不 spawn `cargo metadata`——守住 ≈40ms steady-state gate）＋`validate_authoritative`（`cargo metadata --no-deps` 驗 workspace_root==candidate，**僅 build staging 前執行**）。**兩面一致性契約**：metadata 驗證失敗的 root 不得進 topology descriptor（stamp 面與 query 面的 roots 集合才會逐位元組同源）；implicit `[package]` 案例由 build 面晉升（restamp）後才進 identity。

## UC 盤點

### Backlog 關聯
- CR-3（To Do）：本 EP 的追蹤卡——開工雙 ref 掛本 EP 路徑（`ai-analysis/2026-10/10-09-db100-multi-root/ep.md`）；開工 card amendment 改寫 Plan 範圍句（凍結邊界 2 supersession）。
- 自動建卡結果：S1–S4 屬同一 EP 弧（單卡單 EP 形態）。

### 掃描範圍
- `AGENTS.md` Capabilities：`:107` One-shot data-plane preparation（build 語義）、`:112` Source identity freshness face（identity v1 規格行）、`:96` Read-chain index maintenance。
- `backlog task list --plain`：CR-1（Done）、CR-2（Done）、CR-3（To Do）——無同域卡。

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過）。

### 同主題 memory 條目（結案蒸餾範圍）
- 零命中（memory 池僅 gov-probe fixtures）。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| One-shot data-plane preparation | ✅ | AGENTS.md `:107` | 更新 | build 支援 multi-root composition；producer 字串帶 `[roots: ...]` |
| Source identity freshness face | ✅ | AGENTS.md `:112` | 更新 | identity 升 v2；`stale_reasons` 增 `identity-algo-upgrade`；規格行同步（**場景錨點 SM-1/3/7/11 與本 EP SM 表撞名——`:112` 行更新時加 EP 前綴或重編號**） |
| Read-chain index maintenance | ✅ | AGENTS.md `:96` | 無影響 | ensure_indexes 語義不變 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| Multi-Cargo-workspace root composition（逐 workspace Rust leg＋path normalize＋pre-concat＋collision gate） | 📋 | `crates/code-reality/src/rust_projects.rs`＋`src/build.rs` |
| Rust producer-input identity（Cargo/topology 進 freshness trust boundary，v2） | 📋 | `src/engine.rs`＋`src/identity.rs`＋`src/freshness.rs` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | dual-workspace repo root build | repo 根無 Cargo.toml、兩個直系子目錄各為 `[workspace]` root | auto 發現兩 roots；單一 root index＋graph 覆蓋兩 workspace＋root JS/TS/py；meta stamped v2 | `build` exit 0；faces 字串含 rust；`graph_query search` 命中兩 root 的符號 | root composition |
| SM-2 | 單 workspace repo | repo 根有 Cargo.toml | 行為與現狀逐位元組同形（單 leg；face `rust`）；無新錯誤面 | 既有 `tests/build.rs` 全綠零改動 | root composition |
| SM-2b | zero-roots legacy parity | Rust face 被 detection 偵測、全 repo 無 Cargo.toml（既有 fixture 形態 `tests/build.rs` mkrepo） | fallback 單 root `.`、現行單 leg 語義、coverage gate 不啟用；與現狀逐位元組同形 | t5-t7/t9/t12 全綠零改動 | resolver |
| SM-3 | Cargo.lock-only 變更後查 freshness | 只改 lockfile，源碼不動（v2 slot、multi-root/config） | `freshness` exit 1、`fresh=false`、`serves=committed-baseline`；**不得** fresh | JSON `stale_reasons` 含 content-drift | rust-input identity |
| SM-4 | 第三 workspace 出現（config 模式） | `.code-reality.toml` 列兩 roots、第三個 `[workspace]` 子目錄出現 | fail loud `CR-RUST-ROOT-07`（exit 2），不得靜默縮減覆蓋 | 錯誤文本含新 root 名 | resolver |
| SM-4b | resolver 失敗於 freshness／auto-heal 面 | config 非法或 SM-4 態時跑 `freshness`／觸發 auto-heal | `freshness` exit 2（check-failure，錯誤文本含 CR-RUST-ROOT-*，**不宣稱 fresh**）；auto-heal ServeStale＋WARN（不迴圈不重派） | exit code＋stderr 斷言 | resolver |
| SM-5 | 歧義 discovery | 根無 Cargo.toml、子目錄含 implicit `[package]` 與 `[workspace]` 混合 | fail loud `CR-RUST-ROOT-06` 要求顯式 `rust_projects` | exit 2 | resolver |
| SM-6 | 跨 root symbol collision | 兩 workspace 產生同 symbol、不同定義位置 | build 在 publication 前 fail（exit 2），live slot byte-identical；**root 內**既有 duplicate-target 行為不變 | 錯誤含兩邊位置；collision 測試斷言 node count | collision gate |
| SM-7 | 舊 v1 slot 查 freshness | Rust face slot 由 v1 stamp（含 **keyless meta**——無 `source_faces` 可判、detected walk 含 rust） | `stale_reasons` 含 `identity-algo-upgrade`、exit 1、`serves=committed-baseline`；auto-heal 重建 restamp v2；**不得**以 legacy-signals 回 `fresh=true` | JSON 斷言 | rust-input identity |
| SM-8 | root snapshot／delta_tour | git HEAD 提交後 snapshot；兩次 commit 間 delta_tour | `_meta.commit` 正確錨定 HEAD；files/module paths 全 root-relative；delta tour 含實際變更的 root-relative Rust paths | 對照 `git rev-parse HEAD` | E2E |
| SM-9 | rust_projects config 非法值 | 絕對路徑／`..`／重疊／缺 manifest／非 workspace root／**present-but-empty 陣列** | 各自 fail loud `CR-RUST-ROOT-01..07`（exit 2；空陣列歸 `01`） | 錯誤碼逐一（映射表見 TC-8） | resolver |
| SM-10 | 部分 Rust root 產生失敗 | 第二 root 的 rust-analyzer 中途失敗 | 整體 Env error exit 2；舊 slot/index/graph byte-identical 保留 | attempt 清理；slot mtime/hash 不變 | composition |
| SM-11 | boundary gate | `tests/contract/workspace-boundary.test.mjs`（delegate-bridge repo） | 完好且綠（本 EP 零改動對方 repo） | S4 dogfood 步驟 | E2E |
| SM-12 | query 面熱路徑無 metadata | v2 slot 上連續 `freshness` 呼叫 | 全程零 `cargo metadata` spawn（structural resolve 承載）；steady-state gate 維持 ≈40ms 級 | 計時／process 斷言 | resolver |
| SM-13 | v1 非 Rust slot 的一次性退化 | python-only／TS-only repo 的 v1 slot 在 v2 工具下查 freshness | `serves=legacy-signals`、identities 欄 null（**非 fresh、不說謊**）；該 repo 下次 rebuild restamp v2 後恢復 `current-tree`——一次性，揭露於 `:112` 同步 | JSON 前後對照 | rust-input identity |
| SM-14 | Cargo-present、rs-absent repo | python repo 帶 stray Cargo workspace fixture（零 `.rs`） | 不獲 rust face（faces 推導只看源碼記錄）；rust_inputs 不觸發 rust leg；該 Cargo 編輯不影響該 repo identity scope | faces JSON 斷言 | rust-input identity |

## 測試規劃段

**author_family**: zcode（EP 作者與實作者同家族——implement 階段 RED 前須 pre-RED challenge 或 degraded 明示記錄）。

TC 凍結如下（凍結後改動走 amendment 附錄）：

| TC | claim | Given-When | oracle（predicate-ID 拆分） | oracle_source | evidence | uncovered |
|---|---|---|---|---|---|---|
| TC-1 | normalize 後每個 document path＝`<project_root_rel>/<old_rel>`（`"."` root 不加前綴）且解析到磁碟實檔 | fake RA 產 workspace-relative docs 的兩 root fixture＋單 `.`-root fixture → build | TC1-a：全部 docs 以正確前綴存在（`"."` 時無 `./`）；TC1-b：symbol strings 逐位元組不變；TC1-c：無 `..`/絕對路徑殘留 | caller＋codex 對 dutymail 1.96.0 實測（H——兩次獨立） | L2 | RA 未來版本錨定變更（負向測試會抓） |
| TC-2 | pre-concat 合流＝Σ docs 且零改動 `merge_and_publish` | 兩 root partial → 合流 | TC2-a：合流 partial doc count＝Σ 且路徑集合相等；TC2-b：`merge_and_publish`/`StagedLeg` 原始碼零 diff（rg 斷言）；TC2-c：`tests/build.rs` 既有測試（t5-t7/t9/t12 含 zero-roots fixture）零改動全綠（**Q6 fallback 承載**） | EP 凍結設計（S——Q3/Q6） | L2 | — |
| TC-3 | 跨 root DEF collision 在 publication 前拒絕 | fixture：兩 root 同 crate name/version、同 symbol 不同定義位置 | TC3-a：exit 2 含雙方位置；TC3-b：live slot `artifact_hash` 不變；TC3-c：同 root 內 duplicate-target 不新增拒絕（既有 RA 行為對齊） | graph_db INSERT OR IGNORE 語義（S——凍結於 SM-6） | L2 | within-root 語義的 upstream 變更 |
| TC-4 | Cargo-only 變更 → stale | v2 slot（multi-root/config）；只改 Cargo.lock 內容 | TC4-a：`freshness` exit 1；TC4-b：`stale_reasons` 含 content-drift；TC4-c：`serves=committed-baseline` | identity v2 規格（S——凍結於 Q4） | L2 | 全域 `~/.cargo/config`、外部 toolchain（明示不覆蓋） |
| TC-5 | v2 content-addressing 同構＋config 效果面 | 同 content＋topology、不同 checkout 路徑；config 註解/空白編輯 | TC5-a：identity 值相等；TC5-b：mtime-only 觸碰不變值；TC5-c：byte-exact spec pin（比照 `identity_entry_format_pin`）；TC5-d：config 外觀編輯（roots/mode 不變）identity 不變 | TC-1a 先例＋v2 header 定義＋Q4 效果面聲明（S） | L2 | — |
| TC-6 | v1/legacy/keyless Rust slot 升級語義 | 人工造 v1-stamped slot（faces 含 rust）＋keyless meta slot（detected walk 含 rust） | TC6-a：`stale_reasons` 含 `identity-algo-upgrade`；TC6-b：exit 1 不得 fresh；TC6-c：auto-heal 觸發重建且 restamp v2；TC6-d：keyless 態同列 fatal | engine.rs:910-960 guard 語義＋本 EP 凍結（S） | L2 | 非 Rust face legacy 政策路徑（不動——見 TC-9） |
| TC-7 | E2E AC1-4 | 真實 git fixture：兩 Cargo workspace＋root .py＋.mjs；build→freshness→snapshot→delta_tour；commit-to-commit 變更；Cargo-only 變更 | TC7-a：graph 含兩 root＋JS/TS/py 節點且三 faces stamped；TC7-b：snapshot `_meta.commit`＝git HEAD；TC7-c：delta tour 路徑 root-relative；TC7-d：node count＝Σ queryable DEF sets（跨 root 交集零斷言）；TC7-e：Cargo-only 後 freshness stale | 來信 AC1-4＋本 EP SM 表（S） | L4（真 git＋真 RA） | derived views UX、跨語言 process edges |
| TC-8 | resolver 錯誤碼 | SM-9 非法形態全集 | TC8 映射表：absolute/`..`/empty-entry/empty-array→`01`；canonicalize 失敗→`02`；duplicate/overlap→`03`；manifest 缺損→`04`；非 workspace root→`05`；歧義 auto→`06`；unlisted 第三 workspace→`07`；coverage 缺漏（gate 範圍內）→`08`；逐一 exit 2＋文本斷言；合法 auto/config 排序可重現 | codex 錯誤碼表＋本 EP 補映射（S——凍結） | L2 | — |
| TC-9 | v1 非 Rust slot 一次性退化（揭露性） | python-only repo 的 v1 slot 在 v2 工具下查 freshness → rebuild → 再查 | TC9-a：rebuild 前 `serves=legacy-signals`、identities null、**非 fresh**；TC9-b：rebuild 後 `serves=current-tree`、identities 帶值；TC9-c：`stale_reasons` 不含 identity-algo-upgrade（該值僅 Rust-inclusive） | SM-13 凍結＋Q4 裁決（S） | L2 | — |
| TC-10 | faces 推導隔離 | SM-14 fixture（Cargo-present、rs-absent） | TC10-a：faces JSON 無 rust；TC10-b：rust leg 不觸發；TC10-c：該 Cargo 編輯不變 identity | Q4 裁決（S） | L2 | — |
| TC-11 | 兩面 roots 集合一致性 | stamped（build 面 validated）vs query 面 structural resolve | TC11-a：無 metadata 驗證失敗的 root 進 descriptor（build 面）；TC11-b：freshness 面全程零 `cargo metadata` spawn；TC11-c：兩面 roots 集合相等（同一 repo 狀態） | Q8 凍結（S） | L2 | 環境級 metadata 差異（明示不覆蓋） |

**pre-RED challenge**（full＋trust-boundary 觸發）：implement 階段 RED 前跑——fresh context 盲推 TC4/TC5 的 identity 構造再比對凍結規格；findings 走 amendment。

### TC amendment 附錄

| TC | old oracle | new oracle | reason | authority |
|---|---|---|---|---|
| TC-9 | v1 非 Rust slot downgrade 後「非 fresh」（`assert_ne!(code,0)`） | `serves=legacy-signals`＋identities null＋reasons 無 `identity-algo-upgrade`；**fresh 隨既有 legacy trio**（源碼未變＝fresh=true 合法） | 「legacy-signals 本非 fresh」是措辭錯誤：serves 是降級揭露欄非 stale 宣告；強制 stale 會讓全部存量 v1 非 Rust repo 一次性爆 stale——違反凍結裁決「非 Rust face legacy 政策不動」（TC-6 uncovered 欄）。實作期發現（tc9 紅因剖析），independent evidence＝freshness verdict JSON（fresh=true＋serves=legacy-signals 並存） | 新 integration evidence（verdict 實測）→judge |
| TC-7 | `identity_algo == "sha256-v1"` 字串斷言（source_identity.rs :82/:189/:538/:577/:604） | `"sha256-v2"` | EP 凍結 Q4：algo 換代 v1→v2 是本 EP 核心變更的既定後果；byte-exact spec pin（identity_entry_format_pin）同步 v2 header 釘新值 | EP 凍結裁決（Q4，本檔）→producer 側記錄 |
| TC-2c | 「tests/build.rs 既有測試零改動全綠」 | t6/t7（＋js_ts_build s1）fixture 補碟上 `crates/a.rs`/`crates/b.rs`（S3 修正腿 F2 的磁碟實檔檢查使 rich.scip docs 必須在場）；**斷言零削弱**、zero-roots fallback（t5）未動 | 磁碟實檔檢查（TC-1a impl 面）的必然伴隨 fixture 修正——與 S3 scip_bytes metadata amendment 同型（fixture-only，朝 TC-1a 規格對齊） | 修正腿實測（全套零 FAILED）→judge |
| SM-2b | fallback「與現狀逐位元組同形」 | 限縮為「**published index 語義同形**＋新增磁碟前置條件（docs-on-disk gate）」——fallback partial 現也過 normalize（protobuf roundtrip）且 producer 字串帶 `[roots: .]` | 弧級 intent 腿指出：磁碟實檔檢查＋normalize 全面化後 fallback 的嚴格逐位元組宣稱不再成立；行為面（exit 0＋slot 語義）不變 | 弧級 intent 腿（HEAD 對照已驗）→judge |
| TC-3 | 跨 root collision gate 掃全部 DEF occurrences | 增加 carve-out：`local ` 前綴 symbol（rust-analyzer 局部變數——crate-local 命名空間）不進 collision scan | 真倉 dogfood 實測（delegate-bridge root build 第一擊）：`local 0` 跨 root 必撞且語義無關——兩 workspace 的局部變數是無關實體；假 fixture 的 crate-qualified symbol 蓋不到此形態 | 真倉 dogfood 實測（H——真 RA 1.96.0 carrier 行為）→judge |

## 段落劃分原則

依賴方向單向：S1（resolver，純新增＋config 面）→ S2（identity v2，消費 S1 的 structural resolve）→ S3（composition，消費 S1 roots＋validated authority＋S2 capture 面）→ S4（E2E，全部）。每段獨立可編譯可測；S2/S3 之間以 structural inventory 為共享契約。

---

## S1 — Rust root resolver（`rust_projects.rs`＋profile 擴充）

### Context
- **UC**：Multi-Cargo-workspace root composition（發現面）。
- **依賴關係**：被 S2（identity inventory）與 S3（build composition）消費；動 `profile.rs`（`VALID_KEYS` 加 `rust_projects`）。
- **語義約束**：與 S2/S3 共享「resolved roots 是 canonical、sorted、repo-relative、非重疊」＋Q8 兩層設計契約；`scan_root`（py/boundary 面）語義不動。
- **基礎設施盤點**：`profile.rs` loader（crash-only、missing→Ok(None)）可複用擴充；`language.rs::LanguageFace` 不動。
- **依賴錨點**：`Profile`/`load_profile` → 定義 `src/profile.rs:46-52,56-80` / 消費 `src/engine.rs:614`（walk）、`src/build.rs`（新）；`count_sources` → 定義 `src/build.rs:122-156` / 消費 `:484`（不改 detection）；`walk_sources` 全 caller 清單（review 補全）：`build.rs:246`（collect_py_corpus）、`engine.rs:817`（evaluate_staleness）、`:1049`（ProductionIdentity::capture）、`build.rs:806`（cache repopulate）、`build.rs:1083`（heal doc-delta）、`engine.rs:1401`（source_line）、`js_ts_corpus.rs:29`（collect_js_ts_corpus——TS 語料，讀 js/ts 鍵行為相容）、`examples/scip_fixture_producer.rs:80`。
- **成功標準**：TC-8/TC-11 全綠；單 workspace 與 zero-roots 回歸同形。

### 核心實作要點
1. 新模組 `src/rust_projects.rs`：`pub(crate) struct RustProjects { roots: Vec<String> }`＋**兩層面**——`resolve_structural(repo, profile) -> Result<RustProjects, BuildError>`（純 TOML：`[workspace]` 表在場即 root 候選；query/freshness 面唯一來源）＋`validate_authoritative(repo, &roots) -> Result<(), BuildError>`（`cargo metadata --no-deps` 驗 workspace_root==candidate；**僅 build staging 前執行**，stderr 入錯誤）。
2. Config 面（`rust_projects` 陣列）：非空（**present-but-empty 陣列＝`01`**）、relative、canonicalize-inside-root、去 `..`/絕對、canonicalize 後去重與 ancestor/descendant 拒絕；`"."` 允許。
3. Auto 面（無 config）：根有 Cargo.toml → `["."]`（另有獨立子 workspace → 拒）；根無 → 枚舉直系子目錄（TOML `[workspace]` 表在場）；歧義 → `06`；子 member crate 不是 root——誤認即拒。
4. **Zero-roots fallback（Q6）**：Rust face 被 detection 偵測但 structural resolve 出零 roots（無任何 Cargo.toml）→ roots=`["."]`、單 leg 現行語義、coverage gate 不啟用。
5. Config 模式下偵測未列入的 `[workspace]` 子目錄 → `07`。
6. 錯誤碼 CR-RUST-ROOT-01..08（設計 advisory §Q1 表）＋**`profile.rs:77` 未知鍵錯誤訊息的合法鍵清單同步**（drift：訊息硬編清單須含新鍵）。
7. `walk_sources` 的 resolver 整合：structural resolve 失敗在 walk 內 **fail-loud（Err 傳播）**——`engine.rs:1401` 的 `.ok()` 吞錯點同段修正（resolver 錯誤禁靜默折進「無 drift」分支）。

### Pseudo Code
```rust
pub(crate) struct RustProjects { pub(crate) roots: Vec<String> } // repo-relative, sorted, deduped

pub(crate) fn resolve_structural(repo: &Path, profile: Option<&Profile>) -> Result<RustProjects, BuildError> {
    match configured_roots(profile) {              // profile.rust_projects（empty array -> 01）
        Some(list) => { validate_structural(list, repo)?;  // 01..03（TOML 面：relative/canonical/dup/overlap）
                        scan_unlisted_workspaces(repo, &list)?;   // 07
                        Ok(RustProjects { roots: sorted(list) }) }
        None => auto_discover(repo),                   // root TOML? "." : child-scan（[workspace] 表）；歧義 06
    }
}
// build staging 前：validate_authoritative(repo, &roots)  // 04/05 via cargo metadata；失敗 root 不進 topology
// walk_sources 內：Rust face detected && structural==zero roots -> fallback roots=["."]（Q6）
```

### 驗證策略
- TC-8（`tests/rust_projects.rs`）＋TC-11（兩面一致性）＋SM-2b 回歸（既有 `tests/build.rs` fixture 全綠零改動）。
- 單 workspace 回歸：既有套件全綠零改動。
- 前期 POC：真實 delegate-bridge repo 跑 resolve（唯讀）——收錄於 S4 dogfood。

---

## S2 — identity v2：Rust producer inputs 進 freshness trust boundary

### Context
- **UC**：Rust producer-input identity（v2）。
- **依賴關係**：消費 S1 的 `resolve_structural`；`ProductionIdentity::capture`（build.rs 三 capture 點 `:551-554,659-683`）與 `evaluate_staleness`（engine.rs:817,910-960）同步擴充；`stamp_meta_locked`（engine.rs:1113）加新欄位 stamp。
- **語義約束**：與 S3 共享「rust_inputs 不混入 rs doc set／records()」；`stale_reasons` 詞彙擴充為 JSON additive。
- **基礎設施盤點**：`compute_identity`（identity.rs:271-315）參數擴充；`artifact_hash`（engine.rs:1161）可複用；identity cache 隨 algo 升級廢棄（`corrupt_cache_discards_and_recomputes` 慣例——v1 cache 對 v2 slot 無效自動重算；per-file (size,mtime) cache 閘對 rust_inputs 同樣生效，熱路徑成本僅 lock mtime churn 的單檔 re-hash）。
- **依賴錨點**：`IDENTITY_ALGO` → 定義 `src/identity.rs:32` / 消費 `engine.rs:931-939`、`freshness.rs:135`；`walk_sources` → 定義 `engine.rs:610` / 消費清單見 S1。
- **成功標準**：TC-4/5/6/9/10 全綠；`AGENTS.md:112` 同步。

### 核心實作要點
1. `SourceWalk` 增 `rust_inputs: BTreeMap<String, SourceRecord>`——由 `resolve_structural` 的 roots inventory 展開（每 root `Cargo.toml`、`Cargo.lock` present/absent marker、成員樹內全部 `Cargo.toml` 保守全含、root/locale `rust-toolchain(.toml)`、`.cargo/config(.toml)`）。**rs 面（SCIP doc set）與 `records()` 不動——rust_inputs 只走 `compute_identity` 獨立參數**（Q4／review F4）。
2. `IDENTITY_ALGO="sha256-v2"`、header `cr-identity-v2\0`：`SHA256(header-v2 || sorted face records（現行格式） || domain-separated sorted rust-input/topology records)`。無絕對路徑、無 mtime、無 metadata 時戳。TC-1a 同構保持（TC-5a）。
3. topology descriptor：sorted canonical roots＋discovery mode＋expected input rel-paths（present/absent）——config bytes 不逐字入（效果面承載，TC-5d）；build 面以 `validate_authoritative` 通過的 roots 為 descriptor 來源（**兩面一致性：metadata 驗證失敗的 root 不進 descriptor**）。
4. `evaluate_staleness`：stamped meta 缺 v2 三元組、或 algo=v1、或 keyless，且（stamped faces 含 rust 或 detected walk 含 rust）→ `stale_reasons` 增 `identity-algo-upgrade`、fatal to Rust-inclusive freshness 與 auto-heal；**非 Rust face 的 v1 slot 走既有 legacy-signals（不 fatal——TC-9 揭露其一次性退化）**；keyless meta 且 detected walk 含 rust ⇒ 同列 fatal（review F7）。
5. Observation A 三 capture 點擴 topology/config 效果面——build 中 root 到貨即拒。
6. `stamp_meta_locked` validated 分支 stamp 新欄位；manual restamp preserve 規則不動。
7. 非目標：全域 `~/.cargo/config`、shell 環境 Cargo 變數、toolchain 升級偵測。

### Pseudo Code
```rust
// identity.rs
pub const IDENTITY_ALGO: &str = "sha256-v2";
pub const IDENTITY_HEADER: &[u8] = b"cr-identity-v2\0";
// compute_identity(repo, records, rust_inputs, topology, faces, ...) {
//   h.update(IDENTITY_HEADER);
//   for r in sorted(records):     h.update(face\0rel\0size\0sha256\0)          // 現行源碼面
//   for r in sorted(rust_inputs): h.update(RUST_INPUT_DOMAIN\0rel\0size\0sha256\0present)
//   for t in sorted(topology):    h.update(TOPO_DOMAIN\0root\0mode\0marker)
// }  // rust_inputs 是獨立參數——不進 records()，faces 推導只看源碼記錄（Q4/F4/TC-10）

// engine.rs evaluate_staleness（在既有 :931-939 guard 前加 pre-check）：
let rust_scope = stamped_faces.map_or(false, |f| f.contains(Rust)) || detected_walk.rs_nonempty();
if rust_scope && (stamped_algo != IDENTITY_ALGO || stamped_triple_missing() || keyless_meta()) {
    stale_reasons.push(IdentityAlgoUpgrade); needs_rebuild = true;   // fatal，never fresh
}  // 非 rust_scope 的 v1 slot 照走既有 guard（legacy-signals——TC-9 揭露一次性退化）
```

#### Invariant Impact（freshness trust boundary 是跨 context invariant）
- **受影響 domain invariant**：freshness「never claims fresh without evidence」鐵律（v1 下 Cargo 變更可假 fresh＝既有缺陷，本段修復）；identity content-addressing 同構（TC-1a/TC-5a）。
- **critical path 觸及**：freshness verdict 面（SM-3/7/13）＋build publication fence（Observation A）＋query 熱路徑（SM-12）。
- **驗證對齊**：TC-4a-c、TC-5a-d、TC-6a-d、TC-9a-c、TC-10a-c、TC-11a-c——producer 自證。

---

## S3 — Rust composition：multi-root stage＋normalize＋pre-concat＋collision gate

### Context
- **UC**：Multi-Cargo-workspace root composition（產生面）。
- **依賴關係**：消費 S1 `resolve_structural`＋`validate_authoritative`；消費 S2 v2 capture 面；**不改** `merge_and_publish`/`StagedLeg`。
- **語義約束**：與 S2 共享分離契約；root 內既有 duplicate-symbol 行為保持不變——gate 只抓跨 root 衝突。
- **基礎設施盤點**：`scip::types::Index::parse_from_bytes`（build.rs:279 先例）；`concat_scip`（:192）；`validate_partial`（:212）；attempt 子目錄（:521 `stage_dir`——**現由 TS 腿 lazily 建立（ts_producer.rs:250），ORDERED 序下 Rust 腿先行——composition loop 首行 `std::fs::create_dir_all(stage_dir)`（review F11）**）。
- **依賴錨點**：`stage_rust` → 定義 `src/build.rs:374` / 消費 `:604`（呼點改）；`concat_scip` → 定義 `:192` / 消費新 loop＋`project` overlay（既有消費不動）；`merge_and_publish` → 定義 `:432` / 消費 `:718`（零改動，TC2-b）；`graph_db.rs:795-809` rel-path 解析。
- **成功標準**：TC-1/2/3 全綠；SM-1/2b/6/10 成立；真 delegate-bridge root build exit 0。

### 核心實作要點
1. composition loop（`:604` 呼點改寫；fallback 態 roots=`["."]` 走同 loop 單迭代）：逐 root `stage_rust` → `validate_partial` → normalize（`"."` root 不加前綴；拒絕絕對/`..`/空/非正規分隔符/canonicalize 逃逸/合流內碰撞）→ DEF collision scan（跨 root 同 symbol 異定義位置→fail，含雙方位置；同 root 重複 target 不新增拒絕）→ **coverage gate（僅 roots>1 或 config 模式啟用——Q7；單 root 維持現行 WARN）**：normalized docs ⊇ governed rust walk 根內源碼，缺漏 fail loud `08` → sorted order `concat_scip` → `load_index` 驗證合流 candidate。
2. 合流後推 `StagedLeg { family: Rust, path: rust-composed.scip }`——下游零改動。
3. producer 字串：`rust-analyzer <version> [roots: <sorted rel roots>]`；失敗逐 root 報；舊 slot 全程不動。
4. 空 corpus 收斂：現行 all_policy_empty 語義照舊。

### Pseudo Code
```rust
let stage_dir = slot_dir.join(format!(".stage-{attempt}"));
std::fs::create_dir_all(&stage_dir)?;                                   // review F11——Rust 腿先於 TS 腿
let rust_projects = RustProjects::resolve_structural(&resolved, profile.as_ref())?;  // 含 Q6 fallback
RustProjects::validate_authoritative(&resolved, &rust_projects)?;       // Q8 build 面（失敗 root 已不進 topology）
let composed = stage_dir.join("rust-composed.scip");
let mut defs: HashMap<Symbol, Vec<(root, rel, pos)>> = ...;
let gate_fatal = rust_projects.roots.len() > 1 || config_mode;          // Q7
for (i, root) in rust_projects.roots.iter().enumerate() {
    let raw = stage_dir.join(format!("{i}-raw.scip"));
    let ver = stage_rust(repo.join(root), &raw, roots)?;                // :374 本體不動
    validate_partial(&raw)?;
    normalize_scip_paths(&raw, root)?;                                  // "." 不加前綴；symbol ID 不動
    collision_scan(&raw, root, &mut defs)?;                             // 跨 root 異位 -> Env error
    coverage_gate(&raw, root, &rust_walk, gate_fatal)?;                 // gate_fatal=false -> 現行 WARN
    if i == 0 { fs::copy(&raw, &composed)? } else { concat_scip(&composed, &raw)?; }
}
engine::load_index(&composed)?;
staged.push(StagedLeg { family: ProducerFamily::Rust, path: composed });
```

### 驗證策略
- TC-1（含 `.`-root 斷言）/TC-2/TC-3（新 `tests/multi_rust_build.rs`：fake RA per-invocation workspace-relative docs）；`tests/build.rs` 既有測試零改動全綠（TC2-c）。
- kill criteria #1/#2 的 Probe 在本段首個測試執行。
- 真倉 dogfood（S4）。

---

## S4 — E2E acceptance（AC1-4）＋真倉 dogfood

### Context
- **UC**：兩個新 UC 的整合驗收。
- **依賴關係**：S1+S2+S3 落地後執行；不含新架構。
- **語義約束**：無新假設；delegate-bridge repo 零改動（`.code-reality/` sidecar 資料面除外——gitignored 隔離層；清理建議歸對方主權）。
- **依賴錨點**：`snapshot.rs:304-329`；`delta_tour` snapshot-pair；`freshness.rs` verdict 面。
- **成功標準**：TC-7 全綠＝來信 AC1-4 成立。

### 核心實作要點
1. 新 `tests/multi_rust_root_e2e.rs`：真 git fixture——build→freshness→snapshot→delta_tour→Cargo-only→第三 workspace→resolver 失敗態（SM-4b）。
2. dogfood（手動腿）：delegate-bridge root `build`＋`freshness`＋`graph_query search` 雙 root 符號；對方 boundary test 照跑（SM-11）。報告回寄 delegate-bridge-marshal（outward，屆時 AUTH）。
3. 效能觀察（不設 AC）：root build 耗時記錄（anchor ≈8 分鐘級）。

### 驗證策略
- TC-7a-e；SM-1/3/6/8/10/11/12 真倉對照。
- caveat：within-root duplicate-symbol 是 RA 既有行為——oracle 用 queryable DEF set 差集；snapshot HEAD 錨定與源碼 freshness 分開驗。

---

## 整合策略

- **baseline**: `25fd932f6b1cbe0fca3cfa95c4c26975c2bf9abf`
- 分支慣例：卡 WT 形態（cr-3 branch；repo marshal admission guard 強制——原「直行 main」計畫偏差見 S1 進度區偏差②）；弧尾一次 commit 經 merge 收線。
- 版本面：隨下次 release（本 EP 不動 workspace version）。
- 文檔同步面（implement 5b）：`AGENTS.md` `:107`（multi-root 語義＋producer 字串）、`:112`（v2 規格＋`identity-algo-upgrade`＋**SM-13 一次性退化揭露**；`:112` 行內既有場景錨點加 EP 前綴或重編號）、Capabilities 兩行新增；`plugin/skills/code-reality-tools/SKILL.md` 對應工具事實面。
- 下游消費者：`graph_query`/`scip_refs`/`hub_refs`/`snapshot`/`delta_tour`/`freshness`/MCP 全 faces 零介面變更；`lsp-bridge`/wrapper 零改動；`project` overlay 既有 `concat_scip` 消費不受影響。
- **跨 repo 詞彙消費面（review I2）**：收尾時 `rg "stale_reasons|identity-algo" ~/.agents/skills/cr-query/SKILL.md`——有引用則回信 ai-guide-marshal 提案同步（不直接寫對方 repo）。

## 收尾步驟（implement 階段 5/6 執行）

1. **Capabilities＋Kanban**：AGENTS.md Capabilities 新增兩行＋更新 `:107`/`:112`；CR-3 結案兩步；消費場景自 SM 表提煉寫卡 notes。
2. **SYSTEM-MAP**：無（跳過）。
3. **instruction 檢查**：`crates/AGENTS.md` 職責描述同步；`plugin/skills/code-reality-tools/SKILL.md` 同步；drift 掃描 `rg "sha256-v1|cr-identity-v1" --glob '!target' --glob '!references'`＋跨 repo 詞彙檢查（上節）。
4. **/audit-test**：新增測試品質稽核。
5. **弧結案蒸餾**：bridge_task glm 派工 mcpSupply 接線缺口（db-71 提案在飛）；GLM 腿 32MB 截斷（partial-delivery receipt 判讀）；EP review 抓出的 zero-roots／兩面一致性設計缺口（fresh-eyes 價值實證）。

---

## EP Review 區段（規劃期帳本——review-engine boundary profile：fresh+intent 雙腿）

**執行記錄**：fresh-eyes 腿（code-reviewer，源碼全讀＋錨點抽查全中）verdict **needs-changes**；intent-primed 腿（code-reviewer-primed，意圖對齊）verdict **approve**。

| # | 來源 | 級別 | 標題 | 裁決 | 落點 |
|---|---|---|---|---|---|
| F1 | fresh | P0 | zero-roots 情境未定義＋coverage gate 打爆既有 fixture（mkrepo 無 Cargo.toml→resolve 空 roots） | ✅ 採納 | Q6（legacy parity fallback）＋Q7（gate 範圍限縮）＋SM-2b＋TC2-c |
| F2 | fresh | P1 | algo 升 v2 → v1 非 Rust slot 一次性退化 legacy-signals（「非 Rust 政策不動」可觀測面不成立） | ✅ 採納（方案 b：揭露＋TC，不雙軌） | 凍結邊界 4＋SM-13＋TC-9＋`:112` 同步揭露 |
| F3 | fresh | P1 | resolve() metadata 職責矛盾（熱路徑成本 vs 兩面一致性） | ✅ 採納 | Q8 兩層設計＋SM-12＋TC-11 |
| F4 | fresh | P1 | rust_inputs 進 records() 與 faces 推導矛盾（假 rust face） | ✅ 採納 | Q4 修正＋TC-10＋SM-14＋S2 pseudo |
| F5 | fresh | P1→併 F1 | coverage gate 對單 workspace 是行為退化 | ✅ 併入 Q7（gate 範圍限縮） | S3 |
| F6 | fresh | P2 | `.`-root normalize 特例未釘 | ✅ 採納 | Q2＋TC-1 |
| F7 | fresh | P2 | keyless legacy Rust slot 升級分類未定義 | ✅ 採納 | SM-7＋TC-6d＋S2 要點 4 |
| F8 | fresh | P2 | walk_sources caller 清單不完整＋source_line `.ok()` 吞錯 | ✅ 採納 | S1 錨點清單＋要點 7（fail-loud） |
| F9 | fresh | P2 | resolver 失敗於 freshness/auto-heal 語義缺 SM | ✅ 採納 | SM-4b＋TC-8 |
| F10 | fresh | P2 | profile.rs 未知鍵訊息硬編清單須同步 | ✅ 採納 | S1 要點 6 |
| F11 | fresh | P2 | composition loop 寫 stage_dir 前無 create_dir_all（Rust 腿先於 TS 腿 ENOENT） | ✅ 採納 | S3 pseudo 首行 |
| F12 | fresh | P2 | TC-8 九態對八碼計數不齊；空陣列無歸屬 | ✅ 採納 | TC-8 映射表（空陣列→`01`） |
| I1 | intent | P2 | card↔EP supersession 缺標記（卡範圍寫舊案 staged-multi-leg） | ✅ 採納 | 凍結邊界 2＋開工 card amendment |
| I2 | intent | P2 | `stale_reasons` 新值跨 repo 消費面（ai-guide cr-query）缺同步檢查 | ✅ 採納 | 整合策略末項＋收尾 3 |
| I3 | intent | P2 | EP 中文內容位於非豁免路徑 `2026-10/`（English-content rule） | ✅ 採納 | 任務家搬 `ai-analysis/2026-10/10-09-db100-multi-root/`（已執行） |
| I4 | intent | P2 | `:110` 列號錯置（實為 `:96`） | ✅ 採納 | UC 盤點掃描範圍 |
| I5 | intent | P2 | SM-n 標籤與 AGENTS.md `:112` 既有場景錨點撞名 | ✅ 採納 | UC 既有 UC 表說明＋整合策略 |
| I6 | intent | P2 | config bytes 不逐字入 v2 的裁決未顯性聲明 | ✅ 採納 | Q4 效果面聲明＋TC-5d |

**Accepted eligibility**：ledger 全 terminal（上表 18 項全 implemented）＋修正回寫完成——本 EP 具備 `/implement` accepted-EP 進場資格。

---

## 段落進度（implement 階段 2 收斂記錄）

### S1 — Rust root resolver（2026-10-09 收斂）

1. **產物**：`crates/code-reality/src/rust_projects.rs`（新）＋`src/profile.rs`（VALID_KEYS 5 鍵＋`Profile.rust_projects: Option<Vec<String>>`＋parse＋合法鍵訊息同步）＋`src/lib.rs`（pub mod）＋`tests/rust_projects.rs`（26 測試）＋`tests/s4b_hazard_hubrefs.rs`（literal 補欄）。working tree＝卡 WT（cr-3 branch），未 commit（弧尾一次 commit——EP「每段一顆 commit」修訂，偏差記 plan_changes）。
2. **實跑**：`cargo test -p code-reality --test rust_projects` → 25 passed / 0 failed；`cargo test -p code-reality --lib profile` → 12 passed（回歸）。RED receipt：`red-receipts.md`（test 檔 digest 1031cdf…、pre-change compile-fail 紅）。
3. **未驗與下一步**：`validate_authoritative` 的真實 cargo metadata 只在單測 fixture 驗（真 delegate-bridge 兩 workspace 的 authoritative 驗證留 S3 composition＋S4 dogfood）；walk_sources 的 resolver 整合（要點 7 fail-loud＋Q6 fallback 接線）歸 S2。
4. **review 覆蓋**：EP 設計面已由 EP Review 雙腿覆蓋（ledger）；段級 review **round-1 verdict needs-changes（2 P1＋3 P2）→ 全採 apply → 修正後 27/27 綠，loop 收斂**——F1 auto "." 分支靜默回縮覆蓋（補 sibling workspace 掃描→06，測試拆修）、F2 member fixture vacuous-green（05 mismatch 分支真覆蓋＋`workspace_root=` 斷言）、F3 合法鍵訊息補 hazard_registry、F4 新釘 config ["."]＋[package] members、F5 repo canonicalize 失敗補 02 碼＋String/BuildError 型別收斂記 S2。
5. **偏差記錄**：①resolver 對 repo base 先 canonicalize（macOS /var→/private/var symlink——containment 比對基準須統一；EP pseudo 未寫，實作必要）②EP「直行 main」→ 卡 WT 形態（repo marshal admission guard 強制 wt-open.sh；EP 整合策略行過時）③validate_authoritative 的 cargo bin 取 PATH（EP 無 roots 注入參數；測試環境 cargo 必備）④fixture 修正：standalone package manifest 須帶 src/lib.rs（cargo metadata 要求至少一個 target）。

### S2 — identity v2（啟動材料，2026-10-09）

- 測試模板：`tests/source_identity.rs` 的 Lab fixture（fake py/ts deriving producers、`mkrepo`＋git_init、`stamped_meta` 讀 slot meta、`clean_identity` 走 walk+compute_identity）。**`mkrepo` 不含 Cargo.toml**——TC-4/5 的 lab 須補 root manifest（或 multi-root 子目錄）；fallback 態（無 manifest）rust_inputs 恆空、Cargo-only 場景不存在。
- S2 時 build 仍單 leg 呼叫（stage_rust 呼點 S3 才改）——rust_inputs 只動 identity/walk 面；`validate_authoritative` 的 build 接線歸 S3。
- rust_inputs 收集語義：rust face scope 的 repo（含 "." root 單 workspace）收 root/member manifests/locks/toolchain config；無 manifest 的 fallback repo → 空 rust_inputs。
- 段級 review 觸發：trust-boundary（TC 凍結面）＋v1→v2 一次性退化語義。

### S3 — Rust composition（2026-10-09 收斂）

1. **產物**：`src/build.rs`（`stage_rust_multi`：resolve→validate_authoritative→逐 root stage→normalize→collision scan→coverage gate→pre-concat→rename 取代 part；三 helper `normalize_scip_paths`/`collision_scan_defs`/`coverage_gate`；staging loop Rust 分支改呼＋create_dir_all）＋`tests/multi_rust_build.rs`（5 測試）。**impl-lite 腿收尾**（marshal 派工——ArcOverride：main session 直做→marshal 派工，user 指示）。working tree 卡 WT，未 commit（弧尾一次 commit 不變）。
2. **實跑**（marshal 機械複驗非自述）：`cargo test --test multi_rust_build` 5/5；全套 45 套件全 ok（559 passed）零 FAILED；build 零 warning。diff 範圍全在 EP scope（8 modified＋rust_projects.rs 新檔）。
3. **fixture amendment 兩筆（impl-lite 提報、marshal 追認）**：①`scip_bytes()` 補 `Index.metadata`（ToolInfo＋project_root）——原合成索引 120 bytes 撞 frozen 空索引閘（EMPTY_INDEX_BYTES=128，build.rs:76 註解明言 trap 102-122B、合法最小 725B；真 RA 1.96.0 輸出必帶 metadata；前例 rich.scip 1747B 帶 metadata）；②TC-3 shared symbol 修入 rust.scip 的 crates/b.rs——原 fixture 兩 root 從未定義同 symbol、碰撞 gate 無標的（測試意圖缺陷，非斷言改動）。
4. **未驗與下一步**：真 rust-analyzer 的 normalize 面歸 S4 dogfood（kill criteria #1 Probe）；resolver 失敗態 freshness/auto-heal 面（SM-4b）歸 S4 E2E 場景。
5. **review 覆蓋**：段級 review spawn 排隊（外部整合觸發）；弧級 review 排隊。
6. **偏差記錄**：⑦spawn prompt 的紅因假設（呼叫鏈未對齊）被 impl-lite 實測否證——真紅因＝fixture 缺陷（DBG 證據 per-root 呼叫正確）——lite 腿的實證紀律正面樣本；⑧red-receipts S3 條目由 marshal 補記（腿未記）；⑨malformed profile 的 exit family 漂移（rust 臂 Core=1 vs py 腿 Env=2——pre-existing 非 S3 引入，統一收斂歸後續清潔弧，本弧不擴 scope）。
7. **段級 review verdict approve（零 P0/P1，四項 P2＋一項 note）→ 修正腿二波已派**：F1 coverage_gate 死碼 prefix 刪除；F2 normalize 拒絕清單補 backslash＋磁碟實檔檢查（TC-1 impl 面）；F3 collision 測試補舊 slot byte-identical oracle＋測試名去撞號；metadata last-root 註解。核心凍結面（Q2/Q3/Q6/Q7/Q8、TC-1/2/3、cleanup/atomicity、contract 零改動）逐項核對無行為缺陷；rename atomicity（stage_dir→slot_dir 同 FS）與 cleanup 完整性 ✓。

### S4 — E2E＋真倉 dogfood（2026-10-09 收斂）

1. **產物**：`tests/multi_rust_root_e2e.rs`（5 測試，impl-lite 腿，src 零改動）——TC7-a（build fresh 三 faces 雙 root graph＋rust_topology stamp 斷言）/TC7-b（snapshot 錨 HEAD）/TC7-c（delta_tour root-relative 變更路徑，**無降級**——真 snapshot pair）/TC7-e（Cargo-only stale）/SM-4b（resolver 失敗 exit 2 不宣稱 fresh）。
2. **實跑**（marshal 複驗）：`cargo test --test multi_rust_root_e2e` 5/5；全套 46 targets **565 passed** 零 FAILED；RED 靈敏度探針在卷（移除跨檔 REF→斷言紅→還原綠）。
3. **真倉 dogfood（marshal 本職，delegate-bridge root，sidecar 資料面）**：
   - `build`：**exit 0**，producer `rust-analyzer 1.96.0 [roots: dutymail, rust]`、py 4＋ts 98 governed docs
   - 第一擊抓到真 bug：collision gate 誤傷 `local N`（RA 局部變數 crate-local 命名空間，跨 root 必撞無語義；假 fixture 的 crate-qualified symbol 蓋不到）——修正腿 skip `local ` 前綴＋tc3c 新測試（RED→GREEN 往返）→重跑 **exit 0**
   - `freshness`：fresh=True、sha256-v2、三 faces、serves=current-tree（**AC2 真倉實證**）
   - `graph_db build`：**3787 節點/14667 邊**（舊 root graph dutymail 0 節點→現涵蓋雙 workspace，**AC1**）；`graph_query search` 命中兩 root 符號且 qualified_name root-relative
   - **對方 boundary test：18 pass / 0 fail**（**AC4/SM-11**——delegate-bridge repo 零源面改動）
   - 觀察項（不擴 scope）：跨 root `local N` 流入合流 index 後由 graph_db UNIQUE+INSERT OR IGNORE 留首 root 節點——既有 single-root duplicate 同型語義；local 不進 graph 屬新設計決策，記後續觀察
4. **review 覆蓋**：S4 測試面由弧級 review fresh 腿覆蓋（全弧 scope）；弧級 intent 腿對齊卡 AC 十項；AC#10 dogfood 本節即證。
5. **弧級 fresh 腿 findings 處置（needs-changes：1 P1＋4 P2）**：P1 config entry 非正規形態（`./x`/`x/`）靜默擊穿 Q7 coverage gate＋污染 graph 路徑→修正腿三波（正規化＋mode 真值＋第三 workspace E2E 補測）；P2 mode 零熵常數→同腿（Q4 discovery mode 帶真值）；P2 TC2-c fixture 增檔未記 amendment→已補帳（上表第三筆）；P2 S4 進度區缺記錄→已補（本節）；「第三 workspace build face E2E」併修正腿；SM-4b auto-heal 後半（ServeStale＋WARN）與 TC-11-b（零 metadata spawn 的機械 oracle）**遞延聲明**——前者結構性由 needs_rebuild gate 承載（heal 觸發條件即同一判定）、後者結構性由 `validate_authoritative` 單呼叫點保證（build.rs 一處），兩者補測價值低於成本，明示不補；P2 帳本半同步→正文已回寫＋偏差⑨改記 latent（capture A 先於 staging loop、walk_sources 載 profile 失敗即 Err→Env——malformed profile 恆 exit 2，rust 臂 Core 為不可達 latent 態，行為無害）。

### 修正迴圈收斂（跨家族 review → 5.3 judge → 修正腿五波，2026-10-09）

1. **跨家族 review**（bridge；codex job-mv0pi8d6 task face 路徑承載——review face 撞 28K web 閘後照階梯 (b)）：verdict needs-changes，3 P1＋1 P2——**全為 GLM 系五輪未抓到的跨家族盲點**（face scope 違反／ancestor config 漏收集／orphan.rs 擊穿 coverage／symlink containment）。
2. **flash 腿交付截斷**（finalText 233 字，findings 未取得——部分交付第二次實證）：顯性降級記錄，GLM 家族視角由 5.3 judge＋五輪 in-harness GLM 審查承載。
3. **5.3 judge**（bridge GLM-5.3 旗艦 job-mv0qj0mn＋mcpSupply）：**5 採納 / 0 不採納 / 0 需確認**——P1-A/B/C/D 全採（各帶控制流逐行證據＋終態修正：P1-C 實害非 false-fresh 而是「靜默部分發佈＋永久 stale/heal 反覆」——freshness 永不宣稱 fresh 由 F7 upgrade 守衛攔下）＋**judge 獨立新 finding P3-E**（topology roots 逗號 join 假相等）＋窗口論證（v2 未發佈，identity body 現在改零成本）。
4. **修正腿五波**（impl-lite）：五項全修＋補測 9 枚（全 RED→GREEN）——face scope 閘（identity.rs 兩域包 rust-face 條件）、explicit pin 勝磁碟、ancestor 鏈入 rust_inputs（markers BTreeSet 去重）、orphan → CR-RUST-ROOT-08（producer spawn 前 fail fast）、stamp 靜默跳過改 rep.notes WARN、canonical containment（symlink escape 拒絕）、topology `root[idx]` 編碼。
5. **marshal 驗收**：全套 46 targets **578 passed** 零 FAILED 零 warning；真倉 dogfood 重驗（identity body 變更後）：delegate-bridge root build exit 0＋freshness **fresh=True／sha256-v2／三 faces／current-tree／reasons=[]**——修正迴圈收斂。

### S2 — identity v2（2026-10-09 收斂）

1. **產物**：`src/identity.rs`（v2 常數＋`compute_identity_scoped` 帶 rust_inputs/TOPOLOGY domain＋wrapper 轉發）、`src/engine.rs`（`SourceWalk.rust_inputs/rust_topology`＋`collect_rust_inputs`＋resolver 整合 fail-loud＋`StalenessSnapshot.identity_algo_upgrade`＋needs_rebuild 併入＋capture scoped＋`rust_topology` 欄）、`src/build.rs`（cache repopulate scoped）、`src/freshness.rs`（`identity-algo-upgrade` reason＋serves 裁決：upgrade 態恆 committed-baseline）、`src/profile.rs`/`src/rust_projects.rs`（S1 修正輪）；`tests/rust_identity_v2.rs`（7 測試）＋`tests/source_identity.rs`（5 處 v1→v2 amendment）。working tree 卡 WT，未 commit。
2. **實跑**：`cargo test -p code-reality` 全套 → 44 套件全 ok 零 FAILED（含 rust_identity_v2 7/7、source_identity 15/15、lib 65/65、rust_projects 27/27）；零 warning。
3. **未驗與下一步**：multi-root（N≥2 roots）下的 rust_inputs/identity 面歸 S3 composition＋S4 E2E 真倉驗；`refresh.rs` 的 head-sync restamp 面（preserve 六鍵＋rust_topology 第七鍵）由既有 refresh 測試覆蓋（全綠）——真 commit 燈號歸 S4。
4. **review 覆蓋**：段級 review（trust-boundary 觸發）排隊 spawn；弧級 review 排隊。
5. **TC amendment 兩筆**（EP 附錄已記）：TC-9 oracle 修正（legacy-signals 非 stale 宣告——「非 fresh」措辭錯誤，實測 verdict 為證）；TC-7 及 source_identity 五處 v1→v2 字串（Q4 凍結裁決既定後果）。
6. **偏差記錄**：⑤compute_identity 簽名不變——新增 `compute_identity_scoped`，舊函數變 wrapper（~30 caller 零 churn，EP pseudo 的「新 typed input parameter」以 scoped 函數承載）；⑥walk 端 rust_inputs 收集條件＝detected rust face 非空（EP「Rust in stamped/current face scope」的 query 面落地——stamped 無 faces 可看、detected 是唯一機械判準）。
7. **段級 review round-1 verdict needs-changes（3 P2，核心 approve-grade）**：F1 `engine.rs` source_line 的 `walk_sources(r).ok()` 仍吞 resolver 錯誤（S1 要點 7 具名點——歸下一波修正腿，與 S3 findings 併派）；F2 `AGENTS.md:112` v1 spec 行＋`stale_reasons` 詞彙＋SM-13 揭露未同步（**遞延記錄**：維持收尾 5b 執行，含 I5 錨點撞名處理；drift 掃描 `rg "sha256-v1"` 屆時打中三檔為預期）；F3 `identity_entry_format_pin` doc comment v1 殘留（併修正腿）。核心 trust-boundary 逐項核對無行為缺陷（domain 分隔/collision 面升級裁決/Observation A fence/faces 隔離全 ✓）。
