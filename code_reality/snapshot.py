"""弧 snapshot——CRG module-edge 集導出為 commit 錨定 sidecar。

UC5 Transition 的原料（報告 §8 #2）：Depwire 有此能力但共享 worktree 危害
（R6 stash 事故）＋BUSL——自建輕量版＝CRG 邊集導出＋commit 錨定。schema 是
transition.py 的合約：``{"_meta": {...}, "files": [...], "module_edges":
[[src_mod, dst_mod, kind], ...]}``（module 由 repo profile ``[[module]]``
規則決定——見 profile.py）。

用法::

    uv run python -m code_reality.snapshot [--repo PATH] [--label <ep>]

產物：``~/.mosaic/code-reality/snapshots/<repo>-<short-hash>.json``（同
commit 重跑覆寫＝冪等）。
"""

import argparse
import json
import sqlite3
import subprocess
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

from code_reality.common import (
    EDGE_KINDS,
    assert_db_unchanged,
    connect_ro,
    db_mtime_ns,
    graph_db_path,
    make_meta,
    repo_relative,
)
from code_reality.exclusions import is_excluded
from code_reality.profile import Profile, load_profile, module_of

DEFAULT_OUT_DIR = Path.home() / ".mosaic" / "code-reality" / "snapshots"


@dataclass(frozen=True)
class EdgeExport:
    files: list[str]
    module_edges: list[list[str]]
    raw_edge_count: int = 0


def _repo_relative(qualified: str, repo_root: Path) -> str | None:
    return repo_relative(qualified.split("::")[0], repo_root)


def export_module_edges(
    conn: sqlite3.Connection, repo_root: Path, profile: Profile | None
) -> EdgeExport:
    """全量 module-edge 導出（評估弧 POC 實測定案 SQL——git 歷史）。

    ``files``＝**參與 module-edge 的檔案**（非全 repo 清單——bare import
    解析不到邊的檔不入列；post-build D6：dogfood 實測報告 +15 vs git +19
    .py 的落差即此語義）。
    """
    repo_root = repo_root.resolve()
    edges: set[tuple[str, str, str]] = set()
    files: set[str] = set()
    for kind, src_q, dst_q in conn.execute(
        "SELECT kind, source_qualified, target_qualified FROM edges "
        f"WHERE kind IN ({','.join('?' * len(EDGE_KINDS))})",
        EDGE_KINDS,
    ):
        src_rel, dst_rel = (
            _repo_relative(src_q, repo_root),
            _repo_relative(dst_q, repo_root),
        )
        if src_rel is None or dst_rel is None:
            continue
        if is_excluded(src_rel, profile) or is_excluded(dst_rel, profile):
            continue
        files.update((src_rel, dst_rel))
        src_mod, dst_mod = module_of(src_rel, profile), module_of(dst_rel, profile)
        if src_mod != dst_mod:
            edges.add((src_mod, dst_mod, kind))
    return EdgeExport(
        files=sorted(files),
        module_edges=sorted(list(e) for e in edges),
        raw_edge_count=int(conn.execute("SELECT COUNT(*) FROM edges").fetchone()[0]),
    )


def head_sha(repo_root: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def head_commit_time(repo_root: Path) -> datetime:
    out = subprocess.run(
        ["git", "log", "-1", "--format=%cI"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return datetime.fromisoformat(out)


def detect_stale(
    meta: dict[str, str],
    head_sha_value: str | None,
    head_time: datetime,
    db_mtime: datetime | None = None,
) -> str | None:
    """CRG 圖是否落後 HEAD——回傳 stale 原因；新鮮回 None（SM-6）。

    首選 sha 比對（精確、免時區假設）；缺 ``git_head_sha`` 時 fallback
    ``last_updated``（naive——假設 local tz，POC 字串直比誤判的修正）→
    db mtime。
    """
    graph_sha = meta.get("git_head_sha")
    if graph_sha:
        if graph_sha != head_sha_value:
            return f"graph sha {graph_sha[:8]} != HEAD {head_sha_value or '?'}"
        return None
    updated = meta.get("last_updated")
    if updated:
        try:
            graph_t = datetime.fromisoformat(updated).astimezone()
        except ValueError:
            graph_t = None
        if graph_t is not None:
            if graph_t < head_time:
                return f"graph last_updated {updated} < HEAD commit {head_time.isoformat()}"
            return None
    if db_mtime is not None and db_mtime < head_time:
        return (
            f"graph mtime {db_mtime.isoformat()} < HEAD commit {head_time.isoformat()}"
        )
    return None


def _load_metadata(db_path: Path) -> dict[str, str]:
    """metadata 載入——空/半套（build 進行中）retry 一次後 crash-only。

    retry 僅涵蓋「無 -wal 且 metadata 半套」的極短窗口（-wal 存在時
    connect_ro 在首次迭代即 crash——immutable 連線看不到 WAL-committed
    資料，分鐘級 build 等不完屬預期，crash 方向安全）。非 CRG db（無
    metadata 表/非 sqlite）包成附安裝指引的錯誤。
    """
    for attempt in range(2):
        conn = connect_ro(db_path)
        try:
            meta = dict(conn.execute("SELECT key, value FROM metadata"))
        except sqlite3.DatabaseError as e:
            raise AssertionError(
                f"非 CRG graph.db（讀 metadata 失敗：{e}）：{db_path}"
                "——先跑 `uvx code-review-graph build`"
            ) from e
        finally:
            conn.close()
        if meta.get("git_head_sha") or meta.get("last_updated"):
            return meta
        if attempt == 0:
            time.sleep(1.0)
    raise AssertionError(
        f"CRG metadata 不完整（build 進行中？）：{db_path}——稍後重跑或 uvx code-review-graph build"
    )


def _assert_git_root(repo_root: Path) -> None:
    """--repo 必須正是 git root——git rev-parse 會往上爬，指到子目錄時
    commit 錨定會靜默錯植外層 repo 的 HEAD。"""
    top = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    assert top == str(repo_root), (
        f"--repo 指到 {repo_root} 但 git root 是 {top}——commit 錨定會錯植"
        "外層 repo；--repo 須指 repo 根"
    )


@dataclass
class Snapshot:
    meta: dict[str, Any]
    files: list[str]
    module_edges: list[list[str]]

    @property
    def default_path(self) -> Path:
        return Path(f"{self.meta['repo']}-{self.meta['commit'][:8]}.json")

    def write(self, out_dir: Path) -> Path:
        out_dir.mkdir(parents=True, exist_ok=True)
        path = out_dir / self.default_path
        path.write_text(
            json.dumps(
                {
                    "_meta": self.meta,
                    "files": self.files,
                    "module_edges": self.module_edges,
                },
                indent=1,
            )
        )
        return path


def build_snapshot(repo_root: Path, label: str | None = None) -> Snapshot:
    repo_root = repo_root.resolve()
    db_path = graph_db_path(repo_root)
    assert db_path.exists(), (
        f"graph.db 不存在：{db_path}——先跑 `uvx code-review-graph build`（SM-11）"
    )
    _assert_git_root(repo_root)

    meta_db = _load_metadata(db_path)
    sha = head_sha(repo_root)
    stale_reason = detect_stale(
        meta_db,
        sha,
        head_commit_time(repo_root),
        db_mtime=datetime.fromtimestamp(db_path.stat().st_mtime).astimezone(),
    )

    m0 = db_mtime_ns(db_path)
    profile = load_profile(repo_root)
    conn = connect_ro(db_path)
    try:
        exported = export_module_edges(conn, repo_root, profile)
    finally:
        conn.close()
    assert_db_unchanged(db_path, m0)

    meta = make_meta("code_reality.snapshot", repo_root, commit=sha)
    meta.update(
        {
            "label": label,
            "stale": stale_reason,
            "crg_last_updated": meta_db.get("last_updated"),
            "crg_last_build_type": meta_db.get("last_build_type"),
            "crg_raw_edges": exported.raw_edge_count,
        }
    )
    return Snapshot(meta=meta, files=exported.files, module_edges=exported.module_edges)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path.cwd(),
        help="repo 根（含 .code-review-graph/）",
    )
    parser.add_argument("--label", default=None, help="EP/弧標籤（記入 _meta）")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    args = parser.parse_args()

    snap = build_snapshot(args.repo, label=args.label)
    path = snap.write(args.out_dir)
    if snap.meta.get("stale"):
        print(
            f"[WARN] CRG graph stale: {snap.meta['stale']}——先 uvx code-review-graph build 再 snapshot"
        )
    if not snap.files:
        print(
            f"[WARN] snapshot 空集合（0 files，db raw {snap.meta.get('crg_raw_edges')} 邊）"
            "——graph.db 與 --repo 不同 root？下游 transition 會誤報無結構變化"
        )
    print(
        f"[OK] snapshot: {len(snap.files)} files, {len(snap.module_edges)} module edges -> {path}"
    )
    print(f"[LOG] rg '\"module_edges\"' {path} | head")


if __name__ == "__main__":
    main()
