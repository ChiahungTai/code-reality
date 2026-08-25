"""Cross-language parity harness: frozen Python graph family vs Rust carrier.

Six tools (snapshot/transition/graph_audit/graph_csv/scip_refs --audit) run
on identical synthetic inputs (git repo + CRG-compatible db via
tests/fixtures/crg_db.py); stdout bytes + exit codes are compared (stderr is
management-only). Environment-absent cases are valid equivalence: both sides
fail loud with the same exit (e.g. rust-analyzer missing → exit 2 on both).
Usage (`-h`) faces compare with the prog prefix normalized (argparse embeds
the invocation form; the description body is byte-compared) — the Rust side
is additionally byte-pinned in cargo tests.

EP: ai-analysis/execution-plans/ep-rust-r4-graph-family.md S6 (master R4①).
"""

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tests" / "fixtures"))
from crg_db import make_crg_db

pytestmark = pytest.mark.parity


def _rust_bin() -> Path:
    bin_path = REPO / "target" / "release" / "code-reality"
    if not bin_path.exists():
        subprocess.run(
            ["cargo", "build", "--release", "-p", "code-reality"],
            cwd=REPO,
            check=True,
            capture_output=True,
        )
    assert bin_path.exists()
    return bin_path


def run_python(module: str, args: list[str]) -> tuple[str, str, int]:
    proc = subprocess.run(
        [sys.executable, "-m", f"code_reality.{module}", *args],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout, proc.stderr, proc.returncode


def run_rust(sub: str, args: list[str]) -> tuple[str, str, int]:
    proc = subprocess.run(
        [str(_rust_bin()), sub, *args],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout, proc.stderr, proc.returncode


def normalize(out: str, tmp: Path) -> str:
    out = out.replace(str(tmp / "py"), "<S>").replace(str(tmp / "rs"), "<S>").replace(str(tmp / "p"), "<S>").replace(str(tmp / "r"), "<S>")
    return out.replace(str(tmp), "<T>")


def git(repo: Path, *args: str) -> None:
    env = {
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@t",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@t",
    }
    subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True, env=env
    )


@pytest.fixture()
def tmp():
    with tempfile.TemporaryDirectory() as d:
        yield Path(d)


def make_repo(tmp: Path, name: str = "repo") -> Path:
    # canonical up front: export relativizes against the RESOLVED root, so
    # synthetic qualified paths must be real paths (pytest tmp_path on macOS
    # is already canonical; tempfile.TemporaryDirectory is not)
    repo = (tmp / name).resolve()
    (repo / "pkg" / "alpha").mkdir(parents=True)
    (repo / "pkg" / "beta").mkdir(parents=True)
    (repo / ".code-review-graph").mkdir()
    (repo / ".code-reality.toml").write_text(
        '[[module]]\nprefix = "pkg/"\nexclude = [".agent-tmp/"]\n'
    )
    git(repo, "init", "-q")
    git(repo, "add", ".")
    git(repo, "commit", "-qm", "init")
    return repo


def head_sha(repo: Path) -> str:
    out = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return out.stdout.strip()


def edges_for(repo: Path, sha: str, fresh: bool) -> None:
    """CRG db with two structural edges (fresh=True anchors HEAD)."""
    def q(rel: str, sym: str = "Cls.method") -> str:
        return f"{repo / rel}::{sym}"

    make_crg_db(
        repo / ".code-review-graph" / "graph.db",
        edges=[
            ("IMPORTS_FROM", q("pkg/alpha/a.py"), q("pkg/beta/b.py")),
            ("CALLS", q("pkg/alpha/a.py"), q("pkg/alpha/c.py")),
        ],
        metadata={"git_head_sha": sha if fresh else "0" * 40},
    )


def parity(tool_py: str, sub_rs: str, args_py: list[str], args_rs: list[str], tmp: Path):
    py_out, _, py_code = run_python(tool_py, args_py)
    rs_out, _, rs_code = run_rust(sub_rs, args_rs)
    assert py_code == rs_code, f"exit codes differ: py={py_code} rs={rs_code}\npy:{py_out}\nrs:{rs_out}"
    assert normalize(py_out, tmp) == normalize(rs_out, tmp), (
        f"stdout bytes differ\n--- py ---\n{py_out}\n--- rs ---\n{rs_out}"
    )
    return py_out, py_code


class TestSnapshotParity:
    def test_ok_and_log_lines(self, tmp):
        repo = make_repo(tmp)
        edges_for(repo, head_sha(repo), fresh=True)
        py_out, _ = parity(
            "snapshot", "snapshot",
            ["--repo", str(repo), "--out-dir", str(tmp / "py")],
            ["--repo", str(repo), "--out-dir", str(tmp / "rs")],
            tmp,
        )
        assert py_out.startswith("[OK] snapshot: 3 files, 1 module edges -> ")

    def test_stale_warn_face(self, tmp):
        repo = make_repo(tmp)
        edges_for(repo, head_sha(repo), fresh=False)
        py_out, _ = parity(
            "snapshot", "snapshot",
            ["--repo", str(repo), "--out-dir", str(tmp / "py")],
            ["--repo", str(repo), "--out-dir", str(tmp / "rs")],
            tmp,
        )
        assert py_out.startswith("[WARN] CRG graph stale: graph sha 00000000 != HEAD ")

    def test_empty_set_warn(self, tmp):
        repo = make_repo(tmp)
        make_crg_db(
            repo / ".code-review-graph" / "graph.db",
            edges=[("IMPORTS_FROM", "/elsewhere/x.py::A", "/elsewhere/y.py::B")],
            metadata={"git_head_sha": head_sha(repo)},
        )
        py_out, _ = parity(
            "snapshot", "snapshot",
            ["--repo", str(repo), "--out-dir", str(tmp / "py")],
            ["--repo", str(repo), "--out-dir", str(tmp / "rs")],
            tmp,
        )
        assert "[WARN] snapshot 空集合（0 files，db raw 1 邊）" in py_out

    def test_missing_db_crash_exit_1_empty_stdout(self, tmp):
        repo = make_repo(tmp)  # no db
        py_out, code = parity(
            "snapshot", "snapshot",
            ["--repo", str(repo), "--out-dir", str(tmp / "py")],
            ["--repo", str(repo), "--out-dir", str(tmp / "rs")],
            tmp,
        )
        assert code == 1
        assert py_out == ""

    def test_subdir_repo_missing_db_crash(self, tmp):
        repo = make_repo(tmp)
        sub = repo / "pkg"
        py_out, code = parity(
            "snapshot", "snapshot",
            ["--repo", str(sub), "--out-dir", str(tmp / "py")],
            ["--repo", str(sub), "--out-dir", str(tmp / "rs")],
            tmp,
        )
        assert code == 1
        assert py_out == ""


class TestTransitionParity:
    @staticmethod
    def snap(path: Path, commit: str, files: list[str], edges: list[list[str]]) -> Path:
        path.write_text(
            json.dumps(
                {
                    "_meta": {"repo": "r", "commit": commit},
                    "files": files,
                    "module_edges": edges,
                }
            )
        )
        return path

    def test_ok_lines_and_reversal(self, tmp):
        a = self.snap(tmp / "a.json", "aaaa1111", ["pkg/x.py"], [["pkg/a", "pkg/b", "CALLS"]])
        b = self.snap(tmp / "b.json", "bbbb2222", [], [["pkg/b", "pkg/a", "CALLS"]])
        py_out, _ = parity(
            "transition", "transition",
            [str(a), str(b), "-o", str(tmp / "p" / "t")],
            [str(a), str(b), "-o", str(tmp / "r" / "t")],
            tmp,
        )
        assert "[OK] transition aaaa1111 -> bbbb2222: +1 / -1 / reversed 1 -> " in py_out
        assert "[LOG] rg 'changed_not_claimed'" in py_out

    def test_no_change_face(self, tmp):
        a = self.snap(tmp / "a.json", "aaaa1111", ["f.py"], [["m/a", "m/b", "CALLS"]])
        py_out, _ = parity(
            "transition", "transition",
            [str(a), str(a), "-o", str(tmp / "p" / "t")],
            [str(a), str(a), "-o", str(tmp / "r" / "t")],
            tmp,
        )
        assert "[OK] transition aaaa1111 -> aaaa1111: +0 / -0 / reversed 0 -> " in py_out
        md = (tmp / "p" / "t.md").read_text()
        assert "## 無結構變化" in md

    def test_claims_profileless_warn(self, tmp):
        a = self.snap(tmp / "a.json", "aaaa1111", [], [])
        b = self.snap(tmp / "b.json", "bbbb2222", [], [["m/a", "m/b", "CALLS"]])
        ep = tmp / "ep.md"
        ep.write_text("body mentions pkg/alpha/x.py\n")
        py_out, _ = parity(
            "transition", "transition",
            [str(a), str(b), "--ep", str(ep), "--repo", str(tmp), "-o", str(tmp / "p" / "t")],
            [str(a), str(b), "--ep", str(ep), "--repo", str(tmp), "-o", str(tmp / "r" / "t")],
            tmp,
        )
        assert "[WARN] claims 恆 NONE" in py_out

    def test_missing_ep_crash(self, tmp):
        a = self.snap(tmp / "a.json", "aaaa1111", [], [])
        b = self.snap(tmp / "b.json", "bbbb2222", [], [])
        py_out, code = parity(
            "transition", "transition",
            [str(a), str(b), "--ep", str(tmp / "nope.md")],
            [str(a), str(b), "--ep", str(tmp / "nope.md")],
            tmp,
        )
        assert code == 1
        assert py_out == ""

    def test_warn_survives_crash_when_profileless(self, tmp):
        # cwd-independent: --repo points at a dir with NO profile → the
        # WARN prints to stdout BEFORE the missing-EP crash (Python order)
        a = self.snap(tmp / "a.json", "aaaa1111", [], [])
        b = self.snap(tmp / "b.json", "bbbb2222", [], [["m/a", "m/b", "CALLS"]])
        norepo = tmp / "norepo"
        norepo.mkdir()
        py_out, code = parity(
            "transition", "transition",
            [str(a), str(b), "--ep", str(tmp / "nope.md"), "--repo", str(norepo)],
            [str(a), str(b), "--ep", str(tmp / "nope.md"), "--repo", str(norepo)],
            tmp,
        )
        assert code == 1
        assert py_out == (
            "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，"
            "宣稱對照不生效（--repo 預設 cwd）\n"
        )


RISK_RS = """\
pub struct Thing;

impl Thing {
    pub fn zebra(&self) -> Thing {
        Thing
    }
    pub fn dup(&self) -> Thing {
        Thing
    }
}

impl Clone for Thing {
    fn clone(&self) -> Thing {
        Thing
    }
}

impl Copy for Thing {
    fn zebra(&self) -> Thing {
        Thing
    }
    fn dup(&self) -> Thing {
        Thing
    }
}
"""


class TestGraphAuditParity:
    def _risk_repo(self, tmp: Path, db_nodes: int) -> Path:
        repo = make_repo(tmp)
        src = repo / "pkg" / "alpha" / "thing.rs"
        src.parent.mkdir(parents=True, exist_ok=True)
        src.write_text(RISK_RS)
        git(repo, "add", ".")
        git(repo, "commit", "-qm", "rs")
        # db side: `dup` risk file (2 impl blocks); node count decides
        # clean (≥2) vs missing (<2) — built once, all nodes in one db
        # `clone`/`zebra` always complete in the db; `db_nodes` controls
        # only the dup count (RA sees dup×2 + zebra×2 + clone×1; the
        # overlap face first-seen order is [zebra, dup] — sorted output
        # is byte-pinned by this fixture)
        nodes: list[tuple[str, str | None, str, str]] = [
            ("clone", None, f"{src}::Thing.clone", str(src)),
            ("zebra", None, f"{src}::Thing.zebra0", str(src)),
            ("zebra", None, f"{src}::Thing.zebra1", str(src)),
        ]
        nodes.extend(
            ("dup", None, f"{src}::Thing.dup{i}", str(src)) for i in range(db_nodes)
        )
        attrs: dict[str, tuple[str, str, int, int | None]] = {
            f"{src}::Thing.clone": ("Function", "rust", 0, None),
            f"{src}::Thing.zebra0": ("Function", "rust", 0, None),
            f"{src}::Thing.zebra1": ("Function", "rust", 0, None),
        }
        for i in range(db_nodes):
            attrs[f"{src}::Thing.dup{i}"] = ("Function", "rust", 0, None)
        make_crg_db(
            repo / ".code-review-graph" / "graph.db",
            nodes=nodes,
            metadata={"git_head_sha": head_sha(repo)},
            node_attrs=attrs,
        )
        return repo

    def test_json_clean_and_missing_faces(self, tmp):
        # clean: db has both dup nodes → no gap (exit 0 when RA present;
        # exit 2 both sides when absent — parity either way)
        repo = self._risk_repo(tmp, 2)
        py_out, code = parity(
            "graph_audit", "graph_audit",
            ["--repo", str(repo), "--json"],
            ["--repo", str(repo), "--json"],
            tmp,
        )
        if code == 0:
            payload = json.loads(py_out)
            assert payload["missing"] == []
            assert payload["risk_files"][0]["type"] == "Thing"
            assert payload["risk_files"][0]["overlap"] == ["dup", "zebra"]
        # missing: db short by one → exit 1 + missing array (RA present)
        miss = tmp.parent / f"miss-{tmp.name}"
        repo2 = self._risk_repo(miss, 1)
        py_out2, code2 = parity(
            "graph_audit", "graph_audit",
            ["--repo", str(repo2), "--json"],
            ["--repo", str(repo2), "--json"],
            miss,
        )
        if code2 == 1:
            payload = json.loads(py_out2)
            assert payload["missing"] == [
                {"file": str(repo2 / "pkg" / "alpha" / "thing.rs"),
                 "symbol": "dup", "ra_count": 2, "db_count": 1}
            ]

    def test_human_face(self, tmp):
        repo = self._risk_repo(tmp, 2)
        py_out, _ = parity(
            "graph_audit", "graph_audit",
            ["--repo", str(repo)],
            ["--repo", str(repo)],
            tmp,
        )
        assert py_out.startswith("[OK] D1 風險掃描：1 檔")

    def test_missing_graph_exit_2(self, tmp):
        repo = make_repo(tmp)  # no db
        py_out, code = parity(
            "graph_audit", "graph_audit",
            ["--repo", str(repo)],
            ["--repo", str(repo)],
            tmp,
        )
        assert code == 2
        assert py_out == ""

    def test_missing_repo_flag_exit_2(self, tmp):
        py_out, code = parity(
            "graph_audit", "graph_audit",
            [],
            [],
            tmp,
        )
        assert code == 2
        assert py_out == ""


class TestGraphCsvParity:
    def test_ok_line_and_csv_file_bytes(self, tmp):
        repo = make_repo(tmp)
        def q(rel: str) -> str:
            return f"{repo / rel}::File"

        make_crg_db(
            repo / ".code-review-graph" / "graph.db",
            edges=[
                ("CALLS", f"{repo / 'pkg/alpha/a.py'}::A", f"{repo / 'pkg/beta/b.py'}::B"),
                ("IMPORTS_FROM", f"{repo / 'pkg/beta/b.py'}::B", f"{repo / 'pkg/alpha/a.py'}::A2"),
            ],
            nodes=[
                ("a.py", None, q("pkg/alpha/a.py"), str(repo / "pkg/alpha/a.py")),
                ("b.py", None, q("pkg/beta/b.py"), str(repo / "pkg/beta/b.py")),
            ],
            metadata={"git_head_sha": head_sha(repo)},
            node_attrs={
                q("pkg/alpha/a.py"): ("File", "rust", 0, None),
                q("pkg/beta/b.py"): ("File", "rust", 0, None),
            },
            communities=[(1, "core", 1, "rust", "")],
        )
        py_out, _ = parity(
            "graph_csv", "graph_csv",
            ["--repo", str(repo), "--out-dir", str(tmp / "p")],
            ["--repo", str(repo), "--out-dir", str(tmp / "r")],
            tmp,
        )
        assert py_out == (
            f"[OK] graph csv: 2 nodes / 2 links -> graph-nodes.csv + graph-links.csv（{tmp / 'p'}）\n"
        )
        # CSV file BYTES (CRLF + QUOTE_MINIMAL) are part of the parity face
        for name in ("graph-nodes.csv", "graph-links.csv"):
            py_bytes = (tmp / "p" / name).read_bytes()
            rs_bytes = (tmp / "r" / name).read_bytes()
            assert py_bytes == rs_bytes, f"{name} bytes differ"


class TestScipRefsAuditParity:
    def test_audit_no_missing_face(self, tmp):
        # rich.scip index + a repo whose graph_audit is clean → 0-item face
        repo = make_repo(tmp)
        src = repo / "pkg" / "alpha" / "plain.rs"
        src.parent.mkdir(parents=True, exist_ok=True)
        src.write_text("pub fn alone() {}\n")
        make_crg_db(
            repo / ".code-review-graph" / "graph.db",
            nodes=[("alone", None, f"{src}::alone", str(src))],
            metadata={"git_head_sha": head_sha(repo)},
            node_attrs={f"{src}::alone": ("Function", "rust", 0, None)},
        )
        fixture = REPO / "tests" / "parity" / "fixtures" / "rich.scip"
        py_idx, rs_idx = tmp / "py" / "index.scip", tmp / "rs" / "index.scip"
        py_idx.parent.mkdir(parents=True)
        rs_idx.parent.mkdir(parents=True)
        shutil.copy(fixture, py_idx)
        shutil.copy(fixture, rs_idx)
        py_out, code = parity(
            "scip_refs", "scip_refs",
            ["--audit", "--repo", str(repo), "--index", str(py_idx)],
            ["--audit", "--repo", str(repo), "--index", str(rs_idx)],
            tmp,
        )
        # clean when RA present (0 項 face); exit 2 both sides when absent
        assert "[OK] graph_audit 缺差 0 項 → 逐項 SCIP refs 對照：" in py_out or code == 2


class TestHelpFaces:
    @pytest.mark.parametrize(
        ("module", "sub"), 
        [("snapshot", "snapshot"), ("transition", "transition"),
         ("graph_audit", "graph_audit"), ("graph_csv", "graph_csv")],
    )
    def test_help_description_bytes_and_usage_tokens(self, module, sub):
        py_out, _, py_code = run_python(module, ["-h"])
        rs_out, _, rs_code = run_rust(sub, ["-h"])
        assert py_code == rs_code == 0
        # prog prefix normalization: argparse embeds the invocation form
        import re as _re
        py_norm = _re.sub(r"python\d* -m code_reality\." + module + r"\b", module, py_out, count=1)
        # usage block: token-sequence compare (alignment is prog-length
        # relative); description body: byte-exact
        # usage block: flattened token sequence (wrap position is
        # prog-length relative); options/description body: byte-exact
        py_head, py_body = py_norm.split("\n\n", 1)
        rs_head, rs_body = rs_out.split("\n\n", 1)
        assert py_head.split() == rs_head.split(), (
            f"{module} -h usage differs\npy:\n{py_norm}\nrs:\n{rs_out}"
        )
        assert py_body == rs_body, (
            f"{module} -h body differs\npy:\n{py_norm}\nrs:\n{rs_out}"
        )


class TestCrossLanguageInterop:
    """R4-N: Rust snapshot sidecar → frozen Python transition consumes."""

    def test_rust_snapshot_python_transition(self, tmp):
        repo = make_repo(tmp)
        edges_for(repo, head_sha(repo), fresh=True)
        out_dir = tmp / "interop"
        rs_out, _, code = run_rust(
            "snapshot", ["--repo", str(repo), "--out-dir", str(out_dir)]
        )
        assert code == 0, rs_out
        sidecars = list(out_dir.glob("repo-*.json"))
        assert len(sidecars) == 1
        # Python transition consumes the Rust-written sidecar (load_snapshot
        # asserts pass) against itself → no structural change
        py_out, _, code = run_python(
            "transition",
            [str(sidecars[0]), str(sidecars[0]), "-o", str(out_dir / "t")],
        )
        assert code == 0, py_out
        assert "[OK] transition " in py_out
        report = json.loads((out_dir / "t.json").read_text())
        assert report["added"] == [] and report["removed"] == []
        # and the diff against a second Rust snapshot (same repo) also holds
        _, _, code2 = run_rust(
            "snapshot", ["--repo", str(repo), "--out-dir", str(out_dir / "again")]
        )
        assert code2 == 0
        second = next(iter((out_dir / "again").glob("repo-*.json")))
        py_out2, _, code2 = run_python(
            "transition", [str(sidecars[0]), str(second), "-o", str(out_dir / "t2")]
        )
        assert code2 == 0, py_out2
        report2 = json.loads((out_dir / "t2.json").read_text())
        assert report2["added"] == [] and report2["removed"] == []
