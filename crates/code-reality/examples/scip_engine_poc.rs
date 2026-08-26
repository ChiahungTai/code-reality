//! POC2 (throwaway — S3 evidence for ep-v1plus-graph-engine.md): engine-face
//! prototype over the materialized SCIP edge set. std-only: adjacency build,
//! caller-closure BFS, hub degree ranking — the CRG engine queries
//! (get_impact_radius / hub_nodes family) re-implemented over OUR edges to
//! size feasibility and latency for the B1/B2 adjudication.
//!
//! Input: POC1's a1_edges.tsv (caller \t callee \t sites).
//! Usage: cargo run --release --example scip_engine_poc -- [seed-substring]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

fn main() {
    let pats: Vec<String> = if std::env::args().count() > 1 {
        std::env::args().skip(1).collect()
    } else {
        vec!["EventStoreLifecycle]open".into()]
    };

    let t0 = Instant::now();
    // callee -> callers (closure direction) and caller -> callees (fan-out).
    let mut callers_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut callees_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut edge_rows = 0usize;
    let f = BufReader::new(
        File::open(".agent-tmp/poc-scip-injection/a1_edges.tsv")
            .expect("a1_edges.tsv (run scip_edge_poc first)"),
    );
    for line in f.lines() {
        let line = line.unwrap();
        let mut it = line.split('\t');
        let caller = it.next().expect("caller").to_string();
        let callee = it.next().expect("callee").to_string();
        callers_of
            .entry(callee.clone())
            .or_default()
            .insert(caller.clone());
        callees_of.entry(caller).or_default().insert(callee);
        edge_rows += 1;
    }
    let build_ms = t0.elapsed().as_millis();
    let node_count = callers_of.len() + callees_of.len(); // upper bound: both faces counted
    println!(
        "[OK] adjacency built in {build_ms}ms: edge-rows={} distinct-nodes≤{}",
        edge_rows, node_count
    );

    // ---- closure BFS from seed symbols (substring match, mirrors CLI query
    // resolution coarsely), depth ≤3, cycle reentries counted like the lib.
    let t1 = Instant::now();
    let candidates: BTreeSet<String> = callers_of
        .keys()
        .chain(callees_of.keys())
        .cloned()
        .collect();
    let seeds: Vec<String> = candidates
        .iter()
        .filter(|k| pats.iter().all(|p| k.contains(p)))
        .cloned()
        .collect();
    let mut visited: BTreeSet<String> = seeds.iter().cloned().collect();
    let mut reentries = 0usize;
    let mut frontier: Vec<String> = seeds.clone();
    println!("seed symbols matched: {} (pats={:?})", seeds.len(), pats);
    for depth in 1..=3 {
        let mut next: BTreeSet<String> = BTreeSet::new();
        for sym in &frontier {
            for caller in callers_of.get(sym).into_iter().flatten() {
                if visited.contains(caller) {
                    reentries += 1;
                } else {
                    next.insert(caller.clone());
                }
            }
        }
        if next.is_empty() {
            println!("  depth {depth}: 0 new symbols (done); cycle-reentries so far: {reentries}");
            break;
        }
        let n = next.len();
        visited.extend(next.iter().cloned());
        frontier = next.into_iter().collect();
        println!(
            "  depth {depth}: {n} new symbols (cumulative {}); reentries: {reentries}",
            visited.len()
        );
    }
    let closure_ms = t1.elapsed().as_millis();
    println!("[OK] closure BFS in {closure_ms}ms");

    // ---- hub ranking: most-referenced callees (in-degree in caller
    // direction) — the hub_nodes engine query face.
    let t2 = Instant::now();
    let mut by_callers: Vec<(&String, usize)> =
        callers_of.iter().map(|(k, v)| (k, v.len())).collect();
    by_callers.sort_by(|a, b| b.1.cmp(&a.1));
    println!("top-10 most-referenced (hub) symbols:");
    for (sym, n) in by_callers.iter().take(10) {
        println!("  {n:>5}  {}", &sym[sym.len().saturating_sub(90)..]);
    }
    let mut by_fanout: Vec<(&String, usize)> =
        callees_of.iter().map(|(k, v)| (k, v.len())).collect();
    by_fanout.sort_by(|a, b| b.1.cmp(&a.1));
    println!("top-10 highest fan-out callers:");
    for (sym, n) in by_fanout.iter().take(10) {
        println!("  {n:>5}  {}", &sym[sym.len().saturating_sub(90)..]);
    }
    println!("[OK] hub ranking in {}ms", t2.elapsed().as_millis());
}
