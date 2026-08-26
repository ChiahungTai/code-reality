//! S5 graph_csv tests — vote tie-break, projection, degree invariant,
//! quoting/CRLF byte faces (no Python oracle for quoting — pinned from
//! source per the EP), and the no-community case.

mod crg_fixture;

use code_reality::graph_csv::{degrees, load, run, write_csvs};
use std::path::{Path, PathBuf};

fn fixture_repo(tag: &str) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = tmp.join(tag);
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("pkg/a")).unwrap();
    std::fs::create_dir_all(repo.join("pkg/b")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\nexclude = [\".venv/\"]\n",
    )
    .unwrap();
    std::fs::canonicalize(&repo).unwrap()
}

fn build_db(repo: &Path) -> PathBuf {
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = crg_fixture::CrgDbSpec::default();
    // File nodes (c via .venv excluded downstream)
    for rel in ["pkg/a/mod.rs", "pkg/b/mod.rs", ".venv/x.rs"] {
        spec.nodes.push(crg_fixture::NodeSeed {
            name: format!("f_{rel}"),
            parent: None,
            qname: format!("{}::File", abs(rel)),
            file_path: abs(rel),
        });
        spec.node_attrs.push((
            format!("{}::File", abs(rel)),
            crg_fixture::NodeAttr {
                kind: "File",
                language: "rust",
                is_test: 0,
                community_id: None,
            },
        ));
    }
    // community votes: a.py → {1: 2, 2: 2} (tie → id 1); b.py → {3}
    for (qname, comm) in [
        ("pkg/a/mod.rs::Fn1", 1),
        ("pkg/a/mod.rs::Fn2", 1),
        ("pkg/a/mod.rs::Fn3", 2),
        ("pkg/a/mod.rs::Fn4", 2),
        ("pkg/b/mod.rs::Fn5", 3),
    ] {
        let file = qname.split("::").next().unwrap();
        spec.nodes.push(crg_fixture::NodeSeed {
            name: qname.rsplit("::").next().unwrap().to_string(),
            parent: None,
            qname: qname.to_string(),
            file_path: abs(file),
        });
        spec.node_attrs.push((
            qname.to_string(),
            crg_fixture::NodeAttr {
                kind: "Function",
                language: "rust",
                is_test: 0,
                community_id: Some(comm),
            },
        ));
    }
    for (cid, name) in [(1, "core"), (2, "edge"), (3, "solo")] {
        spec.communities
            .push((cid, name.to_string(), 1, "rust".to_string(), String::new()));
    }
    let a = abs("pkg/a/mod.rs");
    let b = abs("pkg/b/mod.rs");
    let v = abs(".venv/x.rs");
    for (kind, s, t) in [
        (
            "CALLS".to_string(),
            format!("{a}::Fn1"),
            format!("{b}::Fn5"),
        ),
        (
            "IMPORTS_FROM".to_string(),
            format!("{a}::Fn2"),
            format!("{b}::Fn5"),
        ),
        (
            "CALLS".to_string(),
            format!("{b}::Fn5"),
            format!("{a}::Fn1"),
        ),
        (
            "INHERITS".to_string(),
            format!("{a}::Fn1"),
            format!("{v}::File"),
        ),
        (
            "INHERITS".to_string(),
            format!("{a}::Fn1"),
            format!("{a}::Fn2"),
        ),
        (
            "REFERENCES".to_string(),
            format!("{a}::Fn1"),
            format!("{b}::Fn5"),
        ),
    ] {
        spec.edges.push((kind, s, t));
    }
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    db
}

#[test]
fn vote_tie_break_and_projection_and_invariant() {
    let repo = fixture_repo("votes");
    let db = build_db(&repo);
    let g = load(&db, &repo).unwrap();
    // excluded .venv file dropped
    let paths: Vec<&str> = g.nodes.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(paths, vec!["pkg/a/mod.rs", "pkg/b/mod.rs"]);
    // tie (2 vs 2) → smallest id 1; solo → 3
    let a = g.nodes.iter().find(|n| n.path == "pkg/a/mod.rs").unwrap();
    let b = g.nodes.iter().find(|n| n.path == "pkg/b/mod.rs").unwrap();
    assert_eq!(a.community, Some(1));
    assert_eq!(b.community, Some(3));
    // pair aggregation: a→b kinds CALLS+IMPORTS_FROM; b→a CALLS; a→a self
    // skipped; excluded endpoint skipped; REFERENCES dropped at SQL level
    assert_eq!(g.links.len(), 2);
    assert!(g.links.iter().any(|l| l.kinds == "CALLS+IMPORTS_FROM"));
    assert!(g.links.iter().any(|l| l.kinds == "CALLS"));
    // Σdegree == 2 × links
    let deg = degrees(&g.links);
    let total: i64 = deg.values().sum();
    assert_eq!(total, 2 * g.links.len() as i64);
}

#[test]
fn csv_bytes_crlf_and_minimal_quotes() {
    let repo = fixture_repo("csv_bytes");
    let db = build_db(&repo);
    let g = load(&db, &repo).unwrap();
    let out_dir = repo.join("out");
    let (nodes_p, links_p) = write_csvs(&g, &out_dir).unwrap();
    let nodes_csv = std::fs::read(&nodes_p).unwrap();
    let text = String::from_utf8(nodes_csv).unwrap();
    assert!(text.starts_with("id,label,community,community_name,lang,is_test,degree\r\n"));
    // community_name rendered; every line CRLF-terminated
    assert!(text.contains(",core,rust,0,"));
    assert!(text.ends_with("\r\n"));
    // no bare-LF rows anywhere: every line break is CRLF
    assert_eq!(text.matches('\n').count(), text.matches("\r\n").count());
    let links_csv = std::fs::read(&links_p).unwrap();
    let ltext = String::from_utf8(links_csv).unwrap();
    assert!(ltext.starts_with("source,target,kind\r\n"));
    assert!(ltext.contains("CALLS+IMPORTS_FROM\r\n") || ltext.contains("CALLS\r\n"));
}

#[test]
fn quote_rules_comma_quote_newline() {
    // field-level faces via a crafted name
    let repo = fixture_repo("quotes");
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = repo.join("weird.rs").to_string_lossy().into_owned();
    let mut spec = crg_fixture::CrgDbSpec::default();
    spec.nodes.push(crg_fixture::NodeSeed {
        name: "weird,\"name\".rs".into(),
        parent: None,
        qname: format!("{abs}::File"),
        file_path: abs.clone(),
    });
    spec.node_attrs.push((
        format!("{abs}::File"),
        crg_fixture::NodeAttr {
            kind: "File",
            language: "rust",
            is_test: 0,
            community_id: None,
        },
    ));
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    let g = load(&db, &repo).unwrap();
    let (nodes_p, _) = write_csvs(&g, &repo.join("out")).unwrap();
    let text = std::fs::read_to_string(&nodes_p).unwrap();
    assert!(text.contains("\"weird,\"\"name\"\".rs\""), "{text}");
}

#[test]
fn no_communities_case_renders_empty_fields() {
    let repo = fixture_repo("nocomm");
    let db = repo.join(".code-review-graph").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = repo.join("pkg/a/lonely.rs").to_string_lossy().into_owned();
    let mut spec = crg_fixture::CrgDbSpec::default();
    spec.nodes.push(crg_fixture::NodeSeed {
        name: "lonely".into(),
        parent: None,
        qname: format!("{abs}::File"),
        file_path: abs.clone(),
    });
    spec.node_attrs.push((
        format!("{abs}::File"),
        crg_fixture::NodeAttr {
            kind: "File",
            language: "rust",
            is_test: 0,
            community_id: None,
        },
    ));
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    let g = load(&db, &repo).unwrap();
    assert_eq!(g.nodes.len(), 1);
    assert_eq!(g.nodes[0].community, None);
    let (nodes_p, _) = write_csvs(&g, &repo.join("out")).unwrap();
    let text = std::fs::read_to_string(&nodes_p).unwrap();
    assert!(text.contains("lonely,,,rust,0,0\r\n"), "{text}");
}

#[test]
fn cli_ok_line_and_missing_db_crash() {
    let repo = fixture_repo("cli");
    build_db(&repo);
    let out = run(&[
        "graph_csv",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join("csv").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "[OK] graph csv: 2 nodes / 2 links -> graph-nodes.csv + graph-links.csv（{}）\n",
            repo.join("csv").display()
        )
    );
    let out = run(&["graph_csv", "--repo", "/tmp"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("graph.db 不存在"), "{}", out.stderr);
    let out = run(&["graph_csv", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out
        .stdout
        .starts_with("usage: graph_csv [-h] [--repo REPO] [--out-dir OUT_DIR]\n"));
}
