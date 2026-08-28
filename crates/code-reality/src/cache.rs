//! cache — derived sqlite three-table cache + query-face selection (adapter).
//!
//! Interop contract with the frozen Python implementation (SM-13): identical
//! DDL, identical scan-order insertion (seq PK), identical meta keys
//! (head/schema/tool). Schema evolution goes through the existing
//! stale-rebuild semantics — the Python guard rebuilds anything it does not
//! recognize, so fn_defs-style extensions live OUTSIDE this db (R3 decision).
//! Rebuild failure falls back to the protobuf face with a stderr WARN
//! ("derived face is an accelerator, not a dependency" — scip_refs.py:459).

use crate::engine::{
    self, fn_tail_name, load_index, loc_line, matches_query, stamped_head, tail, Query,
};
use rusqlite::{Connection, OpenFlags};
use scip::types::Index;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "1";

pub const SCHEMA_SQL: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE occurrences (
    -- seq explicit PK: VACUUM does not renumber (implicit rowid would) —
    -- insertion order = scan order, the basis of byte-equivalent output
    seq      INTEGER PRIMARY KEY,
    symbol   TEXT    NOT NULL,
    rel_path TEXT    NOT NULL,
    line     INTEGER NOT NULL,
    is_def   INTEGER NOT NULL
);
CREATE TABLE symbol_tails (
    -- tail = precomputed descriptor (for manual sqlite inspection;
    -- program-side computes tail() live)
    symbol TEXT PRIMARY KEY,
    tail   TEXT NOT NULL,
    method TEXT NOT NULL
);
CREATE INDEX idx_symbol_tails_method ON symbol_tails(method);
CREATE INDEX idx_occurrences_symbol ON occurrences(symbol, is_def);
";

pub struct Stats {
    pub symbols: usize,
    pub occurrences: usize,
    /// Documents that carried occurrences in the SCIP index but lost
    /// every one to the fn-tail gate (class/variable-only files — the
    /// frozen R2-3 filter; s5-ceiling B8 evidence). Loud list so future
    /// audits don't re-derive this by manual digging.
    pub docs_fully_filtered: usize,
}

pub fn sqlite_path(index_path: &Path) -> PathBuf {
    let name = index_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    index_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{}.db", name))
}

/// Core build — single transaction, atomic swap via tmp file + rename
/// (scip_refs.py:350). Occurrences ingest only FN_TAIL symbols.
pub fn build_db(index: &Index, db_path: &Path, sidecar_head: &str) -> Result<Stats, String> {
    let mut tails: BTreeMap<String, (String, String)> = BTreeMap::new();
    for d in &index.documents {
        for occ in &d.occurrences {
            if let Some(method) = fn_tail_name(&occ.symbol) {
                tails.insert(
                    occ.symbol.clone(),
                    (tail(&occ.symbol).to_string(), method.to_string()),
                );
            }
        }
    }
    let mut count = 0usize;
    let mut docs_fully_filtered = 0usize;

    let tmp = db_path.with_file_name(format!(
        "{}.tmp",
        db_path.file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_file(&tmp); // leftover from a previous crash would break CREATE TABLE
    let conn = Connection::open(&tmp).map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA_SQL)
        .and_then(|_| {
            // One transaction for the whole insert phase: per-statement
            // autocommit is orders of magnitude slower at NT scale (~1M
            // occurrences); atomicity itself is guaranteed by tmp+rename.
            conn.execute_batch("BEGIN")
        })
        .and_then(|_| {
            for (symbol, (tail_s, method)) in &tails {
                conn.execute(
                    "INSERT INTO symbol_tails (symbol, tail, method) VALUES (?, ?, ?)",
                    rusqlite::params![symbol, tail_s, method],
                )?;
            }
            Ok(())
        })
        .and_then(|_| {
            for d in &index.documents {
                let mut kept_any = false;
                for occ in &d.occurrences {
                    if tails.contains_key(&occ.symbol) {
                        kept_any = true;
                        count += 1;
                        conn.execute(
                            "INSERT INTO occurrences (symbol, rel_path, line, is_def)\
                             VALUES (?, ?, ?, ?)",
                            rusqlite::params![
                                occ.symbol,
                                d.relative_path,
                                engine::ln(occ),
                                if occ.symbol_roles & 1 != 0 { 1 } else { 0 }
                            ],
                        )?;
                    }
                }
                if !kept_any && !d.occurrences.is_empty() {
                    docs_fully_filtered += 1;
                }
            }
            Ok(())
        })
        .and_then(|_| {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('head', ?1), ('schema', ?2),\
                 ('tool', 'code_reality.scip_refs')",
                rusqlite::params![sidecar_head, SCHEMA_VERSION],
            )?;
            conn.execute_batch("COMMIT")?;
            Ok(())
        })
        .map_err(|e: rusqlite::Error| e.to_string())?;
    drop(conn);
    std::fs::rename(&tmp, db_path).map_err(|e| e.to_string())?;
    Ok(Stats {
        symbols: tails.len(),
        occurrences: count,
        docs_fully_filtered,
    })
}

fn sidecar_head(index_path: &Path) -> String {
    stamped_head(index_path)
}

// rusqlite 0.40 removed Connection::query_map — helpers go through prepare().
// All three PROPAGATE errors: a corrupt-but-meta-intact db must surface, not
// silently answer empty (the exact "fake-empty" failure this tool cures).

fn query_strings(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params, |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn query_loc_rows(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<(String, i64)>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params, |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn query_text_pairs(conn: &Connection, sql: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect()
}

/// Five staleness signals (scip_refs.py:422-449): db older than index,
/// stat failure, corrupt db (not sqlite / no meta table), schema mismatch,
/// sidecar-head drift. A corrupt db counts as stale — rebuild cures it.
pub fn stale_reason(index_path: &Path, db_path: &Path) -> Option<String> {
    let (db_m, idx_m) = match (db_path.metadata(), index_path.metadata()) {
        (Ok(d), Ok(i)) => match (d.modified(), i.modified()) {
            (Ok(a), Ok(b)) => (a, b),
            // stat succeeded but mtime unavailable → stale (fail-loud, not fresh)
            (Err(e), _) | (_, Err(e)) => return Some(format!("stat 失敗：{}", e)),
        },
        (Err(e), _) | (_, Err(e)) => return Some(format!("stat 失敗：{}", e)),
    };
    if db_m < idx_m {
        return Some("db 比索引檔舊".to_string());
    }
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("db 損壞：{}", e));
    let conn = match conn {
        Ok(c) => c,
        Err(reason) => return Some(reason),
    };
    let meta_rows: BTreeMap<String, String> =
        match query_text_pairs(&conn, "SELECT key, value FROM meta") {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => return Some(format!("db 損壞：{}", e)),
        };
    if meta_rows.get("schema").map(String::as_str) != Some(SCHEMA_VERSION) {
        let got = meta_rows
            .get("schema")
            .cloned()
            .unwrap_or_else(|| "無".into());
        return Some(format!("schema 版本不符（{} ≠ {}）", got, SCHEMA_VERSION));
    }
    if meta_rows.get("head").cloned().unwrap_or_default() != sidecar_head(index_path) {
        return Some("sidecar head 變動（索引重生後重 stamp？）".to_string());
    }
    None
}

fn open_ro(db_path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())
}

/// Query face over either representation. SqliteFace semantics: SQL only
/// narrows candidates (`method = ?`); the predicates in `engine` are the
/// single semantic truth source; `ORDER BY seq` pins insertion = scan order.
/// Query errors (corrupt-but-meta-intact db) propagate as Err — the caller
/// decides (WARN + protobuf fallback), never a silent empty answer.
pub enum Face {
    Protobuf { index: Index },
    Sqlite(Connection),
}

/// Module-level audit-target attribution (`scip_refs.py:203-222`): DEF
/// occurrences → {symbol → (defining file, method name)}. Double-key
/// (file, method) attribution — file-only filtering unions in same-file
/// neighbor refs (216→138, 78 false positives, empirically).
pub fn audit_targets(
    index: &Index,
    files_by_name: &HashMap<String, BTreeSet<String>>,
) -> BTreeMap<String, (String, String)> {
    let mut out = BTreeMap::new();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 == 0 {
                continue;
            }
            let Some(name) = crate::engine::fn_tail_name(&occ.symbol) else {
                continue;
            };
            if let Some(paths) = files_by_name.get(name) {
                if paths.contains(d.relative_path.as_str()) {
                    out.insert(
                        occ.symbol.clone(),
                        (d.relative_path.clone(), name.to_string()),
                    );
                }
            }
        }
    }
    out
}

/// Face selection (open_face, scip_refs.py:456): no db → protobuf (never
/// build on miss); fresh → sqlite; stale → auto-rebuild; rebuild failure →
/// WARN + protobuf fallback (index parsed once, reused). open_ro failure on
/// the fresh path also falls back with a WARN (Python crashes uncaught there
/// — documented divergence: answering beats a traceback).
pub fn open_face(index_path: &Path) -> Result<(Face, Vec<String>), String> {
    let mut stderr: Vec<String> = Vec::new();
    let db_path = sqlite_path(index_path);
    if !db_path.exists() {
        let loaded = load_index(index_path)?;
        stderr.push(loaded.stderr);
        return Ok((
            Face::Protobuf {
                index: loaded.index,
            },
            stderr,
        ));
    }
    match stale_reason(index_path, &db_path) {
        None => match open_ro(&db_path) {
            Ok(conn) => Ok((Face::Sqlite(conn), stderr)),
            Err(e) => {
                stderr.push(format!(
                    "[WARN] 衍生 db 開啟失敗——本次查詢改走 protobuf 全量解析：{}\n",
                    e
                ));
                let loaded = load_index(index_path)?;
                stderr.push(loaded.stderr);
                Ok((
                    Face::Protobuf {
                        index: loaded.index,
                    },
                    stderr,
                ))
            }
        },
        Some(reason) => {
            stderr.push(format!("[WARN] 衍生 db 過期（{}）——自動重建\n", reason));
            let loaded = load_index(index_path)?; // parse once — feeds fallback too
            let head = sidecar_head(index_path);
            stderr.push(loaded.stderr); // <100-docs WARN prints on this path too
            let built = build_db(&loaded.index, &db_path, &head);
            let face = match (built, open_ro(&db_path)) {
                (Ok(_stats), Ok(conn)) => {
                    stderr.push("[OK] 衍生 db 重建完成\n".to_string());
                    Face::Sqlite(conn)
                }
                (build_res, open_res) => {
                    let e = build_res
                        .err()
                        .or(open_res.err())
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    stderr.push(format!(
                        "[WARN] 衍生 db 重建失敗——本次查詢改走 protobuf 全量解析：{}\n",
                        e
                    ));
                    Face::Protobuf {
                        index: loaded.index,
                    }
                }
            };
            Ok((face, stderr))
        }
    }
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// defs via a sqlite connection (SqliteFace.defs semantics, scip_refs.py:513):
/// SQL only narrows candidates; engine predicates are the single truth source.
/// Errors propagate (corrupt-but-meta-intact db must not answer empty).
pub fn sqlite_defs(
    conn: &Connection,
    query: &Query,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let method = match query {
        Query::TypeMethod { method, .. } => method.clone(),
        Query::Bare { name } => name.clone(),
    };
    let candidate_symbols: Vec<String> = if is_identifier(&method) {
        query_strings(
            conn,
            "SELECT symbol FROM symbol_tails WHERE method = ?",
            &[&method],
        )?
    } else {
        // Non-identifier query (dash etc.): method=? no longer a guaranteed
        // superset — full candidates, predicate filters.
        query_strings(conn, "SELECT symbol FROM symbol_tails", &[])?
    };
    let mut defs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for symbol in candidate_symbols {
        if !matches_query(&symbol, query) {
            continue;
        }
        let rows: Vec<(String, i64)> = query_loc_rows(
            conn,
            "SELECT rel_path, line FROM occurrences \
             WHERE symbol = ? AND is_def = 1 ORDER BY seq",
            &[&symbol],
        )?;
        if !rows.is_empty() {
            // ref-only symbols stay out of defs (protobuf same rule)
            defs.insert(
                symbol,
                rows.into_iter().map(|(p, l)| loc_line(&p, l)).collect(),
            );
        }
    }
    Ok(defs)
}

/// refs via a sqlite connection (SqliteFace.refs semantics, scip_refs.py:537).
pub fn sqlite_refs(
    conn: &Connection,
    symbols: &BTreeSet<String>,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut out: HashMap<String, Vec<String>> =
        symbols.iter().map(|s| (s.clone(), Vec::new())).collect();
    for symbol in symbols {
        let rows: Vec<(String, i64)> = query_loc_rows(
            conn,
            "SELECT rel_path, line FROM occurrences \
             WHERE symbol = ? AND is_def = 0 ORDER BY seq",
            &[&symbol],
        )?;
        out.insert(
            symbol.clone(),
            rows.into_iter().map(|(p, l)| loc_line(&p, l)).collect(),
        );
    }
    Ok(out)
}

/// Flat non-DEF rows (symbol, rel_path, line) in seq order — the global
/// scan order across symbols that caller first-site ordering depends on
/// (structured counterpart of sqlite_refs; R3 callers input). Error strings
/// are truncated: a failed `IN (...)` prepare embeds the whole generated
/// SQL (one placeholder per symbol — kilobytes at hub scale), which only
/// bloats the WARN line the fallback prints.
pub fn sqlite_refs_rows(
    conn: &Connection,
    symbols: &BTreeSet<String>,
) -> Result<Vec<(String, String, i64)>, String> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; symbols.len()].join(", ");
    let sql = format!(
        "SELECT symbol, rel_path, line FROM occurrences \
         WHERE symbol IN ({}) AND is_def = 0 ORDER BY seq",
        placeholders
    );
    let params: Vec<&dyn rusqlite::ToSql> =
        symbols.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| truncate_err(&e.to_string()))?;
    let rows = stmt
        .query_map(params.as_slice(), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| truncate_err(&e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| truncate_err(&e.to_string()))
}

/// Char-safe truncation for error strings that may embed generated SQL.
fn truncate_err(e: &str) -> String {
    match e.char_indices().nth(160) {
        Some((i, _)) => format!("{}…", &e[..i]),
        None => e.to_string(),
    }
}

impl Face {
    /// defs via the active face.
    pub fn defs(&self, query: &Query) -> Result<BTreeMap<String, Vec<String>>, String> {
        match self {
            Face::Protobuf { index } => Ok(engine::find_defs(index, query)),
            Face::Sqlite(conn) => sqlite_defs(conn, query),
        }
    }

    /// refs via the active face.
    pub fn refs(&self, symbols: &BTreeSet<String>) -> Result<HashMap<String, Vec<String>>, String> {
        match self {
            Face::Protobuf { index } => Ok(engine::find_refs(index, symbols)),
            Face::Sqlite(conn) => sqlite_refs(conn, symbols),
        }
    }

    /// Audit targets via the active face (`scip_refs.py:495/:550`). The
    /// sqlite path narrows with `symbol_tails.method IN (...)` + `ORDER BY
    /// seq` (insertion-order guarantee), then re-checks FN_TAIL and the
    /// double-key membership — SQL never grows its own matching semantics.
    pub fn audit_targets(
        &self,
        files_by_name: &HashMap<String, BTreeSet<String>>,
    ) -> Result<BTreeMap<String, (String, String)>, String> {
        match self {
            Face::Protobuf { index } => Ok(audit_targets(index, files_by_name)),
            Face::Sqlite(conn) => {
                let names: Vec<&String> = files_by_name.keys().collect();
                if names.is_empty() {
                    return Ok(BTreeMap::new());
                }
                let ph = vec!["?"; names.len()].join(",");
                let sql = format!(
                    "SELECT symbol, rel_path FROM occurrences \
                     WHERE is_def = 1 AND symbol IN \
                     (SELECT symbol FROM symbol_tails WHERE method IN ({ph})) \
                     ORDER BY seq"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| format!("audit_targets 查詢失敗：{}", e))?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(names.iter()), |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(|e| format!("audit_targets 查詢失敗：{}", e))?;
                let mut out = BTreeMap::new();
                for row in rows {
                    let (symbol, rel_path) =
                        row.map_err(|e| format!("audit_targets 讀取失敗：{}", e))?;
                    let Some(name) = fn_tail_name(&symbol).map(str::to_string) else {
                        continue; // re-check — meta-table data ≠ semantic source
                    };
                    if let Some(paths) = files_by_name.get(&name) {
                        if paths.contains(rel_path.as_str()) {
                            out.insert(symbol, (rel_path, name));
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    /// Flat structured non-DEF rows via the active face, global scan order
    /// (R3 callers input; protobuf scan order = sqlite seq insertion order).
    pub fn refs_rows(
        &self,
        symbols: &BTreeSet<String>,
    ) -> Result<Vec<(String, String, i64)>, String> {
        match self {
            Face::Protobuf { index } => Ok(engine::refs_rows(index, symbols)),
            Face::Sqlite(conn) => sqlite_refs_rows(conn, symbols),
        }
    }
}
