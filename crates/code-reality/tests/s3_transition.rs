//! S3 transition tests — direct ports of the frozen `test_transition.py`
//! case families (post-sync 281e07e) plus the no-oracle truncation case
//! pinned from source.

use code_reality::profile::load_profile;
use code_reality::transition::{
    diff_edges, extract_baseline, extract_ep_claims, load_snapshot, path_token_claims,
    render_report, run, summarize,
};
use code_reality::ToolOutput;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

type E = (String, String, String);

fn edges(items: &[(&str, &str, &str)]) -> BTreeSet<E> {
    items
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect()
}

fn write_snap(path: &Path, repo: &str, commit: &str, files: &[&str], es: &[(&str, &str, &str)]) {
    let files_v: Vec<&str> = files.to_vec();
    let edges_v: Vec<Vec<&str>> = es.iter().map(|(a, b, c)| vec![*a, *b, *c]).collect();
    let v = serde_json::json!({
        "_meta": {"repo": repo, "commit": commit},
        "files": files_v,
        "module_edges": edges_v,
    });
    std::fs::write(path, serde_json::to_string(&v).unwrap()).unwrap();
}

#[test]
fn diff_reversed_reports_added_direction() {
    let a = edges(&[("m/a", "m/b", "CALLS")]);
    let b = edges(&[("m/b", "m/a", "CALLS")]);
    let d = diff_edges(&a, &b);
    assert_eq!(d.reversed, vec![("m/b".to_string(), "m/a".to_string())]);
    assert_eq!(
        d.added,
        vec![("m/b".to_string(), "m/a".to_string(), "CALLS".to_string())]
    );
    assert_eq!(
        d.removed,
        vec![("m/a".to_string(), "m/b".to_string(), "CALLS".to_string())]
    );
}

#[test]
fn diff_multi_kind_pair_not_false_reversed() {
    // same pair, different kind → real add+remove, NOT a reversal
    let a = edges(&[("m/a", "m/b", "CALLS")]);
    let b = edges(&[("m/a", "m/b", "IMPORTS_FROM")]);
    let d = diff_edges(&a, &b);
    assert!(d.reversed.is_empty());
    assert_eq!(d.added.len(), 1);
    assert_eq!(d.removed.len(), 1);
    // multi-kind duplicates collapse in the pair projection
    let a2 = edges(&[("m/a", "m/b", "CALLS")]);
    let b2 = edges(&[("m/a", "m/b", "CALLS"), ("m/a", "m/b", "IMPORTS_FROM")]);
    let d2 = diff_edges(&a2, &b2);
    assert!(d2.reversed.is_empty());
    assert_eq!(d2.added.len(), 1);
    assert!(d2.removed.is_empty());
}

#[test]
fn diff_same_set_empty() {
    let a = edges(&[("m/a", "m/b", "CALLS"), ("m/a", "m/c", "CALLS")]);
    let d = diff_edges(&a, &a);
    assert!(d.added.is_empty() && d.removed.is_empty() && d.reversed.is_empty());
}

#[test]
fn load_snapshot_rejects_bad_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.json");
    std::fs::write(&bad, "{\"files\": []}").unwrap();
    let err = load_snapshot(&bad).unwrap_err();
    assert!(err.starts_with("非 S2 snapshot 格式"), "{err}");
    let bad2 = tmp.path().join("bad2.json");
    std::fs::write(&bad2, "{\"_meta\": {}, \"module_edges\": [[\"a\", \"b\"]]}").unwrap();
    let err = load_snapshot(&bad2).unwrap_err();
    assert!(err.contains("三元組"), "{err}");
}

#[test]
fn claims_three_buckets_and_none() {
    let claims: BTreeSet<String> = ["mosaic_alpha/domain", "mosaic_alpha/untouched"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut changed = BTreeSet::new();
    changed.insert("mosaic_alpha/domain".to_string());
    changed.insert("mosaic_alpha/features".to_string());
    let cmp = code_reality::transition::compare_claims(&claims, &changed);
    assert_eq!(cmp.claimed_and_changed, vec!["mosaic_alpha/domain"]);
    assert_eq!(cmp.changed_not_claimed, vec!["mosaic_alpha/features"]);
    assert_eq!(cmp.claimed_not_changed, vec!["mosaic_alpha/untouched"]);
    assert!(!cmp.claims_none);
    let empty: BTreeSet<String> = BTreeSet::new();
    assert!(code_reality::transition::compare_claims(&empty, &changed).claims_none);
}

#[test]
fn sync281e07e_relative_path_claim_verified_against_repo() {
    // repo with real directories; EP prose mentions relative paths only
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("mosaic_alpha/domain")).unwrap();
    std::fs::create_dir_all(tmp.path().join("mosaic_alpha/infra")).unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    write_profile(&repo);
    let p = load_profile(&repo).unwrap().unwrap();
    let claims = path_token_claims(
        "edit mosaic_alpha/domain/mod_a.py and mosaic_alpha/infra/svc.py",
        &p,
        &repo,
    );
    let expect: BTreeSet<String> = ["mosaic_alpha/domain", "mosaic_alpha/infra"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(claims, expect);
}

#[test]
fn sync281e07e_relative_unknown_dir_not_claimed() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("mosaic_alpha/domain")).unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    write_profile(&repo);
    let p = load_profile(&repo).unwrap().unwrap();
    let claims = path_token_claims("touches nonexistent/pkg/x.py", &p, &repo);
    assert!(claims.is_empty());
    // bare filenames (no slash) are unmappable
    let claims2 = path_token_claims("renames mod_a.py", &p, &repo);
    assert!(claims2.is_empty());
}

#[test]
fn sync281e07e_without_repo_root_regex_only() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("mosaic_alpha/domain")).unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    write_profile(&repo);
    let p = load_profile(&repo).unwrap().unwrap();
    let ep = tmp.path().join("ep.md");
    std::fs::write(&ep, "relative-only: domain/mod_a.py\n").unwrap();
    // without repo_root: regex face only → relative token does not match
    let claims = extract_ep_claims(&ep, Some(&p), None).unwrap();
    assert!(claims.is_empty());
    // with repo_root: the relative token resolves through the real dir
    let claims = extract_ep_claims(&ep, Some(&p), Some(&repo)).unwrap();
    assert_eq!(
        claims.into_iter().collect::<Vec<_>>(),
        vec!["mosaic_alpha/domain".to_string()]
    );
}

fn write_profile(repo: &Path) -> PathBuf {
    let p = repo.join(".code-reality.toml");
    std::fs::write(&p, "[[module]]\nprefix = \"mosaic_alpha/\"\n").unwrap();
    p
}

#[test]
fn extract_baseline_bold_literal_required() {
    let tmp = tempfile::tempdir().unwrap();
    let ep = tmp.path().join("ep.md");
    std::fs::write(
        &ep,
        "baseline: aabbccddeeff\nsome **baseline**: 0123456789ab\n",
    )
    .unwrap();
    // the bare line must NOT match; the bold one must
    assert_eq!(
        extract_baseline(&ep).unwrap().as_deref(),
        Some("0123456789ab")
    );
}

#[test]
fn report_no_change_and_claims_faces() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    write_snap(
        &sa_path,
        "r",
        "aaaa1111",
        &["f.py"],
        &[("m/a", "m/b", "CALLS")],
    );
    write_snap(
        &sb_path,
        "r",
        "bbbb2222",
        &["f.py"],
        &[("m/a", "m/b", "CALLS")],
    );
    let sa = load_snapshot(&sa_path).unwrap();
    let sb = load_snapshot(&sb_path).unwrap();
    let (diff, nf, gf) = summarize(&sa, &sb);
    let md = render_report(&sa, &sb, None, &diff, &nf, &gf, None);
    assert!(md.contains("## 無結構變化"));
    assert!(md.contains("兩 snapshot 邊集與檔案集相同（同 commit 或無結構變動）。"));
    assert!(md.ends_with("。\n"));
    // claims faces (need a non-empty diff: the no-change early return
    // precedes the claims section in the frozen renderer)
    let mut sb2 = sb.clone();
    sb2.module_edges
        .insert(("m/a".into(), "m/c".into(), "CALLS".into()));
    let (diff2, nf2, gf2) = summarize(&sa, &sb2);
    let none_md = render_report(&sa, &sb2, Some(&BTreeSet::new()), &diff2, &nf2, &gf2, None);
    assert!(none_md.contains("claims: **NONE**——EP 內無 profile prefix 路徑 mention。"));
    assert!(none_md.contains("實際變動模組（供判讀，無宣稱可比對）：['m/a', 'm/c']"));
    let no_ep = render_report(&sa, &sb2, None, &diff2, &nf2, &gf2, None);
    assert!(no_ep.contains("未提供 `--ep`（EP 宣稱模組路徑對照省略）。"));
}

#[test]
fn files_only_change_counts_as_changed_module() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    write_snap(&sa_path, "r", "aaaa1111", &["mosaic_alpha/a.py"], &[]);
    write_snap(
        &sb_path,
        "r",
        "bbbb2222",
        &["mosaic_alpha/a.py", "mosaic_alpha/conditions/new.py"],
        &[],
    );
    // repo with a depth-1 mosaic profile: module_of maps the new file
    std::fs::write(
        tmp.path().join(".code-reality.toml"),
        "[[module]]\nprefix = \"mosaic_alpha/\"\n",
    )
    .unwrap();
    let out = run_cli(&[
        "transition",
        &sa_path.to_string_lossy(),
        &sb_path.to_string_lossy(),
        "--repo",
        &tmp.path().to_string_lossy(),
        "-o",
        &tmp.path().join("t").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("t.json")).unwrap()).unwrap();
    assert_eq!(
        j["changed_modules"],
        serde_json::json!(["mosaic_alpha/conditions"])
    );
    assert_eq!(j["gone_files"], serde_json::json!([]));
    assert_eq!(
        j["new_files"],
        serde_json::json!(["mosaic_alpha/conditions/new.py"])
    );
}

#[test]
fn truncation_over_twenty_appends_more_line() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    let mut many: Vec<(&str, &str, &str)> = Vec::new();
    for i in 0..25 {
        many.push((
            "m/src",
            Box::leak(format!("m/d{i:02}").into_boxed_str()),
            "CALLS",
        ));
    }
    write_snap(&sa_path, "r", "aaaa1111", &[], &[]);
    write_snap(&sb_path, "r", "bbbb2222", &[], &many);
    let sa = load_snapshot(&sa_path).unwrap();
    let sb = load_snapshot(&sb_path).unwrap();
    let (diff, nf, gf) = summarize(&sa, &sb);
    let md = render_report(&sa, &sb, None, &diff, &nf, &gf, None);
    assert!(md.contains("### added (25)"));
    assert!(md.contains("- ... +5 more"));
    assert!(!md.contains("m/d24")); // the 21st entry is truncated away
}

fn run_cli(args: &[&str]) -> ToolOutput {
    run(args)
}

#[test]
fn cli_ok_and_log_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    write_snap(
        &sa_path,
        "r",
        "aaaa11112222",
        &["x.py"],
        &[("m/a", "m/b", "CALLS")],
    );
    write_snap(
        &sb_path,
        "r",
        "bbbb33334444",
        &[],
        &[("m/b", "m/a", "CALLS")],
    );
    let out = run_cli(&[
        "transition",
        &sa_path.to_string_lossy(),
        &sb_path.to_string_lossy(),
        "-o",
        &tmp.path().join("out").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    let md = tmp.path().join("out.md");
    let js = tmp.path().join("out.json");
    assert_eq!(
        out.stdout,
        format!(
            "[OK] transition aaaa1111 -> bbbb3333: +1 / -1 / reversed 1 -> {md} + {js}\n[LOG] rg 'changed_not_claimed' {js}\n",
            md = md.display(),
            js = js.display()
        )
    );
    // B1 direction visible in the md
    let body = std::fs::read_to_string(&md).unwrap();
    assert!(body.contains("### reversed (1)——added 方向"));
    assert!(body.contains("`m/b <-> m/a`"));
}

#[test]
fn cli_baseline_log_and_profileless_warn() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    write_snap(&sa_path, "r", "aaaa1111", &[], &[]);
    write_snap(&sb_path, "r", "bbbb2222", &[], &[]);
    let ep = tmp.path().join("ep.md");
    std::fs::write(&ep, "# EP\n\n**baseline**: aaaaaaa111\n\nbody\n").unwrap();
    let out = run_cli(&[
        "transition",
        &sa_path.to_string_lossy(),
        &sb_path.to_string_lossy(),
        "--ep",
        &ep.to_string_lossy(),
        "--repo",
        &tmp.path().to_string_lossy(),
        "-o",
        &tmp.path().join("t").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    assert!(out.stdout.contains(
        "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，宣稱對照不生效（--repo 預設 cwd）\n"
    ));
    assert!(out
        .stdout
        .contains("[LOG] EP baseline=aaaaaaa111（diff before 應錨定此 commit）\n"));
}

#[test]
fn cli_missing_ep_crashes_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let sa_path = tmp.path().join("a.json");
    let sb_path = tmp.path().join("b.json");
    write_snap(&sa_path, "r", "aaaa1111", &[], &[]);
    write_snap(&sb_path, "r", "bbbb2222", &[], &[]);
    let out = run_cli(&[
        "transition",
        &sa_path.to_string_lossy(),
        &sb_path.to_string_lossy(),
        "--ep",
        &tmp.path().join("nope.md").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 1);
    // profile-less repo (default cwd): the WARN precedes the crash and
    // SURVIVES it — Python prints WARN to stdout before the assert fires
    assert_eq!(
        out.stdout,
        "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，宣稱對照不生效（--repo 預設 cwd）\n"
    );
    assert!(out.stderr.contains("EP 檔不存在"), "{}", out.stderr);
}

#[test]
fn cli_usage_errors_exit_2() {
    let out = run_cli(&["transition", "a.json"]);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    let out = run_cli(&["transition"]);
    assert_eq!(out.exit_code, 2);
    let out = run_cli(&["transition", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out
        .stdout
        .starts_with("usage: transition [-h] [--ep EP] [--repo REPO]\n"));
}
