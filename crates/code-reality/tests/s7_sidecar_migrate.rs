//! S2 sidecar_migrate tests — idempotent home→in-repo migration
//! (EP data-plane-unification).
//!
//! Coverage boundary (accepted): the EXDEV orchestration
//! (copy_dir_verified → verify → remove-source under a real
//! cross-device rename) is not exercised — cross-device setups are not
//! simulatable here; `copy_verified` unit-covers mtime/bytes only.

use code_reality::sidecar_migrate::{copy_verified, migrate_repo};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn write(p: &Path, body: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn fake_home(home: &Path, name: &str) {
    let slot = home.join("scip").join(name);
    for f in [
        "index.scip",
        "index.scip.db",
        "index.scip.meta.json",
        "index.union.db",
    ] {
        write(&slot.join(f), &format!("body-of-{f}"));
    }
    write(&home.join("boundary").join("61590e48.db"), "b1");
    write(&home.join("boundary").join("9133b899.db"), "b2");
    write(
        &home.join("snapshots").join(&format!("{name}-abc1234.json")),
        "s1",
    );
    write(
        &home.join("snapshots").join(&format!("{name}-def5678.json")),
        "s2",
    );
    // exact-`name` snapshot file (convention-external artifact) — moves
    write(&home.join("snapshots").join(name), "exact");
    // a sibling repo's snapshot: dash-delimited, must NOT move
    write(
        &home
            .join("snapshots")
            .join(&format!("{name}_sibling-999.json")),
        "other",
    );
    write(&home.join("golden").join(&format!("{name}.json")), "golden");
}

fn mtimes(dir: &Path) -> Vec<(String, u128)> {
    let mut v: Vec<(String, u128)> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| {
            let p = e.path();
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                p.metadata().unwrap().mtime_nsec() as u128,
            )
        })
        .collect();
    v.sort();
    v
}

#[test]
fn migrates_slot_dir_with_mtimes_and_drops_union() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    fs::create_dir_all(&repo).unwrap();
    let home = tmp.path().join("homecode-reality");
    fake_home(&home, "myrepo");
    let before = mtimes(&home.join("scip").join("myrepo"));

    let r = migrate_repo(&repo, &home).unwrap();
    let slot = repo.join(".code-reality").join("scip");
    assert!(slot.join("index.scip").is_file());
    assert!(
        !slot.join("index.union.db").exists(),
        "dead artifact dropped"
    );
    assert_eq!(r.dropped, vec!["index.union.db".to_string()]);
    assert!(
        !home.join("scip").join("myrepo").exists(),
        "source slot gone"
    );
    // mtimes preserved (rename semantics — the mtime gate depends on it)
    let expected: Vec<(String, u128)> = before
        .iter()
        .filter(|(n, _)| n != "index.union.db")
        .cloned()
        .collect();
    assert_eq!(mtimes(&slot), expected);
    // snapshots + golden landed in-repo (boundary face is NOT
    // auto-attributed — sha-keyed dbs stay in home for the EP's one-off)
    assert!(home.join("boundary/61590e48.db").is_file());
    assert!(repo
        .join(".code-reality/snapshots/myrepo-abc1234.json")
        .is_file());
    assert!(repo
        .join(".code-reality/snapshots/myrepo-def5678.json")
        .is_file());
    assert!(
        repo.join(".code-reality/snapshots/myrepo").is_file(),
        "exact-name snapshot moves (pinned)"
    );
    assert!(repo.join(".code-reality/golden/myrepo.json").is_file());
    // sibling repo's snapshot untouched
    assert!(home.join("snapshots/myrepo_sibling-999.json").is_file());
    // data dir self-ignored (freshly written this run)
    assert!(repo.join(".code-reality/.gitignore").is_file());
    assert!(r.ensured_gitignore);
}

#[test]
fn both_exist_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("myrepo");
    write(
        &repo_root.join(".code-reality/scip/index.scip"),
        "in-repo-wins",
    );
    let home = tmp.path().join("homecode-reality");
    fake_home(&home, "myrepo");
    let r = migrate_repo(&repo_root, &home).unwrap();
    assert!(!r.warnings.is_empty(), "dual presence must WARN: {:?}", r);
    assert_eq!(
        fs::read_to_string(repo_root.join(".code-reality/scip/index.scip")).unwrap(),
        "in-repo-wins",
        "in-repo content never overwritten"
    );
    assert!(
        home.join("scip")
            .join("myrepo")
            .join("index.scip")
            .is_file(),
        "home preserved for manual adjudication"
    );
    // other faces still migrate (boundary face never auto-attributes)
    assert!(repo_root.join(".code-reality/golden/myrepo.json").is_file());
}

#[test]
fn rerun_is_zero_action() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    fs::create_dir_all(&repo).unwrap();
    let home = tmp.path().join("homecode-reality");
    fake_home(&home, "myrepo");
    migrate_repo(&repo, &home).unwrap();
    let r2 = migrate_repo(&repo, &home).unwrap();
    assert!(
        r2.moved.is_empty(),
        "second run moves nothing: {:?}",
        r2.moved
    );
    assert!(r2.dropped.is_empty());
    assert!(r2.warnings.is_empty());
    assert!(!r2.ensured_gitignore, "second run writes nothing");
}

#[test]
fn dual_presence_file_reports_no_false_move() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    // in-repo already holds one of the repo's snapshots
    write(
        &repo.join(".code-reality/snapshots/myrepo-abc1234.json"),
        "in-repo-wins",
    );
    let home = tmp.path().join("homecode-reality");
    fake_home(&home, "myrepo");
    let r = migrate_repo(&repo, &home).unwrap();
    assert!(
        !r.moved.iter().any(|m| m.contains("myrepo-abc1234")),
        "skipped file must not appear in moved: {:?}",
        r.moved
    );
    assert!(r.moved.iter().any(|m| m.contains("myrepo-def5678")));
    assert!(r.warnings.iter().any(|w| w.contains("myrepo-abc1234")));
    assert!(
        home.join("snapshots/myrepo-abc1234.json").is_file(),
        "home copy kept for adjudication"
    );
}

#[test]
fn nonexistent_repo_fails_fast_without_fabricating_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("homecode-reality");
    fs::create_dir_all(&home).unwrap();
    let ghost = tmp.path().join("ghost-repo");
    assert!(migrate_repo(&ghost, &home).is_err());
    assert!(
        !ghost.exists(),
        "no directory tree fabricated for a typo path"
    );
}

#[test]
fn old_slot_index_detects_retired_home_slot() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    fs::create_dir_all(&repo).unwrap();
    let home = tmp.path().join("homecode-reality");
    write(&home.join("scip").join("myrepo").join("index.scip"), "old");
    let hit = code_reality::sidecar_migrate::old_slot_index(&home, &repo).unwrap();
    assert!(hit.ends_with("scip/myrepo/index.scip"));
    // cache-only leftovers do not trigger
    let home2 = tmp.path().join("home2");
    write(
        &home2.join("scip").join("myrepo").join("index.scip.db"),
        "cache",
    );
    assert!(code_reality::sidecar_migrate::old_slot_index(&home2, &repo).is_none());
    // another repo's slot does not match
    assert!(
        code_reality::sidecar_migrate::old_slot_index(&home, &tmp.path().join("other")).is_none()
    );
}

#[test]
fn run_faces_repo_required_ensured_and_idempotent_output() {
    let out = code_reality::sidecar_migrate::run(&["sidecar_migrate"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--repo"), "{}", out.stderr);

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("myrepo");
    fs::create_dir_all(&repo).unwrap();
    let home = tmp.path().join("homecode-reality");
    let argv_owned: Vec<String> = vec![
        "sidecar_migrate".into(),
        "--repo".into(),
        repo.to_string_lossy().into(),
        "--home".into(),
        home.to_string_lossy().into(),
    ];
    let argv: Vec<&str> = argv_owned.iter().map(String::as_str).collect();
    let out1 = code_reality::sidecar_migrate::run(&argv);
    assert_eq!(out1.exit_code, 0, "{}{}", out1.stdout, out1.stderr);
    assert!(
        out1.stdout.contains("ensured: 資料目錄自帶 .gitignore"),
        "{}",
        out1.stdout
    );
    // rerun on the same (now-clean) state → true no-op face
    let out2 = code_reality::sidecar_migrate::run(&argv);
    assert_eq!(out2.exit_code, 0);
    assert!(out2.stdout.contains("無動作"), "{}", out2.stdout);
}

#[test]
fn copy_verified_preserves_bytes_and_mtime() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src.bin");
    fs::write(&src, "payload").unwrap();
    let dst = tmp.path().join("dst.bin");
    copy_verified(&src, &dst).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "payload");
    assert_eq!(
        src.metadata().unwrap().mtime_nsec(),
        dst.metadata().unwrap().mtime_nsec(),
        "EXDEV fallback must preserve mtime (mtime gate)"
    );
}
