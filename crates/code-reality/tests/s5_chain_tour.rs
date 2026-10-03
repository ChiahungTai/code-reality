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
    // namespaced CR identity (AIR-80): true SCIP symbol of the selected
    // definition occurrence (fixture universe: symbol == qname)
    assert_eq!(
        boot_step["x-codeReality"]["symbol"].as_str().unwrap(),
        "pkg/a.py::boot"
    );
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
    let mut warns = Vec::new();
    let paths = write_tours(&st, &out_dir, &primary, &mut warns).unwrap();
    assert_eq!(paths.len(), 2);
    // degenerate pin (CR-2): out_dir basename carries no family number →
    // legacy `{SS} - heading` titles survive verbatim + the fix-naming WARN
    let t1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    assert_eq!(t1["title"], "01 - Boot scenario");
    assert_eq!(t1["isPrimary"], serde_json::Value::Bool(true));
    let t2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("02.tour")).unwrap()).unwrap();
    assert_eq!(t2["title"], "02 - Second scenario");
    assert!(t2.get("isPrimary").is_none());
    assert!(
        warns
            .iter()
            .any(|w| w.contains("退化") && w.contains("NN-label")),
        "{warns:?}"
    );
    // degenerate-path pin (CR-2 leg-3 primed-F2): the no-NN fold emits
    // exactly one WARN — future additions to this path must show up here
    assert_eq!(warns.len(), 1, "{warns:?}");
}

// ---------- CR-2: family-numbered titles (`NN - label｜heading`) ----------

fn plant_family_manifest(repo: &std::path::Path, toml: &str) {
    let root = repo.join(".tours");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("manifest.toml"), toml).unwrap();
}

fn tour_title(p: &std::path::Path) -> String {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v["title"].as_str().unwrap().to_string()
}

#[test]
fn write_tours_family_template_manifest_label() {
    let (repo, chain) = repo_fixture("fam-label");
    let st = build_tours(&chain, &repo, None).unwrap();
    plant_family_manifest(&repo, "version = 1\n\n[family.\"01\"]\nlabel = \"auth\"\n");
    let out_dir = repo.join(".tours/arch/01-auth");
    let mut warns = Vec::new();
    let paths = write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(paths.len(), 2);
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - auth｜Boot scenario"
    );
    assert_eq!(
        tour_title(&out_dir.join("02.tour")),
        "01 - auth｜Second scenario"
    );
}

#[test]
fn write_tours_family_template_multi_family() {
    let (repo, chain) = repo_fixture("fam-multi");
    let st = build_tours(&chain, &repo, None).unwrap();
    plant_family_manifest(
        &repo,
        "version = 1\n\n[family.\"01\"]\nlabel = \"auth\"\n\n[family.\"02\"]\nlabel = \"runtime\"\n",
    );
    let mut warns = Vec::new();
    write_tours(
        &st,
        &repo.join(".tours/arch/01-auth"),
        &Default::default(),
        &mut warns,
    )
    .unwrap();
    write_tours(
        &st,
        &repo.join(".tours/arch/02-runtime"),
        &Default::default(),
        &mut warns,
    )
    .unwrap();
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(
        tour_title(&repo.join(".tours/arch/01-auth/01.tour")),
        "01 - auth｜Boot scenario"
    );
    assert_eq!(
        tour_title(&repo.join(".tours/arch/02-runtime/01.tour")),
        "02 - runtime｜Boot scenario"
    );
}

#[test]
fn write_tours_family_template_basename_suffix_label() {
    let (repo, chain) = repo_fixture("fam-suffix");
    let st = build_tours(&chain, &repo, None).unwrap();
    let out_dir = repo.join(".tours/arch/01-runtime");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - runtime｜Boot scenario"
    );
}

#[test]
fn write_tours_label_dual_source_conflict_fails_loud() {
    let (repo, chain) = repo_fixture("fam-conflict");
    let st = build_tours(&chain, &repo, None).unwrap();
    plant_family_manifest(
        &repo,
        "version = 1\n\n[family.\"01\"]\nlabel = \"runtime\"\n",
    );
    let out_dir = repo.join(".tours/arch/01-auth");
    let mut warns = Vec::new();
    let err = write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap_err();
    assert!(
        err.contains("雙源不一致") && err.contains("auth") && err.contains("runtime"),
        "{err}"
    );
    // CLI face: the drift aborts generation with a nonzero exit BEFORE
    // any tour file lands
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("雙源不一致"), "{}", out.stderr);
    assert!(!out_dir.join("01.tour").exists());
}

#[test]
fn write_tours_label_absent_folds_with_warn() {
    let (repo, chain) = repo_fixture("fam-fold");
    let st = build_tours(&chain, &repo, None).unwrap();
    let out_dir = repo.join(".tours/arch/01");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(tour_title(&out_dir.join("01.tour")), "01 - Boot scenario");
    assert_eq!(warns.len(), 1, "{warns:?}");
    assert!(warns[0].contains("收摺"), "{}", warns[0]);
}

// ---------- CR-2 leg-3 (F1): manifest corruption faces tell the truth ----------

#[test]
fn write_tours_unreadable_manifest_warns_truthfully() {
    let (repo, chain) = repo_fixture("fam-unreadable");
    let st = build_tours(&chain, &repo, None).unwrap();
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    // syntax-broken manifest: present but unparseable
    std::fs::write(
        repo.join(".tours/manifest.toml"),
        "version = 1\n[family.\"01\" broken\n",
    )
    .unwrap();
    let out_dir = repo.join(".tours/arch/01");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    // the fold itself is legal (parse failure folds to no label source)...
    assert_eq!(tour_title(&out_dir.join("01.tour")), "01 - Boot scenario");
    // ...but the WARN must name the real cause — NOT 雙源皆缺
    assert_eq!(warns.len(), 1, "{warns:?}");
    assert!(warns[0].contains("manifest 不可讀"), "{warns:?}");
    assert!(warns[0].contains("TOML 解析失敗"), "{warns:?}");
    assert!(!warns[0].contains("皆缺"), "{warns:?}");
}

#[test]
fn write_tours_malformed_label_type_warns_truthfully() {
    let (repo, chain) = repo_fixture("fam-badlabel");
    let st = build_tours(&chain, &repo, None).unwrap();
    // label present but not a string — parseable manifest, wrong shape
    plant_family_manifest(&repo, "version = 1\n\n[family.\"01\"]\nlabel = 7\n");
    // suffix `auth` present: the readable source stands, but the WARN
    // must disclose the wrong-typed manifest label instead of silence
    let out_dir = repo.join(".tours/arch/01-auth");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - auth｜Boot scenario"
    );
    assert_eq!(warns.len(), 1, "{warns:?}");
    // C3 (leg-4): the WARN enumerates Malformed's causes instead of
    // asserting the label-type one it cannot know
    assert!(warns[0].contains("結構型別錯"), "{warns:?}");
    assert!(warns[0].contains("label 非 string"), "{warns:?}");
    assert!(!warns[0].contains("皆缺"), "{warns:?}");
}

#[test]
fn run_corrupted_manifest_fails_loud_without_overwrite() {
    let (repo, chain) = repo_fixture("fam-corrupt-run");
    let mpath = repo.join(".tours/manifest.toml");
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let bad = "version = 1\n[tour.\"arch/01-auth/01.tour\"\n";
    std::fs::write(&mpath, bad).unwrap();
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join(".tours/arch/01-auth").to_string_lossy(),
    ]);
    // crash-only (F1b): upserting onto an unparseable manifest would
    // dump a full overwrite and silently evaporate every existing row
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("拒絕"), "{}", out.stderr);
    // the corrupted bytes survive verbatim — dump never ran
    assert_eq!(std::fs::read_to_string(&mpath).unwrap(), bad);
}

// ---------- CR-2 leg-4: C1 preflight / R1 e2e / C3 truthful WARN / R2 warns ----------

#[test]
fn run_corrupted_manifest_preflight_blocks_tour_writes() {
    let (repo, chain) = repo_fixture("leg4-preflight");
    std::fs::create_dir_all(repo.join(".tours/arch/01-auth")).unwrap();
    // a tour from an earlier run: its bytes must survive a refused
    // regen verbatim — a partial regen (files written, provenance leg
    // crashed) is the C1 defect
    let existing = repo.join(".tours/arch/01-auth/01.tour");
    std::fs::write(&existing, "{\"title\":\"old\"}\n").unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    let bad = "version = 1\n[tour.\"arch/01-auth/01.tour\"\n";
    std::fs::write(&mpath, bad).unwrap();
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join(".tours/arch/01-auth").to_string_lossy(),
    ]);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("拒絕"), "{}", out.stderr);
    // zero partial state: old tour untouched, no new files, corrupt
    // manifest bytes verbatim
    assert_eq!(
        std::fs::read_to_string(&existing).unwrap(),
        "{\"title\":\"old\"}\n"
    );
    assert!(!repo.join(".tours/arch/01-auth/02.tour").exists());
    assert_eq!(std::fs::read_to_string(&mpath).unwrap(), bad);
}

#[test]
fn run_wrong_typed_known_key_manifest_crashes_without_overwrite() {
    let (repo, chain) = repo_fixture("leg4-r1-e2e");
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    // parseable TOML, wrong-typed known key (R1's loud face)
    let bad = "version = 1\ntour = 7\n";
    std::fs::write(&mpath, bad).unwrap();
    let out_dir = repo.join(".tours/arch/01-auth");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(
        out.exit_code, 1,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("tour 非 table") && out.stderr.contains("integer"),
        "{}",
        out.stderr
    );
    assert_eq!(std::fs::read_to_string(&mpath).unwrap(), bad);
    assert!(!out_dir.join("01.tour").exists());
}

#[test]
fn write_tours_malformed_family_key_not_table_warns_truthfully() {
    let (repo, chain) = repo_fixture("leg4-fam-nontable");
    let st = build_tours(&chain, &repo, None).unwrap();
    // `family` itself is an integer — Malformed cause #1; no dir suffix
    // → the fold branch, which used to claim 「label 型別錯（非 string）」
    // — a cause it cannot know
    plant_family_manifest(&repo, "version = 1\nfamily = 7\n");
    let out_dir = repo.join(".tours/arch/01");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(tour_title(&out_dir.join("01.tour")), "01 - Boot scenario");
    assert_eq!(warns.len(), 1, "{warns:?}");
    assert!(warns[0].contains("結構型別錯"), "{warns:?}");
    assert!(warns[0].contains("family 非 table"), "{warns:?}");
    assert!(!warns[0].contains("label 型別錯（非 string）"), "{warns:?}");
}

#[test]
fn write_tours_malformed_nn_row_not_table_warns_truthfully() {
    let (repo, chain) = repo_fixture("leg4-row-nontable");
    let st = build_tours(&chain, &repo, None).unwrap();
    // `[family."01"]` row is an integer — Malformed cause #2; dir suffix
    // present → the suffix branch
    plant_family_manifest(&repo, "version = 1\n\n[family]\n\"01\" = 7\n");
    let out_dir = repo.join(".tours/arch/01-auth");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - auth｜Boot scenario"
    );
    assert_eq!(warns.len(), 1, "{warns:?}");
    assert!(warns[0].contains("結構型別錯"), "{warns:?}");
    assert!(warns[0].contains("NN 列非 table"), "{warns:?}");
    assert!(!warns[0].contains("label 型別錯（非 string）"), "{warns:?}");
}

#[test]
fn write_tours_legacy_residue_warns_via_warns_vec() {
    let (repo, chain) = repo_fixture("leg4-legacy-warn");
    let st = build_tours(&chain, &repo, None).unwrap();
    let out_dir = repo.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("chain-old.tour"), "{}\n").unwrap();
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    // the residue WARN must ride the warns vec (run() routes it to
    // stderr; in-process consumers see it) — not a bare eprintln
    assert!(
        warns.iter().any(|w| w.contains("舊檔名格式殘留")),
        "{warns:?}"
    );
}

#[test]
fn write_tours_label_ascii_hyphen_replaced_fullwidth() {
    let (repo, chain) = repo_fixture("fam-hyphen-label");
    let st = build_tours(&chain, &repo, None).unwrap();
    plant_family_manifest(
        &repo,
        "version = 1\n\n[family.\"01\"]\nlabel = \"auth-chain\"\n",
    );
    // bare `01` dir: no basename suffix → the manifest label stands alone
    let out_dir = repo.join(".tours/arch/01");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - auth－chain｜Boot scenario"
    );
    assert!(warns.iter().any(|w| w.contains("－")), "{warns:?}");
}

#[test]
fn write_tours_heading_hyphen_kept_with_truncation_key_warn() {
    let (repo, _chain) = repo_fixture("fam-hyphen-heading");
    let chain = repo.join("chain-h.md");
    std::fs::write(
        &chain,
        "# Boot - cold start\n\n```text\nkernel (pkg/a.py:1)\n└─ boot() (pkg/a.py:5)\n```\n",
    )
    .unwrap();
    let st = build_tours(&chain, &repo, None).unwrap();
    plant_family_manifest(&repo, "version = 1\n\n[family.\"01\"]\nlabel = \"auth\"\n");
    let out_dir = repo.join(".tours/arch/01");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    // heading is author prose: kept verbatim, hyphen included
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "01 - auth｜Boot - cold start"
    );
    // the WARN names the effective truncation key under upstream
    // getTourTitle split semantics ("label｜pre-hyphen heading")
    assert!(
        warns
            .iter()
            .any(|w| w.contains("有效截斷鍵") && w.contains("auth｜Boot")),
        "{warns:?}"
    );
}

#[test]
fn write_tours_three_digit_family_nn() {
    let (repo, chain) = repo_fixture("fam-100");
    let st = build_tours(&chain, &repo, None).unwrap();
    let out_dir = repo.join(".tours/arch/100-platform");
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &Default::default(), &mut warns).unwrap();
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(
        tour_title(&out_dir.join("01.tour")),
        "100 - platform｜Boot scenario"
    );
}

fn run_bin(cwd: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_code-reality"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn regen_redirect_titles_carry_target_family_nn() {
    let (repo, _chain) = repo_fixture("fam-redirect");
    // an existing numbered family already sourced from chain.md — the
    // dup-family guard must redirect the default out-dir there and the
    // titles must carry THAT family's number + suffix label
    std::fs::create_dir_all(repo.join(".tours/arch/01-alpha-chain")).unwrap();
    std::fs::write(
        repo.join(".tours/manifest.toml"),
        "version = 1\n\n[tour.\"arch/01-alpha-chain/01.tour\"]\ngenerator = \"chain_tour\"\nsources = [\"chain.md\"]\nanchored_commit = \"c0\"\n",
    )
    .unwrap();
    let (code, out, err) = run_bin(&repo, &["chain_tour", "chain.md", "--repo", "."]);
    assert_eq!(code, 0, "stdout={out} stderr={err}");
    assert!(out.contains("duplicate-family 防護"), "{out}");
    // basename label `alpha-chain` carries an ASCII hyphen → sanitized to
    // fullwidth (CR-2 decision ⑤) in the title slot
    assert_eq!(
        tour_title(&repo.join(".tours/arch/01-alpha-chain/01.tour")),
        "01 - alpha－chain｜Boot scenario"
    );
}

#[test]
fn run_no_nn_out_dir_degrades_with_stderr_warn() {
    let (repo, chain) = repo_fixture("fam-degen-run");
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
        out.stderr.contains("退化") && out.stderr.contains("--out-dir"),
        "{}",
        out.stderr
    );
    assert_eq!(tour_title(&out_dir.join("01.tour")), "01 - Boot scenario");
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
