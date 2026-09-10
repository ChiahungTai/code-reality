//! S5 (JS/TS language face) tests. Three lanes:
//! - contract tests for the typed `BackendCommand` + generalized `LangSpec`
//!   (no backend required);
//! - hermetic lanes through `examples/fake_lsp_server.rs` (a std-only fake
//!   TypeScript language server — the default suite must not require Node/npm;
//!   the real `typescript-language-server` acceptance is a separate lane);
//! - cross-backend death isolation through the real pyrefly/rust-analyzer
//!   backends (same resolution policy as tests/bridge.rs).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use code_reality_lsp_bridge::server::{
    check_file_impl, edit_file_impl, hover_impl, resolve_typescript_backend_with, status_line,
    Bridge,
};
use code_reality_lsp_bridge::session::{BackendCommand, LangSpec};
use code_reality_lsp_bridge::LspSession;

mod common;

/// One real-rust-analyzer cold load at a time (same policy as
/// tests/rust_backend.rs; test binaries are serial, this covers threads).
static RA_SERIAL: Mutex<()> = Mutex::new(());

// ---- Segment 1: typed backend command -------------------------------

#[test]
fn backend_command_constructor_shapes() {
    let py = BackendCommand::python("pyrefly-lsp");
    assert_eq!(py.program, "pyrefly-lsp");
    assert!(py.args.is_empty(), "python backend takes no args");

    let rs = BackendCommand::rust("rust-analyzer");
    assert_eq!(rs.program, "rust-analyzer");
    assert!(rs.args.is_empty(), "rust backend takes no args");

    let ts = BackendCommand::typescript("typescript-language-server");
    assert_eq!(ts.program, "typescript-language-server");
    assert_eq!(ts.args, vec!["--stdio".to_string()]);
}

#[test]
fn backend_command_display_renders_program_plus_args() {
    let py = BackendCommand::python("pyrefly-lsp");
    assert_eq!(format!("{py}"), "pyrefly-lsp");
    let ts = BackendCommand::typescript("/usr/local/bin/tls");
    assert_eq!(format!("{ts}"), "/usr/local/bin/tls --stdio");
}

// ---- Segment 1: generalized language policy --------------------------

#[test]
fn langspec_extension_table_exact() {
    let cases: &[(&str, &str)] = &[
        ("py", "python"),
        ("rs", "rust"),
        ("js", "javascript"),
        ("mjs", "javascript"),
        ("cjs", "javascript"),
        ("jsx", "javascriptreact"),
        ("ts", "typescript"),
        ("tsx", "typescriptreact"),
    ];
    for (ext, id) in cases {
        let spec = match *ext {
            "py" => LangSpec::python(),
            "rs" => LangSpec::rust(),
            _ => LangSpec::typescript(),
        };
        assert!(
            spec.supports_extension(ext),
            "{ext} should be supported by its family"
        );
        assert_eq!(
            spec.language_id(ext),
            Some(*id),
            "extension {ext} must map to language id {id}"
        );
    }
}

#[test]
fn langspec_rejects_foreign_and_unsupported_extensions() {
    // Cross-family extension must not resolve, and unknown extensions are
    // rejected (the bridge's routing gate rejects before any spawn).
    assert_eq!(LangSpec::typescript().language_id("py"), None);
    assert!(!LangSpec::typescript().supports_extension("py"));
    assert_eq!(LangSpec::python().language_id("rs"), None);
    assert_eq!(LangSpec::rust().language_id("vue"), None);
    assert!(!LangSpec::typescript().supports_extension("vue"));
    // Case-sensitive (EP S-F-5 heritage).
    assert!(!LangSpec::typescript().supports_extension("TS"));
    // The six JS/TS faces belong to exactly one family spec.
    for ext in ["js", "jsx", "mjs", "cjs", "ts", "tsx"] {
        assert!(LangSpec::typescript().supports_extension(ext));
    }
}

// ---- fake-server infrastructure -------------------------------------

/// The fake TypeScript language server, resolved ONCE per test process
/// (every caller shares the same binary — compiling per miss raced when
/// concurrent tests hit the fallback path together). Primary source: the
/// cargo-built example next to this test binary's profile dir. Fallback:
/// compile the example source once with rustc into CARGO_TARGET_TMPDIR
/// (so the lane never depends on one cargo build-order behavior).
static FAKE_SERVER: OnceLock<PathBuf> = OnceLock::new();

/// Process-local uniqueness for fallback compile temp paths.
static FAKE_COMPILE_SEQ: AtomicU64 = AtomicU64::new(0);

fn fake_server_bin() -> PathBuf {
    FAKE_SERVER
        .get_or_init(|| {
            let exe_name = format!("fake_lsp_server{}", std::env::consts::EXE_SUFFIX);
            if let Ok(exe) = std::env::current_exe() {
                // <profile>/deps/<test-binary> → <profile>/examples/<fake>
                if let Some(profile) = exe.parent().and_then(|p| p.parent()) {
                    let cand = profile.join("examples").join(&exe_name);
                    if cand.exists() {
                        return cand;
                    }
                }
            }
            let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(&exe_name);
            let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fake_lsp_server.rs");
            // Staleness check: a cached binary older than the source
            // recompiles (a stale binary once served an abandoned wire
            // format here).
            let fresh = std::fs::metadata(&out)
                .and_then(|o| {
                    std::fs::metadata(&src).map(|s| {
                        o.modified()
                            .ok()
                            .zip(s.modified().ok())
                            .is_none_or(|(om, sm)| om >= sm)
                    })
                })
                .unwrap_or(false);
            if !fresh {
                // Unique temp name (pid + process-local counter): no two
                // compilers — across threads or processes — ever share a
                // path, and rename() atomically replaces the published
                // binary (loud failure instead of a silently missing one).
                let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
                    "fake_lsp_server.{}.{}.tmp",
                    std::process::id(),
                    FAKE_COMPILE_SEQ.fetch_add(1, Ordering::Relaxed),
                ));
                let status = std::process::Command::new("rustc")
                    .args(["--edition", "2021"])
                    .arg(&src)
                    .arg("-o")
                    .arg(&tmp)
                    .status()
                    .expect("run rustc for the fake LSP server");
                assert!(status.success(), "rustc failed for {}", src.display());
                std::fs::rename(&tmp, &out).expect("publish the compiled fake LSP server");
            }
            out
        })
        .clone()
}

fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A TypeScript session over the fake server (fast quiesce — the fake
/// pushes deterministically right after each mutation).
fn ts_session_at(root: &Path) -> Arc<LspSession> {
    Arc::new(LspSession::new(
        fake_ts_command(),
        root.to_path_buf(),
        50,
        LangSpec::typescript(),
    ))
}

fn fake_ts_command() -> BackendCommand {
    BackendCommand::typescript(fake_server_bin().to_string_lossy().into_owned())
}

/// All recorded argv lines from the fake's FAKE_LSP_ARGV_OUT file.
fn read_argv_lines(path: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| {
            serde_json::from_str::<Vec<String>>(l)
                .unwrap_or_else(|e| panic!("bad argv line {l:?}: {e}"))
        })
        .collect()
}

/// A per-test wrapper script that gives the shared fake server a
/// CHILD-LOCAL `FAKE_LSP_ARGV_OUT` pointing at this test's private
/// evidence file, then execs the fake with the received arguments.
/// The variable is never exported process-globally: a sibling thread's
/// fake spawn must not be able to append into this test's evidence.
fn argv_evidence_wrapper(dir: &Path, evidence: &Path) -> String {
    let wrapper = dir.join("fake_lsp_argv_wrapper.sh");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexport FAKE_LSP_ARGV_OUT=\"{}\"\nexec \"{}\" \"$@\"\n",
            evidence.to_string_lossy(),
            fake_server_bin().to_string_lossy(),
        ),
    )
    .unwrap();
    make_executable(&wrapper);
    wrapper.to_string_lossy().into_owned()
}

// ---- Segment 2: spawn argv (typed args contract) --------------------

#[test]
fn spawn_argv_equivalence_and_stdio() {
    // Each constructor family spawns the fake through its own wrapper
    // exporting a private FAKE_LSP_ARGV_OUT, so every evidence file holds
    // exactly this test's spawns — asserted EXACTLY (one spawn ⇒ one
    // line), with no sibling-append tolerance.
    let fake_str = fake_server_bin().to_string_lossy().to_string();
    let dir = tempfile::tempdir().unwrap();

    // Python constructor: no args (the fake exits(9) on the missing
    // --stdio — the handshake error below is EXPECTED; the argv file is
    // written before the guard).
    let py_out = dir.path().join("py.argv");
    let py = Arc::new(LspSession::new(
        BackendCommand::python(argv_evidence_wrapper(dir.path(), &py_out)),
        dir.path().to_path_buf(),
        50,
        LangSpec::python(),
    ));
    let _ = py.request("shutdown", serde_json::Value::Null);
    assert_eq!(
        read_argv_lines(&py_out),
        vec![vec![fake_str.clone()]],
        "python constructor must spawn exactly [program] and nothing else"
    );

    // Rust constructor: same shape.
    let rs_out = dir.path().join("rs.argv");
    let rs = Arc::new(LspSession::new(
        BackendCommand::rust(argv_evidence_wrapper(dir.path(), &rs_out)),
        dir.path().to_path_buf(),
        50,
        LangSpec::rust(),
    ));
    let _ = rs.request("shutdown", serde_json::Value::Null);
    assert_eq!(
        read_argv_lines(&rs_out),
        vec![vec![fake_str.clone()]],
        "rust constructor must spawn exactly [program] and nothing else"
    );

    // TypeScript constructor: exactly [program, --stdio] — and the
    // handshake succeeds because the typed arg reaches the fake through
    // the wrapper's "$@".
    let ts_out = dir.path().join("ts.argv");
    let ts = Arc::new(LspSession::new(
        BackendCommand::typescript(argv_evidence_wrapper(dir.path(), &ts_out)),
        dir.path().to_path_buf(),
        50,
        LangSpec::typescript(),
    ));
    ts.request("shutdown", serde_json::Value::Null)
        .expect("fake serves the initialize handshake under --stdio");
    assert_eq!(
        read_argv_lines(&ts_out),
        vec![vec![fake_str.clone(), "--stdio".to_string()]],
        "typescript constructor must spawn exactly [program, --stdio]"
    );
}

#[test]
fn typescript_backend_requires_stdio_arg() {
    // Pins the fake's guard AND the point of the typed args: a backend
    // command without the --stdio arg cannot serve the TS server.
    let dir = tempfile::tempdir().unwrap();
    let s = LspSession::new(
        BackendCommand {
            program: fake_server_bin().to_string_lossy().into_owned(),
            args: vec![],
        },
        dir.path().to_path_buf(),
        50,
        LangSpec::typescript(),
    );
    let err = s
        .request("shutdown", serde_json::Value::Null)
        .expect_err("fake exits(9) without --stdio");
    assert!(
        err.contains("initialize handshake failed") || err.contains("died"),
        "got: {err}"
    );
}

// ---- Segment 2: routing ---------------------------------------------

#[test]
fn six_extensions_route_with_exact_language_ids() {
    // SM-1..SM-6: every JS/TS face reaches the third session and opens
    // with its exact LSP languageId (echoed by the fake's hover marker).
    let dir = tempfile::tempdir().unwrap();
    let s = ts_session_at(dir.path());
    let cases: &[(&str, &str)] = &[
        ("sample.js", "javascript"),
        ("sample.jsx", "javascriptreact"),
        ("sample.mjs", "javascript"),
        ("sample.cjs", "javascript"),
        ("sample.ts", "typescript"),
        ("sample.tsx", "typescriptreact"),
    ];
    for (name, lang) in cases {
        let f = dir.path().join(name);
        std::fs::write(&f, "const value: number = 1;\n").unwrap();
        let h = hover_impl(&s, &f.to_string_lossy(), 0, 6).unwrap();
        assert!(
            h.contains(&format!("languageId={lang}")),
            "{name} must open as {lang}, got: {h}"
        );
        assert!(h.contains("fake-ts hover"), "non-null hover face: {h}");
    }
}

#[test]
fn session_for_routes_eight_extensions_before_spawn() {
    // Routing decides per family matcher and NEVER spawns: a wrong
    // extension must fail loud before any backend comes up (SM-15).
    let b = Bridge::new(
        BackendCommand::python("definitely-missing-py-s5"),
        BackendCommand::rust("definitely-missing-rs-s5"),
        BackendCommand::typescript("definitely-missing-tls-s5"),
        PathBuf::from("/"),
    );
    let (s_py_route, _) = b.session_for("/tmp/x.py").unwrap();
    assert!(
        Arc::ptr_eq(&s_py_route, &b.py),
        ".py routes to the python session"
    );
    let (s_rs_route, _) = b.session_for("/tmp/x.rs").unwrap();
    assert!(
        Arc::ptr_eq(&s_rs_route, &b.rs),
        ".rs routes to the rust session"
    );
    for f in [
        "/tmp/x.js",
        "/tmp/x.jsx",
        "/tmp/x.mjs",
        "/tmp/x.cjs",
        "/tmp/x.ts",
        "/tmp/x.tsx",
    ] {
        let (s, _) = b.session_for(f).unwrap();
        assert!(Arc::ptr_eq(&s, &b.ts), "{f} routes to the ts session");
    }
    // Nothing was spawned by routing.
    for s in [&b.py, &b.rs, &b.ts] {
        assert!(s.backend_pid().is_none(), "routing must not spawn");
    }
    // Case-sensitive extensions (EP S-F-5 heritage).
    assert!(b.session_for("/tmp/x.PY").is_err());
    assert!(b.session_for("/tmp/x.TS").is_err());
    let err = match b.session_for("/tmp/x.vue") {
        Err(e) => e,
        Ok(_) => panic!(".vue must be rejected"),
    };
    assert!(err.contains("unsupported file type"), "got: {err}");
    // The loud error lists the full supported surface (Segment 3 text).
    for ext in [".py", ".rs", ".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx"] {
        assert!(err.contains(ext), "error must list {ext}: {err}");
    }
}

// ---- Segment 2: diagnostics over the fake server --------------------

#[test]
fn ts_diagnostics_push_then_converge_after_correction() {
    // SM-7: bad edit pushes diagnostics through the range-form
    // full-content didChange; the corrected edit converges to zero.
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.ts");
    std::fs::write(&sample, "const greeting: string = 'hi';\n").unwrap();
    let s = ts_session_at(dir.path());
    let file = sample.to_string_lossy().to_string();

    let before = check_file_impl(&s, &file).unwrap();
    assert!(before.starts_with("count=0"), "clean file: {before}");

    edit_file_impl(&s, &file, "FAKE_TS_ERROR const greeting: string = 1;\n").unwrap();
    let bad = check_file_impl(&s, &file).unwrap();
    assert!(bad.starts_with("count=1"), "marker diagnostic: {bad}");
    assert!(bad.contains("fake-error"), "{bad}");

    edit_file_impl(&s, &file, "const greeting: string = 'fixed';\n").unwrap();
    let good = check_file_impl(&s, &file).unwrap();
    assert!(good.starts_with("count=0"), "converged after fix: {good}");
}

#[test]
fn lru_close_reopen_retains_unpersisted_edit() {
    // SM-13: eviction from the server's open set must not roll the
    // overlay back to disk — the re-open replays the edited content and
    // the fresh diagnostics prove it (the eviction's didClose drops the
    // cached entry, so a converging marker push can only come from the
    // overlay replay).
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.ts");
    std::fs::write(&sample, "const base: number = 0;\n").unwrap();
    let s = ts_session_at(dir.path());
    let file = sample.to_string_lossy().to_string();
    let edited = "FAKE_TS_ERROR const edited: number = 1;\n";
    edit_file_impl(&s, &file, edited).unwrap();

    for i in 0..8 {
        let f = dir.path().join(format!("filler{i}.ts"));
        std::fs::write(&f, format!("const f{i}: number = {i};\n")).unwrap();
        hover_impl(&s, &f.to_string_lossy(), 0, 6).unwrap();
    }
    assert!(
        !s.open_files.lock().unwrap().iter().any(|p| p == &sample),
        "sample should have been evicted from the open set"
    );

    let out = check_file_impl(&s, &file).unwrap();
    assert!(
        !out.contains("[WARN]"),
        "overlay replay should converge: {out}"
    );
    assert!(out.starts_with("count=1"), "overlay edit served: {out}");
    assert!(out.contains("fake-error"), "{out}");
}

#[test]
fn out_of_band_disk_edit_is_synced() {
    // SM-14: a disk edit behind the bridge's back is picked up before
    // the next hover (the fake echoes the doc length it holds).
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.ts");
    std::fs::write(&sample, "const a: string = 'x';\n").unwrap();
    let s = ts_session_at(dir.path());
    let file = sample.to_string_lossy().to_string();

    let h1 = hover_impl(&s, &file, 0, 6).unwrap();
    std::fs::write(&sample, "const a: string = 'x';\n// edited on disk\n").unwrap();
    let h2 = hover_impl(&s, &file, 0, 6).unwrap();
    assert_ne!(h1, h2, "out-of-band edit must change the served doc");
    let len_of = |h: &str| {
        h.rsplit("len=")
            .next()
            .and_then(|t| t.trim().parse::<usize>().ok())
            .unwrap_or(0)
    };
    assert!(
        len_of(&h2) > len_of(&h1),
        "disk state must win after an out-of-band edit: {h1} vs {h2}"
    );
}

// ---- Segment 2: death isolation (real py + ra, fake ts) -------------

fn py_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.py");
    std::fs::write(
        &sample,
        "def greet(name: str) -> str:\n    return \"hello \" + name\n\n\nmsg = greet(\"world\")\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("pyrefly.toml"), "preset = \"strict\"\n").unwrap();
    (dir, sample)
}

fn ts_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("sample.ts");
    std::fs::write(&sample, "const value: number = 1;\n").unwrap();
    (dir, sample)
}

fn nix_kill(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status();
}

fn wait_dead(s: &LspSession) {
    for _ in 0..100 {
        if s.is_dead() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("backend death not detected");
}

#[test]
fn backend_death_isolation_both_directions() {
    // SM-11 + SM-12: killing the JS/TS child leaves Python AND Rust
    // answering; killing the Python child leaves JS/TS AND Rust
    // answering. One rust-analyzer cold load is shared by both
    // directions (the sessions stay warm across the kill).
    let _ra = RA_SERIAL.lock().unwrap();
    let (_py_dir, py_sample) = py_fixture();
    let (_ts_dir, ts_sample) = ts_fixture();
    let b = Bridge::new(
        BackendCommand::python(common::backend_bin()),
        BackendCommand::rust("rust-analyzer"),
        fake_ts_command(),
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
    );
    let rs_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/framing.rs")
        .to_string_lossy()
        .to_string();
    let (s_py, _) = b.session_for(&py_sample.to_string_lossy()).unwrap();
    let (s_rs, _) = b.session_for(&rs_file).unwrap();
    let (s_ts, _) = b.session_for(&ts_sample.to_string_lossy()).unwrap();

    // Warm all three.
    let h_py = hover_impl(&s_py, &py_sample.to_string_lossy(), 0, 4).unwrap();
    assert!(h_py.contains("greet"), "py warm: {h_py}");
    let h_ts = hover_impl(&s_ts, &ts_sample.to_string_lossy(), 0, 6).unwrap();
    assert!(h_ts.contains("fake-ts hover"), "ts warm: {h_ts}");
    let h_rs = hover_impl(&s_rs, &rs_file, 19, 10).unwrap();
    assert!(!h_rs.is_empty(), "rs warm");

    // SM-11: kill the JS/TS backend.
    let ts_pid = s_ts.backend_pid().expect("ts spawned");
    nix_kill(ts_pid);
    wait_dead(&s_ts);
    let err = hover_impl(&s_ts, &ts_sample.to_string_lossy(), 0, 6).unwrap_err();
    assert!(err.contains("died"), "ts death is loud: {err}");
    let h_py2 = hover_impl(&s_py, &py_sample.to_string_lossy(), 0, 4).unwrap();
    assert!(h_py2.contains("greet"), "py died with ts: {h_py2}");
    let h_rs2 = hover_impl(&s_rs, &rs_file, 19, 10).unwrap();
    assert!(!h_rs2.is_empty(), "rs died with ts");

    // SM-12: kill the Python backend.
    let py_pid = s_py.backend_pid().expect("py spawned");
    nix_kill(py_pid);
    wait_dead(&s_py);
    let err = hover_impl(&s_py, &py_sample.to_string_lossy(), 0, 4).unwrap_err();
    assert!(err.contains("died"), "py death is loud: {err}");
    // The dead ts session stays explicitly dead (no silent half-state).
    assert!(s_ts.is_dead());
    let h_ts2 = hover_impl(
        &Arc::new(LspSession::new(
            fake_ts_command(),
            Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
            50,
            LangSpec::typescript(),
        )),
        &ts_sample.to_string_lossy(),
        0,
        6,
    )
    .unwrap();
    assert!(h_ts2.contains("fake-ts hover"), "fresh ts serves: {h_ts2}");
    let h_rs3 = hover_impl(&s_rs, &rs_file, 19, 10).unwrap();
    assert!(!h_rs3.is_empty(), "rs died with py");
    b.shutdown_all();
}

// ---- Segment 2: executable resolution --------------------------------

#[test]
fn resolver_prefers_workspace_node_modules() {
    let ws = tempfile::tempdir().unwrap();
    let bin_dir = ws.path().join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let local = bin_dir.join("typescript-language-server");
    std::fs::write(&local, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&local);
    let cmd = resolve_typescript_backend_with(
        None,
        ws.path(),
        OsStr::new("/definitely-nonexistent-path"),
        None,
    );
    assert_eq!(cmd.program, local.to_string_lossy().to_string());
    assert_eq!(cmd.args, vec!["--stdio".to_string()]);
}

#[test]
fn resolver_honors_node_bin_dir_roots() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    // Present only in the SECOND root: the path-list is scanned in order.
    let cand = second.path().join("typescript-language-server");
    std::fs::write(&cand, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&cand);
    let roots = std::env::join_paths([first.path(), second.path()]).unwrap();
    let cmd = resolve_typescript_backend_with(
        None,
        Path::new("/definitely-nonexistent-workspace"),
        OsStr::new("/definitely-nonexistent-path"),
        Some(roots.as_os_str()),
    );
    assert_eq!(cmd.program, cand.to_string_lossy().to_string());
}

#[test]
fn resolver_falls_back_to_bare_name_on_miss() {
    // Nothing resolvable anywhere: the bare program name stays the
    // contract — availability and spawn resolve it the same (PATH) way,
    // and status reports unavailable instead of failing startup.
    let cmd = resolve_typescript_backend_with(
        None,
        Path::new("/definitely-nonexistent-workspace"),
        OsStr::new("/definitely-nonexistent-path"),
        None,
    );
    assert_eq!(cmd.program, "typescript-language-server");
}

#[test]
fn resolver_explicit_override_short_circuits() {
    // An explicit override wins regardless of resolvability — same
    // semantics as --lsp-command/--rust-backend (loud spawn failure if
    // wrong; never a silent substitution).
    let cmd = resolve_typescript_backend_with(
        Some("/custom/tls-wrapper"),
        Path::new("/"),
        OsStr::new("/"),
        None,
    );
    assert_eq!(cmd.program, "/custom/tls-wrapper");
    assert_eq!(cmd.args, vec!["--stdio".to_string()]);
}

#[test]
fn workspace_node_modules_backend_is_spawned() {
    // SM-9 end-to-end: a repo-local executable (a wrapper script execing
    // the fake) is resolved under a minimal PATH and serves the session —
    // proving the resolved program received the typed --stdio arg.
    let ws = tempfile::tempdir().unwrap();
    let bin_dir = ws.path().join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let local = bin_dir.join("typescript-language-server");
    let fake = fake_server_bin();
    std::fs::write(
        &local,
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", fake.to_string_lossy()),
    )
    .unwrap();
    make_executable(&local);

    let b = Bridge::new(
        BackendCommand::python("definitely-missing-py-s5"),
        BackendCommand::rust("definitely-missing-rs-s5"),
        resolve_typescript_backend_with(
            None,
            ws.path(),
            OsStr::new("/definitely-nonexistent-path"),
            None,
        ),
        ws.path().to_path_buf(),
    );
    let sample = ws.path().join("probe.ts");
    std::fs::write(&sample, "const probed: number = 1;\n").unwrap();
    let (s, _) = b.session_for(&sample.to_string_lossy()).unwrap();
    let h = hover_impl(&s, &sample.to_string_lossy(), 0, 6).unwrap();
    assert!(
        h.contains("fake-ts hover") && h.contains("languageId=typescript"),
        "repo-local backend served the session: {h}"
    );
    b.shutdown_all();
}

// ---- Segment 3: status face -----------------------------------------

#[test]
fn status_report_lists_three_families_in_order() {
    let b = Bridge::new(
        BackendCommand::python("definitely-missing-py-s5"),
        BackendCommand::rust("definitely-missing-rs-s5"),
        BackendCommand::typescript("definitely-missing-tls-s5"),
        PathBuf::from("/"),
    );
    let report = b.status_report();
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines.len(), 3, "one line per family: {report}");
    assert!(lines[0].starts_with("py:"), "{report}");
    assert!(lines[1].starts_with("rs:"), "{report}");
    assert!(lines[2].starts_with("ts:"), "{report}");
    assert!(
        lines[2].contains("definitely-missing-tls-s5 --stdio"),
        "ts line shows the typed display form: {report}"
    );
    assert!(lines[2].contains("state=unavailable"), "{report}");
}

#[test]
fn status_missing_ts_backend_reports_unavailable_with_guidance() {
    // SM-8: the missing JS/TS backend is a family-level unavailable, not
    // a server-wide failure; guidance names the server, the TypeScript
    // runtime + Node, and the resolution roots.
    let s = LspSession::new(
        BackendCommand::typescript("definitely-missing-tls-s5"),
        PathBuf::from("/"),
        0,
        LangSpec::typescript(),
    );
    let line = status_line("ts", &s);
    assert!(line.contains("state=unavailable"), "{line}");
    assert!(line.contains("typescript-language-server"), "{line}");
    assert!(line.contains("Node"), "{line}");
    assert!(line.contains("typescript"), "{line}");
    assert!(line.contains("CODE_REALITY_NODE_BIN_DIR"), "{line}");
    assert!(line.contains("node_modules"), "{line}");
}

#[test]
fn status_present_ts_backend_reports_session_state() {
    // An existing absolute-path backend that is never spawned must render
    // its real state — the availability gate must not swallow it.
    let s = LspSession::new(
        BackendCommand::typescript("/bin/cat"),
        PathBuf::from("/"),
        0,
        LangSpec::typescript(),
    );
    let line = status_line("ts", &s);
    assert!(line.contains("state=alive"), "{line}");
    assert!(line.contains("server=not-spawned-yet"), "{line}");
    assert!(!line.contains("unavailable"), "{line}");
}
