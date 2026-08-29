//! S3 transition tests — module-level faces of the transition domain
//! (load/summarize/diff/claims/json render). The CLI/report face
//! retired in S4 (delta_tour is the sole diff interface): the md-face
//! and CLI-byte tests went with it; the tour-level contracts live in
//! s5_delta_tour.rs.

use code_reality::profile::load_profile;
use code_reality::transition::{
    diff_edges, extract_ep_claims, load_snapshot, path_token_claims, render_json_value, summarize,
};
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
fn summarize_marks_degenerate_pairs() {
    // T22 (S4): the degenerate guard lives at the summarize layer —
    // every consumer (delta_tour and future ones) inherits the marking
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    let healthy = tmp.path().join("h.json");
    write_snap(&a, "r", "aaaa1111", &[], &[]);
    write_snap(&b, "r", "bbbb2222", &[], &[]);
    write_snap(
        &healthy,
        "r",
        "cccc3333",
        &["f.py"],
        &[("m/a", "m/b", "CALLS")],
    );
    let sa = load_snapshot(&a).unwrap();
    let sb = load_snapshot(&b).unwrap();
    let sh = load_snapshot(&healthy).unwrap();
    let both = summarize(&sa, &sb);
    assert_eq!(
        both.degenerate.as_deref(),
        Some("兩側 snapshot files 皆空（退化快照）——diff 無意義，勿下「無結構變化」結論")
    );
    assert!(both.new_files.is_empty() && both.gone_files.is_empty());
    let before_empty = summarize(&sa, &sh);
    assert_eq!(
        before_empty.degenerate.as_deref(),
        Some("before 側 snapshot files 空（退化）——gone-files 清單不可信")
    );
    assert_eq!(before_empty.new_files, vec!["f.py".to_string()]);
    let after_empty = summarize(&sh, &sb);
    assert_eq!(
        after_empty.degenerate.as_deref(),
        Some("after 側 snapshot files 空（退化）——added-files 清單不可信")
    );
    assert_eq!(after_empty.gone_files, vec!["f.py".to_string()]);
    let healthy_pair = summarize(&sh, &sh);
    assert!(healthy_pair.degenerate.is_none());
    assert_eq!(healthy_pair.diff.added.len(), 0);
}

#[test]
fn degenerate_json_face_warning() {
    // T4 (ported off the retired CLI): the json face carries
    // degenerate_warning — the transport delta_tour's build_tour reads
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    write_snap(&a, "r", "aaaa1111", &[], &[]);
    write_snap(&b, "r", "bbbb2222", &[], &[]);
    let sa = load_snapshot(&a).unwrap();
    let sb = load_snapshot(&b).unwrap();
    let s = summarize(&sa, &sb);
    let j = render_json_value(&sa, &sb, &s, None, None);
    assert_eq!(
        j["degenerate_warning"],
        serde_json::json!(
            "兩側 snapshot files 皆空（退化快照）——diff 無意義，勿下「無結構變化」結論"
        )
    );
}

#[test]
fn cross_face_files_diff_warns() {
    // T10 (ported): old-face sidecar (no files_face) vs all-kinds
    // sidecar — the files diff is cross-face incomparable; module_edges
    // stay comparable; same-face pairs do not warn (SM-5)
    let tmp = tempfile::tempdir().unwrap();
    let mk = |p: &Path, commit: &str, face: Option<&str>| {
        let mut meta = serde_json::Map::new();
        meta.insert("repo".into(), serde_json::json!("r"));
        meta.insert("commit".into(), serde_json::json!(commit));
        if let Some(f) = face {
            meta.insert("files_face".into(), serde_json::json!(f));
        }
        let v = serde_json::json!({
            "_meta": meta,
            "files": ["f.py"],
            "module_edges": [["m/a", "m/b", "CALLS"]],
        });
        std::fs::write(p, serde_json::to_string(&v).unwrap()).unwrap();
    };
    let a = tmp.path().join("a.json"); // old structural-only face
    let b = tmp.path().join("b.json"); // all-kinds face
    let c = tmp.path().join("c.json"); // all-kinds face (control)
    let d = tmp.path().join("d.json"); // different explicit face (R12 arm)
    mk(&a, "aaaa1111", None);
    mk(&b, "bbbb2222", Some("all-kinds"));
    mk(&c, "cccc3333", Some("all-kinds"));
    mk(&d, "dddd4444", Some("structural-only"));
    let sa = load_snapshot(&a).unwrap();
    let sb = load_snapshot(&b).unwrap();
    let sc = load_snapshot(&c).unwrap();
    let sd = load_snapshot(&d).unwrap();
    let s = summarize(&sa, &sb);
    let j = render_json_value(&sa, &sb, &s, None, None);
    assert!(
        j["files_face_warning"]
            .as_str()
            .unwrap()
            .contains("files 面跨版本不可比"),
        "cross-face warning missing: {j}"
    );
    // both faces PRESENT but different values — the first match arm
    // (R12: the None-vs-Some direction alone left it unpinned)
    let s_d = summarize(&sb, &sd);
    let j_d = render_json_value(&sb, &sd, &s_d, None, None);
    let w = j_d["files_face_warning"].as_str().unwrap();
    assert!(w.contains("files 面不同"), "value-mismatch arm: {w}");
    assert!(w.contains("before=all-kinds") && w.contains("after=structural-only"));
    let s2 = summarize(&sb, &sc);
    let j2 = render_json_value(&sb, &sc, &s2, None, None);
    assert!(
        j2.get("files_face_warning").is_none(),
        "same-face pair must not warn: {j2}"
    );
}

#[test]
fn files_only_change_counts_as_changed_module() {
    // (ported off the retired CLI): a files-only change still counts as
    // a changed module in the json face
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    write_snap(&a, "r", "aaaa1111", &["mosaic_alpha/a.py"], &[]);
    write_snap(
        &b,
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
    let sa = load_snapshot(&a).unwrap();
    let sb = load_snapshot(&b).unwrap();
    let profile = load_profile(tmp.path()).unwrap();
    let s = summarize(&sa, &sb);
    let j = render_json_value(&sa, &sb, &s, None, profile.as_ref());
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
