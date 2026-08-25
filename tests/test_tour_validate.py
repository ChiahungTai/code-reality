"""tour_validate 單元測試——ts_key 語義／link 檢查／錨三態／manifest source。"""

import json
from pathlib import Path

from code_reality import tour_validate


def test_ts_key_strips_zero_padded_prefix():
    assert tour_validate.ts_key("01 - 2. 場景 1：啟動鏈") == "2. 場景 1：啟動鏈"


def test_ts_key_non_numeric_prefix_untouched():
    assert (
        tour_validate.ts_key("00b - mosaic 總覽（完整）") == "00b - mosaic 總覽（完整）"
    )


def test_ts_key_hyphen_truncation_quirk():
    # utils.ts split("-")[1] 語義：body 連字號 → 鍵截斷
    assert tour_validate.ts_key("02 - 3.1-A 處置股") == "3.1"


def _write(repo: Path, rel: str, tour: dict) -> None:
    p = repo / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(tour, ensure_ascii=False), encoding="utf-8")


def test_links_good_and_broken(tmp_path):
    repo = tmp_path
    _write(
        repo, ".tours/a.tour", {"title": "01 - 目標", "steps": [{"description": "x"}]}
    )
    tours = tour_validate.iter_tours(repo, Path(".tours"))
    idx = tour_validate.key_index(tours)
    by_rel = dict(tours)
    ok = {"steps": [{"description": "去[目標][目標#1]"}]}
    bad = {"steps": [{"description": "去[找不到][不存在的鍵#1]"}]}
    oob = {"steps": [{"description": "去[目標][目標#9]"}]}
    f1, n1 = tour_validate.check_links("ok.tour", ok, idx, by_rel)
    f2, _ = tour_validate.check_links("bad.tour", bad, idx, by_rel)
    f3, _ = tour_validate.check_links("oob.tour", oob, idx, by_rel)
    assert not f1 and n1 == 1
    assert any("無/歧義" in x for x in f2)
    assert any("步號越界" in x for x in f3)


def test_anchor_three_states(tmp_path, capsys):
    repo = tmp_path
    src = repo / "mod.py"
    src.write_text(
        "class Foo:\n    pass\n\ndef bar() -> int:\n    return 1\n", encoding="utf-8"
    )
    exact = {"file": "mod.py", "line": 1, "pattern": r"^class Foo:"}
    corrected = {"file": "mod.py", "line": 3, "pattern": r"^def bar"}
    unverified = {"file": "mod.py", "line": 1, "pattern": r"^def gone"}
    f1, ex, _ = tour_validate.check_anchors("t", {"steps": [exact]}, repo)
    f2, _, co = tour_validate.check_anchors("t", {"steps": [corrected]}, repo)
    f3, _, _ = tour_validate.check_anchors("t", {"steps": [unverified]}, repo)
    assert not f1 and ex == 1
    assert not f2 and co == 1  # corrected 非 fail
    assert any("unverified" in x for x in f3)


def test_file_link_path(tmp_path):
    repo = tmp_path
    (repo / "real.py").write_text("", encoding="utf-8")
    good = {"steps": [{"description": "看[real](./real.py)"}]}
    bad = {"steps": [{"description": "看[ghost](./ghost.py)"}]}
    assert not tour_validate.check_files("g", good, repo)
    assert tour_validate.check_files("b", bad, repo)


def test_manifest_missing_source(tmp_path, capsys):
    from code_reality import tour_manifest

    repo = tmp_path
    _write(repo, ".tours/a.tour", {"title": "t", "steps": []})
    mpath = repo / ".tours" / "manifest.toml"
    data = tour_manifest.upsert(
        {}, "a.tour", generator="manual", sources=["gone.md"], commit="c0ffee"
    )
    tour_manifest.dump(mpath, data)
    tours = tour_validate.iter_tours(repo, Path(".tours"))
    fails = tour_validate.check_manifest(repo, Path(".tours"), tours)
    assert any("source 不存在" in x for x in fails)
