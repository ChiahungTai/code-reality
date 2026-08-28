//! R4② hazard×hub_refs tests — mirrors of the frozen
//! `test_hazard.py`/`test_hub_refs.py` case families (both suites pin
//! the same fixtures = the committed differential), plus serializer
//! byte pins. The rg runner and CRG subprocess faces are covered by the
//! dogfood manual step (external binaries; CLI parity needs uvx+CRG).

use code_reality::hazard::{
    build_getattr_pattern, build_importlib_pattern, build_strentenum_patterns, classify_rg_lines,
    detect_getattr_dispatch, detect_importlib_lazy_load, detect_protocol_duck_typing,
    detect_registry_auto_discovery, detect_static_edge_gap, detect_strentenum_string_dispatch,
    full_findings, hazard_gate_warning, method_name, parse_symbol_facts, resident_findings,
    symbol_facts,
};
use code_reality::hub_refs::{aggregate, caller_files_of, json_payload, resolve_qualified, run};
use code_reality::profile::{HazardRegistry, Profile};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

const STRENTENUM_SRC: &str = "\
from enum import StrEnum

class Interval(StrEnum):
    ONE = \"1d\"
    WEEK = \"1w\"

def other(): ...
";

const STR_ENUM_COMMA: &str = "class Foo(str, Enum):\n    A = \"a\"\n";

fn facts_of(src: &str, symbol: &str) -> code_reality::hazard::SymbolFacts {
    parse_symbol_facts(src, symbol)
}

fn lines_rg(lines: &[&str]) -> impl Fn(&[&str]) -> Result<Vec<String>, String> {
    let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    move |_args: &[&str]| Ok(owned.clone())
}

// ---------- parse_symbol_facts ----------

#[test]
fn strentenum_detected_with_values() {
    let f = facts_of(STRENTENUM_SRC, "Interval");
    assert!(f.is_class);
    assert!(f.is_strentenum);
    assert!(!f.is_protocol);
    assert_eq!(f.enum_values, vec!["1d".to_string(), "1w".to_string()]);
    assert_eq!(f.bases, vec!["StrEnum".to_string()]);
}

#[test]
fn str_enum_comma_form_detected() {
    let f = facts_of(STR_ENUM_COMMA, "Foo");
    assert!(f.is_strentenum);
    assert_eq!(f.enum_values, vec!["a".to_string()]);
}

#[test]
fn protocol_detected() {
    let f = facts_of("class Repo(Protocol):\n    def load(self): ...\n", "Repo");
    assert!(f.is_protocol);
    assert!(!f.is_strentenum);
    assert!(f.is_class);
}

#[test]
fn plain_class_no_traits_and_missing_symbol_safe() {
    let f = facts_of("class Plain:\n    X = \"v\"\n", "Plain");
    assert!(f.is_class && !f.is_strentenum && !f.is_protocol);
    // X IS a top-level class member → captured (same as Python)
    assert_eq!(f.enum_values, vec!["v".to_string()]);
    let missing = facts_of("class Other:\n    pass\n", "Nope");
    assert!(!missing.is_class && missing.bases.is_empty());
}

#[test]
fn syntax_error_source_safe() {
    let f = facts_of("def broken(:\n", "Anything");
    assert!(!f.is_class);
}

#[test]
fn dotted_base_and_nested_class_walk() {
    let src = "def outer():\n    class Inner(enum.StrEnum):\n        A = \"a\"\n";
    let f = facts_of(src, "Inner");
    assert!(f.is_class);
    assert_eq!(f.bases, vec!["enum.StrEnum".to_string()]);
    // note: enum.StrEnum is NOT in STR_ENUM_BASES (name-string compare)
    assert!(!f.is_strentenum);
}

// ---------- pattern builders + classify ----------

#[test]
fn method_name_forms() {
    assert_eq!(method_name("Class.method"), Some("method".to_string()));
    assert_eq!(
        method_name("/abs/path.rs::Class.method"),
        Some("method".to_string())
    );
    assert_eq!(method_name("Class"), None);
}

#[test]
fn pattern_builders() {
    assert_eq!(
        build_getattr_pattern("Interval"),
        r#"getattr\(\s*[A-Za-z_][A-Za-z0-9_.]*\s*,\s*["']Interval["']"#
    );
    assert_eq!(
        build_strentenum_patterns(&["1d".to_string(), "1w".to_string()]),
        vec!["\"1d\"".to_string(), "\"1w\"".to_string()]
    );
    assert_eq!(
        build_importlib_pattern("mosaic.alpha.core"),
        r#"import_module\(\s*["']mosaic\.alpha\.core["']"#
    );
}

fn profile_fixture() -> Profile {
    Profile {
        modules: vec![],
        exclude: vec![".venv/".into()],
        scan_roots: vec![],
        hazard_registries: vec![],
    }
}

#[test]
fn classify_rg_lines_splits() {
    let lines: Vec<String> = vec![
        "pkg/a.py:1:x".to_string(),
        "tests/test_a.py:2:y".to_string(),
        ".venv/lib/z.py:3:w".to_string(),
    ];
    let (prod, test, excluded) = classify_rg_lines(&lines, Some(&profile_fixture()));
    assert_eq!(prod, vec!["pkg/a.py:1:x"]);
    assert_eq!(test, vec!["tests/test_a.py:2:y"]);
    assert_eq!(excluded, vec![".venv/lib/z.py:3:w"]);
}

// ---------- detectors (injected rg) ----------

#[test]
fn strentenum_dispatch_counts_and_excludes_definition() {
    let mut f = facts_of(STRENTENUM_SRC, "Interval");
    f.rel_path = Some("pkg/enum_defs.py".into());
    let rg = lines_rg(&[
        "pkg/enum_defs.py:3:ONE = \"1d\"", // definition file — excluded
        "pkg/use.py:7:v = \"1d\"",
        "tests/test_use.py:9:assert \"1w\"",
        "tests/test_use.py:10:assert \"1d\"",
    ]);
    let finding = detect_strentenum_string_dispatch(&f, &rg, Some(&profile_fixture()))
        .unwrap()
        .unwrap();
    assert_eq!(finding.count, 3);
    assert_eq!(
        finding.detail,
        vec![("prod".to_string(), 1), ("test".to_string(), 2)]
    );
    assert_eq!(
        finding.evidence,
        vec!["pkg/use.py:7:v = \"1d\"".to_string()]
    );
    assert!(finding.summary.contains("1 處 prod + 2 處 test"));
}

#[test]
fn getattr_dispatch_shape() {
    let f = facts_of("class Loader:\n    pass\n", "Loader");
    let rg = lines_rg(&[
        "pkg/a.py:5:m = getattr(x, \"Loader\")",
        "tests/t.py:1:getattr(y, \"Loader\")",
    ]);
    let finding = detect_getattr_dispatch(&f, &rg, None).unwrap().unwrap();
    assert_eq!(finding.count, 2);
    assert!(finding.summary.contains("1 prod + 1 test"));
}

#[test]
fn registry_detection_by_prefix_and_suffix() {
    let mut f = facts_of("class AlphaCondition:\n    pass\n", "AlphaCondition");
    f.is_class = true;
    f.rel_path = Some("mosaic_alpha/conditions/alpha.py".into());
    let regs = vec![HazardRegistry {
        package_prefix: "mosaic_alpha/conditions/".into(),
        suffix: "Condition".into(),
        register_fn: "auto_register".into(),
        registry: "REGISTRY".into(),
        evidence: "x.py:1".into(),
    }];
    let finding = detect_registry_auto_discovery(&f, &regs).unwrap();
    assert_eq!(finding.count, 1);
    assert!(finding
        .summary
        .contains("經 auto_register() 註冊到 REGISTRY"));
    assert_eq!(finding.evidence, vec!["x.py:1".to_string()]);
    // non-matching suffix
    let mut g = f.clone();
    g.name = "Beta".into();
    assert!(detect_registry_auto_discovery(&g, &regs).is_none());
}

#[test]
fn protocol_and_importlib_detectors() {
    let f = facts_of("class Loader(Protocol):\n    pass\n", "Loader");
    let rg = lines_rg(&["pkg/a.py:3:def f(x: Loader):", "tests/t.py:2:y: Loader"]);
    let finding = detect_protocol_duck_typing(&f, &rg, None).unwrap().unwrap();
    assert_eq!(finding.count, 2);

    let mut g = facts_of("class Anything:\n    pass\n", "Anything");
    g.module = Some("pkg.deep.mod".into());
    let rg2 = lines_rg(&["main.py:4:m = import_module(\"pkg.deep.mod\")"]);
    let finding2 = detect_importlib_lazy_load(&g, &rg2, None).unwrap().unwrap();
    assert_eq!(finding2.count, 1);
    assert!(finding2.summary.contains("import_module(\"pkg.deep.mod\")"));
}

#[test]
fn static_edge_gap_two_forms_and_set_diff() {
    // bare class form
    let f = facts_of("class Factor:\n    pass\n", "Factor");
    let rg = lines_rg(&[
        "pkg/a.py:1:Factor(",
        "pkg/b.py:2:x = Factor()",
        "tests/t.py:3:Factor()",
    ]);
    let mut baseline = BTreeSet::new();
    baseline.insert("pkg/a.py".to_string());
    let finding = detect_static_edge_gap(&f, Some(&baseline), &rg, None)
        .unwrap()
        .unwrap();
    assert_eq!(finding.count, 2); // pkg/b.py + tests/t.py
    assert!(finding.summary.contains("prod 1 / test 1"));

    // method form
    let rg_m = lines_rg(&["pkg/c.py:9:o.run(", "pkg/d.py:1:o.run()"]);
    let mut baseline2 = BTreeSet::new();
    baseline2.insert("pkg/c.py".to_string());
    let finding2 = detect_static_edge_gap(&f, Some(&baseline2), &rg_m, Some("run"))
        .unwrap()
        .unwrap();
    assert_eq!(finding2.count, 1);

    // None baseline skips
    assert!(detect_static_edge_gap(&f, None, &rg, None)
        .unwrap()
        .is_none());
    // no diff → no finding
    let mut baseline3 = BTreeSet::new();
    baseline3.insert("pkg/a.py".to_string());
    baseline3.insert("pkg/b.py".to_string());
    baseline3.insert("tests/t.py".to_string());
    assert!(detect_static_edge_gap(&f, Some(&baseline3), &rg, None)
        .unwrap()
        .is_none());
}

#[test]
fn resident_vs_full_and_gate() {
    let mut f = facts_of(STRENTENUM_SRC, "Interval");
    f.rel_path = Some("pkg/e.py".into());
    f.module = Some("pkg.e".into());
    let resident = resident_findings(&f, &[]);
    assert_eq!(resident.len(), 1);
    assert_eq!(resident[0].count, 0); // existence signal
    assert!(resident[0].summary.contains("StrEnum class（'1d', '1w'）"));

    // arg-aware injection: only the strentenum -F query yields lines
    let rg = |args: &[&str]| -> Result<Vec<String>, String> {
        if args.contains(&"-F") {
            Ok(vec![
                "pkg/x.py:1:\"1d\"".to_string(),
                "pkg/x.py:2:\"1w\"".to_string(),
            ])
        } else {
            Ok(Vec::new())
        }
    };
    let full = full_findings(&f, &[], &rg, None, None, None).unwrap();
    assert_eq!(full.len(), 1);
    assert_eq!(full[0].count, 2);

    // gate: threshold inclusive at 2
    let findings = vec![full[0].clone()];
    let w = hazard_gate_warning(2, 0, &findings, 2).unwrap();
    assert!(w.starts_with(
        "[WARN] 靜態 prod callers 僅 2 但命中 1 類 dynamic hazard（strentenum-string-dispatch）"
    ));
    assert!(hazard_gate_warning(3, 0, &findings, 2).is_none());
    assert!(hazard_gate_warning(2, 0, &[], 2).is_none());
}

// ---------- symbol_facts via nodes table ----------

fn repo_with_nodes(tag: &str, dup: bool) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    // canonical up front: nodes-table file paths must relativize against
    // the resolved root (macOS tempfile /var symlink trap)
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::create_dir_all(repo.join(".code-reality")).unwrap();
    let src = repo.join("pkg/mod_.py");
    std::fs::write(&src, STRENTENUM_SRC).unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    let abs = src.to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.nodes.push(graph_db_fixture::NodeSeed {
        name: "Interval".into(),
        parent: None,
        qname: format!("{abs}::Interval"),
        file_path: abs.clone(),
    });
    spec.node_attrs.push((
        format!("{abs}::Interval"),
        graph_db_fixture::NodeAttr {
            kind: "Class",
            language: "python",
            is_test: 0,
            community_id: None,
        },
    ));
    if dup {
        spec.nodes.push(graph_db_fixture::NodeSeed {
            name: "Interval".into(),
            parent: None,
            qname: format!("{abs}2::Interval"),
            file_path: format!("{abs}2"),
        });
        spec.node_attrs.push((
            format!("{abs}2::Interval"),
            graph_db_fixture::NodeAttr {
                kind: "Class",
                language: "python",
                is_test: 0,
                community_id: None,
            },
        ));
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    repo
}

#[test]
fn symbol_facts_unique_and_degraded() {
    let repo = repo_with_nodes("uniq", false);
    let f = symbol_facts("Interval", &repo, None).unwrap();
    assert_eq!(f.rel_path.as_deref(), Some("pkg/mod_.py"));
    assert!(f.is_strentenum);
    assert_eq!(f.module.as_deref(), Some("pkg.mod_"));
    assert_eq!(f.kind.as_deref(), Some("Class"));

    // multiple matches degrade to name-only facts (advisory, no crash)
    let repo2 = repo_with_nodes("dup", true);
    let f2 = symbol_facts("Interval", &repo2, None).unwrap();
    assert!(f2.rel_path.is_none() && !f2.is_class);

    // missing db degrades too
    let tmp = tempfile::tempdir().unwrap();
    let f3 = symbol_facts("X", tmp.path(), None).unwrap();
    assert_eq!(f3.name, "X");
}

// ---------- hub_refs aggregate / payload ----------

fn results_json(items: &[(&str, bool)]) -> serde_json::Value {
    json!(items
        .iter()
        .map(|(fp, t)| json!({"file_path": fp, "is_test": t}))
        .collect::<Vec<_>>())
}

#[test]
fn aggregate_counts_split_top_and_outside() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("r");
    std::fs::create_dir_all(&repo).unwrap();
    let repo = std::fs::canonicalize(&repo).unwrap();
    let results = results_json(&[
        ("{repo}/pkg/a.py", false),
        ("{repo}/pkg/b.py", false),
        ("{repo}/pkg/a.py", false),
        ("{repo}/tests/unit/x.py", false), // is_test false but tests/ prefix
        ("{repo}/tests/y.py", true),
        ("/elsewhere/z.py", false),   // outside
        ("{repo}/.venv/w.py", false), // excluded
    ])
    .to_string()
    .replace("{repo}", &repo.display().to_string());
    let arr: Vec<serde_json::Value> = serde_json::from_str::<serde_json::Value>(&results)
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let agg = aggregate(&arr, &repo, 20).unwrap();
    assert_eq!(agg.total_prod, 3); // a, b, a
    assert_eq!(agg.total_test, 2);
    assert_eq!(agg.excluded, 1);
    assert_eq!(agg.outside, 1);
    assert_eq!(agg.prod, vec![("pkg".to_string(), 3)]);
    assert_eq!(
        agg.test,
        vec![("tests/unit".to_string(), 1), ("tests".to_string(), 1)]
    );
    // top truncation
    let agg2 = aggregate(&arr, &repo, 1).unwrap();
    assert_eq!(agg2.test.len(), 1);
}

#[test]
fn aggregate_tie_keeps_first_seen() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let items = [
        ("{repo}/zzz/a.py", false),
        ("{repo}/aaa/b.py", false),
        ("{repo}/aaa/c.py", false),
        ("{repo}/zzz/d.py", false),
    ];
    let arr: Vec<serde_json::Value> = items
        .iter()
        .map(|(fp, t)| {
            json!({"file_path": fp.replace("{repo}", &repo.display().to_string()), "is_test": t})
        })
        .collect();
    let agg = aggregate(&arr, &repo, 20).unwrap();
    // both 2 — tie keeps FIRST-SEEN (zzz was inserted before aaa)
    assert_eq!(agg.prod[0], ("zzz".to_string(), 2));
    assert_eq!(agg.prod[1], ("aaa".to_string(), 2));
}

#[test]
fn caller_files_set() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let arr = vec![
        json!({"file_path": format!("{}/a.py", repo.display()), "is_test": false}),
        json!({"file_path": format!("{}/.venv/x.py", repo.display()), "is_test": false}),
    ];
    let files = caller_files_of(&arr, &repo, None);
    assert_eq!(files, BTreeSet::from(["a.py".to_string()]));
}

#[test]
fn json_payload_and_compact_bytes() {
    let agg = code_reality::hub_refs::AggResult {
        prod: vec![("pkg".to_string(), 3)],
        test: vec![],
        total_prod: 3,
        total_test: 0,
        excluded: 1,
        outside: 0,
    };
    let findings = vec![code_reality::hazard::HazardFinding {
        kind: "getattr-string-dispatch".into(),
        count: 2,
        summary: "s".into(),
        evidence: vec!["e1".into()],
        detail: vec![("prod".to_string(), 1), ("test".to_string(), 1)],
    }];
    let payload = json_payload(
        "Sym",
        "/abs::Sym",
        "callers",
        &agg,
        &findings,
        None,
        0,
        "full",
    );
    let text = code_reality::common::to_json_py_compact(&payload);
    // Python default separators: ", " and ": " — pinned shape
    assert!(
        text.starts_with("{\"symbol\": \"Sym\", \"target\": \"/abs::Sym\""),
        "{text}"
    );
    assert!(text.contains("\"hazard_findings\": [{\"kind\": \"getattr-string-dispatch\""));
    assert!(text.contains("\"detail\": {\"prod\": 1, \"test\": 1}"));
    assert!(text.contains("\"hazard_gate\": null"));
}

// ---------- make_rg_runner (real rg subprocess) ----------

#[test]
fn rg_runner_strips_prefix_and_honors_exclusions() {
    use code_reality::hazard::make_rg_runner;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::create_dir_all(repo.join(".venv")).unwrap();
    std::fs::write(repo.join("pkg/a.py"), "NEEDLE_FOUND = 1\n").unwrap();
    std::fs::write(repo.join(".venv/b.py"), "NEEDLE_HIDDEN = 1\n").unwrap();
    let rg = make_rg_runner(repo);
    let lines = rg(&["-F", "NEEDLE"]).unwrap();
    assert_eq!(lines, vec!["pkg/a.py:1:NEEDLE_FOUND = 1".to_string()]);
    // no matches → empty, exit 1 tolerated
    assert!(rg(&["-F", "NOT_THERE"]).unwrap().is_empty());
}

#[test]
fn rg_runner_builder_patterns_compatible() {
    use code_reality::hazard::make_rg_runner;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(
        repo.join("pkg/use.py"),
        "v = getattr(x, \"Loader\")\nm = import_module(\"pkg.deep.mod\")\nq = \"1d\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("tests/t.py"), "assert \"1d\"\n").unwrap();
    let rg = make_rg_runner(repo);
    let getattr = rg(&[&build_getattr_pattern("Loader")]).unwrap();
    assert_eq!(
        getattr,
        vec!["pkg/use.py:1:v = getattr(x, \"Loader\")".to_string()]
    );
    let importlib = rg(&[&build_importlib_pattern("pkg.deep.mod")]).unwrap();
    assert_eq!(importlib.len(), 1);
    let mut owned = vec!["-F".to_string()];
    for p in build_strentenum_patterns(&["1d".to_string()]) {
        owned.push("-e".to_string());
        owned.push(p);
    }
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();
    let strent = rg(&args).unwrap();
    assert_eq!(strent.len(), 2); // prod + test
}

// ---------- require_ok output faces ----------

#[test]
fn require_ok_faces() {
    use code_reality::hub_refs::require_ok_test_hook;
    // ok passes
    assert!(require_ok_test_hook(&json!({"status": "ok"})).is_ok());
    // not_found: FAIL on STDOUT, message on stderr, exit 1 (F-01 contract)
    let out = require_ok_test_hook(&json!({
        "status": "not_found", "summary": "no such symbol",
        "candidates": [
            {"qualified_name": "/a::Foo", "is_test": false},
            {"qualified_name": "/b::Bar", "is_test": true},
        ]
    }))
    .unwrap_err();
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stdout,
        "[FAIL] CRG not_found: no such symbol\n  候選: /a::Foo  (is_test=false)\n  候選: /b::Bar  (is_test=true)\n"
    );
    assert_eq!(out.stderr, "CRG query not_found: no such symbol\n");
}

// ---------- resolve_qualified via nodes ----------

#[test]
fn resolve_qualified_faces() {
    let repo = repo_with_nodes("resolve", false);
    // bare exact
    assert_eq!(
        resolve_qualified("Interval", &repo)
            .map_err(|o| o.exit_code)
            .unwrap(),
        format!("{}/pkg/mod_.py::Interval", repo.display())
    );
    // qualified passthrough
    assert_eq!(resolve_qualified("x::y", &repo).unwrap(), "x::y");
    // not found: exit 1, empty stdout
    let out = resolve_qualified("Nope", &repo).unwrap_err();
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("symbol not found: Nope"));
    // ambiguous: FAIL list on stdout
    let repo2 = repo_with_nodes("ambig", true);
    let out2 = resolve_qualified("Interval", &repo2).unwrap_err();
    assert_eq!(out2.exit_code, 1);
    assert!(out2.stdout.starts_with("[FAIL] 'Interval' 匹配 2 個 node"));
}

// ---------- CLI faces ----------

#[test]
fn cli_help_and_usage_family() {
    let out = run(&["hub_refs", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out
        .stdout
        .starts_with("usage: hub_refs [-h] [--repo REPO] [--direction {callers,callees}]\n"));
    let out = run(&["hub_refs"]);
    assert_eq!(out.exit_code, 2);
    let out = run(&["hub_refs", "Sym", "--direction", "sideways"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("invalid choice"));
    let out = run(&["hub_refs", "Sym", "--top", "abc"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("invalid int value"));
}

#[test]
fn cli_missing_db_crash_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    // bare symbol hits nodes resolution first → missing db crash
    let out = run(&["hub_refs", "Sym", "--repo", &repo.to_string_lossy()]);
    assert_eq!(out.exit_code, 1, "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("graph.db 不存在"), "{}", out.stderr);
}

// ---------- S2 cutover: refs_query + resolve on the self-owned db ----------

mod graph_db_fixture;

use code_reality::hub_refs::{refs_query, resolve_symbol};

fn s2_repo(tag: &str) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::create_dir_all(repo.join(".code-reality")).unwrap();
    let db = repo.join(".code-reality/graph.db");
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let spec = graph_db_fixture::GraphDbSpec {
        nodes: vec![
            graph_db_fixture::NodeSeed {
                name: "Interval".into(),
                parent: None,
                qname: format!("{}::Interval", abs("pkg/mod_.py")),
                file_path: abs("pkg/mod_.py"),
            },
            graph_db_fixture::NodeSeed {
                name: "caller_fn".into(),
                parent: None,
                qname: "prod::caller_fn".into(),
                file_path: abs("pkg/calls.py"),
            },
            graph_db_fixture::NodeSeed {
                name: "test_uses".into(),
                parent: None,
                qname: "test::test_uses".into(),
                file_path: abs("tests/test_x.py"),
            },
        ],
        node_attrs: vec![
            (
                "prod::caller_fn".into(),
                graph_db_fixture::NodeAttr {
                    kind: "Function",
                    language: "python",
                    is_test: 0,
                    community_id: None,
                },
            ),
            (
                "test::test_uses".into(),
                graph_db_fixture::NodeAttr {
                    kind: "Function",
                    language: "python",
                    is_test: 1,
                    community_id: None,
                },
            ),
        ],
        edges: vec![
            (
                "CALLS".into(),
                "prod::caller_fn".into(),
                format!("{}::Interval", abs("pkg/mod_.py")),
            ),
            (
                "CALLS".into(),
                "test::test_uses".into(),
                format!("{}::Interval", abs("pkg/mod_.py")),
            ),
        ],
        ..Default::default()
    };
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    repo
}

#[test]
fn refs_query_callers_reads_new_db_with_test_flags() {
    let repo = s2_repo("refsq");
    let db = repo.join(".code-reality/graph.db");
    let target = format!("{}/pkg/mod_.py::Interval", repo.display());
    let resp = refs_query(&db, "callers_of", &target).unwrap();
    assert_eq!(resp["status"], "ok");
    let results = resp["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{results:?}");
    let test_row = results
        .iter()
        .find(|r| r["file_path"].as_str().unwrap().contains("tests/"))
        .unwrap();
    assert_eq!(
        test_row["is_test"], true,
        "caller node is_test joins through"
    );
}

#[test]
fn resolve_symbol_end_to_end_on_new_db() {
    let repo = s2_repo("resolve");
    // dotted form resolves via name+parent_name (no parent here -> bare)
    let resp = resolve_symbol("Interval", &repo, "callers").unwrap();
    let results = resp["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{results:?}");
    // :: form resolves onto the node's symbol key
    let resp2 = resolve_symbol(
        &format!("{}/pkg/mod_.py::Interval", repo.display()),
        &repo,
        "callers",
    )
    .unwrap();
    assert_eq!(resp2["results"].as_array().unwrap().len(), 2);
}

#[test]
fn resolve_symbol_missing_db_and_unknown_symbol_fail_loud() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let out = resolve_symbol("Nope", &repo, "callers").unwrap_err();
    assert!(out.stderr.contains("graph_db build"), "{}", out.stderr);
    let repo2 = s2_repo("missing-sym");
    let out2 = resolve_symbol("Nope", &repo2, "callers").unwrap_err();
    assert!(out2.stderr.contains("symbol not found"), "{}", out2.stderr);
}

#[test]
fn resolve_qualified_dotted_producer_key_retries_qname_lookup() {
    let repo = s2_repo("retry");
    let file = repo.join("pkg/mod_.py");
    // producer-form qualified name: contains both `::` and `.` — the
    // parent_name branch misses (producer rows carry no parent_name),
    // the retry on qname/symbol must resolve to the node's symbol
    let q = format!("{}::Interval", file.display());
    let resolved = code_reality::hub_refs::resolve_qualified(&q, &repo).unwrap();
    assert_eq!(resolved, q, "producer qname resolves onto its symbol key");
}
