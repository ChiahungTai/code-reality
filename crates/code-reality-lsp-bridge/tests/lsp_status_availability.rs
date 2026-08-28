//! SM-8 smoke pin (EP ep-pypi-wheel-distribution S4, review Y2):
//! `lsp_status` must distinguish "backend binary missing from PATH"
//! from "not spawned yet" — wheel installs ship the bridge without
//! rust-analyzer, and that gap has to surface in status output with
//! install guidance, not a silent not-spawned-yet line.

use std::path::PathBuf;

use code_reality_lsp_bridge::server::{backend_available, status_line};
use code_reality_lsp_bridge::session::{LangSpec, LspSession};

#[test]
fn missing_backend_reports_unavailable_with_hint() {
    let s = LspSession::new(
        "definitely-missing-backend-9f2c",
        PathBuf::from("/"),
        0,
        LangSpec::rust(),
    );
    let line = status_line("rs", &s);
    assert!(line.contains("state=unavailable"), "{line}");
    assert!(
        line.contains("rustup component add rust-analyzer"),
        "{line}"
    );
    assert!(line.contains("definitely-missing-backend-9f2c"), "{line}");
}

#[test]
fn present_backend_reports_session_state() {
    // Absolute-path backend that exists but is never spawned: the
    // probe passes and the session renders its real (un-spawned)
    // state — the availability gate must not swallow it.
    let s = LspSession::new("/bin/cat", PathBuf::from("/"), 0, LangSpec::rust());
    let line = status_line("rs", &s);
    assert!(line.contains("state=alive"), "{line}");
    assert!(line.contains("server=not-spawned-yet"), "{line}");
    assert!(!line.contains("unavailable"), "{line}");
}

#[test]
fn availability_probe_resolves_like_spawn() {
    assert!(backend_available("/bin/cat"));
    assert!(!backend_available("definitely-missing-backend-9f2c"));
}
