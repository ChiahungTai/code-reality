"""transition diff——兩 snapshot module-edge 集差異＋「EP 宣稱 vs 實際」對照。

UC5 最後一哩＋intent drift 機械化第一步（報告 §6 裁決）：Depwire diff 的
輕量自建版（共享 worktree 危害 R6——本工具只讀 snapshot sidecar，不碰
working tree）。

用法::

    uv run python -m code_reality.transition <a.json> <b.json> \
        [--ep <ep.md>] [--repo PATH] [-o <prefix>]

輸出 markdown（人讀）＋JSON（機讀）雙格式：``<prefix>.md`` / ``<prefix>.json``。
已知未覆蓋：rename 偵測（module 改名視為 remove+add，報告如實標註）。
"""

import argparse
import json
import re
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from code_reality.profile import Profile, claims_re, load_profile, module_of

Edge = tuple[str, str, str]

_BASELINE_RE = re.compile(r"\*\*baseline\*\*:\s*([0-9a-f]{7,40})")


@dataclass
class LoadedSnapshot:
    path: Path
    meta: dict[str, Any]
    files: set[str]
    module_edges: set[Edge]


@dataclass
class EdgeDiff:
    added: list[Edge]
    removed: list[Edge]
    reversed: list[tuple[str, str]]  # 一律 added 方向（B1）
    changed_modules: set[str] = field(default_factory=set)


@dataclass
class ClaimsCompare:
    claimed_and_changed: list[str]
    changed_not_claimed: list[str]
    claimed_not_changed: list[str]
    claims_none: bool = False


def load_snapshot(path: Path) -> LoadedSnapshot:
    data = json.loads(path.read_text())
    assert isinstance(data, dict) and "_meta" in data and "module_edges" in data, (
        f"非 S2 snapshot 格式（缺 _meta/module_edges）: {path}"
    )
    edges_raw = data["module_edges"]
    assert all(isinstance(e, list) and len(e) == 3 for e in edges_raw), (
        f"module_edges 元素非 [src, dst, kind] 三元組: {path}"
    )
    return LoadedSnapshot(
        path=path,
        meta=data["_meta"],
        files=set(data.get("files", [])),
        module_edges={tuple(e) for e in edges_raw},
    )


def diff_edges(a: set[Edge], b: set[Edge]) -> EdgeDiff:
    """pair 集合差運算（B1：tuple-diff 投影 ≠ pair 集合差——multi-kind
    重複對時兩者不等價，pair 投影才正確）。"""
    removed = a - b
    added = b - a
    removed_pairs = {(s, d) for s, d, _ in removed}
    added_pairs = {(s, d) for s, d, _ in added}
    reversed_added_dir = sorted(added_pairs & {(d, s) for s, d in removed_pairs})
    changed = {m for pair in removed_pairs | added_pairs for m in pair}
    return EdgeDiff(
        added=sorted(added),
        removed=sorted(removed),
        reversed=reversed_added_dir,
        changed_modules=changed,
    )


def summarize(
    sa: LoadedSnapshot, sb: LoadedSnapshot
) -> tuple[EdgeDiff, list[str], list[str]]:
    """(diff, new_files, gone_files) 計算一次——main 與 render 共用（F5）。"""
    return (
        diff_edges(sa.module_edges, sb.module_edges),
        sorted(sb.files - sa.files),
        sorted(sa.files - sb.files),
    )


def extract_ep_claims(ep_path: Path, profile: Profile | None) -> set[str]:
    """EP markdown 內 ``<prefix>/<mod>`` 路徑 mentions（保守 heuristic：
    抽不到如實 NONE，不腦補）。claims regex 由 profile ``[[module]]``
    prefixes 衍生——無 profile／無規則 → 恆 NONE（generic repo 無前綴
    知識，by design）。"""
    assert ep_path.is_file(), (
        f"EP 檔不存在或非檔案：{ep_path}（SM-12——NONE 是檔在但無 mention）"
    )
    return set(claims_re(profile).findall(ep_path.read_text()))


def extract_baseline(ep_path: Path) -> str | None:
    """EP 檔頭 ``baseline: <hash>`` 欄（execution-plan skill 慣例）。"""
    assert ep_path.is_file(), f"EP 檔不存在或非檔案：{ep_path}"
    m = _BASELINE_RE.search(ep_path.read_text())
    return m.group(1) if m else None


def compare_claims(claims: set[str], changed_modules: set[str]) -> ClaimsCompare:
    return ClaimsCompare(
        claimed_and_changed=sorted(claims & changed_modules),
        changed_not_claimed=sorted(changed_modules - claims),
        claimed_not_changed=sorted(claims - changed_modules),
        claims_none=not claims,
    )


def _changed_modules(
    diff: EdgeDiff, new_files: list[str], gone_files: list[str], profile: Profile | None
) -> set[str]:
    """實際變動模組＝邊拓撲變動 ∪ 檔案增刪所屬模組。

    只看邊拓撲會把「模組加檔案但邊不變」誤報為宣稱未動（intent-drift
    假陰性）——檔案級變動也是實際變動。
    """
    return diff.changed_modules | {
        module_of(f, profile) for f in new_files + gone_files
    }


def _fmt_edges(edges: list[Edge], limit: int = 20) -> list[str]:
    lines = [f"- `{s} -> {d}` ({k})" for s, d, k in edges[:limit]]
    if len(edges) > limit:
        lines.append(f"- ... +{len(edges) - limit} more")
    return lines


def _trunc_lines(entries: list[str], limit: int = 20) -> list[str]:
    """截斷一律附尾行（F6）——靜默截斷讓消費者誤以為列表完整。"""
    lines = [f"- {e}" for e in entries[:limit]]
    if len(entries) > limit:
        lines.append(f"- ... +{len(entries) - limit} more")
    return lines


def render_report(
    sa: LoadedSnapshot,
    sb: LoadedSnapshot,
    claims: set[str] | None,
    diff: EdgeDiff,
    new_files: list[str],
    gone_files: list[str],
    profile: Profile | None = None,
) -> str:
    a8, b8 = sa.meta.get("commit", "?")[:8], sb.meta.get("commit", "?")[:8]
    lines = [
        f"# Transition Report: {sb.meta.get('repo', '?')}",
        "",
        f"- before: `{a8}`（{sa.path.name}）",
        f"- after: `{b8}`（{sb.path.name}）",
        f"- module edges: {len(sa.module_edges)} -> {len(sb.module_edges)}"
        f"（+{len(diff.added)} / -{len(diff.removed)} / reversed {len(diff.reversed)}）",
        f"- files: {len(sa.files)} -> {len(sb.files)}（+{len(new_files)} / -{len(gone_files)}）",
        "",
    ]
    if not (diff.added or diff.removed or diff.reversed or new_files or gone_files):
        lines.append("## 無結構變化")
        lines.append("")
        lines.append("兩 snapshot 邊集與檔案集相同（同 commit 或無結構變動）。")
        lines.append("")
        return "\n".join(lines)

    lines.append("## 邊變化")
    lines.append("")
    if diff.added:
        lines.append(f"### added ({len(diff.added)})")
        lines.extend(_fmt_edges(diff.added))
        lines.append("")
    if diff.removed:
        lines.append(f"### removed ({len(diff.removed)})")
        lines.extend(_fmt_edges(diff.removed))
        lines.append("")
    if diff.reversed:
        lines.append(f"### reversed ({len(diff.reversed)})——added 方向")
        lines.extend(_trunc_lines([f"`{s} <-> {d}`" for s, d in diff.reversed]))
        lines.append("")
    if new_files:
        lines.append(f"### new files ({len(new_files)})")
        lines.extend(_trunc_lines(new_files))
        lines.append("")
    if gone_files:
        lines.append(f"### gone files ({len(gone_files)})")
        lines.extend(_trunc_lines(gone_files))
        lines.append("")
    if diff.removed and diff.added:
        lines.append("> 已知未覆蓋：rename 偵測（module 改名表現為 remove+add）。")
        lines.append("")

    lines.append("## EP 宣稱 vs 實際變動")
    lines.append("")
    if claims is None:
        lines.append("未提供 `--ep`（EP 宣稱模組路徑對照省略）。")
    elif not claims:
        lines.append("claims: **NONE**——EP 內無 profile prefix 路徑 mention。")
        lines.append(
            f"- 實際變動模組（供判讀，無宣稱可比對）：{sorted(_changed_modules(diff, new_files, gone_files, profile))}"
        )
    else:
        cmp = compare_claims(
            claims, _changed_modules(diff, new_files, gone_files, profile)
        )
        lines.append(
            f"- 宣稱命中 ({len(cmp.claimed_and_changed)})：{cmp.claimed_and_changed}"
        )
        lines.append(
            f"- 實際超出——EP 沒提卻變了 ({len(cmp.changed_not_claimed)})：{cmp.changed_not_claimed}"
        )
        lines.append(
            f"- 宣稱未動——EP 說要動但沒變 ({len(cmp.claimed_not_changed)})：{cmp.claimed_not_changed}"
        )
    lines.append("")
    return "\n".join(lines)


def render_json(
    sa: LoadedSnapshot,
    sb: LoadedSnapshot,
    claims: set[str] | None,
    diff: EdgeDiff,
    new_files: list[str],
    gone_files: list[str],
    profile: Profile | None = None,
) -> dict[str, Any]:
    out: dict[str, Any] = {
        "_meta": {
            "tool": "code_reality.transition",
            "created_at": datetime.now(UTC).isoformat(),
            "before": sa.meta.get("commit"),
            "after": sb.meta.get("commit"),
            "repo": sb.meta.get("repo"),
        },
        "added": [list(e) for e in diff.added],
        "removed": [list(e) for e in diff.removed],
        "reversed": [list(e) for e in diff.reversed],
        "changed_modules": sorted(
            _changed_modules(diff, new_files, gone_files, profile)
        ),
        "new_files": new_files,
        "gone_files": gone_files,
    }
    if claims is not None:
        cmp = compare_claims(
            claims, _changed_modules(diff, new_files, gone_files, profile)
        )
        out["ep_claims"] = {
            "claims": sorted(claims),
            "claims_none": cmp.claims_none,
            "claimed_and_changed": cmp.claimed_and_changed,
            "changed_not_claimed": cmp.changed_not_claimed,
            "claimed_not_changed": cmp.claimed_not_changed,
        }
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("snapshot_a", type=Path, help="before snapshot（S2 schema）")
    parser.add_argument("snapshot_b", type=Path, help="after snapshot（S2 schema）")
    parser.add_argument(
        "--ep", type=Path, default=None, help="EP markdown（宣稱模組對照）"
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path.cwd(),
        help="repo 根（profile 載入——module/claims 規則來源）",
    )
    parser.add_argument(
        "-o",
        "--output-prefix",
        type=Path,
        default=None,
        help="輸出前綴（預設 transition-<a>..<b>）",
    )
    args = parser.parse_args()

    sa, sb = load_snapshot(args.snapshot_a), load_snapshot(args.snapshot_b)
    profile = load_profile(args.repo)
    if args.ep and profile is None:
        print(
            "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，"
            "宣稱對照不生效（--repo 預設 cwd）"
        )
    claims = extract_ep_claims(args.ep, profile) if args.ep else None
    a8, b8 = sa.meta.get("commit", "?")[:8], sb.meta.get("commit", "?")[:8]

    prefix = args.output_prefix or Path(f"transition-{a8}..{b8}")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    md_path = prefix.with_name(prefix.name + ".md")
    json_path = prefix.with_name(prefix.name + ".json")
    diff, new_files, gone_files = summarize(sa, sb)
    md_path.write_text(
        render_report(sa, sb, claims, diff, new_files, gone_files, profile)
    )
    json_path.write_text(
        json.dumps(
            render_json(sa, sb, claims, diff, new_files, gone_files, profile),
            indent=1,
        )
    )
    print(
        f"[OK] transition {a8} -> {b8}: +{len(diff.added)} / -{len(diff.removed)} /"
        f" reversed {len(diff.reversed)} -> {md_path} + {json_path}"
    )
    if args.ep:
        baseline = extract_baseline(args.ep)
        if baseline:
            print(f"[LOG] EP baseline={baseline}（diff before 應錨定此 commit）")
    print(f"[LOG] rg 'changed_not_claimed' {json_path}")


if __name__ == "__main__":
    main()
