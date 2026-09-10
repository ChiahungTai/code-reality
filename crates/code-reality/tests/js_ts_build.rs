//! JS/TS blueprint S1/S2 integration tests: language-set staging,
//! atomic publication, and the scip-typescript producer contract — all
//! through fake producers (hermetic; no npm/network).

mod support;

use code_reality::build::{build_repo, BuildError};
use code_reality::language::ProducerFamily;
use protobuf::Message;
use scip::types::Index;
use std::path::{Path, PathBuf};
use support::{fake_bin, log_lines, mkrepo, slot_docs, ts_scip_bytes};

const RICH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");

fn write_fixture(path: &Path, bytes: &[u8]) -> String {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
    path.display().to_string()
}

// ---------- fake producers ----------

fn fake_pyrefly(dir: &Path) {
    fake_bin(
        dir,
        "pyrefly-index",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-pyrefly 9.9.9'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--repo\" ] || [ \"$prev\" = \"--out\" ]; then eval \"${{prev#--}}=\\\"$a\\\"\"; fi; prev=\"$a\"; done
mkdir -p \"$(dirname \"$out\")\"
cp '{RICH}' \"$out\"
echo '[OK] fake pyrefly-index'
"
        ),
    );
}

fn fake_rust_analyzer(dir: &Path) {
    fake_bin(
        dir,
        "rust-analyzer",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo 'fake-ra 1.96.0-test'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then output=\"$a\"; fi; prev=\"$a\"; done
cp '{RICH}' \"$output\"
echo '[OK] fake rust-analyzer'
"
        ),
    );
}

/// Fake scip-typescript. Modes:
/// - `good`: always copy `fixture` to --output (logs argv lines to `log`)
/// - `zero_then_good`: the repo-root config gets the pinned zero-file
///   error; the CR derived config (`cr-tsconfig.json`) succeeds
/// - `fail`: exit 1 with a compiler-shaped error, never fall back
fn fake_scip_typescript(dir: &Path, mode: &str, fixture: &Path, log: &Path) {
    let run_good = format!(
        "echo \"$(pwd) $*\" >> '{}'\ncp '{}' \"$output\"\nexit 0",
        log.display(),
        fixture.display()
    );
    let body = match mode {
        "good" => run_good,
        "zero_then_good" => format!(
            "echo \"$(pwd) $*\" >> '{}'\ncase \"$config\" in\n  *cr-tsconfig.json) cp '{}' \"$output\"; exit 0 ;;\n  *) echo 'error: no files got indexed' >&2; exit 1 ;;\nesac",
            log.display(),
            fixture.display()
        ),
        _ => "echo 'error: TS2322 some compiler failure' >&2\nexit 1".to_string(),
    };
    fake_bin(
        dir,
        "scip-typescript",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo '0.4.0-fake'; exit 0; fi
prev=''; config=''; output=''
for a in \"$@\"; do
  if [ \"$prev\" = \"--output\" ]; then output=\"$a\"; prev=''; continue; fi
  if [ \"$prev\" = \"--cwd\" ] || [ \"$prev\" = \"--max-file-byte-size\" ]; then prev=''; continue; fi
  case \"$a\" in -*) ;; *) config=\"$a\" ;; esac
  prev=\"$a\"
done
{body}
"
        ),
    );
}

// ---------- S2: producer semantics ----------

#[test]
fn s2_derived_mode_success_and_report() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/a.mjs", "export function a() {}\n")]);
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path()); // not used; python not detected
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs/repo/src", &[("src/a.mjs", "a")]));
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("ts leg");
    assert_eq!(rep.face, "typescript-face");
    assert!(
        rep.producers
            .iter()
            .any(|p| p.contains("scip-typescript 0.4.0-fake")),
        "{:?}",
        rep.producers
    );
    assert!(rep.notes.iter().any(|n| n.contains("derived config")));
    assert_eq!(slot_docs(&repo), vec!["src/a.mjs".to_string()]);
    // no target-repo mutation: no config written into the repo root
    assert!(!repo.join("tsconfig.json").exists());
    // producer saw the CR-owned derived config, not a repo config
    let calls = log_lines(&log);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains("cr-tsconfig.json"), "{calls:?}");
}

#[test]
fn s2_existing_zero_file_falls_back_exactly_once() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("tsconfig.json", "{}\n"),
            ("src/a.ts", "export function a() {}\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs/repo/src", &[("src/a.ts", "a")]));
    fake_scip_typescript(bindir.path(), "zero_then_good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("fallback build");
    assert_eq!(rep.face, "typescript-face");
    assert_eq!(log_lines(&log).len(), 2, "exactly one derived retry");
    assert_eq!(slot_docs(&repo), vec!["src/a.ts".to_string()]);
}

#[test]
fn s2_existing_unrelated_error_no_fallback() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("tsconfig.json", "{\"compilerOptions\":{}}\n"),
            ("src/a.ts", "export function a() {}\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &[]);
    fake_scip_typescript(bindir.path(), "fail", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("TS2322")),
        "{err:?}"
    );
    assert!(log_lines(&log).is_empty(), "fail mode never copies");
    // the repo tsconfig is untouched
    assert_eq!(
        std::fs::read_to_string(repo.join("tsconfig.json")).unwrap(),
        "{\"compilerOptions\":{}}\n"
    );
}

#[test]
fn s2_filter_excludes_non_governed_docs_derived_exact() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("src/a.mjs", "export const a = 1;\n"),
            ("src/b.ts", "export const b = 1;\n"),
            ("dist/mirror.mjs", "generated\n"),
            (".code-reality.toml", "exclude = [\"dist/\"]\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    // producer output includes the generated mirror; filter must drop it
    write_fixture(
        &fx,
        &ts_scip_bytes(
            "/abs/repo",
            &[
                ("src/a.mjs", "a"),
                ("src/b.ts", "b"),
                ("dist/mirror.mjs", "mirror"),
            ],
        ),
    );
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("filtered build");
    assert!(rep.nodes > 0);
    assert_eq!(
        slot_docs(&repo),
        vec!["src/a.mjs".to_string(), "src/b.ts".to_string()]
    );
}

#[test]
fn s2_all_js_excluded_skips_leg_in_mixed_repo() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("gen/a.mjs", "generated\n"),
            (".code-reality.toml", "exclude = [\"gen/\"]\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs", &[("gen/a.mjs", "x")]));
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("py-only build");
    assert_eq!(rep.face, "python-face");
    assert!(log_lines(&log).is_empty(), "producer never spawned");
    assert!(rep.notes.iter().any(|n| n.contains("governed 語料為空")));
}

#[test]
fn s2_ts_only_all_excluded_converges_to_empty() {
    // codex blocker 1: an all-profile-excluded corpus is a LEGAL empty
    // terminal state — auto-detection converges by removing the index
    // and graph (not erroring forever while stale data lingers). An
    // explicit override onto the empty face stays a loud error.
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("gen/a.mjs", "generated\n"),
            (".code-reality.toml", "exclude = [\"gen/\"]\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &[]);
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots).expect("empty convergence");
    assert_eq!(rep.face, "empty(profile-excluded)");
    assert!(log_lines(&log).is_empty(), "producer never spawned");
    assert!(
        rep.notes.iter().any(|n| n.contains("收斂為空")),
        "{:?}",
        rep.notes
    );
    // the stale artifacts are GONE — freshness converges
    assert!(!repo.join(".code-reality/scip/index.scip").exists());
    assert!(!repo.join(".code-reality/graph.db").exists());
    // explicit override onto the empty face remains loud
    let err = build_repo(&repo, Some(ProducerFamily::TypeScript), &roots).unwrap_err();
    assert!(
        matches!(err, BuildError::Env(ref m) if m.contains("未產出索引")),
        "{err:?}"
    );
}

#[test]
fn s2_missing_binary_env_hint_slot_preserved() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/a.mjs", "export const a = 1;\n")]);
    let err = build_repo(&repo, None, &[PathBuf::from("/nonexistent-bin-dir")]).unwrap_err();
    let msg = match &err {
        BuildError::Env(m) | BuildError::Core(m) => m.clone(),
    };
    assert!(
        msg.contains("npm install --save-dev @sourcegraph/scip-typescript"),
        "{err:?}"
    );
    assert!(msg.contains("CODE_REALITY_NODE_BIN_DIR"), "{err:?}");
}

#[test]
fn s2_repo_local_node_modules_bin_wins() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(&t, &[("src/a.mjs", "export const a = 1;\n")]);
    // local install (version 9.9-local) vs global (9.9-global)
    let local_dir = repo.join("node_modules/.bin");
    std::fs::create_dir_all(&local_dir).unwrap();
    let log = repo.join("node_modules/.log");
    let fx = repo.join("node_modules/.fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    fake_bin(
        &local_dir,
        "scip-typescript",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo '9.9-local'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then output=\"$a\"; fi; prev=\"$a\"; done
cp '{}' \"$output\"
",
            fx.display()
        ),
    );
    let global_dir = tempfile::tempdir().unwrap();
    let glog = global_dir.path().join("ts-calls");
    let gfx = global_dir.path().join("fx.scip");
    write_fixture(&gfx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    fake_scip_typescript(global_dir.path(), "good", &gfx, &glog);
    let _ = log;

    let rep = build_repo(&repo, None, &[global_dir.path().to_path_buf()]).expect("local wins");
    assert!(
        rep.producers
            .iter()
            .any(|p| p.contains("scip-typescript 9.9-local")),
        "{:?}",
        rep.producers
    );
    assert!(log_lines(&glog).is_empty());
}

// ---------- S1: staging and atomic publication ----------

#[test]
fn s1_later_leg_failure_leaves_slot_byte_identical() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let slot = repo.join(".code-reality/scip/index.scip");
    build_repo(&repo, None, &roots).expect("initial mixed build");
    let before = std::fs::read(&slot).unwrap();

    // break ONLY the later (typescript) leg
    fake_scip_typescript(bindir.path(), "fail", &fx, &log);
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(matches!(err, BuildError::Env(_)), "{err:?}");
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "pre-build slot must stay byte-identical"
    );
}

#[test]
fn s1_malformed_later_part_preserves_slot() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let log = bindir.path().join("ts-calls");
    let goodfx = bindir.path().join("fx.scip");
    write_fixture(&goodfx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    fake_scip_typescript(bindir.path(), "good", &goodfx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let slot = repo.join(".code-reality/scip/index.scip");
    build_repo(&repo, None, &roots).expect("initial");
    let before = std::fs::read(&slot).unwrap();

    // producer "succeeds" but emits garbage
    let badfx = bindir.path().join("bad.scip");
    std::fs::write(&badfx, b"not-a-scip-index-at-all").unwrap();
    fake_scip_typescript(bindir.path(), "good", &badfx, &log);
    let err = build_repo(&repo, None, &roots).unwrap_err();
    assert!(matches!(err, BuildError::Env(_)), "{err:?}");
    assert_eq!(std::fs::read(&slot).unwrap(), before);
}

#[test]
fn s1_three_family_merge_is_ordered_and_deterministic() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    fake_rust_analyzer(bindir.path());
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    let ts_bytes = ts_scip_bytes("/abs", &[("src/a.mjs", "a")]);
    write_fixture(&fx, &ts_bytes);
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("three families");
    assert_eq!(rep.face, "mixed(python+rust+typescript)");
    let slot = repo.join(".code-reality/scip/index.scip");

    // frozen merge order: python ++ rust ++ typescript (byte-observable)
    let rich = std::fs::read(RICH).unwrap();
    let mut expect = rich.clone();
    expect.extend_from_slice(&rich);
    expect.extend_from_slice(&ts_bytes);
    assert_eq!(std::fs::read(&slot).unwrap(), expect);

    // determinism: identical partials → identical merged bytes
    let rep2 = build_repo(&repo, None, &roots).expect("rerun");
    assert_eq!(rep.face, rep2.face);
    assert_eq!(std::fs::read(&slot).unwrap(), expect);
}

#[test]
fn s1_py_ts_mixed_face_and_override_notes() {
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    fake_scip_typescript(bindir.path(), "good", &fx, &log);
    let roots = vec![bindir.path().to_path_buf()];

    let rep = build_repo(&repo, None, &roots).expect("py+ts");
    assert_eq!(rep.face, "mixed(python+typescript)");

    // explicit override on the same repo: only the TS leg runs, python
    // reported as omitted in deterministic order
    let rep2 = build_repo(&repo, Some(ProducerFamily::TypeScript), &roots).expect("override ts");
    assert_eq!(rep2.face, "typescript-face");
    assert!(rep2.notes.iter().any(|n| n.contains("未索引：python")));
    assert!(!rep2.notes.iter().any(|n| n.contains("rust")));
}

#[test]
fn s1_mcp_producer_validation_accepts_typescript() {
    // MCP face forwards to the CLI face; validate through the shared
    // parse (invalid values list all three; typescript is legal).
    assert!(ProducerFamily::parse_cli("typescript").is_some());
    assert!(ProducerFamily::parse_cli("python").is_some());
    assert!(ProducerFamily::parse_cli("rust").is_some());
    assert!(ProducerFamily::parse_cli("javascript").is_none());
}

// ---------- node tool roots unit ----------

static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn node_tool_roots_order_and_env() {
    let _g = ENV_MUTEX.lock().unwrap();
    let repo = Path::new("/tmp/some-repo");
    let base = vec![PathBuf::from("/base/a"), PathBuf::from("/base/b")];
    std::env::remove_var("CODE_REALITY_NODE_BIN_DIR");
    let roots = code_reality::ts_producer::node_tool_roots(repo, &base);
    assert_eq!(
        roots,
        vec![
            PathBuf::from("/tmp/some-repo/node_modules/.bin"),
            PathBuf::from("/base/a"),
            PathBuf::from("/base/b"),
        ]
    );
    std::env::set_var("CODE_REALITY_NODE_BIN_DIR", "/nx:/ny");
    let roots2 = code_reality::ts_producer::node_tool_roots(repo, &base);
    assert_eq!(
        roots2,
        vec![
            PathBuf::from("/tmp/some-repo/node_modules/.bin"),
            PathBuf::from("/nx"),
            PathBuf::from("/ny"),
            PathBuf::from("/base/a"),
            PathBuf::from("/base/b"),
        ]
    );
    std::env::remove_var("CODE_REALITY_NODE_BIN_DIR");
}

// ---------- codex-review regressions ----------

#[test]
fn s1_concurrent_builds_do_not_collide_on_staging() {
    // Same-process concurrent builds (the MCP spawn_blocking shape):
    // per-attempt staging names must keep attempts isolated — no
    // cross-cleanup, no cross-rename, and the live slot always parses
    // as one attempt's COMPLETE index.
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            ("app.py", "x = 1\n"),
            ("src/a.mjs", "export const a = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    fake_pyrefly(bindir.path());
    let log = bindir.path().join("ts-calls");
    let fx = bindir.path().join("fx.scip");
    write_fixture(&fx, &ts_scip_bytes("/abs", &[("src/a.mjs", "a")]));
    // slow TS producer so the two builds genuinely overlap
    fake_bin(
        bindir.path(),
        "scip-typescript",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo '0.4.0-fake'; exit 0; fi
prev=''; for a in \"$@\"; do if [ \"$prev\" = \"--output\" ]; then output=\"$a\"; fi; prev=\"$a\"; done
sleep 0.6
cp '{}' \"$output\"
",
            fx.display()
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];
    let repo_a = repo.clone();
    let repo_b = repo.clone();
    let roots_b = roots.clone();
    let h1 = std::thread::spawn(move || build_repo(&repo_a, None, &roots));
    let h2 =
        std::thread::spawn(move || build_repo(&repo_b, Some(ProducerFamily::Python), &roots_b));
    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();
    assert!(r1.is_ok(), "{r1:?}");
    assert!(r2.is_ok(), "{r2:?}");
    // no attempt's staging artifacts survive another attempt's cleanup
    let leftovers: Vec<String> = std::fs::read_dir(repo.join(".code-reality/scip"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.starts_with(".part-") || n.starts_with(".stage-") || n.starts_with(".publish-")
        })
        .collect();
    assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");
    // the live slot is one attempt's COMPLETE index (parses cleanly)
    let slot = repo.join(".code-reality/scip/index.scip");
    let bytes = std::fs::read(&slot).unwrap();
    Index::parse_from_bytes(&bytes).expect("live slot must parse after concurrent builds");
    let _ = log;
}

#[test]
fn s2_existing_partial_coverage_falls_back_to_derived() {
    // codex P0-2: an existing project config that indexes only PART of
    // the governed corpus must fall back to the derived config (which
    // converges exactly) — never publish a partial corpus the S4
    // fingerprint contract cannot represent.
    let t = tempfile::tempdir().unwrap();
    let repo = mkrepo(
        &t,
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"allowJs\":true}}\n",
            ),
            ("src/a.ts", "export const a = 1;\n"),
            ("src/b.ts", "export const b = 1;\n"),
        ],
    );
    let bindir = tempfile::tempdir().unwrap();
    let log = bindir.path().join("ts-calls");
    let full = bindir.path().join("full.scip");
    write_fixture(
        &full,
        &ts_scip_bytes("/abs", &[("src/a.ts", "a"), ("src/b.ts", "b")]),
    );
    let partial = bindir.path().join("partial.scip");
    write_fixture(&partial, &ts_scip_bytes("/abs", &[("src/a.ts", "a")]));
    fake_bin(
        bindir.path(),
        "scip-typescript",
        &format!(
            "#!/bin/sh
if [ \"$1\" = \"--version\" ]; then echo '0.4.0-fake'; exit 0; fi
prev=''; config=''; output=''
for a in \"$@\"; do
  if [ \"$prev\" = \"--output\" ]; then output=\"$a\"; prev=''; continue; fi
  if [ \"$prev\" = \"--cwd\" ] || [ \"$prev\" = \"--max-file-byte-size\" ]; then prev=''; continue; fi
  case \"$a\" in -*) ;; *) config=\"$a\" ;; esac
  prev=\"$a\"
done
echo \"$config\" >> '{}'
case \"$config\" in
  *cr-tsconfig.json) cp '{}' \"$output\"; exit 0 ;;
  *) cp '{}' \"$output\"; exit 0 ;;
esac
",
            log.display(),
            full.display(),
            partial.display()
        ),
    );
    let roots = vec![bindir.path().to_path_buf()];
    let rep = build_repo(&repo, None, &roots).expect("fallback build");
    // exactly two producer runs (existing partial → derived full)
    assert_eq!(log_lines(&log).len(), 2, "{:?}", log_lines(&log));
    assert!(rep.notes.iter().any(|n| n.contains("derived config")));
    let mut docs = slot_docs(&repo);
    docs.sort();
    assert_eq!(docs, vec!["src/a.ts".to_string(), "src/b.ts".to_string()]);
}
