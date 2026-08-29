//! Equivalence battery (EP S5, acceptance criterion ①): the bridge's
//! hover face vs the frozen pyright baseline. The baseline is generated
//! by `tests/fixtures/equivalence/gen_baseline.py` (pyright in its
//! golden-oracle role) and normalized with the SAME spec as here:
//! 1. take the first ```python fenced block,
//! 2. strip a leading `(kind) ` prefix (engines use different kind
//!    token sets),
//! 3. fold whitespace runs into single spaces.
//! After normalization the type strings must match EXACTLY — the
//! battery uses explicit annotations, so any divergence is a finding,
//! not a tolerated inference difference.

use std::path::Path;
use std::sync::Arc;

use code_reality_lsp_bridge::server::hover_impl;
use code_reality_lsp_bridge::session::LangSpec;
use code_reality_lsp_bridge::LspSession;

mod common;

fn backend_bin() -> String {
    common::backend_bin()
}

/// Shared normalization spec (mirror of gen_baseline.py `normalize`):
/// 1. first ```python fenced block, 2. strip `(kind) ` prefix,
/// 3. strip a leading `name: ` prefix (pyrefly signs functions
///    `scale: def scale(...)`; variables are symmetric on both
///    engines), 4. strip a trailing `: ...` implementation marker
///    (pyrefly only), 5. fold whitespace runs.
fn normalize(hover: &str) -> String {
    let Some(start) = hover.find("```python\n") else {
        return String::new();
    };
    let rest = &hover[start + "```python\n".len()..];
    let Some(end) = rest.find("```") else {
        return String::new();
    };
    let body = rest[..end].trim().to_string();
    // Strip a leading kind prefix like "(variable) " — lowercase
    // letters/spaces only, so "(class) Box" keeps its name.
    let body = match body.strip_prefix('(') {
        Some(after_open) => match after_open.find(')') {
            Some(close) if close > 0 => {
                let inside = &after_open[..close];
                if inside.chars().all(|c| c.is_ascii_lowercase() || c == ' ') {
                    after_open[close + 1..].trim_start().to_string()
                } else {
                    body.clone()
                }
            }
            _ => body.clone(),
        },
        None => body,
    };
    // Strip a leading `name: ` prefix (identifier immediately followed
    // by ": " — `def scale(` has no colon after `def`, so signatures
    // survive; variables strip symmetrically on both engines).
    let body = if let Some(colon) = body.find(": ") {
        let name = &body[..colon];
        let valid = !name.is_empty()
            && !name.chars().next().unwrap().is_ascii_digit()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if valid {
            body[colon + 2..].trim_start().to_string()
        } else {
            body
        }
    } else {
        body
    };
    // Strip a trailing ": ..." implementation marker (pyrefly only).
    let body = match body.rfind(": ...") {
        Some(pos) => body[..pos].trim_end().to_string(),
        None => body,
    };
    let mut out = String::new();
    let mut in_ws = false;
    for c in body.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

#[test]
fn hover_parity_vs_pyright_baseline() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/equivalence");
    // Copy into a temp dir: the fixture must not sit inside the repo's
    // gitignore reach, and pyrefly must see it as a plain project dir.
    let tmp = tempfile::tempdir().unwrap();
    for name in ["battery.py"] {
        std::fs::copy(fixture_dir.join(name), tmp.path().join(name)).unwrap();
    }
    let session = Arc::new(LspSession::new(
        &backend_bin(),
        tmp.path().to_path_buf(),
        300,
        LangSpec::python(),
    ));
    let battery = tmp.path().join("battery.py").to_string_lossy().to_string();

    let baseline_raw =
        std::fs::read_to_string(fixture_dir.join("pyright_hover_baseline.json")).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&baseline_raw).unwrap();

    // Exact-string parity: variable, function signature, attribute —
    // all explicitly annotated so inference divergence is excluded.
    let cases: &[(&str, u32, u32)] = &[
        ("count_var", 0, 2),
        ("scale_func", 3, 6),
        ("attr_probe", 12, 14),
    ];
    for (label, line, ch) in cases {
        let hover = hover_impl(&session, &battery, *line, *ch)
            .unwrap_or_else(|e| panic!("{label}: hover failed: {e}"));
        let got = normalize(&hover);
        let want = baseline
            .pointer(&format!("/_positions/{label}"))
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{label}: missing in baseline"));
        assert_eq!(
            got, want,
            "{label} ({line}:{ch}) parity mismatch\nraw hover: {hover}"
        );
    }

    // Class kind, pyrefly-side assertion (excluded from exact parity:
    // pyrefly shows the constructor signature where pyright shows just
    // the name — display-depth difference, see gen_baseline.py notes).
    let class_hover = hover_impl(&session, &battery, 11, 6).unwrap();
    assert!(
        class_hover.contains("(class) Box"),
        "class hover missing: {class_hover}"
    );

    session.shutdown().ok();
}
