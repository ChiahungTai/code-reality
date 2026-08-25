"""合成 viztracer trace 產生器——單元測試用，不依賴 viztracer 本體。

事件格式對齊 viztracer JSON：fee/X complete events，name 格式
``qualname (path:line)``，ts/dur 單位微秒。
"""

from typing import Any


def make_event(
    name: str,
    ts: int,
    dur: int,
    tid: int = 1,
    pid: int | None = None,
    cat: str = "fee",
    ph: str = "X",
) -> dict[str, Any]:
    ev: dict[str, Any] = {
        "name": name,
        "ts": ts,
        "dur": dur,
        "tid": tid,
        "cat": cat,
        "ph": ph,
    }
    if pid is not None:
        ev["pid"] = pid
    return ev


def make_trace(
    *events: dict[str, Any], extra: dict[str, Any] | None = None
) -> dict[str, Any]:
    trace: dict[str, Any] = {"traceEvents": list(events)}
    if extra:
        trace.update(extra)
    return trace
