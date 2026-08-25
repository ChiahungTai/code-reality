"""S4 hub_refs 單元測試——fixture JSON 聚合邏輯＋名稱解析（SM-9/10/13）。

CRG CLI JSON 形態（ok/ambiguous/not_found）基於 2026-08-21 實測；名稱解析
走 nodes 表 sqlite 精確匹配（CLI fuzzy ambiguous 不含精確匹配——實測證據
見 hub_refs.resolve_qualified docstring）。整合測試見
test_hub_refs_integration.py。
"""

import json
import subprocess as sp
from pathlib import Path

import pytest
from crg_db import make_crg_db
from profile_repo import write_mosaic_profile

from code_reality.hazard import HazardFinding, SymbolFacts
from code_reality.hub_refs import (
    AggResult,
    aggregate,
    crg_query,
    hazard_stage,
    json_payload,
    resolve_qualified,
    resolve_symbol,
)

REPO = Path("/abs/repo")


def crg_node(
    name: str, rel_path: str, is_test: bool = False, qname: str | None = None
) -> dict:
    # rel_path 支援絕對路徑（repo 外節點）——不拼接 REPO
    file_path = rel_path if rel_path.startswith("/") else f"{REPO}/{rel_path}"
    return {
        "id": 1,
        "kind": "Class" if "." not in name else "Function",
        "name": name,
        "qualified_name": qname or f"{file_path}::{name}",
        "file_path": file_path,
        "line_start": 1,
        "line_end": 2,
        "language": "python",
        "parent_name": None,
        "is_test": is_test,
    }


def crg_ok(results: list[dict]) -> dict:
    return {
        "status": "ok",
        "pattern": "callers_of",
        "result_count": len(results),
        "results": results,
    }


@pytest.fixture
def crg_repo(tmp_path: Path) -> Path:
    """含 .code-review-graph/graph.db（nodes 表）的 repo fixture。"""
    (tmp_path / "mosaic_alpha" / "common").mkdir(parents=True)
    db = tmp_path / ".code-review-graph" / "graph.db"
    db.parent.mkdir(parents=True)
    make_crg_db(
        db,
        nodes=[
            (
                "Interval",
                None,
                f"{tmp_path}/mosaic_alpha/common/enums.py::Interval",
                f"{tmp_path}/mosaic_alpha/common/enums.py",
            ),
            (
                "calculate",
                "FeatureService",
                f"{tmp_path}/mosaic_alpha/features/service.py::FeatureService.calculate",
                f"{tmp_path}/mosaic_alpha/features/service.py",
            ),
        ],
    )
    return tmp_path


class TestAggregate:
    def test_dir_counts_test_prod_split(self) -> None:
        refs = crg_ok(
            [
                crg_node("a", "mosaic_alpha/conditions/x.py"),
                crg_node("b", "mosaic_alpha/conditions/y.py"),
                crg_node("c", "mosaic_alpha/features/z.py"),
                crg_node("t1", "tests/unit_tests/conditions/t.py", is_test=True),
                # CRG is_test 漏標的 tests 路徑——heuristic 補（實測案例）
                crg_node("t2", "tests/unit_tests/alpha_forge/t.py", is_test=False),
            ]
        )
        agg = aggregate(refs["results"], REPO)
        assert agg.prod == [
            ("mosaic_alpha/conditions", 2),
            ("mosaic_alpha/features", 1),
        ]
        assert agg.test == [
            ("tests/unit_tests/conditions", 1),
            ("tests/unit_tests/alpha_forge", 1),
        ]
        assert (agg.total_prod, agg.total_test) == (3, 2)

    def test_excluded_paths_filtered(self, tmp_path: Path) -> None:
        write_mosaic_profile(tmp_path)
        refs = crg_ok(
            [
                crg_node("a", str(tmp_path / "mosaic_alpha/conditions/x.py")),
                crg_node("noise", str(tmp_path / "ai-analysis/reports/r.py")),
                crg_node("noise2", str(tmp_path / "stubs/s.py")),
            ]
        )
        agg = aggregate(refs["results"], tmp_path)
        assert agg.prod == [("mosaic_alpha/conditions", 1)]
        assert agg.excluded == 2

    def test_outside_repo_filtered(self) -> None:
        refs = crg_ok(
            [
                crg_node("a", "mosaic_alpha/conditions/x.py"),
                crg_node("o", "/elsewhere/o.py"),
            ]
        )
        agg = aggregate(refs["results"], REPO)
        assert agg.total_prod == 1

    def test_top_truncation(self) -> None:
        refs = crg_ok([crg_node(f"f{i}", f"mosaic_alpha/m{i}/x.py") for i in range(30)])
        agg = aggregate(refs["results"], REPO, top=10)
        assert len(agg.prod) == 10
        assert agg.total_prod == 30  # total 不截斷


class TestCrgQuery:
    def test_subprocess_failure_raises_with_stderr(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        def fake_run(cmd, **kwargs):
            return sp.CompletedProcess(cmd, 2, stdout="", stderr="boom: network down")

        monkeypatch.setattr("code_reality.hub_refs.subprocess.run", fake_run)
        with pytest.raises(AssertionError, match="boom"):
            crg_query("callers_of", "X::y", REPO)

    def test_returns_parsed_json(self, monkeypatch: pytest.MonkeyPatch) -> None:
        payload = crg_ok([crg_node("a", "mosaic_alpha/conditions/x.py")])

        def fake_run(cmd, **kwargs):
            return sp.CompletedProcess(cmd, 0, stdout=json.dumps(payload), stderr="")

        monkeypatch.setattr("code_reality.hub_refs.subprocess.run", fake_run)
        out = crg_query("callers_of", "X::y", REPO)
        assert out["status"] == "ok"

    def test_timeout_raises(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """post-build D7：TimeoutExpired crash-only 分支釘住。"""

        def fake_run(cmd, **kwargs):
            raise sp.TimeoutExpired(cmd, timeout=1)

        monkeypatch.setattr("code_reality.hub_refs.subprocess.run", fake_run)
        with pytest.raises(AssertionError, match="逾時"):
            crg_query("callers_of", "X::y", REPO)

    def test_uv_missing_raises(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """post-build D7：FileNotFoundError（uvx 缺席）分支釘住。"""

        def fake_run(cmd, **kwargs):
            raise FileNotFoundError("uvx")

        monkeypatch.setattr("code_reality.hub_refs.subprocess.run", fake_run)
        with pytest.raises(AssertionError, match="uvx 不在 PATH"):
            crg_query("callers_of", "X::y", REPO)


class TestResolveQualified:
    def test_qualified_passthrough(self) -> None:
        assert resolve_qualified("/x/y.py::Cls.m", REPO) == "/x/y.py::Cls.m"

    def test_bare_name_exact(self, crg_repo: Path) -> None:
        q = resolve_qualified("Interval", crg_repo)
        assert q == f"{crg_repo}/mosaic_alpha/common/enums.py::Interval"

    def test_class_method_by_parent_name(self, crg_repo: Path) -> None:
        """Class.method 裸名：name 欄只存 method 名，parent_name 定位 class。"""
        q = resolve_qualified("FeatureService.calculate", crg_repo)
        assert (
            q
            == f"{crg_repo}/mosaic_alpha/features/service.py::FeatureService.calculate"
        )

    def test_not_found_raises(self, crg_repo: Path) -> None:
        with pytest.raises(SystemExit, match="not found"):
            resolve_qualified("NoSuchThing", crg_repo)

    def test_multiple_matches_ambiguous(self, tmp_path: Path) -> None:
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir(parents=True)
        make_crg_db(
            db,
            nodes=[
                ("Dup", None, f"{tmp_path}/a.py::Dup", f"{tmp_path}/a.py"),
                ("Dup", None, f"{tmp_path}/b.py::Dup", f"{tmp_path}/b.py"),
            ],
        )
        with pytest.raises(SystemExit, match="ambiguous"):
            resolve_qualified("Dup", tmp_path)

    def test_excluded_only_match_treated_as_not_found(self, tmp_path: Path) -> None:
        """唯一匹配在 ai-analysis/（excluded）→ 視同無匹配。"""
        write_mosaic_profile(tmp_path)
        db = tmp_path / ".code-review-graph" / "graph.db"
        db.parent.mkdir(parents=True)
        make_crg_db(
            db,
            nodes=[
                (
                    "X",
                    None,
                    f"{tmp_path}/ai-analysis/x.md::X",
                    f"{tmp_path}/ai-analysis/x.md",
                )
            ],
        )
        with pytest.raises(SystemExit, match="not found"):
            resolve_qualified("X", tmp_path)


class TestResolveSymbol:
    def test_bare_name_resolves_then_queries(
        self, crg_repo: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        seen: list[str] = []

        def fake_query(pattern: str, target: str, repo_root: Path) -> dict:
            seen.append(target)
            return crg_ok([crg_node("caller", "mosaic_alpha/conditions/x.py")])

        monkeypatch.setattr("code_reality.hub_refs.crg_query", fake_query)
        out = resolve_symbol("Interval", crg_repo)
        assert out["status"] == "ok"
        assert seen == [f"{crg_repo}/mosaic_alpha/common/enums.py::Interval"]

    def test_direction_callees_uses_callees_pattern(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """post-build D2：--direction callees 宣稱 ✅ 但原本零覆蓋——釘住
        direction → pattern 映射（callees_of 為 CRG 實測存在的 pattern 名）。"""
        seen: dict[str, str] = {}

        def fake_query(pattern: str, target: str, repo_root: Path) -> dict:
            seen["pattern"] = pattern
            return crg_ok([])

        monkeypatch.setattr("code_reality.hub_refs.crg_query", fake_query)
        monkeypatch.setattr(
            "code_reality.hub_refs.resolve_qualified", lambda s, r: "q::X"
        )
        out = resolve_symbol("X", REPO, direction="callees")
        assert out["status"] == "ok"
        assert seen["pattern"] == "callees_of"

    def test_qualified_not_found_raises_not_silent_empty(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """🔴 review 修正釘住：qualified name 查無 node 時 CRG 回 not_found＋
        exit 0——必須明確失敗（非 ``[OK] 0 refs`` 假陰性）。"""
        not_found = {"status": "not_found", "summary": "No node found matching 'X'."}

        def fake_query(pattern: str, target: str, repo_root: Path) -> dict:
            return dict(not_found)

        monkeypatch.setattr("code_reality.hub_refs.crg_query", fake_query)
        with pytest.raises(SystemExit, match="not_found"):
            resolve_symbol("/abs/no/such.py::Cls.m", Path("/abs/repo"))

    def test_ambiguous_response_forwards_candidates(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        ambiguous = {
            "status": "ambiguous",
            "summary": "matches 2",
            "candidates": [
                crg_node("Dup", "mosaic_alpha/a/x.py"),
                crg_node("Dup", "mosaic_alpha/b/y.py"),
            ],
        }

        def fake_query(pattern: str, target: str, repo_root: Path) -> dict:
            return dict(ambiguous)

        monkeypatch.setattr("code_reality.hub_refs.crg_query", fake_query)
        with pytest.raises(SystemExit, match="ambiguous"):
            resolve_symbol("/abs/repo/a.py::Dup", Path("/abs/repo"))


class TestHazardStage:
    """§5.4 分層觸發——常駐 AST 級 vs static_prod ≤ 2 觸發 rg 級。"""

    def _patch(
        self, monkeypatch: pytest.MonkeyPatch, facts: SymbolFacts, rg_lines: list[str]
    ) -> list:
        monkeypatch.setattr("code_reality.hub_refs.symbol_facts", lambda s, r, p: facts)
        rg_calls: list[list[str]] = []

        def fake_runner(repo_root: Path):
            def run(args: list[str]) -> list[str]:
                rg_calls.append(args)
                return list(rg_lines)

            return run

        monkeypatch.setattr("code_reality.hub_refs.make_rg_runner", fake_runner)
        return rg_calls

    def test_low_prod_triggers_full_scan(self, tmp_path, monkeypatch) -> None:
        """static_prod=0（ConsolidationCondition 形態）→ rg 級全掃＋gate 警告。"""
        facts = SymbolFacts(name="X", is_class=True, is_protocol=True)
        rg_calls = self._patch(monkeypatch, facts, ["src/a.py:5:x: X"])
        findings, warn, level = hazard_stage(
            "X",
            tmp_path,
            direction="callers",
            total_prod=0,
            total_test=0,
            results=[],
        )
        assert rg_calls  # rg 級啟動
        assert any(f.kind == "protocol-duck-typing" for f in findings)
        assert warn is not None
        assert "protocol-duck-typing" in warn
        assert level == "full"

    def test_high_prod_resident_only(self, tmp_path, monkeypatch) -> None:
        """static_prod=4（Interval 形態）→ 不跑 rg，常駐存在性訊號＋無警告。"""
        facts = SymbolFacts(
            name="Interval", is_class=True, is_strentenum=True, enum_values=["1d"]
        )
        rg_calls = self._patch(monkeypatch, facts, [])
        findings, warn, level = hazard_stage(
            "Interval",
            tmp_path,
            direction="callers",
            total_prod=4,
            total_test=1,
            results=[],
        )
        assert not rg_calls  # 未觸發——rg 成本放在危險路徑
        assert [f.kind for f in findings] == ["strentenum-string-dispatch"]
        assert findings[0].count == 0
        assert warn is None
        assert level == "resident"

    def test_trigger_boundary_inclusive_at_two(self, tmp_path, monkeypatch) -> None:
        """觸發條件是 ≤（含 2 非 <）——off-by-one regression 釘住（審查 F6）。"""
        facts = SymbolFacts(name="X", is_class=True)
        rg_calls = self._patch(monkeypatch, facts, [])
        _, _, level = hazard_stage(
            "X", tmp_path, direction="callers", total_prod=2, total_test=0, results=[]
        )
        assert rg_calls
        assert level == "full"

    def test_no_trigger_at_three(self, tmp_path, monkeypatch) -> None:
        facts = SymbolFacts(name="X", is_class=True)
        rg_calls = self._patch(monkeypatch, facts, [])
        _, _, level = hazard_stage(
            "X", tmp_path, direction="callers", total_prod=3, total_test=0, results=[]
        )
        assert not rg_calls
        assert level == "resident"

    def test_force_flag_full_scan_despite_high_prod(
        self, tmp_path, monkeypatch
    ) -> None:
        """--hazard（force）→ 高 callers 也全掃（研究/審計用）。"""
        facts = SymbolFacts(name="X", is_class=True)
        rg_calls = self._patch(monkeypatch, facts, [])
        hazard_stage(
            "X",
            tmp_path,
            direction="callers",
            total_prod=20,
            total_test=5,
            results=[],
            force=True,
        )
        assert rg_calls

    def test_callees_force_skips_gate(self, tmp_path, monkeypatch) -> None:
        """callees 方向無 callers baseline 語意——force 進場但不 gate 警告。"""
        facts = SymbolFacts(name="X", is_class=True)
        self._patch(monkeypatch, facts, [])
        _, warn, _ = hazard_stage(
            "X",
            tmp_path,
            direction="callees",
            total_prod=0,
            total_test=0,
            results=[],
            force=True,
        )
        assert warn is None


class TestJsonPayload:
    def test_shape_and_serializable(self) -> None:
        agg = AggResult(
            prod=[("a", 2)],
            test=[],
            total_prod=2,
            total_test=0,
            excluded=0,
            outside=0,
        )
        f = HazardFinding(kind="k", count=1, summary="s")
        payload = json_payload(
            "X", "q::X", "callers", agg, [f], "w", 3, hazard_level="full"
        )
        assert payload["symbol"] == "X"
        assert payload["aggregate"]["total_prod"] == 2
        assert payload["aggregate"]["prod"] == [["a", 2]]
        assert payload["hazard_findings"][0]["kind"] == "k"
        assert payload["hazard_level"] == "full"
        assert payload["hazard_gate"] == "w"
        assert payload["results_omitted"] == 3
        json.dumps(payload, ensure_ascii=False)  # 可序列化

    def test_empty_hazard_shape(self) -> None:
        agg = AggResult(prod=[], test=[], total_prod=0, total_test=0, excluded=0)
        payload = json_payload("X", "q::X", "callers", agg, [], None, 0)
        assert payload["hazard_findings"] == []
        assert payload["hazard_level"] == "resident"  # 預設值
        assert payload["hazard_gate"] is None
