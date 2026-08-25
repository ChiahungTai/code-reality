"""R3 caller-edge parity (self-contained): dual-face byte equality.

protobuf face (no db/sidecar) vs build-cache'd sqlite face must produce
byte-identical stdout for both caller-edge modes on the committed
rich_callers fixture — the face-selection ladder, ordering, and output
assembly are all on the equality path.

Open-source test policy: no external-repo inputs. The legacy NT
name-level pin / closure-start / SM-9 drill / LSP oracle fixture moved
out with the policy; their adjudication history lives in the archived
R3 EP (16 callers / 18 sites, three-source closure).

EP: ai-analysis/execution-plans/_done/ep-rust-r3-caller-edges.md (S4).
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
FIXTURES = Path(__file__).resolve().parent / "fixtures"
RICH_CALLERS = FIXTURES / "rich_callers.scip"

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


def run_rust(args: list[str]) -> tuple[str, str, int]:
    proc = subprocess.run(
        [str(_rust_bin()), "scip_refs", *args],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout, proc.stderr, proc.returncode


class TestDualFaceEquivalence:
    def test_callers_and_closure_byte_equal_across_faces(self):
        """protobuf face (no db/sidecar) vs build-cache'd sqlite face must
        produce byte-identical stdout for both caller-edge modes."""
        with tempfile.TemporaryDirectory() as d:
            idx = Path(d) / "index.scip"
            shutil.copy(RICH_CALLERS, idx)
            for mode in (["--callers"], ["--closure", "--depth", "3"]):
                pb_out, _, pb_code = run_rust(
                    [*mode, "EventStoreLifecycle.open", "--index", str(idx)]
                )
                _, err, code = run_rust(["--build-cache", "--index", str(idx)])
                assert code == 0, err
                sq_out, _, sq_code = run_rust(
                    [*mode, "EventStoreLifecycle.open", "--index", str(idx)]
                )
                assert pb_code == sq_code
                assert pb_out == sq_out, (
                    f"{mode} faces differ\n--- pb ---\n{pb_out}\n--- sq ---\n{sq_out}"
                )
