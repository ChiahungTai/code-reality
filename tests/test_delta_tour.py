"""S1 delta_tour 單元測試——transition＋git hunk 錨 → CodeTour .tour（SM-1/2/3）。

POC 對照組＝.agent-tmp/ui/build_delta_tour.py（22 步實證，搬運忠實度由
integration 測試對真 repo 歷史釘住）；此處釘行為語義：
- 步序：弧總覽 → 新檔（＋）→ 修改（M＋hunk 錨）→ 刪檔（−＋不可跳明示）
- SM-2：無 ep_claims → 「claims: NONE」＋實際變動模組清單
- SM-3：刪檔步無 :0 無效錨
"""

import json
import re
import subprocess
import sys
from datetime import date
from pathlib import Path
from typing import Any

import pytest
from profile_repo import write_mosaic_profile

from code_reality.common import anchor_pattern
from code_reality.delta_tour import (
    build_tour,
    cleanup_expired,
    first_change_lines,
    kebab,
    local_today,
)
from code_reality.delta_tour import main as delta_main


def _git(repo: Path, *args: str) -> str:
    r = subprocess.run(
        ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
    )
    return r.stdout.strip()


def make_repo(tmp_path: Path) -> tuple[Path, str, str]:
    """兩 commit mini repo：B 改中段行＋新增模組檔＋刪檔。"""
    repo = tmp_path / "repo"
    (repo / "mosaic_alpha" / "domain").mkdir(parents=True)
    (repo / "mosaic_alpha" / "old").mkdir()
    write_mosaic_profile(repo)
    _git(repo, "init", "-q")
    (repo / "mosaic_alpha" / "domain" / "mod_a.py").write_text(
        "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n"
    )
    (repo / "mosaic_alpha" / "old" / "gone.py").write_text("bye\n")
    (repo / "README.md").write_text("readme\n")
    _git(repo, "add", "-A")
    _git(repo, "commit", "-qm", "A")
    sha_a = _git(repo, "rev-parse", "HEAD")
    (repo / "mosaic_alpha" / "domain" / "mod_a.py").write_text(
        "l1\nl2\nl3\nl4\nCHANGED\nl6\nl7\nl8\n"
    )
    (repo / "mosaic_alpha" / "newpkg").mkdir()
    (repo / "mosaic_alpha" / "newpkg" / "new_mod.py").write_text("hello\n")
    (repo / "mosaic_alpha" / "old" / "gone.py").unlink()
    _git(repo, "add", "-A")
    _git(repo, "commit", "-qm", "B")
    sha_b = _git(repo, "rev-parse", "HEAD")
    return repo, sha_a, sha_b


def trans_data(sha_a: str, sha_b: str, **over: object) -> dict[str, Any]:
    d: dict[str, Any] = {
        "_meta": {"before": sha_a, "after": sha_b, "repo": "r"},
        "added": [],
        "removed": [],
        "reversed": [],
        "changed_modules": [
            "mosaic_alpha/domain",
            "mosaic_alpha/newpkg",
            "mosaic_alpha/old",
        ],
        "new_files": ["mosaic_alpha/newpkg/new_mod.py"],
        "gone_files": ["mosaic_alpha/old/gone.py"],
    }
    d.update(over)
    return d


class TestFirstChangeLines:
    def test_mid_file_change_hunk_line(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        jump, added = first_change_lines(repo, a, b)
        assert jump["mosaic_alpha/domain/mod_a.py"] == 5
        assert jump["mosaic_alpha/newpkg/new_mod.py"] == 1
        assert added == {"mosaic_alpha/newpkg/new_mod.py"}

    def test_leading_deletion_anchors_later_hunk(self, tmp_path: Path) -> None:
        """首 hunk 純刪除（+0,0）不得讓整檔從 tour 消失——錨後續 hunk。"""
        repo = tmp_path / "repo3"
        (repo / "m").mkdir(parents=True)
        _git(repo, "init", "-q")
        (repo / "m" / "f.py").write_text("".join(f"l{i}\n" for i in range(1, 12)))
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        content = [f"l{i}\n" for i in range(1, 12)]
        del content[0:3]  # 刪 l1-l3
        content[3] = "CHANGED\n"  # 原 l7 → 新位置 4
        (repo / "m" / "f.py").write_text("".join(content))
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        jump, _ = first_change_lines(repo, sha_a, sha_b)
        assert jump["m/f.py"] == 4

    def test_anchors_all_positive(self, tmp_path: Path) -> None:
        """純刪除 hunk 也不產生 :0 無效錨（EP：+0 hunk 濾除）。"""
        repo = tmp_path / "repo2"
        (repo / "m").mkdir(parents=True)
        _git(repo, "init", "-q")
        (repo / "m" / "f.py").write_text("a\nb\nc\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        (repo / "m" / "f.py").write_text("a\n")  # 刪 b、c
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        jump, _ = first_change_lines(repo, sha_a, sha_b)
        assert all(line >= 1 for line in jump.values())

    def test_bad_refs_crash_loud(self, tmp_path: Path) -> None:
        """before/after 非本 repo ref → crash（非靜默 {}——錯 repo 使用是輸入錯誤）。"""
        repo, _, _ = make_repo(tmp_path)
        with pytest.raises(subprocess.CalledProcessError):
            first_change_lines(repo, "0" * 40, "1" * 40)


class TestBuildTour:
    def test_step_order_and_tags(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        titles = [s["title"] for s in tour["steps"]]
        assert tour["steps"][0]["title"].startswith("弧總覽")
        assert any("＋新檔 new_mod.py" in t for t in titles)
        assert any(t.startswith("M修改 mod_a.py") for t in titles)
        assert any("−刪檔" in t for t in titles)
        i_new = next(i for i, t in enumerate(titles) if "＋新檔" in t)
        i_mod = next(i for i, t in enumerate(titles) if t.startswith("M修改"))
        i_gone = next(i for i, t in enumerate(titles) if "−刪檔" in t)
        assert 0 < i_new < i_mod < i_gone
        # Deletions collapse (mosaic dogfood bug 2) — names live in description.
        assert "gone.py" in tour["steps"][i_gone]["description"]

    def test_modified_step_anchored_at_first_hunk(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        mod = next(s for s in tour["steps"] if s["title"].startswith("M修改"))
        assert mod["file"] == "mosaic_alpha/domain/mod_a.py"
        assert mod["line"] == 5
        assert "第 5 行" in mod["description"]

    def test_claims_none_shows_changed_modules(self, tmp_path: Path) -> None:
        """SM-2：無 ep_claims → claims NONE＋實際變動模組清單（不誤導）。"""
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        desc = tour["steps"][0]["description"]
        assert "NONE" in desc
        assert "mosaic_alpha/domain" in desc

    def test_claims_present_tags_steps(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        data = trans_data(
            a,
            b,
            ep_claims={
                "claims": ["mosaic_alpha/newpkg"],
                "claims_none": False,
                "claimed_and_changed": ["mosaic_alpha/newpkg"],
                "changed_not_claimed": ["mosaic_alpha/domain", "mosaic_alpha/old"],
                "claimed_not_changed": [],
            },
        )
        tour = build_tour(data, repo)
        titles = [s["title"] for s in tour["steps"]]
        assert any("✓宣稱命中" in t for t in titles)
        assert any("⚠EP沒提卻變了" in t for t in titles)

    def test_gone_step_no_invalid_anchor(self, tmp_path: Path) -> None:
        """SM-3：刪檔步明示不可跳、line 不為 0。"""
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        gone = next(s for s in tour["steps"] if "−刪檔" in s["title"])
        assert gone["line"] == 1
        assert "無法跳轉" in gone["description"]

    def test_summary_anchor_prefers_ep_file(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        ep = tmp_path / "ep.md"
        ep.write_text("# EP\n")
        tour = build_tour(trans_data(a, b), repo, ep_path=ep)
        assert tour["steps"][0]["file"] == str(ep)

    def test_summary_anchor_fallback_new_file(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        assert tour["steps"][0]["file"] == "mosaic_alpha/newpkg/new_mod.py"

    def test_git_added_without_snapshot_entry_tagged_new(self, tmp_path: Path) -> None:
        """git-A 但不入 snapshot files（新檔無 module edge）→ ＋新檔不誤標 M。"""
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b, new_files=[], gone_files=[]), repo)
        titles = [s["title"] for s in tour["steps"]]
        assert any("＋新檔 new_mod.py" in t for t in titles)
        assert not any(t.startswith("M修改 new_mod.py") for t in titles)


class TestPatternEmission:
    """tour-contract EP S1——pattern 取 after commit 錨行；三類省略條件全釘：
    空錨行／不在 after commit（刪檔步、untracked EP）／binary 檔。"""

    def test_modified_step_pattern_from_after_commit(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        mod = next(s for s in tour["steps"] if s["title"].startswith("M修改"))
        assert mod["pattern"] == anchor_pattern("CHANGED")  # B 版第 5 行內容
        # 抗縮排：日後加縮排仍命中
        assert re.search(mod["pattern"], "l1\n    CHANGED\nl3\n", re.MULTILINE)

    def test_new_file_and_overview_pattern_line1(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        new = next(s for s in tour["steps"] if "＋新檔" in s["title"])
        assert new["pattern"] == anchor_pattern("hello")
        # 無 --ep 時總覽步錨 new_files[0]（after commit 內）→ 同樣發射
        assert tour["steps"][0]["pattern"] == anchor_pattern("hello")

    def test_gone_and_untracked_ep_steps_no_pattern(self, tmp_path: Path) -> None:
        """省略條件②：刪檔步（git show 128）＋untracked EP 錨總覽步（混兩源）不發射。"""
        repo, a, b = make_repo(tmp_path)
        ep = tmp_path / "ep.md"
        ep.write_text("# EP\n")
        tour = build_tour(trans_data(a, b), repo, ep_path=ep)
        gone = next(s for s in tour["steps"] if "−刪檔" in s["title"])
        assert "pattern" not in gone
        assert "pattern" not in tour["steps"][0]

    def test_blank_anchor_line_no_pattern(self, tmp_path: Path) -> None:
        """省略條件①（delta 側）：hunk 錨行是插入的空行 → 不發射。"""
        repo = tmp_path / "repo_blank"
        (repo / "m").mkdir(parents=True)
        _git(repo, "init", "-q")
        (repo / "m" / "f.py").write_text("l1\nl2\nl3\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        (repo / "m" / "f.py").write_text("l1\nl2\n\nl3\n")  # 插入空行於第 3 行
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        tour = build_tour(trans_data(sha_a, sha_b), repo)
        mod = next(s for s in tour["steps"] if s["title"].startswith("M修改"))
        assert mod["line"] == 3  # hunk 錨在插入的空行
        assert "pattern" not in mod

    def test_binary_step_no_pattern_no_crash(self, tmp_path: Path) -> None:
        """省略條件③：binary 檔 git show bytes 不得 UnicodeDecodeError 全線
        crash（build review F2 真實 blob 重現）——decode 失敗＝不發射。"""
        repo = tmp_path / "repo_bin"
        (repo / "m").mkdir(parents=True)
        _git(repo, "init", "-q")
        (repo / "m" / "f.py").write_text("a\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        (repo / "m" / "img.png").write_bytes(b"\x89PNG\r\n\x00\x1a\nbinary")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        data = trans_data(sha_a, sha_b, new_files=["m/img.png"], gone_files=[])
        tour = build_tour(data, repo)
        bin_step = next(s for s in tour["steps"] if s["file"] == "m/img.png")
        assert bin_step["line"] == 1  # binary diff 退 line 1（漏檔比弱錨糟）
        assert "pattern" not in bin_step


class TestTaskTitleAndKebab:
    """tour-contract EP S2（SM-6）——tour title＝``<task> 變更導覽``（hash 出
    title 進 description 開頭）；弧總覽「步」title 保留 hash（EP review F6）。"""

    def test_title_task_no_hash_overview_step_keeps_hash(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo, task="my-task")
        assert tour["title"] == "my-task 變更導覽"
        assert tour["steps"][0]["title"] == f"弧總覽：{a[:8]} → {b[:8]}"
        assert tour["description"].startswith(f"before `{a[:8]}` → after `{b[:8]}`")

    def test_default_task_review(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        assert tour["title"] == "review 變更導覽"

    def test_missing_ep_anchor_new_file_emits_pattern(self, tmp_path: Path) -> None:
        """re-review F1：EP 檔不在 working tree（路徑恰等於本弧新檔）→ 總覽
        錨退 new_files[0]（after-commit 檔）——gating 用 exists() 判準仍應發射。
        ep_path 用 tmp repo 絕對路徑：相對路徑會探測 pytest CWD（post-build F-4）。"""
        ep_rel = "mosaic_alpha/newpkg/new_mod.py"
        repo, a, b = make_repo(tmp_path)
        (repo / ep_rel).unlink()  # working tree 刪除——gating 邊界
        tour = build_tour(trans_data(a, b), repo, ep_path=repo / ep_rel)
        assert tour["steps"][0]["file"] == ep_rel  # fallback 錨＝new_files[0]
        assert tour["steps"][0]["pattern"] == anchor_pattern("hello")

    def test_kebab_ascii(self) -> None:
        assert kebab("ep-code-reality-ui") == "ep-code-reality-ui"
        assert kebab("My Fancy EP") == "my-fancy-ep"
        assert kebab("EP_2026") == "ep-2026"
        assert kebab("中文") == ""  # 純非 ASCII → 空（main 端 or DEFAULT_TASK 接手）


class TestCleanupExpired:
    """tour-contract EP S2（SM-7）——>7 天 delta tour 清理；非日期命名不動。"""

    def test_only_expired_removed(self, tmp_path: Path) -> None:
        out = tmp_path / "delta"
        out.mkdir()
        old = out / "2026-08-14-old-task.tour"  # 8 天前（>7 → 刪）
        yday = out / "2026-08-21-y.tour"
        today_f = out / "2026-08-22-z.tour"
        keep = out / "keep-me.tour"
        date_dir = out / "2026-08-14-not-a-file"  # 日期命名的目錄（build review F4）
        dated_note = out / "2026-08-01-notes.md"  # 日期前綴非 tour 檔（post-build F-3）
        for p in (old, yday, today_f, keep, dated_note):
            p.write_text("{}")
        date_dir.mkdir()
        removed = cleanup_expired(out, today=date(2026, 8, 22))
        assert removed == 1
        assert not old.exists()
        assert yday.exists() and today_f.exists() and keep.exists()
        assert date_dir.exists()  # 目錄不 unlink（會炸）也不誤刪
        assert dated_note.exists()  # 只清 .tour——日期前綴手作檔不動

    def test_boundary_seven_days_kept(self, tmp_path: Path) -> None:
        """恰好 7 天前＝仍在窗口（嚴格 >7 才刪）。"""
        out = tmp_path / "delta"
        out.mkdir()
        edge = out / "2026-08-15-edge.tour"
        edge.write_text("{}")
        assert cleanup_expired(out, today=date(2026, 8, 22)) == 0
        assert edge.exists()

    def test_missing_dir_noop(self, tmp_path: Path) -> None:
        assert cleanup_expired(tmp_path / "nope") == 0


class TestRangeTruthSteps:
    """B2 fix (mosaic dogfood 2026-08-25) — step set single-sourced from the
    claimed git range: snapshot-pair file drift cannot pollute steps;
    deletions collapse to one summary step; renames are walkable."""

    def test_snapshot_gone_pollution_not_in_steps(self, tmp_path: Path) -> None:
        """Bug 2 core: gone_files polluted by out-of-range snapshot drift
        (older-commit deletions) must not become steps."""
        repo, a, b = make_repo(tmp_path)
        data = trans_data(
            a,
            b,
            gone_files=["ai-analysis/_archive/old.md", "mosaic_alpha/old/gone.py"],
        )
        tour = build_tour(data, repo)
        assert "_archive" not in json.dumps(tour, ensure_ascii=False)
        assert any("−刪檔" in s["title"] for s in tour["steps"])  # real deletion stays

    def test_deletions_collapsed_single_step(self, tmp_path: Path) -> None:
        """Unjumpable deletions do not expand into dead steps — one summary."""
        repo = tmp_path / "repo_del2"
        (repo / "mosaic_alpha" / "old").mkdir(parents=True)
        write_mosaic_profile(repo)
        _git(repo, "init", "-q")
        for n in ("a.py", "b.py"):
            (repo / "mosaic_alpha" / "old" / n).write_text("x\n")
        (repo / "mosaic_alpha" / "keep.py").write_text("l1\nl2\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        (repo / "mosaic_alpha" / "old" / "a.py").unlink()
        (repo / "mosaic_alpha" / "old" / "b.py").unlink()
        (repo / "mosaic_alpha" / "keep.py").write_text("l1\nCHANGED\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        data = trans_data(
            sha_a,
            sha_b,
            new_files=[],
            gone_files=["mosaic_alpha/old/a.py", "mosaic_alpha/old/b.py"],
        )
        tour = build_tour(data, repo)
        gone_steps = [s for s in tour["steps"] if "−刪檔" in s["title"]]
        assert len(gone_steps) == 1
        desc = gone_steps[0]["description"]
        assert "old/a.py" in desc and "old/b.py" in desc
        assert "無法跳轉" in desc
        assert "pattern" not in gone_steps[0]

    def test_rename_step_anchors_new_path(self, tmp_path: Path) -> None:
        """Renames (invisible under the old AM filter) become walkable steps
        anchored on the new path's first declaration."""
        repo = tmp_path / "repo_rn"
        (repo / "mosaic_alpha").mkdir(parents=True)
        write_mosaic_profile(repo)
        _git(repo, "init", "-q")
        (repo / "mosaic_alpha" / "orig.py").write_text(
            "l1\n# Copyright (c) 2026\nl3\ndef main():\n    pass\n"
        )
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        _git(repo, "mv", "mosaic_alpha/orig.py", "mosaic_alpha/renamed.py")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        tour = build_tour(trans_data(sha_a, sha_b, new_files=[], gone_files=[]), repo)
        # title-based selection: the overview step is anchored to the first
        # added file too (line 1) — file-based next() would grab it instead
        rn = next(s for s in tour["steps"] if "→改名" in s["title"])
        assert rn["line"] == 4  # first declaration, not the copyright header
        assert "orig.py" in rn["description"]

    def test_overview_counts_match_steps(self, tmp_path: Path) -> None:
        """Counts are derived from the same sets as steps (by construction
        consistent — kills the old 3-vs-5 new-file mismatch)."""
        repo, a, b = make_repo(tmp_path)
        tour = build_tour(trans_data(a, b), repo)
        desc = tour["steps"][0]["description"]
        titles = [s["title"] for s in tour["steps"]]
        assert sum("＋新檔" in t for t in titles) == 1 and "1 新檔" in desc
        assert sum(t.startswith("M修改") for t in titles) == 1 and "1 修改" in desc
        assert sum("−刪檔" in t for t in titles) == 1 and "1 刪檔" in desc


class TestClaimsThreeState:
    """B1 fix — claims three-state: ⚠ only in the compared state. Extraction
    failure (no profile / no mention / zero-hit guard) degrades to
    not-compared instead of mass "EP didn't mention this" accusations."""

    @staticmethod
    def _claims_data(tmp_path: Path, **over: object) -> tuple[Path, dict[str, Any]]:
        repo, a, b = make_repo(tmp_path)
        base: dict[str, Any] = {
            "claims": [],
            "claims_none": True,
            "claimed_and_changed": [],
            "changed_not_claimed": [
                "mosaic_alpha/domain",
                "mosaic_alpha/newpkg",
                "mosaic_alpha/old",
            ],
            "claimed_not_changed": [],
        }
        base.update(over)
        return repo, trans_data(a, b, ep_claims=base)

    def test_claims_none_state_no_false_warning(self, tmp_path: Path) -> None:
        repo, data = self._claims_data(tmp_path)
        tour = build_tour(data, repo)
        assert not any("⚠" in s["title"] for s in tour["steps"])
        assert not any("✓宣稱命中" in s["title"] for s in tour["steps"])
        assert "未比對" in tour["steps"][0]["description"]

    def test_zero_hit_degrades_to_not_compared(
        self, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        repo, data = self._claims_data(
            tmp_path, claims_none=False, claims=["mosaic_alpha/ghost"]
        )
        tour = build_tour(data, repo)
        assert not any("⚠" in s["title"] for s in tour["steps"])
        assert "未比對" in tour["steps"][0]["description"]
        assert "WARN" in capsys.readouterr().err

    def test_partial_hit_still_compares(self, tmp_path: Path) -> None:
        repo, data = self._claims_data(
            tmp_path,
            claims_none=False,
            claims=["mosaic_alpha/domain", "mosaic_alpha/ghost"],
            claimed_and_changed=["mosaic_alpha/domain"],
        )
        tour = build_tour(data, repo)
        assert any("✓宣稱命中" in s["title"] for s in tour["steps"])
        assert any("⚠EP沒提卻變了" in s["title"] for s in tour["steps"])


class TestNewFileDeclAnchor:
    """B3 fix — new-file anchors land on the first declaration line, not the
    copyright header."""

    def test_code_file_anchors_first_decl(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo_decl"
        (repo / "mosaic_alpha").mkdir(parents=True)
        write_mosaic_profile(repo)
        _git(repo, "init", "-q")
        (repo / "mosaic_alpha" / "s.py").write_text("l1\n")
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "A")
        sha_a = _git(repo, "rev-parse", "HEAD")
        (repo / "mosaic_alpha" / "n.py").write_text(
            "# Copyright (c) 2026 MIT\n# see LICENSE\n\ndef main():\n    pass\n"
        )
        _git(repo, "add", "-A")
        _git(repo, "commit", "-qm", "B")
        sha_b = _git(repo, "rev-parse", "HEAD")
        tour = build_tour(
            trans_data(sha_a, sha_b, new_files=["mosaic_alpha/n.py"], gone_files=[]),
            repo,
        )
        new = next(s for s in tour["steps"] if "＋新檔 n.py" in s["title"])
        assert new["line"] == 4
        assert new["pattern"] == anchor_pattern("def main():")


class TestDescriptionSemantics:
    """B3 fix — descriptions carry the range commit subject as the cheapest
    mechanical 'why'."""

    def test_modified_step_carries_commit_subject(self, tmp_path: Path) -> None:
        repo, a, b = make_repo(tmp_path)  # commit subjects "A" / "B"
        tour = build_tour(trans_data(a, b), repo)
        mod = next(s for s in tour["steps"] if s["title"].startswith("M修改"))
        assert "commit: B" in mod["description"]


class TestCli:
    @staticmethod
    def _snapshot_pair(tmp_path: Path, sha_a: str, sha_b: str) -> tuple[Path, Path]:
        snap: dict[str, Any] = {
            "_meta": {"repo": "repo", "commit": "", "created_at": "t", "tool": "t"},
            "files": [
                "mosaic_alpha/domain/mod_a.py",
                "mosaic_alpha/old/gone.py",
                "README.md",
            ],
            "module_edges": [],
        }
        sa, sb = tmp_path / "a.json", tmp_path / "b.json"
        snap["_meta"]["commit"] = sha_a
        sa.write_text(json.dumps(snap))
        snap["files"] = [
            "mosaic_alpha/domain/mod_a.py",
            "mosaic_alpha/newpkg/new_mod.py",
            "README.md",
        ]
        snap["_meta"]["commit"] = sha_b
        sb.write_text(json.dumps(snap))
        return sa, sb

    def test_end_to_end_produces_tour(self, tmp_path: Path) -> None:
        """CLI 全鏈：snapshot sidecar ×2 → ``<date>-<task>.tour``（--task 釘住）。"""
        repo, a, b = make_repo(tmp_path)
        sa, sb = self._snapshot_pair(tmp_path, a, b)
        out = tmp_path / "out"
        r = subprocess.run(
            [
                sys.executable,
                "-m",
                "code_reality.delta_tour",
                str(sa),
                str(sb),
                "--repo",
                str(repo),
                "--out-dir",
                str(out),
                "--task",
                "demo",
            ],
            cwd=tmp_path,
            capture_output=True,
            text=True,
            check=False,
        )
        assert r.returncode == 0, r.stderr
        tour_path = out / f"{local_today():%Y-%m-%d}-demo.tour"
        assert tour_path.exists()
        tour = json.loads(tour_path.read_text())
        assert tour["title"] == "demo 變更導覽"
        assert tour["steps"][0]["title"].startswith("弧總覽")
        # 總覽＋新檔＋修改（mod_a 在 git diff、非 new/gone）＋刪檔
        assert len(tour["steps"]) == 4

    def test_default_out_dir_and_task_from_ep_stem(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        """tour-contract EP S2（SM-6/7/8，EP review F7）——CLI 級：預設
        ``.tours/delta/``＋--ep stem kebab 化入檔名＋生成後 >7 天清理接線。
        in-process main()＋chdir 讓 cwd 相對的預設路徑落 tmp，不污真 .tours/。"""
        repo, a, b = make_repo(tmp_path)
        sa, sb = self._snapshot_pair(tmp_path, a, b)
        ep = tmp_path / "My Fancy EP.md"
        ep.write_text("# EP\n")
        monkeypatch.chdir(tmp_path)
        (tmp_path / ".tours" / "delta").mkdir(parents=True)
        expired = tmp_path / ".tours" / "delta" / "2026-01-01-old.tour"
        expired.write_text("{}")
        monkeypatch.setattr(
            sys,
            "argv",
            ["delta_tour", str(sa), str(sb), "--repo", str(repo), "--ep", str(ep)],
        )
        delta_main()
        expected = (
            tmp_path / ".tours" / "delta" / f"{local_today():%Y-%m-%d}-my-fancy-ep.tour"
        )
        assert expected.exists()
        tour = json.loads(expected.read_text())
        assert tour["title"] == "my-fancy-ep 變更導覽"
        assert not expired.exists()  # main() 生成後清理接線（build review F-D）
        assert "cleaned 1 expired" in capsys.readouterr().out
