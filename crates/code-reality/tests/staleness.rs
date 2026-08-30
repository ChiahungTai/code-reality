//! Staleness primitives (EP ep-index-query-time-self-heal S2):
//! walk_sources / evaluate_staleness / doc_set_delta — the
//! index↔source guard layer feeding the WARN faces (S2) and the
//! auto-heal (S3).

use code_reality::engine::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn mkrepo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    for (rel, content) in files {
        let p = t.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let path = t.path().to_path_buf();
    (t, path)
}

fn slot_with(repo: &Path, bytes: &[u8]) -> PathBuf {
    let dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&dir).unwrap();
    let slot = dir.join("index.scip");
    std::fs::write(&slot, bytes).unwrap();
    slot
}

const ANY_BYTES: &[u8] = b"placeholder-not-parsed-here";

#[test]
fn walk_sources_skips_and_collects() {
    let (t, repo) = mkrepo(&[
        ("a.py", "x"),
        ("sub/g.rs", "x"),
        ("target/b.py", "x"), // NOT skipped (superset-walk rule)
        ("venv/c.py", "x"),
        ("node_modules/d.py", "x"),
        ("__pycache__/e.py", "x"),
        (".hidden/f.py", "x"),
        ("notes.txt", "x"),
    ]);
    let w = walk_sources(&repo).unwrap();
    assert!(w.py.contains("a.py"));
    assert!(w.py.contains("target/b.py"), "target stays walkable");
    assert!(!w.py.contains("venv/c.py"));
    assert!(!w.py.contains("node_modules/d.py"));
    assert!(!w.py.contains("__pycache__/e.py"));
    assert!(!w.py.contains(".hidden/f.py"));
    assert!(w.rs.contains("sub/g.rs"));
    assert!(w.newest.is_some());
    drop(t);
}

#[test]
fn evaluate_staleness_trigger_split() {
    // fresh: slot written after all sources
    let (t, repo) = mkrepo(&[("a.py", "x")]);
    let slot = slot_with(&repo, ANY_BYTES);
    let s = evaluate_staleness(&repo, &slot).unwrap();
    assert!(!s.source_newer);
    assert_eq!(s.head_drift, None, "unstamped → no head info");

    // edit after slot → trigger (SM-3, the line-drift incident shape)
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("a.py"), "y").unwrap();
    assert!(evaluate_staleness(&repo, &slot).unwrap().source_newer);

    // re-freshen slot, then a NEW file → trigger (SM-2, missing-file shape)
    std::fs::write(&slot, ANY_BYTES).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("new_mod.py"), "z").unwrap();
    assert!(evaluate_staleness(&repo, &slot).unwrap().source_newer);
    drop(t);
}

#[test]
fn evaluate_staleness_head_drift_without_git() {
    // non-git repo with a stamped meta: git_head fails → no head info,
    // never an extra warn here (SM-16 — source_line owns the single one)
    let (t, repo) = mkrepo(&[("a.py", "x")]);
    let slot = slot_with(&repo, ANY_BYTES);
    std::fs::write(
        meta_path(&slot),
        "{\"head\": \"deadbeef\", \"repo\": \"x\"}\n",
    )
    .unwrap();
    let s = evaluate_staleness(&repo, &slot).unwrap();
    assert_eq!(s.head_drift, None);
    drop(t);
}

#[test]
fn evaluate_staleness_head_drift_in_git_repo() {
    let (t, repo) = mkrepo(&[("a.py", "x")]);
    for args in [
        vec!["init", "-q"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "x",
        ],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
    let slot = slot_with(&repo, ANY_BYTES);
    std::fs::write(meta_path(&slot), "{\"head\": \"deadbeef\"}\n").unwrap();
    assert_eq!(
        evaluate_staleness(&repo, &slot).unwrap().head_drift,
        Some(true)
    );
    let head = git_head(&repo).unwrap();
    std::fs::write(meta_path(&slot), format!("{{\"head\": \"{head}\"}}\n")).unwrap();
    assert_eq!(
        evaluate_staleness(&repo, &slot).unwrap().head_drift,
        Some(false)
    );
    drop(t);
}

#[test]
fn walk_sources_rust_face_skips_target() {
    let (t, repo) = mkrepo(&[
        ("crates/a.rs", "x"),
        ("target/debug/out/gen.rs", "x"), // OUT_DIR artifact — never corpus
        ("target/tool.py", "x"),          // pyrefly DOES index .py under target
    ]);
    let w = walk_sources(&repo).unwrap();
    assert!(w.rs.contains("crates/a.rs"));
    assert!(
        !w.rs.contains("target/debug/out/gen.rs"),
        "rust face skips target/"
    );
    assert!(w.py.contains("target/tool.py"), "python face keeps target/");
    drop(t);
}

#[test]
fn walk_sources_newest_is_max_and_read_failure_is_err() {
    use std::os::unix::fs::PermissionsExt;
    // newest == max over NON-target sources: the newest file overall is
    // an OUT_DIR .rs (written last) — it must not win the trigger signal
    let (t, repo) = mkrepo(&[("a.py", "x"), ("b.py", "x"), ("target/gen.rs", "x")]);
    std::thread::sleep(std::time::Duration::from_millis(15));
    std::fs::write(repo.join("b.py"), "y").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(15));
    std::fs::write(repo.join("target/gen.rs"), "y").unwrap(); // newest overall
    let w = walk_sources(&repo).unwrap();
    let b_m = repo.join("b.py").metadata().unwrap().modified().unwrap();
    assert_eq!(
        w.newest,
        Some(b_m),
        "newest is the newest non-target source"
    );

    // read failure → Err (fail-loud, never an empty walk posing as fresh)
    let locked = repo.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("z.py"), "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let r = walk_sources(&repo);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(r.is_err(), "walk error must surface, got {:?}", r.ok());
    drop(t);
}

#[test]
fn doc_set_delta_face_scoped_missing_extra() {
    let (_t, repo) = mkrepo(&[("a.py", "x"), ("b.py", "x"), ("c.rs", "x")]);
    let w = walk_sources(&repo).unwrap();

    // python-face index: .rs not compared (face scoping)
    let docs: BTreeSet<String> = ["a.py".to_string()].into_iter().collect();
    let d = doc_set_delta(&docs, &w);
    assert_eq!(d.missing, 1, "b.py on disk but not in index");
    assert_eq!(d.examples, vec!["b.py".to_string()]);
    assert_eq!(d.extra, 0);

    // index carries a file deleted from disk → extra
    let docs2: BTreeSet<String> = ["a.py".to_string(), "gone.py".to_string()]
        .into_iter()
        .collect();
    let d2 = doc_set_delta(&docs2, &w);
    assert_eq!(d2.missing, 1);
    assert_eq!(d2.extra, 1, "gone.py indexed but absent on disk");

    // rename = both sides
    let docs3: BTreeSet<String> = ["old.py".to_string()].into_iter().collect();
    let d3 = doc_set_delta(&docs3, &w);
    assert_eq!(d3.missing, 2);
    assert_eq!(d3.extra, 1);
}
