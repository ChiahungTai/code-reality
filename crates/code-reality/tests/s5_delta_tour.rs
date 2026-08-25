//! R5-S5 delta_tour tests — mirrors of the absorbed test_delta_tour
//! case families (281e07e semantics): range-sourced step set, claims
//! three-state, deletion collapse, commit subjects, cleanup window.

use code_reality::delta_tour::{build_tour, cleanup_expired, kebab};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C").arg(repo).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().unwrap();
    assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn rev(repo: &Path) -> String { git(repo, &["rev-parse", "HEAD"]) }

/// Two-commit fixture: A(before) adds files; B(after) modifies one,
/// adds another, deletes one, renames one.
fn repo_fixture(tag: &str) -> (PathBuf, String, String) {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(repo.join(".code-reality.toml"), "[[module]]\nprefix = \"pkg/\"\n").unwrap();
    std::fs::write(repo.join("pkg/mod.py"), "# header\ndef keep():\n    pass\n").unwrap();
    std::fs::write(repo.join("pkg/gone.py"), "x = 1\n").unwrap();
    std::fs::write(repo.join("pkg/old_name.py"), "def renamed_fn():\n    pass\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    let before = rev(&repo);
    std::fs::write(repo.join("pkg/mod.py"), "# header\ndef keep():\n    return 42\n").unwrap();
    std::fs::write(
        repo.join("pkg/new_file.py"),
        "# copyright\n\ndef fresh_decl():\n    pass\n",
    )
    .unwrap();
    std::fs::remove_file(repo.join("pkg/gone.py")).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["mv", "pkg/old_name.py", "pkg/new_place.py"]);
    git(&repo, &["commit", "-qm", "the change"]);
    let after = rev(&repo);
    (repo, before, after)
}

fn trans_json(before: &str, after: &str) -> Value {
    json!({
        "_meta": {"before": before, "after": after, "repo": "r"},
        "added": [["m/a", "m/b", "CALLS"]],
        "removed": [],
        "changed_modules": ["pkg"],
        "ep_claims": {
            "claims": ["pkg"],
            "claims_none": false,
            "claimed_and_changed": ["pkg"],
            "changed_not_claimed": [],
            "claimed_not_changed": []
        }
    })
}

#[test]
fn step_set_from_range_with_collapse_and_anchors() {
    let (repo, before, after) = repo_fixture("steps");
    let mut stderr = String::new();
    let tour = build_tour(&trans_json(&before, &after), &repo, None, "test", &mut stderr).unwrap();
    let steps = tour["steps"].as_array().unwrap().clone();
    // overview + mod(M) + new_file(A) + new_place(R) + gone collapse(D)
    assert_eq!(steps.len(), 5, "{tour}");
    let overview = &steps[0];
    assert!(overview["title"].as_str().unwrap().starts_with("弧總覽："));
    assert!(overview["description"].as_str().unwrap().contains("1 新檔、1 改名、1 修改、1 刪檔"));
    // claims compared state — ✓ present in the overview section
    assert!(overview["description"].as_str().unwrap().contains("✓ 命中 (1)：pkg"));
    // M step anchors first hunk (line 3 in mod.py)
    let m_step = steps.iter().skip(1).find(|s| s["file"] == "pkg/mod.py").unwrap();
    assert_eq!(m_step["line"], 3);
    // A step anchors first declaration (blank line after copyright →
    // fresh_decl lands on line 3 — copyright skipped)
    let a_step = steps.iter().skip(1).find(|s| s["file"] == "pkg/new_file.py").unwrap();
    assert_eq!(a_step["line"], 3, "{}", a_step);
    // R step notes the old name
    let r_step = steps.iter().skip(1).find(|s| s["file"] == "pkg/new_place.py").unwrap();
    let r_desc = r_step["description"].as_str().unwrap();
    assert!(r_desc.contains("改名自 `pkg/old_name.py`"), "{r_desc}");
    // commit subject present
    assert!(r_desc.contains("commit: the change"), "{r_desc}");
    // D collapse step
    let d_step = steps.last().unwrap();
    assert_eq!(d_step["file"], "pkg/gone.py");
    assert!(d_step["title"].as_str().unwrap().starts_with("−刪檔 ×1"));
}

#[test]
fn claims_three_states() {
    let (repo, before, after) = repo_fixture("claims");
    let mut stderr = String::new();
    // no_ep
    let tour = build_tour(&json!({
        "_meta": {"before": before, "after": after},
        "added": [], "removed": [], "changed_modules": ["pkg"]
    }), &repo, None, "t", &mut stderr).unwrap();
    let desc0 = tour["steps"][0]["description"].as_str().unwrap();
    assert!(desc0.contains("**EP 宣稱**：NONE（未提供 --ep）"), "{desc0}");
    // not_compared: claims_none
    let tour2 = build_tour(&json!({
        "_meta": {"before": before, "after": after},
        "added": [], "removed": [], "changed_modules": ["pkg"],
        "ep_claims": {"claims_none": true, "claimed_and_changed": [], "changed_not_claimed": [], "claimed_not_changed": []}
    }), &repo, None, "t", &mut stderr).unwrap();
    let desc2 = tour2["steps"][0]["description"].as_str().unwrap();
    assert!(desc2.contains("**EP 宣稱對照：未比對**"), "{desc2}");
    // zero-hit guard: not_compared with WARN to stderr
    let tour3 = build_tour(&json!({
        "_meta": {"before": before, "after": after},
        "added": [], "removed": [], "changed_modules": ["pkg", "other"],
        "ep_claims": {"claims_none": false, "claimed_and_changed": [], "changed_not_claimed": ["pkg", "other"], "claimed_not_changed": []}
    }), &repo, None, "t", &mut stderr).unwrap();
    let desc3 = tour3["steps"][0]["description"].as_str().unwrap();
    assert!(desc3.contains("未比對"), "{desc3}");
    assert!(stderr.contains("[WARN] 宣稱對照 0 命中"), "{stderr}");
    // ⚠ tag NOT applied in not-compared states
    let m_step = tour3["steps"].as_array().unwrap().iter().find(|s| s["file"] == "pkg/mod.py").unwrap();
    assert!(!m_step["title"].as_str().unwrap().contains("⚠"), "{}", m_step);
}

#[test]
fn kebab_and_cleanup() {
    assert_eq!(kebab("My EP Name"), "my-ep-name");
    assert_eq!(kebab("ep.r4-測試"), "ep-r4");
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("2020-01-01-old.tour"), "{}").unwrap();
    std::fs::write(dir.join("2999-01-01-future.tour"), "{}").unwrap();
    std::fs::write(dir.join("notadate.tour"), "{}").unwrap();
    std::fs::write(dir.join("2020-01-01-notes.md"), "{}").unwrap();
    let removed = cleanup_expired(dir, 7, "2026-08-26");
    assert_eq!(removed, 1); // only the old .tour
    assert!(!dir.join("2020-01-01-old.tour").exists());
    assert!(dir.join("notadate.tour").exists());
    assert!(dir.join("2020-01-01-notes.md").exists());
}

#[test]
fn cli_faces() {
    let out = code_reality::delta_tour::run(&["delta_tour", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("usage: delta_tour [-h]"));
    let out = code_reality::delta_tour::run(&["delta_tour", "a.json"]);
    assert_eq!(out.exit_code, 2);
}

#[test]
fn cli_end_to_end_with_snapshots() {
    let (repo, before, after) = repo_fixture("cli");
    let tmp = tempfile::tempdir().unwrap();
    let sa = tmp.path().join("a.json");
    let sb = tmp.path().join("b.json");
    std::fs::write(&sa, serde_json::to_string(&json!({
        "_meta": {"repo": "r", "commit": before}, "files": ["pkg/mod.py"],
        "module_edges": [["pkg/a", "pkg/b", "CALLS"]]
    })).unwrap()).unwrap();
    std::fs::write(&sb, serde_json::to_string(&json!({
        "_meta": {"repo": "r", "commit": after}, "files": ["pkg/mod.py"],
        "module_edges": [["pkg/a", "pkg/b", "CALLS"]]
    })).unwrap()).unwrap();
    let out_dir = repo.join("out");
    let out = code_reality::delta_tour::run(&[
        "delta_tour",
        &sa.to_string_lossy(),
        &sb.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("[OK] delta tour: 5 steps -> "), "{}", out.stdout);
    let today = code_reality::delta_tour::local_today();
    let written = out_dir.join(format!("{today}-review.tour"));
    assert!(written.exists());
    let tour: Value =
        serde_json::from_str(&std::fs::read_to_string(written).unwrap()).unwrap();
    assert_eq!(tour["title"], "review 變更導覽");
}
