//! R5-S1 tour trio tests — mirrors of test_tour_manifest /
//! test_tour_validate / test_tour_upgrade case families.

use code_reality::tour_manifest;
use code_reality::tour_upgrade;
use code_reality::tour_validate;
use serde_json::json;
use std::path::{Path, PathBuf};

fn repo_fixture(tag: &str) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(&repo).unwrap();
    repo
}

fn write_tour(root: &Path, rel: &str, title: &str, steps: &[serde_json::Value]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let body = json!({
        "title": title,
        "description": format!("tour {title}"),
        "steps": steps,
    });
    std::fs::write(
        &p,
        format!("{}\n", serde_json::to_string_pretty(&body).unwrap()),
    )
    .unwrap();
}

fn step(file: &str, line: i64, desc: &str) -> serde_json::Value {
    json!({"file": file, "line": line, "description": desc})
}

// ---------- tour_manifest ----------

#[test]
fn manifest_upsert_dump_roundtrip_with_unknown_keys() {
    let repo = repo_fixture("manifest");
    let mpath = repo.join(".tours").join("manifest.toml");
    std::fs::create_dir_all(mpath.parent().unwrap()).unwrap();
    // hand-written key that must survive upsert+dump roundtrips
    std::fs::write(
        &mpath,
        "version = 1\naudience = \"newcomer\"\n\n[tour.\"a.tour\"]\ngenerator = \"manual\"\nsources = []\nanchored_commit = \"aaa\"\nnote-level = 7\n",
    )
    .unwrap();
    let mut m = tour_manifest::load(&mpath).unwrap();
    tour_manifest::upsert(
        &mut m,
        "b.tour",
        "chain_tour",
        &["src/x.py".to_string()],
        "bbb",
    );
    tour_manifest::dump(&mpath, &m).unwrap();
    let text = std::fs::read_to_string(&mpath).unwrap();
    assert!(text.contains("audience = \"newcomer\""), "{text}");
    assert!(text.contains("[tour.\"a.tour\"]"));
    assert!(text.contains("note-level = 7"), "row unknown key preserved");
    assert!(text.contains("[tour.\"b.tour\"]"));
    assert!(text.contains("sources = [\"src/x.py\"]"));
    // reload → still parses with all keys
    let m2 = tour_manifest::load(&mpath).unwrap();
    assert_eq!(m2.tour.len(), 2);
}

#[test]
fn manifest_init_scan_generator_guess_and_skip_dirs() {
    let repo = repo_fixture("initscan");
    let tours = repo.join(".tours");
    std::fs::create_dir_all(tours.join("arch")).unwrap();
    std::fs::create_dir_all(tours.join("delta")).unwrap();
    std::fs::create_dir_all(tours.join("dev-fixture")).unwrap();
    write_tour(&repo, ".tours/arch/chain-foo.tour", "01 - Foo", &[]);
    write_tour(&repo, ".tours/arch/07.tour", "07 - Bar", &[]);
    write_tour(&repo, ".tours/arch/hand-made.tour", "Hand", &[]);
    write_tour(&repo, ".tours/delta/regen.tour", "d", &[]); // skipped
                                                            // non-git repo → warn + "unknown"
    let (data, warn) = tour_manifest::init_scan(&repo, Path::new(".tours")).unwrap();
    assert!(warn.contains("[WARN] git HEAD 取不到"), "{warn}");
    assert_eq!(data.tour.len(), 3);
    assert_eq!(
        data.tour["arch/chain-foo.tour"]
            .get("generator")
            .unwrap()
            .as_str(),
        Some("chain_tour")
    );
    assert_eq!(
        data.tour["arch/07.tour"].get("generator").unwrap().as_str(),
        Some("chain_tour")
    );
    assert_eq!(
        data.tour["arch/hand-made.tour"]
            .get("generator")
            .unwrap()
            .as_str(),
        Some("manual")
    );
    assert!(
        data.tour["arch/07.tour"]
            .get("anchored_commit")
            .unwrap()
            .as_str()
            == Some("unknown")
    );
}

#[test]
fn manifest_tours_root_walk() {
    let repo = repo_fixture("root");
    std::fs::create_dir_all(repo.join(".tours").join("arch").join("sub")).unwrap();
    let root = tour_manifest::tours_root_of(&repo.join(".tours").join("arch").join("sub"));
    assert_eq!(root.file_name().unwrap(), ".tours");
    let nowhere = tour_manifest::tours_root_of(&repo);
    assert_ne!(nowhere.file_name().unwrap_or_default(), ".tours");
}

// ---------- tour_validate ----------

#[test]
fn ts_key_strips_number_prefix() {
    assert_eq!(tour_validate::ts_key("03 - Foo Bar"), "Foo Bar");
    assert_eq!(tour_validate::ts_key("#12 -X"), "X");
    assert_eq!(tour_validate::ts_key("Plain Title"), "Plain Title");
    // truncation at the FIRST hyphen (codetour semantics)
    assert_eq!(tour_validate::ts_key("05 - A - B"), "A");
}

#[test]
fn iter_tours_excludes_delta_by_default() {
    let repo = repo_fixture("iter");
    write_tour(&repo, ".tours/a.tour", "01 - A", &[]);
    write_tour(&repo, ".tours/delta/d.tour", "D", &[]);
    let default = tour_validate::iter_tours(&repo, Path::new(".tours"), false).unwrap();
    assert_eq!(default.len(), 1);
    let all = tour_validate::iter_tours(&repo, Path::new(".tours"), true).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn validate_link_and_anchor_faces() {
    let repo = repo_fixture("validate");
    write_tour(
        &repo,
        ".tours/01.tour",
        "01 - Target",
        &[step("src/a.py", 1, "base")],
    );
    write_tour(
        &repo,
        ".tours/02.tour",
        "02 - Linker",
        &[
            step(
                "src/a.py",
                1,
                "see [Target][Target#1] and [Missing][ghost key]",
            ),
            step("src/a.py", 99, "prose [not a link] here"),
            step("src/gone.py", 1, "missing file"),
        ],
    );
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/a.py"),
        "fn anchor_line() {}\nfn other() {}\n",
    )
    .unwrap();
    // give step1 a valid pattern and step2 no anchor fields
    let p = repo.join(".tours/02.tour");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    v["steps"][0]["pattern"] = json!("^fn anchor_line");
    v["steps"][1].as_object_mut().unwrap().remove("line");
    v["steps"][2].as_object_mut().unwrap().remove("line");
    std::fs::write(&p, serde_json::to_string(&v).unwrap()).unwrap();
    let out = tour_validate::validate(&repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stdout
            .contains("[FAIL] .tours/02.tour 步1 tour link 無/歧義目標: ghost key"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("[WARN] .tours/02.tour 步2 單括號非 link 文字: [not a link]"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("[OK] tour validate: 2 tours | links=1"),
        "{}",
        out.stdout
    );
    // anchor corrected path: bump the line so the pattern re-anchors
    let mut v2 = v.clone();
    v2["steps"][0]["line"] = json!(2);
    std::fs::write(&p, serde_json::to_string(&v2).unwrap()).unwrap();
    let out2 = tour_validate::validate(&repo, Path::new(".tours"), false);
    assert!(
        out2.stdout.contains("錨 corrected: src/a.py L2->L1"),
        "{}",
        out2.stdout
    );
}

// ---------- tour_upgrade ----------

#[test]
fn build_step_pattern_decl_and_backtick_fallback() {
    let repo = repo_fixture("pattern");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/m.rs"),
        "struct Foo;\nimpl Foo {\n    pub fn bar(&self) {}\n}\n",
    )
    .unwrap();
    let decl_step = json!({"file": "src/m.rs", "line": 3, "description": "the `fn bar` decl"});
    let pat = tour_upgrade::build_step_pattern(&decl_step, &repo).unwrap();
    // regex::escape escapes a wider set than Python re.escape (e.g. `&`)
    // — matching semantics identical, pattern-string bytes are a recorded
    // non-gate boundary (same decision as common::anchor_pattern)
    assert_eq!(pat, "^[ \\t]*pub fn bar\\(\\&self\\) \\{\\}[ \\t]*$");
    // nearest-hit must equal the original line, else None
    let off = json!({"file": "src/m.rs", "line": 4, "description": ""});
    assert!(tour_upgrade::build_step_pattern(&off, &repo).is_none());
    // backtick fallback: target line NOT a declaration itself, within ±1
    // of the backtick-named one (file: line1 let, line2 struct decl)
    std::fs::write(repo.join("src/m.rs"), "let x = 1;\nstruct Foo;\n").unwrap();
    let near = json!({"file": "src/m.rs", "line": 1, "description": "see `struct Foo`"});
    let pat2 = tour_upgrade::build_step_pattern(&near, &repo).unwrap();
    assert_eq!(pat2, "^[ \\t]*struct Foo;[ \\t]*$");
}

#[test]
fn sanitize_brackets_and_crossrefs() {
    // frozen quirks (faithful): (a) the TEXT bracket of a double-bracket
    // link converts (only ](/][ adjacent pass); (b) an END-OF-STRING
    // bracket passes — Python's `after` is "" and `"" in "]("` is True
    let (out, n) = tour_upgrade::sanitize_brackets(
        "keep [link][Target#1] and [md](./f.py); rust #[derive] is code [inner]",
    );
    assert_eq!(n, 2);
    assert!(out.contains("［derive］"), "{out}");
    assert!(out.contains("［link］[Target#1]"), "{out}");
    assert!(out.ends_with("code [inner]"), "{out}");
    let (mid, n2) = tour_upgrade::sanitize_brackets("a [x] b");
    assert_eq!((mid.as_str(), n2), ("a ［x］ b", 1));
    assert!(out.contains("[md](./f.py)"));
    let mut keys = std::collections::BTreeMap::new();
    keys.insert(3i64, "Target".to_string());
    let (out2, n2) = tour_upgrade::revive_crossrefs("as noted in [3 - Target]", &keys);
    assert_eq!(n2, 1);
    assert_eq!(out2, "as noted in [Target][Target#1]");
}

#[test]
fn upgrade_dry_run_and_apply() {
    let repo = repo_fixture("upgrade");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/m.rs"), "struct Foo;\nfn main() {}\n").unwrap();
    write_tour(
        &repo,
        ".tours/01.tour",
        "01 - One",
        &[step("src/m.rs", 1, "starts [1 - One]")],
    );
    let out = code_reality::tour_upgrade::run(&["tour_upgrade", "--repo", &repo.to_string_lossy()]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("[OK] tour_upgrade DRY-RUN: 1 tours | pattern +1 skip 0 | crossref 1"),
        "{}",
        out.stdout
    );
    // dry-run: file untouched
    let raw = std::fs::read_to_string(repo.join(".tours/01.tour")).unwrap();
    assert!(!raw.contains("pattern"), "{raw}");
    // apply: writes pattern + revived crossref + manifest
    let out2 = code_reality::tour_upgrade::run(&[
        "tour_upgrade",
        "--repo",
        &repo.to_string_lossy(),
        "--apply",
    ]);
    assert!(
        out2.stdout.contains("[OK] tour_upgrade APPLY: 1 tours"),
        "{}",
        out2.stdout
    );
    let raw2 = std::fs::read_to_string(repo.join(".tours/01.tour")).unwrap();
    assert!(raw2.contains("\"pattern\""), "{raw2}");
    assert!(raw2.contains("［One］[One#1]"), "{raw2}");
    let manifest = std::fs::read_to_string(repo.join(".tours/manifest.toml")).unwrap();
    assert!(manifest.contains("[tour.\"01.tour\"]"), "{manifest}");
    assert!(manifest.contains("generator = \"manual\""));
}

// ---------- tour_validate acceptance faces (mosaic relay 2026-08-27) ----------
// Post-R7 the parity harness is retired; cargo synthetic-repo tests are
// the sole gate face. These three pin the 2026-08-26 relative-path bug
// class (glob root silently finding 0 tours) that shipped because no
// face existed.

fn clean_corpus(repo: &Path) {
    write_tour(
        repo,
        ".tours/arch/alpha/01.tour",
        "01 - Alpha",
        &[step("src/a.py", 1, "entry point")],
    );
    write_tour(
        repo,
        ".tours/arch/alpha/02.tour",
        "02 - Second",
        &[step("src/a.py", 2, "next")],
    );
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.py"), "fn one() {}\nfn two() {}\n").unwrap();
}

#[test]
fn validate_absolute_tours_dir_matches_relative() {
    let repo = repo_fixture("abs-rel");
    clean_corpus(&repo);
    let rel = tour_validate::validate(&repo, Path::new(".tours"), false);
    let abs = tour_validate::validate(
        &repo,
        &std::fs::canonicalize(repo.join(".tours")).unwrap(),
        false,
    );
    assert_eq!(rel.exit_code, 0, "{}{}", rel.stdout, rel.stderr);
    assert_eq!(abs.exit_code, 0, "{}{}", abs.stdout, abs.stderr);
    assert_eq!(
        rel.stdout, abs.stdout,
        "relative and absolute --tours-dir must yield identical output"
    );
    assert!(rel.stdout.contains("fails=0"), "{}", rel.stdout);
    assert!(
        !rel.stdout.contains("無 .tour"),
        "glob root regression: {}",
        rel.stdout
    );
}

#[test]
fn validate_happy_path_fails_zero_nonzero_tours() {
    let repo = repo_fixture("happy");
    clean_corpus(&repo);
    let out = tour_validate::validate(&repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("2 tours"), "{}", out.stdout);
    assert!(out.stdout.contains("fails=0"), "{}", out.stdout);
}

#[test]
fn validate_manifest_mode_fails_zero() {
    let repo = repo_fixture("manifest-face");
    clean_corpus(&repo);
    // manifest aligned with the corpus files
    let mpath = repo.join(".tours/manifest.toml");
    let mut m = tour_manifest::Manifest::default();
    tour_manifest::upsert(
        &mut m,
        "arch/alpha/01.tour",
        "chain_tour",
        &["md/a.md".into()],
        "aaa",
    );
    tour_manifest::upsert(
        &mut m,
        "arch/alpha/02.tour",
        "chain_tour",
        &["md/a.md".into()],
        "aaa",
    );
    std::fs::create_dir_all(repo.join("md")).unwrap();
    std::fs::write(repo.join("md/a.md"), "# a\n").unwrap();
    tour_manifest::dump(&mpath, &m).unwrap();
    let out = tour_validate::validate(&repo, Path::new(".tours"), true);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("fails=0"), "{}", out.stdout);
    // and a manifest pointing at a missing file must fail loud
    let mut m2 = tour_manifest::Manifest::default();
    tour_manifest::upsert(
        &mut m2,
        "arch/alpha/gone.tour",
        "chain_tour",
        &["md/a.md".into()],
        "aaa",
    );
    tour_manifest::dump(&mpath, &m2).unwrap();
    let out2 = tour_validate::validate(&repo, Path::new(".tours"), true);
    assert!(
        out2.stdout.contains("FAIL") || out2.exit_code != 0,
        "{}",
        out2.stdout
    );
}
