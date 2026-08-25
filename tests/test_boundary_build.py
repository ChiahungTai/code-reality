"""boundary_build 單元測試——合成 mini-case（NT 實形狀的縮影）。

素材：poc/test_pyo3_boundary_poc.py 提煉擴充（EP Dogfood——POC→RED 銜接）
＋ 4 事故 regression（SM-8，對照 gap-prototypes 報告 §1.6 原文）：
rindex（repo 目錄與 package 同名）/ doc-skip（doc comment 後 #[new] 誤判）/
last-wins（name= 與 signature= 並列蓋掉 rename）/ tuple-struct（body 掃描跑飛）。
"""

import sqlite3
from pathlib import Path

import pytest
from profile_repo import write_nt_profile

from code_reality.boundary_build import (
    _crate_of,
    build_boundary,
    build_sidecar,
    parse_pyi,
    pyi_module,
    scan_rust_file,
    write_sidecar,
)

RUST = r"""//! demo crate
#[derive(Debug)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.live", unsendable)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.live")
)]
pub struct LiveNode {
    inner: u8,
}

#[pyo3_stub_gen::derive::gen_stub_pymethods]
#[pymethods]
impl LiveNode {
    #[staticmethod]
    #[pyo3(name = "build")]
    #[pyo3(signature = "(name, config=None)")]
    fn py_build(name: String) -> PyResult<Self> {
        todo!()
    }
    #[getter]
    #[pyo3(name = "trader_id")]
    fn py_trader_id(&self) -> u8 {
        self.inner
    }
    #[getter]
    fn get_level(&self) -> u8 {
        self.inner
    }
    #[setter]
    fn set_level(&mut self, v: u8) {
        self.inner = v;
    }
    /// doc comment 在 #[new] 前——事故 2 場景（曾把 #[new] 吃掉誤判 method）
    #[new]
    fn py_new() -> Self {
        todo!()
    }
    fn plain_method(&self) -> u8 {
        self.inner
    }
    /// char-literal 事故（build review F1）：format! 字串含 '{}'
    /// 曾讓 impl body 提早關閉、後續方法漏掃（NT corpus 實測 269 methods）
    fn py_describe(&self) -> String {
        format!("<{}.{}: '{}'>", 1, 2, 3)
    }
    fn after_format(&self) -> u8 {
        self.inner
    }
    fn __str__(&self) -> String {
        todo!()
    }
}

#[pyclass(name = "LiveNodeBuilder", module = "nautilus_trader.live")]
pub struct LiveNodeBuilderPy {
    inner: u8,
}

#[pymethods]
impl LiveNodeBuilderPy {
    #[pyo3(name = "with_name")]
    fn py_with_name(&self) -> Self {
        todo!()
    }
}

/// tuple struct——事故 4 場景：無 body，lookahead 不得吃掉後續 item
#[pyclass(name = "TickPyAlias", module = "nautilus_trader.live")]
pub struct TickPy(pub u8);

#[pyclass(module = "nautilus_trader.live")]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.live")]
pub enum AxEnvironment {
    #[pyo3(name = "SANDBOX2")]
    Sandbox,
    Prod,
    UsdM,
    IsolatedMargin,
}

/// credential Known Gap 縮影：secret_key 欄位 pyi 無對應 property（選擇性省略）
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.live", get_all)
)]
pub struct ConfigPy {
    pub base_url_http: String,
    pub secret_key: String,
}

/// #[new] 雙形態：pyi 呈 __init__（config 類形態——U-4）
#[pyclass(module = "nautilus_trader.live")]
pub struct SizerPy {
    inner: u8,
}

#[pymethods]
impl SizerPy {
    #[new]
    fn py_new() -> Self {
        todo!()
    }
}

#[pyo3_stub_gen::derive::gen_stub_pyfunction(module = "nautilus_trader.trading")]
#[pyfunction]
fn fx_next_start(x: u8) -> u8 {
    x
}

/// pyfunction 無 rename 的 py_ 剝前綴（U-3：method/pyfunction 兩分支）
#[pyo3_stub_gen::derive::gen_stub_pyfunction(module = "nautilus_trader.trading")]
#[pyfunction]
fn py_calibrate(x: u8) -> u8 {
    x
}
"""

# 跨 crate 同名 pyclass（build review R3：CustomData 縮影——crate-join 素材）
COMMON_RUST = r"""#[pyclass(module = "nautilus_trader.common")]
pub struct ConfigPy {
    pub endpoint: String,
}

#[pymethods]
impl ConfigPy {
    #[getter]
    fn get_endpoint(&self) -> String {
        todo!()
    }
}
"""

PYI = """import enum
import typing

@typing.final
class LiveNode:
    @staticmethod
    def build(name: str) -> LiveNode: ...
    @property
    def trader_id(self) -> int: ...
    @property
    def level(self) -> int: ...
    def __new__(cls) -> LiveNode: ...
    def plain_method(self) -> int: ...
    def describe(self) -> str: ...
    def after_format(self) -> int: ...

@typing.final
class LiveNodeBuilder:
    def with_name(self) -> LiveNodeBuilder: ...

@typing.final
class TickPyAlias: ...

class AxEnvironment(enum.Enum):
    SANDBOX2 = 1
    PROD = 2
    USD_M = 3
    ISOLATED_MARGIN = 4

@typing.final
class ConfigPy:
    @property
    def base_url_http(self) -> str: ...

class SizerPy:
    def __init__(cls) -> SizerPy: ...
"""

COMMON_PYI = """class ConfigPy:
    @property
    def endpoint(self) -> str: ...
"""


def _run(repo: Path):
    """合成 mini repo（NT 形狀縮影，雙 crate）→ 掃描＋對帳。"""
    rs = repo / "crates/live/src/node/mod.rs"
    rs.parent.mkdir(parents=True)
    rs.write_text(RUST)
    rs_common = repo / "crates/common/src/custom.rs"  # 跨 crate 同名 ConfigPy
    rs_common.parent.mkdir(parents=True)
    rs_common.write_text(COMMON_RUST)
    pyi_live = repo / "python/nautilus_trader/live/__init__.pyi"
    pyi_live.parent.mkdir(parents=True)
    pyi_live.write_text(PYI)
    pyi_common = repo / "python/nautilus_trader/common/__init__.pyi"
    pyi_common.parent.mkdir(parents=True)
    pyi_common.write_text(COMMON_PYI)
    pyi_trading = repo / "python/nautilus_trader/trading/__init__.pyi"
    pyi_trading.parent.mkdir(parents=True)
    pyi_trading.write_text(
        "def fx_next_start(x: int) -> int: ...\ndef calibrate(x: int) -> int: ...\n"
    )

    classes, methods, functions = [], [], []
    for p in [rs, rs_common]:
        c, m, f = scan_rust_file(p, repo)
        classes += c
        methods += m
        functions += f
    py_classes, py_functions = [], []
    for p in [pyi_live, pyi_common, pyi_trading]:
        pcs, pfs = parse_pyi(p, repo)
        mod = pyi_module(str(p))
        py_classes += [(mod, c) for c in pcs]
        py_functions += [(mod, f) for f in pfs]
    edges, cov = build_boundary(classes, methods, functions, py_classes, py_functions)
    return classes, methods, functions, edges, cov


# ---------------------------------------------------------------------------
# POC 3 tests 提煉擴充
# ---------------------------------------------------------------------------


def test_class_extraction(tmp_path: Path):
    classes, _, _, _, _ = _run(tmp_path)
    names = [c.rust_name for c in classes]
    assert sorted(names) == sorted(
        [
            "LiveNode",
            "LiveNodeBuilderPy",
            "TickPy",
            "AxEnvironment",
            "ConfigPy",
            "ConfigPy",  # 跨 crate 同名（R3 素材——live＋common 各一）
            "SizerPy",
        ]
    )
    by_key = {(_crate_of(c.rs_path), c.rust_name): c for c in classes}
    ln = by_key[("live", "LiveNode")]
    assert ln.py_module == "nautilus_trader.live"  # cfg_attr 巢狀 module= 解析
    assert ln.exposed == "LiveNode"
    b = by_key[("live", "LiveNodeBuilderPy")]
    assert b.exposed == "LiveNodeBuilder"  # pyclass(name=) rename
    assert b.py_module == "nautilus_trader.live"
    assert by_key[("live", "TickPy")].exposed == "TickPyAlias"  # tuple struct rename
    assert by_key[("live", "ConfigPy")].py_module == "nautilus_trader.live"
    assert by_key[("common", "ConfigPy")].py_module == "nautilus_trader.common"
    assert by_key[("live", "SizerPy")].py_module == "nautilus_trader.live"


def test_method_rename_and_getter(tmp_path: Path):
    _, methods, _, _, _ = _run(tmp_path)
    by_exposed: dict[str, list] = {}
    for m in methods:
        if m.rust_class == "LiveNode":
            by_exposed.setdefault(m.exposed, []).append(m)
    assert by_exposed["build"][0].kind == "staticmethod"  # py_build → build
    assert by_exposed["build"][0].renamed  # 真 #[pyo3(name=)]
    assert by_exposed["trader_id"][0].kind == "getter"  # py_trader_id → trader_id
    levels = by_exposed["level"]
    assert {m.kind for m in levels} == {"getter", "setter"}  # get_/set_ 皆剝前綴（R2）
    assert all(not m.renamed for m in levels)
    assert by_exposed["__new__"][0].kind == "new"  # #[new] → __new__
    assert by_exposed["plain_method"][0].kind == "method"
    describe = by_exposed["describe"][
        0
    ]  # py_describe → describe（py_ 剝前綴、無 rename）
    assert describe.kind == "method"
    assert not describe.renamed  # 非 rename——是 stub-gen 自動剝前綴（U-3）
    assert by_exposed["after_format"][0].kind == "method"
    builder = [m for m in methods if m.rust_class == "LiveNodeBuilderPy"]
    assert builder and builder[0].exposed == "with_name"
    variants = {m.exposed: m for m in methods if m.rust_class == "AxEnvironment"}
    assert variants["SANDBOX2"].renamed  # variant 級 #[pyo3(name)] 優先
    assert variants["PROD"].kind == "variant"  # 單 hump → UPPER
    assert "USD_M" in variants and "ISOLATED_MARGIN" in variants  # R1 screaming_snake


def test_edges_and_coverage(tmp_path: Path):
    _, _, functions, edges, cov = _run(tmp_path)
    py_symbols = {e["py_symbol"] for e in edges}
    assert "nautilus_trader.live.LiveNode" in py_symbols
    assert "nautilus_trader.live.LiveNodeBuilder" in py_symbols  # rename 後符號
    assert "nautilus_trader.live.TickPyAlias" in py_symbols  # tuple struct class 邊
    assert "nautilus_trader.live.AxEnvironment.SANDBOX2" in py_symbols
    assert "nautilus_trader.live.AxEnvironment.PROD" in py_symbols
    assert "nautilus_trader.live.AxEnvironment.USD_M" in py_symbols  # R1
    assert "nautilus_trader.live.AxEnvironment.ISOLATED_MARGIN" in py_symbols  # R1
    assert "nautilus_trader.live.ConfigPy.base_url_http" in py_symbols
    assert "nautilus_trader.common.ConfigPy.endpoint" in py_symbols  # R3 crate-join
    assert (
        "nautilus_trader.live.SizerPy.__init__" in py_symbols
    )  # #[new] __init__ 形態（U-4）
    assert "nautilus_trader.live.LiveNode.build" in py_symbols
    assert "nautilus_trader.live.LiveNode.trader_id" in py_symbols
    assert "nautilus_trader.live.LiveNode.level" in py_symbols
    assert "nautilus_trader.live.LiveNode.__new__" in py_symbols
    assert "nautilus_trader.live.LiveNode.describe" in py_symbols
    assert "nautilus_trader.live.LiveNode.after_format" in py_symbols
    assert "nautilus_trader.live.LiveNodeBuilder.with_name" in py_symbols
    assert "nautilus_trader.trading.fx_next_start" in py_symbols
    assert (
        "nautilus_trader.trading.calibrate" in py_symbols
    )  # pyfunction py_ 剝前綴（U-3）
    kinds = {e["py_symbol"]: e["match_kind"] for e in edges}
    assert kinds["nautilus_trader.live.LiveNode.build"] == "PYO3_NAME_RENAME"
    assert kinds["nautilus_trader.live.LiveNodeBuilder"] == "PYCLASS_NAME_RENAME"
    assert kinds["nautilus_trader.live.LiveNode.level"] == "GETTER_PROPERTY"
    assert kinds["nautilus_trader.live.LiveNode.trader_id"] == "PYO3_NAME_RENAME"
    assert kinds["nautilus_trader.live.LiveNode.plain_method"] == "NAME_MATCH"
    assert kinds["nautilus_trader.live.LiveNode.describe"] == "NAME_MATCH"
    assert kinds["nautilus_trader.live.SizerPy.__init__"] == "NAME_MATCH"
    assert kinds["nautilus_trader.common.ConfigPy.endpoint"] == "GETTER_PROPERTY"
    # function 分支的 match_kind 由「名稱不等」判定（POC 原樣——py_ 剝前綴
    # 後 exposed != rust_fn，標 PYO3_NAME_RENAME；method 分支才用 renamed flag）
    assert kinds["nautilus_trader.trading.calibrate"] == "PYO3_NAME_RENAME"
    assert kinds["nautilus_trader.live.AxEnvironment.SANDBOX2"] == "PYO3_NAME_RENAME"
    assert kinds["nautilus_trader.live.AxEnvironment.PROD"] == "ENUM_VARIANT"
    assert kinds["nautilus_trader.live.AxEnvironment.USD_M"] == "ENUM_VARIANT"
    assert kinds["nautilus_trader.live.ConfigPy.base_url_http"] == "FIELD_PROPERTY"
    assert cov["classes"]["matched"] == 7  # 含跨 crate 兩個 ConfigPy
    assert cov["classes"]["pyi_only"] == 0
    assert cov["classes"]["rs_only"] == 0
    # LiveNode 8（含 setter；__str__ dunder 另計）+ Builder 1 + variants 4 +
    # live ConfigPy 1 + common ConfigPy 1 + SizerPy 1 = 16；
    # secret_key（credential 縮影）rs-only
    assert cov["methods"]["matched"] == 16
    assert cov["methods"]["rs_only"] == 1
    assert cov["methods"]["rs_only_by_kind"] == {"field_property": 1}
    assert cov["methods"]["dunder_rs_only"] == 1  # __str__ 另計
    assert cov["methods"]["unresolved_class"] == 0
    assert functions[0].py_module == "nautilus_trader.trading"
    assert cov["functions"]["matched"] == 2
    assert len(edges) == 25  # class 7 + method 16 + function 2


# ---------------------------------------------------------------------------
# SM-8：4 事故 regression（原型期真 bug 的反例釘住——報告 §1.6）
# ---------------------------------------------------------------------------


def test_regression_rindex_module_path(tmp_path: Path):
    """事故 1：repo 目錄名與 package 同名（nautilus_trader/nautilus_trader），
    絕對路徑 parts.index() 命中 repo 目錄 → 全體 module 路徑錯（0 匹配假象）。"""
    repo = tmp_path / "nautilus_trader"  # repo 目錄＝package 名
    pyi_init = repo / "python/nautilus_trader/live/__init__.pyi"
    assert pyi_module(str(pyi_init)) == "nautilus_trader.live"
    pyi_mod_file = repo / "python/nautilus_trader/live/node.pyi"
    assert pyi_module(str(pyi_mod_file)) == "nautilus_trader.live.node"


def test_regression_doc_skip_new(tmp_path: Path):
    """事故 2：doc comment 後的 #[new] 被推進邏輯吃掉 → 誤判 method（原型期
    406 個 #[new] 誤判）。doc 在 attrs **之前**的形狀最易踩。"""
    _, methods, _, _, _ = _run(tmp_path)
    ln_new = [m for m in methods if m.kind == "new" and m.rust_class == "LiveNode"]
    assert len(ln_new) == 1
    assert ln_new[0].exposed == "__new__"
    # 誤判形態：py_new 以 method kind 落進 methods（暴露名會是 py_new 而非 __new__）
    assert not any(
        m.kind == "method" and m.rust_fn == "py_new" and m.rust_class == "LiveNode"
        for m in methods
    )


def test_regression_last_wins_rename(tmp_path: Path):
    """事故 3：#[pyo3(name=...)] 與 #[pyo3(signature=...)] 並列時，後到的
    signature attr 把 rename 清成 None（build 邊 match_kind 誤標）。"""
    _, _, _, edges, _ = _run(tmp_path)
    build_edges = [e for e in edges if e["py_symbol"].endswith("LiveNode.build")]
    assert len(build_edges) == 1
    assert build_edges[0]["match_kind"] == "PYO3_NAME_RENAME"
    assert build_edges[0]["rs_symbol"] == "LiveNode::py_build"


def test_regression_tuple_struct(tmp_path: Path):
    """事故 4：tuple struct（struct X(...) 無 body）body 掃描跑飛——lookahead
    吃掉後續 item 的 body（枚舉整個消失）。"""
    classes, methods, _, edges, _ = _run(tmp_path)
    # tuple struct 自身：class 邊在、無偽 field_property
    assert {c.rust_name for c in classes} >= {"TickPy", "AxEnvironment"}
    assert not [m for m in methods if m.rust_class == "TickPy"], (
        "tuple struct 不得合成 field_property"
    )
    # 後續 item（enum）未被 lookahead 吃掉：class 邊＋兩 variant 邊都在
    py_symbols = {e["py_symbol"] for e in edges}
    assert "nautilus_trader.live.AxEnvironment" in py_symbols
    assert "nautilus_trader.live.AxEnvironment.PROD" in py_symbols


def test_regression_char_literal_in_format_string(tmp_path: Path):
    """F1（build review 🔴，POC 忠實繼承）：impl body 內 ``format!("<... '{}'>")``
    的 char-literal regex 曾 malformed（match `'{` 兩字元）→ 孤兒 ``}`` →
    brace depth -1 → body 提早關閉 → format 字串行之後的方法全部漏掃
    （NT corpus 實測 269 methods 靜默缺邊、AxEnvironment 零 method 邊）。"""
    _, methods, _, edges, _ = _run(tmp_path)
    exposed = {m.exposed for m in methods if m.rust_class == "LiveNode"}
    assert "describe" in exposed  # format 字串行所在方法自身
    assert "after_format" in exposed, "format! 字串行之後的方法不得漏掃"
    py_symbols = {e["py_symbol"] for e in edges}
    assert "nautilus_trader.live.LiveNode.after_format" in py_symbols
    assert "nautilus_trader.live.LiveNode.describe" in py_symbols
    # pyi 缺席的 dunder 不建邊、另計（見 coverage dunder_rs_only）——真正的
    # 鑑別點是上兩行 after_format/describe 的存在；此行僅防禦性釘住
    assert "nautilus_trader.live.LiveNode.__str__" not in py_symbols


def test_regression_variant_screaming_snake(tmp_path: Path):
    """R1（post-build review）：variant 曝露名 naive ``.upper()`` 產
    ``USDM``/``ISOLATEDMARGIN`` 永不匹配 pyi 的 ``USD_M``/
    ``ISOLATED_MARGIN``——NT corpus 實測 261 條 ENUM_VARIANT 邊系統性
    缺失。CamelCase→SCREAMING_SNAKE_CASE 修復。"""
    _, methods, _, edges, _ = _run(tmp_path)
    variants = {m.exposed for m in methods if m.rust_class == "AxEnvironment"}
    assert "USD_M" in variants, "UsdM → USD_M（非 USDM）"
    assert "ISOLATED_MARGIN" in variants, "IsolatedMargin → ISOLATED_MARGIN"
    assert "USDM" not in variants and "ISOLATEDMARGIN" not in variants
    py_symbols = {e["py_symbol"] for e in edges}
    assert "nautilus_trader.live.AxEnvironment.USD_M" in py_symbols


def test_regression_cross_crate_same_name(tmp_path: Path):
    """R3（post-build review）：``class_by_rust`` 裸名 last-wins——跨 crate
    同名 ``CustomData``（common＋model）下 common 的 methods 全錯掛 model
    （sidecar 實證 common 側零 method 邊）。crate-qualified join 修復。"""
    _, _, _, edges, cov = _run(tmp_path)
    py_symbols = {e["py_symbol"] for e in edges}
    # 兩個 ConfigPy（live＋common crate）各自的 class 邊都在
    assert "nautilus_trader.live.ConfigPy" in py_symbols
    assert "nautilus_trader.common.ConfigPy" in py_symbols
    # common crate 的 getter 歸 common module（裸名 last-wins 會錯掛 live）
    assert "nautilus_trader.common.ConfigPy.endpoint" in py_symbols
    rs_paths = {
        e["rs_path"] for e in edges if e["py_symbol"].endswith("ConfigPy.endpoint")
    }
    assert rs_paths == {"crates/common/src/custom.rs"}
    assert cov["methods"]["unresolved_class"] == 0


# ---------------------------------------------------------------------------
# sidecar（SM-5/SM-6）
# ---------------------------------------------------------------------------


def test_write_sidecar_schema_and_idempotent(tmp_path: Path):
    _, _, _, edges, cov = _run(tmp_path / "nt")
    out = tmp_path / "sidecar"
    sha = "a" * 40
    p1 = write_sidecar(tmp_path / "nt", sha, edges, cov, out_dir=out)
    assert p1.name == "aaaaaaaa.db"  # 檔名錨 NT short sha
    conn = sqlite3.connect(p1)
    cols = [r[1] for r in conn.execute("PRAGMA table_info(boundary_edges)")]
    assert cols == [
        "id",
        "level",
        "py_symbol",
        "pyi_path",
        "pyi_line",
        "rs_symbol",
        "rs_path",
        "rs_line",
        "match_kind",
        "method_kind",
    ]
    assert conn.execute("SELECT COUNT(*) FROM boundary_edges").fetchone()[0] == len(
        edges
    )
    meta = dict(conn.execute("SELECT key, value FROM meta"))
    conn.close()
    assert meta["nt_commit"] == sha
    assert meta["repo"] == "nt"
    assert meta["tool"] == "code_reality.boundary_build"
    assert int(meta["edges_count"]) == len(edges)
    # 冪等（SM-6）：同 sha 重跑覆寫同名檔、不重複
    p2 = write_sidecar(tmp_path / "nt", sha, edges, cov, out_dir=out)
    assert p2 == p1
    conn = sqlite3.connect(p2)
    assert conn.execute("SELECT COUNT(*) FROM boundary_edges").fetchone()[0] == len(
        edges
    )
    conn.close()


def test_build_sidecar_missing_repo_crash(tmp_path: Path):
    """SM-1b：無 profile 掃描根 → crash-only 附指引（不內建 repo 預設）。"""
    with pytest.raises(AssertionError, match="scan_root"):
        build_sidecar(tmp_path / "nope")


def test_build_sidecar_profile_missing_dirs_crash(tmp_path: Path):
    """有 profile 但 scan_root base 不存在 → crash-only（glob 拼字錯誤防線）。"""
    write_nt_profile(tmp_path)
    with pytest.raises(AssertionError, match="scan_root base 不存在"):
        build_sidecar(tmp_path)
