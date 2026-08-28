//! S1 graph-engine substrate tests (ep-v1plus-engine-parity.md): loader
//! ordering / filtering / adjacency semantics against the synthetic
//! self-owned graph-db fixture. Rowid (insertion) order is the parity
//! contract inherited from the CRG-era ORDER-BY-less `SELECT *` queries.

mod graph_db_fixture;

use code_reality::graph_engine::{load_edges, load_flow_adjacency, load_nodes, open};
use graph_db_fixture::{make_graph_db, GraphDbSpec, NodeAttr, NodeSeed};

fn spec() -> GraphDbSpec {
    let nodes = vec![
        NodeSeed {
            name: "alpha".into(),
            parent: None,
            qname: "/repo/src/a.rs::alpha".into(),
            file_path: "/repo/src/a.rs".into(),
        },
        NodeSeed {
            name: "beta".into(),
            parent: None,
            qname: "/repo/src/a.rs::beta".into(),
            file_path: "/repo/src/a.rs".into(),
        },
        NodeSeed {
            name: "gamma".into(),
            parent: None,
            qname: "/repo/src/b.rs::gamma".into(),
            file_path: "/repo/src/b.rs".into(),
        },
    ];
    // a File-kind node: excluded by default loader, present in adjacency
    let node_attrs = vec![
        (
            "/repo/src/a.rs::alpha".into(),
            NodeAttr {
                kind: "Function",
                language: "rust",
                is_test: 0,
                community_id: Some(1),
            },
        ),
        (
            "/repo/src/a.rs::beta".into(),
            NodeAttr {
                kind: "Function",
                language: "rust",
                is_test: 1,
                community_id: None,
            },
        ),
        (
            "/repo/src/b.rs::gamma".into(),
            NodeAttr {
                kind: "File",
                language: "rust",
                is_test: 0,
                community_id: None,
            },
        ),
    ];
    let edges = vec![
        (
            "CALLS".into(),
            "/repo/src/a.rs::alpha".into(),
            "/repo/src/a.rs::beta".into(),
        ),
        (
            "CALLS".into(),
            "/repo/src/a.rs::alpha".into(),
            "/repo/src/a.rs::beta".into(),
        ),
        (
            "TESTED_BY".into(),
            "/repo/src/a.rs::alpha".into(),
            "/repo/src/a.rs::beta".into(),
        ),
        (
            "IMPORTS_FROM".into(),
            "/repo/src/b.rs::gamma".into(),
            "/repo/src/a.rs::alpha".into(),
        ),
    ];
    GraphDbSpec {
        nodes,
        node_attrs,
        edges,
        ..Default::default()
    }
}

#[test]
fn load_nodes_excludes_files_and_keeps_rowid_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let nodes = load_nodes(&conn, true).unwrap();
    assert_eq!(nodes.len(), 2, "File node excluded");
    assert_eq!(nodes[0].name, "alpha");
    assert_eq!(nodes[1].name, "beta");
    assert!(nodes[1].is_test);
    assert_eq!(nodes[0].community_id, Some(1));

    let all = load_nodes(&conn, false).unwrap();
    assert_eq!(all.len(), 3, "File node included when asked");
    assert_eq!(all[2].kind, "File");
}

#[test]
fn load_edges_keeps_rowid_order_and_all_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let edges = load_edges(&conn).unwrap();
    assert_eq!(edges.len(), 4);
    assert_eq!(edges[0].kind, "CALLS");
    assert_eq!(edges[1].kind, "CALLS", "duplicate edge rows preserved");
    assert_eq!(edges[2].kind, "TESTED_BY");
    assert_eq!(edges[3].kind, "IMPORTS_FROM");
}

#[test]
fn flow_adjacency_calls_and_tested_by_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let adj = load_flow_adjacency(&conn).unwrap();
    // all nodes incl. File (graph.py:1493 reads nodes unfiltered)
    assert_eq!(adj.nodes_by_key.len(), 3);
    assert!(adj.nodes_by_key.contains_key("/repo/src/b.rs::gamma"));
    // CALLS appended with duplicates, only CALLS feed calls_out
    let out = &adj.calls_out["/repo/src/a.rs::alpha"];
    assert_eq!(out.len(), 2, "duplicate CALLS rows both appended");
    assert_eq!(out[0], "/repo/src/a.rs::beta");
    // TESTED_BY records its SOURCE (the tested node, CRG #515)
    assert!(adj.has_tested_by.contains("/repo/src/a.rs::alpha"));
    // id map mirrors qn map
    assert_eq!(adj.nodes_by_id.len(), 3);
}

#[test]
fn open_missing_db_fails_loud() {
    let dir = tempfile::tempdir().unwrap();
    let err = open(dir.path()).unwrap_err();
    assert!(!err.is_empty());
}

// ---------- S2: hub + bridge ----------

use code_reality::graph_engine::{find_bridge_nodes, find_hub_nodes};

fn chain_spec() -> GraphDbSpec {
    // directed chain a->b->c plus d->b; File node f isolated from kinds but
    // present; e is a zero-degree Function (must not appear in hub)
    let mut spec = GraphDbSpec::default();
    let qn = |s: &str| format!("/repo/src/x.rs::{s}");
    spec.nodes = vec!["a", "b", "c", "d", "e", "f"]
        .into_iter()
        .map(|s| NodeSeed {
            name: s.into(),
            parent: None,
            qname: qn(s),
            file_path: "/repo/src/x.rs".into(),
        })
        .collect();
    let attrs: Vec<(&str, &str)> = vec![
        ("a", "Function"),
        ("b", "Function"),
        ("c", "Function"),
        ("d", "Function"),
        ("e", "Function"),
        ("f", "File"),
    ];
    spec.node_attrs = attrs
        .into_iter()
        .map(|(s, kind)| {
            (
                qn(s),
                NodeAttr {
                    kind,
                    language: "rust",
                    is_test: 0,
                    community_id: None,
                },
            )
        })
        .collect();
    let edge = |k: &str, s: &str, t: &str| (k.to_string(), qn(s), qn(t));
    spec.edges = vec![
        edge("CALLS", "a", "b"),
        edge("CALLS", "a", "b"), // duplicate row: hub counts it, bridge graph dedupes
        edge("CALLS", "b", "c"),
        edge("CALLS", "d", "b"),
    ];
    spec
}

#[test]
fn hub_counts_all_edge_rows_and_excludes_zero_degree() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &chain_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let hubs = find_hub_nodes(&conn, 10).unwrap();
    // b: in=3 (a,a,d) out=1 (c) total=4; a: out=2; c: in=1; d: out=1;
    // e excluded (zero degree), f excluded (File)
    assert_eq!(hubs.len(), 4);
    assert_eq!(hubs[0]["qualified_name"], "/repo/src/x.rs::b");
    assert_eq!(hubs[0]["total_degree"], 4);
    assert_eq!(hubs[0]["in_degree"], 3);
    assert_eq!(hubs[0]["out_degree"], 1);
    // a and c tie at 2 vs 1: a(2) then c(1)/d(1) in rowid order
    assert_eq!(hubs[1]["qualified_name"], "/repo/src/x.rs::a");
    assert_eq!(hubs[1]["total_degree"], 2);
    assert_eq!(hubs[2]["qualified_name"], "/repo/src/x.rs::c");
    assert_eq!(hubs[3]["qualified_name"], "/repo/src/x.rs::d");
}

#[test]
fn bridge_brandes_directed_normalized_positive_only() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &chain_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let bridges = find_bridge_nodes(&conn, 10).unwrap();
    // n=4 (edge-incident nodes a,b,c,d; e/f not in graph) — b on a->c and
    // d->c: 2 pairs / (n-1)(n-2)=6 -> 0.333333; others 0 -> filtered
    assert_eq!(bridges.len(), 1);
    assert_eq!(bridges[0]["qualified_name"], "/repo/src/x.rs::b");
    assert_eq!(bridges[0]["betweenness"], 0.333333);
}

// ---------- S3: flows family ----------

use code_reality::graph_engine::{affected_flows, detect_entry_points, trace_flows};

fn flows_spec() -> GraphDbSpec {
    // main (name-pattern entry) -> a -> b -> c -> b (cycle); d is an
    // uncalled function with no outgoing (trivial flow, skipped);
    // "auth_handler" name hits a SECURITY_KEYWORD; ext target absent from
    // nodes (external call); file-source CALLS edge to "wired" must NOT
    // hide it as an entry (include_file_sources=False semantics).
    let mut spec = GraphDbSpec::default();
    let qn = |s: &str| format!("/repo/src/f.rs::{s}");
    let names = [
        "main",
        "a",
        "b",
        "c",
        "d",
        "auth_handler",
        "wired",
        "filemod",
    ];
    spec.nodes = names
        .iter()
        .map(|s| NodeSeed {
            name: s.to_string(),
            parent: None,
            qname: qn(s),
            file_path: if *s == "c" {
                "/repo/src/g.rs".into()
            } else {
                "/repo/src/f.rs".into()
            },
        })
        .collect();
    let mut attrs: Vec<(String, NodeAttr)> = names
        .iter()
        .map(|s| {
            (
                qn(s),
                NodeAttr {
                    kind: "Function",
                    language: "rust",
                    is_test: 0,
                    community_id: None,
                },
            )
        })
        .collect();
    // filemod is a File node whose CALLS edge must not count as a caller
    attrs.push((
        qn("filemod"),
        NodeAttr {
            kind: "File",
            language: "rust",
            is_test: 0,
            community_id: None,
        },
    ));
    spec.node_attrs = attrs;
    let edge = |k: &str, s: &str, t: &str| (k.to_string(), qn(s), qn(t));
    spec.edges = vec![
        edge("CALLS", "main", "a"),
        edge("CALLS", "a", "b"),
        edge("CALLS", "b", "c"),
        edge("CALLS", "c", "b"),            // cycle back
        edge("CALLS", "a", "ext::missing"), // external target (no node)
        edge("CALLS", "filemod", "wired"),  // File-source: wired stays an entry
    ];
    spec
}

#[test]
fn trace_flows_entries_bfs_and_criticality() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &flows_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    // entry detection face: name-pattern (main), true roots (d,
    // auth_handler), File-source call does not hide wired
    let entry_nodes = detect_entry_points(&conn, false).unwrap();
    let entries: Vec<&str> = entry_nodes
        .iter()
        .map(|n| n.qualified_name.as_str())
        .collect();
    assert!(entries.contains(&"/repo/src/f.rs::main"), "{entries:?}");
    assert!(entries.contains(&"/repo/src/f.rs::wired"), "{entries:?}");
    assert!(
        entries.contains(&"/repo/src/f.rs::auth_handler"),
        "{entries:?}"
    );
    assert!(entries.contains(&"/repo/src/f.rs::d"), "{entries:?}");
    assert!(!entries.contains(&"/repo/src/f.rs::a"));
    assert!(!entries.contains(&"/repo/src/f.rs::b"));
    assert!(!entries.contains(&"/repo/src/f.rs::c"));

    // flow face: trivial single-node flows skipped
    let flows = trace_flows(&conn, 15, false).unwrap();
    let flow_entries: Vec<&str> = flows
        .iter()
        .map(|f| f["entry_point"].as_str().unwrap())
        .collect();
    assert_eq!(
        flow_entries,
        vec!["/repo/src/f.rs::main"],
        "{flow_entries:?}"
    );

    let main_flow = flows
        .iter()
        .find(|f| f["entry_point"] == "/repo/src/f.rs::main")
        .unwrap();
    assert_eq!(main_flow["depth"], 3); // main->a->b->c
    assert_eq!(main_flow["node_count"], 4); // main,a,b,c (cycle re-entry blocked)
    assert_eq!(main_flow["file_count"], 2);
    // criticality hand-check: files {f,g} -> spread (2-1)/4=0.25 -> .075
    // external: a->ext missing = 1 -> 1/5 -> *0.2 = .04
    // security: none of main/a/b/c hit keywords -> 0
    // test gap: no TESTED_BY -> 1.0 -> .15 ; depth 3 -> 0.3 -> .03
    // total = .075+.04+.15+.03 = .295
    assert_eq!(main_flow["criticality"], 0.295);
}

#[test]
fn affected_flows_filters_by_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &flows_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let all = trace_flows(&conn, 15, false).unwrap();
    let affected = affected_flows(&conn, &["/repo/src/g.rs".to_string()], 15, false).unwrap();
    // only main's flow reaches g.rs (via c)
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0]["entry_point"], "/repo/src/f.rs::main");
    assert!(all.len() >= affected.len());
}

// ---------- S4: impact_radius ----------

use code_reality::graph_engine::impact_radius;

fn impact_spec() -> GraphDbSpec {
    let mut spec = GraphDbSpec::default();
    let qn = |f: &str, s: &str| format!("/repo/{f}::{s}");
    let push = |spec: &mut GraphDbSpec, file: &str, sym: &str| {
        spec.nodes.push(NodeSeed {
            name: sym.into(),
            parent: None,
            qname: qn(file, sym),
            file_path: format!("/repo/{file}"),
        });
    };
    for (f, s) in [
        ("a.rs", "f1a"),
        ("a.rs", "f1b"),
        ("b.rs", "f2a"),
        ("c.rs", "f3a"),
        ("b.rs", "x"),
        ("c.rs", "y"),
    ] {
        push(&mut spec, f, s);
    }
    spec.node_attrs = spec
        .nodes
        .iter()
        .map(|n| {
            (
                n.qname.clone(),
                NodeAttr {
                    kind: "Function",
                    language: "rust",
                    is_test: 0,
                    community_id: None,
                },
            )
        })
        .collect();
    let e = |k: &str, s: &str, t: &str| (k.to_string(), qn("a.rs", s).clone(), qn("b.rs", t));
    let _ = e; // silence unused in macro-less helper
    let q2 = |f: &str, s: &str| format!("/repo/{f}::{s}");
    spec.edges = vec![
        ("CALLS".into(), q2("a.rs", "f1a"), q2("b.rs", "f2a")),
        ("IMPORTS_FROM".into(), q2("a.rs", "f1a"), q2("c.rs", "f3a")),
        ("CALLS".into(), q2("b.rs", "f2a"), q2("c.rs", "f3a")),
        ("CONTAINS".into(), q2("a.rs", "f1a"), q2("b.rs", "x")),
    ];
    spec
}

#[test]
fn impact_radius_relaxation_scores_and_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &impact_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let out = impact_radius(&conn, &["/repo/a.rs".to_string()], 2, 500).unwrap();
    // depth1: f2a = 1.0*1.0*0.6 = 0.6 ; f3a = 1.0*0.5*0.6 = 0.3 ; x = 0.18
    // depth2: f3a via f2a = 0.6*1.0*0.6 = 0.36 (beats 0.3) ; y = 0.18*0.3*0.6
    //        = 0.0324 < floor 0.05 -> pruned
    let scores = &out["impact_scores"];
    assert_eq!(scores["/repo/b.rs::f2a"], 0.6);
    assert_eq!(scores["/repo/c.rs::f3a"], 0.36);
    assert_eq!(scores["/repo/b.rs::x"], 0.18);
    assert!(scores.get("/repo/c.rs::y").is_none(), "floor-pruned");
    let impacted: Vec<&str> = out["impacted_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["qualified_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        impacted,
        vec!["/repo/b.rs::f2a", "/repo/c.rs::f3a", "/repo/b.rs::x"] // .6 > .36 > .18
    );
    assert_eq!(out["truncated"], false);
    assert_eq!(out["total_impacted"], 3);
    let changed: Vec<&str> = out["changed_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["qualified_name"].as_str().unwrap())
        .collect();
    assert_eq!(changed.len(), 2); // f1a + f1b
                                  // edges among seeds+impacted: all four edges qualify
    assert_eq!(out["edges"].as_array().unwrap().len(), 4);
}

#[test]
fn impact_radius_cap_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &impact_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let out = impact_radius(&conn, &["/repo/a.rs".to_string()], 2, 1).unwrap();
    assert_eq!(out["truncated"], true);
    assert_eq!(out["total_impacted"], 3);
    assert_eq!(out["impacted_nodes"].as_array().unwrap().len(), 1);
}

#[test]
fn impact_radius_excludes_verilog_extra() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &impact_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE nodes SET extra='{\"verilog_kind\":\"module\"}' \
         WHERE symbol='/repo/b.rs::f2a'",
        [],
    )
    .unwrap();

    let out = impact_radius(&conn, &["/repo/a.rs".to_string()], 2, 500).unwrap();
    let scores = &out["impact_scores"];
    assert!(
        scores.get("/repo/b.rs::f2a").is_none(),
        "verilog node excluded"
    );
}

// ---------- S5: communities Tier 0 + architecture_overview ----------

use code_reality::graph_engine::{architecture_overview, detect_communities};

fn comm_spec() -> GraphDbSpec {
    let mut spec = GraphDbSpec::default();
    let node = |spec: &mut GraphDbSpec, qname: String, file: &str, kind: &'static str| {
        spec.nodes.push(NodeSeed {
            name: qname.rsplit("::").next().unwrap().into(),
            parent: None,
            qname: qname.clone(),
            file_path: file.into(),
        });
        spec.node_attrs.push((
            qname,
            NodeAttr {
                kind,
                language: "rust",
                is_test: 0,
                community_id: None,
            },
        ));
    };
    let q = |s: &str| format!("/repo/src/alpha/mod.rs::{s}");
    // alpha dir: two order_* functions (keyword "order") + internal edge
    node(
        &mut spec,
        q("order_entry"),
        "/repo/src/alpha/mod.rs",
        "Function",
    );
    node(
        &mut spec,
        q("order_cancel"),
        "/repo/src/alpha/mod.rs",
        "Function",
    );
    // beta dir: two nodes, cross edge to alpha
    let qb = |s: &str| format!("/repo/src/beta/mod.rs::{s}");
    node(
        &mut spec,
        qb("price_tick"),
        "/repo/src/beta/mod.rs",
        "Function",
    );
    node(
        &mut spec,
        qb("price_depth"),
        "/repo/src/beta/mod.rs",
        "Function",
    );
    // LCP is /repo/src; depth1 groups = {alpha, beta}
    spec.edges = vec![
        ("CALLS".into(), q("order_entry"), q("order_cancel")),
        ("CALLS".into(), qb("price_tick"), qb("price_depth")),
        ("CALLS".into(), qb("price_tick"), q("order_entry")), // cross
        ("TESTED_BY".into(), q("order_entry"), qb("price_tick")), // ignored in overview
    ];
    spec
}

#[test]
fn communities_directory_grouping_naming_cohesion() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &comm_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let comms = detect_communities(&conn, 2).unwrap();
    assert_eq!(comms.len(), 2, "{comms:?}");
    let by_dir: Vec<&str> = comms
        .iter()
        .map(|c| c["description"].as_str().unwrap())
        .collect();
    assert!(by_dir.contains(&"Directory-based community: alpha"));
    assert!(by_dir.contains(&"Directory-based community: beta"));
    for c in &comms {
        let desc = c["description"].as_str().unwrap();
        let (name, cohesion) = (c["name"].as_str().unwrap(), c["cohesion"].as_f64().unwrap());
        if desc.ends_with("alpha") {
            // alpha: internal 1 (entry->cancel); external 2 (tick->entry
            // target + TESTED_BY entry->tick source — cohesion walks ALL
            // edges, no kind filter, communities.py:212)
            // cohesion = 1/(1+2) = 0.3333 ; prefix alpha, keyword order
            assert_eq!(name, "alpha-order", "{c:?}");
            assert_eq!(cohesion, 0.3333);
            assert_eq!(c["size"], 2);
        } else {
            // beta: internal 1, external 2 (tick->entry source + TESTED_BY
            // tick target) -> 1/3 ; keyword: price
            assert_eq!(name, "beta-price", "{c:?}");
            assert_eq!(cohesion, 0.3333);
        }
        assert_eq!(c["dominant_language"], "rust");
        assert_eq!(c["level"], 0);
    }
}

#[test]
fn architecture_overview_cross_edges_and_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &comm_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();

    let out = architecture_overview(&conn, 100, false).unwrap();
    let cross = out["cross_community_edges"].as_array().unwrap();
    // one cross CALLS edge (tick->entry); TESTED_BY skipped
    assert_eq!(cross.len(), 1);
    assert_eq!(cross[0]["edge_kind"], "CALLS");
    // single cross edge < 10 -> no warning
    assert_eq!(out["warnings"].as_array().unwrap().len(), 0);
    assert_eq!(out["communities"].as_array().unwrap().len(), 2);
}

// ---------- S6: detect_changes + risk ----------

use code_reality::graph_engine::{
    compute_risk_score, detect_changes, map_changes_to_nodes, parse_unified_diff,
};

const DIFF_TEXT: &str = "--- a/pkg/a.py\n+++ b/pkg/a.py\n@@ -1,2 +1,3 @@\n ctx\n+new\n ctx\n@@ -10 +10,2 @@\n old\n+add\n--- b/pkg/gone.py\n+++ /dev/null\n@@ -1 +0,0 @@\n bye\n";

#[test]
fn parse_unified_diff_hunks() {
    let ranges = parse_unified_diff(DIFF_TEXT);
    let a = &ranges["pkg/a.py"];
    assert!(a.contains(&(1, 3)), "{a:?}");
    assert!(a.contains(&(10, 11)), "{a:?}"); // single-line form +start,count omitted
    assert!(
        !ranges.contains_key("pkg/gone.py"),
        "/dev/null file skipped"
    );
}

fn risk_spec() -> (GraphDbSpec, Vec<String>) {
    let q = |s: &str| format!("/repo/src/r.rs::{s}");
    let mut spec = GraphDbSpec::default();
    let names = ["victim", "caller1", "caller2", "helper", "tester"];
    spec.nodes = names
        .iter()
        .map(|s| NodeSeed {
            name: s.to_string(),
            parent: None,
            qname: q(s),
            file_path: "/repo/src/r.rs".into(),
        })
        .collect();
    spec.node_attrs = names
        .iter()
        .enumerate()
        .map(|(i, s)| {
            (
                q(s),
                NodeAttr {
                    kind: if *s == "tester" { "Test" } else { "Function" },
                    language: "rust",
                    is_test: if *s == "tester" { 1 } else { 0 },
                    community_id: if i < 2 { Some(1) } else { Some(2) },
                },
            )
        })
        .collect();
    spec.node_spans = vec![
        (q("victim"), 10, 12),
        (q("caller1"), 6, 8),
        (q("caller2"), 7, 9),
        (q("helper"), 40, 42),
        (q("tester"), 50, 52),
    ];
    spec.edges = vec![
        ("CALLS".into(), q("caller1"), q("victim")), // same community as victim? victim cid=1, caller1 cid=1 -> not crossing
        ("CALLS".into(), q("caller2"), q("victim")), // cid=2 -> crossing
        ("TESTED_BY".into(), q("victim"), q("tester")),
    ];
    spec.flow_crits = vec![(7, 0.2), (8, 0.1)];
    // victim node id = 1 (first inserted)
    spec.flow_members = vec![(7, 1), (8, 1)];
    (spec, names.iter().map(|s| q(s)).collect())
}

#[test]
fn risk_score_six_factors_handcheck() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _qns) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let nodes = load_nodes(&conn, true).unwrap();
    let victim = nodes.iter().find(|n| n.name == "victim").unwrap();
    // flow participation: sum criticalities 0.2+0.1 = 0.3 -> cap 0.25
    // community crossing: caller2 (cid2 != cid1) -> 0.05
    // test coverage: 1 transitive test -> 0.30 - (1/5)*0.25 = 0.25
    // security: none ; caller count: 2/20 = 0.1 ; churn: off
    // total = 0.25+0.05+0.25+0.1 = 0.65
    let risk = compute_risk_score(&conn, victim, None).unwrap();
    assert_eq!(risk, 0.65);
}

#[test]
fn map_changes_overlap_and_suffix_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    // exact path + overlap on victim (10..10 hits range (5,15))
    let mut ranges = std::collections::BTreeMap::new();
    ranges.insert("/repo/src/r.rs".to_string(), vec![(5, 15)]);
    let nodes = map_changes_to_nodes(&conn, &ranges).unwrap();
    assert!(nodes.iter().any(|n| n.name == "victim"));
    assert!(!nodes.iter().any(|n| n.name == "helper"), "line 40 outside");
    // suffix fallback: relative path resolves via LIKE
    let mut rel = std::collections::BTreeMap::new();
    rel.insert("src/r.rs".to_string(), vec![(10, 10)]);
    let nodes2 = map_changes_to_nodes(&conn, &rel).unwrap();
    assert!(nodes2.iter().any(|n| n.name == "victim"));
}

#[test]
fn detect_changes_composition() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut ranges = std::collections::BTreeMap::new();
    ranges.insert("/repo/src/r.rs".to_string(), vec![(5, 15)]);
    let out = detect_changes(
        &conn,
        None,
        &["/repo/src/r.rs".to_string()],
        Some(&ranges),
        None,
    )
    .unwrap();
    assert!(out["summary"]
        .as_str()
        .unwrap()
        .contains("changed function"));
    assert!(out["changed_functions"].as_array().unwrap().len() >= 2);
    assert!(out["risk_score"].as_f64().unwrap() > 0.0);
    // victim has TESTED_BY -> not a gap; caller1/caller2/helper untested -> gaps
    let gaps = out["test_gaps"].as_array().unwrap();
    assert!(gaps.iter().all(|g| g["name"].as_str().unwrap() != "victim"));
}

// ---------- S7/S8: search + composition ----------

use code_reality::graph_engine::{get_minimal_context, get_review_context, search_nodes};

#[test]
fn search_fts_then_like_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    // the fixture ships an FTS table (production parity) -> fts5 face
    let (nodes, method) = search_nodes(&conn, "victim", 20).unwrap();
    assert_eq!(method, "fts5");
    assert!(nodes.iter().any(|n| n["name"] == "victim"));
    // multi-word AND semantics
    let (none, _) = search_nodes(&conn, "victim zzz", 20).unwrap();
    assert!(none.is_empty());
    // drop the FTS table -> LIKE fallback path
    conn.execute_batch("DROP TABLE nodes_fts;").unwrap();
    let (nodes2, method2) = search_nodes(&conn, "victim", 20).unwrap();
    assert_eq!(method2, "like");
    assert!(nodes2.iter().any(|n| n["name"] == "victim"));
    // recreate for the fts5 re-check below
    conn.execute_batch(
        "CREATE VIRTUAL TABLE nodes_fts USING fts5(name, content='nodes', content_rowid='id');",
    )
    .unwrap();
    conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')", [])
        .unwrap();
    let (nodes2, method2) = search_nodes(&conn, "victim", 20).unwrap();
    assert_eq!(method2, "fts5");
    assert!(nodes2.iter().any(|n| n["name"] == "victim"));
}

#[test]
fn minimal_context_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let out = get_minimal_context(&conn, "review PR 42", &["/repo/src/r.rs".to_string()]).unwrap();
    assert!(out["summary"].as_str().unwrap().contains("nodes"));
    assert!(out["summary"].as_str().unwrap().contains("Risk:"));
    assert_eq!(
        out["next_tool_suggestions"],
        serde_json::json!(["detect_changes", "get_affected_flows", "get_review_context"])
    );
}

#[test]
fn review_context_structural_keys() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let (spec, _) = risk_spec();
    make_graph_db(&db, &spec).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    std::fs::write(dir.path().join("x.rs"), "line1\nline2\n").unwrap();
    let out = get_review_context(&conn, dir.path(), &["x.rs".to_string()], 2, true, 200).unwrap();
    assert_eq!(out["status"], "ok");
    assert!(out["summary"].as_str().unwrap().contains("Review context"));
    assert!(out["context"]["impact"].is_object());
    assert!(out["context"]["source_snippets"]["x.rs"]
        .as_str()
        .unwrap()
        .contains("line1"));
    assert!(out["context"]["review_guidance"].as_str().is_some());
}

// ---------- S9 review fixes: argv parser faces ----------

use code_reality::graph_engine::run as gq_run;

fn gq_repo_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let graph_dir = dir.path().join(".code-reality");
    std::fs::create_dir_all(&graph_dir).unwrap();
    let (spec, _) = risk_spec();
    make_graph_db(&graph_dir.join("graph.db"), &spec).unwrap();
    (dir, graph_dir.join("graph.db"))
}

#[test]
fn gq_run_missing_repo_is_loud() {
    let out = gq_run(&["graph_query", "hub"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("--repo"), "{}", out.stderr);
}

#[test]
fn gq_run_bad_numeric_flag_is_loud() {
    let out = gq_run(&["graph_query", "hub", "--repo", "/x", "--limit", "abc"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("非非負整數"), "{}", out.stderr);
}

#[test]
fn gq_run_comma_encoding_rejects_empty_segments() {
    let out = gq_run(&[
        "graph_query",
        "impact_radius",
        "--repo",
        "/x",
        "--files",
        "a,,b",
    ]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("逗號分隔"), "{}", out.stderr);
}

#[test]
fn gq_run_hub_end_to_end() {
    let (dir, _db) = gq_repo_db();
    let out = gq_run(&[
        "graph_query",
        "hub",
        "--repo",
        dir.path().to_string_lossy().as_ref(),
        "--limit",
        "2",
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.starts_with("[OK] graph_query hub"));
    assert!(out.stdout.contains("\"total_degree\""));
}

// ---------- deep-work: Leiden Tier 1 + union mapper + document_symbols ----------

use code_reality::graph_engine::{detect_communities_leiden, document_symbols_at};

fn two_cluster_spec() -> GraphDbSpec {
    // two dense trios joined by one weak (CONTAINS) bridge edge
    let q = |f: &str, s: &str| format!("/repo/{f}::{s}");
    let mut spec = GraphDbSpec::default();
    let syms = [
        ("a.rs", "a1"),
        ("a.rs", "a2"),
        ("a.rs", "a3"),
        ("b.rs", "b1"),
        ("b.rs", "b2"),
        ("b.rs", "b3"),
    ];
    spec.nodes = syms
        .iter()
        .map(|(f, s)| NodeSeed {
            name: s.to_string(),
            parent: None,
            qname: q(f, s),
            file_path: format!("/repo/{f}"),
        })
        .collect();
    spec.node_attrs = syms
        .iter()
        .map(|(f, s)| {
            (
                q(f, s),
                NodeAttr {
                    kind: "Function",
                    language: "rust",
                    is_test: 0,
                    community_id: None,
                },
            )
        })
        .collect();
    let e = |k: &str, a: (&str, &str), b: (&str, &str)| (k.to_string(), q(a.0, a.1), q(b.0, b.1));
    spec.edges = vec![
        e("CALLS", ("a.rs", "a1"), ("a.rs", "a2")),
        e("CALLS", ("a.rs", "a2"), ("a.rs", "a3")),
        e("CALLS", ("a.rs", "a3"), ("a.rs", "a1")),
        e("CALLS", ("b.rs", "b1"), ("b.rs", "b2")),
        e("CALLS", ("b.rs", "b2"), ("b.rs", "b3")),
        e("CALLS", ("b.rs", "b3"), ("b.rs", "b1")),
        e("CONTAINS", ("a.rs", "a1"), ("b.rs", "b1")), // weak bridge
    ];
    spec
}

#[test]
fn leiden_separates_clusters_and_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    make_graph_db(&db, &two_cluster_spec()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let r1 = detect_communities_leiden(&conn, 2, 42).unwrap();
    let r2 = detect_communities_leiden(&conn, 2, 42).unwrap();
    assert_eq!(r1, r2, "seeded run must be bit-for-bit deterministic");
    // the weak bridge must not merge the trios: >= 2 communities, none
    // holding all six nodes
    assert!(r1.len() >= 2, "{r1:?}");
    let max_size = r1
        .iter()
        .map(|c| c["size"].as_i64().unwrap_or(0))
        .max()
        .unwrap_or(0);
    assert!(max_size < 6, "no mega-community: max {max_size}");
    assert!(r1.iter().all(|c| c["description"]
        .as_str()
        .unwrap()
        .starts_with("Leiden community")));
}

fn make_union_slot(dir: &std::path::Path) -> std::path::PathBuf {
    use rusqlite::Connection;
    let index = dir.join("index.scip");
    std::fs::write(&index, b"").unwrap();
    // cache: occurrences (is_def rows map symbols to rel files)
    let cache = dir.join("index.scip.db");
    let c = Connection::open(&cache).unwrap();
    c.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE occurrences (
            seq INTEGER PRIMARY KEY, symbol TEXT NOT NULL, rel_path TEXT NOT NULL,
            line INTEGER NOT NULL, is_def INTEGER NOT NULL);
         CREATE TABLE symbol_tails (symbol TEXT PRIMARY KEY, tail TEXT NOT NULL, method TEXT);",
    )
    .unwrap();
    c.execute(
        "INSERT INTO occurrences (seq, symbol, rel_path, line, is_def) VALUES
         (1, 'scip alpha a1().', 'src/a.rs', 10, 1),
         (2, 'scip beta b1().', 'src/b.rs', 20, 1)",
        [],
    )
    .unwrap();
    // sidecar: one edge a1->b1 (SCIP symbols)
    let side = dir.join("index.union.db");
    let sdb = Connection::open(&side).unwrap();
    sdb.execute_batch(
        "CREATE TABLE edges (
            caller_symbol TEXT NOT NULL, callee_symbol TEXT NOT NULL,
            sites INTEGER NOT NULL, kind TEXT NOT NULL DEFAULT 'REFERENCES',
            provenance TEXT NOT NULL DEFAULT 'SCIP', updated_at REAL NOT NULL,
            PRIMARY KEY (caller_symbol, callee_symbol));",
    )
    .unwrap();
    sdb.execute(
        "INSERT INTO edges (caller_symbol, callee_symbol, sites, updated_at)
         VALUES ('scip alpha a1().', 'scip beta b1().', 3, 0.0)",
        [],
    )
    .unwrap();
    index
}

#[test]
fn document_symbols_at_outlines_file() {
    let dir = tempfile::tempdir().unwrap();
    let index = make_union_slot(dir.path());
    let out = document_symbols_at(&index, "src/a.rs").unwrap();
    assert_eq!(out["symbols"][0]["name"], "a1");
    assert_eq!(out["symbols"][0]["line"], 10);
    assert!(out["note"].as_str().unwrap().contains("LSP-only"));
    // missing cache -> loud error
    let err = document_symbols_at(&dir.path().join("nope.scip"), "x.rs");
    assert!(err.is_err());
}

// ---------- R2 (dual-context review): symbol≠qname key-space ----------

/// In the real self-owned db, symbol (producer string) and qname
/// (display) differ. Community cohesion is computed against edge
/// endpoints (symbols) — with members keyed by qname it was structurally
/// 0.0 and cross-community edges were structurally empty (masked by the
/// fixture universe where symbol==qname). This db splits the two spaces.
#[test]
fn communities_cohesion_and_cross_edges_survive_symbol_ne_qname() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("g.db");
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, name TEXT NOT NULL,
                qname TEXT NOT NULL, file_path TEXT NOT NULL, line_start INTEGER,
                line_end INTEGER, language TEXT, parent_name TEXT,
                is_test INTEGER DEFAULT 0, extra TEXT DEFAULT '{}',
                updated_at REAL NOT NULL, community_id INTEGER, provenance TEXT);
             CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL, caller_symbol TEXT NOT NULL, callee_symbol TEXT NOT NULL,
                provenance TEXT NOT NULL, file_path TEXT NOT NULL, line INTEGER DEFAULT 0,
                confidence REAL DEFAULT 1.0, confidence_tier TEXT DEFAULT 'EXTRACTED',
                updated_at REAL NOT NULL);",
        )
        .unwrap();
        // two directories, qnames deliberately unlike symbols
        for (sym, qname, file) in [
            ("sym a1().", "/repo/a.rs::alpha_one", "/repo/a.rs"),
            ("sym a2().", "/repo/a.rs::alpha_two", "/repo/a.rs"),
            ("sym b1().", "/repo/b.rs::beta_one", "/repo/b.rs"),
            ("sym b2().", "/repo/b.rs::beta_two", "/repo/b.rs"),
        ] {
            c.execute(
                "INSERT INTO nodes (symbol, kind, name, qname, file_path, language, updated_at)
                 VALUES (?1, 'Function', ?2, ?3, ?4, 'Rust', 0)",
                rusqlite::params![sym, sym, qname, file],
            )
            .unwrap();
        }
        for (cs, ct) in [
            ("sym a1().", "sym a2()."),
            ("sym a2().", "sym a1()."),
            ("sym b1().", "sym b2()."),
            ("sym a1().", "sym b1()."), // the cross-community edge
        ] {
            c.execute(
                "INSERT INTO edges (kind, caller_symbol, callee_symbol, provenance, file_path, updated_at)
                 VALUES ('CALLS', ?1, ?2, 'test', '/repo', 0)",
                rusqlite::params![cs, ct],
            )
            .unwrap();
        }
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let comms = code_reality::graph_engine::detect_communities(&conn, 2).unwrap();
    assert_eq!(comms.len(), 2, "two directories");
    assert!(
        comms
            .iter()
            .any(|c| c["cohesion"].as_f64().unwrap_or(0.0) > 0.0),
        "R2: cohesion must be computed over the symbol key space, not 0.0"
    );
    let arch = code_reality::graph_engine::architecture_overview(&conn, 10, false).unwrap();
    // cross edges are a TOP-LEVEL array (graph_engine.rs:1463) — the R2
    // regression face: with qname-keyed members this was structurally
    // empty; the fixture carries exactly one cross edge (a1 → b1)
    let cross = arch["cross_community_edges"].as_array().unwrap();
    assert!(
        !cross.is_empty(),
        "R2: cross-community edges must survive the symbol key space"
    );
    for c in comms {
        for m in c["members"].as_array().unwrap() {
            let ms = m.as_str().unwrap();
            assert!(
                ms.starts_with("sym "),
                "R2: members must be symbols, got {ms}"
            );
        }
    }
}
