//! S2 snapshot tests — synthetic graph db + a real throwaway git repo.
//! Byte faces (OK/WARN line shapes, file schema) are pinned from the
//! frozen Python source; cross-language byte comparison lives in the
//! S6 parity harness.

mod graph_db_fixture;

use code_reality::snapshot::{build_snapshot, detect_stale, export_module_edges, run};
use code_reality::ToolOutput;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn git(repo: &Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(st.success(), "git {:?} failed", args);
}

/// Throwaway repo: profile with two modules, an exclude entry, a commit.
/// The path is canonicalized up front: export relativizes against the
/// resolved root, so synthetic qualified names must be real paths too
/// (pytest's tmp_path is already canonical — same posture).
fn repo_fixture(tag: &str) -> PathBuf {
    let repo = tempfile::tempdir().unwrap().keep().join(tag);
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("mosaic_alpha/domain")).unwrap();
    std::fs::create_dir_all(repo.join("mosaic_alpha/infra")).unwrap();
    std::fs::create_dir_all(repo.join(".venv/pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"mosaic_alpha/\"\nexclude = [\".agent-tmp/\"]\n",
    )
    .unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    std::fs::canonicalize(&repo).unwrap()
}

fn db_with_edges(repo: &Path) -> PathBuf {
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.metadata
        .push(("git_head_sha".into(), "deadbeefdeadbeef".into()));
    let q = |rel: &str, sym: &str| graph_db_fixture::qualified(repo, rel, sym);
    for (kind, s, t) in [
        (
            "IMPORTS_FROM",
            q("mosaic_alpha/domain/a.py", "A"),
            q("mosaic_alpha/infra/b.py", "B"),
        ),
        // same-module edge: files still counted, edge dropped
        (
            "CALLS",
            q("mosaic_alpha/domain/a.py", "A"),
            q("mosaic_alpha/domain/c.py", "C"),
        ),
        // excluded endpoint: skipped entirely
        (
            "INHERITS",
            q("mosaic_alpha/domain/a.py", "A"),
            q(".venv/pkg/v.py", "V"),
        ),
        // non-structural kinds: participate in the files face (S2 all-kind),
        // never in module_edges; d.py joins ONLY through its REFERENCES edge
        // (the explicit two-face separation pin, T5)
        (
            "REFERENCES",
            q("mosaic_alpha/infra/b.py", "B"),
            q("mosaic_alpha/domain/a.py", "A2"),
        ),
        (
            "REFERENCES",
            q("mosaic_alpha/infra/b.py", "B"),
            q("mosaic_alpha/infra/d.py", "D"),
        ),
    ] {
        spec.edges.push((kind.into(), s, t));
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    db
}

#[test]
fn export_counts_same_module_files_and_skips_excluded() {
    let repo = repo_fixture("export");
    let db = db_with_edges(&repo);
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let profile = code_reality::profile::load_profile(&repo).unwrap();
    let out = export_module_edges(&conn, &repo, profile.as_ref()).unwrap();
    assert_eq!(
        out.files,
        vec![
            "mosaic_alpha/domain/a.py".to_string(),
            "mosaic_alpha/domain/c.py".to_string(),
            "mosaic_alpha/infra/b.py".to_string(),
            // d.py participates ONLY via the REFERENCES edge — in the
            // all-kind files face, absent from module_edges (T5)
            "mosaic_alpha/infra/d.py".to_string(),
        ]
    );
    assert_eq!(
        out.module_edges,
        vec![vec![
            "mosaic_alpha/domain".to_string(),
            "mosaic_alpha/infra".to_string(),
            "IMPORTS_FROM".to_string(),
        ]]
    );
    assert_eq!(out.raw_edge_count, 5); // all kinds counted raw
                                       // two-face separation: the REFERENCES-only file never leaks into the
                                       // structural face (module edge set has no infra->infra row for it)
    assert!(out
        .module_edges
        .iter()
        .all(|e| !e.iter().any(|c| c.contains("d.py"))));
    assert_eq!(out.kind_edge_count, 3); // IMPORTS_FROM + CALLS + INHERITS rows
}

#[test]
fn detect_stale_three_levels() {
    let mut meta = HashMap::new();
    meta.insert("git_head_sha".to_string(), "aaaa".to_string());
    // sha mismatch beats everything
    assert_eq!(
        detect_stale(&meta, Some("bbbb"), 1000, "T", None),
        Some("graph sha aaaa != HEAD bbbb".to_string())
    );
    // sha match → fresh regardless of times
    assert_eq!(detect_stale(&meta, Some("aaaa"), 1000, "T", None), None);
    // last_updated older → stale reason embeds the raw value + HEAD iso
    let mut meta2 = HashMap::new();
    meta2.insert(
        "last_updated".to_string(),
        "2020-01-01T00:00:00".to_string(),
    );
    assert_eq!(
        detect_stale(
            &meta2,
            Some("bbbb"),
            1_800_000_000,
            "2026-12-31T00:00:00+08:00",
            None
        ),
        Some(
            "graph last_updated 2020-01-01T00:00:00 < HEAD commit 2026-12-31T00:00:00+08:00"
                .to_string()
        )
    );
    // last_updated newer → fresh
    meta2.insert(
        "last_updated".to_string(),
        "2030-01-01T00:00:00".to_string(),
    );
    assert_eq!(
        detect_stale(&meta2, Some("bbbb"), 1_800_000_000, "T", None),
        None
    );
    // falls to mtime level
    let meta3 = HashMap::new();
    assert_eq!(
        detect_stale(
            &meta3,
            Some("x"),
            1_800_000_000,
            "T",
            Some((1_700_000_000, "old".into()))
        ),
        Some("graph mtime old < HEAD commit T".to_string())
    );
    assert_eq!(
        detect_stale(
            &meta3,
            Some("x"),
            1_800_000_000,
            "T",
            Some((1_900_000_000, "new".into()))
        ),
        None
    );
}

fn run_cli(args: &[&str]) -> ToolOutput {
    run(args)
}

#[test]
fn cli_ok_line_and_sidecar_schema() {
    let repo = repo_fixture("cli_ok");
    let db = db_with_edges(&repo);
    let sha = String::from_utf8(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    // align the graph sha with HEAD → fresh (no WARN)
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE metadata SET value = ?1 WHERE key = 'git_head_sha'",
        (&sha,),
    )
    .unwrap();
    drop(conn);

    let out_dir = repo.join("snaps");
    let repo_s = repo.to_string_lossy().to_string();
    let out_dir_s = out_dir.to_string_lossy().to_string();
    let out = run_cli(&["snapshot", "--repo", &repo_s, "--out-dir", &out_dir_s]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    assert!(out.stderr.is_empty());
    let want_ok = format!(
        "[OK] snapshot: 4 files, 1 module edges -> {}\n",
        out_dir
            .join(format!(
                "{}-{}.json",
                repo.file_name().unwrap().to_string_lossy(),
                &sha[..8]
            ))
            .display()
    );
    assert_eq!(
        out.stdout,
        format!(
            "{want_ok}[LOG] rg '\"module_edges\"' {} | head\n",
            out_dir
                .join(format!(
                    "{}-{}.json",
                    repo.file_name().unwrap().to_string_lossy(),
                    &sha[..8]
                ))
                .display()
        )
    );

    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join(format!(
            "{}-{}.json",
            repo.file_name().unwrap().to_string_lossy(),
            &sha[..8]
        )))
        .unwrap(),
    )
    .unwrap();
    let keys: Vec<&str> = sidecar["_meta"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        vec![
            "repo",
            "commit",
            "created_at",
            "tool",
            "label",
            "stale",
            "crg_last_updated",
            "crg_last_build_type",
            "crg_raw_edges",
            // face-version marker (T12 tail addition; T10 value pin)
            "files_face"
        ]
    );
    assert_eq!(sidecar["_meta"]["crg_raw_edges"], serde_json::json!(5));
    assert_eq!(
        sidecar["_meta"]["files_face"],
        serde_json::json!("all-kinds")
    );
    assert_eq!(sidecar["files"].as_array().unwrap().len(), 4);
}

#[test]
fn cli_stale_warn_line_shape() {
    let repo = repo_fixture("cli_stale");
    db_with_edges(&repo); // sha stays deadbeef... → mismatch WARN
    let out_dir = repo.join("snaps");
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0);
    assert!(out
        .stdout
        .starts_with("[WARN] graph stale: graph sha deadbeef != HEAD "));
    assert!(out
        .stdout
        .contains("——先 `code-reality graph_db build --repo <repo>` 再 snapshot\n"));
}

#[test]
fn cli_subdir_repo_is_git_root_crash() {
    let repo = repo_fixture("cli_subdir");
    db_with_edges(&repo);
    let sub = repo.join("mosaic_alpha");
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &sub.to_string_lossy(),
        "--out-dir",
        &repo.join("snaps").to_string_lossy(),
    ]);
    // no graph.db under the subdir → missing-db crash family first
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.contains("[FAIL] graph.db 不存在"),
        "{}",
        out.stderr
    );
}

#[test]
fn cli_missing_db_crash_message() {
    let repo = repo_fixture("cli_missing");
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join("snaps").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr
            .contains("先跑 `code-reality graph_db build --repo <repo>`"),
        "{}",
        out.stderr
    );
}

#[test]
fn cli_empty_set_warn() {
    let repo = repo_fixture("cli_empty");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "x".into()));
    spec.edges.push((
        "IMPORTS_FROM".into(),
        "/elsewhere/x.py::A".into(),
        "/elsewhere/y.py::B".into(),
    ));
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join("snaps").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0);
    // root branch: kind-matched rows exist but none resolve into the repo
    // root — the "different root?" attribution is truthful HERE (S1 T1;
    // bytes must not flip again in S2 — the count is kind-matched rows
    // at the loop entry either way)
    assert!(out.stdout.contains(
        "[WARN] snapshot 空集合（0 files，db raw 1 邊）——結構 kind 匹配 1 邊但全數無法解析進 repo root 或被 profile 排除：graph.db 與 --repo 不同 root？下游 diff（delta_tour）會誤報無結構變化\n"
    ));
    assert!(out
        .stdout
        .contains("[OK] snapshot: 0 files, 0 module edges -> "));
}

#[test]
fn cli_empty_set_warn_zero_edge_db() {
    // zero-edge db (an aborted/empty build): the third WARN branch with
    // its rebuild guidance — pins the branch both other empty-set tests
    // cannot reach (kind_n==0 ∧ raw==0; review F1/R-1)
    let repo = repo_fixture("cli_empty_zero");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    // metadata must be present (load_metadata crashes on a half-set db —
    // that is a different failure family); zero edges is the point here
    spec.metadata.push(("git_head_sha".into(), "x".into()));
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join("snaps").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains(
        "[WARN] snapshot 空集合（0 files，db 零邊——空 build？）——先 `code-reality graph_db build --repo <repo>`；下游 diff（delta_tour）會誤報無結構變化\n"
    ));
    assert!(out
        .stdout
        .contains("[OK] snapshot: 0 files, 0 module edges -> "));
}

#[test]
fn cli_empty_set_warn_kind_distribution() {
    // non-structural edges that ALL fail to resolve into the repo root:
    // post-S2 the in-root REFERENCES fixture yields files>0 (that is the
    // fix, pinned by export_files_face_is_all_kinds) — this branch's
    // remaining population is out-of-root/excluded non-structural edges
    // (T2, reworked in S2 per the recorded EP ripple note)
    let repo = repo_fixture("cli_empty_kind");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "x".into()));
    for (s, t) in [
        ("/elsewhere/r1.py::A", "/elsewhere/r2.py::B"),
        ("/elsewhere/r3.py::C", "/elsewhere/r4.py::D"),
    ] {
        spec.edges.push(("REFERENCES".into(), s.into(), t.into()));
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let out = run_cli(&[
        "snapshot",
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &repo.join("snaps").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains(
        "[WARN] snapshot 空集合（0 files，db raw 2 邊）——結構 kind 匹配 0 邊、raw 全數非結構 kind 且無法解析進 repo root 或被 profile 排除；下游 diff 會誤報無結構變化\n"
    ));
    assert!(out
        .stdout
        .contains("[OK] snapshot: 0 files, 0 module edges -> "));
}

#[test]
fn export_files_face_is_all_kinds() {
    // the S2 headline (T7): a REFERENCES-only db with in-repo endpoints
    // yields a NON-empty participating file set — the snapshot-owned kind
    // decision (files face = all kinds) pinned at the export level
    let repo = repo_fixture("all_kinds");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "x".into()));
    let q = |rel: &str, sym: &str| graph_db_fixture::qualified(&repo, rel, sym);
    for (s, t) in [
        (
            q("mosaic_alpha/domain/a.py", "A"),
            q("mosaic_alpha/infra/b.py", "B"),
        ),
        (
            q("mosaic_alpha/domain/c.py", "C"),
            q("mosaic_alpha/infra/b.py", "B2"),
        ),
    ] {
        spec.edges.push(("REFERENCES".into(), s, t));
    }
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let profile = code_reality::profile::load_profile(&repo).unwrap();
    let out = export_module_edges(&conn, &repo, profile.as_ref()).unwrap();
    assert_eq!(
        out.files,
        vec![
            "mosaic_alpha/domain/a.py".to_string(),
            "mosaic_alpha/domain/c.py".to_string(),
            "mosaic_alpha/infra/b.py".to_string(),
        ],
        "REFERENCES-only participating files must be exported (all-kind face)"
    );
    assert!(out.module_edges.is_empty());
    assert_eq!(out.kind_edge_count, 0); // zero structural rows matched
    assert_eq!(out.raw_edge_count, 2);
}

#[test]
fn mixed_kind_files_union_module_edges_structural() {
    // T9: CALLS + REFERENCES db — files = the cross-kind union,
    // module_edges = the structural subset only
    let repo = repo_fixture("mixed_kinds");
    let db = repo.join(".code-reality").join("graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut spec = graph_db_fixture::GraphDbSpec::default();
    spec.metadata.push(("git_head_sha".into(), "x".into()));
    let q = |rel: &str, sym: &str| graph_db_fixture::qualified(&repo, rel, sym);
    spec.edges.push((
        "CALLS".into(),
        q("mosaic_alpha/domain/a.py", "A"),
        q("mosaic_alpha/infra/b.py", "B"),
    ));
    spec.edges.push((
        "REFERENCES".into(),
        q("mosaic_alpha/infra/b.py", "B"),
        q("mosaic_alpha/infra/d.py", "D"),
    ));
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let profile = code_reality::profile::load_profile(&repo).unwrap();
    let out = export_module_edges(&conn, &repo, profile.as_ref()).unwrap();
    assert_eq!(
        out.files,
        vec![
            "mosaic_alpha/domain/a.py".to_string(),
            "mosaic_alpha/infra/b.py".to_string(),
            "mosaic_alpha/infra/d.py".to_string(),
        ],
        "files = union across kinds"
    );
    assert_eq!(
        out.module_edges,
        vec![vec![
            "mosaic_alpha/domain".to_string(),
            "mosaic_alpha/infra".to_string(),
            "CALLS".to_string()
        ]],
        "module_edges = structural subset only"
    );
    assert_eq!(out.kind_edge_count, 1);
}

#[test]
fn build_snapshot_writes_committed_label() {
    let repo = repo_fixture("label");
    db_with_edges(&repo);
    let snap = build_snapshot(&repo, Some("ep-rust-r4")).unwrap();
    assert_eq!(snap.meta["label"], serde_json::json!("ep-rust-r4"));
    assert!(snap.meta["stale"].is_string()); // deadbeef mismatch
}

#[test]
fn help_face_bytes() {
    let out = run_cli(&["snapshot", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out
        .stdout
        .starts_with("usage: snapshot [-h] [--repo REPO] [--label LABEL]\n"));
    assert!(out.stdout.ends_with("  --out-dir OUT_DIR\n"));
}

#[test]
fn usage_error_exit_2() {
    let out = run_cli(&["snapshot", "--nope"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
}

// ---------- S3 cutover: endpoint resolution via nodes table ----------

#[test]
fn export_resolves_symbols_via_nodes_with_dangling_fallback() {
    let repo = repo_fixture("symres");
    let db = repo.join(".code-reality/graph.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let spec = graph_db_fixture::GraphDbSpec {
        nodes: vec![graph_db_fixture::NodeSeed {
            name: "foo".into(),
            parent: None,
            qname: "lsp python mosaic_alpha/domain/a.py foo().".into(),
            file_path: repo.join("mosaic_alpha/domain/a.py").display().to_string(),
        }],
        edges: vec![
            // producer-form endpoint: resolvable ONLY via the nodes map
            (
                "CALLS".into(),
                "lsp python mosaic_alpha/domain/a.py foo().".into(),
                "dangling::B".into(),
            ),
        ],
        ..Default::default()
    };
    graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    let conn = code_reality::common::connect_ro(&db).unwrap();
    let out = export_module_edges(&conn, &repo, None).unwrap();
    // dangling endpoint falls back to the ::-split: no repo file -> edge
    // skipped, but the nodes-resolved endpoint cannot drag it in either
    assert!(out.module_edges.is_empty(), "{:?}", out.module_edges);
    // both endpoints must exist in the repo for the edge to export: give
    // the dangling endpoint a node too
    std::fs::remove_file(&db).unwrap();
    let spec2 = graph_db_fixture::GraphDbSpec {
        nodes: vec![
            graph_db_fixture::NodeSeed {
                name: "foo".into(),
                parent: None,
                qname: "lsp python mosaic_alpha/domain/a.py foo().".into(),
                file_path: repo.join("mosaic_alpha/domain/a.py").display().to_string(),
            },
            graph_db_fixture::NodeSeed {
                name: "bar".into(),
                parent: None,
                qname: "lsp python mosaic_alpha/infra/b.py bar().".into(),
                file_path: repo.join("mosaic_alpha/infra/b.py").display().to_string(),
            },
        ],
        edges: vec![(
            "CALLS".into(),
            "lsp python mosaic_alpha/domain/a.py foo().".into(),
            "lsp python mosaic_alpha/infra/b.py bar().".into(),
        )],
        ..Default::default()
    };
    graph_db_fixture::make_graph_db(&db, &spec2).unwrap();
    let conn2 = code_reality::common::connect_ro(&db).unwrap();
    let profile = code_reality::profile::load_profile(&repo).unwrap();
    let out2 = export_module_edges(&conn2, &repo, profile.as_ref()).unwrap();
    assert_eq!(
        out2.module_edges,
        vec![vec![
            "mosaic_alpha/domain".to_string(),
            "mosaic_alpha/infra".to_string(),
            "CALLS".to_string()
        ]],
        "symbol endpoints resolve via nodes"
    );
}

#[test]
fn default_out_dir_is_in_repo_and_self_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let d = code_reality::snapshot::default_out_dir(tmp.path()).unwrap();
    assert_eq!(
        d,
        tmp.path()
            .canonicalize()
            .unwrap()
            .join(".code-reality")
            .join("snapshots")
    );
    let gi = d.parent().unwrap().join(".gitignore");
    assert!(
        gi.is_file(),
        "data-dir gitignore written by default_out_dir"
    );
    let body = std::fs::read_to_string(&gi).unwrap();
    assert!(body.trim_end().ends_with('*') && !body.contains('!'));
}
