# research 摘要 — DB-100 multi-Cargo-workspace root graph

> 本檔是 EP 段落 0 研究產物的索引與收斂結論；完整材料在 `references/`。

## 研究腿構成（五個獨立證據源）

| 材料 | 形態 | 關鍵貢獻 |
|---|---|---|
| `references/cr-root-advisory-codex-result.md` | 研究 advisory（delegate-bridge 派 codex web 腿） | 方案 (e)：SCIP 層 composition；DB-80 分離承重事實；反合併 blast radius |
| `references/cr-root-advisory-glm-result.md` | 研究 advisory（GLM 腿） | 同一設計獨立收斂；root index 10-03 健康史實；(b) 判死補強（rust-project.json 劫持）；release profile/lock 紀律不對稱 |
| `references/cr-root-implementation-design-codex-result.md` | 設計 advisory（本 EP 主設計輸入；job-mv08v0d8-osov8k） | 五開放點完整裁定＋CR-RUST-ROOT-01..08 錯誤碼表＋逐段測試計畫＋驗收 caveat |
| `references/glm-source-map.md` | 源碼逐行地圖（GLM-5.3 腿；job-mv08v0zy-5emog3；**outputTruncated=true——設計裁定缺席，地圖完整**） | §16 identity 只收 face 檔案實證；§17 argparse 無 repeatable flag 機制；§13 stamp-meta 不重算 identity 的程式碼路徑 |
| caller 一手實測（收信 session） | 兩次獨立實測 | ①`rust-analyzer scip dutymail/` document paths＝`crates/...`（project-root-relative，無 workspace 前綴）②`walk_sources`（engine.rs:610-680）只收 LanguageFace 副檔名——Cargo.toml/lock 不在 identity corpus |

## 收斂裁決（EP 凍結，勿重辯）

| # | 裁決 | 證據強度 |
|---|---|---|
| Q1 | **Combined 發現**：`.code-reality.toml` `rust_projects` 為權威；缺省時保守 auto（root 無 Cargo.toml → 枚舉直系子目錄）；歧義 fail-loud（CR-RUST-ROOT-06）要求顯式 config；`--rust-root` CLI flag 不做（argparse 無 repeatable 機制，YAGNI） | codex 設計＋GLM §17＋雙 advisory 對立觀點合流 |
| Q2 | **document path 必須 normalize**：`rust-analyzer scip <workspace-dir>` 產出 project-root-relative paths；每 partial 在 stage 後、pre-concat 前 protobuf parse＋rewrite `relative_path = project_root_rel.join(old)`；symbol ID 不動 | 三重實證（caller＋codex 各自對 dutymail 實跑 1.96.0） |
| Q3 | **Pre-concat**：rust family 內部合成單一 partial 後交既有 `merge_and_publish`；`StagedLeg`/`merge_and_publish` 契約零改動；跨 root DEF collision gate 在 pre-concat 前執行 | codex 設計裁決（其原 advisory 的 staged-multi-leg 案已自我修訂）＋GLM §2/§5 機制面 |
| Q4 | **identity v2**：`IDENTITY_ALGO` 升 `sha256-v2`（domain header `cr-identity-v2\0`）；`SourceWalk` 增 `rust_inputs` 獨立記錄面（不混入 rs doc set）；topology descriptor（sorted roots＋member manifests/locks＋toolchain config＋present/absent markers）入 identity；v1/legacy Rust stamp＝`identity-algo-upgrade` 強制 stale 不得假 fresh；`--stamp-meta` 反洗白語義不動 | caller＋codex 各自獨立驗證 GLM「walk 已涵蓋」宣稱為偽；identity 規格 pin（identity.rs:3-7） |
| Q5 | **4 segments**：S1 root resolver → S2 freshness v2 → S3 Rust composition＋collision gate → S4 E2E acceptance | codex 設計表 |

## 已驗事實錨點（file:line，baseline 25fd932）

- `build.rs:17-26` cat-merge 合法性＋metadata last-value-wins；`:192-199` `concat_scip`；`:204-207` `StagedLeg{family,path}`；`:374-407` `stage_rust`（唯一 Rust 產生點，`:385-391` 傳 repo dir）；`:432-466` `merge_and_publish`（`:446-447` `find(family)` 靜默掉第二 Rust leg）；`:484-492` detection/selection（副檔名面，不看 manifests）；`:566` part 檔名 family-keyed；`:652-655` StagedLeg 唯一建構點；`:551-554,659-683` Observation A 三 capture
- `engine.rs:610-680` `walk_sources`（只收 `.py/.rs/六 JS/TS`）；`:516-528` `SourceWalk`；`:1034-1078` `ProductionIdentity`（`:1063-1068` profile hash 僅入 in-flight 比較欄）；`:746-754` `needs_rebuild`；`:910-960` identity_algo 比較（mismatch→legacy 三信號）；`:1101-1214` stamp_meta_core/locked（`:1158-1185` 手動 restamp preserve 規則）
- `identity.rs:32-39` `IDENTITY_ALGO="sha256-v1"`＋header；`:3-7` 規格（content-addressed、TC-1a）；`:271-315` `compute_identity`
- `graph_db.rs:795-809` node 位置＝repo_abs.join(rel)——錯誤前綴→節點指錯檔；`nodes.symbol UNIQUE`＋`INSERT OR IGNORE` 靜默掉碰撞
- `snapshot.rs:304-329` `assert_git_root`（root 錨定已備）；`profile.rs:19` `VALID_KEYS` 現值 `["module","exclude","scan_root","hazard_registry"]`
- 測試慣例：`crates/code-reality/tests/`＋`tests/support/mod.rs`（fake producer binaries、git_init、parsed-SCIP 斷言）
- argparse 無 repeatable flag（`Kind` 僅 StoreTrue/Value，last-wins——glm-source-map.md §17）

## 風險假設清單

| # | 假設 | 等級 | 驗證落點 |
|---|---|---|---|
| R1 | rust-analyzer 1.96.0 `scip` 對每個 workspace-dir 都輸出 workspace-relative paths（兩次實測同形，但未 protobuf-dump 全部 documents） | 中 | S3 測試以 fake RA 釘形態；真 leg E2E 再驗 |
| R2 | 跨 workspace symbol 碰撞會發生嗎（crate-qualified 命名理論上隔離） | 中 | S3 負向測試構造同 crate name/version fixture；RA 實測顯示 workspace 內本有 duplicate-symbol 診斷（不炸）——gate 必須只抓「跨 root 不同定義位置」 |
| R3 | `cargo metadata` 在 resolve 驗證中的成本（每次 build 一次可接受；禁入 freshness 熱路徑——用 TOML 本地解析） | 低 | S1 設計（resolve 一次共用）；S2 用 descriptor 不跑 metadata |
| R4 | composed graph 的 derived views（communities/flows）對兩個 disconnected Rust components 的渲染 | 低 | S4 觀察項（兩 components 是正確圖像；UX 不設 AC） |
| R5 | 根 index 建置耗時（兩 rust leg＋js/py；anchor ≈8 分鐘/舊單 workspace root face） | 低 | 低頻 face，S4 實測記錄不設 AC |

## kill criteria（AIR-131）

1. **Assumption**：rust-analyzer scip 的 document relative_path 以傳入目錄為相對根（normalize 可機械補前綴）。**Probe**：對兩個真實 workspace 各跑一次 scip＋protobuf dump 全 documents 檢查前綴形態（S3 首個測試即此）。**Kill observation**：出現混合錨定（部分 git-root-relative 部分 workspace-relative）或路徑含 `..`/符號連結逃逸且無法安全改寫 → 停止 normalize 設計，改查 RA 上游。**Action**：pivot 到 per-root 後處理層（合流後 parse 重寫）或升級討論。
2. **Assumption**：`StagedLeg`/`merge_and_publish` 在 pre-concat 設計下零改動可行（合流後單 Rust partial 行為與現行單 leg 完全同形）。**Probe**：S3 既有單 workspace 測試（`tests/build.rs:331-387`）不動全綠＋合流 partial 過 `load_index`。**Kill observation**：單 leg 快速路徑或 face 標籤語義需要動 `merge_and_publish` → 停，改評估 staged-multi-leg 案。**Action**：pivot EP S3 設計（amendment）。
