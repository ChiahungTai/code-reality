# RED receipts — DB-100 EP（card CR-3）

baseline：`8c01663`（cr-3 branch；EP baseline 25fd932＋開工 commits）

## S1 — tests/rust_projects.rs

- TC 映射：TC-8（CR-RUST-ROOT-01..08 mapping table）、TC-11（validate_authoritative 兩面權威）、TC-5d（排序冪等 resolver 面）、SM-2b（zero-roots Ok 空集契約）、review F10（profile 合法鍵訊息 drift）
- test 檔 sha256：`1031cdfea96940e205aef2b1445ab71dc87639338ba67c754a123ee6c02e82b3`
- baseline 身份：cr-3@8c01663（測試在場、src/rust_projects.rs 不存在）
- RED 證據：`cargo test -p code-reality --test rust_projects` → E0432/E0433（`code_reality::rust_projects` 不存在）×5 errors、build failed——pre-change 紅 ✓
- 三栓：STRUCT_OK（26 test fn、無 skip/xfail）／SEM_OK（逐 TC predicate 對照 EP TC-8 映射表）／BEFORE_GREEN（compile fail＝紅）
- failing predicates（GREEN 後應全綠）：01×4、02×2、03×2、04×2、05×1、06×2、07×1、auto/config happy×6、zero-roots×1、order×1、profile drift×3、authoritative×2

## S1 review round-1 修正（段級 review needs-changes → 全採 apply）

- 修正：auto "." 分支補 sibling `[workspace]` 掃描→06（F1，拆測試）；member fixture 補 src/lib.rs＋斷言 `workspace_root=`（F2——mismatch 分支真覆蓋）；profile 訊息補 hazard_registry（F3）；新測試 config ["."]＋[package] members 不觸 07（F4）；repo canonicalize 失敗補 02 碼（F5①）
- 修正後 digest：`4a82fe09513c380f12b0eaf5f37e0489da703701e1b12d55cd9e8326ad53b838`
- 修正後：`cargo test -p code-reality --test rust_projects` → 27 passed / 0 failed

## S2 — tests/rust_identity_v2.rs

- TC 映射：TC-4（Cargo-only stale）、TC-5a/c（v2 同構＋algo pin）、TC-5d（config 效果面不退化）、TC-6a-c（v1 rust slot upgrade→rebuild restamp）、TC-6d（keyless）、TC-9（py-only v1 一次性退化）、TC-10（Cargo-present rs-absent 隔離）
- test 檔 sha256：`b74113a90a56c6a0ad846110a4f0d788e0086ff1a79acc1886aaa4063622bf5c`
- baseline 身份：cr-3@8c01663＋S1 working tree（resolver 在場、identity v1）
- RED 證據：`cargo test -p code-reality --test rust_identity_v2` → 5 failed（tc4/5/6/6d/9——目標行為缺席：meta identity_algo Null、無 upgrade 語義、Cargo-only 不 stale）＋2 passed（tc5d/tc10 回歸釘，v1 下本成立）
- 三栓：STRUCT_OK（7 test fn）／SEM_OK（逐 TC predicate 對照 EP）／BEFORE_GREEN（行為紅非環境紅——fixture producer 已建、fake RA 在場）
- fixture 註記：fake rust-analyzer cp rich.scip（identity 測試不驗 doc 內容——path 形態歸 S3 normalize 測試）

## S3 — tests/multi_rust_build.rs

- TC 映射：TC-1（normalize 三 predicate）、TC-2a/b（合流＝Σ＋契約 rg 斷言）、TC-3/3b（跨 root collision fail＋slot 不存在；within-root 放行）、coverage gate 08（Q7 multi-root fatal）
- test 檔 sha256（RED 態）：`635b11683d23bd43d5453d23819511379f2824f336a4ce5bb0e72325d0601fd2`
- baseline 身份：cr-3@8c01663＋S1/S2 working tree
- RED 證據：5/5 failed——fake RA 收到 repo 根（單 leg 呼叫形態，composition 未接）→case 不匹配 exit 1； GREEN 後 fixture amendment 兩筆（marshal 追認，見 EP S3 進度③）
