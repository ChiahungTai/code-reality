//! Oracle S: accepted Round 1 EP C (R0-06/07/10). Real filesystem/SQLite
//! integration; publication assertions are independent of the hash algorithm.
mod support;
use code_reality::{build, engine, graph_db, identity::IdentityCachePolicy};
use std::path::{Path, PathBuf};

fn lab() -> (tempfile::TempDir, tempfile::TempDir, Vec<PathBuf>) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("app.py"), "x = 1\n").unwrap();
    support::git_init(t.path());
    let bins = tempfile::tempdir().unwrap();
    support::install_py_deriving_fake(bins.path());
    let roots = vec![bins.path().to_path_buf()];
    build::build_repo(t.path(), None, &roots).unwrap();
    (t, bins, roots)
}

fn meta(repo: &Path) -> serde_json::Value {
    engine::load_meta(&support::slot_of(repo)).0.unwrap()
}

#[test]
fn manual_restamp_cannot_certify_content_edit() {
    let (t, _bins, roots) = lab();
    let repo = t.path();
    let before = meta(repo)["source_identity"].clone();
    std::fs::write(repo.join("app.py"), "x = 2\n").unwrap();
    engine::stamp_meta_core(repo, &support::slot_of(repo), &roots, None, None).unwrap();
    assert_eq!(meta(repo)["source_identity"], before);
    assert_eq!(
        engine::evaluate_staleness(repo, &support::slot_of(repo), IdentityCachePolicy::ReadOnly)
            .unwrap()
            .identity_drift,
        Some(true)
    );
}

#[test]
fn replaced_index_cannot_inherit_identity() {
    let (t, _bins, roots) = lab();
    let repo = t.path();
    std::fs::copy("tests/fixtures/rich_callers.scip", support::slot_of(repo)).unwrap();
    engine::stamp_meta_core(repo, &support::slot_of(repo), &roots, None, None).unwrap();
    assert!(meta(repo)["source_identity"].is_null());
}

#[test]
fn missing_main_graph_is_torn_alternate_is_optional() {
    let (t, _bins, roots) = lab();
    let repo = t.path();
    std::fs::remove_file(graph_db::db_path(repo)).unwrap();
    let snap =
        engine::evaluate_staleness(repo, &support::slot_of(repo), IdentityCachePolicy::ReadOnly)
            .unwrap();
    assert!(snap.graph_lags && snap.needs_rebuild(), "{snap:?}");
    let alt = repo.join(".code-reality/projections/p/index.scip");
    std::fs::create_dir_all(alt.parent().unwrap()).unwrap();
    std::fs::copy(support::slot_of(repo), &alt).unwrap();
    std::fs::copy(
        engine::meta_path(&support::slot_of(repo)),
        engine::meta_path(&alt),
    )
    .unwrap();
    let snap = engine::evaluate_staleness(repo, &alt, IdentityCachePolicy::ReadOnly).unwrap();
    assert!(!snap.graph_lags && !snap.needs_rebuild(), "{snap:?}");
    assert!(matches!(
        build::ensure_fresh(repo, &roots).unwrap(),
        build::HealOutcome::Healed { .. }
    ));
    assert!(graph_db::db_path(repo).is_file());
}

#[test]
fn production_edit_preserves_published_artifacts() {
    let (t, bins, roots) = lab();
    let repo = t.path();
    let paths = [
        support::slot_of(repo),
        engine::meta_path(&support::slot_of(repo)),
        graph_db::db_path(repo),
    ];
    let before: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    support::fake_bin(bins.path(), "pyrefly-index", &format!(
        "#!/bin/sh\n'{}' \"$@\" || exit $?\ncase \"$*\" in *--version*) exit 0;; esac\nprintf 'x = 2\\n' > '{}'/app.py\n", support::fixture_producer_bin().display(), repo.display()));
    assert!(
        build::build_repo(repo, None, &roots).is_err(),
        "production drift must reject publication"
    );
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), bytes, "{}", path.display());
    }
}

#[test]
fn four_same_process_writers_publish_complete_graphs() {
    let tmp = tempfile::tempdir().unwrap();
    let index = tmp.path().join("index.scip");
    std::fs::copy("tests/fixtures/rich_callers.scip", &index).unwrap();
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|s| {
        let threads: Vec<_> = (0..4)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    graph_db::build_from_cache_at(tmp.path(), &index)
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap().expect("serialized writer");
        }
    });
    let conn = rusqlite::Connection::open(graph_db::db_path(tmp.path())).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    for table in ["flows", "flow_memberships", "communities"] {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        // This Rust occurrence fixture has references but no CALLS flows.
        // Communities must exist; flow tables must be readable and consistent.
        if table == "communities" {
            assert!(n > 0);
        }
    }
    assert_eq!(conn.query_row("SELECT count(*) FROM flow_memberships m LEFT JOIN flows f ON f.id=m.flow_id LEFT JOIN nodes n ON n.id=m.node_id WHERE f.id IS NULL OR n.id IS NULL", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn replaced_index_without_restamp_cannot_serve_current_tree() {
    let (t, _bins, _roots) = lab();
    std::fs::copy(
        "tests/fixtures/rich_callers.scip",
        support::slot_of(t.path()),
    )
    .unwrap();
    // Rebuilding the graph must not accidentally validate the old meta's
    // source identity against the replacement index.
    graph_db::build_from_cache(t.path()).unwrap();
    let verdict = code_reality::freshness::freshness(t.path(), true);
    let v: serde_json::Value = serde_json::from_str(&verdict.stdout).unwrap();
    assert_ne!(v["serves"], "current-tree");
}

#[test]
fn fresh_manual_stamp_has_no_consumption_evidence() {
    let (t, _bins, roots) = lab();
    std::fs::remove_file(engine::meta_path(&support::slot_of(t.path()))).unwrap();
    engine::stamp_meta_core(t.path(), &support::slot_of(t.path()), &roots, None, None).unwrap();
    assert!(meta(t.path())["source_identity"].is_null());
    let verdict = code_reality::freshness::freshness(t.path(), true);
    let v: serde_json::Value = serde_json::from_str(&verdict.stdout).unwrap();
    assert_eq!(v["serves"], "legacy-signals");
}

#[test]
fn source_edit_while_waiting_for_publication_preserves_old_plane() {
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};
    let (t, bins, roots) = lab();
    let repo = t.path();
    let paths = [
        support::slot_of(repo),
        engine::meta_path(&support::slot_of(repo)),
        graph_db::db_path(repo),
    ];
    let before: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(repo.join(".code-reality/.writer.lock"))
        .unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let marker = repo.join("producer-finished");
    support::fake_bin(bins.path(), "pyrefly-index", &format!(
        "#!/bin/sh\n'{}' \"$@\" || exit $?\ncase \"$*\" in *--version*) exit 0;; esac\ntouch '{}'\n", support::fixture_producer_bin().display(), marker.display()));
    std::thread::scope(|s| {
        let attempt = s.spawn(|| build::build_repo(repo, None, &roots));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "producer did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Tiny corpus; hold publication closed while postproduction work runs.
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(repo.join("app.py"), "x = 2\n").unwrap();
        drop(lock);
        let err = attempt.join().unwrap().unwrap_err();
        assert!(format!("{err:?}").contains("before publication"), "{err:?}");
    });
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), bytes, "{}", path.display());
    }
    assert!(!std::fs::read_dir(repo.join(".code-reality/scip"))
        .unwrap()
        .any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".part-")));
}

#[test]
fn policy_edit_during_production_rejects_publication() {
    let (t, bins, roots) = lab();
    let repo = t.path();
    let before = std::fs::read(support::slot_of(repo)).unwrap();
    support::fake_bin(bins.path(), "pyrefly-index", &format!(
        "#!/bin/sh\n'{}' \"$@\" || exit $?\ncase \"$*\" in *--version*) exit 0;; esac\nprintf 'exclude = [\"nothing/\"]\\n' > '{}'/'.code-reality.toml'\n", support::fixture_producer_bin().display(), repo.display()));
    assert!(build::build_repo(repo, None, &roots).is_err());
    assert_eq!(std::fs::read(support::slot_of(repo)).unwrap(), before);
}
