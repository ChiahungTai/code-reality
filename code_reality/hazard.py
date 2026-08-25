"""dynamic dispatch hazard 偵測——hub_refs 的 hazard stage（判定層純函數）。

hub_refs 靜態 callers（CRG／Tree-sitter）看不到 dynamic dispatch；本模組
對目標 symbol 偵測六類 hazard pattern，輸出「N prod callers ⚠ K dynamic
hazards」註記（防 LLM 誤判「無引用可刪」）。收編自 mosaic P2 原型
（`.agent-tmp/research/p2/`，四 case 對帳報告同目錄 REPORT.md）。

分層（策略在 hub_refs 端——hazard_stage）：
  常駐 AST 級（零 rg 成本，callers 查詢每次都跑）：registry-auto-discovery
    ＋ protocol/strentenum 存在性（count=0＝存在性訊號，未跑 rg 計數）
  觸發式 rg 級（static_prod ≤ 2 或 --hazard 才跑）：全六規則含計數——
    rg 全 repo 掃描每條 ~1-3s；「callers 少才需要 hazard 警告」，callers
    多時靜態證據已足

Hazard 規則（判定在純函數層，可測）：
  strentenum-string-dispatch——StrEnum member 字串值被當 literal 消費
    （YAML config／dict key，靜態 callers 圖完全看不到）
  getattr-string-dispatch——getattr(obj, "<symbol>") 動態取得
  registry-auto-discovery——經 auto_register_*() 註冊（profile
    ``[[hazard_registry]]``——repo 事實歸 repo，工具層不內建）
  protocol-duck-typing——Protocol subclass（實作類消費不經繼承邊）
  importlib-lazy-load——所在模組被 import_module("<literal>") 字串引用
  static-edge-gap——rg 呼叫檔集合 − CRG callers 檔集合（CRG 漏邊補洞，
    FactorCache 的 labels_extend.py:84 構造實證）

已知限制（原型誠實清單，收編沿用）：rg 文字匹配短字串值（"1d"）有
false positive（量級訊號有效、精確計數無效）；f-string／變數形式
import_module 偵測不到（需資料流分析）；method 名跨 class 噪聲
（``.method(`` 匹配任何同名 method）；AST base 名比對不解析 import 別名。
"""

import ast
import re
import sqlite3
import subprocess
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from code_reality.common import (
    assert_db_unchanged,
    connect_ro,
    db_mtime_ns,
    graph_db_path,
    repo_relative,
)
from code_reality.exclusions import is_excluded
from code_reality.profile import HazardRegistry, Profile

# rg 匹配行類型：path:line:content
RgRunner = Callable[[list[str]], list[str]]

STR_ENUM_BASES = {"StrEnum", "BaseStrEnum"}
PROTOCOL_BASES = {"Protocol"}


@dataclass
class SymbolFacts:
    """AST 解析出的 symbol 事實——判定層唯一輸入。"""

    name: str
    is_class: bool = False
    bases: list[str] = field(default_factory=list)
    is_strentenum: bool = False
    is_protocol: bool = False
    enum_values: list[str] = field(default_factory=list)
    rel_path: str | None = None  # repo 相對定義檔（None=repo 外或解析失敗）
    module: str | None = None  # dotted module path
    kind: str | None = None  # CRG node kind


@dataclass
class HazardFinding:
    """單一 hazard 命中——pattern 類別 × 計數 × 代表證據。"""

    kind: str
    count: int
    summary: str
    evidence: list[str] = field(default_factory=list)
    detail: dict[str, int] = field(default_factory=dict)  # 細分計數（如 prod/test）


# ══════════════════════════════════════════════════════════════════
# 判定層（純函數——測試用合成源碼直接餵）
# ══════════════════════════════════════════════════════════════════


def parse_symbol_facts(source: str, symbol: str) -> SymbolFacts:
    """源碼字串 → symbol 事實（AST）。

    bases 用「名稱字串」比對（不解析 import 別名——
    ``from enum import StrEnum as SE`` 會漏，記為 gap）。
    """
    facts = SymbolFacts(name=symbol)
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return facts
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef) and node.name == symbol:
            facts.is_class = True
            facts.bases = [
                b.id if isinstance(b, ast.Name) else _dotted_attr(b)
                for b in node.bases
                if isinstance(b, (ast.Name, ast.Attribute))
            ]
            base_names = set(facts.bases)
            facts.is_strentenum = bool(base_names & STR_ENUM_BASES) or (
                "str" in base_names and "Enum" in base_names
            )
            facts.is_protocol = bool(base_names & PROTOCOL_BASES)
            facts.enum_values = _extract_str_members(node)
            break
    return facts


def _dotted_attr(node: ast.Attribute) -> str:
    parts: list[str] = []
    cur: ast.expr = node
    while isinstance(cur, ast.Attribute):
        parts.append(cur.attr)
        cur = cur.value
    if isinstance(cur, ast.Name):
        parts.append(cur.id)
    return ".".join(reversed(parts))


def _extract_str_members(cls_node: ast.ClassDef) -> list[str]:
    """class body 兡 `NAME = "value"` 字串 assignment——StrEnum member 值。"""
    values: list[str] = []
    for stmt in cls_node.body:
        if (
            isinstance(stmt, ast.Assign)
            and len(stmt.targets) == 1
            and isinstance(stmt.targets[0], ast.Name)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        ):
            values.append(stmt.value.value)
    return values


def method_name(symbol: str) -> str | None:
    """``Class.method``／``<path>::Class.method`` → method 名；裸 class → None。"""
    bare = symbol.split("::", 1)[1] if "::" in symbol else symbol
    return bare.split(".", 1)[1] if "." in bare else None


def build_getattr_pattern(symbol: str) -> str:
    """getattr 派發 rg pattern——`getattr(obj, "<symbol>")`。"""
    return rf"getattr\(\s*[A-Za-z_][A-Za-z0-9_.]*\s*,\s*[\"']{re.escape(symbol)}[\"']"


def build_strentenum_patterns(values: list[str]) -> list[str]:
    """StrEnum member 值的 rg -F pattern（含引號錨定）。"""
    return [f'"{v}"' for v in values]


def build_importlib_pattern(module: str) -> str:
    """import_module("<module>") literal 引用 pattern。"""
    return rf"import_module\(\s*[\"']{re.escape(module)}[\"']"


def classify_rg_lines(
    lines: list[str], profile: Profile | None = None
) -> tuple[list[str], list[str], list[str]]:
    """rg -n 輸出行 → (prod, test, excluded) path:line 清單。

    tests/ 前綴切分照 hub_refs.aggregate 的 heuristic；exclusions 用
    profile（無 profile 走 generic fallback）。
    """
    prod: list[str] = []
    test: list[str] = []
    excluded: list[str] = []
    for ln in lines:
        rel = ln.split(":", 1)[0]
        if rel.startswith("tests/"):
            test.append(ln)
        elif is_excluded(rel, profile):
            excluded.append(ln)
        else:
            prod.append(ln)
    return prod, test, excluded


def detect_strentenum_string_dispatch(
    facts: SymbolFacts, rg: RgRunner, profile: Profile | None = None
) -> HazardFinding | None:
    """symbol 是 StrEnum → member 字串值 literal 消費計數。

    排除定義檔自身；prod/test 分開計。噪聲限制：短值（"1d"）會匹配
    非 enum 用途的字串（如 polars rule "1d"）——不辨語義，誠實計總。
    """
    if not facts.is_strentenum or not facts.enum_values:
        return None
    args = ["-F"]
    for p in build_strentenum_patterns(facts.enum_values):
        args.extend(["-e", p])
    lines = rg(args)
    if facts.rel_path:
        lines = [ln for ln in lines if not ln.startswith(facts.rel_path)]
    prod, test, _ = classify_rg_lines(lines, profile)
    if not lines:
        return None
    return HazardFinding(
        kind="strentenum-string-dispatch",
        count=len(prod) + len(test),
        summary=(
            f"StrEnum member 字串值 literal 消費 {len(prod)} 處 prod + "
            f"{len(test)} 處 test（YAML/config/dict key——靜態 callers 圖外）"
        ),
        evidence=prod[:5],
        detail={"prod": len(prod), "test": len(test)},
    )


def detect_getattr_dispatch(
    facts: SymbolFacts, rg: RgRunner, profile: Profile | None = None
) -> HazardFinding | None:
    """getattr(obj, "<symbol>") 動態取得點。"""
    pattern = build_getattr_pattern(facts.name)
    lines = rg([pattern])
    if facts.rel_path:
        lines = [ln for ln in lines if not ln.startswith(facts.rel_path)]
    prod, test, _ = classify_rg_lines(lines, profile)
    if not lines:
        return None
    return HazardFinding(
        kind="getattr-string-dispatch",
        count=len(prod) + len(test),
        summary=f'getattr(<obj>, "{facts.name}") 動態取得 {len(prod)} prod + {len(test)} test 處',
        evidence=(prod + test)[:5],
        detail={"prod": len(prod), "test": len(test)},
    )


def detect_registry_auto_discovery(
    facts: SymbolFacts, registries: tuple[HazardRegistry, ...]
) -> HazardFinding | None:
    """symbol 定義在 registry 掃描路徑內 + 名稱 suffix 匹配 → 註冊推定。"""
    if not facts.is_class or not facts.rel_path:
        return None
    for reg in registries:
        if facts.rel_path.startswith(reg.package_prefix) and facts.name.endswith(
            reg.suffix
        ):
            return HazardFinding(
                kind="registry-auto-discovery",
                count=1,
                summary=(
                    f"經 {reg.register_fn}() 註冊到 {reg.registry}——"
                    "callers 邊不含 registry 字串 spec_name dispatch 點；"
                    "「0 callers 可刪」判斷對 registry 成員恆為誤導"
                ),
                evidence=[reg.evidence] if reg.evidence else [],
            )
    return None


def detect_protocol_duck_typing(
    facts: SymbolFacts, rg: RgRunner, profile: Profile | None = None
) -> HazardFinding | None:
    """symbol 是 Protocol subclass → 型別標註消費點計數。

    duck-typing 重點：實作類不經繼承邊消費 Protocol；Protocol 本身的
    「引用」是參數標註位置（CRG nodes 有 Type 節點但呼叫邊語意不同）。
    """
    if not facts.is_protocol:
        return None
    pattern = rf"(?::\s*|->\s*|isinstance\([^,]*,\s*){re.escape(facts.name)}\b"
    lines = rg([pattern])
    prod, test, _ = classify_rg_lines(lines, profile)
    if not lines:
        return None
    return HazardFinding(
        kind="protocol-duck-typing",
        count=len(prod) + len(test),
        summary=(
            f"Protocol 型別標註/檢查 {len(prod)} prod + {len(test)} test 處——"
            "實作類消費不經繼承邊（structural typing）"
        ),
        evidence=prod[:5],
        detail={"prod": len(prod), "test": len(test)},
    )


def detect_importlib_lazy_load(
    facts: SymbolFacts, rg: RgRunner, profile: Profile | None = None
) -> HazardFinding | None:
    """symbol 所在模組被 import_module("<literal>") 引用。"""
    if not facts.module:
        return None
    pattern = build_importlib_pattern(facts.module)
    lines = rg([pattern])
    if not lines:
        return None
    prod, test, _ = classify_rg_lines(lines, profile)
    return HazardFinding(
        kind="importlib-lazy-load",
        count=len(prod) + len(test),
        summary=f'import_module("{facts.module}") literal 引用——模組邊經字串',
        evidence=(prod + test)[:5],
        detail={"prod": len(prod), "test": len(test)},
    )


def detect_static_edge_gap(
    facts: SymbolFacts,
    static_caller_files: set[str] | None,
    rg: RgRunner,
    method: str | None = None,
) -> HazardFinding | None:
    """rg 呼叫檔集合 − CRG callers 檔集合——CRG 漏邊（量化 hub_refs 缺口）。

    ``static_caller_files=None``（callees 方向等無 baseline 場景）→ 跳過。
    兩形態：裸 class → ctor 呼叫 ``\\bSymbol\\(``；Class.method → 屬性呼叫
    ``\\.method\\(``。差集非空 = 有檔案呼叫了 symbol 但 CRG 沒建邊
    （實證：FactorCache 的 labels_extend.py:84 構造被 CRG 漏）。
    噪聲限制：method 形態匹配任何同名 method（跨 class）；作漏邊定位，
    非精確邊。
    """
    if static_caller_files is None:
        return None
    if method is None and "." not in facts.name:
        pattern = rf"\b{re.escape(facts.name)}\("
    elif method:
        pattern = rf"\.{re.escape(method)}\("
    else:
        return None
    lines = rg([pattern])
    if facts.rel_path:
        lines = [ln for ln in lines if not ln.startswith(facts.rel_path)]
    rg_files = {ln.split(":", 1)[0] for ln in lines}
    prod_missing = {
        f for f in rg_files - static_caller_files if not f.startswith("tests/")
    }
    test_missing = {f for f in rg_files - static_caller_files if f.startswith("tests/")}
    missing = prod_missing | test_missing
    if not missing:
        return None
    return HazardFinding(
        kind="static-edge-gap",
        count=len(missing),
        summary=(
            f"{'.' + method if method else facts.name} 呼叫檔 "
            f"{len(missing)} 個不在 CRG callers（prod {len(prod_missing)} / "
            f"test {len(test_missing)}）——rg 可見但靜態圖漏邊"
        ),
        evidence=sorted(missing)[:5],
        detail={
            "rg_files": len(rg_files),
            "crg_files": len(static_caller_files),
            "missing_prod": len(prod_missing),
            "missing_test": len(test_missing),
        },
    )


def resident_findings(
    facts: SymbolFacts, registries: tuple[HazardRegistry, ...]
) -> list[HazardFinding]:
    """常駐 AST 級（零 rg 成本）——存在性安全網。

    count=0＝存在性訊號（rg 級才有計數）。價值實證：Interval 有 4 個
    prod callers（高於 rg 觸發閾值）但字串值消費 900 處——常駐層讓
    高 callers 符號的 hidden hazard 仍可見。
    """
    findings: list[HazardFinding] = []
    if facts.is_strentenum and facts.enum_values:
        vals = ", ".join(repr(v) for v in facts.enum_values[:3])
        more = " 等" if len(facts.enum_values) > 3 else ""
        findings.append(
            HazardFinding(
                kind="strentenum-string-dispatch",
                count=0,
                summary=(
                    f"StrEnum class（{vals}{more}）——member 字串值 literal "
                    "消費在靜態 callers 圖外"
                ),
            )
        )
    f = detect_registry_auto_discovery(facts, registries)
    if f:
        findings.append(f)
    if facts.is_protocol:
        findings.append(
            HazardFinding(
                kind="protocol-duck-typing",
                count=0,
                summary="Protocol subclass——實作類消費不經繼承邊（structural typing）",
            )
        )
    return findings


def full_findings(
    facts: SymbolFacts,
    registries: tuple[HazardRegistry, ...],
    rg: RgRunner,
    static_caller_files: set[str] | None,
    profile: Profile | None = None,
    method: str | None = None,
) -> list[HazardFinding]:
    """全規則掃描（rg 級，觸發式）——六規則齊發含計數。"""
    findings: list[HazardFinding] = []
    for det in (
        detect_strentenum_string_dispatch,
        detect_getattr_dispatch,
    ):
        f = det(facts, rg, profile)
        if f:
            findings.append(f)
    f = detect_registry_auto_discovery(facts, registries)
    if f:
        findings.append(f)
    f = detect_protocol_duck_typing(facts, rg, profile)
    if f:
        findings.append(f)
    f = detect_importlib_lazy_load(facts, rg, profile)
    if f:
        findings.append(f)
    f = detect_static_edge_gap(facts, static_caller_files, rg, method=method)
    if f:
        findings.append(f)
    return findings


def hazard_gate_warning(
    static_prod: int,
    static_test: int,
    findings: list[HazardFinding],
    threshold: int = 2,
) -> str | None:
    """§5.4 語意：靜態 callers 少（prod ≤ threshold）且 hazard 命中 → 警告行。

    ``threshold`` 由 hub_refs 傳 ``RG_TRIGGER_PROD``——觸發掃描與 gate 警告
    須同一閾值（分裂則警告覆蓋面靜默窄於掃描面）。
    """
    if static_prod <= threshold and findings:
        kinds = "、".join(f.kind for f in findings)
        return (
            f"[WARN] 靜態 prod callers 僅 {static_prod} 但命中 {len(findings)} 類 "
            f"dynamic hazard（{kinds}）——「無引用可刪」判斷需先查 hazard 明細"
        )
    return None


# ══════════════════════════════════════════════════════════════════
# Orchestration 層（hub_refs hazard_stage 的支撐）
# ══════════════════════════════════════════════════════════════════


def symbol_facts(symbol: str, repo_root: Path, profile: Profile | None) -> SymbolFacts:
    """nodes 表解析 symbol 定義檔（照 hub_refs.resolve_qualified 慣例）→ facts。

    hub_refs 已為 callers 查詢解析過 symbol；本函式重查 nodes 表取定義檔
    ／kind 供 AST 規則用。解析失敗（非唯一／不在 repo／無 graph.db）不炸
    ——hazard 是 advisory stage，降級為 name-only facts（rg 規則仍可用）。
    """
    bare = symbol.split("::", 1)[1] if "::" in symbol else symbol
    cls_name = bare.split(".", 1)[0]
    facts = SymbolFacts(name=cls_name)
    db_path = graph_db_path(repo_root)
    if not db_path.exists():
        return facts
    m0 = db_mtime_ns(db_path)
    conn = connect_ro(db_path)
    try:
        if "." in bare:
            parent, _, name = bare.rpartition(".")
            rows = conn.execute(
                "SELECT qualified_name, file_path, kind FROM nodes "
                "WHERE name = ? AND parent_name = ?",
                (name, parent),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT qualified_name, file_path, kind FROM nodes WHERE name = ?",
                (bare,),
            ).fetchall()
    except sqlite3.DatabaseError as e:
        # 與 hub_refs.resolve_qualified 同訊息品質（qualified-name 流不經
        # 該函式的 db 檢查——hazard stage 會是第一個碰表的）
        raise AssertionError(
            f"非 CRG graph.db（讀 nodes 失敗：{e}）：{db_path}"
            "——先跑 `uvx code-review-graph build`"
        ) from e
    finally:
        conn.close()
    assert_db_unchanged(db_path, m0)
    pairs: list[tuple[str, str, str]] = []
    for q, fp, k in rows:
        rel = repo_relative(fp, repo_root)
        if rel is not None and not is_excluded(rel, profile):
            pairs.append((q, rel, k))
    if len(pairs) != 1:
        return facts
    _, rel, kind = pairs[0]
    parsed = parse_symbol_facts(_read_source(repo_root / rel), cls_name)
    parsed.rel_path = rel
    parsed.module = str(Path(rel).with_suffix("")).replace("/", ".")
    parsed.kind = kind
    return parsed


def _read_source(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def make_rg_runner(repo_root: Path) -> RgRunner:
    """rg -n --no-heading runner——搜尋範圍 py+yaml+json+toml。

    排除 stubs/ai-analysis（跨 repo AI workspace 慣例目錄；.venv/
    .agent-tmp/.code-review-graph 為 hidden 本就跳過）；輸出行統一轉
    repo 相對路徑（與 CRG callers 檔集合可比對）。

    **cwd=root＋路徑 ``.``（非絕對路徑 arg）**：rg 的 ``-g`` glob 錨定
    在路徑arg 形態——絕對路徑 root 下 ``!stubs/**`` 對
    ``/abs/root/stubs/s.py`` 不匹配（排除靜默失效，原型實證）；``.``
    讓 glob 恆錨 repo 根，輸出 ``./rel`` 再剝前綴。
    """
    root = repo_root.resolve()

    def run(args: list[str]) -> list[str]:
        cmd = [
            "rg",
            "-n",
            "--no-heading",
            "-t",
            "py",
            "-t",
            "yaml",
            "-t",
            "json",
            "-t",
            "toml",
            *args,
            ".",
            "-g",
            "!.venv/**",
            "-g",
            "!stubs/**",
            "-g",
            "!ai-analysis/**",
            "-g",
            "!.agent-tmp/**",
            "-g",
            "!.code-review-graph/**",
        ]
        proc = subprocess.run(
            cmd, cwd=root, capture_output=True, text=True, check=False
        )
        if proc.returncode not in (0, 1):
            raise AssertionError(f"rg 失敗（exit {proc.returncode}）: {proc.stderr}")
        out = []
        for ln in proc.stdout.splitlines():
            ln = ln.removeprefix("./")
            if ln.strip():
                out.append(ln)
        return out

    return run
