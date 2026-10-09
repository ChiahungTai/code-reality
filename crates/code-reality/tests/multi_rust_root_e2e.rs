//! Multi-root root-graph E2E acceptance (EP DB-100 S4; frozen TC-7).
//!
//! Real-git fixture: two sibling `[workspace]` roots (`rust/`,
//! `dutymail/`) plus a root-level Python (`app.py`) and JavaScript
//! (`tool.mjs`) face — the full AC1-4 chain: build → freshness →
//! snapshot → delta_tour → Cargo-only change → resolver failure
//! (SM-1/3/4b/8). The fake rust-analyzer emits WORKSPACE-relative
//! documents per invoked project dir (the empirically verified RA
//! 1.96.0 anchoring); its docs carry DEF spans plus one cross-file REF
//! (`b.rs` → `a()`) so the composed graph has a real edge and the
//! snapshot files face is non-empty. Snapshot / delta_tour faces
//! follow the s2_snapshot / s5_delta_tour test conventions.

mod support;

use code_reality::build::{build_repo, ensure_fresh, HealOutcome};
use code_reality::engine::{default_index_path, meta_path};
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

// ---------- SCIP fixture builders ----------

const A_SYM: &str = "rust-analyzer cargo a/1.0.0 a().";
const B_SYM: &str = "rust-analyzer cargo b/1.0.0 b().";
const DM_SYM: &str = "rust-analyzer cargo dm/1.0.0 dm().";

fn def_occ(symbol: &str, enc: [i32; 4]) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = 1; // DEF
    o.range = vec![0, 7, 8];
    o.enclosing_range = enc.to_vec();
    o
}

fn ref_occ(symbol: &str, line: i32) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = 0; // REF
    o.range = vec![line, 4, 5];
    o
}

fn doc(rel: &str, occs: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    d.relative_path = rel.to_string();
    d.occurrences = occs;
    d
}

fn scip_bytes(docs: Vec<Document>) -> Vec<u8> {
    let mut index = Index::new();
    // Real rust-analyzer scip output always carries Index.metadata —
    // without it the synthetic index trips the EMPTY_INDEX_BYTES gate
    // (same fixture amendment as multi_rust_build, DB-100 S3).
    let mut tool = ToolInfo::new();
    tool.name = "rust-analyzer".to_string();
    tool.version = "1.96.0-test".to_string();
    tool.arguments = vec!["scip".to_string(), "file:///test-workspace-root".to_string()];
    let mut meta = Metadata::new();
    meta.tool_info = protobuf::MessageField::some(tool);
    meta.project_root = "file:///test-workspace-root".to_string();
    meta.text_document_encoding = protobuf::EnumOrUnknown::from(TextEncoding::UTF8);
    index.metadata = protobuf::MessageField::some(meta);
    index.documents = docs;
    index.write_to_bytes().unwrap()
}

// ---------- lab ----------

struct Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    /// Holds the per-root fixture SCIP files the fake RA copies from —
    /// must outlive the test (the fake script embeds these paths).
    _scipdir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

/// Dual-workspace + root py/js lab (TC-7 fixture: two Cargo workspaces
/// + root .py + root .mjs in one git repo).
fn lab_e2e() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/Cargo.lock", "version = 3\n");
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    write(&repo, "rust/crates/b.rs", "pub fn b() {}\n");
    write(&repo, "dutymail/Cargo.toml", workspace_manifest());
    write(&repo, "dutymail/Cargo.lock", "version = 3\n");
    write(&repo, "dutymail/crates/dm.rs", "pub fn dm() {}\n");
    // the third faces: root-level Python + JavaScript
    write(&repo, "app.py", "x = 1\n");
    write(&repo, "tool.mjs", "export const tool = 1;\n");
    support::git_init(&repo);

    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());

    // Fake rust-analyzer: workspace-relative docs keyed on the passed
    // project dir; b.rs REFs a() inside b()'s span → one real edge.
    let scipdir = tempfile::tempdir().unwrap();
    let rust_path = scipdir.path().join("rust.scip");
    let dm_path = scipdir.path().join("dutymail.scip");
    std::fs::write(
        &rust_path,
        scip_bytes(vec![
            doc("crates/a.rs", vec![def_occ(A_SYM, [0, 0, 2, 1])]),
            doc(
                "crates/b.rs",
                vec![def_occ(B_SYM, [0, 0, 3, 1]), ref_occ(A_SYM, 1)],
            ),
        ]),
    )
    .unwrap();
    std::fs::write(
        &dm_path,
        scip_bytes(vec![doc("crates/dm.rs", vec![def_occ(DM_SYM, [0, 0, 1, 1])])]),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi\ndir=\"$2\"\nout=''; prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then out=\"$a\"; fi; prev=\"$a\"; done\ncase \"$dir\" in\n  */rust) cp '{rp}' \"$out\" ;;\n  */dutymail) cp '{dm}' \"$out\" ;;\n  *) echo \"unknown project dir: $dir\" >&2; exit 1 ;;\nesac\n",
        rp = rust_path.display(),
        dm = dm_path.display(),
    );
    support::fake_bin(bindir.path(), "rust-analyzer", &script);
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        _scipdir: scipdir,
        roots,
    }
}

// ---------- git helpers (real git, -c identity flags, CWD = repo) ----------

fn git(repo: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_all(repo: &Path, msg: &str) {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", msg]);
}

fn head(repo: &Path) -> String {
    git(repo, &["rev-parse", "HEAD"])
}

// ---------- face helpers ----------

fn verdict(out: &code_reality::ToolOutput) -> serde_json::Value {
    serde_json::from_str(out.stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not the verdict JSON ({e}): {:?}", out.stdout))
}

fn freshness_json(repo: &Path) -> code_reality::ToolOutput {
    code_reality::freshness::run(&[
        "freshness",
        "--repo",
        &repo.to_string_lossy(),
        "--json",
    ])
}

fn snapshot_to(repo: &Path, out_dir: &Path) -> code_reality::ToolOutput {
    code_reality::snapshot::run(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ])
}

fn sidecar_path(repo: &Path, out_dir: &Path, sha: &str) -> std::path::PathBuf {
    out_dir.join(format!(
        "{}-{}.json",
        repo.file_name().unwrap().to_string_lossy(),
        &sha[..8]
    ))
}

// --- TC7-a: build → fresh + three stamped faces + dual-root graph ---

#[test]
fn tc7a_build_is_fresh_with_three_faces_and_dual_root_graph() {
    let l = lab_e2e();
    let report = build_repo(&l.repo, None, &l.roots).expect("TC7-a: dual-root build");
    assert!(
        report.producers.iter().any(|p| p.contains("roots:")),
        "TC7-a: multi-root producer marker: {:?}",
        report.producers
    );

    // freshness verdict face (exit 0 = fresh)
    let out = freshness_json(&l.repo);
    assert_eq!(
        out.exit_code, 0,
        "TC7-a: fresh after build: {} {}",
        out.stdout, out.stderr
    );
    let v = verdict(&out);
    assert_eq!(v["fresh"], serde_json::json!(true), "{v}");
    assert_eq!(v["serves"], serde_json::json!("current-tree"), "{v}");
    assert_eq!(v["identity_algo"], serde_json::json!("sha256-v2"), "{v}");
    let faces = v["faces"].as_array().unwrap();
    for want in ["python", "rust", "javascript"] {
        assert!(
            faces.iter().any(|f| f == want),
            "TC7-a: verdict faces contain {want}: {faces:?}"
        );
    }

    // stamp face: v2 triple + multi-root topology descriptor
    let meta: serde_json::Value =
        std::fs::read_to_string(meta_path(&default_index_path(&l.repo).unwrap()))
            .unwrap()
            .parse()
            .unwrap();
    let stamped = meta["source_faces"].as_array().unwrap();
    for want in ["python", "rust", "javascript"] {
        assert!(
            stamped.iter().any(|f| f == want),
            "TC7-a: stamped source_faces contain {want}: {stamped:?}"
        );
    }
    let topo = meta["rust_topology"].as_array().unwrap();
    // P3-E (EP 5.3): roots carry collision-proof indexed keys (sorted
    // order ⇒ stable idx) instead of the comma-joined pair.
    for (idx, want) in [("root[0]", "dutymail"), ("root[1]", "rust")] {
        assert!(
            topo.iter()
                .any(|kv| kv == &serde_json::json!([idx, want])),
            "TC7-a: topology carries sorted root {idx}={want}: {topo:?}"
        );
    }

    // graph face: nodes from BOTH workspaces + the py/js faces
    let db = l.repo.join(".code-reality").join("graph.db");
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let mut stmt = conn
        .prepare("SELECT file_path, language FROM nodes")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for want in [
        "rust/crates/a.rs",
        "rust/crates/b.rs",
        "dutymail/crates/dm.rs",
        "app.py",
        "tool.mjs",
    ] {
        assert!(
            rows.iter().any(|(f, _)| f.ends_with(want)),
            "TC7-a: graph node for {want}: {rows:?}"
        );
    }
    for want in ["Rust", "Python", "JavaScript"] {
        assert!(
            rows.iter().any(|(_, lg)| lg == want),
            "TC7-a: graph language {want}: {rows:?}"
        );
    }

    // CLI search face hits symbols from BOTH roots (SM-1)
    let repo_s = l.repo.to_string_lossy().to_string();
    for (query, want) in [("b", "rust/crates/b.rs"), ("dm", "dutymail/crates/dm.rs")] {
        let out = code_reality::graph_engine::run(&[
            "graph_query",
            "search",
            "--repo",
            &repo_s,
            "--query",
            query,
        ]);
        assert_eq!(out.exit_code, 0, "{}", out.stderr);
        let json_part = out.stdout.split_once('\n').unwrap().1;
        let v: serde_json::Value = serde_json::from_str(json_part).unwrap();
        let hits = v["results"].as_array().unwrap();
        assert!(
            hits.iter()
                .any(|n| n["file_path"].as_str().unwrap_or("").ends_with(want)),
            "TC7-a: search '{query}' hits {want}: {hits:?}"
        );
    }
}

// --- TC7-b: post-build commit → snapshot anchors the new HEAD ---

#[test]
fn tc7b_snapshot_anchors_head_commit() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("build");
    // A post-build commit moves HEAD; the snapshot must anchor the NEW
    // head (the consumer flow), not the build-time head.
    write(&l.repo, "app.py", "x = 1\n# consumer edit\n");
    commit_all(&l.repo, "tc7-b consumer commit");
    let sha = head(&l.repo);

    let out_dir = l.repo.join("snaps");
    let out = snapshot_to(&l.repo, &out_dir);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let path = sidecar_path(&l.repo, &out_dir, &sha);
    assert!(
        path.exists(),
        "sidecar at {}: {}",
        path.display(),
        out.stdout
    );
    let sidecar: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        sidecar["_meta"]["commit"],
        serde_json::json!(sha),
        "TC7-b: _meta.commit == git rev-parse HEAD"
    );
    // the files face is root-relative: both rust-workspace files
    // participate through the one cross-file edge (py/js carry no edges)
    assert_eq!(
        sidecar["files"],
        serde_json::json!(["rust/crates/a.rs", "rust/crates/b.rs"]),
        "TC7-b: root-relative files face: {sidecar}"
    );
}

// --- TC7-c: modify a .rs inside a root → delta_tour carries the path ---

#[test]
fn tc7c_delta_tour_carries_root_relative_rust_change() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("build");
    let snaps = tempfile::tempdir().unwrap();
    let out_a = snapshot_to(&l.repo, snaps.path());
    assert_eq!(out_a.exit_code, 0, "{}{}", out_a.stdout, out_a.stderr);
    let sa = sidecar_path(&l.repo, snaps.path(), &head(&l.repo));

    // change a .rs INSIDE the rust workspace root, commit
    write(&l.repo, "rust/crates/b.rs", "pub fn b() {\n    let _ = 1;\n}\n");
    commit_all(&l.repo, "tc7-c: touch rust/crates/b.rs");
    let after = head(&l.repo);
    let out_b = snapshot_to(&l.repo, snaps.path());
    assert_eq!(out_b.exit_code, 0, "{}{}", out_b.stdout, out_b.stderr);
    let sb = sidecar_path(&l.repo, snaps.path(), &after);

    let tours = tempfile::tempdir().unwrap();
    let out = code_reality::delta_tour::run(&[
        "delta_tour",
        &sa.to_string_lossy(),
        &sb.to_string_lossy(),
        "--repo",
        &l.repo.to_string_lossy(),
        "--out-dir",
        &tours.path().to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let tour_path = tours.path().join(format!(
        "{}-review.tour",
        code_reality::delta_tour::local_today()
    ));
    assert!(tour_path.exists(), "tour written: {}", out.stdout);
    let tour: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&tour_path).unwrap()).unwrap();
    assert_eq!(tour["ref"], serde_json::json!(after), "tour forks at after");
    let steps = tour["steps"].as_array().unwrap();
    let m = steps
        .iter()
        .find(|s| s["file"] == "rust/crates/b.rs")
        .expect("TC7-c: changed root-relative rust path in tour steps");
    assert!(
        m["line"].as_i64().unwrap_or(0) >= 1,
        "TC7-c: step anchored: {m}"
    );
    let overview = steps[0]["description"].as_str().unwrap();
    assert!(
        overview.contains("1 修改"),
        "TC7-c: overview counts the modification: {overview}"
    );
}

// --- TC7-e: Cargo.lock-only change inside a root → stale ---

#[test]
fn tc7e_cargo_lock_only_change_is_stale() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("build");
    let out = freshness_json(&l.repo);
    assert_eq!(out.exit_code, 0, "sanity: fresh after build: {out:?}");

    // Cargo-only edit inside one root; zero source touch.
    write(&l.repo, "rust/Cargo.lock", "version = 3\n# tc7-e dep bump\n");
    let out = freshness_json(&l.repo);
    assert_eq!(
        out.exit_code, 1,
        "TC7-e: cargo-only change is stale: {} {}",
        out.stdout, out.stderr
    );
    let v = verdict(&out);
    assert_eq!(v["fresh"], serde_json::json!(false), "{v}");
    let reasons = v["stale_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "content-drift"),
        "TC7-e: content-drift in {reasons:?}"
    );
    assert_eq!(v["serves"], serde_json::json!("committed-baseline"), "{v}");
}

// --- SM-4b: resolver failure on the freshness face → exit 2, never fresh ---

#[test]
fn sm4b_resolver_failure_never_claims_fresh() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("build");
    // Unresolvable configured root: freshness must fail loud (check-
    // failure exit 2) naming CR-RUST-ROOT-02, with NO verdict JSON —
    // a resolver failure must never degrade into a fresh claim.
    write(&l.repo, ".code-reality.toml", "rust_projects = [\"ghost\"]\n");
    let out = freshness_json(&l.repo);
    assert_eq!(out.exit_code, 2, "SM-4b: check-failure exit: {out:?}");
    assert!(
        out.stderr.contains("CR-RUST-ROOT-02"),
        "SM-4b: named resolver error: {}",
        out.stderr
    );
    assert!(
        out.stdout.is_empty(),
        "SM-4b: no verdict — never claims fresh: {:?}",
        out.stdout
    );
}

// --- SM-4b auto-heal half (EP deferred-claim collection): a resolver
// --- failure on the heal face serves stale, never loops, slot intact.

#[test]
fn sm4b_heal_on_resolver_failure_serves_stale_without_loop() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("SM-4b: converged build");
    let slot = default_index_path(&l.repo).unwrap();
    let graph = l.repo.join(".code-reality").join("graph.db");
    // sanity: the build is converged before the resolver breaks
    assert_eq!(
        ensure_fresh(&l.repo, &l.roots).unwrap(),
        HealOutcome::Fresh,
        "SM-4b: fresh before the bad config"
    );
    let slot_before = std::fs::read(&slot).unwrap();
    let graph_before = std::fs::read(&graph).unwrap();

    // Unresolvable configured root: the heal CHECK fails inside
    // evaluate_staleness (walk_sources → resolver CR-RUST-ROOT-02),
    // structurally BEFORE any rebuild dispatch. ensure_fresh degrades to
    // its check-failure contract leg ("Err = the caller warns and
    // answers from the existing index") — that Err IS the serve-stale
    // class at this layer: zero rebuild dispatch ⇒ no loop is reachable
    // (the byte-identical slot below is the mechanical proof). The
    // ServeStale VARIANT (rebuild dispatched, then failed) is the t16
    // shape in tests/build.rs and stays covered there — recorded as the
    // deliberate shape choice for this fixture (the existing helpers do
    // not surface a ServeStale variant for a check-side failure).
    write(&l.repo, ".code-reality.toml", "rust_projects = [\"ghost\"]\n");

    for call in 1..=2 {
        let err = ensure_fresh(&l.repo, &l.roots)
            .err()
            .unwrap_or_else(|| panic!("SM-4b: call {call} must fail loud, got Ok"));
        assert!(
            err.contains("CR-RUST-ROOT-02") && err.contains("ghost"),
            "SM-4b: call {call} carries the resolver message: {err}"
        );
        assert_eq!(
            std::fs::read(&slot).unwrap(),
            slot_before,
            "SM-4b: call {call} — slot byte-identical (no rebuild dispatched)"
        );
        assert_eq!(
            std::fs::read(&graph).unwrap(),
            graph_before,
            "SM-4b: call {call} — graph.db byte-identical"
        );
        assert!(
            !l.repo.join(".code-reality/scip/.heal.lock").exists(),
            "SM-4b: call {call} — no heal lock left behind"
        );
    }

    // Consumer face: the check-failure converts into the serve-stale
    // WARN (cli.rs heal block) and the query still answers from the
    // surviving slot.
    let q = code_reality::cli::run(&["scip_refs", "--repo", &l.repo.to_string_lossy(), "a"]);
    assert_eq!(q.exit_code, 0, "SM-4b: query answers: {} {}", q.stdout, q.stderr);
    assert!(
        q.stderr.contains("索引過期檢查失敗") && q.stderr.contains("CR-RUST-ROOT-02"),
        "SM-4b: serve-stale WARN names the resolver failure: {}",
        q.stderr
    );
    assert!(
        q.stdout.contains("DEF") && q.stdout.contains("rust/crates/a.rs"),
        "SM-4b: answer rides the surviving slot: {}",
        q.stdout
    );
}

// --- TC-7d: composed graph node count == Σ per-root DEF sets ---

/// The composed multi-root graph must not invent or drop rust-face
/// nodes: the node count in graph.db equals the size of the union of
/// the per-root queryable DEF symbol sets (the fixture keeps the roots
/// disjoint, so union == Σ per-root sizes, cross-root intersection 0).
/// The expected set is derived mechanically from the same fixture SCIP
/// bytes the fake rust-analyzer serves — no hand-counted constant.
#[test]
fn tc7d_node_count_equals_sum_of_root_def_sets() {
    let l = lab_e2e();
    build_repo(&l.repo, None, &l.roots).expect("TC7-d: dual-root build");

    // Parse the per-root fixture indexes and collect their distinct DEF
    // symbols; the roots must stay disjoint (asserted) so the expected
    // composed node count is exactly the sum of the set sizes.
    let mut union: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut set_sizes: Vec<(&str, usize)> = Vec::new();
    for (root_name, file) in [("rust", "rust.scip"), ("dutymail", "dutymail.scip")] {
        let bytes = std::fs::read(l._scipdir.path().join(file)).unwrap();
        let index = Index::parse_from_bytes(&bytes).unwrap();
        let defs: std::collections::HashSet<String> = index
            .documents
            .iter()
            .flat_map(|d| &d.occurrences)
            .filter(|o| o.symbol_roles & 1 != 0)
            .map(|o| o.symbol.clone())
            .collect();
        assert!(
            !defs.is_empty(),
            "TC7-d: {root_name} fixture carries no DEF — derivation vacuous"
        );
        for s in &defs {
            assert!(
                union.insert(s.clone()),
                "TC7-d: symbol {s} defined in multiple roots — fixture must keep roots disjoint"
            );
        }
        set_sizes.push((root_name, defs.len()));
    }
    let expected: usize = set_sizes.iter().map(|(_, n)| n).sum();
    assert_eq!(
        union.len(),
        expected,
        "TC7-d: disjoint roots ⇒ union == Σ per-root sets"
    );

    // The graph face: rust-language node count == expected. REFs (the
    // b.rs → a() edge) ride the edges table, never mint nodes.
    let db = l.repo.join(".code-reality").join("graph.db");
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let rust_nodes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE language = 'Rust'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        rust_nodes as usize, expected,
        "TC7-d: composed rust-face node count == Σ per-root DEF set sizes \
         (rust={}, dutymail={})",
        set_sizes[0].1, set_sizes[1].1
    );
}

// --- SM-4 (build face): config under-coverage + third workspace → exit 2 ---

#[test]
fn sm4_config_third_workspace_build_fails_07() {
    // Rust-only lab (no py/js faces → no producer bin is needed before
    // the resolver gate fires). Config lists ONE root while a second
    // root AND a third `[workspace]` sibling are in place: the build
    // face must fail loud CR-RUST-ROOT-07 (Env → exit 2) naming an
    // unlisted workspace — adding a sibling must never silently retain
    // stale coverage (EP SM-4, build-face leg). The unlisted-workspace
    // scan walks read_dir order, so the flagged name is whichever
    // unlisted root it meets first (dutymail or gamma).
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    write(&repo, "rust/Cargo.toml", workspace_manifest());
    write(&repo, "rust/crates/a.rs", "pub fn a() {}\n");
    write(&repo, "dutymail/Cargo.toml", workspace_manifest());
    write(&repo, "dutymail/crates/dm.rs", "pub fn dm() {}\n");
    write(&repo, "gamma/Cargo.toml", workspace_manifest());
    write(&repo, "gamma/crates/g.rs", "pub fn g() {}\n");
    write(&repo, ".code-reality.toml", "rust_projects = [\"rust\"]\n");
    support::git_init(&repo);

    let out = code_reality::build::run(&["build", "--repo", &repo.to_string_lossy()]);
    assert_eq!(out.exit_code, 2, "SM-4: build env-failure exit: {out:?}");
    assert!(
        out.stderr.contains("CR-RUST-ROOT-07"),
        "SM-4: named coverage gate: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("dutymail") || out.stderr.contains("gamma"),
        "SM-4: error names an unlisted workspace root: {}",
        out.stderr
    );
}

// --- P1-C (EP 5.3): governed .rs outside every resolved root fails loud ---

#[test]
fn p1c_orphan_rs_outside_every_root_fails_08() {
    let l = lab_e2e();
    // A governed rust source at the repo root: under NEITHER resolved
    // workspace root. Pre-fix it silently escaped both the producer and
    // the per-root coverage gate; the build must now fail loud (Env →
    // CLI exit 2, CR-RUST-ROOT-08) naming the path and the escape
    // hatches. Asserted through the lib API with injectable producer
    // roots — the gate fires before any producer spawn, and the CLI
    // face's exit-2 mapping for BuildError::Env is covered by SM-4.
    write(&l.repo, "orphan.rs", "pub fn orphan() {}\n");
    let err = build_repo(&l.repo, None, &l.roots)
        .err()
        .expect("P1-C: orphan build fails loud");
    assert!(
        matches!(err, code_reality::build::BuildError::Env(_)),
        "P1-C: Env-class failure (CLI exit 2): {err:?}"
    );
    let msg = format!("{err:?}");
    assert!(
        msg.contains("CR-RUST-ROOT-08"),
        "P1-C: named orphan gate: {msg}"
    );
    assert!(
        msg.contains("orphan.rs"),
        "P1-C: error names the orphan path: {msg}"
    );
    assert!(
        msg.contains("rust_projects"),
        "P1-C: guidance names the rust_projects escape hatch: {msg}"
    );
}
