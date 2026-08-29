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
    assert_eq!(
        report.defs, 14,
        "11 real defs + 2 pseudo-ctor defs in core.py + run() in main"
    );
    assert_eq!(
        report.references, 6,
        "2 emitted imports (alias P import dropped by the refs-side guard) + CONSTANT load + top_fn load (handler) + Wrapper base load + isinstance Plain load"
    );
    assert_eq!(report.call_sites, 9, "6 in core.py + 3 in main.py");
    // B7b pseudo-constructor mint: three constructor call sites hit
    // class-shaped targets (Plain(), Wrapper.Inner(), alias P()) and two
    // distinct classes get the one-shot DEF backfill.
    assert_eq!(report.minted_pseudo_ctor_refs, 3, "pseudo-ctor call refs");
    assert_eq!(
        report.minted_pseudo_ctor_defs, 2,
        "Plain(). + Wrapper#Inner(). defs"
    );
    assert!(
        report.minted_pseudo_ctor_refs >= report.minted_pseudo_ctor_defs,
        "refs >= defs (idempotence)"
    );

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
    // B7a guard (SM-10): a class WITH a corpus `__init__` must NOT also
    // get a pseudo-constructor mint — the site keeps its method grain.
    assert!(
        !symbols.contains(&format!("{disc}`pkg.core`/Greeter().")),
        "B7a site must not be pseudo-minted"
    );

    // B7b pseudo-constructor forms (SM-1/SM-12): plain class (no corpus
    // `__init__`) and nested class minted in fn shape, def + call ref.
    for pseudo in [
        format!("{disc}`pkg.core`/Plain()."),
        format!("{disc}`pkg.core`/Wrapper#Inner()."),
    ] {
        let occs: Vec<&scip::types::Occurrence> = loaded
            .index
            .documents
            .iter()
            .flat_map(|d| d.occurrences.iter())
            .filter(|o| o.symbol == pseudo)
            .collect();
        assert!(
            occs.iter().any(|o| o.symbol_roles & 1 != 0),
            "pseudo-ctor {pseudo} needs a DEF occurrence (backfill): {occs:#?}"
        );
        assert!(
            occs.iter().any(|o| o.symbol_roles & 1 == 0),
            "pseudo-ctor {pseudo} needs a call ref: {occs:#?}"
        );
    }

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
    // on physical line 26 of pkg/core.py → range[0] == 25.
    let top_fn_def = loaded
        .index
        .documents
        .iter()
        .filter(|d| d.relative_path == "pkg/core.py")
        .flat_map(|d| d.occurrences.iter())
        .find(|o| o.symbol.ends_with("`/top_fn().") && o.symbol_roles & 1 != 0)
        .expect("top_fn def occurrence");
    assert_eq!(top_fn_def.range.first(), Some(&25), "0-based def line");
    assert_eq!(
        top_fn_def.enclosing_range.first(),
        Some(&25),
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

    // CALLS-vs-REFERENCES split (occurrence EP S3-F2, build-side
    // derivation): 8 of the 9 resolved call sites become CALLS edges
    // (2 B7a constructors via the class-segment fallback, greet,
    // inner_helper, 2× top_fn, Plain() + Wrapper.Inner() via tail match)
    // while the alias site P() (syntactic mark "P" vs tail "Plain") and
    // the `handler = top_fn` load stay REFERENCES — exact counts, a kind
    // regression must fail.
    assert_eq!(
        built.calls_edges, 8,
        "CALLS edges (8 of the 9 fixture call sites)"
    );
    {
        let conn = rusqlite::Connection::open_with_flags(
            &built.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open graph.db");
        let calls: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges WHERE kind = 'CALLS'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let refs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE kind = 'REFERENCES'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let nested_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE kind = 'CALLS' AND callee_symbol LIKE '%inner_helper().'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // B7b: the pseudo-constructor node exists (def row) and its edge
        // is CALLS; the alias site is the recorded REFERENCES residual.
        let plain_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE kind = 'CALLS' AND callee_symbol LIKE '%/Plain().'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let plain_node: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE symbol LIKE '%/Plain().'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let inner_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE kind = 'CALLS' AND callee_symbol LIKE '%/Wrapper#Inner().'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(calls, 8, "graph CALLS edges");
        assert_eq!(
            refs, 2,
            "graph REFERENCES edges (handler load + alias P() residual)"
        );
        assert!(nested_calls >= 1, "nested-fn CALLS edge: {nested_calls}");
        assert_eq!(plain_calls, 1, "Plain(). pseudo-ctor CALLS edge");
        assert_eq!(plain_node, 1, "Plain(). node exists exactly once");
        assert_eq!(inner_calls, 1, "Wrapper#Inner(). pseudo-ctor CALLS edge");
    }

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
    assert_eq!(
        bytes_a, bytes_b,
        "two emits of the same repo must be byte-identical"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn emit_invalidates_stale_sidecar_artifacts() {
    // Silent bad-db relay (2026-08-28): a fresh pyrefly-index over a slot
    // still holding an lsp-harvest-era cache db + stamped meta must not
    // leave them behind — graph_db build's lsp fast-path would silently
    // trust the superseded producer (CALLS 0 / REFERENCES-only db).
    let repo = temp_repo_tagged("inval");
    let index_path = repo.join("index.scip");
    let cache_db = code_reality::cache::sqlite_path(&index_path);
    let meta = code_reality::engine::meta_path(&index_path);
    {
        let c = rusqlite::Connection::open(&cache_db).unwrap();
        c.execute_batch(code_reality::cache::SCHEMA_SQL).unwrap();
        c.execute(
            "INSERT INTO meta (key, value) VALUES ('producer', 'lsp-harvest-poc(pyright-langserver)')",
            [],
        )
        .unwrap();
    }
    std::fs::write(&meta, "{\"stale\": true}\n").unwrap();
    let fndefs_db = code_reality::fndefs::fndefs_path(&index_path);
    std::fs::write(&fndefs_db, "stale fn-defs sidecar\n").unwrap();

    let report = pyrefly_producer::emit(&repo, Some(&index_path)).expect("emit");
    assert_eq!(
        report.invalidated_sidecars.len(),
        3,
        "cache db + stamped meta + fn-defs sidecar removed: {:?}",
        report.invalidated_sidecars
    );
    assert!(!cache_db.exists(), "stale cache db gone");
    assert!(!meta.exists(), "stale stamped meta gone");
    assert!(!fndefs_db.exists(), "stale fn-defs sidecar gone");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn pyrefly_index_version_face_carries_rev() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pyrefly-index"))
        .arg("--version")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn pyrefly-index bin");
    assert!(out.status.success());
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        line == env!("CARGO_PKG_VERSION")
            || line.starts_with(concat!(env!("CARGO_PKG_VERSION"), "+")),
        "pkg or pkg+rev face (git-less builds fall back to pkg-only): {line}"
    );
    assert!(!line.contains(' '), "no spaces: {line}");
}

#[test]
fn pyrefly_lsp_version_face_carries_rev() {
    // pyrefly-lsp keeps its own face (engine rev rides along); it never
    // warns — it is a backend spawned by the bridge.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pyrefly-lsp"))
        .arg("--version")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn pyrefly-lsp bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(engine: pinned git rev 1d64c4b)"),
        "engine rev still reported: {stdout}"
    );
}

/// EP data-plane-unification S1: the default slot resolves in-repo, the
/// data dir self-ignores (single `*`, no negation), and a real git repo
/// stays porcelain-clean through the producer→cache→graph_db chain on
/// default paths.
#[test]
fn default_slot_chain_is_in_repo_and_git_clean() {
    let d = temp_repo_tagged("inrepo");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .expect("spawn git")
    };
    assert!(git(&["init", "-q"]).success());
    assert!(git(&["add", "-A"]).success());
    assert!(git(&["commit", "-qm", "init"]).success());

    let report = pyrefly_producer::emit(&d, None).expect("emit");
    let canon = d.canonicalize().unwrap();
    let slot = canon.join(".code-reality").join("scip").join("index.scip");
    assert_eq!(report.index_path, slot);
    let gi = canon.join(".code-reality").join(".gitignore");
    assert!(gi.is_file());
    let body = std::fs::read_to_string(&gi).unwrap();
    assert!(body.trim_end().ends_with('*') && !body.contains('!'));

    // reader side on default paths: cache + graph db consume the in-repo slot
    let loaded = code_reality::engine::load_index(&slot).expect("load_index");
    let db_path = code_reality::cache::sqlite_path(&slot);
    code_reality::cache::build_db(&loaded.index, &db_path, "e2ehead").expect("build_db");
    code_reality::graph_db::build_from_cache(&d).expect("graph_db build");
    assert!(canon.join(".code-reality").join("graph.db").is_file());

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&d)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "",
        "data dir must be fully ignored"
    );
    let _ = std::fs::remove_dir_all(&d);
}
