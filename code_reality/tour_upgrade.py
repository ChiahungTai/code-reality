"""tour corpus 舊格式遷移——pattern 補全＋cross-ref 活化＋manifest 寫入。

預設 dry-run（只報告不改檔）——curated 保護鐵律：認真 corpus 不盲目覆蓋。
pattern 生成策略：步錨行本身是宣告行（py/rust class/def/struct/fn/trait）→ 由該行
組 literal-ish 全行 pattern（與 chain_tour anchor_pattern 同哲學）；否則從 description
的 backtick 符號線索回找檔內宣告行（最近行須等於原 line 才採）。
"""

import argparse
import json
import re
import sys
from pathlib import Path

from . import tour_manifest, tour_validate

DECL_RE = re.compile(
    r"^\s*(?:pub\s+)?(?:async\s+)?(?:unsafe\s+)?"
    r"(struct|enum|trait|fn|impl|class|def)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
BACKTICK_DECL = re.compile(
    r"`(?:pub\s+)?(?:async\s+)?(struct|enum|trait|fn|impl|class|def)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
CROSSREF = re.compile(r"\[(\d{1,2})\s*-\s*([^\]]+)\]")


def _line_pattern(line: str) -> str:
    return "^[ \\t]*" + re.escape(line.strip()) + "[ \\t]*$"


def build_step_pattern(step: dict, repo: Path) -> str | None:
    """回傳可驗證的 pattern（最近命中＝原 line），否則 None。"""
    f, ln = step.get("file"), step.get("line")
    if not (f and ln is not None):
        return None
    p = repo / f
    if not p.exists():
        return None
    lines = p.read_text(encoding="utf-8").splitlines()
    if not (0 <= ln - 1 < len(lines)):
        return None
    target = lines[ln - 1]
    if DECL_RE.match(target):
        pat = _line_pattern(target)
        hits = tour_validate._hits(lines, pat)
        return pat if hits == [ln - 1] else None
    # fallback：description backtick 宣告線索 → 檔內最近行須等於原 line
    for m in BACKTICK_DECL.finditer(step.get("description", "") or ""):
        cand = re.compile(
            r"^[ \t]*(?:pub\s+)?(?:async\s+)?(?:unsafe\s+)?"
            + m.group(1)
            + r"\s+"
            + re.escape(m.group(2))
            + r"\b"
        )
        hits = [i for i, x in enumerate(lines) if cand.search(x)]
        near = [i for i in hits if abs(i - (ln - 1)) <= 1]
        if len(near) == 1:
            return _line_pattern(lines[near[0]])
    return None


BRACKET = re.compile(r"\[([^\]\[]+)\]")


def sanitize_brackets(desc: str) -> tuple[str, int]:
    """未解析的單方括號（rust 屬性／代碼片段）→ 全形括號——player 的 TOUR_REF 會把它們
    誤判 tour link（壞 link 提示）。放過 markdown file link `](` 與雙方括號 link `][`。"""
    n = 0

    def repl(m: re.Match) -> str:
        nonlocal n
        start, end = m.span()
        before = m.string[start - 1] if start > 0 else ""
        after = m.string[end] if end < len(m.string) else ""
        if before in "]([" or after in "](":
            return m.group(0)
        n += 1
        return f"［{m.group(1)}］"

    return BRACKET.sub(repl, desc), n


def revive_crossrefs(desc: str, key_by_num: dict[int, str]) -> tuple[str, int]:
    n = 0

    def repl(m: re.Match) -> str:
        nonlocal n
        num, name = int(m.group(1)), m.group(2)
        key = key_by_num.get(num)
        if not key or "]" in key or "#" in key:
            return m.group(0)
        n += 1
        return f"[{name}][{key}#1]"

    return CROSSREF.sub(repl, desc), n


def upgrade_tour(
    rel: str, tour: dict, repo: Path, key_by_num: dict[int, str], apply: bool
) -> dict:
    pat_add = pat_skip = refs = 0
    for step in tour.get("steps", []):
        if step.get("pattern"):
            continue
        pat = build_step_pattern(step, repo)
        if pat:
            step["pattern"] = pat
            pat_add += 1
        else:
            pat_skip += 1
    for step in tour.get("steps", []):
        desc = step.get("description", "") or ""
        new_desc, n = revive_crossrefs(desc, key_by_num)
        if n:
            step["description"] = new_desc
            refs += n
    for step in tour.get("steps", []):
        desc = step.get("description", "") or ""
        new_desc, n = sanitize_brackets(desc)
        if n:
            step["description"] = new_desc
    return {"pattern_added": pat_add, "pattern_skip": pat_skip, "crossrefs": refs}


def run(repo: Path, tours_dir: Path, apply: bool) -> int:
    tours = tour_validate.iter_tours(repo, tours_dir)
    assert tours, f"{repo / tours_dir} 無 .tour"
    # N→title 鍵（NN 前綴補零；鍵用 ts_key——含連字號截斷語義）
    key_by_num: dict[int, str] = {}
    for _, d in tours:
        m = re.match(r"^#?0*(\d+)\s-", d.get("title", ""))
        if m:
            key_by_num[int(m.group(1))] = tour_validate.ts_key(d["title"])
    dup = {k: v for k, v in key_by_num.items() if v and "-" in v}
    total_add = total_skip = total_refs = 0
    report: list[str] = []
    for rel, tour in tours:
        r = upgrade_tour(rel, tour, repo, key_by_num, apply)
        total_add += r["pattern_added"]
        total_skip += r["pattern_skip"]
        total_refs += r["crossrefs"]
        report.append(
            f"  {rel}: pattern +{r['pattern_added']} skip {r['pattern_skip']} crossref {r['crossrefs']}"
        )
    mode = "APPLY" if apply else "DRY-RUN"
    print(
        f"[OK] tour_upgrade {mode}: {len(tours)} tours | pattern +{total_add} skip {total_skip} | crossref {total_refs}"
    )
    for line in report:
        print(line)
    for k, v in dup.items():
        print(f"[WARN] 編號 {k} 的匹配鍵含 '-'（截斷風險）: {v[:40]}")
    if not apply:
        return 0
    root = repo / tours_dir
    for rel, tour in tours:
        path = repo / rel
        path.write_text(
            json.dumps(tour, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
        )
    commit = tour_manifest.git_head(repo)
    mpath = root / "manifest.toml"
    data = tour_manifest.load(mpath)
    data.setdefault("version", 1)
    for rel, _ in tours:
        rel_from_root = (
            str(Path(rel).relative_to(tours_dir)) if str(tours_dir) != "." else rel
        )
        tour_manifest.upsert(
            data, rel_from_root, generator="manual", sources=[], commit=commit
        )
    tour_manifest.dump(mpath, data)
    print(f"[OK] manifest 寫入: {mpath}（{len(tours)} rows, generator=manual=curated）")
    code, _ = tour_validate.validate(repo, tours_dir, with_manifest=True)
    return code


def main() -> None:
    parser = argparse.ArgumentParser(
        description="tour corpus 舊格式遷移（預設 dry-run）"
    )
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--tours-dir", type=Path, default=Path(".tours"))
    parser.add_argument("--apply", action="store_true", help="實際寫檔（預設 dry-run）")
    args = parser.parse_args()
    sys.exit(run(args.repo, args.tours_dir, args.apply))


if __name__ == "__main__":
    main()
