//! R5-S2 runtime_edges tests — mirrors of test_runtime_edges case
//! families (nesting extraction, pid/tid grouping, repo-only filter,
//! aggregation stats, BooleanOptionalAction pair).

use code_reality::runtime_edges::{aggregate, event_path, extract_edges, qualname, repo_only_filter, run};
use serde_json::json;
use std::path::{Path, PathBuf};

fn ev(pid: i64, tid: i64, ts: i64, dur: i64, name: impl Into<String>) -> serde_json::Value {
    json!({"cat": "fee", "ph": "X", "pid": pid, "tid": tid, "ts": ts, "dur": dur, "name": name.into()})
}

#[test]
fn name_parsing() {
    assert_eq!(qualname("fn (pkg/a.py:12)"), "fn");
    assert_eq!(qualname("plain"), "plain");
    assert_eq!(event_path("fn (pkg/a.py:12)").as_deref(), Some("pkg/a.py"));
    assert!(event_path("genexpr noise").is_none());
}

#[test]
fn nesting_extraction_nearest_ancestor() {
    let events = vec![
        ev(1, 10, 0, 100, "outer (a.py:1)"),
        ev(1, 10, 10, 30, "mid (a.py:2)"),
        ev(1, 10, 15, 5, "inner (a.py:3)"),
        ev(1, 10, 50, 10, "late (a.py:4)"), // after mid ends — parent = outer
    ];
    let edges = extract_edges(&events).unwrap();
    let pairs: Vec<(&str, &str)> = edges.iter().map(|(c, f, _)| (c.as_str(), f.as_str())).collect();
    assert_eq!(
        pairs,
        vec![
            ("outer (a.py:1)", "mid (a.py:2)"),
            ("mid (a.py:2)", "inner (a.py:3)"),
            ("outer (a.py:1)", "late (a.py:4)"),
        ]
    );
}

#[test]
fn pid_tid_grouping_prevents_cross_process_edges() {
    // same tid in two pids: no fabricated cross-process edge
    let events = vec![
        ev(1, 10, 0, 50, "p1fn (a.py:1)"),
        ev(2, 10, 10, 5, "p2fn (b.py:1)"), // overlaps p1's window but other process
    ];
    let edges = extract_edges(&events).unwrap();
    assert!(edges.is_empty(), "{edges:?}");
    // sibling intervals (end == next ts) do not nest
    let siblings = vec![
        ev(1, 10, 0, 10, "s1 (a.py:1)"),
        ev(1, 10, 10, 10, "s2 (a.py:2)"),
    ];
    assert!(extract_edges(&siblings).unwrap().is_empty());
}

#[test]
fn missing_tid_or_dur_is_loud() {
    let bad = vec![json!({"cat": "fee", "ph": "X", "name": "n", "ts": 1})];
    let err = extract_edges(&bad).unwrap_err();
    assert!(err.contains("缺 tid/dur"), "{err}");
    let none = vec![json!({"cat": "other", "ph": "X", "name": "n"})];
    let err2 = extract_edges(&none).unwrap_err();
    assert!(err2.contains("無函式事件"), "{err2}");
}

#[test]
fn repo_only_keeps_one_endpoint_inside() {
    let repo = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(repo.path()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let edges = vec![
        (format!("a ({}/x.py:1)", abs("")), format!("b ({}/y.py:1)", abs("")), 1.0),
        ("noise genexpr".to_string(), format!("c ({}/z.py:1)", abs("")), 1.0),
        (format!("d ({}/w.py:1)", abs("")), "other (/elsewhere/q.py:1)".to_string(), 1.0),
        ("g (/elsewhere/e.py:1)".to_string(), "h (/elsewhere/f.py:1)".to_string(), 1.0),
    ];
    let kept = repo_only_filter(&edges, &repo);
    assert_eq!(kept.len(), 3); // rows 1-3 (row 2's callee is in-repo; row 4 both outside)
    let _ = Path::new(".");
}

#[test]
fn aggregate_stats_count_desc_and_nearest_rank_p95() {
    // dur µs → ms rounding; p50 even-n mean; p95 ceil rank
    let mk = |n: usize| -> Vec<(String, String, f64)> {
        (0..n)
            .map(|i| ("caller (a.py:1)".to_string(), "callee (b.py:1)".to_string(), (i + 1) as f64 * 100.0))
            .collect()
    };
    let rows = aggregate(&mk(20));
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r["count"], json!(20));
    assert_eq!(r["p50_ms"], json!(1.05)); // median of 100..2000µs (even-n mean) = 1050µs
    assert_eq!(r["p95_ms"], json!(1.9)); // ceil(0.95*20)=19th sorted = 1900µs
    // ordering: higher count first
    let mut mixed = mk(5);
    mixed.extend(vec![("x (a.py:1)".to_string(), "y (b.py:1)".to_string(), 50.0); 7]);
    let rows2 = aggregate(&mixed);
    assert_eq!(rows2[0]["count"], json!(7));
    assert_eq!(rows2[1]["count"], json!(5));
}

#[test]
fn cli_faces_and_boolean_optional_action() {
    let out = run(&["runtime_edges", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("--repo-only, --no-repo-only"));
    let out = run(&["runtime_edges"]);
    assert_eq!(out.exit_code, 2);
    let out = run(&["runtime_edges", "t.json", "--top", "abc"]);
    assert_eq!(out.exit_code, 2);
}

#[test]
fn cli_end_to_end_output_faces() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let abs = |rel: &str| repo.join(rel).to_string_lossy().into_owned();
    let trace = repo.join("trace.json");
    let events = vec![
        json!({"cat": "meta", "ph": "M", "pid": 1, "tid": 10, "name": "m", "ts": 0, "dur": 0}),
        ev(1, 10, 0, 100, format!("outer ({}/pkg/a.py:1)", abs(""))),
        ev(1, 10, 10, 30, format!("inner ({}/pkg/a.py:2)", abs(""))),
    ];
    std::fs::write(&trace, serde_json::to_string(&json!({"traceEvents": events})).unwrap()).unwrap();
    // make_meta runs `git rev-parse HEAD` with check=True semantics — the
    // fixture needs at least one commit (crash family otherwise)
    let g = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
    };
    g(&["init", "-q"]);
    g(&["add", "."]);
    g(&["commit", "-qm", "init"]);
    let out = run(&[
        "runtime_edges",
        &trace.to_string_lossy(),
        "--repo-root",
        &repo.to_string_lossy(),
        "-o",
        &repo.join("edges.json").to_string_lossy(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("[OK] 1 edges from 3 events -> "), "{}", out.stdout);
    assert!(out.stdout.contains("top: outer -> inner x1 p50=0.03ms"), "{}", out.stdout);
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo.join("edges.json")).unwrap()).unwrap();
    let meta_keys: Vec<&str> = written["_meta"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
    assert_eq!(
        meta_keys,
        vec!["repo", "commit", "created_at", "tool", "trace", "repo_only", "pids", "total_events", "total_edges"]
    );
    assert_eq!(written["edges"][0]["count"], json!(1));

    // --no-repo-only keeps the noise edges too (BooleanOptionalAction off)
    let out2 = run(&[
        "runtime_edges",
        &trace.to_string_lossy(),
        "--no-repo-only",
        "--repo-root",
        &repo.to_string_lossy(),
        "-o",
        &repo.join("edges2.json").to_string_lossy(),
    ]);
    assert!(out2.stdout.contains("[OK] 1 edges from 3 events"), "{}", out2.stdout);
    let _ = PathBuf::new();
}
