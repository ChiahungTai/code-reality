//! Oracle S: accepted Round 1 segment A final error cap and lifecycle contract.
//! Oracle H: Round 0 real stdio cancellation/EOF observations.
#![cfg(unix)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

struct Peer {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.input.take();
        // The fixture owns a fresh process group, including spawned producers.
        // Kill the group before waiting, including on assertion/timeout unwind.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Peer {
    fn start(bin_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
            .process_group(0)
            .arg("--stdio")
            .env(
                "PATH",
                format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
            )
            .env("CR_REPO", "/nonexistent")
            .env("CODE_REALITY_AUTOHEAL", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let value = serde_json::from_str(&line).unwrap();
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut peer = Self {
            child,
            input,
            output,
            reader: Some(reader),
        };
        peer.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"regression","version":"1"}}}));
        assert!(peer.receive(1).get("result").is_some());
        peer.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        peer
    }

    fn send(&mut self, value: Value) {
        writeln!(self.input.as_mut().unwrap(), "{value}").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }

    fn receive(&self, id: u64) -> Value {
        let value = self
            .output
            .recv_timeout(Duration::from_secs(15))
            .expect("bounded response");
        assert_eq!(value["id"], id);
        value
    }

    fn build(&mut self, repo: &std::path::Path) {
        self.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"build","arguments":{"repo_root":repo,"producer":"python"}}}));
    }

    fn close(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "bounded server exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn fixture(body: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("example.py"),
        "def example():\n    return 1\n",
    )
    .unwrap();
    let bin = dir.path().join("pyrefly-index");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 0.9.3; exit 0; fi\n{body}\nexit 2\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[test]
fn oversized_error_is_bounded_and_stdio_survives() {
    let dir = fixture("/usr/bin/head -c 1200000 /dev/zero | /usr/bin/tr '\\000' E >&2");
    let mut peer = Peer::start(dir.path());
    peer.build(dir.path());
    let response = peer.receive(2);
    assert_eq!(response["error"]["code"], -32603);
    let text = response["error"]["message"].as_str().unwrap();
    let (prefix, marker) = text.split_once("\n[TRUNCATED]").expect("error cap marker");
    assert!(prefix.len() <= 1 << 20);
    assert!(marker.len() < 256);
    peer.send(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}));
    let tools = peer.receive(3);
    for name in ["build", "snapshot", "delta_tour", "project"] {
        let tool = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        let description = tool["description"].as_str().unwrap();
        assert!(description.contains("Cancellation suppresses response delivery"));
        assert!(description.contains("publish"));
        assert!(description.contains("EOF may await"));
    }
    peer.close();
}

#[test]
fn cancellation_and_eof_retain_started_work() {
    for cancel in [true, false] {
        let dir = fixture("here=$(dirname \"$0\"); printf start > \"$here/started\"; /bin/sleep 2; printf done > \"$here/completed\"");
        let mut peer = Peer::start(dir.path());
        peer.build(dir.path());
        let deadline = Instant::now() + Duration::from_secs(10);
        while !dir.path().join("started").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        if cancel {
            peer.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2,"reason":"regression"}}));
            peer.send(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}));
            assert!(peer.receive(3).get("result").is_some());
            while !dir.path().join("completed").exists() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            peer.send(json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}));
            assert!(peer.receive(4).get("result").is_some());
        } else {
            peer.input.take();
            std::thread::sleep(Duration::from_millis(200));
            assert!(peer.child.try_wait().unwrap().is_none());
        }
        peer.close();
        assert!(dir.path().join("completed").exists());
        if cancel {
            assert!(peer.output.try_iter().all(|v| v["id"] != 2));
        }
    }
}

#[test]
fn unwind_cleans_up_started_producer_without_explicit_close() {
    let dir = fixture("here=$(dirname \"$0\"); printf start > \"$here/started\"; /bin/sleep 2; printf leaked > \"$here/completed\"");
    let result = std::panic::catch_unwind(|| {
        let mut peer = Peer::start(dir.path());
        peer.build(dir.path());
        let deadline = Instant::now() + Duration::from_secs(10);
        while !dir.path().join("started").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("exercise fixture unwind after producer starts");
    });
    let panic = result.expect_err("intentional unwind must run");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"exercise fixture unwind after producer starts")
    );
    assert!(dir.path().join("started").exists());
    std::thread::sleep(Duration::from_millis(2200));
    assert!(
        !dir.path().join("completed").exists(),
        "producer escaped fixture cleanup"
    );
}
