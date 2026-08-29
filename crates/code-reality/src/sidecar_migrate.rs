//! `sidecar_migrate` — one-shot migration of the retired home sidecar
//! face (EP data-plane-unification S2): moves the basename-keyed scip
//! slot, `<repo>-*` snapshots and the golden baseline from
//! `~/.mosaic/code-reality/` into `<repo>/.code-reality/`.
//!
//! Semantics (EP S2 + review F5-2/F5-4/F4-5):
//! - the scip slot moves as a directory (atomic same-volume rename — the
//!   producer-invalidation contract assumes the sidecar files stay a
//!   consistent set; a per-file move would split their mtime relation
//!   mid-interruption);
//! - `index.union.db` is a knowingly-dropped dead artifact (the union
//!   plane materialized inside graph.db since v1+ S4);
//! - dual presence never overwrites — WARN with both paths for manual
//!   adjudication (data integrity first);
//! - reruns converge to zero actions (idempotent — same-volume rename
//!   path only: an interrupted EXDEV fallback leaves a partial in-repo
//!   slot that dual-presence WARNs until a human removes it;
//!   intentional stop-loss, data integrity first);
//! - the data dir gets the self-contained single-`*` gitignore.
//!
//! Not concurrent-safe: the exists-check → rename window is a TOCTOU
//! seam; this is a one-shot single-user CLI by design.
//!
//! Known limitations (attribution is unknowable to a generic tool):
//! - boundary dbs — sha-keyed with no repo identity in the filename;
//!   the NT pair moved as a one-off data op in the EP;
//! - a basename-collision slot (another repo's data under this name)
//!   migrates like any other — pollution is undetectable here; the
//!   downstream staleness/mtime gates are the backstop;
//! - snapshot matching is dash-delimited (`name-`), so a repo `foo`
//!   would also claim `foo-bar-*` snapshots — flat names cannot
//!   disambiguate; none of the migrated repos collide.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::engine::{expand_home, resolve_repo, write_data_dir_gitignore};
use crate::ToolOutput;
use std::path::{Path, PathBuf};

/// errno EXDEV (cross-device rename) — Linux/macOS both 18; the Windows
/// stub face is out of scope (EP NOT).
const EXDEV: i32 = 18;

/// Retired home sidecar root (pre data-plane-unification); the
/// transitional bridge (`cli` missing-index error) and this tool's
/// `--home` default both key off it.
pub const RETIRED_HOME_ROOT: &str = "~/.mosaic/code-reality";

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--home",
            short: None,
            kind: Kind::Value {
                metavar: "HOME_SIDECAR_ROOT",
            },
        },
    ],
    positionals: &[],
};

const HELP: &str = "usage: code-reality sidecar_migrate --repo <repo> [--home <root>]
  --repo REPO              repo root whose sidecar face migrates in-repo
  --home HOME_SIDECAR_ROOT retired home root (default ~/.mosaic/code-reality)
";

#[derive(Default, Debug)]
pub struct Report {
    pub moved: Vec<String>,
    pub dropped: Vec<String>,
    pub warnings: Vec<String>,
    /// the data-dir .gitignore was absent and got written this run
    pub ensured_gitignore: bool,
}

/// Migrate the retired home sidecar face of `repo` into
/// `<repo>/.code-reality/`. `home_root` is injectable for tests.
pub fn migrate_repo(repo: &Path, home_root: &Path) -> Result<Report, String> {
    let resolved = resolve_repo(repo);
    if !resolved.is_dir() {
        return Err(format!(
            "--repo {} 不是目錄——請確認路徑（不建立目錄）",
            repo.display()
        ));
    }
    let name = resolved
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("--repo {} 解析不出 repo 名", repo.display()))?;
    let data_root = resolved.join(".code-reality");
    let ensured = !data_root.join(".gitignore").exists();
    let mut r = Report {
        ensured_gitignore: ensured,
        ..Default::default()
    };
    write_data_dir_gitignore(&data_root)?;

    // 1) scip slot — directory-level move
    let old_slot = home_root.join("scip").join(&name);
    if old_slot.is_dir() {
        let new_slot = data_root.join("scip");
        if new_slot.exists() {
            r.warnings.push(format!(
                "兩邊都在（不覆寫）：{} vs {}——請人工裁決後重跑",
                old_slot.display(),
                new_slot.display()
            ));
        } else {
            let union = old_slot.join("index.union.db");
            if union.is_file() {
                std::fs::remove_file(&union)
                    .map_err(|e| format!("刪除死 artifact {}: {e}", union.display()))?;
                r.dropped.push("index.union.db".to_string());
            }
            move_dir(&old_slot, &new_slot)?;
            r.moved.push(format!("scip slot → {}", new_slot.display()));
        }
    }

    // 2) snapshots — exact `name` or dash-delimited `name-` prefix only
    // (mosaic_alpha vs mosaic_alpha_offline_backtesting never collide)
    let old_s = home_root.join("snapshots");
    if old_s.is_dir() {
        let new_s = data_root.join("snapshots");
        let prefix = format!("{name}-");
        for f in list_files(&old_s)? {
            let fname = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if fname != name && !fname.starts_with(&prefix) {
                continue;
            }
            if move_file(&f, &new_s.join(&fname), &mut r)? {
                r.moved.push(format!("snapshot: {fname}"));
            }
        }
    }

    // 3) golden baseline (reconciliation artifact, no programmatic reader)
    let old_g = home_root.join("golden").join(format!("{name}.json"));
    if old_g.is_file() {
        let new_g = data_root.join("golden");
        if move_file(&old_g, &new_g.join(format!("{name}.json")), &mut r)? {
            r.moved.push(format!("golden → {}", new_g.display()));
        }
    }

    Ok(r)
}

fn list_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("讀 {}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    Ok(v)
}

fn move_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{} 建立失敗：{e}", parent.display()))?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            copy_dir_verified(src, dst)?;
            std::fs::remove_dir_all(src).map_err(|e| format!("刪源 {}: {e}", src.display()))
        }
        Err(e) => Err(format!("rename {} → {}: {e}", src.display(), dst.display())),
    }
}

/// `Ok(true)` = moved; `Ok(false)` = dual presence (kept both, warned).
fn move_file(src: &Path, dst: &Path, r: &mut Report) -> Result<bool, String> {
    if dst.exists() {
        r.warnings.push(format!(
            "兩邊都在（不覆寫）：{} vs {}",
            src.display(),
            dst.display()
        ));
        return Ok(false);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{} 建立失敗：{e}", parent.display()))?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(true),
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            copy_verified(src, dst)?;
            std::fs::remove_file(src).map_err(|e| format!("刪源 {}: {e}", src.display()))?;
            Ok(true)
        }
        Err(e) => Err(format!("rename {} → {}: {e}", src.display(), dst.display())),
    }
}

/// `cp -p` preserves mtime — the mtime gates depend on it. Size-verified.
pub fn copy_verified(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{} 建立失敗：{e}", parent.display()))?;
    }
    let status = std::process::Command::new("cp")
        .arg("-p")
        .arg(src)
        .arg(dst)
        .status()
        .map_err(|e| format!("spawn cp: {e}"))?;
    if !status.success() {
        return Err(format!("cp -p {} → {} 失敗", src.display(), dst.display()));
    }
    match (src.metadata(), dst.metadata()) {
        (Ok(s), Ok(d)) if s.len() == d.len() => Ok(()),
        _ => Err(format!(
            "拷貝驗證失敗（size 不符）：{} → {}",
            src.display(),
            dst.display()
        )),
    }
}

fn copy_dir_verified(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("{} 建立失敗：{e}", dst.display()))?;
    for e in std::fs::read_dir(src).map_err(|e| format!("讀 {}: {e}", src.display()))? {
        let entry = e.map_err(|e| e.to_string())?;
        let p = entry.path();
        let t = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_verified(&p, &t)?;
        } else {
            copy_verified(&p, &t)?;
        }
    }
    Ok(())
}

/// The retired home slot's index path for `repo`, if present — the
/// transitional missing-index bridge face (detection is the index file
/// specifically; cache-only leftovers don't trigger, they are not what
/// a query misses).
pub fn old_slot_index(home_root: &Path, repo: &Path) -> Option<PathBuf> {
    let resolved = crate::engine::resolve_repo(repo);
    let name = resolved.file_name()?;
    let old = home_root.join("scip").join(name).join("index.scip");
    old.exists().then_some(old)
}

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((_tool, toks)) = argv.split_first() else {
        return ToolOutput::fail(HELP.trim_end());
    };
    let values = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, .. } => values,
    };
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let home = values
        .get("--home")
        .and_then(|v| v.clone())
        .map(|v| expand_home(&v))
        .unwrap_or_else(|| expand_home(RETIRED_HOME_ROOT));

    match migrate_repo(Path::new(&repo), &home) {
        Ok(r) => {
            let mut out = format!("[OK] sidecar_migrate: {repo}\n");
            if r.ensured_gitignore {
                out.push_str("  ensured: 資料目錄自帶 .gitignore\n");
            }
            if r.moved.is_empty()
                && r.dropped.is_empty()
                && r.warnings.is_empty()
                && !r.ensured_gitignore
            {
                out.push_str("  無動作（冪等）\n");
            }
            for m in &r.moved {
                out.push_str(&format!("  moved: {m}\n"));
            }
            for d in &r.dropped {
                out.push_str(&format!(
                    "  dropped: {d}（死 artifact——union 面已物化進 graph.db）\n"
                ));
            }
            for w in &r.warnings {
                out.push_str(&format!("[WARN] {w}\n"));
            }
            ToolOutput {
                stdout: out,
                stderr: String::new(),
                exit_code: 0,
            }
        }
        Err(e) => ToolOutput::crash(format!("sidecar_migrate: {e}")),
    }
}
