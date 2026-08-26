//! `scip_edges` — v1+ S1: SCIP reference-edge export + sidecar union-edge
//! injection ((A) adjudication, EP ep-v1plus-graph-engine.md S1: edges
//! land in a code-reality-owned sidecar db sibling of the index, never in
//! CRG graph.db).
//!
//! Edge semantics = the `scip_refs --callers` face: every is_def=0
//! occurrence attributed to the innermost enclosing fn span — reference
//! edges, NOT call-only (old-schema index has no is_call_reference). The
//! sidecar keeps `kind='REFERENCES'` on the semantic axis and
//! `provenance='SCIP'` on the provenance axis; the pair grain
//! (caller_symbol, callee_symbol) is the PRIMARY KEY (own schema, own
//! UNIQUE semantics — deliberately not CRG's tier-unaware upsert key).
//!
//! Workspace filter: the callee must carry a DEF in the index (the NT
//! corpus index holds zero external paths — referenced std/core symbols
//! simply have no DEF occurrence, so DEF membership IS the test). The
//! caller side needs no check by construction — callers come from
//! fn-span DEFs, a subset of the DEF universe (invariant holds only
//! while spans derive from DEFs; revisit if spans_source ever admits
//! non-DEF sources). Skipped external edges stay available through the
//! export face.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::cache::{self, Face};
use crate::callers;
use crate::common::to_json_indent1;
use crate::engine::{default_index_path, fn_tail_name, ln};
use crate::fndefs;
use crate::ToolOutput;
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct EdgeRow {
    pub caller: String,
    pub callee: String,
    pub sites: usize,
}

pub struct DeriveReport {
    pub ref_sites: usize,
    pub item_level: usize,
    pub edges_total: usize,
    pub edges_workspace: usize,
    pub external_skipped: usize,
}

pub struct InjectReport {
    pub report: DeriveReport,
    pub db_rows: usize,
    pub swept: usize,
    pub dry_run: bool,
    pub db_path: PathBuf,
    pub warns: Vec<String>,
}

/// Sidecar sibling of the index: `index.scip` → `index.union.db`.
pub fn union_db_path(index_path: &Path) -> PathBuf {
    let stem = index_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "index".to_string());
    let mut p = index_path.to_path_buf();
    p.set_file_name(format!("{stem}.union.db"));
    p
}

/// Keep only edges whose callee has a DEF in the index.
pub fn filter_workspace(edges: Vec<EdgeRow>, defs: &BTreeSet<String>) -> (Vec<EdgeRow>, usize) {
    let mut kept = Vec::new();
    let mut skipped = 0usize;
    for e in edges {
        if defs.contains(&e.callee) {
            kept.push(e);
        } else {
            skipped += 1;
        }
    }
    (kept, skipped)
}

/// Ref rows + DEF symbols via the family face ladder (fresh sqlite cache
/// → protobuf in-memory; never builds the cache on miss). The sqlite face
/// stores only fn-tailed symbols' occurrences (cache::build_db filter) —
/// the protobuf branch mirrors that (`fn_tail_name` gate) so both faces
/// carry the same fn-callee universe.
fn scan_rows_and_defs(
    face: &Face,
) -> Result<(Vec<(String, String, i64)>, BTreeSet<String>), String> {
    match face {
        Face::Sqlite(conn) => {
            let mut rows = Vec::new();
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT symbol, rel_path, line FROM occurrences \
                         WHERE is_def = 0 ORDER BY seq",
                    )
                    .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
                let it = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })
                    .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
                for row in it {
                    rows.push(row.map_err(|e| format!("scip_edges 掃描失敗：{e}"))?);
                }
            }
            let mut defs = BTreeSet::new();
            let mut stmt = conn
                .prepare("SELECT DISTINCT symbol FROM occurrences WHERE is_def = 1")
                .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
            let it = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| format!("scip_edges 掃描失敗：{e}"))?;
            for s in it {
                defs.insert(s.map_err(|e| format!("scip_edges 掃描失敗：{e}"))?);
            }
            Ok((rows, defs))
        }
        Face::Protobuf { index } => {
            let mut rows = Vec::new();
            let mut defs = BTreeSet::new();
            for d in &index.documents {
                for occ in &d.occurrences {
                    if fn_tail_name(&occ.symbol).is_none() {
                        continue;
                    }
                    if occ.symbol_roles & 1 != 0 {
                        defs.insert(occ.symbol.clone());
                    } else {
                        rows.push((occ.symbol.clone(), d.relative_path.clone(), ln(occ)));
                    }
                }
            }
            Ok((rows, defs))
        }
    }
}

/// Full derivation (POC A1 semantics through the real lib ladder).
/// Returns ALL edges (external included) in (caller, callee) order; the
/// report carries the workspace split.
fn derive_internal(
    index_path: &Path,
) -> Result<(Vec<EdgeRow>, BTreeSet<String>, DeriveReport, Vec<String>), String> {
    let (face, mut warns) = cache::open_face(index_path)?;
    let (rows, defs) = scan_rows_and_defs(&face)?;
    // Mixed-face cost note: a fresh sqlite cache + missing fndefs sidecar
    // makes the Sqlite arm re-parse the full protobuf for spans (accepted;
    // the sidecar ladder rebuilds itself on first touch).
    let spans_result = match &face {
        Face::Protobuf { index } => fndefs::spans_source(index_path, Some(index)),
        Face::Sqlite(_) => fndefs::spans_source(index_path, None),
    };
    let (spans, span_warns) = spans_result?;
    warns.extend(span_warns);

    let ref_sites = rows.len();
    let mut by_callee: BTreeMap<String, Vec<(String, String, i64)>> = BTreeMap::new();
    for (sym, rel, line) in rows {
        by_callee
            .entry(sym.clone())
            .or_default()
            .push((sym, rel, line));
    }
    let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut item_level = 0usize;
    for (callee, group) in &by_callee {
        let res = callers::attribute(group, &spans);
        item_level += res.item_level.len();
        for c in &res.callers {
            *edges.entry((c.symbol.clone(), callee.clone())).or_insert(0) += c.sites.len();
        }
    }
    let all: Vec<EdgeRow> = edges
        .into_iter()
        .map(|((caller, callee), sites)| EdgeRow {
            caller,
            callee,
            sites,
        })
        .collect();
    let edges_workspace = all.iter().filter(|e| defs.contains(&e.callee)).count();
    let report = DeriveReport {
        ref_sites,
        item_level,
        edges_total: all.len(),
        edges_workspace,
        external_skipped: all.len() - edges_workspace,
    };
    Ok((all, defs, report, warns))
}

/// Public derive: ALL edges (external included) + report + ladder WARNs.
pub fn derive_edges(
    index_path: &Path,
) -> Result<(Vec<EdgeRow>, DeriveReport, Vec<String>), String> {
    let (all, _defs, report, warns) = derive_internal(index_path)?;
    Ok((all, report, warns))
}

fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Idempotent upsert into the union sidecar + stale sweep. One txn:
/// upsert every current edge at `ts`, then delete rows older than `ts`
/// (absent-from-index leftovers). Same-instant re-runs are safe — the
/// strict `<` keeps this run's rows.
pub fn inject(index_path: &Path, dry_run: bool) -> Result<InjectReport, String> {
    let (all, defs, report, warns) = derive_internal(index_path)?;
    let (kept, _skipped) = filter_workspace(all, &defs);
    let db_path = union_db_path(index_path);
    if dry_run {
        return Ok(InjectReport {
            report,
            db_rows: 0,
            swept: 0,
            dry_run: true,
            db_path,
            warns,
        });
    }
    let mut conn = Connection::open(&db_path)
        .map_err(|e| format!("union db 開啟失敗（{}）：{e}", db_path.display()))?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(|e| format!("union db busy_timeout 設定失敗：{e}"))?;
    // Schema-version self-heal (family sidecar convention): a mismatched
    // union.db is fully regenerable — recreate instead of failing mid-txn.
    const UNION_SCHEMA_VERSION: i64 = 1;
    let uv: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);
    if uv != 0 && uv != UNION_SCHEMA_VERSION {
        drop(conn);
        std::fs::remove_file(&db_path).map_err(|e| format!("union db 舊 schema 清除失敗：{e}"))?;
        conn = Connection::open(&db_path)
            .map_err(|e| format!("union db 開啟失敗（{}）：{e}", db_path.display()))?;
        conn.execute_batch("PRAGMA busy_timeout = 5000;")
            .map_err(|e| format!("union db busy_timeout 設定失敗：{e}"))?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS edges (
            caller_symbol TEXT NOT NULL,
            callee_symbol TEXT NOT NULL,
            sites INTEGER NOT NULL,
            kind TEXT NOT NULL DEFAULT 'REFERENCES',
            provenance TEXT NOT NULL DEFAULT 'SCIP',
            updated_at REAL NOT NULL,
            PRIMARY KEY (caller_symbol, callee_symbol)
        ); PRAGMA user_version = 1;",
    )
    .map_err(|e| format!("union db 建表失敗：{e}"))?;
    let ts = now_ts();
    let tx = conn
        .transaction()
        .map_err(|e| format!("union db 交易開啟失敗：{e}"))?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO edges (caller_symbol, callee_symbol, sites, kind, provenance, updated_at) \
                 VALUES (?1, ?2, ?3, 'REFERENCES', 'SCIP', ?4) \
                 ON CONFLICT(caller_symbol, callee_symbol) \
                 DO UPDATE SET sites = excluded.sites, updated_at = excluded.updated_at",
            )
            .map_err(|e| format!("union db 寫入準備失敗：{e}"))?;
        for e in &kept {
            stmt.execute(rusqlite::params![e.caller, e.callee, e.sites as i64, ts])
                .map_err(|e| format!("union db 寫入失敗：{e}"))?;
        }
    }
    // provenance-scoped: the schema reserves kind/provenance axes for
    // future second writers — the sweep must never eat another
    // provenance's rows (F3, review 2026-08-26).
    let swept = tx
        .execute(
            "DELETE FROM edges WHERE updated_at < ?1 AND provenance = 'SCIP'",
            rusqlite::params![ts],
        )
        .map_err(|e| format!("union db 掃除失敗：{e}"))? as usize;
    tx.commit().map_err(|e| format!("union db 提交失敗：{e}"))?;
    let db_rows = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("union db 計數失敗：{e}"))? as usize;
    Ok(InjectReport {
        report,
        db_rows,
        swept,
        dry_run: false,
        db_path,
        warns,
    })
}

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--index",
            short: None,
            kind: Kind::Value { metavar: "INDEX" },
        },
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--inject",
            short: None,
            kind: Kind::StoreTrue,
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
    "usage: scip_edges [-h] [--index INDEX] [--repo REPO] [--inject]\n",
    "                  [--dry-run] [--json]\n",
    "\n",
    "SCIP reference 邊匯出與 sidecar 注入（v1+ S1；邊語義＝scip_refs --callers\n",
    "的 occurrence 歸屬面——reference 邊非 call-only）。\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --index INDEX         SCIP index 路徑（或 --repo 解析預設 slot）\n",
    "  --repo REPO           repo root（解析 repo-keyed 預設 index slot）\n",
    "  --inject              注入 sidecar union-edge db（index 同目錄 .union.db）\n",
    "  --dry-run             僅報告不寫入（伴 --inject）\n",
    "  --json                注入報告 JSON 面（伴 --inject）\n",
);

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 scip_edges");
    };
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
    let inject_flag = values.contains_key("--inject");
    let dry = values.contains_key("--dry-run");
    let json = values.contains_key("--json");
    if dry && !inject_flag {
        return ToolOutput::fail("--dry-run 僅伴 --inject 使用");
    }
    if json && !inject_flag {
        return ToolOutput::fail("--json 僅伴 --inject 使用");
    }
    let index_path = match values.get("--index").and_then(|v| v.clone()) {
        Some(p) => PathBuf::from(p),
        None => match values.get("--repo").and_then(|v| v.clone()) {
            Some(repo) => match default_index_path(Path::new(&repo)) {
                Ok(p) => p,
                Err(msg) => return ToolOutput::fail(msg),
            },
            None => return ToolOutput::fail("需 --index（或 --repo 解析 repo-keyed 預設 slot）"),
        },
    };
    if !index_path.exists() {
        return ToolOutput::fail(format!("索引不在：{}", index_path.display()));
    }
    if inject_flag {
        return match inject(&index_path, dry) {
            Ok(rep) => {
                let stderr: String = rep.warns.concat();
                if json {
                    let v = json!({
                        "ref_sites": rep.report.ref_sites,
                        "item_level": rep.report.item_level,
                        "edges_total": rep.report.edges_total,
                        "edges_workspace": rep.report.edges_workspace,
                        "external_skipped": rep.report.external_skipped,
                        "db_rows": rep.db_rows,
                        "swept": rep.swept,
                        "dry_run": rep.dry_run,
                        "db": rep.db_path.display().to_string(),
                    });
                    ToolOutput {
                        stdout: format!("{}\n", to_json_indent1(&v)),
                        stderr,
                        exit_code: 0,
                    }
                } else if rep.dry_run {
                    ToolOutput {
                        stdout: format!(
                            "[OK] scip_edges inject（dry-run）：edges={}（workspace）external={} db={}\n",
                            rep.report.edges_workspace,
                            rep.report.external_skipped,
                            rep.db_path.display()
                        ),
                        stderr,
                        exit_code: 0,
                    }
                } else {
                    ToolOutput {
                        stdout: format!(
                            "[OK] scip_edges inject：rows={} edges={} external={} swept={} db={}\n",
                            rep.db_rows,
                            rep.report.edges_workspace,
                            rep.report.external_skipped,
                            rep.swept,
                            rep.db_path.display()
                        ),
                        stderr,
                        exit_code: 0,
                    }
                }
            }
            Err(e) => ToolOutput::crash(e),
        };
    }
    // export face: full TSV (external included) + [OK] summary on stderr
    match derive_edges(&index_path) {
        Ok((edges, report, warns)) => {
            let mut stdout = String::new();
            for e in &edges {
                stdout.push_str(&format!("{}\t{}\t{}\n", e.caller, e.callee, e.sites));
            }
            let mut stderr = warns.concat();
            stderr.push_str(&format!(
                "[OK] scip_edges: ref-sites={} edges={}（workspace {}／external {}）item-level={}\n",
                report.ref_sites,
                report.edges_total,
                report.edges_workspace,
                report.external_skipped,
                report.item_level
            ));
            ToolOutput {
                stdout,
                stderr,
                exit_code: 0,
            }
        }
        Err(e) => ToolOutput::crash(e),
    }
}
