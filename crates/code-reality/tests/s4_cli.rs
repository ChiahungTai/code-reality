//! S4 CLI tests: mutex family, guard order, mode routing, exit codes, and
//! full stdout shapes on the shared rich.scip fixture.

use code_reality::cli::run;
use std::path::PathBuf;

fn fixture_copy(tmp: &tempfile::TempDir) -> PathBuf {
    let dst = tmp.path().join("index.scip");
    std::fs::copy(fixture_src(), &dst).unwrap();
    dst
}

fn fixture_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rich.scip")
}

fn argv<'a>(parts: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec!["scip_refs"];
    v.extend(parts.iter().copied());
    v
}

#[test]
fn mutex_family_verbatim_messages() {
    // --help exits 0 with usage on stdout (existence-predicate contract)
    let out = run(&argv(&["--help"]));
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("usage: scip_refs"));

    // empty argv → subcommand-required fail, no panic
    let out = run(&[]);
    assert_eq!(out.exit_code, 2);

    let out = run(&argv(&["q", "--build-cache", "--stamp-meta"]));
    assert_eq!(out.exit_code, 2);
    assert_eq!(
        out.stderr,
        "[FAIL] --build-cache 與 --stamp-meta/--audit/查詢互斥\n"
    );

    let out = run(&argv(&["q", "--stamp-meta"]));
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --stamp-meta 與 --audit/查詢互斥\n");

    let out = run(&argv(&["--stamp-meta"]));
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] --stamp-meta 需 --repo\n");
}

#[test]
fn index_resolution_failures() {
    // no --index no --repo
    let out = run(&argv(&["q"]));
    assert_eq!(out.exit_code, 2);
    assert_eq!(
        out.stderr,
        "[FAIL] 需 --index（或 --repo 解析 in-repo 預設 slot）\n"
    );

    // explicit --index missing
    let out = run(&argv(&["q", "--index", "/nonexistent/index.scip"]));
    assert_eq!(out.exit_code, 2);
    assert!(out
        .stderr
        .contains("[FAIL] 索引不在：/nonexistent/index.scip"));

    // default slot missing (unique tempdir basename → no slot, no legacy)
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_string_lossy().to_string();
    let out = run(&argv(&["q", "--repo", &repo]));
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("預設索引不在："));
    assert!(out.stderr.contains("in-repo slot"));
}

#[test]
fn stamp_meta_on_default_slot_ensures_data_dir_ignore() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    let slot = repo.join(".code-reality/scip/index.scip");
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    std::fs::copy(fixture_src(), &slot).unwrap();
    let repo_s = repo.to_string_lossy().to_string();
    // non-git tempdir: stamp itself fails on HEAD, but the write-mode
    // ensure has already self-ignored the data dir (capability: zero
    // consumer gitignore setup)
    let out = run(&argv(&["--repo", &repo_s, "--stamp-meta"]));
    assert_ne!(out.exit_code, 0);
    let gi = repo.join(".code-reality/.gitignore");
    assert!(gi.is_file(), "write modes self-ignore the data dir");
    let body = std::fs::read_to_string(gi).unwrap();
    assert!(body.trim_end().ends_with('*') && !body.contains('!'));
}

#[test]
fn empty_string_query_lands_on_final_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let out = run(&argv(&["", "--index", &idx_s]));
    assert_eq!(out.exit_code, 2);
    assert_eq!(out.stderr, "[FAIL] 需提供查詢或 --audit\n");
}

#[test]
fn query_stdout_full_shape_on_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();

    // Type.method query, no meta / no repo → no [SRC] line at all.
    // Fixture symbols space-separate the path and descriptor → tail() is the
    // descriptor alone (unlike NT symbols where the descriptor carries "kernel/…").
    let out = run(&argv(&["EventStoreLifecycle.open", "--index", &idx_s]));
    assert_eq!(out.exit_code, 0);
    let expected = concat!(
        "[OK] EventStoreLifecycle#open().\n",
        "  DEF  crates/b.rs:7\n",
        "  refs: 1 處（跨檔）\n",
        "    crates/b.rs:10\n",
        "[OK] impl#[EventStoreLifecycle][EventStore]open().\n",
        "  DEF  crates/b.rs:6\n",
        "  refs: 0 處（跨檔）\n",
        "[OK] impl#[EventStoreLifecycle]open().\n",
        "  DEF  crates/a.rs:11\n",
        "  refs: 9 處（跨檔）\n",
        "    crates/b.rs:8\n",
        "    crates/b.rs:9\n",
        "    crates/b.rs:10\n",
        "    crates/b.rs:12\n",
        "    crates/b.rs:13\n",
        "    crates/b.rs:14\n",
        "    ...共 9 處\n"
    );
    assert_eq!(out.stdout, expected);

    // no-DEF query → exit 1 with warn
    let out = run(&argv(&["Nothing.here", "--index", &idx_s]));
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stdout, "[WARN] 查無 DEF：Nothing.here\n");
}

#[test]
fn unstamped_remediation_split_by_divergence() {
    // drifted: a source newer than the slot → rebuild guidance (S2).
    // Both repos are git repos — source_line early-returns before the
    // unstamped branch when repo_sha is also absent (git missing).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("seed.txt"), "s\n").unwrap();
    git_setup(tmp.path());
    let idx = fixture_copy(&tmp);
    std::fs::write(tmp.path().join("src_new.py"), "x = 1\n").unwrap(); // newer than the copy
    let out = run(&argv(&[
        "f",
        "--index",
        &idx.to_string_lossy(),
        "--repo",
        &tmp.path().to_string_lossy(),
    ]));
    assert!(
        out.stderr
            .contains("未 stamp 且原始碼已較新——索引過期：跑 code-reality build"),
        "stderr={}",
        out.stderr
    );

    // not drifted: source older than the slot → legacy re-stamp guidance
    let tmp2 = tempfile::tempdir().unwrap();
    std::fs::write(tmp2.path().join("old.py"), "x = 1\n").unwrap();
    git_setup(tmp2.path());
    let idx2 = fixture_copy(&tmp2);
    // fs::copy on macOS preserves the source mtime (fclonefileat fast
    // path) — pin the copy to NOW so old.py is genuinely older
    std::fs::File::open(&idx2)
        .unwrap()
        .set_modified(std::time::SystemTime::now())
        .unwrap();
    let out2 = run(&argv(&[
        "f",
        "--index",
        &idx2.to_string_lossy(),
        "--repo",
        &tmp2.path().to_string_lossy(),
    ]));
    assert!(
        out2.stderr.contains("未 stamp（生成後跑 --stamp-meta）"),
        "stderr={}",
        out2.stderr
    );
    assert!(!out2.stderr.contains("已較新"), "stderr={}", out2.stderr);
}

fn git_setup(repo: &std::path::Path) {
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
fn head_drift_warn_e2e() {
    // stamped index left behind by a docs-only commit → the single-source
    // head-drift WARN (no heal — explicit --index is user-owned)
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::write(repo.join("a.py"), "x = 1\n").unwrap();
    git_setup(repo);
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let repo_s = repo.to_string_lossy().to_string();

    let out = run(&argv(&[
        "--stamp-meta",
        "--repo",
        &repo_s,
        "--index",
        &idx_s,
    ]));
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);

    std::fs::write(repo.join("docs.md"), "docs\n").unwrap();
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
        assert!(st.success());
    }

    let out = run(&argv(&[
        "EventStoreLifecycle.open",
        "--index",
        &idx_s,
        "--repo",
        &repo_s,
    ]));
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        out.stderr.contains("已離開 index 生成點"),
        "stderr={}",
        out.stderr
    );
}

#[test]
fn stamp_meta_writes_sidecar_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let repo_s = repo.to_string_lossy().to_string();

    let out = run(&argv(&[
        "--stamp-meta",
        "--repo",
        &repo_s,
        "--index",
        &idx_s,
    ]));
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    let head = code_reality::engine::git_head(&repo).unwrap();
    assert_eq!(
        out.stdout,
        format!(
            "[OK] meta stamped：{}（code-reality @ {}）\n",
            code_reality::engine::meta_path(&idx).display(),
            &head[..7]
        )
    );
    let sidecar = std::fs::read_to_string(code_reality::engine::meta_path(&idx)).unwrap();
    assert!(sidecar.contains(&format!("\"head\": \"{}\"", head)));
    assert!(sidecar.contains("\"stamped_at\": \""));
    assert!(sidecar.contains("+00:00"));
    assert!(sidecar.contains("\"tool\": \"code_reality.scip_refs\""));
    assert!(
        sidecar.contains("\"producer\": \""),
        "S2: stamp records the producer version face"
    );

    // idempotent rerun → same shape, file overwritten
    let out2 = run(&argv(&[
        "--stamp-meta",
        "--repo",
        &repo_s,
        "--index",
        &idx_s,
    ]));
    assert_eq!(out2.exit_code, 0);
    assert_eq!(out2.stdout, out.stdout);
}

#[test]
fn stamp_meta_git_failure_is_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let repo_s = tmp.path().to_string_lossy().to_string();
    let out = run(&argv(&[
        "--stamp-meta",
        "--repo",
        &repo_s,
        "--index",
        &idx_s,
    ]));
    assert_eq!(out.exit_code, 2);
    // The git-failure WARN line is prepended before the FAIL (Python parity:
    // scip_refs.py prints the WARN first). Its tail embeds git's own stderr,
    // which varies — assert prefix/suffix, not full equality. (Latent broken
    // assertion on f388b5d: exact-equality contradicted the committed code.)
    assert!(
        out.stderr.starts_with("[WARN] git rev-parse 失敗"),
        "unexpected stderr: {}",
        out.stderr
    );
    assert!(out
        .stderr
        .ends_with("[FAIL] 取不到 repo HEAD——meta 未 stamp\n"));
}

#[test]
fn build_cache_stats_line_on_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let out = run(&argv(&["--build-cache", "--index", &idx_s]));
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "[OK] cache built：{}（7 symbols/17 occurrences）\n",
            code_reality::cache::sqlite_path(&idx).display()
        )
    );
    assert!(code_reality::cache::sqlite_path(&idx).exists());
}

#[test]
fn query_with_repo_no_meta_yields_src_repo_part_only() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = fixture_copy(&tmp);
    let idx_s = idx.to_string_lossy().to_string();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let repo_s = repo.to_string_lossy().to_string();

    let out = run(&argv(&[
        "EventStoreLifecycle.open",
        "--index",
        &idx_s,
        "--repo",
        &repo_s,
    ]));
    assert_eq!(out.exit_code, 0);
    let head = code_reality::engine::git_head(&repo).unwrap();
    assert_eq!(
        out.stdout.lines().next().unwrap(),
        format!("[SRC] repo HEAD @ {}", &head[..7])
    );
    assert!(out.stderr.contains("index meta 未 stamp"));
}
