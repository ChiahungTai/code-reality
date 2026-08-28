# [infra] Binary freshness face — rev 嵌入＋--version＋stale WARN＋post-commit 自動重裝

## 目標

安裝面 binary 與 repo HEAD 的靜默漂移變 loud：`git describe` 嵌入三
bin（build.rs）、`--version` 面、消費時 stale WARN（CR checkout 在場
且 rev 不符才印）、CR repo post-commit hook 背景重裝變動 crate。

## 相關

- EP：`ai-analysis/execution-plans/ep-binary-freshness-face.md`（baseline `2442692`）
- 動機實案：2026-08-28 W3/fix relay 弧三次踩到 `cargo install` 早於
  程式碼編輯（記憶 code-reality-rust-route ㊶）
- 前置已修：資料殘留面（sidecar 失效）＝commit `2442692`

## 驗收標準

1. 三 CLI bin `--version` 印 `<pkg>+<rev>`；git 缺席 fallback
2. 安裝面落後 HEAD → 任一 CLI 呼叫帶一行 stderr WARN；無 checkout
   機器零輸出
3. commit → hook 背景重裝（前景 <1s），install.log 兩行
4. index/graph.db 產出 byte-determinism 不受影響（rev 只進 binary）
5. 回歸測試：rev 比對純函式表驅動＋三 bin version 面

## 備註

- 慣行定位：rev 嵌入/--version＝Rust 慣行（vergen/ripgrep 先例）；
  post-commit hook＝repo 自有自動化，非生態 norm（EP 查證段）
