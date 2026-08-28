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

## EP Review Findings

獨立 agent 審查（2026-08-28，F1-F5＋深層思考；錨點逐一實讀、PyPI
三名實查 404 可用）。judge 結果：R1＋Y1-Y4＋I1-I3＋I5-I7 採納回寫；
I4 維持現狀。

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| R1 | 🔴 必須修正 | S2/S3/整合策略 | repo 無 git remote，EP 無此前置——dry-run（server-side Actions）與 trusted publishing（OIDC 綁死 slug `ChiahungTai/code-reality`）全假設 remote 存在；AGENTS.md「remote GitHub」自述與 `git remote -v` 空矛盾 | S0 前置步驟（見段落劃分原則）；降級路線＝手動上傳 fallback 記錄不為預設 | implemented |
| Y1 | 🟡 | S4 | 錨點 drift：GUI-PATH 約束實際在 `README.md:71-73`（67-69 是 JSON snippet；0.1.3 弧編輯後位移） | 改錨點 | implemented |
| Y2 | 🟡 | 收尾 4 | audit-test 跳過理由對 S4 不成立——`lsp_status` availability 探測是 Rust 行為變更（crate 有現存測試面） | 跳過 scope 到 S1-S3；S4 補最小 smoke test 或免測理由 | implemented |
| Y3 | 🟡 | S4 | plugin bump 段漏 dist slice 重生成——directory-source 驗證會用舊 slice | 補 rerun `dist-marketplace.sh`＋marketplace refresh | implemented |
| Y4 | 🟡 | S4/S3 | `uvx pyrefly-producer` 必敗——dist 無同名 bin（bins＝pyrefly-index/pyrefly-lsp） | 文檔條款補 `uvx --from pyrefly-producer <bin>` | implemented |
| I1 | ℹ️ | S3 | 正面查證：version-face 測試動態斷言 `env!("CARGO_PKG_VERSION")` 未釘死；三 PyPI 名 2026-08-28 均 404 | 條件句收斂為肯定句 | implemented |
| I2 | ℹ️ | S1 | EP 伪碼缺 `[build-system]`（maturin 必要件）；root 殘留 `index.scip` | 伪碼補齊＋清檔 | implemented |
| I3 | ℹ️ | S2 | `rust-toolchain.toml` 釘 1.96.0——「rustup stable」文字與實際不符；target add 需對 pinned toolchain 生效 | 文字修正＋workflow 步驟改 rustup show | implemented |
| I4 | ℹ️ | 全文 | EP 中文撰寫、封存語料混用——AGENTS.md 英文義務不含 ai-analysis 規劃文檔，不違規 | 維持 | wontfix（記錄） |
| I5 | ℹ️ | SM | 缺 uv tool upgrade 迴路、CLI PATH 疊影兩場景 | SM-9/SM-10 | implemented |
| I6 | ℹ️ | S2 | upload-artifact v4 同名跨 matrix 衝突 | 要點註明 per-leg 唯一名（實作已用 `wheels-<target>`） | implemented |
| I7 | ℹ️ | S3/S1 | build.rs 頭註「no tags」首 tag 後過時；`publish = false` 與 PyPI 發布語義相悖（僅指 crates.io） | S3 順手改註；Cargo.toml 加註解 | implemented |

審查深層思考兩點收納：trusted publishing 定位為**終態而非首發必要條件**
（remote 延後時手動發布是唯一可執行路）；S4 文檔面（rust-analyzer
系統依賴行、Quickstart 雙路徑）不技術依賴 wheels 上線——remote 卡住
時可獨立先行。

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
| SM-8 | Python-only 機器（無 rust-analyzer）查型別面／呼叫 `.rs` 工具 | `lsp_status`／`hover(.rs)` | status 標 backend unavailable＋安裝指引（S4 補，現行靜默 not-spawned-yet——已查證 server.rs 不探測 binary 存在性）；工具呼叫 loud error | S4 | Unified MCP |
| SM-9 | wheel 版落後升級 | `uv tool upgrade <dist>` | 拉最新 PyPI 版（條款 7 迴路） | 無 | 新增 UC |
| SM-10 | CLI 直呼時 cargo bin 與 uv tool bin 同名疊影 | `which code-reality` | 文檔明示以 which／絕對路徑辨識（spawn 面＝SM-3） | S4 文檔 | freshness face |

## 段落劃分原則

依賴鏈：S1（本地 metadata）→ S2（CI matrix dry-run）→ S3（首發）→
S4（spawn 翻轉＋文檔，需 wheels 已上線才有裁決依據）。S2 的 Linux
build 是全 EP 唯一未先驗的風險，設 kill-gate（descope 不擋軸）。
每段驗證自足；S1/S2 可在同 session 連做，S3 需 PyPI 手動一次性設定。

**S0 前置（review R1）**：repo 目前無 git remote——S2 dry-run（GitHub
Actions 是 server-side）與 S3 trusted publishing（OIDC 綁死 repo slug）
都假設 public repo `ChiahungTai/code-reality` 存在（pyproject urls 與 publisher
皆寫死此 slug）。**建立 remote＋首次 push 是 user 的 outward action，
EP 不自動執行**；S2 本地可驗面（workflow 撰寫＋yamllint）不受阻擋，
dry-run 驗收以此為 gate。降級路線（remote 延後時）：本機
`maturin build`＋API token 手動上傳——trusted publishing 仍為終態。

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
  version 由 maturin 從 Cargo.toml 帶入（**已實測定案**：無需
  `dynamic` 宣告；`[build-system]` 是必要件——缺它報 `missing field
  build-system`，review I2 build 偏差記錄）。
- 不加 `[tool.maturin]`（spike 證明零配置即 bin bindings）。
- 風險：pyproject.toml 進 crate 目錄可能影響其他工具掃描（cargo 忽略
  非慣例檔，無風險；`rg`/`fd` 面新增三個 toml，無消費者）。

**Pseudo Code（檔案佈局）**
```
crates/code-reality/pyproject.toml
crates/pyrefly-producer/pyproject.toml
crates/code-reality-lsp-bridge/pyproject.toml
# 內容同構，僅 name/description 差異：
# [build-system]                 # 必需——pyproject 在場時 maturin 要求（build 實測）
# requires = ["maturin>=1.5,<2.0"]
# build-backend = "maturin"
# [project]
# name = "code-reality"          # = crate name = PyPI dist name
# description = "..."
# requires-python = ">=3.8"
# license = "MIT"
# [project.urls]
# Repository = "https://github.com/ChiahungTai/code-reality"
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
  - matrix（**縮編裁決 user 2026-08-28「之後只要驗證 macos arm 就好，
    還沒有要做這麼大事業」：macOS arm64 單腿×3 crate＝3 wheels**；
    原 4 平台矩陣保留在 workflow 註解，Linux/x86_64 腿 deferred——
    首個 in-flight dry-run `33182529285` 以舊定義跑了全矩陣，其
    Linux 結果作免費情報記錄，不再 gate 本 EP）：
    - `macos-latest`（arm64 native）
    - macOS x86_64：`rustup target add x86_64-apple-darwin`＋
      `maturin build --target x86_64-apple-darwin`（Apple 原生 cross）
    - `ubuntu-latest`（x86_64 native）
    - `ubuntu-24.04-arm`（aarch64 native runner；public repo 免費額度）
  - 每 job：checkout → rustup（**尊重 `rust-toolchain.toml` pin
    1.96.0**——runner 預裝 stable 但 cargo shim 走 pin；`rustup
    target add` 須對 pinned toolchain 生效，review I3）→ `uv tool
    install maturin`（pin 版）→ 三次 maturin build → upload-artifact
    （**artifact 名每 leg 唯一** `wheels-<target>`——v4 同名跨
    matrix 衝突，review I6）。
- **Kill-gate（已隨縮編裁決除役）**：原設計「pyrefly-producer 在
  Linux 編譯失敗 → descope Linux」——2026-08-28 縮編後 Linux 腿
  本來就不跑，gate 不存在；in-flight 全矩陣 dry-run 的 Linux 結果
  僅作未來重啟的情報。

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
- **前置 hard gate（R1）**：dry-run 需 remote 已建立＋push 後方可在
  GitHub Actions 執行——本地 yamllint 面除外。
- `workflow_dispatch` dry-run 全綠；artifacts 下載後 `unzip -l` 驗
  平台 tag（`macosx_11_0_arm64` / `macosx_10_12_x86_64`（或 maturin
  實際給的 deployment target） / `manylinux_*_x86_64` / `_aarch64`）。

**Dry-run 收案（2026-08-28，run `33182529285`，pre-narrow 全矩陣定義
＝縮編範圍的嚴格超集）**：四腿全綠、publish gate 正確 skipped、
artifacts 12 顆 tag 全驗——`macosx_11_0_arm64`（與本機 spike 逐字
同）、`macosx_10_12_x86_64`（Apple cross 成立）、
`manylinux_2_39_{x86_64,aarch64}`。**Linux 免費情報：pyrefly engine
git-dep 於 Linux 兩架構編譯成功——原 kill-gate 問題答案＝可行**；
manylinux_2_39 綁 ubuntu-24.04 的 glibc 2.39，未來重啟 Linux 腿時
再評估舊 glibc 相容面（maturin cross-glibc 選項）。S2 驗證收案。
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
- PyPI 一次性手動步驟：三筆 pending publisher——**PyPI 約束（2026-08-28
  實撞）：同一 `(repo, workflow, environment)` 組合只能綁一個專案名**
  ，monorepo 標準解法＝每 dist 一個 environment：`code-reality`→
  `release`、`code-reality-lsp-bridge`→`release-lsp-bridge`、
  `pyrefly-producer`→`release-pyrefly-producer`（三 environments 已
  於 GitHub 建妥）。**publish job 重寫形態（W4 佔位的正式解）＝
  拆三個 per-project job**：各 `environment: <自己的>`＋各據
  download-artifact 過濾自己的 wheels＋`pypa/gh-action-pypi-publish`
  走 OIDC（繞開 maturin publish 重建問題，也消滅 `|| true`）。
- 首發流程：workspace version bump → commit → `git tag v0.2.0` →
  push → CI 發布 → 驗證。
- build review 順手項（2026-08-28 fresh-eyes，W1/W4/W5）：**W1**
  pyproject 補 `readme`（root README 引用或 per-crate——現況 wheel
  無 Description body，PyPI 專案頁僅一行 summary）；**W4** publish
  步驟重寫時勿繼承佔位的 `|| true`（吞 per-crate 失敗）——改下載
  dist 批次上傳＋逐專案 fail-loud；**W5** `requires-python` 順手升
  `>=3.9`（3.8 已 EOL）。W2（matrix `cross` key 純自述）保留；
  W3（arm runner rustup 預裝）由 dry-run 首跑關閉。
- 順手更新三份 `build.rs` 頭註「this repo carries no tags」——首個
  tag 後該句過時（機制不變：`--exclude=*` 保證 hash-only；review
  I7a）。
- 驗證（消費者陌生路徑：中性 cwd、純 PATH）：
  - `uv tool install code-reality`×3（或確認 uv 多套件單環境語法，
    不支援則文檔寫三條）
  - `uvx code-reality --version`（免安裝路徑）
  - 有 CR checkout 的本機：bin 呼叫時 WARN 行為如常（SM-7）
  - PyPI 專案頁 metadata rendering 目檢

**驗證策略**
- 上列即驗證；另跑 `cargo test --workspace` 回歸（review I1 已查證
  version-face 測試動態斷言 `env!("CARGO_PKG_VERSION")`、未釘死
  `0.1.0`——bump 無需改測試；三個 PyPI 名 2026-08-28 實查皆 404
  可用，佐證條款 2「0.1.x 無消費契約」）。
- 版號面盤點（ai-rules ① 裁決項）：發布後目檢三面各自自洽（PyPI＝
  workspace 版；plugin＝接線軸版本；兩軸關係 README 已載）。
- 文檔分工抽查（ai-rules ②）：MCP 相關安裝示例一律 `uv tool install`
  而非 `uvx`。

**S3 收案（2026-08-28 深夜，run `33186412174`）**：`v0.2.0` tag →
build 綠＋三 publish job 各自 environment OIDC 全過——PyPI 三專案上線
（各一顆 macosx_11_0_arm64 wheel）。消費者驗證：`uvx code-reality
--version`＝`0.2.0+aacebd6`（PyPI 下載、陌生路徑、無 WARN＝rev 對
齊）；PyPI 直裝 venv 三 dist 五 bin 全 `0.2.0+aacebd6`、pyrefly-lsp
engine pin 面正確。版號面盤點：PyPI＝workspace＝wheel＝`--version`
pkg 段＝0.2.0 ✓；plugin 軸 0.1.3 獨立（兩軸關係文檔隨 S4）。本機
`uv tool install` 實裝刻意延後（S4 spawn 翻轉前避免 ~/.local/bin 與
開發 cargo 面疊影——以 uvx＋venv 等價驗證）。剩餘：S4＋弧收尾。
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
  於 PATH」（`README.md:71-73` 既有約束——review Y1 修正錨點，0.1.3
  弧編輯後位移）——這是當初 fallback 存在
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
  ＋三處 marketplace/manifest 同步（0.1.3 弧建立的紀律）＋**rerun
  `scripts/dist-marketplace.sh` 與 in-app marketplace refresh**
  （review Y3——directory-source 驗證用 slice，漏重跑會驗到舊
  slice）＋ZCode 端重裝驗證。
- 文檔：repo README Quickstart 增 uv/pip 消費者路徑（cargo 降為
  developer face 標題）；`plugin/README.md` prerequisites 增 wheel
  選項；AGENTS.md Usage 段一句話帶 wheel 安裝。**工具分工條款
  （ai-rules ②）**：`uv tool install`＝MCP server／常駐 bin 的文檔
  定位（stdio server 每 session spawn——uvx 每次 invocation 帶
  resolve/cache 層，對常駐 spawn 是純啟動延遲）；`uvx`＝一次性
  查詢／CI 腳本。`.mcp.json` 維持直 spawn PATH binary（現狀即是，
  不引入 uvx）。**uvx 免安裝僅對 dist 名＝bin 名者直接可用**
  （`code-reality`／`code-reality-lsp-bridge`）；`pyrefly-producer`
  無同名 bin——文檔寫 `uvx --from pyrefly-producer pyrefly-index`
  （review Y4），避免消費者在 PyPI 頁照抄失敗。
- **rust-analyzer 系統依賴語義（ai-rules ③，查證成立）**：wheel 裝
  得到 bridge、裝不到 rust-analyzer——Python-only 機器（wheel 分發
  的主要族群）`.rs` 型別面缺。現行 `lsp_status` 不探測 backend
  binary 存在性（`server.rs:135-156` 只印 session 狀態；backend
  lazy 到首次工具呼叫才 loud）——補兩件：Quickstart/README 記系統
  依賴行（rust-analyzer 不隨 wheel＋安裝命令）；`lsp_status` 對
  backend binary 缺場標 `unavailable`＋安裝指引（PATH 查找探測，
  fail-loud 風格）。

**驗證策略**
- ZCode 新 session：兩 server mount＋工具實呼（比照 0.1.3 弧驗證法）。
- 模擬 PATH 剝離（`env -i /bin/sh -c <wrapper>`）驗 GUI 場景仍可
  spawn。
- freshness WARN 在 cargo-face 與 pip-face 各驗一次（SM-3）。

**S4 收案（2026-08-29）**：裁決＝**候選 (b) 定案**——`plugin/.mcp.json`
兩 wrapper 改為「`command -v` PATH 命中先用 → miss 依序試
`~/.local/bin`（wheel 面）→ `~/.cargo/bin`（開發面）→ 全缺場
fail-loud（stderr 安裝指引＋exit 127）」；fallback 順序 deliberate
（pip stable 優於 cargo HEAD，與翻轉方向一致）。直接執行驗證三情境
×兩 server：T1 PATH 命中（fake bin 證走 PATH 非 cargo 絕對路徑）、
T2 `env -i`（無 PATH、HOME 在場——real binary 經 fallback spawn 成功
，stdin 即關的 `[FAIL] connection closed` 為預期離場）、T3 全缺場
（指引＋127）。freshness WARN 兩通道各一次：cargo face（T2 過程
`installed aacebd6 != repo HEAD 3d2be88`）＋pip face（`uvx
code-reality --version`＝`0.2.0+aacebd6`＋WARN）。**ZCode 新 session
雙 mount 實呼＝user 端步驟**（marketplace refresh＋重裝 0.1.4 後），
非本 session 可驗。`lsp_status` availability：`server.rs` 新增
`backend_available`（std PATH 查找，無外部依賴）＋`status_line` 抽出
——缺場 backend 印 `state=unavailable`＋安裝指引（SM-8）；Python
backend 的 `install_hint` 翻轉為 `uv tool install pyrefly-producer`
優先（spawn 錯誤訊息同步受益）；Y2 smoke test 落地
`tests/lsp_status_availability.rs`（3/3 綠）。文檔四件：repo README
Quickstart 雙路徑（uv/pip 消費者面在前、cargo 降 developer face）
＋uvx `--from` 條款＋rust-analyzer 系統依賴行＋兩軸版號關係＋SM-10
疊影辨識；plugin/README prerequisites wheel 塊；AGENTS.md Usage 段
wheel 安裝。plugin `0.1.3→0.1.4` 三處 bump＋`dist-marketplace.sh`
重跑（slice 內三檔 0.1.4＋新 wrapper 驗畢，Y3 條款滿足）。已知邊角
（記錄不擴 scope）：freshness WARN 的 rerun 提示固定說 `cargo
install --path`——pip-face binary 帶 checkout 的罕見組合下建議通道
不匹配（身分標示功能不受影響）；`rustfmt` 1.9.0（2026-05-25）與
repo 既有格式全面 drift（含未觸檔案）——判定環境級 drift，本弧只
約束自己新增的行，不整批重排；`rust_backend.rs:112` unused var
為預存項。收尾：Capabilities 兩行（新增 wheel 分發行＋MCP 行附註
spawn 翻轉）＋kanban Done＋本 EP 歸檔；ai-rules `[cr-dist]` handoff
prompt 隨收尾報告交付（口徑＝本段文檔條款）。

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
6. **版號面地圖（ai-rules 回饋 ①，2026-08-28 裁決：部分採納）**：
   版本面有二軸三通道——binary 契約軸（workspace＝PyPI，本條款
   1-5）與接線軸（plugin manifest＋marketplace entries，0.1.x 序列）。
   **維持刻意不同步**：兩軸變更理由不同（binary 契約 vs plugin 內容），
   且機械上無交叉比較發生（ZCode 更新提示只比 marketplace entry vs
   已裝 plugin.json；`uv tool upgrade` 只看 PyPI）——強制「一次 bump、
   雙發佈」＝無內容變更的 cache churn＋假版本史。「哪個是最新」的混淆
   以文檔解（README 記兩軸關係）＋S3「版號面盤點」把關，不以 bump
   同步解。
7. **無 checkout 機器的 freshness 語義（ai-rules 回饋 ④）**：stale
   WARN 的比對源是 CR checkout——wheel-only 機器永遠靜默＝設計使然
   （無可比對源）；該場景的版本真相源＝PyPI（`uv tool upgrade` 迴路），
   `--version` 自帶 rev 供人工比對已足。`check-update` 子命令（查
   PyPI 最新版）列未來候選，不入本 EP。

## NOT（scope boundary——防 scope creep）

- **不做** NT 式私有 dev-wheel index（freshness 已由 post-commit hook
  本機解；外部 bleeding-edge 消費者出現再議）。
- **不做** CRG 式 `setup`/`install` 子命令（獨立薄弧；解 onboarding
  不解 freshness）。
- **不做** CC community marketplace 投稿（wheels 落地後另議）。
- **不做** Windows wheels、musllinux、**Linux（glibc）——2026-08-28
  縮編裁決：驗證與發布矩陣＝macOS arm64 單腿，Linux 消費者出現再
  重啟（CI 面低成本擴充）**。
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
4. `/audit-test`：S1-S3 為 yaml/pyproject/config 軸（無 callable
   新增）——跳過成立；**S4 例外**（review Y2）：`lsp_status`
   availability 探測是 Rust 行為變更——S4 build 時補最小 smoke
   test（status 輸出含 availability 欄）或明示免測理由（PATH 環境
   依賴）。
5. **ai-rules handoff**：交付 prompt（code-reality SKILL.md 安裝段
   於 wheels 上線後翻轉；觸發條件＝S3 首發落地）。EP 內不自動跨 repo
   寫入。ai-rules 端 `[cr-dist]` 翻轉卡已在該 repo Backlog（2026-08-28
   回饋附記）——S3 落地時觸發該卡，內容口徑與本 EP S4 文檔條款
  （uv tool install 定位、rust-analyzer 系統依賴行）對齊。
