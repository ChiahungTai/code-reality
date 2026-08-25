//! R5-S4 chain_tour tests — mirrors of test_chain_tour case families on
//! synthetic markdown + crg_db fixtures.

mod crg_fixture;

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
    std::fs::create_dir_all(repo.join(".code-review-graph")).unwrap();
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
            .arg("-C").arg(&repo).args(args)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .status().unwrap();
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
    let db = repo.join(".code-review-graph").join("graph.db");
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = crg_fixture::CrgDbSpec::default();
    // kernel at graph line 1 (same), boot at graph line 9 (moved +4)
    for (name, qname, line) in [
        ("kernel", "pkg/a.py::kernel", 1),
        ("boot", "pkg/a.py::boot", 9),
        ("load_config", "pkg/cfg.py::load_config", 2),
    ] {
        let file = if name == "load_config" { abs("pkg/cfg.py") } else { abs("pkg/a.py") };
        spec.nodes.push(crg_fixture::NodeSeed {
            name: name.into(),
            parent: None,
            qname: qname.into(),
            file_path: file,
        });
        spec.node_attrs.push((
            qname.into(),
            crg_fixture::NodeAttr {
                kind: "Function",
                language: "python",
                is_test: 0,
                community_id: None,
            },
        ));
        spec.node_lines.push((qname.into(), line));
    }
    crg_fixture::make_crg_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let g0 = st.g_counts.get("same").copied().unwrap_or(0);
    let g1 = st.g_counts.get("moved").copied().unwrap_or(0);
    assert!(g0 >= 2, "{:?}", st.g_counts);
    assert!(g1 >= 1, "{:?}", st.g_counts);
    // moved step: line re-anchored to graph line, description carries delta
    let t0 = st.tours[0].as_object().unwrap();
    let steps = t0["steps"].as_array().unwrap();
    let boot_step = steps.iter().find(|s| s["title"].as_str().unwrap().contains("boot")).unwrap();
    assert_eq!(boot_step["line"], 9);
    assert!(boot_step["description"].as_str().unwrap().contains("graph +4"), "{}", boot_step);
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
    assert!(out.stdout.contains("[OK] chain tours: 2 場景 / 6 幀 / 5 步 / skipped 1"), "{}", out.stdout);
    assert!(out.stdout.contains("manifest skip: out-dir 不在 .tours/ 樹內"), "{}", out.stdout);
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
