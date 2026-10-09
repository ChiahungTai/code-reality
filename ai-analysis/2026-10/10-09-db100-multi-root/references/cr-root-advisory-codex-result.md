## Root CR advisory

### 1\. Position：推薦方案 (e) — CR 原生支援 Multi-Cargo-Workspace，於 SCIP 層合併

**建議保留 rust/ 與 dutymail/ 兩個獨立 Cargo workspaces，修改 Code Reality 的 Rust producer，使同一 git root 下的多個 Cargo workspaces 可以產生單一、完整且具 freshness 保證的 root-level graph。**

具體採用：

```
delegate-bridge/                  ← Git root / CR root
├── .code-reality.toml            ← Rust projects 宣告
├── .code-reality/
│   ├── scip/index.scip           ← 合併後的 canonical SCIP
│   └── graph.db                  ← 唯一 root-level graph
├── rust/
│   ├── Cargo.toml                ← Workspace A
│   └── crates/...
├── dutymail/
│   ├── Cargo.toml                ← Workspace B
│   └── crates/...
├── scripts/                      ← JS/TS producer
└── tests/                        ← JS/TS producer
```

核心流程：

```
                   Git root
                      │
          Code Reality build
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
       rust/       dutymail/    root JS/TS
          │           │           │
     rust-analyzer  rust-analyzer scip-typescript
          │           │           │
          ▼           ▼           ▼
       Rust SCIP   Rust SCIP     JS/TS SCIP
          │           │           │
          └───────────┼───────────┘
                      ▼
              Path normalization
                      ▼
                SCIP merge
                      ▼
              Root graph.db
                      ▼
            snapshot / delta_tour
```

**合併點應選 SCIP，而非 SQLite graph.db。**

我檢查了 CR 實際來源，這個方向與現有架構高度吻合：

- build.rs (line 17)：已有 protobuf SCIP cat-merge。
- build.rs (line 374)：Rust producer 目前只接受一次 repo directory。
- build.rs (line 432)：已有 staged partial validation 與單一 index publication。
- graph\_db.rs (line 795)：節點位置由 repo root 加上 SCIP document relative path 推導。
- snapshot.rs (line 306)：明確要求 git root，這個約束應保留。

目前根目錄既有 graph 的唯讀查證結果：

| Graph | Nodes | Edges | 狀態 |
| --- | --: | --: | --- |
| root | 2,266 | 6,811 | 10/03 建立，已過期 |
| rust/ | 2,097 | 7,051 | 10/09 建立 |
| dutymail/ | 1,009 | 6,717 | 10/09 建立 |

根目錄舊 graph 有 Rust、scripts、tests 等節點，但 **dutymail 節點為 0**。

這證明現有 root graph 無法代表整個 repository。

兩個獨立 Rust graph 的節點合計為 3,106，僅能作為比較基準；最終合併 graph 還包含 JavaScript/TypeScript，不能直接要求總節點數等於 3,106。

### 2\. Concrete implementation sketch

主要修改應發生在 /Users/ctai/Github/code-reality。

**A. 擴充 CR project configuration**

建議在 delegate-bridge 的 .code-reality.toml 新增：

```
exclude = ["dist/", "dist-stage/"]

# Proposed schema; not currently supported.
rust_projects = [
    "rust",
    "dutymail",
]
```

這是建議新增的 CR schema，目前不能直接使用。

採用顯式設定有兩個優點：project topology 明確，以及避免誤將某個 nested crate 當成第三個 workspace。

CR 應驗證每個指定 project：

- 路徑經 canonicalize 後必須位於 repo root 內。
- 必須具有有效 Cargo workspace manifest。
- Project roots 不得互相重疊。
- 不得重複列入同一 workspace。
- 缺少 project 或載入失敗時 fail loud。

未提供設定時，可保留現有單一 workspace 行為，並在後續增加自動偵測。

**B. 修改 CR Rust producer**

主要位置：

code-reality/src/build.rs (line 372)

目前實作：

```
stage_rust(repo, part, roots)
```

需要擴充成概念上的：

```
for project in configured_rust_projects:
    run rust-analyzer scip <project-root>
    validate partial SCIP
    normalize document paths to git-root-relative
    stage partial

merge all Rust partials
merge Python / JS / TS partials
validate combined index
publish canonical root index
build root graph.db
```

有一個容易漏掉的實作細節。

目前 StagedLeg 以 ProducerFamily 識別，merge\_and\_publish() 對每個 family 只取第一個 matching leg。

因此不能只是把兩個 Rust partial push 進既有 staged vector。

必須將 staged identity 擴充為：

```
(family, project_root)
```

並依固定順序合併該 family 的所有 project。

**C. SCIP Document path normalization**

假設 rust-analyzer scip rust/ 產生：

```
crates/bridge-core/src/lib.rs
```

合併前必須轉為：

```
rust/crates/bridge-core/src/lib.rs
```

同樣地：

```
crates/dutymail-core/src/lib.rs
```

轉為：

```
dutymail/crates/dutymail-core/src/lib.rs
```

否則 graph builder 會把檔案解析到錯誤位置，甚至產生 path collision。

這一步需要解析 SCIP protobuf 並修改 Document.relative\_path。

原始 symbol ID 原則上應保留，因為 SCIP symbol identity 需要跨文件保持一致。但必須檢查不同 workspace 的 symbol ID 是否碰撞；若同一 symbol 對應不同 definitions，應拒絕發布並顯示衝突，而不能靠 document prefix 掩蓋。

**D. Freshness 必須涵蓋 project topology**

這是我認為最重要的驗收條件之一。

目前 ProductionIdentity::capture (line 1045) 主要依 source corpus 與 profile 建立 identity。

新的 root graph 除了 .rs 原始碼，也應將下列 producer inputs 納入 freshness：

```
.code-reality.toml

rust/Cargo.toml
rust/Cargo.lock
rust/crates/*/Cargo.toml

dutymail/Cargo.toml
dutymail/Cargo.lock
dutymail/crates/*/Cargo.toml

Relevant Rust toolchain / Cargo configuration
```

並記錄 project roots 的集合與排序。

否則修改 Cargo dependency、workspace membership 或 feature configuration 時，即使 Rust 原始碼完全沒有修改，CR 仍可能錯誤宣稱 graph fresh。

**驗收時應要求：**

1. Rust A、Rust B 與 JS/TS producer 都成功，才能發布完整 root index。
2. SCIP document paths 全部指向真實、位於 git root 內的檔案。
3. 每個宣告的 Rust workspace 都有對應的 governed document coverage，缺漏不得靜默略過。
4. freshness --repo <git-root> 能驗證完整 root corpus 與 project configuration。
5. snapshot、delta\_tour 成功使用 git-root graph，產出正確的 root-relative paths。
6. 任一 producer 失敗，不得將部分成功的 index 宣告為完整 graph。

原本 CR 已有 staged publication 與 torn-plane 偵測，應沿用這套機制，並驗證 graph.db 與 SCIP index 屬於同一 generation。

### 3\. Blast radius：各方案比較

| 方案 | CR 修改 | delegate-bridge 修改 | 架構風險 | 判斷 |
| --- | --- | --- | --- | --- |
| (a) 合併 Cargo workspace | 低 | 高 | 高 | 不建議 |
| (b) root project hint | 視 producer 而定 | 低 | 中 | 先 POC |
| (c) SQLite graph stitching | 高 | 低 | 高 | 不建議 |
| (d) Subworkspace router | 低 | 低 | 低 | 長期不足 |
| **(e) SCIP-level composition** | **中** | **低** | **中低** | **推薦** |

#### 方案 (a)：合併 Cargo workspace

這個方案的成本高於表面。

目前：

```
rust/Cargo.toml
rust/Cargo.lock
rust/target/

dutymail/Cargo.toml
dutymail/Cargo.lock
dutymail/target/
```

改成單一 root workspace，通常必須重新安排 nested workspace manifests，並改為：

```
Cargo.toml
Cargo.lock
target/
```

連帶影響：

**Build / dependency resolution**

兩個 workspace 會共享 lockfile 與 dependency resolution。

Cargo features 也可能因為同一次 build 包含更多 packages 而出現不同的 feature unification 結果。

此外，原本獨立更新 dependency 的能力會減弱。

**Release SOP**

assemble-release.mjs (line 78) 目前明確從兩個 target directories 收集三個 binary leaves。

合併後必須修改 build commands、artifact discovery 和驗證路徑。

sync-version.mjs (line 1) 與 version-sites.mjs (line 30) 也需要重新處理 lockfile 更新與 version sites。

補充一個現況校正：

**dutymail 雖然擁有獨立 Cargo workspace，但目前發布版本已經綁定 repo-root VERSION。**

這由 dutymail/AGENTS.md:9-11 與 release scripts 直接確認。

因此現況是「獨立 source/build ownership，統一 plugin distribution version」，而非完全獨立的版本發布流程。

**Tests / architecture**

workspace-boundary.test.mjs (line 268) 已把 bridge workspace 的四個成員明確固定。

若合併 workspace，這項測試必須改寫。

更大的問題是，原本依靠 Cargo workspace separation 表達的產品獨立性，會變成僅由 lint 與測試保護。

**Instruction documents**

至少需要重新檢視：

- Root AGENTS.md
- dutymail/AGENTS.md
- docs/runbook.md
- docs/build-loop.md
- Rust build / packaging 指引

整體而言，為了解決 CR project discovery 而改動 Cargo architecture，影響範圍過大。

#### 方案 (b)：Rust project hint

我檢查了目前 rust-analyzer scip --help。

CLI 介面是：

```
rust-analyzer scip <path>
    --output <path>
    --config-path <config_path>
```

其中 \--config-path 描述為客製化 Cargo behavior 的 JSON configuration。

它沒有直接提供多個 project roots 的 CLI 參數。

linkedProjects 主要是 rust-analyzer 的 LSP configuration 概念，不能直接推定 scip batch producer 會依相同機制載入多個 Cargo workspaces。

rust-project.json 可以描述 project，但手工維護整套 crates、dependencies、cfg 與環境資訊，可能使索引脫離真實 Cargo build graph。

**此方案值得做隔離 POC，但在確認兩個 workspaces 均產生完整 SCIP documents 前，不應採用。**

#### 方案 (c)：SQLite graph stitching

直接合併兩個 graph.db 看似快速，但必須處理：

- Node identity 與重複 symbol。
- Edge endpoint resolution。
- Module names 與 file paths。
- Derived indexes、communities、flows。
- Source identity 與 provenance。
- Schema migration。
- Snapshot consistency。

CR 既然已有 SCIP merge，就沒有充分理由另建 SQLite graph composition implementation。

#### 方案 (d)：Router

Router 可以降低日常查詢摩擦，但沒有解決 root graph 缺席。

它仍無法提供完整的 root-anchored snapshot / delta\_tour。

因此只適合當臨時查詢工具，不應成為最終架構。

### 4\. No-path-dep boundary guard

推薦方案 (e) **沒有削弱 Cargo workspace isolation**，所以既有機械防護應保留。

現有 workspace-boundary.test.mjs (line 390) 已有真實 dependency graph 檢查：

```
bridge → dutymail: forbidden path dependency
dutymail → bridge: forbidden path dependency
```

測試會透過 Cargo metadata 取得依賴資訊，再 canonicalize path，包含 symlink 情況。

另外，它也固定 bridge workspace member set。

我建議保留這兩項 gate，並在 CR 增加一項不同目的的 validation：

```
CR project roots:
    rust/
    dutymail/

Each project:
    has its own Cargo workspace
    has a non-overlapping source corpus
    produces a valid SCIP index

Merged graph:
    paths remain root-relative
    symbol definitions have no ambiguous collisions
    project coverage is complete
```

需要區分兩個概念：

**Cargo workspace boundary 是 architecture invariant；CR composite graph 是 observation model。**

CR 可以觀察不同 workspaces 的 reference relationship，但這不代表兩者因此取得 source dependency authority。

即使 root graph 出現跨 project references，也不能直接推斷架構違規；真正的 Cargo path-dependency 違規仍以現有 canonical metadata gate 判定。

### 5\. Risks、限制及未驗證事項

**Risk 1：SCIP symbol identity collision**

不同 Cargo workspaces 可能存在相同 crate name/version。

直接合併時，若兩個 producer 產生相同 SCIP symbol ID，graph\_db 可能將不同 definitions 視為同一 symbol。

這需要負向測試，不能只比較 node counts。

**Risk 2：Root source coverage**

本次唯讀檢查找到的 Rust 原始碼均位於：

```
rust/
dutymail/
```

但 CR 是通用工具，未來其他 repository 可能有更多 nested projects，或具有刻意排除的 source directories。

因此 discovery 與 corpus coverage 必須明確，不能依照這個 repo 的目錄名稱硬編碼。

**Risk 3：Freshness false positive**

Cargo manifests、lockfiles 或 project root 清單改變時，舊 SCIP 可能已不再代表當前 compilation graph。

這是需要同步修正的 correctness boundary。

**Risk 4：Mixed-language graph limitations**

Root graph 可以同時涵蓋 Rust 與 JS/TS，但這不代表 shell/Node process spawning 能自動推導為 Rust function call edges。

跨語言 runtime relationship 仍可能需要獨立的 boundary evidence。

**尚未驗證：**

- rust-analyzer scip 是否存在能直接支援這兩個 workspaces 的未記載 configuration。
- 將兩個真實 Rust SCIP 文件正規化後，是否完全保留 symbol/reference integrity。
- 新 root graph 的完整 node/edge 數與跨 workspace references。
- 新實作的 build、freshness、snapshot、delta\_tour 實際端到端結果。
- Code Reality 現行安裝版本與修改後 source build 的部署一致性。

本次依要求僅做唯讀查證，沒有執行任何 producer、build 或測試，也沒有寫入檔案。

最後檢查 git status，delegate-bridge 原有兩個未提交 Markdown 變更仍在；本次未修改它們。

### 6\. Strong counter-argument against recommendation

**最有力的反對理由是：這原本可以只是 delegate-bridge 的 project layout 問題，為何要修改整個 Code Reality？**

尤其採用方案 (a)，在 repo root 建立單一 Cargo workspace，rust-analyzer 就可以走更標準的 project discovery 流程。

這將避免 CR 維護多 project discovery、SCIP path rewriting、source identity composition 等額外邏輯。

而且 CR 是跨 repository 工具，任何新增的通用功能，都需要額外的 regression coverage。

這個反對理由成立。

但我仍不建議方案 (a)。

原因在於 **Cargo workspace 與 git repository 是兩個不同的邊界**。

Cargo workspace 控制 compilation/dependency organization；git repository 則是變更、commit、snapshot 與 review 的共同單位。

一個 git repository 包含多個 Cargo workspaces 是合理的架構形態，並不代表 repository 結構錯誤。

CR 的目標既然包含 root-level dependency analysis、commit-anchored snapshots 與 delta tours，它就應該能處理這種 repository topology。

尤其現有 CR 已經支援：

```
Python SCIP
    +
Rust SCIP
    +
TypeScript SCIP
    ↓
Single graph
```

把 Rust 從單一 producer invocation 擴充為多個 project invocation，是這個架構自然的延伸。

它引入的複雜度可以限制在 producer staging、path normalization、coverage verification 與 freshness。

相比之下，方案 (a) 將 CR 的限制轉嫁給 Cargo architecture、release SOP 和產品邊界。

**最終裁決：採用方案 (e)，把 multi-workspace 支援實作在 Code Reality 的 SCIP producer/composition 層；保留兩個 Cargo workspaces 與現有 dependency boundary tests。**

交付驗收標準應是以下四項同時成立：

```
[AC1] root CR graph 包含 rust + dutymail + root JS/TS
[AC2] root freshness 對完整 corpus / project topology 正確
[AC3] root snapshot / delta_tour 能以 git HEAD 正確錨定
[AC4] 兩個 Cargo workspaces 與 no-cross-path-dep gate 保持有效
```

達到這四項，才能認定 root-level CR 的根因已解決。