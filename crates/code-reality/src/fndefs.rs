//! fndefs — fn-span sidecar, the sqlite carrier for closure/callers spans
//! (adapter). SM-13: completely independent from the frozen three-table db
//! — own file, own `meta` keys, own schema constant. Python never reads or
//! writes this artifact; nothing here touches the shared db. Staleness and
//! the accelerator ladder mirror the three-table guard family: absent →
//! protobuf in-memory spans (never build on miss), stale/corrupt → WARN +
//! rebuild, rebuild failure → WARN + protobuf spans (answer anyway).

use crate::engine::{self, stamped_head, FnSpan};
use rusqlite::{Connection, OpenFlags};
use scip::types::Index;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FNDEFS_SCHEMA_VERSION: &str = "1";

const FNDEFS_SQL: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE fn_defs (
    -- seq explicit PK: VACUUM does not renumber (implicit rowid would) —
    -- insertion order = scan order, the same-width tie first-seen basis
    -- (mirrors the occurrences table precedent)
    seq        INTEGER PRIMARY KEY,
    symbol     TEXT    NOT NULL,
    rel_path   TEXT    NOT NULL,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL
);
CREATE INDEX idx_fn_defs_rel_path ON fn_defs(rel_path);
";

/// Sidecar path: full file name + `.fndefs.db` (same rule as
/// [`crate::cache::sqlite_path`]) → `index.scip.fndefs.db` — a sibling of
/// the index and the three-table db, never merged with either.
pub fn fndefs_path(index_path: &Path) -> PathBuf {
    let mut name = index_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".fndefs.db");
    index_path.with_file_name(name)
}

/// Core build — single transaction, tmp-first-clear + rename (build_db
/// shape). Returns (span count, warn lines from span parsing). The meta keys
/// are the sidecar's own (`tool` marks the Rust owner).
pub fn build_sidecar(
    index: &Index,
    db_path: &Path,
    sidecar_head: &str,
) -> Result<(usize, Vec<String>), String> {
    let (spans_by_doc, warns) = engine::fn_spans(index);
    let flat: Vec<&FnSpan> = spans_by_doc.values().flatten().collect();

    let tmp = db_path.with_file_name(format!(
        "{}.tmp",
        db_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_file(&tmp); // crash leftover would break CREATE TABLE
    let conn = Connection::open(&tmp).map_err(|e| e.to_string())?;
    conn.execute_batch(FNDEFS_SQL).map_err(|e| e.to_string())?;
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    // The original protobuf scan seq is stored verbatim (not renumbered by
    // the per-file map flatten) — the roundtrip then holds on real indexes
    // whose document order differs from the map's sort order.
    for s in &flat {
        conn.execute(
            "INSERT INTO fn_defs (seq, symbol, rel_path, start_line, end_line)\
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![s.seq as i64, s.symbol, s.rel_path, s.start_line, s.end_line],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('head', ?1), ('schema', ?2),\
         ('tool', 'code-reality.fndefs')",
        rusqlite::params![sidecar_head, FNDEFS_SCHEMA_VERSION],
    )
    .map_err(|e| e.to_string())?;
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    drop(conn);
    std::fs::rename(&tmp, db_path).map_err(|e| e.to_string())?;
    Ok((flat.len(), warns))
}

fn head_of(index_path: &Path) -> String {
    stamped_head(index_path)
}

fn query_meta(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM meta")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect()
}

/// Five staleness signals, mirroring [`crate::cache::stale_reason`]: db
/// older than index, stat failure, corrupt db (open/meta probe — sqlite
/// reports non-db files lazily, so probe a real query), schema mismatch,
/// sidecar-head drift. Corrupt counts as stale — rebuild cures it.
pub fn stale_sidecar_reason(index_path: &Path, db_path: &Path) -> Option<String> {
    let (db_m, idx_m) = match (db_path.metadata(), index_path.metadata()) {
        (Ok(d), Ok(i)) => match (d.modified(), i.modified()) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Some(format!("stat 失敗：{}", e)),
        },
        (Err(e), _) | (_, Err(e)) => return Some(format!("stat 失敗：{}", e)),
    };
    if db_m < idx_m {
        return Some("sidecar 比索引檔舊".to_string());
    }
    let conn = match Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(e) => return Some(format!("sidecar 損壞：{}", e)),
    };
    let meta: BTreeMap<String, String> = match query_meta(&conn) {
        Ok(rows) => rows.into_iter().collect(),
        Err(e) => return Some(format!("sidecar 損壞：{}", e)),
    };
    if meta.get("schema").map(String::as_str) != Some(FNDEFS_SCHEMA_VERSION) {
        let got = meta.get("schema").cloned().unwrap_or_else(|| "無".into());
        return Some(format!(
            "sidecar schema 版本不符（{} ≠ {}）",
            got, FNDEFS_SCHEMA_VERSION
        ));
    }
    if meta.get("head").cloned().unwrap_or_default() != head_of(index_path) {
        return Some("sidecar head 變動（索引重生後重 stamp？）".to_string());
    }
    None
}

/// Load spans grouped per file, `ORDER BY seq` (scan order; the tie rule
/// needs the original seq). Errors propagate — the caller decides the ladder.
pub fn load_spans(db_path: &Path) -> Result<BTreeMap<String, Vec<FnSpan>>, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT seq, symbol, rel_path, start_line, end_line FROM fn_defs ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FnSpan {
                seq: r.get::<_, i64>(0)? as usize,
                symbol: r.get::<_, String>(1)?,
                rel_path: r.get::<_, String>(2)?,
                start_line: r.get::<_, i64>(3)?,
                end_line: r.get::<_, i64>(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let flat = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    let mut map: BTreeMap<String, Vec<FnSpan>> = BTreeMap::new();
    for s in flat {
        map.entry(s.rel_path.clone()).or_default().push(s);
    }
    Ok(map)
}

/// Spans-source ladder (mirrors [`crate::cache::open_face`]): fresh sidecar
/// → sqlite spans; absent → protobuf in-memory spans from `index` (loading
/// the index only if not already in hand — never builds the sidecar on
/// miss); stale or corrupt → stderr WARN + rebuild; rebuild failure → WARN
/// + protobuf spans (accelerator, not dependency). Returns (spans, stderr).
pub fn spans_source(
    index_path: &Path,
    index: Option<&Index>,
) -> Result<SpansWithStderr, String> {
    let mut stderr: Vec<String> = Vec::new();
    let path = fndefs_path(index_path);
    // Load-once slot: reused across the ladder's protobuf branches without
    // copying the (potentially NT-scale) index.
    let mut owned: Option<Index> = None;

    if !path.exists() {
        // miss never builds (family rule): protobuf spans
        let idx = resolve_idx(index, &mut owned, &mut stderr, index_path)?;
        let (spans, warns) = engine::fn_spans(idx);
        stderr.extend(warns);
        return Ok((spans, stderr));
    }
    let stale = stale_sidecar_reason(index_path, &path);
    if stale.is_none() {
        if let Ok(spans) = load_spans(&path) {
            return Ok((spans, stderr));
        }
        // corrupt-but-fresh (mtime lied): fall through to the rebuild ladder
    }
    let reason = stale.unwrap_or_else(|| "sidecar 損壞（開啟或讀取失敗）".to_string());
    stderr.push(format!(
        // "改用 protobuf spans" (not the family's "改走 protobuf 全量解析"):
        // only the span source degrades here — rows may still come from
        // the sqlite face (mixed face).
        "[WARN] fn_defs sidecar 過期（{}）——自動重建\n",
        reason
    ));
    let idx = resolve_idx(index, &mut owned, &mut stderr, index_path)?;
    match build_sidecar(idx, &path, &head_of(index_path)) {
        Ok((n, warns)) => {
            stderr.extend(warns);
            stderr.push(format!("[OK] fn_defs sidecar 重建完成（{} spans）\n", n));
            match load_spans(&path) {
                Ok(spans) => Ok((spans, stderr)),
                Err(e) => {
                    stderr.push(format!(
                        "[WARN] fn_defs sidecar 重建後讀取失敗——本次查詢改用 protobuf spans：{}\n",
                        e
                    ));
                    let (spans, warns) = engine::fn_spans(idx);
                    stderr.extend(warns);
                    Ok((spans, stderr))
                }
            }
        }
        Err(e) => {
            stderr.push(format!(
                "[WARN] fn_defs sidecar 重建失敗——本次查詢改用 protobuf spans：{}\n",
                e
            ));
            let (spans, warns) = engine::fn_spans(idx);
            stderr.extend(warns);
            Ok((spans, stderr))
        }
    }
}

/// Spans map + accumulated stderr lines (the ladder's answer shape).
pub type SpansWithStderr = (BTreeMap<String, Vec<FnSpan>>, Vec<String>);

/// Caller-provided index when available, else a load-once slot.
fn resolve_idx<'a>(
    index: Option<&'a Index>,
    owned: &'a mut Option<Index>,
    stderr: &mut Vec<String>,
    index_path: &Path,
) -> Result<&'a Index, String> {
    match index {
        Some(i) => Ok(i),
        None => {
            if owned.is_none() {
                let l = engine::load_index(index_path)?;
                stderr.push(l.stderr);
                *owned = Some(l.index);
            }
            Ok(owned.as_ref().unwrap())
        }
    }
}
