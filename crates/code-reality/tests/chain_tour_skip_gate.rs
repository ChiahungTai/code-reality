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
    // gated stdout survives the fail-closed return (consumer_db WARN
    // pushed before the gate rides stdout)
    assert!(out.stdout.contains("graph.db 不存在"), "{}", out.stdout);
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

// ---------- review round 1 (A reconciliation / B primary order / C disclosure / D coverage) ----------

/// md blocks: three anchored scenarios + a shell block, so a regen can
/// flip the middle scenario to a shell.
const REAL_A: &str = "# Real A\n\n```text\nkernel (pkg/a.py:1)\n└─ boot() (pkg/a.py:5)\n```\n";
const REAL_B: &str = "# Real B\n\n```text\n└─ boot() (pkg/a.py:5)\n```\n";
const REAL_C: &str =
    "# Real C\n\n```text\nkernel (pkg/a.py:1)\n└─ boot() (pkg/a.py:5)  # entry\n```\n";
const SHELL_BLOCK: &str = "# 夜間維運（殼）\n\n```text\ndeploy_step() (scripts/deploy.sh:1)\n└─ restart() (scripts/ghost.py:3)\n```\n";

fn run_out_dir(repo: &std::path::Path, chain: &std::path::Path) -> code_reality::ToolOutput {
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

/// A: regen shrink must reconcile the owned family — the pre-existing
/// 03.tour (and its manifest row) disappears when the middle scenario
/// turned shell; remaining tours/rows stay intact and tour_validate is
/// clean (the stale file used to survive and collide on ts_key).
#[test]
fn partial_shell_regen_reconciles_family_residue() {
    let md3 = format!("{REAL_A}\n{REAL_B}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-recon", &md3);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    assert!(out_dir.join("03.tour").exists());
    // the middle scenario becomes a shell; the family shrinks 3 → 2
    let md2 = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    std::fs::write(&chain, &md2).unwrap();
    let out = run_out_dir(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // written set: Real A + Real C (renumbered); the stale 03.tour and
    // its row are gone
    assert!(out_dir.join("01.tour").exists());
    assert!(out_dir.join("02.tour").exists());
    assert!(
        !out_dir.join("03.tour").exists(),
        "stale 03.tour must be reconciled away"
    );
    let keys = tour_row_keys(&repo);
    assert_eq!(
        keys,
        vec![
            "arch/01-gate/01.tour".to_string(),
            "arch/01-gate/02.tour".to_string()
        ],
        "{keys:?}"
    );
    // the written tours are Real A and Real C, not shifted shells
    let t: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("02.tour")).unwrap()).unwrap();
    assert_eq!(t["title"].as_str().unwrap(), "01 - gate｜Real C");
    // disclosure names exactly what was removed
    assert!(out.stderr.contains("重產對帳"), "{}", out.stderr);
    assert!(
        out.stderr.contains("arch/01-gate/03.tour"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("重編號"), "{}", out.stderr);
    // the corpus validates clean — the reproduced defect was a ts_key
    // collision FAIL against the stale 03.tour
    let v = code_reality::tour_validate::validate(&repo, std::path::Path::new(".tours"), false);
    assert_eq!(v.exit_code, 0, "stdout={} stderr={}", v.stdout, v.stderr);
}

/// A ownership fence: reconciliation never touches data this generator
/// does not own — a foreign-generator row (+ its file) and a rowless
/// numbered file outside the write set both survive.
#[test]
fn reconciliation_never_touches_foreign_or_rowless_residue() {
    let md3 = format!("{REAL_A}\n{REAL_B}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-recon-owned", &md3);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    // plant: 05.tour backed by a foreign-generator row, 06.tour with no
    // row at all — both outside any 2-wide write set
    std::fs::write(out_dir.join("05.tour"), "{\"title\":\"05 - manual\"}\n").unwrap();
    std::fs::write(out_dir.join("06.tour"), "{\"title\":\"06 - rowless\"}\n").unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    let mut m = code_reality::tour_manifest::load(&mpath).unwrap();
    if m.version.is_none() {
        m.version = Some(toml::Value::Integer(1));
    }
    m.tour.insert(
        "arch/01-gate/05.tour".into(),
        "generator = \"manual_tool\"\nsources = [\"chain.md\"]\nanchored_commit = \"c0\""
            .parse()
            .unwrap(),
    );
    code_reality::tour_manifest::dump(&mpath, &m).unwrap();
    // middle scenario becomes a shell → write set shrinks to 2
    let md2 = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    std::fs::write(&chain, &md2).unwrap();
    let out = run_out_dir(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // chain_tour-owned stale residue is gone; foreign/rowless survives
    assert!(!out_dir.join("03.tour").exists());
    assert!(
        out_dir.join("05.tour").exists(),
        "foreign-generator row's file must survive"
    );
    assert!(
        out_dir.join("06.tour").exists(),
        "rowless file must survive"
    );
    let mut keys = tour_row_keys(&repo);
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "arch/01-gate/01.tour".to_string(),
            "arch/01-gate/02.tour".to_string(),
            "arch/01-gate/05.tour".to_string(),
        ],
        "{keys:?}"
    );
    // the removal disclosure lists only the owned removal
    assert!(
        out.stderr.contains("arch/01-gate/03.tour"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("05.tour"), "{}", out.stderr);
}

/// B: `--primary` numbers md scenarios (1-based, shells included) — the
/// pre-CR-11 meaning. With a shell first, `--primary 2` must land on md
/// scenario 2 (written 01.tour), not the compressed second file.
#[test]
fn primary_numbers_md_scenarios_shells_included() {
    let md = format!("{SHELL_BLOCK}\n{REAL_A}\n{REAL_B}");
    let (repo, chain) = repo_fixture("gate-primary-md", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "2",
    ]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    let t1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let t2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("02.tour")).unwrap()).unwrap();
    assert_eq!(t1["title"].as_str().unwrap(), "01 - gate｜Real A");
    assert_eq!(t1["isPrimary"], serde_json::Value::Bool(true), "{t1}");
    assert!(t2.get("isPrimary").is_none(), "{t2}");
}

/// B: a primary pointing at an omitted shell fails loud in the invalid
/// argument face — exit 1, zero writes, naming the scenario title.
#[test]
fn primary_pointing_at_shell_fails_loud() {
    let md = format!("{SHELL_BLOCK}\n{REAL_A}\n{REAL_B}");
    let (repo, chain) = repo_fixture("gate-primary-shell", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "1",
    ]);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("[FAIL]"), "{}", out.stderr);
    assert!(out.stderr.contains("--primary 1"), "{}", out.stderr);
    assert!(out.stderr.contains("殼省略"), "{}", out.stderr);
    assert!(out.stderr.contains("夜間維運（殼）"), "{}", out.stderr);
    // zero writes
    assert!(!out_dir.exists());
    assert!(!repo.join(".tours/manifest.toml").exists());
}

/// B: middle-shell renumbering — md scenario 3 is written as 02.tour and
/// `--primary 3` marks it; md scenario 2 (the shell) fails loud.
#[test]
fn primary_middle_shell_renumbering() {
    let md = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-primary-mid", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let fail = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "2",
    ]);
    assert_eq!(
        fail.exit_code, 1,
        "stdout={} stderr={}",
        fail.stdout, fail.stderr
    );
    assert!(fail.stderr.contains("夜間維運（殼）"), "{}", fail.stderr);
    assert!(!out_dir.exists());
    let ok = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "3",
    ]);
    assert_eq!(ok.exit_code, 0, "stdout={} stderr={}", ok.stdout, ok.stderr);
    let t2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("02.tour")).unwrap()).unwrap();
    assert_eq!(t2["title"].as_str().unwrap(), "01 - gate｜Real C");
    assert_eq!(t2["isPrimary"], serde_json::Value::Bool(true), "{t2}");
    let t1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    assert!(t1.get("isPrimary").is_none(), "{t1}");
}

/// C: the all-shell fail-closed face over an EXISTING family must
/// disclose that the family's tours/rows remain (zero-write does not
/// clean) — old files stay byte-identical.
#[test]
fn all_shell_existing_family_discloses_residual() {
    let md2 = format!("{REAL_A}\n{REAL_B}");
    let (repo, chain) = repo_fixture("gate-allshell-residual", &md2);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    let old1 = std::fs::read_to_string(out_dir.join("01.tour")).unwrap();
    let old2 = std::fs::read_to_string(out_dir.join("02.tour")).unwrap();
    // the md goes all-shell; the fail-closed face must not touch the
    // corpus and must say the residual family needs manual cleanup
    let md_shell = SHELL_BLOCK.to_string();
    std::fs::write(&chain, &md_shell).unwrap();
    let out = run_out_dir(&repo, &chain);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("全場景殼形"), "{}", out.stderr);
    assert!(out.stderr.contains("不自動清除"), "{}", out.stderr);
    assert!(out.stderr.contains("2 條"), "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(out_dir.join("01.tour")).unwrap(),
        old1
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.join("02.tour")).unwrap(),
        old2
    );
    assert_eq!(tour_row_keys(&repo).len(), 2);
}

/// D: the all-shell fail-closed face preserves gated stdout — the
/// duplicate-family WARN (emitted before the gate; explicit --out-dir
/// keeps both families, the in-process relative default out-dir would
/// resolve against the test process cwd) rides stdout while [FAIL] rides
/// stderr, and the pre-existing family is untouched.
#[test]
fn all_shell_gate_preserves_gated_stdout() {
    let (repo, chain) = repo_fixture("gate-allshell-stdout", &SHELL_BLOCK);
    // an existing family already sourced from this md — the explicit
    // --out-dir warns both-families-coexist, then the shell gate fires
    std::fs::create_dir_all(repo.join(".tours/arch/99-old")).unwrap();
    std::fs::write(
        repo.join(".tours/arch/99-old/01.tour"),
        "{\"title\":\"01 - old\"}\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".tours/manifest.toml"),
        "version = 1\n\n[tour.\"arch/99-old/01.tour\"]\ngenerator = \"chain_tour\"\nsources = [\"chain.md\"]\nanchored_commit = \"c0\"\n",
    )
    .unwrap();
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // gated stdout survives the fail-closed return
    assert!(out.stdout.contains("duplicate-family"), "{}", out.stdout);
    assert!(out.stderr.contains("全場景殼形"), "{}", out.stderr);
    // zero writes anywhere: the coexisting family untouched, out_dir absent
    assert_eq!(
        std::fs::read_to_string(repo.join(".tours/arch/99-old/01.tour")).unwrap(),
        "{\"title\":\"01 - old\"}\n"
    );
    assert!(!out_dir.exists());
}

// ---------- review round 2 (NF1 delete-after-persist / NF2 .tour predicate / mapping gap) ----------

/// NF1(a) + NF3(c): a manifest dump failure must crash the run with ZERO
/// files deleted and the on-disk rows byte-identical (deleting files and
/// then failing the dump left a corpus of dangling rows — the defect).
/// Enforcement: the manifest's DIRECTORY goes read-only, which the
/// atomic temp-file replace cannot create into; a read-only manifest
/// FILE would no longer fail (rename replaces by directory permission).
/// Skips cleanly when the permission model cannot be enforced (root).
#[test]
fn dump_failure_leaves_files_and_rows_intact() {
    use std::os::unix::fs::PermissionsExt;
    let md3 = format!("{REAL_A}\n{REAL_B}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-dump-fail", &md3);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    let mpath = repo.join(".tours/manifest.toml");
    let rows_before = std::fs::read_to_string(&mpath).unwrap();
    // the middle scenario becomes a shell → write set shrinks to 2
    let md2 = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    std::fs::write(&chain, &md2).unwrap();
    // dump-failure enforcement: the manifest's DIRECTORY goes read-only —
    // the atomic replace must create its temp file there. (A read-only
    // manifest FILE no longer forces a failure: the rename replaces by
    // directory permission — POSIX semantics.) Skips when the permission
    // model cannot be enforced (e.g. running as root).
    let tours = repo.join(".tours");
    let mut perms = std::fs::metadata(&tours).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&tours, perms).unwrap();
    if std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(tours.join(".probe"))
        .is_ok()
    {
        let mut p = std::fs::metadata(&tours).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&tours, p).unwrap();
        let _ = std::fs::remove_file(tours.join(".probe"));
        eprintln!("skip: permission model not enforceable in this environment");
        return;
    }
    let out = run_out_dir(&repo, &chain);
    let mut p = std::fs::metadata(&tours).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&tours, p).unwrap();
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // zero deletions: the stale 03.tour survives…
    assert!(out_dir.join("03.tour").exists());
    // …and the on-disk rows are byte-identical (dump never landed)
    assert_eq!(std::fs::read_to_string(&mpath).unwrap(), rows_before);
    // no temp residue beside the target
    let residue: Vec<String> = std::fs::read_dir(&tours)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".manifest.toml.tmp-"))
        .collect();
    assert!(residue.is_empty(), "{residue:?}");
}

/// NF4 RAII: clears the immutable flag on Drop — loud on failure; when a
/// panic is already unwinding, degrades to eprintln (no double-panic
/// abort), so a panic anywhere in the test body still clears the flag.
#[cfg(target_os = "macos")]
struct UchgGuard(std::path::PathBuf);

#[cfg(target_os = "macos")]
impl UchgGuard {
    fn set(p: &std::path::Path) -> Self {
        let ok = std::process::Command::new("/usr/bin/chflags")
            .arg("uchg")
            .arg(p)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "chflags uchg failed on {}", p.display());
        UchgGuard(p.to_path_buf())
    }
}

#[cfg(target_os = "macos")]
impl Drop for UchgGuard {
    fn drop(&mut self) {
        let ok = std::process::Command::new("/usr/bin/chflags")
            .arg("nouchg")
            .arg(&self.0)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if std::thread::panicking() {
            if !ok {
                eprintln!(
                    "LEAKED uchg flag on {} (clear failed while panicking)",
                    self.0.display()
                );
            }
        } else {
            assert!(ok, "chflags nouchg failed on {}", self.0.display());
        }
    }
}

/// NF1(b): a per-file removal failure is NOT a crash — the run
/// continues (exit 0 with WARNs), the WARN names the surviving path,
/// and the already-persisted manifest keeps its consistent row-removed
/// state (a disclosed rowless orphan file is the tolerated direction).
/// The enforcement mechanism is the macOS immutable file flag: a
/// read-only family DIR would crash the earlier write_tours leg (the
/// same run writes into that dir). Skips when the flag cannot be
/// enforced (non-macOS / root bypass). The summary disclosure counts
/// the failed removal apart from the removed ones.
#[cfg(target_os = "macos")]
#[test]
fn file_removal_failure_warns_and_continues() {
    let md3 = format!("{REAL_A}\n{REAL_B}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-file-fail", &md3);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    // enforcement probe: the flag rides the same RAII guard (loud on
    // every set/clear) — a delete that succeeds means this environment
    // ignores uchg → skip
    let probe = out_dir.join(".probe-uchg");
    std::fs::write(&probe, "x").unwrap();
    let enforced = {
        let _guard = UchgGuard::set(&probe);
        std::fs::remove_file(&probe).is_err()
    };
    if !enforced {
        let _ = std::fs::remove_file(&probe);
        eprintln!("skip: immutable flag not enforced (root?)");
        return;
    }
    let _ = std::fs::remove_file(&probe);
    let stale = out_dir.join("03.tour");
    // the middle scenario becomes a shell → write set shrinks to 2
    let md2 = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    std::fs::write(&chain, &md2).unwrap();
    let out = {
        // RAII: the flag clears on scope exit even if the run panics
        let _guard = UchgGuard::set(&stale);
        run_out_dir(&repo, &chain)
    };
    let stale_exists = stale.exists();
    let keys = tour_row_keys(&repo);
    let _ = std::fs::remove_file(&stale); // unflagged by the guard drop
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("刪除失敗"), "{}", out.stderr);
    assert!(out.stderr.contains("03.tour"), "{}", out.stderr);
    // the summary disclosure distinguishes the failed removal
    assert!(
        out.stderr
            .contains("移除寫出集外殘留 1 項（1 項失敗殘留，見 WARN）"),
        "{}",
        out.stderr
    );
    assert!(stale_exists, "the un-deletable file must survive");
    assert_eq!(
        keys,
        vec![
            "arch/01-gate/01.tour".to_string(),
            "arch/01-gate/02.tour".to_string(),
        ],
        "{keys:?}"
    );
    // the run's normal product is intact
    assert!(out.stdout.contains("shells-omitted 1"), "{}", out.stdout);
}

/// NF2: the deletion predicate requires the row to name a `.tour` — a
/// chain_tour-owned row over a foreign-format name (`03.md`) is not this
/// generator's product format, is never a deletion candidate, and is
/// not policed (left untouched, silent).
#[test]
fn reconciliation_never_touches_non_tour_rows() {
    let md3 = format!("{REAL_A}\n{REAL_B}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-non-tour", &md3);
    let out_dir = repo.join(".tours/arch/01-gate");
    let first = run_out_dir(&repo, &chain);
    assert_eq!(first.exit_code, 0, "{}{}", first.stdout, first.stderr);
    // mis-provenanced row: chain_tour generator over a foreign-format name
    std::fs::write(out_dir.join("03.md"), "# not a tour\n").unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    let mut m = code_reality::tour_manifest::load(&mpath).unwrap();
    m.tour.insert(
        "arch/01-gate/03.md".into(),
        "generator = \"chain_tour\"\nsources = [\"chain.md\"]\nanchored_commit = \"c0\""
            .parse()
            .unwrap(),
    );
    code_reality::tour_manifest::dump(&mpath, &m).unwrap();
    // scenario 3 becomes a shell → write set shrinks to 2; row 03 falls
    // outside the write set and would look deletable without the .tour
    // requirement (the .md row and the .tour row share the stem "03")
    let md2 = format!("{REAL_A}\n{SHELL_BLOCK}\n{REAL_C}");
    std::fs::write(&chain, &md2).unwrap();
    let out = run_out_dir(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    // the .md file and its row survive untouched
    assert!(out_dir.join("03.md").exists());
    let mut keys = tour_row_keys(&repo);
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "arch/01-gate/01.tour".to_string(),
            "arch/01-gate/02.tour".to_string(),
            "arch/01-gate/03.md".to_string(),
        ],
        "{keys:?}"
    );
    assert!(!out.stderr.contains("03.md"), "{}", out.stderr);
}

/// Mapping combination (round-2 note): shells at md positions 1 AND 2
/// with the real scenario at 3 → written 01.tour is md scenario 3;
/// `--primary 3` marks it and `--primary 1` fails as shell-targeted.
#[test]
fn primary_mapping_with_leading_double_shell() {
    let md = format!("{SHELL_BLOCK}\n{SHELL_BLOCK}\n{REAL_C}");
    let (repo, chain) = repo_fixture("gate-primary-2shell", &md);
    let out_dir = repo.join(".tours/arch/01-gate");
    let fail = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "1",
    ]);
    assert_eq!(
        fail.exit_code, 1,
        "stdout={} stderr={}",
        fail.stdout, fail.stderr
    );
    assert!(fail.stderr.contains("殼省略"), "{}", fail.stderr);
    assert!(!out_dir.exists());
    let ok = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "3",
    ]);
    assert_eq!(ok.exit_code, 0, "stdout={} stderr={}", ok.stdout, ok.stderr);
    let t1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    assert_eq!(t1["title"].as_str().unwrap(), "01 - gate｜Real C");
    assert_eq!(t1["isPrimary"], serde_json::Value::Bool(true), "{t1}");
    assert!(
        ok.stdout
            .contains("[OK] chain tours: 1 場景 / 6 幀 / 2 步 / skipped 4 / shells-omitted 2"),
        "{}",
        ok.stdout
    );
}

// ---------- round 3 (NF3 atomic manifest persistence) ----------

/// NF3(a): dump into a read-only directory errors, the target manifest
/// stays byte-identical, and no temp residue is left beside it. Skips
/// when the permission model cannot be enforced (root).
#[test]
fn dump_readonly_dir_errors_without_residue() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap().keep();
    let dir = tmp.join("tours");
    std::fs::create_dir_all(&dir).unwrap();
    let mpath = dir.join("manifest.toml");
    std::fs::write(&mpath, "version = 1\n").unwrap();
    let before = std::fs::read_to_string(&mpath).unwrap();
    let mut perms = std::fs::metadata(&dir).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&dir, perms).unwrap();
    if std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(dir.join(".probe"))
        .is_ok()
    {
        let mut p = std::fs::metadata(&dir).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&dir, p).unwrap();
        let _ = std::fs::remove_file(dir.join(".probe"));
        eprintln!("skip: permission model not enforceable in this environment");
        return;
    }
    let m = code_reality::tour_manifest::load(&mpath).unwrap();
    let err = code_reality::tour_manifest::dump(&mpath, &m)
        .expect_err("dump into a read-only dir must fail");
    let mut p = std::fs::metadata(&dir).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&dir, p).unwrap();
    assert!(err.contains("寫入失敗"), "{err}");
    assert_eq!(std::fs::read_to_string(&mpath).unwrap(), before);
    let residue: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".manifest.toml.tmp-"))
        .collect();
    assert!(residue.is_empty(), "{residue:?}");
}

/// NF3(b): successful dumps leave no temp files behind, and the rename
/// replaces the target EVEN when the target file itself is read-only —
/// rename keys on directory permission (POSIX); the old direct-write
/// face failed there instead. Pinning that boundary on purpose.
#[test]
fn dump_atomic_replace_leaves_no_temp_residue() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap().keep();
    let dir = tmp.join("tours");
    std::fs::create_dir_all(&dir).unwrap();
    let mpath = dir.join("manifest.toml");
    let m1 = code_reality::tour_manifest::load(&dir.join("absent.toml")).unwrap();
    code_reality::tour_manifest::dump(&mpath, &m1).unwrap();
    code_reality::tour_manifest::dump(&mpath, &m1).unwrap();
    // exactly the manifest, no temp-pattern residue
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["manifest.toml".to_string()], "{names:?}");
    // a read-only TARGET file is replaced successfully (rename face);
    // restore afterwards so tempdir cleanup is unaffected
    let mut perms = std::fs::metadata(&mpath).unwrap().permissions();
    perms.set_mode(0o444);
    std::fs::set_permissions(&mpath, perms).unwrap();
    code_reality::tour_manifest::dump(&mpath, &m1)
        .expect("rename replaces a read-only target (dir permission gates)");
    let mut p = std::fs::metadata(&mpath).unwrap().permissions();
    p.set_mode(0o644);
    std::fs::set_permissions(&mpath, p).unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["manifest.toml".to_string()], "{names:?}");
}

// ---------- round 4 (NF5 exclusive temp creation) ----------

/// NF5: temp creation is exclusive — a pre-planted symlink at the child
/// process's first temp name (pid + seq 0 are predictable for a fresh
/// process) must NOT be followed. The old non-exclusive File::create
/// face wrote straight through it, clobbering the sentinel and renaming
/// the link onto the manifest. Exclusive create_new fails with
/// AlreadyExists → the retry picks the next name; planted link +
/// sentinel stay byte-identical, manifest correct, no extra residue.
#[cfg(unix)]
#[test]
fn dump_temp_creation_is_exclusive_and_never_follows_symlinks() {
    use std::io::{BufRead, Write};
    use std::os::unix::fs::symlink;
    use std::process::{Command, Stdio};
    let md = format!("{REAL_A}\n{REAL_B}");
    let (repo, chain) = repo_fixture("gate-exclusive-tmp", &md);
    let mdir = repo.join(".tours");
    std::fs::create_dir_all(&mdir).unwrap();
    // the sentinel a followed symlink would clobber
    let sentinel = mdir.join("sentinel.txt");
    std::fs::write(&sentinel, "do-not-clobber\n").unwrap();
    // child process held on stdin, so its pid is knowable before its
    // first (and only) manifest dump
    let bin = env!("CARGO_BIN_EXE_code-reality");
    let script = format!(
        "echo $$; read _; exec {} chain_tour {} --repo {}",
        bin,
        chain.display(),
        repo.display()
    );
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child_out = child.stdout.take().unwrap();
    let pid = {
        let mut line = String::new();
        std::io::BufReader::new(&mut child_out)
            .read_line(&mut line)
            .unwrap();
        let pid = line.trim().to_string();
        assert!(
            !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()),
            "expected the child pid, got {line:?}"
        );
        pid
    };
    // the collision: a symlink at the child's FIRST temp name
    let planted = mdir.join(format!(".manifest.toml.tmp-{pid}-0"));
    symlink(&sentinel, &planted).unwrap();
    child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    child.stdin.take();
    let st = child.wait().unwrap();
    let _ = std::io::Read::read_to_string(&mut child_out, &mut String::new());
    assert!(st.success(), "child run failed");
    // the planted symlink was never followed: link + sentinel untouched
    assert_eq!(
        std::fs::read_to_string(&sentinel).unwrap(),
        "do-not-clobber\n"
    );
    let meta = std::fs::symlink_metadata(&planted).unwrap();
    assert!(
        meta.file_type().is_symlink(),
        "{planted:?} must survive as a link"
    );
    // the manifest landed correctly (fresh names, not through the link)
    let mut keys = tour_row_keys(&repo);
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "arch/chain/01.tour".to_string(),
            "arch/chain/02.tour".to_string(),
        ],
        "{keys:?}"
    );
    // residue: exactly the planted name, nothing else in the pattern
    let temps: Vec<String> = std::fs::read_dir(&mdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".manifest.toml.tmp-"))
        .collect();
    assert_eq!(
        temps,
        vec![format!(".manifest.toml.tmp-{pid}-0")],
        "{temps:?}"
    );
}

/// CR-20: a frame whose file lives outside every module prefix now
/// resolves through the repo segment (`repo-direct`) and produces a
/// step — the external bucket shrinks to genuinely-unfindable paths.
/// The shell-gate assertions above stay byte-identical: `ghost.py`
/// exists nowhere (still external), `deploy.sh` is still unparseable
/// (noref).
#[test]
fn repo_rescue_turns_module_outside_frames_into_steps() {
    let md =
        "# Ops\n\n```text\ndeploy_step() (pkg/a.py:1)\n└─ restart() (scripts/deploy.py:3)\n```\n"
            .to_string();
    let (repo, chain) = repo_fixture("skip-gate-repo-rescue", &md);
    // the module-outside file the second frame names
    std::fs::create_dir_all(repo.join("scripts")).unwrap();
    std::fs::write(
        repo.join("scripts/deploy.py"),
        "#!/usr/bin/env python3\ndef restart():\n    pass\n",
    )
    .unwrap();
    let out_dir = repo.join(".tours/arch/01-gate");
    let out = run_chain_tour(&repo, &chain);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    let t: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "{}", out.stdout);
    assert_eq!(steps[1]["file"], "scripts/deploy.py");
    // the rescue is counted on the new observable line
    assert!(out.stdout.contains("[LOG] repo 段救援"), "{}", out.stdout);
    assert!(out.stdout.contains("repo-direct"), "{}", out.stdout);
    // and nothing landed in the external bucket
    assert!(!out.stdout.contains("external"), "{}", out.stdout);
}
