# CR-5 bisect 證據（marshal 機械 bisect，fixture /tmp/cr5-fix：mini cargo lib crate）

| 條件 | stamped? | 判讀 |
|---|---|---|
| A bare crate（src/lib.rs 唯一 face 檔） | True | 基線健康 |
| B + tests/fixtures/orphan.rs（非 cargo target 的散落 .rs） | **False** | **重現本 bug——孤兒 .rs 擊穿 docs == a.docs** |
| C + tests/fixtures/orphan.py | True | python 腿吃 collect_py_corpus 明確列表，孤兒 .py 兩側一致 |
| D + examples/demo.rs（真 cargo target） | True | ra 索引 targets，examples 不破 |

因果：walk_sources 收所有 repo 內 .rs（僅跳 target/），rust-analyzer scip 只 emit cargo
targets → 有孤兒 .rs 的 repo 生產集 ⊂ 走查集 → `docs == a.docs`（engine.rs:1405）恆假 →
identity 三元組恆不落 → freshness 恆 identity-algo-upgrade stale。delegate-bridge 倖免＝
該 repo 無孤兒 .rs。CR repo 本體有 tests/fixtures/*.rs 等多顆孤兒 .rs。
