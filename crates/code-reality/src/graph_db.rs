//! `graph_db` — v1+ S4: the self-owned graph database face
//! (`<repo>/.code-reality/graph.db`).
//!
//! Schema ownership: the node key is the PRODUCER symbol (the native
//! SCIP/LSP string) — the query-time double-key join and its collision
//! class disappear. Edges live in a single ontology, one row per call
//! site, materialized at build time (zero query-side joins).
//!
//! Attribution is producer-conditional (data-face difference, not a style
//! mix): the SCIP face carries fn spans → spans-based innermost
//! containment (`callers::attribute`, the scip_edges inject face — the
//! sidecar-era algorithm, an S4 layer-2 prerequisite). The LSP-harvest
//! cache has line-level occurrences only (no end_line → no spans, and
//! its `index.scip` is a placeholder the protobuf ladder cannot parse)
//! → the bootstrap nearest-preceding-def rule stays THE algorithm there.
//!
//! Write safety: the db is built at a temp sibling and atomically renamed
//! — a crash never leaves a half db blocking the rebuild (the bootstrap
//! `graph.exists()` guard is retired with it).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::cache::{self, Face};
use crate::callers;
use crate::common::{resolve, to_json_indent1};
use crate::engine::{fn_tail_name, ln};
use crate::fndefs;
use crate::py_calls;
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The self-owned db home: `<repo>/.code-reality/graph.db`.
pub fn db_path(repo_root: &Path) -> PathBuf {
    resolve(repo_root).join(".code-reality").join("graph.db")
}

#[derive(Debug)]
pub struct BuildReport {
    pub nodes: usize,
    pub edges: usize,
    /// Edges derived as CALLS by the build-side syntactic split
    /// (occurrence EP S3-F2); the remainder are REFERENCES.
    pub calls_edges: usize,
    pub item_level_refs: usize,
    pub self_ref_skipped: usize,
    pub non_fn_defs_skipped: usize,
    pub external_skipped: usize,
    pub flows: usize,
    pub communities: usize,
    pub db: PathBuf,
}

#[derive(Debug)]
pub struct DerivedReport {
    pub flows: usize,
    pub communities: usize,
}

/// Post-build materialization of the derived tables: `detect_changes`
/// hard-joins flows/flow_memberships (prepare failure = error) and
/// `get_minimal_context` soft-reads communities — with the ops-untouched
/// constraint, the build face is the only writer. Runs the engine ops
/// over the fresh db and writes flows/communities back, plus the
/// nodes.community_id column.
pub fn materialize_derived(repo: &Path) -> Result<DerivedReport, String> {
    let db = db_path(repo);
    if !db.exists() {
        return Err(format!(
            "graph.db 不在：{}（先 graph_db build）",
            db.display()
        ));
    }
    let mut conn = Connection::open(&db).map_err(|e| format!("graph.db 開啟失敗：{e}"))?;
    let flows = crate::graph_engine::trace_flows(&conn, 15, false)?;
    let communities = crate::graph_engine::detect_communities(&conn, 2)?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("materialize 交易開啟失敗：{e}"))?;
    tx.execute_batch(
        "DELETE FROM flows; DELETE FROM flow_memberships; \
                      DELETE FROM communities; UPDATE nodes SET community_id = NULL;",
    )
    .map_err(|e| format!("materialize 清表失敗：{e}"))?;
    {
        // prepared once, reused per row (NT scale: ~150K membership rows)
        let mut ins_flow = tx
            .prepare(
                "INSERT INTO flows (id, name, entry_point_id, depth, node_count, \
                 file_count, criticality, path_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(|e| format!("flows 準備失敗：{e}"))?;
        let mut ins_member = tx
            .prepare(
                "INSERT OR REPLACE INTO flow_memberships (flow_id, node_id, position) \
                 VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("flow_memberships 準備失敗：{e}"))?;
        for (i, f) in flows.iter().enumerate() {
            let fid = (i + 1) as i64;
            ins_flow
                .execute(rusqlite::params![
                    fid,
                    f["name"].as_str().unwrap_or(""),
                    f["entry_point_id"].as_i64().unwrap_or(0),
                    f["depth"].as_i64().unwrap_or(0),
                    f["node_count"].as_i64().unwrap_or(0),
                    f["file_count"].as_i64().unwrap_or(0),
                    f["criticality"].as_f64().unwrap_or(0.0),
                    serde_json::to_string(&f["path"]).unwrap_or_else(|_| "[]".into()),
                ])
                .map_err(|e| format!("flows 寫入失敗：{e}"))?;
            if let Some(path) = f["path"].as_array() {
                for (pos, nid) in path.iter().enumerate() {
                    ins_member
                        .execute(rusqlite::params![
                            fid,
                            nid.as_i64().unwrap_or(0),
                            pos as i64
                        ])
                        .map_err(|e| format!("flow_memberships 寫入失敗：{e}"))?;
                }
            }
        }
    }
    for (i, c) in communities.iter().enumerate() {
        let cid = (i + 1) as i64;
        tx.execute(
            "INSERT INTO communities (id, name, level, size, cohesion, \
             dominant_language, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                cid,
                c["name"].as_str().unwrap_or(""),
                c["level"].as_i64().unwrap_or(0),
                c["size"].as_i64().unwrap_or(0),
                c["cohesion"].as_f64().unwrap_or(0.0),
                c["dominant_language"].as_str().unwrap_or(""),
                c["description"].as_str().unwrap_or(""),
            ],
        )
        .map_err(|e| format!("communities 寫入失敗：{e}"))?;
        if let Some(members) = c["members"].as_array() {
            for m in members {
                tx.execute(
                    "UPDATE nodes SET community_id = ?1 WHERE symbol = ?2",
                    rusqlite::params![cid, m.as_str().unwrap_or("")],
                )
                .map_err(|e| format!("community_id 回寫失敗：{e}"))?;
            }
        }
    }
    tx.commit()
        .map_err(|e| format!("materialize 提交失敗：{e}"))?;
    Ok(DerivedReport {
        flows: flows.len(),
        communities: communities.len(),
    })
}

const DDL: &str = "
CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    qname TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line_start INTEGER,
    line_end INTEGER,
    language TEXT,
    parent_name TEXT,
    is_test INTEGER DEFAULT 0,
    extra TEXT DEFAULT '{}',
    updated_at REAL NOT NULL,
    community_id INTEGER,
    provenance TEXT NOT NULL);
CREATE TABLE edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    caller_symbol TEXT NOT NULL,
    callee_symbol TEXT NOT NULL,
    provenance TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line INTEGER DEFAULT 0,
    confidence REAL DEFAULT 1.0,
    confidence_tier TEXT DEFAULT 'EXTRACTED',
    updated_at REAL NOT NULL);
CREATE TABLE communities (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    level INTEGER NOT NULL DEFAULT 0,
    parent_id INTEGER,
    cohesion REAL DEFAULT 0.0,
    size INTEGER DEFAULT 0,
    dominant_language TEXT,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT 'graph_db build');
CREATE TABLE flows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    entry_point_id INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    node_count INTEGER NOT NULL,
    file_count INTEGER NOT NULL,
    criticality REAL NOT NULL DEFAULT 0.0,
    path_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT 'graph_db build',
    updated_at TEXT NOT NULL DEFAULT 'graph_db build');
CREATE TABLE flow_memberships (
    flow_id INTEGER NOT NULL,
    node_id INTEGER NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (flow_id, node_id));
CREATE VIRTUAL TABLE nodes_fts USING fts5(name, content='nodes', content_rowid='id');
PRAGMA user_version = 1;";

/// Test-entry face for `is_test` on producer nodes: Rust `tests/`
/// directories plus the Python `test_` prefix conventions (the
/// engine-side `is_test_file` regex does not cover Rust — EP finding 1).
pub fn is_test_rel(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    p.starts_with("tests/")
        || p.contains("/tests/")
        || p.starts_with("test_")
        || p.contains("/test_")
}

/// LSP-harvest synthesizes `lsp python <rel> [L<line>] <name>().`
/// symbols — the `lsp ` prefix is the language discriminator, the
/// optional `L<line>` middle segment disambiguates same-file same-name
/// defs (both shapes parse). scip-python emits
/// `scip-python python <project> <version> \`symbol\`...` and
/// pyrefly-producer mirrors that shape with a `pyrefly ` discriminator
/// — both leading Python discriminators (F1). Everything else is the
/// rust-analyzer SCIP face.
fn infer_language(symbol: &str) -> &'static str {
    if symbol.starts_with("lsp ")
        || symbol.starts_with("scip-python ")
        || symbol.starts_with("pyrefly ")
    {
        "Python"
    } else {
        "Rust"
    }
}

/// Own-class segment of a method-shaped symbol (`…\`mod\`/Class#m().`
/// → `Class`); None for non-method symbols.
fn class_segment(symbol: &str) -> Option<&str> {
    let hash = symbol.rfind('#')?;
    let head = &symbol[..hash];
    let start = head.rfind('/').map(|i| i + 1).unwrap_or(0);
    let name = &head[start..];
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn producer_of(face: &Face) -> String {
    match face {
        Face::Sqlite(conn) => {
            let p: Option<String> = conn
                .query_row("SELECT value FROM meta WHERE key = 'producer'", [], |r| {
                    r.get(0)
                })
                .ok();
            match p {
                Some(s) if s.starts_with("lsp") => "lsp-harvest".to_string(),
                _ => "scip".to_string(),
            }
        }
        Face::Protobuf { .. } => "scip".to_string(),
    }
}

struct ScanRows {
    /// (symbol, rel_path, line)
    defs: Vec<(String, String, i64)>,
    /// (symbol, rel_path, line) in scan order
    refs: Vec<(String, String, i64)>,
    /// non-fn-tail occurrences skipped (protobuf arm parity — counted,
    /// never silently dropped)
    non_fn_skipped: usize,
}

fn scan(face: &Face) -> Result<ScanRows, String> {
    match face {
        Face::Sqlite(conn) => {
            let mut rows = ScanRows {
                defs: Vec::new(),
                refs: Vec::new(),
                non_fn_skipped: 0,
            };
            let mut stmt = conn
                .prepare("SELECT symbol, rel_path, line, is_def FROM occurrences ORDER BY seq")
                .map_err(|e| format!("graph_db 掃描失敗：{e}"))?;
            let it = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|e| format!("graph_db 掃描失敗：{e}"))?;
            for row in it {
                let (s, f, l, d) = row.map_err(|e| format!("graph_db 讀取失敗：{e}"))?;
                if fn_tail_name(&s).is_none() {
                    rows.non_fn_skipped += 1; // symmetric with the protobuf arm
                    continue;
                }
                if d != 0 {
                    rows.defs.push((s, f, l));
                } else {
                    rows.refs.push((s, f, l));
                }
            }
            Ok(rows)
        }
        Face::Protobuf { index } => {
            let mut rows = ScanRows {
                defs: Vec::new(),
                refs: Vec::new(),
                non_fn_skipped: 0,
            };
            for d in &index.documents {
                for occ in &d.occurrences {
                    if fn_tail_name(&occ.symbol).is_none() {
                        rows.non_fn_skipped += 1;
                        continue;
                    }
                    let l = ln(occ);
                    if occ.symbol_roles & 1 != 0 {
                        rows.defs
                            .push((occ.symbol.clone(), d.relative_path.clone(), l));
                    } else {
                        rows.refs
                            .push((occ.symbol.clone(), d.relative_path.clone(), l));
                    }
                }
            }
            Ok(rows)
        }
    }
}

/// Stamp `git_head_sha` + `last_updated` into metadata (S3 cutover):
/// the snapshot staleness face consumes these keys. Git failure skips
/// stamping silently — staleness then falls back to db mtime.
fn stamp_snapshot_metadata(conn: &Connection, repo: &Path) {
    if let Ok(sha) = crate::common::git_rev_parse_head(repo) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('git_head_sha', ?1)",
            [&sha],
        );
    }
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        let iso = crate::common::local_epoch_to_iso_auto(now.as_secs() as i64, now.subsec_nanos());
        let _ = conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('last_updated', ?1)",
            [iso],
        );
    }
}

/// Staleness WARN face for graph.db consumers: the build-time
/// `git_head_sha` stamp vs the repo's current HEAD — the same
/// `[SRC]`-style drift guard scip_refs carries for its index. None when
/// the db carries no stamp (legacy build) or the repo has no HEAD.
pub fn stale_head_warn(db_path: &Path, repo_root: &Path) -> Option<String> {
    let conn = crate::common::connect_ro(db_path).ok()?;
    let stamped: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'git_head_sha'",
            [],
            |r| r.get(0),
        )
        .ok()?;
    let head = crate::common::git_rev_parse_head(repo_root).ok()?;
    (stamped != head).then(|| {
        format!(
            "[WARN] graph.db 建於 {stamped}，repo HEAD 已前移至 {head}——查詢結果可能過期，重跑 `code-reality graph_db build --repo <repo>`"
        )
    })
}

fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// One attributed site edge: (caller_symbol, callee_symbol, rel_path, line).
type SiteEdge = (String, String, String, i64);

/// Nearest-preceding-def attribution (the bootstrap rule — the LSP face
/// has no spans). Every ref site becomes one edge row; refs with no
/// preceding def count as item-level, self-refs (ref attributed to the
/// callee's own def) count separately — never silently dropped.
fn attribute_nearest(
    defs: &[(String, String, i64)],
    refs: &[(String, String, i64)],
) -> (Vec<SiteEdge>, usize, usize) {
    let mut by_file: BTreeMap<&str, Vec<(i64, &str)>> = BTreeMap::new();
    for (sym, rel, line) in defs {
        by_file
            .entry(rel.as_str())
            .or_default()
            .push((*line, sym.as_str()));
    }
    for v in by_file.values_mut() {
        v.sort();
    }
    let mut out = Vec::new();
    let mut item_level = 0usize;
    let mut self_refs = 0usize;
    for (symbol, rel, line) in refs {
        let caller = by_file
            .get(rel.as_str())
            .and_then(|v| v.iter().rev().find(|(dl, _)| *dl <= *line))
            .map(|(_, s)| s.to_string());
        match caller {
            Some(c) if &c != symbol => {
                out.push((c, symbol.clone(), rel.clone(), *line));
            }
            Some(_) => self_refs += 1,
            None => item_level += 1,
        }
    }
    (out, item_level, self_refs)
}

/// Build the self-owned db from any producer cache (rust-analyzer SCIP,
/// the pyrefly producer, or the LSP-harvest golden face — one schema, one
/// edge ontology). Core over an explicit index path; the CLI face
/// resolves the in-repo slot.
pub fn build_from_cache_at(repo: &Path, index_path: &Path) -> Result<BuildReport, String> {
    let cache_db = cache::sqlite_path(index_path);
    if !cache_db.exists() && !index_path.exists() {
        return Err(format!(
            "cache 不在：{}（rust-analyzer 走 scip_refs --build-cache；Python 走 pyrefly producer）",
            cache_db.display()
        ));
    }
    let repo_abs = resolve(repo);
    // LSP-harvest slots carry a placeholder index.scip (no protobuf face):
    // an existing cache with an lsp producer IS the authoritative face —
    // the open_face ladder would try parsing the placeholder on sub-second
    // mtime drift (bootstrap connected directly for the same reason).
    // One contradiction still fails loud (silent bad-db relay,
    // 2026-08-28): an index.scip NEWER than the lsp cache means a fresh
    // producer run has already superseded this sidecar — trusting it
    // silently built a CALLS-0 db off a stale lsp-harvest cache.
    let face = if cache_db.exists() {
        let probe = crate::common::connect_ro(&cache_db)?;
        let is_lsp: bool = probe
            .query_row("SELECT value FROM meta WHERE key = 'producer'", [], |r| {
                r.get::<_, String>(0)
            })
            .map(|p| p.starts_with("lsp"))
            .unwrap_or(false);
        if is_lsp {
            let cache_m = cache_db
                .metadata()
                .and_then(|m| m.modified())
                .map_err(|e| format!("stat {}: {e}", cache_db.display()))?;
            let idx_m = index_path
                .metadata()
                .and_then(|m| m.modified())
                .map_err(|e| format!("stat {}: {e}", index_path.display()))?;
            if cache_m < idx_m {
                return Err(format!(
                    "lsp-harvest 快取已被較新的 index.scip 取代（{} 舊於 {}）——殘留 sidecar 不可作為生產面：重跑 pyrefly-index（自動失效殘留 sidecar）再 build",
                    cache_db.display(),
                    index_path.display()
                ));
            }
            Face::Sqlite(probe)
        } else {
            drop(probe);
            cache::open_face(index_path)?.0
        }
    } else {
        cache::open_face(index_path)?.0
    };
    let producer = producer_of(&face);
    let rows = scan(&face)?;
    // CALLS-vs-REFERENCES split (occurrence EP S3-F2, build-side
    // mechanism): SCIP has no call role and neither producer marks one —
    // the build site holds repo root + sources, so it re-derives call
    // positions syntactically. The lsp-harvest golden face stays
    // REFERENCES-only (documented; its CALLS story is out of scope).
    let (call_marks, _call_warns) = if producer == "lsp-harvest" {
        (py_calls::CallSiteSet::default(), Vec::new())
    } else {
        // .py gate: rust-analyzer faces share the "scip" producer class —
        // without it every Rust repo build would feed .rs sources to the
        // Python parser (WARN spam + pointless full-tree I/O).
        let rels: std::collections::BTreeSet<String> = rows
            .refs
            .iter()
            .map(|(_, r, _)| r.clone())
            .filter(|r| r.ends_with(".py"))
            .collect();
        let (marks, warns) = py_calls::call_sites(&repo_abs, &rels);
        for w in &warns {
            eprintln!("{w}");
        }
        (marks, warns)
    };
    // attribution: (caller, callee, rel, line) site rows
    let def_symbols: BTreeSet<String> = rows.defs.iter().map(|(s, _, _)| s.clone()).collect();
    let (edge_rows, item_level, self_refs): (Vec<SiteEdge>, usize, usize) =
        if producer == "lsp-harvest" {
            attribute_nearest(&rows.defs, &rows.refs)
        } else {
            // spans ladder (the Sqlite arm re-parses the protobuf when the
            // fndefs sidecar is cold — accepted cost, same as scip_edges)
            let spans_result = match &face {
                Face::Protobuf { index } => fndefs::spans_source(index_path, Some(index)),
                Face::Sqlite(_) => fndefs::spans_source(index_path, None),
            };
            let (spans, _span_warns) = spans_result?;
            let mut by_callee: BTreeMap<String, Vec<(String, String, i64)>> = BTreeMap::new();
            for (sym, rel, line) in &rows.refs {
                by_callee
                    .entry(sym.clone())
                    .or_default()
                    .push((sym.clone(), rel.clone(), *line));
            }
            let mut out = Vec::new();
            let mut item = 0usize;
            let mut selfs = 0usize;
            for (callee, group) in &by_callee {
                let res = callers::attribute(group, &spans);
                item += res.item_level.len();
                for c in &res.callers {
                    for (rel, line) in &c.sites {
                        if c.symbol == *callee {
                            selfs += 1;
                            continue;
                        }
                        out.push((c.symbol.clone(), callee.clone(), rel.clone(), *line));
                    }
                }
            }
            (out, item, selfs)
        };
    let external_skipped = edge_rows
        .iter()
        .filter(|(_, callee, _, _)| !def_symbols.contains(callee))
        .count();

    // write: temp sibling + atomic rename
    let db = db_path(repo);
    let dir = db.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("目錄建立失敗（{}）：{e}", dir.display()))?;
    crate::engine::write_data_dir_gitignore(dir)
        .map_err(|e| format!("graph_db 資料目錄自帶 ignore 失敗：{e}"))?;
    let tmp_db = dir.join("graph.db.tmp-build");
    if let Err(e) = std::fs::remove_file(&tmp_db) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "[WARN] 舊 build 暫存檔清除失敗（{}）：{e}",
                tmp_db.display()
            );
        }
    }
    let mut g = Connection::open(&tmp_db).map_err(|e| format!("graph.db 建立失敗：{e}"))?;
    g.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(|e| format!("graph.db busy_timeout 設定失敗：{e}"))?;
    g.execute_batch(DDL)
        .map_err(|e| format!("graph.db schema 建立失敗：{e}"))?;
    // single index source: build and ensure_indexes both apply
    // ENGINE_INDEX_DDL (schema DDL carries tables only — cannot drift)
    g.execute_batch(ENGINE_INDEX_DDL)
        .map_err(|e| format!("graph.db 索引建立失敗：{e}"))?;
    let now = now_ts();
    let tx = g
        .transaction()
        .map_err(|e| format!("graph.db 交易開啟失敗：{e}"))?;
    let mut nodes = 0usize;
    {
        // (rel_path, line) insertion order — bootstrap rowid parity
        let mut ordered: Vec<&(String, String, i64)> = rows.defs.iter().collect();
        ordered.sort_by(|a, b| (&a.1, &a.2).cmp(&(&b.1, &b.2)));
        for (symbol, rel, line) in ordered {
            let Some(name) = fn_tail_name(symbol) else {
                continue;
            };
            let file_abs = resolve(&repo_abs.join(rel));
            let file_s = file_abs.display().to_string();
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO nodes \
                     (symbol, kind, name, qname, file_path, line_start, line_end, \
                      language, extra, updated_at, is_test, provenance) \
                     VALUES (?1, 'Function', ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        symbol,
                        name,
                        format!("{file_s}::{name}"),
                        file_s,
                        line,
                        infer_language(symbol),
                        "{\"producer\":\"graph_db\"}",
                        now,
                        is_test_rel(rel) as i64,
                        producer
                    ],
                )
                .map_err(|e| format!("節點寫入失敗：{e}"))?;
            nodes += n;
        }
    }
    let mut edges = 0usize;
    let mut calls_edges = 0usize;
    // Construct immunity against producer-side occurrence duplication:
    // the edge fact is the natural key (kind, caller, callee, file,
    // line) — a repeated occurrence is the same edge, counted once.
    // NT cold-start relay 2026-08-29 saw exactly-2x edge rows once
    // (nodes constant, reruns converged; producers since proven
    // byte-deterministic — unreproducible transient, deduped here
    // regardless of cause).
    let mut seen_edges = std::collections::HashSet::new();
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO edges \
                 (kind, caller_symbol, callee_symbol, provenance, file_path, line, \
                  confidence, confidence_tier, updated_at) \
                 VALUES (?7, ?1, ?2, ?3, ?4, ?5, 1.0, 'EXTRACTED', ?6)",
            )
            .map_err(|e| format!("邊寫入準備失敗：{e}"))?;
        for (caller, callee, rel, line) in &edge_rows {
            if !def_symbols.contains(callee) {
                continue;
            }
            // S3-F2: a reference row at a (file, line) that carries a
            // call to the symbol's fn tail is a CALLS edge; the rest stay
            // REFERENCES. Dunder-collapsed constructor edges carry tail
            // `__init__` while the syntactic callee is the CLASS name —
            // fall back to the symbol's own-class segment before `#`.
            let tail_match = fn_tail_name(callee)
                .is_some_and(|t| call_marks.contains(&(rel.clone(), *line, t.to_string())));
            let class_match = class_segment(callee)
                .is_some_and(|c| call_marks.contains(&(rel.clone(), *line, c.to_string())));
            let is_calls = tail_match || class_match;
            let kind = if is_calls { "CALLS" } else { "REFERENCES" };
            let file_abs = resolve(&repo_abs.join(rel));
            if !seen_edges.insert((
                kind,
                caller.clone(),
                callee.clone(),
                file_abs.display().to_string(),
                *line,
            )) {
                continue;
            }
            if is_calls {
                calls_edges += 1;
            }
            stmt.execute(rusqlite::params![
                caller,
                callee,
                producer,
                file_abs.display().to_string(),
                line,
                now,
                kind
            ])
            .map_err(|e| format!("邊寫入失敗：{e}"))?;
            edges += 1;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES ('producer', ?1)",
        [&producer],
    )
    .map_err(|e| format!("metadata 寫入失敗：{e}"))?;
    stamp_snapshot_metadata(&tx, repo);
    tx.execute("INSERT INTO nodes_fts(nodes_fts) VALUES ('rebuild')", [])
        .map_err(|e| format!("nodes_fts 重建失敗：{e}"))?;
    tx.commit().map_err(|e| format!("graph.db 提交失敗：{e}"))?;
    drop(g);
    std::fs::rename(&tmp_db, &db).map_err(|e| {
        format!(
            "graph.db 原子替換失敗（{} → {}）：{e}",
            tmp_db.display(),
            db.display()
        )
    })?;
    // derived tables (detect_changes hard-joins them — the build face is
    // the writer under the ops-untouched constraint)
    let derived = materialize_derived(repo)?;
    Ok(BuildReport {
        nodes,
        edges,
        calls_edges,
        item_level_refs: item_level,
        self_ref_skipped: self_refs,
        non_fn_defs_skipped: rows.non_fn_skipped,
        external_skipped,
        flows: derived.flows,
        communities: derived.communities,
        db,
    })
}

/// CLI face: resolves the in-repo default slot.
pub fn build_from_cache(repo: &Path) -> Result<BuildReport, String> {
    let index = crate::engine::default_index_path(repo)?;
    build_from_cache_at(repo, &index)
}

/// Indexes the engine read chain filters on (edges by either endpoint +
/// kind, flow_memberships by node) — shared by build-time DDL and
/// `ensure_indexes` for dbs built before this schema revision.
const ENGINE_INDEX_DDL: &str = "
CREATE INDEX IF NOT EXISTS idx_edges_caller ON edges(caller_symbol, kind);
CREATE INDEX IF NOT EXISTS idx_edges_callee ON edges(callee_symbol, kind);
CREATE INDEX IF NOT EXISTS idx_flow_memberships_node ON flow_memberships(node_id);
CREATE INDEX IF NOT EXISTS idx_nodes_name_file_line ON nodes(name, file_path, line_start);";

pub struct EnsureIndexesReport {
    pub db: PathBuf,
    pub created: usize,
    pub skipped: usize,
}

/// `CREATE INDEX IF NOT EXISTS` on the self-owned db (engine read-chain
/// indexes — for dbs built before this schema revision; fresh builds get
/// them via DDL). Idempotent; index-only — no row data is touched, query
/// results are unchanged. The legacy-db anchor index half was removed
/// with the legacy consumer cutover (chain_tour reads the self-owned db;
/// the same covering index lives in ENGINE_INDEX_DDL now).
pub fn ensure_indexes(repo: &Path) -> Result<EnsureIndexesReport, String> {
    let mut created = 0usize;
    let mut skipped = 0usize;
    let db = db_path(repo);
    if !db.exists() {
        return Err(format!(
            "新庫不在：{}——先 `graph_db build --repo` 再 ensure_indexes",
            db.display()
        ));
    }
    let conn =
        Connection::open(&db).map_err(|e| format!("{} 開啟失敗（rw）：{e}", db.display()))?;
    count_index_ddl(&conn, ENGINE_INDEX_DDL, &mut created, &mut skipped)?;
    Ok(EnsureIndexesReport {
        db,
        created,
        skipped,
    })
}

/// Default db resolution for the graph-reading consumers: the self-owned
/// db when present, plus a fail-soft warn when the db is missing.
pub fn consumer_db(repo: &Path) -> (Option<PathBuf>, Vec<String>) {
    let db = db_path(repo);
    let mut warns = Vec::new();
    if !db.exists() {
        warns.push(format!(
            "[WARN] .code-reality/graph.db 不存在（{}）——先 `code-reality graph_db build --repo <repo>`\n",
            db.display()
        ));
        return (None, warns);
    }
    (Some(db), warns)
}

fn count_index_ddl(
    conn: &Connection,
    ddl: &str,
    created: &mut usize,
    skipped: &mut usize,
) -> Result<(), String> {
    for stmt in ddl.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND sql IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("sqlite_master 計數失敗：{e}"))?;
        conn.execute_batch(stmt)
            .map_err(|e| format!("索引建立失敗（{stmt}）：{e}"))?;
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND sql IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("sqlite_master 計數失敗：{e}"))?;
        if after > before {
            *created += 1;
        } else {
            *skipped += 1;
        }
    }
    Ok(())
}

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: graph_db [-h] <build|ensure_indexes> --repo REPO [--json]\n",
    "\n",
    "自有格式 graph.db 面（v1+ S4；.code-reality/graph.db——producer symbol\n",
    "鍵＋單一邊本體，取代 CRG 相容格式的角色）。\n",
    "\n",
    "ops:\n",
    "  build            cache index（任何 producer）→ nodes+edges+derived 物化\n",
    "  ensure_indexes   引擎讀鏈索引（edges 端點/flow node/nodes anchor）\n",
    "                   IF NOT EXISTS，冪等，不動資料——補舊版 build 的庫\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo root（解析 in-repo 預設 index slot）\n",
    "  --json                報告 JSON 面\n",
);

pub fn run(argv: &[&str]) -> ToolOutput {
    // argv = ["graph_db", <op>, flags...] — the umbrella passes the tool
    // name through (graph_engine::run's positional-op pattern)
    let Some((&_tool, rest)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 graph_db <build>");
    };
    let Some((op, toks)) = rest.split_first() else {
        return ToolOutput::fail("需提供操作（graph_db build / ensure_indexes）");
    };
    if op == &"-h" || op == &"--help" {
        return ToolOutput {
            stdout: HELP.to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let values = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            }
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok {
            values,
            positionals,
        } => {
            if !positionals.is_empty() {
                return ToolOutput::fail(format!("無法辨認的位置參數：{}", positionals[0]));
            }
            values
        }
    };
    if op != &"build" && op != &"ensure_indexes" {
        return ToolOutput::fail(format!("未知操作：{op}（支援 build / ensure_indexes）"));
    }
    let Some(repo_s) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("需 --repo");
    };
    let json = values.contains_key("--json");
    if op == &"ensure_indexes" {
        return match ensure_indexes(Path::new(&repo_s)) {
            Ok(rep) => {
                if json {
                    let v = serde_json::json!({
                        "created": rep.created,
                        "skipped": rep.skipped,
                        "db": rep.db.display().to_string(),
                    });
                    ToolOutput {
                        stdout: format!("{}\n", to_json_indent1(&v)),
                        stderr: String::new(),
                        exit_code: 0,
                    }
                } else {
                    ToolOutput {
                        stdout: format!(
                            "[OK] graph_db ensure_indexes：created={} skipped={} → {}\n",
                            rep.created,
                            rep.skipped,
                            rep.db.display()
                        ),
                        stderr: String::new(),
                        exit_code: 0,
                    }
                }
            }
            Err(e) => ToolOutput::crash(e),
        };
    }
    match build_from_cache(Path::new(&repo_s)) {
        Ok(rep) => {
            if json {
                let v = serde_json::json!({
                    "nodes": rep.nodes,
                    "edges": rep.edges,
                    "calls_edges": rep.calls_edges,
                    "item_level_refs": rep.item_level_refs,
                    "self_ref_skipped": rep.self_ref_skipped,
                    "non_fn_defs_skipped": rep.non_fn_defs_skipped,
                    "external_skipped": rep.external_skipped,
                    "flows": rep.flows,
                    "communities": rep.communities,
                    "db": rep.db.display().to_string(),
                });
                ToolOutput {
                    stdout: format!("{}\n", to_json_indent1(&v)),
                    stderr: String::new(),
                    exit_code: 0,
                }
            } else {
                ToolOutput {
                    stdout: format!(
                        "[OK] graph_db build：{} 節點 / {} 邊（CALLS {} / REFERENCES {}） / item-level refs 略過 {} / self-ref 略過 {} / external 略過 {} → {}\n",
                        rep.nodes,
                        rep.edges,
                        rep.calls_edges,
                        rep.edges - rep.calls_edges,
                        rep.item_level_refs,
                        rep.self_ref_skipped,
                        rep.external_skipped,
                        rep.db.display()
                    ),
                    stderr: String::new(),
                    exit_code: 0,
                }
            }
        }
        Err(e) => ToolOutput::crash(e),
    }
}

#[cfg(test)]
mod tests {
    use super::infer_language;

    #[test]
    fn python_prefixes_cover_all_python_producers() {
        // lsp-harvest synthesized shape
        assert_eq!(infer_language("lsp python src/a.py target()."), "Python");
        // lsp-harvest line-disambiguated shape
        assert_eq!(
            infer_language("lsp python src/a.py L10 target()."),
            "Python"
        );
        // scip-python emitted shape (F1: previously fell through to Rust)
        assert_eq!(
            infer_language("scip-python python proj 0.1.0 `pkg.mod`/fn()."),
            "Python"
        );
        assert_eq!(
            infer_language("scip-python python proj 0.1.0 `pkg.mod`/Class#method()."),
            "Python"
        );
        // pyrefly-producer mirrored shape (ep-pyrefly-native-producer S1)
        assert_eq!(
            infer_language("pyrefly python proj 0.1.0 `pkg.mod`/fn()."),
            "Python"
        );
        assert_eq!(
            infer_language("pyrefly python proj 0.1.0 `pkg.mod`/Class#method()."),
            "Python"
        );
        // rust-analyzer SCIP face stays Rust
        assert_eq!(infer_language("file:///repo/src/lib.rs/`main`"), "Rust");
    }
}
