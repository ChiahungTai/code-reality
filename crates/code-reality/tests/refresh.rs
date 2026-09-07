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
    // burst debounce wiring: event marker + heartbeat + runner detach +
    // the quiet-window env knob
    assert!(text.contains("refresh.pending"), "{text}");
    assert!(text.contains("refresh.scheduled"), "{text}");
    assert!(text.contains("CODE_REALITY_REFRESH_QUIET_SECS"), "{text}");
    assert!(
        text.contains(") > /dev/null 2>&1 &"),
        "runner detached: {text}"
    );
    // the resolved ABSOLUTE bin path is embedded (GUI no-PATH trap)
    assert!(
        text.contains(&bindir.path().join("code-reality").display().to_string()),
        "{text}"
    );
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

// ---------- hook burst debounce (trailing edge) ----------

fn fire_hook(repo: &Path) {
    let st = std::process::Command::new(repo.join(".githooks/post-commit"))
        .current_dir(repo)
        .env("CODE_REALITY_REFRESH_QUIET_SECS", "1")
        .status()
        .unwrap();
    assert!(st.success(), "hook run failed");
}

fn counter_lines(c: &Path) -> usize {
    std::fs::read_to_string(c)
        .map(|t| t.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

/// Returns (repo, counter, bin-guard) — the bindir tempdir must outlive
/// the hook firings (the script embeds its absolute path; dropping the
/// TempDir deletes the fake bin before the runner calls it).
fn debounced_fixture(t: &tempfile::TempDir) -> (PathBuf, PathBuf, tempfile::TempDir) {
    let repo = mkrepo(t, &[("app.py", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let counter = bindir.path().join("calls");
    fake_bin(
        bindir.path(),
        "code-reality",
        &format!("#!/bin/sh\necho x >> '{}'\n", counter.display()),
    );
    let out = hook_install(&repo, &[bindir.path().to_path_buf()]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    (repo, counter, bindir)
}

#[test]
fn hook_burst_coalesces_into_one_tail_refresh() {
    let t = tempfile::tempdir().unwrap();
    let (repo, counter, _bin_guard) = debounced_fixture(&t);
    // burst: three rapid fires (rebase-replay shape) — one runner, one
    // tail refresh after the quiet window
    for _ in 0..3 {
        fire_hook(&repo);
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    assert_eq!(counter_lines(&counter), 0, "quiet window holds the refresh");
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert_eq!(counter_lines(&counter), 1, "burst tail = ONE refresh");

    // a later isolated event refreshes exactly once more
    fire_hook(&repo);
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert_eq!(counter_lines(&counter), 2, "isolated event = one refresh");
}

#[test]
fn hook_dead_runner_marker_is_respawned() {
    let t = tempfile::tempdir().unwrap();
    let (repo, counter, _bin_guard) = debounced_fixture(&t);
    // a scheduled marker left by a crashed runner (ancient epoch) must
    // not swallow events forever — the hook detects the dead heartbeat
    // and respawns
    let data = repo.join(".code-reality");
    std::fs::create_dir_all(&data).unwrap(); // the hook mkdirs at run time
    std::fs::write(data.join("refresh.scheduled"), "1000\n").unwrap();
    fire_hook(&repo);
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(
        counter_lines(&counter) >= 1,
        "dead marker respawned and refreshed"
    );
    assert!(
        !data.join("refresh.scheduled").exists(),
        "runner cleaned up its marker after firing"
    );
}

#[test]
fn hook_non_numeric_marker_is_sanitized() {
    let t = tempfile::tempdir().unwrap();
    let (repo, counter, _bin_guard) = debounced_fixture(&t);
    let data = repo.join(".code-reality");
    std::fs::create_dir_all(&data).unwrap();
    // garbage marker content must not abort the hook (dash-family sh
    // exits 2 on the arithmetic — the case-sanitize coerces to 0 and
    // respawns instead)
    std::fs::write(data.join("refresh.scheduled"), "abc\n").unwrap();
    fire_hook(&repo); // asserts exit 0 despite the garbage marker
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(
        counter_lines(&counter) >= 1,
        "sanitized marker respawned and refreshed"
    );
}

#[test]
fn hook_install_upgrades_managed_script_in_place() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];
    // an OLD-format managed hook (marker + pre-debounce one-liner body)
    let hooks = repo.join(".githooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(
        hooks.join("post-commit"),
        format!(
            "#!/bin/sh\n# code-reality post-commit refresh (opt-in)\nmkdir -p .code-reality\nnohup '{}' refresh --repo \"$(git rev-parse --show-toplevel)\" >> .code-reality/refresh.log 2>&1 &\n",
            bindir.path().join("code-reality").display()
        ),
    )
    .unwrap();

    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.contains("升級"),
        "upgrade message: {}",
        out.stdout
    );
    let upgraded = std::fs::read_to_string(hooks.join("post-commit")).unwrap();
    assert!(
        upgraded.contains("refresh.pending"),
        "current template in place: {upgraded}"
    );
    assert_eq!(
        git_config(&repo, "core.hooksPath").as_deref(),
        Some(".githooks"),
        "upgrade also ensures the hook stays wired"
    );

    // byte-identical rerun stays a no-op (bytes unchanged + message)
    let bytes = std::fs::read(hooks.join("post-commit")).unwrap();
    let out2 = hook_install(&repo, &roots);
    assert_eq!(out2.exit_code, 0);
    assert!(
        out2.stdout.contains("冪等"),
        "no-op message: {}",
        out2.stdout
    );
    assert_eq!(std::fs::read(hooks.join("post-commit")).unwrap(), bytes);
}

#[test]
fn hook_install_allows_rerun_with_inert_local_hooks() {
    // hooksPath already `.githooks` (the managed normal state): inert
    // .git/hooks/* leftovers must not block reruns/upgrades — the flip
    // guard only guards the flip
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];
    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    std::fs::write(repo.join(".git/hooks/pre-commit"), "#!/bin/sh\necho hi\n").unwrap();
    let out2 = hook_install(&repo, &roots);
    assert_eq!(
        out2.exit_code, 0,
        "inert local hooks must not block a managed rerun: {}",
        out2.stderr
    );
}

#[test]
fn hook_install_refuses_managed_hook_with_foreign_hooks_path() {
    // a managed script whose hooksPath was repointed (husky etc.) gets a
    // loud refusal instead of a misleading idempotent OK
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];
    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["config", "core.hooksPath", ".husky"])
        .status()
        .unwrap();
    assert!(st.success());
    let out2 = hook_install(&repo, &roots);
    assert_eq!(out2.exit_code, 2, "{}", out2.stderr);
    assert!(out2.stderr.contains("不覆寫"), "{}", out2.stderr);
}

#[test]
fn refresh_nudges_old_format_managed_hook() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    build_repo(&repo, None, &[bindir.path().to_path_buf()]).expect("build");
    // an old-format managed hook must draw the upgrade hint on refresh
    let hooks = repo.join(".githooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(
        hooks.join("post-commit"),
        "#!/bin/sh\n# code-reality post-commit refresh (opt-in)\nexit 0\n",
    )
    .unwrap();

    let repo_s = repo.display().to_string();
    let out = run(&["refresh", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        out.stderr.contains("hook install") && out.stderr.contains("舊格式"),
        "nudge present: {}",
        out.stderr
    );
}

#[test]
fn refresh_does_not_nudge_current_format_or_absent_hook() {
    // reverse control: a current-format (v2) managed hook draws no
    // nudge, and neither does having no hook at all
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "def f():\n    return 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    fake_bin(bindir.path(), "code-reality", "#!/bin/sh\nexit 0\n");
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");
    let out = hook_install(&repo, &roots);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);

    let repo_s = repo.display().to_string();
    let out = run(&["refresh", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        !out.stderr.contains("舊格式"),
        "current-format hook draws no nudge: {}",
        out.stderr
    );

    std::fs::remove_file(repo.join(".githooks/post-commit")).unwrap();
    let out = run(&["refresh", "--repo", &repo_s]);
    assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
    assert!(
        !out.stderr.contains("舊格式"),
        "absent hook draws no nudge: {}",
        out.stderr
    );
}
