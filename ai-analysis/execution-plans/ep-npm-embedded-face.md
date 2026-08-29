# EP: npm 內嵌 binary 面（CC 一鍵全棧；驗證置前）

> **ep_type**: implementation
> baseline: 63b42c8（EP 建立點）／ build-start: 2016828（S2 開工時重記，/implement 階段 1）

Source: user 2026-08-29 連續裁決——①npm probe 雙側實證（ZCode
**不**跑 npm ci＝機制 CC-only；CC 跑且平台套件落地）②「CC 上完全用
plugin、零 uv」值得開路。**uv/PyPI 主通道不動**；本 EP 是 CC 消費者的
便利面（雙面架構：uv 主面＋npm 內嵌 fallback 面）。

## S1 結算（2026-08-29 probe，kill-gate 已過）

**判定：P2 通 ⇒ EP 續行（S2 開工）。**

| 路線 | 結果 | 證據（debug log 原文節錄） |
|---|---|---|
| P2 npm deps spawn（重點） | ✅ 通 | `Connection failed after 224ms (CONNECTION_CLOSED): Connection closed`——process 真實執行（shim→esbuild 印 version→exit）後連線關閉；**非 ENOENT**。`${CLAUDE_PLUGIN_ROOT}` 展開＋node_modules 路徑解析＋binary 執行全鏈成立 |
| P1a bin/ Bash PATH | ✅ 通 | `command -v crbinprobe` → plugin root `bin/` 完整路徑（bin/ 注入 Bash 工具 PATH） |
| P1b bin/ 裸名 spawn | ❌ 不通 | `Executable not found in $PATH: "crbinprobe"`（7ms）——MCP spawn PATH **不含** plugin bin/ |
| P3 pnpm 變體 | ❌ CC 不支援 | pnpm-lock.yaml→靜默無 node_modules；`packageManager: "pnpm@…"` 同樣無視。CC 安裝＝「有 package-lock.json 才 npm ci，否則靜默跳過」，嚴格 npm-only |

**S1 附帶發現（已吸收進 S2/S3 設計）**：

1. **spawn-failure backoff**（S1 原判讀「usage-gate」經 S3 probe 修正）：
   plugin MCP server **hard spawn failure 後**，後續 session 跳過重試
   （S1 run1 ENOENT→run2-4 跳過；刪 `pluginUsage` 條目可清除）。成功
   連線則**每 session 穩定 eager connect**——S3 SM-1 probe 實證：
   `usageCount: 0` 下第二 session 照樣 `Successfully connected`（雙
   server）。雞生蛋風險降級：殘留風險＝「首 session 炸掉→環境修好
   （如補裝 uv）→backoff 殘留」——對策：README 註記重裝 plugin 即
   重置。
2. **local-marketplace root 錯位**：directory-source 的
   `${CLAUDE_PLUGIN_ROOT}`＝**來源目錄**、CC npm ci 落 **cache**——probe
   以「來源目錄自行 npm ci」等價重現生產形態。github marketplace（生產
   形態）clone 進 cache、npm ci 落 cache、root＝cache，三者一致；S3 以
   真 marketplace 安裝補驗。
3. **x64-trap**：本機 npm（Homebrew x64）三處安裝全挑 darwin-x64；
   x64 binary 經 Rosetta 可跑（實測）。→ S2 對策定案。
4. **shim 形**：esbuild `.bin/` 是 node-script shim（shebang
   `#!/usr/bin/env node`，spawn 需 node 在 PATH）；**平台套件直接宣告
   bin** 則 `.bin/<bin>` symlink 指 native binary 本體——零 node 依賴。
   → S2 採後者。

## 已證事實（2026-08-29 雙 probe 合併）

| 事實 | 證據 |
|---|---|
| CC 2.1.250 自動 `npm ci --ignore-scripts` | cache 落 node_modules（is-odd＋esbuild＋`@esbuild/darwin-x64`） |
| CC 安裝嚴格 npm-only | pnpm-lock／packageManager 均無視且靜默跳過（S1-P3 兩變體） |
| `.mcp.json` command 欄 `${CLAUDE_PLUGIN_ROOT}` 展開＋posix_spawn | S1-P2：展開後完整路徑出現在錯誤訊息 |
| optionalDependencies 平台挑選生效；required 直接依賴平台套件才炸 notsup | x64 npm 挑 darwin-x64；首版 probe 實撞 `notsup Actual cpu: x64` |
| ZCode 無 npm ci 機制 | cache 只複製 package.json/lock、零 node_modules、秒裝 |
| npm 發 Rust binary 模式成熟 | esbuild／biome／Turborepo／`@openai/codex` 先例 |

**未證環節（S3 承接）**：args 欄（`/bin/sh -c` 字串內）placeholder
展開（command 欄已證）；interactive session 的 usage-gate 行為；真
github marketplace 形態的 root 一致性。

## EP Review Findings

| ID | 嚴重度 | EP 段落 | 問題 | 建議 | 狀態 |
|----|--------|---------|------|------|------|
| 1 | 🔴 必須修正 | S2 | npm-publish 鏈三斷點：`npm pack --dry-run` 不產檔（upload 無物可上）、publish job 缺 download-artifact、tarball glob 誤植 `npm-` 前綴（實際檔名 `code-reality-darwin-arm64-<ver>.tgz`） | pack 兩步（dry-run 驗清單＋真 pack 產檔）＋upload；publish 增 download-artifact＋glob 修正 | implemented |
| 2 | 🔴 必須修正 | S2 | `needs: build` drift——實際 job id `build-wheels`（release-wheels.yml:35） | 改 `needs: build-wheels`＋download-artifact 補 name/merge-multiple/path（沿襲既有三個 publish job 參數形） | implemented |
| 3 | 🟡 建議 | S3 | 根 marketplace.json 版號錨點行號錯：version 在 line 9（line 11 是 tags） | 錨點改 `marketplace.json:9` | implemented |
| 4 | 🟡 建議 | S2 | `.gitignore` 漏 `npm/*/bin/`——bootstrap 本機組裝 ~80MB raw binaries 誤 commit 風險 | 增 `node_modules/`＋`npm/*/bin/` 兩條 | implemented |
| 5 | 🟡 建議 | S3 | wrapper 四象限 sh 回歸無落地檔——repo 測試面 cargo test 唯一，sh 測試是新載體無家 | `scripts/test-plugin-wrapper.sh`：jq 從 .mcp.json 抽 args 實測（測本體非複本）＋受控 env 四象限斷言 | implemented |
| 6 | 🟡 建議 | S4/SM-2 | Linux/Win CC 消費者指引死路——uv 面同為 macOS arm64 only，「uv tool install」在那些平台不可達 | README/SM-2 明示「兩面皆 macOS arm64 only」 | implemented |
| 7 | 🟡 建議 | S3 | `dist-marketplace.sh:17` `cp -R` 無排除——本地 npm ci 後 node_modules 進 ZCode mirror slice | dist 再生排除 node_modules；CC 本地測試走源目錄 | implemented |
| 8 | 🟡 建議 | S2 | npm-pack 未指 runs-on；版號 guard 依賴 runner 預裝 cargo（image 漂移非釘版契約）；npm pack 缺 working-directory | 補 `runs-on: ubuntu-latest`＋`dtolnay/rust-toolchain@stable`＋working-directory | implemented |
| 9 | ℹ️ | S2 | npm 套件 LICENSE 來源未說明（根 /LICENSE 存在且 MIT 相符） | 註明複製根 /LICENSE | implemented |
| 10 | ℹ️ | S2 | 要點列 files 欄但 pseudo package.json 缺 | 補 `"files": ["bin"]` | implemented |
| 11 | ℹ️ | S2 | bootstrap 後對當前版（複驗時＝0.3.0）重跑 npm 發布會 E403（npm 版號不可重 publish） | cadence 條款寫明：bootstrap 後下一 tag 必 >當前版 | implemented |
| 12 | ℹ️ | S3 | github 形態 npm ci 落點若非 plugin 目錄（root 錯位重現）無應急退路 | 已知未覆蓋補 package.json 移位條款 | implemented |

審查實證附註（不需改段落）：關鍵兜底假設「PATH prepend → lsp-bridge
backend 解析」**源碼級證實**——`crates/code-reality-lsp-bridge/src/session.rs:237`
`Command::new(&self.backend_cmd)` 裸名＋無 env_clear（child PATH 繼承）、
`server.rs:225` PATH 逐段掃描（backend_available `server.rs:208`）同語義
（行號隨併行 d8dd68e 收斂閘門修正位移 :216→:237；機制不變，2026-08-29
複驗）。npm 名稱可用（registry 404）、
PyPI wheels 在線（審查時驗 0.2.0；複驗時三 dist 已 0.3.0——bootstrap
源可重現）、SM-3 35MB 宣稱實測
34.9MB 相符、`claude plugin tag` 語意相符且 tag namespace
（`code-reality--v*`）與 crate `v*` 不碰撞、unzip glob 不觸 sboms、
五 bin 跨三 wheel 檔名零碰撞。

## UC 盤點

### Backlog 關聯
- `.kanban/Backlog/npm-embedded-face.md`（本 EP 追蹤＋新 UC 合一；
  驗收標準 1「S1 結論」已交付）。無缺卡——所有 UC 已有卡片。

### SYSTEM-MAP 影響
- 無 SYSTEM-MAP.md——跳過（收尾步驟同步跳過）。

### 掃描範圍
- 根 AGENTS.md Capabilities；`.kanban/Backlog/npm-embedded-face.md`

### 既有 UC 狀態
| 能力 | 狀態 | 影響 |
|------|------|------|
| Unified MCP interface（plugin 面） | ✅ | 更新——CC 端 wrapper 增 node_modules 候選（PATH 之後）＋child PATH prepend |
| PyPI platform-wheel distribution | ✅ | 不動——維持主面；本 EP 是疊加面 |

### 新增 UC
| 能力 | 狀態 | 實作路徑 |
|------|------|---------|
| CC 一鍵全棧（plugin 內嵌五 bin、零 uv） | 📋 | npm 平台套件（S2）＋plugin package.json/lockfile＋wrapper node_modules 候選（S3） |

## Scenario Matrix

| # | 場景 | 觸發 | 預期行為 | Checkpoint | 對應能力 |
|---|------|------|---------|------------|---------|
| SM-1 | CC 消費者只裝 plugin | marketplace install | npm ci 落五 bin；wrapper 從 node_modules spawn；真 server 連線＋工具可用 | S3 | 新增 UC |
| SM-2 | x64 npm 機器（arm64 Mac＋Rosetta node） | npm ci | optionalDep mismatch → **skip 非 fail**；node_modules 空 → wrapper 落 PATH/uv 指引（uv 裝 arm64 wheel 可用）——fail-loud 且指引可達；Linux/Win 消費者＝兩面皆無支援（wheel 軸同縮編）——S4 明示平台邊界，指引不誤導 | S2/S3 | 新增 UC |
| SM-3 | plugin 更新（版號 bump） | marketplace update | 內嵌 bin 隨 lockfile 更新（~35MB 級重拉）；plugin 版號＝內嵌套件版號＝workspace 版號（三同） | S3 | 新增 UC |
| SM-4 | 兩面並存（uv＋內嵌） | 使用者兩者都裝 | **PATH-first**：uv 面（可獨立升級）優先，node_modules 是零 uv 救援；優先序文件化；freshness WARN 照常（rev 嵌入與載體無關） | S3 | freshness face |
| SM-5 | ZCode 使用者裝同 plugin | install | 無 node_modules（機制缺席）→ placeholder 展開為空→路徑不存在→graceful 落 PATH/fallback——**行為不變**；文檔明示兩家差異 | S3/S4 | 新增 UC |
| SM-6 | bin/ 路線子驗證 | P1 probe | （已結算）Bash PATH 可見 ✅／MCP spawn 不可見 ❌——各自記錄，S3 不走 bin/ 面 | S1 ✅ | 新增 UC |
| SM-7 | spawn-failure backoff | 首 session server 炸掉（如 x64-npm 空 node_modules） | 後續 session 跳過重試（backoff）；環境修好後**重裝 plugin** 即重置——README 註記。成功連線則每 session 穩定連（S3 probe 雙 session 實證） | S3/S4 | 新增 UC |
| SM-8 | 零 uv 機器上的 type face | lsp-bridge spawn pyrefly-lsp backend | wrapper 命中 node_modules 時 **prepend `node_modules/.bin` 進 child PATH** → backend 找到內嵌 pyrefly-lsp；rust-analyzer 維持 system dep（缺席則 lsp_status 報 unavailable＋指引） | S3 | 新增 UC |
| SM-9 | npm 面使用 CLI 工具 | session 用 Bash 跑 `code-reality` CLI | **v1 邊界**：npm 面=MCP 工具面（查詢/審計）；CLI 面（graph_db build、snapshot、pyrefly-index 產製）歸 uv 主面——plugin/README 明示（不 ship bin/ symlink：ZCode 端 dangling symlink 有 shadow 風險） | S4 | 新增 UC |

## 段落劃分原則

S1 已結算（見上）。S2（產製側：npm 套件＋release 整合）→ S3（消費
側：plugin 整合＋雙面條款）→ S4（文檔）。S3 依賴 S2 的套件**已上
registry**（lockfile 生成需 registry 解析 integrity）——S2 bootstrap
publish 是 S3 的硬前置。

---

## S1: 雙路線 probe（✅ 已結算，2026-08-29）

結果與證據見頂部「S1 結算」段。原始 log 已隨暫存目錄清除；判定證據
原文已全數收錄於結算表與附帶發現段。

## S2: npm 平台套件打包軸

### Context

- **UC 引用**：實作「CC 一鍵全棧（plugin 內嵌五 bin、零 uv）」的產製
  側——npm 平台套件 `code-reality-darwin-arm64`（五 bin、macOS
  arm64 only、版號跟 workspace）。
- **依賴關係**：S3 的硬前置（lockfile 需已發布套件）；獨立於其他 EP。
- **基礎設施盤點**（研究實證）：
  - 五 bin＝`crates/{code-reality,pyrefly-producer,code-reality-lsp-bridge}/src/bin/*`
    自動發現（無 `[[bin]]` 段）；workspace 成員 glob `crates/*`
  - 版本單一源：根 `Cargo.toml:7` `[workspace.package] version = "0.3.0"`；
    三 crate 皆 `version.workspace = true`
  - wheel 佈局：`<pkg>_<ver>.data/scripts/<bin>`（unzip 實證）、零
    Python 模組、`py3-none-macosx_11_0_arm64`——bins 可直接解包
  - release：`.github/workflows/release-wheels.yml`——tag `v*`＋
    `workflow_dispatch` dry-run；maturin 1.15.0 釘版、toolchain
    1.96.0（rust-toolchain.toml）；現役 matrix 僅
    `macos-latest / aarch64-apple-darwin`（x64/linux 註解留用）；
    build job 產 `wheels-aarch64-apple-darwin` artifact；三個 PyPI
    publish job 各綁 environment（PyPI per-project 限制——npm 無此
    限制，單 environment/token 即可）
  - repo 零 npm 資產（`fd package.json` 零命中）；`.gitignore` 無
    node_modules 條目（本段新增）
  - `launchd/` plist 指絕對路徑 `~/.cargo/bin/code-reality-mcp`——與
    npm 面完全解耦，零影響
- **設計決策**（修訂原草稿）：
  1. **單一套件形**（棄「主套件＋平台套件」兩層）：零 JS 下主套件
     （原草稿 `code-reality-bin`）是純 indirection；plugin 以
     `optionalDependencies` 直接依賴平台套件。未來需要 JS shim 再升
     兩層形（演化保留）。
  2. **平台套件直接宣告五 bin**（S1 發現 4）：`node_modules/.bin/<bin>`
     symlink 指 native binary——spawn 零 node 依賴。
  3. **x64-trap 對策＝(b) arm64-only＋fail-loud 指引**：縮編哲學沿襲；
     x64-npm 消費者 optionalDep skip→wrapper 落 uv 指引（uv 裝的是
     arm64 wheel，機器本身 arm64——指引可達）。對策 (a)（出 x64 變體；
     Rosetta 可跑已實證）留作重啟條件：x64-npm 消費者實際出現。
  4. **版號機械 guard**：CI 比對 npm package.json 版號 vs `cargo
     metadata` workspace 版號，不符即 fail——單一版號源不靠紀律。
- **技術選型**：npm 套件從**同 tag 的 wheel artifact 解包組裝**（產物
  同源、rev 嵌入不變）；首次 0.3.0 以手動 bootstrap publish（源＝
  PyPI 已發布的 0.3.0 wheels，下載解包——零重 build、可重現）；之後
  由 workflow 自動化。
- **成功標準**：`npm view code-reality-darwin-arm64` 可見 0.3.0；本機
  scratch install（x64 npm）驗 skip 語義；tag dry-run 產出正確 tarball。

### 核心實作要點

1. `npm/code-reality-darwin-arm64/`：package.json（name/version/os/
   cpu/bin×5/files/LICENSE/README）——版號欄由 guard 死鎖 workspace。
2. `.github/workflows/release-wheels.yml` 增 `npm-pack`＋`npm-publish`
  job：`needs` build → download wheels artifact → `unzip -j` 取
  `.data/scripts/*` 五 bin → 組裝 → 版號 guard →（tag 觸發）
  `npm publish`（`NODE_AUTH_TOKEN`＝`NPM_TOKEN` secret；
  `workflow_dispatch` 只 pack＋upload artifact 不 publish——沿襲
  dry-run 慣例）。npm 無 PyPI 的 per-project environment 限制，單
  environment `release-npm`。
3. `.gitignore` 增 `node_modules/`＋`npm/*/bin/`（前者＝plugin 本地
   測試產物；後者＝bootstrap/CI 組裝的 raw binaries——進 repo 的只有
   package.json/README/LICENSE，bin/ 永不入 repo）。
4. 套件名可用性：build 時 `npm view code-reality-darwin-arm64` 前置
   檢查（被占則改 scoped `@code-reality/darwin-arm64`，S3 lockfile
   同步）。

### Pseudo Code

```
npm/
  code-reality-darwin-arm64/
    package.json    # 發布內容定義（此檔進 repo；bin/ 由 CI 組裝，不進 repo）
    README.md       # 短——雙面說明指標（S4 內容化）
    LICENSE         # MIT——複製根 /LICENSE（與 plugin.json 一致）
```

`npm/code-reality-darwin-arm64/package.json`：

```json
{
  "name": "code-reality-darwin-arm64",
  "version": "0.3.0",
  "description": "Native binaries for code-reality (macOS arm64) — embedded face for the Claude Code plugin",
  "license": "MIT",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "files": ["bin"],
  "bin": {
    "code-reality": "bin/code-reality",
    "code-reality-mcp": "bin/code-reality-mcp",
    "pyrefly-index": "bin/pyrefly-index",
    "pyrefly-lsp": "bin/pyrefly-lsp",
    "code-reality-lsp-bridge": "bin/code-reality-lsp-bridge"
  }
}
```

workflow 增段（簡記）：

```yaml
  npm-pack:
    needs: build-wheels     # 實際 job id（release-wheels.yml:35）
    runs-on: ubuntu-latest  # 只解包組裝——cargo 僅供版號 guard，不 build
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable   # guard 用；版號讀 manifest 不需釘 1.96.0
      - uses: actions/download-artifact@v5
        with: { name: wheels-aarch64-apple-darwin, merge-multiple: true, path: wheels }
      - run: |              # 版號 guard（機械單一源；drift 即 loud fail）
          ws=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages | map(select(.name == "code-reality")) | .[0].version')
          pj=$(jq -r .version npm/code-reality-darwin-arm64/package.json)
          [ "$ws" = "$pj" ] || { echo "version drift: workspace=$ws npm=$pj"; exit 1; }
      - run: |              # 解包五 bin（.data/scripts/ → 套件 bin/；glob 不觸 sboms、五 bin 跨三 wheel 零碰撞）
          for whl in wheels/*.whl; do unzip -j -o "$whl" '*.data/scripts/*' -d npm/code-reality-darwin-arm64/bin/; done
          chmod +x npm/code-reality-darwin-arm64/bin/*
      - working-directory: npm/code-reality-darwin-arm64
        run: |
          npm pack --dry-run   # 清單驗證（五 bin＋LICENSE＋README＋os/cpu）
          npm pack             # 真產檔：code-reality-darwin-arm64-<ver>.tgz
      - uses: actions/upload-artifact@v5
        with: { name: npm-tarball, path: "npm/code-reality-darwin-arm64/*.tgz" }
  npm-publish:
    needs: npm-pack
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    environment: release-npm
    steps:
      - uses: actions/download-artifact@v5
        with: { name: npm-tarball, path: tgz }
      - uses: actions/setup-node@v5
        with: { registry-url: 'https://registry.npmjs.org' }
      - run: npm publish tgz/code-reality-darwin-arm64-*.tgz
        env: { NODE_AUTH_TOKEN: "${{ secrets.NPM_TOKEN }}" }
```

bootstrap（一次性、本段內執行）：從 PyPI 下載三個 **0.3.0** wheel →
同法解包組裝 → `npm publish`（維持者本機登入；版本隨併行 v0.3.0
release 重錨——原規劃 0.2.0，v0.3.0 已上 PyPI，三 dist 實證 0.3.0）。
可重現：任何人從 PyPI 0.3.0 wheels 重走同路徑得 byte 同源 bin（rev
嵌入面相同）。**不可重跑條款**：bootstrap 後 npm 0.3.0 永久佔位——
含 npm job 的 workflow 不得再對 0.3.0 發布（npm 版號不可重
publish，E403）；下一 tag 必 >0.3.0（cadence 條款，見 S3 決策 4）。

### 驗證策略

- **S2-POC（scratch、不進 repo）**：x64 npm 本機 `npm install <tarball>`
  → 驗 optionalDep **skip 語義**（預期：套件被跳過、npm 不報錯——
  arm64-only 套件在 x64 npm 的行為）；`npm install --os=darwin
  --cpu=arm64 <tarball>`（若 npm 支援 override）驗正向安裝＋
  `node_modules/.bin/` 五連結指向 native binary。POC 檔頭格式見
  /ep-validate。
- **機械驗證**：`npm pack --dry-run` 輸出清單含五 bin＋os/cpu 欄位；
  版號 guard 兩方向（一致通過／drift fail）各跑一次。
- **tag 面驗證**：`workflow_dispatch` dry-run → npm-tarball artifact
  內容檢查（不 publish）。
- **已知未覆蓋**：`npm publish` 本身（首個 v tag 才觸發）；NPM_TOKEN
  設定為前置 user 手動（npmjs 帳號＋2FA＋granular token→repo secret；
  OIDC trusted publishing 若可用可取代 token——build 時查證擇一）。
- **Invariant Impact**：無（發布/打包軸，不觸 domain invariant）。

## S3: plugin 整合＋雙面條款

### Context

- **UC 引用**：完成「CC 一鍵全棧」消費側＋更新「Unified MCP interface
  （plugin 面）」。
- **依賴關係**：硬前置＝S2 套件已上 registry（lockfile 生成需
  registry 解析 integrity——`npm install --package-lock-only` 對未發布
  套件無解）。
- **基礎設施盤點**：
  - `plugin/` 四檔：`.claude-plugin/plugin.json`（0.1.4）、`.mcp.json`
    （16 行、兩 server、wrapper＝inline `/bin/sh -c`）、`README.md`、
    `skills/code-reality/SKILL.md`
  - wrapper 現行邏輯（`plugin/.mcp.json:6,12`）：`command -v`（PATH）
    → `~/.local/bin` → `~/.cargo/bin` → stderr 安裝指引＋`exit 127`
  - 版號三處同步：`plugin/.claude-plugin/plugin.json:3`、根
    `marketplace.json:9`、`.claude-plugin/marketplace.json:11`（
    `claude plugin tag` 驗一致性；`scripts/dist-marketplace.sh` 再生
    `dist/marketplace/`）
- **設計決策**（修訂原草稿，理由）：
  1. **PATH-first 維持**（草稿原寫 node_modules→PATH；修訂）：「uv＝
     主面（獨立更新迴圈）」條款下，可獨立升級的 PATH 版（uv tool）
     不應被 plugin 鎖版 binary 蓋掉。node_modules＝**第二候選**＝零
     uv 消費者的救援路徑。優先序：PATH → node_modules →
     `~/.local/bin` → `~/.cargo/bin` → fail-loud（雙面指引）。
  2. **placeholder 容錯設計**：node_modules 候選路徑以 shell 變數
     `"${CLAUDE_PLUGIN_ROOT}/node_modules/.bin"` 內插——CC 有展開
     （command 欄已證；args 欄未證→本段 POC）則命中；**未展開時
     env var 為空→路徑不存在→graceful 落下一候選**（ZCode 相容：
     行為不變）。
  3. **child PATH prepend**（SM-8）：wrapper 命中 node_modules 時
     `PATH="$nb:$PATH"; export PATH` 再 exec——lsp-bridge 的
     pyrefly-lsp backend 在零 uv 機器上由此解析（rust-analyzer 維持
     system dep）。
  4. **版號三同**：plugin 0.1.6→0.3.0 對齊 workspace（現值 0.1.6——
     併行 session 兩度 bump：0.1.5 release、0.1.6 skill absorb）；
     內嵌套件 lockfile 同釘 0.3.0。後續 cadence：workspace bump→tag→
     npm 套件發布→plugin bump＋lockfile 更新→marketplace 發布
     （plugin 跟隨 crate tag 一步）。
  5. **不 ship `plugin/bin/` symlink**（SM-9）：ZCode 端
     node_modules 缺席→dangling symlink 有 shadow 真_bin_ 風險
     （PATH 注入順序未證）；CLI 面明確歸 uv 主面文檔化。
  6. **usage-gate 對策**（SM-7）：plugin/README 教「裝完第一個
     session 先叫一次工具（如 lsp_status）」；S3 驗證 interactive
     行為後定稿（若 interactive 不受 gate 影響則降級為已知行為
     註記）。
- **成功標準**：local-marketplace probe 安裝後 fresh session 對真
  `code-reality-mcp` 出現 `Successfully connected`＋工具列表（非
  CONNECTION_CLOSED）；第二 session 行為記錄；無 node_modules 時
  wrapper 逐候選降級機械測綠。

### 核心實作要點

1. `plugin/package.json`（新增，`private: true`）：name
   `code-reality-plugin`、`optionalDependencies:
   {"code-reality-darwin-arm64": "0.3.0"}`；lockfile 以
   `npm install --package-lock-only --ignore-scripts` 生成後提交。
2. `plugin/.mcp.json` 兩 server 的 args 字串改寫（優先序見決策 1；
   fail-loud 訊息增雙面指引：「update the plugin (embedded face) or
   uv tool install code-reality (main face)」）。
3. 版號三處 bump 0.3.0（現值 0.1.6）＋`scripts/dist-marketplace.sh`
   再生（**排除
   `plugin/node_modules`**——該腳本 `cp -R` 無排除邏輯，本地 npm ci
   後會把 ~35MB 帶進 ZCode mirror slice；CC 本地測試走源目錄不經
   dist，slice 不需 node_modules）；plugin skills 內容不動。
4. `scripts/test-plugin-wrapper.sh`（新增，wrapper 回歸落地檔）：`jq`
   從 `plugin/.mcp.json` 抽 args 字串**直接執行**（測本體非複本——
   JSON 字串改了測試才會跟著動）；受控 env 四象限（PATH 有/無 fake
   bin × CLAUDE_PLUGIN_ROOT 指向有/無 node_modules 的目錄）斷言逐候選
   降級＋exit 127 fail-loud 訊息。

### Pseudo Code

`plugin/package.json`：

```json
{
  "name": "code-reality-plugin",
  "version": "0.3.0",
  "private": true,
  "optionalDependencies": {
    "code-reality-darwin-arm64": "0.3.0"
  }
}
```

wrapper（兩 server 同形，bin 名各異）：

```sh
nb="${CLAUDE_PLUGIN_ROOT}/node_modules/.bin"
if command -v code-reality-mcp >/dev/null 2>&1; then exec code-reality-mcp --stdio; fi
if [ -x "$nb/code-reality-mcp" ]; then PATH="$nb:$PATH"; export PATH; exec "$nb/code-reality-mcp" --stdio; fi
for d in "$HOME/.local/bin" "$HOME/.cargo/bin"; do
  if [ -x "$d/code-reality-mcp" ]; then PATH="$d:$PATH"; export PATH; exec "$d/code-reality-mcp" --stdio; fi
done
echo "code-reality-mcp not found (PATH, plugin node_modules, ~/.local/bin, ~/.cargo/bin) — main face: uv tool install code-reality; embedded face: update the code-reality plugin" >&2
exit 127
```

### 驗證策略

- **S3-POC（scratch marketplace、不進 repo）**：
  - args 欄 placeholder 展開（CC 未證環節）——`.mcp.json` args 內
    `${CLAUDE_PLUGIN_ROOT}` 從錯誤訊息/spawn 路徑判讀是否展開；未
    展開→退化驗證 graceful 降級。
  - 真 server 連線：local marketplace（來源 npm ci 重現 root 有
    node_modules）→ fresh session（usage stats 淨空）→ 預期
    `Successfully connected`＋MCP 工具列表＝**SM-1 驗收**（對照
    S1 的 CONNECTION_CLOSED＝process 會跑但不是 server；這次是
    真 server 講 protocol）。
  - usage-gate：同 plugin 第二 session 觀察 MCP 連線是否被跳過；
    interactive 手驗（user 執行一次 interactive session 用工具）。
  - ZCode 回歸：wrapper sh 邏輯機械測——`CLAUDE_PLUGIN_ROOT` 空、
    無 node_modules、PATH 有/無 bin 四象限，逐候選降級＋fail-loud
    斷言（純 sh 測試，不需 ZCode 本體）。
- **機械驗證**：lockfile 含 platform 套件＋integrity；三處版號一致
  （`claude plugin validate`）；`git grep` 無殘留舊 wrapper 字串。
- **已知未覆蓋**：真 github marketplace 形態（root 一致性）——
  plugin 發布後以 `claude plugin marketplace add <owner>/<repo>` 實裝
  驗證，屬本段驗收的最後一哩（發布動作本身=user outward 同意）。
  **應急條款**：若最後一哩發現 CC npm ci 落點非 plugin 目錄（root
  錯位在 github 形態重現），退路＝package.json/lockfile 移位到實際
  落點——S2 盤點「repo 零 npm 資產」敘述隨之修訂。
- **Invariant Impact**：無。

## S4: 文檔

### Context

- **UC 引用**：文檔化「CC 一鍵全棧」＋雙面條款；SM-9 邊界明示。
- 依賴：S3 定稿後（usage-gate 對策定案、優先序實測結果）。

### 核心實作要點

1. `plugin/README.md`：安裝段雙面——CC 一鍵（marketplace install，
   零 uv；「裝完先叫一次工具」指引）vs 通用 uv 面；SM-9 邊界（npm
   面=MCP 工具面；CLI/graph build 走 uv）；SM-5 兩家差異（ZCode 無
   npm ci→uv 面）；**平台邊界明示：兩面（uv＋npm）皆 macOS arm64
   only、其他平台不支援**——防 Linux/Win 消費者照 uv 指引撞牆。
2. `npm/code-reality-darwin-arm64/README.md` 內容化（套件用途＋非
   直接消費聲明——它是 plugin 的 optionalDependency）。
3. 根 `AGENTS.md` Capabilities：新增「CC 一鍵全棧（npm 內嵌面）」行
   （入口：plugin marketplace install；狀態 ✅）＋「PyPI
   platform-wheel distribution」行附註雙面。
4. **ai-rules handoff**（跨 repo——寫入責任在 spawn 端）：本 repo
   備妥 `ai-reality` skill（ai-rules `skills/code-reality/SKILL.md`）
   安裝段的修訂內容（CC 一鍵敘述），**不跨 repo 寫**——交付為
   handoff 區塊（本 EP 收尾時列出），由 ai-rules session 套用。

### 驗證策略

- 文檔驗證（docs mode）：`rg` 殘留、跨檔一致（版號三同敘述、優先序
  敘述與 S3 實測一致）、`claude plugin validate` 綠。
- 已知未覆蓋：ai-rules 套用本身（另一 repo 的 session 驗證）。

## NOT（scope boundary）

- **不做** ZCode npm 面（機制缺席，probe 實證）。
- **不動** uv/PyPI 主面（疊加非取代）。
- **不綁** rust-analyzer（system dep 維持 rustup）。
- **不做** Windows/Linux 平台套件（縮編沿襲；重啟條件同 wheel 軸）。
- **不做** x64 npm 變體（對策 (b) 定案；(a) 重啟條件＝x64-npm 消費者實際出現）。
- **不做** launchd/CLI/CI 面的 npm 分發（那些場景 uv/uvx 已覆蓋）。
- **不 ship** `plugin/bin/` symlink（ZCode dangling shadow 風險）。

## 整合策略

- S2→S3 線性依賴（S3 lockfile 需 S2 套件上 registry）；S4 收尾。與
  data-plane EP 併行不互堵（動檔案不相交：本 EP 觸
  `npm/`、`plugin/`、workflow、`.gitignore`、`AGENTS.md` Capabilities）。
- 版號單一源條款延伸：npm 套件版號＝workspace 版號（guard 機械
  死鎖）；plugin 版號自本 EP 起對齊 workspace（三同 cadence 見 S3
  決策 4）。
- baseline：EP 建立點 `63b42c8`；S2 build 開始時 /implement 階段 1
  重記（data-plane 併行 commit 會移動 HEAD）。**併行變更已吸收**
  （2026-08-29 複驗重錨）：v0.3.0 release（workspace＋PyPI 三 dist
  實證 0.3.0）、plugin 0.1.6、release-wheels actions 升 Node-24
  majors（checkout@v5／upload/download-artifact@v5——`build-wheels`
  job id 與 artifact 名未變）、lsp-bridge 行號位移；EP 內版號字面
  與 action majors 已隨之更新。

### 執行結算（2026-08-29 deep-work；含 EOTP 後狀態修訂）

**已執行（證據）**：
- S2 全部 repo 面：npm/ 三檔、workflow npm-pack/npm-publish（含
  review F3 補的五 bin 完整性斷言）、版號 guard **雙向實測**
  （一致 PASS／drift loud-fail）、`.gitignore` 三條（`node_modules/`
  ＋`npm/*/bin/`＋`npm/*/*.tgz`）。
- S2-POC 雙向：x64 npm＋arm64-only optionalDep → **skip 非 fail**
  （SM-2 語義）；`--os=darwin --cpu=arm64` override → 五 bin 落地、
  `.bin/` symlink 指 native binary 本體。
- bootstrap 組裝：PyPI 0.3.0 三 wheels → 五 bin（Mach-O arm64 實證）
  → tarball 35.2MB（`.agent-tmp/npm-bootstrap/bootstrap.sh` 可重現
  路徑）。
- **bootstrap publish：被 EOTP 擋下**——web-login token 需一次性
  密碼；繞過 OTP 的 granular token 只存在 gh secret（不可讀）。
  registry 實測 E404（未發布）。
- S3：wrapper 改寫、`scripts/test-plugin-wrapper.sh`（四象限＋Q4b
  unset＋Q5 PATH-beats-node_modules，**掛載** `crates/code-reality/
  tests/plugin_wrapper.rs` 進 cargo test 唯一測試面——review F2）、
  dist 排除、版號六處 0.3.0、**plugin.json schema 修復**
  （skills/mcpServers 需 path-form——原裸名使 CC 2.1.251 fresh
  install 全炸，`claude plugin validate` 綠）。
- **SM-1 probe（file: staging 偏差形態）**：probe 副本以 file:
  tarball＋override 旗標裝真 bin、剝離 PATH 強制 embedded face →
  fresh session **雙真 server `Successfully connected`**（code-reality
  673ms／lsp-bridge 1193ms，rmcp 3.1.4、hasTools:true）；第二 session
  照樣連線——**S1 usage-gate 判讀修正為 spawn-failure backoff**
  （成功連線不觸發）。registry 解析形態的 probe 留給 publish 後。
- S4：plugin/README 雙面、AGENTS.md Capabilities（🟡 標注未完結）。
- 驗證閘門：全量 `cargo test` rerun **exit 0**（43 test-result-ok、
  零 FAILED；首跑單一失敗＝平行 session f82b274 記錄的 starvation
  flake，隔離 32/32 綠＋本弧零 .rs 歸因）。

**PENDING-user（依序；①② 已於 2026-08-29 完成）**：
1. ~~bootstrap publish~~ **DONE**——registry 實證 `npm view
   code-reality-darwin-arm64` → 0.3.0。
2. ~~lockfile＋registry probe~~ **DONE**——lockfile（registry 解析＋
   integrity，與 tarball shasum 同源）已提交；registry 形態 probe：
   CC marketplace add＋install 全程成功（CC 自身 npm ci 在 x64 npm
   下乾淨 skip arm64 套件——SM-2 機器形態由 CC 親測）＋剝離 PATH
   session 雙真 server `Successfully connected`（89ms／621ms）。
3. interactive session 手驗（headless 已雙 session 實證；interactive
   補最後一塊）。
4. push＋github marketplace 最後一哩（`claude plugin marketplace add
   ChiahungTai/code-reality` 實裝；含 GH environment `release-npm`
   確認——workflow 首次 tag run 前須存在或可自動建立）。
5. token 輪替：首發後 granular 縮 scope 到只釘
   `code-reality-darwin-arm64`＋更新 `NPM_TOKEN`；**或遷移 OIDC
   trusted publishing**（npmjs 網頁 Settings→Trusted Publishing 掛
   `ChiahungTai/code-reality` release workflow——消除 90 天輪替義務；
   user 裁決）。

**tree 紀律**：`scripts/lsp_answers.json` 是非本 EP 的未 commit 檔——
不審不改不納入 commit；本 EP 相關 commit 只 add 指名檔案（EP、kanban
卡、npm/、workflow、plugin/、scripts/、.gitignore、AGENTS.md）。

**tree 紀律**：`scripts/lsp_answers.json` 是非本 EP 的未 commit 檔——
不審不改不納入 commit；本 EP 相關 commit 只 add 指名檔案（EP、kanban
卡、npm/、workflow、plugin/、scripts/、.gitignore、AGENTS.md）。

## 收尾步驟

1. Capabilities：新增「CC 一鍵全棧（npm 內嵌面）」行（入口：plugin
   marketplace install＋`node_modules/.bin` spawn；狀態 ✅）；更新
   「Unified MCP interface」行（wrapper 增 node_modules 候選）；kanban
   `npm-embedded-face.md` 搬 Done；EP 歸檔 `_done/`。
2. 無 SYSTEM-MAP——跳過。
3. instruction 檔：根 AGENTS.md Capabilities＋plugin/README 雙面同步。
4. /audit-test：S3 `.mcp.json` 行為變更——wrapper 探針回歸
   （`scripts/test-plugin-wrapper.sh`：fail-loud／優先序四象限）併入。
5. ai-rules handoff 交付（內容區塊列出，ai-rules session 套用——
   觸發＝本 EP build 完成）。
