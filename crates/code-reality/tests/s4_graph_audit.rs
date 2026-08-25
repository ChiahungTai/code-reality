//! S4 graph_audit tests — D1 state machine (kernel triple-impl shapes),
//! RA symbol parsing, db_functions kind filter, the audit flow with an
//! injected ra_lookup, audit_targets double-key attribution on both
//! faces, and the CLI env/usage/audit-guard families.

mod crg_fixture;

use code_reality::cache::{audit_targets, build_db};
use code_reality::cli;
use code_reality::graph_audit::{
    audit, db_functions, parse_ra_symbols, risk_scan, run, OrderedCounter,
};
use scip::types::{Document, Index, Occurrence};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

// ---------- D1 state machine ----------

fn write_repo_file(repo: &Path, rel: &str, body: &str) -> PathBuf {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn risk_scan_kernel_triple_impl_per_block_counting() {
    let tmp = tempfile::tempdir().unwrap();
    let body = "\
impl KernelEventStore {
    fn open(&self) {}
    fn seal(&self) {}
}
impl Drop for KernelEventStore {
    fn drop(&mut self) {}
}
impl Marker for KernelEventStore {
    fn open(&self) {}
    fn other(&self) {}
}
";
    let f = write_repo_file(tmp.path(), "src/kernel.rs", body);
    let risk = risk_scan(std::slice::from_ref(&f));
    // per-block counting: `open` in 2 blocks → overlap; `other`/`seal`/`drop` once
    assert_eq!(risk.len(), 1);
    assert_eq!(risk[0].0, f);
    assert_eq!(risk[0].1, "KernelEventStore");
    assert_eq!(risk[0].2, vec!["open".to_string()]);
}

#[test]
fn risk_scan_indented_impl_inside_inline_mod() {
    let tmp = tempfile::tempdir().unwrap();
    let body = "\
mod inner {
    impl Thing {
        fn dup(&self) {}
    }
}
impl Thing {
    fn dup(&self) {}
}
";
    let f = write_repo_file(tmp.path(), "src/lib.rs", body);
    let risk = risk_scan(&[f]);
    assert_eq!(risk.len(), 1);
    assert_eq!(risk[0].1, "Thing");
    assert_eq!(risk[0].2, vec!["dup".to_string()]);
}

#[test]
fn risk_scan_unclosed_impl_conservative_bloat() {
    let tmp = tempfile::tempdir().unwrap();
    // the `}` at indent 2 > impl indent 0 does NOT close — later fns keep
    // attributing to the SAME block (conservative bloat, by design); the
    // overlap fires from the second impl of the same type
    let body = "\
impl Thing {
  fn dup(&self) {}
  }
  fn dup(&self) {}
impl Marker for Thing {
  fn dup(&self) {}
}
";
    let f = write_repo_file(tmp.path(), "src/unclosed.rs", body);
    let risk = risk_scan(&[f]);
    assert_eq!(risk.len(), 1);
    assert_eq!(risk[0].1, "Thing");
    assert_eq!(risk[0].2, vec!["dup".to_string()]);
}

#[test]
fn risk_scan_overlap_sorted_not_first_seen() {
    // first-seen order is [zebra, dup]; the overlap face is sorted()
    // (graph_audit.py:115) — multi-element, non-alpha order pins the sort
    let tmp = tempfile::tempdir().unwrap();
    let body = "impl T {
    pub fn zebra(&self) {}
    pub fn dup(&self) {}
}
impl U for T {
    fn zebra(&self) {}
    fn dup(&self) {}
}
";
    let f = write_repo_file(tmp.path(), "src/sorted.rs", body);
    let risk = risk_scan(&[f]);
    assert_eq!(risk.len(), 1);
    assert_eq!(risk[0].2, vec!["dup".to_string(), "zebra".to_string()]);
}

#[test]
fn fn_re_modifier_prefixes() {
    // pub(crate)/const/async/unsafe/extern "C" variants all capture
    let tmp = tempfile::tempdir().unwrap();
    let body = "\
impl T {
    pub fn a() {}
    pub(crate) fn b() {}
    const fn c() {}
    async unsafe fn d() {}
    extern \"C\" fn e() {}
    fn plain() {}
}
";
    let f = write_repo_file(tmp.path(), "src/m.rs", body);
    let risk = risk_scan(&[f]);
    assert!(risk.is_empty(), "{risk:?}"); // each name once → no overlap
}

// ---------- RA parsing ----------

#[test]
fn parse_ra_symbols_filters_kind_and_preserves_order() {
    let text = "\
symbol label: \"open\" kind: SymbolKind(Function) extra
symbol label: \"seal\" kind: SymbolKind(Method)
symbol label: \"Struct\" kind: SymbolKind(Struct)
symbol label: \"open\" kind: SymbolKind(Function)
symbol label: \"nope\" kind: SymbolKind(Field)
";
    let c = parse_ra_symbols(text);
    let items: Vec<(&str, usize)> = c.iter().collect();
    assert_eq!(items, vec![("open", 2), ("seal", 1)]);
    assert_eq!(c.total(), 3);
}

#[test]
fn ordered_counter_bump_semantics() {
    let mut c = OrderedCounter::default();
    c.bump("b");
    c.bump("a");
    c.bump("b");
    let items: Vec<(&str, usize)> = c.iter().collect();
    assert_eq!(items, vec![("b", 2), ("a", 1)]); // first-seen, not sorted
}

// ---------- db_functions ----------

#[test]
fn db_functions_kind_includes_test_and_resolves_path() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let src = write_repo_file(&repo, "src/a.rs", "fn x() {}\n");
    let mut spec = crg_fixture::CrgDbSpec::default();
    for (kind, name, qname) in [
        ("Function", "x", "src/a.rs::x"),
        ("Test", "t_x", "src/a.rs::t_x"),
        ("Class", "x", "src/a.rs::X.x"), // wrong kind: not counted
    ] {
        spec.nodes.push(crg_fixture::NodeSeed {
            name: name.into(),
            parent: None,
            qname: qname.into(),
            file_path: src.to_string_lossy().into_owned(),
        });
        spec.node_attrs.push((
            qname.to_string(),
            crg_fixture::NodeAttr { kind, language: "rust", is_test: 0, community_id: None },
        ));
    }
    // add a second Test with the same name for the count
    spec.nodes.push(crg_fixture::NodeSeed {
        name: "t_x".into(),
        parent: None,
        qname: "src/a.rs::t_x2".into(),
        file_path: src.to_string_lossy().into_owned(),
    });
    spec.node_attrs.push((
        "src/a.rs::t_x2".into(),
        crg_fixture::NodeAttr { kind: "Test", language: "rust", is_test: 1, community_id: None },
    ));
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let counts = db_functions(&conn, &src);
    assert_eq!(counts.get("x"), Some(&1));
    assert_eq!(counts.get("t_x"), Some(&2));
    assert_eq!(counts.get("X.x"), None); // Class kind filtered
}

// ---------- audit flow (injected lookup) ----------

fn lookup_from(map: HashMap<String, usize>) -> impl Fn(&Path) -> Result<Option<OrderedCounter>, String> {
    move |_p: &Path| {
        let mut c = OrderedCounter::default();
        for (k, v) in &map {
            for _ in 0..*v {
                c.bump(k);
            }
        }
        Ok(Some(c))
    }
}

#[test]
fn audit_assembles_missing_and_vacuous_warn() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let body = "\
impl T {
    fn open(&self) {}
}
impl U for T {
    fn open(&self) {}
}
";
    let src = write_repo_file(&repo, "src/k.rs", body);
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = crg_fixture::CrgDbSpec::default();
    spec.nodes.push(crg_fixture::NodeSeed {
        name: "open".into(),
        parent: None,
        qname: "src/k.rs::T.open".into(),
        file_path: src.to_string_lossy().into_owned(),
    });
    spec.node_attrs.push((
        "src/k.rs::T.open".into(),
        crg_fixture::NodeAttr { kind: "Function", language: "rust", is_test: 0, community_id: None },
    ));
    crg_fixture::make_crg_db(&db, &spec).unwrap();

    let lookup = lookup_from(HashMap::from([("open".to_string(), 2usize)]));
    let (risk, audited, missing, errors, total_ra, warns) =
        audit(&repo, &db, false, Some(&lookup)).unwrap();
    assert_eq!(risk.len(), 1);
    assert_eq!(audited, 1);
    assert_eq!(total_ra, 2);
    assert!(errors.is_empty());
    assert!(warns.is_empty()); // non-empty ra output → no vacuous warn
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].symbol, "open");
    assert_eq!(missing[0].ra_count, 2);
    assert_eq!(missing[0].db_count, 1); // single node ingested

    // zero-output lookup on a non-empty file → vacuous warn + total 0
    let zero = lookup_from(HashMap::new());
    let (_, audited2, missing2, _, total2, warns2) =
        audit(&repo, &db, false, Some(&zero)).unwrap();
    assert_eq!(audited2, 1);
    assert_eq!(total2, 0);
    assert!(missing2.is_empty());
    assert_eq!(warns2.len(), 1);
    assert!(warns2[0].contains("零輸出"), "{warns2:?}");

    // timeout/failure lookup → error line, skipped
    let fail = |_p: &Path| Ok(None);
    let (_, _, _, errors3, _, _) = audit(&repo, &db, false, Some(&fail)).unwrap();
    assert_eq!(errors3.len(), 1);
    assert!(errors3[0].contains("逾時/失敗（跳過）"), "{errors3:?}");
}

#[test]
fn audit_default_scope_is_risk_files_all_flag_expands() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let risky = "\
impl T {
    fn dup(&self) {}
}
impl U for T {
    fn dup(&self) {}
}
";
    let _ = write_repo_file(&repo, "src/risky.rs", risky);
    let _ = write_repo_file(&repo, "src/clean.rs", "fn alone() {}\n");
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    crg_fixture::make_crg_db(&db, &crg_fixture::CrgDbSpec::default()).unwrap();
    let lookup = lookup_from(HashMap::new());
    let (_, audited, _, _, _, _) = audit(&repo, &db, false, Some(&lookup)).unwrap();
    assert_eq!(audited, 1); // risk-file scope only
    let (_, audited_all, _, _, _, _) = audit(&repo, &db, true, Some(&lookup)).unwrap();
    assert_eq!(audited_all, 2);
}

// ---------- audit_targets double-key (both faces) ----------

fn occ(symbol: &str, line: i32, is_def: bool) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.range = vec![line, 1, line, 2];
    o.symbol_roles = if is_def { 1 } else { 0 };
    o
}

fn doc(rel: &str, occurrences: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    d.relative_path = rel.to_string();
    d.occurrences = occurrences;
    d
}

fn audit_fixture_index() -> Index {
    let mut index = Index::new();
    index.documents = vec![
        doc(
            "src/a.rs",
            vec![
                occ("kernel#KernelEventStore#open().", 10, true),
                occ("kernel#KernelEventStore#open().", 44, false),
                occ("kernel#KernelEventStore#seal().", 80, true),
            ],
        ),
        doc(
            "src/b.rs",
            vec![
                // same method name, different file: double-key filters it
                occ("other#Thing#open().", 12, true),
                occ("kernel#KernelEventStore#open().", 30, false),
            ],
        ),
    ];
    index
}

#[test]
fn audit_targets_double_key_file_name_attribution() {
    let index = audit_fixture_index();
    let mut files_by_name: HashMap<String, BTreeSet<String>> = HashMap::new();
    files_by_name
        .entry("open".to_string())
        .or_default()
        .insert("src/a.rs".to_string());
    let out = audit_targets(&index, &files_by_name);
    // only the DEF on src/a.rs with method `open` matches; the src/b.rs
    // DEF with the same name is a different (file, name) key
    assert_eq!(out.len(), 1);
    let (file, name) = out.values().next().unwrap();
    assert_eq!((file.as_str(), name.as_str()), ("src/a.rs", "open"));
    let sym = out.keys().next().unwrap();
    assert_eq!(sym, "kernel#KernelEventStore#open().");
}

#[test]
fn audit_targets_sqlite_face_matches_protobuf() {
    let tmp = tempfile::tempdir().unwrap();
    let index = audit_fixture_index();
    let db = tmp.path().join("derived.db");
    build_db(&index, &db, "headsha").unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut files_by_name: HashMap<String, BTreeSet<String>> = HashMap::new();
    files_by_name
        .entry("open".to_string())
        .or_default()
        .insert("src/b.rs".to_string());
    let out = code_reality::cache::Face::Sqlite(conn)
        .audit_targets(&files_by_name)
        .unwrap();
    assert_eq!(out.len(), 1);
    let (file, name) = out.values().next().unwrap();
    assert_eq!((file.as_str(), name.as_str()), ("src/b.rs", "open"));
    // protobuf face agrees
    let proto = code_reality::cache::Face::Protobuf { index };
    let out2 = proto.audit_targets(&files_by_name).unwrap();
    assert_eq!(out, out2);
}

// ---------- CLI faces ----------

fn run_cli(args: &[&str]) -> code_reality::ToolOutput {
    run(args)
}

fn run_scip(args: &[&str]) -> code_reality::ToolOutput {
    cli::run(args)
}

#[test]
fn cli_help_and_usage_family() {
    let out = run_cli(&["graph_audit", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("usage: graph_audit [-h] --repo REPO [--all] [--json] [--graph GRAPH]\n"));
    assert!(out.stdout.contains("  --graph GRAPH  覆寫 graph.db 路徑（預設 <repo>/.code-review-graph/graph.db）\n"));
    let out = run_cli(&["graph_audit"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    let out = run_cli(&["graph_audit", "--repo", "/x", "--nope"]);
    assert_eq!(out.exit_code, 2);
}

#[test]
fn cli_env_gate_missing_db_exit_2() {
    // whichever env gate fires (RA presence is machine-dependent), the
    // exit family is 2 with empty stdout
    let out = run_cli(&[
        "graph_audit",
        "--repo",
        "/tmp",
        "--graph",
        "/tmp/definitely-missing-graph.db",
    ]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("[FAIL]"), "{}", out.stderr);
}

#[test]
fn scip_audit_guard_family_before_index_resolution() {
    // guards fire before any index work — no index needed
    let out = run_scip(&["scip_refs", "--audit", "SomeQuery"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--audit 與查詢字串互斥"), "{}", out.stderr);
    let out = run_scip(&["scip_refs", "--audit"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--audit 需 --repo（graph_audit 目標）"), "{}", out.stderr);
    let out = run_scip(&["scip_refs", "--build-cache", "--audit", "--repo", "/x"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--build-cache 與 --stamp-meta/--audit/查詢互斥"), "{}", out.stderr);
    let out = run_scip(&["scip_refs", "--stamp-meta", "--audit", "--repo", "/x"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--stamp-meta 與 --audit/查詢互斥"), "{}", out.stderr);
    // abbreviation still resolves through the new flag
    let out = run_scip(&["scip_refs", "--a", "q"]);
    assert!(out.stderr.contains("--audit 與查詢字串互斥"), "{}", out.stderr);
}
