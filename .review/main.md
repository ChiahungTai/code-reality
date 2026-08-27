# .review/main.md — post-build dual-context findings（ep-pyrefly-native-producer）

模式：uncommitted diff（mode=uncommitted；無 transition 機械底稿——
build 起點 snapshot 因 dogfood graph.db 缺失降級，LLM 對照替代）。

- fresh 審查：F-1..F-8（F-1 🔴 runtime 重現）
- primed 審查：R-1..R-7（EP 對照 14/14 機械查證全落地＋S1 驗證鏈獨立重現）
- judge 裁決＋修正：見 EP Post-Build Findings 段（PB-1..PB-8）
- followup 驗證：cargo test 全綠（pyrefly-producer 3+5＋workspace）＋
  最終鏈刷新（mosaic slot 產物含行號修正）

open（無）：🔴/🟡 全數處置（6 implemented＋2 adjudicated）。
