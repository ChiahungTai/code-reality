//! JS/TS blueprint S6 capability-boundary tests: graph_audit (and the
//! scip_refs --audit wrapper) can never return clean-silent when JS/TS
//! nodes sit outside the rust-analyzer completeness oracle; hub_refs
//! --hazard marks JS/TS targets unsupported instead of an empty-clean
//! dynamic answer; project rejects JS/TS planned sources.

mod support;

use protobuf::Message;
use scip::types::{Document, Index};
use std::path::{Path, PathBuf};
use support::{doc, occ, write};

/// Test-side environment probe (the production `which` helper is
/// crate-internal — test topology must not shape production API).
fn has_rust_analyzer() -> bool {
    std::process::Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Build an index at the in-repo slot and materialize graph.db from it.
/// `cases`: (rel_path, kind) where kind picks a producer-shaped symbol.
fn build_graph(t: &tempfile::TempDir, docs: Vec<Document>) -> PathBuf {
    let repo = t.path().to_path_buf();
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot = slot_dir.join("index.scip");
    let mut index = Index::new();
    index.documents = docs;
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    code_reality::graph_db::build_from_cache_at(&repo, &slot).expect("graph build");
    repo
}

fn js_call_graph_docs() -> Vec<Document> {
    // lib.ts defines tsHelper (enc 4-6); app.mjs defines main (enc 1-8)
    // and calls tsHelper inside it (line 3) — with the real sources on
    // disk (js_sources) the tree-sitter mark makes it a CALLS edge (the
    // graph-engine callers_of query filters kind='CALLS').
    vec![
        doc(
            "src/lib.ts",
            vec![occ(
                "scip-typescript npm . . /r/src/`lib.ts`/tsHelper().",
                1,
                vec![4, 0, 8],
                Some(vec![4, 0, 6, 1]),
            )],
        ),
        doc(
            "src/app.mjs",
            vec![
                occ(
                    "scip-typescript npm . . /r/src/`app.mjs`/main().",
                    1,
                    vec![1, 0, 4],
                    Some(vec![1, 0, 8, 1]),
                ),
                occ(
                    "scip-typescript npm . . /r/src/`lib.ts`/tsHelper().",
                    0,
                    vec![2, 2, 10],
                    None,
                ),
            ],
        ),
    ]
}

/// Real on-disk syntax matching `js_call_graph_docs` positions.
fn js_sources(repo: &Path) {
    write(
        repo,
        "src/lib.ts",
        "export class Greeter {\n  greet() { return 'hi'; }\n}\nexport function tsHelper(): string {\n  return 'ts';\n}\n",
    );
    write(
        repo,
        "src/app.mjs",
        "import { tsHelper } from './lib.ts';\nexport function main() {\n  tsHelper();\n}\n",
    );
}

fn rust_graph_docs() -> Vec<Document> {
    vec![doc(
        "src/lib.rs",
        vec![occ(
            "rust-analyzer cargo x 0.1.0 kernel/open().",
            1,
            vec![0, 0, 3],
            Some(vec![0, 0, 2, 1]),
        )],
    )]
}

fn mixed_graph_docs() -> Vec<Document> {
    let mut docs = rust_graph_docs();
    docs.extend(js_call_graph_docs());
    docs
}

#[test]
fn s6_pure_js_graph_audit_never_clean() {
    let t = tempfile::tempdir().unwrap();
    let repo = build_graph(&t, js_call_graph_docs());
    let graph = repo.join(".code-reality/graph.db");
    let repo_s = repo.display().to_string();
    let graph_s = graph.display().to_string();
    let out =
        code_reality::graph_audit::run(&["graph_audit", "--repo", &repo_s, "--graph", &graph_s]);
    assert_ne!(out.exit_code, 0, "pure-JS graph must not pass: {out:?}");
    assert!(
        out.stderr.contains("僅支援 Rust") || out.stderr.contains("rust-analyzer 不在 PATH"),
        "either the capability guard or the ra env gate fires — never clean: {out:?}"
    );
    if has_rust_analyzer() {
        assert!(out.stderr.contains("javascript"), "{out:?}");
        assert_eq!(out.exit_code, 2, "capability-unsupported family: {out:?}");
    }
}

#[test]
fn s6_mixed_graph_audit_is_partial_not_clean() {
    if !has_rust_analyzer() {
        // env-coupled face: without rust-analyzer the loud env gate is
        // itself the correct non-clean answer; the Partial semantics are
        // covered by s6_coverage_decision_matrix.
        eprintln!("SKIP mixed e2e: rust-analyzer absent (coverage_decision covered separately)");
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let repo = build_graph(&t, mixed_graph_docs());
    let graph = repo.join(".code-reality/graph.db");
    let out = code_reality::graph_audit::run(&[
        "graph_audit",
        "--repo",
        &repo.display().to_string(),
        "--graph",
        &graph.display().to_string(),
    ]);
    assert_eq!(out.exit_code, 2, "partial coverage is non-passing: {out:?}");
    assert!(
        out.stdout.contains("[PARTIAL]") && out.stdout.contains("javascript"),
        "coverage statement must be in the verdict body: {out:?}"
    );
    // JSON face carries the additive coverage fields
    let out_json = code_reality::graph_audit::run(&[
        "graph_audit",
        "--repo",
        &repo.display().to_string(),
        "--graph",
        &graph.display().to_string(),
        "--json",
    ]);
    assert_eq!(out_json.exit_code, 2);
    let v: serde_json::Value = serde_json::from_str(&out_json.stdout).unwrap();
    assert_eq!(v["audited_languages"], serde_json::json!(["rust"]));
    assert!(
        v["unsupported_languages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "javascript"),
        "{}",
        out_json.stdout
    );
}

#[test]
fn s6_scip_refs_audit_wrapper_routes_the_same_guard() {
    let t = tempfile::tempdir().unwrap();
    let repo = build_graph(&t, js_call_graph_docs());
    if !has_rust_analyzer() {
        eprintln!("SKIP wrapper e2e: rust-analyzer absent");
        return;
    }
    let out = code_reality::cli::run(&[
        "scip_refs",
        "--audit",
        "--repo",
        &repo.display().to_string(),
    ]);
    assert_eq!(out.exit_code, 2, "{out:?}");
    assert!(
        out.stderr.contains("能力邊界") && out.stderr.contains("javascript"),
        "{out:?}"
    );
}

#[test]
fn s6_hazard_js_target_unsupported_both_flag_and_auto() {
    let t = tempfile::tempdir().unwrap();
    js_sources(t.path()); // real syntax → CALLS edge → callers query resolves
    let repo = build_graph(&t, js_call_graph_docs());
    let repo_s = repo.display().to_string();
    for args in [
        vec![
            "hub_refs", "--repo", &repo_s, "--json", "--hazard", "tsHelper",
        ],
        // auto-trigger: callers direction with low prod refs
        vec!["hub_refs", "--repo", &repo_s, "--json", "tsHelper"],
    ] {
        let out = code_reality::hub_refs::run(&args);
        assert_eq!(out.exit_code, 0, "{args:?}: {out:?}");
        let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            v["hazard_level"], "unsupported-js-ts",
            "{args:?}: {}",
            out.stdout
        );
        assert_eq!(v["hazard_supported"], false, "{args:?}: {}", out.stdout);
        assert!(
            v["hazard_findings"].as_array().unwrap().is_empty(),
            "{args:?}"
        );
        assert!(
            v["hazard_gate"]
                .as_str()
                .unwrap_or("")
                .contains("動態 hazard 覆蓋不成立"),
            "{args:?}: {}",
            out.stdout
        );
        // static aggregation stays usable
        assert!(
            v["aggregate"]["total_prod"].as_i64().unwrap() >= 1,
            "{args:?}"
        );
    }
    // text face says the same explicitly
    let out = code_reality::hub_refs::run(&["hub_refs", "--repo", &repo_s, "--hazard", "tsHelper"]);
    assert_eq!(out.exit_code, 0, "{out:?}");
    assert!(
        out.stdout.contains("動態 hazard 覆蓋不成立"),
        "text face limitation line: {out:?}"
    );
}

#[test]
fn s6_hazard_python_target_in_mixed_repo_still_supported() {
    let t = tempfile::tempdir().unwrap();
    let repo_path = t.path().to_path_buf();
    write(&repo_path, "app.py", "def f():\n    return 1\n");
    let mut docs = mixed_graph_docs();
    // python face: def f in app.py + a call from main (py symbol)
    docs.push(doc(
        "app.py",
        vec![occ(
            "pyrefly python proj 0.1.0 `m`/f().",
            1,
            vec![0, 4, 5],
            Some(vec![0, 0, 1, 11]),
        )],
    ));
    let repo = build_graph(&t, docs);
    let out = code_reality::hub_refs::run(&[
        "hub_refs",
        "--repo",
        &repo.display().to_string(),
        "--json",
        "--hazard",
        "f",
    ]);
    assert_eq!(out.exit_code, 0, "{out:?}");
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        v["hazard_supported"], true,
        "python target keeps hazard: {out:?}"
    );
    assert_ne!(v["hazard_level"], "unsupported-js-ts", "{out:?}");
}

#[test]
fn s6_project_rejects_js_ts_planned_sources() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.mjs"), "export const a = 1;\n").unwrap();
    // project requires a real index at the repo slot before evaluating
    // any plan — seed a minimal one so the test reaches OUR guard
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let mut seed = Index::new();
    seed.documents = vec![doc(
        "src/a.mjs",
        vec![occ(
            "scip-typescript npm . . /r/src/`a.mjs`/a().",
            1,
            vec![0, 0, 1],
            Some(vec![0, 0, 1, 1]),
        )],
    )];
    std::fs::write(slot_dir.join("index.scip"), seed.write_to_bytes().unwrap()).unwrap();
    let plan_dir = t.path().join("planA");
    std::fs::create_dir_all(plan_dir.join("sources")).unwrap();
    std::fs::write(&plan_dir.join("my-plan.toml"), "[plan]\n").unwrap();
    std::fs::write(plan_dir.join("sources/hypo.ts"), "export const x = 1;\n").unwrap();
    let out = code_reality::project::run(&[
        "project",
        "--repo",
        &repo.display().to_string(),
        "--plan",
        &plan_dir.join("my-plan.toml").display().to_string(),
    ]);
    assert_eq!(out.exit_code, 2, "{out:?}");
    assert!(
        out.stderr.contains("JS/TS") && out.stderr.contains("hypo.ts"),
        "rejection names the offender: {out:?}"
    );

    // python-only planned sources in the SAME JS-containing repo pass the
    // guard (scope = planned sources, not repo languages — SM-10)
    let plan_b = t.path().join("planB");
    std::fs::create_dir_all(plan_b.join("sources")).unwrap();
    std::fs::write(&plan_b.join("ok-plan.toml"), "[plan]\n").unwrap();
    std::fs::write(plan_b.join("sources/hypo.py"), "def x():\n    pass\n").unwrap();
    let out_b = code_reality::project::run(&[
        "project",
        "--repo",
        &repo.display().to_string(),
        "--plan",
        &plan_b.join("ok-plan.toml").display().to_string(),
    ]);
    assert!(
        !out_b.stderr.contains("JS/TS"),
        "python plan must not be over-blocked: {out_b:?}"
    );
    // it proceeds past the guard and stops at the next real gate
    // (overlay-gen not installed in the test env) — NOT at the guard
    assert!(
        out_b.stderr.contains("overlay-gen") || out_b.stderr.contains("安裝"),
        "guard scoping proof — next gate is the tool resolution: {out_b:?}"
    );
}

#[test]
fn s6_hazard_qualified_js_target_also_unsupported() {
    // muse P1-2: a `::`-qualified JS target (the shape graph output
    // teaches users to copy) never matches the bare-name column — the
    // guard must resolve from the RESOLVED target's producer prefix.
    let t = tempfile::tempdir().unwrap();
    js_sources(t.path());
    let repo = build_graph(&t, js_call_graph_docs());
    let repo_s = repo.display().to_string();
    let lib_ts = repo.join("src/lib.ts").display().to_string();
    let qualified = format!("{lib_ts}::tsHelper");
    let out = code_reality::hub_refs::run(&[
        "hub_refs", "--repo", &repo_s, "--json", "--hazard", &qualified,
    ]);
    assert_eq!(out.exit_code, 0, "{qualified}: {out:?}");
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        v["hazard_level"], "unsupported-js-ts",
        "{qualified}: {}",
        out.stdout
    );
    assert_eq!(v["hazard_supported"], false, "{qualified}");
}
