"""S2 snapshot/exclusions 單元測試——fixture sqlite 覆蓋 SM-5/6/11 的函數層。

驗證意圖提煉自評估弧 POC（git 歷史 poc/poc_crg_module_edges.py；schema 直讀已全量
驗證）；本測確保升級（stale 首選 sha、冪等、crash-only）行為正確。
真 graph.db 的整合測試見 test_snapshot_integration.py。
"""

import json
from datetime import datetime
from pathlib import Path

import pytest
from crg_db import make_crg_db, qualified
from profile_repo import write_mosaic_profile

from code_reality.common import connect_ro
from code_reality.exclusions import is_excluded
from code_reality.profile import Profile, load_profile
from code_reality.snapshot import build_snapshot, detect_stale, export_module_edges

HEAD_T = datetime.fromisoformat("2026-08-21T10:00:00+08:00")

MOSAIC_EXCLUDES = ("stubs/", "ai-analysis/", ".venv/", "snapshot/")


@pytest.fixture
def repo(tmp_path: Path) -> Path:
    (tmp_path / "mosaic_alpha" / "conditions").mkdir(parents=True)
    (tmp_path / "mosaic_alpha" / "features").mkdir(parents=True)
    write_mosaic_profile(tmp_path)
    return tmp_path


class TestExclusions:
    def test_excluded_prefixes_hit(self) -> None:
        # 條目一律目錄粒度（帶斜線）——無斜線條目會誤傷同名開頭檔（post-build F4）
        profile = Profile(exclude=MOSAIC_EXCLUDES)
        for prefix in MOSAIC_EXCLUDES:
            assert is_excluded(f"{prefix}x/x.py", profile), prefix

    def test_similar_named_file_not_caught(self) -> None:
        profile = Profile(exclude=MOSAIC_EXCLUDES)
        assert not is_excluded(".venv-setup.py", profile)
        assert not is_excluded("stubs_legacy.py", profile)

    def test_no_profile_fallback_venv_only(self) -> None:
        # generic fallback：無 profile 僅排除 .venv/（SM-1a）
        assert is_excluded(".venv/x.py", None)
        assert not is_excluded("stubs/x.py", None)

    def test_production_paths_pass(self, repo: Path) -> None:
        profile = load_profile(repo)
        for rel in (
            "mosaic_alpha/conditions/service.py",
            "tests/x/test_a.py",
            "tools/y.py",
        ):
            assert not is_excluded(rel, profile), rel


class TestExportModuleEdges:
    def test_cross_module_edges_and_files(self, repo: Path) -> None:
        db = repo / "graph.db"
        make_crg_db(
            db,
            [
                (
                    "CALLS",
                    qualified(repo, "mosaic_alpha/conditions/a.py"),
                    qualified(repo, "mosaic_alpha/features/b.py"),
                ),
                (
                    "IMPORTS_FROM",
                    qualified(repo, "mosaic_alpha/conditions/a.py"),
                    qualified(repo, "mosaic_alpha/conditions/c.py"),
                ),  # 同模組 → 不入邊
                (
                    "CALLS",
                    qualified(repo, "mosaic_alpha/conditions/a.py"),
                    "/elsewhere/stubs/s.py::X",
                ),
            ],
        )
        conn = connect_ro(db)
        result = export_module_edges(conn, repo, load_profile(repo))
        conn.close()
        assert result.module_edges == [
            ["mosaic_alpha/conditions", "mosaic_alpha/features", "CALLS"]
        ]
        # 檔案集含兩端 repo 內檔案（同模組邊的檔案仍計入結構覆蓋面）
        assert "mosaic_alpha/features/b.py" in result.files
        assert "mosaic_alpha/conditions/c.py" in result.files

    def test_outside_repo_skipped(self, repo: Path) -> None:
        db = repo / "graph.db"
        make_crg_db(db, [("CALLS", "/elsewhere/a.py::X", "/elsewhere/b.py::Y")])
        conn = connect_ro(db)
        result = export_module_edges(conn, repo, load_profile(repo))
        conn.close()
        assert result.module_edges == []
        assert result.files == []


class TestDetectStale:
    def test_fresh_by_sha(self) -> None:
        assert detect_stale({"git_head_sha": "abc123"}, "abc123", HEAD_T) is None

    def test_stale_by_sha(self) -> None:
        reason = detect_stale({"git_head_sha": "old"}, "new", HEAD_T)
        assert reason is not None and "old" in reason

    def test_fallback_last_updated_fresh(self) -> None:
        # 缺 git_head_sha：last_updated（naive→假設 local）晚於 HEAD commit → 新鮮
        assert (
            detect_stale({"last_updated": "2026-08-21T22:16:00"}, None, HEAD_T) is None
        )

    def test_fallback_stale_by_time(self) -> None:
        reason = detect_stale({"last_updated": "2026-07-01T00:00:00"}, None, HEAD_T)
        assert reason is not None

    def test_fallback_stale_by_mtime(self) -> None:
        old_mtime = datetime.fromisoformat("2026-07-01T00:00:00+08:00")
        reason = detect_stale({}, None, HEAD_T, db_mtime=old_mtime)
        assert reason is not None


class TestCrashOnly:
    def test_missing_db_raises_with_hint(self, repo: Path) -> None:
        with pytest.raises(AssertionError, match="code-review-graph"):
            build_snapshot(repo)  # tmp repo 無 .code-review-graph/ → SM-11


class TestBuildSnapshotIdempotent:
    def test_same_commit_overwrites(
        self, repo: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        db = repo / ".code-review-graph" / "graph.db"
        db.parent.mkdir(parents=True)
        make_crg_db(
            db,
            [
                (
                    "CALLS",
                    qualified(repo, "mosaic_alpha/conditions/a.py"),
                    qualified(repo, "mosaic_alpha/features/b.py"),
                )
            ],
            {"git_head_sha": "deadbeef", "last_build_type": "incremental"},
        )
        monkeypatch.setattr("code_reality.snapshot.head_sha", lambda root: "deadbeef")
        monkeypatch.setattr(
            "code_reality.snapshot.head_commit_time", lambda root: HEAD_T
        )
        monkeypatch.setattr(
            "code_reality.snapshot._assert_git_root", lambda root: None
        )  # tmp fixture 非 git root——錨定驗證另由 integration 測
        out_dir = repo / "out"
        for _ in range(2):  # 同 commit 跑兩次 → 同檔名覆寫（SM-5 冪等）
            build_snapshot(repo, label="ep-test").write(out_dir)
        path = out_dir / f"{repo.name}-deadbeef.json"
        assert path.exists()
        data = json.loads(path.read_text())
        assert data["_meta"]["commit"] == "deadbeef"
        assert data["_meta"]["label"] == "ep-test"
        assert data["module_edges"] == [
            ["mosaic_alpha/conditions", "mosaic_alpha/features", "CALLS"]
        ]
