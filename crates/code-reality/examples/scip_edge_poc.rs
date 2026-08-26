//! POC (throwaway — lives only until the v1+ B1/B2 research EP absorbs its
//! numbers): quantify the SCIP-vs-CRG edge gap on the NT corpus.
//!
//! SCIP-side face: every cached reference occurrence (is_def = 0) attributed
//! through the REAL lib logic (`callers::attribute` + `fndefs::spans_source`)
//! — the edge semantics `scip_refs --callers` serves today and the same
//! occurrence+containment derivation the scip-callgraph reference impl uses
//! (verified: neither this corpus nor scip-callgraph carries call-kind
//! relationships — the index is old-schema, relationships live on
//! SymbolInformation without is_call_reference).
//!
//! Outputs land in POC_OUT (default .agent-tmp/poc-scip-injection):
//! a1_edges.tsv (caller \t callee \t sites) and a1_sites.tsv
//! (callee \t rel \t line) for the CRG call-site join.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

type Row = (String, String, i64);

fn main() {
    let index_path = PathBuf::from(format!(
        "{}/.mosaic/code-reality/scip/nautilus_trader/index.scip",
        std::env::var("HOME").expect("HOME")
    ));
    let out_dir =
        std::env::var("POC_OUT").unwrap_or_else(|_| ".agent-tmp/poc-scip-injection".into());
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    // Spans via the real ladder: sidecar miss → protobuf in-memory (never
    // builds the sidecar on miss — family rule), exactly like a live call.
    let (spans, stderr) = code_reality::fndefs::spans_source(&index_path, None).expect("spans");
    eprint!("{}", stderr.join(""));

    // Reference occurrences (is_def = 0 mirrors cache::sqlite_refs_rows
    // semantics; ORDER BY seq mirrors insertion scan order).
    let db = index_path.with_file_name("index.scip.db");
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open cache db");
    let mut stmt = conn
        .prepare("SELECT symbol, rel_path, line FROM occurrences WHERE is_def = 0 ORDER BY seq")
        .expect("prepare scan");
    let mut by_callee: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    let mut sites_w = File::create(format!("{out_dir}/a1_sites.tsv")).expect("sites out");
    let mut it = stmt.query([]).expect("query");
    while let Ok(Some(r)) = it.next() {
        let sym: String = r.get(0).expect("symbol");
        let rel: String = r.get(1).expect("rel_path");
        let line: i64 = r.get(2).expect("line");
        by_callee
            .entry(sym.clone())
            .or_default()
            .push((String::new(), rel.clone(), line));
        writeln!(sites_w, "{sym}\t{rel}\t{line}").expect("sites write");
    }
    drop(sites_w);

    let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut item_level: usize = 0;
    for (callee, rows) in &by_callee {
        let res = code_reality::callers::attribute(rows, &spans);
        item_level += res.item_level.len();
        for c in &res.callers {
            *edges.entry((c.symbol.clone(), callee.clone())).or_insert(0) += c.sites.len();
        }
    }
    let mut w = File::create(format!("{out_dir}/a1_edges.tsv")).expect("edges out");
    for ((caller, callee), n) in &edges {
        writeln!(w, "{caller}\t{callee}\t{n}").expect("edges write");
    }
    println!(
        "[OK] A1 SCIP face: ref-sites={} distinct-callees={} edges={} item-level-sites={}",
        by_callee.values().map(Vec::len).sum::<usize>(),
        by_callee.len(),
        edges.len(),
        item_level
    );
}
