//! build umbrella integration tests (EP ep-build-umbrella). Fake-bin
//! injection: producers are shell stubs on synthetic roots — the
//! process-global PATH is never mutated (cargo-test parallelism).

mod support;

use code_reality::build::{
    build_repo, count_sources, ensure_fresh, heal_outcome_after_rebuild_err, BuildError,
    HealOutcome,
};
use code_reality::common::resolve_bin;
use code_reality::language::ProducerFamily;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");

fn fake_bin(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn mkrepo(t: &tempfile::TempDir, files: &[(&str, &str)]) -> PathBuf {
    let repo = t.path().to_path_buf();
    for (rel, content) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    repo
}

/// Parses `--repo <v>` / `--output <v>` / `--out <v>` pairs from "$@".
fn arg_parse_sh() -> &'static str {
    r#"prev=''; for a in "$@"; do if [ "$prev" = "--repo" ] || [ "$prev" = "--output" ] || [ "$prev" = "--out" ]; then eval "${prev#--}=\"$a\""; fi; prev="$a"; done"#
}

/// Corpus-consistent python fixture: docs == {app.py, app2.py} — the
/// heal-path tests converge on exactly that disk set after writing
/// app2.py, so the S4 missing/extra convergence gates pass (the rich
/// fixture's .rs docs would read as a permanent corpus mismatch). The
/// defs deliberately avoid the name `f` (tests asserting 查無 DEF for
/// query f rely on that).
fn pyfx_bytes() -> Vec<u8> {
    use protobuf::Message;
    use scip::types::{Document, Index, Occurrence};
    let mut index = Index::new();
    for (path, names) in [
        ("app.py", ["app_main", "app_aux"]),
        ("app2.py", ["app_second", "app_third"]),
    ] {
        let mut d = Document::new();
        d.relative_path = path.to_string();
        for name in names {
            let mut occ = Occurrence::new();
            occ.symbol = format!("pyrefly python proj 0.1.0 `m`/{name}().");
            occ.symbol_roles = 1;
            occ.range = vec![0, 0, 1];
            d.occurrences.push(occ);
        }
        index.documents.push(d);
    }
    index.write_to_bytes().unwrap()
}

fn install_pyfx(dir: &Path) -> PathBuf {
    let p = dir.join("pyfx.scip");
    std::fs::write(&p, pyfx_bytes()).unwrap();
    p
}

fn fake_pyrefly(dir: &Path) {
    let fx = install_pyfx(dir);
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
mkdir -p \"$(dirname \"$out\")\"
cp '{}' \"$out\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh(),
            fx.display()
        ),
    );
}

fn fake_rust_analyzer(dir: &Path, empty: bool) {
    let payload = if empty {
        "printf 'x0123456789x0123456789x0123456789x0123456789x0123456789' > \"$output\"".to_string()
    } else {
        format!("cp '{FIXTURE}' \"$output\"")
    };
    fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi
{}
{payload}
echo '[OK] fake rust-analyzer'
",
            arg_parse_sh()
        ),
    );
}

#[test]
fn t1_count_sources_detection_matrix() {
    use code_reality::build::SourceInventory;
    use code_reality::language::ProducerFamily;
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("a.py", "x"),
            ("b.py", "x"),
            ("src/c.rs", "x"),
            ("target/gen.py", "x"), // SKIP_DIRS
            (".hidden/d.py", "x"),  // dot-dir
            ("notes.txt", "x"),
            ("app.mjs", "x"),    // JS family
            ("widget.tsx", "x"), // TS family
        ],
    );
    assert_eq!(
        count_sources(&repo).unwrap(),
        SourceInventory {
            py: 2,
            rs: 1,
            js_ts: 2
        }
    );

    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("only.py", "x")]);
    assert_eq!(
        count_sources(&repo2).unwrap(),
        SourceInventory {
            py: 1,
            rs: 0,
            js_ts: 0
        }
    );

    let t3 = tempfile::tempdir().unwrap();
    let repo3 = mkrepo(&t3, &[("only.rs", "x")]);
    assert_eq!(
        count_sources(&repo3).unwrap(),
        SourceInventory {
            py: 0,
            rs: 1,
            js_ts: 0
        }
    );

    // detection → producer families: every combination SM-1..SM-7 (S1)
    let inv = |py: usize, rs: usize, js_ts: usize| SourceInventory { py, rs, js_ts };
    let fams = |i: SourceInventory| {
        let mut out = std::collections::BTreeSet::new();
        if i.py > 0 {
            out.insert(ProducerFamily::Python);
        }
        if i.rs > 0 {
            out.insert(ProducerFamily::Rust);
        }
        if i.js_ts > 0 {
            out.insert(ProducerFamily::TypeScript);
        }
        out
    };
    assert_eq!(
        fams(inv(1, 0, 0)),
        [ProducerFamily::Python].into_iter().collect()
    );
    assert_eq!(
        fams(inv(0, 1, 0)),
        [ProducerFamily::Rust].into_iter().collect()
    );
    assert_eq!(
        fams(inv(0, 0, 1)),
        [ProducerFamily::TypeScript].into_iter().collect()
    );
    assert_eq!(
        fams(inv(1, 1, 0)),
        [ProducerFamily::Python, ProducerFamily::Rust]
            .into_iter()
            .collect()
    );
    assert_eq!(
        fams(inv(1, 0, 1)),
        [ProducerFamily::Python, ProducerFamily::TypeScript]
            .into_iter()
            .collect()
    );
    assert_eq!(
        fams(inv(0, 1, 1)),
        [ProducerFamily::Rust, ProducerFamily::TypeScript]
            .into_iter()
            .collect()
    );
    assert_eq!(
        fams(inv(1, 1, 1)),
        [
            ProducerFamily::Python,
            ProducerFamily::Rust,
            ProducerFamily::TypeScript
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn t2_resolve_bin_injection() {
    let t = tempfile::tempdir().unwrap();
    fake_bin(t.path(), "pyrefly-index", "#!/bin/sh\nexit 0\n");
    let roots = vec![t.path().to_path_buf()];
    assert!(resolve_bin("pyrefly-index", &roots, "hint").is_ok());

    // non-executable file is not a hit
    std::fs::write(t.path().join("rust-analyzer"), "not exec").unwrap();
    let miss = resolve_bin("rust-analyzer", &roots, "INSTALL-HINT").unwrap_err();
    assert!(miss.contains("INSTALL-HINT"), "miss={miss}");
    assert!(miss.contains(&t.path().display().to_string()));
}

#[test]
fn t3_run_usage_and_producer_validation() {
    let o = code_reality::build::run(&["build"]);
    assert_eq!(o.exit_code, 2);
    let o = code_reality::build::run(&["build", "--repo"]);
    assert_eq!(o.exit_code, 2);
    let o = code_reality::build::run(&["build", "--repo", "/nonexistent-xyz", "--producer", "go"]);
    assert_eq!(o.exit_code, 2);
    assert!(
        o.stderr.contains("rust、python 或 typescript"),
        "{}",
        o.stderr
    );
    // the shared JS/TS family value passes validation (fails later on
    // the nonexistent repo, NOT on producer spelling)
    let o = code_reality::build::run(&[
        "build",
        "--repo",
        "/nonexistent-xyz",
        "--producer",
        "typescript",
    ]);
    assert_eq!(o.exit_code, 2);
    assert!(o.stderr.contains("不是目錄"), "{}", o.stderr);
    let o = code_reality::build::run(&["build", "--help"]);
    assert_eq!(o.exit_code, 0);
    assert!(o.stdout.contains("--producer"));
}

fn git_init(repo: &Path) {
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

#[test]
fn t4_python_leg_e2e_and_idempotent_rerun() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("python leg");
    assert_eq!(rep.face, "python-face");
    assert!(rep.nodes > 0, "nodes={}", rep.nodes);
    assert!(graph_db_path(&repo).exists());
    assert!(rep.producers.iter().any(|p| p.contains("fake-pyrefly")));
    // umbrella stamps index provenance in-process (relay Finding B)
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "index.scip.meta missing after build"
    );

    let rep2 = build_repo(&repo, None, &roots).expect("rerun");
    assert!(rep2.graph_rebuilt, "second run rebuilds");
    assert_eq!(rep2.indexes_created, 0);
    assert!(rep2.indexes_skipped > 0);
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "meta survives idempotent rerun"
    );
}

#[test]
fn t5_rust_empty_index_guard() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), true);
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("空索引")),
        "{err:?}"
    );
    // failed leg leaves no sibling part behind
    assert!(!repo.join(".code-reality/scip/.rust-part.scip").exists());
}

#[test]
fn t6_rust_leg_e2e() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/lib.rs", "pub fn f() {}\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_rust_analyzer(bindir.path(), false);
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots).expect("rust leg");
    assert_eq!(rep.face, "rust-face");
    assert!(rep.nodes > 0);
    assert!(rep.producers.iter().any(|p| p.contains("fake-ra")));
    assert!(
        code_reality::engine::meta_path(&slot_of(&repo)).exists(),
        "rust path stamps too"
    );
    // single-leg atomicity: the slot is the renamed part (fixture bytes),
    // no sibling residue
    let fixture_len = std::fs::metadata(FIXTURE).unwrap().len();
    assert_eq!(
        std::fs::metadata(slot_of(&repo)).unwrap().len(),
        fixture_len
    );
    assert!(!repo.join(".code-reality/scip/.rust-part.scip").exists());
}

#[test]
fn t7_mixed_repo_unified_graph_concat() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "def g():\n    return 2\n"),
            ("src/lib.rs", "pub fn h() {}\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    fake_rust_analyzer(bindir.path(), false);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("mixed unified");
    assert_eq!(rep.face, "mixed(python+rust)");
    assert!(rep.nodes > 0);
    // cat-merge proof: the slot is exactly python-part ++ rust-part (the
    // frozen ORDERED merge — python first).
    let rich = std::fs::read(FIXTURE).unwrap();
    let mut expect = pyfx_bytes();
    expect.extend_from_slice(&rich);
    let slot_bytes = std::fs::read(slot_of(&repo)).unwrap();
    assert_eq!(slot_bytes, expect);
    assert!(!rep.repo.join(".code-reality/scip/.rust-part.scip").exists());

    // single-leg override on a mixed repo → note about the other face
    let rep2 = build_repo(&repo, Some(ProducerFamily::Rust), &roots).expect("producer override");
    assert_eq!(rep2.face, "rust-face");
    assert!(rep2.notes.iter().any(|n| n.contains("未索引：python")));
}

#[test]
fn t8_missing_bin_is_env_fail_with_hint() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x = 1\n")]);
    let empty_roots = vec![PathBuf::from("/nonexistent-bin-dir")];
    let err = build_repo(&repo, None, &empty_roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("uv tool install pyrefly-producer")),
        "{err:?}"
    );
}

#[test]
fn t9_child_failure_maps_env_with_stderr_and_rust_hint() {
    // python leg: producer exits 1 with stderr → Env, child stderr embedded (SM-7)
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x = 1\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 1; fi\necho 'boom-child-stderr' >&2\nexit 1\n",
    );
    let roots = vec![bindir.path().to_path_buf()];
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("boom-child-stderr")),
        "{err:?}"
    );
    // rust leg: bin missing → hint mentions rustup (SM-14)
    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("src/lib.rs", "pub fn f() {}\n")]);
    let err2 = build_repo(&repo2, None, &roots).unwrap_err();
    assert!(
        matches!(err2, BuildError::Env(ref m) if m.contains("rustup component add rust-analyzer")),
        "{err2:?}"
    );
}

#[test]
fn t10_run_non_dir_fails_loud() {
    let o = code_reality::build::run(&["build", "--repo", "/nonexistent-xyz-abc"]);
    assert_eq!(o.exit_code, 2);
    assert!(o.stderr.contains("不是目錄"), "stderr={}", o.stderr);
}

#[test]
fn t11_empty_repo_env_fail() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("notes.txt"), "no code").unwrap();
    let err = build_repo(t.path(), None, &[]).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("找不到可索引原始碼")),
        "{err:?}"
    );
}

#[test]
fn t12_edge_dedupe_doubled_index_converges() {
    // The NT cold-start transient shape: the same index concatenated
    // twice (protobuf same-type merge stacks occurrences). Edge counts
    // must converge — dedupe at the materialization boundary.
    // rich_callers carries attributed refs (rich.scip is defs-only)
    let single = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rich_callers.scip"
    ))
    .unwrap();
    let mk = |bytes: Vec<u8>| {
        let t = tempfile::tempdir().unwrap();
        let slot_dir = t.path().join(".code-reality/scip");
        std::fs::create_dir_all(&slot_dir).unwrap();
        let idx = slot_dir.join("index.scip");
        std::fs::write(&idx, bytes).unwrap();
        (t, idx)
    };
    let (t1, i1) = mk(single.clone());
    let g1 = code_reality::graph_db::build_from_cache_at(t1.path(), &i1).unwrap();
    let (t2, i2) = mk([single.clone(), single].concat());
    let g2 = code_reality::graph_db::build_from_cache_at(t2.path(), &i2).unwrap();
    assert!(g1.edges > 0);
    assert_eq!(
        g1.edges, g2.edges,
        "doubled index must converge: single={} doubled={}",
        g1.edges, g2.edges
    );
    assert_eq!(g1.nodes, g2.nodes);
}

fn graph_db_path(repo: &Path) -> PathBuf {
    repo.join(".code-reality/graph.db")
}

// ---------- Python-face profile exclusion (exclude-consistency) ----------

/// SCIP bytes with docs on both sides of a profile boundary. The fake
/// producer (like the real `pyrefly-index`, which takes no profile
/// input) emits the excluded doc; the merge-layer filter must narrow
/// the staged partial to the governed set.
fn pyfx_boundary_bytes() -> Vec<u8> {
    use protobuf::Message;
    use scip::types::{Document, Index, Occurrence};
    let mut index = Index::new();
    for (path, names) in [("src/a.py", ["a", "a_aux"]), ("dist/c.py", ["c", "c_aux"])] {
        let mut d = Document::new();
        d.relative_path = path.to_string();
        for name in names {
            let mut occ = Occurrence::new();
            occ.symbol = format!("pyrefly python proj 0.1.0 `m`/{name}().");
            occ.symbol_roles = 1;
            occ.range = vec![0, 0, 1];
            d.occurrences.push(occ);
        }
        index.documents.push(d);
    }
    index.write_to_bytes().unwrap()
}

fn fake_pyrefly_boundary(dir: &Path) {
    let fx = dir.join("pyfx-boundary.scip");
    std::fs::write(&fx, pyfx_boundary_bytes()).unwrap();
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
mkdir -p \"$(dirname \"$out\")\"
cp '{}' \"$out\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh(),
            fx.display()
        ),
    );
}

fn slot_doc_paths(repo: &Path) -> Vec<String> {
    use protobuf::Message;
    let bytes = std::fs::read(slot_of(repo)).unwrap();
    let idx = scip::types::Index::parse_from_bytes(&bytes).unwrap();
    let mut docs: Vec<String> = idx
        .documents
        .iter()
        .map(|d| d.relative_path.clone())
        .collect();
    docs.sort();
    docs
}

#[test]
fn t13_python_leg_drops_profile_excluded_docs() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("src/a.py", "def a():\n    return 1\n"),
            ("dist/c.py", "def c():\n    return 3\n"),
            (".venv/lib/b.py", "def b():\n    return 2\n"),
            (".code-reality.toml", "exclude = [\"dist/\"]\n"),
        ],
    );
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly_boundary(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("python leg");
    assert_eq!(rep.face, "python-face");
    assert_eq!(slot_doc_paths(&repo), vec!["src/a.py".to_string()]);
    assert!(
        rep.notes.iter().any(|n| n.contains("governed")),
        "{:?}",
        rep.notes
    );
}

#[test]
fn t14_python_leg_without_profile_keeps_dist() {
    // No profile → DEFAULT_EXCLUDE (`.venv/`) only: dist/ stays governed.
    // (.venv/ never appears — dot-dirs are corpus-invisible on every face.)
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("src/a.py", "def a():\n    return 1\n"),
            ("dist/c.py", "def c():\n    return 3\n"),
            (".venv/lib/b.py", "def b():\n    return 2\n"),
        ],
    );
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly_boundary(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("python leg");
    assert_eq!(rep.face, "python-face");
    assert_eq!(
        slot_doc_paths(&repo),
        vec!["dist/c.py".to_string(), "src/a.py".to_string()]
    );
}

#[test]
fn t16_python_partial_producer_warns_missing_governed() {
    // F1 narrowed: the producer succeeds but omits a governed file — the
    // build still publishes (one bad file must not sink indexing) with a
    // loud WARN naming the missing governed document.
    use protobuf::Message;
    use scip::types::{Document, Index, Occurrence};
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("src/a.py", "def a():\n    return 1\n"),
            ("src/b.py", "def b():\n    return 2\n"),
        ],
    );
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "src/a.py".to_string();
    for name in ["a", "a_aux", "a_third", "a_fourth"] {
        let mut occ = Occurrence::new();
        occ.symbol = format!("pyrefly python proj 0.1.0 `m`/{name}().");
        occ.symbol_roles = 1;
        occ.range = vec![0, 0, 1];
        d.occurrences.push(occ);
    }
    index.documents.push(d);
    let fx = bindir.path().join("pyfx-subset.scip");
    std::fs::write(&fx, index.write_to_bytes().unwrap()).unwrap();
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
mkdir -p \"$(dirname \"$out\")\"
cp '{}' \"$out\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh(),
            fx.display()
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("partial publish succeeds");
    assert_eq!(rep.face, "python-face");
    assert_eq!(slot_doc_paths(&repo), vec!["src/a.py".to_string()]);
    let warn = rep
        .notes
        .iter()
        .find(|n| n.contains("未索引") && n.contains("src/b.py"));
    assert!(
        warn.is_some(),
        "missing governed must WARN: {:?}",
        rep.notes
    );
}

#[test]
fn t17_build_report_prints_effective_excludes() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("src/a.py", "def a():\n    return 1\n"),
            (".code-reality.toml", "exclude = [\"dist/\"]\n"),
        ],
    );
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly_boundary(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("python leg");
    let first = rep.notes.first().expect("report carries排除集");
    assert!(
        first.contains("排除集")
            && first.contains(".venv/（內建）")
            && first.contains("dist/（profile）"),
        "unexpected first note: {first}"
    );
}

#[test]
fn t15_python_all_excluded_converges_to_empty() {
    // TS `s2_ts_only_all_excluded_converges_to_empty` mirror: an
    // all-profile-excluded Python corpus is a legal empty terminal state
    // under auto-detection; an explicit override stays a loud error.
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("gen/a.py", "x = 1\n"),
            (".code-reality.toml", "exclude = [\"gen/\"]\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly_boundary(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("empty convergence");
    assert_eq!(rep.face, "empty(profile-excluded)");
    assert!(!slot_of(&repo).exists());
    assert!(!graph_db_path(&repo).exists());
    let err = build_repo(&repo, Some(ProducerFamily::Python), &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("未產出索引")),
        "{err:?}"
    );
}

fn slot_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/index.scip")
}

// ---------- S3: query-time heal (ep-index-query-time-self-heal) ----------

fn fake_pyrefly_pre(dir: &Path, pre: &str) {
    let fx = install_pyfx(dir);
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
{pre}
mkdir -p \"$(dirname \"$out\")\"
cp '{}' \"$out\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh(),
            fx.display()
        ),
    );
}

fn fake_rust_analyzer_counted(dir: &Path, counter: &Path) {
    fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi
{}
echo x >> '{}'
cp '{FIXTURE}' \"$output\"
echo '[OK] fake rust-analyzer'
",
            arg_parse_sh(),
            counter.display()
        ),
    );
}

fn counter_lines(c: &Path) -> usize {
    std::fs::read_to_string(c)
        .map(|t| t.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

fn git_commit(repo: &Path) {
    for args in [
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

fn lock_of(repo: &Path) -> PathBuf {
    repo.join(".code-reality/scip/.heal.lock")
}

#[test]
fn t13_fresh_noop_is_zero_spawn() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_pyrefly_pre(bindir.path(), &format!("echo x >> '{}'", counter.display()));
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("initial build");
    std::fs::remove_file(&counter).unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert_eq!(out, HealOutcome::Fresh);
    assert_eq!(counter_lines(&counter), 0, "steady state spawns nothing");
}

#[test]
fn t14_stale_heals_and_releases_lock() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::Healed { nodes, .. } if nodes > 0),
        "out={out:?}"
    );
    assert!(!lock_of(&repo).exists(), "lock released after heal");
}

#[test]
fn t15_head_drift_only_no_rebuild() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_pyrefly_pre(bindir.path(), &format!("echo x >> '{}'", counter.display()));
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::fs::remove_file(&counter).unwrap();
    std::fs::write(repo.join("docs.md"), "docs only\n").unwrap();
    git_commit(&repo);

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert_eq!(out, HealOutcome::Fresh, "head-drift-only never rebuilds");
    assert_eq!(counter_lines(&counter), 0);
}

#[test]
fn t16_producer_fail_serve_stale_lock_released() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    // swap the producer to fail on run (--version still answers)
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
exit 1
",
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();

    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::ServeStale(ref lines) if lines.iter().any(|l| !l.is_empty())),
        "out={out:?}"
    );
    assert!(!lock_of(&repo).exists(), "lock released after failed heal");
}

#[test]
fn t17_half_success_probe() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");

    // index fresh + graph step failed → Healed with a graph note, not
    // mislabeled serve-stale (SM-17)
    let out =
        heal_outcome_after_rebuild_err(&repo, &slot_of(&repo), "graph core boom".into()).unwrap();
    match &out {
        HealOutcome::Healed { notes, .. } => {
            assert!(notes.iter().any(|n| n.contains("graph 未重建")), "{out:?}")
        }
        other => panic!("expected Healed, got {other:?}"),
    }

    // index still stale + rebuild failed → ServeStale carrying the reason
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let out2 =
        heal_outcome_after_rebuild_err(&repo, &slot_of(&repo), "graph core boom".into()).unwrap();
    assert!(
        matches!(out2, HealOutcome::ServeStale(ref l) if l[0].contains("graph core boom")),
        "out={out2:?}"
    );
}

#[test]
fn t18_false_stale_warns_once_no_loop() {
    // Disk-vs-index alignment per the EP fixture spec: rich.scip's docs
    // are crates/a.rs + crates/b.rs, so the repo carries exactly those
    // plus one extra disk file the fake never indexes → persistent
    // missing=1 (the false-stale shape).
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("crates/a.rs", "x"), ("crates/b.rs", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_rust_analyzer_counted(bindir.path(), &counter);
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::fs::remove_file(&counter).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("crates/c.rs"), "x").unwrap();

    let out1 = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out1, HealOutcome::ServeStale(ref l) if l[0].contains("語料不一致")),
        "out={out1:?}"
    );
    // Post-codex-P0-4 semantics: the non-converged heal ARMS the churn
    // cooldown, so the second query is cooldown-ServeStale — never Fresh
    // (calling an incomplete index fresh was the closed hole). The
    // warn-once core invariant holds either way: no further rebuild.
    let out2 = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out2, HealOutcome::ServeStale(ref l) if l.iter().any(|s| s.contains("cooldown"))),
        "out={out2:?}"
    );
    assert_eq!(
        counter_lines(&counter),
        1,
        "exactly one rebuild across both calls"
    );
}

#[test]
fn t19_lock_escape_and_single_flight() {
    // (a) an abandoned lock (mtime past max age) is stolen
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    std::fs::write(lock_of(&repo), "1 2020-01-01T00:00:00+00:00\n").unwrap();
    let st = std::process::Command::new("touch")
        .args(["-t", "200001010000"])
        .arg(lock_of(&repo))
        .status()
        .unwrap();
    assert!(st.success());
    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "out={out:?}");
    assert!(!lock_of(&repo).exists());

    // (b) concurrent stale hits single-flight through the lock: a slow
    // producer runs exactly once across both healers
    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo2);
    let bindir2 = tempfile::tempdir().unwrap();
    let counter2 = bindir2.path().join("calls");
    fake_pyrefly_pre(
        bindir2.path(),
        &format!("sleep 1; echo x >> '{}'", counter2.display()),
    );
    let roots2 = vec![bindir2.path().to_path_buf()];
    build_repo(&repo2, None, &roots2).expect("build");
    std::fs::remove_file(&counter2).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo2.join("app2.py"), "x = 1\n").unwrap();

    let h1 = {
        let (r, roots) = (repo2.clone(), roots2.clone());
        std::thread::spawn(move || ensure_fresh(&r, &roots))
    };
    let h2 = {
        let (r, roots) = (repo2.clone(), roots2.clone());
        std::thread::spawn(move || ensure_fresh(&r, &roots))
    };
    let o1 = h1.join().unwrap().unwrap();
    let o2 = h2.join().unwrap().unwrap();
    assert!(
        !matches!(o1, HealOutcome::ServeStale(_)) && !matches!(o2, HealOutcome::ServeStale(_)),
        "o1={o1:?} o2={o2:?}"
    );
    assert_eq!(
        counter_lines(&counter2),
        1,
        "single-flight: one producer run"
    );
}

#[test]
fn t20_readonly_slot_dir_serves_stale() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let dir = repo.join(".code-reality/scip");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = ensure_fresh(&repo, &roots).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(out, HealOutcome::ServeStale(ref l) if l[0].contains("heal lock")),
        "out={out:?}"
    );
}

// env var is process-global — guard the off-switch test against the
// parallel heal tests in this file
static HEAL_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn t21_cli_heal_across_query_modes() {
    // e2e through the real cli face: the heal's build_repo resolves the
    // producer from the real roots (PATH) — the L4-shaped path with the
    // actual installed pyrefly-index (machine prerequisite). Holds
    // HEAL_ENV throughout: the env gate is process-global and t22
    // flips it (parallel-test race guard).
    let _guard = HEAL_ENV.lock().unwrap();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let repo_s = repo.display().to_string();
    let stale = |n: &str| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(repo.join(n), "def q():\n    return f()\n").unwrap();
    };

    // query mode: heal fires, the fresh real index answers
    stale("app2.py");
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);
    assert!(out.stdout.contains("f()."), "stdout={}", out.stdout);

    // callers / closure / audit ride the same hook (placed after the
    // final guard, before every mode branch)
    stale("app3.py");
    let out = code_reality::cli::run(&["scip_refs", "--callers", "f", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);

    stale("app4.py");
    let out = code_reality::cli::run(&["scip_refs", "--closure", "f", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);

    // audit mode (graph_audit env prerequisite: real rust-analyzer)
    stale("app5.py");
    let out = code_reality::cli::run(&["scip_refs", "--audit", "--repo", &repo_s]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("index healed"), "stderr={}", out.stderr);
}

#[test]
fn t23_sm15_staleness_check_err_face() {
    // SM-15: the staleness CHECK itself failing (unreadable subdir) →
    // loud WARN on stderr, query still answers from the existing index
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    let locked = repo.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("z.py"), "x = 1\n").unwrap();
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let repo_s = repo.display().to_string();
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        out.stderr.contains("索引過期檢查失敗"),
        "stderr={}",
        out.stderr
    );
    // still answers: the rich fixture has no f() → no-DEF exit 1 is an answer
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.contains("[WARN] 查無 DEF"));
}

#[test]
fn t22_cli_off_switch_and_non_heal_faces() {
    let _guard = HEAL_ENV.lock().unwrap();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let slot = slot_of(&repo);
    let before = std::fs::read(&slot).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();
    let repo_s = repo.display().to_string();
    let slot_s = slot.display().to_string();

    // env off → old behavior: no heal, stale slot answered as-is
    std::env::set_var("CODE_REALITY_AUTOHEAL", "off");
    let out = code_reality::cli::run(&["scip_refs", "f", "--repo", &repo_s]);
    std::env::remove_var("CODE_REALITY_AUTOHEAL");
    assert_eq!(out.exit_code, 1, "rich fixture has no f() → 查無 DEF");
    assert!(
        !out.stderr.contains("index healed"),
        "stderr={}",
        out.stderr
    );
    assert_eq!(std::fs::read(&slot).unwrap(), before, "slot untouched");

    // explicit --index is user-owned: never healed
    let out = code_reality::cli::run(&["scip_refs", "f", "--index", &slot_s, "--repo", &repo_s]);
    assert_eq!(out.exit_code, 1);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(std::fs::read(&slot).unwrap(), before);

    // write modes never heal (stamp / build-cache own their flow)
    let out = code_reality::cli::run(&["scip_refs", "--stamp-meta", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "stamp rewrites meta only"
    );
    let out = code_reality::cli::run(&["scip_refs", "--build-cache", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(!out.stderr.contains("index healed"));
    assert_eq!(std::fs::read(&slot).unwrap(), before);
}

// ---------- AIR-33 ③: churn cooldown ----------

#[test]
fn t18_churn_cooldown_skips_reheal_within_window() {
    let _env = churn_env_guard();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    // A "producer" that finishes but leaves a source NEWER than the slot
    // (touch AFTER the index copy — simulates an active writer editing
    // during the heal); the SM-9 loop guard arms the churn marker.
    fake_bin(
        bindir.path(),
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
{}
mkdir -p \"$(dirname \"$out\")\"
cp '{FIXTURE}' \"$out\"
echo x >> '{}'
touch \"$repo/app.py\"
echo '[OK] fake pyrefly-index'
",
            arg_parse_sh(),
            counter.display()
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("initial build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "x = 1\n").unwrap();

    let first = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&first, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("又變動"))),
        "out={first:?}"
    );
    assert!(
        repo.join(".code-reality/scip/.heal-churn").exists(),
        "churn marker armed by the non-converging heal"
    );
    let calls_after_first = counter_lines(&counter);
    assert!(calls_after_first >= 1);

    let second = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&second, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("cooldown"))),
        "out={second:?}"
    );
    assert_eq!(
        counter_lines(&counter),
        calls_after_first,
        "cooldown spawns nothing"
    );
}

#[test]
fn t19_head_drift_overrides_cooldown_and_convergence_clears() {
    let _env = churn_env_guard();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    // arm the churn marker directly (as a non-converging heal would)
    std::fs::write(repo.join(".code-reality/scip/.heal-churn"), b"").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app3.py"), "y = 2\n").unwrap();

    // head unchanged → the cooldown holds
    let held = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&held, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("cooldown"))),
        "out={held:?}"
    );
    assert_eq!(counter_lines(&counter), 0, "held query spawns nothing");

    // commit moves HEAD → override heals (converging producer clears it)
    git_commit(&repo);
    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::Healed { nodes, .. } if nodes > 0),
        "out={out:?}"
    );
    assert!(
        !repo.join(".code-reality/scip/.heal-churn").exists(),
        "converging heal clears the marker"
    );
}

/// Serializes tests that mutate or depend on the cooldown env var
/// (process-global; cargo runs test threads in parallel).
fn churn_env_guard() -> std::sync::MutexGuard<'static, ()> {
    static M: std::sync::Mutex<()> = std::sync::Mutex::new(());
    M.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn t20_cooldown_env_zero_disables() {
    let _env = churn_env_guard();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    // an armed fresh marker would hold — the documented escape hatch
    // must bypass it
    std::fs::write(repo.join(".code-reality/scip/.heal-churn"), b"").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app4.py"), "z = 3\n").unwrap();
    std::env::set_var("CODE_REALITY_HEAL_COOLDOWN_SECS", "0");
    let out = ensure_fresh(&repo, &roots).unwrap();
    std::env::remove_var("CODE_REALITY_HEAL_COOLDOWN_SECS");
    assert!(
        matches!(out, HealOutcome::Healed { nodes, .. } if nodes > 0),
        "out={out:?}"
    );
}

#[test]
fn t21_wait_peer_respects_armed_marker() {
    let _env = churn_env_guard();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app5.py"), "z = 4\n").unwrap();
    // A peer holds the heal lock; while we poll it "finishes" its
    // non-converging heal (arms the marker) then releases the lock —
    // our re-evaluation must serve stale on the cooldown instead of
    // becoming the healer.
    std::fs::write(lock_of(&repo), b"").unwrap();
    let peer_repo = repo.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let _ = std::fs::write(peer_repo.join(".code-reality/scip/.heal-churn"), b"");
        let _ = std::fs::remove_file(lock_of(&peer_repo));
    });
    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&out, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("cooldown"))),
        "out={out:?}"
    );
}

#[test]
fn t22_expired_marker_lets_heal_resume() {
    let _env = churn_env_guard();
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let marker = repo.join(".code-reality/scip/.heal-churn");
    std::fs::write(&marker, b"").unwrap();
    // backdate past the 600s window
    let st = std::process::Command::new("touch")
        .args(["-t", "202001010000"])
        .arg(&marker)
        .status()
        .unwrap();
    assert!(st.success());
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app6.py"), "z = 5\n").unwrap();
    let out = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(out, HealOutcome::Healed { nodes, .. } if nodes > 0),
        "out={out:?}"
    );
    assert!(
        !marker.exists(),
        "converging heal clears even a backdated marker"
    );
}
