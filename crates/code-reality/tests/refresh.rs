//! S4 refresh/hook tests (ep-index-query-time-self-heal): installer
//! bytes/idempotency/refusals/reverse; refresh heal + docs-only head-sync
//! (real producer via the natural roots — the L4-shaped path).

use code_reality::build::build_repo;
use code_reality::refresh::{hook_install, hook_remove, run};
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

fn fake_pyrefly(dir: &Path) {
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--repo\" ]; then repo=\"$a\"; fi; prev=\"$a\"; done
mkdir -p \"$repo/.code-reality/scip\"
cp '{FIXTURE}' \"$repo/.code-reality/scip/index.scip\"
echo '[OK] fake pyrefly-index'
"
        ),
    );
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

fn git_config(repo: &Path, key: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--get", key])
        .output()
        .unwrap();
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn git_head_of(repo: &Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn hook_install_bytes_idempotent_and_reverse() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];

    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    let hook = repo.join(".githooks/post-commit");
    let text = std::fs::read_to_string(&hook).unwrap();
    assert!(text.contains("# code-reality post-commit refresh (opt-in)"));
    assert!(text.contains("nohup"), "{text}");
    // F12: heal failures stay observable in the data dir's own log
    assert!(text.contains("refresh.log"), "{text}");
    // the resolved ABSOLUTE bin path is embedded (GUI no-PATH trap)
    assert!(
        text.contains(&bindir.path().join("code-reality").display().to_string()),
        "{text}"
    );
    assert!(text.trim_end().ends_with('&'), "background form: {text}");
    assert!(
        std::fs::metadata(&hook).unwrap().permissions().mode() & 0o111 != 0,
        "hook must be executable"
    );
    assert_eq!(
        git_config(&repo, "core.hooksPath").as_deref(),
        Some(".githooks")
    );

    // idempotent rerun: marker match, bytes unchanged
    let bytes_before = std::fs::read(&hook).unwrap();
    let out2 = hook_install(&repo, &roots);
    assert_eq!(out2.exit_code, 0);
    assert_eq!(std::fs::read(&hook).unwrap(), bytes_before);

    // reverse: file gone, config unset
    let out3 = hook_remove(&repo);
    assert_eq!(out3.exit_code, 0, "{}", out3.stdout);
    assert!(!hook.exists());
    assert_eq!(git_config(&repo, "core.hooksPath"), None);
}

#[test]
fn hook_install_refuses_unmanaged_hook_and_foreign_hooks_path() {
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];

    // unmanaged existing post-commit: loud refusal, file untouched
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let hooks = repo.join(".githooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("post-commit"), "#!/bin/sh\necho custom\n").unwrap();
    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 2, "stderr={}", out.stderr);
    assert!(out.stderr.contains("不覆蓋"), "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(hooks.join("post-commit")).unwrap(),
        "#!/bin/sh\necho custom\n"
    );

    // foreign core.hooksPath: loud refusal, config untouched
    let t2 = tempfile::tempdir().unwrap();
    let repo2 = mkrepo(&t2, &[("app.py", "x")]);
    git_init(&repo2);
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo2)
        .args(["config", "core.hooksPath", ".husky"])
        .status()
        .unwrap();
    assert!(st.success());
    let out2 = hook_install(&repo2, &roots);
    assert_eq!(out2.exit_code, 2, "stderr={}", out2.stderr);
    assert!(out2.stderr.contains("不覆寫"), "{}", out2.stderr);
    assert_eq!(
        git_config(&repo2, "core.hooksPath").as_deref(),
        Some(".husky")
    );
}

#[test]
fn hook_install_refuses_active_local_hooks() {
    // flipping core.hooksPath would silently disable .git/hooks/* —
    // refuse over active local hooks instead (post-build F10)
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let hooks = repo.join(".git/hooks");
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho hi\n").unwrap();
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let out = hook_install(&repo, &[bindir.path().to_path_buf()]);
    assert_eq!(out.exit_code, 2, "stderr={}", out.stderr);
    assert!(out.stderr.contains("停用"), "{}", out.stderr);
    assert!(!repo.join(".githooks/post-commit").exists());
}

#[test]
fn refresh_heals_stale_via_real_producer() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    build_repo(&repo, None, &[bindir.path().to_path_buf()]).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(repo.join("app2.py"), "def g():\n    return 2\n").unwrap();

    let repo_s = repo.display().to_string();
    let out = run(&["refresh", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(out.stderr.contains("已重產"), "stderr={}", out.stderr);
    let slot = repo.join(".code-reality/scip/index.scip");
    let snap = code_reality::engine::evaluate_staleness(&repo, &slot).unwrap();
    assert!(!snap.source_newer, "post-refresh slot is fresh");
}

#[test]
fn refresh_docs_only_head_syncs_meta_only() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    build_repo(&repo, None, &[bindir.path().to_path_buf()]).expect("build");
    let slot = repo.join(".code-reality/scip/index.scip");
    let before = std::fs::read(&slot).unwrap();

    std::fs::write(repo.join("docs.md"), "docs only\n").unwrap();
    git_commit(&repo);
    let repo_s = repo.display().to_string();
    let out = run(&["refresh", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        out.stderr.contains("meta head 已同步") || out.stderr.is_empty(),
        "docs-only: head-sync (or fresh-no-drift); stderr={}",
        out.stderr
    );
    // index bytes untouched — the re-stamp writes the meta sidecar only
    assert_eq!(std::fs::read(&slot).unwrap(), before);
    let meta = std::fs::read_to_string(code_reality::engine::meta_path(&slot)).unwrap();
    assert!(
        meta.contains(&git_head_of(&repo)),
        "meta head synced to current HEAD"
    );
}

#[test]
fn refresh_arg_guards() {
    let o = run(&["refresh"]);
    assert_eq!(o.exit_code, 2);
    let o = run(&["refresh", "--help"]);
    assert_eq!(o.exit_code, 0);
    assert!(o.stdout.contains("usage:"));
    let o = run(&["hook"]);
    assert_eq!(o.exit_code, 2);
    let o = run(&["hook", "install"]);
    assert_eq!(o.exit_code, 2);
    let o = run(&["hook", "frobnicate", "--repo", "/tmp"]);
    assert_eq!(o.exit_code, 2);
    assert!(o.stderr.contains("install 或 remove"), "{}", o.stderr);
}
