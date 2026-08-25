"""EP-F1 委派 runtime 證據 driver——viztracer 追蹤目標。

真走 ConditionService.calculate → FeatureService.calculate 委派（評估弧
R6b golden edge）。runtime_edges 整合測試的 fixture（subprocess 執行）。

配方（五坑解，見 ai-analysis/reports/code-reality-tools-evaluation.md R6b）::

    uv run --with viztracer viztracer -o out.json --ignore_c_function \
      --min_duration 0.002 --tracer_entries 2000000 <this driver>

禁 ``--`` 分隔、禁 python 前綴（後續參數被當 script 讀出 null bytes
crash）；pytest 模式錄不到測試階段（buffer 被 import 噪音灌爆）→ 用自足
driver script。
"""

import os
import tempfile

# CACHE_PATH 隔離：測試環境由 tests/conftest.py hard-set（subprocess 繼承
# tmp）；standalone 手跑時以 throwaway temp 防寫 prod cache。必須在
# import mosaic_alpha 前設定（features cache 在 import 時讀 env）。
os.environ.setdefault("CACHE_PATH", tempfile.mkdtemp(prefix="code_reality_driver_"))

from datetime import UTC, datetime, timedelta

import numpy as np
import polars as pl
from mosaic_alpha.common.enums import Interval, ValidationMode
from mosaic_alpha.conditions.discovery import auto_register_conditions
from mosaic_alpha.conditions.service import ConditionService
from mosaic_alpha.features.discovery import auto_register_features


def main() -> None:
    auto_register_features()
    auto_register_conditions()

    np.random.seed(42)
    n = 300
    dates = [datetime(2020, 1, 1, tzinfo=UTC) + timedelta(days=i) for i in range(n)]
    closes = 100.0 * np.exp(np.cumsum(np.random.normal(0, 0.02, n)))
    df = pl.DataFrame(
        {
            "datetime": dates,
            "open": closes * (1 + np.abs(np.random.normal(0, 0.005, n))),
            "high": closes * (1 + np.abs(np.random.normal(0, 0.01, n))),
            "low": closes * (1 - np.abs(np.random.normal(0, 0.01, n))),
            "close": closes,
            "volume": (1000 + np.arange(n)).astype(float),
        }
    )

    result = ConditionService().calculate(
        df,
        instrument="EQDELEG",
        interval=Interval.DAILY,
        validation_mode=ValidationMode.LENIENT,
    )
    print(f"[OK] delegation result columns: {len(result.columns)}")


# main guard（post-build F8）：本檔由 subprocess 執行；conftest 把 fixtures/
# 插入 sys.path，任何測試誤 import 本名時不得觸發註冊＋計算 side-effect
if __name__ == "__main__":
    main()
