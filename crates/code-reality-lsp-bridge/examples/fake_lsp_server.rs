//! Fake TypeScript language server for HERMETIC bridge tests (std-only,
//! single file — also compilable standalone via
//! `rustc --edition 2021 examples/fake_lsp_server.rs`).
//!
//! Deliberately NOT a `[[bin]]`: example targets are not packaged into
//! the PyPI wheel, so the shipped product stays exactly one binary.
//!
//! Contract (mirrors what the planning POC proved about the real
//! `typescript-language-server 6.0.0 --stdio`, P7):
//! - requires `--stdio` in argv — exits(9) otherwise, so a bridge that
//!   drops the typed args fails its handshake instead of passing;
//! - `FAKE_LSP_ARGV_OUT=<path>` (optional) APPENDS the received argv as
//!   one JSON-array line per spawn at startup — the argv-equivalence
//!   proof. The variable is never set process-globally: each
//!   argv-evidence test spawns the fake through its own wrapper script
//!   (tests/ts_backend.rs) that exports the variable for the child only,
//!   pointing at a private evidence file, so a file only ever contains
//!   its own test's spawns; the fake itself stays stateless about it.
//! - `initialize` → capabilities + `serverInfo` (name fake-ts-server);
//! - `textDocument/didOpen`/`didChange` record (languageId, version,
//!   text) per URI and immediately push `publishDiagnostics` WITH the
//!   document version: one error diagnostic iff the text contains
//!   `FAKE_TS_ERROR`, else an empty array;
//! - `textDocument/hover` → markdown hover echoing the recorded
//!   languageId and text length (`fake-ts hover languageId=<id> len=<n>`);
//! - `shutdown` → null result; `exit` → process exits 0;
//! - `textDocument/didClose` drops the recorded document.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if let Some(out) = std::env::var_os("FAKE_LSP_ARGV_OUT") {
        let items: Vec<String> = argv
            .iter()
            .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&out)
        {
            // ONE write syscall: O_APPEND keeps concurrent appends from
            // interleaving mid-line.
            let line = format!("[{}]\n", items.join(","));
            let _ = f.write_all(line.as_bytes());
        }
    }
    if !argv.iter().skip(1).any(|a| a == "--stdio") {
        eprintln!("fake_lsp_server: --stdio argument required (typed backend args contract)");
        std::process::exit(9);
    }

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // uri -> (languageId, version, raw escaped text)
    let mut docs: HashMap<String, (String, i64, String)> = HashMap::new();

    while let Some(len) = read_content_length(&mut reader) {
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).is_err() {
            break;
        }
        let body = String::from_utf8_lossy(&buf).to_string();
        let id_opt = extract_i64(&body, "\"id\":");
        // Requests always carry an id; -1 is only formatted by handlers
        // that are never reached for notifications.
        let id = id_opt.unwrap_or(-1);
        let method = extract_str(&body, "\"method\":\"").unwrap_or_default();
        match method.as_str() {
            "initialize" => {
                let resp = format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"capabilities\":\
                     {{\"textDocumentSync\":1,\"hoverProvider\":true}},\"serverInfo\":\
                     {{\"name\":\"fake-ts-server\",\"version\":\"6.0.0-fake\"}}}}}}"
                );
                if send(&mut out, &resp).is_err() {
                    break;
                }
            }
            "initialized" => {}
            "shutdown" => {
                let resp = format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
                if send(&mut out, &resp).is_err() {
                    break;
                }
            }
            "exit" => break,
            "textDocument/didOpen" => {
                if let (Some(uri), Some(lang), Some(v), Some(text)) = (
                    extract_str(&body, "\"uri\":\""),
                    extract_str(&body, "\"languageId\":\""),
                    extract_i64(&body, "\"version\":"),
                    extract_str(&body, "\"text\":\""),
                ) {
                    let has_err = text.contains("FAKE_TS_ERROR");
                    docs.insert(uri.clone(), (lang, v, text));
                    let _ = send(&mut out, &diag_push(&uri, v, has_err));
                }
            }
            "textDocument/didChange" => {
                if let (Some(uri), Some(v), Some(text)) = (
                    extract_str(&body, "\"uri\":\""),
                    extract_i64(&body, "\"version\":"),
                    extract_str(&body, "\"text\":\""),
                ) {
                    let has_err = text.contains("FAKE_TS_ERROR");
                    let lang = docs
                        .get(&uri)
                        .map(|(l, _, _)| l.clone())
                        .unwrap_or_else(|| "-".to_string());
                    docs.insert(uri.clone(), (lang, v, text));
                    let _ = send(&mut out, &diag_push(&uri, v, has_err));
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = extract_str(&body, "\"uri\":\"") {
                    docs.remove(&uri);
                }
            }
            "textDocument/hover" => {
                let uri = extract_str(&body, "\"uri\":\"").unwrap_or_default();
                let marker = match docs.get(&uri) {
                    Some((lang, _, text)) => {
                        format!("fake-ts hover languageId={lang} len={}", text.len())
                    }
                    None => "fake-ts hover languageId=- len=-1".to_string(),
                };
                let resp = format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":\
                     {{\"kind\":\"markdown\",\"value\":\"{marker}\"}}}}}}"
                );
                if send(&mut out, &resp).is_err() {
                    break;
                }
            }
            // Any other request gets an empty result; notifications are
            // irrelevant to the fake.
            other => {
                if let Some(id) = id_opt {
                    if !other.is_empty() {
                        let resp = format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
                        if send(&mut out, &resp).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Read LSP headers up to the blank line; return the Content-Length
/// value (None on EOF).
fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut len = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return len;
        }
        if let Some(v) = trimmed.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
}

fn send(out: &mut impl Write, body: &str) -> std::io::Result<()> {
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    out.flush()
}

/// Escaping-aware string field extraction: value runs to the next
/// unescaped double quote. Raw (still-escaped) slice — the fake's
/// markers never need decoding.
fn extract_str(body: &str, key: &str) -> Option<String> {
    let start = body.find(key)? + key.len();
    let bytes = body.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(body[start..i].to_string()),
            _ => i += 1,
        }
    }
    None
}

fn extract_i64(body: &str, key: &str) -> Option<i64> {
    let start = body.find(key)? + key.len();
    let rest = &body[start..];
    let digits: usize = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .map(|c| c.len_utf8())
        .sum();
    if digits == 0 {
        return None;
    }
    rest[..digits].parse().ok()
}

fn diag_push(uri: &str, version: i64, has_error: bool) -> String {
    let diags = if has_error {
        "[{\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\
         \"character\":1}},\"severity\":1,\"code\":\"fake-error\",\"source\":\
         \"fake-ts\",\"message\":\"FAKE_TS_ERROR marker present\"}]"
    } else {
        "[]"
    };
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\
         \"params\":{{\"uri\":\"{uri}\",\"version\":{version},\
         \"diagnostics\":{diags}}}}}"
    )
}
