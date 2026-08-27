# CR 作為統一語言智慧層——use cases、情境矩陣與完全取代路線

> 2026-08-28 arch-thinking 結晶（pyrefly＋occurrence 雙弧收斂後）。
> 前提：harness 語言智慧的取得趨勢＝plugin 安裝（MCP tools），
> 而非 harness 內建 LSP 整合。本文回答：CR 要什麼才能**完全取代
> lsp python/rust**。

## 一、本質拆解：「取代 LSP」取代的是什麼

LSP 對 AI session 提供兩個面：

| 面 | LSP 操作 | 特性 |
|---|---|---|
| **結構面** | goToDefinition／findReferences／callHierarchy／documentSymbol／workspaceSymbol | 查詢型、可批次、可離線 |
| **型別面** | hover（型別簽名）／diagnostics（串流錯誤）／typeHierarchy | 互動型、增量、工作區狀態相依 |

**CR 現況**：結構面 ✅ 已達（Python＝pyrefly producer 78s 全量；
Rust＝rust-analyzer SCIP index）且**超越**（graph 級：impact
radius／hazard／communities／flows——LSP 結構性做不到）。型別面
❌ 真空（mosaic lsp_mcp 全刪後 ZCode 端無 hover/diagnostics）。

## 二、Use Cases × 情境矩陣（現況）

| # | 情境（session 問句） | CR 入口 | 取代的 LSP 操作 | 狀態 |
|---|---|---|---|---|
| U1 | 「X 定義在哪、誰引用它」 | `refs(symbol)` | goToDefinition＋findReferences | ✅ |
| U2 | 「誰呼叫 X（含間接）」 | `callers`/`closure` | incomingCalls（且 LSP 無 transitive） | ✅ 超越 |
| U3 | 「X 刪掉安全嗎」 | `hub_refs --hazard` | 無對應（手動 grep 時代） | ✅ 獨有 |
| U4 | 「改 X 會炸到誰」 | `impact_radius`/`affected_flows` | 無對應 | ✅ 獨有 |
| U5 | 「這檔案的結構」 | `symbols(file)` | documentSymbol | ✅ |
| U6 | 「找叫這名字的符號」 | `search`（FTS） | workspace/symbol | ✅（FTS 關鍵字非符號索引——詞法 vs 語義小差） |
| U7 | 「這 repo 的架構長怎樣」 | `arch_overview`/`hub`/`communities` | 無對應 | ✅ 獨有 |
| U8 | 「graph 完整嗎／有沒有漏」 | `audit` | 無對應 | ✅ 獨有 |
| U9 | 「這個運算式的型別／這檔的錯誤」 | —— | hover／diagnostics | ❌ **唯一缺口** |
| U10 | 變更前後的結構敘事 | `snapshot`/`transition`/`delta_tour` | 無對應 | ✅ 獨有 |

**矩陣結論**：LSP 九大操作中八項已覆蓋（兩項超越、四項獨有），
**唯型別面（U9）缺口**。

## 三、完全取代路線（依賴拓撲）

```
[done] Python 結構面：pyrefly producer（78s 全量、byte-deterministic）
[done] Rust 結構面：rust-analyzer SCIP index
[Next EP] Python 型別面：pyrefly-lsp bridge（已裁決 B——橋官方
         `pyrefly lsp` 子命令，薄 LSP↔MCP 轉譯層）
[同模式複製] Rust 型別面：rust-analyzer-lsp bridge
[鮮度軸] stale WARN 已有；pyrefly 全量 78s 已近「隨手重跑」量級；
         真增量列後續（發生才做）
[分發軸] plugin 安裝（marketplace 0.1.1+）＋usage skill＝本矩陣的
         精簡面
```

**關鍵架構洞見**：LSP↔MCP bridge 是**語言無關基建**——
`pyrefly lsp` 與 `rust-analyzer` 都說 LSP；**一個 bridge crate、
兩個 backend 參數**（server 啟動命令）。Rust 型別面的邊際成本
因此塌縮成「換一個 spawn 命令＋測一輪」。

## 四、三主線檢視

- **① 依賴規則**：bridge 是新 adapter（對外依賴 LSP client 協議，
  對內暴露 MCP tools）；引擎（pyrefly lib）與 bridge（pyrefly
  lsp 進程）並存不耦合——生產者與橋共用上游、各自生命週期 ✓
- **② bounded context**：**結構面與型別面是兩個 context**——
  batch／冪等／graph 為真相源 vs 互動／增量／工作區狀態。已裁決
  「不把 resident State 塞進 code-reality-mcp（無狀態設計）」
  正是此邊界；bridge 獨立進程＝context 邊界的物理化
- **③ use case 驅動**：消費者＝經 plugin 掛載 MCP 的 AI session。
  完全取代的驗收不是「功能清單打勾」而是**U1-U10 全綠＋鮮度
  可接受**——U9 的 EP 驗收三條（hover 對照 pyright／diagnostics
  .py 過濾／串流）已在 mosaic relay 凍結

## 五、第二層後果（誠實追蹤）

1. **共存先於取代**：CC 內建 LSP 不必關——MCP 與內建共存無害，
   取代由用戶端配置自然發生（誰好用用誰）；CR 的籌碼是 U3/U4/
   U7/U8 的獨有面
2. **新鮮度是取代的真正摩擦**：結構面 index-based（stale 需
   重生）vs LSP live——78s 全量＋stale WARN 已把摩擦壓到可接受；
   跨過 U9 後若仍有摩擦，屆時做 pyrefly 增量（watch 模式），
   現在不做（YAGNI）
3. **統一＋超越才是價值主張**：取代 LSP 是手段；「一個 plugin
   給全部語言、同時給 LSP 给不了的 graph 智慧」是定位
