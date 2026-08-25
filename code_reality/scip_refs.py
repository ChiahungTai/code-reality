"""scip_refs——rust-analyzer SCIP 索引查詢端（Rust refs/callers 真相源 sidecar）。

定位：CRG graph.db 對「同型別多 impl 同名方法」有同鍵去重漏收（見
graph_audit——NT 實測 861 顆），受影響符號的 callers 查詢回**假空**。
本工具改查 rust-analyzer 生成的 SCIP 索引（type-aware：inherent 與
trait impl 各自獨立 symbol——``impl#[Type]method().`` vs
``impl#[Type][Trait]method().``——正是 CRG 缺的消歧），作為這些符號的
def/refs 真相源。

SCIP 符號形態（查詢匹配涵蓋兩種）：
  impl 方法：``<mod>/impl#[Type]method().``、``<mod>/impl#[Type][Trait]method().``
  trait 宣告位址：``<mod>/<Trait>#method().``（型別以 ``#`` 為後綴——經
  dyn/泛界呼叫的引用解析到這顆，漏它＝低報 refs）

索引生成（~8 分鐘牆鐘／~270MB／rebase 後重生；輸出寫在 **cwd** 非
stdout）＋生成後 stamp 版本 sidecar（⑤標註資料面）::

    mkdir -p ~/.mosaic/code-reality/scip/<repo-basename> \\
        && cd ~/.mosaic/code-reality/scip/<repo-basename> \\
        && rust-analyzer scip <repo-root>
    uv run --project ~/Github/ai-rules python -m code_reality.scip_refs \\
        --stamp-meta --repo <repo-root>

索引慣例（repo-keyed slot）：有 ``--repo`` 時 ``--index`` 可省略——解析
``~/.mosaic/code-reality/scip/<repo-basename>/index.scip``（多 repo 共用
全局單一檔會互蓋）；顯式 ``--index`` 永遠優先；query 模式無 ``--repo``
仍需顯式 ``--index``。**既有全局 slot 索引搬遷**（免 8 分鐘重生成）：
``mkdir -p ~/.mosaic/code-reality/scip/<repo-basename> && mv
~/.mosaic/code-reality/scip/index.scip <dir>/``。

scip_pb2.py 重生（schema 變更時；grpcio-tools 內含 protoc；scip.proto
已 vendored 同目錄）::

    cd code_reality && uv run --with grpcio-tools python -m grpc_tools.protoc \\
        --proto_path=. --python_out=. scip.proto

用法::

    uv run --project ~/Github/ai-rules python -m code_reality.scip_refs \\
        EventStoreLifecycle.open --index <anywhere>/index.scip  # 顯式覆蓋
    uv run --project ~/Github/ai-rules python -m code_reality.scip_refs \\
        EventStoreLifecycle.open --repo <repo-root>     # 預設 slot
    uv run --project ~/Github/ai-rules python -m code_reality.scip_refs \\
        --audit --repo <repo-root>                       # 預設 slot
    uv run --project ~/Github/ai-rules python -m code_reality.scip_refs \\
        --build-cache --repo <repo-root>    # 衍生 sqlite 查詢面（一次構建）

source 標註（facade 契約「每回應附 source 與 commit 版本」）：stamp 過
sidecar 或給 ``--repo`` 的回應，輸出首行帶 ``[SRC] scip index @ <sha>``
（· ``repo HEAD @ <sha>``）；sidecar 與 HEAD 不一致 → WARN（漂移守衛——
A3 graph.db 過時事件同型防線）。顯式 ``--index`` 無 sidecar 無 ``--repo``
→ 無 [SRC] 行，legacy 輸出位元組不變（NT 查詢契約）。

衍生 sqlite 查詢面（``--build-cache`` 落 ``<index>.scip.db``；建議時序
＝生成索引 → ``--stamp-meta`` → ``--build-cache``）：occurrences 表只收
函數形態符號（``FN_TAIL_RE`` 命中者——查詢/audit 的消費集恆為該子集，
非函數符號的 occurrences 不入庫）。查詢優先走 db；匹配語義單一真相源
仍在本模組（``_matcher``/``FN_TAIL_RE``）——SQL 只做 ``method=?`` 候選
縮小、Python 複檢。無 db → protobuf 全量解析（~40s/次）路徑不變；過期
（db 比索引檔舊、或 sidecar head 變動）→ WARN＋自動重建——管理訊息走
stderr，查詢 stdout 兩路徑**位元組相同**（衍生面不該改變答案）。

退出碼：0=有結果｜1=查無｜2=環境錯誤（索引不在/損壞/protobuf 未裝/
graph_audit 子進程失敗/stamp 取不到 HEAD/衍生 db 構建失敗）。
"""

import argparse
import json
import os
import re
import sqlite3
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

try:
    from code_reality import scip_pb2  # vendored gencode（需 protobuf runtime）
except ImportError:
    scip_pb2 = None  # type: ignore[assignment]

FN_TAIL_RE = re.compile(r"(?<!\w)(\w+)\(\)\.$")

# repo-keyed slot 慣例（boundary sidecar 同族）：多 repo 共用全局單一
# index.scip 會互蓋——basename 為鍵（同名異路徑 repo 需顯式 --index）。
DEFAULT_INDEX_ROOT = Path.home() / ".mosaic" / "code-reality" / "scip"
META_SUFFIX = ".meta.json"
DB_SUFFIX = ".db"
SCHEMA_VERSION = "1"  # 结构變更時遞增——舊 schema db 視同過期重建


def load_index(path: Path):
    """解析索引；損壞/截斷與空索引走環境錯誤（exit 2），不裸 traceback。

    protobuf 無完整性校驗——截斷恰落在 field 邊界時會靜默解析得部分
    結果（假「查無」），故加文件數下限作最小健全性檢查。
    """
    index = scip_pb2.Index()
    try:
        with open(path, "rb") as f:
            index.ParseFromString(f.read())
    except Exception as e:  # DecodeError 等
        print(f"[FAIL] 索引解析失敗（損壞/截斷？）：{e}", file=sys.stderr)
        sys.exit(2)
    if len(index.documents) == 0:
        print("[FAIL] 索引 0 文檔——空或損壞", file=sys.stderr)
        sys.exit(2)
    if len(index.documents) < 100:
        print(
            f"[WARN] 索引僅 {len(index.documents)} 文檔——可能截斷，結果存疑",
            file=sys.stderr,
        )
    return index


def ln(occ) -> int:
    r = occ.range
    return r[0] + 1 if len(r) >= 2 else -1


def loc_line(rel_path: str, line: int) -> str:
    return f"{rel_path}:?" if line <= 0 else f"{rel_path}:{line}"


def loc(doc_path: str, occ) -> str:
    return loc_line(doc_path, ln(occ))


def tail(symbol: str) -> str:
    """`rust-analyzer cargo <crate> <ver> <mod>/descriptor` → descriptor 部分。"""
    parts = symbol.split(" ")
    return parts[-1] if len(parts) > 4 else symbol


def _matcher(query: str):
    """查詢 → symbol 匹配閉包。

    ``Type.method``：匹配 impl 變體（marker ``[Type]``）**與** trait 宣告
    位址（``Type#method`` 形態——漏它會低報 refs）；裸 ``name``：任何
    邊界正確的 ``name().`` 結尾（``(?<!\\w)`` 擋掉 my_open/reopen 誤配）。
    """
    if "." in query:
        type_name, method = query.rsplit(".", 1)
        name_pat = re.compile(r"(?<!\w)" + re.escape(method) + r"\(\)\.$")
        marker = f"[{type_name}]"
        trait_decl = re.compile(r"(?<![\w#])" + re.escape(type_name) + r"#")

        def match(s: str) -> bool:
            return bool(name_pat.search(s)) and (
                marker in s or bool(trait_decl.search(s))
            )

    else:
        name_pat = re.compile(r"(?<!\w)" + re.escape(query) + r"\(\)\.$")

        def match(s: str) -> bool:
            return bool(name_pat.search(s))

    return match


def find_defs(index, query: str) -> dict[str, list[str]]:
    """回 {symbol → [file:line...]}——DEF occurrences 中符合查詢者。"""
    match = _matcher(query)
    defs: dict[str, list[str]] = {}
    for d in index.documents:
        for occ in d.occurrences:
            if occ.symbol_roles & 1 and match(occ.symbol):
                defs.setdefault(occ.symbol, []).append(loc(d.relative_path, occ))
    return defs


def find_refs(index, symbols: set[str]) -> dict[str, list[str]]:
    refs: dict[str, list[str]] = {s: [] for s in symbols}
    for d in index.documents:
        for occ in d.occurrences:
            if occ.symbol in refs and not (occ.symbol_roles & 1):
                refs[occ.symbol].append(loc(d.relative_path, occ))
    return refs


def report(face, query: str, src_line: str | None = None) -> int:
    if src_line:
        print(src_line)
    defs = face.defs(query)
    if not defs:
        print(f"[WARN] 查無 DEF：{query}")
        return 1
    refs = face.refs(set(defs))
    for symbol in sorted(defs):
        d_list, r_list = defs[symbol], refs[symbol]
        print(f"[OK] {tail(symbol)}")
        for loc_str in d_list:
            print(f"  DEF  {loc_str}")
        print(f"  refs: {len(r_list)} 處（跨檔）")
        for r in r_list[:6]:
            print(f"    {r}")
        if len(r_list) > 6:
            print(f"    ...共 {len(r_list)} 處")
    return 0


def audit_targets(
    documents, files_by_name: dict[str, set[str]]
) -> dict[str, tuple[str, str]]:
    """DEF occurrences → {symbol → (定義檔, 方法名)}。

    歸屬按 **(定義檔, 方法名) 雙鍵**——只按檔過濾會把同檔鄰居的 refs
    聯集進來（獨立審查實證：216→138，78 項假陽性）。
    """
    target_symbols: dict[str, tuple[str, str]] = {}
    for d in documents:
        for occ in d.occurrences:
            if not (occ.symbol_roles & 1):
                continue
            m = FN_TAIL_RE.search(occ.symbol)
            if not m:
                continue
            name = m.group(1)
            if name in files_by_name and d.relative_path in files_by_name[name]:
                target_symbols[occ.symbol] = (d.relative_path, name)
    return target_symbols


def missing_refs(
    missing: dict[str, object],
    target_symbols: dict[str, tuple[str, str]],
    refs_count: dict[str, list[str]],
) -> list[str]:
    """單一 graph_audit 缺差項 → 對應 SCIP refs（雙鍵歸屬過濾）。"""
    rel = missing["_rel"]
    return [
        r
        for sym, (d_file, d_name) in target_symbols.items()
        if d_file == rel and d_name == missing["symbol"]
        for r in refs_count[sym]
    ]


def _repo_rel(file_str: str, repo: Path) -> str:
    p = Path(file_str)
    try:
        return str(p.relative_to(repo))
    except ValueError:
        # repo 外路徑與 SCIP relative_path 恆不匹配（該項 refs 報 0）——
        # loud 標記防「假 0 callers」靜默混入
        print(
            f"[WARN] 缺差項路徑不在 repo 下（歸屬失敗，refs 將報 0）：{p}",
            file=sys.stderr,
        )
        return p.as_posix()


def audit_mode(index_path: Path, repo: Path, src_line: str | None = None) -> int:
    """graph_audit 缺差清單 → 逐符號 SCIP refs——「假 0 callers」的直接解法。

    兩遍式（861 項逐項全掃＝小時級）；graph_audit 經 subprocess
    ``sys.executable -m``（env 檢查＋exit code 契約全在其 main() 重用）。
    ``repo.resolve()`` 正規化——graph_audit 的 file 鍵是 resolved 絕對路徑，
    非 canonical ``--repo``（symlink 別名）會讓每項歸屬失敗（refs 恆 0）。
    """
    repo = repo.resolve()
    try:
        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "code_reality.graph_audit",
                "--repo",
                str(repo),
                "--json",
            ],
            capture_output=True,
            text=True,
            timeout=600,
            check=False,  # 退出碼 0/1/2 本身是契約——returncode 由呼叫端判讀
        )
    except subprocess.TimeoutExpired:
        print("[FAIL] graph_audit 逾時", file=sys.stderr)
        return 2
    if proc.returncode == 2:
        print(f"[FAIL] graph_audit 環境錯誤：{proc.stderr.strip()}", file=sys.stderr)
        return 2
    if proc.returncode not in (0, 1) or not proc.stdout.strip():
        print(f"[FAIL] graph_audit 異常退出 {proc.returncode}", file=sys.stderr)
        return 2
    try:
        missing = json.loads(proc.stdout)["missing"]
    except (json.JSONDecodeError, KeyError) as e:
        print(f"[FAIL] graph_audit 輸出異常：{e}", file=sys.stderr)
        return 2

    if src_line:
        print(src_line)
    print(f"[OK] graph_audit 缺差 {len(missing)} 項 → 逐項 SCIP refs 對照：")
    face = open_face(index_path)

    files_by_name: dict[str, set[str]] = {}  # name → {rel path}
    for m in missing:
        m["_rel"] = _repo_rel(str(m["file"]), repo)
        files_by_name.setdefault(m["symbol"], set()).add(m["_rel"])

    target_symbols = face.audit_targets(files_by_name)
    refs_count = face.refs(set(target_symbols))

    with_refs = 0
    for m in missing:
        r_list = missing_refs(m, target_symbols, refs_count)
        if r_list:
            with_refs += 1
        print(
            f"  {m['_rel']}: {m['symbol']}({m['db_count']}/{m['ra_count']})"
            f" → SCIP refs {len(r_list)}"
        )
    print(f"[OK] {with_refs}/{len(missing)} 項在 SCIP 有 refs（非零 callers）")
    return 0


SCHEMA_SQL = """
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE occurrences (
    -- seq 顯式 PK：VACUUM 不重編（隱式 rowid 會）——插入序＝掃描序，
    -- 兩路徑輸出位元組等價的次序基礎；人工 VACUUM 壓縮後仍成立
    seq      INTEGER PRIMARY KEY,
    symbol   TEXT    NOT NULL,
    rel_path TEXT    NOT NULL,
    line     INTEGER NOT NULL,
    is_def   INTEGER NOT NULL
);
CREATE TABLE symbol_tails (
    -- tail＝預計算 descriptor（供人工 sqlite 探查；程式端由 tail() 現算）
    symbol TEXT PRIMARY KEY,
    tail   TEXT NOT NULL,
    method TEXT NOT NULL
);
CREATE INDEX idx_symbol_tails_method ON symbol_tails(method);
CREATE INDEX idx_occurrences_symbol ON occurrences(symbol, is_def);
"""


def sqlite_path(index_path: Path) -> Path:
    return index_path.parent / (index_path.name + DB_SUFFIX)


def _sidecar_head(index_path: Path) -> str:
    meta = load_meta(index_path)
    return str(meta.get("head") or "") if meta else ""


def _build_db(index, db_path: Path, sidecar_head: str) -> dict[str, int]:
    """核心構建——單一交易寫入後原子換入（暫存檔＋``os.replace``）。

    occurrences 只收 ``FN_TAIL_RE`` 符號：查詢/audit 消費集（defs 匹配、
    refs 收集、雙鍵歸屬）恆為函數形態符號，非函數符號入庫只膨脹不會被
    讀。meta 記構建時 sidecar head——過期判定的第二訊號（重 stamp＝索引
    重生蹤跡）。訊息路由在呼叫端：CLI 模式走 stdout、查詢內自動重建走
    stderr（查詢 stdout 位元組不變的硬約束）。
    """
    tails: dict[str, tuple[str, str]] = {}
    for d in index.documents:
        for occ in d.occurrences:
            m = FN_TAIL_RE.search(occ.symbol)
            if m:
                tails[occ.symbol] = (tail(occ.symbol), m.group(1))
    stats = {"symbols": len(tails), "occurrences": 0}

    def occ_rows():
        for d in index.documents:
            for occ in d.occurrences:
                if occ.symbol in tails:
                    stats["occurrences"] += 1
                    yield (
                        occ.symbol,
                        d.relative_path,
                        ln(occ),
                        1 if occ.symbol_roles & 1 else 0,
                    )

    tmp = db_path.with_name(db_path.name + ".tmp")
    tmp.unlink(missing_ok=True)  # 前次崩潰殘檔會讓 CREATE TABLE 失敗
    conn = sqlite3.connect(tmp)
    try:
        conn.executescript(SCHEMA_SQL)
        conn.executemany(
            "INSERT INTO symbol_tails (symbol, tail, method) VALUES (?, ?, ?)",
            ((s, t, m) for s, (t, m) in tails.items()),
        )
        conn.executemany(
            "INSERT INTO occurrences (symbol, rel_path, line, is_def)"
            " VALUES (?, ?, ?, ?)",
            occ_rows(),
        )
        conn.executemany(
            "INSERT INTO meta (key, value) VALUES (?, ?)",
            (
                ("head", sidecar_head),
                ("schema", SCHEMA_VERSION),
                ("tool", "code_reality.scip_refs"),
            ),
        )
        conn.commit()
    finally:
        conn.close()
    os.replace(tmp, db_path)
    return stats


def build_cache_mode(index_path: Path) -> int:
    db_path = sqlite_path(index_path)
    try:
        stats = _build_db(load_index(index_path), db_path, _sidecar_head(index_path))
    except (OSError, sqlite3.Error) as e:
        print(f"[FAIL] 衍生 db 構建失敗：{db_path}：{e}", file=sys.stderr)
        return 2
    print(
        f"[OK] cache built：{db_path}"
        f"（{stats['symbols']} symbols/{stats['occurrences']} occurrences）"
    )
    return 0


def _stale_reason(index_path: Path, db_path: Path) -> str | None:
    """過期雙訊號＋schema 守衛——db mtime＜index mtime、sidecar head 與
    構建時不同、或 meta 的 schema 版本不符。

    db 損壞（非 sqlite 檔/meta 表缺/舊 schema）視同過期：重建即治，不必
    讓查詢端處理半殘 db——「valid sqlite 但形狀不對」若放行會到查詢時才
    crash。
    """
    try:
        if db_path.stat().st_mtime < index_path.stat().st_mtime:
            return "db 比索引檔舊"
    except OSError as e:
        return f"stat 失敗：{e}"
    try:
        conn = sqlite3.connect(f"{db_path.resolve().as_uri()}?mode=ro", uri=True)
        try:
            meta_rows = dict(conn.execute("SELECT key, value FROM meta").fetchall())
        finally:
            conn.close()
    except sqlite3.Error as e:
        return f"db 損壞：{e}"
    if meta_rows.get("schema") != SCHEMA_VERSION:
        got = meta_rows.get("schema", "無")
        return f"schema 版本不符（{got} ≠ {SCHEMA_VERSION}）"
    db_head = meta_rows.get("head", "")
    if db_head != _sidecar_head(index_path):
        return "sidecar head 變動（索引重生後重 stamp？）"
    return None


def _open_ro(db_path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"{db_path.resolve().as_uri()}?mode=ro", uri=True)


def open_face(index_path: Path):
    """查詢面解析——fresh db → SqliteFace；無/過期 db → 重建或 protobuf。

    自動重建失敗（磁碟等 OSError/sqlite 錯誤）回 protobuf 全量解析：
    衍生面是加速器不是依賴，壞了不該擋服務。索引本身損壞時
    ``load_index`` 的 exit 2 照常傳播（protobuf 路徑也會走到同一結局）。
    """
    db_path = sqlite_path(index_path)
    if not db_path.exists():
        return ProtobufFace(load_index(index_path))
    reason = _stale_reason(index_path, db_path)
    if reason is None:
        return SqliteFace(_open_ro(db_path))
    print(f"[WARN] 衍生 db 過期（{reason}）——自動重建", file=sys.stderr)
    try:
        index = load_index(index_path)  # 解析一次留存——build 失敗直接餵 protobuf
        _build_db(index, db_path, _sidecar_head(index_path))
    except (OSError, sqlite3.Error) as e:
        print(
            f"[WARN] 衍生 db 重建失敗——本次查詢改走 protobuf 全量解析：{e}",
            file=sys.stderr,
        )
        return ProtobufFace(index)
    print("[OK] 衍生 db 重建完成", file=sys.stderr)
    return SqliteFace(_open_ro(db_path))


class ProtobufFace:
    """protobuf 索引查詢面——無 db 時的原路徑（委託既有掃描函數）。"""

    def __init__(self, index):
        self.index = index

    def defs(self, query: str) -> dict[str, list[str]]:
        return find_defs(self.index, query)

    def refs(self, symbols: set[str]) -> dict[str, list[str]]:
        return find_refs(self.index, symbols)

    def audit_targets(
        self, files_by_name: dict[str, set[str]]
    ) -> dict[str, tuple[str, str]]:
        return audit_targets(self.index.documents, files_by_name)


class SqliteFace:
    """衍生 sqlite 查詢面——SQL 只做 ``method=?`` 候選縮小。

    語義複檢全在本模組既有原語（``_matcher``/``FN_TAIL_RE``＋歸屬過濾）
    ——SQL 若長出自己的匹配語義就是第二真相源，drift 即靜默錯答。
    ``ORDER BY seq`` 釘住插入序＝protobuf 文檔/occurrence 序（顯式 PK
    ——VACUUM 不重編；輸出位元組等價的次序保證）。
    """

    def __init__(self, conn: sqlite3.Connection):
        self.conn = conn

    def defs(self, query: str) -> dict[str, list[str]]:
        match = _matcher(query)
        method = query.rsplit(".", 1)[1] if "." in query else query
        if re.fullmatch(r"\w+", method):
            candidates = self.conn.execute(
                "SELECT symbol FROM symbol_tails WHERE method = ?", (method,)
            ).fetchall()
        else:
            # 非 identifier 查詢（含 '-' 等）：method=? 鍵對不上 FN_TAIL_RE
            # 的 \w+ 捕獲，縮小不再保證超集——退全候選，Python 複檢把關
            candidates = self.conn.execute("SELECT symbol FROM symbol_tails").fetchall()
        defs: dict[str, list[str]] = {}
        for (symbol,) in candidates:
            if not match(symbol):
                continue
            rows = self.conn.execute(
                "SELECT rel_path, line FROM occurrences"
                " WHERE symbol = ? AND is_def = 1 ORDER BY seq",
                (symbol,),
            ).fetchall()
            if rows:  # 無 DEF 的 ref-only 符號不入 defs（protobuf 同律）
                defs[symbol] = [loc_line(rel_path, line) for rel_path, line in rows]
        return defs

    def refs(self, symbols: set[str]) -> dict[str, list[str]]:
        out: dict[str, list[str]] = {s: [] for s in symbols}
        for symbol in symbols:
            out[symbol] = [
                loc_line(rel_path, line)
                for rel_path, line in self.conn.execute(
                    "SELECT rel_path, line FROM occurrences"
                    " WHERE symbol = ? AND is_def = 0 ORDER BY seq",
                    (symbol,),
                )
            ]
        return out

    def audit_targets(
        self, files_by_name: dict[str, set[str]]
    ) -> dict[str, tuple[str, str]]:
        names = list(files_by_name)
        if not names:
            return {}
        ph = ",".join("?" * len(names))
        rows = self.conn.execute(
            "SELECT symbol, rel_path FROM occurrences"
            f" WHERE is_def = 1 AND symbol IN"
            f" (SELECT symbol FROM symbol_tails WHERE method IN ({ph}))"
            " ORDER BY seq",
            names,
        ).fetchall()
        target_symbols: dict[str, tuple[str, str]] = {}
        for symbol, rel_path in rows:
            m = FN_TAIL_RE.search(symbol)  # 複檢——meta 表資料不替代語義源
            if not m:
                continue
            name = m.group(1)
            if name in files_by_name and rel_path in files_by_name[name]:
                target_symbols[symbol] = (rel_path, name)
        return target_symbols


def default_index_path(repo: Path) -> Path:
    """repo-keyed slot：``DEFAULT_INDEX_ROOT/<repo-basename>/index.scip``。

    ``resolve()`` 先行——相對 ``--repo .`` 取 cwd basename（不 resolve 的
    ``Path('.').name`` 是空字串，``/`` 空段會靜默塌縮回全局單檔＝①要防
    的互蓋）。
    """
    name = repo.resolve().name
    if not name:
        print(
            f"[FAIL] --repo {repo} 解析不出 repo 名——請給絕對路徑",
            file=sys.stderr,
        )
        sys.exit(2)
    return DEFAULT_INDEX_ROOT / name / "index.scip"


def meta_path(index_path: Path) -> Path:
    return index_path.parent / (index_path.name + META_SUFFIX)


def load_meta(index_path: Path) -> dict | None:
    p = meta_path(index_path)
    if not p.exists():
        return None
    try:
        meta = json.loads(p.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        print(f"[WARN] index meta 損壞（[SRC] 缺 index 版本）：{e}", file=sys.stderr)
        return None
    if not isinstance(meta, dict) or not isinstance(meta.get("head"), str):
        print("[WARN] index meta 形狀非預期（[SRC] 缺 index 版本）", file=sys.stderr)
        return None
    return meta


def _git_head(repo: Path) -> str | None:
    """repo live HEAD——取不到回 None＋WARN（標註輔助，不致命）。"""
    try:
        proc = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except subprocess.TimeoutExpired:
        print("[WARN] git rev-parse 逾時——[SRC] 略過 repo HEAD", file=sys.stderr)
        return None
    except FileNotFoundError:
        print("[WARN] git 不在 PATH——[SRC] 略過 repo HEAD", file=sys.stderr)
        return None
    if proc.returncode != 0 or not proc.stdout.strip():
        print(
            f"[WARN] git rev-parse 失敗——[SRC] 略過 repo HEAD：{proc.stderr.strip()}",
            file=sys.stderr,
        )
        return None
    return proc.stdout.strip()


def _short(sha: str) -> str:
    return sha[:7]


def source_line(index_path: Path, repo: Path | None) -> str | None:
    """facade 契約「每回應附 source 與 commit 版本」的輸出面。

    證據優先序：stamp sidecar 的 head（index 生成點真相）→ ``--repo``
    live HEAD（audit 端真相）。皆無 → None 不輸出（顯式 --index legacy
    呼叫輸出維持位元組不變——NT 查詢契約）。三道守衛（皆 WARN 不擋服務）：
    sidecar 比 index 檔舊（重生後未重 stamp）；stamp 的 repo 與 ``--repo``
    不符（同名 basename——sha 歸屬可能錯）；兩 sha 皆有但不一致（漂移——
    A3 graph.db 過時事件同型防線）。
    """
    stale_stamp = False
    try:
        stale_stamp = meta_path(index_path).stat().st_mtime < index_path.stat().st_mtime
    except OSError:
        stale_stamp = False
    meta = load_meta(index_path)
    idx_sha = meta.get("head") if meta else None
    repo_sha = _git_head(repo) if repo else None
    if idx_sha is None and repo_sha is None:
        return None
    if stale_stamp and meta is not None:
        print(
            "[WARN] stamp 比索引檔舊——索引重生成後未重 stamp（跑 --stamp-meta）",
            file=sys.stderr,
        )
    parts: list[str] = []
    if idx_sha:
        stamped = str(meta.get("stamped_at", ""))[:10]
        parts.append(
            f"scip index @ {_short(idx_sha)}" + (f"（{stamped}）" if stamped else "")
        )
    else:
        print(
            "[WARN] index meta 未 stamp（生成後跑 --stamp-meta）——[SRC] 缺 index 版本",
            file=sys.stderr,
        )
    if repo_sha:
        parts.append(f"repo HEAD @ {_short(repo_sha)}")
    if idx_sha and repo_sha:
        stamped_repo = meta.get("repo")
        if stamped_repo and stamped_repo != str(repo.resolve()):
            print(
                f"[WARN] stamp 的 repo（{stamped_repo}）與 --repo 不符——"
                "index sha 歸屬可能錯（同名 basename？改用顯式 --index）",
                file=sys.stderr,
            )
        if idx_sha != repo_sha:
            print(
                f"[WARN] repo HEAD 已離開 index 生成點（index @ {_short(idx_sha)}"
                f" vs HEAD @ {_short(repo_sha)}）——重生索引並重跑 --stamp-meta"
                "後再查",
                file=sys.stderr,
            )
    return "[SRC] " + " · ".join(parts)


def stamp_meta(index_path: Path, repo: Path) -> int:
    """資料面：索引生成後落版本 sidecar（重跑覆寫，冪等）。

    查詢端被動讀——未 stamp 的舊索引 [SRC] 缺 index 版本（WARN 提示），
    不拒絕服務。欄位刻意不共用 ``common.make_meta``：``head``/
    ``stamped_at`` 語義是「index 生成點」非產物創建時刻、``repo`` 存
    resolved 全路徑（同名異路徑 repo 排查用），且 make_meta 的
    ``check=True`` 裸 traceback 不合本工具 exit-2 契約。
    """
    head = _git_head(repo)
    if head is None:  # WARN 已印
        print("[FAIL] 取不到 repo HEAD——meta 未 stamp", file=sys.stderr)
        return 2
    sidecar = meta_path(index_path)
    payload = {
        "repo": str(repo.resolve()),
        "head": head,
        "stamped_at": datetime.now(UTC).isoformat(timespec="seconds"),
        "tool": "code_reality.scip_refs",
    }
    try:
        sidecar.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    except OSError as e:
        print(f"[FAIL] sidecar 寫入失敗：{sidecar}：{e}", file=sys.stderr)
        return 2
    print(f"[OK] meta stamped：{sidecar}（{repo.name} @ {_short(head)}）")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("query", nargs="?", help="Type.method 或裸函數名")
    parser.add_argument(
        "--index",
        type=Path,
        default=None,
        help=(
            "SCIP index 路徑（生成命令見 docstring；rebase 後需重生）；"
            "省略時以 --repo 解析 repo-keyed 預設 slot"
        ),
    )
    parser.add_argument(
        "--audit", action="store_true", help="graph_audit 缺差 → SCIP refs 對照"
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=None,
        help=(
            "目標 repo——audit 餵 graph_audit；--index 省略時解析預設 slot；"
            "補 [SRC] live HEAD 標註"
        ),
    )
    parser.add_argument(
        "--stamp-meta",
        action="store_true",
        help="索引生成後落版本 sidecar（配 --repo；[SRC] 標註的資料面）",
    )
    parser.add_argument(
        "--build-cache",
        action="store_true",
        help=(
            "構建衍生 sqlite 查詢面 <index>.scip.db（一次構建；"
            "查詢/audit 自動優先使用，過期自動重建）"
        ),
    )
    args = parser.parse_args()

    if args.build_cache and (args.stamp_meta or args.audit or args.query is not None):
        print(
            "[FAIL] --build-cache 與 --stamp-meta/--audit/查詢互斥",
            file=sys.stderr,
        )
        return 2
    if args.stamp_meta and (args.audit or args.query is not None):
        print("[FAIL] --stamp-meta 與 --audit/查詢互斥", file=sys.stderr)
        return 2
    if args.stamp_meta:
        if args.repo is None:
            print("[FAIL] --stamp-meta 需 --repo", file=sys.stderr)
            return 2
    elif scip_pb2 is None:  # stamp 不解析 index——protobuf 非其依賴
        print(
            "[FAIL] protobuf 未安裝——scip_pb2（vendored gencode）需要 "
            "google.protobuf runtime（uv add --group dev protobuf，或單發 "
            "uv run --with protobuf）",
            file=sys.stderr,
        )
        return 2
    if args.audit and args.query is not None:
        print("[FAIL] --audit 與查詢字串互斥", file=sys.stderr)
        return 2
    if args.audit and args.repo is None:
        print("[FAIL] --audit 需 --repo（graph_audit 目標）", file=sys.stderr)
        return 2

    default_resolved = False
    if args.index is None:
        if args.repo is None:
            print(
                "[FAIL] 需 --index（或 --repo 解析 repo-keyed 預設 slot）",
                file=sys.stderr,
            )
            return 2
        args.index = default_index_path(args.repo)
        default_resolved = True
    if not args.index.exists():
        if default_resolved:
            print(
                f"[FAIL] 預設索引不在：{args.index}"
                f"（--repo {args.repo} → repo-keyed slot；生成命令或搬遷見 docstring）",
                file=sys.stderr,
            )
            legacy = DEFAULT_INDEX_ROOT / "index.scip"
            if legacy.exists():
                print(
                    f"  既有全局 slot 索引可搬遷（免重生成；僅當該索引生成自"
                    f" --repo 指定的 repo——搬錯 repo 的索引會全域查無）："
                    f"mkdir -p {args.index.parent} && mv {legacy} {args.index.parent}/",
                    file=sys.stderr,
                )
        else:
            print(f"[FAIL] 索引不在：{args.index}", file=sys.stderr)
        return 2

    if args.stamp_meta:
        return stamp_meta(args.index, args.repo)
    if args.build_cache:
        return build_cache_mode(args.index)
    if not args.audit and not args.query:
        print("[FAIL] 需提供查詢或 --audit", file=sys.stderr)
        return 2
    src_line = source_line(args.index, args.repo)
    if args.audit:
        return audit_mode(args.index, args.repo, src_line)
    return report(open_face(args.index), args.query, src_line)


if __name__ == "__main__":
    sys.exit(main())
