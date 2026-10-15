//! CR-12 — TC-14 heal/stamp double-evaluation elimination (Plan A:
//! `StalenessSnapshot` threading from the pre-query heal check into the
//! per-query serving stamp).
//!
//! Contract under test (card CR-12 ②①/②②/②③):
//! - a fresh default-slot scip_refs query runs the full staleness
//!   evaluation EXACTLY once (the heal check's snapshot is the stamp's
//!   snapshot — evaluation-count evidence via
//!   `engine::staleness_evaluations`);
//! - per-outcome snapshot semantics: Fresh / Healed (post-build
//!   recompute) / HealedByPeer (post-wait recompute) reuse; ServeStale
//!   reuses only when no heal activity touched the slot in this flow
//!   (pre-dispatch churn/lock short-circuits), otherwise recomputes
//!   conservatively; a missing snapshot NEVER reads as fresh;
//! - the stamp-time and post-load binding legs are preserved on the
//!   reused snapshot — a mismatch consumes the one bounded retry (one
//!   full recompute) exactly as before (D11 bounded consistency).
//!
//! Every test in this file serializes on `ENV_LOCK`: the evaluation
//! counter is process-global (parallel tests would interleave counts)
//! and two tests mutate PATH / CODE_REALITY_AUTOHEAL.

mod support;

use code_reality::build::{ensure_fresh, ensure_fresh_check, HealOutcome};
use code_reality::cli::run;
use code_reality::engine::{default_index_path, staleness_evaluations, DeltaUnavailable};
use code_reality::freshness::{serving_stamp_reusing_with_budget, stamp_after_load};
use code_reality::ToolOutput;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    bindir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

/// Python-face lab (deriving pyrefly fake): `mod_a.py` → the
/// `mod_afn1` DEF the queries below resolve.
fn lab_py() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("mod_a.py"), "def fn1():\n    pass\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        bindir,
        roots,
    }
}

impl Lab {
    fn slot(&self) -> PathBuf {
        default_index_path(&self.repo).unwrap()
    }
    fn build(&self) {
        code_reality::build::build_repo(&self.repo, None, &self.roots).expect("build");
    }
    /// Drift the governed corpus (a new untracked module) — the slot's
    /// content identity no longer matches: stale.
    fn drift(&self, rel: &str) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(self.repo.join(rel), "def fn1():\n    return 1\n").unwrap();
    }
    /// scip_refs through the DEFAULT slot (no `--index`): the pre-query
    /// heal hook is live on this path — the path the card optimizes.
    fn scip_default(&self, args: &[&str]) -> ToolOutput {
        let mut argv = vec!["scip_refs".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        argv.push("--repo".to_string());
        argv.push(self.repo.display().to_string());
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run(&refs)
    }
}

/// The `[CR_VERDICT]` line of a query's stdout.
fn verdict_line(out: &ToolOutput) -> &str {
    out.stdout
        .lines()
        .find(|l| l.starts_with("[CR_VERDICT] "))
        .unwrap_or_else(|| panic!("[CR_VERDICT] line present: {}", out.stdout))
}

/// Prepend a directory to PATH for the guard's lifetime (the heal's
/// `build_repo` resolves producers from the real roots = PATH); Drop
/// restores the previous value even when the test panics.
struct PathGuard {
    old: std::ffi::OsString,
}

impl PathGuard {
    fn prepend(dir: &Path) -> Self {
        let old = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![dir.to_path_buf()];
        paths.extend(std::env::split_paths(&old));
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        PathGuard { old }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        std::env::set_var("PATH", &self.old);
    }
}

// ---------- ②① the fresh path: one evaluation, one snapshot ----------

#[test]
fn fresh_default_slot_query_evaluates_staleness_exactly_once() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let before = staleness_evaluations();
    let out = lab.scip_default(&["mod_afn1"]);
    let evals = staleness_evaluations() - before;
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(
        !out.stderr.contains("index healed"),
        "fresh slot: no heal: {}",
        out.stderr
    );
    let line = verdict_line(&out);
    assert!(line.contains("tier=exact exhaustive=true"), "{line}");
    assert_eq!(
        evals, 1,
        "the heal check's snapshot IS the stamp's snapshot — a fresh \
         query pays exactly one full staleness evaluation"
    );
}

#[test]
fn fresh_callers_and_closure_modes_also_evaluate_once() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    for args in [
        vec!["mod_afn1", "--callers"],
        vec!["mod_afn1", "--closure", "--depth", "1"],
    ] {
        let before = staleness_evaluations();
        let out = lab.scip_default(&args);
        let evals = staleness_evaluations() - before;
        assert_eq!(out.exit_code, 0, "{args:?}: {}\n{}", out.stdout, out.stderr);
        let line = verdict_line(&out);
        assert!(
            line.contains("tier=exact exhaustive=true"),
            "{args:?}: {line}"
        );
        assert_eq!(evals, 1, "{args:?}: one evaluation per query");
    }
}

// ---------- ②② Healed: the post-build recompute is the stamp ----------

#[test]
fn healed_query_reuses_the_post_build_snapshot() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.drift("mod_b.py");
    let _path = PathGuard::prepend(lab.bindir.path());
    let before = staleness_evaluations();
    let out = lab.scip_default(&["mod_afn1"]);
    let evals = staleness_evaluations() - before;
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("[OK] index healed（"),
        "the stale slot healed before answering: {}",
        out.stderr
    );
    // the annotation describes the HEALED generation (bound to the
    // actually-serving slot): clean delta, exact tier
    let line = verdict_line(&out);
    assert!(line.contains("tier=exact exhaustive=true"), "{line}");
    assert!(line.contains("delta_changed=0"), "{line}");
    assert!(!line.contains("delta_paths="), "{line}");
    // pre-heal check (1) + run_heal_locked's post-build loop-guard
    // recompute (2) — the stamp reuses the latter, never a third pass
    assert_eq!(
        evals, 2,
        "heal path: the post-build snapshot is the stamp's snapshot"
    );
}

// ---------- ②③ HealedByPeer: the post-wait recompute is the stamp ----------

#[test]
fn peer_healed_query_reuses_the_post_wait_snapshot() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.drift("mod_b.py");
    // a live peer holds the single-flight lock (fresh mtime: not
    // stealable) — the query's heal check sees stale, cannot acquire,
    // and polls for the release
    let lock = lab.slot().parent().unwrap().join(".heal.lock");
    std::fs::write(&lock, format!("{} peer\n", std::process::id())).unwrap();
    let before = staleness_evaluations();
    let waiter = {
        let repo = lab.repo.clone();
        std::thread::spawn(move || {
            let argv = ["scip_refs", "mod_afn1", "--repo"];
            let repo_s = repo.display().to_string();
            let mut full: Vec<&str> = argv.to_vec();
            full.push(&repo_s);
            run(&full)
        })
    };
    // let the waiter run its pre-check and enter the poll loop (the
    // pre-check is milliseconds on a one-file corpus; the lock stays
    // held until we release it, so an early release is impossible)
    std::thread::sleep(std::time::Duration::from_secs(2));
    // the peer converges the slot in-process (no evaluate_staleness
    // inside build_repo) and releases the lock
    lab.build();
    std::fs::remove_file(&lock).unwrap();
    let out = waiter.join().unwrap();
    let evals = staleness_evaluations() - before;
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("同 slot 併發 heal，等待後重用"),
        "HealedByPeer face: {}",
        out.stderr
    );
    let line = verdict_line(&out);
    assert!(line.contains("tier=exact exhaustive=true"), "{line}");
    assert!(line.contains("delta_changed=0"), "{line}");
    // pre-wait check (1) + post-wait recompute (2); the pre-wait
    // snapshot is never the stamp (②③) and no third pass runs
    assert_eq!(
        evals, 2,
        "peer-heal path: the post-wait snapshot is the stamp's snapshot"
    );
}

// ---------- ②② ServeStale: pre-dispatch short-circuit reuses ----------

#[test]
fn churn_cooldown_serve_stale_reuses_the_pre_dispatch_snapshot() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.drift("mod_b.py");
    // an armed churn marker (fresh mtime) short-circuits the heal
    // BEFORE any dispatch: the slot is untouched by this flow, so the
    // check's snapshot is exactly what a recompute would answer
    std::fs::write(lab.slot().parent().unwrap().join(".heal-churn"), b"").unwrap();
    let before = staleness_evaluations();
    let out = lab.scip_default(&["mod_afn1"]);
    let evals = staleness_evaluations() - before;
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stderr.contains("cooldown"), "{}", out.stderr);
    // the served body is the stale slot and the annotation says so
    let line = verdict_line(&out);
    assert!(line.contains("tier=possibly-stale"), "{line}");
    assert!(line.contains("mod_b.py"), "{line}");
    assert_eq!(evals, 1, "no heal activity touched the slot: reuse");
}

// ---------- ②② ServeStale after a failed heal: conservative recompute ----------

#[test]
fn failed_heal_serve_stale_recomputes_the_stamp_conservatively() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.drift("mod_b.py");
    // swap the producer for one that fails on run (--version answers)
    support::fake_bin(
        lab.bindir.path(),
        "pyrefly-index",
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '0.4.0-fixture'; exit 0; fi\nexit 1\n",
    );
    let _path = PathGuard::prepend(lab.bindir.path());
    let before = staleness_evaluations();
    let out = lab.scip_default(&["mod_afn1"]);
    let evals = staleness_evaluations() - before;
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("本次查詢以現存索引作答"),
        "serve-stale face: {}",
        out.stderr
    );
    let line = verdict_line(&out);
    assert!(line.contains("tier=possibly-stale"), "{line}");
    // pre-check (1) + the post-attempt recompute inside the heal (2) +
    // the stamp's own conservative recompute (3): a ServeStale produced
    // after heal activity never lends its snapshot to the stamp
    assert_eq!(evals, 3, "post-attempt ServeStale: the stamp recomputes");
}

// ---------- None ≠ fresh: no snapshot still yields a real verdict ----------

#[test]
fn autoheal_off_computes_the_stamp_without_a_heal_snapshot() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    std::env::set_var("CODE_REALITY_AUTOHEAL", "off");
    let before = staleness_evaluations();
    let out = lab.scip_default(&["mod_afn1"]);
    let evals = staleness_evaluations() - before;
    std::env::remove_var("CODE_REALITY_AUTOHEAL");
    assert_eq!(
        out.exit_code, 0,
        "stdout={}\nstderr={}",
        out.stdout, out.stderr
    );
    let line = verdict_line(&out);
    assert!(line.contains("tier=exact exhaustive=true"), "{line}");
    assert_eq!(evals, 1, "no heal check ran: the stamp computes its own");
}

// ---------- ②① the load face: HealCheck per outcome ----------

/// The slot meta's `index_sha256` (the binding the stamp verifies).
fn bound_index_sha(lab: &Lab) -> String {
    let (meta, _) = code_reality::engine::load_meta(&lab.slot());
    meta.unwrap()["index_sha256"].as_str().unwrap().to_string()
}

/// Overwrite the slot meta's `index_sha256` — the stamp-time binding
/// check now sees a meta that does not bind the hashed bytes.
fn tamper_bound_index_sha(lab: &Lab, value: &str) {
    let mp = code_reality::engine::meta_path(&lab.slot());
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mp).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!(value);
    std::fs::write(
        &mp,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
}

#[test]
fn ensure_fresh_check_lends_the_fresh_snapshot_and_nothing_for_a_missing_slot() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    // no slot yet: Fresh (the caller's missing-index FAIL stays
    // authoritative) with NO snapshot — absence is never a verdict
    let check = ensure_fresh_check(&lab.repo, &lab.roots).unwrap();
    assert_eq!(check.outcome, HealOutcome::Fresh);
    assert_eq!(check.reusable_snapshot, None, "{check:?}");
    lab.build();
    let before = staleness_evaluations();
    let check = ensure_fresh_check(&lab.repo, &lab.roots).unwrap();
    assert_eq!(staleness_evaluations() - before, 1);
    assert_eq!(check.outcome, HealOutcome::Fresh);
    let snap = check
        .reusable_snapshot
        .expect("the fresh check lends its snapshot");
    assert!(!snap.needs_rebuild(), "{snap:?}");
    assert_eq!(
        snap.serving_artifact_hash.as_deref(),
        Some(bound_index_sha(&lab).as_str()),
        "the lent snapshot hashed the bytes the meta binds"
    );
    // the outcome-only projection is the same decision
    assert_eq!(
        ensure_fresh(&lab.repo, &lab.roots).unwrap(),
        HealOutcome::Fresh
    );
}

// ---------- ②① the stamp face: binding legs + budget on a lent snapshot ----------

#[test]
fn serving_stamp_reusing_binds_or_consumes_the_one_retry() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let slot = lab.slot();
    let budget = Duration::from_secs(5);
    let snap = ensure_fresh_check(&lab.repo, &lab.roots)
        .unwrap()
        .reusable_snapshot
        .expect("fresh snapshot");

    // (a) bound seed: zero evaluations, the seed IS the stamp
    let before = staleness_evaluations();
    let stamp =
        serving_stamp_reusing_with_budget(Some(&lab.repo), &slot, Some(snap.clone()), budget);
    assert_eq!(staleness_evaluations() - before, 0, "seed reused as-is");
    assert_eq!(stamp.snapshot.as_ref(), Some(&snap));
    assert_eq!(stamp.reason, None);

    // (b) no seed: the unseeded path evaluates once (serving_stamp parity)
    let before = staleness_evaluations();
    let stamp = serving_stamp_reusing_with_budget(Some(&lab.repo), &slot, None, budget);
    assert_eq!(staleness_evaluations() - before, 1);
    assert_eq!(
        stamp.snapshot.as_ref(),
        Some(&snap),
        "same slot, same verdict"
    );

    // (c) a seed whose hash no longer binds the meta consumes the one
    // retry: exactly one full recompute, which binds the real slot
    let mut forged = snap.clone();
    forged.serving_artifact_hash = Some("deadbeef".to_string());
    let before = staleness_evaluations();
    let stamp = serving_stamp_reusing_with_budget(Some(&lab.repo), &slot, Some(forged), budget);
    assert_eq!(staleness_evaluations() - before, 1, "one bounded retry");
    assert_eq!(stamp.reason, None, "{stamp:?}");
    assert_eq!(
        stamp
            .snapshot
            .as_ref()
            .and_then(|s| s.serving_artifact_hash.as_deref()),
        Some(bound_index_sha(&lab).as_str()),
        "the retry re-bound the actually-serving bytes"
    );

    // (d) budget gate on the reuse leg: the stamp's wall-clock ceiling
    // holds regardless of how the snapshot was obtained
    let before = staleness_evaluations();
    let stamp = serving_stamp_reusing_with_budget(
        Some(&lab.repo),
        &slot,
        Some(snap.clone()),
        Duration::ZERO,
    );
    assert_eq!(staleness_evaluations() - before, 0);
    assert_eq!(stamp.snapshot, None);
    assert_eq!(stamp.reason, Some(DeltaUnavailable::BudgetExceeded));

    // (e) seed mismatch AND the retry mismatches (meta no longer binds
    // the slot bytes): one recompute, then `indeterminate` — never a
    // third pass, never an optimistic answer
    tamper_bound_index_sha(
        &lab,
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    let before = staleness_evaluations();
    let stamp = serving_stamp_reusing_with_budget(Some(&lab.repo), &slot, Some(snap), budget);
    assert_eq!(
        staleness_evaluations() - before,
        1,
        "bounded: seed + one retry"
    );
    assert_eq!(stamp.snapshot, None);
    assert_eq!(
        stamp.reason,
        Some(DeltaUnavailable::ManifestBindingAuditFailed),
        "{stamp:?}"
    );
}

#[test]
fn stamp_after_load_still_degrades_a_lent_snapshot_on_a_post_load_swap() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let slot = lab.slot();
    let snap = ensure_fresh_check(&lab.repo, &lab.roots)
        .unwrap()
        .reusable_snapshot
        .expect("fresh snapshot");
    let stamp = serving_stamp_reusing_with_budget(
        Some(&lab.repo),
        &slot,
        Some(snap),
        Duration::from_secs(5),
    );
    assert!(stamp.snapshot.is_some());
    // intact: the post-load leg passes the stamp through
    let passed = stamp_after_load(&stamp, &slot);
    assert_eq!(passed.snapshot, stamp.snapshot);
    assert_eq!(passed.reason, None);
    // a slot swap inside the stamp→load window (meta re-bound to other
    // bytes): the post-load leg degrades outright, no retry
    tamper_bound_index_sha(
        &lab,
        "1111111111111111111111111111111111111111111111111111111111111111",
    );
    let before = staleness_evaluations();
    let after = stamp_after_load(&stamp, &slot);
    assert_eq!(
        staleness_evaluations() - before,
        0,
        "the post-load leg never recomputes"
    );
    assert_eq!(after.snapshot, None);
    assert_eq!(
        after.reason,
        Some(DeltaUnavailable::ManifestBindingAuditFailed)
    );
}
