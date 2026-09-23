# 段落 0 研究材料：source identity（dirty-WT/content-aware freshness）

- 產出：cr-research agent（唯讀；CR MCP callers/refs 腿＋rg＋定義檔全文 Read；本 session 無 LSP，negative-claim 雙腿以「CR callers」或「rg＋全文 Read」替代，逐條標注腿別）
- 日期：2026-09-23；index baseline `68d7636`（查詢時自癒實證：首個 callers 查詢觸發 `[OK] index healed（14.9s，1459 nodes）`）
- 消費方式：EP 正文只摘要引用；本檔是完整結構化宣稱紀錄（機械可驗收：`fd research.md` 命中＋`rg "\[SRC\]"` 非空）

---

## Q1. query-time staleness 現有成本形狀（最關鍵）

**每次查詢都重新 walk+stat 整個語料——無任何快取。**「每次查詢已付 stat-walk 成本」**成立**；content hash 的增量成本是「讀檔內容＋hash」，不是「walk」。

- 入口：`ensure_fresh`（`crates/code-reality/src/build.rs:1127`）→ 第一件事 `evaluate_staleness(&repo, &slot)`（build.rs:1133）。
- `evaluate_staleness`（`crates/code-reality/src/engine.rs:750-844`）開頭 `let walk = walk_sources(repo)?;`
- `walk_sources`（engine.rs:595-668）無狀態：`read_dir` 遞迴 stack、每 file 一次 `ent.metadata()`（engine.rs:630）取 mtime、建 `BTreeSet`。`SourceWalk` 結構只有 py/rs/js/ts 路徑集＋newest（engine.rs:520-528），**無快取欄位、無 memoization**。
- doc-set drift：`walk.fingerprint_for_faces(faces) != *stamped_fp`（engine.rs:803-806）——同一 walk 物件上算 FNV，**無第二次磁碟往返**。
- 成本公式（stamped slot 每次 scip_refs 查詢）：1×full walk＋N 次 stat＋1 次 meta.json 讀＋**1 次 `git rev-parse` spawn**（head_drift；engine.rs:831-837 `match git_head(repo)`）。engine.rs:737-739 自述：「Cheap staleness evaluation (Stage A): walk + stats + meta read. Zero producer spawns (a stamped meta adds one `git rev-parse`)」。零-spawn 測試 `t13_fresh_noop_is_zero_spawn`（tests/build.rs:791-796）pin 的是**零 producer spawn**，非零 git spawn。
- **heal 內 walk 重複付費**：`run_heal_locked` 一次 heal 至少 3 次 full walk——`build_repo` 內部 producer 語料收集、heal 後再 `evaluate_staleness`（build.rs:999）＋`walk_sources`（build.rs:1008）、`stamp_meta_core` 內又 `walk_sources`（engine.rs:1001）。實證：query-time heal **14.9s / 1459 nodes**。
- single-flight：`acquire_heal_lock`（build.rs:803-837）——`OpenOptions::create_new` 於 `<slot_dir>/.heal.lock`，payload `"{pid} {iso}"`；遺失鎖 mtime > `HEAL_LOCK_MAX_AGE=600s` 可偷（build.rs:793）；等 peer `HEAL_WAIT_BUDGET=120s`／`HEAL_POLL=200ms`（build.rs:795-796）；Drop 釋放（build.rs:785-789）。
- 分流點：`ensure_fresh`（build.rs:1140-1145）churn-cooldown guard 先於 Fresh 短路；`!snap.needs_rebuild()` → `Fresh`（零 producer 工作）；否則搶鎖→`run_heal_locked`。heal 內分流：fingerprint-only drift（無新 mtime）→ false-stale 精確診斷 serve-stale（build.rs:1018-1029）；未收斂 → churn marker＋serve-stale（build.rs:1031-1041）；收斂 → Healed。cooldown 預設 `HEAL_CHURN_COOLDOWN=600s`（build.rs:801），`CODE_REALITY_HEAL_COOLDOWN_SECS=0` 逃生。

## Q2. stamp 面

- `stamp_meta_core`（engine.rs:921-1041）寫 `index.scip.meta.json`（`META_SUFFIX=".meta.json"`，engine.rs:16；`meta_path` engine.rs:434-441）。固定 keys（engine.rs:956-963）：`repo/head/stamped_at/tool/producer/selection`（key 順序＝frozen Python dict order）。
- **anti-laundering 實作位置**（engine.rs:964-1036）：
  - fresh pair 只在 `disk == docs` 時蓋章（engine.rs:1009）→ 寫 `source_faces`/`source_set_fingerprint`，JS/TS face 時加 `js_ts_profile_fingerprint`（engine.rs:1010-1019）。
  - mismatch/失敗 → `preserve_prior_keys` closure（engine.rs:977-991）逐字保三鍵＋WARN（engine.rs:1032-1035）：`"[WARN] stamp-meta：索引文檔集與磁碟語料不一致——保留既有 source-set fingerprint（drift 保持可見；重跑 build 產出一致配對）"`。
  - 註解 design source：engine.rs:964-975「stamping a fingerprint of drifted disk state over an old index would launder a delete into freshness … (codex blocker — the earlier drop-the-keys design degraded to mtime-only)」。
  - 測試：`s4_stamp_preserves_prior_identity_keys_when_docs_differ_from_disk`（tests/js_ts_freshness.rs:254-286）。
- **graph.db metadata writer 單一點**：`stamp_snapshot_metadata`（graph_db.rs:396-410）寫 `git_head_sha`＋`last_updated`（`INSERT OR REPLACE`；git 失敗靜默跳過、退 db mtime）。CR callers：`stamp_snapshot_metadata：1 callers（1 sites）→ graph_db/build_from_cache_at() crates/code-reality/src/graph_db.rs:895` **[SRC]**。rg 腿：生產碼無其他 INSERT `'last_updated'`（餘皆測試 fixture 或 `crg_last_updated`，transition.rs:194/205）。
- 讀者：`snapshot::detect_stale`（snapshot.rs:218-258，sha→last_updated→db mtime 三級）；`load_metadata`（snapshot.rs:263-302，half-set retry 1s）；`graph_db::stale_head_warn`（graph_db.rs:416-431）。

## Q3. 字串鍵互補腿（rg 全 crates/，CR 圖盲區）

| key | 生產碼讀/寫點 | 測試 pin |
|---|---|---|
| `source_set_fingerprint` | 寫 engine.rs:1012；讀 engine.rs:800-806；preserve engine.rs:979-983 | js_ts_freshness.rs 12 測試（`s4_delete_detected_by_fingerprint_not_mtime` :86 等） |
| `source_faces` | 寫 engine.rs:1011；讀 engine.rs:718-735 `parse_stamped_faces`；preserve :979-983 | js_ts_freshness.rs:207/230/254 |
| `js_ts_profile_fingerprint` | 寫 engine.rs:1017；讀 engine.rs:811-817；產生 engine.rs:699-716 | js_ts_freshness.rs:176-204 |
| `head`（meta） | 讀 engine.rs:829-830、:1080-1091 `stamped_head`、`source_line` :1188 | staleness.rs:102-154 |
| `selection`（meta key） | 讀 engine.rs:760-763、engine.rs:948-954（head-sync preserve）；寫入 build.rs:720-724。cache.rs/graph_engine.rs/lib.rs 的 "selection" 命中是**無關語義**（查詢選擇），cli.rs:419 是 doc comment | tests/s3_cache.rs（非 meta 鍵） |
| `git_head_sha` | 寫 graph_db.rs:399；讀 graph_db.rs:420、snapshot.rs:225/284 | graph_db.rs:370-385、s1_foundation.rs:21-47、s2_snapshot.rs:53-216、s4b_hazard_hubrefs.rs:867、s6_mcp_server.rs |
| `last_updated` | 寫 graph_db.rs:406；讀 snapshot.rs:240/288、common.rs:400（註解層）、transition.rs:206（`crg_last_updated` 為**另一鍵**） | graph_db.rs:370-379、s2_snapshot.rs:140-161、s3_transition.rs:299-358 |

## Q4. JS/TS corpus face

- 單一源：`crates/code-reality/src/js_ts_corpus.rs` — `collect_js_ts_corpus`（:28-49）底層就是 `crate::engine::walk_sources`（:29）；doc header（:3-6）：「the producer leg … and the freshness walk … must never answer it twice with different rules (AD-11)」。分隔線正規化 `normalize_rel`（:55-58）。
- face-scoped staleness：`parse_stamped_faces`（engine.rs:718-35，未知 face name → legacy meta）；`newest_for_faces`/`paths_for_faces`（engine.rs:533-558）；eval scope＝stamped ∪ detected（auto）或 pinned（explicit）（engine.rs:782-791）。測試 `s4_face_isolation_python_only_slot_ignores_ts`（js_ts_freshness.rs:207）。
- **identity 延伸插入點（保留 face scoping）**：`SourceWalk` per-face `BTreeSet<String>`（engine.rs:520-527）升格為 (relpath, size, mtime, content_hash) 記錄；`fingerprint_for_faces`（engine.rs:565-588，FNV-1a64 over `"<face>\0<path>\0"`）同構改餵新記錄即天然保 face scope——`paths_for_faces`（:533）是唯一 face→paths 投影點。`walk_sources` 是唯一 walk 實作；producer 側 `collect_py_files` 反向 import `code_reality::engine::SKIP_DIRS`（pyrefly-producer/src/lib.rs:367-370「Skip list single-sourced in code-reality」）。

## Q5. refresh.rs docs-only re-stamp 腿

- **沒有 git-diff 檔案分類**——「docs-only」從 staleness 訊號**推斷**：`refresh_run` 自存 `evaluate_staleness` snapshot（refresh.rs:101），`ensure_fresh` 回 `Fresh` 且 `snap.head_drift == Some(true)` → 只 `stamp_meta_core(..., None, None)` head-sync（refresh.rs:114-125），stderr `[OK] refresh：索引新鮮，meta head 已同步`。
- 「never restamping over corpus drift」不在 refresh.rs 強制，而在 `stamp_meta_core` 內 `disk == docs` gate（engine.rs:1009）——head-sync 走同一函數自動繼承。**identity keys 保護＝進 `stamp_meta_core` 的 fresh/preserve 分支即被這條腿保護，refresh.rs 無需改。**
- 測試：`refresh_docs_only_head_syncs_meta_only`（tests/refresh.rs:246-274）——pin「index bytes untouched」＋「meta head synced」。
- hook 面：burst debounce（refresh.rs:321-323 內嵌 sh）：`refresh.pending`/`refresh.scheduled` heartbeat、`CODE_REALITY_REFRESH_QUIET_SECS` 預設 5s、dead-runner 3×QUIET respawn；註解（refresh.rs:318-320）：「The query-time heal is the backstop when a source-changing tail event is lost」。

## Q6. 既有測試腿盤點

`tests/staleness.rs`（8）：`walk_sources_skips_and_collects`(:32)、`walk_sources_profile_excludes_python_not_rust`(:56)、`evaluate_staleness_trigger_split`(:80)、`evaluate_staleness_head_drift_without_git`(:102)、`evaluate_staleness_head_drift_in_git_repo`(:118)、`walk_sources_rust_face_skips_target`(:156)、`walk_sources_newest_is_max_and_read_failure_is_err`(:173)、`doc_set_delta_face_scoped_missing_extra`(:202)。

`tests/js_ts_freshness.rs`（12）：`s4_delete_detected_by_fingerprint_not_mtime`(:86)、`s4_rename_detected_and_converges`(:130)、`s4_excluded_edit_is_silent_included_edit_stales`(:151)、`s4_profile_policy_change_triggers_rebuild`(:176)、`s4_face_isolation_python_only_slot_ignores_ts`(:207)、`s4_legacy_meta_without_keys_uses_baseline_behavior`(:230)、`s4_stamp_preserves_prior_identity_keys_when_docs_differ_from_disk`(:254)、`s4_ts_producer_drift_note_on_flagged_path`(:289)、`s4_nonconverged_heal_arms_cooldown_not_fresh`(:333)、`s4_all_excluded_corpus_heals_to_empty_then_fresh`(:368)、`s4_auto_index_heals_on_new_language_arrival`(:395)、`s4_torn_graph_lags_slot_forces_heal`(:417)。

`tests/refresh.rs`（16）：`refresh_heals_stale_via_real_producer`(:226)、`refresh_docs_only_head_syncs_meta_only`(:246)、hook 系列(:115-:510) 12 條、`release_first_roots_prefers_non_cargo_home`(:544)、`broken_release_bin_falls_through_to_dev_face`(:576)。

`tests/build.rs`（heal 鏈）：`t13_fresh_noop_is_zero_spawn`(:791)、`t14_stale_heals_and_releases_lock`(:806)、`t15_head_drift_only_no_rebuild`(:828)、`t16_producer_fail_serve_stale_lock_released`(:854)、`t18_false_stale_warns_once_no_loop`(:919)、`t19_lock_escape_and_single_flight`(:941)、`t20_readonly_slot_dir_serves_stale`(:1015)、cooldown 家族(:1202/:1243/:1286)、marker 家族(:1317/:1345)。

`crates/cr-freshness/tests/staleness.rs`（9，binary 軸）：`rev_mismatch_table`(:12) 等。

`crates/pyrefly-producer/tests/end_to_end.rs`：`write_is_atomic_no_tmp_residue`(:65)、`emit_is_byte_deterministic`(:327)、`emit_invalidates_stale_sidecar_artifacts`(:346)、`default_slot_chain_is_in_repo_and_git_clean`(:420 porcelain-clean)、`cjk_sources_emit_consistent_positions`(:469)；`overlay_gen.rs` `golden_e2e_byte_deterministic`(:92) 等 9。

## Q7. 消費端 face 現況

- scip_refs 家族 heal gate：`cli::run`（cli.rs:330-375），僅 `default_resolved && args.repo.is_some()`（explicit `--index` 永不 heal，cli.rs:336-338）；`CODE_REALITY_AUTOHEAL=off` 關閉（cli.rs:339）。WARN 文案群：`[OK] index healed（…）`、ServeStale 各線（build.rs:941/1023/1037/1113）、`[WARN] 索引過期檢查失敗（{e}）——本次查詢以現存索引作答`（cli.rs:370-372）。`source_line`（engine.rs:1177-1269）`[SRC]` 面＋`[WARN] repo HEAD 已離開 index 生成點…`（:1262）。
- CLI JSON keys：`graph_db build --json`（graph_db.rs:1128-1143）與 umbrella `build --json`（build.rs:1165-1176）**無任何 freshness/identity key**。
- --version freshness face：main.rs:16 `cr_freshness::stale_binary_warn`；`version_face()`＝`{pkg}+{rev}`（cr-freshness lib.rs:94-99）。
- 新子命令註冊點：`crates/code-reality/src/bin/code-reality/main.rs:28-70` `route()` `match argv.first()`＋`SUBCOMMANDS: [&str; 20]`（main.rs:72-93）。命名慣例蛇形小寫；argv 自製 `argparse`（禁 clap，main.rs:5-9 frozen-Python argparse semantics）。
- MCP 工具面（mcp_server.rs）：refs 類 `refs/callers/closure/audit`（:376-437）；graph 12 工具；data-plane `build/snapshot/delta_tour/project`（:844-:939）。identity/freshness 承載面候選＝refs 類或新 CLI 子命令。

## Q8. sidecar 佈局與並發

- 實測佈局：`.code-reality/{.gitignore, graph.db, refresh.log, scip/{index.scip, index.scip.meta.json}, snapshots/}`；暫態：`scip/.heal.lock`、`scip/.heal-churn`（build.rs:914-918）、`refresh.pending`/`refresh.scheduled`。data-dir self-gitignore：`write_data_dir_gitignore`（engine.rs:450-462）單一 `*`＋frozen header（不加 `!.gitignore` negation 是刻意的，:444-448）。
- 鎖語義：Q1 已述。**沒有通用鎖 API**——identity cache 並發寫要自帶同型鎖或掛 heal lock 翼展。
- 先例對照：(a) JSON sidecar＝`index.scip.meta.json`（slot sibling、serde_json、無鎖、stamp-time 單寫者；caller 3 生產碼：build.rs:711、refresh.rs:119、cli.rs:469）；(b) sqlite 表＝graph.db `metadata(key,value)`（build-time 單寫 graph_db.rs:895；查詢面 `connect_ro` 唯讀；ensure_indexes 是僅有的 query-time 寫先例 graph_db.rs:1095-1123）。producer 刪 superseded sidecar 先例：pyrefly-producer/src/lib.rs:208-230；umbrella publish 點 build.rs:693-698；空收斂移除全套 build.rs:658-662。atomic write 先例：`write_is_atomic_no_tmp_residue`（end_to_end.rs:65；emit.rs:138/141）。

## Q9. git 依賴與邊界

- **「engine/staleness 路徑零 git 依賴」被否證**：`evaluate_staleness` 在 stamped slot 上**每次** spawn `git rev-parse HEAD`（engine.rs:831-837 → `git_head` engine.rs:1106-1122；git 不在 PATH → head_drift=None 不擋查詢）；`source_line` 亦 spawn（engine.rs:1192-1198）。query steady-state＝walk＋stats＋1 git spawn。
- binary 軸 git 屬 cr-freshness（crates/cr-freshness/src/lib.rs:42-76，dev-face gated）——與 index 軸分離無共用碼。producer build.rs git describe 屬 build-rev 軸。
- **symlink 不對稱**：engine `walk_sources` 用 `ent.file_type()`（engine.rs:614，不跟隨）→ symlink 檔/目錄**靜默跳過**；producer `collect_py_files` 用 `p.is_dir()`（pyrefly lib.rs:373，跟隨）→ symlinked .py 進 index 但 staleness walk 看不見（doc_set extra>0 → false-stale precise branch）。rg `symlink` 生產碼零命中。
- .gitignore 互動＝零：walk 只排 dot-dirs＋`SKIP_DIRS=["__pycache__","venv","node_modules"]`（engine.rs:475）＋profile exclude 前綴（rust 僅排 `target/` 下，engine.rs:638-641，註解記 49 個 OUT_DIR .rs finding :601-608）。

## Q10. 風險清單（content-hash identity 的 load-bearing 假設）

1. **並發查詢×build 競態**（高）：heal 內三重 walk；identity cache 寫入若無鎖可讀到半寫——唯一防線 `.heal.lock`（僅 heal/refresh 序列化，純查詢 Fresh 路徑無鎖）。cache 採 atomic-rename 寫。
2. **cache 污染/laundering**（高）：anti-laundering trust anchor＝「stamp 時 disk==docs 才寫」；per-file (size,mtime)→hash cache 引入第二信任層——mtime 未變但內容變會讓 cache hash 過期仍被採用。需明確 cache validity 語義＋殘餘風險揭露。
3. **大語料成本**（中）：冷 hash＝讀全部語料一次；增量後 steady-state 只付 changed files——changed 偵測靠 mtime（:630），與風險 2 同根。
4. **symlink 不對稱**（中）：N6——hash identity 會放大差異；identity 走 walk 集即繼承。
5. **Windows path 大小寫/分隔線**（中）：rel path 原樣進 BTreeSet（engine.rs:625-629），僅 JS/TS corpus 有 `\`→`/` 正規化（js_ts_corpus.rs:34-39）——identity 鍵須統一正規化。
6. **hash 演算法約束**（低-中）：workspace 零 hash crate（N8）；FNV-1a64 非抗碰撞（engine.rs:561-562 自註「NOT a security boundary」）身份本體不可沿用；byte-deterministic 先例＝producer（emit_is_byte_deterministic :327）；BTreeSet 迭代有序保路徑序。
7. **graph.db/slot 雙 face 漂移**（中）：identity 落 stamp（index 軸）vs graph.db 獨立 `git_head_sha`/`last_updated`——torn data plane guard（engine.rs:821-827）用 graph.db mtime<slot mtime 強制 heal；新 identity 需決定 graph_db 軸要不要跟。
8. **git spawn 已在關鍵路徑**（事實）：head_drift 每 query 一 spawn；純 walk+hash 方向不加 git。

## Q11. 成本 hook 點

- **重用點＝`walk_sources` 單點**：所有 staleness（:751）、corpus（js_ts_corpus:29）、stamp（:1001）、heal 診斷（build.rs:1008）都流經它。hash-on-change 最小接入＝`ent.metadata()` 收集處（engine.rs:630，已 stat 一次）順手取 size+mtime，cache 命中即免讀檔；miss 才 read+hash。**不需要新 walk。**
- cache 落點取捨：(a) sidecar JSON（先例 meta.json：slot sibling、serde_json、無鎖）——與 slot 同生命週期、producer invalidation 契約可照抄；缺點整檔重寫、並發寫需 atomic rename。(b) graph.db sqlite 表——單檔＋rusqlite 既有；缺點 query-time 寫打破「讀鏈唯讀」face 且要自帶鎖。(c) meta.json additive keys——per-file 記錄撐爆人讀 JSON face，不建議。

---

## (a) Negative-claim register

| # | 宣稱 | type | evidence（雙腿） | verdict |
|---|---|---|---|---|
| N1 | freshness/staleness 軸現無任何 content-hash 計算 | 行為缺席 | 腿1 rg `content_hash\|hash_file\|digest\|Sha256\|blake3\|twox\|Md5` 全 crates → 0；腿2 定義檔全文 Read（engine.rs:520-1269、graph_db.rs 相關段、pyrefly walk/lib） | 成立（rg+read） |
| N2 | `last_updated` 生產碼單一寫入點 | 單寫者 | 腿1 CR callers `stamp_snapshot_metadata`＝1 caller（graph_db.rs:895，[SRC] index @68d7636＝HEAD）；腿2 rg 無其他 INSERT | 成立 |
| N3 | `stamp_meta_core` 生產碼 caller＝3（build/refresh/cli）＋1 test | caller 集 | CR callers＝4 sites＋rg 交叉 | 成立 |
| N4 | `walk_sources` 無快取/無 memoization | 結構 | 腿1 全文 Read engine.rs:520-668；腿2 rg `cache` 於 engine.rs 僅指 cache.rs | 成立（read+rg） |
| N5 | walk 不讀 .gitignore | 行為缺席 | rg `gitignore` → 僅 `write_data_dir_gitignore`（self-data-dir） | 成立（rg+read） |
| N6 | engine walk 靜默跳過 symlink；producer walk 跟隨（不對稱） | 行為差異 | 腿1 read engine.rs:613-621 vs pyrefly lib.rs:373；腿2 rg `symlink` 生產碼 0 命中 | 成立 |
| N7 | `graph_db build --json`/umbrella `build --json` 無 freshness/identity key | 介面缺席 | 全文 Read 兩處 render 逐 key 枚舉 | 成立 |
| N8 | workspace 無 hash crate 依賴 | 依賴缺席 | rg `sha\|blake\|xxh\|crc\|digest` 全 Cargo.toml → exit 1＋deps 清單 read | 成立 |
| N9 | metadata 表生產碼寫入僅 `stamp_snapshot_metadata` 一處 | 單寫者 | CR callers（N2）＋rg `INSERT OR REPLACE INTO metadata` | 成立 |

## (b) Open questions（EP 開題裁決見 ep.md「已決策」）

1. hash 演算法：新 dep vs 手寫（FNV 不可用於身份）。
2. cache validity 殘餘風險政策：mtime+size 命中即 reuse 是否可接受。
3. graph.db 軸是否同步升級 content identity。
4. symlink 不對稱本 EP 修否。
5. consumer face：新 CLI 子命令 vs 既有 face 加欄位 vs MCP additive。
6. `selection: explicit`（pinned face）與 identity face scoping 的互動。
