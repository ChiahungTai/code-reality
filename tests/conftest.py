"""conftest for tests/ — fixtures/ helper import 路徑。

fixtures/make_trace.py（合成 trace 產生器）非 test 模組，加入 sys.path
供 test 檔 top-level import。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "fixtures"))
