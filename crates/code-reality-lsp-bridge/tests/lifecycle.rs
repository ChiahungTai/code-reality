//! S oracle: accepted EP B and independent Round 0 subprocess reproductions.
//! Real pipes, multi-MiB writes, child state and wall-clock assertions.
use code_reality_lsp_bridge::{
    server::check_file_impl,
    session::{BackendCommand, LangSpec, LspSession},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, OnceLock},
    time::{Duration, Instant},
};
static BIN: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
fn session(dir: &std::path::Path, mode: &str, timeout: u64) -> Arc<LspSession> {
    let (_, bin) = BIN.get_or_init(|| {
        let d = tempfile::tempdir().unwrap();
        let bin = d.path().join("lifecycle-backend");
        let result = std::process::Command::new("rustc")
            .args(["--edition=2021"])
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/lifecycle_backend.rs"),
            )
            .arg("-o")
            .arg(&bin)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        (d, bin)
    });
    let mut lang = LangSpec::python();
    lang.slow_timeout_ms = timeout;
    Arc::new(LspSession::new(
        BackendCommand {
            program: bin.display().to_string(),
            args: vec![mode.into(), dir.display().to_string()],
        },
        dir.into(),
        50,
        lang,
    ))
}
fn alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .unwrap()
        .status
        .success()
}
fn kill(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
}
fn file(dir: &std::path::Path, large: bool) -> String {
    let p = dir.join("sample.py");
    std::fs::write(
        &p,
        if large {
            "x".repeat(4 * 1024 * 1024)
        } else {
            "x = 1\n".into()
        },
    )
    .unwrap();
    p.display().to_string()
}
#[test]
fn cached_check_rejects_known_death() {
    let d = tempfile::tempdir().unwrap();
    let s = session(d.path(), "normal", 1000);
    let f = file(d.path(), false);
    s.request("probe", Value::Null).unwrap();
    assert_eq!(check_file_impl(&s, &f).unwrap(), "count=0\n");
    kill(s.backend_pid().unwrap());
    let limit = Instant::now() + Duration::from_secs(2);
    while !s.is_dead() {
        assert!(Instant::now() < limit);
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = check_file_impl(&s, &f);
    s.shutdown().unwrap();
    assert!(result.unwrap_err().contains("died"));
}
#[test]
fn rejected_initialization_is_synchronously_terminal_and_reaped() {
    let d = tempfile::tempdir().unwrap();
    let s = session(d.path(), "reject-once", 1000);
    assert!(s
        .request("probe", Value::Null)
        .unwrap_err()
        .contains("initialize"));
    let pid = std::fs::read_to_string(d.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(s.is_dead(), "failed init must be terminal before returning");
    assert!(
        !alive(pid),
        "failed init child must be reaped before returning"
    );
    assert!(s.request("probe", Value::Null).is_err());
    let fresh = session(d.path(), "reject-once", 1000);
    assert!(fresh.request("probe", Value::Null).is_ok());
    fresh.shutdown().unwrap();
}
fn blocked(concurrent_shutdown: bool) {
    let d = tempfile::tempdir().unwrap();
    let s = session(
        d.path(),
        "normal",
        if concurrent_shutdown { 20_000 } else { 400 },
    );
    let f = file(d.path(), true);
    s.request("stop-reading", Value::Null).unwrap();
    let pid = s.backend_pid().unwrap();
    let start = Instant::now();
    let (tx, rx) = mpsc::channel();
    let child = s.clone();
    let check = std::thread::spawn(move || {
        tx.send(check_file_impl(&child, &f)).unwrap();
    });
    let shutdown = if concurrent_shutdown {
        std::thread::sleep(Duration::from_millis(100));
        let child = s.clone();
        Some(std::thread::spawn(move || {
            let t = Instant::now();
            child.shutdown().unwrap();
            t.elapsed()
        }))
    } else {
        None
    };
    let observed = rx.recv_timeout(Duration::from_secs(if concurrent_shutdown {
        12
    } else {
        2
    }));
    let rescued = observed.is_err();
    if rescued {
        kill(pid);
    }
    let result = observed.unwrap_or_else(|_| rx.recv_timeout(Duration::from_secs(3)).unwrap());
    check.join().unwrap();
    let shutdown_elapsed = shutdown.map(|h| h.join().unwrap());
    s.shutdown().unwrap();
    eprintln!("blocked concurrent_shutdown={concurrent_shutdown} elapsed_ms={} shutdown={shutdown_elapsed:?} pid={pid} alive={} rescue={rescued} result={result:?}", start.elapsed().as_millis(), alive(pid));
    assert!(!rescued, "deadline required external rescue");
    if let Some(elapsed) = shutdown_elapsed {
        assert!(elapsed < Duration::from_secs(12));
    }
    assert!(result.is_err());
    assert!(s.is_dead());
    assert!(!alive(pid));
}
#[test]
fn blocked_write_check_deadline_includes_sync() {
    blocked(false);
}
#[test]
fn concurrent_shutdown_bounds_blocked_write_and_reaps() {
    blocked(true);
}
#[test]
fn healthy_diagnostics_and_server_request_reply() {
    let d = tempfile::tempdir().unwrap();
    let s = session(d.path(), "normal", 1000);
    assert_eq!(
        s.request("ask-client", Value::Null).unwrap()["result"],
        "client-replied"
    );
    assert_eq!(
        check_file_impl(&s, &file(d.path(), false)).unwrap(),
        "count=0\n"
    );
    let pid = s.backend_pid().unwrap();
    s.shutdown().unwrap();
    assert!(!alive(pid));
}

#[test]
fn check_deadline_includes_pending_interaction_wait() {
    let d = tempfile::tempdir().unwrap();
    let s = session(d.path(), "normal", 400);
    let f = file(d.path(), false);
    s.request("probe", Value::Null).unwrap();
    let pid = s.backend_pid().unwrap();
    let child = s.clone();
    let waiter = std::thread::spawn(move || child.request("hold-response", Value::Null));
    let limit = Instant::now() + Duration::from_secs(2);
    while !d.path().join("holding").exists() {
        assert!(Instant::now() < limit);
        std::thread::sleep(Duration::from_millis(2));
    }
    let start = Instant::now();
    let result = check_file_impl(&s, &f);
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(waiter.join().unwrap().is_err());
    assert!(!alive(pid));
    eprintln!(
        "interaction deadline elapsed_ms={} reaped=true",
        start.elapsed().as_millis()
    );
}

#[test]
fn blocked_initialize_write_uses_check_entry_deadline() {
    let d = tempfile::tempdir().unwrap();
    let template = session(d.path(), "no-read", 400);
    let mut spec = LangSpec::python();
    spec.slow_timeout_ms = 3000;
    // A multi-MiB rootUri fills the actual initialization pipe.
    let s = LspSession::new(
        template.backend().clone(),
        PathBuf::from(format!("/{}", "x".repeat(4 * 1024 * 1024))),
        50,
        spec,
    );
    let start = Instant::now();
    let result = check_file_impl(&s, &file(d.path(), false));
    let pid = std::fs::read_to_string(d.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(result.is_err());
    assert!(s.is_dead());
    assert!(!alive(pid));
    assert!(start.elapsed() < Duration::from_secs(5));
    eprintln!(
        "initialize write deadline elapsed_ms={} reaped=true",
        start.elapsed().as_millis()
    );
}

#[test]
fn shutdown_bounds_request_write_and_reader_reply_backpressure() {
    for method in ["large-request", "flood-client"] {
        let d = tempfile::tempdir().unwrap();
        let s = session(d.path(), "normal", 1000);
        s.request(
            if method == "large-request" {
                "stop-reading"
            } else {
                "flood-client"
            },
            Value::Null,
        )
        .unwrap();
        let pid = s.backend_pid().unwrap();
        let waiter = if method == "large-request" {
            let child = s.clone();
            Some(std::thread::spawn(move || {
                child.request("probe", Value::String("x".repeat(4 * 1024 * 1024)))
            }))
        } else {
            None
        };
        std::thread::sleep(Duration::from_millis(200));
        let start = Instant::now();
        s.shutdown().unwrap();
        assert!(start.elapsed() < Duration::from_secs(12));
        assert!(!alive(pid));
        assert!(s.backend_pid().is_none());
        if let Some(waiter) = waiter {
            assert!(waiter.join().unwrap().is_err());
        }
        eprintln!(
            "{method} shutdown elapsed_ms={} reaped=true transport_joined=true",
            start.elapsed().as_millis()
        );
    }
}
