"""S2 chain_tour 整合測試——真 callchain 文檔 × 真 graph.db（數字錨）。

錨點＝M2 重生成 callstack（mosaic e81bc28d 生成＋85b06ed5 三軌重組，
2026-08-23 重錨——原 callstack-v1 錨隨 M2 清除退役）。數字為錨定時點快照，
**corpus 或 graph.db 再生成皆需重錨**（moved-file 座標隨 graph 世代漂移；
陳舊 graph 會把幀錨到越界行——pattern 靜默不發射屬設計內防禦）。
缺 graph.db 的環境 skip。
"""

import json
import re
from pathlib import Path

import pytest

from code_reality.chain_tour import build_tours, write_tours
from code_reality.common import graph_db_path

REPO_ROOT = (
    Path.home() / "Github" / "mosaic_alpha_offline_backtesting"
)  # 搬遷後錨定 mosaic checkout（整合資料：graph.db/.tours/EP）
CHAIN_MD = REPO_ROOT / "ai-analysis/blueprint/callstack/paper-trading-lifecycle.md"

pytestmark = pytest.mark.integration


@pytest.mark.skipif(
    not graph_db_path(REPO_ROOT).exists(), reason="缺 .code-review-graph/graph.db"
)
@pytest.mark.skipif(not CHAIN_MD.exists(), reason="缺 callchain 文檔")
def test_real_chain_tours(tmp_path: Path) -> None:
    st = build_tours(CHAIN_MD, REPO_ROOT, graph_db_path(REPO_ROOT))

    # 數字錨（2026-08-23 實跑快照）：7 場景／131 幀／skipped 6／步數 125。
    # skipped＝「無 abs 錨」的真實不可成步數；不變量：步數＝幀−跳過。
    assert len(st.tours) == 7
    assert st.frames == 131
    assert st.skipped == 6
    total_steps = sum(len(t["steps"]) for t in st.tours)
    assert total_steps == 125
    assert total_steps == st.frames - st.skipped  # 不變量：步數＝幀−跳過

    flat = [s for t in st.tours for s in t["steps"]]
    # 每步的錨檔真的在磁碟上（moved-file 步指向新檔；其餘指向解析後原檔）
    missing = [s["file"] for s in flat if not (REPO_ROOT / s["file"]).exists()]
    assert missing == [], f"步錨到不存在的檔：{missing[:5]}"

    # tour-contract S1 pattern 抽查：發射出的 pattern 對最終錨檔命中
    # （pattern 由重錨後行內容生成——moved/moved-file 步取新行）
    sampled = [s for s in flat if "pattern" in s]
    assert sampled, "無任何步發射 pattern"
    for s in sampled:
        content = (REPO_ROOT / s["file"]).read_text(encoding="utf-8")
        assert re.search(s["pattern"], content, re.MULTILINE), (
            f"pattern 未命中：{s['file']}"
        )

    # tour-contract S2（SM-4）：寫檔 title 帶 NN - 前綴、上游連鎖 regex 可解析
    paths = write_tours(st, tmp_path)
    assert len(paths) == 7
    for i, p in enumerate(paths, 1):
        written = json.loads(p.read_text())
        m = re.match(r"^#?(\d+)\s+-", written["title"])
        assert m and m.group(1) == f"{i:02d}"

    # SM-5：跨檔重錨由 g_counts 承載——本錨點文檔雙空格修正後 moved-file=0
    # （原唯一實例是 ident 污染＋陳舊 graph 的複合意外，note 分離後消失）

    # 重錨分佈（2026-08-23 新 graph 快照——mosaic e3e0b781 full rebuild 後：
    # same 101/moved 12/moved-file 0/noref 6/not-in-graph 12。因果鏈：
    # 70（單空格污染＋陳舊 graph）→58（雙空格修正）→12（graph 重建）
    assert st.g_counts.get("same", 0) >= 90
    assert st.g_counts.get("moved", 0) >= 8
