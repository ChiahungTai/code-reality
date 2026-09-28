//! P2 equivalence battery: bridge-internal vs direct-client
//! round-trip consistency against the frozen rust-analyzer baseline
//! (same-engine oracle — the P2 gate). Normalization joins ALL
//! ```rust fences in order (module path + signature); the battery
//! first asserts the PATH rust-analyzer version matches the frozen
//! one (drift is a hard failure in every mode) and warms up with a
//! discarded hover (during workspace load the module-path fence may
//! be absent).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use code_reality_lsp_bridge::server::hover_impl;
use code_reality_lsp_bridge::session::{BackendCommand, LangSpec};
use code_reality_lsp_bridge::LspSession;

fn normalize(hover: &str) -> String {
    let body = hover.trim();
    let mut parts: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("```rust\n") {
        let after = &rest[start + "```rust\n".len()..];
        match after.find("```") {
            Some(end) => {
                parts.push(after[..end].trim().to_string());
                rest = &after[end + 3..];
            }
            None => break,
        }
    }
    parts.join(" | ")
}

#[test]
fn rust_hover_roundtrip_vs_frozen_baseline() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ra_equivalence");
    let baseline_raw = std::fs::read_to_string(fixture_dir.join("ra_hover_baseline.json")).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&baseline_raw).unwrap();

    let session = Arc::new(LspSession::new(
        BackendCommand::rust("rust-analyzer"),
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
        300,
        LangSpec::rust(),
    ));

    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/framing.rs")
        .to_string_lossy()
        .to_string();

    // Opening a document initializes the session without shutting it down.
    session.sync_open(Path::new(&file)).unwrap();
    require_version(
        &session.server_info(),
        baseline["_version"].as_str().unwrap(),
    );
    let source = std::fs::read_to_string(&file).unwrap();
    let cases = ["write_message", "read_message"].map(|label| {
        let anchor = format!("pub fn {label}<");
        let (line, text) = source
            .lines()
            .enumerate()
            .find(|(_, text)| text.starts_with(&anchor))
            .unwrap_or_else(|| panic!("missing declaration anchor {label}"));
        (label, line as u32, (text.find(label).unwrap() + 2) as u32)
    });
    for (label, line, ch) in cases {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let hover = hover_impl(&session, &file, line, ch).unwrap();
            let got = normalize(&hover);
            // During load the module-path fence may be missing — the
            // got text then differs; retry until stable or deadline.
            if got.contains("framing") {
                let want = baseline
                    .pointer(&format!("/_positions/{label}"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("{label}: missing in baseline"));
                assert_eq!(
                    got, want,
                    "{label} ({line}:{ch}) round-trip mismatch\nraw: {hover}"
                );
                eprintln!("[OK] frozen hover {label} executed at {line}:{ch}");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{label}: module path never stabilized"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    session.shutdown().unwrap();
}

// S: frozen version identity includes release, revision AND build date.
fn require_version(live: &str, frozen: &str) {
    let live = live
        .strip_prefix("rust-analyzer ")
        .expect("unexpected server name");
    assert_eq!(live, frozen, "rust-analyzer version drift: equivalence was NOT verified; review the oracle independently");
}

#[test]
fn full_version_identity_never_silently_skips() {
    let frozen = "1.96.0 (ac68faa2 2026-05-25)";
    require_version(&format!("rust-analyzer {frozen}"), frozen);
    for different in [
        "1.96.1 (ac68faa2 2026-05-25)",
        "1.96.0 (deadbeef 2026-05-25)",
        "1.96.0 (ac68faa2 2026-05-26)",
    ] {
        assert!(std::panic::catch_unwind(|| require_version(
            &format!("rust-analyzer {different}"),
            frozen
        ))
        .is_err());
    }
}
