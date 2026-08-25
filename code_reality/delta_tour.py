"""delta-review tour——transition diff＋git hunk 錨 → CodeTour `.tour`。

「review commit」從看圖改成走讀：step 1 弧總覽（宣稱對照），之後每步一個
變動檔錨第一個 hunk（vscode:// 跳轉錨）。渲染消費者＝CodeTour（vanilla
即可讀 `.tours/`；fork 增強屬 codetour repo arc）。

用法::

    uv run python -m code_reality.delta_tour <a.json> <b.json> \
        [--ep <ep.md>] [--repo PATH] \
        [--out-dir .tours/delta] [--task <task>]

輸出 ``<out-dir>/YYYY-MM-DD-<task>.tour``（task 預設＝--ep 檔名 stem
kebab 化、無 --ep 時 ``review``）；生成時清理 out-dir 內 >7 天舊檔
（delta tour 不 commit 的本地 7 天生命週期義務）。
已知口徑：claims 只認 profile ``[[module]]`` prefixes 衍生路徑（code-reality
skill「口徑限制」段）——不符前綴的弧宣稱恆 NONE 屬預期，如實顯示
（SM-2，ep-code-reality-ui）。
"""

import argparse
import json
import re
import subprocess
import sys
from datetime import date, datetime
from pathlib import Path
from typing import Any

from code_reality.common import anchor_pattern
from code_reality.exclusions import is_excluded
from code_reality.profile import load_profile, module_of
from code_reality.transition import (
    extract_ep_claims,
    load_snapshot,
    render_json,
    summarize,
)

HUNK_RE = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)", re.MULTILINE)


def local_today() -> date:
    """本地日期（tour 檔名日期／清理窗語義——刻意非 UTC date）。"""
    return datetime.now().astimezone().date()


# 無 --task 且無 --ep 時的任務身分（build_tour 簽名預設同源）
DEFAULT_TASK = "review"


def first_change_lines(
    repo_root: Path, before: str, after: str
) -> tuple[dict[str, int], set[str]]:
    """git diff（unified=0）每檔第一個正錨 hunk 的新行號＋git-A 檔集——跳轉錨。

    ``--diff-filter=AM`` 排除純刪檔與 rename；錨取第一個起始行 >0 的 hunk
    （檔首刪除 hunk 是 ``+0,0`` 被跳過——首 hunk 刪除＋後續修改的檔仍錨到
    後續 hunk，不整檔消失；非檔首的純刪除 hunk（``+N,0``）錨到刪除點
    前一行）；有 diff 輸出但無正錨（binary/清空）退 line 1（漏檔比弱錨糟）。
    before/after 須是本 repo 可解析的 commit——git 失敗直接 crash
    （靜默退化會讓 tour 無聲漏檔，輸入錯誤要大聲）。
    """
    out = subprocess.run(
        [
            "git",
            "diff",
            "--name-status",
            "-z",
            "--diff-filter=AM",
            before,
            after,
        ],
        cwd=repo_root,
        capture_output=True,
        check=True,
    ).stdout.decode(
        # binary-as-text 只需路徑欄可解析；非 UTF-8 檔名經 replace 弄髒後續
        # diff 空輸出＝靜默漏步（Linux 理論邊界，macOS 不可構造——已知口徑）
        "utf-8",
        errors="replace",
    )
    parts = iter(p for p in out.split("\0") if p)
    # 兩兩配對依賴 AM filter：A/M 條目恰兩欄位（status\0path\0）；R/C 是
    # 三欄位（R100\0old\0new\0）會錯位——放寬 filter 前須改解析
    added: set[str] = set()
    files: list[str] = []
    for status, f in zip(parts, parts):
        files.append(f)
        if status == "A":
            added.add(f)
    lines: dict[str, int] = {}
    for f in files:
        hunks = subprocess.run(
            ["git", "diff", "--unified=0", before, after, "--", f],
            cwd=repo_root,
            capture_output=True,
            check=True,
        ).stdout.decode(
            "utf-8", errors="replace"
        )  # binary-as-text 內容（無 NUL 被 git 當文字 diff）只需 @@ 標頭可解析
        anchor = next(
            (int(m.group(1)) for m in HUNK_RE.finditer(hunks) if int(m.group(1)) > 0),
            None,
        )
        if anchor is not None:
            lines[f] = anchor
        elif hunks.strip():
            lines[f] = 1
    return lines, added


# First-declaration anchor for freshly added/renamed code files (mosaic
# dogfood bug 3: landing on a copyright header is not what a reader wants).
_DECL_RE = re.compile(
    r"^\s*(?:async\s+def\b|def\b|class\b|fn\b|struct\b|enum\b|impl\b|"
    r"trait\b|mod\b|func\b|type\b|interface\b)"
)
_CODE_SUFFIXES = {
    ".py",
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".go",
    ".java",
    ".c",
    ".h",
    ".cpp",
}


def range_status(
    repo_root: Path, before: str, after: str
) -> tuple[dict[str, list[str]], dict[str, str]]:
    """Full ``git diff --name-status -z`` for the claimed range — the single
    source of truth for the step set.

    Steps derived from the range are in-range by construction; the previous
    snapshot-pair file sets drifted whenever the profile/exclusions changed
    between the two exports (mosaic dogfood bug 2: 18 out-of-range deletion
    steps from an older archive-cleanup commit). Returns
    ``({"A": [...], "M": [...], "D": [...]}, renames {new: old})``;
    typechanges (T) count as M, copies (C) as A.
    """
    out = subprocess.run(
        ["git", "diff", "--name-status", "-z", before, after],
        cwd=repo_root,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8", errors="replace")
    toks = [t for t in out.split("\0") if t]
    by_status: dict[str, list[str]] = {"A": [], "M": [], "D": []}
    renames: dict[str, str] = {}
    i = 0
    while i < len(toks):
        code = toks[i][0]
        if code in ("R", "C"):
            old, new = toks[i + 1], toks[i + 2]
            i += 3
            if code == "R":
                renames[new] = old
            else:
                by_status["A"].append(new)
        else:
            path = toks[i + 1]
            i += 2
            if code == "A":
                by_status["A"].append(path)
            elif code == "D":
                by_status["D"].append(path)
            else:  # M / T / anything content-ish
                by_status["M"].append(path)
    return by_status, renames


def new_file_anchor(repo_root: Path, after: str, path: str) -> int:
    """Anchor line for added/renamed files: first declaration line for code
    files; line 1 otherwise (and as fallback when unreadable)."""
    if Path(path).suffix.lower() not in _CODE_SUFFIXES:
        return 1
    r = subprocess.run(
        ["git", "show", f"{after}:{path}"],
        cwd=repo_root,
        capture_output=True,
        check=False,
    )
    if r.returncode != 0:
        return 1
    try:
        text = r.stdout.decode("utf-8")
    except UnicodeDecodeError:
        return 1
    for idx, line in enumerate(text.split("\n"), start=1):
        if _DECL_RE.match(line):
            return idx
    return 1


def file_subjects(repo_root: Path, before: str, after: str, path: str) -> list[str]:
    """Commit subjects touching ``path`` within the range — the cheapest
    mechanical "why" for step descriptions (mosaic dogfood bug 3)."""
    out = subprocess.run(
        ["git", "log", "--format=%s", f"{before}..{after}", "--", path],
        cwd=repo_root,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8", errors="replace")
    return [s for s in out.splitlines() if s.strip()]


def _after_lines(
    repo_root: Path, after: str, path: str, lines_cache: dict[str, list[str] | None]
) -> list[str] | None:
    """after commit 版本的檔案行集（``git show``），同檔多步快取。

    不在 after commit（exit≠0，如刪檔步）或 binary（utf-8 decode 失敗）
    → None——兩者皆 pattern 省略條件，不發射。
    """
    if path not in lines_cache:
        r = subprocess.run(
            ["git", "show", f"{after}:{path}"],
            cwd=repo_root,
            capture_output=True,
            check=False,
        )
        if r.returncode != 0:
            lines_cache[path] = None
        else:
            try:
                # split("\n") 非 splitlines()：git hunk 行號只數 \n——
                # \r/\x0c 等切行會讓行索引位移、pattern 錯行
                lines_cache[path] = r.stdout.decode("utf-8").split("\n")
            except UnicodeDecodeError:
                lines_cache[path] = None
    return lines_cache[path]


def build_tour(
    data: dict[str, Any],
    repo_root: Path,
    *,
    ep_path: Path | None = None,
    task: str = DEFAULT_TASK,
) -> dict[str, Any]:
    """transition JSON（``transition.render_json`` 形狀）→ CodeTour tour dict。

    Step-set truth = the claimed git range (``range_status``)——snapshot 檔集
    不再餵步驟（跨快照 profile/exclusion drift 會偽造 gone/new；mosaic
    dogfood bug 2）。claims 三態（bug 1）：⚠ 只在可比較態；刪檔收斂單步、
    description 帶 range commit subject（bug 3）。
    """
    profile = load_profile(repo_root)
    meta = data["_meta"]
    statuses, renames = range_status(repo_root, meta["before"], meta["after"])
    jump, _ = first_change_lines(repo_root, meta["before"], meta["after"])

    claims = data.get("ep_claims") or {}
    c_hit = claims.get("claimed_and_changed", [])
    c_sur = claims.get("changed_not_claimed", [])
    c_miss = claims.get("claimed_not_changed", [])

    # claims three-state (bug 1): ⚠ "EP didn't mention this" is a serious
    # accusation — emit it only when the comparison actually ran. Extraction
    # failure (no profile / no mention / zero-hit guard) degrades to
    # not-compared instead of mass false warnings.
    if "ep_claims" not in data:
        claims_state, nc_reason = "no_ep", None
    elif claims.get("claims_none"):
        claims_state = "not_compared"
        nc_reason = (
            "profile 未載入——--repo 未指到含 .code-reality.toml 的 checkout"
            if profile is None
            else "EP 內無 profile 前綴路徑 mention（相對路徑需可解析至前綴下）"
        )
    elif not c_hit and c_sur:
        claims_state = "not_compared"
        nc_reason = "宣稱對照 0 命中且有多個變更模組——matcher 異常訊號，整塊降級未比對"
        print(
            f"[WARN] {nc_reason}（步驟不標 ✓/⚠——避免把抽取失效誤呈為"
            "「EP 沒提卻變了」）",
            file=sys.stderr,
        )
    else:
        claims_state, nc_reason = "compared", None

    def claim_tag(module: str) -> str:
        if claims_state != "compared":
            return ""
        if module in c_hit:
            return "✓宣稱命中"
        if module in c_sur:
            return "⚠EP沒提卻變了"
        return ""

    if claims_state == "compared":
        claims_section = (
            f"**宣稱對照**——✓ 命中 ({len(c_hit)})：{', '.join(c_hit) or '無'}；\n"
            f"⚠ EP 沒提卻變了 ({len(c_sur)})：{', '.join(c_sur) or '無'}；\n"
            f"✗ 宣稱未動 ({len(c_miss)})：{', '.join(c_miss) or '無'}。"
        )
    elif claims_state == "not_compared":
        claims_section = (
            f"**EP 宣稱對照：未比對**——{nc_reason}；本 tour 不對步驟標註 ✓/⚠。"
            "\n實際變動模組（供判讀）："
            f"{', '.join(data.get('changed_modules', [])) or '無'}"
        )
    else:
        claims_section = (
            "**EP 宣稱**：NONE（未提供 --ep）。"
            "\n實際變動模組（供判讀）："
            f"{', '.join(data.get('changed_modules', [])) or '無'}"
        )

    def _noise(f: str) -> bool:
        # Symmetric across all statuses now (bug 2: the old gone-file branch
        # had no filter → out-of-range deletions leaked in). .tours/ covers
        # this tool's own output.
        return is_excluded(f, profile) or f.startswith((".kanban/", ".tours/"))

    a_files = sorted(f for f in statuses["A"] if not _noise(f))
    r_files = sorted(f for f in renames if not _noise(f))
    m_files = sorted(f for f in statuses["M"] if not _noise(f) and f not in renames)
    d_files = sorted(f for f in statuses["D"] if not _noise(f))

    # Overview counts derive from the same sets as the steps — consistent by
    # construction (kills the old "3 new files" vs 5 new-file steps mismatch).
    summary = (
        f"before `{meta['before'][:8]}` → after `{meta['after'][:8]}`："
        f"+{len(data['added'])}/−{len(data['removed'])} 模組邊、"
        f"{len(a_files)} 新檔、{len(r_files)} 改名、{len(m_files)} 修改、"
        f"{len(d_files)} 刪檔。\n\n"
        + claims_section
        + "\n\n之後每步一個變動檔（修改錨第一個 hunk、新檔錨第一個宣告行）。"
    )

    ep_on_disk = ep_path is not None and ep_path.exists()
    ep_anchor = str(ep_path) if ep_on_disk else None
    first_added = a_files[0] if a_files else (r_files[0] if r_files else None)
    if ep_anchor is None:
        ep_anchor = first_added

    # pattern：錨行取 after commit 版本（與錨語義一致——hunk 行號指 B 版）
    lines_cache: dict[str, list[str] | None] = {}

    def step_pattern(f: str, ln: int) -> str | None:
        lines = _after_lines(repo_root, meta["after"], f, lines_cache)
        # ln 恆 ≥1（literal 1 或 hunk 正錨濾過 >0），免 chain 側的下界 guard
        if lines is None or ln - 1 >= len(lines) or not lines[ln - 1].strip():
            return None  # 空錨行/不在 after commit/binary——播放端 fallback line
        return anchor_pattern(lines[ln - 1])

    overview: dict[str, Any] = {
        "file": ep_anchor or "README.md",
        "line": 1,
        "title": f"弧總覽：{meta['before'][:8]} → {meta['after'][:8]}",
        "description": summary,
    }
    steps: list[dict[str, Any]] = [overview]
    # 總覽步錨在 EP working-tree 檔時不發射 pattern（播放端開的是工作樹檔，
    # 與 after-commit 內容混兩源）；錨 range 檔（皆 after-commit 檔）才發射
    # ——gating 用 exists() 判準，非路徑字串等值
    if not ep_on_disk and ep_anchor is not None:
        overview_pattern = step_pattern(overview["file"], 1)
        if overview_pattern is not None:
            overview["pattern"] = overview_pattern

    entries: list[tuple[str, str, int]] = [
        (f, "＋新檔", new_file_anchor(repo_root, meta["after"], f)) for f in a_files
    ]
    entries += [
        (f, "→改名", new_file_anchor(repo_root, meta["after"], f)) for f in r_files
    ]
    entries += [(f, "M修改", jump.get(f, 1)) for f in m_files]
    for f, tag, ln in entries:
        mod = module_of(f, profile)
        ct = claim_tag(mod)
        # The cheapest mechanical "why": the range commit that touched the
        # file (bug 3 — descriptions were pure path/module metadata).
        subs = file_subjects(repo_root, meta["before"], meta["after"], f)
        description = f"{f} · 模組 `{mod}`" + (f" · {ct}" if ct else "")
        if tag == "→改名":
            description += f"\n改名自 `{renames[f]}`。"
        if subs:
            description += f"\ncommit: {subs[0]}"
            if len(subs) > 1:
                description += f"（range 內共 {len(subs)} commits）"
        if ln > 1:
            description += f"\n\n錨：第 {ln} 行。"
        step = {
            "file": f,
            "line": ln,
            "title": f"{tag} {f.rsplit('/', 1)[-1]}" + (f"（{ct}）" if ct else ""),
            "description": description,
        }
        pattern = step_pattern(f, ln)
        if pattern is not None:
            step["pattern"] = pattern
        steps.append(step)

    # Deletions collapse into one unjumpable summary step (bug 2: dead steps
    # were 24% of the mosaic walk-through).
    if d_files:
        steps.append(
            {
                "file": d_files[0],
                "line": 1,
                "title": f"−刪檔 ×{len(d_files)}（range 內彙總）",
                "description": (
                    f"本弧刪除 {len(d_files)} 檔——無法跳轉，僅清單：\n"
                    + "\n".join(f"- {f}" for f in d_files)
                ),
            }
        )

    # title＝task 身分（panel 時間排序軸是檔名日期，title 不編號不帶 hash）；
    # hash 資訊在 description 開頭（弧總覽「步」title 保留 hash——契約僅約束
    # tour title，步 title 是敘事內容）
    return {
        "title": f"{task} 變更導覽",
        "description": summary,
        "steps": steps,
    }


def kebab(name: str) -> str:
    """stem → ASCII kebab-case task 段（panel 檔名 parse 用）。

    EP 檔名恆英文 kebab，非 ASCII 段落摺成 ``-`` 即可；delta title 中文
    自由是顯示層非檔名層——與 chain 子目錄 stem 非 ASCII 保留語義不同。
    """
    return re.sub(r"[^A-Za-z0-9]+", "-", name).strip("-").lower()


def cleanup_expired(
    out_dir: Path, *, keep_days: int = 7, today: date | None = None
) -> int:
    """刪 out-dir 內檔名日期 >keep_days 天的 delta tour，回傳刪除數。

    panel 只隱藏不刪——清理是 generator 義務（delta 不 commit 的本地生命
    週期收尾）；非日期命名或非 ``.tour`` 檔不動（手作檔誤刪防線）。
    """
    today = today or local_today()
    if not out_dir.exists():
        return 0
    removed = 0
    for p in sorted(out_dir.iterdir()):
        if not p.is_file():
            continue  # 日期命名的目錄不 unlink（會炸）——只清檔案
        if p.suffix != ".tour":
            continue  # 日期前綴的非 tour 檔（如手作筆記）不在清理範圍
        m = re.match(r"^(\d{4}-\d{2}-\d{2})-", p.name)
        if not m:
            continue
        try:
            file_date = date.fromisoformat(m.group(1))
        except ValueError:
            continue
        if (today - file_date).days > keep_days:
            p.unlink()
            removed += 1
    return removed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("snapshot_a", type=Path, help="before snapshot（S2 schema）")
    parser.add_argument("snapshot_b", type=Path, help="after snapshot（S2 schema）")
    parser.add_argument(
        "--ep", type=Path, default=None, help="EP markdown（宣稱對照＋總覽步錨點）"
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path.cwd(),
        help="repo 根（before/after commit 需在此 git 歷史內）",
    )
    parser.add_argument(
        "--task",
        default=None,
        help="任務身分（檔名 <date>-<task>.tour 與 title；建議 ASCII "
        "kebab-case——panel 檔名慣例，原樣使用；預設 --ep stem kebab 化，"
        "無 --ep 時 review）",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path(".tours/delta"),
        help="輸出目錄（預設 .tours/delta/）",
    )
    args = parser.parse_args()

    if args.task is not None:
        assert args.task, "--task 不得為空字串"
        task = args.task
    elif args.ep is not None:
        task = kebab(args.ep.stem) or DEFAULT_TASK
    else:
        task = DEFAULT_TASK

    sa, sb = load_snapshot(args.snapshot_a), load_snapshot(args.snapshot_b)
    profile = load_profile(args.repo)
    if args.ep and profile is None:
        print(
            "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，"
            "宣稱對照不生效（--repo 預設 cwd）"
        )
    claims = (
        extract_ep_claims(args.ep, profile, repo_root=args.repo) if args.ep else None
    )
    diff, new_files, gone_files = summarize(sa, sb)
    data = render_json(sa, sb, claims, diff, new_files, gone_files)
    tour = build_tour(data, args.repo, ep_path=args.ep, task=task)

    args.out_dir.mkdir(parents=True, exist_ok=True)
    out_path = args.out_dir / f"{local_today():%Y-%m-%d}-{task}.tour"
    out_path.write_text(
        json.dumps(tour, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    print(f"[OK] delta tour: {len(tour['steps'])} steps -> {out_path}")
    cleaned = cleanup_expired(args.out_dir)
    if cleaned:
        print(f"[OK] cleaned {cleaned} expired delta tours（>7 天）")
    print("[LOG] CodeTour 擴充載入 .tours/ 走讀（vanilla 或 fork 皆可）")


if __name__ == "__main__":
    main()
