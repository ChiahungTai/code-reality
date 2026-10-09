//! Multi-root Rust composition (EP DB-100 S3; frozen TC-1/2/3).
//!
//! Fixture: two sibling `[workspace]` roots (`rust/`, `dutymail/`), a
//! fake rust-analyzer that emits WORKSPACE-relative documents per
//! invocation (the empirically verified rust-analyzer 1.96.0 anchoring),
//! and a composition pipeline that must normalize to git-root-relative
//! paths, pre-concat into ONE Rust partial (StagedLeg/merge_and_publish
//! untouched), and reject cross-root DEF collisions before publication.

mod support;

use code_reality::build::build_repo;
use code_reality::engine::default_index_path;
use protobuf::Message as _;
use scip::types::{Document, Index, Metadata, Occurrence, TextEncoding, ToolInfo};
use std::path::Path;

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, text).unwrap();
}

fn workspace_manifest() -> &'static str {
    "[workspace]\nresolver = \"2\"\nmembers = []\n"
}

fn scip_bytes(docs: &[(&str, &str)]) -> Vec<u8> {
    let mut index = Index::new();
    // Fixture amendment (impl-full, DB-100 S3): real rust-analyzer scip
    // output always carries Index.metadata (tool_info + project_root) —
    // without it these synthetic indexes serialize to 65-120 bytes and
    // trip the pre-existing EMPTY_INDEX_BYTES trap guard in
    // validate_partial (whose calibration comment puts a legal minimal
    // RA index at ~725 bytes). Mirrors the S2 checked-in rich.scip
    // precedent (1747 bytes, metadata-carrying). Assertions unchanged.
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
        let mut o = Occurrence::new();
        o.symbol = symbol.to_string();
        o.symbol_roles = 1; // DEF
        o.range = vec![0, 0, 1];
        d.occurrences.push(o);
        index.documents.push(d);
    }
    index.write_to_bytes().unwrap()
}

struct Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    /// Holds the per-root fixture SCIP files the fake RA copies from —
    /// must outlive the test (the fake script embeds these paths).
    _scipdir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

/// Dual-workspace lab. `shared_symbol` makes BOTH workspaces emit a DEF
/// for the same symbol string (TC-3 collision fixture).
fn lab_dual(shared_symbol: bool) -> Lab {
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
    let roots = vec![bindir.path().to_path_buf()];
    // Fake rust-analyzer: emits workspace-relative docs keyed on the
    // passed project dir (positional after the `scip` subcommand).
    // Fixture amendment (impl-full, DB-100 S3): in `shared_symbol` mode
    // BOTH workspaces must emit a DEF for the SAME symbol string (this
    // helper's own contract, EP TC-3) — the original draft put the
    // shared symbol only in the dutymail fixture, so no cross-root
    // collision existed for the gate to fire on. Assertions unchanged.
    let shared = "rust-analyzer cargo shared/1.0.0 dup().";
    let dm_symbol = if shared_symbol { shared } else { "rust-analyzer cargo dm/1.0.0 dm()." };
    let b_symbol = if shared_symbol { shared } else { "rust-analyzer cargo b/1.0.0 b()." };
    let scipdir = tempfile::tempdir().unwrap();
    let rust_path = scipdir.path().join("rust.scip");
    let dm_path = scipdir.path().join("dutymail.scip");
    std::fs::write(
        &rust_path,
        scip_bytes(&[
            ("crates/a.rs", "rust-analyzer cargo a/1.0.0 a()."),
            ("crates/b.rs", b_symbol),
        ]),
    )
    .unwrap();
    std::fs::write(&dm_path, scip_bytes(&[("crates/dm.rs", dm_symbol)])).unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */rust) cp '{rp}' \"$out\" ;;\n  */dutymail) cp '{dm}' \"$out\" ;;\n  *) echo \"unknown project dir: $dir\" >&2; exit 1 ;;\nesac\n",
        rp = rust_path.display(),
        dm = dm_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        _scipdir: scipdir,
        roots,
    }
}

fn slot_docs(repo: &Path) -> Vec<String> {
    let bytes = std::fs::read(default_index_path(repo).unwrap()).unwrap();
    let index = Index::parse_from_bytes(&bytes).unwrap();
    let mut docs: Vec<String> = index
        .documents
        .iter()
        .map(|d| d.relative_path.clone())
        .collect();
    docs.sort();
    docs
}

fn slot_symbols(repo: &Path) -> Vec<String> {
    let bytes = std::fs::read(default_index_path(repo).unwrap()).unwrap();
    let index = Index::parse_from_bytes(&bytes).unwrap();
    let mut syms: Vec<String> = index
        .documents
        .iter()
        .flat_map(|d| d.occurrences.iter().map(|o| o.symbol.clone()))
        .collect();
    syms.sort();
    syms.dedup();
    syms
}

// --- TC-1: normalization to git-root-relative, symbols untouched ---

#[test]
fn tc1_normalize_prefixes_roots_and_preserves_symbols() {
    let l = lab_dual(false);
    let report = build_repo(&l.repo, None, &l.roots).expect("TC1: dual-root build succeeds");
    assert!(
        report.producers.iter().any(|p| p.contains("roots:")),
        "producer string carries the root list: {:?}",
        report.producers
    );
    let docs = slot_docs(&l.repo);
    assert_eq!(
        docs,
        vec![
            "dutymail/crates/dm.rs".to_string(),
            "rust/crates/a.rs".to_string(),
            "rust/crates/b.rs".to_string(),
        ],
        "TC1-a: every document is git-root-relative"
    );
    let syms = slot_symbols(&l.repo);
    assert!(
        syms.iter().all(|s| s.contains("a().") || s.contains("b().") || s.contains("dm().")),
        "TC1-b: symbol strings preserved verbatim: {syms:?}"
    );
    assert!(
        docs.iter().all(|d| !d.contains("..") && !d.starts_with('/')),
        "TC1-c: no traversal or absolute paths"
    );
}

// --- TC-2: pre-concat sum + merge contracts untouched ---

#[test]
fn tc2_composed_docs_equal_sum_and_contracts_untouched() {
    let l = lab_dual(false);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let docs = slot_docs(&l.repo);
    assert_eq!(docs.len(), 3, "TC2-a: composed = Σ per-root docs");
    // TC2-b: the frozen contracts are byte-present in source (rg face).
    let build_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/build.rs"
    ))
    .unwrap();
    assert!(
        build_src.contains("staged.iter().find(|l| l.family == family)"),
        "TC2-b: merge_and_publish find(family) untouched"
    );
    assert!(
        build_src.contains("struct StagedLeg"),
        "TC2-b: StagedLeg shape untouched"
    );
}

// --- TC-3: cross-root DEF collision fails BEFORE publication ---

#[test]
fn tc3_cross_root_collision_fails_and_leaves_slot_absent() {
    let l = lab_dual(true);
    // Old-slot oracle (impl-lite): a pre-existing live slot must survive a
    // failing collision build byte-identical — the failure path never
    // touches the published slot.
    let slot = default_index_path(&l.repo).unwrap();
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::write(&slot, b"sentinel").unwrap();
    let err = build_repo(&l.repo, None, &l.roots)
        .err()
        .expect("TC3-a: collision build fails");
    let msg = format!("{err:?}");
    assert!(msg.contains("CR-RUST-ROOT-08") || msg.contains("collision"), "TC3-a: named collision error: {msg}");
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        b"sentinel" as &[u8],
        "TC3-b: old live slot untouched (byte-identical sentinel)"
    );
}

#[test]
fn tc3b_within_root_duplicate_not_newly_rejected() {
    // Both docs in the SAME root carrying the same symbol string is the
    // pre-existing rust-analyzer behavior (duplicate-symbol diagnostics
    // without failure) — the gate only fires across roots.
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    // b.rs must exist on disk: normalize now resolves every document to a
    // real repo file (TC-1 disk face), and the dup fixture emits both docs.
    write(&repo, "rust/crates/b.rs", "pub fn b() {}\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let dup = scip_bytes(&[
        ("crates/a.rs", "rust-analyzer cargo a/1.0.0 dup()."),
        ("crates/b.rs", "rust-analyzer cargo a/1.0.0 dup()."),
    ]);
    let tmp_scip = tempfile::tempdir().unwrap();
    let dup_path = tmp_scip.path().join("rust.scip");
    std::fs::write(&dup_path, dup).unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncp '{dup}' \"$out\"\n",
        dup = dup_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("TC3-b: within-root duplicate stays accepted");
}

/// TC-3c: rust-analyzer mints function-local variables as unqualified
/// `local N` symbols (crate-local namespace) — the same string recurs in
/// unrelated workspaces with no semantic relation, so the build must not
/// trip the cross-root collision gate (real-dogfood failure on
/// delegate-bridge) AND the cleanup leg (EP DB-100) strips them from the
/// composed index: they carry no cross-file semantics, and letting them
/// through has graph_db's `nodes.symbol UNIQUE + INSERT OR IGNORE`
/// silently keep only the first root's node.
#[test]
fn tc3c_cross_root_local_symbols_not_collisions() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/local.rs", "pub fn holder() { let x = 1; }\n");
    write(&repo, "dutymail/Cargo.toml", workspace_manifest());
    write(&repo, "dutymail/crates/dm.rs", "pub fn dm() { let y = 2; }\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    // Both roots emit a DEF for the SAME `local 0` string, in different
    // files (scip_bytes carries Index.metadata per the tc3 fixture
    // convention); the dm root also carries a `local 0` REF — a local's
    // references are equally file-local and strip alongside its DEF.
    let rust_scip = scip_bytes(&[("crates/local.rs", "local 0")]);
    let mut dm_index = Index::parse_from_bytes(&scip_bytes(&[("crates/dm.rs", "local 0")])).unwrap();
    let mut local_ref = Occurrence::new();
    local_ref.symbol = "local 0".to_string();
    local_ref.symbol_roles = 0; // REF
    local_ref.range = vec![1, 4, 5];
    dm_index.documents[0].occurrences.push(local_ref);
    let tmp_scip = tempfile::tempdir().unwrap();
    let rust_path = tmp_scip.path().join("rust.scip");
    let dm_path = tmp_scip.path().join("dutymail.scip");
    std::fs::write(&rust_path, rust_scip).unwrap();
    std::fs::write(&dm_path, dm_index.write_to_bytes().unwrap()).unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */rust) cp '{rp}' \"$out\" ;;\n  */dutymail) cp '{dm}' \"$out\" ;;\n  *) exit 1 ;;\nesac\n",
        rp = rust_path.display(),
        dm = dm_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots)
        .expect("TC3-c: cross-root `local N` DEFs are not collisions");
    // Documents survive — the strip keeps drained-to-zero documents (the
    // document face still participates in doc-set semantics).
    let docs = slot_docs(&repo);
    assert_eq!(
        docs,
        vec!["dutymail/crates/dm.rs".to_string(), "rust/crates/local.rs".to_string()],
        "TC3-c-a: both roots' documents survive into the composed slot"
    );
    let bytes = std::fs::read(default_index_path(&repo).unwrap()).unwrap();
    let index = Index::parse_from_bytes(&bytes).unwrap();
    let locals: Vec<&str> = index
        .documents
        .iter()
        .flat_map(|d| d.occurrences.iter().map(|o| o.symbol.as_str()))
        .filter(|s| s.starts_with("local "))
        .collect();
    assert!(
        locals.is_empty(),
        "TC3-c-b: `local N` DEF/REF occurrences are stripped from the composed slot: {locals:?}"
    );
}

// --- coverage gate (Q7: fatal only in multi-root/config mode) ---

#[test]
fn tc_coverage_gate_missing_root_doc_fails_08() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    // b.rs is in the fixture scip AND on disk — the gate must fire on the
    // coverage hole (missing.rs indexed on disk, absent from docs), not on
    // normalize's disk-resolution check.
    write(&repo, "rust/crates/b.rs", "pub fn b() {}\n");
    write(&repo, "rust/crates/missing.rs", "pub fn gone() {}\n");
    write(&repo, "dutymail/Cargo.toml", workspace_manifest());
    write(&repo, "dutymail/crates/dm.rs", "pub fn dm() {}\n");
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    // rust.scip covers a.rs + b.rs but the disk carries missing.rs — the
    // multi-root coverage gate must fail loud (08).
    let rust_scip = scip_bytes(&[
        ("crates/a.rs", "rust-analyzer cargo a/1.0.0 a()."),
        ("crates/b.rs", "rust-analyzer cargo b/1.0.0 b()."),
    ]);
    let dm_scip = scip_bytes(&[("crates/dm.rs", "rust-analyzer cargo dm/1.0.0 dm().")]);
    let tmp_scip = tempfile::tempdir().unwrap();
    let rust_path = tmp_scip.path().join("rust.scip");
    let dm_path = tmp_scip.path().join("dutymail.scip");
    std::fs::write(&rust_path, rust_scip).unwrap();
    std::fs::write(&dm_path, dm_scip).unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */rust) cp '{rp}' \"$out\" ;;\n  */dutymail) cp '{dm}' \"$out\" ;;\n  *) exit 1 ;;\nesac\n",
        rp = rust_path.display(),
        dm = dm_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots)
        .err()
        .expect("coverage gate: missing governed doc fails loud in multi-root mode");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("CR-RUST-ROOT-08"),
        "TC-coverage: 08 on missing root doc: {msg}"
    );
}

// --- P2-D (EP 5.3): canonical containment — the symlink escape gate ---

/// Single-workspace lab whose fake rust-analyzer emits a doc for
/// `crates/link.rs`, a symlink on disk. `escape=true` points the symlink
/// OUTSIDE the repo; otherwise at a sibling real file inside it.
struct SymLab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _outside: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    _scipdir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

fn lab_symlink(escape: bool) -> SymLab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    write(&repo, "rust/crates/real.rs", "pub fn r() {}\n");
    // The escape target must EXIST outside the repo (is_file resolves the
    // symlink first; containment is judged on the canonical path).
    let outside = tempfile::tempdir().unwrap();
    if escape {
        let target = outside.path().join("outside.rs");
        std::fs::write(&target, "pub fn o() {}\n").unwrap();
        std::os::unix::fs::symlink(target, repo.join("rust/crates/link.rs")).unwrap();
    } else {
        // relative target inside the same directory
        std::os::unix::fs::symlink("real.rs", repo.join("rust/crates/link.rs")).unwrap();
    }
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let scipdir = tempfile::tempdir().unwrap();
    let fixture = scipdir.path().join("rust.scip");
    std::fs::write(
        &fixture,
        scip_bytes(&[
            ("crates/a.rs", "rust-analyzer cargo a/1.0.0 a()."),
            ("crates/real.rs", "rust-analyzer cargo r/1.0.0 r()."),
            ("crates/link.rs", "rust-analyzer cargo l/1.0.0 l()."),
        ]),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncp '{fx}' \"$out\"\n",
        fx = fixture.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    SymLab {
        repo,
        _t: t,
        _outside: outside,
        _bindir: bindir,
        _scipdir: scipdir,
        roots,
    }
}

/// P2-D (a): a producer-emitted document that resolves through an
/// in-repo symlink to a file OUTSIDE the repo fails loud (exit 2) —
/// component-shape checks cannot see the escape. Asserted through the
/// lib API with injectable producer roots: the CLI face maps
/// BuildError::Env to exit 2, and the process-global PATH must not be
/// mutated (the resolution contract this test relies on).
#[test]
fn p2d_symlink_escape_doc_fails_loud() {
    let l = lab_symlink(true);
    let err = build_repo(&l.repo, None, &l.roots).err().expect("P2-D: escape build fails");
    assert!(
        matches!(err, code_reality::build::BuildError::Env(_)),
        "P2-D: Env-class failure (CLI exit 2): {err:?}"
    );
    let msg = format!("{err:?}");
    assert!(
        msg.contains("symlink escape"),
        "P2-D: named escape: {msg}"
    );
    assert!(
        msg.contains("link.rs"),
        "P2-D: error names the escaping doc: {msg}"
    );
}

/// P2-D (b): a symlink resolving INSIDE the repo passes containment —
/// the gate rejects escapes, not symlinks. The symlink doc itself is
/// invisible to the source walk, so the stamp cannot certify docs==disk:
/// the P1-C WARN must say so explicitly instead of staying silent.
#[test]
fn p2d_symlink_inside_doc_passes() {
    let l = lab_symlink(false);
    let report = build_repo(&l.repo, None, &l.roots)
        .expect("P2-D: inside symlink passes containment");
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("source identity")),
        "P1-C: uncertified stamp disclosed via WARN note: {:?}",
        report.notes
    );
    assert!(
        report.source_identity.is_none(),
        "premise: the symlink doc blocks docs==disk certification"
    );
}
