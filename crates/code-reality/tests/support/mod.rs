//! Shared test-support layer for the JS/TS suites (and future migrations
//! of the older files). One home for the repo fixture, the static SCIP
//! fixture builders, and the producer fakes — no per-file copies (the
//! duplication a review closed: the staged-`--out` producer change broke
//! four independent fakes before this layer existed).
//!
//! Deriving fakes point at `examples/scip_fixture_producer` — a binary
//! that derives its output through the PRODUCTION corpus helpers, so the
//! test doubles never mirror the walk/hash/protobuf logic.

// Consumers take the subset they need; per-crate dead-code analysis
// would flag the rest.
#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use protobuf::Message;
use scip::types::{Document, Index, Occurrence};

// ---------- repo fixtures ----------

pub fn fake_bin(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn mkrepo(t: &tempfile::TempDir, files: &[(&str, &str)]) -> PathBuf {
    let repo = t.path().to_path_buf();
    for (rel, content) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    repo
}

/// Write one file into `repo` (parent dirs created).
pub fn write(repo: &Path, rel: &str, content: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

pub fn git_init(repo: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "x",
        ],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
}

pub fn slot_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/index.scip")
}

pub fn slot_docs(repo: &Path) -> Vec<String> {
    let bytes = std::fs::read(slot_of(repo)).unwrap();
    let idx = Index::parse_from_bytes(&bytes).unwrap();
    let mut docs: Vec<String> = idx
        .documents
        .iter()
        .map(|d| d.relative_path.clone())
        .collect();
    docs.sort();
    docs
}

pub fn log_lines(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .map(|t| {
            t.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ---------- static SCIP fixtures ----------

pub fn occ(symbol: &str, is_def: i32, range: Vec<i32>, enc: Option<Vec<i32>>) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = is_def;
    o.range = range;
    if let Some(e) = enc {
        o.enclosing_range = e;
    }
    o
}

pub fn doc(rel: &str, occs: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    d.relative_path = rel.to_string();
    d.occurrences = occs;
    d
}

/// scip-typescript-shaped one-DEF-per-doc fixture (REAL symbol shapes).
pub fn ts_scip_bytes(dir: &str, docs: &[(&str, &str)]) -> Vec<u8> {
    let mut index = Index::new();
    for (file, name) in docs {
        let mut d = Document::new();
        d.relative_path = file.to_string();
        let mut o = Occurrence::new();
        o.symbol = format!("scip-typescript npm . . {dir}/`{file}`/{name}().");
        o.symbol_roles = 1;
        o.range = vec![0, 0, 1];
        d.occurrences.push(o);
        index.documents.push(d);
    }
    index.write_to_bytes().unwrap()
}

// ---------- deriving producer fakes (production-logic-backed) ----------

/// The compiled `examples/scip_fixture_producer` binary (built by
/// `cargo test`; honors CARGO_TARGET_DIR).
pub fn fixture_producer_bin() -> PathBuf {
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let p = target.join("debug/examples/scip_fixture_producer");
    assert!(
        p.exists(),
        "fixture producer not built: {p:?} (cargo test builds examples; \
         pass --examples if running a filtered build)"
    );
    p
}

/// Deriving scip-typescript fake: delegates argv to the fixture binary
/// (one DEF document per governed file in the CR derived config).
pub fn install_ts_deriving_fake(bindir: &Path) {
    let fixture = fixture_producer_bin();
    fake_bin(
        bindir,
        "scip-typescript",
        &format!("#!/bin/sh\nexec '{}' \"$@\"\n", fixture.display()),
    );
}

/// Deriving pyrefly-index fake: delegates to the fixture binary in py
/// mode (one DEF per on-disk .py from the shared source walk).
pub fn install_py_deriving_fake(bindir: &Path) {
    let fixture = fixture_producer_bin();
    fake_bin(
        bindir,
        "pyrefly-index",
        &format!("#!/bin/sh\nexec '{}' \"$@\"\n", fixture.display()),
    );
}
