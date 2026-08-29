//! P2 integration tests: the rust-analyzer backend through the SAME
//! bridge machinery (extension routing, hover, edit→check convergence,
//! backend independence). Targets run against the real
//! `rust-analyzer` on PATH; hover positions are machine-verified
//! against `cat -n` of framing.rs (P1's line-number lesson).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use code_reality_lsp_bridge::server::{check_file_impl, edit_file_impl, hover_impl, Bridge};
use code_reality_lsp_bridge::session::LangSpec;
use code_reality_lsp_bridge::LspSession;

// framing.rs (0-based): line 11 `pub fn write_message`, line 19
// `pub fn read_message`; mid-identifier columns (ra's hit-test returns
// empty on some boundary positions).
const WRITE_MSG: (u32, u32) = (11, 10);
const READ_MSG: (u32, u32) = (19, 10);

fn framing_rs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/framing.rs")
}

fn bridge_at(root: &Path) -> Arc<Bridge> {
    Arc::new(Bridge::new(
        "pyrefly-lsp",
        "rust-analyzer",
        root.to_path_buf(),
    ))
}

fn rust_session() -> Arc<LspSession> {
    Arc::new(LspSession::new(
        "rust-analyzer",
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
        300,
        LangSpec::rust(),
    ))
}

#[test]
fn route_by_extension() {
    let b = bridge_at(Path::new("/tmp"));
    let (s_py, _) = b.session_for("/tmp/x.py").unwrap();
    let (s_rs, _) = b.session_for("/tmp/x.rs").unwrap();
    assert!(format!("{:p}", &*s_py) != format!("{:p}", &*s_rs));
    let err = match b.session_for("/tmp/x.txt") {
        Err(e) => e,
        Ok(_) => panic!("should reject .txt"),
    };
    assert!(err.contains("unsupported file type"), "got: {err}");
    // Case-sensitive (EP S-F-5).
    assert!(b.session_for("/tmp/x.PY").is_err());
}

#[test]
fn rust_hover_function_signatures() {
    let s = rust_session();
    let file = framing_rs().to_string_lossy().to_string();
    let (line, ch) = READ_MSG;
    let h = hover_impl(&s, &file, line, ch).unwrap();
    assert!(
        h.contains("read_message") && h.contains("```rust"),
        "got: {h}"
    );
}

#[test]
fn rust_edit_then_check_native_diagnostics() {
    // SM-8: in-memory edit converges rust-analyzer's NATIVE
    // diagnostics (flycheck/cargo-check runs on disk content —
    // documented semantic limit, C-F-05).
    let s = rust_session();
    let file = framing_rs().to_string_lossy().to_string();
    // Warm open + establish the cache first.
    let _ = hover_impl(&s, &file, READ_MSG.0, READ_MSG.1).unwrap();

    // Rewrite with a type error: use a `usize` variable as a String.
    let edited = r#"pub fn __bridge_edit_probe(x: usize) -> String {
    let s: String = x;
    s
}
"#;
    edit_file_impl(&s, &file, edited).unwrap();
    let out = check_file_impl(&s, &file).unwrap();
    if out.contains("[WARN]") {
        let uri = LspSession::file_uri(&framing_rs());
        let cache = s.diag_cache.lock().unwrap().get(&uri).cloned();
        eprintln!(
            "[DEBUG] cache={:?} overlay_ver={:?}",
            cache.map(|e| (e.version, e.diagnostics.len())),
            s.overlay
                .lock()
                .unwrap()
                .get(&framing_rs())
                .map(|e| e.version)
        );
    }
    assert!(!out.contains("[WARN]"), "should converge: {out}");
    assert!(
        out.contains("mismatched-types") || out.starts_with("count="),
        "got: {out}"
    );
    // The convergence itself proves ra pushes carry a `version` field
    // (P1's strict Some(version) condition held — C-F-02 resolved).
}

#[test]
fn mixed_language_sessions_are_independent() {
    // SM-3/SM-9: routing both languages through one Bridge spawns both
    // backends lazily and neither disturbs the other. Root = crate
    // root: rust-analyzer loads the workspace from the rootUri (a
    // tempdir root would leave the .rs file detached — EP C-F-04's
    // degraded mode).
    let (dir, sample) = py_fixture();
    let b = bridge_at(Path::new(env!("CARGO_MANIFEST_DIR")));
    let (s_py, _) = b.session_for(&sample.to_string_lossy()).unwrap();
    let (s_rs, _) = b.session_for(&framing_rs().to_string_lossy()).unwrap();

    // CONCURRENT form (SM-9): spawn the rs cold-load hover on a thread
    // (its 30s retry window spans the whole workspace load) and prove
    // the py session answers immediately meanwhile — the per-session
    // interaction locks mean ra's load blocks nobody else.
    let rs = Arc::clone(&s_rs);
    let rs_file = framing_rs().to_string_lossy().to_string();
    let handle =
        std::thread::spawn(move || hover_impl(&rs, &rs_file, READ_MSG.0, READ_MSG.1).unwrap());
    std::thread::sleep(Duration::from_millis(200)); // let the rs load start
    let t0 = Instant::now();
    let h_py = hover_impl(&s_py, &sample.to_string_lossy(), 3, 4).unwrap();
    let py_elapsed = t0.elapsed();
    assert!(h_py.contains("greet"), "py hover broken: {h_py}");
    assert!(
        py_elapsed < Duration::from_secs(15),
        "py hover blocked {py_elapsed:?} while rs was cold-loading"
    );
    let h_rs = handle.join().unwrap();
    assert!(h_rs.contains("read_message"), "rs hover broken: {h_rs}");
    b.shutdown_all();
}

#[test]
fn rust_backend_death_leaves_python_alive() {
    // SM-7: kill the ra backend; the pyrefly session keeps serving.
    let (dir, sample) = py_fixture();
    let b = bridge_at(dir.path());
    let (s_py, _) = b.session_for(&sample.to_string_lossy()).unwrap();
    let (s_rs, _) = b.session_for(&framing_rs().to_string_lossy()).unwrap();
    let _ = hover_impl(
        &s_rs,
        &framing_rs().to_string_lossy(),
        READ_MSG.0,
        READ_MSG.1,
    )
    .unwrap();
    let pid = s_rs.backend_pid().expect("ra spawned");
    let _ = std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status();
    for _ in 0..100 {
        if s_rs.is_dead() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(s_rs.is_dead(), "ra death not detected");
    let err = hover_impl(
        &s_rs,
        &framing_rs().to_string_lossy(),
        READ_MSG.0,
        READ_MSG.1,
    )
    .unwrap_err();
    assert!(err.contains("died"), "got: {err}");
    // pyrefly unaffected.
    let h = hover_impl(&s_py, &sample.to_string_lossy(), 3, 4).unwrap();
    assert!(h.contains("greet"), "py died with ra: {h}");
}

/// Strict-preset Python fixture (same shape as tests/bridge.rs).
fn py_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.py");
    std::fs::write(
        &sample,
        "import math\n\n\ndef greet(name: str) -> str:\n    return \"hello \" + name\n\n\nmsg = greet(\"world\")\ncount: int = msg\nnan_invalid: int = math.nan\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("pyrefly.toml"), "preset = \"strict\"\n").unwrap();
    (dir, sample)
}

#[test]
fn rust_latency_budget_numbers() {
    // P2 gate: record the latency budget this workspace exhibits
    // (cold-load first hover vs warm hover). Asserts generous bounds
    // only — the numbers are printed for the EP settlement.
    let s = rust_session();
    let file = framing_rs().to_string_lossy().to_string();
    let t0 = Instant::now();
    let _ = hover_impl(&s, &file, WRITE_MSG.0, WRITE_MSG.1).unwrap();
    let cold = t0.elapsed();
    let t1 = Instant::now();
    let _ = hover_impl(&s, &file, READ_MSG.0, READ_MSG.1).unwrap();
    let warm = t1.elapsed();
    println!("[LATENCY] cold first hover: {cold:?}, warm hover: {warm:?}");
    assert!(cold < Duration::from_secs(60), "cold load absurd: {cold:?}");
    assert!(warm < Duration::from_secs(10), "warm hover slow: {warm:?}");
}
