"""Cross-language parity harness: frozen Python scip_refs vs Rust `code-reality scip_refs`.

Runs both implementations on identical inputs and compares stdout bytes +
exit codes (the NT contract surface; stderr is management-only and not gated).
All mutating drills (touch mtime, meta head rewrite, corrupt bytes, db builds)
run on fixture COPIES in per-side tempdirs — the NT real-index cases are
strictly read-only (skip if the slot db is stale — a rebuild would write into
the live sidecar home).

EP: ai-analysis/execution-plans/ep-rust-r2-scip-family.md S5 (master R2 gate).
"""

import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).resolve().parent / "fixtures" / "rich.scip"
NT_REPO = Path.home() / "Github" / "nautilus_trader"
NT_INDEX = Path.home() / ".mosaic/code-reality/scip/nautilus_trader/index.scip"

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


def run_python(args: list[str]) -> tuple[str, str, int]:
    proc = subprocess.run(
        [sys.executable, "-m", "code_reality.scip_refs", *args],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout, proc.stderr, proc.returncode


def run_rust(args: list[str]) -> tuple[str, str, int]:
    proc = subprocess.run(
        [str(_rust_bin()), "scip_refs", *args],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout, proc.stderr, proc.returncode


def side_copy(tmp: Path, name: str) -> Path:
    d = tmp / name
    d.mkdir(exist_ok=True)
    idx = d / "index.scip"
    shutil.copy(FIXTURE, idx)
    return idx


def normalize(out: str, tmp: Path) -> str:
    out = out.replace(str(tmp / "py"), "<S>").replace(str(tmp / "rs"), "<S>")
    return out.replace(str(tmp), "<T>")


@pytest.fixture()
def tmp():
    with tempfile.TemporaryDirectory() as d:
        yield Path(d)


def parity(args_py: list[str], args_rs: list[str], tmp: Path) -> None:
    py_out, _, py_code = run_python(args_py)
    rs_out, _, rs_code = run_rust(args_rs)
    assert py_code == rs_code, f"exit codes differ: py={py_code} rs={rs_code}"
    assert normalize(py_out, tmp) == normalize(rs_out, tmp), (
        f"stdout bytes differ\n--- py ---\n{py_out}\n--- rs ---\n{rs_out}"
    )


class TestFixtureQueries:
    def test_type_method_full_shape(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["EventStoreLifecycle.open", "--index", str(py_idx)],
            ["EventStoreLifecycle.open", "--index", str(rs_idx)],
            tmp,
        )

    def test_bare_name(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["open", "--index", str(py_idx)], ["open", "--index", str(rs_idx)], tmp)

    def test_dash_query_non_identifier_path(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["my-open", "--index", str(py_idx)], ["my-open", "--index", str(rs_idx)], tmp)

    def test_no_def_exit_1(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["Nothing.here", "--index", str(py_idx)], ["Nothing.here", "--index", str(rs_idx)], tmp)

    def test_empty_query_final_guard_exit_2(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["", "--index", str(py_idx)], ["", "--index", str(rs_idx)], tmp)

    def test_missing_index_exit_2(self, tmp):
        parity(
            ["q", "--index", str(tmp / "nope.scip")],
            ["q", "--index", str(tmp / "nope.scip")],
            tmp,
        )

    def test_corrupt_index_exit_2_empty_stdout(self, tmp):
        for name in ("py", "rs"):
            p = tmp / name / "index.scip"
            p.parent.mkdir(parents=True)
            p.write_bytes(b"garbage not protobuf")
        parity(
            ["q", "--index", str(tmp / "py/index.scip")],
            ["q", "--index", str(tmp / "rs/index.scip")],
            tmp,
        )


class TestModes:
    def test_build_cache_stats_line(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["--build-cache", "--index", str(py_idx)],
            ["--build-cache", "--index", str(rs_idx)],
            tmp,
        )

    def test_stamp_meta_idempotent(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        py_args = ["--stamp-meta", "--repo", str(REPO), "--index", str(py_idx)]
        rs_args = ["--stamp-meta", "--repo", str(REPO), "--index", str(rs_idx)]
        parity(py_args, rs_args, tmp)
        parity(py_args, rs_args, tmp)  # rerun → idempotent overwrite

    def test_stamp_meta_needs_repo_exit_2(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["--stamp-meta", "--index", str(py_idx)],
            ["--stamp-meta", "--index", str(rs_idx)],
            tmp,
        )

    def test_src_line_repo_only_without_meta(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["open", "--index", str(py_idx), "--repo", str(REPO)],
            ["open", "--index", str(rs_idx), "--repo", str(REPO)],
            tmp,
        )


class TestCrossLanguageDbInterop:
    def test_python_built_db_rust_reads(self, tmp):
        """SM-13 direction A: Python --build-cache, Rust query answers from it.
        Asserts the Rust side actually READ the db (no WARN-rebuild/fallback
        masking — a schema divergence would surface as a 衍生 db WARN)."""
        d_py = tmp / "shared"
        d_py.mkdir()
        idx = d_py / "index.scip"
        shutil.copy(FIXTURE, idx)
        run_python(["--build-cache", "--index", str(idx)])
        assert (d_py / "index.scip.db").exists()
        rs_out, rs_err, rs_code = run_rust(["open", "--index", str(idx)])
        assert rs_code == 0
        assert "refs: 9 處（跨檔）" in rs_out
        assert "[WARN] 衍生 db" not in rs_err, f"db not actually read: {rs_err}"

    def test_rust_built_db_python_reads(self, tmp):
        """SM-13 direction B: Rust --build-cache, frozen Python query answers."""
        d_rs = tmp / "shared"
        d_rs.mkdir()
        idx = d_rs / "index.scip"
        shutil.copy(FIXTURE, idx)
        run_rust(["--build-cache", "--index", str(idx)])
        assert (d_rs / "index.scip.db").exists()
        py_out, py_err, py_code = run_python(["open", "--index", str(idx)])
        assert py_code == 0
        assert "refs: 9 處（跨檔）" in py_out
        assert "[WARN] 衍生 db" not in py_err, f"db not actually read: {py_err}"

    def test_auto_rebuild_on_stale_mtime(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        for idx in (py_idx, rs_idx):
            run_python(["--build-cache", "--index", str(idx)])
        time.sleep(0.05)
        for idx in (py_idx, rs_idx):
            data = idx.read_bytes()
            idx.write_bytes(data)  # index newer than db → stale
        parity(["open", "--index", str(py_idx)], ["open", "--index", str(rs_idx)], tmp)

    def test_drift_warn_stdout_unchanged(self, tmp):
        """SM-3: meta head ≠ repo HEAD → stderr WARN on both sides; stdout cmp'd."""
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        for idx in (py_idx, rs_idx):
            idx.with_name("index.scip.meta.json").write_text(
                '{"repo": "'
                + str(REPO)
                + '", "head": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",'
                ' "stamped_at": "2026-08-24T13:45:02+00:00", "tool": "code_reality.scip_refs"}'
            )
        parity(
            ["open", "--index", str(py_idx), "--repo", str(REPO)],
            ["open", "--index", str(rs_idx), "--repo", str(REPO)],
            tmp,
        )


@pytest.fixture(scope="module")
def nt_fresh():
    """Read-only guard: skip if the slot or its fresh db is absent (a stale db
    would trigger an auto-rebuild = sidecar mutation)."""
    db = NT_INDEX.with_name("index.scip.db")
    if not NT_INDEX.exists() or not db.exists():
        pytest.skip("NT real index slot absent")
    if db.stat().st_mtime < NT_INDEX.stat().st_mtime:
        pytest.skip("NT slot db stale — read-only policy forbids the auto-rebuild")
    return NT_INDEX


class TestArgparseEdge:
    """argparse-mimicking parse edge cases (agent-review adversarial probes)."""

    def test_lone_dash_is_positional(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["-", "--index", str(py_idx)], ["-", "--index", str(rs_idx)], tmp)

    def test_negative_number_is_positional(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["-5", "--index", str(py_idx)], ["-5", "--index", str(rs_idx)], tmp)

    def test_dashdash_separated_token_is_positional(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["--index", str(py_idx), "--", "-weird"],
            ["--index", str(rs_idx), "--", "-weird"],
            tmp,
        )

    def test_flag_abbreviation_resolves(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["open", "--ind", str(py_idx)],
            ["open", "--ind", str(rs_idx)],
            tmp,
        )

    def test_extra_positional_is_exit_2(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["open", "extra", "--index", str(py_idx)],
            ["open", "extra", "--index", str(rs_idx)],
            tmp,
        )

    def test_dot_minus_is_unrecognized_exit_2(self, tmp):
        """`-.` has no digit after the dot — not a negative number (3.14 matcher)."""
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(["-.", "--index", str(py_idx)], ["-.", "--index", str(rs_idx)], tmp)

    def test_trailing_digitless_shapes_are_positional(self, tmp):
        """3.14 prefix matcher: `-5.`, `-5x`, `-5.5.5` all count as negative numbers."""
        for q in ("-5.", "-5x", "-5.5.5"):
            py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
            parity(
                [q, "--index", str(py_idx)],
                [q, "--index", str(rs_idx)],
                tmp,
            )

    def test_help_abbreviation_exits_0(self, tmp):
        for flag in ("--h", "--hel"):
            _py_out, _, py_code = run_python([flag])
            _rs_out, _, rs_code = run_rust([flag])
            assert py_code == rs_code == 0, f"{flag}: py={py_code} rs={rs_code}"

    def test_bool_flag_with_inline_value_is_exit_2(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["--stamp-meta=1", "--repo", str(REPO), "--index", str(py_idx)],
            ["--stamp-meta=1", "--repo", str(REPO), "--index", str(rs_idx)],
            tmp,
        )

    def test_option_like_flag_value_is_exit_2(self, tmp):
        py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
        parity(
            ["--repo", "--stamp-meta", "q", "--index", str(py_idx)],
            ["--repo", "--stamp-meta", "q", "--index", str(rs_idx)],
            tmp,
        )

    def test_sidecar_without_stamped_at_omits_date_part(self, tmp):
        """Absent key → no （date） suffix; explicit null → （None） (both sides)."""
        for value in ('', 'null'):
            py_idx, rs_idx = side_copy(tmp, "py"), side_copy(tmp, "rs")
            for idx in (py_idx, rs_idx):
                extra = f'"stamped_at": {value}, ' if value else ""
                idx.with_name("index.scip.meta.json").write_text(
                    '{"repo": "'
                    + str(REPO)
                    + '", "head": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", '
                    + extra
                    + '"tool": "code_reality.scip_refs"}'
                )
            parity(
                ["open", "--index", str(py_idx), "--repo", str(REPO)],
                ["open", "--index", str(rs_idx), "--repo", str(REPO)],
                tmp,
            )


class TestNtRealIndex:
    def test_type_method_18_refs(self, nt_fresh):
        parity(
            ["EventStoreLifecycle.open", "--repo", str(NT_REPO)],
            ["EventStoreLifecycle.open", "--repo", str(NT_REPO)],
            Path("/nonexistent"),  # no tmp normalization needed (shared slot)
        )

    def test_bare_name_default_backend_opener(self, nt_fresh):
        parity(
            ["default_backend_opener", "--repo", str(NT_REPO)],
            ["default_backend_opener", "--repo", str(NT_REPO)],
            Path("/nonexistent"),
        )

    def test_no_slot_repo_exit_2(self, tmp):
        # SM-4: repo without a slot index → loud exit 2 (uses a unique tmp name)
        fake = tmp / "no_such_repo_xyz"
        fake.mkdir()
        parity(
            ["q", "--repo", str(fake)],
            ["q", "--repo", str(fake)],
            tmp,
        )
