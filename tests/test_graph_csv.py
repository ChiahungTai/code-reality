"""S3 graph_csv 單元測試——graph.db → nodes/links CSV（SM-7 語義）。

POC 對照組＝.agent-tmp/ui/graph_csv.py（1,218 nodes/2,815 links 已抽乾淨可跑；
量級由 integration 對真 graph.db 釘住）。此處釘：exclusions 過濾、edge kind
白名單、pair 聚合、community 多數決導出、degree 不變量。
"""

import csv
import subprocess
import sys
from pathlib import Path

from crg_db import make_crg_db, qualified
from profile_repo import write_mosaic_profile

from code_reality.graph_csv import load, write_csvs


def make_db(tmp_path: Path) -> tuple[Path, Path]:
    """fixture db：2 檔在 repo 內＋1 檔 excluded；多 kind 邊＋白名單外 kind。"""
    repo = (tmp_path / "repo").resolve()
    repo.mkdir()
    write_mosaic_profile(repo)
    db = repo / ".code-review-graph" / "graph.db"
    db.parent.mkdir()
    a, b, s = (
        qualified(repo, "mosaic_alpha/a.py", "A.f"),
        qualified(repo, "mosaic_alpha/b.py", "B.h"),
        qualified(repo, "stubs/s.py", "S.x"),
    )
    ag, bk = (
        qualified(repo, "mosaic_alpha/a.py", "A.g"),
        qualified(repo, "mosaic_alpha/b.py", "B.k"),
    )
    make_crg_db(
        db,
        edges=[
            ("CALLS", a, b),
            ("IMPORTS_FROM", ag, bk),  # 同 pair 第二 kind → 聚合
            ("REFERENCES", a, b),  # 白名單外 kind → 忽略
            ("INHERITS", b, a),  # 反向 pair
            ("CALLS", a, ag),  # self-loop（同檔）→ 忽略
        ],
        communities=[
            (1, "domain", 2, "Python", "desc-domain"),
            (2, "svc", 2, "Python", "desc-svc"),
        ],
        nodes=[
            # File 節點（qualified_name＝絕對路徑——CRG 慣例）
            (
                "a.py",
                None,
                str(repo / "mosaic_alpha/a.py"),
                str(repo / "mosaic_alpha/a.py"),
            ),
            (
                "b.py",
                None,
                str(repo / "mosaic_alpha/b.py"),
                str(repo / "mosaic_alpha/b.py"),
            ),
            ("s.py", None, str(repo / "stubs/s.py"), str(repo / "stubs/s.py")),
            # 成員節點（帶 community_id 供多數決）
            ("A.f", None, a, str(repo / "mosaic_alpha/a.py")),
            ("A.g", None, ag, str(repo / "mosaic_alpha/a.py")),
            ("B.h", None, b, str(repo / "mosaic_alpha/b.py")),
            ("B.k", None, bk, str(repo / "mosaic_alpha/b.py")),
            ("S.x", None, s, str(repo / "stubs/s.py")),
        ],
        node_attrs={
            str(repo / "mosaic_alpha/a.py"): ("File", "Python", 0, None),
            str(repo / "mosaic_alpha/b.py"): ("File", "Python", 0, None),
            str(repo / "stubs/s.py"): ("File", "Python", 0, None),
            a: ("Function", "Python", 0, 1),
            ag: ("Function", "Python", 0, 1),
            b: ("Function", "Python", 0, 2),
            bk: ("Function", "Python", 0, 2),
        },
    )
    return db, repo


class TestLoad:
    def test_files_and_links(self, tmp_path: Path) -> None:
        db, repo = make_db(tmp_path)
        g = load(db, repo)
        # stubs/ excluded → 只剩 a/b 兩檔
        assert sorted(f["path"] for f in g.nodes) == [
            "mosaic_alpha/a.py",
            "mosaic_alpha/b.py",
        ]
        # REFERENCES 忽略＋self-loop 忽略 → 兩條聚合 link
        by_pair = {(e["s"], e["t"]): e["kinds"] for e in g.links}
        assert len(g.links) == 2
        ids = {f["path"]: f["id"] for f in g.nodes}
        assert by_pair[(ids["mosaic_alpha/a.py"], ids["mosaic_alpha/b.py"])] == (
            "CALLS+IMPORTS_FROM"
        )
        assert (
            by_pair[(ids["mosaic_alpha/b.py"], ids["mosaic_alpha/a.py"])] == "INHERITS"
        )

    def test_community_majority_vote(self, tmp_path: Path) -> None:
        """File 節點 community_id 全 NULL——以成員多數決導出。"""
        db, repo = make_db(tmp_path)
        g = load(db, repo)
        comm = {f["path"]: f["community"] for f in g.nodes}
        assert comm["mosaic_alpha/a.py"] == 1
        assert comm["mosaic_alpha/b.py"] == 2

    def test_community_tie_break_smallest_id(self, tmp_path: Path) -> None:
        """平手 tie-break：取 community id 較小者（確定論，與掃描順序無關）。"""
        repo = (tmp_path / "repo").resolve()
        repo.mkdir()
        write_mosaic_profile(repo)
        db = repo / ".code-review-graph" / "graph.db"
        db.parent.mkdir()
        f1 = qualified(repo, "mosaic_alpha/t.py", "T.f")
        f2 = qualified(repo, "mosaic_alpha/t.py", "T.g")
        fp = str(repo / "mosaic_alpha/t.py")
        make_crg_db(
            db,
            nodes=[
                ("T.f", None, f1, fp),
                ("T.g", None, f2, fp),
                ("t.py", None, fp, fp),
            ],
            node_attrs={
                fp: ("File", "Python", 0, None),
                f1: ("Function", "Python", 0, 2),
                f2: ("Function", "Python", 0, 1),
            },
        )
        g = load(db, repo)
        assert g.nodes[0]["community"] == 1  # 1v1 平手 → id 小者


class TestWriteCsvs:
    def test_headers_and_rows(self, tmp_path: Path) -> None:
        db, repo = make_db(tmp_path)
        g = load(db, repo)
        nodes_p, links_p = write_csvs(g, tmp_path / "out")
        with open(nodes_p) as fh:
            rows = list(csv.DictReader(fh))
        assert list(rows[0].keys()) == [
            "id",
            "label",
            "community",
            "community_name",
            "lang",
            "is_test",
            "degree",
        ]
        by_label = {r["label"]: r for r in rows}
        assert by_label["a.py"]["community"] == "1"
        assert by_label["a.py"]["community_name"] == "domain"
        # degree＝link 數（a：出 1 ＋ 入 1）
        assert by_label["a.py"]["degree"] == "2"
        with open(links_p) as fh:
            lrows = list(csv.DictReader(fh))
        assert list(lrows[0].keys()) == ["source", "target", "kind"]
        # degree 不變量：Σdegree == 2 × links
        assert sum(int(r["degree"]) for r in rows) == 2 * len(lrows)


class TestCli:
    def test_end_to_end(self, tmp_path: Path) -> None:
        _db, repo = make_db(tmp_path)
        out = tmp_path / "csv-out"
        r = subprocess.run(
            [
                sys.executable,
                "-m",
                "code_reality.graph_csv",
                "--repo",
                str(repo),
                "--out-dir",
                str(out),
            ],
            cwd=tmp_path,
            capture_output=True,
            text=True,
            check=False,
        )
        assert r.returncode == 0, r.stderr
        assert (out / "graph-nodes.csv").exists()
        assert (out / "graph-links.csv").exists()
        with open(out / "graph-nodes.csv") as fh:
            assert len(list(csv.DictReader(fh))) == 2
