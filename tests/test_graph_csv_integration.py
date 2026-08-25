"""S3 graph_csv 整合測試——真 CRG graph.db（量級錨＝當前 graph 重建快照）。

錨 1,112/2,732 是 graph.db 重建後的實測快照（graph 隨 CRG 增量演進，
斷言語義＝「真 graph 非空殼」的量級下界——重建縮小時隨實況更新錨）。
缺 graph.db 的環境 skip。
"""

import csv
from pathlib import Path

import pytest

from code_reality.common import graph_db_path
from code_reality.graph_csv import load, write_csvs

REPO_ROOT = (
    Path.home() / "Github" / "mosaic_alpha_offline_backtesting"
)  # 搬遷後錨定 mosaic checkout（整合資料：graph.db/.tours/EP）

pytestmark = [
    pytest.mark.integration,
    pytest.mark.skipif(
        not graph_db_path(REPO_ROOT).exists(), reason="缺 .code-review-graph/graph.db"
    ),
]


def test_real_graph_csv_invariants(tmp_path: Path) -> None:
    db = graph_db_path(REPO_ROOT)
    g = load(db, REPO_ROOT)
    # 量級錨＝最新刪檔後 graph 重建實測（抽離弧 1,112/2,732；POC 期 1,218/2,815）——
    # 下界反映「真 graph 非空殼」語義而非單調成長（mosaic 刪檔弧後錨隨之更新）
    assert len(g.nodes) >= 1112
    assert len(g.links) >= 2732
    # community 欄：多數決結果必屬 communities 表（或無成員投票為空）
    comm_ids = set(g.communities)
    assert comm_ids, "真 graph 應有 communities"
    assert all(
        c in comm_ids for c in (f["community"] for f in g.nodes) if c is not None
    )

    nodes_p, links_p = write_csvs(g, tmp_path)
    with open(nodes_p) as fh:
        nrows = list(csv.DictReader(fh))
    with open(links_p) as fh:
        lrows = list(csv.DictReader(fh))
    assert len(nrows) == len(g.nodes)
    assert len(lrows) == len(g.links)
    assert sum(int(r["degree"]) for r in nrows) == 2 * len(lrows)
