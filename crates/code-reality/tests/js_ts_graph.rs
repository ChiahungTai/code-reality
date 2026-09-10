//! JS/TS blueprint S3 graph-semantics tests: language labels from the
//! defining document, queryable `Type` nodes for `#` symbols, syntax
//! CALLS vs REFERENCES, shared test-path classification, and the
//! containment-coverage denominators. Fixtures are hand-built SCIP
//! indexes with REAL scip-typescript symbol shapes; the on-disk sources
//! carry real syntax so the tree-sitter collector derives actual marks.

mod support;

use code_reality::engine::{find_defs, is_test_path, queryable_symbol, Query};
use protobuf::Message;
use scip::types::{Document, Index};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use support::{mkrepo, occ};

const LIB_TS: &str = "\
export class Greeter {
  greet() { return 'hi'; }
}
export function tsHelper(): string {
  return 'ts';
}
";
const APP_MJS: &str = "\
import { tsHelper, Greeter } from './lib.ts';
export function main() {
  tsHelper();
  const g = new Greeter();
  const alias = tsHelper; tsHelper();
  return g.greet();
}
";

fn symbol(doc: &str, tail: &str) -> String {
    format!("scip-typescript npm . . /r/src/`{doc}`/{tail}")
}

/// lib.ts defs: Greeter# (class), greet (method), tsHelper (fn) — all
/// with enclosing_range (0-based lines).
fn lib_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "src/lib.ts".to_string();
    d.occurrences = vec![
        occ(
            &symbol("lib.ts", "Greeter#"),
            1,
            vec![0, 0, 6],
            Some(vec![0, 0, 2, 1]),
        ),
        occ(
            &symbol("lib.ts", "Greeter#greet()."),
            1,
            vec![1, 2, 7],
            Some(vec![1, 2, 1, 21]),
        ),
        occ(
            &symbol("lib.ts", "tsHelper()."),
            1,
            vec![3, 0, 8],
            Some(vec![3, 0, 5, 1]),
        ),
    ];
    d
}

/// app.mjs: def main (lines 1-7 0-based), refs inside it — a call at
/// line 3 (col 2), a constructor at line 4 (col 16), and the codex-P1-5
/// identity case at line 5: a plain load (col 16) AND a real call
/// (col 26) of the same name on ONE line. SCIP occurrence ranges carry
/// the matching start columns.
fn app_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "src/app.mjs".to_string();
    d.occurrences = vec![occ(
        &symbol("app.mjs", "main()."),
        1,
        vec![1, 0, 4],
        Some(vec![1, 0, 7, 1]),
    )];
    // REF rows: (defining symbol, line, col) — cols match the source
    for (sym_tail, range) in [
        ("tsHelper().", vec![2, 2, 10]),  // tsHelper();        ← call
        ("Greeter#", vec![3, 16, 23]),    // new Greeter()      ← constructor
        ("tsHelper().", vec![4, 16, 24]), // const alias = tsHelper; ← plain load
        ("tsHelper().", vec![4, 26, 34]), // tsHelper(); (same line) ← call
    ] {
        d.occurrences
            .push(occ(&symbol("lib.ts", sym_tail), 0, range, None));
    }
    d
}

fn test_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "__tests__/helper.test.ts".to_string();
    d.occurrences = vec![occ(
        &symbol("__tests__/helper.test.ts", "probeFn()."),
        1,
        vec![0, 0, 8],
        Some(vec![0, 0, 1, 1]),
    )];
    d
}

fn write_repo(t: &tempfile::TempDir) -> PathBuf {
    mkrepo(
        t,
        &[
            ("src/lib.ts", LIB_TS),
            ("src/app.mjs", APP_MJS),
            ("__tests__/helper.test.ts", "export function probeFn() {}\n"),
        ],
    )
}

fn build_graph(t: &tempfile::TempDir) -> (PathBuf, code_reality::graph_db::BuildReport) {
    let repo = write_repo(t);
    let mut index = Index::new();
    index.documents = vec![lib_doc(), app_doc(), test_doc()];
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot = slot_dir.join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let rep = code_reality::graph_db::build_from_cache_at(&repo, &slot).unwrap();
    (repo, rep)
}

fn query_rows(repo: &Path, sql: &str) -> Vec<(String, String)> {
    let conn = code_reality::common::connect_ro(&repo.join(".code-reality/graph.db")).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows
}

fn edge_kinds(repo: &Path, caller: &str, callee: &str) -> Vec<String> {
    query_rows(
        repo,
        &format!(
            "SELECT kind, CAST(line AS TEXT) FROM edges \
             WHERE caller_symbol LIKE '%/{caller}%' AND callee_symbol LIKE '%/{callee}%'"
        ),
    )
    .into_iter()
    .map(|(k, l)| format!("{k}@{l}"))
    .collect()
}

#[test]
fn s3_language_labels_and_node_kinds() {
    let t = tempfile::tempdir().unwrap();
    let (repo, rep) = build_graph(&t);
    assert!(rep.nodes >= 4, "nodes={}", rep.nodes);

    let langs = query_rows(&repo, "SELECT symbol, language FROM nodes ORDER BY symbol");
    // keyed by full symbol (name alone could collide across languages —
    // a name-keyed map would mask a mislabel)
    let by_symbol: std::collections::BTreeMap<String, String> = langs.into_iter().collect();
    let lang_of = |tail: &str| {
        by_symbol
            .iter()
            .find(|(s, _)| s.contains(tail))
            .map(|(_, l)| l.clone())
            .unwrap_or_default()
    };
    assert_eq!(lang_of("main()."), "JavaScript");
    assert_eq!(lang_of("tsHelper()."), "TypeScript");
    assert_eq!(lang_of("greet()."), "TypeScript");
    assert_eq!(lang_of("Greeter#"), "TypeScript");
    assert!(
        !by_symbol.values().any(|v| v == "Rust"),
        "no JS/TS node may report Rust: {by_symbol:?}"
    );

    let kinds = query_rows(&repo, "SELECT name, kind FROM nodes ORDER BY name");
    let kind_by_name: std::collections::BTreeMap<String, String> = kinds.into_iter().collect();
    assert_eq!(
        kind_by_name.get("Greeter").map(String::as_str),
        Some("Type")
    );
    assert_eq!(
        kind_by_name.get("main").map(String::as_str),
        Some("Function")
    );
    assert_eq!(
        kind_by_name.get("tsHelper").map(String::as_str),
        Some("Function")
    );
}

#[test]
fn s3_calls_vs_references_edges() {
    let t = tempfile::tempdir().unwrap();
    let (repo, rep) = build_graph(&t);
    assert!(
        rep.calls_edges >= 3,
        "expected direct + constructor + same-line call CALLS, got {} (edges={})",
        rep.calls_edges,
        rep.edges
    );
    // EXACT site identity (codex P1-5/P1-8): tsHelper edges from main are
    // CALLS@3 (the call), REFERENCES@5 + CALLS@5 (same-line plain load
    // vs call — column grain must split them, line+name would pollute)
    let ts_edges = edge_kinds(&repo, "main().", "tsHelper().");
    let mut sorted_ts = ts_edges.clone();
    sorted_ts.sort();
    assert_eq!(
        sorted_ts,
        vec![
            "CALLS@3".to_string(),
            "CALLS@5".to_string(),
            "REFERENCES@5".to_string(),
        ],
        "tsHelper sites: {ts_edges:?}"
    );
    // new Greeter() constructor: CALLS to the class symbol via class segment
    let gr_edges = edge_kinds(&repo, "main().", "Greeter#");
    assert_eq!(gr_edges, vec!["CALLS@4".to_string()], "{gr_edges:?}");
}

#[test]
fn s3_test_file_classification_shared() {
    let t = tempfile::tempdir().unwrap();
    let (repo, _rep) = build_graph(&t);
    let is_test = query_rows(
        &repo,
        "SELECT name, CAST(is_test AS TEXT) FROM nodes WHERE name = 'probeFn'",
    );
    assert_eq!(
        is_test.first().map(|(_, v)| v.as_str()),
        Some("1"),
        "__tests__/*.test.ts node must be is_test"
    );
    // shared helper agrees on the JS shapes (SM-13)
    assert!(is_test_path("__tests__/a.ts"));
    assert!(is_test_path("pkg/b.spec.tsx"));
    assert!(is_test_path("pkg/c.test.mjs"));
    assert!(
        !is_test_path("pkg/c.test.go"),
        "suffix logic stays JS/TS-scoped"
    );
    assert!(is_test_path("tests/x.rs"), "rust tests/ dir preserved");
    assert!(
        is_test_path("test_util.py"),
        "python test_ prefix preserved"
    );
    assert!(!is_test_path("src/lib.ts"));
}

#[test]
fn s3_queryable_policy_and_bare_class_query() {
    // unit: queryable_symbol document-awareness
    let js_class = "scip-typescript npm . . /r/src/`a.js`/Named#";
    let ts_class = "scip-typescript npm . . /r/src/`a.ts`/Named#";
    let py_class = "pyrefly python p 0.1 `m`/Named#";
    let rs_class = "rust-analyzer cargo x 0.1 k/Named#";
    assert_eq!(
        queryable_symbol(js_class, "src/a.js").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    assert_eq!(
        queryable_symbol(ts_class, "src/a.ts").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    assert_eq!(
        queryable_symbol(py_class, "m/a.py").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    assert!(queryable_symbol(rs_class, "k/named.rs").is_none());
    // same rust-shaped symbol on a JS document IS queryable (doc authority)
    assert!(queryable_symbol(rs_class, "src/named.js").is_some());

    // integration: bare class query resolves through the protobuf face
    let mut index = Index::new();
    index.documents = vec![lib_doc()];
    let defs = find_defs(&index, &Query::parse("Greeter"));
    assert!(
        defs.keys().any(|s| s.ends_with("Greeter#")),
        "bare class query must resolve: {defs:?}"
    );

    // and through the derived cache (sqlite face)
    let tmp = tempfile::tempdir().unwrap();
    let slot = tmp.path().join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let db = tmp.path().join("index.scip.db");
    code_reality::cache::build_db(&index, &db, "").unwrap();
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let defs2 = code_reality::cache::sqlite_defs(&conn, &Query::parse("Greeter")).unwrap();
    assert!(
        defs2.keys().any(|s| s.ends_with("Greeter#")),
        "sqlite face bare class query: {defs2:?}"
    );
}

#[test]
fn s3_coverage_denominators_recorded() {
    let t = tempfile::tempdir().unwrap();
    let (_repo, rep) = build_graph(&t);
    // 4 queryable fn defs (greet, tsHelper, main, probeFn), all spanned
    assert_eq!(rep.fn_def_total, 4, "total={}", rep.fn_def_total);
    assert_eq!(rep.fn_def_spanned, 4, "spanned={}", rep.fn_def_spanned);
    // an unspanned fn def drops only the numerator (SM-12)
    let t2 = tempfile::tempdir().unwrap();
    let repo = write_repo(&t2);
    let mut index = Index::new();
    let mut nospan = app_doc();
    nospan.occurrences[0].enclosing_range = vec![];
    index.documents = vec![lib_doc(), nospan, test_doc()];
    let slot = repo.join(".code-reality/scip/index.scip");
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let rep2 = code_reality::graph_db::build_from_cache_at(&repo, &slot).unwrap();
    assert_eq!(rep2.fn_def_total, 4);
    assert_eq!(rep2.fn_def_spanned, 3);
}

#[test]
fn s3_rust_regression_types_stay_filtered_and_untestable() {
    let rs_type = "rust-analyzer cargo code-reality 0.6.3 build/BuildError#";
    assert!(queryable_symbol(rs_type, "build.rs").is_none());
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "build.rs".to_string();
    d.occurrences = vec![occ(rs_type, 1, vec![0, 0, 5], Some(vec![0, 0, 3, 1]))];
    index.documents = vec![d];
    assert!(find_defs(&index, &Query::parse("BuildError")).is_empty());
    let mut set = BTreeSet::new();
    set.insert(rs_type.to_string());
    let _ = set;
}
