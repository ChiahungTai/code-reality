//! rev_mismatch predicate (table, moved from code-reality's
//! tests/freshness.rs) + the signal-decision core against temp git
//! fixtures — repo injected, no bin spawn (EP S1/F3 test form).

use cr_freshness::{rev_mismatch, staleness};
use std::path::PathBuf;
use std::process::Command;

const HEAD: &str = "2442692582edb2031f07a820da94b4b921f84888";

#[test]
fn rev_mismatch_table() {
    // same commit: abbreviated embedded prefixes the full head hash
    assert!(!rev_mismatch("2442692", HEAD));
    // dirty install is not a stale one
    assert!(!rev_mismatch("2442692-dirty", HEAD));
    // abbreviation-length drift (8-char embed of the same commit)
    assert!(!rev_mismatch("24426925", HEAD));
    // different commit
    assert!(rev_mismatch("3980fe1", HEAD));
    assert!(rev_mismatch("3980fe1-dirty", HEAD));
    // absent/unusable embedded face never warns
    assert!(!rev_mismatch("", HEAD));
    assert!(!rev_mismatch("unknown", HEAD));
}

fn git(repo: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git fixture")
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

fn commit_all(repo: &std::path::Path, msg: &str) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            msg,
        ],
    );
}

fn head_of(repo: &std::path::Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn fixture_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("cr");
    std::fs::create_dir_all(repo.join("crates/code-reality")).unwrap();
    git(&repo, &["init", "-q", "."]);
    for (path, body) in files {
        let p = repo.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    commit_all(&repo, "fixture base");
    (tmp, repo)
}

#[test]
fn staleness_silent_on_docs_only_gap() {
    // S2: a docs-only HEAD gap leaves the binary functionally current —
    // no WARN (the 2026-08-30 tour_validate misdiagnosis bait dies here).
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    std::fs::write(repo.join("docs.md"), "docs").unwrap();
    commit_all(&repo, "docs only");
    assert_eq!(staleness("code-reality", &embedded, &repo), None);
}

#[test]
fn staleness_lagged_when_crates_commit_moves() {
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    std::fs::write(repo.join("crates/code-reality/x.rs"), "fn b(){}").unwrap();
    commit_all(&repo, "crates change");
    let out = staleness("code-reality", &embedded, &repo);
    assert!(
        out.as_deref().unwrap_or_default().contains("!= repo HEAD"),
        "{out:?}"
    );
}

#[test]
fn staleness_dirty_embedded_stripped_before_crates_diff() {
    // F2: a dirty-tree install embeds `<rev>-dirty`; the strip must
    // happen before the crates diff or every later docs-only gap would
    // fail to resolve the rev and warn.
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    std::fs::write(repo.join("docs.md"), "docs").unwrap();
    commit_all(&repo, "docs only");
    assert_eq!(
        staleness("code-reality", &format!("{embedded}-dirty"), &repo),
        None
    );
}

#[test]
fn staleness_unknown_rev_conservatively_lags() {
    // SM-6: an embedded rev that left history (rebase) makes the git
    // diff fail — the conservative answer is WARN, not silence.
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let out = staleness("code-reality", "deadbeef0", &repo);
    assert!(
        out.as_deref().unwrap_or_default().contains("!= repo HEAD"),
        "{out:?}"
    );
}

#[test]
fn is_dev_face_component_wise_prefix() {
    use cr_freshness::is_dev_face;
    use std::path::Path;
    let home = Path::new("/Users/x/.cargo");
    assert!(is_dev_face(
        Path::new("/Users/x/.cargo/bin/code-reality"),
        home
    ));
    assert!(!is_dev_face(
        Path::new("/Users/x/.local/bin/code-reality"),
        home
    ));
    // component-wise: siblings and look-alikes never match
    assert!(!is_dev_face(Path::new("/Users/x/.cargo-bin/x"), home));
    assert!(!is_dev_face(Path::new("/Users/x/.cargo/bin-old/x"), home));
}

#[test]
fn staleness_dirty_checkout() {
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    std::fs::write(repo.join("crates/code-reality/x.rs"), "fn b(){}").unwrap();
    let out = staleness("code-reality", &embedded, &repo);
    assert!(
        out.as_deref()
            .unwrap_or_default()
            .contains("uncommitted changes"),
        "{out:?}"
    );
}

#[test]
fn staleness_fresh_is_none() {
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    assert_eq!(staleness("code-reality", &embedded, &repo), None);
}

#[test]
fn staleness_unknown_crate_dir_is_silent() {
    let (_tmp, repo) = fixture_repo(&[("crates/code-reality/x.rs", "fn a(){}")]);
    let embedded = head_of(&repo);
    std::fs::write(repo.join("docs.md"), "docs").unwrap();
    commit_all(&repo, "docs only");
    assert_eq!(staleness("some-other-crate", &embedded, &repo), None);
}
