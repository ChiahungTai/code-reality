//! JS/TS blueprint S4 freshness tests: fingerprint-driven
//! add/delete/rename detection (the mtime-only blind spot), corpus-policy
//! drift, face isolation, legacy-metadata fallback, and the stamp
//! consistency rule. Fake producers only — hermetic.
//!
//! Both fakes DERIVE their output through the shared fixture producer
//! (`tests/support` + `examples/scip_fixture_producer`): the TS fake
//! emits one DEF document per governed file in the CR-owned derived
//! config and the py fake one per on-disk .py, so a heal after
//! add/delete/rename converges to the real corpus instead of replaying
//! a stale fixture. The one deliberate exception keeps a static fixture:
//! the non-convergence test needs a producer that PERSISTENTLY OMITS a
//! disk file.

mod support;

use code_reality::build::{build_repo, ensure_fresh, HealOutcome};
use code_reality::engine::evaluate_staleness;
use code_reality::identity::IdentityCachePolicy;
use code_reality::language::ProducerFamily;
use protobuf::Message;
use scip::types::{Document, Index, Occurrence};
use std::path::{Path, PathBuf};
use support::{git_init, mkrepo, slot_docs, slot_of};

/// Python-shaped STATIC fixture for the one test that needs a producer
/// persistently omitting a disk file (the non-convergence premise): the
/// doc set is pinned to {app.py} and never tracks later disk additions,
/// while the several DEFs clear the python leg's 128-byte empty-index
/// guard. The deriving fakes cannot express this shape.
fn py_scip_bytes() -> Vec<u8> {
    let mut index = Index::new();
    let mut d = Document::new();
    d.relative_path = "app.py".to_string();
    for name in ["app_main", "app_aux", "app_third", "app_fourth"] {
        let mut occ = Occurrence::new();
        occ.symbol = format!("pyrefly python proj 0.1.0 `m`/{name}().");
        occ.symbol_roles = 1;
        occ.range = vec![0, 0, 1];
        d.occurrences.push(occ);
    }
    index.documents.push(d);
    index.write_to_bytes().unwrap()
}

fn fake_pyrefly(dir: &Path, py_fixture: &Path) {
    support::fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--repo\" ] || [ \"$prev\" = \"--out\" ]; then eval \"${{prev#--}}=\\\"$a\\\"\"; fi; prev=\"$a\"; done
mkdir -p \"$(dirname \"$out\")\"
cp '{py}' \"$out\"
echo '[OK] fake pyrefly-index'
",
            py = py_fixture.display()
        ),
    );
}

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

fn lab(files: &[(&str, &str)]) -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = support::mkrepo(&t, files);
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

#[test]
fn s4_delete_detected_by_fingerprint_not_mtime() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
        ("src/b.mjs", "export const b = 1;\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("initial build");
    assert_eq!(
        slot_docs(&l.repo),
        vec![
            "app.py".to_string(),
            "src/a.mjs".to_string(),
            "src/b.mjs".to_string()
        ]
    );

    // DELETE with the surviving sources all OLDER than the slot: the
    // mtime-only rule sees nothing; the fingerprint must.
    std::fs::remove_file(l.repo.join("src/a.mjs")).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap.source_newer, "mtime must be silent here");
    assert_eq!(
        snap.doc_set_drift,
        Some(true),
        "fingerprint sees the delete"
    );
    assert!(snap.needs_rebuild());

    // heal converges: the deleted doc disappears from the rebuilt index
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    match &out {
        HealOutcome::Healed { .. } => {}
        other => panic!("expected Healed, got {other:?}"),
    }
    assert_eq!(
        slot_docs(&l.repo),
        vec!["app.py".to_string(), "src/b.mjs".to_string()]
    );
    // converged: no loop
    let snap2 = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap2.needs_rebuild(), "{snap2:?}");
}

#[test]
fn s4_rename_detected_and_converges() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    std::thread::sleep(std::time::Duration::from_millis(20));
    // rename preserves mtime — mtime stays silent, fingerprint fires
    std::fs::rename(l.repo.join("src/a.mjs"), l.repo.join("src/renamed.mjs")).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap.source_newer);
    assert_eq!(snap.doc_set_drift, Some(true));
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    assert_eq!(
        slot_docs(&l.repo),
        vec!["app.py".to_string(), "src/renamed.mjs".to_string()]
    );
}

#[test]
fn s4_excluded_edit_is_silent_included_edit_stales() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
        ("dist/gen.mjs", "generated\n"),
        (".code-reality.toml", "exclude = [\"dist/\"]\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    // excluded edit: no staleness (producer corpus and freshness corpus
    // share one policy — AD-11 / SM-24)
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(l.repo.join("dist/gen.mjs"), "generated v2\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(
        !snap.needs_rebuild(),
        "excluded edit must not stale: {snap:?}"
    );
    // included edit: exactly the expected transition
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let snap2 = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(snap2.source_newer, "included edit stales: {snap2:?}");
}

#[test]
fn s4_profile_policy_change_triggers_rebuild() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
        ("gen/x.mjs", "to be excluded later\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    assert_eq!(
        slot_docs(&l.repo),
        vec![
            "app.py".to_string(),
            "gen/x.mjs".to_string(),
            "src/a.mjs".to_string()
        ]
    );
    // adding the exclusion changes the corpus policy WITHOUT touching
    // any source mtime — corpus_policy_drift must fire
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(l.repo.join(".code-reality.toml"), "exclude = [\"gen/\"]\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap.source_newer, "no mtime movement: {snap:?}");
    assert_eq!(snap.corpus_policy_drift, Some(true), "{snap:?}");
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    assert_eq!(
        slot_docs(&l.repo),
        vec!["app.py".to_string(), "src/a.mjs".to_string()]
    );
}

#[test]
fn s4_face_isolation_python_only_slot_ignores_ts() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    // explicit python-only face in a py+js repo
    build_repo(&l.repo, Some(ProducerFamily::Python), &l.roots).expect("py-only build");
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(
        !snap.needs_rebuild(),
        "a newer .ts/.mjs must not stale a python-only face: {snap:?}"
    );
    // meta stamps only the python face
    let meta =
        std::fs::read_to_string(l.repo.join(".code-reality/scip/index.scip.meta.json")).unwrap();
    assert!(meta.contains("\"source_faces\""), "{meta}");
    assert!(meta.contains("python"), "{meta}");
    assert!(!meta.contains("typescript"), "{meta}");
}

#[test]
fn s4_legacy_meta_without_keys_uses_baseline_behavior() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    // rewrite meta in the legacy shape (no S4 keys)
    let slot = slot_of(&l.repo);
    std::fs::write(
        code_reality::engine::meta_path(&slot),
        r#"{"repo": "/x", "head": "h", "stamped_at": "2026-01-01T00:00:00+00:00", "tool": "t", "producer": "p"}"#,
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::remove_file(l.repo.join("src/a.mjs")).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot, IdentityCachePolicy::WriteBack).unwrap();
    assert_eq!(
        snap.doc_set_drift, None,
        "legacy meta: no fingerprint compare"
    );
    assert!(!snap.needs_rebuild(), "baseline mtime semantics apply");
}

#[test]
fn s4_stamp_preserves_prior_identity_keys_when_docs_differ_from_disk() {
    // codex blocker 2: the identity keys describe the INDEX's corpus
    // contract. On disk/index mismatch a head-sync restamp must PRESERVE
    // the prior keys verbatim — they still truthfully describe the
    // unchanged index, so the delete stays visible as fingerprint drift.
    // (Dropping the keys would launder it into mtime-only freshness.)
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let slot = slot_of(&l.repo);
    let meta_before = std::fs::read_to_string(code_reality::engine::meta_path(&slot)).unwrap();
    assert!(
        meta_before.contains("source_set_fingerprint"),
        "{meta_before}"
    );

    // delete a doc, then head-sync-style stamp
    std::fs::remove_file(l.repo.join("src/a.mjs")).unwrap();
    code_reality::engine::stamp_meta_core(&l.repo, &slot, &l.roots, None, None).expect("stamp");
    let meta_after = std::fs::read_to_string(code_reality::engine::meta_path(&slot)).unwrap();
    assert!(
        meta_after.contains("source_set_fingerprint"),
        "prior keys must survive the restamp: {meta_after}"
    );
    assert!(meta_after.contains("source_faces"), "{meta_after}");

    // and the preserved keys keep the delete visible: drift fires
    let snap = evaluate_staleness(&l.repo, &slot, IdentityCachePolicy::WriteBack).unwrap();
    assert_eq!(snap.doc_set_drift, Some(true), "{snap:?}");
    assert!(snap.needs_rebuild(), "{snap:?}");
}

#[test]
fn s4_ts_producer_drift_note_on_flagged_path() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    let slot = slot_of(&l.repo);
    // stamp a DIFFERENT scip-typescript version than the fixture reports
    let meta = std::fs::read_to_string(code_reality::engine::meta_path(&slot)).unwrap();
    let stamped = serde_json::from_str::<serde_json::Value>(&meta).unwrap();
    let producer = stamped["producer"].as_str().unwrap().to_string();
    let swapped = producer.replace("scip-typescript 0.4.0-fixture", "scip-typescript 0.1.0");
    assert_ne!(
        swapped, producer,
        "premise: the stamped producer carries the fixture version"
    );
    let mut v = stamped.clone();
    v["producer"] = serde_json::json!(swapped);
    std::fs::write(
        code_reality::engine::meta_path(&slot),
        serde_json::to_string_pretty(&v).unwrap(),
    )
    .unwrap();
    // The drift note rides the flagged paths only — drive the churn
    // cooldown lane (it reads the STAMPED meta without rebuilding; a
    // converged heal would restamp the current version and erase the
    // drift by design)
    std::fs::write(l.repo.join(".code-reality/scip/.heal-churn"), b"").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(l.repo.join("src/a.mjs"), "export const a = 2;\n").unwrap();
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    match &out {
        HealOutcome::ServeStale(lines) => assert!(
            lines.iter().any(|n| n.contains("cooldown"))
                && lines
                    .iter()
                    .any(|n| n.contains("scip-typescript") && n.contains("0.1.0")),
            "{lines:?}"
        ),
        other => panic!("expected cooldown ServeStale with drift note, got {other:?}"),
    }
}

#[test]
fn s4_nonconverged_heal_arms_cooldown_not_fresh() {
    // codex P0-4: a producer that persistently omits a disk file must
    // not let the NEXT query call the incomplete index Fresh — the
    // missing/extra branch arms the churn marker (EP S4 "arm existing
    // cooldown").
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("app.py", "x = 1\n")]);
    git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    let py_fx = bindir.path().join("py.scip");
    std::fs::write(&py_fx, py_scip_bytes()).unwrap();
    fake_pyrefly(bindir.path(), &py_fx);
    let roots = vec![bindir.path().to_path_buf()];
    build_repo(&repo, None, &roots).expect("build");

    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(repo.join("app2.py"), "x = 2\n").unwrap();
    let first = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&first, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("語料不一致"))),
        "producer omits app2.py → non-convergence: {first:?}"
    );
    assert!(
        repo.join(".code-reality/scip/.heal-churn").exists(),
        "churn marker must be armed by the non-converged heal"
    );
    // second query inside the cooldown window: NOT Fresh
    let second = ensure_fresh(&repo, &roots).unwrap();
    assert!(
        matches!(&second, HealOutcome::ServeStale(l) if l.iter().any(|s| s.contains("cooldown"))),
        "cooldown must hold before re-burning the heal: {second:?}"
    );
}

#[test]
fn s4_all_excluded_corpus_heals_to_empty_then_fresh() {
    // codex blocker 1 invariant on the freshness axis: a previously
    // indexed JS corpus whose profile comes to exclude everything must
    // HEAL to the empty terminal state (index+graph removed), then the
    // next check is Fresh — never a permanent serve-stale loop.
    let l = lab(&[("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("initial build");
    assert!(slot_of(&l.repo).exists());
    std::thread::sleep(std::time::Duration::from_millis(30));
    // exclude the whole JS corpus — corpus-policy drift fires, the heal
    // must converge to empty
    std::fs::write(l.repo.join(".code-reality.toml"), "exclude = [\"src/\"]\n").unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(snap.needs_rebuild(), "{snap:?}");
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    assert!(!slot_of(&l.repo).exists(), "index removed — empty terminal");
    assert!(
        !l.repo.join(".code-reality/graph.db").exists(),
        "stale graph removed"
    );
    // converged: next check is Fresh (slot absence = fresh)
    let second = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert_eq!(second, HealOutcome::Fresh, "{second:?}");
}

#[test]
fn s4_auto_index_heals_on_new_language_arrival() {
    // muse P0-1: an AUTO-built single-face index must heal when a file
    // of a NEW language arrives — the eval scope unions stamped faces
    // with detected disk faces (an explicit override stays pinned).
    let l = lab(&[("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("auto ts-only build");
    // sanity: JS-only, faces={javascript}
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap.needs_rebuild(), "{snap:?}");
    std::thread::sleep(std::time::Duration::from_millis(30));
    // a Python file arrives — mtime-newer on a face the stamp lacks
    std::fs::write(l.repo.join("app.py"), "x = 1\n").unwrap();
    let snap2 = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(snap2.needs_rebuild(), "new face must trigger: {snap2:?}");
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    // healed: the rebuilt index carries both faces; next check Fresh
    let snap3 = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(!snap3.needs_rebuild(), "{snap3:?}");
}

#[test]
fn s4_torn_graph_lags_slot_forces_heal() {
    // muse P1-3: the slot published but the graph build failed — a
    // graph.db older than the slot is a torn pair and must force a heal
    // (it would otherwise read Fresh forever).
    let l = lab(&[("src/a.mjs", "export const a = 1;\n")]);
    build_repo(&l.repo, None, &l.roots).expect("build");
    assert!(!evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack)
        .unwrap()
        .needs_rebuild());
    // simulate the torn state: bump the SLOT mtime past the graph
    let bytes = std::fs::read(slot_of(&l.repo)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(slot_of(&l.repo), &bytes).unwrap();
    let snap = evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack).unwrap();
    assert!(
        snap.needs_rebuild(),
        "graph older than slot must heal: {snap:?}"
    );
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    // graph rebuilt after the slot → converged
    let g = l.repo.join(".code-reality/graph.db");
    let gm = g.metadata().unwrap().modified().unwrap();
    let sm = slot_of(&l.repo).metadata().unwrap().modified().unwrap();
    assert!(gm > sm, "graph must land after the slot");
    assert!(!evaluate_staleness(&l.repo, &slot_of(&l.repo), IdentityCachePolicy::WriteBack)
        .unwrap()
        .needs_rebuild());
}
