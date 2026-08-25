"""Generate the shared synthetic SCIP index fixture for parity tests.

Writes a real protobuf ``.scip`` mirroring ``tests/test_scip_refs.py``
``rich_index()`` coverage: three symbol shapes (inherent / trait impl /
trait decl), boundary rejections (``my_open``, dash form), ref-only symbol,
non-function symbol, empty range, >6 refs truncation, cross-file ordering.

EP: ai-analysis/execution-plans/ep-rust-r2-scip-family.md (S3/S5)
Usage: uv run python tests/parity/make_fixture.py [out-path]
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
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/dep.rs "
    "impl#[RefOnly]run()."
)
NON_FN = "rust-analyzer cargo nautilus 1.0.0 crates/common/src/types.rs SomeStruct"


def _occ(doc, symbol: str, roles: int, rng: list[int]) -> None:
    o = doc.occurrences.add()
    o.symbol = symbol
    o.symbol_roles = roles
    o.range.extend(rng)


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


def main() -> int:
    out = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path(__file__).resolve().parent / "fixtures" / "rich.scip"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(build_index().SerializeToString())
    print(f"[OK] fixture: {out} ({out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
