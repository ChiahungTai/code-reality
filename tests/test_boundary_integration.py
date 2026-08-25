"""boundary 整合測試——真 NT repo 掃描＋查詢（SM-1/2/3/6/7）。

NT checkout 缺席即 skip。錨點驗證採自洽式（sidecar 指到的 rs 行內容確實
含預期宣告文字）——不寫死行號，對 upstream 前進穩健；tour 錨原文對照見
gap-prototypes 報告 §1.5（mod.rs:158-162／node.rs:103/1151 @ 61590e48）。
"""

import json
import sqlite3
from pathlib import Path

import pytest

from code_reality.boundary import load_sidecar, query_py
from code_reality.boundary_build import build_sidecar, nt_head_sha

pytestmark = pytest.mark.integration

NT_REPO = Path.home() / "Github" / "nautilus_trader"  # 整合資料錨（唯讀）


@pytest.fixture(scope="module")
def built_db(tmp_path_factory: pytest.TempPathFactory) -> Path:
    if not NT_REPO.is_dir():
        pytest.skip(f"NT repo 不存在：{NT_REPO}——整合測試需 NT checkout")
    return build_sidecar(NT_REPO, out_dir=tmp_path_factory.mktemp("boundary"))


def _rows(db: Path, sql: str, params: tuple = ()) -> list[tuple]:
    conn = sqlite3.connect(f"file:{db}?immutable=1", uri=True)
    try:
        return conn.execute(sql, params).fetchall()
    finally:
        conn.close()


def _rs_lines(rs_path: str, rs_line: int, window: int = 5) -> str:
    text = (NT_REPO / rs_path).read_text(errors="replace").splitlines()
    return "\n".join(text[rs_line - 1 : rs_line - 1 + window])


def test_sm1_livenode_class_anchor(built_db: Path) -> None:
    rows = _rows(
        built_db,
        "SELECT rs_path, rs_line, match_kind FROM boundary_edges "
        "WHERE py_symbol = 'nautilus_trader.live.LiveNode' AND level = 'class'",
    )
    assert len(rows) == 1
    rs_path, rs_line, kind = rows[0]
    assert (
        rs_path == "crates/live/src/node/mod.rs"
    )  # native struct 落點（非 binding 檔）
    assert kind == "NAME_MATCH"
    # 自洽錨：掃描器指到的行確實是 pyclass 宣告（cfg_attr 巢狀、live module）
    blob = _rs_lines(str(rs_path), int(rs_line))
    assert "pyclass" in blob
    assert 'module = "nautilus_trader.live"' in blob


def test_sm1_livenode_build_rename_anchor(built_db: Path) -> None:
    rows = _rows(
        built_db,
        "SELECT rs_path, rs_line, rs_symbol, match_kind FROM boundary_edges "
        "WHERE py_symbol = 'nautilus_trader.live.LiveNode.build'",
    )
    assert len(rows) == 1
    rs_path, rs_line, rs_symbol, kind = rows[0]
    assert rs_path == "crates/live/src/python/node.rs"
    assert rs_symbol == "LiveNode::py_build"
    assert kind == "PYO3_NAME_RENAME"
    assert "fn py_build" in _rs_lines(str(rs_path), int(rs_line), 1)


def test_sm1_livenodebuilder_dual_declaration(built_db: Path) -> None:
    rows = _rows(
        built_db,
        "SELECT rs_path, rs_symbol, match_kind FROM boundary_edges "
        "WHERE py_symbol = 'nautilus_trader.live.LiveNodeBuilder' AND level = 'class'",
    )
    assert len(rows) == 2, "同名雙宣告（native＋wrapper）兩邊都該有邊"
    by_kind = {kind: (path, sym) for path, sym, kind in rows}
    assert set(by_kind) == {"NAME_MATCH", "PYCLASS_NAME_RENAME"}
    native_path, _ = by_kind["NAME_MATCH"]
    wrapper_path, wrapper_sym = by_kind["PYCLASS_NAME_RENAME"]
    assert native_path.endswith("node/builder.rs")  # native struct
    assert wrapper_path == "crates/live/src/python/node.rs"  # binding 檔 wrapper
    assert wrapper_sym == "LiveNodeBuilderPy"


def test_sm2_coverage_thresholds(built_db: Path) -> None:
    meta = dict(_rows(built_db, "SELECT key, value FROM meta"))
    summary = json.loads(meta["coverage_summary"])
    assert int(meta["edges_count"]) >= 9000  # 原型 9,681 等級
    assert summary["class_pct"] >= 90.0  # 原型 92.2% 容差內
    assert summary["method_pct"] >= 84.0  # 原型 84.6% 容差內
    # Known Gaps 分類計數存在（custom_data 估＋credential 殘差）
    gaps = json.loads(meta["known_gaps"])
    assert gaps["pyi_only_class_custom_data_macro_est"] >= 10  # 原型 16
    assert gaps["rs_only_method_field_property"] >= 400  # 原型 576
    # meta 錨定 NT HEAD（SM-6 檔名語義）
    assert meta["nt_commit"] == nt_head_sha(NT_REPO)
    assert built_db.stem == meta["nt_commit"][:8]


def test_sm6_idempotent_rerun(built_db: Path) -> None:
    again = build_sidecar(NT_REPO, out_dir=built_db.parent)
    assert again == built_db  # 同 NT sha 覆寫同名檔
    n = _rows(built_db, "SELECT COUNT(*) FROM boundary_edges")[0][0]
    n2 = _rows(again, "SELECT COUNT(*) FROM boundary_edges")[0][0]
    assert n == n2


def test_sm3_query_on_real_sidecar(built_db: Path) -> None:
    """S2 查詢 happy：LiveNode.build → Rust 真身 py_build（真 NT sidecar）。"""
    conn, meta, db_path = load_sidecar(NT_REPO, sidecar_dir=built_db.parent)
    try:
        rows = query_py(conn, "nautilus_trader.live.LiveNode.build")
        assert len(rows) == 1
        assert rows[0]["rs_symbol"] == "LiveNode::py_build"
        assert rows[0]["rs_path"] == "crates/live/src/python/node.rs"
        assert rows[0]["match_kind"] == "PYO3_NAME_RENAME"
    finally:
        conn.close()
    assert meta["nt_commit"] == nt_head_sha(NT_REPO)  # 新鮮 sidecar 無 stale
    assert db_path == built_db
