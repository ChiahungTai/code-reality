"""tour_manifest init_scan 單元測試——generator 檔名慣例猜測。

D-f 後新增純序號 NN.tour→chain_tour 分支；delta/dev-fixture 排除；
manual 判定（含全形數字 isascii 防禦）。
"""

from pathlib import Path

import pytest

from code_reality import tour_manifest


def _touch(root: Path, rel: str) -> None:
    p = root / ".tours" / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text("{}", encoding="utf-8")


def test_init_scan_filename_conventions(tmp_path: Path) -> None:
    _touch(tmp_path, "01-族名/01.tour")
    _touch(tmp_path, "02-舊格式/chain-01-x.tour")
    _touch(tmp_path, "00 - codetour 總覽.tour")
    _touch(tmp_path, "03-全形/１２.tour")
    _touch(tmp_path, "delta/2026-08-23-task.tour")
    data = tour_manifest.init_scan(tmp_path, Path(".tours"))
    assert data["tour"]["01-族名/01.tour"]["generator"] == "chain_tour"
    assert data["tour"]["02-舊格式/chain-01-x.tour"]["generator"] == "chain_tour"
    assert data["tour"]["00 - codetour 總覽.tour"]["generator"] == "manual"
    assert data["tour"]["03-全形/１２.tour"]["generator"] == "manual"
    assert "delta/2026-08-23-task.tour" not in data["tour"]


def test_dump_preserves_unknown_top_level_keys(tmp_path: Path) -> None:
    """F7（NT dogfood）：load→upsert→dump roundtrip 不得刪未知頂層鍵（audience 被刪兩次）。"""
    mpath = tmp_path / "manifest.toml"
    mpath.write_text(
        'version = 1\naudience = "newcomer"\n\n[tour."arch/x/01.tour"]\n'
        'generator = "chain_tour"\nsources = ["a.md"]\nanchored_commit = "abc"\n',
        encoding="utf-8",
    )
    data = tour_manifest.load(mpath)
    tour_manifest.upsert(
        data, "arch/y/01.tour", generator="chain_tour", sources=["b.md"], commit="def"
    )
    tour_manifest.dump(mpath, data)
    back = tour_manifest.load(mpath)
    assert back["audience"] == "newcomer"
    assert back["tour"]["arch/x/01.tour"] == {
        "generator": "chain_tour",
        "sources": ["a.md"],
        "anchored_commit": "abc",
    }
    assert "arch/y/01.tour" in back["tour"]


@pytest.mark.parametrize(
    "val",
    [True, False, 2, 3.5, "文本", "", [], [1, "a", True], [True, False]],
)
def test_toml_value_roundtrip_types(val: object, tmp_path: Path) -> None:
    """J5：型別矩陣 roundtrip——bool list 同時釘 bool-before-int 順序
    （順序錯了 bool 會輸出 `True`＝非法 TOML）。"""
    mpath = tmp_path / "manifest.toml"
    tour_manifest.dump(mpath, {"k": val})
    back = tour_manifest.load(mpath)
    assert back["k"] == val


def test_toml_special_keys_and_del_char(tmp_path: Path) -> None:
    """J2/J3：非 bare key 鍵名 quoting；U+007F 補跳脫——兩者原形態寫出非法 TOML。"""
    mpath = tmp_path / "manifest.toml"
    tour_manifest.dump(mpath, {"my key": "a\x7fb"})
    back = tour_manifest.load(mpath)
    assert back["my key"] == "a\x7fb"


def test_dump_preserves_row_unknown_keys(tmp_path: Path) -> None:
    """J1：row 未知鍵 roundtrip 保存——upsert 全列替換限重產列（工具權威）。"""
    mpath = tmp_path / "manifest.toml"
    mpath.write_text(
        '[tour."arch/x/01.tour"]\ngenerator = "chain_tour"\nsources = []\n'
        'anchored_commit = "abc"\nowner = "ctai"\n',
        encoding="utf-8",
    )
    data = tour_manifest.load(mpath)
    tour_manifest.dump(mpath, data)
    back = tour_manifest.load(mpath)
    assert back["tour"]["arch/x/01.tour"]["owner"] == "ctai"


def test_dump_loud_on_unsupported_top_level_type(tmp_path: Path) -> None:
    """非 scalar／scalar list 的頂層鍵 loud 拒寫——不 silent 掉資料。"""
    with pytest.raises(ValueError, match="型別不支援"):
        tour_manifest.dump(tmp_path / "m.toml", {"weird": {"nested": 1}})


def test_dump_loud_on_nonfinite_float(tmp_path: Path) -> None:
    """inf/nan float 走 str(v) 會寫出非法 TOML（下游 load 才炸）——源頭 loud 拒寫。"""
    with pytest.raises(ValueError, match="非有限"):
        tour_manifest.dump(tmp_path / "m.toml", {"ratio": float("inf")})
    with pytest.raises(ValueError, match="非有限"):
        tour_manifest.dump(tmp_path / "m.toml", {"list_val": [1.0, float("nan")]})
