//! P2 equivalence battery: bridge-internal vs direct-client
//! round-trip consistency against the frozen rust-analyzer baseline
//! (same-engine oracle — the P2 gate). Normalization joins ALL
//! ```rust fences in order (module path + signature); the battery
//! first asserts the PATH rust-analyzer version matches the frozen
//! one (drift → loud skip, not a false fail) and warms up with a
//! discarded hover (during workspace load the module-path fence may
//! be absent).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use code_reality_lsp_bridge::server::hover_impl;
use code_reality_lsp_bridge::session::LangSpec;
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
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ra_equivalence");
    let baseline_raw =
        std::fs::read_to_string(fixture_dir.join("ra_hover_baseline.json")).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&baseline_raw).unwrap();

    let session = Arc::new(LspSession::new(
        "rust-analyzer",
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
        300,
        LangSpec::rust(),
    ));

    // Version pin: rust-analyzer on PATH must match the frozen one;
    // drift skips LOUDLY rather than failing on text drift.
    if let Err(e) = session.request("shutdown", serde_json::Value::Null) {
        panic!("ra failed to start: {e}");
    }
    let live_version = session.server_info();
    let frozen = baseline["_version"].as_str().unwrap_or("?");
    // server_info is "<name> <version>"; compare the version token.
    let live = live_version.rsplit(' ').next().unwrap_or("");
    let frozen_v = frozen.split(' ').next().unwrap_or("");
    if live != frozen_v {
        // BRIDGE_STRICT_BATTERY=1 turns the loud skip into a fail
        // (local acceptance runs); plain CI keeps it as a skip.
        if std::env::var("BRIDGE_STRICT_BATTERY").ok().as_deref() == Some("1") {
            panic!(
                "rust-analyzer version drift under strict battery: live {live} vs frozen {frozen_v} — regenerate the baseline"
            );
        }
        eprintln!(
            "[SKIP] rust-analyzer version drift: live {live} vs frozen {frozen_v} — regenerate the baseline"
        );
        return;
    }

    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/framing.rs")
        .to_string_lossy()
        .to_string();

    // Warm-up hover (discarded): module-path fence appears only after
    // the workspace finishes loading.
    let warmed = hover_impl(&session, &file, 11, 10).unwrap();
    if !warmed.contains("framing") {
        eprintln!("[WARN] warm-up hover lacks module path: {warmed}");
    }

    let cases: &[(&str, u32, u32)] = &[("write_message", 11, 10), ("read_message", 19, 10)];
    for (label, line, ch) in cases {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let hover = hover_impl(&session, &file, *line, *ch).unwrap();
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
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{label}: module path never stabilized"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    session.shutdown().ok();
}
