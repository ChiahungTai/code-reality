//! End-to-end integration (EP S1): fixture repo → emit SCIP index →
//! build-cache three-table db → `graph_db build` — the real consumer
//! pipeline (integrator-type segment: real boundaries, no mocks).
//!
//! The fixture is copied to a temp dir first so the derived
//! `<repo>/.code-reality/graph.db` never lands in the working tree.

use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
}

fn temp_repo() -> PathBuf {
    temp_repo_tagged("e2e")
}

/// Per-test temp dirs: cargo runs integration tests in parallel and a
/// shared pid-keyed path would have one test's cleanup delete another's
/// mid-emit fixture.
fn temp_repo_tagged(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pyrefly-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    copy_tree(&fixture(), &d);
    d
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let t = dst.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &t);
        } else {
            std::fs::copy(&p, &t).unwrap();
        }
    }
}

fn all_symbols(index: &scip::types::Index) -> Vec<String> {
    let mut v = Vec::new();
    for d in &index.documents {
        for o in &d.occurrences {
            v.push(o.symbol.clone());
        }
    }
    v
}

fn occurrences_in(index: &scip::types::Index, rel: &str) -> Vec<(String, i32)> {
    index
        .documents
        .iter()
        .filter(|d| d.relative_path == rel)
        .flat_map(|d| {
            d.occurrences
                .iter()
                .map(|o| (o.symbol.clone(), o.symbol_roles))
        })
        .collect()
}

#[test]
fn emit_build_cache_and_graph_db_on_fixture() {
    let repo = temp_repo();
    let index_path = repo.join("index.scip");
    // project_identity falls back to the repo basename for repos without
    // pyproject metadata — derive the expected discriminator the same way.
    let disc = format!(
        "pyrefly python {} 0.0.0 ",
        repo.file_name().unwrap().to_string_lossy()
    );

    let report = pyrefly_producer::emit(&repo, Some(&index_path)).expect("emit");
    // The fixture is fixed content — pin exact counts (loose >= bounds
    // let regressions pass silently; the line off-by-one escaped that way).
    assert_eq!(report.files, 2, "pkg/core.py + main.py");
    assert_eq!(report.defs, 10, "9 defs in core.py + run() in main.py");
    assert_eq!(report.references, 3, "Greeter import + top_fn import + CONSTANT load");
    assert_eq!(report.call_sites, 6, "4 in core.py + 2 in main.py");

    let loaded = code_reality::engine::load_index(&index_path).expect("parse index");
    let symbols = all_symbols(&loaded.index);

    // Def forms present.
    for expected in [
        format!("{disc}`pkg.core`/top_fn()."),
        format!("{disc}`pkg.core`/Greeter#"),
        format!("{disc}`pkg.core`/Greeter#greet()."),
        format!("{disc}`pkg.core`/CONSTANT.CONSTANT."),
        format!("{disc}`pkg.core`/Greeter#tag."),
        format!("{disc}`main`/run()."),
    ] {
        assert!(
            symbols.contains(&expected),
            "missing def symbol {expected} in {symbols:#?}"
        );
    }

    // Attribute-call resolution: `g.greet()` binds the METHOD, not the
    // receiver (the spike's whole-func-position bug class).
    assert!(
        symbols.contains(&format!("{disc}`pkg.core`/Greeter#greet().")),
        "greet call/reference missing"
    );

    // Constructor resolution: `Greeter()` lands on the initializer
    // (fn-shaped, gate-passing) — via the dunder pair when pyrefly
    // produces one, via the single __init__ target otherwise. It must
    // NOT only appear as the class symbol.
    assert!(
        symbols.contains(&format!("{disc}`pkg.core`/Greeter#__init__().")),
        "constructor target missing"
    );

    // Import sites in main.py reference the imported defs (Import role bit 2).
    let main_occs = occurrences_in(&loaded.index, "main.py");
    let import_ref = main_occs
        .iter()
        .any(|(s, r)| s.ends_with("`/Greeter#") && r & 2 != 0);
    assert!(
        import_ref,
        "import reference of Greeter missing in main.py: {main_occs:#?}"
    );

    // Nested-function face consistency (review R-2): the def face (scope
    // stack) and the target face (enclosing_chain) must mint the SAME
    // symbol for a function nested in a function — one def occurrence
    // plus at least one resolved call reference.
    let nested = format!("{disc}`pkg.core`/make().inner_helper().");
    let nested_rows = symbols.iter().filter(|s| **s == nested).count();
    assert!(
        nested_rows >= 2,
        "nested fn symbol {nested} appeared {nested_rows} times (need def + call ref)"
    );

    // Line contract (dual-review F-1 regression pin): the protobuf carries
    // 0-based lines — `engine::ln` adds the +1 on read. `def top_fn` sits
    // on physical line 17 of pkg/core.py → range[0] == 16.
    let top_fn_def = loaded
        .index
        .documents
        .iter()
        .filter(|d| d.relative_path == "pkg/core.py")
        .flat_map(|d| d.occurrences.iter())
        .find(|o| o.symbol.ends_with("`/top_fn().") && o.symbol_roles & 1 != 0)
        .expect("top_fn def occurrence");
    assert_eq!(top_fn_def.range.first(), Some(&16), "0-based def line");
    assert_eq!(
        top_fn_def.enclosing_range.first(),
        Some(&16),
        "0-based enclosing line"
    );

    // tool_info provenance.
    let md = loaded.index.metadata.as_ref().expect("metadata");
    let ti = md.tool_info.as_ref().expect("tool_info");
    assert_eq!(ti.name, "pyrefly-1.3");

    // build-cache: three-table db from the emitted index.
    let db_path = code_reality::cache::sqlite_path(&index_path);
    let stats =
        code_reality::cache::build_db(&loaded.index, &db_path, "e2ehead").expect("build_db");
    assert!(
        stats.symbols > 0 && stats.occurrences > 0,
        "stats: symbols={} occurrences={}",
        stats.symbols,
        stats.occurrences
    );

    // graph_db build consumes the slot (protobuf face — no cache db needed
    // in between, but both exist here like the real flow).
    let built =
        code_reality::graph_db::build_from_cache_at(&repo, &index_path).expect("graph_db build");
    assert!(built.nodes > 0, "nodes: {}", built.nodes);
    assert!(built.edges > 0, "edges: {}", built.edges);

    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn emit_fails_loud_on_repo_without_python() {
    let empty = std::env::temp_dir().join(format!("pyrefly-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).unwrap();
    let err = pyrefly_producer::emit(&empty, None);
    assert!(err.is_err(), "expected fail-loud on no-python repo");
    let _ = std::fs::remove_dir_all(&empty);
}

#[test]
fn emit_is_byte_deterministic() {
    // pyrefly's Handles iterate an internal HashSet — without the
    // rel-path sort the document order (and the whole index) flips
    // run to run (dual-review F-2).
    let repo = temp_repo_tagged("det");
    let a = repo.join("emit-a.scip");
    let b = repo.join("emit-b.scip");
    pyrefly_producer::emit(&repo, Some(&a)).expect("emit a");
    pyrefly_producer::emit(&repo, Some(&b)).expect("emit b");
    let bytes_a = std::fs::read(&a).expect("read a");
    let bytes_b = std::fs::read(&b).expect("read b");
    assert_eq!(bytes_a, bytes_b, "two emits of the same repo must be byte-identical");
    let _ = std::fs::remove_dir_all(&repo);
}
