//! CR-2 companion suite — `tour_manifest` dump's table-valued extra
//! keys (`[family."NN"]` label/journey, `[route.name]`): load accepts
//! them via the unknown-key extra roundtrip, so dump must hand them
//! back or the load→upsert→re-dump pipe partial-writes (tour files
//! landed, manifest provenance lost — the chain_tour post-generation
//! leg and `tour_manifest --init-scan` share the defect). Separate
//! suite by scope fence: the previous CR-2 leg's four files stay
//! untouched by this fix.

use code_reality::tour_manifest;
use std::path::PathBuf;

/// Hand-written corpus manifest: scalar extra + family table (the
/// chain_tour label source) + route table (nested bare-key sub-table).
const FAMILY_MANIFEST: &str = concat!(
    "version = 1\n",
    "audience = \"newcomer\"\n",
    "\n",
    "[family.\"01\"]\n",
    "label = \"auth\"\n",
    "journey = \"boot → login\"\n",
    "\n",
    "[route.core-day]\n",
    "blurb = \"core\"\n",
    "order = 2\n",
    "\n",
    "[notes]\n",
);

fn table<'a>(m: &'a tour_manifest::Manifest, key: &str) -> &'a toml::value::Table {
    let (_, v) = m
        .extra
        .iter()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("extra key {key} missing: {:?}", m.extra));
    v.as_table()
        .unwrap_or_else(|| panic!("extra key {key} is not a table"))
}

#[test]
fn manifest_dump_roundtrips_table_valued_extras() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("table-extras");
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let mpath = repo.join(".tours").join("manifest.toml");
    std::fs::write(&mpath, FAMILY_MANIFEST).unwrap();

    let mut m = tour_manifest::load(&mpath).unwrap();
    tour_manifest::upsert(
        &mut m,
        "arch/01-auth/01.tour",
        "chain_tour",
        &["chain.md".to_string()],
        "c0",
    );
    tour_manifest::dump(&mpath, &m).unwrap();

    // reload → family/route tables survive key-by-key, scalar extra too
    let m2 = tour_manifest::load(&mpath).unwrap();
    let f01 = table(&m2, "family").get("01").unwrap().as_table().unwrap();
    assert_eq!(f01.len(), 2, "{f01:?}");
    assert_eq!(f01.get("label").unwrap().as_str(), Some("auth"));
    assert_eq!(f01.get("journey").unwrap().as_str(), Some("boot → login"));
    let route = table(&m2, "route")
        .get("core-day")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(route.len(), 2, "{route:?}");
    assert_eq!(route.get("blurb").unwrap().as_str(), Some("core"));
    assert_eq!(route.get("order").unwrap().as_integer(), Some(2));
    // an empty table extra is data too — it must not vanish on dump
    assert!(table(&m2, "notes").is_empty());
    assert!(m2
        .extra
        .iter()
        .any(|(k, v)| k == "audience" && v.as_str() == Some("newcomer")));
    // the upserted provenance row landed alongside the preserved tables
    assert!(m2.tour.contains_key("arch/01-auth/01.tour"));

    // dump→load→dump is byte-idempotent (dump sorts, no format churn)
    let text1 = std::fs::read_to_string(&mpath).unwrap();
    let m3 = tour_manifest::load(&mpath).unwrap();
    tour_manifest::dump(&mpath, &m3).unwrap();
    let text2 = std::fs::read_to_string(&mpath).unwrap();
    assert_eq!(text1, text2);
}

// ---------- CR-2 leg-3 (F4): deep nesting roundtrip + leaf type errors
// naming the full dotted path ----------

#[test]
fn manifest_dump_roundtrips_three_level_nested_table() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("nested3");
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    std::fs::write(
        &mpath,
        concat!(
            "version = 1\n",
            "\n",
            "[grid.\"01\".phase]\n",
            "name = \"cold\"\n",
            "\n",
            "[grid.\"01\".meta]\n",
            "order = 2\n",
        ),
    )
    .unwrap();
    let mut m = tour_manifest::load(&mpath).unwrap();
    tour_manifest::upsert(
        &mut m,
        "arch/01-auth/01.tour",
        "chain_tour",
        &["chain.md".to_string()],
        "c0",
    );
    tour_manifest::dump(&mpath, &m).unwrap();

    // reload → every nesting level survives, provenance row alongside
    let m2 = tour_manifest::load(&mpath).unwrap();
    let g01 = table(&m2, "grid").get("01").unwrap().as_table().unwrap();
    let phase = g01.get("phase").unwrap().as_table().unwrap();
    assert_eq!(phase.get("name").unwrap().as_str(), Some("cold"));
    let meta = g01.get("meta").unwrap().as_table().unwrap();
    assert_eq!(meta.get("order").unwrap().as_integer(), Some(2));
    assert!(m2.tour.contains_key("arch/01-auth/01.tour"));

    // dump→load→dump byte-idempotent at depth too
    let text1 = std::fs::read_to_string(&mpath).unwrap();
    let m3 = tour_manifest::load(&mpath).unwrap();
    tour_manifest::dump(&mpath, &m3).unwrap();
    assert_eq!(text1, std::fs::read_to_string(&mpath).unwrap());
}

#[test]
fn manifest_dump_leaf_type_error_names_dotted_path() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("leaf-err");
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    // datetime leaf inside a nested family table — a TOML value `dump`
    // cannot serialize
    std::fs::write(
        &mpath,
        concat!(
            "version = 1\n",
            "\n",
            "[family.\"01\"]\n",
            "label = \"auth\"\n",
            "when = 1979-05-27T07:32:00Z\n",
        ),
    )
    .unwrap();
    let m = tour_manifest::load(&mpath).unwrap();
    let err = tour_manifest::dump(&mpath, &m).unwrap_err();
    // the nested leaf's full dotted path — no more false 「頂層」 claim
    assert!(err.contains("family.01.when"), "{err}");
    assert!(!err.contains("頂層"), "{err}");
    // dump aborted before any write — the planted bytes survive
    let text = std::fs::read_to_string(&mpath).unwrap();
    assert!(text.contains("label = \"auth\""), "{text}");
}

// ---------- CR-2 leg-4 (R1): known-key wrong types are loud on load ----------
// `load` used to drop a parseable-but-wrong-typed `tour`/`delta_arc`
// silently — a later dump would then evaporate the rows wholesale (the
// F1b doctrine applied at the load boundary). Three states, one red
// test each: `tour` not a table / a `[tour]` row not a table /
// `delta_arc` not an array.

fn wrong_type_repo(tag: &str, manifest: &str) -> (PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let mpath = repo.join(".tours/manifest.toml");
    std::fs::write(&mpath, manifest).unwrap();
    (repo, mpath)
}

#[test]
fn load_wrong_typed_tour_key_fails_loud() {
    let (_repo, mpath) = wrong_type_repo("r1-tour-key", "version = 1\ntour = 7\n");
    let err = tour_manifest::load(&mpath).unwrap_err();
    assert!(err.contains("tour 非 table"), "{err}");
    assert!(err.contains("integer"), "{err}");
}

#[test]
fn load_wrong_typed_tour_row_fails_loud() {
    let (_repo, mpath) = wrong_type_repo("r1-tour-row", "version = 1\n\n[tour]\n\"a.tour\" = 7\n");
    let err = tour_manifest::load(&mpath).unwrap_err();
    // the row's key and the actual type — not just a generic refusal
    assert!(err.contains("a.tour"), "{err}");
    assert!(err.contains("integer"), "{err}");
}

#[test]
fn load_wrong_typed_delta_arc_fails_loud() {
    let (_repo, mpath) = wrong_type_repo("r1-delta-arc", "version = 1\ndelta_arc = \"x\"\n");
    let err = tour_manifest::load(&mpath).unwrap_err();
    assert!(err.contains("delta_arc 非 array"), "{err}");
    assert!(err.contains("string"), "{err}");
}

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
"#;

/// Minimal git corpus mirroring the s5_chain_tour fixture (chain md +
/// module profile + anchored sources + committed HEAD), with the
/// family-table manifest planted under `.tours/`.
fn chain_repo(tag: &str) -> PathBuf {
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
    std::fs::write(repo.join("chain.md"), CHAIN_MD).unwrap();
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    std::fs::write(repo.join(".tours/manifest.toml"), FAMILY_MANIFEST).unwrap();
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
    repo
}

#[test]
fn chain_tour_manifest_upsert_keeps_family_table() {
    let repo = chain_repo("fam-e2e");
    let chain = repo.join("chain.md");
    // family-numbered out_dir: write_tours reads the manifest family
    // label, then run() does load→upsert→re-dump on the same manifest
    let out = code_reality::chain_tour::run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join(".tours/arch/01-auth").to_string_lossy(),
    ]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(
        out.stdout.contains("[OK] manifest upsert:"),
        "{}",
        out.stdout
    );

    // tour files + provenance landed; family table still in the manifest
    assert!(repo.join(".tours/arch/01-auth/01.tour").exists());
    let mpath = repo.join(".tours/manifest.toml");
    let text = std::fs::read_to_string(&mpath).unwrap();
    assert!(text.contains("[family."), "{text}");
    assert!(text.contains("label = \"auth\""), "{text}");
    assert!(text.contains("[tour.\"arch/01-auth/01.tour\"]"), "{text}");
    let m = tour_manifest::load(&mpath).unwrap();
    let f01 = table(&m, "family").get("01").unwrap().as_table().unwrap();
    assert_eq!(f01.get("label").unwrap().as_str(), Some("auth"));
    assert_eq!(f01.get("journey").unwrap().as_str(), Some("boot → login"));
    let row = &m.tour["arch/01-auth/01.tour"];
    assert_eq!(row.get("generator").unwrap().as_str(), Some("chain_tour"));
    assert_eq!(row.get("sources").unwrap().as_array().unwrap().len(), 1);
}
