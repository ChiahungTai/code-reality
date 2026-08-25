"""boundary 查詢 CLI——python 符號 → Rust 真身（path:line＋match_kind）。

v2 遷移期消費場景：「這個 python 符號的實作在哪個 Rust 檔哪一行」——
migration status dashboard 的資料層前身（評估報告 §5.4 L4 PROVENANCE 地基）。

sidecar 由 ``boundary_build`` 產生（``~/.mosaic/code-reality/boundary/
<nt-short-sha>.db``）；本工具唯讀消費（``connect_ro`` 泛用慣例——自家
sidecar 無 WAL，走 ``immutable=1``）。

用法::

    uv run python -m code_reality.boundary <symbol> [--repo PATH] [--rs]

symbol 形態：完整（``nautilus_trader.live.LiveNode``）或裸名（``LiveNode``
——後綴匹配；LIKE 萬用字元 ``%``/``_`` 已轉義，符號語義精確）；``--rs``
反向（Rust 符號 → python 面）。同名雙宣告（如 LiveNodeBuilder native＋
wrapper）列全部邊，非報錯。
"""

import argparse
import sqlite3
from pathlib import Path

from code_reality.boundary_build import DEFAULT_OUT_DIR, nt_head_sha
from code_reality.common import assert_db_unchanged, connect_ro, db_mtime_ns


def _read_meta(db: Path) -> dict[str, str]:
    conn = connect_ro(db)
    try:
        return dict(conn.execute("SELECT key, value FROM meta"))
    except sqlite3.DatabaseError as e:
        raise AssertionError(
            f"非 boundary sidecar（讀 meta 失敗：{e}）：{db}"
            "——sidecar 目錄混入外部 .db？"
        ) from e
    finally:
        conn.close()


def _open(db: Path) -> sqlite3.Connection:
    conn = connect_ro(db)
    conn.row_factory = sqlite3.Row
    return conn


def load_sidecar(
    nt_repo: Path,
    sidecar_dir: Path = DEFAULT_OUT_DIR,
    head: str | None = None,
) -> tuple[sqlite3.Connection, dict[str, str], Path]:
    """載入 sidecar——回傳（唯讀連線, meta, db 路徑）。

    ``nt_repo`` 顯式必傳（SM-1b：不內建 repo 預設——stale 比對的 HEAD
    來源）。多檔語義（EP S2）：優先 ``_meta.nt_commit == NT 當前 HEAD``
    檔；無對應檔則 mtime 最新＋[WARN] 附重跑指引（不靜默 fallback）。
    NT repo 缺席（head 無法取得）時退 mtime 最新、不誤發 stale 警示。
    """
    dbs = sorted(sidecar_dir.glob("*.db")) if sidecar_dir.is_dir() else []
    assert dbs, (
        f"boundary sidecar 不存在：{sidecar_dir}——先跑 "
        "`uv run python -m code_reality.boundary_build`"
    )
    if head is None and nt_repo.is_dir():
        head = nt_head_sha(nt_repo)
    if head is not None:
        for db in dbs:
            try:
                meta = _read_meta(db)
            except AssertionError:
                # 外部 .db 混入目錄——容忍跳過（R7），不擋住後續正確檔
                print(f"[WARN] 非 boundary sidecar，跳過：{db}")
                continue
            if meta.get("nt_commit") == head:
                return _open(db), meta, db
    latest = max(dbs, key=lambda p: p.stat().st_mtime)
    meta = _read_meta(latest)
    if head is not None:
        print(
            f"[WARN] sidecar 落後（sidecar {meta.get('nt_commit', '?')[:8]} vs NT HEAD "
            f"{head[:8]}）——建議重跑 uv run python -m code_reality.boundary_build"
        )
    return _open(latest), meta, latest


def _like_escape(symbol: str) -> str:
    """LIKE pattern 轉義——``%``/``_`` 是萬用字元，符號匹配語義要精確。"""
    return symbol.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


def query_py(conn: sqlite3.Connection, symbol: str) -> list[sqlite3.Row]:
    """python 符號 → 邊（精確匹配；class 符號展開 method 層；裸名後綴解析）。"""
    pat = _like_escape(symbol)
    rows = conn.execute(
        "SELECT * FROM boundary_edges WHERE py_symbol = ? OR py_symbol LIKE ? "
        "ESCAPE '\\' OR py_symbol LIKE ? ESCAPE '\\' OR py_symbol LIKE ? ESCAPE '\\' "
        "ORDER BY py_symbol, rs_path, rs_line",
        (symbol, f"%.{pat}", f"{pat}.%", f"%.{pat}.%"),
    ).fetchall()
    return list(rows)


def query_rs(conn: sqlite3.Connection, symbol: str) -> list[sqlite3.Row]:
    """Rust 符號 → python 邊（``LiveNode::py_build`` 方法段或 struct 名）。"""
    rows = conn.execute(
        "SELECT * FROM boundary_edges WHERE rs_symbol = ? OR rs_symbol LIKE ? "
        "ESCAPE '\\' ORDER BY py_symbol, rs_path, rs_line",
        (symbol, f"%::{_like_escape(symbol)}"),
    ).fetchall()
    return list(rows)


def _candidates(conn: sqlite3.Connection, symbol: str) -> list[str]:
    """not-found 候選：末段子字串匹配（消歧——刻意 fuzzy，LIKE 萬用字元
    在此是特性非 bug）。EP S2 原文「同 module 前綴候選」——build review
    F3 改為末段 fuzzy（單一機制涵蓋裸名/完整符號 typo）。rs_mode 時搜
    rs_symbol（R13）。"""
    seg = symbol.rsplit(".", 1)[-1].rsplit("::", 1)[-1]
    return [
        str(r[0])
        for r in conn.execute(
            "SELECT DISTINCT py_symbol FROM boundary_edges WHERE py_symbol LIKE ? "
            "ORDER BY py_symbol LIMIT 10",
            (f"%{seg}%",),
        )
    ]


def _rs_candidates(conn: sqlite3.Connection, symbol: str) -> list[str]:
    """not-found 候選（--rs 反向）：rs_symbol 末段子字串。"""
    seg = symbol.rsplit("::", 1)[-1]
    return [
        str(r[0])
        for r in conn.execute(
            "SELECT DISTINCT rs_symbol FROM boundary_edges WHERE rs_symbol LIKE ? "
            "ORDER BY rs_symbol LIMIT 10",
            (f"%{seg}%",),
        )
    ]


def run_query(
    conn: sqlite3.Connection,
    meta: dict[str, str],
    db: Path,
    symbol: str,
    *,
    rs_mode: bool = False,
) -> None:
    """查詢＋hub_refs 式輸出（[OK]/[FAIL]/[LOG]）。not-found → SystemExit。

    mtime 防線：查詢期間 sidecar 被同 sha rebuild 覆寫 → 輸出可能基於
    陳舊快照而無警示——mtime 比對 crash 提醒重跑（immutable 連線持有
    舊 inode fd，讀取自洽但可能過時）。
    """
    m0 = db_mtime_ns(db)
    rows = query_rs(conn, symbol) if rs_mode else query_py(conn, symbol)
    candidates = (
        (_rs_candidates(conn, symbol) if rs_mode else _candidates(conn, symbol))
        if not rows
        else []
    )
    assert_db_unchanged(db, m0)
    if not rows:
        print(f"[FAIL] symbol not found: {symbol}（sidecar {db.name}）")
        for c in candidates:
            print(f"  候選: {c}")
        raise SystemExit(f"symbol not found: {symbol}")
    print(
        f"[OK] {symbol}: {len(rows)} edges"
        f"（sidecar {meta.get('nt_commit', '?')[:8]} @ {db}）"
    )
    for r in rows:
        print(
            f"  {r['py_symbol']}  {r['match_kind']}  {r['rs_path']}:{r['rs_line']}"
            f"  <- pyi {r['pyi_path']}:{r['pyi_line']}"
        )
    col = "rs_symbol" if rs_mode else "py_symbol"
    print(
        f"[LOG] sqlite3 {db} 'SELECT * FROM boundary_edges WHERE {col} LIKE \"%{symbol}%\"'"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="boundary 查詢：python 符號 → Rust 真身"
    )
    parser.add_argument(
        "symbol",
        help="python 符號（nautilus_trader.live.LiveNode 或裸名 LiveNode）；--rs 時為 Rust 符號",
    )
    parser.add_argument(
        "--repo",
        type=Path,
        required=True,
        help="sidecar 對應的 repo 根（stale 比對用；顯式必給——SM-1b）",
    )
    parser.add_argument(
        "--rs", action="store_true", help="反向查詢：Rust 符號 → python 面"
    )
    parser.add_argument("--sidecar-dir", type=Path, default=DEFAULT_OUT_DIR)
    args = parser.parse_args()

    conn, meta, db = load_sidecar(args.repo, sidecar_dir=args.sidecar_dir)
    try:
        run_query(conn, meta, db, args.symbol, rs_mode=args.rs)
    finally:
        conn.close()


if __name__ == "__main__":
    main()
