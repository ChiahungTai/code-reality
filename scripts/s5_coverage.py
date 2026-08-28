#!/usr/bin/env python
"""S5 resolved-legacy coverage metric (EP ep-producer-completion-b7b W1).

Recomputes the import_legacy retirement figure from a repo's
`.code-reality/graph.db` in four layers:

  raw   — producer CALLS pairs vs legacy-resolved CALLS pairs, F6 frozen
          node-key grain ((name, file basename) per endpoint). Reproduces
          the 2c44534 settlement (72.3% on mosaic @24ced017).
  r2_3  — legacy pairs whose callee node kind is Class are carved out of
          the denominator (EP R2-3 frozen clause, unapplied at 2c44534;
          correctly applied -> 94.7% on the same corpus).
  b7a   — producer method-callee `__init__` edges are normalized to their
          own-class key (graph_db.rs class_segment semantics) so grain-
          mismatched constructor edges count as true matches.
  gate  — FULL legacy-resolved denominator (no carve) with the b7a
          normalization: the W3 retirement gate figure. Post-B7b this is
          the pseudo-constructor true-match number.

Usage: uv run python scripts/s5_coverage.py --db <repo>/.code-reality/graph.db [--json]
"""

import argparse
import json
import os
import sqlite3
import sys


def node_key(name: str, file_path: str) -> tuple[str, str]:
    return (name, os.path.basename(file_path))


def class_segment(symbol: str) -> str | None:
    """Own-class segment of a method-shaped symbol (graph_db.rs parity)."""
    hash_idx = symbol.rfind("#")
    if hash_idx < 0:
        return None
    head = symbol[:hash_idx]
    start = head.rfind("/")
    name = head[start + 1 :] if start >= 0 else head
    return name or None


def load_graph(db_path: str):
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    nodes = {
        r["symbol"]: (r["name"], r["file_path"], r["kind"])
        for r in conn.execute("SELECT symbol, name, file_path, kind FROM nodes")
    }
    calls = {"scip": [], "treesitter-legacy": []}
    references = []
    synthesized = 0
    for r in conn.execute(
        "SELECT caller_symbol, callee_symbol, kind, provenance FROM edges "
        "WHERE kind IN ('CALLS', 'REFERENCES')"
    ):
        caller = nodes.get(r["caller_symbol"])
        callee = nodes.get(r["callee_symbol"])
        if caller is None or callee is None:
            if r["kind"] == "CALLS":
                synthesized += 1
            continue
        entry = (
            node_key(*caller[:2]),
            node_key(callee[0], callee[1]),
            callee[2],
            r["caller_symbol"],
            r["callee_symbol"],
        )
        if r["kind"] == "CALLS":
            calls.get(r["provenance"], []).append(entry)
        elif r["provenance"] == "scip":
            references.append(entry)
    conn.close()
    return nodes, calls, references, synthesized


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", required=True)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    nodes, calls, references, synthesized = load_graph(args.db)
    producer = {(e[0], e[1]): e for e in calls["scip"]}
    legacy = {(e[0], e[1]): e for e in calls["treesitter-legacy"]}

    # raw layer — F6 frozen grain, callee kind rides along for the carve
    raw_inter = len(set(producer) & set(legacy))
    raw_denom = len(legacy)
    raw_cov = raw_inter / raw_denom if raw_denom else 0.0

    # r2_3 layer — carve legacy Class-callee pairs out of the denominator
    # (criterion = the callee node's kind in the nodes table, legacy side)
    legacy_nonclass = {k for k, e in legacy.items() if e[2] != "Class"}
    r23_inter = len(set(producer) & legacy_nonclass)
    r23_denom = len(legacy_nonclass)
    r23_cov = r23_inter / r23_denom if r23_denom else 0.0

    # b7a layer — producer `__init__` callees normalized to own-class keys
    # (L side is never normalized: legacy callees carry no `__init__` tail)
    producer_norm = set()
    init_callee_edges = 0
    for k, e in producer.items():
        ek = k[1]
        if e[2] != "Class" and e[1][0] == "__init__":
            seg = class_segment(e[4])
            if seg:
                init_callee_edges += 1
                ek = (seg, ek[1])
        producer_norm.add((k[0], ek))
    b7a_inter = len(producer_norm & legacy_nonclass)
    b7a_denom = len(legacy_nonclass)
    b7a_cov = b7a_inter / b7a_denom if b7a_denom else 0.0

    # gate layer — FULL denominator, B7a-normalized (W3 retirement gate)
    gate_inter = len(producer_norm & set(legacy))
    gate_denom = len(legacy)
    gate_cov = gate_inter / gate_denom if gate_denom else 0.0

    # Diagnostics (EP review F1/F2): (a) basename-collision keys —
    # (name, basename) mapping to more than one full path inflates
    # true-match by cross-file mispairing; (b) producer REFERENCES edges
    # whose pair key hits a legacy CALLS pair — the alias-call residual
    # bucket (syntactic mark = alias name != symbol tail).
    node_paths: dict[tuple[str, str], set[str]] = {}
    for _, (name, file_path, _) in nodes.items():
        node_paths.setdefault(node_key(name, file_path), set()).add(file_path)
    collision_keys = {k for k, v in node_paths.items() if len(v) > 1}
    collision_pairs = sum(
        1 for (ck, ek) in producer_norm if ck in collision_keys or ek in collision_keys
    )
    refs_hit_legacy = sum(
        1 for e in references if (e[0], e[1]) in legacy
    )

    report = {
        "db": args.db,
        "grain": "caller (name,file-basename) x callee (name,file-basename)",
        "producer_pairs": len(producer),
        "legacy_pairs_resolved": len(legacy),
        "legacy_pairs_with_synthesized_endpoint": synthesized,
        "raw": {
            "intersection": raw_inter,
            "denominator": raw_denom,
            "coverage": round(raw_cov, 4),
        },
        "r2_3_class_callee_carved": {
            "carved_pairs": len(legacy) - len(legacy_nonclass),
            "intersection": r23_inter,
            "denominator": r23_denom,
            "coverage": round(r23_cov, 4),
        },
        "b7a_normalized": {
            "init_callee_edges_normalized": init_callee_edges,
            "intersection_carved_denom": b7a_inter,
            "denominator_carved": b7a_denom,
            "coverage_carved": round(b7a_cov, 4),
        },
        "gate_full_denominator": {
            "intersection": gate_inter,
            "denominator": gate_denom,
            "coverage": round(gate_cov, 4),
        },
        "diagnostics": {
            "basename_collision_keys": len(collision_keys),
            "producer_pairs_with_collision_key": collision_pairs,
            "producer_REFERENCES_edges_hitting_legacy_CALLS": refs_hit_legacy,
        },
    }
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"[OK] s5_coverage: {json.dumps(report, indent=2)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
