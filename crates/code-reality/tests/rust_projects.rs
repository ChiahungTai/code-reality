//! Multi-Cargo-workspace root resolver (EP DB-100 S1, frozen TC-8/TC-11).
//!
//! Contract under test: `rust_projects::resolve_structural` (pure TOML —
//! the sole query/freshness-face source, never spawns cargo metadata) and
//! `rust_projects::validate_authoritative` (cargo-metadata workspace-root
//! check, build staging only). Error codes CR-RUST-ROOT-01..08 are a
//! frozen contract (EP TC-8 mapping table).

use code_reality::profile::{load_profile, PROFILE_FILENAME};
use code_reality::rust_projects::resolve_structural;
use std::path::Path;

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, text).unwrap();
}

fn workspace_manifest() -> &'static str {
    "[workspace]\nresolver = \"2\"\nmembers = []\n"
}

fn package_manifest() -> &'static str {
    "[package]\nname = \"standalone\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
}

fn profile_toml(roots: &[&str]) -> String {
    let items: Vec<String> = roots.iter().map(|r| format!("\"{r}\"")).collect();
    format!("rust_projects = [{}]\n", items.join(", "))
}

fn resolve_with_profile(repo: &Path, roots: &[&str]) -> Result<Vec<String>, String> {
    write(repo, PROFILE_FILENAME, &profile_toml(roots));
    let profile = load_profile(repo).unwrap().unwrap();
    resolve_structural(repo, Some(&profile)).map(|p| p.roots)
}

fn resolve_auto(repo: &Path) -> Result<Vec<String>, String> {
    resolve_structural(repo, None).map(|p| p.roots)
}

// --- Q1 config mode (TC-8: 01..04, 07) ---

#[test]
fn config_dual_workspaces_resolve_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "beta/Cargo.toml", workspace_manifest());
    write(tmp.path(), "alpha/Cargo.toml", workspace_manifest());
    let roots = resolve_with_profile(tmp.path(), &["beta", "alpha"]).unwrap();
    assert_eq!(roots, vec!["alpha".to_string(), "beta".to_string()]);
}

#[test]
fn config_dot_root_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Cargo.toml", workspace_manifest());
    let roots = resolve_with_profile(tmp.path(), &["."]).unwrap();
    assert_eq!(roots, vec![".".to_string()]);
}

#[test]
fn config_absolute_path_rejected_01() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["/etc"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-01"), "{err}");
}

#[test]
fn config_dotdot_component_rejected_01() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["a/../b"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-01"), "{err}");
}

#[test]
fn config_empty_entry_rejected_01() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve_with_profile(tmp.path(), &[""]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-01"), "{err}");
}

#[test]
fn config_empty_array_rejected_01() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve_with_profile(tmp.path(), &[]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-01"), "{err}");
}

#[test]
fn config_missing_dir_rejected_02() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve_with_profile(tmp.path(), &["ghost"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-02"), "{err}");
}

#[test]
fn config_escape_via_symlink_rejected_02() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "Cargo.toml", workspace_manifest());
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("link")).unwrap();
    let err = resolve_with_profile(tmp.path(), &["link"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-02"), "{err}");
}

#[test]
fn config_overlapping_roots_rejected_03() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/Cargo.toml", workspace_manifest());
    write(tmp.path(), "a/b/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["a", "a/b"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-03"), "{err}");
}

#[test]
fn config_alias_duplicate_rejected_03() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["a", "./a"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-03"), "{err}");
}

#[test]
fn config_missing_manifest_rejected_04() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "empty/keep.txt", "x");
    let err = resolve_with_profile(tmp.path(), &["empty"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-04"), "{err}");
}

#[test]
fn config_broken_manifest_rejected_04() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "bad/Cargo.toml", "not [valid toml");
    let err = resolve_with_profile(tmp.path(), &["bad"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-04"), "{err}");
}

// --- fresh-leg findings: P1 cosmetic-entry normalization ---

#[test]
fn config_cosmetic_forms_normalized() {
    // P1: cosmetic entry spellings fold to the canonical repo-relative
    // form at the single resolution point — downstream (walk-side rel
    // prefixing, the Q7 coverage gate, the topology marker) never sees
    // "./rust" or a trailing slash.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "rust/Cargo.toml", workspace_manifest());
    write(tmp.path(), "dutymail/Cargo.toml", workspace_manifest());
    let roots = resolve_with_profile(tmp.path(), &["./rust/", "dutymail"]).unwrap();
    assert_eq!(roots, vec!["dutymail".to_string(), "rust".to_string()]);
}

// --- Q1 auto mode (TC-8: 06 + zero-roots contract) ---

#[test]
fn auto_root_manifest_without_siblings_is_dot() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Cargo.toml", workspace_manifest());
    write(tmp.path(), "src/lib.rs", "fn f() {}\n");
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec![".".to_string()]);
}

#[test]
fn auto_root_manifest_with_sibling_workspace_rejected_06() {
    // EP Q1: "." only when no other independent workspace exists — a
    // sibling workspace beside a root manifest is silent coverage loss
    // (the exact bug class DB-100 fixes), so it fails loud. Config
    // cannot express this layout (["."] trips 07, [".", "sib"] trips 03)
    // — failing is the terminal state, per the frozen advisory.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Cargo.toml", workspace_manifest());
    write(tmp.path(), "sibling/Cargo.toml", workspace_manifest());
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-06"), "{err}");
    assert!(err.contains("sibling"), "{err}");
}

#[test]
fn auto_discovers_two_explicit_workspaces() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "rust/Cargo.toml", workspace_manifest());
    write(tmp.path(), "dutymail/Cargo.toml", workspace_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["dutymail".to_string(), "rust".to_string()]);
}

#[test]
fn auto_zero_candidates_empty_roots() {
    // SM-2b contract: resolve returns an EMPTY root set (Ok) — the
    // legacy-parity fallback (single "." root) is the CALLER's decision.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "src/lib.rs", "fn f() {}\n");
    let roots = resolve_auto(tmp.path()).unwrap();
    assert!(roots.is_empty());
}

#[test]
fn auto_single_implicit_package_accepted() {
    // Historical one-child standalone [package] case — accepted at the
    // structural layer; workspace-root identity is the build face's
    // authoritative check (TC-11: cargo metadata confirms standalone
    // packages are their own workspace root).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "only/Cargo.toml", package_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["only".to_string()]);
}

#[test]
fn auto_ambiguous_mixed_children_rejected_06() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "ws1/Cargo.toml", workspace_manifest());
    write(tmp.path(), "ws2/Cargo.toml", workspace_manifest());
    write(tmp.path(), "pkg/Cargo.toml", package_manifest());
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-06"), "{err}");
    assert!(err.contains("rust_projects"), "{err}");
}

#[test]
fn auto_two_implicit_packages_rejected_06() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "p1/Cargo.toml", package_manifest());
    write(tmp.path(), "p2/Cargo.toml", package_manifest());
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-06"), "{err}");
}

// --- Q1 config-mode inventory (TC-8: 07) ---

#[test]
fn config_unlisted_third_workspace_rejected_07() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "rust/Cargo.toml", workspace_manifest());
    write(tmp.path(), "dutymail/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["rust"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-07"), "{err}");
    assert!(err.contains("dutymail"), "{err}");
}

// --- deterministic ordering (TC-5d resolver face) ---

#[test]
fn config_order_irrelevant_sorted_output() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "b/Cargo.toml", workspace_manifest());
    write(tmp.path(), "a/Cargo.toml", workspace_manifest());
    write(tmp.path(), "c/Cargo.toml", workspace_manifest());
    let r1 = resolve_with_profile(tmp.path(), &["b", "a", "c"]).unwrap();
    let r2 = resolve_with_profile(tmp.path(), &["c", "b", "a"]).unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r1, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

// --- profile layer drift (review F10: legal-key message carries the new key) ---

#[test]
fn config_dot_root_with_package_members_no_false_07() {
    // Root-workspace repos carry member crates (bare [package] manifests)
    // under the root — the unlisted-workspace scan keys on the
    // [workspace] TABLE, not on Cargo.toml presence, so members must not
    // trip 07. Pins the signal choice against future loosening.
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"m1\", \"m2\"]\n",
    );
    write(tmp.path(), "m1/Cargo.toml", package_manifest());
    write(tmp.path(), "m2/Cargo.toml", package_manifest());
    let roots = resolve_with_profile(tmp.path(), &["."]).unwrap();
    assert_eq!(roots, vec![".".to_string()]);
}

#[test]
fn unknown_key_message_lists_rust_projects() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), PROFILE_FILENAME, "rust_project = [\"a\"]\n");
    let err = load_profile(tmp.path()).unwrap_err();
    assert!(err.contains("rust_projects"), "{err}");
}

#[test]
fn config_non_string_entry_is_profile_schema_error() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), PROFILE_FILENAME, "rust_projects = [1]\n");
    assert!(load_profile(tmp.path()).is_err());
}

#[test]
fn config_non_array_is_profile_schema_error() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), PROFILE_FILENAME, "rust_projects = \"a\"\n");
    assert!(load_profile(tmp.path()).is_err());
}

// --- fresh-leg findings: P2 discovery-mode truth ---

#[test]
fn auto_vs_config_mode_differs() {
    // P2: the resolver carries its discovery mode as a truth value —
    // identical roots under both faces differ ONLY in `mode`, and the
    // walk-side topology marker consumes that value (no "structural"
    // constant).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "rust/Cargo.toml", workspace_manifest());
    write(tmp.path(), "dutymail/Cargo.toml", workspace_manifest());
    let auto = resolve_structural(tmp.path(), None).unwrap();
    write(tmp.path(), PROFILE_FILENAME, &profile_toml(&["rust", "dutymail"]));
    let profile = load_profile(tmp.path()).unwrap().unwrap();
    let config = resolve_structural(tmp.path(), Some(&profile)).unwrap();
    assert_eq!(auto.roots, config.roots, "same fixture ⇒ same roots");
    assert_eq!(auto.mode, "auto");
    assert_eq!(config.mode, "config");
    assert_ne!(auto.mode, config.mode);
}

// --- Q8 authoritative face (TC-11: real cargo metadata) ---

#[test]
fn validate_authoritative_accepts_workspace_and_standalone() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "pkg/Cargo.toml", package_manifest());
    write(tmp.path(), "pkg/src/lib.rs", "fn f() {}\n");
    let projects = code_reality::rust_projects::RustProjects {
        roots: vec!["ws".to_string(), "pkg".to_string()],
        mode: "config",
    };
    code_reality::rust_projects::validate_authoritative(tmp.path(), &projects).unwrap();
}

#[test]
fn validate_authoritative_rejects_member_as_root_05() {
    // child is a MEMBER of the parent workspace (workspace_root resolves
    // to the parent) — declaring it as an independent root must fail 05.
    // The fixture carries src/lib.rs so cargo metadata PARSES (no targets
    // would die in the stderr branch before the workspace_root mismatch
    // comparison — the branch this test exists to pin).
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "parent/Cargo.toml",
        "[workspace]\nmembers = [\"child\"]\n",
    );
    write(tmp.path(), "parent/child/Cargo.toml", package_manifest());
    write(tmp.path(), "parent/child/src/lib.rs", "fn f() {}\n");
    let projects = code_reality::rust_projects::RustProjects {
        roots: vec!["parent/child".to_string()],
        mode: "config",
    };
    let err = code_reality::rust_projects::validate_authoritative(tmp.path(), &projects)
        .unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-05"), "{err}");
    assert!(err.contains("workspace_root="), "{err}");
}
