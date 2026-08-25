"""tour_upgrade 單元測試——pattern 生成策略／cross-ref 活化／dry-run 保護。"""

import json
from pathlib import Path

from code_reality import tour_upgrade


def _step(repo: Path, name: str, content: str, line: int, desc: str = "") -> dict:
    (repo / name).write_text(content, encoding="utf-8")
    return {"file": name, "line": line, "description": desc}


def test_pattern_from_declaration_line(tmp_path):
    repo = tmp_path
    content = "import x\n\nclass DataActor:\n    pass\n"
    step = _step(repo, "mod.pyi", content, 3, "看 `class DataActor` 合約")
    pat = tour_upgrade.build_step_pattern(step, repo)
    assert pat is not None and "DataActor" in pat
    assert "class" in pat


def test_pattern_from_rust_pub_struct(tmp_path):
    repo = tmp_path
    content = "pub struct DataActorCore {\n    id: u8,\n}\n"
    step = _step(repo, "core.rs", content, 1)
    pat = tour_upgrade.build_step_pattern(step, repo)
    assert pat is not None and "DataActorCore" in pat and "struct" in pat


def test_pattern_skip_when_line_not_declaration_and_no_clue(tmp_path):
    repo = tmp_path
    content = "let x = 1;\n"
    step = _step(repo, "m.rs", content, 1, "只是敘事")
    assert tour_upgrade.build_step_pattern(step, repo) is None


def test_pattern_fallback_backtick_near_line(tmp_path):
    repo = tmp_path
    content = "# comment\ndef on_bar(self, bar):\n    pass\n"
    step = _step(repo, "m.py", content, 2, "掃 :2 `def on_bar(...)`")
    pat = tour_upgrade.build_step_pattern(step, repo)
    assert pat is not None and "on_bar" in pat


def test_revive_crossrefs():
    key_by_num = {3: "資料流：一個 Bar 的旅程"}
    out, n = tour_upgrade.revive_crossrefs("接著看[3 - 資料流]的細節", key_by_num)
    assert n == 1
    assert out == "接著看[資料流][資料流：一個 Bar 的旅程#1]的細節"


def test_revive_crossrefs_unknown_number_kept():
    out, n = tour_upgrade.revive_crossrefs("[99 - 幽靈]", {1: "x"})
    assert n == 0 and out == "[99 - 幽靈]"


def test_dry_run_does_not_touch_files(tmp_path, capsys):

    repo = tmp_path
    p = repo / ".tours" / "t.tour"
    p.parent.mkdir(parents=True)
    orig = {
        "title": "01 - A",
        "steps": [{"file": "m.py", "line": 1, "description": "見[2 - B]"}],
    }
    p.write_text(json.dumps(orig, ensure_ascii=False), encoding="utf-8")
    (repo / "m.py").write_text("class A:\n", encoding="utf-8")
    (repo / ".tours" / "b.tour").write_text(
        json.dumps({"title": "02 - B", "steps": []}, ensure_ascii=False),
        encoding="utf-8",
    )
    before = p.read_text(encoding="utf-8")
    code = tour_upgrade.run(repo, Path(".tours"), apply=False)
    assert code == 0
    assert p.read_text(encoding="utf-8") == before
