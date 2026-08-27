//! `graph_db` — v1+ S4: the self-owned graph database face
//! (`<repo>/.code-reality/graph.db`, replacing the CRG-compatible
//! `.code-review-graph/` role for the engine read chain).
//!
//! Schema ownership: the node key is the PRODUCER symbol (the native
//! SCIP/LSP string; `import_legacy` mints qname-keyed nodes for the
//! tree-sitter era) — the query-time double-key join and its collision
//! class disappear. Edges live in a single ontology, one row per call
//! site: SCIP REFERENCES, LSP-harvest, and legacy tree-sitter rows
//! cohabitate, materialized at build time (zero query-side joins).
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
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
/// defs (both shapes parse); everything else is the rust-analyzer SCIP
/// face.
fn infer_language(symbol: &str) -> &'static str {
    if symbol.starts_with("lsp ") {
        "Python"
    } else {
        "Rust"
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

/// Build the self-owned db from any producer cache (rust-analyzer SCIP or
/// the LSP-harvest adapter — one schema, one edge ontology). Core over an
/// explicit index path; the CLI face resolves the repo-keyed slot.
pub fn build_from_cache_at(repo: &Path, index_path: &Path) -> Result<BuildReport, String> {
    let cache_db = cache::sqlite_path(index_path);
    if !cache_db.exists() && !index_path.exists() {
        return Err(format!(
            "cache 不在：{}（rust-analyzer 走 scip_refs --build-cache；Python 走 LSP-harvest adapter）",
            cache_db.display()
        ));
    }
    let repo_abs = resolve(repo);
    // LSP-harvest slots carry a placeholder index.scip (no protobuf face):
    // an existing cache with an lsp producer IS the authoritative face —
    // the open_face ladder would try parsing the placeholder on sub-second
    // mtime drift (bootstrap connected directly for the same reason).
    let face = if cache_db.exists() {
        let probe = crate::common::connect_ro(&cache_db)?;
        let is_lsp: bool = probe
            .query_row("SELECT value FROM meta WHERE key = 'producer'", [], |r| {
                r.get::<_, String>(0)
            })
            .map(|p| p.starts_with("lsp"))
            .unwrap_or(false);
        if is_lsp {
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
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO edges \
                 (kind, caller_symbol, callee_symbol, provenance, file_path, line, \
                  confidence, confidence_tier, updated_at) \
                 VALUES ('REFERENCES', ?1, ?2, ?3, ?4, ?5, 1.0, 'EXTRACTED', ?6)",
            )
            .map_err(|e| format!("邊寫入準備失敗：{e}"))?;
        for (caller, callee, rel, line) in &edge_rows {
            if !def_symbols.contains(callee) {
                continue;
            }
            let file_abs = resolve(&repo_abs.join(rel));
            stmt.execute(rusqlite::params![
                caller,
                callee,
                producer,
                file_abs.display().to_string(),
                line,
                now
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
        item_level_refs: item_level,
        self_ref_skipped: self_refs,
        non_fn_defs_skipped: rows.non_fn_skipped,
        external_skipped,
        flows: derived.flows,
        communities: derived.communities,
        db,
    })
}

/// CLI face: resolves the repo-keyed default slot.
pub fn build_from_cache(repo: &Path) -> Result<BuildReport, String> {
    let index = crate::engine::default_index_path(repo)?;
    build_from_cache_at(repo, &index)
}

// ---------- S3: legacy importer (nodes + edges) ----------

#[derive(Debug)]
pub struct ImportLegacyReport {
    /// true when `.code-review-graph/graph.db` is absent (skip, not error)
    pub skipped: bool,
    pub legacy_nodes: usize,
    /// legacy Function nodes whose (file, name) resolved uniquely onto a
    /// producer node — legacy span/flags patched onto it
    pub merged_nodes: usize,
    /// legacy nodes minted as qname-keyed symbols (Class/Test/File and
    /// any unresolved or colliding key — nothing is dropped)
    pub synthesized_nodes: usize,
    /// (file, name) keys hitting multiple producer symbols (counted,
    /// routed to synthesis — merge stays conservative)
    pub collision_keys: usize,
    /// qname INSERTs ignored because the symbol already existed
    pub symbol_collision_skipped: usize,
    pub legacy_edges: usize,
    /// source rows imported (duplicates preserved — row-count conservation)
    pub mapped_edges: usize,
    /// rows skipped: an endpoint absent from the legacy nodes table
    pub dangling_edges: usize,
    pub derived: Option<DerivedReport>,
    pub db: PathBuf,
    pub legacy_db: PathBuf,
    pub dry_run: bool,
}

/// One-shot import of the CRG-era `.code-review-graph/graph.db` into the
/// self-owned db (EP S3): legacy Function nodes whose (file, name)
/// resolves uniquely onto a producer node merge (span/is_test patched);
/// everything else mints a qname-keyed symbol (provenance
/// 'treesitter-legacy'). Edges import when both endpoints resolve
/// (producer symbol first, legacy qname second); dangling endpoints are
/// skipped and classified, never guessed. Idempotent: the legacy
/// provenance is swept before writing; re-runs converge. The legacy db is
/// opened read-only (oracle, never written).
pub fn import_legacy(repo: &Path, dry_run: bool) -> Result<ImportLegacyReport, String> {
    let legacy = crate::common::graph_db_path(repo);
    if !legacy.exists() {
        return Ok(ImportLegacyReport {
            skipped: true,
            legacy_nodes: 0,
            merged_nodes: 0,
            synthesized_nodes: 0,
            collision_keys: 0,
            symbol_collision_skipped: 0,
            legacy_edges: 0,
            mapped_edges: 0,
            dangling_edges: 0,
            derived: None,
            db: db_path(repo),
            legacy_db: legacy,
            dry_run,
        });
    }
    let db = db_path(repo);
    if !db.exists() {
        return Err(format!(
            "新庫不在：{}——先 `graph_db build --repo` 再 import_legacy",
            db.display()
        ));
    }
    let repo_abs = resolve(repo);
    let lc = crate::common::connect_ro(&legacy)?;
    /// (kind, name, qname, file_path, line_start, line_end, language,
    /// parent_name, is_test, extra, community_id) — one legacy node row
    type LegacyNode = (
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        i64,
        String,
        Option<i64>,
    );
    let legacy_nodes: Vec<LegacyNode> = {
        let mut stmt = lc
            .prepare(
                "SELECT kind, name, qualified_name, file_path, line_start, line_end, \
                 language, parent_name, is_test, extra, community_id FROM nodes",
            )
            .map_err(|e| format!("legacy nodes 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, Option<i64>>(10)?,
                ))
            })
            .map_err(|e| format!("legacy nodes 查詢失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("legacy nodes 讀取失敗：{e}"))?
    };
    let mut conn = Connection::open(&db).map_err(|e| format!("graph.db 開啟失敗：{e}"))?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(|e| format!("graph.db busy_timeout 設定失敗：{e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("import 交易開啟失敗：{e}"))?;
    // idempotence sweep first, then build the merge map over the
    // producer universe only. R1 (dual-context review 2026-08-27):
    // dry-run NEVER sweeps and NEVER commits — a dry-run that swept
    // would silently delete an existing import and commit it.
    if !dry_run {
        tx.execute_batch(
            "DELETE FROM edges WHERE provenance = 'treesitter-legacy'; \
                          DELETE FROM nodes WHERE provenance = 'treesitter-legacy';",
        )
        .map_err(|e| format!("import 冪等清理失敗：{e}"))?;
    }
    let mut key_symbols: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT symbol, file_path, name FROM nodes                  WHERE provenance != 'treesitter-legacy'",
            )
            .map_err(|e| format!("nodes 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("nodes 查詢失敗：{e}"))?;
        for row in rows {
            let (sym, file, name) = row.map_err(|e| format!("nodes 讀取失敗：{e}"))?;
            key_symbols
                .entry((
                    resolve(Path::new(&file)).to_string_lossy().into_owned(),
                    name,
                ))
                .or_default()
                .push(sym);
        }
    }
    let now = now_ts();
    let mut merged = 0usize;
    let mut synthesized = 0usize;
    let mut collisions = 0usize;
    let mut symbol_collisions = 0usize;
    // qname -> resolved symbol for the edge pass
    let mut qn_symbol: HashMap<String, String> = HashMap::with_capacity(legacy_nodes.len());
    for (kind, name, qname, file_path, ls, le, lang, parent, is_test, extra, community) in
        &legacy_nodes
    {
        let resolved = if kind == "Function" {
            match key_symbols.get(&(
                resolve(Path::new(file_path)).to_string_lossy().into_owned(),
                name.clone(),
            )) {
                Some(cands) if cands.len() == 1 => Some(cands[0].clone()),
                Some(_) => {
                    collisions += 1;
                    None // conservative: colliding keys route to synthesis
                }
                None => None,
            }
        } else {
            None
        };
        match resolved {
            Some(sym) => {
                if !dry_run {
                    // legacy values win when present (the tree-sitter face
                    // carries full spans; layer-2 range semantics depend
                    // on them) — NULL legacy fields keep producer values
                    tx.execute(
                        // community_id is NOT merged: materialize_derived
                        // NULLs and recomputes it from directory grouping
                        "UPDATE nodes SET \
                         line_start = COALESCE(?1, line_start), \
                         line_end = COALESCE(?2, line_end), \
                         language = COALESCE(?3, language), \
                         parent_name = COALESCE(?4, parent_name), \
                         is_test = MAX(is_test, ?5), \
                         extra = CASE WHEN extra = '{}' AND ?6 != '{}' THEN ?6 ELSE extra END \
                         WHERE symbol = ?7",
                        rusqlite::params![ls, le, lang, parent, is_test, extra, sym],
                    )
                    .map_err(|e| format!("merge 更新失敗：{e}"))?;
                }
                merged += 1;
                qn_symbol.insert(qname.clone(), sym);
            }
            None => {
                if !dry_run {
                    let n = tx
                        .execute(
                            "INSERT OR IGNORE INTO nodes \
                             (symbol, kind, name, qname, file_path, line_start, line_end, \
                              language, parent_name, is_test, extra, updated_at, \
                              community_id, provenance) \
                             VALUES (?1, ?2, ?3, ?1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'treesitter-legacy')",
                            rusqlite::params![
                                qname, kind, name, file_path, ls, le, lang, parent, is_test,
                                extra, now, community
                            ],
                        )
                        .map_err(|e| format!("legacy 節點寫入失敗：{e}"))?;
                    if n == 0 {
                        symbol_collisions += 1;
                    }
                }
                synthesized += 1;
                qn_symbol.insert(qname.clone(), qname.clone());
            }
        }
    }
    // edges: stream source rows, resolve both endpoints. Resolvable
    // endpoints map through (producer symbol | legacy qname); DANGLING
    // endpoints (absent from the legacy nodes table — external symbols)
    // land as-is: edges carry no FK, BFS/impact/hub skip unknown endpoints
    // by construction, and the criticality external factor COUNTS them —
    // the layer-2 parity bar pinned this (dropping them deflates
    // criticality by exactly the external term).
    let mut mapped = 0usize;
    let mut dangling = 0usize;
    let mut total_edges = 0usize;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO edges \
                 (kind, caller_symbol, callee_symbol, provenance, file_path, line, \
                  confidence, confidence_tier, updated_at) \
                 VALUES (?1, ?2, ?3, 'treesitter-legacy', ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(|e| format!("legacy 邊寫入準備失敗：{e}"))?;
        let mut src_stmt = lc
            .prepare(
                "SELECT kind, source_qualified, target_qualified, file_path, line, \
                 confidence, confidence_tier FROM edges",
            )
            .map_err(|e| format!("legacy edges 查詢失敗：{e}"))?;
        let rows = src_stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(|e| format!("legacy edges 查詢失敗：{e}"))?;
        for row in rows {
            let (kind, sq, tq, file, line, conf, tier) =
                row.map_err(|e| format!("legacy edges 讀取失敗：{e}"))?;
            total_edges += 1;
            let src_known = qn_symbol.contains_key(&sq);
            let tgt_known = qn_symbol.contains_key(&tq);
            let cs = if src_known {
                qn_symbol[&sq].clone()
            } else {
                sq
            };
            let ct = if tgt_known {
                qn_symbol[&tq].clone()
            } else {
                tq
            };
            if src_known && tgt_known {
                mapped += 1;
            } else {
                dangling += 1;
            }
            if !dry_run {
                // legacy file_path is repo-absolute already; anchor under
                // the resolved repo root when it drifted relative
                let file_abs = if file.starts_with('/') {
                    file.clone()
                } else {
                    repo_abs.join(&file).to_string_lossy().into_owned()
                };
                stmt.execute(rusqlite::params![
                    kind, cs, ct, file_abs, line, conf, tier, now
                ])
                .map_err(|e| format!("legacy 邊寫入失敗：{e}"))?;
            }
        }
    }
    if dry_run {
        // read-only face: nothing was written (sweep/INSERT/UPDATE all
        // gated) — roll the open transaction back, never commit
        tx.rollback()
            .map_err(|e| format!("dry-run rollback 失敗：{e}"))?;
    } else {
        tx.commit().map_err(|e| format!("import 提交失敗：{e}"))?;
        // FTS rebuild (F1, dual-context 2026-08-27): imported legacy
        // nodes are ~43% of the db — a stale FTS index silently hides
        // them from search_nodes (FTS hit short-circuits the LIKE
        // fallback)
        conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES ('rebuild')", [])
            .map_err(|e| format!("import 後 nodes_fts 重建失敗：{e}（舊 schema 庫——重跑 `graph_db build` 重建含 FTS 的 schema 再 import）"))?;
    }
    let derived = if dry_run {
        None
    } else {
        Some(materialize_derived(repo)?)
    };
    Ok(ImportLegacyReport {
        skipped: false,
        legacy_nodes: legacy_nodes.len(),
        merged_nodes: merged,
        synthesized_nodes: synthesized,
        collision_keys: collisions,
        symbol_collision_skipped: symbol_collisions,
        legacy_edges: total_edges,
        mapped_edges: mapped,
        dangling_edges: dangling,
        derived,
        db,
        legacy_db: legacy,
        dry_run,
    })
}

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--dry-run",
            short: None,
            kind: Kind::StoreTrue,
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
    "usage: graph_db [-h] <build|import_legacy> --repo REPO [--dry-run] [--json]\n",
    "\n",
    "自有格式 graph.db 面（v1+ S4；.code-reality/graph.db——producer symbol\n",
    "鍵＋單一邊本體，取代 CRG 相容格式的角色）。\n",
    "\n",
    "ops:\n",
    "  build            cache index（任何 producer）→ nodes+edges+derived 物化\n",
    "  import_legacy    舊 .code-review-graph/graph.db 一次性匯入（唯讀源）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo root（解析 repo-keyed 預設 index slot）\n",
    "  --dry-run             import_legacy 僅報告不寫入（完全唯讀）\n",
    "  --json                報告 JSON 面\n",
);

pub fn run(argv: &[&str]) -> ToolOutput {
    // argv = ["graph_db", <op>, flags...] — the umbrella passes the tool
    // name through (graph_engine::run's positional-op pattern)
    let Some((&_tool, rest)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 graph_db <build>");
    };
    let Some((op, toks)) = rest.split_first() else {
        return ToolOutput::fail("需提供操作（graph_db build / import_legacy）");
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
    if op != &"build" && op != &"import_legacy" {
        return ToolOutput::fail(format!("未知操作：{op}（支援 build / import_legacy）"));
    }
    if values.contains_key("--dry-run") && op == &"build" {
        return ToolOutput::fail(
            "build 不支援 --dry-run（冪等重跑即預覽；僅 import_legacy 有 dry-run 面）",
        );
    }
    let Some(repo_s) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("需 --repo");
    };
    let dry = values.contains_key("--dry-run");
    let json = values.contains_key("--json");
    if op == &"import_legacy" {
        return match import_legacy(Path::new(&repo_s), dry) {
            Ok(rep) => {
                if rep.skipped {
                    if json {
                        let v = serde_json::json!({
                            "skipped": true,
                            "db": rep.db.display().to_string(),
                            "legacy_db": rep.legacy_db.display().to_string(),
                        });
                        ToolOutput {
                            stdout: format!("{}\n", to_json_indent1(&v)),
                            stderr: String::new(),
                            exit_code: 0,
                        }
                    } else {
                        ToolOutput {
                            stdout: format!(
                                "[OK] graph_db import_legacy：舊庫不在（{}）——skip 非 error\n",
                                rep.legacy_db.display()
                            ),
                            stderr: String::new(),
                            exit_code: 0,
                        }
                    }
                } else if json {
                    let v = serde_json::json!({
                        "legacy_nodes": rep.legacy_nodes,
                        "merged_nodes": rep.merged_nodes,
                        "synthesized_nodes": rep.synthesized_nodes,
                        "collision_keys": rep.collision_keys,
                        "symbol_collision_skipped": rep.symbol_collision_skipped,
                        "legacy_edges": rep.legacy_edges,
                        "mapped_edges": rep.mapped_edges,
                        "dangling_edges": rep.dangling_edges,
                        "flows": rep.derived.as_ref().map(|d| d.flows),
                        "communities": rep.derived.as_ref().map(|d| d.communities),
                        "db": rep.db.display().to_string(),
                        "legacy_db": rep.legacy_db.display().to_string(),
                        "dry_run": rep.dry_run,
                    });
                    ToolOutput {
                        stdout: format!("{}\n", to_json_indent1(&v)),
                        stderr: String::new(),
                        exit_code: 0,
                    }
                } else {
                    let mode = if rep.dry_run { "（dry-run）" } else { "" };
                    ToolOutput {
                        stdout: format!(
                            "[OK] graph_db import_legacy{mode}：nodes merged={} synthesized={} collision={} symbol-skip={}；edges mapped={}/{} dangling={} → {}\n",
                            rep.merged_nodes,
                            rep.synthesized_nodes,
                            rep.collision_keys,
                            rep.symbol_collision_skipped,
                            rep.mapped_edges,
                            rep.legacy_edges,
                            rep.dangling_edges,
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
                        "[OK] graph_db build：{} 節點 / {} 邊（REFERENCES site rows） / item-level refs 略過 {} / self-ref 略過 {} / external 略過 {} → {}\n",
                        rep.nodes,
                        rep.edges,
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
