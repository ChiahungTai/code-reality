//! Multi-Cargo-workspace root resolver (EP DB-100 S1, frozen TC-8/TC-11).
//!
//! Contract under test: `rust_projects::resolve_structural` (pure TOML —
//! the sole query/freshness-face source, never spawns cargo metadata) and
//! `rust_projects::validate_authoritative` (cargo-metadata workspace-root
//! check, build staging only). Error codes CR-RUST-ROOT-01..08 are a
//! frozen contract (EP TC-8 mapping table).
//!
//! CR-19 extends the auto face with bounded deep-layout discovery
//! (crates/<name>/, apps/<x>/src-tauri): when the repo root carries no
//! Cargo.toml AND the immediate-child scan finds zero candidates, a
//! depth <= 3 recursive scan classifies manifests — `[workspace]` =>
//! root, bare `[package]` under a workspace ancestor => member, bare
//! `[package]` with no ancestor => standalone root (the mosaic_alpha
//! oracle mixes both kinds). Depth-1 candidate behavior is
//! byte-unchanged.

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
    write(
        tmp.path(),
        PROFILE_FILENAME,
        &profile_toml(&["rust", "dutymail"]),
    );
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
    let err =
        code_reality::rust_projects::validate_authoritative(tmp.path(), &projects).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-05"), "{err}");
    assert!(err.contains("workspace_root="), "{err}");
}

// --- CR-19: bounded deep-layout auto discovery ---

#[test]
fn auto_deep_two_level_workspace_layout_multi_root() {
    // crates/<name>/ (cargo book convention, no repo-root manifest): every
    // deep [workspace] manifest is a root — the mosaic_alpha shape that
    // used to fall through to the "." fallback and die in rust-analyzer
    // with "no projects".
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/beta/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/alpha/Cargo.toml", workspace_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(
        roots,
        vec!["crates/alpha".to_string(), "crates/beta".to_string()]
    );
}

#[test]
fn auto_deep_third_level_workspace_discovered() {
    // apps/<x>/src-tauri = depth 3 — inside the cap.
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "apps/shell/src-tauri/Cargo.toml",
        workspace_manifest(),
    );
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["apps/shell/src-tauri".to_string()]);
}

#[test]
fn auto_deep_mixed_kinds_accepted() {
    // The mosaic oracle shape: [workspace] roots AND standalone
    // [package] roots in one discovery run — the strict depth-1 "mixed
    // candidates" 06 would deadlock real consumers, so the deep face
    // accepts mixed root kinds (crates/mosaic-strategy-kc and
    // apps/shell_tauri/src-tauri carry no [workspace]).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/pkg/Cargo.toml", package_manifest());
    write(
        tmp.path(),
        "apps/shell/src-tauri/Cargo.toml",
        package_manifest(),
    );
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(
        roots,
        vec![
            "apps/shell/src-tauri".to_string(),
            "crates/pkg".to_string(),
            "crates/ws".to_string(),
        ]
    );
}

#[test]
fn auto_deep_package_under_workspace_is_member_not_root() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "crates/ws/Cargo.toml",
        "[workspace]\nmembers = [\"inner\"]\n",
    );
    write(tmp.path(), "crates/ws/inner/Cargo.toml", package_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["crates/ws".to_string()]);
}

#[test]
fn auto_deep_package_under_bare_package_ambiguous_06() {
    // A bare [package] whose ancestor chain holds ONLY non-workspace
    // manifests: no [workspace] can own it as a member (a bare package
    // owns nothing), while treating it as a root would overlap its
    // package ancestor — ownership is ambiguous, fail loud 06 instead of
    // silently classifying it as a member nobody covers.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/pkg/Cargo.toml", package_manifest());
    write(tmp.path(), "a/pkg/sub/Cargo.toml", package_manifest());
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-06"), "{err}");
    assert!(err.contains("a/pkg/sub"), "{err}");
}

#[test]
fn auto_deep_nested_workspaces_overlap_03() {
    // [workspace] ⇒ root unconditionally: a workspace nested inside
    // another workspace yields two ancestor/descendant roots — the
    // overlap the 03 gate exists for.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/outer/Cargo.toml", workspace_manifest());
    write(
        tmp.path(),
        "crates/outer/nested/Cargo.toml",
        workspace_manifest(),
    );
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-03"), "{err}");
    assert!(err.contains("crates/outer"), "{err}");
}

#[test]
fn auto_deep_invalid_toml_fails_loud_04() {
    // Depth-1 semantics carried into the deep scan: a readable but
    // invalid manifest is a hard 04 — a scan failure must never fold
    // into a silent zero-candidate answer.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/good/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/bad/Cargo.toml", "not [valid toml");
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-04"), "{err}");
    assert!(err.contains("crates/bad"), "{err}");
}

#[test]
fn auto_deep_profile_excluded_manifest_not_candidate() {
    // Profile exclude constrains the discovery range: a deep workspace
    // under an excluded path contributes no candidate (poc/ trees stay
    // out of governance without any global vendor hardcoding).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "poc/deep/Cargo.toml", workspace_manifest());
    write(tmp.path(), PROFILE_FILENAME, "exclude = [\"poc/\"]\n");
    let profile = load_profile(tmp.path()).unwrap().unwrap();
    let roots = resolve_structural(tmp.path(), Some(&profile))
        .unwrap()
        .roots;
    assert_eq!(roots, vec!["crates/ws".to_string()]);
}

#[test]
fn auto_deep_skips_target_node_modules_and_dot_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "target/pkg/Cargo.toml", workspace_manifest());
    write(
        tmp.path(),
        "node_modules/x/Cargo.toml",
        workspace_manifest(),
    );
    write(tmp.path(), ".hidden/y/Cargo.toml", workspace_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert!(roots.is_empty(), "skip list: {roots:?}");
}

#[test]
fn auto_deep_fourth_level_manifest_not_collected() {
    // Depth cap 3: a/b/c/d is the 4th level — out of range, never a
    // candidate (paired with auto_deep_third_level_workspace_discovered
    // above, which pins the inclusive boundary).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "a/b/c/d/Cargo.toml", workspace_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert!(roots.is_empty(), "depth cap: {roots:?}");
}

#[test]
fn auto_deep_unreadable_manifest_skipped() {
    // Frozen deep-scan rule: a manifest that cannot be READ is skipped
    // (only readable-but-invalid TOML is a fatal 04) — unlike the
    // depth-1 face, where a candidate manifest's read error is fatal.
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/locked/Cargo.toml", workspace_manifest());
    let locked = tmp.path().join("crates/locked/Cargo.toml");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Guard the fixture itself: a privileged runner (root) reads
    // through 0o000 and the test would pin nothing.
    if std::fs::read_to_string(&locked).is_ok() {
        eprintln!("skip: privileged runner (chmod 000 still readable)");
        return;
    }
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["crates/ws".to_string()]);
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644));
}

// --- CR-19: config face over deep layouts (07 inventory alignment) ---

#[test]
fn config_deep_roots_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/alpha/Cargo.toml", workspace_manifest());
    let roots = resolve_with_profile(tmp.path(), &["crates/alpha"]).unwrap();
    assert_eq!(roots, vec!["crates/alpha".to_string()]);
}

#[test]
fn config_unlisted_deep_workspace_rejected_07() {
    // The 07 inventory gate scans the SAME deep range as the auto face:
    // a deep sibling [workspace] missing from rust_projects is a
    // coverage hole, not silently ignored.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/alpha/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/beta/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["crates/alpha"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-07"), "{err}");
    assert!(err.contains("crates/beta"), "{err}");
}

#[test]
fn config_deep_members_do_not_trip_07() {
    // The 07 gate keys on the [workspace] table — bare member manifests
    // under a CONFIGURED deep root are expected layout, not holes.
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "crates/ws/Cargo.toml",
        "[workspace]\nmembers = [\"inner\"]\n",
    );
    write(tmp.path(), "crates/ws/inner/Cargo.toml", package_manifest());
    let roots = resolve_with_profile(tmp.path(), &["crates/ws"]).unwrap();
    assert_eq!(roots, vec!["crates/ws".to_string()]);
}

// --- CR-19 review batch: overlap-gate completeness, scan fail-loud, pins ---

#[test]
fn auto_deep_overlap_gate_non_adjacent_pair_caught_03() {
    // String byte order puts "crates/a-c" ('-' = 0x2D) BETWEEN the
    // ancestor "crates/a" and the descendant "crates/a/sub" ('/' =
    // 0x2F), so a windows(2) gate over byte-sorted roots never pairs
    // them — the overlap must be caught under component-wise Path
    // order, where an ancestor is always adjacent to its minimal
    // descendant.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/a/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/a-b/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/a/sub/Cargo.toml", workspace_manifest());
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-03"), "{err}");
    assert!(err.contains("crates/a"), "{err}");
}

#[test]
fn auto_deep_prefix_similar_disjoint_roots_accepted() {
    // The flip side of the overlap gate: "crates/a-b" shares a string
    // prefix with "crates/a" but is NOT a path descendant — both
    // standalone roots must survive (the Path-order gate must not
    // over-reject prefix-similar names).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/a/Cargo.toml", package_manifest());
    write(tmp.path(), "crates/a-b/Cargo.toml", package_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(
        roots,
        vec!["crates/a".to_string(), "crates/a-b".to_string()]
    );
}

#[test]
fn auto_deep_unreadable_directory_fails_loud() {
    // Enumeration failure is NOT the frozen "unreadable manifest" skip:
    // a directory the scan cannot LIST means discovery is broken, not
    // empty — fail loud with the path instead of silently returning
    // zero candidates (the exact silent-zero shape the review banned).
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "crates/locked/Cargo.toml", workspace_manifest());
    let locked = tmp.path().join("crates/locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Guard the fixture: a privileged runner (root) lists 0o000 dirs
    // and the test would pin nothing.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skip: privileged runner (chmod 000 dir still listable)");
        return;
    }
    let err = resolve_auto(tmp.path()).unwrap_err();
    assert!(err.contains("crates/locked"), "path context: {err}");
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
}

#[test]
fn auto_deep_excluded_unreadable_subtree_pruned_not_fatal() {
    // Exclusion = out of governance: a profile-excluded subtree the
    // scan could not LIST anyway (permission denied) must not abort
    // discovery — the traversal prunes excluded directories before
    // descending, so an ungoverned subtree's permission errors never
    // reach the fail-loud enumeration path. The mirror of
    // auto_deep_unreadable_directory_fails_loud above, which pins the
    // NOT-excluded direction.
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "crates/ws/Cargo.toml", workspace_manifest());
    write(tmp.path(), "poc/private/Cargo.toml", workspace_manifest());
    let locked = tmp.path().join("poc");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Guard the fixture: a privileged runner (root) lists 0o000 dirs
    // and the test would pin nothing.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skip: privileged runner (chmod 000 dir still listable)");
        return;
    }
    write(tmp.path(), PROFILE_FILENAME, "exclude = [\"poc/\"]\n");
    let profile = load_profile(tmp.path()).unwrap().unwrap();
    let roots = resolve_structural(tmp.path(), Some(&profile))
        .unwrap()
        .roots;
    assert_eq!(roots, vec!["crates/ws".to_string()]);
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
}

#[test]
fn auto_depth1_candidate_wins_over_deep_workspaces() {
    // Trigger-pin: the deep scan only runs when the immediate-child
    // scan finds ZERO candidates — a depth-1 candidate keeps the
    // depth-1 face byte-unchanged and the deep [workspace] never
    // participates (deep dead routes route to ROOT-09 remediation at
    // the build face, per the frozen compatibility strategy).
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "only/Cargo.toml", package_manifest());
    write(tmp.path(), "crates/deep/Cargo.toml", workspace_manifest());
    let roots = resolve_auto(tmp.path()).unwrap();
    assert_eq!(roots, vec!["only".to_string()]);
}

#[test]
fn config_unlisted_root_workspace_rejected_07() {
    // The deep-aligned 07 inventory scan also sees the repo-ROOT
    // [workspace] manifest: a config that lists something else while a
    // root workspace exists is a coverage hole (the message names the
    // root as '.').
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Cargo.toml", workspace_manifest());
    write(tmp.path(), "sub/Cargo.toml", workspace_manifest());
    let err = resolve_with_profile(tmp.path(), &["sub"]).unwrap_err();
    assert!(err.contains("CR-RUST-ROOT-07"), "{err}");
    assert!(err.contains("'.'"), "root named as '.': {err}");
}

#[test]
fn config_excluded_deep_workspace_not_07() {
    // Profile exclude constrains the 07 inventory to the same range as
    // the auto face: a [workspace] under an excluded path is out of
    // governance and must not trip the gate.
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "sub/Cargo.toml", workspace_manifest());
    write(tmp.path(), "poc/deep/Cargo.toml", workspace_manifest());
    write(
        tmp.path(),
        PROFILE_FILENAME,
        "exclude = [\"poc/\"]\nrust_projects = [\"sub\"]\n",
    );
    let profile = load_profile(tmp.path()).unwrap().unwrap();
    let roots = resolve_structural(tmp.path(), Some(&profile)).unwrap();
    assert_eq!(roots.roots, vec!["sub".to_string()]);
}
