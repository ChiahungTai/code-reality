//! AIR-80 — `tour materialize` surface: manifest provenance roundtrip
//! (`[[delta_arc]]`) and snapshot-pair resolution semantics.

use code_reality::tour::snapshot_for;
use code_reality::tour_manifest::{dump, load, upsert_delta_arc};

#[test]
fn delta_arc_roundtrip_and_replace_by_arcid() {
    let tmp = std::env::temp_dir().join(format!(
        "cr-s8-manifest-{}",
        std::process::id() as u64
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("manifest.toml");

    let mut m = load(&path).unwrap(); // absent → default
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-78".into())),
            ("cardId", toml::Value::String("AIR-78".into())),
            ("base", toml::Value::String("beb8642".into())),
            ("target", toml::Value::String("0d97bd7".into())),
            (
                "ep",
                toml::Value::String("ai-analysis/_tasks/x/ep.md".into()),
            ),
            ("quality", toml::Value::String("full".into())),
        ],
    );
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-79".into())),
            ("base", toml::Value::String("a".into())),
            ("target", toml::Value::String("b".into())),
        ],
    );
    // same arcId re-materialize → authoritative full replace (canonical key)
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-78".into())),
            ("base", toml::Value::String("beb8642".into())),
            ("target", toml::Value::String("0d97bd7".into())),
            ("quality", toml::Value::String("degraded".into())),
            (
                "tourPath",
                toml::Value::String(".tours/delta/air-78.tour".into()),
            ),
        ],
    );
    dump(&path, &m).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[[delta_arc]]"), "{text}");

    let back = load(&path).unwrap();
    assert_eq!(back.delta_arc.len(), 2);
    let row78 = back
        .delta_arc
        .iter()
        .find(|r| r.get("arcId").and_then(|v| v.as_str()) == Some("air-78"))
        .unwrap();
    // full replace: cardId/ep from the first write are gone (tool-authoritative)
    assert!(row78.get("cardId").is_none());
    assert!(row78.get("ep").is_none());
    assert_eq!(
        row78.get("quality").and_then(|v| v.as_str()),
        Some("degraded")
    );
    assert_eq!(
        row78.get("tourPath").and_then(|v| v.as_str()),
        Some(".tours/delta/air-78.tour")
    );
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn snapshot_resolution_hit_miss_ambiguous() {
    let tmp = std::env::temp_dir().join(format!(
        "cr-s8-snap-{}",
        std::process::id() as u64
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("ai-rules-beb86429.json"), "{}").unwrap();

    // hit: sha8 suffix match (commit longer than 8)
    let hit = snapshot_for(&tmp, "beb864291234567890abcdef").unwrap();
    assert!(hit.ends_with("ai-rules-beb86429.json"));

    // miss: fail-loud with the expected pattern + ask-once hint
    let err = snapshot_for(&tmp, "fffffffffffffff").unwrap_err();
    assert!(err.contains("-ffffffff.json"), "{err}");
    assert!(err.contains("snapshot 缺席"), "{err}");

    // ambiguous: two files share the sha8 suffix
    std::fs::write(tmp.join("other-beb86429.json"), "{}").unwrap();
    let amb = snapshot_for(&tmp, "beb86429").unwrap_err();
    assert!(amb.contains("歧義"), "{amb}");
    std::fs::remove_dir_all(&tmp).ok();
}

// ------------------------------------------------- run()-level e2e (R6)

use code_reality::tour;

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn rev(repo: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Temp git repo + fabricated S2 snapshot pair + EP md.
///
/// `mod.py` content embeds the tag so each fixture produces UNIQUE commit
/// shas — parallel fixtures with identical content+timestamp would mint
/// identical shas across repos, muddying which repo owns an object when a
/// flake surfaces.
fn e2e_fixture(tag: &str) -> (tempdir::TempDir, std::path::PathBuf, String, String) {
    let tmp = tempdir::TempDir::new_unique();
    let repo = tmp.path().join(tag);
    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("pkg/mod.py"),
        format!("# header {tag}\ndef keep():\n    pass\n"),
    )
    .unwrap();
    std::fs::write(repo.join("ep.md"), "# EP\n\n- pkg/（宣稱）\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    let before = rev(&repo);
    std::fs::write(
        repo.join("pkg/mod.py"),
        format!("# header {tag}\ndef keep():\n    return 42\n"),
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "the change"]);
    let after = rev(&repo);
    // fabricated S2 snapshots (producer equivalent: <repo>-<sha8>.json)
    let snaps = repo.join(".code-reality").join("snapshots");
    std::fs::create_dir_all(&snaps).unwrap();
    for (sha, files) in [(&before, "pkg/mod.py"), (&after, "pkg/mod.py")] {
        let body = serde_json::json!({
            "_meta": {"commit": sha, "repo": tag},
            "files": [files],
            "module_edges": [],
        });
        std::fs::write(
            snaps.join(format!("e2e-{}.json", &sha[..8])),
            body.to_string(),
        )
        .unwrap();
    }
    (tmp, repo, before, after)
}

fn manifest_row(repo: &std::path::Path, arc: &str) -> Option<toml::Table> {
    let m = load(&repo.join(".tours").join("manifest.toml")).unwrap();
    m.delta_arc
        .iter()
        .find(|r| r.get("arcId").and_then(|v| v.as_str()) == Some(arc))
        .cloned()
}

#[test]
fn register_then_materialize_intent_only_full_chain() {
    let _guard = e2e_lock();
    let (_tmp, repo, before, after) = e2e_fixture("e2e-full");
    // register with 7-char shas (R3: rev-parse canonicalization) — pending row
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before[..7],
        "--target",
        &after[..7],
        "--ep",
        "ep.md",
        "--card",
        "E2E-1",
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let row = manifest_row(&repo, "e2e-arc").expect("pending row persisted");
    assert!(
        row.get("tourPath").is_none(),
        "register must not set tourPath"
    );
    assert!(!repo.join(".tours/delta/e2e-arc.tour").exists());
    // intent-only materialize (row-driven, no flags)
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-arc",
        "--repo",
        repo.to_str().unwrap(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let row = manifest_row(&repo, "e2e-arc").expect("row survives materialize");
    assert_eq!(
        row.get("tourPath").and_then(|v| v.as_str()),
        Some(".tours/delta/e2e-arc.tour")
    );
    assert_eq!(row.get("quality").and_then(|v| v.as_str()), Some("full"));
    assert_eq!(row.get("cardId").and_then(|v| v.as_str()), Some("E2E-1"));
    let tour: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo.join(".tours/delta/e2e-arc.tour")).unwrap(),
    )
    .unwrap();
    assert_eq!(tour["ref"].as_str().unwrap(), after.as_str());
    // overview step anchors the EP by its REPO-RELATIVE path (never absolute)
    assert_eq!(tour["steps"][0]["file"], "ep.md");
    // overwrite: same arcId → same tourPath, row not duplicated
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-arc",
        "--repo",
        repo.to_str().unwrap(),
    ]);
    assert_eq!(out.exit_code, 0);
    let m = load(&repo.join(".tours").join("manifest.toml")).unwrap();
    assert_eq!(m.delta_arc.len(), 1, "arcId is the canonical key");
}

#[test]
fn stale_snapshot_fails_loud_and_row_stays_pending() {
    let _guard = e2e_lock();
    let (_tmp, repo, before, after) = e2e_fixture("e2e-stale");
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-stale-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before,
        "--target",
        &after,
    ]);
    assert_eq!(out.exit_code, 0);
    // poison the target snapshot with a stale marker
    let snap = repo.join(format!(".code-reality/snapshots/e2e-{}.json", &after[..8]));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&snap).unwrap()).unwrap();
    v["_meta"]["stale"] = serde_json::json!("graph sha beb86429 != HEAD 11fd0d73");
    std::fs::write(&snap, v.to_string()).unwrap();
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-stale-arc",
        "--repo",
        repo.to_str().unwrap(),
    ]);
    assert_ne!(out.exit_code, 0, "stale gate must fail loud");
    assert!(out.stderr.contains("stale"), "{}", out.stderr);
    assert!(!repo.join(".tours/delta/e2e-stale-arc.tour").exists());
    let row = manifest_row(&repo, "e2e-stale-arc").expect("pending row must survive failure");
    assert!(
        row.get("tourPath").is_none(),
        "failed materialize must not set tourPath"
    );
}

mod tempdir {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new_unique() -> Self {
            let p = std::env::temp_dir().join(format!(
                "cr-s8-e2e-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = Command::new("rm").arg("-rf").arg(&self.0).status();
        }
    }
}

/// Serialize the run()-level e2e tests: each spawns several git processes on
/// /var/folders tempdirs; observed flake mode is transient object ENOENT
/// under parallel git on macOS (s5-style crates' tempfile never hit it) —
/// the shipped code has no shared state, so gate the infra, not the logic.
/// Poison-tolerant: one test's failure must not cascade via lock poisoning.
static E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn e2e_lock() -> std::sync::MutexGuard<'static, ()> {
    E2E_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn external_ep_keeps_provenance_across_rematerialize() {
    // N1: repo-外 EP——claims provenance 必須跨重產存活（row 保留 ep spec、
    // quality=full），tour step 永不含絕對路徑。
    let _guard = e2e_lock();
    let (_tmp, repo, before, after) = e2e_fixture("e2e-ext");
    let outside = _tmp.path().join("outside-ep.md");
    std::fs::write(&outside, "# 外部 EP\n\n- pkg/（宣稱）\n").unwrap();
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-ext-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before,
        "--target",
        &after,
        "--ep",
        outside.to_str().unwrap(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let row = manifest_row(&repo, "e2e-ext-arc").unwrap();
    let outside_real = std::fs::canonicalize(&outside).unwrap();
    assert_eq!(
        row.get("ep").and_then(|v| v.as_str()),
        Some(outside_real.to_str().unwrap()),
        "repo-外 EP 以 realpath absolute canonical 持久化（/var symlink 已解析）"
    );
    for _ in 0..2 {
        let out = tour::run(&[
            "tour",
            "materialize",
            "e2e-ext-arc",
            "--repo",
            repo.to_str().unwrap(),
        ]);
        assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    }
    let row = manifest_row(&repo, "e2e-ext-arc").unwrap();
    assert_eq!(
        row.get("ep").and_then(|v| v.as_str()),
        Some(outside_real.to_str().unwrap()),
        "重產後 provenance 仍在（可等價重建 ep_fs）"
    );
    assert_eq!(row.get("quality").and_then(|v| v.as_str()), Some("full"));
    let tour: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo.join(".tours/delta/e2e-ext-arc.tour")).unwrap(),
    )
    .unwrap();
    for s in tour["steps"].as_array().unwrap() {
        let f = s["file"].as_str().unwrap();
        assert!(!f.starts_with('/'), "絕對路徑不得進 step file: {f}");
    }
}

#[test]
fn materialize_lone_card_flag_fails_loud() {
    // N2: --card 無 --base/--target＝fail-loud（silent drop 修）。
    let _guard = e2e_lock();
    let (_tmp, repo, before, after) = e2e_fixture("e2e-flag");
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-flag-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before,
        "--target",
        &after,
    ]);
    assert_eq!(out.exit_code, 0);
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-flag-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--card",
        "SNEAKY",
    ]);
    assert_ne!(out.exit_code, 0, "lone --card must fail loud");
    assert!(out.stderr.contains("註冊形"), "{}", out.stderr);
    let row = manifest_row(&repo, "e2e-flag-arc").unwrap();
    assert!(row.get("cardId").is_none(), "sneaky card must not leak in");
}

#[test]
fn relative_dotdot_ep_escape_normalizes_to_absolute() {
    // N1 residual (final review): `--ep ../outside.md` 的 lexical strip_prefix
    // 穿透——normalize 後 containment 判定必須把它歸為 repo 外（row 存
    // absolute），且不得成為 tour step anchor。
    let _guard = e2e_lock();
    let (_tmp, repo, before, after) = e2e_fixture("e2e-dotdot");
    let outside = _tmp.path().join("outside-dotdot.md");
    std::fs::write(&outside, "# 外部 EP\n\n- pkg/（宣稱）\n").unwrap();
    std::fs::copy(&outside, repo.join("outside-dotdot.md")).unwrap();
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-dotdot-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before,
        "--target",
        &after,
        "--ep",
        "../outside-dotdot.md",
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let row = manifest_row(&repo, "e2e-dotdot-arc").unwrap();
    let ep = row.get("ep").and_then(|v| v.as_str()).unwrap();
    assert!(
        ep.starts_with('/') && !ep.contains(".."),
        "row.ep 必為 normalized absolute: {ep}"
    );
    let outside_canonical = std::fs::canonicalize(&outside).unwrap();
    assert_eq!(
        ep,
        outside_canonical.to_str().unwrap(),
        "canonical 指向真實外部檔（macOS /var→/private/var symlink 已 normalize）"
    );
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-dotdot-arc",
        "--repo",
        repo.to_str().unwrap(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let tour: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo.join(".tours/delta/e2e-dotdot-arc.tour")).unwrap(),
    )
    .unwrap();
    for s in tour["steps"].as_array().unwrap() {
        let f = s["file"].as_str().unwrap();
        assert!(!f.contains(".."), "repo-外 EP 不得作 step anchor: {f}");
    }
}

#[test]
fn in_repo_symlink_ep_resolves_to_real_location() {
    // round-3 residual: repo/ep-link.md -> ../outside.md——lexical containment
    // 穿透；realpath 解析後 row 必存外部 absolute provenance，且 step 不得
    // 出現 ep-link.md（repo-外 EP 不作 step anchor）。
    let (_tmp, repo, before, after) = e2e_fixture("e2e-symlink");
    let outside = _tmp.path().join("outside-symlink.md");
    std::fs::write(&outside, "# 外部 EP\n\n- pkg/（宣稱）\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, repo.join("ep-link.md")).unwrap();
    let out = tour::run(&[
        "tour",
        "register",
        "e2e-symlink-arc",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &before,
        "--target",
        &after,
        "--ep",
        "ep-link.md",
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let row = manifest_row(&repo, "e2e-symlink-arc").unwrap();
    let ep = row.get("ep").and_then(|v| v.as_str()).unwrap();
    let outside_real = std::fs::canonicalize(&outside).unwrap();
    assert_eq!(
        ep,
        outside_real.to_str().unwrap(),
        "row.ep 必為 realpath absolute"
    );
    let out = tour::run(&[
        "tour",
        "materialize",
        "e2e-symlink-arc",
        "--repo",
        repo.to_str().unwrap(),
    ]);
    assert_eq!(out.exit_code, 0, "{}{}", out.stdout, out.stderr);
    let tour: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo.join(".tours/delta/e2e-symlink-arc.tour")).unwrap(),
    )
    .unwrap();
    for s in tour["steps"].as_array().unwrap() {
        let f = s["file"].as_str().unwrap();
        assert_ne!(f, "ep-link.md", "symlink EP 不得作 step anchor");
        assert!(!f.starts_with('/'), "絕對路徑不得進 step: {f}");
    }
}
