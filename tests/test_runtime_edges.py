"""S1 runtime_edges 單元測試——合成 trace 覆蓋 SM-1/3/4（巢套/兄弟/遞迴/crash-only）。

驗證意圖提煉自評估弧 POC（git 歷史 poc/poc_runtime_edges.py；掃描線巢套演算法已全量
驗證），非整段貼——斷言對象是搬運升級後的 code_reality.runtime_edges，
確保升級（repo-only filter 併入、_meta 慣例）不改變抽取語義。
"""

import json
from pathlib import Path

import pytest
from make_trace import make_event, make_trace

from code_reality.runtime_edges import (
    aggregate,
    extract_edges,
    load_trace,
    qualname,
    repo_only_filter,
)


class TestExtractEdges:
    def test_nested_parent_child_edge(self) -> None:
        events = [make_event("A", 0, 100), make_event("B", 10, 5)]
        edges = extract_edges(events)
        assert [("A", "B")] == [(c, f) for c, f, _ in edges]

    def test_siblings_no_false_edge(self) -> None:
        events = [make_event("A", 0, 10), make_event("B", 20, 10)]
        assert extract_edges(events) == []

    def test_recursion_self_edge(self) -> None:
        events = [make_event("A", 0, 100), make_event("A", 10, 5)]
        edges = extract_edges(events)
        assert [("A", "A")] == [(c, f) for c, f, _ in edges]

    def test_deep_nesting_nearest_ancestor(self) -> None:
        # A 包 B 包 C：C 的 caller 是 B（最近祖先）非 A
        events = [
            make_event("A", 0, 100),
            make_event("B", 10, 50),
            make_event("C", 20, 5),
        ]
        edges = extract_edges(events)
        assert {("A", "B"), ("B", "C")} == {(c, f) for c, f, _ in edges}

    def test_cross_thread_no_edge(self) -> None:
        events = [make_event("A", 0, 100, tid=1), make_event("B", 10, 5, tid=2)]
        assert extract_edges(events) == []

    def test_cross_process_same_tid_no_false_edge(self) -> None:
        # 多進程 trace 兩進程 tid 撞號：(pid,tid) 分組——跨進程不偽造邊
        events = [
            make_event("A", 0, 100, tid=7, pid=1),
            make_event("C", 10, 5, tid=7, pid=2),
        ]
        assert extract_edges(events) == []

    def test_same_process_same_tid_edge(self) -> None:
        events = [
            make_event("A", 0, 100, tid=7, pid=1),
            make_event("B", 10, 5, tid=7, pid=1),
        ]
        assert [(c, f) for c, f, _ in extract_edges(events)] == [("A", "B")]

    def test_missing_dur_raises(self) -> None:
        ev = make_event("A", 0, 100)
        del ev["dur"]
        with pytest.raises(AssertionError, match="tid/dur"):
            extract_edges([ev])

    def test_missing_tid_raises(self) -> None:
        ev = make_event("A", 0, 100)
        del ev["tid"]
        with pytest.raises(AssertionError, match="tid/dur"):
            extract_edges([ev])

    def test_non_fee_events_ignored(self) -> None:
        events = [
            make_event("A", 0, 100),
            make_event("C", 10, 5, cat="c_function"),
            make_event("D", 10, 5, ph="B"),
        ]
        edges = extract_edges(events)
        assert edges == []

    def test_all_filtered_raises(self) -> None:
        with pytest.raises(AssertionError, match="函式事件"):
            extract_edges([make_event("X", 0, 1, cat="other")])


class TestLoadTrace:
    def test_missing_trace_events_raises(self, tmp_path: Path) -> None:
        p = tmp_path / "bad.json"
        p.write_text("{}")
        with pytest.raises(AssertionError, match="viztracer"):
            load_trace(p)

    def test_missing_file_raises(self, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError):
            load_trace(tmp_path / "nonexistent.json")

    def test_roundtrip(self, tmp_path: Path) -> None:
        p = tmp_path / "ok.json"
        trace = make_trace(make_event("A", 0, 10), extra={"other": 1})
        p.write_text(json.dumps(trace))
        assert load_trace(p) == trace


class TestQualname:
    def test_strips_path_suffix(self) -> None:
        assert qualname("B (b.py:2)") == "B"

    def test_plain_name(self) -> None:
        assert qualname("plain") == "plain"


class TestAggregate:
    def test_count_and_percentiles(self) -> None:
        edges = [
            ("A (a.py:1)", "B (b.py:2)", 100.0),
            ("A (a.py:1)", "B (b.py:2)", 200.0),
            ("A (a.py:1)", "B (b.py:2)", 300.0),
            ("A (a.py:1)", "B (b.py:2)", 400.0),
        ]
        rows = aggregate(edges)
        assert len(rows) == 1
        r = rows[0]
        assert (r["caller"], r["callee"]) == ("A", "B")
        assert r["count"] == 4
        assert r["p50_ms"] == 0.25  # median(100..400)us = 250us
        assert r["p95_ms"] == 0.4  # nearest-rank：ceil(0.95*4)-1 = 3 → 400us

    def test_p95_nearest_rank_n20(self) -> None:
        # n=20（floor 索引會取 max＝p100 的退化點）：nearest-rank 第 19 小
        edges = [("A", "B", float(i)) for i in range(1, 21)]
        (r,) = aggregate(edges)
        assert (
            r["p95_ms"] == 0.02
        )  # ceil(0.95*20)-1 = 18 → sorted[18]=19us → round 0.02

    def test_sorted_by_count_desc(self) -> None:
        edges = [("A", "B", 1.0), ("C", "D", 1.0), ("C", "D", 2.0)]
        rows = aggregate(edges)
        assert [r["callee"] for r in rows] == ["D", "B"]


class TestRepoOnlyFilter:
    def test_filters_no_path_and_outside(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        edges = [
            (f"f ({repo}/a.py:1)", f"g ({repo}/b.py:2)", 1.0),  # 兩端在 repo → 保留
            ("h", f"i ({repo}/b.py:2)", 1.0),  # callee 在 repo → 保留
            (f"j ({tmp_path}/outside.py:1)", "k", 1.0),  # caller path 在外 → 濾
            ("m", "n", 1.0),  # 兩端皆無 path（genexpr import 噪音）→ 濾
        ]
        kept = repo_only_filter(edges, repo)
        assert {("f", "g"), ("h", "i")} == {
            (qualname(c), qualname(f)) for c, f, _ in kept
        }
