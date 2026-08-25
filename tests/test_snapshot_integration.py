"""S2 整合測試——對本 repo 真 CRG graph.db 跑 snapshot（SM-5/6）。

graph.db 缺席（fresh clone）即 skip。graph stale 時 skip 方向抽查（過時
圖的邊方向不可作斷言依據——skip 在結構斷言**之前**判斷）；schema 合約
斷言不受新鮮度影響。方向抽查依據 R3b 實證（impact --files
conditions/service.py 有 features/ui 真信號）。
"""

import json
from pathlib import Path

import pytest

from code_reality.exclusions import is_excluded
from code_reality.profile import load_profile
from code_reality.snapshot import build_snapshot

pytestmark = pytest.mark.integration

REPO_ROOT = (
    Path.home() / "Github" / "mosaic_alpha_offline_backtesting"
)  # 搬遷後錨定 mosaic checkout（整合資料：graph.db/.tours/EP）


def test_real_graph_snapshot(tmp_path: Path) -> None:
    if not (REPO_ROOT / ".code-review-graph" / "graph.db").exists():
        pytest.skip("graph.db 不存在（fresh clone）——先 uvx code-review-graph build")

    snap = build_snapshot(REPO_ROOT, label="s2-integration")
    # schema 合約（不受新鮮度影響）
    assert snap.module_edges, "真 graph.db 的 module edges 不應為空"
    assert snap.files, "files 不應為空"
    # exclusions 單一源生效：stubs/ai-analysis/_archive 不得混入
    assert all(not is_excluded(f, load_profile(REPO_ROOT)) for f in snap.files)
    path = snap.write(tmp_path)
    data = json.loads(path.read_text())
    assert data["_meta"]["repo"] == REPO_ROOT.name
    assert len(data["_meta"]["commit"]) == 40

    if snap.meta.get("stale"):
        pytest.skip(
            f"CRG graph stale（{snap.meta['stale']}）——跳過方向抽查（過時圖不可作斷言依據）"
        )
    # 方向抽查：conditions 相關跨模組邊（R3b 實證方向）
    cond_related = [
        e for e in snap.module_edges if "mosaic_alpha/conditions" in (e[0], e[1])
    ]
    assert cond_related, "conditions 跨模組邊不應為空"
    dsts = {e[1] for e in snap.module_edges if e[0] == "mosaic_alpha/conditions"}
    assert dsts & {
        "mosaic_alpha/features",
        "mosaic_alpha/ui",
        "mosaic_alpha/config",
    }, f"conditions 出邊與 R3b 實證方向不符: {sorted(dsts)}"
