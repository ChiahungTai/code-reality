//! R5-S4 chain_tour tests — mirrors of test_chain_tour case families on
//! synthetic markdown + graph_db fixtures.

use code_reality::chain_tour::{
    best_ident, build_tours, parse_blocks, parse_frames, prefix_len, write_tours,
};
use code_reality::language::{LanguageFace, JS_TS_EXTS};
use std::path::{Path, PathBuf};

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
    // CR-9: best_ident returns (last segment, full dotted winning token)
    assert_eq!(best_ident("kernel()"), ("kernel".into(), "kernel".into()));
    assert_eq!(
        best_ident("obj.method()"),
        ("method".into(), "obj.method".into())
    );
    assert_eq!(
        best_ident("mod.sub.long_name"),
        ("long_name".into(), "mod.sub.long_name".into())
    );
    // Python oracle: the regex grabs the longest bare word
    assert_eq!(
        best_ident("no idents here 123!"),
        ("idents".into(), "idents".into())
    );
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

// --- CR-7 symbol-only (line-less) anchors ---

const CHAIN_MD_LINELESS: &str = r#"# Boot scenario lineless

```text
kernel (pkg/a.py)
├─ boot() (pkg/a.py)  # entry
│  └─ load_config() (pkg/cfg.py)
└─ helper() (pkg/a.py)
└─ ghost() (pkg/a.py)
```
"#;

const CHAIN_MD_MIXED: &str = r#"# Mixed scenario

```text
kernel (pkg/a.py:1)
└─ boot() (pkg/a.py)
```
"#;

/// a.py: kernel@1, boot@5, helper@9; cfg.py: load_config@1.
fn repo_fixture_lineless(tag: &str) -> (PathBuf, PathBuf) {
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
        "def kernel():\n    pass\n\n\ndef boot():\n    load_config()\n\n\ndef helper():\n    pass\n",
    )
    .unwrap();
    std::fs::write(repo.join("pkg/cfg.py"), "def load_config():\n    pass\n").unwrap();
    let chain = repo.join("chain.md");
    std::fs::write(&chain, CHAIN_MD_LINELESS).unwrap();
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

fn seed_node(
    spec: &mut graph_db_fixture::GraphDbSpec,
    name: &str,
    qname: &str,
    file: &str,
    line: i64,
    kind: &'static str,
    abs: &dyn Fn(&str) -> String,
) {
    spec.nodes.push(graph_db_fixture::NodeSeed {
        name: name.into(),
        parent: None,
        qname: qname.into(),
        file_path: abs(file),
    });
    spec.node_attrs.push((
        qname.into(),
        graph_db_fixture::NodeAttr {
            kind,
            language: "python",
            is_test: 0,
            community_id: None,
        },
    ));
    spec.node_lines.push((qname.into(), line));
}

#[test]
fn lineless_tail_grammar() {
    // positive: wrapped tail with note
    let frames = parse_frames(&["└─ boot() (pkg/a.py)  # entry".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[0].line, None);
    assert_eq!(frames[0].ident, "boot");
    assert_eq!(frames[0].note, "entry");
    // positive: bare tail without wrapper
    let frames = parse_frames(&["└─ boot pkg/a.py".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[0].line, None);
    // the symbol remainder (not the path) becomes the symbol text
    let frames = parse_frames(&["└─ boot() (pkg/a.py)".to_string()]);
    assert_eq!(frames[0].symbol, "boot()");
    // negatives: suffix junk / residual :xxx / URL boundary / two path
    // tokens — none may be captured as a line-less anchor (noref stays)
    for frame in [
        "└─ x (pkg/a.pyc)",
        "└─ x (pkg/a.py.bak)",
        "└─ x (pkg/a.py:abc)",
        "└─ x (https://host/foo.py)",
        "└─ x pkg/a.py extra (pkg/cfg.py)",
    ] {
        let frames = parse_frames(&[frame.to_string()]);
        assert_eq!(frames[0].path, None, "{frame}");
        assert_eq!(frames[0].line, None, "{frame}");
    }
    // explicit anchors keep their byte-exact behavior
    let frames = parse_frames(&["└─ boot() (pkg/a.py:5)  # entry".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[0].line, Some(5));
}

#[test]
fn build_tours_lineless_fill_and_miss() {
    let (repo, chain) = repo_fixture_lineless("lineless-fill");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    seed_node(&mut spec, "boot", "pkg/a.py::boot", "pkg/a.py", 5, "Function", &abs);
    seed_node(
        &mut spec,
        "load_config",
        "pkg/cfg.py::load_config",
        "pkg/cfg.py",
        1,
        "Function",
        &abs,
    );
    seed_node(&mut spec, "helper", "pkg/a.py::helper", "pkg/a.py", 9, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    // 5 frames: 4 unique graph hits filled, ghost skipped (not-in-graph)
    assert_eq!(st.g_counts.get("filled"), Some(&4), "{:?}", st.g_counts);
    assert_eq!(
        st.g_counts.get("not-in-graph"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    let t = st.tours[0].as_object().unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    assert_eq!(st.skipped, 1);
    assert!(t["description"].as_str().unwrap().contains("noanchor 1"));
    let boot = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("boot"))
        .unwrap();
    assert_eq!(boot["line"], 5);
    assert!(boot["description"].as_str().unwrap().contains("graph 補行號"));
    assert_eq!(
        boot["x-codeReality"]["symbol"].as_str().unwrap(),
        "pkg/a.py::boot"
    );
    // every emitted step carries a real pattern (validator exact-match
    // ingredient — no line=1 pseudo-coordinates anywhere)
    for s in steps {
        assert!(s["line"].as_i64().unwrap() >= 1);
        assert!(s.get("pattern").is_some(), "{s}");
    }
}

#[test]
fn build_tours_lineless_requires_graph() {
    let (repo, chain) = repo_fixture_lineless("lineless-nograph");
    // no graph supplied: line-less anchors cannot fill — all frames skip
    // with noanchor, none may degrade to a line=1 step
    let st = build_tours(&chain, &repo, None).unwrap();
    assert_eq!(st.skipped, 5, "{:?}", st.g_counts);
    assert_eq!(
        st.g_counts.get("graph-unavailable"),
        Some(&5),
        "{:?}",
        st.g_counts
    );
    assert!(st
        .tours
        .iter()
        .all(|t| t["steps"].as_array().unwrap().is_empty()));
}

#[test]
fn build_tours_lineless_ambiguous_fails_closed() {
    let (repo, chain) = repo_fixture_lineless("lineless-ambig");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    // two same-name defs in the SAME file: the unique-anchor gate must
    // fail closed (no ORDER-BY-nearest blind pick). Distinct qnames (the
    // fixture universe keys symbol==qname UNIQUE) with the same name.
    seed_node(&mut spec, "boot", "pkg/a.py::boot#1", "pkg/a.py", 5, "Function", &abs);
    seed_node(&mut spec, "boot", "pkg/a.py::boot#2", "pkg/a.py", 7, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    // CHAIN_MD_LINELESS has 5 frames; only kernel fills here
    assert_eq!(st.g_counts.get("filled"), Some(&1), "{:?}", st.g_counts);
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    let t = st.tours[0].as_object().unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert!(steps[0]["title"].as_str().unwrap().contains("kernel"));
    assert_eq!(st.skipped, 4);
}

#[test]
fn build_tours_lineless_mixed_explicit_unchanged() {
    // explicit anchor keeps the legacy face (same + 行號一致); the
    // line-less sibling fills from graph
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("lineless-mixed");
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("pkg/a.py"),
        "def kernel():\n    pass\n\n\ndef boot():\n    load_config()\n",
    )
    .unwrap();
    let chain = repo.join("chain.md");
    std::fs::write(&chain, CHAIN_MD_MIXED).unwrap();
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
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    seed_node(&mut spec, "boot", "pkg/a.py::boot", "pkg/a.py", 5, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    let kernel = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("kernel"))
        .unwrap();
    assert_eq!(kernel["line"], 1);
    assert!(kernel["description"].as_str().unwrap().contains("行號一致"));
    let boot = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("boot"))
        .unwrap();
    // a.py: boot sits at line 5 (two blank lines after pass); the graph
    // hit agrees, and the F1 blank-target gate would have rejected a
    // wrong seed (this assertion caught exactly that during review)
    assert_eq!(boot["line"], 5);
    assert!(boot["description"].as_str().unwrap().contains("graph 補行號"));
}

#[test]
fn lineless_roundtrip_tour_validate() {
    let (repo, chain) = repo_fixture_lineless("lineless-roundtrip");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    seed_node(&mut spec, "boot", "pkg/a.py::boot", "pkg/a.py", 5, "Function", &abs);
    seed_node(
        &mut spec,
        "load_config",
        "pkg/cfg.py::load_config",
        "pkg/cfg.py",
        1,
        "Function",
        &abs,
    );
    seed_node(&mut spec, "helper", "pkg/a.py::helper", "pkg/a.py", 9, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    // an empty tour validates trivially — pin the filled steps first so
    // the roundtrip actually exercises the graph-filled coordinates
    assert_eq!(
        st.tours[0]["steps"].as_array().unwrap().len(),
        4,
        "{}",
        st.tours[0]
    );
    let out_dir = repo.join(".tours").join("arch").join("01-lineless");
    let mut primary = std::collections::BTreeSet::new();
    primary.insert(1);
    let mut warns = Vec::new();
    write_tours(&st, &out_dir, &primary, &mut warns).unwrap();
    // consumer roundtrip: the generated .tour validates exact — the
    // graph-filled steps' file/line/pattern triple must hit the source
    let out = code_reality::tour_validate::validate(
        &repo,
        std::path::Path::new(".tours/arch/01-lineless"),
        false,
    );
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    assert!(!out.stdout.contains("[FAIL]"), "{}", out.stdout);
}

// review F5/F6: full-width wrappers and buried `.py` substrings
#[test]
fn lineless_tail_grammar_review_fixes() {
    // F5: full-width parens (CJK docs) work as wrappers
    let frames = parse_frames(&["└─ boot（pkg/a.py）".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[0].ident, "boot");
    // F6: a `.py` buried in a longer identifier does not veto the anchor
    // (the buried token still wins legacy best_ident — symbol naming is
    // the doc author's concern; the anchor surviving is the contract)
    let frames = parse_frames(&["└─ client.py_helper() (pkg/a.py)".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/a.py"));
    assert_eq!(frames[0].ident, "py_helper");
    // two-path rejection still holds after the boundary fix
    let frames = parse_frames(&["└─ x pkg/a.py extra (pkg/cfg.py)".to_string()]);
    assert_eq!(frames[0].path, None);
}

// review F1: blank/out-of-range fill target downgrades to a typed skip
#[test]
fn build_tours_lineless_blank_target_fails_closed() {
    let (repo, chain) = repo_fixture_lineless("lineless-blank");
    // a.py line 3 is blank — a graph hit pointing there must not fill
    std::fs::write(&chain, "# T\n\n```text\n└─ boot() (pkg/a.py)\n```\n").unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "boot", "pkg/a.py::boot", "pkg/a.py", 3, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("invalid-line"), Some(&1), "{:?}", st.g_counts);
    assert_eq!(st.skipped, 1);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

// review F4 (#3): empty symbol remainder → typed no-ident skip
#[test]
fn build_tours_lineless_no_ident() {
    let (repo, chain) = repo_fixture_lineless("lineless-noident");
    std::fs::write(&chain, "# T\n\n```text\n└─ (pkg/a.py)\n```\n").unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("no-ident"), Some(&1), "{:?}", st.g_counts);
    assert_eq!(st.skipped, 1);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

// review F4 (#5): cross-file hits must NOT be inferred — exact-path query
// degrades to not-in-graph, never moved-file
#[test]
fn build_tours_lineless_cross_file_stays_unfilled() {
    let (repo, chain) = repo_fixture_lineless("lineless-crossfile");
    std::fs::write(&chain, "# T\n\n```text\n└─ load_config() (pkg/a.py)\n```\n").unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    // load_config exists ONLY in cfg.py; the anchor names a.py
    seed_node(
        &mut spec,
        "load_config",
        "pkg/cfg.py::load_config",
        "pkg/cfg.py",
        1,
        "Function",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("not-in-graph"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.g_counts.get("moved-file").is_none(), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

// review F4 (#7): a prose mention that survives the grammar is a visible
// noanchor skip (documented by-design trade-off), not a fake step
#[test]
fn build_tours_lineless_prose_survivor_visible_skip() {
    let (repo, chain) = repo_fixture_lineless("lineless-prose");
    std::fs::write(&chain, "# T\n\n```text\n└─ see pkg/a.py\n```\n").unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    // ident "see" is not in the graph → not-in-graph skip
    assert_eq!(
        st.g_counts.get("not-in-graph"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

// review F4 (#9): explicit `:0` pins the legacy face (moved via the
// nearest-line probe) — this arc does not touch explicit-anchor behavior
#[test]
fn build_tours_explicit_zero_pins_legacy_face() {
    let (repo, chain) = repo_fixture_lineless("lineless-zero");
    std::fs::write(&chain, "# T\n\n```text\n└─ kernel (pkg/a.py:0)\n```\n").unwrap();
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_node(&mut spec, "kernel", "pkg/a.py::kernel", "pkg/a.py", 1, "Function", &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 1);
    assert!(steps[0]["description"].as_str().unwrap().contains("graph +1"));
}

// CR-7 calibration (mosaic corpus): single-space ` # ` notes are the
// corpus norm — line-less frames must still anchor AND carry the note
#[test]
fn lineless_single_space_note_calibrated() {
    let frames = parse_frames(&[
        "└─ get_catalog mosaic_alpha/data/__init__.py # module singleton；重複呼叫回同實例".to_string(),
    ]);
    assert_eq!(frames[0].path.as_deref(), Some("mosaic_alpha/data/__init__.py"));
    assert_eq!(frames[0].line, None);
    assert_eq!(frames[0].ident, "get_catalog");
    assert_eq!(frames[0].note, "module singleton；重複呼叫回同實例");
    // sample C: a pseudo-anchor inside the note must not be captured —
    // the note segment is cut before tail extraction
    let frames = parse_frames(&[
        "└─ IntelService.punish_stocks mosaic_alpha/services/intel.py # services 經過段（外層唯一消費 watchlist/ranking.py:339）".to_string(),
    ]);
    assert_eq!(frames[0].path.as_deref(), Some("mosaic_alpha/services/intel.py"));
    assert_eq!(frames[0].line, None);
    assert!(frames[0].note.contains("ranking.py:339"));
    // and the same frame WITH explicit line keeps byte-legacy behavior —
    // single-space notes ride in the symbol residue (known asymmetry,
    // documented on the card; note recovery is a line-less-face feature)
    let frames = parse_frames(&[
        "└─ get_catalog mosaic_alpha/data/__init__.py:34 # module singleton".to_string(),
    ]);
    assert_eq!(frames[0].path.as_deref(), Some("mosaic_alpha/data/__init__.py"));
    assert_eq!(frames[0].line, Some(34));
    assert_eq!(frames[0].note, "");
    assert!(frames[0].symbol.contains("module singleton"));
}

// ---------- CR-9: same-file same-name disambiguation (R1 pair / R2 qualified name / R3 hint) ----------

/// Real SCIP-shaped symbol builders (pyrefly / rust-analyzer faces) —
/// the CR-9 fixtures must exercise the exact descriptor grammar the
/// resolver parses, not qname stand-ins.
fn py_sym(module: &str, descriptor: &str) -> String {
    format!("`pyrefly python demo 1.0 `{module}`/{descriptor}")
}

fn rs_sym(descriptor: &str) -> String {
    format!("`rust-analyzer cargo demo 0.1.0 pkg/{descriptor}")
}

#[allow(clippy::too_many_arguments)]
fn seed_sym(
    spec: &mut graph_db_fixture::GraphDbSpec,
    symbol: &str,
    name: &str,
    file: &str,
    line: i64,
    kind: &'static str,
    language: &'static str,
    abs: &dyn Fn(&str) -> String,
) {
    spec.nodes.push(graph_db_fixture::NodeSeed {
        name: name.into(),
        parent: None,
        qname: symbol.into(),
        file_path: abs(file),
    });
    spec.node_attrs.push((
        symbol.into(),
        graph_db_fixture::NodeAttr {
            kind,
            language,
            is_test: 0,
            community_id: None,
        },
    ));
    spec.node_lines.push((symbol.into(), line));
}

/// Repo fixture for the CR-9 tests: pkg/ module profile, arbitrary
/// source files, chain md, one git commit.
fn repo_fixture_cr9(tag: &str, files: &[(&str, String)], chain_md: &str) -> (PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    for (rel, content) in files {
        std::fs::write(repo.join(rel), content).unwrap();
    }
    let chain = repo.join("chain.md");
    std::fs::write(&chain, chain_md).unwrap();
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

fn db_of(repo: &std::path::Path) -> PathBuf {
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    db
}

const CHAIN_MD_R1: &str = r#"# R1 scenario

```text
Plain (pkg/r1.py)
└─ Inner (pkg/r1.py)
```
"#;

/// R1 positive (Plan ⑧): the candidate set is exactly the {Type `X#`,
/// pseudo-ctor `X().`} pair — for a plain class AND a nested class.
/// Both fill the class line and stamp the Type symbol.
#[test]
fn r1_pseudo_ctor_pair_fills_type_symbol() {
    let src = "# module doc\n\nclass Plain:\n    pass\n\nclass Outer:\n    class Inner:\n        pass\n";
    let (repo, chain) = repo_fixture_cr9("cr9-r1-pos", &[("pkg/r1.py", src.into())], CHAIN_MD_R1);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1", "Plain#"),
        "Plain",
        "pkg/r1.py",
        3,
        "Type",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1", "Plain()."),
        "Plain",
        "pkg/r1.py",
        3,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1", "Outer#Inner#"),
        "Inner",
        "pkg/r1.py",
        7,
        "Type",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1", "Outer#Inner()."),
        "Inner",
        "pkg/r1.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&2), "{:?}", st.g_counts);
    assert!(st.g_counts.get("ambiguous").is_none(), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    let plain = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("Plain"))
        .unwrap();
    assert_eq!(plain["line"], 3);
    assert_eq!(
        plain["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.r1", "Plain#")
    );
    assert!(plain["description"].as_str().unwrap().contains("graph 補行號"));
    let inner = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("Inner"))
        .unwrap();
    assert_eq!(inner["line"], 7);
    assert_eq!(
        inner["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.r1", "Outer#Inner#")
    );
    assert!(!st.tours[0]["description"]
        .as_str()
        .unwrap()
        .contains("歧義候選"));
}

const CHAIN_MD_R1_NEG_FN: &str = r#"# R1 neg scenario

```text
└─ Widget (pkg/r1n.py)
```
"#;

/// R1 negative #1: Type `Widget#` plus a same-name FUNCTION on a
/// different line — the pseudo-ctor pairing requires equal line_start;
/// the set stays ambiguous (bare doc name matches both chains in R2).
#[test]
fn r1_negative_same_name_fn_other_line_stays_ambiguous() {
    let src = "# mod\n\nclass Widget:\n    pass\n\n\ndef Widget():\n    pass\n";
    let (repo, chain) =
        repo_fixture_cr9("cr9-r1-negfn", &[("pkg/r1n.py", src.into())], CHAIN_MD_R1_NEG_FN);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1n", "Widget#"),
        "Widget",
        "pkg/r1n.py",
        3,
        "Type",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1n", "Widget()."),
        "Widget",
        "pkg/r1n.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(st.g_counts.get("filled").is_none(), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("歧義候選：Widget → Widget# :3（Type）／Widget(). :7（Function）"),
        "{desc}"
    );
}

const CHAIN_MD_R1_NEG_SAME_LINE: &str = r#"# R1 neg same-line

```text
└─ run (pkg/r1s.py)
```
"#;

/// R1 negative #2: two same-name method nodes on the SAME line with no
/// Type/ctor shape (both Function kind) — the pairing gate must not
/// fire even when the lines coincide.
#[test]
fn r1_negative_same_line_unpaired_stays_ambiguous() {
    let src = "class Alpha:\n    def run(self):\n        pass\n\n\nclass Beta:\n    def run(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-r1-negsl",
        &[("pkg/r1s.py", src.into())],
        CHAIN_MD_R1_NEG_SAME_LINE,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1s", "Alpha#run()."),
        "run",
        "pkg/r1s.py",
        3,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1s", "Beta#run()."),
        "run",
        "pkg/r1s.py",
        3,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("Alpha#run(). :3（Function）／Beta#run(). :3（Function）"),
        "{desc}"
    );
}

const CHAIN_MD_R1_NEG_NESTED: &str = r#"# R1 neg nested

```text
└─ inner (pkg/r1f.py)
```
"#;

/// R1 negative #3: two nested same-name functions — no Type/ctor pair
/// exists in the set at all, and the bare doc name tail-aligns into
/// BOTH chains in R2 → ambiguous (the CR-7 addendum adversarial face:
/// docs without qualifying information never get a blind pick).
#[test]
fn r1_negative_nested_fns_no_ctor_stays_ambiguous() {
    let src = "def outer():\n    def inner():\n        pass\n\n\ndef other():\n    def inner():\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-r1-negn",
        &[("pkg/r1f.py", src.into())],
        CHAIN_MD_R1_NEG_NESTED,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1f", "outer().inner()."),
        "inner",
        "pkg/r1f.py",
        3,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.r1f", "other().inner()."),
        "inner",
        "pkg/r1f.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(st.g_counts.get("filled").is_none(), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("outer().inner(). :3（Function）／other().inner(). :7（Function）"),
        "{desc}"
    );
}

const CHAIN_MD_R2: &str = r#"# R2 scenario

```text
DataPhaseStep.execute (pkg/qual.py)
└─ Outer.A.execute (pkg/nest.py)
```
"#;

/// R2 positive (mosaic MOS-190 shape): `DataPhaseStep.execute` against
/// three same-name methods fills exactly the named one; the full
/// `Outer.A.execute` chain fills the nested class's method.
#[test]
fn r2_qualified_name_picks_unique_candidate() {
    let qual = "class DataPhaseStep:\n    def execute(self):\n        pass\n\n\nclass PowerStep:\n    def execute(self):\n        pass\n\n\nclass GridStep:\n    def execute(self):\n        pass\n";
    let nest = "class Outer:\n    class A:\n        def execute(self):\n            pass\n\n\nclass Other:\n    class A:\n        def execute(self):\n            pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-r2-pos",
        &[("pkg/qual.py", qual.into()), ("pkg/nest.py", nest.into())],
        CHAIN_MD_R2,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.qual", "DataPhaseStep#execute()."),
        "execute",
        "pkg/qual.py",
        2,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.qual", "PowerStep#execute()."),
        "execute",
        "pkg/qual.py",
        7,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.qual", "GridStep#execute()."),
        "execute",
        "pkg/qual.py",
        12,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.nest", "Outer#A#execute()."),
        "execute",
        "pkg/nest.py",
        3,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.nest", "Other#A#execute()."),
        "execute",
        "pkg/nest.py",
        9,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&2), "{:?}", st.g_counts);
    assert!(st.g_counts.get("ambiguous").is_none(), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    let dps = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("DataPhaseStep"))
        .unwrap();
    assert_eq!(dps["line"], 2);
    assert_eq!(
        dps["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.qual", "DataPhaseStep#execute().")
    );
    let outer_a = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("Outer"))
        .unwrap();
    assert_eq!(outer_a["line"], 3);
    assert_eq!(
        outer_a["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.nest", "Outer#A#execute().")
    );
    assert!(!st.tours[0]["description"]
        .as_str()
        .unwrap()
        .contains("歧義候選"));
}

const CHAIN_MD_R2_NEG: &str = r#"# R2 neg scenario

```text
Nope.execute (pkg/qual2.py)
├─ Base.run (pkg/qual2.rs)
└─ execute (pkg/qual2.py)
```
"#;

/// R2 negatives: a qualified name with NO matching candidate, a
/// qualified name matching TWO candidates (Rust inherent + trait impl
/// of the same method — the no-unique-evidence fixture), and a bare
/// name over multiple methods (CR-7 addendum adversarial face: docs
/// without qualifying information stay fail-closed). All ambiguous,
/// all with rendered candidates.
#[test]
fn r2_unresolvable_sets_stay_ambiguous() {
    let qual = "class DataPhaseStep:\n    def execute(self):\n        pass\n\n\nclass PowerStep:\n    def execute(self):\n        pass\n";
    let lab = "struct Base;\ntrait Runner {\n    fn run(&self);\n}\nimpl Runner for Base {\n    fn run(&self) {}\n}\nimpl Base {\n    fn run(&self) {}\n}\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-r2-neg",
        &[("pkg/qual2.py", qual.into()), ("pkg/qual2.rs", lab.into())],
        CHAIN_MD_R2_NEG,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.qual2", "DataPhaseStep#execute()."),
        "execute",
        "pkg/qual2.py",
        2,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.qual2", "PowerStep#execute()."),
        "execute",
        "pkg/qual2.py",
        7,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Base]run()."),
        "run",
        "pkg/qual2.rs",
        6,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Base][WsLab]run()."),
        "run",
        "pkg/qual2.rs",
        9,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&3), "{:?}", st.g_counts);
    assert!(st.g_counts.get("filled").is_none(), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    // 限定無候選符: Nope.execute matches nothing, candidates still listed
    assert!(
        desc.contains("Nope.execute → DataPhaseStep#execute(). :2（Function）／PowerStep#execute(). :7（Function）"),
        "{desc}"
    );
    // 雙候選符: Base.run matches BOTH impl chains — no unique evidence
    assert!(
        desc.contains("Base.run → impl#[Base]run(). :6（Function）／impl#[Base][WsLab]run(). :9（Function）"),
        "{desc}"
    );
    // bare 名多法: unqualified doc over two methods stays fail-closed
    assert!(
        desc.contains("execute → DataPhaseStep#execute(). :2（Function）／PowerStep#execute(). :7（Function）"),
        "{desc}"
    );
    // exactly three detailed frames — no fold marker at the cap boundary
    assert!(!desc.contains("另有"), "{desc}");
}

const CHAIN_MD_RS_POS: &str = r#"# Rust scenario

```text
└─ Lab.m (pkg/lab.rs)
```
"#;

/// Rust dual-bracket positive (Plan ⑧): `impl#[Lab][WsLab]m().` vs
/// `impl#[WsLab]m().` — the doc's `Lab.m` chain takes the FIRST bracket
/// group (inherent type) and fills the dual-bracket candidate.
#[test]
fn r2_rust_impl_dual_bracket_positive() {
    let src = "mod lab {\n    pub struct Lab;\n    pub trait WsLab {\n        fn m(&self);\n    }\n    impl WsLab for Lab {\n        fn m(&self) {}\n    }\n}\n";
    let (repo, chain) =
        repo_fixture_cr9("cr9-rs-pos", &[("pkg/lab.rs", src.into())], CHAIN_MD_RS_POS);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Lab][WsLab]m()."),
        "m",
        "pkg/lab.rs",
        7,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[WsLab]m()."),
        "m",
        "pkg/lab.rs",
        7,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&1), "{:?}", st.g_counts);
    assert!(st.g_counts.get("ambiguous").is_none(), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 7);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Lab][WsLab]m().")
    );
}

const CHAIN_MD_BARE_TIER1: &str = r#"# bare tier-1 scenario

```text
└─ X (pkg/bt1.py)
```
"#;

/// Bare-tier-1 contract pin: over the same-file pair {top-level fn
/// `X`, method `Outer#X`}, the bare doc token `X` fills via R2 tier-1 —
/// the top-level candidate's complete scope chain (`X`) equals the
/// token exactly while the scoped candidate's (`Outer`, `X`) does not,
/// leaving exactly one tier-1 survivor. Bare-token fills carry no
/// qualifying information by definition; this test pins the exact
/// full-chain-equality path as contract (an undocumented fill route
/// that nevertheless never guesses: zero ambiguity in evidence).
#[test]
fn r2_bare_token_tier1_exact_chain_fills_top_level_fn() {
    let src = "def X():\n    pass\n\n\nclass Outer:\n    def X(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-bare-tier1",
        &[("pkg/bt1.py", src.into())],
        CHAIN_MD_BARE_TIER1,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.bt1", "X()."),
        "X",
        "pkg/bt1.py",
        1,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.bt1", "Outer#X()."),
        "X",
        "pkg/bt1.py",
        6,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&1), "{:?}", st.g_counts);
    // clippy unnecessary_get_then_check face (the older CR-9 tests
    // predate the lint): negated contains_key, no new warnings.
    assert!(!st.g_counts.contains_key("ambiguous"), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 1);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.bt1", "X().")
    );
    assert!(!st.tours[0]["description"]
        .as_str()
        .unwrap()
        .contains("歧義候選"));
}

const CHAIN_MD_MOD_QUAL: &str = r#"# module-qualified boundary

```text
└─ mod.Cls.m (pkg/mq.py)
```
"#;

/// Boundary pin: a module-qualified doc token can never resolve. The
/// descriptor chain cuts the module segment, so the over-qualified
/// chain (`mod`, `Cls`, `m`) is LONGER than every candidate chain
/// (`Cls`, `m` / `Other`, `m`): no tier-1 equality and no strictly
/// longer tail-align exists. Fail-closed ambiguous with the hint line.
#[test]
fn r2_module_qualified_token_stays_ambiguous_with_hint() {
    let src = "class Cls:\n    def m(self):\n        pass\n\n\nclass Other:\n    def m(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-mod-qual",
        &[("pkg/mq.py", src.into())],
        CHAIN_MD_MOD_QUAL,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.mq", "Cls#m()."),
        "m",
        "pkg/mq.py",
        2,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.mq", "Other#m()."),
        "m",
        "pkg/mq.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("filled"), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("歧義候選：mod.Cls.m → Cls#m(). :2（Function）／Other#m(). :7（Function）"),
        "{desc}"
    );
}

const CHAIN_MD_EMPTY_SEGMENT: &str = r#"# internal-empty-segment boundary

```text
└─ a..b (pkg/es.py)
```
"#;

/// Boundary pin (observed bucket: ambiguous): `a..b` survives ident
/// extraction as ONE token — the ident regex allows an empty
/// dot-segment — with non-empty base `b`, so it is not the no-ident
/// bucket; but the empty middle segment fails R2's usable-chain gate
/// (any split segment "" refuses the doc chain outright) → ambiguous
/// with the hint line.
#[test]
fn r2_internal_empty_segment_token_stays_ambiguous() {
    let src =
        "class A:\n    def b(self):\n        pass\n\n\nclass B:\n    def b(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-empty-seg",
        &[("pkg/es.py", src.into())],
        CHAIN_MD_EMPTY_SEGMENT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.es", "A#b()."),
        "b",
        "pkg/es.py",
        2,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.es", "B#b()."),
        "b",
        "pkg/es.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("filled"), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("歧義候選：a..b → A#b(). :2（Function）／B#b(). :7（Function）"),
        "{desc}"
    );
}

const CHAIN_MD_PROSE_NEG: &str = r#"# prose frame negative

```text
└─ see DataPhaseStep.execute (pkg/daily.py)
```
"#;

/// COD-1 whole-frame gate, negative: the symbol remainder carries prose
/// around the token (`see DataPhaseStep.execute`) — prose must not gain
/// disambiguation power. Over the same-file trio of `execute` methods
/// the frame stays ambiguous (zero steps, no x-codeReality anywhere)
/// and the R3 hint still lists all three candidates (fail-visible).
#[test]
fn prose_frame_stays_ambiguous_with_candidates() {
    let src = "class DataPhaseStep:\n    def execute(self):\n        pass\n\n\nclass PowerStep:\n    def execute(self):\n        pass\n\n\nclass GridStep:\n    def execute(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-prose-neg",
        &[("pkg/daily.py", src.into())],
        CHAIN_MD_PROSE_NEG,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (cls, line) in [("DataPhaseStep", 2), ("PowerStep", 7), ("GridStep", 12)] {
        seed_sym(
            &mut spec,
            &py_sym("pkg.daily", &format!("{cls}#execute().")),
            "execute",
            "pkg/daily.py",
            line,
            "Function",
            "python",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("filled"), "{:?}", st.g_counts);
    let t = st.tours[0].as_object().unwrap();
    assert!(t["steps"].as_array().unwrap().is_empty());
    let tour_json = serde_json::to_string(t).unwrap();
    assert!(!tour_json.contains("x-codeReality"), "{tour_json}");
    let desc = t["description"].as_str().unwrap();
    assert!(desc.contains("ambiguous:1"), "{desc}");
    assert!(
        desc.contains(
            "歧義候選：see DataPhaseStep.execute → DataPhaseStep#execute(). :2（Function）／PowerStep#execute(). :7（Function）／GridStep#execute(). :12（Function）"
        ),
        "{desc}"
    );
}

const CHAIN_MD_PROSE_POS: &str = r#"# prose gate boundary positive

```text
└─ DataPhaseStep.execute (pkg/daily2.py)
```
"#;

/// COD-1 whole-frame gate, positive boundary: the same-shaped frame
/// WITHOUT the prose prefix stays eligible — the gate blocks prose, not
/// qualified-name frames. R2 tier-1 keeps exactly one survivor and
/// fills it.
#[test]
fn qualified_frame_without_prose_still_fills() {
    let src = "class DataPhaseStep:\n    def execute(self):\n        pass\n\n\nclass PowerStep:\n    def execute(self):\n        pass\n\n\nclass GridStep:\n    def execute(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-prose-pos",
        &[("pkg/daily2.py", src.into())],
        CHAIN_MD_PROSE_POS,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (cls, line) in [("DataPhaseStep", 2), ("PowerStep", 7), ("GridStep", 12)] {
        seed_sym(
            &mut spec,
            &py_sym("pkg.daily2", &format!("{cls}#execute().")),
            "execute",
            "pkg/daily2.py",
            line,
            "Function",
            "python",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("ambiguous"), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 2);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.daily2", "DataPhaseStep#execute().")
    );
    assert!(!st.tours[0]["description"]
        .as_str()
        .unwrap()
        .contains("歧義候選"));
}

const CHAIN_MD_MALFORMED_BRACKET: &str = r#"# malformed bracket scenario

```text
└─ Lab.m (pkg/lab2.rs)
```
"#;

/// COD-2 bracket-group validation: a malformed trait group
/// (`impl#[Lab][]m().` — empty second bracket group) makes the whole
/// candidate set unresolvable, even though tier-1 would otherwise match
/// `Lab.m` against it. Fail-closed ambiguous; the malformed candidate
/// also stays out of the R3 hint (only cleanly-parsing candidates
/// render there).
#[test]
fn r2_malformed_bracket_group_stays_ambiguous() {
    let src = "mod lab2 {\n    pub struct Lab;\n    pub trait WsLab {\n        fn m(&self);\n    }\n    impl WsLab for Lab {\n        fn m(&self) {}\n    }\n}\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr9-malformed-bracket",
        &[("pkg/lab2.rs", src.into())],
        CHAIN_MD_MALFORMED_BRACKET,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Lab][]m()."),
        "m",
        "pkg/lab2.rs",
        7,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[WsLab]m()."),
        "m",
        "pkg/lab2.rs",
        7,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("filled"), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains("歧義候選：Lab.m → impl#[WsLab]m(). :7（Function）"),
        "{desc}"
    );
    // the malformed descriptor never renders into the hint
    assert!(!desc.contains("impl#[Lab][]m()"), "{desc}");
}

const CHAIN_MD_R3: &str = r#"# R3 scenario

```text
m1 (pkg/cap.py)
├─ m2 (pkg/cap.py)
├─ m3 (pkg/cap.py)
└─ m4 (pkg/cap.py)
```
"#;

/// R3 caps: a 5-candidate frame renders ≤4 with an ellipsis marker; a
/// 4-ambiguous-frame scenario details ≤3 and folds the rest into an
/// explicit remaining count. Candidate order follows the fill query's
/// line_start,symbol; no `path.py:NN` shape ever appears.
#[test]
fn r3_hint_caps_per_frame_and_folds_frames() {
    let mut src = String::new();
    for c in ["C1", "C2", "C3", "C4", "C5"] {
        src.push_str(&format!("class {c}:\n    def m1(self):\n        pass\n\n"));
    }
    for (cls, m) in [
        ("D1", "m2"),
        ("D2", "m2"),
        ("E1", "m3"),
        ("E2", "m3"),
        ("F1", "m4"),
        ("F2", "m4"),
    ] {
        src.push_str(&format!("class {cls}:\n    def {m}(self):\n        pass\n\n"));
    }
    let (repo, chain) =
        repo_fixture_cr9("cr9-r3-caps", &[("pkg/cap.py", src)], CHAIN_MD_R3);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (i, c) in ["C1", "C2", "C3", "C4", "C5"].iter().enumerate() {
        seed_sym(
            &mut spec,
            &py_sym("pkg.cap", &format!("{c}#m1().")),
            "m1",
            "pkg/cap.py",
            (i as i64) * 4 + 2,
            "Function",
            "python",
            &abs,
        );
    }
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "D1#m2()."),
        "m2",
        "pkg/cap.py",
        22,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "D2#m2()."),
        "m2",
        "pkg/cap.py",
        26,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "E1#m3()."),
        "m3",
        "pkg/cap.py",
        30,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "E2#m3()."),
        "m3",
        "pkg/cap.py",
        34,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "F1#m4()."),
        "m4",
        "pkg/cap.py",
        38,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.cap", "F2#m4()."),
        "m4",
        "pkg/cap.py",
        42,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("ambiguous"), Some(&4), "{:?}", st.g_counts);
    let desc = st.tours[0]["description"].as_str().unwrap();
    let expected = "\n歧義候選：m1 → C1#m1(). :2（Function）／C2#m1(). :6（Function）／C3#m1(). :10（Function）／C4#m1(). :14（Function）／…；m2 → D1#m2(). :22（Function）／D2#m2(). :26（Function）；m3 → E1#m3(). :30（Function）／E2#m3(). :34（Function）；另有 1 幀（略）";
    assert!(desc.ends_with(expected), "{desc}");
    assert!(!desc.contains("C5#m1"), "per-frame cap: {desc}");
    assert!(!desc.contains("F1#m4"), "folded frame: {desc}");
    assert!(!desc.contains(".py:"), "no path:line shape: {desc}");
}

/// R3 zero-change: a tour with no ambiguity keeps the byte-identical
/// legacy description; the explicit-line face is untouched (same +
/// 文檔錨 path:line in the frozen step face).
#[test]
fn r3_no_ambiguity_description_byte_identical_explicit_face_pinned() {
    const MD: &str = r#"# Clean scenario

```text
kernel (pkg/clean.py:1)
└─ helper (pkg/clean.py:4)
```
"#;
    let src = "def kernel():\n    pass\n\n\ndef helper():\n    pass\n";
    let (repo, chain) =
        repo_fixture_cr9("cr9-r3-clean", &[("pkg/clean.py", src.into())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.clean", "kernel()."),
        "kernel",
        "pkg/clean.py",
        1,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.clean", "helper()."),
        "helper",
        "pkg/clean.py",
        4,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert_eq!(
        desc,
        "2 幀 → 2 步；0 幀跳過（無——例：無）。\ngraph 重錨分佈：same:2",
        "no-ambiguity description must stay byte-identical: {desc}"
    );
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    let kernel = steps
        .iter()
        .find(|s| s["title"].as_str().unwrap().contains("kernel"))
        .unwrap();
    assert_eq!(kernel["line"], 1);
    assert!(kernel["description"].as_str().unwrap().contains("行號一致"));
    assert!(kernel["description"]
        .as_str()
        .unwrap()
        .contains("文檔錨 pkg/clean.py:1"));
}

// ---------- CR-17: explicit-line trait collision (ambiguity disclosure + safe selection) ----------

/// NT-shaped price.rs: filler lines with a real `fn fmt` head at 839
/// (Debug) and 845 (Display) so an exact-hit kept anchor reads a
/// non-blank pattern line.
fn price_rs_fixture() -> String {
    let mut lines: Vec<String> = (1..=850).map(|i| format!("// filler {i}")).collect();
    lines[838] = "    fn fmt(&self, f: &mut Formatter) -> Result {".into(); // line 839
    lines[844] = "    fn fmt(&self, f: &mut Formatter) -> Result {".into(); // line 845
    lines.join("\n")
}

fn seed_price_impls(spec: &mut graph_db_fixture::GraphDbSpec, abs: &dyn Fn(&str) -> String) {
    for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
        seed_sym(
            spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]fmt().")),
            "fmt",
            "pkg/price.rs",
            line,
            "Function",
            "rust",
            abs,
        );
    }
}

const CHAIN_MD_CR17_DRIFT: &str = r#"# Price formatting drift

```text
└─ Price::fmt() pkg/price.rs:776
```
"#;

/// AC-1 (NT original repro): doc anchor 776 vs the same-file same-name
/// trait-impl pair (839 [Debug] / 845 [Display], same inherent type
/// [Price]) — |839-776|=63 < |845-776|=69, so the legacy nearest-line
/// SQL silently minted the Debug SCIP identity as a `moved` anchor. The
/// explicit-line collision gate must refuse the nearest guess
/// (fail-closed, no step), disclose every candidate, and count the
/// frame under its own `explicit_trait_ambiguous` bucket — never under
/// `ambiguous` (line-less semantics) and never as a silent moved step.
#[test]
fn explicit_line_trait_collision_drift_refuses_nearest() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-drift",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_DRIFT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    // the silent nearest pick is gone: no moved step, no Debug identity
    assert!(!st.g_counts.contains_key("moved"), "{:?}", st.g_counts);
    assert!(
        !st.g_counts.contains_key("ambiguous"),
        "{:?}",
        st.g_counts
    );
    let t = st.tours[0].as_object().unwrap();
    assert!(t["steps"].as_array().unwrap().is_empty());
    assert_eq!(st.skipped, 1);
    let tour_json = serde_json::to_string(t).unwrap();
    assert!(
        !tour_json.contains("x-codeReality"),
        "refused frame must mint no SCIP identity: {tour_json}"
    );
    // single-frame all-skip scenario is a CR-11 shell: disclosed, not
    // written — the skip reason carries the collision key
    assert_eq!(st.shells_omitted.len(), 1);
    assert_eq!(
        st.shells_omitted[0]
            .skip_reasons
            .get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.shells_omitted[0].skip_reasons
    );
    // the description discloses the full candidate set + the refusal
    let desc = t["description"].as_str().unwrap();
    assert!(
        desc.contains(
            "trait 碰撞候選：Price::fmt()（文檔錨 pkg/price.rs:776）→ impl#［Price］［Debug］fmt(). :839（Function）／impl#［Price］［Display］fmt(). :845（Function）（行號未精確命中，拒絕最近猜測）"
        ),
        "{desc}"
    );
    assert!(desc.contains("explicit_trait_ambiguous 1"), "{desc}");
    assert!(desc.contains("explicit_trait_ambiguous:1"), "{desc}");
}

const CHAIN_MD_CR17_EXACT: &str = r#"# Price formatting exact

```text
└─ Price::fmt() pkg/price.rs:845
```
"#;

/// AC-2: the doc anchor line exactly hits one candidate (845 Display) —
/// the exact line is real evidence, so the anchor stays (`same`), the
/// step carries the Display SCIP symbol, and the collision payload
/// stays attached on both the step description and the scenario
/// description for human identification.
#[test]
fn explicit_line_trait_collision_exact_hit_keeps_anchor() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-exact",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_EXACT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("same"), Some(&1), "{:?}", st.g_counts);
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert_eq!(st.skipped, 0);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
    // the kept step discloses the colliding siblings for human review
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("trait 碰撞"), "{sdesc}");
    assert!(
        sdesc.contains("impl#［Price］［Debug］fmt(). :839（Function）"),
        "{sdesc}"
    );
    assert!(
        sdesc.contains("impl#［Price］［Display］fmt(). :845（Function）"),
        "{sdesc}"
    );
    assert!(sdesc.contains("行號精確命中"), "{sdesc}");
    // the scenario description hint marks the kept mode
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("（文檔錨 pkg/price.rs:845）"), "{desc}");
    assert!(desc.contains("（行號精確命中，保留該錨）"), "{desc}");
    assert!(desc.contains("explicit_trait_ambiguous:1"), "{desc}");
}

/// F1 round-trip (review): the kept-step candidate note must survive
/// tour_validate's link scan — ASCII brackets in a descriptor
/// (`impl#[Price][Debug]fmt().`) parse back as a markdown reference
/// link and FAIL check_links on an empty tour index. The renderer
/// escapes them full-width (the note is display text;
/// `x-codeReality.symbol` keeps the raw SCIP symbol).
#[test]
fn explicit_line_trait_collision_step_note_survives_check_links() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-f1-roundtrip",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_EXACT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    let mut stdout = String::new();
    let (fails, _) = code_reality::tour_validate::check_links(
        "01.tour",
        &st.tours[0],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        &mut stdout,
    );
    assert!(fails.is_empty(), "check_links fails: {fails:?}\n{stdout}");
}

/// C2 (review): five same-file same-type impls with differing trait
/// groups — a ≥5 candidate set truncates at 4 with an explicit omitted
/// count, identical on all three faces (kept-step note, scenario
/// description hint, CLI WARN) because one renderer builds them. The
/// doc anchor exactly hits the 4th candidate (845), so the kept-anchor
/// note face is live too.
fn price_rs_five_impls_fixture() -> String {
    let mut lines: Vec<String> = (1..=860).map(|i| format!("// filler {i}")).collect();
    for line in [830usize, 835, 840, 845, 850] {
        lines[line - 1] = "    fn fmt(&self, f: &mut Formatter) -> Result {".into();
    }
    lines.join("\n")
}

const CHAIN_MD_CR17_FIVE: &str = r#"# Price five-way collision

```text
└─ Price::fmt() pkg/price.rs:845
```
"#;

#[test]
fn explicit_line_trait_collision_five_candidates_truncation_counts_all_faces() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-five",
        &[("pkg/price.rs", price_rs_five_impls_fixture())],
        CHAIN_MD_CR17_FIVE,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (trait_name, line) in [
        ("Debug", 830),
        ("Display", 835),
        ("LowerExp", 840),
        ("UpperExp", 845),
        ("Binary", 850),
    ] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]fmt().")),
            "fmt",
            "pkg/price.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out_dir = repo.join(".tours").join("arch").join("02-five");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    // CLI WARN face: first four candidates + the explicit omitted count
    assert!(
        out.stdout.contains(
            "候選 impl#［Price］［Debug］fmt(). :830（Function）／impl#［Price］［Display］fmt(). :835（Function）／impl#［Price］［LowerExp］fmt(). :840（Function）／impl#［Price］［UpperExp］fmt(). :845（Function）／…（另有 1 候選）"
        ),
        "{}",
        out.stdout
    );
    // the omitted candidate never renders
    assert!(!out.stdout.contains("［Binary］"), "{}", out.stdout);
    // written tour: scenario description hint face + kept-step note face
    let tour: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let desc = tour["description"].as_str().unwrap();
    assert!(desc.contains("trait 碰撞候選："), "{desc}");
    assert!(desc.contains("…（另有 1 候選）"), "{desc}");
    let steps = tour["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("trait 碰撞：候選"), "{sdesc}");
    assert!(sdesc.contains("…（另有 1 候選）"), "{sdesc}");
    // the kept anchor is the exact-hit candidate (UpperExp @ 845)
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][UpperExp]fmt().")
    );
}

/// C1 (review): a mid-read failure in the explicit-line candidate set
/// (per-row decode error — BLOB kind here; the schema probe and prepare
/// still pass) must fail closed: `graph-read-error`, a doc-line step
/// with NO minted identity, and no nearest-line fallback — the old
/// filter_map would silently drop the row, empty the set, and route the
/// frame to not-in-graph / cross-file moved-file. The [LOG]
/// distribution and one WARN disclose the frames.
#[test]
fn explicit_line_graph_read_error_fails_closed() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-read-error",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_DRIFT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Debug]fmt()."),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute("UPDATE nodes SET kind=X'00' WHERE name='fmt'", [])
            .unwrap();
    }
    let out_dir = repo.join(".tours").join("arch").join("03-read-error");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    // the doc line itself is still author evidence — a step exists, the
    // run succeeds; the frame is NOT a skip (not a shell)
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("[WARN] graph 讀取錯誤 1 幀——fail-closed 不錨定"),
        "{}",
        out.stdout
    );
    // [LOG] distribution carries the new key
    assert!(out.stdout.contains("graph-read-error"), "{}", out.stdout);
    let tour: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let tour_json = serde_json::to_string(&tour).unwrap();
    assert!(
        !tour_json.contains("x-codeReality"),
        "read-error frame must mint no SCIP identity: {tour_json}"
    );
    let steps = tour["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    // the step sits at the doc anchor line with no re-anchored coordinate
    assert_eq!(steps[0]["line"], 776);
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("graph 讀取錯誤"), "{sdesc}");
    assert!(sdesc.contains("文檔錨 pkg/price.rs:776"), "{sdesc}");
    // no nearest-line / cross-file pick ever happened
    assert!(!out.stdout.contains("moved-file"), "{}", out.stdout);
}

/// F2 (review): the all-shell fail-closed face at run() level — a
/// single-frame corpus whose only frame is a refused collision anchor
/// exits 1 with the trait WARNs on STDERR (the stdout WARN section
/// belongs to the success face) and zero writes: no .tour file.
#[test]
fn run_all_shell_trait_refusal_face_is_stderr_fail_closed() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-f2-shell",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_DRIFT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out_dir = repo.join(".tours").join("arch").join("04-shell");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("[WARN] trait 碰撞"), "{}", out.stderr);
    assert!(out.stderr.contains("全場景殼形"), "{}", out.stderr);
    // the success-face stdout WARN section is not reached on this face
    assert!(
        !out.stdout.contains("[WARN] trait 碰撞"),
        "{}",
        out.stdout
    );
    // zero writes: no .tour file lands under out_dir
    let tour_count = match std::fs::read_dir(&out_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "tour"))
            .count(),
        Err(_) => 0,
    };
    assert_eq!(tour_count, 0, "fail-closed face must write zero tours");
}

/// F3(a) (review): two impl rows sharing the doc anchor LINE — a
/// non-unique exact hit is still a refusal (only `exact.len() == 1`
/// keeps the anchor); both candidates disclose on the description.
#[test]
fn explicit_line_non_unique_exact_hit_refuses() {
    const MD: &str = "# Same-line pair\n\n```text\n└─ Price::fmt() pkg/price.rs:845\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr17-f3a", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for trait_name in ["Debug", "Display"] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]fmt().")),
            "fmt",
            "pkg/price.rs",
            845,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(st.shells_omitted.len(), 1);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(
        desc.contains(
            "impl#［Price］［Debug］fmt(). :845（Function）／impl#［Price］［Display］fmt(). :845（Function）"
        ),
        "{desc}"
    );
}

/// F3(b) (review): a mixed candidate set (impl collision pair + a
/// same-name free function) — the free fn does not trigger the
/// collision guard (non-impl shape) but IS a sibling the legacy
/// nearest-line SQL picked from, so the refusal discloses the FULL
/// same-file same-name set (superset of the colliding pair).
#[test]
fn explicit_line_mixed_set_refusal_lists_full_superset() {
    const MD: &str = "# Mixed set\n\n```text\n└─ Price::fmt() pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr17-f3b", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("fmt()."),
        "fmt",
        "pkg/price.rs",
        830,
        "Function",
        "rust",
        &abs,
    );
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    let desc = st.tours[0]["description"].as_str().unwrap();
    // the full superset renders in (line, symbol) order: free fn first,
    // then the colliding pair — nothing dropped
    assert!(desc.contains("fmt(). :830（Function）"), "{desc}");
    assert!(
        desc.contains("impl#［Price］［Debug］fmt(). :839（Function）"),
        "{desc}"
    );
    assert!(
        desc.contains("impl#［Price］［Display］fmt(). :845（Function）"),
        "{desc}"
    );
}

/// F3(c) (review): more than three trait-collision encounters in one
/// scenario — the description hint details the first three frames and
/// folds the rest into an explicit count (`另有 N 幀（略）`), mirroring
/// the R3 frame-fold idiom.
#[test]
fn explicit_line_fourth_encounter_folds_in_description() {
    const MD: &str = r#"# Four collisions

```text
└─ Price::fmt() pkg/p1.rs:776
└─ Price::fmt() pkg/p2.rs:776
└─ Price::fmt() pkg/p3.rs:776
└─ Price::fmt() pkg/p4.rs:776
```
"#;
    let files: Vec<(String, String)> = ["p1", "p2", "p3", "p4"]
        .iter()
        .map(|p| (format!("pkg/{p}.rs"), price_rs_fixture()))
        .collect();
    let refs: Vec<(&str, String)> = files
        .iter()
        .map(|(rel, content)| (rel.as_str(), content.clone()))
        .collect();
    let (repo, chain) = repo_fixture_cr9("cr17-f3c", &refs, MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    // per-file descriptor prefixes: `rs_sym` folds the descriptor into
    // ONE symbol namespace (`pkg/{descriptor}`), so four files sharing a
    // bare `impl#[Price][…]fmt().` descriptor would collide on the
    // `nodes.symbol` UNIQUE key. Prefixing the file stem keeps each
    // symbol unique while `descriptor_region` (last-`/` boundary) still
    // reads `impl#[Price][…]fmt().` — the resolver face is unchanged.
    for p in ["p1", "p2", "p3", "p4"] {
        for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
            seed_sym(
                &mut spec,
                &rs_sym(&format!("{p}/impl#[Price][{trait_name}]fmt().")),
                "fmt",
                &format!("pkg/{p}.rs"),
                line,
                "Function",
                "rust",
                &abs,
            );
        }
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&4),
        "{:?}",
        st.g_counts
    );
    assert_eq!(st.shells_omitted.len(), 1);
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("trait 碰撞候選："), "{desc}");
    assert!(desc.contains("另有 1 幀（略）"), "{desc}");
    assert!(desc.contains("explicit_trait_ambiguous 4"), "{desc}");
}

const CHAIN_MD_CR17_BOTH: &str = r#"# Price formatting both modes

```text
Price::fmt() pkg/price.rs:776
└─ Price::fmt() pkg/price.rs:845
```
"#;

/// AC-5: the CLI report and the scenario description agree — one stdout
/// WARN per encounter (refusal + exact-hit faces, same candidate
/// lists), the [LOG] distribution carries the same
/// explicit_trait_ambiguous count as the description, and the written
/// tour embeds the same candidate payloads.
#[test]
fn explicit_line_trait_collision_cli_and_description_consistent() {
    let (repo, chain) = repo_fixture_cr9(
        "cr17-cli",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR17_BOTH,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out_dir = repo.join(".tours").join("arch").join("01-trait");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    // one WARN per encounter, refusal first (doc order), same candidates
    assert!(
        out.stdout.contains(
            "[WARN] trait 碰撞：Price::fmt()（文檔錨 pkg/price.rs:776）候選 impl#［Price］［Debug］fmt(). :839（Function）／impl#［Price］［Display］fmt(). :845（Function）——行號已漂移（未精確命中），拒絕最近行號猜測，該幀不產生錨定（explicit_trait_ambiguous）"
        ),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains(
            "[WARN] trait 碰撞：Price::fmt()（文檔錨 pkg/price.rs:845）候選 impl#［Price］［Debug］fmt(). :839（Function）／impl#［Price］［Display］fmt(). :845（Function）——文檔錨行號精確命中，保留該錨；請人工確認 trait 語意（explicit_trait_ambiguous）"
        ),
        "{}",
        out.stdout
    );
    // the [LOG] distribution carries the same independent count
    assert!(
        out.stdout.contains("[LOG] graph 重錨分佈"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("explicit_trait_ambiguous"),
        "{}",
        out.stdout
    );
    // the written tour description embeds the same candidate payloads
    // and the same count as the CLI face
    let tour: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let desc = tour["description"].as_str().unwrap();
    assert!(desc.contains("trait 碰撞候選："), "{desc}");
    assert!(
        desc.contains("impl#［Price］［Debug］fmt(). :839（Function）"),
        "{desc}"
    );
    assert!(
        desc.contains("impl#［Price］［Display］fmt(). :845（Function）"),
        "{desc}"
    );
    assert!(desc.contains("explicit_trait_ambiguous:2"), "{desc}");
    // 2 frames -> 1 step (refused frame skipped), kept step is Display
    assert!(desc.contains("2 幀 → 1 步；1 幀跳過"), "{desc}");
    let steps = tour["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
}

/// AC-4: no trait collision (same-file same-name methods scoped under
/// DIFFERENT types, non-impl descriptors) — the drifted explicit-line
/// anchor keeps the legacy nearest-line selection byte-identically.
#[test]
fn explicit_line_no_collision_drift_keeps_nearest() {
    const MD: &str = "# Nearest kept\n\n```text\n└─ m() (pkg/mv.py:5)\n```\n";
    let src =
        "class A:\n    def m(self):\n        pass\n\n\nclass B:\n    def m(self):\n        pass\n";
    let (repo, chain) = repo_fixture_cr9("cr17-nearest", &[("pkg/mv.py", src.into())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &py_sym("pkg.mv", "A#m()."),
        "m",
        "pkg/mv.py",
        2,
        "Function",
        "python",
        &abs,
    );
    seed_sym(
        &mut spec,
        &py_sym("pkg.mv", "B#m()."),
        "m",
        "pkg/mv.py",
        7,
        "Function",
        "python",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved"), Some(&1), "{:?}", st.g_counts);
    assert!(
        !st.g_counts.contains_key("explicit_trait_ambiguous"),
        "{:?}",
        st.g_counts
    );
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    // |5-7|=2 < |5-2|=3 — the nearest pick, unchanged
    assert_eq!(steps[0]["line"], 7);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        py_sym("pkg.mv", "B#m().")
    );
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(!desc.contains("trait 碰撞候選"), "{desc}");
}

/// Adversarial (SCIP grammar): generic-bearing trait groups
/// (`Add<Self>` / `Sub<Self>`) are unparseable by `descriptor_chain`
/// (R2 refuses them) — the CR-17 collision reader takes bracket groups
/// verbatim, so a drifted anchor over the same-type generic-trait pair
/// still refuses to guess.
#[test]
fn explicit_line_generic_trait_groups_still_collide() {
    const MD: &str = "# Generic traits\n\n```text\n└─ Quantity::add() (pkg/qty.rs:10)\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr17-generics",
        &[("pkg/qty.rs", "struct Quantity;\n".into())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (trait_group, line) in [("`Add<Self>`", 20), ("`Sub<Self>`", 30)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Quantity][{trait_group}]add().")),
            "add",
            "pkg/qty.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved"), "{:?}", st.g_counts);
    let t = st.tours[0].as_object().unwrap();
    assert!(t["steps"].as_array().unwrap().is_empty());
    let desc = t["description"].as_str().unwrap();
    assert!(
        desc.contains("impl#［Quantity］［`Add<Self>`］add(). :20（Function）"),
        "{desc}"
    );
    assert!(
        desc.contains("impl#［Quantity］［`Sub<Self>`］add(). :30（Function）"),
        "{desc}"
    );
}

/// Adjudication pin: an inherent impl (zero trait groups) and a trait
/// impl of the same method on the same type also count as a collision —
/// the hazard (silent nearest-line pick between semantically distinct
/// same-type same-name impls) is the same one R2 already refuses to
/// resolve on the line-less face (the r2_unresolvable fixture shape).
#[test]
fn explicit_line_inherent_vs_trait_impl_counts_as_collision() {
    const MD: &str = "# Inherent vs trait\n\n```text\n└─ Base.run() (pkg/base.rs:7)\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr17-inherent",
        &[("pkg/base.rs", "struct Base;\n".into())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Base]run()."),
        "run",
        "pkg/base.rs",
        6,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Base][WsLab]run()."),
        "run",
        "pkg/base.rs",
        9,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    // |7-6|=1 would have silently picked the inherent impl
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

// ---------- CR-18: md trait-qualifier disambiguation (Price::fmt[Display]) ----------

/// AC-1 pin (pre-fix baseline, the POC trap): `Price::fmt[Display]` with
/// no call parens — best_ident length-ranks `Display`(7) over `fmt`(3),
/// so the frame's query ident is the TRAIT name, not the method. This
/// test ran against the pre-CR-18 tree asserting `ident == "Display"`
/// to prove the trap before flipping to the corrected contract.
#[test]
fn qualifier_frame_parse_selects_method_ident() {
    let frames = parse_frames(&["└─ Price::fmt[Display] pkg/price.rs:776".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/price.rs"));
    assert_eq!(frames[0].line, Some(776));
    // the method is the name-axis ident (SQL name=?); the trait lives in
    // the qualifier, never in the ident
    assert_eq!(frames[0].ident, "fmt");
    // author text survives on the frame symbol (CodeTour title keeps
    // the disambiguation intent)
    assert_eq!(frames[0].symbol, "Price::fmt[Display]");
    // the query identity is the parsed QualifiedMethod triple
    assert_eq!(
        frames[0].qualifier,
        code_reality::symbol_resolve::FrameQualifier::WellFormed(
            code_reality::symbol_resolve::QualifiedMethod {
                receiver: "Price".into(),
                method: "fmt".into(),
                trait_name: "Display".into(),
            }
        )
    );
}

/// AC-1 grammar battery: canonical / compatible / path-parens wrapper
/// forms parse to the same triple; non-qualifier shapes stay legacy
/// (None); qualifier-looking shapes that do not parse are Malformed.
#[test]
fn qualifier_syntax_forms_and_malformed_shapes() {
    use code_reality::symbol_resolve::{
        parse_qualifier, FrameQualifier, QualifiedMethod,
    };
    let wf = |r: &str, m: &str, t: &str| {
        FrameQualifier::WellFormed(QualifiedMethod {
            receiver: r.into(),
            method: m.into(),
            trait_name: t.into(),
        })
    };
    // canonical
    assert_eq!(parse_qualifier("Price::fmt[Display]"), wf("Price", "fmt", "Display"));
    // compatible call-paren form
    assert_eq!(
        parse_qualifier("Price::fmt()[Display]"),
        wf("Price", "fmt", "Display")
    );
    // parameter-carrying call form
    assert_eq!(
        parse_qualifier("Ctx::execute(ctx)[Display]"),
        wf("Ctx", "execute", "Display")
    );
    // multi-segment receiver chain — the immediate type qualifies
    assert_eq!(
        parse_qualifier("model::Price::fmt[Display]"),
        wf("Price", "fmt", "Display")
    );
    // generic trait groups ride through verbatim (bracket depth)
    assert_eq!(
        parse_qualifier("Quantity::add[`Add<Self>`]"),
        wf("Quantity", "add", "`Add<Self>`")
    );
    assert_eq!(
        parse_qualifier("Buf::as_ref[AsRef<[u8]>]"),
        wf("Buf", "as_ref", "AsRef<[u8]>")
    );
    // non-qualifier shapes stay legacy
    assert_eq!(parse_qualifier("Price::fmt()"), FrameQualifier::None);
    assert_eq!(parse_qualifier("Price::fmt"), FrameQualifier::None);
    assert_eq!(parse_qualifier("kernel"), FrameQualifier::None);
    assert_eq!(parse_qualifier("items[0]"), FrameQualifier::None);
    assert_eq!(parse_qualifier(""), FrameQualifier::None);
    // qualifier-looking but broken → Malformed (fail loud, no fallback)
    assert_eq!(parse_qualifier("Price::fmt[Display"), FrameQualifier::Malformed);
    assert_eq!(parse_qualifier("Price::fmt[]"), FrameQualifier::Malformed);
    assert_eq!(parse_qualifier("Price::fmt[Display] junk"), FrameQualifier::Malformed);
    assert_eq!(parse_qualifier("Price::fmt[Display][Debug]"), FrameQualifier::Malformed);
    assert_eq!(parse_qualifier("::fmt[Display]"), FrameQualifier::Malformed);
}

/// AC-1 path-extraction ordering: the `(path:line)` wrapper form — the
/// path ref strips FIRST, the leftover wrapper `()` folds off the kept
/// author text, and the qualifier still parses off the remainder.
#[test]
fn qualifier_path_parens_wrapper_form() {
    let frames = parse_frames(&["└─ Price::fmt[Display] (pkg/price.rs:776)".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/price.rs"));
    assert_eq!(frames[0].line, Some(776));
    assert_eq!(frames[0].ident, "fmt");
    // wrapper residue folded — the title keeps the clean author text
    assert_eq!(frames[0].symbol, "Price::fmt[Display]");
    // compatible form with the wrapper
    let frames = parse_frames(&["└─ Price::fmt()[Display] (pkg/price.rs:776)".to_string()]);
    assert_eq!(frames[0].path.as_deref(), Some("pkg/price.rs"));
    assert_eq!(frames[0].line, Some(776));
    assert_eq!(frames[0].ident, "fmt");
    assert_eq!(frames[0].symbol, "Price::fmt()[Display]");
}

/// CR-18 review F-3 (content-shape tightening, judge half-step): a
/// bracket group whose content carries NO ASCII letter (indexing prose
/// like `self::items[0]` — the trait slot reads "0") is not a trait
/// group; the frame stays legacy (None) instead of minting a bogus
/// triple or skipping as Malformed. Grammar-legal qualifier content
/// always carries ≥1 letter (trait groups are ident/backtick shapes),
/// so this costs zero legal coverage. An EMPTY group stays Malformed
/// (the shape was declared, the trait is missing — pinned at
/// `qualifier_syntax_forms_and_malformed_shapes`).
#[test]
fn qualifier_bracket_content_without_letters_stays_legacy() {
    use code_reality::symbol_resolve::{parse_qualifier, FrameQualifier, QualifiedMethod};
    // indexing prose after `::` — None, never WellFormed(trait="0")
    assert_eq!(parse_qualifier("self::items[0]"), FrameQualifier::None);
    assert_eq!(parse_qualifier("list::items[42]"), FrameQualifier::None);
    // a letterless symbol-only group is prose too
    assert_eq!(parse_qualifier("map::get[$1]"), FrameQualifier::None);
    // lettered content keeps the qualifier shape (digits ride along)
    assert_eq!(
        parse_qualifier("Vec::get[i32]"),
        FrameQualifier::WellFormed(QualifiedMethod {
            receiver: "Vec".into(),
            method: "get".into(),
            trait_name: "i32".into(),
        })
    );
    // empty group stays Malformed — the pinned neighbor above
    assert_eq!(parse_qualifier("Price::fmt[]"), FrameQualifier::Malformed);
}

/// AC-2 unique state + AC-7 NT shape: the doc anchor 776 drifted off
/// both trait impls, but the qualifier names the declared identity —
/// the qualified selector picks Display@845 (bypassing the CR-17
/// collision refusal and its WARN), counts under the legacy `moved`
/// key, and the step carries the Display SCIP symbol.
#[test]
fn qualifier_unique_hit_selects_display_even_on_drift() {
    let (repo, chain) = repo_fixture_cr9(
        "cr18-unique",
        &[("pkg/price.rs", price_rs_fixture())],
        CHAIN_MD_CR18_QUALIFIER_DRIFT,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved"), Some(&1), "{:?}", st.g_counts);
    // declared identity bypasses the collision semantics — no refusal
    // key, no collision encounter payload
    assert!(
        !st.g_counts.contains_key("explicit_trait_ambiguous"),
        "{:?}",
        st.g_counts
    );
    assert!(st.trait_collisions.is_empty(), "no collision WARN");
    assert_eq!(st.skipped, 0);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
    // the step discloses the qualifier evidence (declared identity)
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("qualifier 命中"), "{sdesc}");
    assert!(sdesc.contains("［Display］"), "{sdesc}");
}

const CHAIN_MD_CR18_QUALIFIER_DRIFT: &str = r#"# Price qualifier drift

```text
└─ Price::fmt[Display] pkg/price.rs:776
```
"#;

/// AC-2 no_hit state: the named trait has no impl among the same-file
/// same-name candidates — fail-loud `qualifier_no_hit`, never a nearest
/// fallback, never R1/R2.
#[test]
fn qualifier_no_hit_fails_loud() {
    const MD: &str = "# No hit\n\n```text\n└─ Price::fmt[ToString] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-nohit", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(st.skipped, 1);
    // single-frame all-skip: shell with the qualifier reason on the
    // skip axis
    assert_eq!(st.shells_omitted.len(), 1);
    assert_eq!(
        st.shells_omitted[0].skip_reasons.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.shells_omitted[0].skip_reasons
    );
}

/// AC-2 ambiguous state: two rows satisfy the declared triple (fixture
/// keeps symbols unique via per-row descriptor prefixes) — refuse as
/// `qualifier_ambiguous`, never entering R1/R2 disambiguation.
#[test]
fn qualifier_ambiguous_multiple_matches_refuse() {
    const MD: &str = "# Ambiguous\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-ambig", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (pfx, line) in [("a", 839), ("b", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("{pfx}/impl#[Price][Display]fmt().")),
            "fmt",
            "pkg/price.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    // both matching candidates disclose on the description hint
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display］fmt(). :839（Function）"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display］fmt(). :845（Function）"), "{desc}");
}

/// AC-2 malformed state: qualifier-looking syntax that does not parse
/// (`::` plus an unterminated bracket group) skips the frame with its
/// own key even though the frame carries an explicit line number.
#[test]
fn qualifier_malformed_skips_with_explicit_line() {
    const MD: &str = "# Malformed\n\n```text\n└─ Price::fmt[Display pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-malformed", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_malformed"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(
        st.shells_omitted[0].skip_reasons.get("qualifier_malformed"),
        Some(&1),
        "{:?}",
        st.shells_omitted[0].skip_reasons
    );
}

/// AC-3 triple identity: trait-only matching would hit both candidates;
/// the receiver axis must rule the Other-type Display impl out.
#[test]
fn qualifier_triple_match_rejects_other_receiver() {
    const MD: &str = "# Triple\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-triple", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Other][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        830,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved"), Some(&1), "{:?}", st.g_counts);
    assert!(
        !st.g_counts.contains_key("qualifier_ambiguous"),
        "{:?}",
        st.g_counts
    );
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
}

/// AC-2 line-less face: the qualifier fills the unique Display impl —
/// `filled` at 845 with the Display symbol, bypassing R1/R2.
#[test]
fn qualifier_lineless_fills_unique_impl() {
    const MD: &str = "# Lineless\n\n```text\n└─ Price::fmt[Display] pkg/price.rs\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-lineless", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("filled"), Some(&1), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("qualifier 命中"), "{sdesc}");
}

/// AC-2 three-face consistency: the CLI report carries one WARN per
/// refusal, the [LOG] distribution the same key, and the scenario
/// description the same skip reason + candidate payload.
#[test]
fn qualifier_cli_warn_and_counts_consistent() {
    const MD: &str = r#"# Mixed qualifier faces

```text
└─ Price::fmt[Display] pkg/price.rs:776
└─ Price::fmt[ToString] pkg/price.rs:776
```
"#;
    let (repo, chain) =
        repo_fixture_cr9("cr18-cli", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out_dir = repo.join(".tours").join("arch").join("05-qualifier");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    // WARN face names the refusal, the triple, and the key
    assert!(
        out.stdout.contains("[WARN] qualifier 拒絕：Price::fmt[ToString]"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("qualifier_no_hit"), "{}", out.stdout);
    // [LOG] distribution carries the same independent count
    assert!(
        out.stdout.contains("[LOG] graph 重錨分佈") && out.stdout.contains("qualifier_no_hit"),
        "{}",
        out.stdout
    );
    // written tour description: skip reason + the candidate payload
    let tour: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("01.tour")).unwrap()).unwrap();
    let desc = tour["description"].as_str().unwrap();
    assert!(desc.contains("qualifier_no_hit 1"), "{desc}");
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
    assert!(
        desc.contains("impl#［Price］［Display］fmt(). :845（Function）"),
        "{desc}"
    );
    // the unique-hit frame anchored; the refused frame skipped
    let steps = tour["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
}

/// AC-4 no-qualifier cross-file: single-file convergence over a
/// trait-colliding set refuses under its own key instead of the legacy
/// silent lowest-line pick (F7 hazard).
#[test]
fn cross_file_single_convergence_collision_refuses() {
    const MD: &str = "# Cross file\n\n```text\n└─ render() pkg/old.rs:5\n```\n";
    let old_rs = "// moved out\n// nothing here\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-col",
        &[("pkg/old.rs", old_rs.into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]render().")),
            "render",
            "pkg/new.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("cross_file_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved-file"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("cross_file_trait_ambiguous"), "{desc}");
    assert!(
        desc.contains("impl#［Price］［Display］render(). :845（Function）"),
        "{desc}"
    );
}

/// AC-4 no-collision cross-file: the legacy lowest-line pick survives
/// byte-identically (moved-file + g_symbol).
#[test]
fn cross_file_no_collision_keeps_legacy_pick() {
    const MD: &str = "# Cross file unique\n\n```text\n└─ render() pkg/old.rs:5\n```\n";
    let old_rs = "// moved out\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-ok",
        &[("pkg/old.rs", old_rs.into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]render()."),
        "render",
        "pkg/new.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved-file"), Some(&1), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(steps[0]["file"], "pkg/new.rs");
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]render().")
    );
}

/// AC-4 qualifier cross-file: single-file convergence + declared
/// identity → moved-file with the qualifier-selected symbol.
#[test]
fn cross_file_qualifier_unique_hits_moved_file() {
    const MD: &str =
        "# Cross qualifier\n\n```text\n└─ Price::render[Display] pkg/old.rs:5\n```\n";
    let old_rs = "// moved out\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-qual",
        &[("pkg/old.rs", old_rs.into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]render().")),
            "render",
            "pkg/new.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved-file"), Some(&1), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(steps[0]["file"], "pkg/new.rs");
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]render().")
    );
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("qualifier 命中"), "{sdesc}");
}

/// AC-4 rels>1: multiple target files keep moved-file-ambiguous — the
/// qualifier never selects across files on its own.
#[test]
fn cross_file_multi_file_stays_ambiguous_with_qualifier() {
    const MD: &str =
        "# Cross multi\n\n```text\n└─ Price::render[Display] pkg/old.rs:5\n```\n";
    let old_rs = "// moved out\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-multi",
        &[
            ("pkg/old.rs", old_rs.into()),
            ("pkg/new.rs", price_rs_fixture()),
            ("pkg/other.rs", price_rs_fixture()),
        ],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (pfx, file, line) in [("a", "pkg/new.rs", 839), ("b", "pkg/other.rs", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("{pfx}/impl#[Price][Display]render().")),
            "render",
            file,
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("moved-file-ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    // legacy face: the doc anchor stands as a step, no identity minted —
    // the qualifier never selects across files on its own
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 5);
    let tour_json = serde_json::to_string(&st.tours[0]).unwrap();
    assert!(
        !tour_json.contains("x-codeReality"),
        "multi-file face must mint no identity: {tour_json}"
    );
}

/// AC-4 read integrity: a mid-read decode failure in the cross-file
/// full candidate set degrades explicitly to graph-read-error — never
/// treated as a complete set for a silent pick.
#[test]
fn cross_file_read_error_degrades_explicitly() {
    const MD: &str = "# Cross read error\n\n```text\n└─ render() pkg/old.rs:5\n```\n";
    let old_rs = "// moved out\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-err",
        &[("pkg/old.rs", old_rs.into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]render()."),
        "render",
        "pkg/new.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute("UPDATE nodes SET kind=X'00' WHERE name='render'", [])
            .unwrap();
    }
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("graph-read-error"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved-file"), "{:?}", st.g_counts);
    let tour_json = serde_json::to_string(&st.tours[0]).unwrap();
    assert!(
        !tour_json.contains("x-codeReality"),
        "read-error frame must mint no identity: {tour_json}"
    );
}

/// AC-5 no-qualifier F6: a malformed impl sibling (receiver parses as
/// Price, trait group unterminated) blocks the legacy nearest pick —
/// the set now carries the collision gate (drifted line refuses).
#[test]
fn f6_malformed_sibling_blocks_no_qualifier_nearest() {
    const MD: &str = "# F6 nearest\n\n```text\n└─ Price::fmt() pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-f6-nq", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("m1/impl#[Price][Display"),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("explicit_trait_ambiguous"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
}

/// AC-5 qualifier F6: the unique ValidImpl triple-match + a malformed
/// sibling that may share receiver/method → `qualifier_unverified`
/// (identity not provable, no selection).
#[test]
fn f6_malformed_sibling_blocks_qualifier_unique() {
    const MD: &str = "# F6 qualifier\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-f6-q", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("m1/impl#[Price][Display"),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_unverified"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
}

/// AC-5 ruling-out: a malformed sibling whose parseable receiver is a
/// DIFFERENT type cannot be the declared identity — the unique
/// Price-match still selects.
#[test]
fn f6_malformed_other_receiver_ruled_out() {
    const MD: &str = "# F6 ruled out\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-f6-out", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("m1/impl#[Other][Display"),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved"), Some(&1), "{:?}", st.g_counts);
    assert!(
        !st.g_counts.contains_key("qualifier_unverified"),
        "{:?}",
        st.g_counts
    );
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
}

/// AC-5 nested generic brackets: `AsRef<[u8]>` parses with depth
/// tracking on BOTH the descriptor side and the qualifier side — the
/// first-`]` truncation would read `AsRef<[u8` and never match.
#[test]
fn nested_generic_bracket_depth_qualifier_hit() {
    const MD: &str = "# Nested\n\n```text\n└─ Buf::as_ref[AsRef<[u8]>] pkg/buf.rs:5\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-nested",
        &[("pkg/buf.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Buf][AsRef<[u8]>]as_ref()."),
        "as_ref",
        "pkg/buf.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Buf][Display]as_ref()."),
        "as_ref",
        "pkg/buf.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved"), Some(&1), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 839);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Buf][AsRef<[u8]>]as_ref().")
    );
}

// ---------- CR-18 review fixes (judge verdict batch 1: C1/F-1/C2) ----------

/// Review C1/F-1 (a): explicit-line qualifier frame, ZERO same-file
/// candidates, and the ident IS substring-present in the doc file
/// (`drift-far`) — the legacy substring gate used to answer
/// `not-in-graph` (a doc-line step, zero WARN) while the line-less face
/// refused under `qualifier_no_hit`. Both faces must treat a declared
/// identity the same way: fail-loud skip + WARN, never an unverified
/// step. The graph carries no `fmt` rows anywhere (rels==0).
#[test]
fn qualifier_explicit_zero_same_file_skips_with_warn() {
    const MD: &str = "# Zero same-file\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:5\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-rv-zero", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let spec = graph_db_fixture::GraphDbSpec::default();
    // no fmt rows anywhere: same-file candidates AND cross-file rels are
    // both empty — only the substring gate differs from test (b)
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("not-in-graph"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(st.skipped, 1);
    assert_eq!(
        st.shells_omitted[0].skip_reasons.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.shells_omitted[0].skip_reasons
    );
    // WARN face: single-frame all-skip rides the fail-closed exit-1 face
    // with the refusal WARN on stderr
    let out_dir = repo.join(".tours").join("arch").join("06-rv-zero");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    assert!(
        out.stderr.contains("[WARN] qualifier 拒絕：Price::fmt[Display]（文檔錨 pkg/price.rs:5）"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("qualifier_no_hit"), "{}", out.stderr);
}

/// Review C1/F-1 (b): the zero-rels terminal itself — substring missing
/// AND length ≥ 4 (the legacy gates PASS), zero graph rows for the ident
/// anywhere: the qualifier face must answer `qualifier_no_hit`, not the
/// legacy `not-in-graph` doc-line step.
#[test]
fn qualifier_zero_rels_fails_loud() {
    const MD: &str = "# Zero rels\n\n```text\n└─ Price::render[Display] pkg/old.rs:5\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-rv-rels0",
        &[("pkg/old.rs", "// moved out\n".into())],
        MD,
    );
    let db = db_of(&repo);
    let spec = graph_db_fixture::GraphDbSpec::default();
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("not-in-graph"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(
        st.shells_omitted[0].skip_reasons.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.shells_omitted[0].skip_reasons
    );
    assert_eq!(st.qualifier_refusals.len(), 1);
    assert_eq!(st.qualifier_refusals[0].reason, "qualifier_no_hit");
}

/// Review C2: a SHORT method ident (`fmt`, 3 chars) under the legacy
/// `chars().count() < 4` cross-file gate — the gate used to answer
/// `not-in-graph` before the cross-file convergence query, so
/// `Price::fmt[Display]` relocated to a single new file could never
/// converge. With the gate qm-scoped, the declared identity drives the
/// cross-file convergence: moved-file + the qualifier-selected line +
/// the Display SCIP symbol.
#[test]
fn cross_file_qualifier_short_ident_bypasses_length_gate() {
    const MD: &str = "# Short ident\n\n```text\n└─ Price::fmt[Display] pkg/old.rs:5\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-rv-short",
        &[("pkg/old.rs", "// moved out\n".into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    // the colliding fmt pair lives ONLY in the new file — old file has
    // zero same-file candidates, exactly the C1 empty-rows shape
    for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]fmt().")),
            "fmt",
            "pkg/new.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("moved-file"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("not-in-graph"), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(steps[0]["file"], "pkg/new.rs");
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("qualifier 命中"), "{sdesc}");
}

// ---------- CR-18 review fixes (judge verdict batch 3: F-7 coverage holes) ----------

/// F-7 hole 1: the selector's `0 matches + shadow` arm — NO ValidImpl
/// satisfies the declared triple, and the only candidate is a
/// malformed same-receiver sibling that might BE the identity →
/// `qualifier_unverified` (identity must be proven, never assumed),
/// with the candidate disclosed. The existing f6 battery only covered
/// 1-match + shadow.
#[test]
fn f6_zero_valid_matches_with_shadow_unverified() {
    const MD: &str = "# F6 zero shadow\n\n```text\n└─ Price::fmt[ToString] pkg/price.rs:776\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-f6-0sh", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("m1/impl#[Price][Display"),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_unverified"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("qualifier_no_hit"), "{:?}", st.g_counts);
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    // the shadowing sibling is disclosed on the description face
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display :839（Function）"), "{desc}");
}

/// F-7 hole 2: a qualifier unique hit whose selected row sits at delta
/// zero — counts under the legacy `same` key (the existing
/// `qualifier_unique_hit_selects_display_even_on_drift` pins only the
/// drifted `moved` face).
#[test]
fn qualifier_exact_line_delta_zero_counts_same() {
    const MD: &str = "# Same key\n\n```text\n└─ Price::fmt[Display] pkg/price.rs:845\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-same", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_price_impls(&mut spec, &abs);
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(st.g_counts.get("same"), Some(&1), "{:?}", st.g_counts);
    assert!(!st.g_counts.contains_key("moved"), "{:?}", st.g_counts);
    let steps = st.tours[0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["line"], 845);
    assert_eq!(
        steps[0]["x-codeReality"]["symbol"].as_str().unwrap(),
        rs_sym("impl#[Price][Display]fmt().")
    );
    let sdesc = steps[0]["description"].as_str().unwrap();
    assert!(sdesc.contains("qualifier 命中"), "{sdesc}");
    assert!(sdesc.contains("graph ✓ 行號一致"), "{sdesc}");
}

/// F-7 hole 3 (F-2 connected): the LINE-LESS F6 sibling — unique
/// ValidImpl triple-match + a malformed same-receiver sibling →
/// `qualifier_unverified` via fill_anchor's NoAnchor path. The F-2 fix
/// carries the candidate set through the line-less refusal: BOTH the
/// description hint and the CLI WARN must print non-empty candidates
/// (the judge probe proved payload=0 pre-fix on this exact face).
#[test]
fn qualifier_lineless_f6_shadow_discloses_candidates() {
    const MD: &str = "# F6 lineless\n\n```text\n└─ Price::fmt[Display] pkg/price.rs\n```\n";
    let (repo, chain) =
        repo_fixture_cr9("cr18-f6-ll", &[("pkg/price.rs", price_rs_fixture())], MD);
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    seed_sym(
        &mut spec,
        &rs_sym("impl#[Price][Display]fmt()."),
        "fmt",
        "pkg/price.rs",
        845,
        "Function",
        "rust",
        &abs,
    );
    seed_sym(
        &mut spec,
        &rs_sym("m1/impl#[Price][Display"),
        "fmt",
        "pkg/price.rs",
        839,
        "Function",
        "rust",
        &abs,
    );
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_unverified"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    assert_eq!(st.qualifier_refusals.len(), 1);
    // description face: BOTH candidates (valid match + shadow) render
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display］fmt(). :845（Function）"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display :839（Function）"), "{desc}");
    // WARN face: the same payload rides the all-shell exit-1 stderr
    let out_dir = repo.join(".tours").join("arch").join("07-rv-f6-ll");
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--graph",
        &db.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    assert!(
        out.stderr.contains("[WARN] qualifier 拒絕：Price::fmt[Display]（文檔錨 pkg/price.rs）"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("impl#［Price］［Display :839（Function）／impl#［Price］［Display］fmt(). :845（Function）"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("qualifier_unverified"), "{}", out.stderr);
}

/// F-7 hole 4: cross-file qualifier REFUSAL over a colliding converged
/// set — single-file convergence reaches the colliding render pair,
/// the declared trait (`ToString`) matches neither impl →
/// `qualifier_no_hit` on the cross-file face (never the legacy
/// lowest-line pick, never `cross_file_trait_ambiguous` — the
/// qualifier decides before the collision gate), with the converged
/// file and the FULL colliding candidate set disclosed.
#[test]
fn cross_file_qualifier_refusal_over_colliding_set() {
    const MD: &str = "# Cross refusal\n\n```text\n└─ Price::render[ToString] pkg/old.rs:5\n```\n";
    let (repo, chain) = repo_fixture_cr9(
        "cr18-xf-ref",
        &[("pkg/old.rs", "// moved out\n".into()), ("pkg/new.rs", price_rs_fixture())],
        MD,
    );
    let db = db_of(&repo);
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    for (trait_name, line) in [("Debug", 839), ("Display", 845)] {
        seed_sym(
            &mut spec,
            &rs_sym(&format!("impl#[Price][{trait_name}]render().")),
            "render",
            "pkg/new.rs",
            line,
            "Function",
            "rust",
            &abs,
        );
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let st = build_tours(&chain, &repo, Some(&db)).unwrap();
    assert_eq!(
        st.g_counts.get("qualifier_no_hit"),
        Some(&1),
        "{:?}",
        st.g_counts
    );
    assert!(!st.g_counts.contains_key("moved-file"), "{:?}", st.g_counts);
    assert!(
        !st.g_counts.contains_key("cross_file_trait_ambiguous"),
        "{:?}",
        st.g_counts
    );
    assert!(st.tours[0]["steps"].as_array().unwrap().is_empty());
    // the converged file names the relocation target on the refusal
    assert_eq!(st.qualifier_refusals.len(), 1);
    assert_eq!(
        st.qualifier_refusals[0].converged_file.as_deref(),
        Some("pkg/new.rs")
    );
    // both members of the colliding converged set disclose
    let desc = st.tours[0]["description"].as_str().unwrap();
    assert!(desc.contains("qualifier 拒絕"), "{desc}");
    assert!(desc.contains("（收斂至 pkg/new.rs）"), "{desc}");
    assert!(desc.contains("impl#［Price］［Debug］render(). :839（Function）"), "{desc}");
    assert!(desc.contains("impl#［Price］［Display］render(). :845（Function）"), "{desc}");
}

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
    let hit = anchor.anchor(&repo.join("src/a.rs"), 10, "dup", "exact", None);
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

// --- CR-20 Leg A: the frame grammar covers all eight governed extensions ---

/// The frame-grammar extension table, enumerated INDEPENDENTLY of
/// `language::JS_TS_EXTS` (a constant omission must turn the cross-check
/// below red, never silently shrink these cases). `py`/`rs` are
/// frame-grammar literals on the assembly side; the six JS/TS entries
/// mirror language.rs's frozen set.
const FRAME_TABLE_EXTS: [&str; 8] = ["py", "rs", "js", "jsx", "mjs", "cjs", "ts", "tsx"];

#[test]
fn frame_grammar_eight_extensions_explicit_lineless_and_stray() {
    for ext in FRAME_TABLE_EXTS {
        // Cross-face pin: every frame-grammar extension must be a
        // governed LanguageFace — a grammar entry without a graph face
        // would mint steps the graph can never verify.
        assert!(
            LanguageFace::from_path(Path::new(&format!("f.{ext}"))).is_some(),
            "{ext}: frame-grammar ext must be a governed LanguageFace"
        );
        let path = format!("pkg/a.{ext}");
        // explicit `path:line`
        let frames = parse_frames(&[format!("└─ boot() ({path}:5)")]);
        assert_eq!(
            frames[0].path.as_deref(),
            Some(path.as_str()),
            "{ext} explicit path"
        );
        assert_eq!(frames[0].line, Some(5), "{ext} explicit line");
        // line-less tail: grammar accepts, line stays None
        let frames = parse_frames(&[format!("└─ boot() ({path})")]);
        assert_eq!(
            frames[0].path.as_deref(),
            Some(path.as_str()),
            "{ext} lineless path"
        );
        assert_eq!(frames[0].line, None, "{ext} lineless line");
        // stray second path token in the symbol remainder → no anchor
        // (noref): `see foo.{ext} bar.py` is prose, not an anchor
        let frames = parse_frames(&[format!("└─ see foo.{ext} bar.py")]);
        assert_eq!(frames[0].path, None, "{ext} stray");
        assert_eq!(frames[0].line, None, "{ext} stray");
        // suffix junk rejects the line-less tail (extension must be the
        // token's true end)
        let frames = parse_frames(&[format!("└─ x ({path}.bak)")]);
        assert_eq!(frames[0].path, None, "{ext} .bak");
    }
}

#[test]
fn frame_grammar_long_extension_first_pin() {
    // `any_path_re` has no trailing anchor: a short-first alternation
    // would let `foo.tsx` match as `foo.ts` and `stray_path_token` would
    // then read the trailing `x` as a word character — the frame would
    // mint a line-less anchor off the trailing `bar.py`. Both long-ext
    // remainders must stay stray → noref (tsx/jsx pinned first).
    for frame in ["└─ see foo.tsx bar.py", "└─ see foo.jsx bar.js"] {
        let frames = parse_frames(&[frame.to_string()]);
        assert_eq!(frames[0].path, None, "{frame}");
        assert_eq!(frames[0].line, None, "{frame}");
    }
}

#[test]
fn frame_grammar_table_covers_every_js_ts_ext() {
    // Drift guard (muse Q1 cross-check): when language.rs grows the
    // JS/TS face set, this goes red until the frame-grammar table (and
    // thus the assembled alternation) explicitly reckons with the
    // newcomer. The cases above stay independently enumerated — the
    // constant is only the completeness oracle here.
    for ext in JS_TS_EXTS {
        assert!(
            FRAME_TABLE_EXTS.contains(&ext),
            "JS/TS ext {ext} missing from the frame-grammar table"
        );
    }
    assert!(FRAME_TABLE_EXTS.contains(&"py"), "py stays a frame literal");
    assert!(FRAME_TABLE_EXTS.contains(&"rs"), "rs stays a frame literal");
}
