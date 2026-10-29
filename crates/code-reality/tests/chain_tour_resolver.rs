//! CR-20 Leg B: PathResolver four-stage resolution tests. The module
//! segment (exact direct → suffix pool → basename pool → ctx majority)
//! is frozen — these tests pin it — while the repo segment
//! (repo-relative exact direct → lazily indexed repo filename pool)
//! rescues frames whose files live outside every `[[module]]` prefix.
//! Resolver-level coverage is new: the module was previously exercised
//! only indirectly through `build_tours`.

use code_reality::chain_tour::{build_tours, run, PathResolver};
use std::path::PathBuf;

/// Plant a synthetic repo: `files` are (repo-relative path, content)
/// pairs; `profile` (optional) becomes `.code-reality.toml`.
fn write_repo(tag: &str, files: &[(&str, &str)], profile: Option<&str>) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join(tag);
    for (rel, content) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    if let Some(toml) = profile {
        std::fs::write(repo.join(".code-reality.toml"), toml).unwrap();
    }
    repo
}

const PKG_PROFILE: &str = "[[module]]\nprefix = \"pkg/\"\n";

/// Stage 1 pin: module-exact direct beats a same-basename file anywhere
/// outside the module (frozen face — the direct loop never changes).
#[test]
fn module_exact_wins_over_same_basename_outside_the_module() {
    let repo = write_repo(
        "module-exact",
        &[
            ("pkg/name.py", "def f():\n    pass\n"),
            ("other/name.py", "def g():\n    pass\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("name.py");
    assert_eq!(kind, "direct");
    assert_eq!(p.unwrap(), repo.join("pkg/name.py"));
}

/// Stage 2: a full repo-relative frame names the repo file directly —
/// even though the module prefix owns none of it (the mosaic rust/TS
/// frame shape: `crates/...`, `web/...`).
#[test]
fn repo_relative_full_path_frame_reaches_the_repo_file() {
    let repo = write_repo(
        "repo-direct",
        &[("crates/x/src/lib.rs", "pub fn f() {}\n")],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("crates/x/src/lib.rs");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("crates/x/src/lib.rs"));
}

/// Stage-2-before-stage-3 pin (the basename false-grab counter-example):
/// with a same-basename `lib.rs` inside the module, the old pool fell
/// back to basename matching and could never reach the correct repo
/// file. The repo-relative exact stage resolves first; module-local
/// basename resolution is untouched for module-relative frames.
#[test]
fn repo_relative_frame_beats_module_basename_pool() {
    let repo = write_repo(
        "basename-grab",
        &[
            ("pkg/lib.rs", "pub mod m;\n"),
            ("crates/sj/src/lib.rs", "pub fn real() {}\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("crates/sj/src/lib.rs");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("crates/sj/src/lib.rs"));
    let (p, kind) = r.resolve("lib.rs");
    assert_eq!(kind, "direct");
    assert_eq!(p.unwrap(), repo.join("pkg/lib.rs"));
}

/// Stage-3 terminal pin: a module-segment ambiguity is final — the repo
/// segment must never second-guess it (fail-closed over a double-
/// referent guess; a same-basename file outside the module stays
/// unreachable from the ambiguous frame).
#[test]
fn module_ambiguous_is_terminal_and_never_enters_the_repo_pool() {
    let repo = write_repo(
        "module-ambiguous",
        &[
            ("pkg/a/dup.py", "x = 1\n"),
            ("pkg/b/dup.py", "x = 2\n"),
            ("other/dup.py", "x = 3\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("dup.py");
    assert_eq!(kind, "ambiguous");
    assert!(p.is_none());
}

/// Exclusion consistency (the module/direct asymmetry stays frozen; the
/// repo segment is corpus rescue, and corpus includes the exclusion
/// layer): excluded paths are refused on the repo-direct arm and never
/// enter the repo pool — the frame keeps its external verdict.
#[test]
fn excluded_paths_stay_out_of_the_repo_segment() {
    let repo = write_repo(
        "excluded",
        &[
            ("pkg/a.py", "def f():\n    pass\n"),
            ("third_party/x/util.py", "y = 1\n"),
        ],
        Some("exclude = [\"third_party/\"]\n\n[[module]]\nprefix = \"pkg/\"\n"),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("third_party/x/util.py");
    assert_eq!(kind, "none");
    assert!(p.is_none());
    let (p, kind) = r.resolve("util.py");
    assert_eq!(kind, "none");
    assert!(p.is_none());
}

/// Stage 4 pool hygiene: `node_modules` is pruned at index build, so a
/// decoy same-basename file under it can neither steal the hit (a repo
/// copy exists → unique) nor be adopted when it is the only copy
/// (external preserved).
#[test]
fn node_modules_decoys_never_enter_the_repo_pool() {
    let repo = write_repo(
        "node-decoy",
        &[
            ("web/node_modules/leftpad/util.ts", "export const x = 1;\n"),
            ("web/src/util.ts", "export const y = 2;\n"),
            ("node_modules/only/util.py", "z = 1\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("util.ts");
    assert_eq!(kind, "repo-suffix");
    assert_eq!(p.unwrap(), repo.join("web/src/util.ts"));
    let (p, kind) = r.resolve("util.py");
    assert_eq!(kind, "none");
    assert!(p.is_none());
}

/// Stage 4 decision shape: a multi-candidate pool needs a UNIQUE winner —
/// zero ctx evidence or a ctx tie refuses (`ambiguous`, fail-closed);
/// a prior repo-segment hit in one candidate's directory breaks the tie
/// through the same ctx continuity (`bump_dir`) the module segment uses.
#[test]
fn repo_pool_tie_requires_a_unique_ctx_winner() {
    let repo = write_repo(
        "repo-tie",
        &[
            ("web/src/dup2.ts", "export const a = 1;\n"),
            ("lib/client/dup2.ts", "export const b = 2;\n"),
            ("web/src/other.ts", "export const c = 3;\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("dup2.ts");
    assert_eq!(kind, "ambiguous");
    assert!(p.is_none());
    // ctx continuity: a repo-direct hit in web/src/ tips the majority
    let (p, kind) = r.resolve("web/src/other.ts");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("web/src/other.ts"));
    let (p, kind) = r.resolve("dup2.ts");
    assert_eq!(kind, "repo-suffix");
    assert_eq!(p.unwrap(), repo.join("web/src/dup2.ts"));
}

/// R1 regression (repair round 1): a POSITIVE ctx tie between the top
/// two candidates must refuse. The unique-winner check must compare the
/// top TWO scores — after `swap_remove(0)` the tail element sits in
/// slot 0, so a post-removal best-vs-`scored[0]` comparison weighs best
/// against WORST and lets a lexicographic-first pick through a tie.
#[test]
fn repo_pool_positive_ctx_tie_among_top_two_refuses() {
    let repo = write_repo(
        "repo-tie-top2",
        &[
            ("web/src/dup3.ts", "export const a = 1;\n"),
            ("lib/client/dup3.ts", "export const b = 2;\n"),
            ("other/dup3.ts", "export const c = 3;\n"),
            ("web/src/lead1.ts", "export const x = 1;\n"),
            ("lib/client/lead2.ts", "export const y = 2;\n"),
        ],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    // web/src and lib/client each carry exactly one prior hit — the top
    // ctx score is SHARED by two dup3.ts candidates, the third scores
    // lower
    let (p, kind) = r.resolve("web/src/lead1.ts");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("web/src/lead1.ts"));
    let (p, kind) = r.resolve("lib/client/lead2.ts");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("lib/client/lead2.ts"));
    let (p, kind) = r.resolve("dup3.ts");
    assert_eq!(kind, "ambiguous", "top-two positive ctx tie must refuse");
    assert!(p.is_none());
}

/// Stage-2 hygiene (R2, repair round 1): a repo-relative frame that
/// escapes the repo root (`../`) never wins — containment is checked on
/// the canonical path. The escape target is planted as a REAL sibling
/// of the repo (same temp parent), so the frame's `is_file` probe
/// succeeds and the canonical containment check is genuinely exercised
/// (the previous fixture planted the target under an unrelated temp
/// root — `is_file` refused first and the check was never reached).
#[test]
fn repo_direct_refuses_paths_escaping_the_repo_root() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let root = std::fs::canonicalize(&tmp).unwrap();
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(repo.join("pkg/a.py"), "def f():\n    pass\n").unwrap();
    std::fs::write(repo.join(".code-reality.toml"), PKG_PROFILE).unwrap();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let leak = outside.join("leak.py");
    std::fs::write(&leak, "x = 1\n").unwrap();
    // non-vacuous precondition: the escape target really exists at the
    // frame's spelling repo/../outside/leak.py
    assert!(repo.join("../outside/leak.py").exists());
    assert_eq!(
        std::fs::canonicalize(repo.join("../outside/leak.py")).unwrap(),
        std::fs::canonicalize(&leak).unwrap(),
        "escape target must be the true sibling file"
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("../outside/leak.py");
    assert_eq!(kind, "none");
    assert!(p.is_none());
}

/// Stage-2 hygiene, symlink leg (R2, repair round 1): an in-repo
/// symlink naming an EXISTING outside file — canonicalization resolves
/// the link to the outside entity and the containment check must refuse
/// there (a symlink may not smuggle an outside file into the corpus).
#[cfg(unix)]
#[test]
fn symlink_pointing_outside_the_repo_is_refused() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let root = std::fs::canonicalize(&tmp).unwrap();
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(repo.join("pkg/a.py"), "def f():\n    pass\n").unwrap();
    std::fs::write(repo.join(".code-reality.toml"), PKG_PROFILE).unwrap();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let leak = outside.join("leak.py");
    std::fs::write(&leak, "x = 1\n").unwrap();
    std::os::unix::fs::symlink(&leak, repo.join("link.py")).unwrap();
    // the link names a real, existing outside entity
    assert!(repo.join("link.py").exists());
    assert_eq!(
        std::fs::canonicalize(repo.join("link.py")).unwrap(),
        std::fs::canonicalize(&leak).unwrap()
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("link.py");
    assert_eq!(
        kind, "none",
        "canonical containment must refuse the outside entity"
    );
    assert!(p.is_none());
}

/// Canonicalization pin (fill_anchor composes `{repo_root}/{rel}` from
/// the returned path and matches it against the graph's canonical
/// entries): a `./` segment in the frame resolves to the canonical file.
#[test]
fn dot_segment_frame_resolves_to_the_canonical_path() {
    let repo = write_repo(
        "canonical",
        &[("crates/x/src/lib.rs", "pub fn f() {}\n")],
        Some(PKG_PROFILE),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("./crates/x/src/lib.rs");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("crates/x/src/lib.rs"));
}

/// Canonicalization pin, symlink leg: the returned path canonicalizes
/// through the link so the graph's stored abs path is matched, not the
/// link spelling.
#[cfg(unix)]
#[test]
fn symlinked_frame_resolves_to_the_canonical_file() {
    let repo = write_repo(
        "symlink",
        &[("crates/x/src/lib.rs", "pub fn f() {}\n")],
        Some(PKG_PROFILE),
    );
    std::os::unix::fs::symlink(repo.join("crates/x/src/lib.rs"), repo.join("alias.rs")).unwrap();
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("alias.rs");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("crates/x/src/lib.rs"));
}

/// End-to-end observability (AC7): repo-segment rescues are counted per
/// kind on `ScenarioTours` and surface as one `[LOG]` line in run
/// output — the mosaic oracle's measurable delta.
#[test]
fn build_tours_counts_repo_rescues_and_run_emits_the_log_line() {
    let files: Vec<(&str, &str)> = vec![
        (
            "chain.md",
            "# S\n\n```text\nboot() (crates/x/src/lib.rs:1)\n└─ use_util() (util.ts:1)\n```\n",
        ),
        ("crates/x/src/lib.rs", "pub fn boot() {}\n"),
        ("web/src/util.ts", "export function use_util() {}\n"),
    ];
    let repo = write_repo("rescue-count", &files, Some(PKG_PROFILE));
    let chain = repo.join("chain.md");
    let st = build_tours(&chain, &repo, None).unwrap();
    assert_eq!(
        st.repo_rescues.get("repo-direct"),
        Some(&1),
        "{:?}",
        st.repo_rescues
    );
    assert_eq!(
        st.repo_rescues.get("repo-suffix"),
        Some(&1),
        "{:?}",
        st.repo_rescues
    );
    assert_eq!(st.tours[0]["steps"].as_array().unwrap().len(), 2);
    let out_dir = repo.join(".tours/arch/01-rescue");
    let out = run(&[
        "chain_tour",
        &chain.to_string_lossy(),
        "--repo",
        &repo.to_string_lossy(),
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(
        out.exit_code, 0,
        "stdout={} stderr={}",
        out.stdout, out.stderr
    );
    assert!(out.stdout.contains("[LOG] repo 段救援"), "{}", out.stdout);
    assert!(out.stdout.contains("repo-direct"), "{}", out.stdout);
    assert!(out.stdout.contains("repo-suffix"), "{}", out.stdout);
}

/// R3 (repair round 2): a traversal error anywhere in the repo walk
/// marks the index incomplete, and the pool arms must refuse — a
/// partial scan must never fake a unique hit (the unreadable
/// directory's same-name file may have been the true referent).
/// Deterministic unix fixture: a chmod-000 decoy directory holding a
/// same-name file + a visible same-name file; restoring permissions
/// flips the verdict back to the unique resolution. Skipped
/// behaviorally when privileges make chmod ineffective (root /
/// CAP_DAC_OVERRIDE): the readability probe doubles as the privileged-
/// identity check, no new dependency.
#[cfg(unix)]
#[test]
fn incomplete_repo_index_refuses_the_pool_arms() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("incomplete");
    for rel in ["web/visible", "web/hidden"] {
        std::fs::create_dir_all(repo.join(rel)).unwrap();
    }
    std::fs::write(repo.join("web/visible/util.ts"), "export const v = 1;\n").unwrap();
    std::fs::write(repo.join("web/hidden/util.ts"), "export const h = 2;\n").unwrap();
    // ctx seed for the restored phase: the decoy is same-named BY DESIGN
    // (it makes the incomplete-scan fake-unique risk real), so once the
    // index is whole the pool holds a genuine two-candidate world — a
    // prior repo-direct hit in web/visible is the majority evidence the
    // pool's own ctx tie-break needs to resolve uniquely
    std::fs::write(repo.join("web/visible/lead.ts"), "export const l = 3;\n").unwrap();
    std::fs::write(repo.join(".code-reality.toml"), PKG_PROFILE).unwrap();
    let hidden = repo.join("web/hidden");
    let locked = std::fs::Permissions::from_mode(0o000);
    let open = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&hidden, locked).unwrap();
    // privileged-identity probe: when the restriction does not bind the
    // fixture cannot distinguish incomplete from complete — restore and
    // skip (manual verification path: run the test unprivileged)
    if std::fs::read_dir(&hidden).is_ok() {
        std::fs::set_permissions(&hidden, open.clone()).unwrap();
        println!("skipped: chmod 000 ineffective under this identity (root/CAP_DAC_OVERRIDE)");
        return;
    }

    // incomplete index → the pool arm refuses even though a same-name
    // file IS visible: the unreadable sibling may have been the referent
    let verdict = {
        let mut r = PathResolver::new(&repo).unwrap();
        r.resolve("util.ts")
    };
    // restore BEFORE asserting — the tempdir must stay cleanable even
    // if an assertion below panics
    std::fs::set_permissions(&hidden, open).unwrap();
    assert_eq!(
        verdict.1, "ambiguous",
        "incomplete index must refuse the pool arm"
    );
    assert!(verdict.0.is_none());

    // restored permissions → complete index → BOTH candidates are real
    // now (the decoy was same-named by design); the web/visible repo-
    // direct hit gives ctx the unique majority, and the visible file
    // resolves as the pool's unique winner
    let (p, kind) = {
        let mut r = PathResolver::new(&repo).unwrap();
        let (seed, seed_kind) = r.resolve("web/visible/lead.ts");
        assert_eq!(
            seed_kind, "repo-direct",
            "complete index, repo-direct arm unaffected"
        );
        assert_eq!(seed.unwrap(), repo.join("web/visible/lead.ts"));
        r.resolve("util.ts")
    };
    assert_eq!(kind, "repo-suffix");
    assert_eq!(p.unwrap(), repo.join("web/visible/util.ts"));
}

/// R4 (repair round 3): a frame path under an excluded prefix stays
/// external even when the repo carries an unrelated same-basename file —
/// the repo pool's basename fallback must not launder an excluded frame
/// into a wrong anchor. The decoy file is what makes the old flow reach
/// the pool and "resolve" (vacuous without it).
#[test]
fn excluded_frame_path_is_not_laundered_by_the_basename_pool() {
    let repo = write_repo(
        "excl-launder",
        &[
            ("pkg/a.py", "def f():\n    pass\n"),
            ("poc/x/foo.rs", "pub fn excluded() {}\n"),
            ("web/y/foo.rs", "pub fn visible() {}\n"),
        ],
        Some("exclude = [\"poc/\"]\n\n[[module]]\nprefix = \"pkg/\"\n"),
    );
    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("poc/x/foo.rs");
    assert_eq!(
        kind, "none",
        "excluded frame stays external — no basename laundering"
    );
    assert!(p.is_none());
}

/// R5 (repair round 3): the walker prunes excluded subtrees BEFORE
/// descending — their contents never enter the pool, so an unreadable
/// directory under an excluded prefix is corpus-external noise and must
/// NOT trip the incomplete flag (which would disable all pool rescue).
/// The pool stays usable: the visible same-name file resolves uniquely.
/// (The counterpart — a NON-excluded unreadable directory refusing the
/// pool — is the R3 test's face; not repeated here.)
#[cfg(unix)]
#[test]
fn excluded_unreadable_dir_does_not_poison_the_pool() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap().keep();
    let repo = std::fs::canonicalize(&tmp).unwrap().join("excl-noise");
    for rel in ["web/visible", "poc/junk"] {
        std::fs::create_dir_all(repo.join(rel)).unwrap();
    }
    std::fs::write(repo.join("web/visible/util.ts"), "export const v = 1;\n").unwrap();
    std::fs::write(repo.join("poc/junk/util.ts"), "export const j = 2;\n").unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "exclude = [\"poc/\"]\n\n[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    let hidden = repo.join("poc/junk");
    let locked = std::fs::Permissions::from_mode(0o000);
    let open = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&hidden, locked).unwrap();
    // privileged-identity probe (same protocol as the R3 fixture):
    // when the restriction does not bind, restore and skip
    if std::fs::read_dir(&hidden).is_ok() {
        std::fs::set_permissions(&hidden, open).unwrap();
        println!("skipped: chmod 000 ineffective under this identity (root/CAP_DAC_OVERRIDE)");
        return;
    }

    let (p, kind) = {
        let mut r = PathResolver::new(&repo).unwrap();
        r.resolve("util.ts")
    };
    // restore BEFORE asserting — the tempdir must stay cleanable even
    // if an assertion below panics
    std::fs::set_permissions(&hidden, open).unwrap();
    assert_eq!(
        kind, "repo-suffix",
        "excluded-region unreadability must not poison the pool"
    );
    assert_eq!(p.unwrap(), repo.join("web/visible/util.ts"));
}

/// R6 (repair round 4): the containment refusal gets the same
/// query-path-level gate as exclusion (R4) — a `../` frame whose
/// outside target REALLY exists stays external even when the repo
/// carries a same-basename decoy: the basename fallback must not
/// launder what the containment check just refused. The decoy is what
/// makes the old flow reach the pool and "resolve" (vacuous without
/// it); by its own repo-relative name the decoy stays resolvable.
#[test]
fn escaped_frame_path_is_not_laundered_by_the_basename_pool() {
    let tmp = tempfile::tempdir().unwrap().keep();
    let root = std::fs::canonicalize(&tmp).unwrap();
    let repo = root.join("repo");
    for rel in ["pkg", "web/y"] {
        std::fs::create_dir_all(repo.join(rel)).unwrap();
    }
    std::fs::write(repo.join("pkg/a.py"), "def f():\n    pass\n").unwrap();
    std::fs::write(repo.join("web/y/leak.py"), "x = 1\n").unwrap();
    std::fs::write(repo.join(".code-reality.toml"), PKG_PROFILE).unwrap();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("leak.py"), "x = 2\n").unwrap();
    // non-vacuous preconditions: the escape target really exists at the
    // frame's spelling, and the in-repo basename decoy is in place
    assert!(repo.join("../outside/leak.py").exists());
    assert!(repo.join("web/y/leak.py").is_file());

    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("../outside/leak.py");
    assert_eq!(
        kind, "none",
        "escaped frame stays external — no basename laundering"
    );
    assert!(p.is_none());
    // the decoy itself remains legitimately resolvable by its own
    // repo-relative name (the gate must not over-fire)
    let (p, kind) = r.resolve("web/y/leak.py");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("web/y/leak.py"));
}

/// R7 (repair round 5): the query-path gate sits at the FRONT of the
/// repo segment — a frame whose TEXTUAL spelling lives under an
/// excluded prefix refuses even when it is a symlink whose canonical
/// target is a perfectly governed in-repo file (author intent: an
/// excluded-region path is refused, wherever the link lands). The
/// in-repo target stays reachable by its own honest spelling.
#[cfg(unix)]
#[test]
fn excluded_spelling_symlink_frame_is_refused() {
    let repo = write_repo(
        "excl-symlink",
        &[
            ("pkg/a.py", "def f():\n    pass\n"),
            ("web/foo.rs", "pub fn governed() {}\n"),
        ],
        Some("exclude = [\"poc/\"]\n\n[[module]]\nprefix = \"pkg/\"\n"),
    );
    std::fs::create_dir_all(repo.join("poc/x")).unwrap();
    std::os::unix::fs::symlink(repo.join("web/foo.rs"), repo.join("poc/x/foo.rs")).unwrap();
    // fixture sanity: the frame spelling sits under the excluded
    // prefix, the canonical target is a governed in-repo file
    assert!(repo.join("poc/x/foo.rs").is_file());
    assert_eq!(
        std::fs::canonicalize(repo.join("poc/x/foo.rs")).unwrap(),
        std::fs::canonicalize(repo.join("web/foo.rs")).unwrap()
    );

    let mut r = PathResolver::new(&repo).unwrap();
    let (p, kind) = r.resolve("poc/x/foo.rs");
    assert_eq!(
        kind, "none",
        "excluded spelling refuses regardless of canonical target"
    );
    assert!(p.is_none());
    // the honest spelling of the same file stays resolvable
    let (p, kind) = r.resolve("web/foo.rs");
    assert_eq!(kind, "repo-direct");
    assert_eq!(p.unwrap(), repo.join("web/foo.rs"));
}

/// R8 (repair round 6): equivalent spellings must not bypass the
/// textual exclusion — the gate evaluates on the LEXICALLY NORMALIZED
/// spelling (`.`/empty segments folded, `..` resolved as pure text, no
/// FS), so `./poc/...` and `web/../poc/...` refuse exactly like
/// `poc/...` (the symlink-into-corpus shape from R7 is what the bypass
/// used to reach). A `..` fold escaping the root refuses outright at
/// the lexical stage. Normalization feeds the gate only — resolution
/// still canonicalizes through common::resolve; the non-excluded
/// equivalent spelling resolves normally.
#[cfg(unix)]
#[test]
fn equivalent_spellings_cannot_bypass_the_exclusion_gate() {
    let repo = write_repo(
        "excl-equiv",
        &[
            ("pkg/a.py", "def f():\n    pass\n"),
            ("web/foo.rs", "pub fn governed() {}\n"),
            ("web/kchart/a.ts", "export const k = 1;\n"),
        ],
        Some("exclude = [\"poc/\"]\n\n[[module]]\nprefix = \"pkg/\"\n"),
    );
    std::fs::create_dir_all(repo.join("poc/x")).unwrap();
    std::os::unix::fs::symlink(repo.join("web/foo.rs"), repo.join("poc/x/foo.rs")).unwrap();

    let mut r = PathResolver::new(&repo).unwrap();
    // two bypass spellings of the R7 shape (excluded prefix, symlink
    // into the corpus): lexical normalization exposes both to the
    // textual exclusion
    for frame in ["./poc/x/foo.rs", "web/../poc/x/foo.rs"] {
        let (p, kind) = r.resolve(frame);
        assert_eq!(
            kind, "none",
            "{frame}: equivalent spelling must not bypass the gate"
        );
        assert!(p.is_none(), "{frame}");
    }
    // control: a non-excluded equivalent spelling resolves normally
    let (p, kind) = r.resolve("./web/kchart/a.ts");
    assert_eq!(kind, "repo-direct", "./web/kchart/a.ts must resolve");
    assert_eq!(p.unwrap(), repo.join("web/kchart/a.ts"));
}
