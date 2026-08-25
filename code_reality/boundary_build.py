"""PyO3 boundary extractor——NT pyo3 宣告 ↔ Python .pyi 合約對照 sidecar。

跨語言縫是 evidence fusion 的最後盲區（pyright 看不進 Rust、rust-analyzer
看不見 Python caller）：掃 NT 全部 crates ``*.rs`` 的 pyo3 宣告，與
``python/nautilus_trader/**/*.pyi`` 合約做命名對應對帳，寫成 commit 錨定
sidecar DB（``~/.mosaic/code-reality/boundary/<nt-short-sha>.db``，冪等
覆寫）。查詢消費端：``code_reality/boundary.py``。

pyclass 兩種落點（native struct ``cfg_attr`` 巢狀／binding 檔 wrapper），
掃描範圍＝全部 crates/**/*.rs（pyclass 不只在 src/python/——原型實證）。
module 路徑真相源＝pyclass / gen_stub_pyclass 的 ``module=`` derive
（.pyi 產生源，語義權威）。

Known Gaps（文檔化不擋——gap-prototypes 報告 §1.4；收案記錄見
ai-analysis/execution-plans/_done/ep-boundary-extractor-formalization.md）：
- ``custom_data!`` 巨集內宣告掃不到（掃描器看不進巨集）→ pyi-only class
  殘差（原型期 16 class）
- credential 欄位選擇性省略機制未釘死 → field_property rs-only 殘差大宗
  （同型 ``Option<String>`` 的 base_url_http 有 property、api_key 無——
  pyi 只進 ``__init__`` kwargs）
- pyi 空白 stub class（無任何 property——bybit http models）→ getter
  rs-only 殘差
- brace 計數非 string-aware：字串內不對稱 ``{``/``}`` 或 ``//``（如 URL
  ``"://"``）會誤判 impl body 邊界（build review 實測 NT corpus 殘差
  1/586 impls——persistence/feather.rs 一個 impl 的 8 個 method 漏掃）
- ``#[pyo3(getter)]`` parenthesized 形態不識別（現 corpus 零命中——
  僅裸名/``pyo3::getter`` 形態）
- method→class join 假設 impl 與 pyclass 宣告同 crate（binding 檔慣例，
  build review R3）；跨 crate 同名時 bare-name 歧義的方法跳過（計入
  ``methods.unresolved_class``）
- sidecar commit-keyed 檔案累積（``<out-dir>/<sha>.db`` 每 commit 一顆）——
  遲需清理策略（保留最新 N 個候選）

用法::

    uv run python -m code_reality.boundary_build [--repo PATH] [--out-dir PATH]

NT repo 只讀——所有產出寫 sidecar（repo 外）。
"""

import argparse
import ast
import json
import re
import sqlite3
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from code_reality.common import connect_ro, make_meta
from code_reality.profile import ScanRoot, load_profile, scan_roots

TOOL = "code_reality.boundary_build"
DEFAULT_OUT_DIR = Path.home() / ".mosaic" / "code-reality" / "boundary"


# ---------------------------------------------------------------------------
# Rust 側掃描
# ---------------------------------------------------------------------------

RE_FN = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:unsafe\s+)?(?:extern\s+\"C\"\s+)?fn\s+(\w+)"
)
RE_STRUCT = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?struct\s+(\w+)")
RE_ENUM = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)")
RE_IMPL = re.compile(r"^impl\b")
RE_KV_STR = re.compile(r'(\w+)\s*=\s*"([^"]*)"')


@dataclass
class RsClass:
    rs_path: str
    line: int
    rust_name: str
    exposed: str
    py_module: str | None


@dataclass
class RsMethod:
    rs_path: str
    line: int
    rust_class: str
    rust_fn: str
    exposed: str
    kind: str  # method | getter | setter | new | staticmethod | classmethod | dunder
    renamed: bool = False  # 真 #[pyo3(name=...)]（get_ 剝前綴不算）


RE_FIELD = re.compile(r"^\s*pub\s+(?:\(crate\)\s+)?(\w+)\s*:")
RE_VARIANT = re.compile(r"^(\w+)\s*(?:\([^)]*\))?\s*,?\s*$")


@dataclass
class RsFunction:
    rs_path: str
    line: int
    rust_fn: str
    exposed: str
    py_module: str | None


def _balanced(s: str) -> bool:
    return s.count("(") == s.count(")") and s.count("[") == s.count("]")


def _collect_attrs(lines: list[str], i: int) -> tuple[list[str], int]:
    """從 i 起收集連續屬性（各可跨行），回傳（屬性文字清單, 下一 index）。"""
    attrs: list[str] = []
    while i < len(lines):
        stripped = lines[i].strip()
        if not stripped.startswith("#["):
            break
        text = stripped
        j = i
        while not _balanced(text) and j + 1 < len(lines):
            j += 1
            text += " " + lines[j].strip()
        attrs.append(text)
        i = j + 1
    return attrs, i


def _skip_doc_blank(lines: list[str], i: int) -> int:
    while i < len(lines):
        s = lines[i].strip()
        if not s or s.startswith(("///", "//!", "//")):
            i += 1
        else:
            break
    return i


def _attr_kind(a: str) -> str | None:
    inner = a[2:-1] if a.endswith("]") else a[2:]
    if re.search(r"\bpyclass\b", inner):
        return "pyclass"
    if re.search(r"\bpymethods\b", inner):
        return "pymethods"
    if re.search(r"\bpyfunction\b", inner):
        return "pyfunction"
    if re.search(r"\bpymodule\b", inner):
        return "pymodule"
    if "gen_stub_pyclass" in inner:
        return "gen_stub_pyclass"
    if "gen_stub_pyfunction" in inner:
        return "gen_stub_pyfunction"
    if re.search(r"^(pyo3::)?(getter|setter)\b", inner):
        return "getter" if inner.startswith(("getter", "pyo3::getter")) else "setter"
    if re.match(r"^(pyo3::)?new$", inner):
        return "new"
    if re.match(r"^(pyo3::)?staticmethod$", inner):
        return "staticmethod"
    if re.match(r"^(pyo3::)?classmethod$", inner):
        return "classmethod"
    if re.match(r"^pyo3\s*\(", inner):
        return "pyo3"
    return None


def _attr_kv(a: str, key: str) -> str | None:
    m = RE_KV_STR.search(a)
    while m:
        if m.group(1) == key:
            return m.group(2)
        m = RE_KV_STR.search(a, m.end())
    return None


def _impl_self_type(header: str) -> str | None:
    """``impl <T> Foo<T> where ... {`` → ``Foo``；trait impl（含 `` for ``）→ None。"""
    body = header.split("{", 1)[0]
    body = body[body.find("impl") + 4 :]
    if " for " in body:
        return None
    if body.startswith("<"):
        depth = 0
        for k, ch in enumerate(body):
            if ch == "<":
                depth += 1
            elif ch == ">":
                depth -= 1
                if depth == 0:
                    body = body[k + 1 :]
                    break
    seg = body.strip().split("<")[0].strip()
    name = seg.split("::")[-1].strip()
    return name if name.isidentifier() else None


def _impl_body_end(lines: list[str], brace_line: int) -> int:
    """從含 ``{`` 的行起算 brace 平衡，回傳 body 結束行 index（含）。"""
    depth = 0
    for k in range(brace_line, len(lines)):
        code = re.sub(r"//.*$", "", lines[k])
        code = re.sub(r"'[{}]'", "X", code)  # char literal（'{' / '}'）防干擾
        depth += code.count("{") - code.count("}")
        if depth == 0:
            return k
    return len(lines) - 1


def _screaming_snake(name: str) -> str:
    """CamelCase → SCREAMING_SNAKE_CASE（pyi enum member 命名慣例）。

    機械證據（2026-08-22 NT 實測）：bybit ``IsolatedMargin`` ↔ pyi
    ``ISOLATED_MARGIN``、binance ``UsdM`` ↔ ``USD_M``——naive ``.upper()``
    會產 ``ISOLATEDMARGIN``/``USDM`` 永不匹配（build review R1，261 邊）。
    單 hump（``Sandbox``→``SANDBOX``）兩規則同答案——AxEnvironment 實證。
    """
    s = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", name)
    s = re.sub(r"(?<=[A-Z])(?=[A-Z][a-z])", "_", s)
    return s.upper()


def _exposed_method_name(fn: str, kind: str, rename: str | None) -> str:
    """python 暴露名：rename > getter/setter get_/set_ 剝前綴 > py_ 剝前綴 > 原名。

    ``py_`` 剝前綴規則的機械證據（2026-08-22 NT 實測）：全 41 個 .pyi
    ``rg 'def py_'`` 零命中——pyo3-stub-gen 對無 rename 的 ``py_*`` 自動
    剝前綴（pyi:153 ``with_credentials`` ↔ http.rs ``py_with_credentials``）。
    setter 同剝（build review R2：actor.rs ``set_actor_id`` ↔ pyi
    ``@actor_id.setter``——pyi 側鍵為 property 名）。
    """
    if rename:
        return rename
    if kind == "new":
        return "__new__"  # canonical；build_boundary 端按 pyi 實際形態重解析
    if kind in ("getter", "setter") and fn.startswith(("get_", "set_")):
        return fn[4:]
    if fn.startswith("py_"):
        return fn[3:]
    return fn


def scan_rust_file(
    path: Path, repo: Path
) -> tuple[list[RsClass], list[RsMethod], list[RsFunction]]:
    rel = str(path.relative_to(repo))
    lines = path.read_text(errors="replace").splitlines()
    classes: list[RsClass] = []
    methods: list[RsMethod] = []
    functions: list[RsFunction] = []

    i = 0
    while i < len(lines):
        attrs, j = _collect_attrs(lines, i)
        kinds = [_attr_kind(a) for a in attrs]
        j = _skip_doc_blank(lines, j)
        if j >= len(lines):
            break
        line = lines[j]
        stripped = line.strip()

        # ---- pymethods impl：掃 body 收 method ----
        if "pymethods" in kinds and RE_IMPL.match(stripped):
            header = stripped
            k = j
            while "{" not in header and k + 1 < len(lines):
                k += 1
                header += " " + lines[k].strip()
            cls_name = _impl_self_type(header)
            end = _impl_body_end(lines, k)
            m = j + 1
            while m < end:
                s = lines[m].strip()
                if not s or s.startswith(("///", "//!", "//")):
                    m += 1
                    continue
                inner_attrs, n = _collect_attrs(lines, m)
                inner_kinds = [_attr_kind(a) for a in inner_attrs]
                n = _skip_doc_blank(lines, n)
                if n >= end:
                    break
                fn_m = RE_FN.match(lines[n].strip())
                if fn_m:
                    fn = fn_m.group(1)
                    rename = None
                    for a, kd in zip(inner_attrs, inner_kinds):
                        if kd == "pyo3":
                            # name= 與 signature= 常並列——只覆寫有 name= 的，
                            # 後到的 signature attr 不得清掉 rename（2026-08-22
                            # 抽樣驗證抓到的 last-wins bug）
                            rename = _attr_kv(a, "name") or rename
                    if "new" in inner_kinds:
                        kind = "new"
                    elif "getter" in inner_kinds:
                        kind = "getter"
                    elif "setter" in inner_kinds:
                        kind = "setter"
                    elif "staticmethod" in inner_kinds:
                        kind = "staticmethod"
                    elif "classmethod" in inner_kinds:
                        kind = "classmethod"
                    elif fn.startswith("__") and fn.endswith("__"):
                        kind = "dunder"
                    else:
                        kind = "method"
                    exposed = _exposed_method_name(fn, kind, rename)
                    methods.append(
                        RsMethod(
                            rel,
                            n + 1,
                            cls_name or "?",
                            fn,
                            exposed,
                            kind,
                            rename is not None,
                        )
                    )
                    m = n + 1
                else:
                    m += 1  # attrs 沒跟到 fn——保守前進一步（doc-skip 事故修法）
            i = end + 1
            continue

        # ---- pyclass struct/enum ----
        if ("pyclass" in kinds or "gen_stub_pyclass" in kinds) and (
            (sm := RE_STRUCT.match(stripped)) or (sm := RE_ENUM.match(stripped))
        ):
            rust_name = sm.group(1)
            is_enum = RE_ENUM.match(stripped) is not None
            module = None
            name_attr = None
            attr_blob = " ".join(attrs)
            for a, kd in zip(attrs, kinds):
                if kd in ("pyclass", "gen_stub_pyclass"):
                    module = _attr_kv(a, "module") or module
                    name_attr = _attr_kv(a, "name") or name_attr
            classes.append(
                RsClass(
                    rel,
                    i + 1,  # attr 區塊起始行（pyclass 錨點語義，對齊 tour 錨）
                    rust_name,
                    name_attr or rust_name,
                    module,
                )
            )
            # 欄位/variant 合成：from_py_object/get_all → pyi property；
            # pyclass enum → pyi member（variant）。此為 pyi property 大宗的
            # 產生機制（from_py_object 596 處 vs 顯式 #[getter] 少量——
            # 2026-08-22 NT 實證，見報告）。
            body_start = j
            while (
                "{" not in lines[body_start]
                and body_start + 1 < len(lines)
                and body_start < j + 3
            ):
                body_start += 1
            if "{" not in lines[body_start]:  # tuple struct 無 body——跳過合成
                i = j + 1
                continue
            body_end = _impl_body_end(lines, body_start)
            if is_enum:
                k2 = body_start + 1
                pending_rename: str | None = None
                while k2 < body_end:
                    s2 = lines[k2].strip()
                    if not s2 or s2.startswith(("///", "//")):
                        k2 += 1
                        continue
                    if s2.startswith("#["):
                        if nm := _attr_kv(s2, "name"):
                            pending_rename = nm
                        k2 += 1
                        continue
                    vm = RE_VARIANT.match(s2)
                    if vm:
                        # pyi member 命名＝SCREAMING_SNAKE_CASE（CamelCase
                        # 轉換——UsdM↔USD_M；build review R1），variant 級
                        # #[pyo3(name)] rename 優先（AxEnvironment Sandbox
                        # ↔ pyi SANDBOX）
                        exposed = pending_rename or _screaming_snake(vm.group(1))
                        methods.append(
                            RsMethod(
                                rel,
                                k2 + 1,
                                rust_name,
                                vm.group(1),
                                exposed,
                                "variant",
                                pending_rename is not None,
                            )
                        )
                        pending_rename = None
                    k2 += 1
            elif "from_py_object" in attr_blob or "get_all" in attr_blob:
                for k2 in range(body_start + 1, body_end):
                    fm = RE_FIELD.match(lines[k2])
                    if fm:
                        methods.append(
                            RsMethod(
                                rel,
                                k2 + 1,
                                rust_name,
                                fm.group(1),
                                fm.group(1),
                                "field_property",
                            )
                        )
            i = body_end + 1
            continue

        # ---- pyfunction（module 從 gen_stub_pyfunction / pyfunction(module=)）----
        if ("pyfunction" in kinds or "gen_stub_pyfunction" in kinds) and (
            fn_m := RE_FN.match(stripped)
        ):
            module = None
            rename = None
            for a, kd in zip(attrs, kinds):
                if kd in ("pyfunction", "gen_stub_pyfunction"):
                    module = _attr_kv(a, "module") or module
                if kd == "pyo3":
                    # 同 method 分支的 last-wins 保護（R4）——signature attr
                    # 不得清掉 name=
                    rename = _attr_kv(a, "name") or rename
            fn = fn_m.group(1)
            exposed = rename or (fn.removeprefix("py_"))
            functions.append(RsFunction(rel, j + 1, fn, exposed, module))
            i = j + 1
            continue

        # 其餘屬性後 item（非 pyo3）——跳過該行
        i = j + 1 if attrs else i + 1
    return classes, methods, functions


# ---------------------------------------------------------------------------
# Python .pyi 側解析
# ---------------------------------------------------------------------------


@dataclass
class PyClass:
    pyi_path: str
    line: int
    name: str
    is_enum: bool
    methods: dict[str, int]  # exposed name -> lineno（含 property/staticmethod）
    members: dict[str, int] = field(default_factory=dict)  # enum member（Assign）


@dataclass
class PyFunction:
    pyi_path: str
    line: int
    name: str


def pyi_module(pyi_path: str) -> str:
    """python/nautilus_trader/common/__init__.pyi → nautilus_trader.common。

    用 rindex：repo 目錄名可與 package 同名（nautilus_trader repo 內含
    python/nautilus_trader/ package）——絕對路徑下 index 會命中 repo 目錄
    （事故 1，見模組 docstring）。precondition：路徑須含 nautilus_trader
    package 段（呼叫端以 ``python/nautilus_trader/`` rglob 保證），否則
    ValueError crash。
    """
    parts = Path(pyi_path).parts
    idx = len(parts) - 1 - parts[::-1].index("nautilus_trader")
    mod_parts = list(parts[idx:-1])
    if parts[-1] != "__init__.pyi":
        mod_parts.append(Path(parts[-1]).stem)
    return ".".join(mod_parts)


def parse_pyi(path: Path, repo: Path) -> tuple[list[PyClass], list[PyFunction]]:
    rel = str(path.relative_to(repo))
    tree = ast.parse(path.read_text())
    classes: list[PyClass] = []
    functions: list[PyFunction] = []
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            is_enum = any(
                (isinstance(b, ast.Name) and b.id in ("Enum", "StrEnum", "IntEnum"))
                or (
                    isinstance(b, ast.Attribute)
                    and b.attr in ("Enum", "StrEnum", "IntEnum")
                )
                for b in node.bases
            )
            methods: dict[str, int] = {}
            members: dict[str, int] = {}
            for sub in node.body:
                if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    decorators = {
                        d.id
                        if isinstance(d, ast.Name)
                        else d.attr
                        if isinstance(d, ast.Attribute)
                        else ""
                        for d in sub.decorator_list
                    }
                    methods[sub.name] = sub.lineno
                    # 防禦 @property get_x 形態（POC 原樣保留；NT pyi 現況
                    # rg -U '@property\s+def get_' 零命中——2026-08-22 實證）
                    if "property" in decorators and sub.name.startswith("get_"):
                        methods[sub.name[4:]] = sub.lineno
                elif isinstance(sub, ast.Assign):
                    for t in sub.targets:
                        if isinstance(t, ast.Name):
                            # enum member（Rust 側 variant 不在 method 對帳範圍——
                            # 獨立桶，見 coverage enum_members）
                            members.setdefault(t.id, sub.lineno)
            classes.append(
                PyClass(rel, node.lineno, node.name, is_enum, methods, members)
            )
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            functions.append(PyFunction(rel, node.lineno, node.name))
    return classes, functions


# ---------------------------------------------------------------------------
# 對帳＋建邊
# ---------------------------------------------------------------------------


def _crate_of(rs_path: str) -> str:
    """``crates/<crate>/src/...`` → ``<crate>``；非 crates 路徑回首段。

    method→class join 的 crate 限定鍵材料（R3）：跨 crate 同名 pyclass
    （實證 ``CustomData`` 在 common＋model）下，pymethods impl 與 pyclass
    宣告同 crate（binding 檔慣例）——裸名 join 會 last-wins 錯掛。
    """
    parts = rs_path.split("/")
    return parts[1] if len(parts) > 1 and parts[0] == "crates" else parts[0]


def build_boundary(
    classes: list[RsClass],
    methods: list[RsMethod],
    functions: list[RsFunction],
    py_classes: list[tuple[str, PyClass]],
    py_functions: list[tuple[str, PyFunction]],
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    class_by_key: dict[tuple[str, str], RsClass] = {}
    classes_by_rust: dict[str, list[RsClass]] = {}
    for c in classes:
        class_by_key.setdefault((_crate_of(c.rs_path), c.rust_name), c)
        classes_by_rust.setdefault(c.rust_name, []).append(c)
    py_class_index: dict[tuple[str, str], PyClass] = {}
    for pmod, pc in py_classes:
        py_class_index.setdefault((pmod, pc.name), pc)
    py_fn_index: dict[tuple[str, str], PyFunction] = {}
    for pmod, pfn in py_functions:
        py_fn_index.setdefault((pmod, pfn.name), pfn)

    edges: list[dict[str, Any]] = []
    matched_keys: set[tuple[str, str]] = set()
    matched_class_count = 0
    rs_only_classes: list[RsClass] = []
    unresolved_module = 0

    for c in classes:
        if c.py_module is None:
            unresolved_module += 1
            rs_only_classes.append(c)
            continue
        ckey = (c.py_module, c.exposed)
        py = py_class_index.get(ckey)
        if py is not None:
            matched_keys.add((_crate_of(c.rs_path), c.rust_name))
            matched_class_count += 1
            edges.append(
                {
                    "level": "class",
                    "py_symbol": f"{c.py_module}.{c.exposed}",
                    "pyi_path": py.pyi_path,
                    "pyi_line": py.line,
                    "rs_symbol": f"{c.rust_name}",
                    "rs_path": c.rs_path,
                    "rs_line": c.line,
                    "match_kind": "NAME_MATCH"
                    if c.exposed == c.rust_name
                    else "PYCLASS_NAME_RENAME",
                }
            )
        else:
            rs_only_classes.append(c)

    rs_exposed_keys = {(x.py_module, x.exposed) for x in classes if x.py_module}
    pyi_only_classes = [
        (mod, c) for mod, c in py_classes if (mod, c.name) not in rs_exposed_keys
    ]

    # method 對帳（只在 class 兩邊都匹配時做）
    method_stats: dict[str, Any] = {
        "rs_methods_on_matched_classes": 0,
        "matched": 0,
        "rs_only": 0,
        "pyi_only": 0,
        "unresolved_class": 0,
    }
    rs_only_by_kind: dict[str, int] = {}  # Known Gaps 分類用（credential 殘差）
    dunder_rs_only = 0
    rs_method_keys: set[tuple[str, str, str]] = set()
    for m in methods:
        mkey0 = (_crate_of(m.rs_path), m.rust_class)
        rc = class_by_key.get(mkey0)
        if rc is None:
            # crate 缺席 fallback：裸名全 corpus 唯一才用（歧義跳過＋計數）
            candidates = classes_by_rust.get(m.rust_class, [])
            if len(candidates) == 1:
                rc = candidates[0]
                mkey0 = (_crate_of(rc.rs_path), rc.rust_name)
        if rc is None:
            method_stats["unresolved_class"] += 1
            continue
        if mkey0 not in matched_keys:
            continue
        assert rc.py_module is not None  # matched class 必有 module（匹配鍵）
        method_stats["rs_methods_on_matched_classes"] += 1
        py = py_class_index.get((rc.py_module, rc.exposed))
        assert py is not None
        # #[new] 在 pyi 有 __init__ 與 __new__ 兩種形態（pyo3-stub-gen 版本
        # 行為差異；實證：architect_ax config 類 __init__、risk sizer __new__）
        exposed = m.exposed
        target = py.members if m.kind == "variant" else py.methods
        if m.kind == "new":
            exposed = next(
                (n for n in ("__init__", "__new__") if n in py.methods), m.exposed
            )
        mkey = (rc.py_module, rc.exposed, exposed)
        rs_method_keys.add(mkey)
        if exposed in target:
            method_stats["matched"] += 1
            if m.renamed:
                kind_str = "PYO3_NAME_RENAME"
            elif m.kind in ("getter", "setter"):
                kind_str = "GETTER_PROPERTY"
            elif m.kind == "field_property":
                kind_str = "FIELD_PROPERTY"
            elif m.kind == "variant":
                kind_str = "ENUM_VARIANT"
            else:
                kind_str = "NAME_MATCH"
            edges.append(
                {
                    "level": "method",
                    "py_symbol": f"{rc.py_module}.{rc.exposed}.{exposed}",
                    "pyi_path": py.pyi_path,
                    "pyi_line": target[exposed],
                    "rs_symbol": f"{m.rust_class}::{m.rust_fn}",
                    "rs_path": m.rs_path,
                    "rs_line": m.line,
                    "match_kind": kind_str,
                    "method_kind": m.kind,
                }
            )
        elif m.kind == "dunder":
            dunder_rs_only += 1  # dunder 另計，不入 rs_only
        else:
            method_stats["rs_only"] += 1
            rs_only_by_kind[m.kind] = rs_only_by_kind.get(m.kind, 0) + 1
    method_stats["rs_only_by_kind"] = rs_only_by_kind
    method_stats["dunder_rs_only"] = dunder_rs_only
    # pyi_only methods（matched classes 上）
    for pmod, pc in py_classes:
        if (pmod, pc.name) not in rs_exposed_keys:
            continue
        for name in pc.methods:
            if (pmod, pc.name, name) not in rs_method_keys and not name.startswith(
                "__"
            ):
                method_stats["pyi_only"] += 1

    # function 對帳
    fn_stats: dict[str, int] = {
        "rs_functions": len(functions),
        "matched": 0,
        "rs_only": 0,
        "pyi_only": 0,
    }
    for f in functions:
        if f.py_module is None:
            unresolved_module += 1
            continue
        pf = py_fn_index.get((f.py_module, f.exposed))
        if pf is not None:
            fn_stats["matched"] += 1
            edges.append(
                {
                    "level": "function",
                    "py_symbol": f"{f.py_module}.{f.exposed}",
                    "pyi_path": pf.pyi_path,
                    "pyi_line": pf.line,
                    "rs_symbol": f.rust_fn,
                    "rs_path": f.rs_path,
                    "rs_line": f.line,
                    "match_kind": "PYO3_NAME_RENAME"
                    if f.exposed != f.rust_fn
                    else "NAME_MATCH",
                }
            )
        else:
            fn_stats["rs_only"] += 1
    rs_fn_keys = {(f.py_module, f.exposed) for f in functions if f.py_module}
    for pmod, pfn in py_functions:
        if (pmod, pfn.name) not in rs_fn_keys:
            fn_stats["pyi_only"] += 1

    coverage = {
        "classes": {
            "rs_pyclass_total": len(classes),
            "matched": matched_class_count,
            "rs_only": len(rs_only_classes),
            "pyi_total": len(py_classes),
            "pyi_only": len(pyi_only_classes),
        },
        "methods": method_stats,
        "functions": fn_stats,
        "unresolved_module": unresolved_module,
    }
    return edges, coverage


# ---------------------------------------------------------------------------
# sidecar（commit 錨定、冪等）
# ---------------------------------------------------------------------------


def nt_head_sha(nt_repo: Path) -> str:
    """NT repo 當前 HEAD——sidecar 檔名錨定＋S2 stale 比對源。"""
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=nt_repo,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def coverage_summary(coverage: dict[str, Any]) -> dict[str, Any]:
    """三層覆蓋率摘要（_meta/ stdout 共用）。"""
    c, m, f = coverage["classes"], coverage["methods"], coverage["functions"]

    def pct(matched: int, total: int) -> float:
        return round(matched / total * 100, 1) if total else 0.0

    return {
        "class_matched": c["matched"],
        "class_total": c["rs_pyclass_total"],
        "class_pct": pct(c["matched"], c["rs_pyclass_total"]),
        "method_matched": m["matched"],
        "method_total": m["rs_methods_on_matched_classes"],
        "method_pct": pct(m["matched"], m["rs_methods_on_matched_classes"]),
        "function_matched": f["matched"],
        "function_total": f["rs_functions"],
        "function_pct": pct(f["matched"], f["rs_functions"]),
    }


def known_gaps_of(coverage: dict[str, Any]) -> dict[str, int]:
    """殘差 Known Gaps 分類計數（機械可歸因的桶——見模組 docstring）。"""
    by_kind = coverage["methods"]["rs_only_by_kind"]
    return {
        # custom_data! 巨集內宣告掃不到 → pyi-only class（全歸因是估值）
        "pyi_only_class_custom_data_macro_est": coverage["classes"]["pyi_only"],
        # 有 pyclass 無 stub 面（Ax* enums、examples/）
        "rs_only_class_declared_not_stubbed": coverage["classes"]["rs_only"],
        # credential 欄位選擇性省略 → field_property rs-only 殘差大宗
        "rs_only_method_field_property": by_kind.get("field_property", 0),
        # pyi 空白 stub class（無任何 property——bybit http models）
        "rs_only_method_getter_empty_stub_est": by_kind.get("getter", 0),
        # variant 命名轉換後仍不匹配的殘餘（R1 修復後 ~96）
        "rs_only_method_variant_residual": by_kind.get("variant", 0),
    }


def write_sidecar(
    nt_repo: Path,
    nt_commit: str,
    edges: list[dict[str, Any]],
    coverage: dict[str, Any],
    out_dir: Path = DEFAULT_OUT_DIR,
) -> Path:
    """寫 sidecar DB（``<nt-short-sha>.db`` 冪等覆寫）＋meta 表。

    不併入 CRG graph.db——rebuild 會清外掛表（gap-prototypes §1.7）。
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    db = out_dir / f"{nt_commit[:8]}.db"
    if db.exists():
        db.unlink()
    meta = make_meta(TOOL, nt_repo, commit=nt_commit)
    # nt_commit 為唯一權威鍵（S2 消費端語義）；make_meta 泛用 commit 鍵同值
    # 冗餘，不入表（build review R10）
    del meta["commit"]
    meta.update(
        {
            "nt_commit": nt_commit,
            "edges_count": str(len(edges)),
            "coverage_summary": json.dumps(coverage_summary(coverage)),
            "known_gaps": json.dumps(known_gaps_of(coverage)),
        }
    )
    conn = sqlite3.connect(db)
    conn.executescript(
        """
        CREATE TABLE boundary_edges (
            id INTEGER PRIMARY KEY,
            level TEXT,            -- class | method | function
            py_symbol TEXT,        -- nautilus_trader.live.LiveNode.build
            pyi_path TEXT, pyi_line INTEGER,
            rs_symbol TEXT,        -- LiveNode::py_build
            rs_path TEXT, rs_line INTEGER,
            match_kind TEXT,       -- NAME_MATCH | PYCLASS_NAME_RENAME | PYO3_NAME_RENAME | GETTER_PROPERTY
            method_kind TEXT
        );
        CREATE INDEX idx_edges_py ON boundary_edges(py_symbol);
        CREATE INDEX idx_edges_rs ON boundary_edges(rs_path, rs_line);
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
        """
    )
    conn.executemany(
        "INSERT INTO boundary_edges (level,py_symbol,pyi_path,pyi_line,rs_symbol,"
        "rs_path,rs_line,match_kind,method_kind) VALUES (?,?,?,?,?,?,?,?,?)",
        [
            (
                e["level"],
                e["py_symbol"],
                e["pyi_path"],
                e["pyi_line"],
                e["rs_symbol"],
                e["rs_path"],
                e["rs_line"],
                e["match_kind"],
                e.get("method_kind"),
            )
            for e in edges
        ],
    )
    conn.executemany(
        "INSERT INTO meta (key, value) VALUES (?, ?)",
        [(k, str(v)) for k, v in meta.items()],
    )
    conn.commit()
    conn.close()
    return db


def _glob_base(glob: str) -> Path:
    """glob 的 literal 前綴目錄（第一個含萬用字元的段之前）——存在性驗證用。"""
    parts = []
    for seg in Path(glob).parts:
        if any(ch in seg for ch in "*?["):
            break
        parts.append(seg)
    return Path(*parts) if parts else Path()


def _assert_repo(repo: Path, roots: tuple[ScanRoot, ...]) -> None:
    """crash-only 驗證掃描前提：repo 存在＋各 scan_root 的 base 目錄存在。

    掃描根由 profile ``[[scan_root]]`` 擁有（NT 深層形狀——pyo3 宣告＋
    .pyi 合約樹——是 NT 專案假設，記錄於 code-reality skill）。已知邊界：
    多個 scan_root 的 glob 重疊時不去重（重掃＋重複邊）——profile 應
    保持 scan_root 互斥（自曝慣例，對齊 claims_re 的重疊 prefix 警示）。"""
    assert repo.is_dir(), f"repo 不存在：{repo}"
    for sr in roots:
        for glob in (sr.path, sr.pyi):
            base = repo / _glob_base(glob)
            assert base.is_dir(), (
                f"scan_root base 不存在：{base}（profile glob {glob!r}）"
                "——檢查 .code-reality.toml [[scan_root]] 與 --repo 是否同 repo"
            )


def build_sidecar(repo: Path, out_dir: Path = DEFAULT_OUT_DIR) -> Path:
    """完整流程：驗證 repo → 掃描 → 對帳 → sidecar。回傳 DB 路徑。"""
    repo = repo.resolve()
    roots = scan_roots(load_profile(repo))
    assert roots, (
        f"{repo} 無 boundary 掃描根——需 repo profile（.code-reality.toml）定義 "
        "[[scan_root]]（path=rs glob、pyi=pyi glob）＋顯式 --repo（SM-1b："
        "不內建任何 repo 預設）"
    )
    _assert_repo(repo, roots)
    sha = nt_head_sha(repo)

    classes: list[RsClass] = []
    methods: list[RsMethod] = []
    functions: list[RsFunction] = []
    rs_files = sorted(p for sr in roots for p in repo.glob(sr.path))
    for p in rs_files:
        c, m, f = scan_rust_file(p, repo)
        classes.extend(c)
        methods.extend(m)
        functions.extend(f)

    pyi_files = sorted(p for sr in roots for p in repo.glob(sr.pyi))
    py_classes: list[tuple[str, PyClass]] = []
    py_functions: list[tuple[str, PyFunction]] = []
    for p in pyi_files:
        mod = pyi_module(str(p))
        pcs, pfs = parse_pyi(p, repo)
        py_classes.extend((mod, c) for c in pcs)
        py_functions.extend((mod, f) for f in pfs)

    edges, coverage = build_boundary(
        classes, methods, functions, py_classes, py_functions
    )
    return write_sidecar(repo, sha, edges, coverage, out_dir=out_dir)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="PyO3 boundary extractor（sidecar build）"
    )
    parser.add_argument(
        "--repo",
        type=Path,
        required=True,
        help="掃描目標 repo 根（唯讀；需該 repo 的 .code-reality.toml 定義 [[scan_root]]）",
    )
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    args = parser.parse_args()

    db = build_sidecar(args.repo, out_dir=args.out_dir)
    conn = connect_ro(db)
    meta = dict(conn.execute("SELECT key, value FROM meta"))
    conn.close()
    summary = json.loads(meta["coverage_summary"])
    gaps = json.loads(meta["known_gaps"])
    print(
        f"[OK] boundary sidecar: {meta['edges_count']} edges -> {db}"
        f"（NT {meta['nt_commit'][:8]}）"
    )
    print(
        f"  class: {summary['class_matched']}/{summary['class_total']}"
        f"（{summary['class_pct']}%）｜method: {summary['method_matched']}/"
        f"{summary['method_total']}（{summary['method_pct']}%）｜function: "
        f"{summary['function_matched']}/{summary['function_total']}"
        f"（{summary['function_pct']}%）"
    )
    print(
        f"  known gaps: pyi-only class {gaps['pyi_only_class_custom_data_macro_est']}"
        f"（custom_data! 巨集估）、rs-only class {gaps['rs_only_class_declared_not_stubbed']}"
        f"（declared-not-stubbed）、field_property {gaps['rs_only_method_field_property']}"
        f"（credential 省略）、getter {gaps['rs_only_method_getter_empty_stub_est']}"
        f"（空白 stub 估）、variant {gaps['rs_only_method_variant_residual']}（轉換殘餘）"
    )
    print(
        f"[LOG] 查詢：uv run --project ~/Github/ai-rules "
        f"python -m code_reality.boundary <symbol> --repo <repo>"
        f"｜裸 sqlite：sqlite3 {db} 'SELECT * FROM boundary_edges WHERE py_symbol LIKE \"%LiveNode%\"'"
    )


if __name__ == "__main__":
    main()
