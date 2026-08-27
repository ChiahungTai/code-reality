"""Golden-corpus reconciliation harness (EP ep-occurrence-producer S1).

Freezes a reconciliation baseline from a cache three-table db (the mosaic
dogfood lsp-harvest cache is the golden oracle) and diffs a candidate
producer's cache against it. Reconciliation grain mirrors scip_edges'
workspace filter: only reference rows whose symbol has a DEF in the corpus
count (stdlib/builtins references are dropped), then per-symbol reference
multisets are compared.

Site-grain caveat (EP R2-3 / S5): pyright per-site vs tree-sitter lexical
sites differ in grain, so the primary metric is per-symbol reference
COUNTS plus per-(symbol, rel_path) file-level counts.

Usage:
  uv run python scripts/golden_corpus.py --db <cache.db>                 # extract baseline (JSON to stdout)
  uv run python scripts/golden_corpus.py --self --db <cache.db>          # self-consistency
  uv run python scripts/golden_corpus.py --golden <db> --candidate <db>  # reconciliation report
      [--report out.json] [--top N] [--max-list N]
"""
import argparse
import json
import sqlite3
import sys
from collections import Counter


def extract(db_path: str) -> dict:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        defs = {r[0] for r in conn.execute(
            "SELECT DISTINCT symbol FROM occurrences WHERE is_def = 1")}
        sym_counts = Counter()      # per-symbol reference count (workspace-filtered)
        file_counts = Counter()     # per-(symbol, rel_path) count
        dangling = 0                # reference rows whose symbol has no def (pre-filter)
        for symbol, rel_path, _line in conn.execute(
                "SELECT symbol, rel_path, line FROM occurrences WHERE is_def = 0 ORDER BY seq"):
            if symbol not in defs:
                dangling += 1
                continue
            sym_counts[symbol] += 1
            file_counts[(symbol, rel_path)] += 1
        meta = dict(conn.execute("SELECT key, value FROM meta"))
    finally:
        conn.close()
    return {
        "defs": defs,
        "sym_counts": sym_counts,
        "file_counts": file_counts,
        "dangling": dangling,
        "meta": meta,
    }


def self_check(db_path: str) -> dict:
    """Self-consistency: schema present, meta keys, counts derive cleanly."""
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        tables = {r[0] for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type = 'table'")}
        expected = {"meta", "occurrences", "symbol_tails"}
        missing = expected - tables
        if missing:
            raise SystemExit(f"[FAIL] missing tables: {missing} (not a cache three-table db?)")
        total = conn.execute("SELECT COUNT(*) FROM occurrences").fetchone()[0]
        n_def_rows = conn.execute(
            "SELECT COUNT(*) FROM occurrences WHERE is_def = 1").fetchone()[0]
        meta = dict(conn.execute("SELECT key, value FROM meta"))
    finally:
        conn.close()
    data = extract(db_path)
    print(f"[OK] golden_corpus self: {len(data['defs'])} defs / "
          f"{total - n_def_rows} ref rows ({sum(data['sym_counts'].values())} workspace / "
          f"{data['dangling']} dangling) / meta={sorted(meta)}")
    return {"defs": len(data["defs"]), "ref_rows": total - n_def_rows,
            "workspace_refs": sum(data["sym_counts"].values()),
            "dangling": data["dangling"], "meta": meta}


def reconcile(golden_db: str, candidate_db: str, top: int, max_list: int) -> dict:
    g = extract(golden_db)
    c = extract(candidate_db)
    missing_defs = sorted(g["defs"] - c["defs"])
    extra_defs = sorted(c["defs"] - g["defs"])
    all_syms = set(g["sym_counts"]) | set(c["sym_counts"])
    diffs = []
    for s in all_syms:
        d = c["sym_counts"].get(s, 0) - g["sym_counts"].get(s, 0)
        if d != 0:
            diffs.append({"symbol": s, "golden": g["sym_counts"].get(s, 0),
                          "candidate": c["sym_counts"].get(s, 0), "delta": d})
    diffs.sort(key=lambda x: (-abs(x["delta"]), x["symbol"]))
    g_total = sum(g["sym_counts"].values())
    c_total = sum(c["sym_counts"].values())
    report = {
        "golden": {"db": golden_db, "meta": g["meta"], "defs": len(g["defs"]),
                   "workspace_refs": g_total, "dangling": g["dangling"]},
        "candidate": {"db": candidate_db, "meta": c["meta"], "defs": len(c["defs"]),
                      "workspace_refs": c_total, "dangling": c["dangling"]},
        "defs": {
            "missing_count": len(missing_defs),
            "missing_sample": missing_defs[:max_list],
            "extra_count": len(extra_defs),
            "extra_sample": extra_defs[:max_list],
            "coverage": (len(g["defs"]) - len(missing_defs)) / max(len(g["defs"]), 1),
        },
        "refs": {
            "golden_total": g_total,
            "candidate_total": c_total,
            "symbols_with_diff": len(diffs),
            "absdiff_sum": sum(abs(d["delta"]) for d in diffs),
            "absdiff_ratio_vs_golden": (sum(abs(d["delta"]) for d in diffs)
                                        / g_total) if g_total else None,
            "top": diffs[:top],
        },
    }
    return report


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--db", help="cache db (extract/self target)")
    ap.add_argument("--self", action="store_true", help="self-consistency check")
    ap.add_argument("--golden", help="golden cache db")
    ap.add_argument("--candidate", help="candidate producer cache db")
    ap.add_argument("--report", help="write JSON report here (else stdout)")
    ap.add_argument("--top", type=int, default=20)
    ap.add_argument("--max-list", type=int, default=50)
    args = ap.parse_args()

    if args.self:
        if not args.db:
            raise SystemExit("[FAIL] --self requires --db")
        if args.golden or args.candidate:
            ap.error("--self is mutually exclusive with --golden/--candidate")
        self_check(args.db)
        return
    if args.golden or args.candidate:
        if not (args.golden and args.candidate):
            ap.error("--golden and --candidate must be provided together")
    else:
        if not args.db:
            ap.error("expected --self --db, or --golden + --candidate")
        data = extract(args.db)
        print(json.dumps({
            "db": args.db, "meta": data["meta"], "defs": len(data["defs"]),
            "workspace_refs": sum(data["sym_counts"].values()),
            "dangling": data["dangling"]}, ensure_ascii=False, indent=2))
        return
    report = reconcile(args.golden, args.candidate, args.top, args.max_list)
    out = json.dumps(report, ensure_ascii=False, indent=2)
    if args.report:
        with open(args.report, "w") as f:
            f.write(out + "\n")
        r, d = report["refs"], report["defs"]
        print(f"[OK] golden_corpus report → {args.report}: defs "
              f"{report['candidate']['defs']}/{report['golden']['defs']} "
              f"(coverage {d['coverage']:.1%}, missing {d['missing_count']})；"
              f"refs {r['candidate_total']}/{r['golden_total']} "
              f"(diff {(r['absdiff_ratio_vs_golden'] or 0):.1%}, "
              f"{r['symbols_with_diff']} symbols)")
    else:
        print(out)


if __name__ == "__main__":
    sys.exit(main())
