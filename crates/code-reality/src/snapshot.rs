//! `snapshot` — the frozen `code_reality/snapshot.py` contract: CRG
//! module-edge export as a commit-anchored sidecar (`snapshot.py:1-283`).
//!
//! stdout faces (byte gate): stale WARN, empty-set WARN, `[OK]`, `[LOG]`.
//! Crashes (missing db, non-CRG db, git failures, tear) map to the
//! crash ToolOutput: empty stdout + exit 1 (D3).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{
    assert_db_unchanged, connect_ro, db_mtime_ns, graph_db_path, make_meta, repo_relative,
    to_json_indent1,
};
use crate::profile::{is_excluded, load_profile, module_of, Profile};
use crate::ToolOutput;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const DEFAULT_OUT_DIR: &str = "~/.mosaic/code-reality/snapshots";

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--label",
            short: None,
            kind: Kind::Value { metavar: "LABEL" },
        },
        FlagSpec {
            long: "--out-dir",
            short: None,
            kind: Kind::Value { metavar: "OUT_DIR" },
        },
    ],
    positionals: &[],
};

/// Usage/help pinned against the local Python oracle (`-h`, prog
/// normalized to the bare tool name; continuation indent follows
/// argparse's prog-length alignment).
const HELP: &str = concat!(
    "usage: snapshot [-h] [--repo REPO] [--label LABEL]\n",
    "                [--out-dir OUT_DIR]\n",
    "\n",
    "弧 snapshot——CRG module-edge 集導出為 commit 錨定 sidecar。\n",
    "\n",
    "options:\n",
    "  -h, --help         show this help message and exit\n",
    "  --repo REPO        repo 根（含 .code-review-graph/）\n",
    "  --label LABEL      EP/弧標籤（記入 _meta）\n",
    "  --out-dir OUT_DIR\n",
);

pub struct EdgeExport {
    pub files: Vec<String>,
    pub module_edges: Vec<Vec<String>>,
    pub raw_edge_count: i64,
}

fn repo_rel_qualified(qualified: &str, repo_root: &Path) -> Option<String> {
    repo_relative(qualified.split("::").next().unwrap_or(qualified), repo_root)
}

/// Full module-edge export (`snapshot.py:53-86`): `files` = the files
/// participating in edges (same-module ends still counted — `files.update`
/// happens before the `src_mod != dst_mod` check); excluded/repo-outside
/// skipped; sorted output; raw count over ALL edge kinds.
pub fn export_module_edges(
    conn: &Connection,
    repo_root: &Path,
    profile: Option<&Profile>,
) -> Result<EdgeExport, String> {
    let repo_root = crate::common::resolve(repo_root);
    let mut edges: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut files: BTreeSet<String> = BTreeSet::new();
    let sql = "SELECT kind, source_qualified, target_qualified FROM edges WHERE kind IN (?1,?2,?3)";
    let kinds = crate::common::EDGE_KINDS;
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("edges 查詢失敗：{}", e))?;
    let rows = stmt
        .query_map(kinds, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("edges 查詢失敗：{}", e))?;
    for row in rows {
        let (kind, src_q, dst_q) = row.map_err(|e| format!("edges 讀取失敗：{}", e))?;
        let (Some(src_rel), Some(dst_rel)) = (
            repo_rel_qualified(&src_q, &repo_root),
            repo_rel_qualified(&dst_q, &repo_root),
        ) else {
            continue;
        };
        if is_excluded(&src_rel, profile) || is_excluded(&dst_rel, profile) {
            continue;
        }
        files.insert(src_rel.clone());
        files.insert(dst_rel.clone());
        let (src_mod, dst_mod) = (module_of(&src_rel, profile), module_of(&dst_rel, profile));
        if src_mod != dst_mod {
            edges.insert((src_mod, dst_mod, kind));
        }
    }
    let raw_edge_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .map_err(|e| format!("edges 計數失敗：{}", e))?;
    Ok(EdgeExport {
        files: files.into_iter().collect(),
        module_edges: edges.into_iter().map(|(s, d, k)| vec![s, d, k]).collect(),
        raw_edge_count,
    })
}

/// `git rev-parse HEAD` with `check=True` semantics (snapshot.py:89-96) —
/// shared strict helper in `common`.
fn head_sha(repo_root: &Path) -> Result<String, String> {
    crate::common::git_rev_parse_head(repo_root)
}

/// HEAD commit time from `git log -1 --format=%cI`: (epoch, iso string).
/// git's strict-ISO output round-trips `fromisoformat().isoformat()`
/// identically, so the original string doubles as the rendering.
fn head_commit_time(repo_root: &Path) -> Result<(i64, String), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("log")
        .arg("-1")
        .arg("--format=%cI")
        .output()
        .map_err(|e| format!("git log 執行失敗：{}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git log -1 --format=%cI 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let epoch = crate::common::parse_iso_to_epoch(&s)
        .ok_or_else(|| format!("HEAD commit time 解析失敗：{s}"))?;
    Ok((epoch, s))
}

/// CRG graph staleness (`snapshot.py:110-141`): sha compare first (exact,
/// no tz assumption), then `last_updated` (naive → local zone), then db
/// mtime. Fresh → None.
pub fn detect_stale(
    meta: &HashMap<String, String>,
    head_sha: Option<&str>,
    head_epoch: i64,
    head_iso: &str,
    db_mtime: Option<(i64, String)>,
) -> Option<String> {
    if let Some(graph_sha) = meta.get("git_head_sha") {
        if !graph_sha.is_empty() {
            // compare against the RAW value (Python :124 compares
            // head_sha_value directly); the "?" substitution is
            // display-only (Python :125 `or '?'`)
            if graph_sha != head_sha.unwrap_or("?") {
                return Some(format!(
                    "graph sha {} != HEAD {}",
                    graph_sha.chars().take(8).collect::<String>(),
                    head_sha.filter(|s| !s.is_empty()).unwrap_or("?")
                ));
            }
            return None;
        }
    }
    if let Some(updated) = meta.get("last_updated") {
        if !updated.is_empty() {
            if let Some(graph_epoch) = crate::common::parse_iso_to_epoch(updated) {
                if graph_epoch < head_epoch {
                    return Some(format!(
                        "graph last_updated {updated} < HEAD commit {head_iso}"
                    ));
                }
                return None;
            }
        }
    }
    if let Some((mtime_epoch, mtime_iso)) = db_mtime {
        if mtime_epoch < head_epoch {
            return Some(format!("graph mtime {mtime_iso} < HEAD commit {head_iso}"));
        }
    }
    None
}

/// metadata load (`snapshot.py:144-169`): empty/half-set (build in
/// progress) retries once after 1s then crashes; non-CRG db (no metadata
/// table / not sqlite) becomes an install-guidance error.
fn load_metadata(db_path: &Path) -> Result<HashMap<String, String>, String> {
    for attempt in 0..2 {
        let conn = connect_ro(db_path)?;
        let rows: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
            let mut s = conn.prepare("SELECT key, value FROM metadata")?;
            let mapped = s
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(mapped)
        })();
        let meta = match rows {
            Ok(pairs) => pairs.into_iter().collect::<HashMap<_, _>>(),
            Err(e) => {
                return Err(format!(
                    "非 CRG graph.db（讀 metadata 失敗：{e}）：{}——先跑 `uvx code-review-graph build`",
                    db_path.display()
                ));
            }
        };
        // Python truthiness: present AND non-empty string counts as set
        let nonempty = meta
            .get("git_head_sha")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || meta
                .get("last_updated")
                .map(|v| !v.is_empty())
                .unwrap_or(false);
        if nonempty {
            return Ok(meta);
        }
        if attempt == 0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(1.0));
        }
    }
    Err(format!(
        "CRG metadata 不完整（build 進行中？）：{}——稍後重跑或 uvx code-review-graph build",
        db_path.display()
    ))
}

/// `--repo` must BE the git root (`snapshot.py:172-185`): rev-parse climbs
/// parents, and a subdirectory would silently anchor the OUTER repo's HEAD.
fn assert_git_root(repo_root: &Path) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map_err(|e| format!("git rev-parse --show-toplevel 執行失敗：{}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse --show-toplevel 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    let top = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if top != repo_root.to_string_lossy() {
        return Err(format!(
            "--repo 指到 {} 但 git root 是 {}——commit 錨定會錯植外層 repo；--repo 須指 repo 根",
            repo_root.display(),
            top
        ));
    }
    Ok(())
}

pub struct Snapshot {
    pub meta: serde_json::Map<String, Value>,
    pub files: Vec<String>,
    pub module_edges: Vec<Vec<String>>,
}

impl Snapshot {
    /// `<repo>-<sha8>.json` (`snapshot.py:196`; 8 chars — a different
    /// source than scip `[SRC]`'s 7).
    pub fn default_path(&self) -> String {
        let repo = self.meta.get("repo").and_then(Value::as_str).unwrap_or("");
        let commit = self
            .meta
            .get("commit")
            .and_then(Value::as_str)
            .unwrap_or("");
        format!(
            "{}-{}.json",
            repo,
            commit.chars().take(8).collect::<String>()
        )
    }

    pub fn write(&self, out_dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(out_dir)
            .map_err(|e| format!("{} 建立失敗：{}", out_dir.display(), e))?;
        let path = out_dir.join(self.default_path());
        let body = to_json_indent1(&json!({
            "_meta": Value::Object(self.meta.clone()),
            "files": self.files,
            "module_edges": self.module_edges,
        }));
        std::fs::write(&path, body).map_err(|e| format!("{} 寫入失敗：{}", path.display(), e))?;
        Ok(path)
    }
}

pub fn build_snapshot(repo_root: &Path, label: Option<&str>) -> Result<Snapshot, String> {
    let repo_root = crate::common::resolve(repo_root);
    let db_path = graph_db_path(&repo_root);
    if !db_path.exists() {
        return Err(format!(
            "graph.db 不存在：{}——先跑 `uvx code-review-graph build`（SM-11）",
            db_path.display()
        ));
    }
    assert_git_root(&repo_root)?;

    let meta_db = load_metadata(&db_path)?;
    let sha = head_sha(&repo_root)?;
    let (head_epoch, head_iso) = head_commit_time(&repo_root)?;
    let md = std::fs::metadata(&db_path)
        .map_err(|e| format!("stat 失敗 {}: {}", db_path.display(), e))?;
    use std::os::unix::fs::MetadataExt;
    let mtime_secs = md.mtime();
    let mtime_nanos = u32::try_from(md.mtime_nsec()).unwrap_or(0);
    let stale_reason = detect_stale(
        &meta_db,
        Some(&sha),
        head_epoch,
        &head_iso,
        Some((
            mtime_secs,
            crate::common::local_epoch_to_iso_auto(mtime_secs, mtime_nanos),
        )),
    );

    let m0 = db_mtime_ns(&db_path)?;
    let profile = load_profile(&repo_root)?;
    let exported = {
        let conn = connect_ro(&db_path)?;
        export_module_edges(&conn, &repo_root, profile.as_ref())
    }?;
    assert_db_unchanged(&db_path, m0)?;

    let mut meta = make_meta("code_reality.snapshot", &repo_root, Some(&sha), vec![])?;
    meta.insert(
        "label".into(),
        match label {
            Some(l) => Value::String(l.to_string()),
            None => Value::Null,
        },
    );
    meta.insert(
        "stale".into(),
        match &stale_reason {
            Some(r) => Value::String(r.clone()),
            None => Value::Null,
        },
    );
    meta.insert(
        "crg_last_updated".into(),
        meta_db
            .get("last_updated")
            .map(|v| Value::String(v.clone()))
            .unwrap_or(Value::Null),
    );
    meta.insert(
        "crg_last_build_type".into(),
        meta_db
            .get("last_build_type")
            .map(|v| Value::String(v.clone()))
            .unwrap_or(Value::Null),
    );
    meta.insert("crg_raw_edges".into(), json!(exported.raw_edge_count));
    Ok(Snapshot {
        meta,
        files: exported.files,
        module_edges: exported.module_edges,
    })
}

// (single export inside the connection scope, tear guard after close —
// mirrors Python's try/finally ordering exactly)

/// Route a `code-reality snapshot ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 snapshot");
    };
    let (values, _positionals) = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok {
            values,
            positionals,
        } => (values, positionals),
    };
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    // user values pass through verbatim (Python type=Path never expands
    // `~`); only the default constant is home-expanded at use time
    let out_dir = values
        .get("--out-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::engine::expand_home(DEFAULT_OUT_DIR));
    let label = values.get("--label").and_then(|v| v.clone());

    let snap = match build_snapshot(&repo, label.as_deref()) {
        Ok(s) => s,
        Err(msg) => return ToolOutput::crash(msg),
    };
    let path = match snap.write(&out_dir) {
        Ok(p) => p,
        Err(msg) => return ToolOutput::crash(msg),
    };
    let mut stdout = String::new();
    if let Some(stale) = snap.meta.get("stale").and_then(Value::as_str) {
        stdout.push_str(&format!(
            "[WARN] CRG graph stale: {stale}——先 uvx code-review-graph build 再 snapshot\n"
        ));
    }
    if snap.files.is_empty() {
        let raw = snap
            .meta
            .get("crg_raw_edges")
            .cloned()
            .unwrap_or(json!(null));
        stdout.push_str(&format!(
            "[WARN] snapshot 空集合（0 files，db raw {raw} 邊）——graph.db 與 --repo 不同 root？下游 transition 會誤報無結構變化\n"
        ));
    }
    stdout.push_str(&format!(
        "[OK] snapshot: {} files, {} module edges -> {}\n",
        snap.files.len(),
        snap.module_edges.len(),
        path.display()
    ));
    stdout.push_str(&format!(
        "[LOG] rg '\"module_edges\"' {} | head\n",
        path.display()
    ));
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}
