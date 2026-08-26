//! `scip_nodes` — v1+ S2: graph_audit missing → SCIP reconciliation →
//! graph.db node injection. THE graph.db write face (everything else in
//! the family reads via `common::connect_ro`); the edge plane lives in
//! the `scip_edges` sidecar instead ((A) adjudication).
//!
//! Reconciliation = the `scip_refs --audit` double-key rule
//! (`cache::audit_targets`): a missing (file, name) becomes a node only
//! when the SCIP index carries a matching fn DEF — rust-analyzer/SCIP
//! drift never fabricates nodes. The `extra {"tier":"SCIP"}` marker is
//! the rollback key; the `.bak-scip-inject` copy (first inject only) is
//! the last line of defense.
//!
//! Residual semantics: a missing item whose ra_count exceeds
//! db_count + 1 (same fn name twice in one file) can never fully close —
//! one qualified_name carries one node. Reported as `residual_missing`,
//! never fabricated with duplicate qnames.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::cache;
use crate::common::{graph_db_path, resolve, to_json_indent1};
use crate::engine::default_index_path;
use crate::graph_audit::{audit, RaLookup};
use crate::ToolOutput;
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct NodeInjectReport {
    pub missing_total: usize,
    pub mapped: usize,
    pub inserted: usize,
    pub collision_skipped: usize,
    pub unmapped: usize,
    pub residual_missing: usize,
    pub audit_errors: usize,
    pub dry_run: bool,
    pub graph_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub warns: Vec<String>,
}

const MARKER_EXTRA: &str = "{\"tier\":\"SCIP\"}";

/// Core injector with an injectable ra_lookup (tests pass a synthetic
/// counter; production passes None for the live rust-analyzer face).
/// `all_files` mirrors graph_audit's scope switch — production uses the
/// default risk scope (the same measure `graph_audit` reports).
#[allow(clippy::type_complexity)]
pub fn inject_nodes_with(
    repo: &Path,
    graph: &Path,
    index_path: &Path,
    dry_run: bool,
    all_files: bool,
    ra_lookup: Option<RaLookup>,
) -> Result<NodeInjectReport, String> {
    let mut warns: Vec<String> = Vec::new();
    let (_risk, _audited, missing, errors, _total_ra, audit_warns) =
        audit(repo, graph, all_files, ra_lookup)?;
    warns.extend(audit_warns);
    let audit_errors = errors.len();
    if audit_errors > 0 {
        warns.push(format!(
            "[WARN] graph_audit errors：{audit_errors} 檔 rust-analyzer 逾時/失敗——missing 面非全覆蓋，本報告非乾淨讀\n"
        ));
    }
    // Freshness gate: a stale index reconciles against an older symbol
    // universe (double-key still guards fabrication) — loud, not blocking.
    if let (Ok(it), Ok(gt)) = (
        std::fs::metadata(index_path).and_then(|m| m.modified()),
        std::fs::metadata(graph).and_then(|m| m.modified()),
    ) {
        if it < gt {
            warns.push(
                "[WARN] SCIP index 比 graph.db 舊——重生 index（rust-analyzer scip）後重注入，對帳面更完整\n"
                    .to_string(),
            );
        }
    }
    let missing_total = missing.len();

    // Reconciliation face: missing (file, name) → SCIP symbol
    let (face, face_warns) = cache::open_face(index_path)?;
    warns.extend(face_warns);
    let repo_abs = resolve(repo);
    let mut files_by_name: HashMap<String, BTreeSet<String>> = HashMap::new();
    for m in &missing {
        if let Ok(rel) = resolve(Path::new(&m.file)).strip_prefix(&repo_abs) {
            files_by_name
                .entry(m.symbol.clone())
                .or_default()
                .insert(rel.to_string_lossy().into_owned());
        }
    }
    let targets = face.audit_targets(&files_by_name)?;
    // (rel_path, name) double keys the index can confirm
    let mut by_key: BTreeSet<(String, String)> = BTreeSet::new();
    for (_sym, (rel, name)) in &targets {
        by_key.insert((rel.clone(), name.clone()));
    }

    // Plan: qname = <abs-file>::<name> (CRG convention). file_path/qname
    // carry the RESOLVED form — db_functions resolves before counting, so
    // the analytic residual model only holds when both sides canonicalize
    // identically (R-1, review 2026-08-26).
    struct Plan {
        qname: String,
        file: String,
        name: String,
        ra_count: usize,
        db_count: usize,
    }
    let mut plans: Vec<Plan> = Vec::new();
    let mut unmapped = 0usize;
    for m in &missing {
        let file_abs = resolve(Path::new(&m.file));
        let hit = file_abs
            .strip_prefix(&repo_abs)
            .ok()
            .and_then(|rel| by_key.get(&(rel.to_string_lossy().into_owned(), m.symbol.clone())));
        match hit {
            Some(_) => plans.push(Plan {
                qname: format!("{}::{}", file_abs.display(), m.symbol),
                file: file_abs.display().to_string(),
                name: m.symbol.clone(),
                ra_count: m.ra_count,
                db_count: m.db_count,
            }),
            None => unmapped += 1,
        }
    }
    let mapped = plans.len();

    if dry_run {
        return Ok(NodeInjectReport {
            missing_total,
            mapped,
            inserted: plans.len(),
            collision_skipped: 0,
            unmapped,
            // Dry-run estimates: inserted = would-insert UPPER bound
            // (qname collisions unknown), residual = analytic LOWER bound
            // (the real-run formula with landed = 0).
            residual_missing: plans.iter().filter(|p| p.ra_count > p.db_count + 1).count()
                + unmapped,
            audit_errors,
            dry_run: true,
            graph_path: graph.to_path_buf(),
            backup_path: None,
            warns,
        });
    }

    // Backup (first inject only — never overwrite an older safety copy).
    // VACUUM INTO, not fs::copy: graph.db is WAL-mode, and a plain file
    // copy can miss un-checkpointed WAL content (and the bare copy of a
    // WAL db won't even open read-only without its sidecars).
    let backup = graph.with_file_name(format!(
        "{}.bak-scip-inject",
        graph
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let backup_path = if backup.exists() {
        None
    } else {
        let src = Connection::open_with_flags(graph, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("graph.db 開啟失敗（備份源）：{e}"))?;
        src.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
            .map_err(|e| format!("graph.db backup 失敗（→ {}）：{e}", backup.display()))?;
        Some(backup)
    };

    // The write face: one txn, marker-tagged, UNIQUE-safe
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let mut conn = Connection::open(graph)
        .map_err(|e| format!("graph.db 開啟失敗（{}）：{e}", graph.display()))?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(|e| format!("graph.db busy_timeout 設定失敗：{e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("graph.db 交易開啟失敗：{e}"))?;
    let mut inserted = 0usize;
    let mut collision_skipped = 0usize;
    let mut landed: HashSet<(String, String)> = HashSet::new();
    for p in &plans {
        let n = tx
            .execute(
                "INSERT INTO nodes (kind, name, qualified_name, file_path, language, extra, \
                 updated_at, is_test) \
                 VALUES ('Function', ?1, ?2, ?3, 'Rust', ?4, ?5, 0) \
                 ON CONFLICT(qualified_name) DO NOTHING",
                rusqlite::params![p.name, p.qname, p.file, MARKER_EXTRA, now],
            )
            .map_err(|e| format!("graph.db 節點寫入失敗：{e}"))?;
        if n == 1 {
            inserted += 1;
            landed.insert((p.file.clone(), p.name.clone()));
        } else {
            collision_skipped += 1;
        }
    }
    tx.commit().map_err(|e| format!("graph.db 提交失敗：{e}"))?;

    // Analytic residual: same measure the acceptance re-run reports,
    // without a second (slow) audit pass — counts only move for touched
    // (file, name) pairs.
    let residual_missing = plans
        .iter()
        .filter(|p| {
            let landed_n = landed.contains(&(p.file.clone(), p.name.clone())) as usize;
            p.ra_count > p.db_count + landed_n
        })
        .count()
        + unmapped;

    Ok(NodeInjectReport {
        missing_total,
        mapped,
        inserted,
        collision_skipped,
        unmapped,
        residual_missing,
        audit_errors,
        dry_run: false,
        graph_path: graph.to_path_buf(),
        backup_path,
        warns,
    })
}

/// Production face: live rust-analyzer missing detection, default risk
/// scope (the same measure `graph_audit` reports).
pub fn inject_nodes(
    repo: &Path,
    graph: &Path,
    index_path: &Path,
    dry_run: bool,
) -> Result<NodeInjectReport, String> {
    inject_nodes_with(repo, graph, index_path, dry_run, false, None)
}

/// Rollback: delete only the marker-tagged nodes.
pub fn rollback_nodes(graph: &Path) -> Result<usize, String> {
    let conn = Connection::open(graph)
        .map_err(|e| format!("graph.db 開啟失敗（{}）：{e}", graph.display()))?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(|e| format!("graph.db busy_timeout 設定失敗：{e}"))?;
    let n = conn
        .execute("DELETE FROM nodes WHERE extra = ?1", [MARKER_EXTRA])
        .map_err(|e| format!("graph.db 回滾失敗：{e}"))?;
    Ok(n)
}

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--graph",
            short: None,
            kind: Kind::Value { metavar: "GRAPH" },
        },
        FlagSpec {
            long: "--index",
            short: None,
            kind: Kind::Value { metavar: "INDEX" },
        },
        FlagSpec {
            long: "--dry-run",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--rollback",
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
    "usage: scip_nodes [-h] --repo REPO [--graph GRAPH] [--index INDEX]\n",
    "                  [--dry-run] [--rollback] [--json]\n",
    "\n",
    "graph_audit missing → SCIP 對帳 → graph.db 節點注入（v1+ S2；本工具是\n",
    "graph.db 唯一寫入面——對帳規則＝scip_refs --audit，無 DEF 不注入）。\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo root（audit 對象＋預設 index slot 解析）\n",
    "  --graph GRAPH         覆寫 graph.db 路徑（預設 <repo>/.code-review-graph/graph.db）\n",
    "  --index INDEX         覆寫 SCIP index 路徑（預設 --repo 解析 slot）\n",
    "  --dry-run             僅報告不寫入\n",
    "  --rollback            回滾：刪除所有 extra 標記節點（僅需 --graph；與 --dry-run/--json 互斥）\n",
    "  --json                報告 JSON 面\n",
);

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 scip_nodes");
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
    let repo_s = values.get("--repo").and_then(|v| v.clone());
    let dry = values.contains_key("--dry-run");
    let json = values.contains_key("--json");
    let rollback = values.contains_key("--rollback");
    let graph = match values.get("--graph").and_then(|v| v.clone()) {
        Some(g) => PathBuf::from(g),
        None => match &repo_s {
            Some(r) => graph_db_path(Path::new(r)),
            None => return ToolOutput::fail("需 --repo（或 --graph 覆寫——rollback 僅需 graph）"),
        },
    };
    if !graph.exists() {
        return ToolOutput::fail(format!("graph.db 不存在：{}", graph.display()));
    }
    // Rollback resolves BEFORE the index: disaster recovery must not be
    // gated on an index the incident itself may have removed (F1, review
    // 2026-08-26). Mutex guards keep --dry-run from being silently
    // swallowed by a destructive rollback.
    if dry && rollback {
        return ToolOutput::fail("--dry-run 與 --rollback 互斥");
    }
    if json && rollback {
        return ToolOutput::fail("--json 與 --rollback 互斥");
    }
    if rollback {
        return match rollback_nodes(&graph) {
            Ok(n) => ToolOutput {
                stdout: format!(
                    "[OK] scip_nodes rollback：刪除 {n} 個 SCIP 標記節點（graph={}）\n",
                    graph.display()
                ),
                stderr: String::new(),
                exit_code: 0,
            },
            Err(e) => ToolOutput::crash(e),
        };
    }
    let Some(repo_s) = repo_s else {
        return ToolOutput::fail("注入需 --repo（audit 對象）");
    };
    // resolve() at entry: relative/symlinked --repo forms keep the
    // injected file_path canonical (db_functions resolves when counting).
    let repo = resolve(Path::new(&repo_s));
    let index_path = match values.get("--index").and_then(|v| v.clone()) {
        Some(p) => PathBuf::from(p),
        None => match default_index_path(&repo) {
            Ok(p) => p,
            Err(msg) => return ToolOutput::fail(msg),
        },
    };
    if !index_path.exists() {
        return ToolOutput::fail(format!("索引不在：{}", index_path.display()));
    }
    match inject_nodes(&repo, &graph, &index_path, dry) {
        Ok(rep) => {
            let stderr: String = rep.warns.concat();
            if json {
                let v = json!({
                    "missing_total": rep.missing_total,
                    "mapped": rep.mapped,
                    "inserted": rep.inserted,
                    "collision_skipped": rep.collision_skipped,
                    "unmapped": rep.unmapped,
                    "residual_missing": rep.residual_missing,
                    "audit_errors": rep.audit_errors,
                    "dry_run": rep.dry_run,
                    "graph": rep.graph_path.display().to_string(),
                    "backup": rep.backup_path.as_ref().map(|p| p.display().to_string()),
                });
                ToolOutput {
                    stdout: format!("{}\n", to_json_indent1(&v)),
                    stderr,
                    exit_code: 0,
                }
            } else {
                let mode = if rep.dry_run { "（dry-run）" } else { "" };
                let backup = rep
                    .backup_path
                    .as_ref()
                    .map(|p| format!(" backup={}", p.display()))
                    .unwrap_or_default();
                ToolOutput {
                    stdout: format!(
                        "[OK] scip_nodes inject{mode}：missing={} mapped={} inserted={} collision={} unmapped={} residual={} errors={} graph={}{}\n",
                        rep.missing_total,
                        rep.mapped,
                        rep.inserted,
                        rep.collision_skipped,
                        rep.unmapped,
                        rep.residual_missing,
                        rep.audit_errors,
                        rep.graph_path.display(),
                        backup
                    ),
                    stderr,
                    exit_code: 0,
                }
            }
        }
        Err(e) => ToolOutput::crash(e),
    }
}
