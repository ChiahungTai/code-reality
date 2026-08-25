"""Generate the shared synthetic SCIP index fixtures for parity tests.

``rich.scip`` mirrors ``tests/test_scip_refs.py`` ``rich_index()`` coverage:
three symbol shapes (inherent / trait impl / trait decl), boundary rejections
(``my_open``, dash form), ref-only symbol, non-function symbol, empty range,
>6 refs truncation, cross-file ordering.

``rich_callers.scip`` (R3) carries ``enclosing_range`` data for the
caller-edge family: nested fns (innermost), same-width tie pair (first-seen),
macro single-line span (3-element enc), double-site caller, item-level ref,
and a mutual-recursion pair for closure cycle detection.

EP: ai-analysis/execution-plans/ep-rust-r2-scip-family.md (S3/S5);
    ai-analysis/execution-plans/ep-rust-r3-caller-edges.md (S4)
Usage: uv run python tests/parity/make_fixture.py
"""

import sys
from pathlib import Path

from code_reality import scip_pb2

IMPL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "impl#[EventStoreLifecycle]open()."
)
TRAIT_IMPL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "impl#[EventStoreLifecycle][EventStore]open()."
)
TRAIT_DECL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "EventStoreLifecycle#open()."
)
OTHER_TYPE = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/other.rs "
    "impl#[OtherType]open()."
)
MY_OPEN = "… impl#[X]my_open()."
MY_OPEN_DASH = "… impl#[T]my-open()."
REF_ONLY = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/dep.rs impl#[RefOnly]run()."
)
NON_FN = "rust-analyzer cargo nautilus 1.0.0 crates/common/src/types.rs SomeStruct"


def _occ(doc, symbol: str, roles: int, rng: list[int]):
    o = doc.occurrences.add()
    o.symbol = symbol
    o.symbol_roles = roles
    o.range.extend(rng)
    return o


def build_index() -> scip_pb2.Index:
    index = scip_pb2.Index()
    a = index.documents.add()
    a.relative_path = "crates/a.rs"
    _occ(a, IMPL, 1, [10, 0, 10, 5])
    _occ(a, OTHER_TYPE, 1, [1, 0, 1, 5])
    _occ(a, MY_OPEN, 1, [2, 0, 2, 5])
    _occ(a, MY_OPEN_DASH, 1, [3, 0, 3, 5])
    _occ(a, NON_FN, 0, [4, 0, 4, 2])
    b = index.documents.add()
    b.relative_path = "crates/b.rs"
    for line in (7, 8, 9, 11, 12, 13, 14, 15):
        _occ(b, IMPL, 0, [line, 0, line, 9])
    _occ(b, IMPL, 0, [])  # empty range → "?" display
    _occ(b, TRAIT_IMPL, 1, [5, 0, 5, 5])
    _occ(b, TRAIT_DECL, 1, [6, 0, 6, 5])
    _occ(b, TRAIT_DECL, 0, [9, 0, 9, 9])
    _occ(b, REF_ONLY, 0, [3, 0, 3, 3])
    return index


# ---------- rich_callers.scip (R3 caller-edge family) ----------

_P = "rust-analyzer cargo x 0.1.0 "
C_TARGET = _P + "kernel/impl#[EventStoreLifecycle]open()."
C_MACRO = _P + "kernel/impl#[EventStoreLifecycle]macro_fn()."
C_TIE1 = _P + "kernel/tests/tie_one()."
C_TIE2 = _P + "kernel/tests/tie_two()."
C_OUTER = _P + "kernel/outer()."
C_INNER = _P + "kernel/inner()."
C_DECL = _P + "kernel/EventStoreLifecycle#open()."
C_DELEGATE = _P + "kernel/impl#[EventStoreLifecycle][KernelEventStore]delegate()."
C_T1 = _P + "kernel/tests/t_one()."
C_T2 = _P + "kernel/tests/t_two()."
C_CYCLE_A = _P + "kernel/cycle_a()."
C_CYCLE_B = _P + "kernel/cycle_b()."


def _occ_enc(
    doc, symbol: str, roles: int, rng: list[int], enc: list[int]
) -> scip_pb2.Occurrence:
    o = _occ(doc, symbol, roles, rng)
    o.enclosing_range.extend(enc)
    return o


def build_callers_index() -> scip_pb2.Index:
    """Occurrence order is load-bearing: it fixes span scan-seq (tie rule)
    and ref first-site ordering. Expected `--callers EventStoreLifecycle.open`
    output: 8 callers / 9 attributed sites / item-level 1 (see
    tests/callers_cli.rs)."""
    index = scip_pb2.Index()
    a = index.documents.add()
    a.relative_path = "crates/a.rs"
    # span seq 0..8 in DEF order
    _occ_enc(a, C_MACRO, 1, [18, 2], [18, 2, 44])  # 3-elem enc → span 19-19 (SM-6)
    _occ(a, C_TARGET, 0, [18, 0, 18, 9])  # ref @19 → MACRO
    _occ_enc(a, C_TIE1, 1, [9, 0, 9, 9], [9, 0, 19, 0])  # span 10-20
    _occ_enc(a, C_TIE2, 1, [9, 1, 9, 9], [9, 0, 19, 0])  # same span — tie: seq decides
    _occ(a, C_TARGET, 0, [14, 0, 14, 9])  # ref @15 → TIE1 (first-seen)
    _occ_enc(a, C_OUTER, 1, [99, 0, 99, 5], [99, 0, 199, 0])  # span 100-200
    _occ_enc(a, C_INNER, 1, [119, 0, 119, 5], [119, 0, 129, 0])  # span 120-130
    _occ(a, C_TARGET, 0, [124, 0, 124, 9])  # ref @125 → INNER (innermost)
    _occ(a, C_TARGET, 0, [109, 0, 109, 9])  # ref @110 → OUTER only
    _occ_enc(a, C_DECL, 1, [500, 0, 500, 5], [500, 0, 510, 0])  # trait decl DEF
    _occ_enc(a, C_DELEGATE, 1, [1349, 0, 1349, 5], [1349, 0, 1359, 0])  # span 1350-1360
    _occ(a, C_TARGET, 0, [1355, 0, 1355, 9])  # ref @1356 → DELEGATE
    _occ_enc(a, C_T1, 1, [1741, 0, 1741, 5], [1741, 0, 1751, 0])  # span 1742-1752
    _occ(a, C_TARGET, 0, [1741, 0, 1741, 9])  # ref @1742 → T1
    _occ_enc(a, C_T2, 1, [2460, 0, 2460, 5], [2460, 0, 2475, 0])  # span 2461-2476
    _occ(a, C_TARGET, 0, [2460, 0, 2460, 9])  # ref @2461 → T2
    _occ(a, C_TARGET, 0, [2469, 0, 2469, 9])  # ref @2470 → T2 (second site)
    _occ(a, C_TARGET, 0, [998, 0, 998, 9])  # ref @999 → item-level
    # the target's own DEF (query resolution needs it); span 544-561 holds
    # no target refs — attribution-neutral, appended last so span seq is
    # unchanged for the DEFs above
    _occ_enc(a, C_TARGET, 1, [543, 0, 543, 9], [543, 0, 560, 0])
    b = index.documents.add()
    b.relative_path = "crates/b.rs"
    _occ_enc(b, C_CYCLE_A, 1, [400, 0, 400, 5], [400, 0, 410, 0])  # span 401-411
    _occ(b, C_CYCLE_B, 0, [404, 0, 404, 5])  # ref of CYCLE_B @405 → inside CYCLE_A
    _occ(b, C_TARGET, 0, [407, 0, 407, 9])  # ref @408 → CYCLE_A
    _occ_enc(b, C_CYCLE_B, 1, [420, 0, 420, 5], [420, 0, 430, 0])  # span 421-431
    _occ(b, C_CYCLE_A, 0, [424, 0, 424, 5])  # ref of CYCLE_A @425 → inside CYCLE_B
    return index


def main() -> int:
    fixtures = Path(__file__).resolve().parent / "fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)
    rich = fixtures / "rich.scip"
    rich.write_bytes(build_index().SerializeToString())
    print(f"[OK] fixture: {rich} ({rich.stat().st_size} bytes)")
    rc = fixtures / "rich_callers.scip"
    rc.write_bytes(build_callers_index().SerializeToString())
    print(f"[OK] fixture: {rc} ({rc.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
