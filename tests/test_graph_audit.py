"""graph_audit 單元測試——NT N1 兩教訓 regression＋掃描集泛化。

教訓一（D1 準則）：per-block 計數非全體交集——kernel.rs 三 impl 形態
（inherent＋Drop＋trait）在交集準則下漏報。教訓二（Test kind）：DB 計數
漏 Test＝全部測試函數誤報。掃描集：profile scan_root 優先、無則
rglob＋exclusions fallback。
"""

import sqlite3
from collections import Counter
from pathlib import Path

from crg_db import make_crg_db, qualified

from code_reality.graph_audit import (
    FN_RE,
    IMPL_RE,
    audit,
    db_functions,
    parse_ra_symbols,
    risk_scan,
    scan_files,
)

KERNEL_THREE_IMPLS = """\
impl EventStoreLifecycle {
    fn open(&self) -> bool { true }
    fn close(&mut self) {}
}
impl Drop for EventStoreLifecycle {
    fn drop(&mut self) {}
}
impl SomeBehavior for EventStoreLifecycle {
    fn open(&self) -> bool { false }
}
"""


def test_risk_scan_per_block_not_intersection(tmp_path: Path) -> None:
    """教訓一：三 impl 形態下全體交集＝{open,close}∩{drop}∩{open}＝∅（NT
    首版 bug）；per-block 計數抓到 inherent＋trait 各一個 open。"""
    f = tmp_path / "kernel.rs"
    f.write_text(KERNEL_THREE_IMPLS, encoding="utf-8")
    assert risk_scan([f]) == [(f, "EventStoreLifecycle", ["open"])]


def test_risk_scan_clean_when_no_shared_name(tmp_path: Path) -> None:
    a = tmp_path / "a.rs"
    a.write_text(
        "impl Foo {\n    fn a(&self) {}\n}\nimpl Bar for Foo {\n    fn b(&self) {}\n}\n",
        encoding="utf-8",
    )
    assert risk_scan([a]) == []


def test_risk_scan_terminator_ends_block(tmp_path: Path) -> None:
    """col-0 ``}`` 終止 impl 塊——之後的游離 fn 不誤歸屬前一 impl。"""
    f = tmp_path / "t.rs"
    f.write_text(
        "impl Foo {\n    fn a(&self) {}\n}\nfn stray() {}\n"
        "impl Bar for Qux {\n    fn b(&self) {}\n}\n",
        encoding="utf-8",
    )
    assert risk_scan([f]) == []


def test_risk_scan_sees_indented_impls(tmp_path: Path) -> None:
    """NT 最終版：縮排 impl（inline mod 內）可見——收編首版的 col-0 blind
    spot（審查 F4）由此關閉。"""
    f = tmp_path / "t.rs"
    f.write_text(
        "#[cfg(test)]\nmod tests {\n    use super::*;\n"
        "    impl Foo {\n        fn open(&self) {}\n    }\n}\n"
        "impl Foo {\n    fn open(&self) {}\n}\n",
        encoding="utf-8",
    )
    assert risk_scan([f]) == [(f, "Foo", ["open"])]


def test_impl_fn_re_variants() -> None:
    """NT 三輪審查迭代形態：unsafe／泛型巢狀／路徑限定 trait／dyn／縮排；
    fn 前綴 const/async/unsafe/extern。"""
    assert IMPL_RE.match("impl Foo").group(1) == "Foo"
    assert IMPL_RE.match("unsafe impl Foo").group(1) == "Foo"
    assert IMPL_RE.match("impl<T: Clone> Foo<T>").group(1) == "Foo"
    assert IMPL_RE.match("impl fmt::Display for Foo").group(1) == "Foo"
    assert IMPL_RE.match("impl SomeTrait for dyn Foo").group(1) == "dyn Foo"
    assert IMPL_RE.match("    impl Foo {").group(1) == "Foo"
    assert FN_RE.match('pub const unsafe extern "C" fn foo()').group(1) == "foo"
    assert FN_RE.match("    async fn bar()").group(1) == "bar"


def test_audit_missing_via_injected_lookup(tmp_path: Path) -> None:
    """audit() 組合邏輯（審查 F2）：D1 風險檔進 scope＋db<ra → missing
    組裝——``ra_lookup`` 替身免 rust-analyzer。"""
    repo = tmp_path / "repo"
    rs = repo / "crates" / "kernel.rs"
    rs.parent.mkdir(parents=True)
    rs.write_text(KERNEL_THREE_IMPLS, encoding="utf-8")
    db = tmp_path / "graph.db"
    q_open = qualified(repo, "crates/kernel.rs", "open")
    make_crg_db(
        db,
        nodes=[("open", None, q_open, str(rs))],
        node_attrs={q_open: ("Function", "rust", 0, None)},
    )

    def fake_lookup(p: Path) -> Counter:
        return Counter({"open": 2})  # RA 見 inherent＋trait 兩個 open；DB 僅 1

    _, audited, missing, errors, total_ra = audit(repo, db, ra_lookup=fake_lookup)
    assert audited == 1  # 僅 D1 風險檔進預設 scope
    assert missing == [
        {"file": str(rs), "symbol": "open", "ra_count": 2, "db_count": 1}
    ]
    assert errors == []
    assert total_ra == 2


def test_parse_ra_symbols_fn_kinds_only() -> None:
    out = (
        '  label: "open" kind: SymbolKind(Function) navigation_range: 123..129\n'
        '  label: "Foo" kind: SymbolKind(Struct) navigation_range: 1..4\n'
        '  label: "drop" kind: SymbolKind(Method) navigation_range: 200..205\n'
    )
    assert parse_ra_symbols(out) == Counter({"open": 1, "drop": 1})


def test_db_functions_includes_test_kind(tmp_path: Path) -> None:
    """教訓二：Test 節點計入——漏計會把 test_* 全部誤報缺差（NT 首跑
    1,670 假警報）。"""
    repo = tmp_path / "repo"
    rs = repo / "crates" / "x.rs"
    rs.parent.mkdir(parents=True)
    rs.write_text("fn run() {}\n", encoding="utf-8")
    db = tmp_path / "graph.db"
    q_run = qualified(repo, "crates/x.rs", "run")
    q_test = qualified(repo, "crates/x.rs", "test_run")
    make_crg_db(
        db,
        nodes=[
            ("run", None, q_run, str(rs)),
            ("test_run", None, q_test, str(rs)),
        ],
        node_attrs={
            q_run: ("Function", "rust", 0, None),
            q_test: ("Test", "rust", 1, None),
        },
    )
    conn = sqlite3.connect(db)
    try:
        assert db_functions(conn, rs) == Counter({"run": 1, "test_run": 1})
    finally:
        conn.close()


def test_scan_files_fallback_excludes_venv(tmp_path: Path) -> None:
    """無 profile → generic fallback：全 *.rs 經 exclusions（.venv/ 排除）。"""
    repo = tmp_path / "repo"
    for rel in ("crates/a.rs", ".venv/lib/b.rs"):
        p = repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("fn x() {}\n", encoding="utf-8")
    assert [f.name for f in scan_files(repo)] == ["a.rs"]


def test_scan_files_profile_scan_root_wins(tmp_path: Path) -> None:
    """有 [[scan_root]] → path glob 為掃描集（Rust 形態 repo）；glob 外
    的 .rs 不入。"""
    repo = tmp_path / "repo"
    for rel in ("crates/live/src/a.rs", "examples/b.rs"):
        p = repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("fn x() {}\n", encoding="utf-8")
    (repo / ".code-reality.toml").write_text(
        '[[scan_root]]\npath = "crates/**/*.rs"\npyi = "python/**/*.pyi"\n',
        encoding="utf-8",
    )
    assert [f.name for f in scan_files(repo)] == ["a.rs"]
