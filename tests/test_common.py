"""common.py 共用 helper 測試——anchor_pattern／repo_relative／connect_ro。

module_of 已隨 profile 引擎化遷至 code_reality.profile（測試見
test_profile.py——模組語義含根檔案歸屬在那裡釘住）。
"""

import re
import sqlite3
from pathlib import Path

import pytest

from code_reality.common import (
    anchor_pattern,
    assert_db_unchanged,
    connect_ro,
    db_mtime_ns,
    repo_relative,
)


class TestAnchorPattern:
    """tour-contract EP S1（SM-1/2）——literal-ish 行匹配契約，與 EP1 編號空間獨立。"""

    def test_literal_ish_special_chars(self) -> None:
        line = "    x = re.match(r'^([a-z]+)\\.[0-9]{2}', s)  # a|b?c"
        pat = anchor_pattern(line)
        assert pat.startswith(r"^[ \t]*") and pat.endswith(r"[ \t]*$")
        re.compile(pat)  # 恆合法（播放端 new RegExp 前提）
        assert re.search(pat, line, re.MULTILINE)  # 命中原行
        assert not re.search(pat, "y = 1", re.MULTILINE)  # 特殊字元群不誤配

    def test_indent_tolerance(self) -> None:
        """SM-2：錨行日後被加縮排（搬進 if 區塊）仍命中——行內縮排容差。"""
        pat = anchor_pattern("def main():")
        assert pat.startswith(r"^[ \t]*")
        assert re.search(pat, "def main():\n", re.MULTILINE)
        assert re.search(pat, "        def main():\n", re.MULTILINE)
        assert re.search(pat, "if x:\n    def main():\n", re.MULTILINE)

    def test_no_blank_line_swallow(self) -> None:
        """build review F1：縮排容差用 ``[ \\t]`` 非 ``\\s``——後者含 ``\\n``，
        錨行上方空行會讓最早命中落在空行（播放端把正確行號校正上去）。"""
        pat = anchor_pattern("def main():")
        content = "import x\n\n\ndef main():\n    pass\n"  # def 在第 4 行
        m = re.search(pat, content, re.MULTILINE)
        assert m is not None
        assert content[: m.start()].count("\n") == 3  # 命中第 4 行非上方空行

    def test_prefix_collision_guarded(self) -> None:
        """post-build F-2：行尾錨防前綻碰撞——``x = 1`` 不得命中 ``x = 12``
        行；尾隨空白容差錨行自身。"""
        pat = anchor_pattern("x = 1")
        assert re.search(pat, "x = 12\n", re.MULTILINE) is None
        assert re.search(pat, "x = 1\n", re.MULTILINE) is not None
        assert re.search(pat, "x = 1  \n", re.MULTILINE) is not None

    def test_strip_normalizes(self) -> None:
        assert anchor_pattern("  hi  ") == anchor_pattern("hi")


class TestRepoRelative:
    def test_inside(self, tmp_path: Path) -> None:
        assert repo_relative(str(tmp_path / "a/b.py"), tmp_path) == "a/b.py"

    def test_outside_none(self, tmp_path: Path) -> None:
        assert repo_relative("/elsewhere/x.py", tmp_path) is None


class TestConnectRo:
    def test_clean_db_immutable_path(self, tmp_path: Path) -> None:
        db = tmp_path / "g.db"
        w = sqlite3.connect(db)
        w.execute("CREATE TABLE t(x)")
        w.execute("INSERT INTO t VALUES (1)")
        w.commit()
        w.close()  # 非 WAL 模式：close 後無 -wal → immutable 路徑
        r = connect_ro(db)
        assert r.execute("SELECT COUNT(*) FROM t").fetchone()[0] == 1
        r.close()

    def test_wal_active_writer_fallback_sees_wal_data(self, tmp_path: Path) -> None:
        """post-build F-WAL：CRG MCP 常駐讓 -wal 恆存在——mode=ro fallback
        必須讀得到 WAL-committed 資料（immutable 會靜默舊讀）。"""
        db = tmp_path / "g.db"
        w = sqlite3.connect(db)
        w.execute("PRAGMA journal_mode=WAL")
        w.execute("CREATE TABLE t(x)")
        w.execute("INSERT INTO t VALUES (1)")
        w.commit()
        assert (tmp_path / "g.db-wal").exists()  # writer 存活、未 checkpoint
        r = connect_ro(db)
        assert r.execute("SELECT COUNT(*) FROM t").fetchone()[0] == 1
        r.close()
        w.close()

    def test_db_unchanged_guard(self, tmp_path: Path) -> None:
        db = tmp_path / "g.db"
        db.write_text("x")
        assert_db_unchanged(db, db_mtime_ns(db))
        db.write_text("y")
        with pytest.raises(AssertionError, match="撕裂"):
            assert_db_unchanged(db, db_mtime_ns(db) - 1)
