# EP: PyPI platform-wheel distribution axis (cargo-free consumer install)

> **ep_type**: implementation
> baseline: 9f25969

Source: distribution-axis design convergence (2026-08-28). Adjudicated
combo: **ruff/pyrefly (PyPI platform wheels) + CRG (wiring face —
deferred to its own thin arc)**. Today every consumer needs a CR
checkout + Rust toolchain + minutes of `cargo install --path`; the
wheel axis removes cargo from the consumer path entirely. Research
grounding (all verified 2026-08-28): ruff ships 17 platform wheels,
pyrefly 11, NT wheels + a private dev index (index rejected here as
YAGNI). Freshness face is channel-agnostic: a pip-installed binary
embeds the same build rev and warns against a local CR checkout the
same way.

## Spike facts (measured 2026-08-28, maturin 1.15.0, macOS arm64)

The fatal assumption — "a pure-Rust cargo workspace can ship as
`py3-none-<platform>` wheels" — was validated locally before this EP:

| crate → wheel | wheel size | bins carried (uncompressed) |
|---|---|---|
| `code_reality` | 8.5 MB | code-reality 8.7 MB + code-reality-mcp 12.7 MB |
| `pyrefly_producer` | 25 MB | pyrefly-index 25 MB + pyrefly-lsp 30.4 MB |
| `code_reality_lsp_bridge` | 1.4 MB | code-reality-lsp-bridge |

- maturin auto-detects bin bindings — **zero Cargo.toml config
  needed** ("Found bin bindings"); invoked per crate via
  `maturin build --release -m crates/<crate>/Cargo.toml`.
- Platform tag `py3-none-macosx_11_0_arm64` (the ruff/pyrefly shape);
  `MACOSX_DEPLOYMENT_TARGET=11.0` auto-set.
- Wheels install into a venv via `uv pip install`; all five bins
  answer `--version` with `0.1.0+9f25969` — the freshness face
  survives the wheel channel; pyrefly-lsp keeps its own face with the
  engine rev pin.
- maturin emits a cyclonedx SBOM per bin into dist-info (free
  provenance metadata).
- Warm-cache build ≈10 s per crate.
- **Dist shape adjudicated: 3 dists, one per crate.** maturin is
  per-crate; merging five bins across crates would need custom
  packaging machinery for zero consumer benefit. Combined ~35 MB is
  far under PyPI's per-file size limit either way (largest single
  wheel 25 MB). PyPI names default
  to crate names (`code-reality`, `pyrefly-producer`,
  `code-reality-lsp-bridge`).
- Not yet verified: `uv tool install` / `uvx` from a real index
  (needs published packages — S3 verifies on first release); Linux
  builds (user scoped the local spike to this arch — S2 is the
  kill-gate).

## UC 盤點

### Backlog 關聯
- 自動建卡結果：本 EP 建立一張追蹤卡（`.kanban/Backlog/`，見收尾）；Backlog 原為空。

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md（本 repo 無此檔，正當跳過）。

### 掃描範圍
- `AGENTS.md` Capabilities（root）；`crates/AGENTS.md`；`.kanban/`。

### 既有 UC 狀態
| 能力 | 狀態 | 來源 | 影響 | 說明 |
|------|------|------|------|------|
| Binary freshness face | ✅ | AGENTS.md | 無影響 | 通道無關（wheel 實測）；S3 復驗一次 |
| Unified MCP interface | ✅ | AGENTS.md | 更新 | S4 spawn 優先序＋prerequisites 文檔增 wheel 路徑 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| PyPI platform-wheel distribution（消費端免 cargo 安裝） | 📋 | `crates/*/pyproject.toml` + `.github/workflows/release-wheels.yml` |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | 無 Rust 環境的消費者安裝 | `uv tool install code-reality ...` | 五 bin 上 PATH、MCP spawn 可用、`--version` 正常 | 無 | 新增 UC |
| SM-2 | 免安裝試用 | `uvx code-reality scip_refs <sym> --repo .` | 直接執行不落地安裝 | 無 | 新增 UC |
| SM-3 | 開發環 cargo HEAD 與 pip stable 並存 | `.mcp.json` spawn | S4 裁決後的優先序；freshness WARN 兩者都標身分 | S4 | freshness face |
| SM-4 | tag 指到 build 不綠的 commit | CI | 不發布（publish 只掛在綠 build 後） | 重 bump＋retag | 新增 UC |
| SM-5 | 三 dist 部分發布失敗 | trusted publishing 某專案失敗 | 其餘已上傳者不可重傳同版（PyPI 禁）→ fix＋patch bump＋retag | 版號條款 | 新增 UC |
| SM-6 | GUI-launched harness 無 `~/.cargo/bin` 於 PATH | spawn | S4 裁決的 wrapper 需同時服務此場景（README.md 既有約束） | S4 | Unified MCP |
| SM-7 | 有 CR checkout 的機器裝了 wheel | 任一 bin 呼叫 | checkout 領先即 WARN（行為不變）；無 checkout 靜默 | S3 復驗 | freshness face |

## 段落劃分原則

依賴鏈：S1（本地 metadata）→ S2（CI matrix dry-run）→ S3（首發）→
S4（spawn 翻轉＋文檔，需 wheels 已上線才有裁決依據）。S2 的 Linux
build 是全 EP 唯一未先驗的風險，設 kill-gate（descope 不擋軸）。
每段驗證自足；S1/S2 可在同 session 連做，S3 需 PyPI 手動一次性設定。

---

## S1: per-crate wheel metadata（本地打包面）

**Context**
Spike wheels 的 METADATA 只有 80 bytes（name＋version）——發布需要
正式 metadata（description/license/requires-python/urls）。UC 引用：
實作「新增 UC：PyPI platform-wheel distribution」的本地面。
- 依賴錨點：`Cargo.toml:5-7`（`[workspace.package] version = "0.1.0"`
  單源）→ 消費端 `crates/*/Cargo.toml:3`（`version.workspace = true`）
- 語義約束：與 S3 共享「workspace version 是唯一版號源」；pyproject
  內**不寫死 version**（maturin 從 Cargo.toml 帶入，機制於本段實測）。
- 基礎設施盤點：maturin 已由 spike 驗證；無其他可複用件（repo 無
  任何 pyproject.toml）。

**要點**
- 每個 crate 目錄加最小 `pyproject.toml`：`[project]` name（=crate
  名）、description、`requires-python = ">=3.8"`（純 binary 無 Python
  ABI 依賴，對齊 pyrefly）、license MIT、urls（GitHub）、classifiers。
  version 由 maturin 從 Cargo.toml 帶入（若需 `dynamic` 宣告，實測後
  定稿）。
- 不加 `[tool.maturin]`（spike 證明零配置即 bin bindings）。
- 風險：pyproject.toml 進 crate 目錄可能影響其他工具掃描（cargo 忽略
  非慣例檔，無風險；`rg`/`fd` 面新增三個 toml，無消費者）。

**Pseudo Code（檔案佈局）**
```
crates/code-reality/pyproject.toml
crates/pyrefly-producer/pyproject.toml
crates/code-reality-lsp-bridge/pyproject.toml
# 內容同構，僅 name/description 差異：
# [project]
# name = "code-reality"          # = crate name = PyPI dist name
# description = "..."
# requires-python = ">=3.8"
# license = "MIT"
# [project.urls]
# Repository = "https://github.com/ctai/code-reality"
```

**驗證策略**
- 三 crate 重跑 `maturin build --release`；`unzip -p <wheel> .../METADATA`
  檢查 description/license/requires-python 到位。
- wheel 檔名仍為 `<name>-<workspace version>-py3-none-<platform>`。
- venv 重裝＋五 bin `--version`（回歸釘：freshness face 不變）。
- 已知未覆蓋：上傳面 metadata（PyPI rendering）留 S3 首發後目檢。

---

## S2: CI release workflow（build matrix，dry-run 先行）

**Context**
Linux build 是全 EP 唯一未先驗風險（pyrefly engine git-dep 於 Linux
編譯）。UC 引用：實作「新增 UC」的產線面。
- 依賴錨點：無既有 workflow（`.github/` 目前空）——本段新建。
- 語義約束：與 S3 共享「publish 只在 tag `v*` 且 build 綠時」。

**要點**
- `.github/workflows/release-wheels.yml`：
  - `workflow_dispatch`（dry-run：只 build＋上傳 artifacts，不發布）
  - `on: push: tags: ['v*']`（build＋發布，發布步驟本段留 no-op，
    S3 接上 trusted publishing）
  - matrix（4 平台 × 3 crate = 12 wheels）：
    - `macos-latest`（arm64 native）
    - macOS x86_64：`rustup target add x86_64-apple-darwin`＋
      `maturin build --target x86_64-apple-darwin`（Apple 原生 cross）
    - `ubuntu-latest`（x86_64 native）
    - `ubuntu-24.04-arm`（aarch64 native runner；public repo 免費額度）
  - 每 job：checkout → rustup（stable）→ `cargo install maturin`（或
    pipx/uv 裝法擇一）→ 三次 maturin build → upload-artifact。
- **Kill-gate**：pyrefly-producer 在 Linux 編譯失敗 → descope Linux
  平台為後續追蹤（首發 macOS-only），不擋軸；成功則全矩陣進 S3。

**Pseudo Code（workflow 骨架）**
```
jobs:
  build-wheels:
    strategy:
      matrix:
        include:
          - { os: macos-latest,  target: aarch64-apple-darwin }
          - { os: macos-latest,  target: x86_64-apple-darwin, cross: true }
          - { os: ubuntu-latest, target: x86_64-unknown-linux-gnu }
          - { os: ubuntu-24.04-arm, target: aarch64-unknown-linux-gnu }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - run: rustup target add ${{ matrix.target }}   # cross job 才需要
      - run: pipx install maturin
      - run: maturin build --release --target ${{ matrix.target }}
             -m crates/{code-reality,pyrefly-producer,code-reality-lsp-bridge}/Cargo.toml  # 三步
      - uses: actions/upload-artifact@v4   # target/wheels/*.whl
```

**驗證策略**
- `workflow_dispatch` dry-run 全綠；artifacts 下載後 `unzip -l` 驗
  平台 tag（`macosx_11_0_arm64` / `macosx_10_12_x86_64`（或 maturin
  實際給的 deployment target） / `manylinux_*_x86_64` / `_aarch64`）。
- Linux wheel 取回一台 Linux 環境裝跑（或首次發布後以 uvx 驗）——
  本機無 Linux，標註未本地驗證項。
- 已知未覆蓋：musllinux（NOT 清單）；Windows（NOT 清單）。

---

## S3: trusted publishing＋首發＋版號基準

**Context**
UC 引用：完成「新增 UC」的發布閉環。
- 依賴錨點：`crates/*/build.rs`（CR_BUILD_REV 嵌入——wheel 通道實測
  有效）→ 消費端 `crates/code-reality/src/freshness.rs`（WARN 邏輯）；
  測試釘 `crates/code-reality/tests/freshness.rs`＋
  `crates/code-reality-lsp-bridge/tests/version_face.rs`。
- 語義約束：版號三層架構（見「版號條款」）——本段只動 workspace
  version，**不碰** plugin.json / marketplace 版號。

**要點**
- PyPI 一次性手動步驟：為三個專案各建 trusted publisher（OIDC 綁
  repo `ctai/code-reality`＋workflow 檔名＋environment `release`）；
  workflow 加 `environment: release`＋`maturin-action`（或 maturin
  CLI）`--upload` 面。
- 首發流程：workspace version bump → commit → `git tag v0.2.0` →
  push → CI 發布 → 驗證。
- 驗證（消費者陌生路徑：中性 cwd、純 PATH）：
  - `uv tool install code-reality`×3（或確認 uv 多套件單環境語法，
    不支援則文檔寫三條）
  - `uvx code-reality --version`（免安裝路徑）
  - 有 CR checkout 的本機：bin 呼叫時 WARN 行為如常（SM-7）
  - PyPI 專案頁 metadata rendering 目檢

**驗證策略**
- 上列即驗證；另跑 `cargo test --workspace` 回歸（版號 bump 不應動
  任何測試——version face 測試若釘死 `0.1.0` 需同步改釘）。
- 已知未覆蓋：Windows 消費者；pip（非 uv）純 Python 環境差異（wheel
  無 ABI 依賴，風險低）。

---

## S4: `.mcp.json` spawn 優先序＋文檔（wheels 上線後的收斂）

**Context**
現行 spawn wrapper 優先 `~/.cargo/bin`（開發環 HEAD 蓋過 pip stable
——wheels 時代方向錯）。UC 引用：更新「Unified MCP interface」。
- 依賴錨點：`plugin/.mcp.json:3-14`（兩個 sh fallback block）→
  消費端 ZCode/CC plugin spawn（0.1.3 cache）。
- 語義約束：與 SM-6 共享「GUI-launched harness 可能無 cargo/pip bin
  於 PATH」（`README.md:67-69` 既有約束）——這是當初 fallback 存在
  的原因，翻轉不得回歸此場景。

**要點**
- 裁決點（build 時定案，候選）：
  (a) 純 PATH（`exec <bin> --stdio`）——最簡，但 GUI 無 PATH 場景回歸；
  (b) sh 鏈反轉：`command -v` 命中 PATH 先用，miss 再試已知絕對路徑
  （cargo 與 uv 兩個已知位置）；
  (c) 維持現狀＋文檔宣告「開發機以 cargo 面為準」。
  預設 leaning：(b)——同時服務三場景（PATH 消費者／GUI 無 PATH／
  開發環），代價是 wrapper 變長一行。
- `.mcp.json` 屬 plugin 內容變更 → plugin version bump 0.1.3→0.1.4
  ＋三處 marketplace/manifest 同步（0.1.3 弧建立的紀律）＋ZCode 端
  重裝驗證。
- 文檔：repo README Quickstart 增 uv/pip 消費者路徑（cargo 降為
  developer face 標題）；`plugin/README.md` prerequisites 增 wheel
  選項；AGENTS.md Usage 段一句話帶 wheel 安裝。

**驗證策略**
- ZCode 新 session：兩 server mount＋工具實呼（比照 0.1.3 弧驗證法）。
- 模擬 PATH 剝離（`env -i /bin/sh -c <wrapper>`）驗 GUI 場景仍可
  spawn。
- freshness WARN 在 cargo-face 與 pip-face 各驗一次（SM-3）。

---

## 版號條款（frozen）

1. **唯一源**：workspace `[workspace.package] version`；三 crate
   `version.workspace = true` 繼承（現狀即是）；pyproject 不寫死
   version。plugin.json 版號是**獨立軸**（僅 `plugin/` 內容變更 bump）
   ，刻意不與 workspace 同步。
2. **首發基準**：`0.1.0 → 0.2.0`（新能力軸；0.1.x 從未發布、無消費
   契約，bump 語義 = 新能力）。
3. **發布迴路**：bump → commit → `git tag v<version>` → push → CI →
   trusted publishing。無日曆制（單維護者）；tag 必須指到綠 build。
4. **失敗重發**：PyPI 禁同版重傳——部分失敗 → fix → patch bump →
   retag（SM-5）。
5. **rev face 正交**：`<pkg>+<rev>`（`crates/*/build.rs`）標 build
   身分不標 release；wheels 與 cargo install 兩通道共享同一機制，
   不因發布改動。

## NOT（scope boundary——防 scope creep）

- **不做** NT 式私有 dev-wheel index（freshness 已由 post-commit hook
  本機解；外部 bleeding-edge 消費者出現再議）。
- **不做** CRG 式 `setup`/`install` 子命令（獨立薄弧；解 onboarding
  不解 freshness）。
- **不做** CC community marketplace 投稿（wheels 落地後另議）。
- **不做** Windows wheels、musllinux（CI 面後續低成本擴充）。
- **不做** crates.io 發布（`cargo install --path` 維持開發者路徑；
  消費者導向 wheels——ruff 先例）。
- **不改** launchd HTTP 面（與本軸正交）。

## 整合策略

- baseline: `9f25969`（EP 建立當下 `git rev-parse HEAD`）。
- S1+S2 可同 session 連做（本地＋CI dry-run）；S3 需 PyPI 手動設定
  ＋真首發（獨立 session）；S4 在首發後獨立小弧。
- 段落間共用驗證資產：spike 的 venv 安裝法（`.agent-tmp/`）＋
  `--version` freshness 釘。

## 收尾步驟

1. Capabilities：AGENTS.md 新增「PyPI platform-wheel distribution」
   行（入口＝三 dist＋release workflow）；「Unified MCP interface」行
   附註 spawn 優先序翻轉。Kanban 卡移 Done/（EP 歸檔 `_done/`）。
2. 無 SYSTEM-MAP.md——正當跳過。
3. instruction 檔：AGENTS.md Usage 段補 wheel 安裝一句；repo README
   Quickstart 消費者/開發者雙路徑（S4 內完成）。
4. `/audit-test`：本 EP 無新增測試檔（yaml/pyproject/config 軸）——
   跳過理由：無 callable 新增；S3 的 version-face 測試釘調整隨該段
   驗證。
5. **ai-rules handoff**：交付 prompt（code-reality SKILL.md 安裝段
   於 wheels 上線後翻轉；觸發條件＝S3 首發落地）。EP 內不自動跨 repo
   寫入。
