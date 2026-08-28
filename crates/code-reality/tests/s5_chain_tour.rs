//! R5-S4 chain_tour tests — mirrors of test_chain_tour case families on
//! synthetic markdown + graph_db fixtures.

use code_reality::chain_tour::{
    best_ident, build_tours, parse_blocks, parse_frames, prefix_len, write_tours,
};
use std::path::PathBuf;

const CHAIN_MD: &str = r#"# Boot scenario

Intro prose.

```text
kernel (pkg/a.py:1)
├─ boot() (pkg/a.py:5)  # entry
│  └─ load_config() (pkg/cfg.py:2)
└─ external tool (https://x)
```

# Second scenario

```text
solo_frame (pkg/a.py:10)
└─ helper() (pkg/cfg.py:6)
```

Plain code block without tree frames:

```python
x = 1
```
"#;

fn repo_fixture(tag: &str) -> (PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("pkg/a.py"),
        "def kernel():\n    pass\n\n\ndef boot():\n    load_config()\n\n\ndef solo_frame():\n    pass\n",
    )
    .unwrap();
    std::fs::write(repo.join("pkg/cfg.py"), "def load_config():\n    pass\n").unwrap();
    let chain = repo.join("chain.md");
    std::fs::write(&chain, CHAIN_MD).unwrap();
    let g = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
    };
    g(&["init", "-q"]);
    g(&["add", "."]);
    g(&["commit", "-qm", "init"]);
    (repo, chain)
}

#[test]
fn prefix_and_blocks_and_frames() {
    assert_eq!(prefix_len("│   ├─ x"), 7);
    let blocks = parse_blocks(CHAIN_MD);
    assert_eq!(blocks.len(), 2); // tree blocks only
    assert_eq!(blocks[0].0, "Boot scenario");
    let frames = parse_frames(&blocks[0].1);
    // depth via stack: kernel=0, boot=1, load_config=2, external=1
    assert_eq!(frames[0].depth, 0);
    assert_eq!(frames[1].depth, 1);
    assert_eq!(frames[2].depth, 2);
    assert_eq!(frames[3].depth, 1);
    assert_eq!(frames[1].note, "entry");
    assert_eq!(frames[1].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[1].line, Some(5));
    assert_eq!(frames[0].ident, "kernel");
    // titles keep the tree prefix
    assert!(!frames[1].prefix.is_empty()); // tree prefix captured
}

#[test]
fn best_ident_call_position_wins() {
    assert_eq!(best_ident("kernel()"), "kernel");
    assert_eq!(best_ident("obj.method()"), "method");
    assert_eq!(best_ident("mod.sub.long_name"), "long_name");
    // Python oracle: the regex grabs the longest bare word
    assert_eq!(best_ident("no idents here 123!"), "idents");
}

#[test]
fn build_tours_skips_external_and_anchors() {
    let (repo, chain) = repo_fixture("build");
    let st = build_tours(&chain, &repo, None).unwrap();
    assert_eq!(st.tours.len(), 2);
    assert_eq!(st.frames, 6);
    assert_eq!(st.skipped, 1); // the external https frame
    let t0 = st.tours[0].as_object().unwrap();
    let desc = t0["description"].as_str().unwrap();
    assert!(desc.contains("4 幀 → 3 步；1 幀跳過"), "{desc}");
    assert!(desc.contains("noref 1"), "{desc}"); // no path ref in the frame → noref
                                                 // steps anchored at doc lines (same-file, no graph)
    let steps = t0["steps"].as_array().unwrap();
    assert_eq!(steps[0]["file"], "pkg/a.py");
    assert_eq!(steps[0]["line"], 1);
    assert!(steps[0].get("pattern").is_some()); // kernel() line is non-blank
}

#[test]
fn build_tours_with_graph_reanchor() {
    let (repo, chain) = repo_fixture("graph");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    // kernel at graph line 1 (same), boot at graph line 9 (moved +4)
    for (name, qname, line) in [
        ("kernel", "pkg/a.py::kernel", 1),
        ("boot", "pkg/a.py::boot", 9),
        ("load_config", "pkg/cfg.py::load_config", 2),
    ] {
        let file = if name == "load_config" {
            abs("pkg/cfg.py")
        } else {
            abs("pkg/a.py")
        };
        spec.nodes.push(graph_db_fixture::NodeSeed {
            name: name.into(),
            parent: None,
            qname: qname.into(),
            file_path: file,
        });
        spec.node_attrs.push((
            qname.into(),
            graph_db_fixture::NodeAttr {
                kind: "Function",
                language: "python",
                is_test: 0,
                community_id: None,
            },
        ));
        spec.node_lines.push((qname.into(), line));
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let g0 = st.g_counts.get("same").copied().unwrap_or(0);
    let g1 = st.g_counts.get("moved").copied().unwrap_or(0);
    assert!(g0 >= 2, "{:?}", st.g_counts);
    assert!(g1 >= 1, "{:?}", st.g_counts);
    // moved step: line re-anchored to graph line, description carries delta
    let t0 = st.tours[0].as_object().unwrap();
    let steps = t0["steps"].as_array().unwrap();
    let boot_step = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("boot"))
        .unwrap();
    assert_eq!(boot_step["line"], 9);
    assert!(
        boot_step["description"]
            .as_str()
            .unwrap()
            .contains("graph +4"),
        "{}",
        boot_step
    );
}

#[test]
fn write_tours_nn_prefix_and_primary() {
    let (repo, chain) = repo_fixture("write");
    let st = build_tours(&chain, &repo, None).unwrap();
    let out_dir = repo.join(".tours").join("arch").join("chain");
    let mut primary = std::collections::BTreeSet::new();
    primary.insert(1);
    let paths = write_tours(&st, &out_dir, &primary).unwrap();
    assert_eq!(paths.len(), 2);
    let t1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    assert_eq!(t1["title"], "01 - Boot scenario");
    assert_eq!(t1["isPrimary"], serde_json::Value::Bool(true));
    let t2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("02.tour")).unwrap()).unwrap();
    assert_eq!(t2["title"], "02 - Second scenario");
    assert!(t2.get("isPrimary").is_none());
}

#[test]
fn cli_faces() {
    let out = code_reality::chain_tour::run(&["chain_tour", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("usage: chain_tour [-h]"));
    let (repo, chain) = repo_fixture("cli");
    let out_dir = repo.join("out");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("[OK] chain tours: 2 場景 / 6 幀 / 5 步 / skipped 1"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("manifest skip: out-dir 不在 .tours/ 樹內"),
        "{}",
        out.stdout
    );
    // primary out of range crashes loudly
    let out2 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
        "--primary",
        "9",
    ]);
    assert_eq!(out2.exit_code, 1);
    assert!(out2.stderr.contains("--primary 越界"), "{}", out2.stderr);
}

// ---------- duplicate-family guard + pattern freshness (mosaic relay 2026-08-26) ----------

use code_reality::chain_tour::dup_family_decision;
use code_reality::tour_manifest::{upsert, Manifest};

fn manifest_with_alias_family() -> Manifest {
    let mut m = Manifest::default();
    upsert(
        &mut m,
        "arch/01-alpha-chain/01.tour",
        "chain_tour",
        &["chain.md".into()],
        "c0",
    );
    m
}

#[test]
fn dup_family_decision_redirect_semantics() {
    let m = manifest_with_alias_family();
    // default out-dir (not explicit) targeting a different family -> redirect
    let d = dup_family_decision(&m, "arch/chain", "chain.md", false).unwrap();
    assert!(d.redirect);
    // numbered family name preserved verbatim (no rename)
    assert_eq!(d.fam, "arch/01-alpha-chain");
    // explicit --out-dir wins -> warn-only
    let d = dup_family_decision(&m, "arch/chain", "chain.md", true).unwrap();
    assert!(!d.redirect);
    // same family -> no decision
    assert!(dup_family_decision(&m, "arch/01-alpha-chain", "chain.md", false).is_none());
    // different source -> no decision
    assert!(dup_family_decision(&m, "arch/chain", "other.md", false).is_none());
}

#[test]
fn regen_explicit_out_dir_dup_warns_both_families() {
    let (repo, chain) = repo_fixture("dup-explicit");
    let alias = repo.join(".tours/arch/alias-family");
    let run1 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &alias.to_string_lossy(),
    ]);
    assert_eq!(run1.exit_code, 0, "{}{}", run1.stdout, run1.stderr);
    let stem = repo.join(".tours/arch/chain");
    let run2 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &stem.to_string_lossy(),
    ]);
    assert_eq!(run2.exit_code, 0, "{}{}", run2.stdout, run2.stderr);
    assert!(
        run2.stdout.contains("duplicate-family") && run2.stdout.contains("alias-family"),
        "{}",
        run2.stdout
    );
    // both families exist (explicit wins — rename-migration path)
    assert!(alias.join("01.tour").exists());
    assert!(stem.join("01.tour").exists());
}

#[test]
fn regen_orphan_source_family_warns() {
    let (repo, chain) = repo_fixture("orphan");
    let alias = repo.join(".tours/arch/alias-family");
    let run1 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &alias.to_string_lossy(),
    ]);
    assert_eq!(run1.exit_code, 0, "{}{}", run1.stdout, run1.stderr);
    // plant a family sourced from a nonexistent md (rename leftover)
    std::fs::create_dir_all(repo.join(".tours/arch/ghost-family")).unwrap();
    std::fs::write(
        repo.join(".tours/manifest.toml"),
        "version = 1\n\n[tour.\"arch/ghost-family/01.tour\"]\ngenerator = \"chain_tour\"\nsources = [\"chain-gone.md\"]\nanchored_commit = \"c0\"\n",
    )
    .unwrap();
    let run2 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &alias.to_string_lossy(),
    ]);
    assert_eq!(run2.exit_code, 0, "{}{}", run2.stdout, run2.stderr);
    assert!(
        run2.stdout.contains("source md 已不存在") && run2.stdout.contains("ghost-family"),
        "{}",
        run2.stdout
    );
}

#[test]
fn regen_refreshes_pattern_when_same_line_content_changes() {
    let (repo, chain) = repo_fixture("pattern-fresh");
    let out_dir = repo.join(".tours/arch/chain");
    let run1 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(run1.exit_code, 0, "{}{}", run1.stdout, run1.stderr);
    let p1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let pat1 = p1["steps"][0]["pattern"].as_str().unwrap();
    assert!(pat1.contains("kernel\\("), "{pat1}");
    // signature change WITHOUT line movement (the mosaic 8e92d957 shape)
    std::fs::write(
        repo.join("pkg/a.py"),
        "def kernel(x):\n    pass\n\n\ndef boot():\n    load_config()\n\n\ndef solo_frame():\n    pass\n",
    )
    .unwrap();
    let run2 = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(run2.exit_code, 0, "{}{}", run2.stdout, run2.stderr);
    let p2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let pat2 = p2["steps"][0]["pattern"].as_str().unwrap();
    assert!(
        pat2.contains("kernel\\(x\\)"),
        "pattern must refresh from source: {pat2}"
    );
    assert!(
        !pat2.contains("kernel\\()"),
        "stale shape must be gone: {pat2}"
    );
}

// ---------- S1 cutover: default db = self-owned .code-reality ----------

mod graph_db_fixture;

use code_reality::chain_tour::GraphAnchor;
use code_reality::graph_db;

fn owned_db(repo: &std::path::Path) -> std::path::PathBuf {
    repo.join(".code-reality/graph.db")
}

#[test]
fn consumer_db_defaults_to_self_owned() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_unresolved = tmp.path().join("repo");
    std::fs::create_dir_all(repo_unresolved.join(".code-reality")).unwrap();
    let repo = std::fs::canonicalize(&repo_unresolved).unwrap();
    graph_db_fixture::make_graph_db(
        &owned_db(&repo),
        &graph_db_fixture::GraphDbSpec {
            nodes: vec![graph_db_fixture::NodeSeed {
                name: "target".into(),
                qname: "s1::target".into(),
                file_path: format!("{}/src/a.rs", repo.display()),
                parent: None,
            }],
            node_lines: vec![("s1::target".into(), 4)],
            ..Default::default()
        },
    )
    .unwrap();
    // owned db present -> db + no warns
    let (db, warns) = graph_db::consumer_db(&repo);
    assert_eq!(db, Some(owned_db(&repo)));
    assert!(warns.is_empty(), "owned db in repo: {warns:?}");

    // no owned db -> None + missing-db warn with build guidance
    let repo2 = tmp.path().join("repo2");
    std::fs::create_dir_all(&repo2).unwrap();
    let (db, warns) = graph_db::consumer_db(&repo2);
    assert_eq!(db, None);
    assert!(
        warns.iter().any(|w| w.contains("graph_db build")),
        "missing-db warn guides build: {warns:?}"
    );
}

#[test]
fn anchor_tiebreak_is_deterministic_on_wider_candidate_set() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_unresolved = tmp.path().join("repo");
    std::fs::create_dir_all(repo_unresolved.join(".code-reality")).unwrap();
    let repo = std::fs::canonicalize(&repo_unresolved).unwrap();
    let file = format!("{}/src/a.rs", repo.display());
    // two same-name nodes equidistant from line 10 (8 and 12) — the new
    // universe allows (name, file) duplicates; the pick must not depend
    // on rowid
    graph_db_fixture::make_graph_db(
        &owned_db(&repo),
        &graph_db_fixture::GraphDbSpec {
            nodes: vec![
                graph_db_fixture::NodeSeed {
                    name: "dup".into(),
                    qname: "s1::dup@12".into(),
                    file_path: file.clone(),
                    parent: None,
                },
                graph_db_fixture::NodeSeed {
                    name: "dup".into(),
                    qname: "s1::dup@8".into(),
                    file_path: file.clone(),
                    parent: None,
                },
            ],
            node_lines: vec![("s1::dup@12".into(), 12), ("s1::dup@8".into(), 8)],
            ..Default::default()
        },
    )
    .unwrap();
    let anchor = GraphAnchor::new(&owned_db(&repo), &repo).unwrap();
    let hit = anchor.anchor(&repo.join("src/a.rs"), 10, "dup", "exact");
    assert_eq!(hit.g_line, Some(8), "tie broken by lower line_start");
}

#[test]
fn graph_anchor_rejects_legacy_schema_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let legacy = repo.join("legacy.db");
    // minimal wrong-schema db (nodes without the symbol column — the
    // CRG-era shape); the probe must fail loud
    {
        let c = rusqlite::Connection::open(&legacy).unwrap();
        c.execute_batch("CREATE TABLE nodes (qualified_name TEXT)")
            .unwrap();
    }
    let err = match GraphAnchor::new(&legacy, &repo) {
        Err(e) => e,
        Ok(_) => panic!("legacy schema must be rejected"),
    };
    assert!(
        err.contains("非自有格式"),
        "legacy-schema --graph must fail loud, got: {err}"
    );
}
