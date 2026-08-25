"""chain tour——callchain 文檔 → 每場景一條 CodeTour `.tour`。

文檔保鮮的機械基礎：文檔錨在一個 commit 後漂移，graph.db 重錨自動給新行號
（same/moved/moved-file 三態）。渲染消費者＝CodeTour（vanilla 讀 `.tours/`；
html viewer 已退役，`.agent-tmp/ui/chain_viewer.py` 留 scratch 當解析機械
來源）。

用法::

    uv run python -m code_reality.chain_tour <chain.md> \
        [--graph <graph.db>] [--repo PATH] \
        [--out-dir .tours/arch/<md-stem>] [--primary 1,3]

映射規則（EP ep-code-reality-ui S2）：幀 DFS 序＝步序；步 title 保樹狀前綴；
步 line 用 graph 重錨優先（moved→新行號、moved-file→新檔行）；無錨幀
（無錨／外部／撞名——launchd/shell 類無 ``.py``/``.rs`` 錨、外部路徑無法解析、
同名撞名無法唯一定位）跳過、tour 描述記實際原因分佈。depth 用 stack 推導（修正 POC
``pl//3`` 對「│+4 空格」縮排的跳層失真）。
"""

import argparse
import json
import re
import sqlite3
from collections.abc import Set as AbstractSet
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from code_reality import tour_manifest
from code_reality.common import (
    anchor_pattern,
    assert_db_unchanged,
    connect_ro,
    db_mtime_ns,
    graph_db_path,
)
from code_reality.exclusions import is_excluded
from code_reality.profile import load_profile

REF_RE = re.compile(r"([\w./\-]+\.(?:py|rs)):(\d+)")
TREE_PREFIX_CHARS = set("│├└─ ")


def prefix_len(line: str) -> int:
    n = 0
    for ch in line:
        if ch in TREE_PREFIX_CHARS:
            n += 1
        else:
            break
    return n


def parse_blocks(text: str) -> list[dict[str, Any]]:
    """含樹狀幀行（├/└）的 code block＋其最近前置標題（=場景名）。"""
    blocks: list[dict[str, Any]] = []
    cur: dict[str, Any] | None = None
    heading, in_code = "", False
    for ln in text.splitlines():
        if not in_code and ln.lstrip().startswith("#"):
            heading = ln.lstrip("# ").strip()
        if ln.strip().startswith("```"):
            if not in_code:
                cur = {"heading": heading, "lines": []}
                in_code = True
            else:
                if cur and any(("├" in line or "└" in line) for line in cur["lines"]):
                    blocks.append(cur)
                cur, in_code = None, False
            continue
        if in_code and cur is not None:
            cur["lines"].append(ln)
    return blocks


def best_ident(symbol: str) -> str:
    """symbol 文本最佳 ident：呼叫狀（尾接 ``(``）優先、再最長；取末段。"""
    cands = []
    for m in re.finditer(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]*)*", symbol):
        nxt = symbol[m.end() : m.end() + 1]
        cands.append((nxt == "(", len(m.group(0)), m.group(0)))
    if not cands:
        return ""
    best = max(cands, key=lambda c: (c[0], c[1]))[2]
    return best.rsplit(".", 1)[-1]


def parse_frames(block: dict[str, Any]) -> list[dict[str, Any]]:
    """樹狀幀行 → frame dict（depth 由 stack 推導——非 pl//3）。"""
    frames: list[dict[str, Any]] = []
    stack: list[tuple[int, int]] = []  # (prefix_len, frame idx)
    for raw in block["lines"]:
        if not raw.strip():
            continue
        pl = prefix_len(raw)
        content = raw[pl:].strip()
        if not content:
            continue
        while stack and stack[-1][0] >= pl:
            stack.pop()
        parent = stack[-1][1] if stack else None
        depth = len(stack)
        note = ""
        if "  # " in content or content.startswith("#"):
            content, _, note = content.partition("#")
            content, note = content.strip(), note.strip()
        ref = REF_RE.search(content)
        path, line_no = (ref.group(1), int(ref.group(2))) if ref else (None, None)
        symbol = REF_RE.sub("", content, count=1).strip() if ref else content
        frames.append(
            {
                "depth": depth,
                "parent": parent,
                "symbol": symbol,
                "ident": best_ident(symbol),
                "path": path,
                "line": line_no,
                "note": note,
                "prefix": raw[:pl],
            }
        )
        stack.append((pl, len(frames) - 1))
    return frames


class PathResolver:
    """package 相對路徑 → 絕對路徑（direct → suffix → ctx 多數 → ambiguous）。

    多 prefix profile 邊界：direct 依序試各 pkg（先到先贏）；name_matches
    跨 pkg 合併時後到 prefix 的同名鍵覆蓋先到。單 prefix（mosaic 現行）
    無此效應——自曝慣例同 claims_re docstring。
    """

    def __init__(self, repo_root: Path) -> None:
        self.repo_root = repo_root.resolve()
        profile = load_profile(self.repo_root)
        # package 根＝profile [[module]] prefixes（F3 參數化）；無 profile →
        # repo 根 generic fallback（SM-1a）
        self._profile = profile
        self._pkg_roots = [
            self.repo_root / rule.prefix.rstrip("/")
            for rule in (profile.modules if profile is not None else ())
        ] or [self.repo_root]
        self._ctx_dirs: dict[str, int] = {}

    def _bump_dir(self, p: Path) -> None:
        d = p.parent.as_posix()
        self._ctx_dirs[d] = self._ctx_dirs.get(d, 0) + 1

    def resolve(self, path: str) -> tuple[Path | None, str]:
        for pkg in self._pkg_roots:
            direct = pkg / path
            if direct.exists():
                self._bump_dir(direct)
                return direct, "direct"
        name_matches: dict[str, Path] = {}
        for pkg in self._pkg_roots:
            for p in pkg.rglob(Path(path).name):
                # generic fallback（pkg_roots=[repo_root]）會掃全 repo——
                # .venv/ 等排除前綴下的同名檔不得進 pool（fresh-eyes FF3）
                rel = p.relative_to(self.repo_root).as_posix()
                if not is_excluded(rel, self._profile):
                    name_matches[p.as_posix()] = p
        # suffix 匹配須有 / 邊界——`xservices/_market.py` 不得匹配查詢
        # `services/_market.py`（目錄名尾碼撞名）；name-only 全集僅當
        # suffix 落空時的寬鬆 fallback（POC 語意）
        pool = {
            k: v for k, v in name_matches.items() if k == path or k.endswith("/" + path)
        }
        pool = pool or name_matches
        if not pool:
            return None, "none"
        if len(pool) == 1:
            p = next(iter(pool.values()))
            self._bump_dir(p)
            return p, "suffix"
        scored = sorted(
            pool.values(),
            key=lambda p: (-self._ctx_dirs.get(p.parent.as_posix(), 0), p.as_posix()),
        )
        if self._ctx_dirs.get(scored[0].parent.as_posix(), 0) > 0:
            self._bump_dir(scored[0])
            return scored[0], "ctx"
        return None, "ambiguous"


def check_anchor(abs_path: Path, line_no: int, ident: str) -> str:
    """substring 判定：ident 是否還在文檔錨行附近（ok/drift/drift-far/missing）。"""
    if not ident or not abs_path.exists():
        return "nocheck"
    lines = abs_path.read_text(encoding="utf-8").splitlines()
    if line_no - 1 < len(lines) and ident in lines[line_no - 1]:
        return "ok"
    window = lines[max(0, line_no - 9) : line_no + 8]
    if any(ident in w for w in window):
        return "drift"
    if any(ident in w for w in lines):
        return "drift-far"
    return "missing"


def _like(part: str) -> str:
    """LIKE pattern literal 段轉義（%/_/\\——路徑必含 ``_``，不轉義會誤配）。"""
    return part.replace("\\", "\\\\").replace("%", r"\%").replace("_", r"\_")


class GraphAnchor:
    """graph.db 重錨：同檔最近 line_start → 跨檔搬家偵測 → not-in-graph。

    跨檔門檻（POC 紀律）：ident 需 substring-missing 且長度 ≥4——否則多半是
    graph 沒索引的變數/屬性在別檔撞名。
    """

    def __init__(self, db: Path, repo_root: Path) -> None:
        self.db = db
        self.repo_root = repo_root.resolve()
        self.conn: sqlite3.Connection = connect_ro(db)

    def close(self) -> None:
        self.conn.close()

    def anchor(
        self,
        abs_path: Path,
        line_no: int,
        ident: str,
        substring_status: str = "",
    ) -> dict[str, Any]:
        rel = abs_path.relative_to(self.repo_root).as_posix()
        hit = self.conn.execute(
            "SELECT line_start FROM nodes "
            "WHERE file_path LIKE ? ESCAPE '\\' AND name=? "
            "AND line_start IS NOT NULL "
            "ORDER BY ABS(line_start-?) LIMIT 1",
            (f"%/{_like(rel)}", ident, line_no),
        ).fetchone()
        if hit is not None and hit[0] is not None:
            delta = hit[0] - line_no
            return {
                "g": "same" if delta == 0 else "moved",
                "g_line": hit[0],
                "g_delta": delta,
            }
        if substring_status != "missing" or len(ident) < 4:
            return {"g": "not-in-graph"}
        prefix = _like(f"{self.repo_root}/")
        rows = self.conn.execute(
            "SELECT DISTINCT substr(file_path, length(?)+2) FROM nodes "
            "WHERE name=? AND file_path LIKE ? ESCAPE '\\' LIMIT 6",
            (str(self.repo_root), ident, f"{prefix}%"),
        ).fetchall()
        rels = [r[0] for r in rows if r[0]]
        if len(rels) == 1:
            ln = self.conn.execute(
                "SELECT MIN(line_start) FROM nodes WHERE name=? "
                "AND file_path LIKE ? ESCAPE '\\'",
                (ident, f"{prefix}{_like(rels[0])}"),
            ).fetchone()
            return {
                "g": "moved-file",
                "g_file": rels[0],
                "g_line": ln[0] if ln and ln[0] is not None else None,
            }
        if len(rels) > 1:
            return {"g": "moved-file-ambiguous", "g_files": rels[:4]}
        return {"g": "not-in-graph"}


@dataclass
class ScenarioTours:
    tours: list[dict[str, Any]]
    frames: int
    skipped: int
    g_counts: dict[str, int]


def _step_of(
    f: dict[str, Any], repo_root: Path, lines_cache: dict[str, list[str]]
) -> dict[str, Any]:
    file_rel = f["abs_path"].relative_to(repo_root).as_posix()
    line = f["line"] or 1
    parts: list[str] = []
    g = f["g"]
    if g == "moved":
        line = f["g_line"]
        delta = f["g_delta"]
        parts.append(f"graph {'+' if delta > 0 else ''}{delta} → :{f['g_line']}")
        parts.append(f"文檔錨 {f['path']}:{f['line']}")
    elif g == "moved-file":
        file_rel = f["g_file"]
        line = f["g_line"] or 1
        parts.append(f"文檔錨 {f['path']}:{f['line']}，已搬家 {f['g_file']}")
    elif g == "same":
        parts.append("graph ✓ 行號一致")
        parts.append(f"文檔錨 {f['path']}:{f['line']}")
    elif g == "moved-file-ambiguous":
        parts.append(f"同名多檔無法自動判定：{f.get('g_files', [])}")
        parts.append(f"文檔錨 {f['path']}:{f['line']}")
    elif f["status"] in ("drift", "drift-far", "missing"):
        parts.append(f"graph 未索引；substring 判定 {f['status']}")
        parts.append(f"文檔錨 {f['path']}:{f['line']}")
    else:
        parts.append(f"文檔錨 {f['path']}:{f['line']}")
    if f["note"]:
        parts.append(f"註：{f['note']}")
    step = {
        "file": file_rel,
        "line": line,
        "title": f["prefix"] + f["symbol"],
        "description": "\n".join(p for p in parts if p),
    }
    # pattern：對最終 file/line 讀行（moved/moved-file 已換重錨後座標——pattern
    # 描述最終錨行）；同檔快取避免逐幀重讀。讀不到（graph 世代落後、搬家目標
    # 已不存在/非 UTF8）、越界或空行 → 不發射——與 delta 省略語義對齊
    p = repo_root / file_rel
    key = p.as_posix()
    if key not in lines_cache:
        try:
            # split("\n") 非 splitlines()：graph line_start 只數 \n——
            # \r/\x0c 等切行會讓行索引位移、pattern 錯行
            lines_cache[key] = p.read_text(encoding="utf-8").split("\n")
        except (OSError, UnicodeDecodeError):
            lines_cache[key] = []
    lines = lines_cache[key]
    if 0 <= line - 1 < len(lines) and lines[line - 1].strip():
        step["pattern"] = anchor_pattern(lines[line - 1])
    return step


def build_tours(
    chain_md: Path, repo_root: Path, graph_db: Path | None = None
) -> ScenarioTours:
    """chain md → 每場景一條 tour（無錨幀跳過、重錨統計）。"""
    repo_root = repo_root.resolve()
    blocks = parse_blocks(chain_md.read_text(encoding="utf-8"))
    assert blocks, (
        f"非 callstack 文檔格式（無樹狀 code block——需含 ├/└ 幀行）：{chain_md}"
        "；參考 ai-analysis/blueprint/callstack-v1/ 慣例"
    )
    resolver = PathResolver(repo_root)
    m0 = db_mtime_ns(graph_db) if graph_db is not None else 0
    ga = GraphAnchor(graph_db, repo_root) if graph_db is not None else None
    tours: list[dict[str, Any]] = []
    lines_cache: dict[str, list[str]] = {}  # 跨場景共用——同檔多幀只讀一次
    frames_total = skipped_total = 0
    g_counts: dict[str, int] = {}
    try:
        for block in blocks:
            frames = parse_frames(block)
            steps: list[dict[str, Any]] = []
            skipped = 0
            skip_examples: list[str] = []
            skip_reasons: dict[str, int] = {}
            scen_g: dict[str, int] = {}
            for f in frames:
                f["abs_path"] = None
                f["status"] = "noref"
                f["g"] = "noref"
                if f["path"]:
                    abs_path, kind = resolver.resolve(f["path"])
                    if abs_path is None:
                        f["status"] = "external" if kind == "none" else "unresolved"
                    else:
                        f["abs_path"] = abs_path
                        f["status"] = check_anchor(abs_path, f["line"], f["ident"])
                        if ga is not None and f["ident"]:
                            f.update(
                                ga.anchor(abs_path, f["line"], f["ident"], f["status"])
                            )
                g_counts[f["g"]] = g_counts.get(f["g"], 0) + 1
                scen_g[f["g"]] = scen_g.get(f["g"], 0) + 1
                if f["abs_path"] is None:
                    skipped += 1
                    skip_reasons[f["status"]] = skip_reasons.get(f["status"], 0) + 1
                    if len(skip_examples) < 3:
                        skip_examples.append(f["symbol"][:30])
                    continue
                steps.append(_step_of(f, repo_root, lines_cache))
            dist = " ".join(f"{k}:{v}" for k, v in sorted(scen_g.items()))
            reasons = (
                "／".join(f"{k} {v}" for k, v in sorted(skip_reasons.items())) or "無"
            )
            tours.append(
                {
                    "title": block["heading"],
                    "description": (
                        f"{len(frames)} 幀 → {len(steps)} 步；{skipped} 幀跳過"
                        f"（{reasons}——例：{'、'.join(skip_examples) or '無'}）。\n"
                        f"graph 重錨分佈：{dist}"
                    ),
                    "steps": steps,
                }
            )
            frames_total += len(frames)
            skipped_total += skipped
    finally:
        if ga is not None:
            ga.close()
    if graph_db is not None:
        assert_db_unchanged(graph_db, m0)
    return ScenarioTours(
        tours=tours, frames=frames_total, skipped=skipped_total, g_counts=g_counts
    )


def write_tours(
    st: ScenarioTours, out_dir: Path, *, primary: AbstractSet[int] = frozenset()
) -> list[Path]:
    """寫檔：``{NN}.tour`` 純序號（user 裁定 2026-08-23——族目錄名承載語義、
    檔名僅穩定鍵，無截斷），JSON title 帶 ``NN - `` 前綴——上游
    連鎖 parse 側 regex ``^#?(\\d+)\\s+-`` 可解析（Number("01")=1）。fork
    連鎖**尋找側**模板已補零（``^#?0*N\\s+[-:]``，codetour
    src/player/index.ts）——補零 title 的 Next/Previous 連鎖生效。

    前綴在 emission 層加——記憶體 title 保持 raw heading，防雙重編號。
    primary（1-based 場景號集合）成員帶
    ``"isPrimary": true``：補零編號 corpus 的唯一有效 primary 機制（上游
    ``1 - `` 偵測只認未補零），預設不標（user 08-22 裁決——primary 是
    corpus 級編輯決策，非單次生成能知）。
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    paths = []
    for i, tour in enumerate(st.tours, 1):
        p = out_dir / f"{i:02d}.tour"
        emitted = dict(tour)
        emitted["title"] = f"{i:02d} - {tour['title']}"
        if i in primary:
            emitted["isPrimary"] = True
        p.write_text(
            json.dumps(emitted, ensure_ascii=False, indent=1), encoding="utf-8"
        )
        paths.append(p)
    legacy = sorted(out_dir.glob("chain-*.tour"))
    if legacy:
        print(
            f"[WARN] 舊檔名格式殘留 {len(legacy)} 檔（chain-*.tour）——新舊同 title "
            "會使 player 撞鍵靜默落第一條（corpus 靜默雙份）；重錨過渡＝刪舊檔"
            f"＋manifest 重建（rm {out_dir}/chain-*.tour 後重產或 init-scan）"
        )
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("chain_md", type=Path, help="callchain 文檔（樹狀 code block）")
    parser.add_argument(
        "--graph",
        type=Path,
        default=None,
        help="CRG graph.db（預設 <repo>/.code-review-graph/graph.db；不存在則退化純文檔錨）",
    )
    parser.add_argument("--repo", type=Path, default=Path.cwd(), help="repo 根")
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help="輸出目錄（預設 .tours/arch/<文檔 stem>/——stem 即 subgroup label）",
    )
    parser.add_argument(
        "--primary",
        default="",
        help="標 isPrimary 的場景編號（1-based 逗號分隔）；預設不標",
    )
    args = parser.parse_args()

    graph_db = args.graph
    if graph_db is None:
        default_db = graph_db_path(args.repo)
        graph_db = default_db if default_db.exists() else None
        if graph_db is None:
            print(f"[WARN] graph.db 不存在（{default_db}）——退化純文檔錨，無重錨")
    else:
        assert graph_db.exists(), f"--graph 指定但不存在：{graph_db}"

    out_dir = (
        args.out_dir
        if args.out_dir is not None
        else Path(".tours") / "arch" / args.chain_md.stem
    )
    primary = {int(x) for x in args.primary.split(",") if x.strip()}
    st = build_tours(args.chain_md, args.repo, graph_db)
    valid = set(range(1, len(st.tours) + 1))
    assert not (primary - valid), (
        f"--primary 越界（共 {len(st.tours)} 場景）："
        f"{sorted(primary - valid)}——輸入錯誤要大聲"
    )
    paths = write_tours(st, out_dir, primary=primary)
    for p in paths:
        print(f"[OK] chain tour -> {p}")
    # corpus provenance：generator 原生寫 manifest（derived/curated 二分的機械載體）
    out_abs = out_dir.resolve()
    mroot = tour_manifest.tours_root_of(out_abs)
    if mroot.name != ".tours":
        print(
            f"[WARN] manifest skip: out-dir 不在 .tours/ 樹內（resolved root={mroot}）"
            "——tour 檔照寫，provenance 不記（暫存/dry-run 目錄零 manifest 副作用）"
        )
    else:
        mpath = mroot / "manifest.toml"
        mdata = tour_manifest.load(mpath)
        mdata.setdefault("version", 1)
        mdata.setdefault("tour", {})
        try:
            src_rel = str(args.chain_md.resolve().relative_to(args.repo.resolve()))
        except ValueError:
            src_rel = str(args.chain_md.resolve())
        commit = tour_manifest.git_head(args.repo)
        for p in paths:
            tour_manifest.upsert(
                mdata,
                p.resolve().relative_to(mroot).as_posix(),
                generator="chain_tour",
                sources=[src_rel],
                commit=commit,
            )
        tour_manifest.dump(mpath, mdata)
        print(
            f"[OK] manifest upsert: {mpath}（{len(paths)} rows, generator=chain_tour）"
        )
    print(
        f"[OK] chain tours: {len(st.tours)} 場景 / {st.frames} 幀 / "
        f"{st.frames - st.skipped} 步 / skipped {st.skipped}"
    )
    print(f"[LOG] graph 重錨分佈: {st.g_counts}")


if __name__ == "__main__":
    main()
