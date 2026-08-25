//! `boundary` — the frozen `code_reality/boundary.py` contract: python
//! symbol → Rust truth (path:line + match_kind) from the boundary_build
//! sidecar. Same-name double declarations list all edges (not an error);
//! not-found exits 1 with disambiguation candidates.

use crate::boundary_build::DEFAULT_OUT_DIR;
use crate::common::{assert_db_unchanged, connect_ro, db_mtime_ns};
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One boundary edge row (py_symbol, pyi_path, pyi_line, rs_symbol,
/// rs_path, rs_line, match_kind, method_kind).
pub type EdgeRow = (String, String, i64, String, String, i64, String, Option<String>);

pub struct LoadedSidecar {
    pub conn: Connection,
    pub meta: BTreeMap<String, String>,
    pub db: PathBuf,
}

fn read_meta(db: &Path) -> Result<BTreeMap<String, String>, String> {
    let meta_err = |e: rusqlite::Error| {
        format!(
            "非 boundary sidecar（讀 meta 失敗：{e}）：{}——sidecar 目錄混入外部 .db？",
            db.display()
        )
    };
    let conn = connect_ro(db).map_err(|e| {
        format!(
            "非 boundary sidecar（讀 meta 失敗：{e}）：{}——sidecar 目錄混入外部 .db？",
            db.display()
        )
    })?;
    let mut s = conn
        .prepare("SELECT key, value FROM meta")
        .map_err(&meta_err)?;
    let mapped = s
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(&meta_err)?;
    let mut out = BTreeMap::new();
    for row in mapped {
        let (k, v) = row.map_err(&meta_err)?;
        out.insert(k, v);
    }
    Ok(out)
}

/// Load the sidecar (`boundary.py:47-83`): prefer the db whose
/// `nt_commit` == the NT repo HEAD; otherwise mtime-latest with a WARN
/// rerun hint (never a silent fallback).
pub fn load_sidecar(
    nt_repo: &Path,
    sidecar_dir: &Path,
    head: Option<&str>,
) -> Result<(LoadedSidecar, String), ToolOutput> {
    let mut stdout = String::new();
    let mut dbs: Vec<PathBuf> = if sidecar_dir.is_dir() {
        glob_files(sidecar_dir, "*.db")
    } else {
        Vec::new()
    };
    dbs.sort();
    if dbs.is_empty() {
        return Err(ToolOutput::crash(format!(
            "boundary sidecar 不存在：{}——先跑 `uv run python -m code_reality.boundary_build`",
            sidecar_dir.display()
        )));
    }
    let head = match head {
        Some(h) => Some(h.to_string()),
        None if nt_repo.is_dir() => crate::boundary_build::nt_head_sha(nt_repo).ok(),
        None => None,
    };
    if let Some(head) = &head {
        for db in &dbs {
            match read_meta(db) {
                Ok(meta) => {
                    if meta.get("nt_commit").map(String::as_str) == Some(head.as_str()) {
                        let conn = connect_ro(db).map_err(ToolOutput::crash)?;
                        return Ok((
                            LoadedSidecar { conn, meta, db: db.clone() },
                            stdout,
                        ));
                    }
                }
                Err(_) => {
                    // foreign .db in the dir — tolerate and skip (R7)
                    stdout.push_str(&format!("[WARN] 非 boundary sidecar，跳過：{}\n", db.display()));
                }
            }
        }
    }
    let latest = dbs
        .iter()
        .max_by_key(|p| {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(p).map(|m| m.mtime()).unwrap_or(0)
        })
        .unwrap()
        .clone();
    let meta = read_meta(&latest).map_err(ToolOutput::crash)?;
    if let Some(head) = &head {
        stdout.push_str(&format!(
            "[WARN] sidecar 落後（sidecar {} vs NT HEAD {}）——建議重跑 uv run python -m code_reality.boundary_build\n",
            meta.get("nt_commit").cloned().unwrap_or_else(|| "?".into()).chars().take(8).collect::<String>(),
            head.chars().take(8).collect::<String>()
        ));
    }
    let conn = connect_ro(&latest).map_err(ToolOutput::crash)?;
    Ok((LoadedSidecar { conn, meta, db: latest }, stdout))
}

fn glob_files(dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut opts = glob::MatchOptions::new();
    opts.require_literal_leading_dot = false;
    glob::glob_with(&dir.join(pattern).to_string_lossy(), opts)
        .into_iter()
        .flatten()
        .filter_map(|r| r.ok())
        .collect()
}

/// LIKE-pattern escape: `%`/`_` are wildcards — symbol matching must be
/// exact (`boundary.py:86-88`).
fn like_escape(symbol: &str) -> String {
    symbol.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn query_rows(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<EdgeRow>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("查詢失敗：{e}"))?;
    let mapped = stmt
        .query_map(params, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| format!("查詢失敗：{e}"))?;
    let mut out = Vec::new();
    for row in mapped {
        out.push(row.map_err(|e| format!("讀取失敗：{e}"))?);
    }
    Ok(out)
}

pub fn query_py(conn: &Connection, symbol: &str) -> Result<Vec<EdgeRow>, String> {
    let pat = like_escape(symbol);
    let sql = "SELECT py_symbol, pyi_path, pyi_line, rs_symbol, rs_path, rs_line, match_kind, method_kind \
               FROM boundary_edges WHERE py_symbol = ?1 OR py_symbol LIKE ?2 ESCAPE '\\' \
               OR py_symbol LIKE ?3 ESCAPE '\\' OR py_symbol LIKE ?4 ESCAPE '\\' \
               ORDER BY py_symbol, rs_path, rs_line";
    query_rows(conn, sql, &[&symbol, &format!("%.{pat}"), &format!("{pat}.%"), &format!("%.{pat}.%")])
}

pub fn query_rs(conn: &Connection, symbol: &str) -> Result<Vec<EdgeRow>, String> {
    let pat = like_escape(symbol);
    let sql = "SELECT py_symbol, pyi_path, pyi_line, rs_symbol, rs_path, rs_line, match_kind, method_kind \
               FROM boundary_edges WHERE rs_symbol = ?1 OR rs_symbol LIKE ?2 ESCAPE '\\' \
               ORDER BY py_symbol, rs_path, rs_line";
    query_rows(conn, sql, &[&symbol, &format!("%::{pat}")])
}

fn candidates(conn: &Connection, table_col: &str, seg: &str) -> Vec<String> {
    let sql = format!(
        "SELECT DISTINCT {table_col} FROM boundary_edges WHERE {table_col} LIKE ?1 ORDER BY {table_col} LIMIT 10"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else { return Vec::new() };
    stmt.query_map([format!("%{seg}%")], |r| r.get::<_, String>(0))
        .map(|i| i.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// Query + hub_refs-style output (`boundary.py:142-181`); not-found →
/// [FAIL] + candidates on stdout, exit 1. mtime tear guard around the
/// read (same-sha rebuild overwrites mid-read).
pub fn run_query(
    sc: &LoadedSidecar,
    symbol: &str,
    rs_mode: bool,
) -> ToolOutput {
    let m0 = match db_mtime_ns(&sc.db) {
        Ok(v) => v,
        Err(e) => return ToolOutput::crash(e),
    };
    let rows = if rs_mode {
        query_rs(&sc.conn, symbol)
    } else {
        query_py(&sc.conn, symbol)
    };
    let rows = match rows {
        Ok(r) => r,
        Err(e) => return ToolOutput::crash(e),
    };
    let cands = if rows.is_empty() {
        if rs_mode {
            let seg = symbol.rsplit("::").next().unwrap_or(symbol);
            candidates(&sc.conn, "rs_symbol", seg)
        } else {
            let seg = symbol.rsplit('.').next().unwrap_or(symbol).rsplit("::").next().unwrap_or(symbol);
            candidates(&sc.conn, "py_symbol", seg)
        }
    } else {
        Vec::new()
    };
    if let Err(e) = assert_db_unchanged(&sc.db, m0) {
        return ToolOutput::crash(e);
    }
    if rows.is_empty() {
        let mut stdout = format!(
            "[FAIL] symbol not found: {symbol}（sidecar {}）\n",
            sc.db.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        );
        for c in &cands {
            stdout.push_str(&format!("  候選: {c}\n"));
        }
        return ToolOutput {
            stdout,
            stderr: format!("symbol not found: {symbol}\n"),
            exit_code: 1,
        };
    }
    let mut stdout = format!(
        "[OK] {symbol}: {} edges（sidecar {} @ {}）\n",
        rows.len(),
        sc.meta.get("nt_commit").cloned().unwrap_or_else(|| "?".into()).chars().take(8).collect::<String>(),
        sc.db.display()
    );
    for (py, pyi_path, pyi_line, rs_sym, rs_path, rs_line, kind, _) in &rows {
        stdout.push_str(&format!(
            "  {py}  {kind}  {rs_path}:{rs_line}  <- pyi {pyi_path}:{pyi_line}  {rs_sym}\n"
        ));
    }
    let col = if rs_mode { "rs_symbol" } else { "py_symbol" };
    stdout.push_str(&format!(
        "[LOG] sqlite3 {} 'SELECT * FROM boundary_edges WHERE {} LIKE \"%{symbol}%\"'\n",
        sc.db.display(),
        col
    ));
    ToolOutput { stdout, stderr: String::new(), exit_code: 0 }
}

/// Route a `code-reality boundary ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 boundary");
    };
    let spec = crate::argparse::ToolSpec {
        flags: &[
            crate::argparse::FlagSpec { long: "--repo", short: None, kind: crate::argparse::Kind::Value { metavar: "REPO" } },
            crate::argparse::FlagSpec { long: "--rs", short: None, kind: crate::argparse::Kind::StoreTrue },
            crate::argparse::FlagSpec { long: "--sidecar-dir", short: None, kind: crate::argparse::Kind::Value { metavar: "SIDECAR_DIR" } },
        ],
        positionals: &["symbol"],
    };
    let (values, positionals) = match crate::argparse::parse(&spec, toks) {
        crate::argparse::Outcome::Help => {
            return ToolOutput {
                stdout: concat!(
                    "usage: boundary [-h] --repo REPO [--rs] [--sidecar-dir SIDECAR_DIR]\n",
                    "                symbol\n",
                    "\n",
                    "boundary 查詢：python 符號 → Rust 真身\n",
                    "\n",
                    "positional arguments:\n",
                    "  symbol                python 符號（nautilus_trader.live.LiveNode 或裸名 LiveNode）；--rs 時為 Rust 符號\n",
                    "\n",
                    "options:\n",
                    "  -h, --help            show this help message and exit\n",
                    "  --repo REPO           sidecar 對應的 repo 根（stale 比對用；顯式必給——SM-1b）\n",
                    "  --rs                  反向查詢：Rust 符號 → python 面\n",
                    "  --sidecar-dir SIDECAR_DIR\n",
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        crate::argparse::Outcome::Err(msg) => return ToolOutput::fail(msg),
        crate::argparse::Outcome::Ok { values, positionals } => (values, positionals),
    };
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let sidecar_dir = values
        .get("--sidecar-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::engine::expand_home(DEFAULT_OUT_DIR));
    let (sc, warn) = match load_sidecar(Path::new(&repo), &sidecar_dir, None) {
        Ok(v) => v,
        Err(out) => return out,
    };
    let mut out = run_query(&sc, &positionals[0], values.contains_key("--rs"));
    out.stdout.insert_str(0, &warn);
    out
}
