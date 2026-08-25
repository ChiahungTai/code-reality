"""R3 caller-edge acceptance (Rust-only — no Python oracle exists).

- NT name-level pin: `EventStoreLifecycle.open` → 16 callers / 18 sites
  (SCIP ground truth `_e2e_rerun.out` + fresh LSP `incomingCalls`,
  adjudicated 2026-08-25 — the upstream "17 callers" was an arithmetic
  slip in the research-report prose; see ep-rust-r3-caller-edges.md).
- closure start-point consistency: depth-1 == the callers set minus the
  query-resolved seed symbols (multi-DEF seeds: the trait impl is both a
  seed and a caller → 15 new + 1 cycle re-entry).
- SM-9 hard gate on the sqlite path, drilled on a tempdir index copy
  (the frozen NT slot stays read-only — sidecar never written there).
- dual-face byte equality (protobuf face vs build-cache'd sqlite face) on
  the committed rich_callers fixture.
- LSP probe integration: skip-when-absent against the persisted
  lsp_incomingcalls_eventstore.json oracle.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
FIXTURES = Path(__file__).resolve().parent / "fixtures"
RICH_CALLERS = FIXTURES / "rich_callers.scip"
LSP_ORACLE = FIXTURES / "lsp_incomingcalls_eventstore.json"

NT_REPO = Path.home() / "Github" / "nautilus_trader"
NT_INDEX = Path.home() / ".mosaic/code-reality/scip/nautilus_trader/index.scip"


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


# (caller tail, [site lines]) — ground truth: ai-rules .agent-tmp/research/
# _e2e_rerun.out:7-24 (18 refs → 16 distinct callers), reproduced
# byte-for-byte by the Rust CLI on 2026-08-25 and set-equal (name-level,
# trait-impl naming artifact aside) to a fresh LSP incomingCalls fetch.
EXPECTED_NT_CALLERS: list[tuple[str, list[int]]] = [
    ("kernel/impl#[EventStoreLifecycle][KernelEventStore]open().", [1356]),
    (
        "kernel/tests/lifecycle_options_custom_registry_captures_registered_message().",
        [1742],
    ),
    (
        "kernel/tests/lifecycle_options_memory_backend_opener_captures_and_seals().",
        [1791],
    ),
    (
        "kernel/tests/kernel_event_store_open_seals_leftover_session_before_reopen().",
        [2461, 2470],
    ),
    (
        "kernel/tests/open_after_halt_re_arms_signal_and_next_run_seals_ended().",
        [2514, 2526],
    ),
    ("kernel/tests/bus_tap_captures_submit_order_sent_through_msgbus().", [2798]),
    (
        "kernel/tests/kernel_with_markers_captures_snapshots_over_synthetic_bus().",
        [2870],
    ),
    ("kernel/tests/boot_recovery_ignores_marker_sidecar_files().", [2937]),
    ("kernel/tests/marker_registry_factory_receives_enabled_classes().", [3050]),
    ("kernel/tests/markers_disabled_installs_no_file_and_no_cost().", [3075]),
    ("kernel/tests/bus_tap_captures_time_event_handler_run().", [3119]),
    (
        "kernel/tests/seal_clears_bus_tap_so_post_seal_dispatches_do_not_capture().",
        [3166],
    ),
    (
        "kernel/tests/bus_tap_captures_trading_command_envelope_with_inner_payload_type().",
        [3211],
    ),
    (
        "kernel/tests/bus_tap_captures_order_event_any_envelope_with_inner_payload_type().",
        [3293],
    ),
    (
        "kernel/tests/bus_tap_captures_data_command_envelopes_with_category_payload_types().",
        [3373],
    ),
    (
        "kernel/tests/bus_tap_captures_data_response_sent_through_correlation_handler().",
        [3474],
    ),
]
NT_KERNEL = "crates/event_store/src/kernel.rs"


@pytest.fixture()
def nt_fresh():
    """Read-only guard (R3-J): skip if the slot / its fresh db are absent,
    if the db is stale (read-only policy forbids the auto-rebuild), or if a
    fn_defs sidecar is already present in the slot (a prior explicit build
    would make the not-written assertion meaningless)."""
    db = NT_INDEX.with_name("index.scip.db")
    if not NT_INDEX.exists() or not db.exists():
        pytest.skip("NT real index slot absent")
    if db.stat().st_mtime < NT_INDEX.stat().st_mtime:
        pytest.skip("NT slot db stale — read-only policy forbids the auto-rebuild")
    sidecar = NT_INDEX.with_name("index.scip.fndefs.db")
    if sidecar.exists():
        pytest.skip("NT slot already carries a fn_defs sidecar (explicit build ran)")
    return NT_INDEX


class TestNtCallers:
    def test_sixteen_callers_name_level_pin(self, nt_fresh):
        out, _, code = run_rust(
            ["--callers", "EventStoreLifecycle.open", "--repo", str(NT_REPO)]
        )
        assert code == 0
        lines = out.splitlines()
        assert lines[0].startswith("[SRC] "), out
        assert lines[1] == "[OK] EventStoreLifecycle.open：16 callers（18 sites）"
        # caller lines + their sites, in first-site scan order
        expect_lines: list[str] = []
        for tail, sites in EXPECTED_NT_CALLERS:
            expect_lines.append(f"  {tail}（{len(sites)} 處）")
            expect_lines.extend(f"    {NT_KERNEL}:{n}" for n in sites)
        expect_lines.append("  item-level：0 處（未歸屬 fn——use/const/屬性層）")
        assert lines[2:] == expect_lines, out
        # R3-J: the query must not write a sidecar into the frozen slot
        # (nt_fresh already skipped on a pre-existing sidecar)
        assert not NT_INDEX.with_name("index.scip.fndefs.db").exists()

    def test_closure_start_point(self, nt_fresh):
        out, _, code = run_rust(
            ["--closure", "EventStoreLifecycle.open", "--repo", str(NT_REPO)]
        )
        assert code == 0
        # multi-DEF seeds: trait impl is both a seed and a caller →
        # depth-1 = 15 new + 1 cycle re-entry; depth-2 empty (tests have
        # no further callers in this index era)
        assert "  depth 1：15 callers" in out, out
        assert "  depth 2：0 callers" in out, out
        assert "  cycles：1 處（frontier 重入已拜訪符號）" in out, out
        assert "    crates/event_store/src/kernel.rs：15 符號" in out, out


class TestSm9SqlitePathDrill:
    def test_build_and_closure_on_tempdir_copy(self):
        """SM-9 hard gate on the sqlite path. The frozen NT slot cannot take
        the sidecar, so the drill copies the index to a tempdir, builds both
        artifacts there (build time recorded), and runs the closure through
        the sqlite faces with a wall-clock bound."""
        if not NT_INDEX.exists():
            pytest.skip("NT real index slot absent")
        with tempfile.TemporaryDirectory() as d:
            slot = Path(d) / "nautilus_trader"
            slot.mkdir()
            idx = slot / "index.scip"
            shutil.copy(NT_INDEX, idx)
            t0 = time.monotonic()
            out, err, code = run_rust(["--build-cache", "--index", str(idx)])
            build_s = time.monotonic() - t0
            assert code == 0, err
            assert out.startswith("[OK] cache built："), out
            assert "fn_defs sidecar built" in err, err
            assert (slot / "index.scip.db").exists()
            assert (slot / "index.scip.fndefs.db").exists()
            t0 = time.monotonic()
            out, err, code = run_rust(
                ["--closure", "EventStoreLifecycle.open", "--index", str(idx)]
            )
            wall_s = time.monotonic() - t0
            assert code == 0, err
            assert "  depth 1：15 callers" in out, out
            # SM-9: seconds-level on the sqlite path (10s = order-of-
            # magnitude bound, not a tight budget; sqlite refs are
            # sub-second, protobuf decode ~0.9s release)
            assert wall_s <= 10.0, f"closure wall-clock {wall_s:.1f}s > 10s"
            print(f"[SM-9] build={build_s:.1f}s closure={wall_s:.1f}s")


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


class TestLspOracle:
    def test_pinned_scip_list_matches_lsp_oracle_fixture(self):
        """Committed-evidence consistency: the pinned SCIP caller list vs
        the persisted LSP oracle JSON, name-level. Both artifacts are
        committed (the oracle was fetched live at build time — see its
        `_meta.fetched`), so this is a data-consistency check; it
        deliberately does NOT probe the live server — a process merely
        occupying 127.0.0.1:8000 adds no evidence either way."""
        oracle = json.loads(LSP_ORACLE.read_text())
        oracle_names = {c["name"] for c in oracle["callers"]}
        assert oracle["callers"], "persisted oracle is empty"
        assert oracle["_meta"]["count"] == len(oracle["callers"])
        scip_names = set()
        for tail, _sites in EXPECTED_NT_CALLERS:
            name = tail.rsplit("/", 1)[-1].removesuffix("().")
            if name.startswith("impl#["):
                name = name.rsplit("]", 1)[-1]  # trait impl → bare fn name
            scip_names.add(name)
        assert scip_names == oracle_names, (
            f"SCIP {len(scip_names)} vs oracle {len(oracle_names)}:\n"
            f"only-SCIP: {sorted(scip_names - oracle_names)}\n"
            f"only-oracle: {sorted(oracle_names - scip_names)}"
        )
