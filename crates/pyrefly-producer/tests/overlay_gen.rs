//! overlay-gen integration tests (EP ep-projected-graph-overlay S1):
//! golden end-to-end on a temp corpus (real pyrefly-index leg + overlay
//! mint + byte-determinism), the consistency gate's fail-loud path, the
//! plan-schema faces, and the relative-`--repo` regression pin for the
//! canonicalize fix (relative roots used to silently drop all
//! cross-module refs to "external").

use std::path::{Path, PathBuf};
use std::process::Command;

const PLAN: &str = r#"
[meta]
name = "graft-demo"
graph_rev = "unstamped"
project = "proj-fixture"
version = "0.1.0"

[[symbols]]
rel_path = "fixmod/planned_coordinator.py"
kind = "class"
name = "PlannedCoordinator"
scope = []

[[symbols]]
rel_path = "fixmod/planned_coordinator.py"
kind = "function"
name = "snapshot"
scope = [{ name = "PlannedCoordinator", class = true }]

[[edges]]
file = "fixmod/planned_coordinator.py"
needle = "compute(points)"
to_module = "fixmod.calc"
to_kind = "function"
to_name = "compute"
"#;

const PLANNED_SOURCE: &str = "from fixmod.calc import compute\n\n\nclass PlannedCoordinator:\n    def snapshot(self, points: list) -> list:\n        reference = compute(points)\n        return reference\n";

fn corpus(dir: &Path) {
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"proj-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("fixmod")).unwrap();
    std::fs::write(dir.join("fixmod/__init__.py"), "").unwrap();
    std::fs::write(
        dir.join("fixmod/calc.py"),
        "def compute(points: list) -> list:\n    return sorted(points)\n\n\ndef untouched_helper(x: int) -> int:\n    return x + 1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("fixmod/real_caller.py"),
        "from fixmod.calc import compute\n\n\ndef existing_consumer(points: list) -> list:\n    return compute(points)\n",
    )
    .unwrap();
}

fn plan_dir(dir: &Path, plan: &str) -> PathBuf {
    let root = dir.join("plan");
    std::fs::create_dir_all(root.join("sources/fixmod")).unwrap();
    std::fs::write(root.join("plan.toml"), plan).unwrap();
    std::fs::write(
        root.join("sources/fixmod/planned_coordinator.py"),
        PLANNED_SOURCE,
    )
    .unwrap();
    root
}

fn run_overlay_gen(
    plan: &Path,
    sources: &Path,
    out: &Path,
    report: Option<&Path>,
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_overlay-gen"));
    cmd.arg("--plan").arg(plan).arg("--sources").arg(sources).arg("--out").arg(out);
    if let Some(r) = report {
        cmd.arg("--report").arg(r);
    }
    cmd.output().unwrap()
}

#[test]
fn golden_e2e_byte_deterministic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    corpus(&repo);

    // Real leg via the actual engine (also pins the canonicalize fix:
    // the corpus path here is absolute; the relative form is pinned in
    // its own test below).
    let leg = tmp.path().join("leg.scip");
    let out = Command::new(env!("CARGO_BIN_EXE_pyrefly-index"))
        .arg("--repo")
        .arg(&repo)
        .arg("--out")
        .arg(&leg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 refs, 1 call sites"), "{stdout}");

    let root = plan_dir(tmp.path(), PLAN);
    let overlay = tmp.path().join("overlay.scip");
    let report = tmp.path().join("report.toml");
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &overlay, Some(&report));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("gate 1/1"));

    // Report contract fields the orchestrator consumes.
    let rep = std::fs::read_to_string(&report).unwrap();
    assert!(rep.contains("name = \"graft-demo\""), "{rep}");
    assert!(rep.contains("minted_defs = 3"), "{rep}"); // class + pseudo-ctor + method
    assert!(rep.contains("minted_edges = 1"), "{rep}");
    assert!(rep.contains("[[touched]]"), "{rep}");
    assert!(rep.contains("name = \"compute\""), "{rep}");
    assert!(rep.contains("overlay_files = [\"fixmod/planned_coordinator.py\"]"), "{rep}");

    // Byte-determinism: same plan + sources → identical overlay bytes.
    let overlay2 = tmp.path().join("overlay2.scip");
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &overlay2, None);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read(&overlay).unwrap(),
        std::fs::read(&overlay2).unwrap()
    );

    // The minted overlay is a valid scip.Index leg: cat-merged with the
    // real leg it stays parseable and the projected graft symbol ID is
    // present verbatim (single-source constructor shape).
    let merged = tmp.path().join("merged.scip");
    std::fs::write(
        &merged,
        [
            std::fs::read(&leg).unwrap(),
            std::fs::read(&overlay).unwrap(),
        ]
        .concat(),
    )
    .unwrap();
    let bytes = String::from_utf8_lossy(&std::fs::read(&merged).unwrap()).into_owned();
    assert!(bytes.contains("`fixmod.planned_coordinator`/PlannedCoordinator#"));
    assert!(bytes.contains("`fixmod.calc`/compute()."));
}

#[test]
fn gate_failure_is_loud_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    corpus(&repo);

    // Needle that never occurs in the planned source.
    let bad = PLAN.replace("compute(points)", "compute(points_x)");
    let root = plan_dir(tmp.path(), &bad);
    let overlay = tmp.path().join("overlay.scip");
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &overlay, None);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("一致性 gate 失敗"), "{stderr}");
    assert!(!overlay.exists(), "gate failure must not write the overlay");
}

#[test]
fn plan_schema_faces_fail_loud() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    corpus(&repo);

    // Illegal name (slot-path safety mirrors the orchestrator's stem rule).
    let bad = PLAN.replace("name = \"graft-demo\"", "name = \"../evil\"");
    let root = plan_dir(tmp.path(), &bad);
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &tmp.path().join("o.scip"), None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("name 非法"));

    // Unknown kind.
    let bad = PLAN.replace("kind = \"class\"", "kind = \"module\"");
    let root = plan_dir(tmp.path(), &bad);
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &tmp.path().join("o.scip"), None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("kind 非法"));

    // Ctor edge to a class not declared in [[symbols]] (B7b pairing).
    let bad = PLAN
        .replace("to_kind = \"function\"", "to_kind = \"class\"")
        .replace("to_name = \"compute\"", "to_name = \"UndeclaredClass\"");
    let root = plan_dir(tmp.path(), &bad);
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &tmp.path().join("o.scip"), None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("B7b"));
}

#[test]
fn usage_and_version_faces() {
    let out = Command::new(env!("CARGO_BIN_EXE_overlay-gen"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).trim().is_empty());

    let out = Command::new(env!("CARGO_BIN_EXE_overlay-gen"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("usage: overlay-gen"));

    let out = Command::new(env!("CARGO_BIN_EXE_overlay-gen"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert!(!out.status.success());
}

/// Regression pin for the canonicalize fix: a RELATIVE --repo used to
/// silently drop every cross-module ref (absolute engine paths fail the
/// corpus strip) — the stats line must show the resolved call site.
#[test]
fn pyrefly_index_relative_repo_resolves_cross_module_refs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    corpus(&repo);
    let out = Command::new(env!("CARGO_BIN_EXE_pyrefly-index"))
        .arg("--repo")
        .arg("repo") // relative to cwd
        .current_dir(tmp.path())
        .arg("--out")
        .arg(tmp.path().join("leg.scip"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1 refs, 1 call sites"),
        "relative --repo must resolve the cross-module call: {stdout}"
    );
}

/// F-19/CR-8: needle binds its FIRST textual occurrence — when that first
/// occurrence is not a call site (comment/docstring mention), the gate
/// must fail loud rather than silently binding the second (real) call.
#[test]
fn needle_first_occurrence_in_comment_fails_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plan");
    std::fs::create_dir_all(root.join("sources/fixmod")).unwrap();
    std::fs::write(root.join("plan.toml"), PLAN).unwrap();
    std::fs::write(
        root.join("sources/fixmod/planned_coordinator.py"),
        PLANNED_SOURCE.replace(
            "from fixmod.calc import compute",
            "# legend: compute(points) is delegated elsewhere\nfrom fixmod.calc import compute",
        ),
    )
    .unwrap();
    let out = run_overlay_gen(
        &root.join("plan.toml"),
        &root.join("sources"),
        &tmp.path().join("o.scip"),
        None,
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("非 compute 的 call site"), "{stderr}");
}

/// F-06: same-file symbols mint into ONE document (start_module per file,
/// not per symbol) — parse the overlay and count.
#[test]
fn same_file_symbols_single_document() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plan_dir(tmp.path(), PLAN);
    let overlay = tmp.path().join("overlay.scip");
    let out = run_overlay_gen(&root.join("plan.toml"), &root.join("sources"), &overlay, None);
    assert!(out.status.success());
    use protobuf::Message;
    let idx = scip::types::Index::parse_from_bytes(&std::fs::read(&overlay).unwrap()).unwrap();
    assert_eq!(idx.documents.len(), 1, "two symbols, one file → one document");
    let defs = idx.documents[0]
        .occurrences
        .iter()
        .filter(|o| o.symbol_roles & 1 != 0)
        .count();
    assert_eq!(defs, 3, "class + pseudo-ctor + method DEFs");
}

/// SM-9: an empty plan (meta only) yields a metadata-only overlay + empty
/// report, exit 0.
#[test]
fn empty_plan_is_vacuously_green() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plan");
    std::fs::create_dir_all(root.join("sources")).unwrap();
    std::fs::write(
        root.join("plan.toml"),
        "[meta]\nname = \"empty\"\nproject = \"proj-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let out = run_overlay_gen(
        &root.join("plan.toml"),
        &root.join("sources"),
        &tmp.path().join("o.scip"),
        Some(&tmp.path().join("r.toml")),
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rep = std::fs::read_to_string(tmp.path().join("r.toml")).unwrap();
    assert!(rep.contains("minted_defs = 0"), "{rep}");
    assert!(rep.contains("minted_edges = 0"), "{rep}");
}
