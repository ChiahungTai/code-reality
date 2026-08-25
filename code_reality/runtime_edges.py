"""runtime edge 抽取器——viztracer trace JSON → runtime 呼叫邊表。

評估弧 R6b 實證 viztracer runtime 事件可還原呼叫邊（EP-F1 委派邊 442ms
巢套 412ms，同 tid、ts 區間包含）；本工具把 LLM 不可消費的巨型 trace
壓縮成邊表 JSON（evidence fusion 的 runtime 源）。

用法::

    uv run python -m code_reality.runtime_edges <trace.json> \
        [-o out.json] [--top N] [--include/--exclude substr] \
        [--repo-only/--no-repo-only] [--repo-root PATH]

viztracer 錄製配方（五坑見 mosaic code-reality-tools-evaluation.md R6b；第六坑與
量產配方見 mosaic gap-prototypes P3 定案）：``--ignore_c_function``＋
``--tracer_entries`` 依負載放大（2M 觸頂線 ≈390 rows——**本負載**實測，
circular buffer
靜默丟頭部 phase，邊集可失真 −88.9%）＋``--min_duration 1~2``（**單位 µs**——
原 ``0.002``＝2ns＝no-op）；事件抽取 ``cat=='fee' && ph=='X'``；nesting＝同
(pid,tid) 且 ts 區間包含。Known gap：repo-only 過濾未接 exclusions.py
（.venv import 噪音邊會進 top 榜——P3 §3.4，正式化時接）。
"""

import argparse
import json
import math
import re
import statistics
import time
from collections import defaultdict
from pathlib import Path
from typing import Any

from code_reality.common import make_meta

# raw name 格式 `qualname (path:line)`——剝 qualname 供聚合、抽 path 供 repo 過濾
_PATH_SUFFIX = re.compile(r" \((.+?):\d+\)$")

# >200MB trace 的 json.load 耗時與記憶體量級提醒（POC 實測 417MB load 1.6s）
_LARGE_TRACE_BYTES = 200 * 1024 * 1024


def qualname(name: str) -> str:
    """``fn (path:line)`` → ``fn``；無 path 尾綴則原樣返回。"""
    return name.split(" (")[0]


def event_path(name: str) -> str | None:
    """``fn (path:line)`` → ``path``；無 path 尾綴（如 genexpr import 噪音）→ None。"""
    m = _PATH_SUFFIX.search(name)
    return m.group(1) if m else None


def load_trace(path: Path) -> dict[str, Any]:
    size = path.stat().st_size
    if size > _LARGE_TRACE_BYTES:
        print(
            f"[WARN] {path.name} {size / 1024 / 1024:.0f}MB：json.load 中（數十秒級）"
        )
    with open(path) as f:
        data = json.load(f)
    assert isinstance(data, dict) and "traceEvents" in data, (
        f"非 viztracer 格式（缺 traceEvents）: {path}"
    )
    return data


def extract_edges(events: list[dict[str, Any]]) -> list[tuple[str, str, float]]:
    """掃描線巢套抽取——回傳 ``(caller_raw, callee_raw, callee_dur_us)``。

    raw name 保留 path 尾綴（repo-only 過濾需要）；qualname 剝離延到
    aggregate。caller 是「ts 區間直接包含 callee」的最近祖先（同進程同
    tid——分組 key 為 ``(pid, tid)``：多進程 trace 兩進程 tid 撞號時
    單 tid 分組會偽造跨進程邊）。

    已知假設與降級（post-build F9）：事件流 well-nested（viztracer fee/X
    complete events 保證配對）；``tracer_entries`` 是 circular buffer——
    溢出丟最舊事件時 caller 會跳級到最近存活祖先（邊仍真實、粒度降級）；
    頭部 phase 整段被丟時邊集大比例缺損（「完全錯誤的圖像」——P3 §3.3，
    見 module docstring 量產配方）。
    1.8M events 已達配方 2M entries 的 90%，錄更大負載時提高 entries。
    """
    by_tid: dict[Any, list[dict[str, Any]]] = defaultdict(list)
    for e in events:
        if e.get("cat") == "fee" and e.get("ph") == "X":
            assert "tid" in e and e.get("dur") is not None, (
                f"fee/X 事件缺 tid/dur 欄位（非 viztracer 慣例格式）: {e.get('name')}"
            )
            by_tid[(e.get("pid"), e["tid"])].append(e)
    assert any(by_tid.values()), (
        "無函式事件（trace 可能被 min_duration 全濾或非 viztracer 格式）"
    )

    edges: list[tuple[str, str, float]] = []
    for group in by_tid.values():
        group.sort(key=lambda e: (e["ts"], -e["dur"]))
        stack: list[tuple[int, int, str]] = []  # (ts, end, name)
        for e in group:
            ts, dur = e["ts"], e["dur"]
            end = ts + dur
            while stack and stack[-1][1] <= ts:
                stack.pop()
            if stack:
                edges.append((stack[-1][2], e["name"], float(dur)))
            stack.append((ts, end, e["name"]))
    return edges


def repo_only_filter(
    edges: list[tuple[str, str, float]], repo_root: Path
) -> list[tuple[str, str, float]]:
    """保留 caller/callee 至少一端 path 在 repo 內的邊。

    POC 實測不過濾時 top 邊全是 import 噪音（``fields.<locals>.<genexpr>``
    ×183,960——無 path 尾綴，此過濾同時排除）。
    """
    root = repo_root.resolve()
    # distinct names 遠少於邊數（417MB trace：1.8M 邊 vs 萬級 names）——
    # Path 構造＋is_relative_to 逐邊重算是 60s 級熱點，name 級快取壓回秒級
    cache: dict[str, bool] = {}

    def in_repo(name: str) -> bool:
        v = cache.get(name)
        if v is None:
            p = event_path(name)
            v = p is not None and Path(p).is_relative_to(root)
            cache[name] = v
        return v

    return [e for e in edges if in_repo(e[0]) or in_repo(e[1])]


def aggregate(edges: list[tuple[str, str, float]]) -> list[dict[str, Any]]:
    """(caller, callee) qualname 聚合 → count/p50/p95（callee dur，ms）。"""
    agg: dict[tuple[str, str], list[float]] = defaultdict(list)
    for caller, callee, dur in edges:
        agg[(qualname(caller), qualname(callee))].append(dur)
    rows = [
        {
            "caller": c,
            "callee": f,
            "count": len(ds),
            "p50_ms": round(statistics.median(ds) / 1000, 2),
            # nearest-rank p95（ceil 取秩）；floor 索引在 n=20 倍數時會取到 max（p100）
            "p95_ms": round(
                sorted(ds)[max(0, math.ceil(0.95 * len(ds)) - 1)] / 1000, 2
            ),
        }
        for (c, f), ds in agg.items()
    ]
    return sorted(rows, key=lambda r: -r["count"])


def _filter_rows(
    rows: list[dict[str, Any]], include: str | None, exclude: str | None
) -> list[dict[str, Any]]:
    if include:
        rows = [r for r in rows if include in r["caller"] or include in r["callee"]]
    if exclude:
        rows = [
            r for r in rows if exclude not in r["caller"] and exclude not in r["callee"]
        ]
    return rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("trace", type=Path, help="viztracer trace JSON 路徑")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="輸出邊表 JSON（預設 <trace>.edges.json）",
    )
    parser.add_argument("--top", type=int, default=0, help="只輸出前 N 邊（0=全部）")
    parser.add_argument(
        "--include", default=None, help="只留 caller/callee 含 substr 的邊"
    )
    parser.add_argument(
        "--exclude", default=None, help="移除 caller/callee 含 substr 的邊"
    )
    parser.add_argument(
        "--repo-only",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="只保留至少一端 path 在 repo 內的邊（預設開——濾 import 噪音）",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path.cwd(),
        help="repo-only 判定根（預設 cwd）",
    )
    args = parser.parse_args()

    t0 = time.time()
    trace = load_trace(args.trace)
    t_load = time.time() - t0

    events = trace["traceEvents"]
    all_edges = extract_edges(events)
    edges = repo_only_filter(all_edges, args.repo_root) if args.repo_only else all_edges
    rows = _filter_rows(aggregate(edges), args.include, args.exclude)
    if args.top:
        rows = rows[: args.top]

    pids = sorted({e.get("pid") for e in events if e.get("pid") is not None})
    out_path = args.output or args.trace.with_suffix(".edges.json")
    out = {
        "_meta": make_meta(
            "code_reality.runtime_edges",
            args.repo_root,
            trace=str(args.trace),
            repo_only=args.repo_only,
            pids=pids,
            total_events=len(events),
            total_edges=len(all_edges),
        ),
        "edges": rows,
    }
    out_path.write_text(json.dumps(out, indent=1))

    print(f"[OK] {len(rows)} edges from {len(events)} events -> {out_path}")
    print(
        f"[LOG] rg '\"callee\"' {out_path} | head -20；load {t_load:.1f}s / "
        f"extract+agg {time.time() - t0 - t_load:.1f}s"
    )
    if args.repo_only and all_edges and not edges:
        print(
            f"[WARN] repo-only 濾除全部 {len(all_edges)} 邊——trace 內 path 與"
            f" --repo-root（{args.repo_root}）不符？（從 repo root 執行或顯式指定）"
        )
    for r in rows[:5]:
        print(
            f"  top: {r['caller']} -> {r['callee']} x{r['count']} p50={r['p50_ms']}ms"
        )
    if len(pids) > 1:
        print(
            f"[WARN] 多進程 trace（pids={pids}）：邊按 (pid,tid) 分組，跨進程邊不在此列"
        )


if __name__ == "__main__":
    main()
