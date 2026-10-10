//! DB-108 supply contract: the bridge bin publishes its own ABI
//! manifest (`--manifest`) and stamps a real server identity on
//! initialize. rmcp's `Implementation::from_build_env()` expands its
//! `env!` inside the rmcp crate, so the stock `get_info` answered
//! `{"name":"rmcp",...}` — useless as a version anchor for the
//! delegate-bridge supply probe (advisory leg, live-reproduced).

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn manifest_face_is_deterministic_publisher_json() {
    let exe = env!("CARGO_BIN_EXE_code-reality-lsp-bridge");
    let run = || {
        let out = Command::new(exe)
            .arg("--manifest")
            .env("CR_REPO", "/nonexistent")
            .output()
            .expect("spawn bridge --manifest");
        (out.status.code(), out.stdout.clone())
    };
    let (code1, out1) = run();
    assert_eq!(code1, Some(0), "manifest face exits 0");
    let v: Value = serde_json::from_slice(&out1).expect("manifest is valid JSON");
    assert_eq!(v["schema"], "cr-mcp-manifest/1");
    assert_eq!(v["name"], "code-reality-lsp-bridge");
    assert_eq!(v["crate_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["index_dependency"], false, "pure LSP face");
    assert!(
        v["backend_prereqs"].is_object(),
        "per-family backend prerequisites declared"
    );
    let tools = v["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 4);
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "tools must be name-sorted for a stable digest");
    for expected in ["hover", "check_file", "edit_file", "lsp_status"] {
        assert!(names.contains(&expected), "tool {expected} present: {names:?}");
    }
    let digest = v["tools_digest"].as_str().expect("digest");
    assert!(
        digest.starts_with("sha256-") && digest.len() == "sha256-".len() + 64,
        "digest shape: {digest}"
    );
    assert!(
        v["write_faces"]
            .as_array()
            .expect("write_faces")
            .iter()
            .any(|w| w.as_str() == Some("edit_file")),
        "edit_file is an in-memory overlay mutation — declared honestly"
    );
    let (_, out2) = run();
    assert_eq!(out1, out2, "manifest must be byte-deterministic");
}

/// The stdio handshake must carry the bridge identity, not the carrier
/// library's name. Line-delimited JSON-RPC, then kill the child.
#[test]
fn initialize_reports_bridge_identity_not_the_library() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_code-reality-lsp-bridge"))
        .arg("--stdio")
        .env("CR_REPO", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bridge stdio");
    let stdin = child.stdin.as_mut().expect("piped stdin");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"probe","version":"0.0.1"}}}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let stdout = child.stdout.take().expect("piped stdout");
    let mut line = String::new();
    BufRead::read_line(&mut BufReader::new(stdout), &mut line).expect("read initialize response");
    let _ = child.kill();
    let resp: Value = serde_json::from_str(line.trim()).expect("valid JSON-RPC response");
    let info = &resp["result"]["serverInfo"];
    assert_eq!(
        info["name"], "code-reality-lsp-bridge",
        "serverInfo.name must be the binary identity: {resp}"
    );
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn unknown_argument_still_fails_loud() {
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-lsp-bridge"))
        .arg("--bogus")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn --bogus");
    assert_eq!(out.status.code(), Some(2));
}

/// Terminal-ordering symmetry with the mcp side (s6
/// `manifest_is_terminal_and_unknown_args_still_reject`): `--stdio` is
/// a no-op flag on the bridge and the terminal `--manifest` face wins,
/// so the publisher ABI manifest stays reachable in any argument order.
#[test]
fn manifest_wins_over_stdio_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_code-reality-lsp-bridge"))
        .args(["--stdio", "--manifest"])
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn bridge --stdio --manifest");
    assert_eq!(out.status.code(), Some(0), "--manifest wins over --stdio");
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("\"schema\""),
        "manifest JSON on stdout: {stdout}"
    );
}
