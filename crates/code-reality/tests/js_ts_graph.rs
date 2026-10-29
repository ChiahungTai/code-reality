//! JS/TS blueprint S3 graph-semantics tests: language labels from the
//! defining document, queryable `Type` nodes for `#` symbols, syntax
//! CALLS vs REFERENCES, shared test-path classification, and the
//! containment-coverage denominators. Fixtures are hand-built SCIP
//! indexes with REAL scip-typescript symbol shapes; the on-disk sources
//! carry real syntax so the tree-sitter collector derives actual marks.

mod support;

use code_reality::engine::{find_defs, ingest_symbol, is_test_path, Query};
use protobuf::Message;
use scip::types::{Document, Index};
use std::path::{Path, PathBuf};
use support::{doc, mkrepo, occ, occ_at_token};

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
const TEST_TS: &str = "export function probeFn() {}\n";

fn symbol(doc: &str, tail: &str) -> String {
    format!("scip-typescript npm . . /r/src/`{doc}`/{tail}")
}

/// lib.ts defs: Greeter# (class), greet (method), tsHelper (fn) — all
/// with enclosing_range (0-based lines).
fn lib_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "src/lib.ts".to_string();
    d.occurrences = vec![
        occ_at_token(
            &symbol("lib.ts", "Greeter#"),
            1,
            LIB_TS,
            "Greeter",
            0,
            Some(vec![0, 0, 2, 1]),
        ),
        occ_at_token(
            &symbol("lib.ts", "Greeter#greet()."),
            1,
            LIB_TS,
            "greet",
            0,
            Some(vec![1, 2, 1, 21]),
        ),
        occ_at_token(
            &symbol("lib.ts", "tsHelper()."),
            1,
            LIB_TS,
            "tsHelper",
            0,
            Some(vec![3, 0, 5, 1]),
        ),
    ];
    d
}

/// app.mjs: occurrence columns come from APP_MJS itself. `tsHelper`
/// appears first in the import, then direct call, alias load, same-line
/// call; `Greeter` appears first in the import, then in `new Greeter()`.
fn app_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "src/app.mjs".to_string();
    d.occurrences = vec![occ_at_token(
        &symbol("app.mjs", "main()."),
        1,
        APP_MJS,
        "main",
        0,
        Some(vec![1, 0, 7, 1]),
    )];
    for (sym_tail, token, nth) in [
        ("tsHelper().", "tsHelper", 1), // direct call
        ("Greeter#", "Greeter", 1),     // constructor
        ("tsHelper().", "tsHelper", 2), // plain alias load
        ("tsHelper().", "tsHelper", 3), // same-line call
    ] {
        d.occurrences.push(occ_at_token(
            &symbol("lib.ts", sym_tail),
            0,
            APP_MJS,
            token,
            nth,
            None,
        ));
    }
    d
}

fn test_doc() -> Document {
    let mut d = Document::new();
    d.relative_path = "__tests__/helper.test.ts".to_string();
    d.occurrences = vec![occ_at_token(
        &symbol("__tests__/helper.test.ts", "probeFn()."),
        1,
        TEST_TS,
        "probeFn",
        0,
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
            ("__tests__/helper.test.ts", TEST_TS),
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
fn s3_calls_match_on_non_ascii_utf16_lines() {
    // End-to-end UTF-16 alignment: the fixture occurrence columns come
    // from occ_at_token (UTF-16, the real scip-typescript contract) and
    // the production marks mint UTF-16 converted from tree-sitter bytes
    // — a call after a non-ASCII prefix MUST still land CALLS. Before
    // the conversion, byte-column marks mis-aligned on such lines
    // (`uni` sits at UTF-16 col 14 but byte col 20 after `ラベル`) and
    // silently degraded the call to REFERENCES.
    const UNI_TS: &str = "export function uni(): string {\n  return '元';\n}\n";
    const UNI_APP: &str =
        "export function useUni() {\n  const ラベル = uni();\n  const alias = uni;\n}\n";
    let t = tempfile::tempdir().unwrap();
    let repo = support::mkrepo(&t, &[("src/uni.ts", UNI_TS), ("src/uni_app.mjs", UNI_APP)]);
    let mut index = Index::new();
    let def_doc = doc(
        "src/uni.ts",
        vec![occ_at_token(
            &symbol("uni.ts", "uni()."),
            1,
            UNI_TS,
            "uni",
            0,
            Some(vec![0, 0, 2, 1]),
        )],
    );
    let mut app_doc = doc(
        "src/uni_app.mjs",
        vec![occ_at_token(
            &symbol("uni_app.mjs", "useUni()."),
            1,
            UNI_APP,
            "useUni",
            0,
            Some(vec![1, 0, 3, 1]),
        )],
    );
    // refs inside useUni: the CALL after the katakana prefix (occurrence
    // #0 of `uni`) and the plain load (#1)
    for nth in [0, 1] {
        app_doc.occurrences.push(occ_at_token(
            &symbol("uni.ts", "uni()."),
            0,
            UNI_APP,
            "uni",
            nth,
            None,
        ));
    }
    index.documents = vec![def_doc, app_doc];
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot = slot_dir.join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let rep = code_reality::graph_db::build_from_cache_at(&repo, &slot).unwrap();
    let call_edges = edge_kinds(&repo, "useUni().", "uni().");
    assert!(
        call_edges.iter().any(|k| k.starts_with("CALLS")),
        "call after a non-ASCII (UTF-16) prefix must stay CALLS: {call_edges:?} (report: calls={})",
        rep.calls_edges
    );
    assert!(
        call_edges.iter().any(|k| k.starts_with("REFERENCES")),
        "plain load stays REFERENCES: {call_edges:?}"
    );
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
fn s3_ingest_policy_and_bare_class_query() {
    // unit: ingest_symbol document-awareness (the MATERIALIZATION axis;
    // bare-name queryability is matches_query's axis and stays frozen)
    let js_class = "scip-typescript npm . . /r/src/`a.js`/Named#";
    let ts_class = "scip-typescript npm . . /r/src/`a.ts`/Named#";
    let py_class = "pyrefly python p 0.1 `m`/Named#";
    let rs_class = "rust-analyzer cargo x 0.1 k/Named#";
    assert_eq!(
        ingest_symbol(js_class, "src/a.js").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    assert_eq!(
        ingest_symbol(ts_class, "src/a.ts").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    assert_eq!(
        ingest_symbol(py_class, "m/a.py").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    // CR-21: rust `Type#` DEFs on Rust documents now ingest (document
    // authority — `LanguageFace::from_path == Rust`, not a symbol prefix)
    assert_eq!(
        ingest_symbol(rs_class, "k/named.rs").map(|q| q.name.to_string()),
        Some("Named".to_string())
    );
    // same rust-shaped symbol on a JS document still ingests (doc authority)
    assert!(ingest_symbol(rs_class, "src/named.js").is_some());

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

/// CR-21 ingest axis (split from the former
/// `s3_rust_regression_types_stay_filtered_and_untestable`): a rust
/// `Type#` DEF materializes as a kind=Type node, exact path+name
/// addressable — the chain_tour line-less fill contract
/// (`same_file_rows`: `file_path = ? AND name = ?`) resolves it.
#[test]
fn s3_rust_types_materialize_for_exact_anchoring() {
    let rs_type = "rust-analyzer cargo cr-fix 0.1.0 src/lib.rs/MarketGate#";
    assert!(
        ingest_symbol(rs_type, "src/lib.rs").is_some(),
        "rust Type DEF must pass the ingest gate"
    );
    const RS_SRC: &str = "pub struct MarketGate;\n";
    let t = tempfile::tempdir().unwrap();
    let repo = support::mkrepo(&t, &[("src/lib.rs", RS_SRC)]);
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "src/lib.rs".to_string();
    d.occurrences = vec![occ(rs_type, 1, vec![0, 11, 21], Some(vec![0, 0, 1, 1]))];
    index.documents = vec![d];
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot = slot_dir.join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let rep = code_reality::graph_db::build_from_cache_at(&repo, &slot).unwrap();
    assert_eq!(rep.nodes, 1, "exactly the Type node: nodes={}", rep.nodes);

    let kinds = query_rows(
        &repo,
        "SELECT kind, CAST(line_start AS TEXT) FROM nodes WHERE name = 'MarketGate'",
    );
    assert_eq!(
        kinds,
        vec![("Type".to_string(), "1".to_string())],
        "rust Type DEF lands as kind=Type"
    );

    // exact path+name anchoring — the production fill face
    let db = repo.join(".code-reality/graph.db");
    let locator = code_reality::symbol_resolve::SymbolLocator::open(&db, &repo).unwrap();
    match locator
        .same_file_fill("src/lib.rs", "MarketGate", "MarketGate")
        .unwrap()
    {
        code_reality::symbol_resolve::FillOutcome::Filled(line, sym) => {
            assert_eq!(line, 1);
            assert_eq!(sym.as_deref(), Some(rs_type));
        }
        code_reality::symbol_resolve::FillOutcome::NoAnchor(reason, _) => {
            panic!("exact path+name fill must resolve the Type node, got NoAnchor({reason})")
        }
    }
}

/// CR-21 keeps the AIR-33 query axis byte-frozen (the other half of the
/// former `s3_rust_regression_types_stay_filtered_and_untestable`): a
/// rust `Type#` DEF is materialized but NOT bare-name queryable — both
/// faces answer empty. The sqlite face now actually exercises the
/// predicate filter: the ingested Type reaches `symbol_tails` (the SQL
/// pre-narrow returns it) and `matches_query` must still deny it.
#[test]
fn s3_rust_bare_name_query_stays_denied() {
    let rs_type = "rust-analyzer cargo code-reality 0.6.3 build/BuildError#";
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "build.rs".to_string();
    d.occurrences = vec![occ(rs_type, 1, vec![0, 0, 5], Some(vec![0, 0, 3, 1]))];
    index.documents = vec![d];
    assert!(find_defs(&index, &Query::parse("BuildError")).is_empty());

    // and through the derived cache (sqlite face)
    let tmp = tempfile::tempdir().unwrap();
    let slot = tmp.path().join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let db = tmp.path().join("index.scip.db");
    let stats = code_reality::cache::build_db(&index, &db, "").unwrap();
    assert_eq!(stats.symbols, 1, "the Type symbol ingests");
    let conn = code_reality::common::connect_ro(&db).unwrap();
    assert!(
        code_reality::cache::sqlite_defs(&conn, &Query::parse("BuildError"))
            .unwrap()
            .is_empty(),
        "bare-name query stays denied on the sqlite face (AIR-33)"
    );
}

/// CR-21 same-name same-file: a rust Type DEF and a rust fn DEF sharing
/// the bare name ingest as TWO nodes (kinds differ); the line-less fill
/// decision core sees two candidates and stays fail-closed ambiguous —
/// behavior over pre-existing materialization sets is unchanged.
#[test]
fn s3_rust_same_name_type_and_function_ingest_both_fill_stays_closed() {
    let rs_type = "rust-analyzer cargo cr-fix 0.1.0 src/lib.rs/Widget#";
    let rs_fn = "rust-analyzer cargo cr-fix 0.1.0 src/lib.rs/Widget().";
    const RS_SRC: &str = "\
pub struct Widget;

impl Widget {
    pub fn widget() -> Widget {
        Widget
    }
}
";
    let t = tempfile::tempdir().unwrap();
    let repo = support::mkrepo(&t, &[("src/lib.rs", RS_SRC)]);
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "src/lib.rs".to_string();
    d.occurrences = vec![
        occ(rs_type, 1, vec![0, 11, 17], Some(vec![0, 0, 1, 1])),
        occ(rs_fn, 1, vec![3, 15, 21], Some(vec![3, 4, 6, 5])),
        // one Type ref — ingested-symbol refs ride the rows like python
        occ(rs_type, 0, vec![5, 8, 14], None),
    ];
    index.documents = vec![d];
    let slot_dir = repo.join(".code-reality/scip");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot = slot_dir.join("index.scip");
    std::fs::write(&slot, index.write_to_bytes().unwrap()).unwrap();
    let rep = code_reality::graph_db::build_from_cache_at(&repo, &slot).unwrap();
    assert_eq!(rep.nodes, 2, "both same-name defs materialize");

    let mut kinds = query_rows(
        &repo,
        "SELECT kind, CAST(line_start AS TEXT) FROM nodes \
         WHERE name = 'Widget' AND file_path LIKE '%/src/lib.rs' ORDER BY kind",
    );
    kinds.sort();
    assert_eq!(
        kinds,
        vec![
            ("Function".to_string(), "4".to_string()),
            ("Type".to_string(), "1".to_string()),
        ],
        "kind=Type and kind=Function coexist for one bare name in one file"
    );

    // the fill decision core is unchanged: two same-name candidates with
    // different lines → no R1 pair (lines differ), no unique R2 →
    // ambiguous with both candidates disclosed (fail-closed)
    let db = repo.join(".code-reality/graph.db");
    let locator = code_reality::symbol_resolve::SymbolLocator::open(&db, &repo).unwrap();
    match locator
        .same_file_fill("src/lib.rs", "Widget", "Widget")
        .unwrap()
    {
        code_reality::symbol_resolve::FillOutcome::NoAnchor(reason, candidates) => {
            assert_eq!(reason, "ambiguous");
            assert_eq!(candidates.len(), 2, "both candidates disclosed");
            let mut lines: Vec<i64> = candidates.iter().map(|c| c.line).collect();
            lines.sort();
            assert_eq!(lines, vec![1, 4]);
        }
        code_reality::symbol_resolve::FillOutcome::Filled(line, _) => {
            panic!("two same-name candidates must stay ambiguous, got Filled({line})")
        }
    }
}
