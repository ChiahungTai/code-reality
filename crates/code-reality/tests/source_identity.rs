//! Source identity integration tests (EP 09-23-source-identity).
//!
//! NOTE the naming neighbors: `tests/freshness.rs` is the BINARY version
//! face pin (ep-binary-freshness-face) and `cr-freshness` is the binary
//! freshness leaf crate — both are the binary axis, NOT this index-axis
//! suite. This file owns the index-axis source-identity face (the
//! `freshness` subcommand lands here in S4).
//!
//! Fake producers DERIVE through the shared fixture producer
//! (`tests/support`), so every build stamps a real identity pair.

mod support;

use code_reality::build::build_repo;
use code_reality::engine::meta_path;
use code_reality::identity::{
    cache_path_for_slot, compute_identity, IdentityCache, IdentityCachePolicy,
};
use code_reality::language::LanguageFace;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use support::{mkrepo, slot_of};
struct Lab {
    repo: std::path::PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<std::path::PathBuf>,
}

fn lab(files: &[(&str, &str)]) -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, files);
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

fn stamped_meta(repo: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(meta_path(&slot_of(repo))).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Clean no-cache identity over the repo's current disk corpus (all faces).
fn clean_identity(repo: &Path) -> String {
    let walk = code_reality::engine::walk_sources(repo).unwrap();
    let faces: BTreeSet<LanguageFace> = walk
        .newest_by_face
        .keys()
        .copied()
        .collect();
    let records: BTreeMap<String, code_reality::identity::SourceRecord> = walk.records();
    compute_identity(
        &code_reality::engine::resolve_repo(repo),
        &records,
        &faces,
        IdentityCachePolicy::ReadOnly,
        &mut IdentityCache::disabled(),
    )
    .unwrap()
    .value
}

/// TC-8a: a consistent build stamps the identity pair (value + algo).
#[test]
fn tc8_stamp_writes_identity_keys_on_consistent_pair() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let meta = stamped_meta(&l.repo);
    let identity = meta["source_identity"].as_str().expect("identity stamped");
    assert_eq!(identity.len(), 64, "sha256 hex: {identity}");
    assert!(
        identity.chars().all(|c| c.is_ascii_hexdigit()),
        "{identity}"
    );
    assert_eq!(meta["identity_algo"], "sha256-v1");
    assert_eq!(identity, clean_identity(&l.repo), "stamped ≡ clean recompute");
}

/// TC-8b (D13): the stamp face is immune to a poisoned query-side
/// cache — a matching-stat entry with a WRONG hash can never reach the
/// stamped identity, because the stamp path recomputes from actual
/// bytes (Full bypasses the gate).
#[test]
fn tc8_stamp_is_immune_to_cache_pollution() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let clean = clean_identity(&l.repo);
    let cache_path = cache_path_for_slot(&slot_of(&l.repo));

    // Warm the cache (true stat entries), then poison one hash in place
    // — size/mtime stay valid, so a gate-trusting stamp would answer
    // with the poisoned byte.
    let mut cache = IdentityCache::load(cache_path.clone(), &l.repo);
    compute_identity(
        &code_reality::engine::resolve_repo(&l.repo),
        &code_reality::engine::walk_sources(&l.repo).unwrap().records(),
        &BTreeSet::from([LanguageFace::Python, LanguageFace::JavaScript]),
        IdentityCachePolicy::WriteBack,
        &mut cache,
    )
    .unwrap();
    let mut payload: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    let entries = payload["entries"].as_object_mut().unwrap();
    assert!(entries.len() >= 2, "warm cache covers the corpus");
    let victim = entries.keys().next().unwrap().clone();
    entries[&victim]["hash"] = serde_json::json!(format!("{:0>64}", "poisoned"));
    std::fs::write(
        &cache_path,
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .unwrap();

    // Rebuild: the stamp recomputes Full — the poisoned entry is ignored
    // AND overwritten with the true value.
    build_repo(&l.repo, None, &l.roots).expect("rebuild");
    let meta = stamped_meta(&l.repo);
    assert_eq!(
        meta["source_identity"].as_str().unwrap(),
        clean,
        "stamped identity must equal the clean full recompute"
    );
    let healed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    assert_ne!(
        healed["entries"][&victim]["hash"],
        serde_json::json!(format!("{:0>64}", "poisoned")),
        "the rebuild purges the poisoned entry in scope"
    );
}

/// TC-8c: a docs-only head-sync restamp keeps the identity VALUE
/// unchanged (identity keys flow through the same disk==docs gate; the
/// docs file is not corpus, so the identity body is untouched).
#[test]
fn tc8_docs_only_head_sync_keeps_identity_value() {
    let l = lab(&[("app.py", "x = 1\n"), ("docs.md", "v1\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let before = stamped_meta(&l.repo)["source_identity"]
        .as_str()
        .unwrap()
        .to_string();

    std::fs::write(l.repo.join("docs.md"), "v2\n").unwrap();
    code_reality::engine::stamp_meta_core(&l.repo, &slot_of(&l.repo), &l.roots, None, None)
        .expect("head-sync stamp");
    let meta_after = stamped_meta(&l.repo);
    let after = meta_after["source_identity"].as_str().unwrap();
    assert_eq!(before, after, "docs-only restamp must not move identity");
}

/// TC-8d: on a disk/index mismatch the restamp preserves the prior
/// identity keys VERBATIM (anti-laundering extension — the drift stays
/// visible for the identity axis exactly like the fingerprint axis).
#[test]
fn tc8_preserve_extends_to_identity_keys() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let stamped = stamped_meta(&l.repo)["source_identity"]
        .as_str()
        .unwrap()
        .to_string();

    // delete a corpus doc → disk != docs → preserve branch
    std::fs::remove_file(l.repo.join("src/a.mjs")).unwrap();
    code_reality::engine::stamp_meta_core(&l.repo, &slot_of(&l.repo), &l.roots, None, None)
        .expect("stamp on mismatch");
    let meta = stamped_meta(&l.repo);
    assert_eq!(
        meta["source_identity"].as_str().unwrap(),
        stamped,
        "prior identity value preserved verbatim"
    );
    assert_eq!(meta["identity_algo"], "sha256-v1");
}

// ---------- S3: the identity-authoritative staleness decision ----------

use code_reality::build::{ensure_fresh, HealOutcome};
use code_reality::engine::evaluate_staleness;

const WB: IdentityCachePolicy = IdentityCachePolicy::WriteBack;

/// Restore `p`'s mtime to `t` (the rsync -a / tar-restore simulation).
fn keep_mtime(p: &Path, t: std::time::SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// TC-4: a dirty working tree (post-build edit) → indexed ≠ current →
/// identity drift fires.
#[test]
fn tc4_dirty_wt_edit_fires_identity_drift() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let snap0 = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(snap0.identity_drift, Some(false), "{snap0:?}");
    assert!(!snap0.needs_rebuild());

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(snap.identity_drift, Some(true), "dirty WT detected: {snap:?}");
    assert!(snap.needs_rebuild(), "{snap:?}");
    assert_eq!(
        snap.current_identity.as_deref(),
        Some(clean_identity(&l.repo).as_str()),
        "current-side ≡ clean recompute"
    );
}

/// TC-5 (SM-3, the headline flip): content swapped UNDER a preserved
/// mtime (rsync -a / tar restore) — mtime is silent, identity catches
/// it, and the heal chain walks REBUILD (not the false-stale serve).
///
/// Two forms, two detection routes:
/// - size-same: tar-restore shape — ustar records seconds, so the
///   restored mtime differs in nanos from the cache-gate tuple → gate
///   miss → re-hash → the content_hash route detects. (The EXACT-nanos
///   restore of a same-size swap is the EP-disclosed precise-forgery
///   residual: accepted, D13-bounded — see the SKILL.md disclosure; it
///   is deliberately NOT constructed here.)
/// - size-diff: full-precision restore — the size route detects
///   regardless of mtime precision.
#[test]
fn tc5_mtime_preserved_swap_rebuilds_not_serves() {
    let lab_mtime = |repo: &Path| repo.join("app.py").metadata().unwrap().modified().unwrap();
    let restore_secs_only = |p: &Path, t: std::time::SystemTime| {
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        keep_mtime(p, std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
    };

    // size-same form: content_hash route (tar-restore granularity)
    let l = lab(&[("app.py", "aaaa"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let old_mtime = lab_mtime(&l.repo);
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("app.py"), "bbbb").unwrap();
    restore_secs_only(&l.repo.join("app.py"), old_mtime);

    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert!(
        !snap.source_newer,
        "mtime preserved — the legacy trigger is silent: {snap:?}"
    );
    assert_eq!(
        snap.identity_drift,
        Some(true),
        "identity sees the swap (aaaa→bbbb): {snap:?}"
    );
    assert!(snap.needs_rebuild());
    // heal chain: a REAL rebuild, never the false-stale serve
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(
        matches!(out, HealOutcome::Healed { .. }),
        "swap must rebuild, not serve: {out:?}"
    );
    let snap2 = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert!(!snap2.needs_rebuild(), "converged: {snap2:?}");

    // size-diff form: size route (full-precision restore)
    let l = lab(&[("app.py", "aaaa"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let old_mtime = lab_mtime(&l.repo);
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("app.py"), "aa").unwrap();
    keep_mtime(&l.repo.join("app.py"), old_mtime);
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(
        snap.identity_drift,
        Some(true),
        "size change detects despite exact mtime restore: {snap:?}"
    );
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
}

/// TC-9 (R22 scope mirror): an explicit python-only slot is not staled
/// by a .mjs edit — the identity eval scope is the pinned face.
#[test]
fn tc9_explicit_pinned_face_scope_ignores_other_faces() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, Some(code_reality::language::ProducerFamily::Python), &l.roots)
        .expect("py-only build");
    let snap0 = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(
        snap0.eval_faces,
        std::collections::BTreeSet::from([LanguageFace::Python]),
        "explicit stays pinned: {snap0:?}"
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(
        snap.identity_drift,
        Some(false),
        "out-of-scope edit invisible: {snap:?}"
    );
    assert!(!snap.needs_rebuild(), "{snap:?}");
}

/// TC-13 (SM-14, judge R1): an identity-stamped slot with a lagging
/// graph.db still forces the heal — the torn-plane guard is NOT
/// short-circuited by identity_drift == Some(false).
#[test]
fn tc13_torn_plane_guard_survives_identity_mode() {
    let l = lab(&[("app.py", "x = 1\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let snap0 = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(
        snap0.identity_drift,
        Some(false),
        "premise: the slot IS identity-stamped"
    );
    // torn state: bump the slot mtime past the graph
    let bytes = std::fs::read(slot_of(&l.repo)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(slot_of(&l.repo), &bytes).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(snap.identity_drift, Some(false), "content unchanged");
    assert!(snap.graph_lags, "torn plane detected");
    assert!(
        snap.needs_rebuild(),
        "graph_lags must be unconditional in identity mode: {snap:?}"
    );
    // C2 (5.3 judge): the torn-plane reason survives to the CLI face —
    // the verdict JSON must name it, or a mapping typo ships unnoticed.
    let vout = freshness(&l.repo, true);
    assert_eq!(vout.exit_code, 1, "torn plane is stale: {vout:?}");
    let v = verdict_json(&vout);
    assert_eq!(
        v["stale_reasons"],
        serde_json::json!(["torn-plane"]),
        "identity matched + content unchanged ⇒ torn-plane is the only fatal reason"
    );
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
}

/// C1 (5.3 judge): a hand-forged auto-mode meta carrying the identity
/// pair but no `source_faces` must degrade to LEGACY (no identity
/// compare) — the identity baseline is the stamped face set mirrored
/// to eval scope (R22); a detected-scope fallback would silently
/// compute a scope that was never stamped. Tool-written meta always
/// carries all keys atomically, so this shape is out-of-band only.
#[test]
fn c1_partial_identity_meta_degrades_to_legacy() {
    let l = lab(&[("app.py", "x = 1\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let slot = slot_of(&l.repo);
    std::fs::write(
        code_reality::engine::meta_path(&slot),
        r#"{"repo": "/x", "head": "h", "stamped_at": "2026-01-01T00:00:00+00:00", "tool": "t", "producer": "p", "selection": "auto", "source_identity": "deadbeef", "identity_algo": "sha256-v1"}"#,
    )
    .unwrap();
    let snap = evaluate_staleness(&l.repo, &slot, WB).unwrap();
    assert_eq!(
        snap.identity_drift,
        None,
        "partial triple (no source_faces) ⇒ legacy, never detected-scope identity"
    );
}

/// TC-15 (SM-13): stash push → stale; stash pop (content restored,
/// mtime moved) → converges FRESH — the idempotence the git-machinery
/// design could not deliver (D1).
#[test]
fn tc15_stash_roundtrip_converges_fresh() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let original = "export const a = 1;\n".to_string();
    let slot = slot_of(&l.repo);

    // stash push: content leaves the tree
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("src/a.mjs"), "wip\n").unwrap();
    let snap_stale = evaluate_staleness(&l.repo, &slot, WB).unwrap();
    assert_eq!(snap_stale.identity_drift, Some(true), "{snap_stale:?}");

    // stash pop: content returns (new mtime — irrelevant to identity)
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("src/a.mjs"), original).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot, WB).unwrap();
    assert_eq!(
        snap.identity_drift,
        Some(false),
        "restored content ⇒ restored identity: {snap:?}"
    );
    assert!(!snap.needs_rebuild());
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert_eq!(out, HealOutcome::Fresh, "pop converges without a rebuild");
}

/// TC-16 (SM-16, judge R14): a profile change that excludes nothing is
/// FRESH in identity mode — corpus_policy_drift still REPORTS the
/// policy movement, but it is no longer a fatal signal.
#[test]
fn tc16_noop_exclude_change_is_fresh_with_degraded_signal() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    // the pattern matches nothing — zero corpus effect
    std::fs::write(l.repo.join(".code-reality.toml"), "exclude = [\"nothing-here/\"]\n")
        .unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), WB).unwrap();
    assert_eq!(
        snap.corpus_policy_drift,
        Some(true),
        "the fingerprint still reports the policy movement: {snap:?}"
    );
    assert_eq!(snap.identity_drift, Some(false), "corpus untouched");
    assert!(
        !snap.needs_rebuild(),
        "policy drift is not fatal in identity mode: {snap:?}"
    );
}

/// D8 legacy regression at the identity level: a meta without identity
/// keys keeps the baseline semantics (zero identity computation, no
/// hash cost) — same shape as s4_legacy_meta_without_keys.
#[test]
fn legacy_meta_keeps_baseline_semantics_and_zero_identity() {
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let slot = slot_of(&l.repo);
    std::fs::write(
        code_reality::engine::meta_path(&slot),
        r#"{"repo": "/x", "head": "h", "stamped_at": "2026-01-01T00:00:00+00:00", "tool": "t", "producer": "p"}"#,
    )
    .unwrap();
    let snap = evaluate_staleness(&l.repo, &slot, WB).unwrap();
    assert_eq!(snap.identity_drift, None, "legacy: no identity compare");
    assert_eq!(snap.stamped_identity, None);
    assert_eq!(snap.current_identity, None);
    assert!(!snap.needs_rebuild(), "baseline mtime semantics: {snap:?}");
}

// ---------- S4: the consumer-facing freshness verdict face ----------

use code_reality::freshness::freshness;

/// Parse the verdict JSON out of stdout (the whole stdout is the doc).
fn verdict_json(out: &code_reality::ToolOutput) -> serde_json::Value {
    serde_json::from_str(out.stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not the verdict JSON ({e}): {:?}", out.stdout))
}

/// TC-7 (amended oracle): the FOUR JSON-producing states are pinned
/// key-by-key, value-by-value; the no-slot state is exit 2 + stderr
/// guidance (empty-terminal note included) and produces NO JSON.
#[test]
fn tc7_freshness_json_contract_four_states() {
    // ---- state 1: fresh ----
    let l = lab(&[("app.py", "x = 1\n"), ("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let out = freshness(&l.repo, true);
    assert_eq!(out.exit_code, 0, "fresh exits 0: {out:?}");
    let v = verdict_json(&out);
    let expected_keys: Vec<&str> = vec![
        "repo",
        "slot",
        "fresh",
        "stale_reasons",
        "head_drift",
        "faces",
        "indexed_source_identity",
        "current_source_identity",
        "identity_algo",
        "serves",
    ];
    assert_eq!(
        v.as_object().unwrap().keys().collect::<Vec<_>>(),
        expected_keys,
        "exact key set and order"
    );
    assert_eq!(v["fresh"], serde_json::json!(true));
    assert_eq!(v["stale_reasons"], serde_json::json!([]));
    assert_eq!(v["head_drift"], serde_json::json!(false));
    assert_eq!(
        v["faces"],
        serde_json::json!(["python", "javascript"]),
        "eval scope faces, BTreeSet order (enum variant order)"
    );
    let indexed = v["indexed_source_identity"].as_str().unwrap();
    let current = v["current_source_identity"].as_str().unwrap();
    assert_eq!(indexed.len(), 64);
    assert_eq!(indexed, current, "fresh: indexed == current");
    assert_eq!(v["identity_algo"], serde_json::json!("sha256-v1"));
    assert_eq!(v["serves"], serde_json::json!("current-tree"));
    assert_eq!(
        v["repo"], 
        serde_json::json!(code_reality::engine::resolve_repo(&l.repo).display().to_string())
    );
    assert_eq!(
        v["slot"],
        serde_json::json!(code_reality::engine::resolve_repo(&l.repo)
            .join(".code-reality/scip/index.scip")
            .display()
            .to_string()),
        "the slot face is the resolved repo's slot"
    );

    // ---- state 2: stale (dirty WT) ----
    let ajs_mtime = l.repo.join("src/a.mjs").metadata().unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let out = freshness(&l.repo, true);
    assert_eq!(out.exit_code, 1, "stale exits 1 (verdict face): {out:?}");
    assert!(!out.stdout.is_empty(), "stale still answers on stdout");
    let v = verdict_json(&out);
    assert_eq!(v["fresh"], serde_json::json!(false));
    assert_eq!(v["stale_reasons"], serde_json::json!(["content-drift"]));
    assert_eq!(v["serves"], serde_json::json!("committed-baseline"));
    assert_ne!(
        v["indexed_source_identity"], v["current_source_identity"],
        "stale: the pair diverges"
    );
    assert!(v["current_source_identity"].is_string());
    assert_eq!(v["identity_algo"], serde_json::json!("sha256-v1"));

    // ---- state 4: legacy (meta without identity keys) ----
    // The corpus edit from state 2 moved mtimes, so this is first the
    // legacy-STALE half of the contract (mtime fatal only in legacy
    // mode, reporting as legacy-signals), then the legacy-FRESH half.
    let slot = slot_of(&l.repo);
    std::fs::write(
        code_reality::engine::meta_path(&slot),
        r#"{"repo": "/x", "head": "h", "stamped_at": "2026-01-01T00:00:00+00:00", "tool": "t", "producer": "p"}"#,
    )
    .unwrap();
    let out = freshness(&l.repo, true);
    assert_eq!(out.exit_code, 1, "legacy stale (mtime newer): {out:?}");
    let v = verdict_json(&out);
    assert_eq!(
        v["stale_reasons"],
        serde_json::json!(["legacy-signals"]),
        "mtime signal is fatal only in legacy mode and reports as legacy-signals"
    );
    assert_eq!(v["serves"], serde_json::json!("legacy-signals"));
    assert!(
        v["indexed_source_identity"].is_null()
            && v["current_source_identity"].is_null(),
        "legacy: current is NOT computed — no comparable face"
    );
    assert_eq!(v["identity_algo"], serde_json::json!("sha256-v1"), "tool algo id, not a computation claim");

    // legacy-FRESH half: restore the corpus file's mtime — the CONTENT
    // stays changed (identity mode called this stale above), but the
    // legacy axis cannot see it: the mtime-only blind spot this EP
    // exists to close, pinned from the legacy side (D8 boundary).
    keep_mtime(&l.repo.join("src/a.mjs"), ajs_mtime);
    let out = freshness(&l.repo, true);
    assert_eq!(out.exit_code, 0, "legacy fresh: {out:?}");
    let v = verdict_json(&out);
    assert_eq!(v["fresh"], serde_json::json!(true));
    assert_eq!(v["stale_reasons"], serde_json::json!([]));
    assert_eq!(v["serves"], serde_json::json!("legacy-signals"));

    // ---- state 5: fresh + head_drift ----
    let l2 = lab(&[("app.py", "x = 1\n"), ("docs.md", "v1\n")]);
    build_repo(&l2.repo, None, &l2.roots).expect("build");
    std::fs::write(l2.repo.join("docs.md"), "v2\n").unwrap();
    for args in [
        vec!["add", "-A"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "docs"],
    ] {
        let st = std::process::Command::new("git")
            .args(&args)
            .current_dir(&l2.repo)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    }
    let out = freshness(&l2.repo, true);
    assert_eq!(out.exit_code, 0, "head drift does not kill fresh (D14): {out:?}");
    let v = verdict_json(&out);
    assert_eq!(v["fresh"], serde_json::json!(true));
    assert_eq!(v["head_drift"], serde_json::json!(true), "disclosed, not fatal");
    assert_eq!(v["stale_reasons"], serde_json::json!([]), "head never enters reasons");
    assert_eq!(v["serves"], serde_json::json!("current-tree"));
}

/// TC-7 no-slot state: exit 2, NO JSON on stdout, stderr guidance that
/// names the build command AND the empty-terminal explanation (SM-17 —
/// never misleading, never claims fresh).
#[test]
fn tc7_no_slot_fails_loud_with_empty_terminal_note() {
    let t = tempfile::tempdir().unwrap();
    let out = freshness(t.path(), true);
    assert_eq!(out.exit_code, 2, "{out:?}");
    assert!(out.stdout.is_empty(), "no-slot produces no JSON: {out:?}");
    assert!(out.stderr.contains("build"), "names the remediation: {out:?}");
    assert!(
        out.stderr.contains("空終態") || out.stderr.contains("排除"),
        "carries the empty-terminal explanation: {out:?}"
    );
    assert!(
        !out.stderr.contains("[OK] fresh") && !out.stderr.contains("\"fresh\": true"),
        "must never claim fresh: {out:?}"
    );

    // human face on a real slot: verdict lines, correct exits
    let l = lab(&[("app.py", "x = 1\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let out = freshness(&l.repo, false);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("[OK] fresh"), "{out:?}");
    assert!(out.stdout.contains("current-tree"), "{out:?}");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(l.repo.join("app.py"), "x = 2\n").unwrap();
    let out = freshness(&l.repo, false);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.contains("[WARN] stale"), "{out:?}");
    assert!(out.stdout.contains("content-drift"), "{out:?}");
}

/// Route smoke: the umbrella bin registers `freshness`; usage errors
/// stay exit-2 argparse faces.
#[test]
fn tc7_route_and_usage_faces() {
    let out = code_reality::freshness::run(&["freshness"]);
    assert_eq!(out.exit_code, 2, "missing --repo is a usage fail: {out:?}");
    let out = code_reality::freshness::run(&["freshness", "--help"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("usage: code-reality freshness"), "{out:?}");

    let bin = env!("CARGO_BIN_EXE_code-reality");
    let t = tempfile::tempdir().unwrap();
    let st = std::process::Command::new(bin)
        .args(["freshness", "--repo", t.path().to_str().unwrap(), "--json"])
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality bin");
    assert_eq!(st.status.code(), Some(2), "no-slot via the bin: {st:?}");
    let stdout = String::from_utf8_lossy(&st.stdout);
    assert!(stdout.trim().is_empty(), "no JSON on the no-slot face");
    // the SUBCOMMANDS face carries freshness
    let st = std::process::Command::new(bin)
        .arg("--help")
        .env("CR_REPO", "/nonexistent")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&st.stdout).contains("freshness"));
}
