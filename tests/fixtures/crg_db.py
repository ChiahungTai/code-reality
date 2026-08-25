"""臨時 CRG graph.db 產生器——S2/S4 單元測試用，不依賴真 CRG 安裝。

schema 對齊 code-review-graph sqlite（nodes/edges/metadata 表），最小欄位
集：export_module_edges 只讀 edges 的 kind/source/target，hub_refs 名稱解析
只讀 nodes 的 name/parent_name/qualified_name/file_path，metadata 只讀
key-value。
"""

import sqlite3
from pathlib import Path


def make_crg_db(
    path: Path,
    edges: list[tuple[str, str, str]] | None = None,
    metadata: dict[str, str] | None = None,
    nodes: list[tuple[str, str | None, str, str]] | None = None,
    *,
    communities: list[tuple[int, str, int, str, str]] | None = None,
    node_attrs: dict[str, tuple[str, str, int, int | None]] | None = None,
    node_lines: dict[str, int] | None = None,
) -> None:
    """建臨時 CRG 相容 db。

    edges: [(kind, source_qualified, target_qualified), ...]
    metadata: CRG metadata key-value（git_head_sha/last_updated/...）
    nodes: [(name, parent_name, qualified_name, file_path), ...]——
        hub_refs 名稱解析用（nodes 表精確匹配）
    communities: [(id, name, size, dominant_language, description), ...]——
        graph_csv 用（community 多數決導出）
    node_attrs: {qualified_name: (kind, language, is_test, community_id)}——
        patch 預設 Class 節點（File 節點/語言/測試旗標/community 歸屬）
    node_lines: {qualified_name: line_start}——graph_csv/chain_tour 重錨用
    qualified 格式 ``<abs-path>::Class.method``（CRG 慣例）。
    """
    conn = sqlite3.connect(path)
    conn.execute("CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT)")
    conn.execute(
        """
        CREATE TABLE nodes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            qualified_name TEXT NOT NULL UNIQUE,
            file_path TEXT NOT NULL,
            line_start INTEGER,
            line_end INTEGER,
            language TEXT,
            parent_name TEXT,
            is_test INTEGER DEFAULT 0,
            updated_at REAL NOT NULL,
            community_id INTEGER
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE communities (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            level INTEGER NOT NULL DEFAULT 0,
            parent_id INTEGER,
            cohesion REAL DEFAULT 0.0,
            size INTEGER DEFAULT 0,
            dominant_language TEXT,
            description TEXT,
            created_at TEXT NOT NULL DEFAULT 'test'
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            source_qualified TEXT NOT NULL,
            target_qualified TEXT NOT NULL,
            file_path TEXT NOT NULL,
            line INTEGER DEFAULT 0,
            extra TEXT DEFAULT '{}',
            confidence REAL DEFAULT 1.0,
            confidence_tier TEXT DEFAULT 'EXTRACTED',
            updated_at REAL NOT NULL
        )
        """
    )
    for key, value in (metadata or {}).items():
        conn.execute("INSERT INTO metadata (key, value) VALUES (?, ?)", (key, value))
    for cid, name, size, lang, desc in communities or []:
        conn.execute(
            "INSERT INTO communities (id, name, size, dominant_language, description)"
            " VALUES (?, ?, ?, ?, ?)",
            (cid, name, size, lang, desc),
        )
    for name, parent, qname, file_path in nodes or []:
        conn.execute(
            "INSERT INTO nodes (kind, name, qualified_name, file_path, parent_name, updated_at)"
            " VALUES ('Class', ?, ?, ?, ?, 0)",
            (name, qname, file_path, parent),
        )
    for qname, (kind, language, is_test, community_id) in (node_attrs or {}).items():
        cur = conn.execute(
            "UPDATE nodes SET kind=?, language=?, is_test=?, community_id=?"
            " WHERE qualified_name=?",
            (kind, language, is_test, community_id, qname),
        )
        assert cur.rowcount == 1, f"node_attrs qname 未命中任何節點：{qname}"
    for qname, line_start in (node_lines or {}).items():
        cur = conn.execute(
            "UPDATE nodes SET line_start=? WHERE qualified_name=?",
            (line_start, qname),
        )
        assert cur.rowcount == 1, f"node_lines qname 未命中任何節點：{qname}"
    for kind, src, dst in edges or []:
        conn.execute(
            "INSERT INTO edges (kind, source_qualified, target_qualified, file_path, updated_at)"
            " VALUES (?, ?, ?, ?, ?)",
            (kind, src, dst, src.split("::")[0], 0.0),
        )
    conn.commit()
    conn.close()


def qualified(repo_root: Path, rel_path: str, symbol: str = "Cls.method") -> str:
    return f"{repo_root / rel_path}::{symbol}"
