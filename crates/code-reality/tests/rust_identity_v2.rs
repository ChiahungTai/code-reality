//! Identity v2 — Rust producer inputs enter the freshness trust
//! boundary (EP DB-100 S2; frozen TC-4/5/6/9/10).
//!
//! v2 contract: Cargo manifests/locks/toolchain config of the resolved
//! workspace roots (`rust_inputs`) plus the topology descriptor
//! (sorted roots + discovery mode) are identity-relevant — a Cargo-only
//! change must NOT answer fresh. v1/legacy/keyless slots with a Rust
//! face answer `identity-algo-upgrade` stale (never fresh); non-Rust v1
//! slots keep the legacy-signals face (one-time degradation, TC-9).

mod support;

use code_reality::build::build_repo;
use code_reality::engine::meta_path;
use code_reality::freshness::freshness;
use std::path::Path;

fn verdict_json(out: &code_reality::ToolOutput) -> serde_json::Value {
    serde_json::from_str(out.stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not the verdict JSON ({e}): {:?}", out.stdout))
}

struct Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

/// Rust-face lab: root manifest + lockfile + the fixture's exact rust
/// docs (rich.scip carries crates/a.rs + crates/b.rs — the stamp's
/// docs==disk equality gate needs the fake producer output to match the
/// governed corpus, mirroring the deriving py/ts fakes).
fn lab_cargo() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(repo.join("Cargo.lock"), "version = 3\n").unwrap();
    std::fs::create_dir_all(repo.join("crates")).unwrap();
    std::fs::write(repo.join("crates/a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo.join("crates/b.rs"), "pub fn b() {}\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    install_rust_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

/// Fake rust-analyzer: emits the shared rich.scip fixture for any scip
/// invocation (identity tests assert meta/freshness behavior, not doc
/// content — path shapes come with S3's normalize tests).
fn install_rust_fake(dir: &Path) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");
    support::fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) shift; cp '{fixture}' \"$1\"; exit 0;; esac; shift; done\nexit 1\n"
        ),
    );
}

/// Python-only lab (no Cargo at all) — the TC-9 one-time-degradation face.
fn lab_python() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("app.py"), "x = 1\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

impl Lab {
    fn build(&self) {
        build_repo(&self.repo, None, &self.roots).expect("build");
    }
    fn downgrade_meta_algo_to_v1(&self) {
        let p = meta_path(&code_reality::engine::default_index_path(&self.repo).unwrap());
        let text = std::fs::read_to_string(&p).unwrap();
        let v: serde_json::Value = text.parse().expect("meta json");
        let mut v = v;
        v["identity_algo"] = serde_json::json!("sha256-v1");
        std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
    fn strip_identity_triple(&self) {
        let p = meta_path(&code_reality::engine::default_index_path(&self.repo).unwrap());
        let text = std::fs::read_to_string(&p).unwrap();
        let mut v: serde_json::Value = text.parse().expect("meta json");
        for k in [
            "source_identity",
            "identity_algo",
            "source_faces",
            "index_sha256",
        ] {
            v.as_object_mut().unwrap().remove(k);
        }
        std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
    fn verdict(&self) -> (i32, serde_json::Value) {
        let out = freshness(&self.repo, true);
        (out.exit_code, verdict_json(&out))
    }
}

// --- TC-4: Cargo-only change → stale (never fresh) ---

#[test]
fn tc4_cargo_lock_only_change_is_stale() {
    let l = lab_cargo();
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after build: {v:?}");
    assert_eq!(v["fresh"], serde_json::json!(true));

    // Cargo-only edit: lockfile content changes, zero .rs/.py/.mjs touch.
    std::fs::write(l.repo.join("Cargo.lock"), "version = 3\n# edited\n").unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "TC4-a: cargo-only change is stale: {v:?}");
    assert_eq!(v["fresh"], serde_json::json!(false), "TC4-b");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "content-drift"),
        "TC4-c: content-drift in {reasons:?}"
    );
    assert_eq!(
        v["serves"],
        serde_json::json!("committed-baseline"),
        "TC4-c"
    );
}

// --- TC-5: v2 content-addressing (same construction ⇒ same identity) ---

#[test]
fn tc5_same_content_two_checkouts_same_identity() {
    let l1 = lab_cargo();
    l1.build();
    let l2 = lab_cargo();
    l2.build();
    let m1 = std::fs::read_to_string(meta_path(
        &code_reality::engine::default_index_path(&l1.repo).unwrap(),
    ))
    .unwrap();
    let m2 = std::fs::read_to_string(meta_path(
        &code_reality::engine::default_index_path(&l2.repo).unwrap(),
    ))
    .unwrap();
    let id1: serde_json::Value = m1.parse().unwrap();
    let id2: serde_json::Value = m2.parse().unwrap();
    assert_eq!(
        id1["source_identity"], id2["source_identity"],
        "TC5-a: same content+topology in different checkouts ⇒ same identity"
    );
    assert_eq!(
        id1["identity_algo"],
        serde_json::json!("sha256-v2"),
        "TC5-c: v2 algo stamped"
    );
}

#[test]
fn tc5d_config_cosmetic_edit_stays_fresh() {
    let l = lab_cargo();
    l.build();
    // Cosmetic-only config edit: roots/mode unchanged ⇒ identity must not
    // move (the topology descriptor carries the EFFECT of the config).
    std::fs::write(l.repo.join(".code-reality.toml"), "# a cosmetic comment\n").unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "TC5-d: cosmetic config edit stays fresh: {v:?}");
}

// --- TC-6: v1/legacy Rust slots answer identity-algo-upgrade (never fresh) ---

#[test]
fn tc6_v1_rust_slot_is_upgrade_stale_then_rebuild_restamps_v2() {
    let l = lab_cargo();
    l.build();
    l.downgrade_meta_algo_to_v1();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "TC6-b: v1 rust slot is stale: {v:?}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "identity-algo-upgrade"),
        "TC6-a: upgrade reason in {reasons:?}"
    );
    assert_eq!(v["serves"], serde_json::json!("committed-baseline"));
    assert_ne!(
        v["serves"],
        serde_json::json!("legacy-signals"),
        "TC6: a v1 RUST slot must NOT answer legacy-signals/fresh"
    );
    // Rebuild restamps v2 and freshness returns to current-tree.
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "TC6-c: rebuild restores fresh: {v:?}");
    assert_eq!(v["identity_algo"], serde_json::json!("sha256-v2"), "TC6-c");
}

#[test]
fn tc6d_keyless_rust_slot_is_upgrade_stale() {
    let l = lab_cargo();
    l.build();
    l.strip_identity_triple();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "TC6-d: keyless rust slot is stale: {v:?}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "identity-algo-upgrade"),
        "TC6-d: keyless rust slot answers upgrade, not legacy-fresh: {reasons:?}"
    );
}

// --- TC-9: non-Rust v1 slot one-time degradation (disclosed, never fresh-lie) ---

#[test]
fn tc9_v1_python_slot_legacy_signals_then_rebuild() {
    let l = lab_python();
    l.build();
    l.downgrade_meta_algo_to_v1();
    // TC-9 oracle (amended — see EP amendment appendix): the v1 non-Rust
    // slot answers legacy-signals with NULL identities (honest
    // degradation — the v2 tool cannot compare a v1 identity), and
    // freshness follows the untouched legacy trio (a source-untouched
    // repo stays fresh=true; forcing stale would one-time-stale every
    // legacy repo on upgrade — the frozen non-Rust-legacy-policy ruling
    // wins over the "never fresh" reading). The upgrade reason is
    // Rust-inclusive and must NOT appear here.
    let (code, v) = l.verdict();
    assert_eq!(
        v["serves"],
        serde_json::json!("legacy-signals"),
        "TC9-a: non-rust v1 slot answers legacy-signals (identities null)"
    );
    assert!(
        v["indexed_source_identity"].is_null(),
        "TC9-a: identities null in legacy-signals"
    );
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        !reasons.iter().any(|r| r == "identity-algo-upgrade"),
        "TC9-c: upgrade reason is Rust-inclusive only: {reasons:?}"
    );
    let _ = code;
    // Rebuild restamps v2 → current-tree returns.
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "TC9-b: rebuild restores current-tree: {v:?}");
    assert_eq!(v["serves"], serde_json::json!("current-tree"));
}

// --- TC-10: Cargo-present + rs-absent never gains a rust face ---

#[test]
fn tc10_cargo_without_rs_no_rust_face_and_no_identity_scope() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(repo.join("app.py"), "x = 1\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let out = freshness(&repo, true);
    let v = verdict_json(&out);
    let faces = v["faces"].as_array().unwrap();
    assert!(
        !faces.iter().any(|f| f == "rust"),
        "TC10-a: no rust face without .rs sources: {faces:?}"
    );
    // Cargo edit does not move a rust-less repo's verdict.
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace]\nmembers = []\n# edit\n",
    )
    .unwrap();
    let (code, v2) = (
        freshness(&repo, true).exit_code,
        verdict_json(&freshness(&repo, true)),
    );
    assert_eq!(
        code, 0,
        "TC10-c: cargo edit keeps a rust-less repo fresh: {v2:?}"
    );
}

// ================= EP DB-100 5.3 judge fixes =================
//
// P1-A face scope, P1-B ancestor chain, P3-E topology encoding — the
// labs below extend this file's TC fixtures with an explicit-python
// lab, a dual-workspace lab, and comma-named workspace variants.

use code_reality::language::ProducerFamily;

/// P1-A lab: app.py over a Cargo-present tree — the explicit python pin
/// deliberately does NOT index the rust face on disk.
struct PyCargoLab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

fn lab_py_cargo() -> PyCargoLab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("app.py"), "x = 1\n").unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(repo.join("Cargo.lock"), "version = 3\n").unwrap();
    std::fs::create_dir_all(repo.join("crates")).unwrap();
    std::fs::write(repo.join("crates/a.rs"), "pub fn a() {}\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    PyCargoLab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

impl PyCargoLab {
    fn build_explicit_python(&self) {
        build_repo(&self.repo, Some(ProducerFamily::Python), &self.roots)
            .expect("explicit python build");
    }
    fn meta(&self) -> serde_json::Value {
        let p = meta_path(&code_reality::engine::default_index_path(&self.repo).unwrap());
        std::fs::read_to_string(p)
            .unwrap()
            .parse()
            .expect("meta json")
    }
    fn downgrade_algo_to_v1(&self) {
        let p = meta_path(&code_reality::engine::default_index_path(&self.repo).unwrap());
        let text = std::fs::read_to_string(&p).unwrap();
        let mut v: serde_json::Value = text.parse().expect("meta json");
        v["identity_algo"] = serde_json::json!("sha256-v1");
        std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
    fn verdict(&self) -> (i32, serde_json::Value) {
        let out = freshness(&self.repo, true);
        (out.exit_code, verdict_json(&out))
    }
}

/// P1-A (a): an explicit python pin is not staled by a Cargo-only edit —
/// the pinned identity body excludes the Rust domains symmetrically on
/// capture and query sides.
#[test]
fn p1a_explicit_python_pin_cargo_edit_stays_fresh() {
    let l = lab_py_cargo();
    l.build_explicit_python();
    let meta = l.meta();
    assert_eq!(meta["selection"], serde_json::json!("explicit"), "{meta}");
    assert_eq!(
        meta["source_faces"],
        serde_json::json!(["python"]),
        "premise: python-only pin: {meta}"
    );
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after explicit python build: {v:?}");

    // Cargo-only edit: lockfile bytes change, zero python touch.
    std::fs::write(l.repo.join("Cargo.lock"), "version = 3\n# p1a edit\n").unwrap();
    let (code, v) = l.verdict();
    assert_eq!(
        code, 0,
        "P1-A: cargo-only edit keeps the pinned slot fresh: {v:?}"
    );
    assert_eq!(v["fresh"], serde_json::json!(true), "{v}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        !reasons.iter().any(|r| r == "content-drift"),
        "P1-A: no content-drift for out-of-pin domains: {reasons:?}"
    );
}

/// P1-A (b): a v1 explicit python pin never upgrade-stales over newly
/// arrived .rs files on disk — the explicit pin beats disk detection for
/// the upgrade check (keyless meta keeps the detected computation).
#[test]
fn p1a_v1_explicit_python_pin_new_rs_never_upgrade() {
    let l = lab_py_cargo();
    l.build_explicit_python();
    l.downgrade_algo_to_v1();
    // a NEW rust source arrives after the stamp
    std::fs::write(l.repo.join("crates/new.rs"), "pub fn n() {}\n").unwrap();
    let (_, v) = l.verdict();
    assert_eq!(
        v["serves"],
        serde_json::json!("legacy-signals"),
        "v1 non-rust pin keeps the legacy face: {v}"
    );
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        !reasons.iter().any(|r| r == "identity-algo-upgrade"),
        "P1-A: explicit non-rust pin beats disk detection: {reasons:?}"
    );
    assert_eq!(
        v["fresh"],
        serde_json::json!(true),
        "pinned scope untouched by the out-of-pin arrival: {v}"
    );
}

/// P1-A (c): capture side — the explicit python identity equals the
/// pure-python-scope identity of a Cargo-absent tree with identical
/// python bytes (the Rust domains never entered the identity body).
#[test]
fn p1a_explicit_python_identity_equals_pure_python_scope() {
    let a = lab_py_cargo();
    a.build_explicit_python();
    // Cargo-absent twin: identical app.py bytes, nothing else.
    let t = tempfile::tempdir().unwrap();
    let repo_b = t.path().join("repo_b");
    std::fs::create_dir_all(&repo_b).unwrap();
    std::fs::write(repo_b.join("app.py"), "x = 1\n").unwrap();
    support::git_init(&repo_b);
    let bindir_b = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir_b.path());
    support::install_py_deriving_fake(bindir_b.path());
    build_repo(&repo_b, None, &[bindir_b.path().to_path_buf()]).expect("pure python build");
    let meta_b: serde_json::Value = std::fs::read_to_string(meta_path(
        &code_reality::engine::default_index_path(&repo_b).unwrap(),
    ))
    .unwrap()
    .parse()
    .unwrap();
    assert_eq!(
        a.meta()["source_identity"],
        meta_b["source_identity"],
        "explicit python identity == pure python scope identity"
    );
}

// --- P1-B: the ancestor chain (repo-root toolchain/config governs nested workspaces) ---

/// One minimal metadata-carrying SCIP index with a single DEF document.
fn scip_one(rel: &str, symbol: &str) -> Vec<u8> {
    use protobuf::Message as _;
    use scip::types::{Document, Index, Metadata, Occurrence, TextEncoding, ToolInfo};
    let mut index = Index::new();
    let mut tool = ToolInfo::new();
    tool.name = "rust-analyzer".to_string();
    tool.version = "1.96.0-test".to_string();
    tool.arguments = vec![
        "scip".to_string(),
        "file:///test-workspace-root".to_string(),
    ];
    let mut meta = Metadata::new();
    meta.tool_info = protobuf::MessageField::some(tool);
    meta.project_root = "file:///test-workspace-root".to_string();
    meta.text_document_encoding = protobuf::EnumOrUnknown::from(TextEncoding::UTF8);
    index.metadata = protobuf::MessageField::some(meta);
    let mut d = Document::new();
    d.relative_path = rel.to_string();
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = 1; // DEF
    o.range = vec![0, 0, 1];
    d.occurrences.push(o);
    index.documents.push(d);
    index.write_to_bytes().unwrap()
}

/// Dual-workspace lab with configurable root names: root[0] carries
/// crates/w.rs (symbol wx), root[1] carries crates/w.rs (symbol wy) —
/// content-identical layout across differently-named trees. The fake
/// rust-analyzer emits workspace-relative docs keyed on the invoked
/// project dir.
struct WsLab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    _scipdir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

fn ws_lab(root_names: [&str; 2]) -> WsLab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    let manifest = "[workspace]\nresolver = \"2\"\nmembers = []\n";
    for (i, name) in root_names.iter().enumerate() {
        let p = repo.join(name);
        std::fs::create_dir_all(p.join("crates")).unwrap();
        std::fs::write(p.join("Cargo.toml"), manifest).unwrap();
        let body = if i == 0 {
            "pub fn wx() {}\n"
        } else {
            "pub fn wy() {}\n"
        };
        std::fs::write(p.join("crates/w.rs"), body).unwrap();
    }
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let scipdir = tempfile::tempdir().unwrap();
    let x = scipdir.path().join("x.scip");
    let y = scipdir.path().join("y.scip");
    std::fs::write(
        &x,
        scip_one("crates/w.rs", "rust-analyzer cargo w/1.0.0 wx()."),
    )
    .unwrap();
    std::fs::write(
        &y,
        scip_one("crates/w.rs", "rust-analyzer cargo w/1.0.0 wy()."),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */{n0}) cp '{xp}' \"$out\" ;;\n  */{n1}) cp '{yp}' \"$out\" ;;\n  *) exit 1 ;;\nesac\n",
        n0 = root_names[0],
        n1 = root_names[1],
        xp = x.display(),
        yp = y.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    WsLab {
        repo,
        _t: t,
        _bindir: bindir,
        _scipdir: scipdir,
        roots,
    }
}

impl WsLab {
    fn build(&self) {
        build_repo(&self.repo, None, &self.roots).expect("dual-ws build");
    }
    fn meta(&self) -> serde_json::Value {
        let p = meta_path(&code_reality::engine::default_index_path(&self.repo).unwrap());
        std::fs::read_to_string(p)
            .unwrap()
            .parse()
            .expect("meta json")
    }
    fn verdict(&self) -> (i32, serde_json::Value) {
        let out = freshness(&self.repo, true);
        (out.exit_code, verdict_json(&out))
    }
}

/// P1-B (d): a repo-root rust-toolchain.toml governs BOTH workspaces
/// through their ancestor chains — editing it must content-drift.
#[test]
fn p1b_repo_root_toolchain_edit_is_content_drift() {
    let l = ws_lab(["rust", "dutymail"]);
    // present BEFORE the build ⇒ a content record, not an absent marker
    std::fs::write(
        l.repo.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.80.0\"\n",
    )
    .unwrap();
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after build: {v:?}");

    std::fs::write(
        l.repo.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.81.0\"\n",
    )
    .unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "P1-B: repo-root toolchain edit is stale: {v:?}");
    assert_eq!(v["fresh"], serde_json::json!(false), "{v}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "content-drift"),
        "P1-B: content-drift in {reasons:?}"
    );
}

/// P1-B (e): a repo-root .cargo/config.toml APPEARING after the stamp
/// moves the identity (absent marker → content record).
#[test]
fn p1b_repo_root_cargo_config_appearance_is_stale() {
    let l = ws_lab(["rust", "dutymail"]);
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after build: {v:?}");

    std::fs::create_dir_all(l.repo.join(".cargo")).unwrap();
    std::fs::write(l.repo.join(".cargo/config.toml"), "[build]\njobs = 2\n").unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "P1-B: appearing repo-root config is stale: {v:?}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "content-drift"),
        "P1-B: content-drift in {reasons:?}"
    );
}

/// P3-E: comma-bearing workspace names must not alias distinct
/// topologies — the old `roots.join(",")` encoding made ["a,b","c"] and
/// ["a","b,c"] byte-identical; indexed keys keep them apart.
#[test]
fn p3e_comma_root_names_keep_distinct_topologies() {
    let l1 = ws_lab(["a,b", "c"]);
    l1.build();
    let l2 = ws_lab(["a", "b,c"]);
    l2.build();
    let topo1 = l1.meta()["rust_topology"].clone();
    let topo2 = l2.meta()["rust_topology"].clone();
    assert_ne!(
        topo1, topo2,
        "distinct topologies must not collide: {topo1} vs {topo2}"
    );
    // indexed keys under the stable sorted-root order
    let t1 = topo1.as_array().unwrap();
    assert!(
        t1.iter()
            .any(|kv| kv == &serde_json::json!(["root[0]", "a,b"])),
        "{topo1}"
    );
    assert!(
        t1.iter()
            .any(|kv| kv == &serde_json::json!(["root[1]", "c"])),
        "{topo1}"
    );
    let t2 = topo2.as_array().unwrap();
    assert!(
        t2.iter()
            .any(|kv| kv == &serde_json::json!(["root[0]", "a"])),
        "{topo2}"
    );
    assert!(
        t2.iter()
            .any(|kv| kv == &serde_json::json!(["root[1]", "b,c"])),
        "{topo2}"
    );
}

/// CR-19: deep-layout roots (crates/<n>/, apps/<x>/src-tauri — the
/// mosaic_alpha shape) enter the topology descriptor through the same
/// resolver the stamp face uses, and a deep workspace APPEARING after
/// the stamp flips the verdict stale (identity v2 carries the topology:
/// deep root add/remove must never read fresh).
#[test]
fn cr19_deep_roots_stamp_topology_and_appearance_stales() {
    let l = ws_lab(["crates/x", "apps/shell/src-tauri"]);
    l.build();
    let topo = l.meta()["rust_topology"].clone();
    let t = topo.as_array().unwrap();
    assert!(
        t.iter()
            .any(|kv| kv == &serde_json::json!(["root[0]", "apps/shell/src-tauri"])),
        "deep root[0] (resolver-sorted): {topo}"
    );
    assert!(
        t.iter()
            .any(|kv| kv == &serde_json::json!(["root[1]", "crates/x"])),
        "deep root[1]: {topo}"
    );
    assert!(
        t.iter()
            .any(|kv| kv == &serde_json::json!(["mode", "auto"])),
        "auto mode truth: {topo}"
    );
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after deep-layout build: {v:?}");

    // A NEW deep [workspace] (empty members, no .rs): the doc sets are
    // unchanged — only the root set / rust inputs move. The topology
    // change must still stale the slot.
    std::fs::create_dir_all(l.repo.join("crates/z")).unwrap();
    std::fs::write(
        l.repo.join("crates/z/Cargo.toml"),
        "[workspace]\nmembers = []\n",
    )
    .unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "new deep root must stale the slot: {v:?}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "content-drift"),
        "identity/topology drift surfaces as content-drift: {reasons:?}"
    );
}

/// CR-19 review batch: the REMOVAL direction — a deep workspace root
/// disappearing after the stamp (manifest + sources gone) must flip the
/// verdict stale too; the topology descriptor may never silently absorb
/// a vanished root.
#[test]
fn cr19_deep_root_removal_flips_stale() {
    let l = ws_lab(["crates/x", "apps/shell/src-tauri"]);
    l.build();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "fresh after deep-layout build: {v:?}");

    std::fs::remove_dir_all(l.repo.join("crates/x")).unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "removed deep root must stale the slot: {v:?}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons
            .iter()
            .any(|r| r == "content-drift" || r == "doc-set-drift"),
        "vanished root moves identity and/or doc sets: {reasons:?}"
    );
}
