//! S oracle: accepted segment B lifecycle contract; real stdio adversary.
use std::io::{BufRead, Read, Write};
use std::time::Duration;
fn send(out: &mut impl Write, body: &str) {
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    out.flush().unwrap();
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = &args[1];
    let dir = std::path::Path::new(&args[2]);
    std::fs::write(dir.join("pid"), std::process::id().to_string()).unwrap();
    if mode == "no-read" {
        std::thread::sleep(Duration::from_secs(120));
        return;
    }
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut out = std::io::stdout();
    loop {
        let mut len = 0;
        loop {
            let mut line = String::new();
            if input.read_line(&mut line).unwrap() == 0 {
                return;
            }
            if line == "\r\n" {
                break;
            }
            if let Some(v) = line.strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0; len];
        input.read_exact(&mut buf).unwrap();
        let body = String::from_utf8(buf).unwrap();
        let id = body.split_once("\"id\":").map(|(_, s)| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        });
        let method = body
            .split_once("\"method\":\"")
            .map(|(_, s)| s.split('"').next().unwrap())
            .unwrap_or("");
        if method == "initialize" {
            if mode == "reject-once" && !dir.join("attempt").exists() {
                std::fs::write(dir.join("attempt"), "1").unwrap();
                send(
                    &mut out,
                    &format!(
                        "{{\"id\":{},\"error\":{{\"code\":-32603,\"message\":\"rejected\"}}}}",
                        id.unwrap()
                    ),
                );
            } else {
                send(&mut out, &format!("{{\"id\":{},\"result\":{{\"capabilities\":{{}},\"serverInfo\":{{\"name\":\"fixture\",\"version\":\"1\"}}}}}}", id.unwrap()));
            }
        } else if method == "stop-reading" {
            send(
                &mut out,
                &format!("{{\"id\":{},\"result\":null}}", id.unwrap()),
            );
            std::thread::sleep(Duration::from_secs(120));
            return;
        } else if method == "ask-client" {
            std::fs::write(dir.join("request-id"), id.unwrap()).unwrap();
            send(
                &mut out,
                "{\"id\":900,\"method\":\"workspace/configuration\",\"params\":{}}",
            );
        } else if method == "hold-response" {
            std::fs::write(dir.join("holding"), "1").unwrap();
        } else if method == "flood-client" {
            send(
                &mut out,
                &format!("{{\"id\":{},\"result\":null}}", id.unwrap()),
            );
            for n in 1000..21000 {
                send(
                    &mut out,
                    &format!(
                        "{{\"id\":{n},\"method\":\"workspace/configuration\",\"params\":{{}}}}"
                    ),
                );
            }
            std::thread::sleep(Duration::from_secs(120));
            return;
        } else if method.is_empty() && id.as_deref() == Some("900") {
            assert!(body.contains("\"result\":[]"));
            let pending = std::fs::read_to_string(dir.join("request-id")).unwrap();
            send(
                &mut out,
                &format!("{{\"id\":{pending},\"result\":\"client-replied\"}}"),
            );
        } else if method == "textDocument/didOpen" {
            let uri = body
                .split_once("\"uri\":\"")
                .unwrap()
                .1
                .split('"')
                .next()
                .unwrap();
            send(&mut out, &format!("{{\"method\":\"textDocument/publishDiagnostics\",\"params\":{{\"uri\":\"{uri}\",\"version\":1,\"diagnostics\":[]}}}}"));
        } else if method == "exit" {
            return;
        } else if let Some(id) = id {
            send(&mut out, &format!("{{\"id\":{id},\"result\":null}}"));
        }
    }
}
