"""S4 整合測試——真 CRG CLI 查詢＋聚合（SM-9/10）。

斷言以 known-CRG 行為寫（EP S4）：CRG 漏 instance-attr 邊是 R2 已知結果
（callers_of FeatureService.calculate 回 130 條全 tests）——本測斷言聚合
行為，非 CRG 完整性，避免測試釘住上游缺陷。
"""

from pathlib import Path

import pytest

from code_reality.hub_refs import aggregate, resolve_symbol

REPO_ROOT = (
    Path.home() / "Github" / "mosaic_alpha_offline_backtesting"
)  # 搬遷後錨定 mosaic checkout（整合資料：graph.db/.tours/EP）

pytestmark = [
    pytest.mark.integration,
    pytest.mark.skipif(
        not (REPO_ROOT / ".code-review-graph" / "graph.db").exists(),
        reason="整合資料缺席：mosaic checkout 的 CRG graph.db（同類 skipif 慣例）",
    ),
]


def test_interval_hub_aggregation() -> None:
    """SM-9：裸名 Interval 精確解析 → 按目錄聚合（非 195KB 洪流）。

    known-CRG 行為：callers_of 只含 CALLS 邊——enum 的 callers 遠少於
    rg 文字匹配的 142 檔（多數使用是 REFERENCES 邊，不在 callers_of）。
    斷言聚焦聚合維度（目錄計數 ≤100 行等級），不釘住 CRG 邊模型。
    """
    resp = resolve_symbol("Interval", REPO_ROOT)
    assert resp["status"] == "ok"
    assert "mosaic_alpha/common/enums.py::Interval" in resp["target"]
    agg = aggregate(resp["results"], REPO_ROOT, top=20)
    # 聚合輸出 = 2 欄 × top 目錄計數（≤100 行等級），refs 總數保留在 totals
    assert agg.total_prod + agg.total_test > 0
    assert len(agg.prod) <= 20 and len(agg.test) <= 20
    # callers 落點是 repo 內目錄（相對路徑、非絕對殘留）
    assert all(not d.startswith("/") for d, _ in agg.prod)


def test_feature_service_calculate_known_crg_behavior() -> None:
    """R2 known behavior：CRG callers 全 tests——斷言聚合 test 側非空。"""
    qname = f"{REPO_ROOT}/mosaic_alpha/features/service.py::FeatureService.calculate"
    resp = resolve_symbol(qname, REPO_ROOT)
    assert resp["status"] == "ok"
    agg = aggregate(resp["results"], REPO_ROOT)
    assert agg.total_test > 0, (
        "known-CRG 行為（R2：130 callers 全 tests）應使 test 側非空"
    )
