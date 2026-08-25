"""boundary 查詢 CLI 單元測試——fixture 小 sidecar（LiveNode 家族縮影）。

涵蓋：查詢 happy／裸名／多命中（雙宣告列全部）／not-found 候選消歧／
load_sidecar 多檔語義（優先 HEAD 對應檔；無則 mtime 最新＋[WARN] 不靜默）／
stale（SM-7）／--rs 反向。
"""

import os
import sqlite3
import time
from pathlib import Path

import pytest

from code_reality import boundary as boundary_mod
from code_reality.boundary import load_sidecar, query_py, query_rs, run_query
from code_reality.boundary_build import write_sidecar

HEAD = "b" * 40
OLD = "c" * 40

EDGES = [
    {
        "level": "class",
        "py_symbol": "nautilus_trader.live.LiveNode",
        "pyi_path": "python/nautilus_trader/live/__init__.pyi",
        "pyi_line": 230,
        "rs_symbol": "LiveNode",
        "rs_path": "crates/live/src/node/mod.rs",
        "rs_line": 158,
        "match_kind": "NAME_MATCH",
    },
    {
        "level": "method",
        "py_symbol": "nautilus_trader.live.LiveNode.build",
        "pyi_path": "python/nautilus_trader/live/__init__.pyi",
        "pyi_line": 244,
        "rs_symbol": "LiveNode::py_build",
        "rs_path": "crates/live/src/python/node.rs",
        "rs_line": 103,
        "match_kind": "PYO3_NAME_RENAME",
        "method_kind": "staticmethod",
    },
    {
        "level": "class",
        "py_symbol": "nautilus_trader.live.LiveNodeBuilder",
        "pyi_path": "python/nautilus_trader/live/__init__.pyi",
        "pyi_line": 272,
        "rs_symbol": "LiveNodeBuilder",
        "rs_path": "crates/live/src/node/builder.rs",
        "rs_line": 84,
        "match_kind": "NAME_MATCH",
    },
    {
        "level": "class",
        "py_symbol": "nautilus_trader.live.LiveNodeBuilder",
        "pyi_path": "python/nautilus_trader/live/__init__.pyi",
        "pyi_line": 272,
        "rs_symbol": "LiveNodeBuilderPy",
        "rs_path": "crates/live/src/python/node.rs",
        "rs_line": 1151,
        "match_kind": "PYCLASS_NAME_RENAME",
    },
    {
        "level": "function",
        "py_symbol": "nautilus_trader.trading.fx_next_start",
        "pyi_path": "python/nautilus_trader/trading/__init__.pyi",
        "pyi_line": 10,
        "rs_symbol": "fx_next_start",
        "rs_path": "crates/trading/src/python.rs",
        "rs_line": 20,
        "match_kind": "NAME_MATCH",
    },
]

COVERAGE = {
    "classes": {
        "rs_pyclass_total": 3,
        "matched": 3,
        "rs_only": 0,
        "pyi_total": 2,
        "pyi_only": 0,
    },
    "methods": {
        "rs_methods_on_matched_classes": 1,
        "matched": 1,
        "rs_only": 0,
        "pyi_only": 0,
        "rs_only_by_kind": {},
    },
    "functions": {"rs_functions": 1, "matched": 1, "rs_only": 0, "pyi_only": 0},
}


@pytest.fixture
def sc(tmp_path: Path):
    """載入小 sidecar（head 錨定 HEAD sha——無 stale WARN）。"""
    write_sidecar(tmp_path / "nt", HEAD, EDGES, COVERAGE, out_dir=tmp_path / "sc")
    conn, meta, db = load_sidecar(
        tmp_path / "nt", sidecar_dir=tmp_path / "sc", head=HEAD
    )
    yield conn, meta, db
    conn.close()


# ---------------------------------------------------------------------------
# load_sidecar 多檔語義（EP S2／Finding 2）＋SM-7 stale
# ---------------------------------------------------------------------------


def test_load_sidecar_prefers_head_match(tmp_path: Path):
    """多檔並存：優先 `_meta.nt_commit == NT HEAD` 檔（非 mtime 最新——U-1：
    HEAD 先寫、OLD 後寫（mtime 較新），純 mtime 實作會誤選 OLD 而失敗；
    R14：OLD mtime 顯式設未來時間，鑑別力與 ms 時序解耦）。"""
    db_head = write_sidecar(
        tmp_path / "nt", HEAD, EDGES, COVERAGE, out_dir=tmp_path / "sc"
    )
    db_old = write_sidecar(
        tmp_path / "nt", OLD, EDGES, COVERAGE, out_dir=tmp_path / "sc"
    )
    future = time.time() + 10
    os.utime(db_old, (future, future))
    conn, meta, db = load_sidecar(
        tmp_path / "nt", sidecar_dir=tmp_path / "sc", head=HEAD
    )
    conn.close()
    assert db == db_head
    assert meta["nt_commit"] == HEAD


def test_load_sidecar_foreign_db_tolerated_when_head_present(tmp_path: Path, capsys):
    """R7：head-match loop 遇外部 .db 容忍跳過（[WARN]），不擋正確檔；
    目錄僅 foreign 時仍 loud crash（見 foreign_db_crash 測試）。"""
    db_head = write_sidecar(
        tmp_path / "nt", HEAD, EDGES, COVERAGE, out_dir=tmp_path / "sc"
    )
    (tmp_path / "sc" / "aaaa.db").write_text("not a sqlite db")  # sorted 在 HEAD 前
    conn, _meta, db = load_sidecar(
        tmp_path / "nt", sidecar_dir=tmp_path / "sc", head=HEAD
    )
    conn.close()
    assert db == db_head
    out = capsys.readouterr().out
    assert "[WARN]" in out and "aaaa.db" in out


def test_load_sidecar_stale_warn_not_silent(tmp_path: Path, capsys):
    """SM-7：無 HEAD 對應檔 → mtime 最新＋[WARN]（不靜默 fallback）。"""
    write_sidecar(tmp_path / "nt", OLD, EDGES, COVERAGE, out_dir=tmp_path / "sc")
    conn, meta, db = load_sidecar(
        tmp_path / "nt", sidecar_dir=tmp_path / "sc", head=HEAD
    )
    conn.close()
    assert meta["nt_commit"] == OLD
    assert db.name == f"{OLD[:8]}.db"
    out = capsys.readouterr().out
    assert "[WARN]" in out
    assert "boundary_build" in out  # 附重跑指引


def test_load_sidecar_missing_dir_crash(tmp_path: Path):
    with pytest.raises(AssertionError, match="boundary_build"):
        load_sidecar(tmp_path / "nt", sidecar_dir=tmp_path / "empty", head=HEAD)


def test_load_sidecar_foreign_db_crash(tmp_path: Path):
    """F4 regression：sidecar 目錄混入非 sidecar 的 .db → loud crash 附檔名。"""
    sc_dir = tmp_path / "sc"
    sc_dir.mkdir()
    (sc_dir / "foreign.db").write_text("not a sqlite db")
    with pytest.raises(AssertionError, match="foreign.db"):
        load_sidecar(tmp_path / "nt", sidecar_dir=sc_dir, head=HEAD)


def test_load_sidecar_readonly_connection(sc):
    conn, _, _ = sc
    with pytest.raises(sqlite3.OperationalError):
        conn.execute("DELETE FROM boundary_edges")  # 唯讀連線拒寫


# ---------------------------------------------------------------------------
# 查詢（SM-3/SM-4）
# ---------------------------------------------------------------------------


def test_query_full_symbol_expands_methods(sc):
    conn, _, _ = sc
    rows = query_py(conn, "nautilus_trader.live.LiveNode")
    assert {r["py_symbol"] for r in rows} == {
        "nautilus_trader.live.LiveNode",
        "nautilus_trader.live.LiveNode.build",
    }


def test_query_bare_name(sc):
    conn, _, _ = sc
    rows = query_py(conn, "LiveNode")
    assert {r["py_symbol"] for r in rows} == {
        "nautilus_trader.live.LiveNode",
        "nautilus_trader.live.LiveNode.build",
    }
    assert not [r for r in rows if "LiveNodeBuilder" in r["py_symbol"]], (
        "裸名後綴匹配不得誤捕更長符號（LiveNodeBuilder）"
    )


def test_query_multi_hit_dual_declaration(sc):
    """同名雙宣告（LiveNodeBuilder native＋wrapper）＝合法多對一——列全部非報錯。"""
    conn, _, _ = sc
    rows = query_py(conn, "LiveNodeBuilder")
    assert len(rows) == 2
    assert {r["rs_symbol"] for r in rows} == {"LiveNodeBuilder", "LiveNodeBuilderPy"}


def test_query_rs_reverse(sc):
    conn, _, _ = sc
    rows = query_rs(conn, "py_build")
    assert len(rows) == 1
    assert rows[0]["py_symbol"] == "nautilus_trader.live.LiveNode.build"
    rows2 = query_rs(conn, "LiveNode")  # Rust struct 名（class 邊）
    assert {r["match_kind"] for r in rows2} == {"NAME_MATCH"}


def test_run_query_not_found_candidates(sc, capsys):
    """SM-4：查無符號 → [FAIL]＋候選消歧（比照 hub_refs）。"""
    conn, meta, db = sc
    with pytest.raises(SystemExit, match="not found"):
        run_query(conn, meta, db, "LiveNod")
    out = capsys.readouterr().out
    assert "[FAIL]" in out
    assert "候選" in out
    assert "nautilus_trader.live.LiveNode" in out  # 候選含正確符號


def test_run_query_output_happy(sc, capsys):
    conn, meta, db = sc
    run_query(conn, meta, db, "LiveNode")
    out = capsys.readouterr().out
    assert out.startswith("[OK] LiveNode: 2 edges")
    assert "PYO3_NAME_RENAME" in out
    assert "crates/live/src/python/node.rs:103" in out
    assert "[LOG]" in out


def test_query_like_wildcards_escaped(sc):
    """F3 regression（build review）：`_`/`%` 是 LIKE 萬用字元——未 escape
    時 `LiveNod_` 會誤中 LiveNode（真 sidecar 實測 91 rows）。escape 後
    符號語義精確（`_`/`%` 是字面）。"""
    conn, _, _ = sc
    assert query_py(conn, "LiveNod_") == []
    assert query_py(conn, "Live%") == []
    assert query_rs(conn, "py_buil_") == []


def test_run_query_mtime_guard(sc, monkeypatch):
    """U-2 接線 regression：查詢期間 sidecar 被覆寫（mtime 變）→
    assert_db_unchanged crash（防線若被重構移除，本測試擋住）。"""
    conn, meta, db = sc
    orig_query = boundary_mod.query_py

    def query_then_rewrite(conn_, symbol):
        rows = orig_query(conn_, symbol)
        future = time.time() + 5
        os.utime(db, (future, future))  # 模擬查詢期間 rebuild 覆寫
        return rows

    monkeypatch.setattr(boundary_mod, "query_py", query_then_rewrite)
    with pytest.raises(AssertionError, match="改寫"):
        boundary_mod.run_query(conn, meta, db, "LiveNode")
