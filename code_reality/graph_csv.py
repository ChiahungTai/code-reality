"""graph CSV export——CRG graph.db → nodes/links CSV（Cosmograph 餵料）。

純資料資產無 UI（退役裁決：圖是資料不是介面——想看圖就把 CSV 拖進
Cosmograph 玩，無 UI 維護負擔）。File 節點的 community_id 在 CRG 全 NULL
（Leiden 在 function/class 層）——以「該檔成員的多數 community」導出
file-level 歸屬。

用法::

    uv run python -m code_reality.graph_csv [--repo PATH] [--out-dir DIR]

輸出 ``<out-dir>/graph-nodes.csv`` ＋ ``graph-links.csv``（預設 ``.agent-tmp/``，
按需重產）。
"""

import argparse
import csv
import sqlite3
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from code_reality.common import (
    EDGE_KINDS,
    assert_db_unchanged,
    connect_ro,
    db_mtime_ns,
    graph_db_path,
)
from code_reality.exclusions import is_excluded
from code_reality.profile import load_profile


@dataclass
class GraphCsv:
    nodes: list[dict[str, Any]]
    links: list[dict[str, Any]]
    communities: dict[int, str]


def load(db_path: Path, repo_root: Path) -> GraphCsv:
    """graph.db → 檔級 nodes/links（exclusions 過濾＋community 多數決）。"""
    repo_root = repo_root.resolve()
    repo = f"{repo_root}/"
    profile = load_profile(repo_root)
    m0 = db_mtime_ns(db_path)
    conn: sqlite3.Connection = connect_ro(db_path)
    try:
        file_ids: dict[str, int] = {}
        qual_file: dict[str, str] = {}
        for nid, kind, qual, fp in conn.execute(
            "SELECT id, kind, qualified_name, file_path FROM nodes"
        ):
            qual_file[qual] = fp or ""
            if kind == "File" and fp:
                file_ids[fp] = nid

        # File 節點的 community_id 全 NULL——以成員多數決導出 file-level 歸屬
        file_comm_votes: dict[str, dict[int, int]] = {}
        for fp, cid in conn.execute(
            "SELECT file_path, community_id FROM nodes "
            "WHERE community_id IS NOT NULL AND kind != 'File'"
        ):
            if fp:
                votes = file_comm_votes.setdefault(fp, {})
                votes[cid] = votes.get(cid, 0) + 1

        def file_community(fp: str) -> int | None:
            votes = file_comm_votes.get(fp)
            # 平手 tie-break：community id 最小者（scan 順序無關、可重現）
            return min(votes, key=lambda c: (-votes[c], c)) if votes else None

        def keep(fp: str) -> bool:
            return fp.startswith(repo) and not is_excluded(fp[len(repo) :], profile)

        files = [
            {
                "id": r[0],
                "name": r[2],
                "path": r[3][len(repo) :],
                "lang": r[4] or "",
                "is_test": bool(r[5]),
                "community": file_community(r[3]),
            }
            for r in conn.execute(
                "SELECT id, kind, name, file_path, language, is_test "
                "FROM nodes WHERE kind='File'"
            )
            if keep(r[3] or "")
        ]
        file_set = {f["path"] for f in files}

        def proj(qual: str) -> str | None:
            fp = qual_file.get(qual)
            if fp:
                fp = fp.removeprefix(repo)
                return fp if fp in file_set else None
            base = qual.split("::")[0]
            base = base.removeprefix(repo)
            return base if base in file_set else None

        pair: dict[tuple[int, int], dict[str, Any]] = {}
        for kind, sq, tq in conn.execute(
            f"SELECT kind, source_qualified, target_qualified FROM edges "
            f"WHERE kind IN ({','.join('?' * len(EDGE_KINDS))})",
            EDGE_KINDS,
        ):
            sp, tp = proj(sq), proj(tq)
            if not sp or not tp or sp == tp:
                continue
            s, t = file_ids[f"{repo_root}/{sp}"], file_ids[f"{repo_root}/{tp}"]
            e = pair.setdefault((s, t), {"kinds": set()})
            e["kinds"].add(kind)
        links = [
            {"s": s, "t": t, "kinds": "+".join(sorted(v["kinds"]))}
            for (s, t), v in pair.items()
        ]
        communities = {
            r[0]: r[1] for r in conn.execute("SELECT id, name FROM communities")
        }
    finally:
        conn.close()
    assert_db_unchanged(db_path, m0)
    return GraphCsv(nodes=files, links=links, communities=communities)


def degrees(links: list[dict[str, Any]]) -> dict[int, int]:
    """link 聚合後的 undirected degree（Σdegree == 2 × links 不變量）。"""
    deg: dict[int, int] = {}
    for e in links:
        deg[e["s"]] = deg.get(e["s"], 0) + 1
        deg[e["t"]] = deg.get(e["t"], 0) + 1
    return deg


def write_csvs(g: GraphCsv, out_dir: Path) -> tuple[Path, Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    deg = degrees(g.links)
    nodes_path = out_dir / "graph-nodes.csv"
    with open(nodes_path, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(
            ["id", "label", "community", "community_name", "lang", "is_test", "degree"]
        )
        for f in g.nodes:
            w.writerow(
                [
                    f["id"],
                    f["name"],
                    f["community"],
                    g.communities.get(f["community"], ""),
                    f["lang"],
                    int(f["is_test"]),
                    deg.get(f["id"], 0),
                ]
            )
    links_path = out_dir / "graph-links.csv"
    with open(links_path, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["source", "target", "kind"])
        for e in g.links:
            w.writerow([e["s"], e["t"], e["kinds"]])
    return nodes_path, links_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path.cwd(),
        help="repo 根（含 .code-review-graph/）",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path(".agent-tmp"),
        help="輸出目錄（預設 .agent-tmp/——CSV 是按需重產的玩圖資產）",
    )
    args = parser.parse_args()

    db_path = graph_db_path(args.repo)
    assert db_path.exists(), (
        f"graph.db 不存在：{db_path}——先跑 `uvx code-review-graph build`"
    )
    g = load(db_path, args.repo)
    nodes_path, links_path = write_csvs(g, args.out_dir)
    print(
        f"[OK] graph csv: {len(g.nodes)} nodes / {len(g.links)} links -> "
        f"{nodes_path.name} + {links_path.name}（{args.out_dir}）"
    )


if __name__ == "__main__":
    main()
