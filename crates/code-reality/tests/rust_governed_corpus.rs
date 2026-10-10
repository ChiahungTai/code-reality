//! Rust governed-corpus gates (CR-5).
//!
//! Root cause (dual-source investigation, frozen design): the
//! `walk_sources` rust leg ignored the repo profile exclusion while
//! rust-analyzer only emits Cargo workspace targets — an orphan `.rs`
//! (probe scripts, test fixtures) made the produced doc set a strict
//! subset of the walk set, so `stamp_meta_locked`'s `docs == a.docs`
//! certification could never hold and every build on such a repo
//! degraded to identity-algo-upgrade stale. The frozen decisions under
//! test here: (1) strict set equality, never relaxed; (2) the rust walk
//! joins the same profile exclusion policy as python/JS/TS; (3) the
//! coverage gate extends to single-root auto mode (CR-RUST-ROOT-09 —
//! fatal, carries the two-directional doc-set diff plus remediation);
//! (4) a stamp-certification failure carries the doc-set diff in the
//! build report notes.

mod support;

use code_reality::build::{build_repo, BuildError};
use code_reality::engine::meta_path;
use protobuf::Message as _;
use scip::types::{Document, Index, Metadata, Occurrence, TextEncoding, ToolInfo};
use std::path::Path;

fn write(repo: &Path, rel: &str, text: &str) {
    support::write(repo, rel, text);
}

fn workspace_manifest() -> &'static str {
    "[workspace]\nresolver = \"2\"\nmembers = []\n"
}

/// Metadata-carrying SCIP bytes (multi_rust_build precedent: without
/// Index.metadata the synthetic index serializes under the 128-byte
/// EMPTY_INDEX_BYTES guard and trips validate_partial).
fn scip_bytes(docs: &[(&str, &str)]) -> Vec<u8> {
    let mut index = Index::new();
    let mut tool = ToolInfo::new();
    tool.name = "rust-analyzer".to_string();
    tool.version = "1.96.0-test".to_string();
    tool.arguments = vec!["scip".to_string(), "file:///test-workspace-root".to_string()];
    let mut meta = Metadata::new();
    meta.tool_info = protobuf::MessageField::some(tool);
    meta.project_root = "file:///test-workspace-root".to_string();
    meta.text_document_encoding = protobuf::EnumOrUnknown::from(TextEncoding::UTF8);
    index.metadata = protobuf::MessageField::some(meta);
    for (file, symbol) in docs {
        let mut d = Document::new();
        d.relative_path = file.to_string();
        for suffix in ["fn1", "fn2", "fn3"] {
            let mut o = Occurrence::new();
            o.symbol = format!("{symbol}{suffix}().");
            o.symbol_roles = 1; // DEF
            o.range = vec![0, 0, 1];
            d.occurrences.push(o);
        }
        index.documents.push(d);
    }
    index.write_to_bytes().unwrap()
}

fn stamp_meta(repo: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(meta_path(
        &code_reality::engine::default_index_path(repo).unwrap(),
    ))
    .unwrap();
    text.parse().expect("meta json")
}

// ---------- S1: the rust walk joins the profile exclusion policy ----------

struct S1Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
}

fn lab_walk() -> S1Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "Cargo.toml", workspace_manifest());
    write(&repo, "crates/a.rs", "pub fn a() {}\n");
    // the canonical orphan shape: a probe script outside every target
    write(&repo, "audit/lsp/probe.rs", "fn probe() {}\n");
    // .venv-embedded .rs: the walk prunes dot-DIRS before any
    // is_excluded consult, so this path never reaches the exclusion
    // layer from the walk side — the dot-dir skip alone explains its
    // absence from the walk assertions below. DEFAULT_EXCLUDE
    // (".venv/") earns its keep on the producer-doc side (paths
    // filtered at the merge layer with no walk — e.g.
    // filter_python_partial), where is_excluded sees it directly; the
    // s1 assertion pins that face.
    write(&repo, ".venv/embedded.rs", "fn v() {}\n");
    S1Lab { repo, _t: t }
}

/// S1 (AC#1): the rust corpus set is profile-filtered — an excluded
/// orphan leaves the governed walk set; without a profile it stays.
#[test]
fn s1_walk_rust_face_joins_profile_exclude() {
    let l = lab_walk();
    let walk = code_reality::engine::walk_sources(&l.repo).unwrap();
    let rs: Vec<String> = walk.rs.keys().cloned().collect();
    assert_eq!(
        rs,
        vec!["audit/lsp/probe.rs".to_string(), "crates/a.rs".to_string()],
        "no profile: the full non-target rust corpus walks"
    );
    assert!(!walk.rs.contains_key(".venv/embedded.rs"), "dot-dir skip");
    assert!(
        code_reality::profile::is_excluded(".venv/embedded.rs", None),
        "DEFAULT_EXCLUDE names .venv/ at the is_excluded layer — \
         the producer-doc side (no walk) relies on it"
    );

    write(&l.repo, ".code-reality.toml", "exclude = [\"audit/\"]\n");
    let walk = code_reality::engine::walk_sources(&l.repo).unwrap();
    let rs: Vec<String> = walk.rs.keys().cloned().collect();
    assert_eq!(
        rs,
        vec!["crates/a.rs".to_string()],
        "profile exclude removes the orphan from the rust walk set"
    );
}

// ---------- S2: single-root auto coverage gate (CR-RUST-ROOT-09) ----------

struct Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    _scipdir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

/// Single-root auto lab: a root `[workspace]` (auto roots = ["."]), the
/// two target .rs the rust fake emits (tests/fixtures/rich.scip carries
/// exactly crates/a.rs + crates/b.rs), an orphan .rs outside every
/// target, and two governed .py files (one under the orphan's dir — the
/// python leg consumes the explicit corpus list, so py docs always match).
fn lab_single(with_profile: bool, with_orphan: bool) -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "Cargo.toml", workspace_manifest());
    write(&repo, "Cargo.lock", "version = 3\n");
    write(&repo, "crates/a.rs", "pub fn a() {}\n");
    write(&repo, "crates/b.rs", "pub fn b() {}\n");
    if with_orphan {
        write(&repo, "audit/lsp/probe.rs", "fn probe() {}\n");
    }
    write(&repo, "app.py", "x = 1\n");
    write(&repo, "audit/keep.py", "y = 2\n");
    if with_profile {
        write(&repo, ".code-reality.toml", "exclude = [\"audit/\"]\n");
    }
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    install_rust_rich_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        _scipdir: tempfile::tempdir().unwrap(),
        roots,
    }
}

/// Fake rust-analyzer: emits the shared rich.scip fixture for any scip
/// invocation (rust_identity_v2 lab_cargo precedent).
fn install_rust_rich_fake(dir: &Path) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");
    support::fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) shift; cp '{fixture}' \"$1\"; exit 0;; esac; shift; done\nexit 1\n"
        ),
    );
}

/// S2 (AC#2, RED): an orphan .rs in single-root auto mode must fail the
/// build LOUD — CR-RUST-ROOT-09, the precise orphan path, both diff
/// directions with counts, and both remediation hints.
#[test]
fn s2_single_root_orphan_rs_fails_gate_with_diff() {
    let l = lab_single(false, true);
    match build_repo(&l.repo, None, &l.roots) {
        Ok(rep) => panic!("orphan .rs must not build silently: {:?}", rep.notes),
        Err(BuildError::Env(m)) => {
            assert!(m.contains("CR-RUST-ROOT-09"), "{m}");
            assert!(m.contains("audit/lsp/probe.rs"), "precise path: {m}");
            assert!(m.contains("walk−index"), "walk−index face: {m}");
            assert!(m.contains("index−walk"), "index−walk face: {m}");
            assert!(m.contains(".code-reality.toml exclude"), "hint 1: {m}");
            assert!(m.contains("[[bin]]/[[test]]"), "hint 2: {m}");
        }
        Err(e) => panic!("unexpected error family: {e:?}"),
    }
}

/// S2 (AC#2→AC#4 chain): with the orphan excluded the build converges —
/// identity triple stamped, freshness answers fresh.
#[test]
fn s2_profile_exclude_converges_build_stamp_fresh() {
    let l = lab_single(true, true);
    build_repo(&l.repo, None, &l.roots).expect("excluded orphan builds");
    let meta = stamp_meta(&l.repo);
    let identity = meta["source_identity"].as_str().expect("identity stamped");
    assert_eq!(identity.len(), 64, "sha256 hex: {identity}");
    assert_eq!(meta["identity_algo"], "sha256-v2");
    assert!(meta["index_sha256"].is_string(), "index_sha256 stamped");
    let out = code_reality::freshness::freshness(&l.repo, true);
    assert_eq!(out.exit_code, 0, "fresh after converged build: {out:?}");
    let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(v["fresh"], serde_json::json!(true));
}

/// S2 (AC#4): a NEW orphan arriving after convergence must fail loud —
/// exclusion is not a blanket pardon for the whole tree.
#[test]
fn s2_new_orphan_after_exclude_fails_loud() {
    let l = lab_single(true, false);
    build_repo(&l.repo, None, &l.roots).expect("clean build");
    write(&l.repo, "misc/probe.rs", "fn probe() {}\n");
    match build_repo(&l.repo, None, &l.roots) {
        Ok(rep) => panic!("new orphan must not slip through: {:?}", rep.notes),
        Err(BuildError::Env(m)) => {
            assert!(m.contains("CR-RUST-ROOT-09"), "{m}");
            assert!(m.contains("misc/probe.rs"), "precise new path: {m}");
        }
        Err(e) => panic!("unexpected error family: {e:?}"),
    }
}

/// S2b (fresh F1): the superset direction (index−walk non-empty —
/// symlink corpus, excluded-dir Cargo workspace) carries its OWN
/// remediation hint. The orphan-direction hints are dead advice on this
/// face (excluding more files cannot remove an already-produced doc),
/// so a PURE superset must not advertise them.
#[test]
fn s2b_superset_diff_names_third_hint_not_orphan_hints() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "Cargo.toml", workspace_manifest());
    write(&repo, "Cargo.lock", "version = 3\n");
    write(&repo, "crates/a.rs", "pub fn a() {}\n");
    write(&repo, "crates/b.rs", "pub fn b() {}\n");
    // Walk-invisible produced doc: a symlink entry never reaches the
    // walk (not a file/dir entry), while rust-analyzer follows it and
    // the normalize containment check resolves INSIDE the repo — the
    // p2d(b) multi-root lab shape, minimized for single-root auto.
    std::os::unix::fs::symlink("b.rs", repo.join("crates/link.rs")).unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let scipdir = tempfile::tempdir().unwrap();
    let fixture = scipdir.path().join("superset.scip");
    std::fs::write(
        &fixture,
        scip_bytes(&[
            ("crates/a.rs", "rust-analyzer cargo a/1.0.0 a"),
            ("crates/b.rs", "rust-analyzer cargo b/1.0.0 b"),
            ("crates/link.rs", "rust-analyzer cargo l/1.0.0 l"),
        ]),
    )
    .unwrap();
    support::fake_bin(
        bindir.path(),
        "rust-analyzer",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) shift; cp '{fx}' \"$1\"; exit 0;; esac; shift; done\nexit 1\n",
            fx = fixture.display()
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];
    match build_repo(&repo, None, &roots) {
        Ok(rep) => panic!("superset doc set must fail the gate: {:?}", rep.notes),
        Err(BuildError::Env(m)) => {
            assert!(m.contains("CR-RUST-ROOT-09"), "{m}");
            assert!(m.contains("index−walk"), "superset face: {m}");
            assert!(m.contains("crates/link.rs"), "diff names the doc: {m}");
            assert!(m.contains("symlink"), "third hint names the cause: {m}");
            assert!(
                m.contains("rust_projects"),
                "third hint names rust_projects roots: {m}"
            );
            assert!(
                !m.contains(".code-reality.toml exclude"),
                "orphan hints must not fire on a pure superset: {m}"
            );
        }
        Err(e) => panic!("unexpected error family: {e:?}"),
    }
}

/// S5 (fresh F1): an all-excluded rust corpus skips the leg — the
/// python/TS `SkippedByProfile` face, symmetric — instead of
/// dead-ending on ROOT-09 (ra follows Cargo regardless of profile
/// excludes, so an empty governed set can never equal the produced
/// set). Rust-only repo: the leg skip converges to the empty terminal.
#[test]
fn s5_rust_corpus_fully_excluded_skips_leg_converges_empty() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "Cargo.toml", workspace_manifest());
    write(&repo, "crates/a.rs", "pub fn a() {}\n");
    write(&repo, ".code-reality.toml", "exclude = [\"crates/\"]\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    // A producer that WOULD emit docs for the real corpus — if the leg
    // ran, the produced set could never match the empty governed set
    // (the exact dead-end this skip removes).
    install_rust_rich_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots)
        .expect("all-excluded rust corpus converges empty, not ROOT-09");
    assert_eq!(rep.face, "empty(profile-excluded)", "{:?}", rep.notes);
    assert!(
        rep.notes
            .iter()
            .any(|n| n.contains("rust 語言面：governed 語料為空")),
        "rust leg skip note: {:?}",
        rep.notes
    );
}

// ---------- S3: stamp-certification failure carries the doc diff ----------

/// Python-only lab whose fake pyrefly-index misses one governed file —
/// the F1-narrowed WARN path (producer miss never fails the build), so
/// the stamp certification legitimately fails and the read-back notes
/// must name the diff. This is the reachable post-gate path to the
/// read-back branch (the S2 gate already blocks the rust orphan route;
/// choosing this path over a gate bypass keeps the gate itself untested
/// against its own diagnostic).
#[test]
fn s3_stamp_failure_reports_doc_diff_notes() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "app.py", "x = 1\n");
    write(&repo, "helper.py", "y = 2\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let scipdir = tempfile::tempdir().unwrap();
    let partial = scipdir.path().join("partial.scip");
    std::fs::write(&partial, scip_bytes(&[("app.py", "pyrefly python proj 0.1.0 `m`/app")]))
        .unwrap();
    // static pyrefly-index fake: emits ONLY app.py (helper.py missed)
    support::fake_bin(
        bindir.path(),
        "pyrefly-index",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-py 0.1.0'; exit 0; fi\nwhile [ $# -gt 0 ]; do case \"$1\" in --out) shift; cp '{p}' \"$1\"; exit 0;; esac; shift; done\nexit 1\n",
            p = partial.display(),
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("producer miss is a WARN, not a failure");
    assert!(
        rep.notes
            .iter()
            .any(|n| n.contains("slot 未取得 source identity 認證")),
        "base WARN present: {:?}",
        rep.notes
    );
    let diff_notes: Vec<&String> = rep
        .notes
        .iter()
        .filter(|n| n.contains("walk−index") || n.contains("index−walk"))
        .collect();
    assert_eq!(diff_notes.len(), 2, "exactly the two diff lines: {rep:?}");
    assert!(
        rep.notes
            .iter()
            .any(|n| n.contains("walk−index：1 筆") && n.contains("helper.py")),
        "walk−index names the missed file: {:?}",
        rep.notes
    );
    assert!(
        rep.notes
            .iter()
            .any(|n| n.contains("index−walk：0 筆")),
        "index−walk is empty: {:?}",
        rep.notes
    );
}

// ---------- S4: the multi-root gate face is unchanged ----------

/// Dual-root contrast (multi_rust_build lab_dual shape): the existing
/// multi-root gate stays green and the multi-root face still converges
/// to a stamped, fresh slot under the new walk filter.
#[test]
fn s4_dual_root_converges_and_stamps() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    write(&repo, "rust/crates/b.rs", "pub fn b() {}\n");
    write(&repo, "dutymail/Cargo.toml", workspace_manifest());
    write(&repo, "dutymail/crates/dm.rs", "pub fn dm() {}\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let scipdir = tempfile::tempdir().unwrap();
    let rust_path = scipdir.path().join("rust.scip");
    let dm_path = scipdir.path().join("dutymail.scip");
    std::fs::write(
        &rust_path,
        scip_bytes(&[
            ("crates/a.rs", "rust-analyzer cargo a/1.0.0 a"),
            ("crates/b.rs", "rust-analyzer cargo b/1.0.0 b"),
        ]),
    )
    .unwrap();
    std::fs::write(
        &dm_path,
        scip_bytes(&[("crates/dm.rs", "rust-analyzer cargo dm/1.0.0 dm")]),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */rust) cp '{rp}' \"$out\" ;;\n  */dutymail) cp '{dm}' \"$out\" ;;\n  *) echo \"unknown project dir: $dir\" >&2; exit 1 ;;\nesac\n",
        rp = rust_path.display(),
        dm = dm_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("dual-root build");
    assert!(
        rep.producers
            .iter()
            .any(|p| p.contains("roots: dutymail, rust")),
        "multi-root producer face (resolver-sorted roots): {:?}",
        rep.producers
    );
    let docs = support::slot_docs(&repo);
    assert_eq!(
        docs,
        vec![
            "dutymail/crates/dm.rs".to_string(),
            "rust/crates/a.rs".to_string(),
            "rust/crates/b.rs".to_string(),
        ]
    );
    let meta = stamp_meta(&repo);
    assert_eq!(meta["identity_algo"], "sha256-v2", "identity stamped");
    let out = code_reality::freshness::freshness(&repo, true);
    assert_eq!(out.exit_code, 0, "fresh: {out:?}");
}
