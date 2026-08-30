//! Binary freshness core — the single source for every WARN-wired bin
//! (EP ep-cr-freshness-extraction, S1 extraction). Extracted verbatim
//! from `code-reality/src/freshness.rs`; the lsp-bridge's hand-copied
//! variant retired with this crate (the no-workspace-dep clause now
//! reads "no workspace TOOL crate; zero-dep leaf crates excepted").
//!
//! `CR_BUILD_REV` is embedded per consuming crate (each owns a build
//! script — `cargo:rustc-env` scopes to the owning crate), so consumers
//! pass their `option_env!` value in; this crate deliberately carries
//! no build script and no dependencies.

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

fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn checkout_path() -> Option<PathBuf> {
    let p = std::env::var_os("CR_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| expand_home("~/Github/code-reality"));
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

fn git_rev_parse_head(repo: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!head.is_empty()).then_some(head)
}

/// crates-relevant drift only — a docs-only HEAD gap leaves the
/// installed binary functionally current (EP S2; mirrors the
/// post-commit hook's reinstall predicate). Any git failure (e.g. the
/// embedded rev left history after a rebase) conservatively counts as
/// changed (SM-6).
fn crates_changed(repo: &Path, base: &str, head: &str) -> bool {
    let spec = format!("{base}..{head}");
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--name-only", &spec, "--", "crates/"])
        .output()
        .map(|o| {
            if !o.status.success() {
                return true; // git could not answer (e.g. rev left history) — assume changed
            }
            !String::from_utf8_lossy(&o.stdout).trim().is_empty()
        })
        .unwrap_or(true)
}

/// `true` when the running binary lives under a cargo-home bin dir —
/// the dev face that tracks the checkout. Pin-driven installs
/// (`~/.local/bin` via the plugin bootstrap, uvx temp venvs) are silent
/// by design: their version authority is the plugin pin, not HEAD.
pub fn is_dev_face(exe: &Path, cargo_home: &Path) -> bool {
    exe.starts_with(cargo_home.join("bin"))
}

/// Signal-decision core with the repo injected — tests drive it against
/// temp git fixtures (no bin spawn; the S2 exe gate would silence any
/// spawned test binary anyway). Returns the full WARN line, byte-
/// identical to the pre-extraction face, or `None` when fresh.
pub fn staleness(crate_dir: &str, embedded: &str, repo: &Path) -> Option<String> {
    if !repo.join("crates").join(crate_dir).is_dir() {
        return None;
    }
    if let Some(head) = git_rev_parse_head(repo) {
        let base = embedded.strip_suffix("-dirty").unwrap_or(embedded);
        if rev_mismatch(embedded, &head) && crates_changed(repo, base, &head) {
            let short = &head[..head.len().min(7)];
            return Some(format!(
                "[WARN] installed binary {embedded} != repo HEAD {short} — rerun: cargo install --path {}/crates/{crate_dir}",
                repo.display()
            ));
        }
    }
    if status_dirty(repo) {
        return Some(format!(
            "[WARN] CR checkout {} has uncommitted changes under crates/ — installed binary may lag (commit triggers auto-reinstall)",
            repo.display()
        ));
    }
    None
}

/// One-line-per-process staleness warning. Silent when no checkout is
/// present (external consumer machines) or git is unavailable.
pub fn stale_binary_warn(crate_dir: &str, embedded: Option<&str>) {
    if WARNED.set(()).is_err() {
        return;
    }
    let Some(embedded) = embedded else {
        return;
    };
    let Some(repo) = checkout_path() else {
        return;
    };
    // EP S2 gate: only the cargo-installed dev face tracks the checkout.
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| expand_home("~/.cargo"));
    let exe = std::env::current_exe().unwrap_or_default();
    if !is_dev_face(&exe, &cargo_home) {
        return;
    }
    if let Some(warn) = staleness(crate_dir, embedded, &repo) {
        eprintln!("{warn}");
    }
}
