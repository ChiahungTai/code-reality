//! CR-10 S2 — base manifest production + freshness delta disclosure
//! (EP `ai-analysis/execution-plans/2026-10-14-cr-delta-freshness`,
//! frozen TC rows TC-1/2/3a/7/12/16/17/18/19; TC-14's freshness face is
//! the manual NT-scale machine check, not an in-tree test).
//!
//! Contract under test (D1-D5/D15):
//! - validated builds publish `.code-reality/scip/index.scip.manifest.json`
//!   (schema v1: repo/selection/faces/identity_algo/index_sha256/
//!   source_identity/topology_hash/entries[{face,rel,size,hash}] — no
//!   mtime) and stamp a `base_manifest_digest` pointer into the meta;
//! - `freshness --json` grows ONLY additive keys: `base_commit`,
//!   `producer_version`, `delta`, `delta_identity`,
//!   `delta_unavailable_reason`. The `delta` object is ALWAYS present;
//!   its members are null (with `delta_complete:false`) exactly when a
//!   reason is set — never `[]` masquerading as no-change;
//! - the binding audit runs legs ① fields ② recomposition ③ digest
//!   ④ serving bytes in that order; any failure degrades (never a
//!   substitute answer, and the verdict never flips fresh).
//!
//! The tests drive only public faces (freshness JSON + the filesystem),
//! so this file compiles against the pre-S2 binary and fails there (RED).

mod support;

use code_reality::build::{build_repo, ensure_fresh, HealOutcome};
use code_reality::engine::{default_index_path, meta_path};
use code_reality::freshness::{freshness, freshness_with};
use serde_json::{json, Value};
use std::path::PathBuf;

const MANIFEST_REL: &str = ".code-reality/scip/index.scip.manifest.json";

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

fn lab(files: &[(&str, &str)]) -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    for (rel, content) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
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

/// Rust-face lab: root manifest + lockfile + the exact docs the shared
/// rich.scip fixture carries (crates/a.rs + crates/b.rs — the stamp's
/// docs==disk gate needs the fake producer output to match the corpus).
fn lab_cargo() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(repo.join("Cargo.lock"), "version = 3\n").unwrap();
    std::fs::create_dir_all(repo.join("crates")).unwrap();
    std::fs::write(repo.join("crates/a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo.join("crates/b.rs"), "pub fn b() {}\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    support::install_rust_fixture_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

/// Sibling-workspace rust lab: w1/ is the single stamped workspace root
/// (auto-discovered immediate-child [workspace]); its crates match the
/// shared rich.scip docs (crates/a.rs + crates/b.rs — the rust leg
/// normalizes them to w1/-prefixed git-relative docs, so the stamp's
/// docs==disk gate passes).
fn lab_rust_ws() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(repo.join("w1/crates")).unwrap();
    std::fs::write(
        repo.join("w1/Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = []\n",
    )
    .unwrap();
    std::fs::write(repo.join("w1/crates/a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo.join("w1/crates/b.rs"), "pub fn b() {}\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    support::install_rust_fixture_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

impl Lab {
    fn slot(&self) -> PathBuf {
        default_index_path(&self.repo).unwrap()
    }
    fn build(&self) {
        build_repo(&self.repo, None, &self.roots).expect("build");
    }
    fn write(&self, rel: &str, content: &str) {
        let p = self.repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    fn meta(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(meta_path(&self.slot())).unwrap()).unwrap()
    }
    fn write_meta(&self, v: &Value) {
        std::fs::write(
            meta_path(&self.slot()),
            format!("{}\n", serde_json::to_string_pretty(v).unwrap()),
        )
        .unwrap();
    }
    fn manifest_bytes(&self) -> Vec<u8> {
        std::fs::read(self.repo.join(MANIFEST_REL)).unwrap()
    }
    fn set_manifest_bytes(&self, bytes: &[u8]) {
        std::fs::write(self.repo.join(MANIFEST_REL), bytes).unwrap();
    }
    fn manifest_json(&self) -> Value {
        serde_json::from_slice(&self.manifest_bytes()).unwrap()
    }
    fn verdict(&self) -> (i32, Value) {
        let out = freshness(&self.repo, true);
        let v: Value = serde_json::from_str(out.stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "stdout not verdict JSON ({e}): {:?} stderr={:?}",
                out.stdout, out.stderr
            )
        });
        (out.exit_code, v)
    }
}

/// A real content drift on a file the manifest attacks never touch —
/// every TC-7 attack runs on a stale base so "never flips fresh" is a
/// positive assertion.
fn stale_base(l: &Lab) {
    l.write("src/a.mjs", "export const a = 2;\n");
}

// ---------- TC-1: docs-only head move, empty delta, intact binding ----------

#[test]
fn tc1_docs_only_head_move_yields_fresh_with_empty_delta() {
    let l = lab(&[("app.py", "x = 1\n"), ("docs.md", "v1\n")]);
    l.build();
    let manifest_before = l.manifest_bytes();
    assert!(
        !manifest_before.is_empty(),
        "a validated build publishes the manifest sidecar"
    );

    // D2 schema v1 face: frozen key set, no mtime in entries
    let mv = l.manifest_json();
    assert_eq!(mv["schema_version"], json!(1));
    for k in [
        "repo",
        "selection",
        "faces",
        "identity_algo",
        "index_sha256",
        "source_identity",
        "topology_hash",
        "entries",
    ] {
        assert!(mv.get(k).is_some(), "manifest key missing: {k}");
    }
    let entries = mv["entries"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e["face"] == "python" && e["rel"] == "app.py"),
        "governed source rows present: {entries:?}"
    );
    assert!(
        entries.iter().all(|e| e.get("mtime").is_none()),
        "D2: entries carry no mtime"
    );

    l.write("docs.md", "v2\n");
    support::git_commit_all(&l.repo, "docs");
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "docs-only head move stays fresh: {v}");
    assert_eq!(v["fresh"], json!(true));
    assert_eq!(v["delta_unavailable_reason"], Value::Null);
    assert_eq!(v["delta"]["delta_complete"], json!(true));
    assert_eq!(v["delta"]["changed"]["count"], json!(0));
    assert_eq!(v["delta"]["added"]["count"], json!(0));
    assert_eq!(v["delta"]["removed"]["count"], json!(0));

    // binding disclosure: digest pointer matches the sidecar bytes; the
    // recomposition identity equals the stamped pair
    let meta = l.meta();
    assert_eq!(
        meta["base_manifest_digest"],
        json!(code_reality::identity::sha256_hex(&l.manifest_bytes())),
    );
    assert_eq!(v["delta_identity"], v["indexed_source_identity"]);
    // D1 display anchor: the STAMPED generation's commit — the docs
    // commit moved HEAD, and the indexed generation stays anchored to
    // its own stamp (a freshness read never re-anchors it)
    assert_eq!(v["base_commit"], meta["head"]);
    assert_ne!(
        v["base_commit"],
        json!(support::git_head(&l.repo)),
        "display anchor is the stamped head, not live HEAD"
    );
    assert_eq!(v["producer_version"], meta["producer"]);

    // the sidecar itself is untouched by the head move
    assert_eq!(
        l.manifest_bytes(),
        manifest_before,
        "head movement must not touch the sidecar"
    );
}

// ---------- TC-2: dirty-at-build delta non-empty while git diff empty ----------

#[test]
fn tc2_dirty_at_build_delta_nonempty_while_git_diff_empty() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    // x=2 is the dirty worktree state — never any commit's content
    l.write("app.py", "x = 2\n");
    l.build(); // the manifest pins x = 2
    l.write("app.py", "x = 3\n"); // a third state
    support::git_commit_all(&l.repo, "x3"); // HEAD == worktree now
    assert!(
        support::git_worktree_diff(&l.repo).is_empty(),
        "premise: git diff HEAD is empty"
    );
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "manifest(x=2) vs walk(x=3) drifts: {v}");
    assert_eq!(v["delta"]["changed"]["count"], json!(1));
    assert_eq!(v["delta"]["changed"]["paths"], json!(["app.py"]));
    assert_eq!(v["delta"]["delta_complete"], json!(true));
    assert_eq!(v["delta"]["added"]["count"], json!(0));
    assert_eq!(v["delta"]["removed"]["count"], json!(0));
}

// ---------- TC-3a: untracked governed file enters delta.added ----------

#[test]
fn tc3a_untracked_governed_file_enters_delta_added() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l.build();
    l.write("new.py", "n = 0\n");
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "corpus growth drifts the identity: {v}");
    assert_eq!(v["delta"]["added"]["count"], json!(1));
    assert_eq!(v["delta"]["added"]["paths"], json!(["new.py"]));
    assert_eq!(v["delta"]["changed"]["count"], json!(0));
    assert_eq!(v["delta"]["removed"]["count"], json!(0));
    assert_eq!(v["delta"]["delta_complete"], json!(true));
    assert_eq!(v["delta_unavailable_reason"], Value::Null);
}

// ---------- TC-7: binding attacks degrade, never flip fresh ----------

#[test]
fn tc7_binding_attacks_degrade_and_never_flip_fresh() {
    // attack 1a — naive single-entry hash tamper: the bytes change (so
    // the digest leg would also fire) but the RECOMPOSITION audit must
    // catch it first — pinning the leg order ①→②→③→④.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        let mut mv = l.manifest_json();
        let entries = mv["entries"].as_array_mut().unwrap();
        let e = entries.iter_mut().find(|e| e["rel"] == "app.py").unwrap();
        e["hash"] = json!("0".repeat(64));
        l.set_manifest_bytes(serde_json::to_string_pretty(&mv).unwrap().as_bytes());
        stale_base(&l);
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["fresh"], json!(false), "an attack must never flip fresh");
        assert_eq!(v["delta"]["delta_complete"], json!(false));
        assert_eq!(v["delta"]["changed"], Value::Null, "no substitute answer");
        assert_eq!(
            v["delta_unavailable_reason"],
            json!("manifest-binding-audit-failed"),
            "the recomposition audit catches the tamper (not the digest leg)"
        );
        assert_ne!(
            v["delta_identity"], v["indexed_source_identity"],
            "the recomposed evidence is disclosed and diverges"
        );
    }

    // attack 1b — self-consistent tamper (digest + meta pointer
    // recomputed): still leg ② — a single-entry tamper cannot forge the
    // stamped identity.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        let mut mv = l.manifest_json();
        let entries = mv["entries"].as_array_mut().unwrap();
        let e = entries.iter_mut().find(|e| e["rel"] == "app.py").unwrap();
        e["hash"] = json!("0".repeat(64));
        let text = format!("{}\n", serde_json::to_string_pretty(&mv).unwrap());
        l.set_manifest_bytes(text.as_bytes());
        let mut meta = l.meta();
        meta["base_manifest_digest"] =
            json!(code_reality::identity::sha256_hex(&l.manifest_bytes()));
        l.write_meta(&meta);
        stale_base(&l);
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["delta"]["delta_complete"], json!(false));
        assert_eq!(
            v["delta_unavailable_reason"],
            json!("manifest-binding-audit-failed"),
            "a digest-consistent tamper is still caught by recomposition"
        );
    }

    // attack 2 — manifest transplanted from another repo: leg ① fields
    // (repo/faces) mismatch.
    {
        let a = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        let b = lab(&[("different.py", "y = 9\n")]);
        a.build();
        b.build();
        let bytes = b.manifest_bytes();
        a.set_manifest_bytes(&bytes);
        stale_base(&a);
        let (code, v) = a.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["fresh"], json!(false));
        assert_eq!(
            v["delta_unavailable_reason"],
            json!("manifest-binding-audit-failed")
        );
        assert_eq!(v["delta"]["delta_complete"], json!(false));
    }

    // attack 3 — index replaced, manifest+meta kept: leg ④ serving
    // bytes no longer match the manifest's index_sha256.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        let mut bytes = std::fs::read(l.slot()).unwrap();
        bytes[0] ^= 0xff;
        std::fs::write(l.slot(), &bytes).unwrap();
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["fresh"], json!(false));
        assert_eq!(
            v["delta_unavailable_reason"],
            json!("manifest-binding-audit-failed"),
            "the serving slot bytes no longer match the bound index_sha256"
        );
        assert_eq!(v["delta"]["delta_complete"], json!(false));
    }

    // leg ③ alone — content-preserving repack (identical logical
    // entries, different bytes): manifest-digest-mismatch.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        let mv = l.manifest_json();
        let text = format!("{}\n", serde_json::to_string(&mv).unwrap()); // compact repack
        l.set_manifest_bytes(text.as_bytes());
        stale_base(&l);
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(
            v["delta_unavailable_reason"],
            json!("manifest-digest-mismatch")
        );
        assert_eq!(v["delta"]["delta_complete"], json!(false));
    }
}

// ---------- TC-12: lifecycle — head-sync / heal / empty / lsp_harvest ----------

#[test]
fn tc12_lifecycle_head_sync_heal_empty_convergence_lsp_harvest() {
    // leg 1 — head-sync: the sidecar and its pointer survive untouched
    let l = lab(&[("app.py", "x = 1\n"), ("docs.md", "v1\n")]);
    l.build();
    let manifest_before = l.manifest_bytes();
    l.write("docs.md", "v2\n");
    support::git_commit_all(&l.repo, "docs");
    code_reality::engine::stamp_meta_core(&l.repo, &l.slot(), &l.roots, None, None)
        .expect("head-sync stamp");
    assert_eq!(
        l.manifest_bytes(),
        manifest_before,
        "head-sync must not touch the sidecar"
    );
    assert_eq!(
        l.meta()["base_manifest_digest"],
        json!(code_reality::identity::sha256_hex(&manifest_before)),
        "the pointer is preserved across the head-sync"
    );
    assert_eq!(l.meta()["head"], json!(support::git_head(&l.repo)));
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["delta"]["delta_complete"], json!(true));
    assert_eq!(v["delta_unavailable_reason"], Value::Null);

    // leg 2 — Healed rebuild: the new sidecar publishes in sync with the
    // new index (the SM-15 "old generation annotates indeterminate" half
    // is S4 vocabulary — out of S2 scope by the D17 fence)
    l.write("app.py", "x = 2\n");
    let out = ensure_fresh(&l.repo, &l.roots).unwrap();
    assert!(matches!(out, HealOutcome::Healed { .. }), "{out:?}");
    let digest = code_reality::identity::sha256_hex(&l.manifest_bytes());
    assert_eq!(
        l.meta()["base_manifest_digest"],
        json!(digest),
        "the new manifest published with the new index"
    );
    let mv = l.manifest_json();
    assert_eq!(mv["index_sha256"], l.meta()["index_sha256"]);
    assert_eq!(mv["source_identity"], l.meta()["source_identity"]);
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "converged: {v}");
    assert_eq!(v["delta"]["changed"]["count"], json!(0));

    // leg 3 — empty convergence: the sidecar goes with the slot (zero
    // orphans)
    let l3 = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l3.build();
    assert!(
        l3.repo.join(MANIFEST_REL).exists(),
        "premise: sidecar present"
    );
    l3.write(".code-reality.toml", "exclude = [\"app.py\", \"src/\"]\n");
    build_repo(&l3.repo, None, &l3.roots).expect("empty-convergence build");
    assert!(!l3.slot().exists(), "slot removed");
    assert!(
        !l3.repo.join(MANIFEST_REL).exists(),
        "sidecar removed with the slot — zero orphans"
    );

    // leg 4 — lsp_harvest-style wholesale meta rewrite: legacy
    // degradation, no panic, verdict follows the legacy axis
    let l4 = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l4.build();
    std::fs::write(
        meta_path(&l4.slot()),
        "{\"head\": \"placeholder\", \"producer\": \"lsp_harvest\"}\n",
    )
    .unwrap();
    let (code, v) = l4.verdict();
    assert_eq!(code, 0, "legacy axis: sources untouched → fresh: {v}");
    assert_eq!(v["delta_unavailable_reason"], json!("legacy-no-manifest"));
    assert_eq!(v["delta"]["delta_complete"], json!(false));
    assert_eq!(v["delta"]["changed"], Value::Null);
    assert_eq!(v["serves"], json!("legacy-signals"));
}

// ---------- TC-16: legacy slot — delta fields null with reason ----------

#[test]
fn tc16_legacy_slot_null_delta_with_reason_and_verdict_intact() {
    let legacy_meta = r#"{"repo": "/x", "head": "h", "stamped_at": "2026-01-01T00:00:00+00:00", "tool": "t", "producer": "p"}"#;

    // legacy-FRESH half: sources untouched
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l.build();
    std::fs::write(meta_path(&l.slot()), legacy_meta).unwrap();
    let (code, v) = l.verdict();
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["serves"], json!("legacy-signals"));
    assert!(v["indexed_source_identity"].is_null());
    assert_eq!(v["delta_unavailable_reason"], json!("legacy-no-manifest"));
    assert_eq!(v["delta"]["delta_complete"], json!(false));
    for k in [
        "changed",
        "added",
        "removed",
        "content_verification",
        "out_of_scope_count",
        "policy_drift_flags",
    ] {
        assert!(
            v["delta"][k].is_null(),
            "delta.{k} must be null on degradation"
        );
    }

    // legacy-STALE half: the mtime axis still kills it (三態照舊)
    let l2 = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l2.build();
    std::fs::write(meta_path(&l2.slot()), legacy_meta).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    l2.write("app.py", "x = 2\n");
    let (code2, v2) = l2.verdict();
    assert_eq!(code2, 1, "{v2}");
    assert_eq!(v2["stale_reasons"], json!(["legacy-signals"]));
    assert_eq!(v2["delta_unavailable_reason"], json!("legacy-no-manifest"));
}

// ---------- TC-17: policy flags fire without folding file counts ----------

#[test]
fn tc17_policy_drift_flags_fire_without_folding_file_counts() {
    // (a) profile exclude switch: the excluded file exists on disk →
    // `profile`; the JS/TS corpus-policy fingerprint moved →
    // `js-ts-config`. Neither folds into the counts.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
            ("legacy/old.py", "o = 1\n"),
        ]);
        l.build();
        l.write(".code-reality.toml", "exclude = [\"legacy/\"]\n");
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "the policy move drifts the corpus: {v}");
        assert_eq!(
            v["delta"]["policy_drift_flags"],
            json!(["js-ts-config", "profile"]),
            "both policy axes fire, sorted"
        );
        assert_eq!(
            v["delta"]["removed"]["count"],
            json!(0),
            "the excluded file must not fold into removed"
        );
        assert_eq!(v["delta"]["changed"]["count"], json!(0));
        assert_eq!(v["delta"]["added"]["count"], json!(0));
        assert_eq!(v["delta"]["delta_complete"], json!(true));
    }

    // (b) sibling workspace root added (auto-discovery takes immediate-
    // child [workspace] manifests — a root nested inside "." would fail
    // loud CR-RUST-ROOT-03): `topology` fires; the new root's rust-input
    // row stays out of the file counts.
    {
        let l = lab_rust_ws();
        l.build();
        l.write(
            "w2/Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = []\n",
        );
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "the new root moves the topology domain: {v}");
        assert_eq!(
            v["delta"]["policy_drift_flags"],
            json!(["topology"]),
            "topology-only movement"
        );
        assert_eq!(
            v["delta"]["added"]["count"],
            json!(0),
            "the topology-attributed rust-input row is not in the counts"
        );
        assert_eq!(v["delta"]["changed"]["count"], json!(0));
        assert_eq!(v["delta"]["removed"]["count"], json!(0));
        assert_eq!(v["delta"]["delta_complete"], json!(true));
    }

    // (c) detected-only face (review intent-F5 extension): a python-only
    // stamped slot never folds a newly governed .mjs into `added` — the
    // face is outside the stamped scope, so the file discloses through
    // `out_of_scope_count` instead.
    {
        let l = lab(&[("app.py", "x = 1\n")]);
        l.build();
        l.write("src/new.mjs", "export const n = 1;\n");
        let (code, v) = l.verdict();
        assert_eq!(
            code, 1,
            "the detected face drifts the eval-scope identity: {v}"
        );
        let added = v["delta"]["added"]["paths"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            !added.iter().any(|p| p == "src/new.mjs"),
            "a detected-only face file never enters added: {added:?}"
        );
        let oos = v["delta"]["out_of_scope_count"]
            .as_u64()
            .expect("out_of_scope_count present");
        assert!(oos >= 1, "the .mjs discloses as out-of-scope: {v}");
        assert_eq!(v["delta"]["delta_complete"], json!(true));
    }
}

// ---------- TC-18: rust-input rows enumerate in delta.changed ----------

#[test]
fn tc18_cargo_toml_content_change_is_a_changed_row() {
    let l = lab_cargo();
    l.build();
    l.write("Cargo.toml", "[workspace]\nmembers = []\n# s2 delta\n");
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "a producer-input change drifts the identity: {v}");
    assert_eq!(v["delta"]["changed"]["count"], json!(1));
    assert_eq!(v["delta"]["changed"]["paths"], json!(["Cargo.toml"]));
    assert_eq!(v["delta"]["delta_complete"], json!(true));
    assert_eq!(
        v["delta"]["policy_drift_flags"],
        json!([]),
        "content-only change: no policy flag fires"
    );
    assert_eq!(v["delta_unavailable_reason"], Value::Null);
}

// ---------- TC-19: poisoned-cache residual disclosed; discard equivalent ----------

#[test]
fn tc19_poisoned_cache_residual_disclosed_and_discard_equivalent() {
    let l = lab(&[
        ("app.py", "x = 1\n"),
        ("src/a.mjs", "export const a = 1;\n"),
    ]);
    l.build();
    l.verdict(); // warm the query-side hash cache (WriteBack)
    let cache_path = code_reality::identity::cache_path_for_slot(&l.slot());
    let mut p: Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    let entries = p["entries"].as_object_mut().unwrap();
    assert!(
        entries.contains_key("app.py"),
        "warm cache covers the corpus"
    );
    // same size + same mtime, wrong hash — the documented stat-gate residual
    entries["app.py"]["hash"] = json!("e".repeat(64));
    std::fs::write(&cache_path, serde_json::to_string_pretty(&p).unwrap()).unwrap();

    let (code, v) = l.verdict();
    assert_eq!(code, 1, "the poisoned hash drifts the identity: {v}");
    assert_eq!(
        v["delta"]["changed"]["paths"],
        json!(["app.py"]),
        "documented residual: the gate-served poisoned hash reaches the delta"
    );
    assert_eq!(
        v["delta"]["content_verification"],
        json!("stat-cache"),
        "honest disclosure: every hash came from the stat gate"
    );
    assert_eq!(v["delta"]["delta_complete"], json!(true));

    // corrupt the cache wholesale → crash-only discard → the answer
    // equals the no-cache full recompute
    std::fs::write(&cache_path, "{not json").unwrap();
    let (code2, v2) = l.verdict();
    assert_eq!(code2, 0, "discard converges back to fresh: {v2}");
    assert_eq!(v2["delta"]["changed"]["count"], json!(0));
    assert_eq!(v2["delta"]["content_verification"], json!("full-rehash"));
    assert_eq!(v2["delta"]["delta_complete"], json!(true));

    // mixed-state disclosure (review intent-F1): one file really read
    // (size moved, so the stat gate MUST miss), the other served by the
    // gate ⇒ `stat-cache` — a partial rehash must not claim full.
    l.write("app.py", "x = 24\n");
    let (code3, v3) = l.verdict();
    assert_eq!(code3, 1, "{v3}");
    assert_eq!(v3["delta"]["changed"]["paths"], json!(["app.py"]));
    assert_eq!(
        v3["delta"]["content_verification"],
        json!("stat-cache"),
        "mixed state (one real read + one gate hit) is not full-rehash"
    );
    assert_eq!(v3["delta"]["delta_complete"], json!(true));
}

// ---------- review additions (fresh-F8 / intent-F5): not frozen TC rows —
// delta-full breadth and the walk-Err exit-2 priority face ----------

/// >50 changed files: the default JSON face bounds each path list to 50
/// while `count` stays the complete value; `--delta-full` enumerates all.
#[test]
fn delta_full_enumerates_all_paths_past_the_take50_bound() {
    let n = 61usize;
    let mut files: Vec<(String, &str)> = Vec::new();
    for i in 0..n {
        files.push((format!("m{i:02}.py"), "x = 1\n"));
    }
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    let l = lab(&refs);
    l.build();
    for i in 0..n {
        // always longer than the built "x = 1\n" — every file drifts
        l.write(&format!("m{i:02}.py"), &format!("x = {};  # v2\n", i + 100));
    }
    let (code, v) = l.verdict();
    assert_eq!(code, 1, "{v}");
    let changed = &v["delta"]["changed"];
    assert_eq!(
        changed["count"],
        json!(n),
        "count is the complete value on the default face"
    );
    assert_eq!(
        changed["paths"].as_array().map(Vec::len),
        Some(50),
        "the default face bounds the list to take-50"
    );
    let full = freshness_with(&l.repo, true, true);
    assert_eq!(full.exit_code, 1, "{}", full.stdout);
    let fv: Value = serde_json::from_str(full.stdout.trim()).unwrap();
    assert_eq!(fv["delta"]["changed"]["count"], json!(n));
    assert_eq!(
        fv["delta"]["changed"]["paths"].as_array().map(Vec::len),
        Some(n),
        "--delta-full enumerates past the bound"
    );
}

/// A governed file that cannot be read (mode 000) fails the identity
/// pass loud (D15): freshness exits 2 with NO verdict JSON — the walk
/// error outranks any stale/fresh answer.
#[test]
fn unreadable_governed_file_fails_loud_exit2_without_json() {
    use std::os::unix::fs::PermissionsExt;
    let l = lab(&[("app.py", "x = 1\n")]);
    l.build();
    let locked = l.repo.join("locked.py");
    std::fs::write(&locked, "s = 1\n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // root reads through mode 000 — the premise is unverifiable there.
    if std::fs::read(&locked).is_ok() {
        return;
    }
    let out = freshness(&l.repo, true);
    assert_eq!(
        out.exit_code, 2,
        "a read failure is fail-loud, never a degraded verdict: {:?}",
        out
    );
    assert!(
        serde_json::from_str::<Value>(out.stdout.trim()).is_err(),
        "no substitute verdict JSON on a check failure: {:?}",
        out.stdout
    );
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644));
}

// ---------- review N-1 (R3) addition: not a frozen TC row — the
// delta_face NotFound dispatch, both production-reachable legs ----------

/// A vanished manifest sidecar splits on the meta `base_manifest_digest`
/// pointer (fresh-F2). Both legs run in identity mode — the full
/// comparable triple is the only production shape that reaches the
/// NotFound branch (a keyless meta answers at the legacy early-return,
/// TC-16's shape) — and both pin `delta_complete == false`.
#[test]
fn manifest_sidecar_missing_dispatches_on_the_meta_pointer() {
    // leg 1 — pointer-present: the S2 stamp carried the pointer but the
    // sidecar vanished (slot retained) → `manifest-missing`. On a stale
    // base (the tc7 convention) "never flips fresh" is a positive
    // assertion: the loss degrades the delta face only, and the verdict
    // stays fail-closed stale.
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        assert!(
            l.meta()
                .get("base_manifest_digest")
                .and_then(|v| v.as_str())
                .is_some_and(|d| !d.is_empty()),
            "premise: the S2 stamp carries the pointer"
        );
        std::fs::remove_file(l.repo.join(MANIFEST_REL)).unwrap();
        assert!(l.slot().exists(), "premise: the slot itself is retained");
        stale_base(&l);
        let (code, v) = l.verdict();
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["fresh"], json!(false), "fail-closed: never flips fresh");
        assert_eq!(v["delta_unavailable_reason"], json!("manifest-missing"));
        assert_eq!(v["delta"]["delta_complete"], json!(false));
        assert_eq!(v["delta"]["changed"], Value::Null, "no substitute answer");
        assert!(
            v["delta_identity"].is_null(),
            "no sidecar bytes, no recomposed identity"
        );
    }

    // leg 2 — pointer-absent: a pre-S2 v2-shaped slot (the full
    // comparable triple, no pointer, no sidecar — the shape every
    // deployed repo carries today) → `legacy-no-manifest`, never
    // `manifest-missing`; with the identity intact the verdict axis is
    // untouched (fresh still serves current-tree).
    {
        let l = lab(&[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ]);
        l.build();
        // Hand-built pre-S2 meta (tc16's wholesale-overwrite form): the
        // S2 stamp writes exactly the pre-S2 key set plus the pointer,
        // so stripping the pointer reproduces the deployed generation's
        // meta byte-shape.
        let mut m = l.meta();
        assert!(
            m.as_object_mut()
                .unwrap()
                .remove("base_manifest_digest")
                .is_some(),
            "premise: the pointer was there to strip"
        );
        l.write_meta(&m);
        std::fs::remove_file(l.repo.join(MANIFEST_REL)).unwrap();
        let (code, v) = l.verdict();
        assert_eq!(code, 0, "{v}");
        assert_eq!(v["fresh"], json!(true));
        assert_eq!(v["serves"], json!("current-tree"));
        assert_eq!(v["delta_unavailable_reason"], json!("legacy-no-manifest"));
        assert_eq!(v["delta"]["delta_complete"], json!(false));
        assert_eq!(v["delta"]["changed"], Value::Null, "no substitute answer");
    }
}
