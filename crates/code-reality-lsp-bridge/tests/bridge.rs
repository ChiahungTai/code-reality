//! Integration tests against the REAL `pyrefly-lsp` backend (L3
//! consumer-side mode — the bridge's consumer is the LS process).
//!
//! Backend binary resolution order (EP R-02):
//! `LSP_BRIDGE_TEST_BIN` env override → `pyrefly-lsp` on PATH →
//! workspace `target/release/pyrefly-lsp`. On a fresh checkout, build
//! it first: `cargo build --release -p pyrefly-producer --bin pyrefly-lsp`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use code_reality_lsp_bridge::server::{check_file_impl, edit_file_impl, hover_impl};
use code_reality_lsp_bridge::LspSession;

mod common;

fn backend_bin() -> String {
    common::backend_bin()
}

const SAMPLE: &str = "import math\n\n\ndef greet(name: str) -> str:\n    return \"hello \" + name\n\n\nmsg = greet(\"world\")\ncount: int = msg\nnan_invalid: int = math.nan\n\nreveal = msg\nprint(reveal, count)\n";

/// Strict-preset fixture with two deliberate type errors (same shape
/// as the spike fixture, whose expected outputs are already verified).
fn strict_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.py");
    std::fs::write(&sample, SAMPLE).unwrap();
    std::fs::write(dir.path().join("pyrefly.toml"), "preset = \"strict\"\n").unwrap();
    (dir, sample)
}

fn session_at(dir: &std::path::Path) -> Arc<LspSession> {
    Arc::new(LspSession::new(&backend_bin(), dir.to_path_buf(), 300))
}

// ---- S1: lifecycle -------------------------------------------------

#[test]
fn session_handshake_and_shutdown() {
    let (dir, _sample) = strict_fixture();
    let s = session_at(dir.path());
    // Lazy: nothing spawned before the first interaction.
    assert!(s.backend_pid().is_none());
    // Any request triggers spawn + handshake.
    s.request("shutdown", serde_json::Value::Null).unwrap();
    let info = s.server_info();
    assert!(
        info.starts_with("pyrefly-lsp"),
        "serverInfo.name should be pyrefly-lsp, got: {info}"
    );
    s.shutdown().unwrap();
    assert!(s.backend_pid().is_none());
}

#[test]
fn backend_spawn_failure_is_loud() {
    let dir = tempfile::tempdir().unwrap();
    let s = Arc::new(LspSession::new(
        "/nonexistent-backend",
        dir.path().to_path_buf(),
        300,
    ));
    let err = s.request("shutdown", serde_json::Value::Null).unwrap_err();
    assert!(err.contains("failed to spawn"), "got: {err}");
    assert!(err.contains("--lsp-command"), "guidance missing: {err}");
}

#[test]
fn backend_death_surfaces_loud_error() {
    let (dir, _sample) = strict_fixture();
    let s = session_at(dir.path());
    s.request("shutdown", serde_json::Value::Null).unwrap();
    let pid = s.backend_pid().expect("spawned");
    nix_kill(pid);
    // Wait for the reader thread to observe EOF.
    for _ in 0..100 {
        if s.is_dead() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(s.is_dead(), "backend death should be detected");
    let err = s.request("shutdown", serde_json::Value::Null).unwrap_err();
    assert!(err.contains("died"), "got: {err}");
}

fn nix_kill(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status();
}

// ---- S2: hover -----------------------------------------------------

#[test]
fn hover_function_def() {
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    let h = hover_impl(&s, &file, 3, 4).unwrap();
    assert!(h.contains("greet") && h.contains("-> str"), "got: {h}");
}

#[test]
fn hover_variable_type() {
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    let h = hover_impl(&s, &file, 7, 0).unwrap();
    assert!(h.contains("msg") && h.contains("str"), "got: {h}");
}

#[test]
fn hover_null_position_returns_no_hover() {
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    // Line 1 is blank — hover there yields null -> bounded retry ->
    // "no hover" (not an error, not a hang).
    let h = hover_impl(&s, &file, 1, 0).unwrap();
    assert!(h.starts_with("no hover"), "got: {h}");
}

#[test]
fn hover_non_python_rejected() {
    let (dir, _sample) = strict_fixture();
    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    let s = session_at(dir.path());
    let err = hover_impl(&s, &txt.to_string_lossy(), 0, 0).unwrap_err();
    assert!(err.contains("not a Python file"), "got: {err}");
}

#[test]
fn hover_missing_file_is_loud() {
    let (dir, _sample) = strict_fixture();
    let s = session_at(dir.path());
    let missing = dir.path().join("ghost.py");
    let err = hover_impl(&s, &missing.to_string_lossy(), 0, 0).unwrap_err();
    assert!(err.contains("cannot read"), "got: {err}");
}

#[test]
fn hover_after_out_of_band_disk_edit() {
    // SM-12: the caller edits the file on disk with its own tools,
    // then hovers — the bridge must serve the disk state.
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    hover_impl(&s, &file, 3, 4).unwrap();
    let edited = SAMPLE.replace("-> str:", "-> int:");
    std::fs::write(&sample, &edited).unwrap();
    let h = hover_impl(&s, &file, 3, 4).unwrap();
    assert!(h.contains("-> int"), "disk edit not picked up: {h}");
}

// ---- S3: diagnostics ------------------------------------------------

#[test]
fn check_strict_reports_both_errors() {
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let out = check_file_impl(&s, &sample.to_string_lossy()).unwrap();
    assert!(out.starts_with("count=2"), "got: {out}");
    assert!(out.contains("bad-assignment"), "got: {out}");
    assert!(!out.contains("[WARN]"), "converged: {out}");
}

#[test]
fn check_repeat_without_edit_returns_cache_immediately() {
    // EP R-07b regression pin: push-only diagnostics — a repeat check
    // with no mutation must answer from the per-URI cache, never with
    // a fake "[WARN] not converged".
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let first = check_file_impl(&s, &sample.to_string_lossy()).unwrap();
    assert!(first.starts_with("count=2"));
    let start = std::time::Instant::now();
    let second = check_file_impl(&s, &sample.to_string_lossy()).unwrap();
    assert_eq!(first, second, "cache answer must be stable");
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "cache answer must be immediate, took {:?}",
        start.elapsed()
    );
}

#[test]
fn check_non_python_rejected() {
    let (dir, _sample) = strict_fixture();
    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    let s = session_at(dir.path());
    let err = check_file_impl(&s, &txt.to_string_lossy()).unwrap_err();
    assert!(err.contains("not a Python file"), "got: {err}");
}

// ---- S4: edit + recheck (streaming face) ----------------------------

#[test]
fn edit_then_check_reflects_new_content() {
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    let before = check_file_impl(&s, &file).unwrap();
    assert!(before.starts_with("count=2"));

    // Flip the return annotation: `-> str` becomes `-> int`. Effects:
    // `return "hello " + name` becomes a bad-return (new), `msg` is now
    // int so the 8:13 assignment error disappears, nan error stays.
    // Net: 2 errors but a DIFFERENT set — bad-return + nan.
    let edited = SAMPLE.replace("-> str:", "-> int:");
    let ack = edit_file_impl(&s, &file, &edited).unwrap();
    assert!(ack.starts_with("edited"), "got: {ack}");
    let after = check_file_impl(&s, &file).unwrap();
    assert!(after.starts_with("count=2"), "got: {after}");
    assert!(after.contains("bad-return"), "new error missing: {after}");
    assert!(!after.contains("8:13"), "stale pre-edit diagnostic served: {after}");

    // Hover reflects the edited signature too (version-gated
    // convergence must not serve the pre-edit hover).
    let h = hover_impl(&s, &file, 3, 4).unwrap();
    assert!(h.contains("-> int"), "got: {h}");
}

#[test]
fn lru_evict_preserves_overlay_edits() {
    // EP R-06/SM-10: evict a dirty file from the server's open set,
    // then check it again — the overlay version must win over disk.
    let (dir, sample) = strict_fixture();
    let s = session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    let edited = SAMPLE.replace("-> str:", "-> int:");
    edit_file_impl(&s, &file, &edited).unwrap();

    // Push 8 other files through to force the LRU eviction of sample.
    for i in 0..8 {
        let f = dir.path().join(format!("filler{i}.py"));
        std::fs::write(&f, format!("x{i}: int = {i}\n")).unwrap();
        hover_impl(&s, &f.to_string_lossy(), 0, 1).unwrap();
    }
    assert!(
        !s.open_files.lock().unwrap().iter().any(|p| p == &sample),
        "sample should have been evicted from the open set"
    );
    let dbg_state = {
        let uri = LspSession::file_uri(&sample);
        let ov = s.overlay.lock().unwrap().get(&sample).cloned();
        let ce = s.diag_cache.lock().unwrap().get(&uri).cloned();
        format!("overlay={:?} cache={:?}", ov.map(|e| e.version), ce.map(|e| (e.version, e.diagnostics.len())))
    };

    // Re-check: overlay content (edited) — bad-return present, the
    // pre-edit 8:13 assignment error absent (msg is int now).
    let out = check_file_impl(&s, &file).unwrap();
    assert!(out.contains("bad-return"), "overlay edits lost ({dbg_state}): {out}");
    assert!(!out.contains("8:13"), "served disk-state diagnostics: {out}");
}

// ---- review-fix regression pins (post-build findings) ---------------

#[test]
fn check_file_cjk_filename_converges() {
    // fresh F1: the bridge's file:// URI must match the backend's
    // percent-encoded form byte-for-byte, or the per-URI cache key
    // never matches and check_file burns its whole 10s deadline.
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("樣本.py");
    std::fs::write(&sample, "import math\n\nnan_invalid: int = math.nan\n").unwrap();
    std::fs::write(dir.path().join("pyrefly.toml"), "preset = \"strict\"\n").unwrap();
    let s = session_at(dir.path());
    let out = check_file_impl(&s, &sample.to_string_lossy()).unwrap();
    assert!(
        !out.contains("[WARN]"),
        "CJK filename failed to converge (URI key mismatch?): {out}"
    );
    assert!(out.contains("bad-assignment"), "got: {out}");
}

#[test]
fn busy_channel_does_not_break_other_file_convergence() {
    // primed F-02 / EP R-07a: pushes for file B (edit storm) must not
    // affect file A's per-URI convergence — the cache is per-URI keyed.
    let (dir, sample) = strict_fixture();
    let other = dir.path().join("storm.py");
    std::fs::write(&other, "x: int = 0\n").unwrap();
    let s = session_at(dir.path());
    let a = sample.to_string_lossy().to_string();
    let b = other.to_string_lossy().to_string();

    // Establish A's cache (converged).
    let a_first = check_file_impl(&s, &a).unwrap();
    assert!(a_first.starts_with("count=2"), "got: {a_first}");

    // Edit storm on B while A stays untouched.
    for i in 0..3 {
        edit_file_impl(&s, &b, &format!("x: int = {i}\ny{i}: str = {i}\n")).unwrap();
    }
    let b_final = check_file_impl(&s, &b).unwrap();
    assert!(!b_final.contains("[WARN]"), "B should converge: {b_final}");
    assert!(b_final.contains("bad-assignment"), "got: {b_final}");

    // A's answer comes straight from its cache entry, unpolluted by B.
    let a_again = check_file_impl(&s, &a).unwrap();
    assert_eq!(a_first, a_again, "A's per-URI answer changed under B's storm");
}

#[test]
fn check_file_timeout_path_warns() {
    // primed F-02 / EP SM-8: an unattainable quiesce window must
    // surface the explicit not-converged marker, never a silent
    // partial. (Slow by design: burns the 10s deadline once.)
    let (dir, sample) = strict_fixture();
    let s = Arc::new(LspSession::new(&backend_bin(), dir.path().to_path_buf(), 60_000));
    let out = check_file_impl(&s, &sample.to_string_lossy()).unwrap();
    assert!(
        out.contains("[WARN] not converged"),
        "timeout marker missing: {out}"
    );
}
