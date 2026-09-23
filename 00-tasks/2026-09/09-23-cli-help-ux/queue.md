# 佇列：CLI help/UX 六條（delegate-bridge DB-30 順手探測轉交，2026-09-23）

來源：scbus 信 `fb3e268c`＋更正封 `c7d07dfb`（sess_64d6fbf9 delegate-bridge-sess）。調查結論轉交，歸 CR 線自決。**排程：當前 source identity 鏈收結後開弧**（standard tier——help 文面＋既有 exit 語義對齊，無新 boundary 決策，不寫 standalone EP；card AC 直行或小弧）。

## 裁決

| # | 建議 | 裁決 | 證據/備註 |
|---|---|---|---|
| ① | `--help` 全命令最高優先（先於 `--repo` 必填檢查；build/delta_tour 已是） | ✅ 採納 | 實測 `graph_query --help`＝exit 2 [FAIL] 面、`delta_tour --help`＝exit 0 正常 help——不一致屬實；需盤點 20 子命令逐一對齊 |
| ② | unknown 子命令 exit 0 → 應 exit 2 | ❌ **不成立**（本線已符合） | 實測 0.9.2+cfa34e4：usage 走 stderr＋**exit 2**；HEAD 源碼錨 `main.rs:64-68`（`_ =>` arm `exit_code: 2`）。來信實測面待對方覆核（可能量到別的 binary/路徑） |
| ③ | 頂層 help 升級：20 子命令分組＋一句話定位＋`詳見 <cmd> --help`＋常見 workflow 一行 | ✅ 採納 | 現為裸列（`main.rs:72-93` SUBCOMMANDS＋usage 行）；分組面＝read face／write face／audit／tour／protocol |
| ④ | 子命令 help 全覆蓋＋對齊 build 模板（中英統一） | ✅ 採納 | 抽查屬實（delta_tour 有 argparse 文件、graph_query 無 help 面） |
| ⑤ | `[FAIL]` 訊息補 how-to-fix 救回指引（graph_query「為何不猜 cwd」為範例） | ✅ 採納 | 逐命令盤點時一併做 |
| ⑥ | 頂層 help 一行註記：delegated/review 場景僅用唯讀面（refs/callers/closure/detect_changes），禁 build/snapshot/delta_tour 寫入面 | ✅ 採納 | 與 bridge-dispatch 紀律對齊，帶到使用現場 |

## 開弧時備忘
- ⑥ 的唯讀面清單以 ai-guide `skills/code-reality/SKILL.md` 接線紀律為準對照（唯讀面枚舉要與 guide 一致，防兩源漂移）。
- ①/⑤ 盤點面＝全部 20 子命令（`SUBCOMMANDS: [&str; 20]`），逐命令跑 `--help`＋一個壞參案例，輸出對照表後改。
- 測試腿：help/exit 行為 pin（exit 2 面已有的測試不破壞；新增 help-priority pin）。
