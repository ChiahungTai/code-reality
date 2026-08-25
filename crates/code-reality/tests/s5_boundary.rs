//! R5-S3 boundary family tests — mirrors of test_boundary_build /
//! test_boundary case families on synthetic NT-shaped fixtures.

use code_reality::boundary::{load_sidecar, run_query};
use code_reality::boundary_build::{
    build_boundary, build_sidecar, parse_pyi, pyi_module, scan_rust_file, screaming_snake,
    PyClass, PyFunction,
};
use std::path::{Path, PathBuf};

const RS_SRC: &str = r#"use pyo3::prelude::*;

/// A live node.
#[pyclass(module = "nautilus_trader.live", get_all)]
pub struct LiveNode {
    pub actor_id: String,
    pub base_url_http: Option<String>,
}

#[pymethods]
impl LiveNode {
    #[new]
    fn new(actor_id: String) -> Self { todo!() }

    #[getter]
    fn get_actor_id(&self) -> &str { &self.actor_id }

    #[pyo3(name = "build")]
    fn py_build(&self) -> Self { todo!() }

    fn __repr__(&self) -> String { todo!() }

    async fn run(&self) {}
}

#[pyclass(module = "nautilus_trader.live")]
enum UsdM {
    #[pyo3(name = "SANDBOX")]
    Sandbox,
    LimitOrder,
}

#[pyfunction(module = "nautilus_trader.live")]
#[pyo3(name = "connect")]
fn py_connect() {}

#[pyclass]
struct NotStubbed;
"#;

const PYI_SRC: &str = r#"from enum import Enum

class LiveNode:
    def __init__(self, actor_id: str) -> None: ...
    @property
    def actor_id(self) -> str: ...
    def build(self) -> "LiveNode": ...
    def __repr__(self) -> str: ...
    async def run(self) -> None: ...

class UsdM(Enum):
    SANDBOX = 0
    USD_M = 1

def connect() -> None: ...
"#;

fn nt_fixture(tag: &str) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("crates/live/src")).unwrap();
    std::fs::create_dir_all(repo.join("python/nautilus_trader")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[scan_root]]\npath = \"crates/**/*.rs\"\npyi = \"python/nautilus_trader/**/*.pyi\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("crates/live/src/node.rs"), RS_SRC).unwrap();
    std::fs::write(repo.join("python/nautilus_trader/live.pyi"), PYI_SRC).unwrap();
    let g = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C").arg(&repo).args(args)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .status().unwrap();
    };
    g(&["init", "-q"]);
    g(&["add", "."]);
    g(&["commit", "-qm", "init"]);
    repo
}

#[test]
fn screaming_snake_conversion() {
    assert_eq!(screaming_snake("UsdM"), "USD_M");
    assert_eq!(screaming_snake("IsolatedMargin"), "ISOLATED_MARGIN");
    assert_eq!(screaming_snake("Sandbox"), "SANDBOX");
}

#[test]
fn pyi_module_rindex_semantics() {
    assert_eq!(
        pyi_module("/x/nautilus_trader/python/nautilus_trader/common/__init__.pyi").unwrap(),
        "nautilus_trader.common"
    );
    assert_eq!(
        pyi_module("/x/nautilus_trader/python/nautilus_trader/live.pyi").unwrap(),
        "nautilus_trader.live"
    );
}

#[test]
fn scan_rust_extracts_all_three_kinds() {
    let repo = nt_fixture("scan");
    let (classes, methods, functions) =
        scan_rust_file(&repo.join("crates/live/src/node.rs"), &repo);
    let names: Vec<&str> = classes.iter().map(|c| c.rust_name.as_str()).collect();
    assert!(names.contains(&"LiveNode") && names.contains(&"UsdM") && names.contains(&"NotStubbed"));
    let ln = classes.iter().find(|c| c.rust_name == "LiveNode").unwrap();
    assert_eq!(ln.py_module.as_deref(), Some("nautilus_trader.live"));
    // method set: new/getter(renamed exposed)/rename/dunder/async + field_property
    let kinds: Vec<&str> = methods.iter().map(|m| m.kind.as_str()).collect();
    for k in ["new", "getter", "method", "dunder", "variant", "field_property"] {
        assert!(kinds.contains(&k), "{k} missing: {kinds:?}");
    }
    let renamed = methods.iter().find(|m| m.rust_fn == "py_build").unwrap();
    assert_eq!(renamed.exposed, "build");
    assert!(renamed.renamed);
    let getter = methods.iter().find(|m| m.rust_fn == "get_actor_id").unwrap();
    assert_eq!(getter.exposed, "actor_id"); // get_ strip
    // variant rename honored; plain variant via SCREAMING_SNAKE
    let sandbox = methods.iter().find(|m| m.rust_fn == "Sandbox").unwrap();
    assert_eq!(sandbox.exposed, "SANDBOX");
    let limit = methods.iter().find(|m| m.rust_fn == "LimitOrder").unwrap();
    assert_eq!(limit.exposed, "LIMIT_ORDER");
    // pyfunction rename + py_ strip fallback
    let f = functions.iter().find(|f| f.rust_fn == "py_connect").unwrap();
    assert_eq!(f.exposed, "connect");
    assert_eq!(f.py_module.as_deref(), Some("nautilus_trader.live"));
    let _ = Path::new(".");
}

#[test]
fn build_boundary_reconciliation() {
    let repo = nt_fixture("reconcile");
    let (classes, methods, functions) =
        scan_rust_file(&repo.join("crates/live/src/node.rs"), &repo);
    let (py_classes, py_functions) =
        parse_pyi(&repo.join("python/nautilus_trader/live.pyi"), &repo).unwrap();
    let module = pyi_module("python/nautilus_trader/live.pyi").unwrap();
    let py_classes: Vec<(String, PyClass)> =
        py_classes.into_iter().map(|c| (module.clone(), c)).collect();
    let py_functions: Vec<(String, PyFunction)> =
        py_functions.into_iter().map(|f| (module.clone(), f)).collect();
    let (edges, cov) = build_boundary(&classes, &methods, &functions, &py_classes, &py_functions);
    // classes: LiveNode + UsdM matched; NotStubbed rs-only
    assert_eq!(cov.classes.matched, 2);
    assert_eq!(cov.classes.rs_only, 1);
    // methods: new(__init__ reparse)/getter/renamed/dunder/async/run matched;
    // field_property×2 rs-only (credential gap); variant SANDBOX matched
    assert!(cov.methods.matched >= 5, "{cov:?}");
    let kinds: Vec<&str> = edges.iter().map(|e| e.match_kind).collect();
    assert!(kinds.contains(&"PYO3_NAME_RENAME"));
    assert!(kinds.contains(&"GETTER_PROPERTY"));
    assert!(kinds.contains(&"ENUM_VARIANT"));
    assert!(kinds.contains(&"FIELD_PROPERTY")); // get_all: actor_id matches @property
    // function connect matched (PYO3_NAME_RENAME)
    assert_eq!(cov.functions.matched, 1);
}

#[test]
fn build_sidecar_and_query_roundtrip() {
    let repo = nt_fixture("roundtrip");
    let out_dir = repo.join("boundary");
    let db = build_sidecar(&repo, &out_dir).unwrap();
    assert!(db.to_string_lossy().ends_with(".db"));
    // query via the boundary face
    let (sc, warn) = load_sidecar(&repo, &out_dir, None).unwrap();
    assert!(warn.is_empty(), "{warn}"); // fresh build matches HEAD
    let out = run_query(&sc, "LiveNode", false);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.starts_with("[OK] LiveNode: "), "{}", out.stdout);
    assert!(out.stdout.contains("NAME_MATCH  crates/live/src/node.rs:"));
    // bare-name suffix + method expansion
    let out2 = run_query(&sc, "LiveNode.build", false);
    assert_eq!(out2.exit_code, 0);
    assert!(out2.stdout.contains("PYO3_NAME_RENAME"), "{}", out2.stdout);
    // --rs reverse
    let out3 = run_query(&sc, "LiveNode::py_build", true);
    assert_eq!(out3.exit_code, 0);
    // not-found: FAIL + candidates + exit 1
    let out4 = run_query(&sc, "LiveNod", false);
    assert_eq!(out4.exit_code, 1);
    assert!(out4.stdout.contains("[FAIL] symbol not found: LiveNod"), "{}", out4.stdout);
    assert!(out4.stdout.contains("候選: nautilus_trader.live.LiveNode"), "{}", out4.stdout);
}

#[test]
fn cli_build_and_query() {
    let repo = nt_fixture("cli");
    let out_dir = repo.join("b");
    let out = code_reality::boundary_build::run(&[
        "boundary_build",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("[OK] boundary sidecar: "), "{}", out.stdout);
    assert!(out.stdout.contains("class: 2/3"), "{}", out.stdout);
    let out2 = code_reality::boundary::run(&[
        "boundary",
        "LiveNode",
        "--repo",
        &repo.to_string_lossy(),
        "--sidecar-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out2.exit_code, 0, "{}{}", out2.stdout, out2.stderr);
    let out3 = code_reality::boundary::run(&["boundary", "X", "--repo", &repo.to_string_lossy()]);
    // default sidecar dir missing → crash
    assert_eq!(out3.exit_code, 1);
}
