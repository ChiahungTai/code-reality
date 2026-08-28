//! Embed the workspace git rev as CR_BUILD_REV (EP ep-binary-freshness-face).
//! `git describe --always --dirty --exclude=*` guarantees a hash-only form
//! (`--exclude=*` drops every tag candidate so `--always` falls back to the
//! abbreviated hash — release tags exist since v0.2.0 and the exclude keeps
//! the embedded rev hash-only regardless). Same-branch commits update
//! the branch ref file, not HEAD — all three (HEAD, the resolved loose ref,
//! packed-refs) must be rerun triggers or the embedded rev silently lags.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&manifest)
            .output()
    };
    let (gitdir, rev) = match (
        run(&["rev-parse", "--git-dir"]),
        run(&["describe", "--always", "--dirty", "--exclude=*"]),
    ) {
        (Ok(g), Ok(d)) if g.status.success() && d.status.success() => (
            PathBuf::from(String::from_utf8_lossy(&g.stdout).trim()),
            String::from_utf8_lossy(&d.stdout).trim().to_string(),
        ),
        _ => {
            println!("cargo:warning=git metadata absent — CR_BUILD_REV not emitted");
            return;
        }
    };
    println!("cargo:rustc-env=CR_BUILD_REV={rev}");
    // HEAD is per-worktree (lives in the worktree gitdir); branch refs
    // and packed-refs live in the COMMON dir — a linked worktree's
    // `--git-dir` is private and holds neither (review R3).
    println!("cargo:rerun-if-changed={}", gitdir.join("HEAD").display());
    let common = run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| gitdir.clone());
    if let Ok(head) = std::fs::read_to_string(common.join("HEAD")) {
        if let Some(branch) = head.strip_prefix("ref: ") {
            let r = common.join(branch.trim());
            if r.exists() {
                println!("cargo:rerun-if-changed={}", r.display());
            }
        }
    }
    let packed = common.join("packed-refs");
    if packed.exists() {
        println!("cargo:rerun-if-changed={}", packed.display());
    }
}
