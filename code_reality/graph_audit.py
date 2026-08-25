"""CRG graph.db Rust 完整度稽核——D1 風險掃描＋D2 rust-analyzer 對帳。

收編自 NT N1 repo-local 腳本（2026-08-24 驗證輪：239 風險檔／187 對帳／
219 確定缺差——kernel.rs 九項全中；同日 NT 三輪獨立審查迭代版同步：縮排
impl 可見、閉合縮排對齊、RA 失敗不崩潰＋聚合假乾淨防護）。偵測 CRG
``nodes.qualified_name UNIQUE``＋``ON CONFLICT DO UPDATE`` 後寫蓋前寫的
靜默丟失：Rust 型別的 inherent impl 與 trait impl 同名方法同鍵 ``X.method``，
前者消失（``head_matches_build`` 新鮮度指標不反映此項——檔案未變不重
parse；根因記錄見 NT memory crg-graph-build-node-loss）。

兩層偵測：
  D1 風險掃描（純文字，秒級）：同名方法出現於 ≥2 個 impl 塊的檔案。
    **per-block 計數非全體交集**——交集會被 Drop 等單方法 impl 清空
    （kernel.rs 三 impl〔inherent＋Drop＋trait〕漏報實證）。縮排 impl
    （inline ``mod`` 內）可匹配；impl 閉合＝與 impl 關鍵字同縮排的 ``}``，
    無法閉合時方法歸屬保守膨脹
  D2 對帳（每檔百毫秒級）：rust-analyzer ``symbols`` stdin 模式（獨立源）
    vs graph.db nodes 每名計數——DB 少於 rust-analyzer＝被去重吃掉。
    **DB 側 kind 須含 'Test'**——CRG 把 #[test] 函數建為 Test 節點、
    rust-analyzer 側是 Function/Method，漏計＝全部測試函數誤報缺差
    （NT N1 首跑 1,670 假警報實證）

已知保守偏差（D1 只膨脹風險清單、不漏）：blanket impl（``impl<T> Trait
for T``）與 ``impl Trait for Vec<X>`` 歸到 ``T``/``Vec`` 名下；``dyn`` 型別
已處理、泛型路徑追蹤從簡。

用法::

    uv run --project ~/Github/ai-rules python -m code_reality.graph_audit \
        --repo <repo> [--all] [--json] [--graph PATH]

掃描集＝profile ``[[scan_root]]`` 的 path glob（Rust 形態 repo）；無
scan_root → repo 全 ``*.rs`` 經 exclusions 過濾（generic fallback）。
graph 預設 ``<repo>/.code-review-graph/graph.db``。退出碼：0=乾淨｜
1=發現缺差｜2=環境錯誤（rust-analyzer 未裝／graph.db 不在／全部檔案 RA
解析失敗——假乾淨防護）。``--json`` 鍵（risk_files/audited_files/
missing/errors）為 NT 治理鉤子契約（errors 鍵＝NT 最終版新增）。
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path

from code_reality.common import connect_ro, graph_db_path
from code_reality.exclusions import is_excluded
from code_reality.profile import load_profile, scan_roots

# D1 正則（NT 三輪獨立審查迭代後形態）：前綴容許 unsafe；泛型段 [^{]* 吞
# 巢狀 >；trait 容許路徑限定 fmt::Display；for 型容許 dyn Foo
IMPL_RE = re.compile(
    r"^\s*(?:unsafe\s+)?impl(?:<[^{]*>)?\s+"
    r"(?:(?:\w+::)*\w+\s+for\s+)?((?:dyn\s+)?[A-Z]\w*)"
)
FN_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:const\s+|async\s+|unsafe\s+|extern(?:\s*\"[^\"]*\")?\s+)*fn\s+(\w+)"
)
RA_LABEL_RE = re.compile(r'label: "([^"]*)"')
RA_KIND_RE = re.compile(r"kind: SymbolKind\((\w+)\)")
RA_FN_KINDS = {"Function", "Method"}


def scan_files(repo: Path) -> list[Path]:
    """掃描集：profile [[scan_root]] path glob；無 → repo 全 *.rs 經
    exclusions 過濾（generic fallback——與 PathResolver/pkg_roots 同型）。"""
    profile = load_profile(repo)
    roots = scan_roots(profile)
    if roots:
        return sorted({p for sr in roots for p in repo.glob(sr.path)})
    return sorted(
        p
        for p in repo.rglob("*.rs")
        if not is_excluded(p.relative_to(repo).as_posix(), profile)
    )


def risk_scan(files: list[Path]) -> list[tuple[Path, str, list[str]]]:
    """D1：同名方法出現於 ≥2 個 impl 塊的檔案（不碰 graph）。

    準則＝任兩塊碰撞（每方法計「出現於幾個 impl 塊」，≥2 即候選）——非全體
    交集（會被 Drop 等單方法 impl 清空——kernel.rs 三 impl 實證漏報）。
    impl 閉合偵測＝與 impl 關鍵字同縮排的 ``}``；無法閉合時方法歸屬保守膨脹。
    """
    at_risk: list[tuple[Path, str, list[str]]] = []
    for f in files:
        impls: list[list] = []  # [type, [fn names], indent]
        cur: list | None = None
        for line in f.read_text(encoding="utf-8", errors="replace").splitlines():
            m = IMPL_RE.match(line)
            if m:
                cur = [m.group(1), [], len(line) - len(line.lstrip())]
                impls.append(cur)
                continue
            if cur is None:
                continue
            stripped = line.strip()
            if stripped == "}":
                if len(line) - len(line.lstrip()) <= cur[2]:
                    cur = None
                continue
            fm = FN_RE.match(line)
            if fm:
                cur[1].append(fm.group(1))
        block_counts: dict[str, Counter] = {}
        for entry in impls:
            counts = block_counts.setdefault(entry[0], Counter())
            for name in set(entry[1]):
                counts[name] += 1
        for t, counts in block_counts.items():
            overlap = sorted(n for n, c in counts.items() if c >= 2)
            if overlap:
                at_risk.append((f, t, overlap))
    return at_risk


def parse_ra_symbols(stdout_text: str) -> Counter:
    """rust-analyzer ``symbols`` 輸出 → (label → fn 計數)；非 fn kind 略過。"""
    counts: Counter = Counter()
    for line in stdout_text.splitlines():
        kind = RA_KIND_RE.search(line)
        if not kind or kind.group(1) not in RA_FN_KINDS:
            continue
        label = RA_LABEL_RE.search(line)
        if label:
            counts[label.group(1)] += 1
    return counts


def ra_symbols(path: Path) -> Counter | None:
    """rust-analyzer stdin 模式（獨立於 CRG parser 的對帳源）。

    逾時/失敗回 None（caller 記 errors 不崩潰）；``check=False``＝非零退出
    輸出空清單——vacuous pass（零比較）非全缺差，audit 迴圈對非空檔零輸出
    印 [WARN]、main 聚合零符號擋 exit 2（雙層防靜默假陰性）。
    """
    try:
        proc = subprocess.run(
            ["rust-analyzer", "symbols"],
            input=path.read_bytes(),
            capture_output=True,
            timeout=60,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return None
    return parse_ra_symbols(proc.stdout.decode("utf-8", errors="replace"))


def db_functions(conn, path: Path) -> Counter:
    """graph.db nodes 每名計數——kind 含 'Test'（漏計＝測試函數全誤報）。"""
    rows = conn.execute(
        "SELECT name, COUNT(*) FROM nodes WHERE file_path = ? "
        "AND kind IN ('Function', 'Test') GROUP BY name",
        (str(path.resolve()),),
    )
    return Counter({name: count for name, count in rows})


def audit(
    repo: Path,
    graph: Path,
    *,
    all_files: bool = False,
    ra_lookup=None,
) -> tuple[
    list[tuple[Path, str, list[str]]], int, list[dict[str, object]], list[str], int
]:
    """D1＋D2 主流程——回 (risk, audited_count, missing, errors, total_ra)。

    ``ra_lookup`` 注入點供測試替身（預設 rust-analyzer subprocess；回 None
    ＝逾時/失敗記 errors 跳過）。
    """
    files = scan_files(repo)
    risk = risk_scan(files)
    scope = files if all_files else sorted({f for f, _, _ in risk})
    lookup = ra_lookup or ra_symbols
    conn = connect_ro(graph)
    try:
        missing: list[dict[str, object]] = []
        errors: list[str] = []
        total_ra = 0
        for f in scope:
            ra = lookup(f)
            if ra is None:
                errors.append(f"{f}: rust-analyzer 逾時/失敗（跳過）")
                continue
            total_ra += sum(ra.values())
            db = db_functions(conn, f)
            if not ra and f.stat().st_size > 0:
                print(
                    f"[WARN] rust-analyzer 對 {f.name} 零輸出（格式 drift 或"
                    "單檔 parse fail）——該檔對帳 vacuous，勿當乾淨讀",
                    file=sys.stderr,
                )
            for name, ra_count in ra.items():
                db_count = db.get(name, 0)
                if db_count < ra_count:
                    missing.append(
                        {
                            "file": str(f),
                            "symbol": name,
                            "ra_count": ra_count,
                            "db_count": db_count,
                        }
                    )
    finally:
        conn.close()
    return risk, len(scope), missing, errors, total_ra


def main() -> int:
    parser = argparse.ArgumentParser(description="CRG graph.db Rust 完整度稽核")
    parser.add_argument("--repo", type=Path, required=True, help="掃描目標 repo 根")
    parser.add_argument(
        "--all", action="store_true", help="對帳全部 .rs（預設僅風險檔）"
    )
    parser.add_argument(
        "--json", action="store_true", help="機器可讀輸出（治理鉤子契約）"
    )
    parser.add_argument(
        "--graph",
        type=Path,
        default=None,
        help="覆寫 graph.db 路徑（預設 <repo>/.code-review-graph/graph.db）",
    )
    args = parser.parse_args()

    if shutil.which("rust-analyzer") is None:
        print(
            "[FAIL] rust-analyzer 不在 PATH——rustup component add rust-analyzer",
            file=sys.stderr,
        )
        return 2
    graph = args.graph if args.graph is not None else graph_db_path(args.repo)
    if not graph.exists():
        print(
            f"[FAIL] graph.db 不存在：{graph}（完整度稽核需要它；"
            "新鮮度指標不保證存在）",
            file=sys.stderr,
        )
        return 2

    risk, audited, missing, errors, total_ra = audit(
        args.repo, graph, all_files=args.all
    )
    # 假乾淨防護：RA 輸出格式漂移或環境異常時 total 為 0——回 2 而非 0
    if audited and total_ra == 0:
        print(
            "[FAIL] 全部檔案 rust-analyzer 符號數為 0——輸出格式漂移或環境錯誤",
            file=sys.stderr,
        )
        return 2

    if args.json:
        print(
            json.dumps(
                {
                    "risk_files": [
                        {"file": str(f), "type": t, "overlap": o} for f, t, o in risk
                    ],
                    "audited_files": audited,
                    "missing": missing,
                    "errors": errors,
                },
                ensure_ascii=False,
                indent=1,
            )
        )
    else:
        print(f"[OK] D1 風險掃描：{len(risk)} 檔（同名方法 ≥2 impl 塊）")
        print(
            f"[OK] D2 對帳：{audited} 檔（rust-analyzer vs graph.db，{total_ra} 符號）"
        )
        for e in errors:
            print(f"[WARN] {e}")
        if missing:
            print(
                f"[WARN] DB 缺差 {len(missing)} 項（同鍵去重吃掉——"
                "head_matches_build 不反映此項）："
            )
            by_file: dict[str, list[dict[str, object]]] = {}
            for m in missing:
                by_file.setdefault(str(m["file"]), []).append(m)
            for f, items in sorted(by_file.items()):
                syms = ", ".join(
                    f"{m['symbol']}({m['db_count']}/{m['ra_count']})" for m in items
                )
                print(f"  {f}: {syms}")
        else:
            print("[OK] 無缺差")

    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
