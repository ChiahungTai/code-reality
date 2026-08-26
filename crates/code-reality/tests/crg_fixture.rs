//! Shared synthetic CRG graph.db builder — the CRG schema as the family
//! consumes it (production-shape columns incl. `extra`, verified against
//! the live NT graph.db DDL on 2026-08-26 — CRG 2.3.7; the retired Python
//! fixture was the original source, superseded when R7 deleted the parity
//! oracle). Integration tests include this via `mod crg_fixture;`.

use rusqlite::Connection;
use std::path::Path;

#[allow(dead_code)] // compiled per test target; not every target uses it
#[derive(Default)]
pub struct NodeSeed {
    pub name: String,
    pub parent: Option<String>,
    pub qname: String,
    pub file_path: String,
}

/// `node_attrs` patch: (kind, language, is_test, community_id).
#[allow(dead_code)] // per-target helper
pub struct NodeAttr {
    pub kind: &'static str,
    pub language: &'static str,
    pub is_test: i64,
    pub community_id: Option<i64>,
}

#[allow(dead_code)] // per-target helper
#[derive(Default)]
pub struct CrgDbSpec {
    /// (kind, source_qualified, target_qualified)
    pub edges: Vec<(String, String, String)>,
    /// metadata key-value (git_head_sha / last_updated / …)
    pub metadata: Vec<(String, String)>,
    pub nodes: Vec<NodeSeed>,
    /// (id, name, size, dominant_language, description)
    pub communities: Vec<(i64, String, i64, String, String)>,
    pub node_attrs: Vec<(String, NodeAttr)>,
    pub node_lines: Vec<(String, i64)>,
    /// (qname, line_start, line_end) full-span patch
    pub node_spans: Vec<(String, i64, i64)>,
    /// (flow_id, node_id) memberships; flows table rows are created per
    /// distinct flow_id (criticality from flow_crits).
    pub flow_members: Vec<(i64, i64)>,
    /// flow_id -> criticality
    pub flow_crits: Vec<(i64, f64)>,
}

/// Build a CRG-compatible synthetic db at `path` (schema verbatim from
/// the Python fixture; `updated_at` filler values are non-null only).
#[allow(dead_code)] // per-target helper
pub fn make_crg_db(path: &Path, spec: &CrgDbSpec) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
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
            extra TEXT DEFAULT '{}',
            updated_at REAL NOT NULL,
            community_id INTEGER
         );
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
         );
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
         );",
    )?;
    for (k, v) in &spec.metadata {
        conn.execute("INSERT INTO metadata (key, value) VALUES (?1, ?2)", (k, v))?;
    }
    for (cid, name, size, lang, desc) in &spec.communities {
        conn.execute(
            "INSERT INTO communities (id, name, size, dominant_language, description)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (cid, name, size, lang, desc),
        )?;
    }
    for n in &spec.nodes {
        conn.execute(
            "INSERT INTO nodes (kind, name, qualified_name, file_path, parent_name, updated_at)
             VALUES ('Class', ?1, ?2, ?3, ?4, 0)",
            (&n.name, &n.qname, &n.file_path, &n.parent),
        )?;
    }
    for (qname, attr) in &spec.node_attrs {
        let n = conn.execute(
            "UPDATE nodes SET kind=?1, language=?2, is_test=?3, community_id=?4
             WHERE qualified_name=?5",
            (
                attr.kind,
                attr.language,
                attr.is_test,
                attr.community_id,
                qname,
            ),
        )?;
        assert_eq!(n, 1, "node_attrs qname 未命中任何節點：{qname}");
    }
    for (qname, line_start) in &spec.node_lines {
        let n = conn.execute(
            "UPDATE nodes SET line_start=?1 WHERE qualified_name=?2",
            (line_start, qname),
        )?;
        assert_eq!(n, 1, "node_lines qname 未命中任何節點：{qname}");
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS flows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            entry_point_id INTEGER NOT NULL,
            depth INTEGER NOT NULL,
            node_count INTEGER NOT NULL,
            file_count INTEGER NOT NULL,
            criticality REAL NOT NULL DEFAULT 0.0,
            path_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT 'test',
            updated_at TEXT NOT NULL DEFAULT 'test'
        );
        CREATE TABLE IF NOT EXISTS flow_memberships (
            flow_id INTEGER NOT NULL,
            node_id INTEGER NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (flow_id, node_id)
        );",
    )?;
    for (fid, crit) in &spec.flow_crits {
        conn.execute(
            "INSERT INTO flows (id, name, entry_point_id, depth, node_count, \
             file_count, criticality, path_json) \
             VALUES (?1, 'f', 0, 1, 1, 1, ?2, '[]')",
            (fid, crit),
        )?;
    }
    for (fid, nid) in &spec.flow_members {
        conn.execute(
            "INSERT OR IGNORE INTO flow_memberships (flow_id, node_id)
             VALUES (?1, ?2)",
            (fid, nid),
        )?;
    }
    for (qname, ls, le) in &spec.node_spans {
        let n = conn.execute(
            "UPDATE nodes SET line_start=?1, line_end=?2 WHERE qualified_name=?3",
            (ls, le, qname),
        )?;
        assert_eq!(n, 1, "node_spans qname 未命中任何節點：{qname}");
    }
    for (kind, src, dst) in &spec.edges {
        conn.execute(
            "INSERT INTO edges (kind, source_qualified, target_qualified, file_path, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (kind, src, dst, src.split("::").next().unwrap_or(src)),
        )?;
    }
    Ok(())
}

/// CRG qualified-name convention: `<abs-path>::<symbol>`.
#[allow(dead_code)] // per-target helper
pub fn qualified(repo_root: &Path, rel_path: &str, symbol: &str) -> String {
    format!("{}::{}", repo_root.join(rel_path).display(), symbol)
}
