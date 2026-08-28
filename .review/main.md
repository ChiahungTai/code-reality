# Review: mcp-bin version face fix (uncommitted diff)

> 2026-08-28（W5 `4a36b22` 後續微修）。根因＝`code-reality-mcp` bin 無
> `--version` 分支——成員測試式 parse（`any(== "--stdio")`）讓一切未知
> flag（含 `--version`/`--help`/拼錯的 `--stdioo`）靜默落進 HTTP 常駐
> 預設：bind 8200→印 `[OK] listening`→阻塞（W5 收尾驗證時實測掛住一
> 個背景進程）。freshness EP 的「SM-1 四 bin version 面」宣稱對 mcp
> bin 不實（該 smoke 從未真跑過 `code-reality-mcp --version`——跑過
> 必掛）。
>
> 前弧記錄：W5 `4a36b22`（全文見 git history）。

## Fix

- `bin/code-reality-mcp/main.rs`：`--version`/`-V`（umbrella 同型
  `pkg[+rev]` 面）與 `--help`/`-h` early-exit；其餘非 `--stdio` 參數
  loud 拒絕（exit 2）——絕不靜默啟動未請求的 listener。launchd 無參
  數＝HTTP 預設、plugin `--stdio` 兩契約不變（调用方詞彙掃描封閉：
  launchd plist 無參數、plugin/README 僅 `--stdio`）。
- `tests/freshness.rs`：`mcp_version_face_carries_rev`（原陷阱回歸
  釘——修前此 spawn 必掛）＋`mcp_rejects_unknown_arg_loudly`（同類
  陷阱：拼錯 flag 不得靜默起 server）。

## 機械驗證

- `cargo test -p code-reality --test freshness`：5/5 綠（含兩新釘）
- live：`--version` → `0.1.0+4a36b22-dirty` exit 0 秒退；F-B 自指
  WARN 於髒樹實境可見（`CR checkout /Users/ctai/Github/code-reality ...`）
- 已知 pipe 教訓再犯一次：live probe `| head -1` 吃掉 bogus 的 exit 2
  （測試釘住，不影響結論）——no-pipe 規則三犯，記錄

## Dual-context 判讀（fresh R1-R4＋primed P1-P4 → judge 全採納或閉合）

- **R-1/P-3 ✅**：`--help` 改 stdout＋exit 0（umbrella/CLI 慣例；原
  eprintln+0 語義矛盾）——`mcp_help_face_answers_on_stdout` 釘。
- **R-2/P-2 ✅（取更強解）**：anywhere-match 改 **ordered per-arg
  loop**（lsp-bridge 同型）——首見未知參即拒（`--bogus --version`
  不再吞 typo）、`--stdio --version` version 勝出，組合序由
  `mcp_arg_priority_is_ordered` 釘。
- **R-3 ✅**：s6 `bin_help_face` 從 vacuous「bin builds」升級為真釘
  （--help stdout+exit 0）；freshness 測試檔 module doc 補 mcp 面。
- **R-4/P-4 ✅**：`freshness::version_face()` 共用 helper（umbrella
  route arm＋mcp bin 同 crate 去重；跨 crate 拷貝維持——no-dep 條款）。
- **P-1 🟡 ✅（EP 閉合）**：freshness EP 追加 v3 結算更正（SM-1/S2
  對 mcp bin 不實＋自相矛盾）後歸檔 `_done/`——持久意圖源不再帶
  假覆蓋信心。
- 雙側 caller 詞彙封閉獨立複掃一致（launchd 無參、plugin/README 唯
  `--stdio`、無 flag 型 port caller）；strict-args＝家族歸隊（bridge/
  pyrefly-index 早已 exit 2 拒未知）。

