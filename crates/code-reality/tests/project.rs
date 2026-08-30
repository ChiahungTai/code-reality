//! `project` integration tests (EP ep-projected-graph-overlay S2):
//! fake-bin injection (tests/build.rs pattern) driving the orchestrator
//! against the committed proj_real_leg.scip + pre-minted overlay fixtures
//! — graft surface, HOLE/MISSING verdicts, non-pollution of the real
//! slot, idempotent reruns, coexisting slots, and the missing-bin /
//! invalid-stem error faces. A guarded test also runs the REAL
//! overlay-gen binary when the workspace target dir is present.

use code_reality::project::project_repo;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Shell fake overlay-gen: copies the pre-minted overlay + report to the
/// requested --out/--report paths (argv-parsed, no PATH mutation).
fn fake_overlay_gen(dir: &Path) -> PathBuf {
    let script = dir.join("overlay-gen");
    let content = format!(
        "#!/bin/sh\nout=; rep=; prev=\nfor a in \"$@\"; do\n  case \"$prev\" in\n    --out) out=\"$a\";;\n    --report) rep=\"$a\";;\n  esac\n  prev=\"$a\"\ndone\ncp {leg} \"$out\"\ncp {rep_src} \"$rep\"\n",
        leg = fixture("proj_overlay_leg.scip").display(),
        rep_src = fixture("proj_overlay_report.toml").display(),
    );
    std::fs::write(&script, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Mini repo with a real-leg slot + the checked-in plan fixture linked in
/// (plan copy sits at tmp root with the fixture `sources/` tree beside it
/// — the plan-root convention).
/// Variant whose report carries a mismatching graph_rev (SM-7 WARN leg).
fn fake_overlay_gen_rev_mismatch(dir: &Path) -> PathBuf {
    let script = dir.join("overlay-gen-rev");
    let report = dir.join("report-rev.toml");
    std::fs::write(
        &report,
        std::fs::read_to_string(fixture("proj_overlay_report.toml"))
            .unwrap()
            .replace("graph_rev = \"unstamped\"", "graph_rev = \"deadbeef\""),
    )
    .unwrap();
    let content = format!(
        "#!/bin/sh\nout=; rep=; prev=\nfor a in \"$@\"; do\n  case \"$prev\" in\n    --out) out=\"$a\";;\n    --report) rep=\"$a\";;\n  esac\n  prev=\"$a\"\ndone\ncp {leg} \"$out\"\ncp {r} \"$rep\"\n",
        leg = fixture("proj_overlay_leg.scip").display(),
        r = report.display(),
    );
    std::fs::write(&script, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// SM-3 orchestrator leg: a failing minter surfaces as Env fail(2).
fn fake_overlay_gen_failing(dir: &Path) -> PathBuf {
    let script = dir.join("overlay-gen-fail");
    std::fs::write(&script, "#!/bin/sh\necho 'gate exploded' >&2\nexit 3\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Identity-guard leg: the fake minter's report declares the WRONG
/// project identity — the orchestrator must fail loud with the real
/// index prefix (ai-rules dogfood relay regression).
fn fake_overlay_gen_wrong_identity(dir: &Path) -> PathBuf {
    let script = dir.join("overlay-gen-wid");
    let report = dir.join("report-wid.toml");
    std::fs::write(
        &report,
        std::fs::read_to_string(fixture("proj_overlay_report.toml"))
            .unwrap()
            .replace(
                "project = \"proj-fixture\"",
                "project = \"proj-fixture-dogfood\"",
            ),
    )
    .unwrap();
    let content = format!(
        "#!/bin/sh\nout=; rep=; prev=\nfor a in \"$@\"; do\n  case \"$prev\" in\n    --out) out=\"$a\";;\n    --report) rep=\"$a\";;\n  esac\n  prev=\"$a\"\ndone\ncp {leg} \"$out\"\ncp {r} \"$rep\"\n",
        leg = fixture("proj_overlay_leg.scip").display(),
        r = report.display(),
    );
    std::fs::write(&script, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

fn repo_with_plan(tmp: &tempfile::TempDir, plan_name: &str) -> (PathBuf, PathBuf) {
    let repo = tmp.path().join("repo");
    let scip = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&scip).unwrap();
    std::fs::copy(fixture("proj_real_leg.scip"), scip.join("index.scip")).unwrap();
    let plan = tmp.path().join(plan_name);
    std::fs::copy(fixture("proj-plan/plan.toml"), &plan).unwrap();
    let src_root = fixture("proj-plan/sources");
    for entry in walk_files(&src_root) {
        let rel = entry.strip_prefix(&src_root).unwrap().to_path_buf();
        let dst = tmp.path().join("sources").join(&rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::copy(&entry, &dst).unwrap();
    }
    (repo, plan)
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in std::fs::read_dir(&dir).unwrap().flatten() {
            if ent.file_type().unwrap().is_dir() {
                stack.push(ent.path());
            } else {
                out.push(ent.path());
            }
        }
    }
    out
}

// The lib-level entry keeps ToolOutput semantics without bin spawning.
fn lib_run(repo: &Path, plan: &Path, roots: &[PathBuf]) -> Result<String, String> {
    project_repo(repo, plan, roots, false).map_err(|e| match e {
        code_reality::project::ProjectError::Env(m)
        | code_reality::project::ProjectError::Core(m) => m,
    })
}

#[test]
fn happy_path_graft_hole_missing_labels() {
    let tmp = tempfile::tempdir().unwrap();
    fake_overlay_gen(tmp.path());
    let (repo, plan) = repo_with_plan(&tmp, "graft-demo.toml");
    let roots = vec![tmp.path().to_path_buf()];
    let out = lib_run(&repo, &plan, &roots).unwrap();

    assert!(out.starts_with("[projected] graft surface"), "{out}");
    assert!(out.contains("假想邊 1 條——宣告，非證據"), "{out}");
    assert!(
        out.contains("[projected] compute: real 2 sites → projected 3 sites（+1 投影）"),
        "{out}"
    );
    assert!(
        out.contains("+ fixmod/planned_coordinator.py:6 ← ")
            && out.contains("PlannedCoordinator#snapshot"),
        "{out}"
    );
    assert!(out.contains("[projected][WIRED] compute"), "{out}");
    assert!(out.contains("[projected][HOLE] untouched_helper"), "{out}");
    assert!(out.contains("[projected][MISSING] no_such_symbol"), "{out}");
    assert!(out.contains("slot: "), "{out}");
}

#[test]
fn real_slot_stays_byte_identical_and_rerun_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let _bin = fake_overlay_gen(tmp.path());
    let (repo, plan) = repo_with_plan(&tmp, "graft-demo.toml");
    let real_index = repo.join(".code-reality/scip/index.scip");
    let before = std::fs::read(&real_index).unwrap();
    let roots = vec![tmp.path().to_path_buf()];

    let first = lib_run(&repo, &plan, &roots).unwrap();
    let after = std::fs::read(&real_index).unwrap();
    assert_eq!(
        before, after,
        "non-pollution: real index bytes must not move"
    );
    // No sidecars sneak in beside the real slot either.
    let scip_dir = real_index.parent().unwrap();
    let entries: Vec<_> = std::fs::read_dir(scip_dir).unwrap().flatten().collect();
    assert_eq!(entries.len(), 1, "real slot dir must stay single-file");

    let second = lib_run(&repo, &plan, &roots).unwrap();
    assert_eq!(first, second, "rerun must be report-idempotent");
}

#[test]
fn two_slots_coexist_without_interference() {
    let tmp = tempfile::tempdir().unwrap();
    let _bin = fake_overlay_gen(tmp.path());
    let (repo, plan_a) = repo_with_plan(&tmp, "alpha.toml");
    let plan_b = tmp.path().join("beta.toml");
    std::fs::copy(fixture("proj-plan/plan.toml"), &plan_b).unwrap();
    let roots = vec![tmp.path().to_path_buf()];

    let a = lib_run(&repo, &plan_a, &roots).unwrap();
    let b = lib_run(&repo, &plan_b, &roots).unwrap();
    let slot_a = repo.join(".code-reality/projections/alpha");
    let slot_b = repo.join(".code-reality/projections/beta");
    assert!(slot_a.join("index.scip").exists());
    assert!(slot_b.join("index.scip").exists());
    assert!(a.contains("projections/alpha"), "{a}");
    assert!(b.contains("projections/beta"), "{b}");
}

#[test]
fn invalid_stem_fails_loud() {
    let tmp = tempfile::tempdir().unwrap();
    let _bin = fake_overlay_gen(tmp.path());
    let (repo, plan) = repo_with_plan(&tmp, "bad name.toml");
    let roots = vec![tmp.path().to_path_buf()];
    let err = lib_run(&repo, &plan, &roots).unwrap_err();
    assert!(err.contains("stem 非法"), "{err}");
}

#[test]
fn missing_bin_gives_install_guidance() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, plan) = repo_with_plan(&tmp, "graft-demo.toml");
    let roots = vec![tmp.path().join("nowhere")];
    let err = lib_run(&repo, &plan, &roots).unwrap_err();
    assert!(err.contains("uv tool install pyrefly-producer"), "{err}");
}

#[test]
fn missing_real_index_gives_build_guidance() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("empty-repo");
    std::fs::create_dir_all(&repo).unwrap();
    let plan = tmp.path().join("p.toml");
    std::fs::write(&plan, "[meta]\n").unwrap();
    let err = lib_run(&repo, &plan, &[tmp.path().to_path_buf()]).unwrap_err();
    assert!(err.contains("code-reality build"), "{err}");
}

/// Real-bin e2e (guarded: skips when the workspace release build is
/// absent — env-coupled skip-on-drift pattern).
#[test]
fn real_overlay_gen_e2e_when_built() {
    let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/overlay-gen");
    if !bin.is_file() {
        eprintln!(
            "[skip] {} 不在——cargo build --release -p pyrefly-producer 後重跑",
            bin.display()
        );
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (repo, plan) = repo_with_plan(&tmp, "e2e.toml");
    let roots = vec![bin.parent().unwrap().to_path_buf()];
    let out = lib_run(&repo, &plan, &roots).unwrap();
    assert!(out.contains("+ fixmod/planned_coordinator.py:6"), "{out}");
    assert!(out.contains("[projected][HOLE] untouched_helper"), "{out}");
}

#[test]
fn graph_rev_mismatch_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_overlay_gen_rev_mismatch(tmp.path());
    let _ = bin;
    // roots must resolve the rev-variant script under its own name.
    let dir = tmp.path();
    let script = dir.join("overlay-gen-rev");
    std::fs::rename(&script, dir.join("overlay-gen")).unwrap();
    let (repo, plan) = repo_with_plan(&tmp, "revcheck.toml");
    let out = lib_run(&repo, &plan, &[dir.to_path_buf()]).unwrap();
    assert!(out.contains("[WARN] graph rev"), "{out}");
}

#[test]
fn minter_failure_is_env_exit_two() {
    let tmp = tempfile::tempdir().unwrap();
    let _ = fake_overlay_gen_failing(tmp.path());
    let dir = tmp.path();
    std::fs::rename(dir.join("overlay-gen-fail"), dir.join("overlay-gen")).unwrap();
    let (repo, plan) = repo_with_plan(&tmp, "failleg.toml");
    let err = lib_run(&repo, &plan, &[dir.to_path_buf()]).unwrap_err();
    assert!(err.contains("overlay-gen 失敗"), "{err}");
    assert!(err.contains("gate exploded"), "{err}");
}

#[test]
fn run_arg_faces() {
    let out = code_reality::project::run(&["project"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--repo"), "{}", out.stderr);
    let out = code_reality::project::run(&["project", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("usage: code-reality project"));
}

#[test]
fn wrong_plan_identity_fails_loud_with_real_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let _ = fake_overlay_gen_wrong_identity(tmp.path());
    std::fs::rename(
        tmp.path().join("overlay-gen-wid"),
        tmp.path().join("overlay-gen"),
    )
    .unwrap();
    let (repo, plan) = repo_with_plan(&tmp, "wid.toml");
    let err = lib_run(&repo, &plan, &[tmp.path().to_path_buf()]).unwrap_err();
    assert!(err.contains("前綴不符"), "{err}");
    assert!(err.contains("proj-fixture-dogfood"), "{err}");
    assert!(err.contains("pyrefly python proj-fixture 0.1.0"), "{err}");
}
