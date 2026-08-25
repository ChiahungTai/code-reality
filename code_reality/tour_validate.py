"""tour corpus 機械驗證——JSON／tour link 鍵／file link 路徑／錨三態／manifest source。

消費端語義源（唯讀參考；正則以 py re 重現，已於 mosaic／codetour／NT 三 corpus 實證等價）：
- getTourTitle 剝前綴：codetour/src/utils.ts:37-43（split("-")[1]——body 連字號截斷）
- TOUR／FILE_REFERENCE_PATTERN：codetour/src/player/index.ts:57-59
- 錨解析四態：codetour/src/player/anchor.ts:103-146（corrected 非 fail——與 player 一致）
"""

import argparse
import json
import re
import sys
from pathlib import Path

TS_KEY_RE = re.compile(r"^#?\d+\s-")
TOUR_REF = re.compile(r"(?:\[([^\]]+)\])?\[(?=\s*[^\]\s])([^\]#]+)?(?:#(\d+))?\](?!\()")
FILE_REF = re.compile(r"\[([^\]]+)\]\((\.[^\)]+)\)")


def ts_key(title: str) -> str:
    """重現 codetour getTourTitle：剝 NN - 前綴（在第一個 '-' 截斷）。"""
    if TS_KEY_RE.match(title):
        return title.split("-")[1].strip()
    return title


EXCLUDED_DIRS = (
    "delta",
    "dev-fixture",
)  # 時間層（可再生）／開發驗收假資料（非 corpus）


def iter_tours(
    repo: Path, tours_dir: Path, include_excluded: bool = False
) -> list[tuple[str, dict]]:
    """遞迴 corpus——預設排除 delta/（時間層：可再生）與 dev-fixture/（開發假資料）。
    link 鍵索引（key_index）應以 include_excluded=True 建——被排除者仍是合法 link 目標。"""
    out: list[tuple[str, dict]] = []
    root = repo / tours_dir
    for f in sorted(root.rglob("*.tour")):
        parts = f.relative_to(root).parts[:-1]
        if not include_excluded and any(d in parts for d in EXCLUDED_DIRS):
            continue
        out.append(
            (f.relative_to(repo).as_posix(), json.loads(f.read_text(encoding="utf-8")))
        )
    return out


def key_index(tours: list[tuple[str, dict]]) -> dict[str, list[str]]:
    idx: dict[str, list[str]] = {}
    for rel, d in tours:
        idx.setdefault(ts_key(d.get("title", "")), []).append(rel)
    return idx


def _hits(lines: list[str], pattern: str) -> list[int]:
    try:
        rx = re.compile(pattern)
    except re.error:
        return []
    return [i for i, ln in enumerate(lines) if rx.search(ln)]


def check_links(
    rel: str, tour: dict, idx: dict[str, list[str]], by_rel: dict[str, dict]
) -> tuple[list[str], int]:
    fails: list[str] = []
    n_links = 0
    for i, step in enumerate(tour.get("steps", []), start=1):
        desc = step.get("description", "") or ""
        for m in TOUR_REF.finditer(desc):
            key = (m.group(2) or "").strip()
            hits = idx.get(key, [])
            if len(hits) != 1:
                if m.group(1) is None and not m.group(3):
                    # 單方括號未解析＝prose 誤判（非作者連結）——WARN 不 fail
                    print(f"[WARN] {rel} 步{i} 單括號非 link 文字: [{key[:36]}]")
                    continue
                fails.append(f"[FAIL] {rel} 步{i} tour link 無/歧義目標: {key[:40]}")
                continue
            n_links += 1
            num = m.group(3)
            if num and int(num) > len(by_rel[hits[0]].get("steps", [])):
                fails.append(f"[FAIL] {rel} 步{i} 步號越界: {key[:40]}#{num}")
    return fails, n_links


def check_anchors(rel: str, tour: dict, repo: Path) -> tuple[list[str], int, int]:
    """回傳 (fails, exact, corrected)——unverified=FAIL、corrected=[WARN] 非 fail。"""
    fails: list[str] = []
    exact = corrected = 0
    for i, step in enumerate(tour.get("steps", []), start=1):
        f, ln, pat = step.get("file"), step.get("line"), step.get("pattern")
        if not (f and ln is not None and pat):
            continue
        p = repo / f
        if not p.exists():
            fails.append(f"[FAIL] {rel} 步{i} 錨檔不存在: {f}")
            continue
        lines = p.read_text(encoding="utf-8").splitlines()
        if not (0 <= ln - 1 < len(lines)) or not re.search(pat, lines[ln - 1]):
            hits = _hits(lines, pat)
            if not hits:
                fails.append(
                    f"[FAIL] {rel} 步{i} pattern 未命中（unverified）: {f}:{ln}"
                )
            else:
                best = min(hits, key=lambda h: abs(h - (ln - 1)))
                corrected += 1
                print(f"[WARN] {rel} 步{i} 錨 corrected: {f} L{ln}->L{best + 1}")
        else:
            exact += 1
    return fails, exact, corrected


def check_files(rel: str, tour: dict, repo: Path) -> list[str]:
    fails = []
    for i, step in enumerate(tour.get("steps", []), start=1):
        desc = step.get("description", "") or ""
        for m in FILE_REF.finditer(desc):
            if not (repo / m.group(2)).exists():
                fails.append(f"[FAIL] {rel} 步{i} file link 路徑不存在: {m.group(2)}")
    return fails


def check_manifest(
    repo: Path, tours_dir: Path, tours: list[tuple[str, dict]]
) -> list[str]:

    from . import tour_manifest

    path = repo / tours_dir / "manifest.toml"
    if not path.exists():
        print(f"[WARN] 無 manifest（{path}）——source 存在性未驗")
        return []
    data = tour_manifest.load(path)
    rows = data.get("tour", {})

    def _root_rel(rel: str) -> str:
        return (
            rel
            if str(tours_dir) == "."
            else Path(rel).relative_to(tours_dir).as_posix()
        )

    fails = []
    for rel in rows:
        if not (repo / tours_dir / rel).exists():
            fails.append(f"[FAIL] manifest 列的 tour 檔不存在: {rel}")
    listed = set(rows)
    for rel, _ in tours:
        if _root_rel(rel) not in listed:
            print(f"[WARN] {rel} 不在 manifest（derived/curated 未申報）")
    for rel, row in rows.items():
        for src in row.get("sources", []):
            if not (repo / src).exists():
                fails.append(f"[FAIL] {rel} manifest source 不存在: {src}")
    return fails


def validate(repo: Path, tours_dir: Path, with_manifest: bool) -> tuple[int, list[str]]:
    fails: list[str] = []
    n_links = n_files = 0
    try:
        tours = iter_tours(repo, tours_dir)
    except json.JSONDecodeError as e:
        print(f"[FAIL] JSON parse: {e}")
        return 1, [str(e)]
    if not tours:
        print(f"[WARN] {repo / tours_dir} 無 .tour")
        return 0, []
    idx = key_index(
        iter_tours(repo, tours_dir, include_excluded=True)
    )  # link 目標含被排除目錄
    by_rel = dict(tours)
    for rel, tour in tours:
        lf, nl = check_links(rel, tour, idx, by_rel)
        fails += lf
        n_links += nl
        n_files += sum(1 for _ in FILE_REF.finditer(tour.get("description") or ""))
        for step in tour.get("steps", []):
            n_files += sum(
                1 for _ in FILE_REF.finditer(step.get("description", "") or "")
            )
        fails += check_files(rel, tour, repo)
        af, _ex, _co = check_anchors(rel, tour, repo)
        fails += af
    if with_manifest:
        fails += check_manifest(repo, tours_dir, tours)
    for f in fails:
        print(f)
    print(
        f"[OK] tour validate: {len(tours)} tours | links={n_links} filelinks={n_files} | fails={len(fails)}"
    )
    return (1 if fails else 0), fails


def main() -> None:
    parser = argparse.ArgumentParser(
        description="tour corpus 機械驗證（.tour 語言契約）"
    )
    parser.add_argument("--repo", type=Path, default=Path.cwd(), help="repo 根")
    parser.add_argument(
        "--tours-dir", type=Path, default=Path(".tours"), help="corpus 根（遞迴）"
    )
    parser.add_argument(
        "--manifest", action="store_true", help="驗 manifest source 存在性"
    )
    args = parser.parse_args()
    code, _ = validate(args.repo, args.tours_dir, args.manifest)
    sys.exit(code)


if __name__ == "__main__":
    main()
