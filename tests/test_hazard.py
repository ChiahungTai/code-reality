"""hazard 判定層測試——合成 mini-case（假 StrEnum 源碼、假 rg 行）。

只測純判定函數（parse_symbol_facts / detect_* / resident/full_findings /
hazard_gate_warning）；orchestration（nodes 表解析、hub_refs 觸發）在
test_hub_refs.py 覆蓋。收編自 mosaic P2 原型 28 tests
（``.agent-tmp/research/p2/test_hazard_scan.py``，四 case 對帳報告見同
目錄 REPORT.md）——收編差異：registry 表改 profile 注入、classify 帶
profile、新增 resident/method_name/symbol_facts 層。
"""

from pathlib import Path

import pytest
from crg_db import make_crg_db
from profile_repo import write_mosaic_profile

from code_reality.hazard import (
    HazardFinding,
    SymbolFacts,
    build_getattr_pattern,
    build_importlib_pattern,
    build_strentenum_patterns,
    classify_rg_lines,
    detect_getattr_dispatch,
    detect_importlib_lazy_load,
    detect_protocol_duck_typing,
    detect_registry_auto_discovery,
    detect_static_edge_gap,
    detect_strentenum_string_dispatch,
    full_findings,
    hazard_gate_warning,
    make_rg_runner,
    method_name,
    parse_symbol_facts,
    resident_findings,
    symbol_facts,
)
from code_reality.profile import HazardRegistry, load_profile

CONDITION_REG = HazardRegistry(
    package_prefix="mosaic_alpha/conditions/",
    suffix="Condition",
    register_fn="auto_register_conditions",
    registry="CONDITION_REGISTRY",
    evidence="mosaic_alpha/conditions/discovery.py:149",
)

FEATURE_REG = HazardRegistry(
    package_prefix="mosaic_alpha/features/",
    suffix="Feature",
    register_fn="auto_register_features",
    registry="FEATURE_REGISTRY",
)

# ── 合成源碼 fixture ──────────────────────────────────────────────

FAKE_STRENUM = """
from enum import StrEnum

class Interval(StrEnum):
    DAILY = "1d"
    WEEKLY = "1w"
    MONTHLY = "1mo"
"""

FAKE_PROTOCOL = """
from typing import Protocol

class FactorCache(Protocol):
    def read_dense(self) -> int: ...
"""

FAKE_PLAIN_CLASS = """
class PlainService:
    def run(self) -> None: ...
"""

FAKE_CONDITION = """
from mosaic_alpha.conditions.base import ConditionBase

class ConsolidationCondition(ConditionBase):
    def evaluate(self) -> bool: ...
"""

FAKE_STR_ENUM_COMMA = """
from enum import Enum

class LegacyInterval(str, Enum):
    DAILY = "1d"
"""


def fake_rg(lines: list[str]):
    """閉包偽造 rg runner——回傳預置 path:line:content 行。"""
    return lambda args: list(lines)


# ── parse_symbol_facts ────────────────────────────────────────────


class TestParseSymbolFacts:
    def test_strentenum_detected(self) -> None:
        facts = parse_symbol_facts(FAKE_STRENUM, "Interval")
        assert facts.is_class is True
        assert facts.is_strentenum is True
        assert sorted(facts.enum_values) == ["1d", "1mo", "1w"]
        assert facts.is_protocol is False

    def test_str_enum_comma_form_detected(self) -> None:
        facts = parse_symbol_facts(FAKE_STR_ENUM_COMMA, "LegacyInterval")
        assert facts.is_strentenum is True
        assert facts.enum_values == ["1d"]

    def test_protocol_detected(self) -> None:
        facts = parse_symbol_facts(FAKE_PROTOCOL, "FactorCache")
        assert facts.is_class is True
        assert facts.is_protocol is True
        assert facts.is_strentenum is False

    def test_plain_class_no_hazard_traits(self) -> None:
        facts = parse_symbol_facts(FAKE_PLAIN_CLASS, "PlainService")
        assert facts.is_class is True
        assert facts.is_strentenum is False
        assert facts.is_protocol is False
        assert facts.enum_values == []

    def test_missing_symbol_returns_empty_facts(self) -> None:
        facts = parse_symbol_facts(FAKE_PLAIN_CLASS, "NoSuchSymbol")
        assert facts.is_class is False
        assert facts.is_strentenum is False

    def test_syntax_error_source_safe(self) -> None:
        facts = parse_symbol_facts("def broken(:", "Interval")
        assert facts.is_class is False


# ── pattern 構建 ──────────────────────────────────────────────────


class TestPatternBuilders:
    def test_getattr_pattern(self) -> None:
        p = build_getattr_pattern("ConsolidationCondition")
        assert "ConsolidationCondition" in p
        assert p.startswith("getattr\\(")

    def test_strentenum_patterns_quote_anchored(self) -> None:
        ps = build_strentenum_patterns(["1d", "1w"])
        assert ps == ['"1d"', '"1w"']

    def test_importlib_pattern(self) -> None:
        p = build_importlib_pattern("mosaic_alpha.venues.tw.venue_profile")
        assert "import_module" in p
        assert "venue_profile" in p


class TestMethodName:
    def test_bare_class_none(self) -> None:
        assert method_name("FactorCache") is None

    def test_class_method(self) -> None:
        assert method_name("FactorCache.read_dense") == "read_dense"

    def test_qualified_name_strips_path(self) -> None:
        assert method_name("/abs/x.py::Class.method") == "method"

    def test_qualified_class_none(self) -> None:
        assert method_name("/abs/x.py::Class") is None


# ── classify_rg_lines ─────────────────────────────────────────────


class TestClassifyRgLines:
    def test_prod_test_split(self) -> None:
        lines = [
            'mosaic_alpha/config/recipes.py:42:interval: str = "1d"',
            'tests/unit_tests/test_x.py:7:x = "1d"',
            'mosaic_alpha/services/foo.py:10:v = "1w"',
        ]
        prod, test, excluded = classify_rg_lines(lines)
        assert len(prod) == 2
        assert len(test) == 1
        assert excluded == []

    def test_excluded_via_profile(self, tmp_path: Path) -> None:
        """收編修正：prototype 呼叫 is_excluded 沒帶 profile——ai-analysis/
        行在 generic fallback 下會誤入 prod。"""
        write_mosaic_profile(tmp_path)
        profile = load_profile(tmp_path)
        lines = [
            'ai-analysis/reports/r.py:1:v = "1d"',
            'mosaic_alpha/services/foo.py:10:v = "1w"',
        ]
        prod, _, excluded = classify_rg_lines(lines, profile)
        assert len(prod) == 1
        assert len(excluded) == 1


# ── detect_* 判定 ─────────────────────────────────────────────────


class TestDetectStrentenumDispatch:
    def test_literal_usage_counted_excluding_def_file(self) -> None:
        facts = parse_symbol_facts(FAKE_STRENUM, "Interval")
        facts.rel_path = "mosaic_alpha/common/enums.py"
        lines = [
            'mosaic_alpha/common/enums.py:73:    DAILY = "1d"',
            'mosaic_alpha/config/recipes.py:42:every: "1d"',
            'configs/foo.yaml:3:interval: "1d"',
            'tests/unit_tests/test_x.py:7:v = "1w"',
        ]
        f = detect_strentenum_string_dispatch(facts, fake_rg(lines))
        assert f is not None
        assert f.kind == "strentenum-string-dispatch"
        assert f.count == 3  # 定義檔行排除後：config + yaml + test
        assert f.detail == {"prod": 2, "test": 1}

    def test_non_enum_returns_none(self) -> None:
        facts = parse_symbol_facts(FAKE_PLAIN_CLASS, "PlainService")
        assert detect_strentenum_string_dispatch(facts, fake_rg([])) is None


class TestDetectGetattrDispatch:
    def test_getattr_hits_reported(self) -> None:
        facts = parse_symbol_facts(FAKE_CONDITION, "ConsolidationCondition")
        lines = [
            "mosaic_alpha/conditions/discovery.py:149:cls = getattr(module, attr_name)",
            "tests/foo.py:1:getattr(m, 'ConsolidationCondition')",
        ]
        f = detect_getattr_dispatch(facts, fake_rg(lines))
        assert f is not None
        assert f.kind == "getattr-string-dispatch"
        assert f.count == 2

    def test_no_hits_returns_none(self) -> None:
        facts = SymbolFacts(name="Whatever")
        assert detect_getattr_dispatch(facts, fake_rg([])) is None

    def test_count_excludes_profile_excluded_lines(self, tmp_path: Path) -> None:
        """count 語義統一＝len(prod)+len(test)——excluded 行不計（審查 FE-F2）。"""
        write_mosaic_profile(tmp_path)
        profile = load_profile(tmp_path)
        facts = SymbolFacts(name="X")
        lines = [
            "mosaic_alpha/a.py:1:getattr(m, 'X')",
            "ai-analysis/r.py:2:getattr(m, 'X')",
        ]
        f = detect_getattr_dispatch(facts, fake_rg(lines), profile)
        assert f is not None
        assert f.count == 1
        assert f.detail == {"prod": 1, "test": 0}


class TestDetectRegistry:
    def test_condition_class_in_registry(self) -> None:
        facts = parse_symbol_facts(FAKE_CONDITION, "ConsolidationCondition")
        facts.rel_path = "mosaic_alpha/conditions/consolidation.py"
        f = detect_registry_auto_discovery(facts, (CONDITION_REG,))
        assert f is not None
        assert f.kind == "registry-auto-discovery"
        assert "auto_register_conditions" in f.summary
        assert "CONDITION_REGISTRY" in f.summary

    def test_feature_class_in_registry(self) -> None:
        facts = SymbolFacts(
            name="MovingAverageFeature",
            is_class=True,
            rel_path="mosaic_alpha/features/moving_average.py",
        )
        f = detect_registry_auto_discovery(facts, (FEATURE_REG,))
        assert f is not None
        assert "auto_register_features" in f.summary

    def test_outside_registry_path_returns_none(self) -> None:
        facts = SymbolFacts(
            name="SomeCondition",
            is_class=True,
            rel_path="mosaic_alpha/strategies/some_condition.py",
        )
        assert detect_registry_auto_discovery(facts, (CONDITION_REG,)) is None

    def test_wrong_suffix_returns_none(self) -> None:
        facts = SymbolFacts(
            name="Helper",
            is_class=True,
            rel_path="mosaic_alpha/conditions/helper.py",
        )
        assert detect_registry_auto_discovery(facts, (CONDITION_REG,)) is None

    def test_empty_registry_table_returns_none(self) -> None:
        """generic repo（profile 無 [[hazard_registry]]）→ 規則靜默不命中。"""
        facts = SymbolFacts(
            name="SomeCondition",
            is_class=True,
            rel_path="mosaic_alpha/conditions/some.py",
        )
        assert detect_registry_auto_discovery(facts, ()) is None


class TestDetectProtocol:
    def test_protocol_annotation_counted(self) -> None:
        facts = parse_symbol_facts(FAKE_PROTOCOL, "FactorCache")
        lines = [
            "mosaic_alpha/features/service.py:60:def load(cache: FactorCache) -> None:",
            "mosaic_alpha/labels/store.py:15:x: FactorCache",
        ]
        f = detect_protocol_duck_typing(facts, fake_rg(lines))
        assert f is not None
        assert f.kind == "protocol-duck-typing"
        assert f.count == 2

    def test_non_protocol_returns_none(self) -> None:
        facts = parse_symbol_facts(FAKE_PLAIN_CLASS, "PlainService")
        assert detect_protocol_duck_typing(facts, fake_rg(["a.py:1:x"])) is None


class TestDetectImportlib:
    def test_literal_module_reference(self) -> None:
        facts = SymbolFacts(
            name="VenueProfile", module="mosaic_alpha.venues.tw.venue_profile"
        )
        lines = [
            'mosaic_alpha/venues/resolver.py:48:mod = importlib.import_module("mosaic_alpha.venues.tw.venue_profile")',
        ]
        f = detect_importlib_lazy_load(facts, fake_rg(lines))
        assert f is not None
        assert f.kind == "importlib-lazy-load"
        assert f.count == 1

    def test_no_module_returns_none(self) -> None:
        facts = SymbolFacts(name="X", module=None)
        assert detect_importlib_lazy_load(facts, fake_rg([])) is None


class TestStaticEdgeGap:
    def _facts(self, name: str, rel: str) -> SymbolFacts:
        return SymbolFacts(name=name, is_class=True, rel_path=rel)

    def test_ctor_file_missing_from_crg_reported(self) -> None:
        """FactorCache 實證場景：labels_extend.py 呼叫了但 CRG 沒建邊。"""
        facts = self._facts("FactorCache", "mosaic_alpha/features/factor_cache.py")
        lines = [
            "mosaic_alpha/features/service.py:201:self._factor_cache = FactorCache(",
            "mosaic_alpha/workflows/labels_extend.py:84:factor_cache = FactorCache(",
            "tests/unit_tests/features/test_output_contract.py:7:c = FactorCache(",
        ]
        crg_files = {
            "mosaic_alpha/features/service.py",
            "tests/unit_tests/features/test_factor_cache.py",
            "tests/unit_tests/features/test_output_contract.py",
        }
        f = detect_static_edge_gap(facts, crg_files, fake_rg(lines))
        assert f is not None
        assert f.kind == "static-edge-gap"
        assert f.count == 1  # 僅 labels_extend 缺
        assert "labels_extend" in f.evidence[0]
        assert f.detail["missing_prod"] == 1

    def test_method_form_uses_dot_pattern(self) -> None:
        facts = self._facts(
            "FactorCache.read_dense", "mosaic_alpha/features/factor_cache.py"
        )
        lines = [
            "mosaic_alpha/features/service.py:793:x.read_dense(",
            "mosaic_alpha/workflows/foo.py:10:y.read_dense(",
        ]
        crg_files = {"mosaic_alpha/features/service.py"}
        f = detect_static_edge_gap(
            facts, crg_files, fake_rg(lines), method="read_dense"
        )
        assert f is not None
        assert "workflows/foo.py" in f.evidence[0]

    def test_no_gap_when_covered(self) -> None:
        facts = self._facts("FactorCache", "mosaic_alpha/features/factor_cache.py")
        lines = ["mosaic_alpha/features/service.py:201:x = FactorCache("]
        crg_files = {"mosaic_alpha/features/service.py"}
        assert detect_static_edge_gap(facts, crg_files, fake_rg(lines)) is None

    def test_none_baseline_skipped(self) -> None:
        """callees 方向無 callers baseline——對帳無意義，跳過。"""
        facts = self._facts("FactorCache", "mosaic_alpha/features/factor_cache.py")
        lines = ["mosaic_alpha/workflows/x.py:1:y = FactorCache("]
        assert detect_static_edge_gap(facts, None, fake_rg(lines)) is None


# ── resident / full 分層 ──────────────────────────────────────────


class TestResidentFindings:
    def test_strentenum_presence_zero_cost(self) -> None:
        facts = parse_symbol_facts(FAKE_STRENUM, "Interval")
        fs = resident_findings(facts, ())
        assert [f.kind for f in fs] == ["strentenum-string-dispatch"]
        assert fs[0].count == 0  # 存在性訊號——未跑 rg 計數

    def test_registry_hit_full_info(self) -> None:
        facts = parse_symbol_facts(FAKE_CONDITION, "ConsolidationCondition")
        facts.rel_path = "mosaic_alpha/conditions/consolidation.py"
        fs = resident_findings(facts, (CONDITION_REG,))
        reg = next(f for f in fs if f.kind == "registry-auto-discovery")
        assert reg.count == 1
        assert reg.evidence == ["mosaic_alpha/conditions/discovery.py:149"]

    def test_protocol_presence(self) -> None:
        facts = parse_symbol_facts(FAKE_PROTOCOL, "FactorCache")
        fs = resident_findings(facts, ())
        assert [f.kind for f in fs] == ["protocol-duck-typing"]

    def test_plain_class_no_findings(self) -> None:
        facts = parse_symbol_facts(FAKE_PLAIN_CLASS, "PlainService")
        assert resident_findings(facts, (CONDITION_REG,)) == []


class TestFullFindings:
    def test_combines_strentenum_and_edge_gap(self) -> None:
        facts = parse_symbol_facts(FAKE_STRENUM, "Interval")
        facts.rel_path = "mosaic_alpha/common/enums.py"
        lines = [
            'mosaic_alpha/config/recipes.py:42:every: "1d"',
            "mosaic_alpha/workflows/labels.py:84:x = Interval(",
        ]
        fs = full_findings(
            facts,
            (),
            fake_rg(lines),
            {"mosaic_alpha/config/recipes.py"},
            None,
        )
        kinds = {f.kind for f in fs}
        # fake rg 不辨 pattern——多規則同時命中是必然；釘住兩目標規則在場
        assert {"strentenum-string-dispatch", "static-edge-gap"} <= kinds


class TestHazardGate:
    def test_few_static_callers_with_hazards_warns(self) -> None:
        f = HazardFinding(kind="registry-auto-discovery", count=1, summary="s")
        warn = hazard_gate_warning(0, 0, [f])
        assert warn is not None
        assert "[WARN]" in warn
        assert "registry-auto-discovery" in warn

    def test_many_static_callers_no_warn(self) -> None:
        f = HazardFinding(kind="strentenum-string-dispatch", count=10, summary="s")
        assert hazard_gate_warning(20, 5, [f]) is None

    def test_no_hazards_no_warn_even_zero_callers(self) -> None:
        assert hazard_gate_warning(0, 0, []) is None


# ── symbol_facts（nodes 表解析，advisory 降級） ───────────────────


class TestSymbolFacts:
    def test_resolves_def_file_and_kind(self, tmp_path: Path) -> None:
        src = tmp_path / "mosaic_alpha" / "common" / "enums.py"
        src.parent.mkdir(parents=True)
        src.write_text(FAKE_STRENUM)
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        make_crg_db(
            db,
            nodes=[("Interval", None, f"{src}::Interval", str(src))],
        )
        facts = symbol_facts("Interval", tmp_path, None)
        assert facts.is_strentenum
        assert facts.rel_path == "mosaic_alpha/common/enums.py"
        assert facts.module == "mosaic_alpha.common.enums"
        assert facts.kind == "Class"

    def test_method_form_resolves_via_parent(self, tmp_path: Path) -> None:
        src = tmp_path / "mosaic_alpha" / "features" / "service.py"
        src.parent.mkdir(parents=True)
        src.write_text(FAKE_PLAIN_CLASS.replace("PlainService", "FeatureService"))
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        make_crg_db(
            db,
            nodes=[
                ("run", "FeatureService", f"{src}::FeatureService.run", str(src)),
            ],
        )
        facts = symbol_facts("FeatureService.run", tmp_path, None)
        assert facts.name == "FeatureService"
        assert facts.is_class
        assert facts.rel_path == "mosaic_alpha/features/service.py"

    def test_no_db_degrades_to_name_only(self, tmp_path: Path) -> None:
        facts = symbol_facts("Whatever", tmp_path, None)
        assert facts.name == "Whatever"
        assert facts.rel_path is None

    def test_ambiguous_degrades_not_crash(self, tmp_path: Path) -> None:
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        make_crg_db(
            db,
            nodes=[
                ("Dup", None, f"{tmp_path}/a.py::Dup", f"{tmp_path}/a.py"),
                ("Dup", None, f"{tmp_path}/b.py::Dup", f"{tmp_path}/b.py"),
            ],
        )
        facts = symbol_facts("Dup", tmp_path, None)
        assert facts.rel_path is None  # advisory 降級——非 SystemExit

    def test_qualified_name_strips_path_prefix(self, tmp_path: Path) -> None:
        src = tmp_path / "mosaic_alpha" / "common" / "enums.py"
        src.parent.mkdir(parents=True)
        src.write_text(FAKE_STRENUM)
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        make_crg_db(db, nodes=[("Interval", None, f"{src}::Interval", str(src))])
        facts = symbol_facts(f"{src}::Interval", tmp_path, None)
        assert facts.is_strentenum

    def test_corrupt_db_raises_actionable_not_bare_traceback(
        self, tmp_path: Path
    ) -> None:
        """qualified-name 流不經 resolve_qualified 的 db 檢查——hazard stage
        是第一個碰表的，訊息品質須與 resolve_qualified 一致（審查 FE-F6）。"""
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        db.write_bytes(b"not a sqlite database")
        with pytest.raises(AssertionError, match="graph.db"):
            symbol_facts("X", tmp_path, None)


class TestMakeRgRunner:
    """真 subprocess rg 路徑（審查 FE-F5）——前綴剝除＋排除 glob＋pattern
    builder 相容性。mock 全繞過了這層，回歸會讓 static-edge-gap 檔集合
    比對靜默失效。"""

    def test_prefix_strip_and_exclusions(self, tmp_path: Path) -> None:
        (tmp_path / "src").mkdir()
        (tmp_path / "src" / "a.py").write_text('x = getattr(m, "X")\n')
        (tmp_path / "ai-analysis").mkdir()
        (tmp_path / "ai-analysis" / "r.py").write_text('x = getattr(m, "X")\n')
        (tmp_path / "stubs").mkdir()
        (tmp_path / "stubs" / "s.py").write_text('x = getattr(m, "X")\n')
        run = make_rg_runner(tmp_path)
        lines = run([build_getattr_pattern("X")])
        # 輸出 repo 相對（絕對前綴剝除）＋僅命中 src（排除 glob 生效）
        assert lines == ['src/a.py:1:x = getattr(m, "X")']

    def test_pattern_builders_rg_compatible(self, tmp_path: Path) -> None:
        """三個 pattern builder 產物對真 rg 可用（非零命中）。"""
        (tmp_path / "m.py").write_text(
            'v = "1d"\nmod = import_module("pkg.mod")\ny = getattr(o, "Sym")\n'
        )
        run = make_rg_runner(tmp_path)
        strentenum_args = ["-F"]
        for p in build_strentenum_patterns(["1d"]):
            strentenum_args += ["-e", p]
        assert run(strentenum_args)
        assert run([build_importlib_pattern("pkg.mod")])
        assert run([build_getattr_pattern("Sym")])
