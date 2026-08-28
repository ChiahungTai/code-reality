# EP: data-plane unification — sidecar 遷入 `<repo>/.code-reality/`（~/.mosaic 退役）

> **ep_type**: implementation
> baseline: e041286

Source: user 設計討論 2026-08-29（第三次問「資料到底放哪」後的
arch-thinking 弧）。裁決反轉先前「sidecar 留集中」立場——查證後
in-repo 勝出，理由鏈：

1. **鍵與位置的一致性**：index/cache 是 per-checkout 派生數據（鍵＝
   工作樹＋rev），資料應與鍵同住。現行 `basename` 鍵＋home 位置＝
   鍵錯＋分離雙重不一致；freshness/mtime/sidecar-invalidation 機械
   全是在為分離買保險。
2. **path-hash central ≡ in-repo（語義等價）**：修好撞名後 central
   也是 per-checkout——「跨 checkout 共享」利益在正確鍵下不存在。
   等價下選 staleness 偵測最簡（同樹同 fs）、code 最少（零鍵計算）、
   rm checkout 即清者＝in-repo。
3. **樹污染是邊際成本**：graph.db（最大 1.5G）早已在樹內、`target/`
   23G 在樹內是 Rust 常態；sidecar 全量 783M（NT 602M＋offline
   106M＋mosaic×2 各 32M＋自倉 11M＋ai-rules 212K）相對小。
4. CRG 同構佐證：`.code-review-graph/` 全 per-repo＋自帶 .gitignore。

**決策類型**：雙向門（資料可搬回、git 可回）。

## 現況盤點（2026-08-29 實查）

- **接觸面三常數**：`engine.rs:17 DEFAULT_INDEX_ROOT`（scip）、
  `boundary_build.rs:19`（boundary）、`snapshot.rs:19`（snapshots）
  ＋鍵邏輯 `engine.rs:322-342 default_index_path`（basename 鍵）；
  pyrefly-producer 側有對稱的 slot 落點（emit 面）。
- **slot 盤點**：NT 602M（index.scip 268M＋cache db 232M＋union 83M
  ＋fndefs 18M）、offline_backtesting 106M、mosaic_alpha 32M、
  trading_lab 32M、code-reality 11M、ai-rules 212K、snapshots 1.3M、
  `__pycache__` 殘留。
- 開著的 bug：snapshot 對自建 graph 回 **0 files**（WARN 自述
  「graph.db 與 --repo 不同 root？」）——跨 root 漂移類，本 EP 的
  釜底抽薪候選。

## UC 盤點

### Backlog 關聯
- 自動建卡：本 EP 追蹤卡（Backlog）＋新 UC 卡合一（單卡）。

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（跳過）。

### 掃描範圍
- root AGENTS.md Capabilities／crates/AGENTS.md／.kanban/。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Symbol truth query 家族 | ✅ | AGENTS.md | 更新 | slot 解析改 in-repo（消費者 CLI 面不變——仍 `--repo`） |
| Self-owned graph db | ✅ | AGENTS.md | 更新 | build 源頭改讀 in-repo slot |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| 統一 in-repo 資料面（sidecar＋自帶 .gitignore；~/.mosaic 退役） | 📋 | `crates/*/src` 三常數＋鍵邏輯＋搬遷工具 |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 既有 repo 首次跑新工具 | 任一查詢/producer | 新 slot 落 `<repo>/.code-reality/scip/`＋自帶 `.gitignore`；`git status` 乾淨 | 無 | 新增 UC |
| SM-2 | 舊 home slot 在場 | 搬遷命令 | 冪等搬遷（同盤 mv 瞬移）；重跑=零動作 | 無 | 新增 UC |
| SM-3 | 兩個同名 repo／同 repo 多 worktree | 各自 producer | 各自 `<repo>/.code-reality/` 天然隔離（basename 撞名類死亡） | 無 | 新增 UC |
| SM-4 | rm -rf checkout | 使用者 | 資料隨樹消失，home 無孤兒 | 無 | 新增 UC |
| SM-5 | index 比 cache 新（同樹內） | producer 重跑後 build | mtime 閘/lsp fast-path fail-loud **保留作動**（同樹對齊仍需） | S3 | graph db |
| SM-6 | 搬遷中斷 | Ctrl-C | 冪等重跑收斂；舊 home slot 未刪前雙源期讀 in-repo 優先 | 無 | 新增 UC |
| SM-7 | 消費端 repo 不想加 .gitignore | 任意 | **零設定**——`.code-reality/.gitignore`（`*`＋`!.gitignore`）工具自寫 | 無 | 新增 UC |
| SM-8 | snapshot 0-files bug | 對自建 graph 跑 snapshot | 預期被治（跨 root 漂移消失）；未治則獨立修 | S3 | 觀察項 |

## 段落劃分原則

S1（常數＋鍵＋自帶 gitignore）→ S2（冪等搬遷＋退役驗證）→
S3（閘門盤點＋snapshot bug 驗證）→ S4（文檔/ai-rules 翻轉）。
S1 是行為變更核心；S2 依賴 S1；S3/S4 在 S2 驗證後。

---

## S1: slot 解析改 in-repo＋自帶 .gitignore

**Context**
UC 引用：實作「新增 UC：統一 in-repo 資料面」。改動面刻意最小：
三常數＋`default_index_path`＋producer 對稱落點。
- 依賴錨點：`engine.rs:17`＋`engine.rs:322-342 default_index_path`
  （定義端）→ 消費端 `scip_refs`/`graph_db build`/producer emit
  （rg `default_index_path|DEFAULT_INDEX_ROOT` 全列）；
  `boundary_build.rs:19`、`snapshot.rs:19` 同型。
- 語義約束：與 S2 共享「新位置＝`<repo>/.code-reality/{scip,boundary,
  snapshots}/`」；`meta.json` 的 repo 欄位語義不變。
- 顯式 `--index-root`/`--out` 覆蓋路徑（若在場）行為不變——只改
  default。

**要點**
- `default_index_path` 改為 `<resolved-repo>/.code-reality/scip/
  index.scip`（basename 鍵邏輯整段刪除）；兩個 DEFAULT_OUT_DIR
  同型改 in-repo。
- `.code-reality/` 建立時自寫 `.gitignore`（內容 `*`＋`!.gitignore`
  ——CRG `incremental.py:272` 模式；寫在自己的資料目錄內，不觸
  消費端 repo config，plugin-stance 相容）。
- 空 basename 防護邏輯隨鍵邏輯退休。

**驗證策略**
- 既有測試全綠（fixtures 走顯式路徑/tempdir，預期零改或小改——
  若有測試釘死 home 路徑則改釘 in-repo 路徑）。
- 新增回歸：producer→查詢→build 全鏈在 tempdir repo 上走一次，
  斷言 slot 落 in-repo＋`.gitignore` 存在＋`git status --porcelain`
  為空（臨 git repo fixture）。
- 效能期待：無（路徑解析次序不變）。

## S2: 冪等搬遷＋`~/.mosaic/code-reality` 退役

**Context**
UC 引用：完成「新增 UC」的存量面。搬遷對象=S1 前的 home slots。
- 語義約束：與 S3 共享「雙源期（home 未刪）讀 in-repo 優先」；
  搬遷後 sidecar 檔 mtime 保持（mv 語義）——mtime 閘依賴它。

**要點**
- 搬遷命令（`code-reality sidecar-migrate --repo <repo>` 或隨
  S1 的 lazy 首跑搬遷——build 時決）：home slot 存在且 in-repo
  缺→同盤 `mv`（NT 602M 瞬移）；兩邊都在→in-repo 優先＋WARN；
  重跑=零動作（冪等）。
- 逐 repo 驗收後刪 home slot；全清後 `~/.mosaic/code-reality/`
  整目錄退役（含 `__pycache__` 殘留）。

**驗證策略**
- 每個 repo：搬遷→`scip_refs` 一查＋`graph_db build` 重建→
  與搬遷前輸出 byte-identical（資料面零損失）。
- 重跑冪等斷言（第二次=零動作）；中斷重跑收斂。

## S3: staleness 閘門盤點＋snapshot bug 驗證

**Context**
語義約束：**逐閘評估、非一刀刪**——同樹內 index vs cache 的
mtime 對齊（SM-5）、lsp fast-path mtime 閘、producer sidecar
失效（㊶ 弧）**全保留**；可退休的是「跨 root 漂移」類的防禦性
假設（若有）。

**要點**
- 盤點清單：freshness face（binary↔checkout——不動）、graph_db
  build mtime 閘（留）、producer invalidation（留）、snapshot
  "不同 root" WARN 路徑（重驗）。
- **snapshot 0-files bug 對照實驗**：in-repo 遷移後重跑——治了
  則收案記錄；沒治則獨立立案（不擴本 EP scope）。

**驗證策略**
- 每個保留閘的既有測試仍綠；退休項列明理由表。

## S4: 文檔／ai-rules 翻轉

**要點**
- repo README「Sidecar home」段改 in-repo 敘述＋消費端零 gitignore
  設定說明；AGENTS.md Usage 段同步；`crates/AGENTS.md`。
- ai-rules handoff：code-reality SKILL.md 若提 sidecar 路徑同步
  翻轉（觸發條件＝本 EP build 完成）。

## NOT（scope boundary）

- **不動** graph.db 位置（已在正確位置）。
- **不動** launchd/HTTP 面。
- **不做** path-hash 鍵（議題隨 in-repo 死亡）。
- **不刪** staleness 閘（只盤點退休候選，同樹對齊類全留）。
- Windows 語義不對齊的 `#[cfg(not(unix))]` stub 不處理（EP NOT 沿襲）。

## 整合策略

- baseline: `e041286`。
- S1+S2 可同 session（S1 code＋S2 逐 repo 搬遷驗收）；S3+S4 收尾。
- 全 repo 驗收清單：nautilus_trader、mosaic_alpha、
  mosaic_alpha_offline_backtesting、mosaic_alpha_trading_lab、
  ai-rules、code-reality（自倉 dogfood）。

## 收尾步驟

1. Capabilities：新增「統一 in-repo 資料面」行；既有兩行附註。
   Kanban 搬 Done＋EP 歸檔。
2. 無 SYSTEM-MAP.md——跳過。
3. instruction 檔：AGENTS.md（root＋crates）Usage/Capabilities 同步。
4. /audit-test：S1 有 callable 變更（default_index_path 語義改）——
   正常跑；新增回歸測試在稽核範圍。
5. ai-rules handoff prompt 交付（sidecar 路徑翻轉，觸發＝build 完成）。
