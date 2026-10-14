//! CR-11 scenario-level skip gate (mosaic CR-9 feedback B): a scenario
//! whose every frame skipped (zero anchored steps) is a shell — it must
//! not write a .tour file nor land a manifest row; the run warns loudly
//! and the summary gains a shells-omitted counter. A fully-shell md
//! fails closed (exit 1, zero writes), folded into the existing
//! parse-fail (a) shape — no third exit state.

use code_reality::chain_tour::run;
use std::path::PathBuf;

const GOOD_BLOCK: &str = "```text\nkernel (pkg/a.py:1)\n└─ boot() (pkg/a.py:5)  # entry\n```";

fn repo_fixture(tag: &str, md: &str) -> (PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("pkg/a.py"),
        "def kernel():\n    pass\n\n\ndef boot():\n    load_config()\n",
    )
    .unwrap();
    let chain = repo.join("chain.md");
    std::fs::write(&chain, md).unwrap();
    let g = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
    };
    g(&["init", "-q"]);
    g(&["add", "."]);
    g(&["commit", "-qm", "init"]);
    (repo, chain)
}

fn run_chain_tour(repo: &std::path::Path, chain: &std::path::Path) -> code_reality::ToolOutput {
    let out_dir = repo.join(".tours/arch/01-gate");
    run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ])
}

fn tour_row_keys(repo: &std::path::Path) -> Vec<String> {
    let m = code_reality::tour_manifest::load(&repo.join(".tours/manifest.toml")).unwrap();
    m.tour.keys().cloned().collect()
}

/// TC①: every scenario a shell → fail-closed exit 1 (the existing (a)
/// shape), zero writes — no .tour file, no out_dir creation, no manifest
/// row — with the per-scenario WARN details (title + frames + skip
/// reasons) and the 全場景殼形 naming on stderr.
#[test]
fn all_shell_md_fails_closed_with_zero_writes() {
    let md = "# 夜間維運（殼）\n\n```text\ndeploy_step() (scripts/deploy.sh:1)\n└─ restart() (scripts/ghost.py:3)\n```\n".to_string();
    let (repo, chain) = repo_fixture("skip-gate-all-shell", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = run_chain_tour(&repo, &chain);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // zero writes: no tour files, no out_dir, no manifest
    assert!(!out_dir.exists(), "out_dir must not be created");
    assert!(!repo.join(".tours/manifest.toml").exists());
    // the explicit WARN carries 族名＋場景標題＋幀數＋skip 原因分佈
    // (.sh refs stay unparseable → noref; the .py ref resolves nowhere →
    // external)
    assert!(out.stderr.contains("[WARN]"), "{}", out.stderr);
    assert!(out.stderr.contains("夜間維運（殼）"), "{}", out.stderr);
    assert!(out.stderr.contains("2 幀"), "{}", out.stderr);
    assert!(out.stderr.contains("noref 1"), "{}", out.stderr);
    assert!(out.stderr.contains("external 1"), "{}", out.stderr);
    // the fail-closed line names the all-shell shape
    assert!(out.stderr.contains("全場景殼形"), "{}", out.stderr);
    assert!(!out.stdout.contains("[OK] chain tour ->"), "{}", out.stdout);
}

/// TC②: one good scenario + one shell → the good one writes normally
/// (exit 0 — shell omission is a corpus fact, not a tool failure), the
/// shell is absent from both the file set and the manifest, the WARN
/// rides stderr, and the summary line carries shells-omitted 1.
#[test]
fn partial_shell_omits_only_the_shell_and_counts_it() {
    let md = format!(
        "# Real scenario\n\n{GOOD_BLOCK}\n\n# 夜間維運（殼）\n\n```text\ndeploy_step() (scripts/deploy.sh:1)\n└─ restart() (scripts/ghost.py:3)\n```\n"
    );
    let (repo, chain) = repo_fixture("skip-gate-partial", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = run_chain_tour(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // the good scenario writes as 01.tour (compact numbering); the shell
    // lands no 02.tour and no manifest row
    assert!(out_dir.join("01.tour").exists());
    assert!(!out_dir.join("02.tour").exists());
    let keys = tour_row_keys(&repo);
    assert_eq!(keys, vec!["arch/01-gate/01.tour".to_string()], "{keys:?}");
    // the written tour is the real scenario, not a renumbered shell
    // (title label = the 01-gate dir suffix, the family-template face)
    let t: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    assert_eq!(t["title"].as_str().unwrap(), "01 - gate｜Real scenario");
    assert_eq!(t["steps"].as_array().unwrap().len(), 2);
    // per-scenario WARN: 族名＋場景標題＋幀數＋skip 原因分佈
    assert!(out.stderr.contains("[WARN]"), "{}", out.stderr);
    assert!(out.stderr.contains("01-gate"), "{}", out.stderr);
    assert!(out.stderr.contains("夜間維運（殼）"), "{}", out.stderr);
    assert!(out.stderr.contains("2 幀"), "{}", out.stderr);
    assert!(out.stderr.contains("noref 1"), "{}", out.stderr);
    assert!(out.stderr.contains("external 1"), "{}", out.stderr);
    // summary counter
    assert!(
        out.stdout
            .contains("[OK] chain tours: 1 場景 / 4 幀 / 2 步 / skipped 2 / shells-omitted 1"),
        "{}",
        out.stdout
    );
}

/// TC③: no shells → behavior byte-for-byte the old shape (plus the
/// always-present shells-omitted 0 field): both scenarios write, both
/// manifest rows land, no shell WARN.
#[test]
fn all_normal_scenarios_unaffected() {
    let md = format!("# Boot scenario\n\n{GOOD_BLOCK}\n\n# Second scenario\n\n```text\nsolo_frame (pkg/a.py:7)\n└─ kernel (pkg/a.py:1)\n```\n");
    let (repo, chain) = repo_fixture("skip-gate-normal", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = run_chain_tour(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out_dir.join("01.tour").exists());
    assert!(out_dir.join("02.tour").exists());
    let mut keys = tour_row_keys(&repo);
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "arch/01-gate/01.tour".to_string(),
            "arch/01-gate/02.tour".to_string()
        ],
        "{keys:?}"
    );
    assert!(!out.stderr.contains("全 skip"), "{}", out.stderr);
    assert!(
        out.stdout
            .contains("[OK] chain tours: 2 場景 / 4 幀 / 4 步 / skipped 0 / shells-omitted 0"),
        "{}",
        out.stdout
    );
}

/// The 1-2-frame real-anchor scenario stays legal — no frame-count floor
/// exists; the only shell criterion is all-skip. A two-frame scenario
/// with real anchors writes normally (mosaic 族 04.tour 2 步 shape).
#[test]
fn two_frame_real_anchor_scenario_stays_legal() {
    let md =
        "# Solo real\n\n```text\nboot() (pkg/a.py:5)\n└─ kernel (pkg/a.py:1)\n```\n".to_string();
    let (repo, chain) = repo_fixture("skip-gate-minimal", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = run_chain_tour(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out_dir.join("01.tour").exists());
    assert!(
        out.stdout
            .contains("[OK] chain tours: 1 場景 / 2 幀 / 2 步 / skipped 0 / shells-omitted 0"),
        "{}",
        out.stdout
    );
}
