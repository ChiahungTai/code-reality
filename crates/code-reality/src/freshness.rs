//! Binary freshness face (EP ep-binary-freshness-face): one stderr line
//! per process when the installed binary's embedded build rev disagrees
//! with the CR checkout, or when the checkout carries uncommitted crate
//! edits (the install-lag trap of 2026-08-28: `cargo install` ran before
//! source edits, so live checks executed a stale binary silently).
//! Non-parity runtime face — deliberately kept out of the frozen
//! `common.rs` byte-parity contract module.
//!
//! Consumers: `code-reality` + `code-reality-mcp` + `pyrefly-index` bins.
//! `code-reality-lsp-bridge` carries a local copy (the crate depends on no
//! workspace crate — keep the two in sync when changing this file).

use std::path::{Path, PathBuf};

static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// `true` when the checkout HEAD does not start with the embedded rev.
/// The `-dirty` suffix is stripped before comparing (a dirty install is
/// not a stale one) and prefix matching is immune to abbreviation-length
/// drift on the embedded side.
pub fn rev_mismatch(embedded: &str, head_full: &str) -> bool {
    let base = embedded.strip_suffix("-dirty").unwrap_or(embedded);
    !base.is_empty() && base != "unknown" && !head_full.starts_with(base)
}

/// The `<pkg>[+<rev>]` version face shared by this crate's bins
/// (umbrella route arm + the mcp bin). The cross-crate bins
/// (pyrefly-index, lsp-bridge) keep their local copies — the
/// no-workspace-dep clause.
pub fn version_face() -> String {
    match option_env!("CR_BUILD_REV") {
        Some(r) => format!("{}+{}", env!("CARGO_PKG_VERSION"), r),
        None => env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn checkout_path() -> Option<PathBuf> {
    let p = std::env::var_os("CR_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::engine::expand_home("~/Github/code-reality"));
    p.is_dir().then_some(p)
}

fn status_dirty(repo: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain", "--", "crates/"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// One-line-per-process staleness warning. Silent when no checkout is
/// present (external consumer machines) or git is unavailable.
pub fn stale_binary_warn(crate_dir: &str) {
    if WARNED.set(()).is_err() {
        return;
    }
    let Some(embedded) = option_env!("CR_BUILD_REV") else {
        return;
    };
    let Some(repo) = checkout_path() else {
        return;
    };
    if !repo.join("crates").join(crate_dir).is_dir() {
        return;
    }
    if let Ok(head) = crate::common::git_rev_parse_head(&repo) {
        if rev_mismatch(embedded, &head) {
            let short = &head[..head.len().min(7)];
            eprintln!(
                "[WARN] installed binary {embedded} != repo HEAD {short} — rerun: cargo install --path {}/crates/{crate_dir}",
                repo.display()
            );
            return;
        }
    }
    if status_dirty(&repo) {
        eprintln!(
            "[WARN] CR checkout {} has uncommitted changes under crates/ — installed binary may lag (commit triggers auto-reinstall)",
            repo.display()
        );
    }
}
