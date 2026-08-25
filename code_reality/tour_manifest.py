"""corpus provenance manifest——.tours/manifest.toml 讀寫。

derived/curated 二分的機械載體：source×generator×anchored_commit。
curated＝generator "manual"；重產 diff 非空的 derived 由 audit 建議升 manual（不覆蓋）。
"""

import argparse
import json
import math
import re
import subprocess
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None  # type: ignore[assignment]


def git_head(repo: Path) -> str:
    out = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode != 0:
        print(
            f"[WARN] git HEAD 取不到（{repo} 非 git repo？）——anchored_commit 記 unknown"
        )
        return "unknown"
    return out.stdout.strip()


def load(path: Path) -> dict:
    if not path.exists():
        return {}
    if tomllib is None:  # pragma: no cover
        raise RuntimeError("需要 Python 3.11+（tomllib）")
    return tomllib.loads(path.read_text(encoding="utf-8"))


def upsert(
    data: dict,
    rel: str,
    *,
    generator: str,
    sources: list[str],
    commit: str,
) -> dict:
    rows = data.setdefault("tour", {})
    rows[rel] = {
        "generator": generator,
        "sources": sources,
        "anchored_commit": commit,
    }
    return data


def _kv(key: str, val: str) -> str:
    return f'{key} = "{val}"'


def _toml_key(key: str) -> str:
    """bare key（[A-Za-z0-9_-]+）直出；其餘 quote——鍵名裸輸出非 bare 形態會寫出不可解析 TOML。"""
    if re.fullmatch(r"[A-Za-z0-9_-]+", key):
        return key
    return json.dumps(key, ensure_ascii=False)


def _toml_value(v: object) -> str:
    """頂層未知鍵的 TOML 序列化（scalar／scalar list）；非支援型別 loud——silent 掉資料更糟。"""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float) and not math.isfinite(v):
        raise ValueError(
            "manifest 頂層鍵含非有限 float（inf/nan）——TOML 無此字面，寫出即非法"
        )
    if isinstance(v, (int, float)):
        return str(v)
    if isinstance(v, str):
        # json 跳脫是 TOML basic string 子集；U+007F 兩家語義不同——補跳脫
        # （否則寫出非法 TOML、延遲到下游 load 才炸）
        return json.dumps(v, ensure_ascii=False).replace("\x7f", "\\u007f")
    if isinstance(v, list) and all(isinstance(x, (bool, int, float, str)) for x in v):
        return "[" + ", ".join(_toml_value(x) for x in v) + "]"
    raise ValueError(
        f"manifest 頂層鍵型別不支援保存（{type(v).__name__}）——只支援 scalar／scalar list"
    )


def dump(path: Path, data: dict) -> None:
    lines = [f"version = {_toml_value(data.get('version', 1))}"]
    # 未知頂層鍵 roundtrip 保存——dump 只重建已知欄位會 silent 刪除人工鍵
    # （F7：NT 的 audience = "newcomer" 兩次被 upsert 刪掉）
    for key in sorted(k for k in data if k not in ("version", "tour")):
        lines.append(f"{_toml_key(key)} = {_toml_value(data[key])}")
    for rel in sorted(data.get("tour", {})):
        row = data["tour"][rel]
        lines.append(f'\n[tour."{rel}"]')
        lines.append(_kv("generator", row["generator"]))
        srcs = ", ".join(f'"{s}"' for s in row.get("sources", []))
        lines.append(f"sources = [{srcs}]")
        lines.append(_kv("anchored_commit", row["anchored_commit"]))
        # row 未知鍵同原則保存（J1）——upsert 全列替換＝重產列歸工具權威；
        # 未動列 roundtrip 不得掉資料
        for key in sorted(
            k for k in row if k not in ("generator", "sources", "anchored_commit")
        ):
            lines.append(f"{_toml_key(key)} = {_toml_value(row[key])}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def tours_root_of(out_dir: Path) -> Path:
    """out_dir（如 .tours/arch/<stem>）往上找名為 .tours 的根；找不到一路上至 filesystem root——呼叫端以 ``name != ".tours"`` 判定非 corpus 樹。"""
    p = out_dir.resolve()
    while p.name and p.name != ".tours" and p.parent != p:
        p = p.parent
    return p


def init_scan(
    repo: Path,
    tours_dir: Path,
    *,
    generator_rule: str = "chain",
) -> dict:
    """掃 corpus 補 manifest——只補缺行（既有行不覆蓋：generator 原生寫入的 sources 保留）；generator 以檔名慣例猜（`chain-*` 或純序號 `NN.tour`→chain_tour、其餘 manual）、sources 留空。"""
    path = repo / tours_dir / "manifest.toml"
    data = load(path) if path.exists() else {}
    data.setdefault("version", 1)
    data.setdefault("tour", {})
    commit = git_head(repo)
    for f in sorted((repo / tours_dir).rglob("*.tour")):
        rel_path = f.relative_to(repo / tours_dir)
        if any(d in rel_path.parts[:-1] for d in ("delta", "dev-fixture")):
            continue  # 時間層可再生／開發假資料——非 manifest 範圍
        rel = rel_path.as_posix()
        if rel in data["tour"]:
            continue
        gen = (
            "chain_tour"
            if generator_rule == "chain"
            and (f.name.startswith("chain-") or (f.stem.isascii() and f.stem.isdigit()))
            else "manual"
        )
        upsert(data, rel, generator=gen, sources=[], commit=commit)
    return data


def main() -> None:
    parser = argparse.ArgumentParser(description="manifest 讀寫／--init-scan 骨架生成")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--tours-dir", type=Path, default=Path(".tours"))
    parser.add_argument(
        "--init-scan", action="store_true", help="掃 corpus 生成 manifest 骨架"
    )
    args = parser.parse_args()
    path = args.repo / args.tours_dir / "manifest.toml"
    if not args.init_scan:
        print(f"[OK] manifest path: {path}（exists={path.exists()}）")
        return
    data = init_scan(args.repo, args.tours_dir)
    dump(path, data)
    print(f"[OK] manifest init: {len(data['tour'])} rows -> {path}")


if __name__ == "__main__":
    main()
