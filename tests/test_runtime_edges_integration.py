"""S1 整合測試——viztracer 真跑 EP-F1 driver → 抽取 → golden edge（SM-1）。

golden oracle 是評估弧 R6b 實證的委派邊
``ConditionService.calculate -> FeatureService.calculate``（442ms 巢套 412ms）。

viztracer 不在 dev deps：缺席即 skip。執行配方::

    uv run --with viztracer pytest -m integration tests/ -v

SM-2（≥400MB trace）歷史量測證據（POC 期 M1 Max 實測，2026-08-21；417MB
trace 已刪——重生成：driver 同款 300-row 全 features 負載）：
417MB / 1.8M events / load 1.7s / extract+agg 2.4s（name 快取後）/
9,955 edges——遠低於 60s 成功標準。本整合測跑的 300-row driver 即同款
負載，量測 print 於測試輸出（不斷言硬門檻——EP 驗證策略）。
"""

import subprocess
import sys
import time
from pathlib import Path

import pytest

from code_reality.runtime_edges import (
    aggregate,
    extract_edges,
    load_trace,
    repo_only_filter,
)

pytestmark = pytest.mark.integration

pytest.importorskip("viztracer")

# 依賴鏈：viztracer 之外還需 mosaic checkout（driver 經 `uv pip install -e`
# 裝的 mosaic-alpha 進 ai-rules venv）——bare `uv sync` 會 prune 掉它，
# driver subprocess 會 loud 失敗（附 stdout）；先 `uv pip install -e
# <mosaic-repo>` 再跑本測試。

DRIVER = Path(__file__).parent / "fixtures" / "delegation_driver.py"
REPO_ROOT = (
    Path.home() / "Github" / "mosaic_alpha_offline_backtesting"
)  # 搬遷後錨定 mosaic checkout（整合資料：graph.db/.tours/EP）
GOLDEN = ("ConditionService.calculate", "FeatureService.calculate")

# viztracer 五坑配方（評估報告 R6b）：禁 `--` 分隔、禁 python 前綴、
# pytest 模式錄不到（故 subprocess 跑自足 driver）、--include_files 無效
VIZTRACER_FLAGS = [
    "--ignore_c_function",
    "--min_duration",
    "0.002",
    "--tracer_entries",
    "2000000",
]


def test_golden_delegation_edge(tmp_path: Path) -> None:
    trace_path = tmp_path / "deleg_trace.json"
    cmd = [
        sys.executable,
        "-m",
        "viztracer",
        "-o",
        str(trace_path),
        *VIZTRACER_FLAGS,
        str(DRIVER),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300, check=False)
    assert proc.returncode == 0, (
        f"viztracer 失敗:\n{proc.stdout[-1000:]}\n{proc.stderr[-2000:]}"
    )

    t0 = time.time()
    trace = load_trace(trace_path)
    t_load = time.time() - t0
    events = trace["traceEvents"]
    edges = repo_only_filter(extract_edges(events), REPO_ROOT)
    rows = aggregate(edges)
    t_extract = time.time() - t0 - t_load

    size_mb = trace_path.stat().st_size / 1024 / 1024
    print(
        f"[LOG] SM-2 量測：trace {size_mb:.0f}MB / {len(events)} events；"
        f"load {t_load:.1f}s / extract+agg {t_extract:.1f}s / {len(rows)} edges"
    )
    assert GOLDEN in {(r["caller"], r["callee"]) for r in rows}, (
        "golden edge 未命中——runtime 證據鏈斷裂，檢查 driver 委派路徑"
    )
    trace_path.unlink(missing_ok=True)  # 數百 MB 級產物即測即清
