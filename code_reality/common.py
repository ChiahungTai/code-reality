"""code_reality 工具鏈共用慣例——_meta 區塊、repo 相對化、CRG db 連線。

全工具輸出 JSON 共用 `_meta` 區塊（commit/timestamp/tool 錨定），定義
見 ep-code-reality-thin-layer 段落劃分原則；CRG graph.db 的唯讀連線慣例
（immutable=1）單一源（snapshot/hub_refs 共用）。
"""

import re
import sqlite3
import subprocess
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

# 檔/module 級結構邊三 kind（snapshot module-edge 與 graph_csv 檔圖共用契約）；
# REFERENCES/TESTED_BY/CONTAINS 屬檔內成員/測試接線，不進結構邊語義——
# transition「無結構變化」結論僅及此三 kind（post-build F3）
EDGE_KINDS = ("IMPORTS_FROM", "CALLS", "INHERITS")


def anchor_pattern(line: str) -> str:
    """錨行 → literal-ish 行匹配 regex（``^[ \\t]*``＋trim 後全 escape＋
    ``[ \\t]*$`` 行尾錨）。

    播放端以 ``new RegExp(pattern, "gm")`` 逐行匹配取命中（codetour
    player/anchor.ts）——寬鬆 regex 的誤配風險由資料側控制：單行、行首錨定、
    特殊字元全 escape、**行內**縮排容差。縮排用 ``[ \\t]`` 而非 ``\\s``：
    ``\\s`` 含 ``\\n``，錨行上方有空行時 match 起點會落在空行、把正確行號
    「校正」上去（build review F1，node 實證）。行尾錨防前綻碰撞——
    ``x = 1`` 不得命中更早出現的 ``x = 12`` 行（post-build F-2），尾隨
    ``[ \\t]*`` 容差錨行自身尾空白。splitlines 切行語義由呼叫端保證與
    git/graph 行計數一致（``split("\\n")``）。re.escape 產物（含 ``\\ ``
    空格轉義）依賴 JS 非 ``u`` 模式——fork 端 try/catch 退化 no-match，
    若加 ``u`` flag 需先改此契約。發射端省略條件（空錨行/不在 after
    commit/binary）由兩 generator 自行判定。
    """
    return r"^[ \t]*" + re.escape(line.strip()) + r"[ \t]*$"


def repo_relative(path: str, repo_root: Path) -> str | None:
    """絕對路徑 → repo 相對；repo 外回 None。"""
    try:
        return str(Path(path).relative_to(repo_root.resolve()))
    except ValueError:
        return None


def graph_db_path(repo_root: Path) -> Path:
    """CRG graph.db 慣例路徑（repo root 下 .code-review-graph/）。"""
    return repo_root / ".code-review-graph" / "graph.db"


def connect_ro(db_path: Path) -> sqlite3.Connection:
    """唯讀連線——無 WAL 用 ``immutable=1``；有 WAL fallback ``mode=ro``。

    實證語義（2026-08-21 三方審查＋08-22 MCP 共存實測）：
    - ``immutable=1`` 連線**完全看不到 WAL-committed 資料**（拿到舊快照、
      無任何錯誤——靜默舊讀）——只適用無 ``-wal`` 的 clean db
    - ``mode=ro``：writer 存活（shm 在）時可開且看得到 WAL-committed 資料；
      hot-WAL-無-shm（writer crash 後）才失敗
    - CRG MCP server 常駐（ZCode 預設載入）讓 ``-wal`` 恆存在——「-wal
      存在即拒讀」會讓任何 MCP session 都不能用工具（post-build F-WAL
      實證），故改 fallback；crash 殘餘場景包成附修法的錯誤
    """
    wal = db_path.parent / (db_path.name + "-wal")
    if not wal.exists():
        return sqlite3.connect(f"file:{db_path}?immutable=1", uri=True)
    try:
        return sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    except sqlite3.OperationalError as e:
        raise AssertionError(
            f"{db_path.name} 有 {wal.name} 但 mode=ro 開啟失敗（writer crash 後"
            " hot-WAL-無-shm）——先 `uvx code-review-graph status` 或 build 後重跑"
        ) from e


def db_mtime_ns(db_path: Path) -> int:
    return db_path.stat().st_mtime_ns


def assert_db_unchanged(db_path: Path, mtime_before: int) -> None:
    """讀取期間主檔被改寫（build/update/checkpoint 併發）→ crash。

    immutable 連線的殘餘窗口防線（post-build F1）：連線後 writer 才啟動的
    撕裂讀，靠讀取前後 mtime 比對兜底。
    """
    assert db_path.stat().st_mtime_ns == mtime_before, (
        f"{db_path.name} 在讀取期間被改寫（build/update 併發）——"
        "快照可能撕裂，重跑本工具"
    )


def make_meta(
    tool: str, repo_root: Path, *, commit: str | None = None, **extra: Any
) -> dict[str, Any]:
    """產出 commit 錨定的 _meta 區塊；extra 併入工具特有欄位。

    commit 可注入（呼叫端已取得 sha 時免重複跑 git；測試注入用）。
    """
    repo_root = repo_root.resolve()
    if commit is None:
        commit = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    return {
        "repo": repo_root.name,
        "commit": commit,
        "created_at": datetime.now(UTC).isoformat(),
        "tool": tool,
        **extra,
    }
