//! `LspSession` — lifecycle + protocol client for one spawned language
//! server backend (a typed `BackendCommand`: program + fixed argv —
//! Python via `pyrefly-lsp`, Rust via `rust-analyzer`, JavaScript/
//! TypeScript via `typescript-language-server --stdio`; the bridge
//! stays language-agnostic — the P2 clause: same crate, backend is a
//! parameter).
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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::transport::{remaining, Transport};

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

/// Typed backend process spec, spawned directly as argv (never through
/// a shell). Family constructors pin the fixed arguments — the
/// TypeScript server's required `--stdio` lives HERE rather than in a
/// shell string, so spawn performs no parsing (EP S5 frozen decision 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl BackendCommand {
    pub fn python(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn rust(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn typescript(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: vec!["--stdio".to_string()],
        }
    }
}

/// Safe display form for status/error text (program + fixed args);
/// this is never executed.
impl std::fmt::Display for BackendCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LangFamily {
    Python,
    Rust,
    TypeScript,
}

/// Per-language backend profile: everything the generic LspSession
/// machinery needs to serve one language family. The P2 clause — the
/// same crate serves any LSP backend given one of these. S5: the
/// single-extension gate became an extension-set matcher with a
/// per-extension language id, so one TypeScript family serves six file
/// faces with four language ids.
#[derive(Clone, Copy)]
pub struct LangSpec {
    pub family: LangFamily,
    /// Extension gate (case-sensitive, includes no dot).
    pub extensions: &'static [&'static str],
    /// Bounded-retry window for the transient null hover while the
    /// backend warms up (rust-analyzer cold-loads a whole workspace:
    /// observed 749ms–9.5s, so Rust uses 30s).
    pub hover_retry_ms: u64,
    /// Entry-to-result deadline for check_file, including synchronization
    /// and initialization (rust-analyzer pushes in
    /// waves — syntax/semantic/flycheck — and under load the semantic
    /// wave can exceed the Python-scale 10s).
    pub slow_timeout_ms: u64,
    /// Install guidance surfaced when the backend binary is missing.
    pub install_hint: &'static str,
    /// Whether this backend stamps `version` on publishDiagnostics
    /// pushes. Measured behavior (S5 real-TLS acceptance): pyrefly and
    /// rust-analyzer always do; typescript-language-server 6.0.0 never
    /// does and ignores the LSP `versionSupport` client capability.
    /// Drives the diagnostic convergence gate — versioned backends keep
    /// the version check (the poisoned-eviction defense), unversioned
    /// ones rely on the time-based freshness basis + quiesce window.
    pub diag_versions: bool,
}

impl LangSpec {
    pub fn python() -> Self {
        Self {
            family: LangFamily::Python,
            extensions: &["py"],
            hover_retry_ms: 500,
            // 20s: under parallel-test load (a dozen backends at once)
            // the recheck wave can overshoot 10s — headroom, not latency.
            slow_timeout_ms: 20_000,
            install_hint: "uv tool install pyrefly-producer (or cargo install --path <checkout>/crates/pyrefly-producer)",
            diag_versions: true,
        }
    }

    pub fn rust() -> Self {
        Self {
            family: LangFamily::Rust,
            extensions: &["rs"],
            hover_retry_ms: 30_000,
            slow_timeout_ms: 30_000,
            install_hint: "rustup component add rust-analyzer",
            diag_versions: true,
        }
    }

    pub fn typescript() -> Self {
        Self {
            family: LangFamily::TypeScript,
            extensions: &["js", "jsx", "mjs", "cjs", "ts", "tsx"],
            // Rust-class policy (EP S5): the large-workspace-safe 30s
            // hover/convergence ceilings — measure real TS workspaces
            // before reducing; the POC's small-fixture latency is not a
            // production upper bound.
            hover_retry_ms: 30_000,
            slow_timeout_ms: 30_000,
            install_hint: "npm install --global typescript-language-server typescript (requires Node.js); a repo-local node_modules/.bin install or a directory listed in CODE_REALITY_NODE_BIN_DIR is picked up automatically",
            // measured on real 6.0.0: publishDiagnostics never carries
            // `version` and the versionSupport client capability is
            // not implemented (bundle-verified)
            diag_versions: false,
        }
    }

    pub fn supports_extension(&self, ext: &str) -> bool {
        self.extensions.contains(&ext)
    }

    /// LSP languageId for one file extension of this family (exact
    /// table from the EP; `None` for extensions the family does not
    /// serve).
    pub fn language_id(&self, ext: &str) -> Option<&'static str> {
        if !self.supports_extension(ext) {
            return None;
        }
        Some(match ext {
            "py" => "python",
            "rs" => "rust",
            "js" | "mjs" | "cjs" => "javascript",
            "jsx" => "javascriptreact",
            "ts" => "typescript",
            "tsx" => "typescriptreact",
            _ => return None,
        })
    }
}

pub(crate) type PendingSlot = Arc<Mutex<Option<(i64, mpsc::SyncSender<Value>)>>>;

pub struct LspSession {
    cmd: BackendCommand,
    root: PathBuf,
    pub quiesce: Duration,
    pub lang: LangSpec,
    interaction: Mutex<()>,
    /// Lifecycle owner only; neither transport thread takes this lock.
    backend: Mutex<Option<Transport>>,
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

impl LspSession {
    pub fn new(cmd: BackendCommand, root: PathBuf, quiesce_ms: u64, lang: LangSpec) -> Self {
        Self {
            cmd,
            root,
            quiesce: Duration::from_millis(quiesce_ms),
            lang,
            interaction: Mutex::new(()),
            backend: Mutex::new(None),
            next_id: AtomicI64::new(1),
            pending: Arc::new(Mutex::new(None)),
            diag_cache: Arc::new(Mutex::new(HashMap::new())),
            open_files: Mutex::new(Vec::new()),
            overlay: Mutex::new(HashMap::new()),
            server_info: Mutex::new(None),
            dead: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn backend(&self) -> &BackendCommand {
        &self.cmd
    }

    /// Per-file language id from the family's mapping; falls back to
    /// the family's primary language when the path carries no
    /// recognized extension (defensive — the tool face routes by
    /// extension before any session is touched).
    fn language_id_for(&self, path: &Path) -> &'static str {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        self.lang
            .language_id(ext)
            .unwrap_or(match self.lang.family {
                LangFamily::Python => "python",
                LangFamily::Rust => "rust",
                LangFamily::TypeScript => "typescript",
            })
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    /// Test hook: the backend child's pid (None before spawn).
    #[doc(hidden)]
    pub fn backend_pid(&self) -> Option<u32> {
        self.backend.lock().unwrap().as_ref().map(Transport::pid)
    }

    pub fn server_info(&self) -> String {
        self.server_info
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "not-spawned-yet".to_string())
    }

    pub(crate) fn check_alive(&self) -> Result<(), String> {
        if self.is_dead() {
            self.terminate();
            return Err(format!(
                "language server backend died (command: {}) — restart the bridge to recover",
                self.cmd
            ));
        }
        Ok(())
    }

    // No transport thread takes the backend slot lock. Keep it through
    // cleanup so concurrent shutdown callers observe completed joins/reaping.
    fn terminate(&self) {
        self.dead.store(true, Ordering::SeqCst);
        if let Some(mut backend) = self.backend.lock().unwrap().take() {
            backend.stop();
        }
        self.pending.lock().unwrap().take();
    }

    pub(crate) fn lock_until<'a, T>(
        &self,
        lock: &'a Mutex<T>,
        deadline: Instant,
    ) -> Result<MutexGuard<'a, T>, String> {
        loop {
            self.check_alive()?;
            if let Err(e) = remaining(&self.dead, deadline, "interaction/state lock") {
                self.terminate();
                return Err(e);
            }
            match lock.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    fn send_until(&self, message: Value, deadline: Instant) -> Result<(), String> {
        self.check_alive()?;
        let writer = self
            .backend
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| "backend not spawned".to_string())?
            .writer
            .clone();
        if let Err(e) = writer.send(message, deadline) {
            self.terminate();
            return Err(e);
        }
        self.check_alive()
    }

    /// Lazy spawn and handshake, under the interaction lock. A failed
    /// initialization is terminal; only a new session may retry.
    fn ensure_spawned_locked(&self, deadline: Instant) -> Result<(), String> {
        self.check_alive()?;
        if self.backend.lock().unwrap().is_some() {
            return Ok(());
        }
        let child = Command::new(&self.cmd.program)
            .args(&self.cmd.args)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!(
                "failed to spawn language server backend `{}`: {e}\ninstall it ({}) or override the backend command",
                self.cmd, self.lang.install_hint
            ))?;
        let mut transport = Transport::start(
            child,
            self.dead.clone(),
            self.pending.clone(),
            self.diag_cache.clone(),
        )?;
        {
            let mut slot = self.backend.lock().unwrap();
            if self.is_dead() {
                transport.stop();
                return Err(
                    "language server backend died during spawn — restart the bridge to recover"
                        .into(),
                );
            }
            *slot = Some(transport);
        }
        let initialized = (|| {
            let params = json!({
                "processId": std::process::id(), "rootUri": file_uri(&self.root),
                "capabilities": {"textDocument": {
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "publishDiagnostics": {"relatedInformation": true, "versionSupport": true}
                }}
            });
            let resp = self.request_locked(
                "initialize",
                params,
                deadline.min(Instant::now() + HANDSHAKE_TIMEOUT),
            )?;
            let result = resp.get("result").cloned().unwrap_or(Value::Null);
            let name = result
                .pointer("/serverInfo/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let version = result
                .pointer("/serverInfo/version")
                .and_then(Value::as_str)
                .unwrap_or("?");
            self.send_until(
                json!({"jsonrpc":"2.0", "method":"initialized", "params":{}}),
                deadline,
            )?;
            *self.server_info.lock().unwrap() = Some(format!("{name} {version}"));
            Ok::<(), String>(())
        })();
        if let Err(e) = initialized {
            self.terminate();
            return Err(format!("initialize handshake failed: {e}"));
        }
        self.check_alive()
    }

    /// One absolute deadline includes interaction wait, lazy initialization,
    /// frame delivery and the id-matched response.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_until(method, params, Instant::now() + REQUEST_TIMEOUT)
    }

    pub(crate) fn request_until(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        let _i = self.lock_until(&self.interaction, deadline)?;
        self.ensure_spawned_locked(deadline)?;
        self.request_locked(method, params, deadline)
    }

    fn request_locked(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::sync_channel(1);
        *self.pending.lock().unwrap() = Some((id, tx));
        self.send_until(
            json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}),
            deadline,
        )?;
        let resp = loop {
            let wait = match remaining(&self.dead, deadline, &format!("response to `{method}`")) {
                Ok(wait) => wait,
                Err(e) => {
                    self.terminate();
                    return Err(e);
                }
            };
            match rx.recv_timeout(wait.min(Duration::from_millis(5))) {
                Ok(resp) => break resp,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(e) => {
                    self.terminate();
                    return Err(e.to_string());
                }
            }
        };
        self.check_alive()?;
        if let Some(e) = resp.get("error") {
            return Err(format!("server error on `{method}`: {e}"));
        }
        Ok(resp)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.notify_until(method, params, Instant::now() + REQUEST_TIMEOUT)
    }

    fn notify_until(&self, method: &str, params: Value, deadline: Instant) -> Result<(), String> {
        let _i = self.lock_until(&self.interaction, deadline)?;
        self.ensure_spawned_locked(deadline)?;
        self.send_until(
            json!({"jsonrpc":"2.0", "method":method, "params":params}),
            deadline,
        )
    }

    /// Total ten-second shutdown budget, measured from entry. Reserve two
    /// seconds for forced cleanup; child kill/reap never needs interaction.
    /// OS-level hangs are outside this application deadline contract.
    pub fn shutdown(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let graceful = deadline - Duration::from_secs(2);
        if self.backend.lock().unwrap().is_some() && !self.is_dead() {
            if let Ok(_i) = self.lock_until(&self.interaction, graceful) {
                let _ = self.request_locked("shutdown", Value::Null, graceful);
                let _ = self.send_until(json!({"jsonrpc":"2.0", "method":"exit"}), graceful);
            }
            while !self.is_dead() && Instant::now() < graceful {
                if self
                    .backend
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_none_or(Transport::exited)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.terminate();
        Ok(())
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
        self.sync_open_until(path, Instant::now() + REQUEST_TIMEOUT)
    }

    pub(crate) fn sync_open_until(
        &self,
        path: &Path,
        deadline: Instant,
    ) -> Result<Option<Instant>, String> {
        self.check_alive()?;
        let uri = file_uri(path);
        let lang_id = self.language_id_for(path);
        let mut mutation: Option<Instant> = None;

        // LRU touch: already-open files move to the back.
        let mut open = self.lock_until(&self.open_files, deadline)?;
        if let Some(pos) = open.iter().position(|p| p == path) {
            open.remove(pos);
            open.push(path.to_path_buf());
        }

        let mut overlay = self.lock_until(&self.overlay, deadline)?;
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
                self.notify_until(
                    "textDocument/didOpen",
                    json!({
                        "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": text}
                    }),
                    deadline,
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
                        self.notify_until(
                            "textDocument/didChange",
                            full_change(&uri, v, &entry.content, &text),
                            deadline,
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
                    self.notify_until(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": entry.content}
                        }),
                        deadline,
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
            self.notify_until(
                "textDocument/didClose",
                json!({"textDocument": {"uri": vuri.clone()}}),
                deadline,
            )?;
            self.diag_cache.lock().unwrap().remove(&vuri);
        }
        Ok(mutation)
    }

    /// Full-content replacement over the open file (range-elided
    /// didChange — the spec's full-sync form; no UTF-16 endpoint math).
    /// The overlay records the new content and version.
    pub fn apply_edit(&self, path: &Path, content: &str) -> Result<Instant, String> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let uri = file_uri(path);
        let mut overlay = self.lock_until(&self.overlay, deadline)?;
        let entry = overlay
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not opened: {}", path.display()))?;
        let v = entry.version + 1;
        let t = Instant::now();
        self.notify_until(
            "textDocument/didChange",
            full_change(&uri, v, &entry.content, content),
            deadline,
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
        self.force_reopen_until(path, Instant::now() + REQUEST_TIMEOUT)
    }

    pub(crate) fn force_reopen_until(
        &self,
        path: &Path,
        deadline: Instant,
    ) -> Result<Instant, String> {
        let uri = file_uri(path);
        let t = Instant::now();
        self.notify_until(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri.clone()}}),
            deadline,
        )?;
        self.diag_cache.lock().unwrap().remove(&uri);
        let entry = self
            .lock_until(&self.overlay, deadline)?
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not opened: {}", path.display()))?;
        let lang_id = self.language_id_for(path);
        self.notify_until(
            "textDocument/didOpen",
            json!({
                "textDocument": {"uri": uri, "languageId": lang_id, "version": 1, "text": entry.content}
            }),
            deadline,
        )?;
        self.lock_until(&self.overlay, deadline)?.insert(
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

impl Drop for LspSession {
    fn drop(&mut self) {
        self.terminate();
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
