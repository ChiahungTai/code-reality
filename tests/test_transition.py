"""S3 transition 單元測試——邊集差異＋EP 宣稱對照（SM-7/8/12）。

S3 帶已知 bug 開工（EP Dogfood 規格）：評估弧 POC（git 歷史）的
assert fail 反例（reversed 回報方向）是本段第一個 RED 素材——斷言 B1
修正後語義：reversed 一律回報 **added 方向**；差異運算在 pair 集合差。
"""

import json
from pathlib import Path

import pytest

from code_reality.profile import ModuleRule, Profile
from code_reality.transition import (
    compare_claims,
    diff_edges,
    extract_baseline,
    extract_ep_claims,
    load_snapshot,
    render_json,
    render_report,
    summarize,
)

MOSAIC_PROFILE = Profile(modules=(ModuleRule(prefix="mosaic_alpha/"),))


def snap(
    path: Path,
    edges: list[list[str]],
    files: list[str] | None = None,
    commit: str = "a" * 40,
) -> Path:
    path.write_text(
        json.dumps(
            {
                "_meta": {
                    "repo": "r",
                    "commit": commit,
                    "created_at": "2026",
                    "tool": "t",
                },
                "files": files or [],
                "module_edges": edges,
            }
        )
    )
    return path


class TestDiffEdges:
    def test_added_removed(self) -> None:
        a = {("X", "Y", "CALLS")}
        b = {("X", "Y", "CALLS"), ("Y", "Z", "IMPORTS_FROM")}
        d = diff_edges(a, b)
        assert d.added == [("Y", "Z", "IMPORTS_FROM")]
        assert d.removed == []
        assert d.reversed == []

    def test_reversed_reports_added_direction(self) -> None:
        # B1 核心案例：a 有 (X,Y)、b 變 (Y,X) → reversed 回報 added 側 (Y,X)
        a = {("X", "Y", "CALLS")}
        b = {("Y", "X", "CALLS")}
        d = diff_edges(a, b)
        assert d.reversed == [("Y", "X")]
        assert d.added == [("Y", "X", "CALLS")]
        assert d.removed == [("X", "Y", "CALLS")]

    def test_multi_kind_pair_not_false_reversed(self) -> None:
        # pair 同 (X,Y) 僅 kind 變：非 reversed；kind 級 added/removed
        a = {("X", "Y", "CALLS")}
        b = {("X", "Y", "IMPORTS_FROM")}
        d = diff_edges(a, b)
        assert d.reversed == []
        assert d.added == [("X", "Y", "IMPORTS_FROM")]
        assert d.removed == [("X", "Y", "CALLS")]

    def test_same_set_empty_diff(self) -> None:
        # SM-8：同集自 diff → 全空（無結構變化）
        s = {("X", "Y", "CALLS"), ("Y", "Z", "CALLS")}
        d = diff_edges(s, s)
        assert d.added == [] and d.removed == [] and d.reversed == []

    def test_changed_modules(self) -> None:
        a = {("X", "Y", "CALLS")}
        b = {("Y", "Z", "CALLS"), ("X", "Y", "CALLS")}
        d = diff_edges(a, b)
        assert d.changed_modules == {"Y", "Z"}


class TestLoadSnapshot:
    def test_missing_meta_raises(self, tmp_path: Path) -> None:
        p = tmp_path / "bad.json"
        p.write_text('{"files": [], "module_edges": []}')
        with pytest.raises(AssertionError, match="snapshot"):
            load_snapshot(p)


class TestEpClaims:
    def test_extracts_mosaic_modules_only(self, tmp_path: Path) -> None:
        ep = tmp_path / "ep.md"
        ep.write_text(
            "# EP\n改 mosaic_alpha/conditions 與 mosaic_alpha/features；"
            "工具在 scripts/（不抽）＋ mosaic_alpha（頂層名不算模組路徑）"
        )
        assert extract_ep_claims(ep, MOSAIC_PROFILE) == {
            "mosaic_alpha/conditions",
            "mosaic_alpha/features",
        }

    def test_no_mentions_empty(self, tmp_path: Path) -> None:
        ep = tmp_path / "ep.md"
        ep.write_text("# 純文檔 EP，無模組路徑")
        assert extract_ep_claims(ep, MOSAIC_PROFILE) == set()

    def test_no_profile_claims_never_match(self, tmp_path: Path) -> None:
        # generic fallback：無 profile → claims 恆空（profile 能力，by design）
        ep = tmp_path / "ep.md"
        ep.write_text("改 mosaic_alpha/conditions")
        assert extract_ep_claims(ep, None) == set()

    def test_missing_file_raises(self, tmp_path: Path) -> None:
        # SM-12：檔不存在 → crash（非 NONE——NONE 是檔在但無路徑 mention）
        with pytest.raises(AssertionError, match="不存在"):
            extract_ep_claims(tmp_path / "nope.md", MOSAIC_PROFILE)

    def test_extract_baseline(self, tmp_path: Path) -> None:
        ep = tmp_path / "ep.md"
        ep.write_text("> **baseline**: 87173a8a0a788a2e\n")
        assert extract_baseline(ep) == "87173a8a0a788a2e"


class TestCompareClaims:
    def test_three_buckets(self) -> None:
        claims = {"mosaic_alpha/conditions", "mosaic_alpha/features", "mosaic_alpha/ui"}
        changed = {"mosaic_alpha/conditions", "mosaic_alpha/data", "tests"}
        result = compare_claims(claims, changed)
        assert result.claimed_and_changed == ["mosaic_alpha/conditions"]
        assert result.changed_not_claimed == ["mosaic_alpha/data", "tests"]
        assert result.claimed_not_changed == [
            "mosaic_alpha/features",
            "mosaic_alpha/ui",
        ]

    def test_empty_claims_is_none_bucket(self) -> None:
        result = compare_claims(set(), {"mosaic_alpha/data"})
        assert result.claimed_and_changed == []
        assert result.claims_none is True


class TestReportRendering:
    def test_no_change_report_says_so(self, tmp_path: Path) -> None:
        a = snap(
            tmp_path / "a.json",
            [("X", "Y", "CALLS")],
            files=["x/a.py"],
            commit="a" * 40,
        )
        b = snap(
            tmp_path / "b.json",
            [("X", "Y", "CALLS")],
            files=["x/a.py"],
            commit="a" * 40,
        )
        sa, sb = load_snapshot(a), load_snapshot(b)
        diff, nf, gf = summarize(sa, sb)
        md = render_report(sa, sb, None, diff, nf, gf)
        assert "無結構變化" in md

    def test_report_contains_edges_and_claims(self, tmp_path: Path) -> None:
        a = snap(tmp_path / "a.json", [("X", "Y", "CALLS")], files=["x/a.py"])
        b = snap(
            tmp_path / "b.json",
            [("Y", "Z", "CALLS")],
            files=["x/a.py", "y/b.py"],
            commit="b" * 40,
        )
        sa, sb = load_snapshot(a), load_snapshot(b)
        diff, nf, gf = summarize(sa, sb)
        md = render_report(sa, sb, {"Y", "W"}, diff, nf, gf)
        assert "Y -> Z" in md
        assert "W" in md  # 宣稱未動

    def test_files_only_change_counts_as_changed_module(self, tmp_path: Path) -> None:
        """檔案級變動併入 changed_modules——「模組加檔案但邊拓撲不變」
        不得誤報為宣稱未動（review F3/K 修正釘住）。"""
        a = snap(tmp_path / "a.json", [("X", "Y", "CALLS")], files=["x/a.py"])
        b = snap(
            tmp_path / "b.json",
            [("X", "Y", "CALLS")],
            files=["x/a.py", "mosaic_alpha/conditions/new.py"],
            commit="b" * 40,
        )
        sa, sb = load_snapshot(a), load_snapshot(b)
        out = render_json(
            sa, sb, {"mosaic_alpha/conditions"}, *summarize(sa, sb), MOSAIC_PROFILE
        )
        assert out["changed_modules"] == ["mosaic_alpha/conditions"]
        assert out["ep_claims"]["claimed_and_changed"] == ["mosaic_alpha/conditions"]
        assert out["ep_claims"]["claimed_not_changed"] == []
