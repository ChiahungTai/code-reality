---
id: CR-3
title: multi-Cargo-workspace root graph——SCIP producer 層 composition（DB-100 責任信採納）
status: In Progress
assignee: []
created_date: '2026-10-09 00:36'
updated_date: '2026-10-09 02:37'
labels:
  - feature
  - bridge-request
dependencies: []
references:
  - ai-analysis/2026-10/10-09-db100-multi-root/ep.md
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
delegate-bridge repo 來信請求（dutymail db100-cr-root-graph-solicit-001＋supplement 補充，寄 code-reality-marshal 門牌——本 repo 首封責任信）：CR 原生支援 multi-Cargo-workspace root graph，SCIP producer 層 composition，各 Cargo workspace 保留不動。

**痛點（來信方實測）**：dual-Cargo-workspace repo（rust/＋dutymail/）下 CR root build exit 2（rust-analyzer reports more than one project）、root graph 對 dutymail 面 stale（10/03 實測 0 dutymail nodes；root index 在 dutymail 落地前一日本是健康三語言 index）、snapshot/delta_tour 因 git-root 錨定不可達——每個結構宣稱都要 per-subworkspace 繞路。

**信中承重事實已在本 repo 逐點驗實（2026-10-09 收信驗證）**：build.rs cat-merge N-way POC-verified；stage_rust 單 repo dir（build.rs:374）；merge_and_publish 每族只取第一 leg——塞兩個 Rust partial 會靜默掉一個（build.rs:447）；StagedLeg identity=(family,path)（build.rs:204）需擴 (family, project_root)；graph_db.rs nodes.symbol UNIQUE＋INSERT OR IGNORE——跨 workspace symbol 碰撞會靜默掉 node（需負向測試）。

**雙腿研究收斂**（codex 腿(e)＝GLM 腿(c)，實體相同）：producer 層 composition。反合併 Cargo workspace：lto=true+panic=abort（crash-only）vs dutymail 無此二者、stable cargo 不能 per-package 覆寫 profile；lock 紀律不對稱（assemble-release rust 腿不帶 --locked、dutymail 腿帶 --locked --offline）；產品邊界從 workspace 隔離退化為僅 lint 保護。反 rust-analyzer linkedProjects：scip CLI 不讀＋root 埋 rust-project.json 會劫持本 repo 所有 RA consumer。

```mermaid
flowchart LR
    R["rust/ workspace"] --> A1["rust-analyzer scip partial"]
    D["dutymail/ workspace"] --> A2["rust-analyzer scip partial"]
    A1 --> M["composition cat-merge（staged identity 擴 project_root）"]
    A2 --> M
    M --> G["root graph.db 全 faces"]
    G --> Q["scip_refs freshness snapshot delta_tour root 可達"]
```
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
〔baseline：code-reality 25fd932（v0.9.5）〕

〔已決策勿重辯：①採 SCIP producer 層 composition——codex 腿(e) 與 GLM 腿(c) 雙腿獨立收斂，實體相同（完整報告：delegate-bridge repo 2026-10/10-09-bridge-architecture/references/cr-root-advisory-{codex,glm}-result.md）②不合併 Cargo workspace——release profile 分離（rust lto+panic=abort crash-only vs dutymail 無）、lock 紀律不對稱（assemble-release rust 腿無 --locked、dutymail 腿有）、產品邊界退化；DB-80 O6 凍結（workspace 搬遷需新決策）③不採 rust-analyzer linkedProjects——scip CLI 不讀（--help 實查）、root 埋 rust-project.json 會劫持本 repo 全部 RA consumer④不做 SQLite 縫合（已有 SCIP merge 無須另建）⑤風險面：跨 workspace SCIP symbol 碰撞被 graph_db.rs INSERT OR IGNORE 靜默掉 node——須負向測試斷言 node count；corpus discovery 禁硬編碼目錄名（CR 是通用工具）〕

〔範圍：stage_rust 支撐多 Cargo workspace roots；**merge 粒度＝pre-concat（EP Q3 凍結取代本句原案的 staged-multi-leg：StagedLeg/merge_and_publish 契約零改動，rust family 內部合流——supersession 依據＝EP 凍結邊界 2）**；root freshness 涵蓋 Cargo manifests/locks/project topology（防 Cargo 變更 false-fresh；identity v2）；root snapshot/delta_tour 以 git HEAD 正確錨定；兩 workspace 雙向 no-cross-path-dep gate 完好。驗收條件（來信 AC1-4，EP TC-7 承載）：AC1 root graph 含 rust+dutymail+JS/TS；AC2 freshness 涵蓋 Cargo 面；AC3 snapshot/delta_tour git 錨定正確；AC4 boundary gate 完好＋symbol 碰撞負向測試。開放設計點已由 EP Q1 裁決：Combined（config rust_projects 權威＋保守 auto，歧義 fail-loud）。規模分級：full → standalone EP＝`ai-analysis/2026-10/10-09-db100-multi-root/ep.md`（v2，EP Review 18 findings 回寫完成）〕
<!-- SECTION:PLAN:END -->
