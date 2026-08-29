//! `LspSession` — lifecycle + protocol client for one spawned language
//! server (default backend: the `pyrefly-lsp` bin, overridable via
//! `--lsp-command`; the bridge stays language-agnostic — P2 Rust type
//! face is the same crate with a different backend command).
//!
//! Concurrency contract (EP R-08): every LSP interaction runs under the
//! `interaction` lock, so writes and response pairing never interleave —
//! pyrefly's `uris_pending_close` accounting assumes a single ordered
//! writer. The reader thread owns stdout: responses go to the pending
//! slot, `publishDiagnostics` notifications land in the per-URI diag
//! cache, and server→client requests always get an empty `[]` response
//! (an unanswered `workspace/configuration` would freeze pyrefly's
//! background indexing — never skip them).

use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::framing::{read_message, write_message};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Latest `publishDiagnostics` push for one document URI.
#[derive(Debug, Clone)]
pub struct DiagEntry {
    pub version: Option<i64>,
    pub diagnostics: Vec<Value>,
    pub last_push: Instant,
}

/// Bridge-side knowledge of a file's current content: what we last sent
/// the server (didOpen text or didChange replacement). Evicting from the
/// server's open set does NOT drop the overlay — a later re-open replays
/// the overlay version, so un-persisted edits are never silently rolled
/// back to disk state (EP R-06).
#[derive(Debug, Clone)]
pub struct OverlayEntry {
    pub content: String,
    pub version: i64,
    /// (mtime, size) of the disk file at the moment this content was
    /// sourced; used to detect out-of-band disk edits (SM-12).
    pub stamp: Option<(std::time::SystemTime, u64)>,
    /// Instant of the last LSP mutation that produced this entry
    /// (didOpen/didChange/force-reopen replay) — the freshness basis for
    /// check_file's convergence gate. F1: stamped at EVERY mutation
    /// origin on the session side, so the gate survives the caller
    /// discarding the returned Instant (a nudge-path check used to run
    /// with mutation_at=None and pass poisoned stale entries).
    pub last_mutation: Option<Instant>,
}

/// Per-language backend profile: everything the generic LspSession
/// machinery needs to serve one language. The P2 clause — the same
/// crate serves any LSP backend given one of these.
#[derive(Clone, Copy)]
pub struct LangSpec {
    pub language_id: &'static str,
    /// Extension gate (case-sensitive, includes no dot).
    pub extension: &'static str,
    /// Bounded-retry window for the transient null hover while the
    /// backend warms up (rust-analyzer cold-loads a whole workspace:
    /// observed 749ms–9.5s, so Rust uses 30s).
    pub hover_retry_ms: u64,
    /// Convergence deadline for check_file (rust-analyzer pushes in
    /// waves — syntax/semantic/flycheck — and under load the semantic
    /// wave can exceed the Python-scale 10s).
    pub slow_timeout_ms: u64,
    /// Install guidance surfaced when the backend binary is missing.
    pub install_hint: &'static str,
}

impl LangSpec {
    pub fn python() -> Self {
        Self {
            language_id: "python",
            extension: "py",
            hover_retry_ms: 500,
            // 20s: under parallel-test load (a dozen backends at once)
            // the recheck wave can overshoot 10s — headroom, not latency.
            slow_timeout_ms: 20_000,
            install_hint: "uv tool install pyrefly-producer (or cargo install --path <checkout>/crates/pyrefly-producer)",
        }
    }

    pub fn rust() -> Self {
        Self {
            language_id: "rust",
            extension: "rs",
            hover_retry_ms: 30_000,
            slow_timeout_ms: 30_000,
            install_hint: "rustup component add rust-analyzer",
        }
    }
}

struct Backend {
    child: Child,
    stdin: ChildStdin,
}

type PendingSlot = Arc<Mutex<Option<(i64, mpsc::SyncSender<Value>)>>>;

pub struct LspSession {
    backend_cmd: String,
    root: PathBuf,
    pub quiesce: Duration,
    pub lang: LangSpec,
    interaction: Mutex<()>,
    /// Shared with the reader thread (it delivers responses and answers
    /// server→client requests through the same backend slot).
    backend: Arc<Mutex<Option<Backend>>>,
    next_id: AtomicI64,
    pending: PendingSlot,
    pub diag_cache: Arc<Mutex<HashMap<String, DiagEntry>>>,
    /// Files currently didOpen on the server, LRU-ordered (oldest first).
    pub open_files: Mutex<Vec<PathBuf>>,
    pub overlay: Mutex<HashMap<PathBuf, OverlayEntry>>,
    server_info: Mutex<Option<String>>,
    dead: Arc<AtomicBool>,
}

/// file:// URI in the same shape `Url::from_file_path` (url crate,
/// PATH encode set) produces — the backend re-serializes URIs that
/// way in its diagnostics pushes, so the bridge's cache key must match
/// byte-for-byte, including percent-encoded non-ASCII (fresh F1).
fn file_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::from("file://");
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'/'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b':'
            | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl LspSession {
    pub fn new(backend_cmd: &str, root: PathBuf, quiesce_ms: u64, lang: LangSpec) -> Self {
        Self {
            backend_cmd: backend_cmd.to_string(),
            root,
            quiesce: Duration::from_millis(quiesce_ms),
            lang,
            interaction: Mutex::new(()),
            backend: Arc::new(Mutex::new(None)),
            next_id: AtomicI64::new(1),
            pending: Arc::new(Mutex::new(None)),
            diag_cache: Arc::new(Mutex::new(HashMap::new())),
            open_files: Mutex::new(Vec::new()),
            overlay: Mutex::new(HashMap::new()),
            server_info: Mutex::new(None),
            dead: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn backend_cmd(&self) -> &str {
        &self.backend_cmd
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    /// Test hook: the backend child's pid (None before spawn).
    #[doc(hidden)]
    pub fn backend_pid(&self) -> Option<u32> {
        self.backend.lock().unwrap().as_ref().map(|b| b.child.id())
    }

    pub fn server_info(&self) -> String {
        self.server_info
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "not-spawned-yet".to_string())
    }

    fn check_alive(&self) -> Result<(), String> {
        if self.is_dead() {
            return Err(format!(
                "language server backend died (command: {}) — restart the bridge to recover",
                self.backend_cmd
            ));
        }
        Ok(())
    }

    /// Lazy spawn + handshake (first tool call pulls the backend up, so
    /// plugin consumers without the backend don't fail at startup).
    fn ensure_spawned(&self) -> Result<(), String> {
        if self.backend.lock().unwrap().is_some() {
            return Ok(());
        }
        let _i = self.interaction.lock().unwrap();
        self.ensure_spawned_locked()
    }

    fn ensure_spawned_locked(&self) -> Result<(), String> {
        // Re-check under the interaction lock (double-checked spawn).
        if self.backend.lock().unwrap().is_some() {
            return Ok(());
        }
        self.check_alive()?;

        let mut child = Command::new(&self.backend_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                format!(
                    "failed to spawn language server backend `{}`: {e}\n\
                     install it ({}) or override the backend command",
                    self.backend_cmd, self.lang.install_hint
                )
            })?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        // Reader thread: three-way split — responses go to the pending
        // slot, diagnostics land in the per-URI cache, server→client
        // requests are always answered with an empty result (never
        // skipped: an unanswered `workspace/configuration` freezes
        // pyrefly's background indexing).
        let pending = Arc::clone(&self.pending);
        let diag_cache = Arc::clone(&self.diag_cache);
        let dead = Arc::clone(&self.dead);
        let backend_for_reader = Arc::clone(&self.backend);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let msg = match read_message(&mut reader) {
                    Ok(Some(m)) => m,
                    Ok(None) | Err(_) => {
                        dead.store(true, Ordering::SeqCst);
                        if let Some((_, tx)) = pending.lock().unwrap().take() {
                            let _ = tx.send(json!({
                                "jsonrpc": "2.0", "id": -1,
                                "error": {"code": -32603, "message": "backend exited"}
                            }));
                        }
                        return;
                    }
                };
                let id = msg.get("id").and_then(Value::as_i64);
                let has_method = msg.get("method").is_some();
                if let (Some(id), false) = (id, has_method) {
                    // Response to our in-flight request.
                    let mut slot = pending.lock().unwrap();
                    if let Some((want_id, _)) = slot.as_ref() {
                        if *want_id == id {
                            let (_, tx) = slot.take().unwrap();
                            let _ = tx.send(msg);
                        }
                    }
                } else if let Some(id) = id {
                    // Server→client request: empty result, always.
                    let reply = json!({"jsonrpc": "2.0", "id": id, "result": []});
                    if let Some(b) = backend_for_reader.lock().unwrap().as_mut() {
                        let _ = write_message(&mut b.stdin, &reply);
                    }
                } else if msg.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
                {
                    if let Some(params) = msg.get("params") {
                        let uri = params
                            .get("uri")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let entry = DiagEntry {
                            version: params.get("version").and_then(Value::as_i64),
                            diagnostics: params
                                .get("diagnostics")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default(),
                            last_push: Instant::now(),
                        };
                        diag_cache.lock().unwrap().insert(uri, entry);
                    }
                }
                // Other notifications are irrelevant to the bridge.
            }
        });

        // Install the backend (child + stdin) in the slot shared with
        // the reader thread.
        *self.backend.lock().unwrap() = Some(Backend { child, stdin });

        // Handshake: initialize → response → `initialized` notification
        // (any didOpen sent before `initialized` is dropped by the
        // server). Caller holds the interaction lock.
        let params = json!({
            "processId": std::process::id(),
            "rootUri": file_uri(&self.root),
            "capabilities": {
                "textDocument": {
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "publishDiagnostics": {"relatedInformation": true}
                }
            }
        });
        let resp = match self.request_locked("initialize", params, HANDSHAKE_TIMEOUT) {
            Ok(r) => r,
            Err(e) => {
                // Roll back the half-installed backend: a server that
                // never completed initialize silently drops every
                // didOpen, turning all later tool calls into empty
                // answers. Killing here lets the next call retry fresh.
                if let Some(mut b) = self.backend.lock().unwrap().take() {
                    let _ = b.child.kill();
                }
                return Err(format!("initialize handshake failed: {e}"));
            }
        };
        let result = resp.get("result").cloned().unwrap_or(Value::Null);
        let name = result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let version = result
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .unwrap_or("?");
        *self.server_info.lock().unwrap() = Some(format!("{name} {version}"));
        {
            let mut g = self.backend.lock().unwrap();
            if let Some(b) = g.as_mut() {
                let _ = write_message(
                    &mut b.stdin,
                    &json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
                );
            }
        }
        Ok(())
    }

    /// Send a request and wait for its response (id-matched). The
    /// interaction lock serializes request/response pairing; the backend
    /// write lock is only held for the write itself.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.ensure_spawned()?;
        let _i = self.interaction.lock().unwrap();
        self.check_alive()?;
        self.request_locked(method, params, REQUEST_TIMEOUT)
    }

    fn request_locked(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::sync_channel(1);
        *self.pending.lock().unwrap() = Some((id, tx));
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        {
            let mut g = self.backend.lock().unwrap();
            let b = g
                .as_mut()
                .ok_or_else(|| "backend not spawned".to_string())?;
            write_message(&mut b.stdin, &msg).map_err(err_str)?;
        }
        let resp = rx
            .recv_timeout(timeout)
            .map_err(|_| format!("timeout waiting for response to `{method}`"))?;
        if let Some(e) = resp.get("error") {
            return Err(format!("server error on `{method}`: {e}"));
        }
        Ok(resp)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.ensure_spawned()?;
        let _i = self.interaction.lock().unwrap();
        self.check_alive()?;
        let mut g = self.backend.lock().unwrap();
        let b = g
            .as_mut()
            .ok_or_else(|| "backend not spawned".to_string())?;
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        write_message(&mut b.stdin, &msg).map_err(err_str)?;
        Ok(())
    }

    /// Graceful shutdown: `shutdown` is a REQUEST (a bare notification
    /// takes the unhandled path), then `exit`, then reap the child.
    pub fn shutdown(&self) -> Result<(), String> {
        if self.backend.lock().unwrap().is_none() {
            return Ok(());
        }
        let _ = self.request("shutdown", Value::Null);
        {
            let mut g = self.backend.lock().unwrap();
            if let Some(b) = g.as_mut() {
                let _ = write_message(&mut b.stdin, &json!({"jsonrpc": "2.0", "method": "exit"}));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let exited = {
                let mut g = self.backend.lock().unwrap();
                match g.as_mut() {
                    None => true,
                    Some(b) => b.child.try_wait().map_err(err_str)?.is_some(),
                }
            };
            if exited {
                self.backend.lock().unwrap().take();
                return Ok(());
            }
            if Instant::now() >= deadline {
                let mut g = self.backend.lock().unwrap();
                if let Some(mut b) = g.take() {
                    let _ = b.child.kill();
                }
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn file_uri(path: &Path) -> String {
        file_uri(path)
    }

    fn disk_stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    /// Bring `path` in sync with the server's open state:
    /// - never opened → read disk, didOpen (version 1)
    /// - open but disk changed out-of-band → didChange full sync
    /// - evicted from the server's open set → re-didOpen **from the
    ///   overlay** (un-persisted edits survive, EP R-06)
    /// - otherwise → no-op
    /// Returns the mutation instant when any LSP mutation was sent
    /// (drives check_file's convergence window), `None` for no-op.
    /// LRU cap: the oldest open file is didClose'd (overlay retained).
    pub fn sync_open(&self, path: &Path) -> Result<Option<Instant>, String> {
        let uri = file_uri(path);
        let lang_id = self.lang.language_id;
        let mut mutation: Option<Instant> = None;

        // LRU touch: already-open files move to the back.
        let mut open = self.open_files.lock().unwrap();
        if let Some(pos) = open.iter().position(|p| p == path) {
            open.remove(pos);
            open.push(path.to_path_buf());
        }

        let mut overlay = self.overlay.lock().unwrap();
        match overlay.get(path).cloned() {
            None => {
                let text = std::fs::read_to_string(path).map_err(|e| {
                    format!(
                        "cannot read {}: {e} — the file must exist on disk (absolute path)",
                        path.display()
                    )
                })?;
                let stamp = Self::disk_stamp(path);
                let t = Instant::now();
                self.notify(
                    "textDocument/didOpen",
                    json!({
                        "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": text}
                    }),
                )?;
                overlay.insert(
                    path.to_path_buf(),
                    OverlayEntry {
                        content: text,
                        version: 1,
                        stamp,
                        last_mutation: Some(t),
                    },
                );
                open.push(path.to_path_buf());
                mutation = Some(t);
            }
            Some(entry) => {
                let stamp = Self::disk_stamp(path);
                if open.iter().any(|p| p == path) {
                    // Open on the server: pick up out-of-band disk edits.
                    if stamp.is_some() && stamp != entry.stamp {
                        let text = std::fs::read_to_string(path)
                            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                        let v = entry.version + 1;
                        let t = Instant::now();
                        self.notify(
                            "textDocument/didChange",
                            full_change(&uri, v, &entry.content, &text),
                        )?;
                        overlay.insert(
                            path.to_path_buf(),
                            OverlayEntry {
                                content: text,
                                version: v,
                                stamp,
                                last_mutation: Some(t),
                            },
                        );
                        mutation = Some(t);
                    }
                } else {
                    // Evicted earlier: re-open from the overlay so
                    // un-persisted edits are not rolled back to disk.
                    let t = Instant::now();
                    self.notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": entry.content}
                        }),
                    )?;
                    overlay.insert(
                        path.to_path_buf(),
                        OverlayEntry {
                            content: entry.content,
                            version: 1,
                            stamp: entry.stamp,
                            last_mutation: Some(t),
                        },
                    );
                    open.push(path.to_path_buf());
                    mutation = Some(t);
                }
            }
        }

        // LRU eviction (cap 8): didClose the oldest, keep the overlay.
        // The evicted URI's diag entry goes with it (fresh F11 — a
        // closed file's stale push must not linger).
        while open.len() > 8 {
            let victim = open.remove(0);
            let vuri = file_uri(&victim);
            self.notify(
                "textDocument/didClose",
                json!({"textDocument": {"uri": vuri.clone()}}),
            )?;
            self.diag_cache.lock().unwrap().remove(&vuri);
        }
        Ok(mutation)
    }

    /// Full-content replacement over the open file (range-elided
    /// didChange — the spec's full-sync form; no UTF-16 endpoint math).
    /// The overlay records the new content and version.
    pub fn apply_edit(&self, path: &Path, content: &str) -> Result<Instant, String> {
        let uri = file_uri(path);
        let mut overlay = self.overlay.lock().unwrap();
        let entry = overlay
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not opened: {}", path.display()))?;
        let v = entry.version + 1;
        let t = Instant::now();
        self.notify(
            "textDocument/didChange",
            full_change(&uri, v, &entry.content, content),
        )?;
        overlay.insert(
            path.to_path_buf(),
            OverlayEntry {
                content: content.to_string(),
                version: v,
                stamp: entry.stamp,
                last_mutation: Some(t),
            },
        );
        Ok(t)
    }

    /// Force-close and re-open from the overlay: the recovery path when
    /// a backend silently drops a didChange (rust-analyzer does when
    /// the change lands right after a hover during load — probe-verified
    /// 2026-08-28). didClose clears the server copy AND the diag-cache
    /// entry; the re-didOpen replays the overlay content at version 1.
    pub fn force_reopen(&self, path: &Path) -> Result<Instant, String> {
        let uri = file_uri(path);
        let t = Instant::now();
        self.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri.clone()}}),
        )?;
        self.diag_cache.lock().unwrap().remove(&uri);
        let entry = self
            .overlay
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not opened: {}", path.display()))?;
        let lang_id = self.lang.language_id;
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": entry.content}
            }),
        )?;
        self.overlay.lock().unwrap().insert(
            path.to_path_buf(),
            OverlayEntry {
                content: entry.content,
                version: 1,
                stamp: entry.stamp,
                last_mutation: Some(t),
            },
        );
        Ok(t)
    }
}

/// Full-content replacement as a RANGE-form change event. The end
/// position spans the OLD content (the text being replaced), in line
/// units — start {0,0} → end {old_lines,0}. The range-elided form is a
/// spec obligation rust-analyzer does not honor (probe-verified
/// 2026-08-28: zero pushes); the range form works on both backends.
/// End is a line start, so no UTF-16 endpoint math is needed.
fn full_change(uri: &str, version: i64, old_content: &str, new_text: &str) -> Value {
    // Empty OLD content spans nothing (end {0,0}); split() on "" would
    // yield a phantom line.
    let end_line = if old_content.is_empty() {
        0
    } else {
        old_content.split('\n').count()
    };
    json!({
        "textDocument": {"uri": uri, "version": version},
        "contentChanges": [{
            "range": {"start": {"line": 0, "character": 0},
                       "end": {"line": end_line, "character": 0}},
            "text": new_text
        }]
    })
}
