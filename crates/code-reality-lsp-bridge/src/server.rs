//! MCP server face of the bridge (rmcp, stdio). Tools are thin: they
//! route by file extension (`.py` → pyrefly backend, `.rs` →
//! rust-analyzer — the P2 clause: same crate, backend is a parameter)
//! and hold no LSP state of their own — every interaction goes through
//! the routed `LspSession` (serialized per backend), and blocking work
//! runs on `spawn_blocking` so the async runtime stays free.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::ErrorCode;
use rmcp::{tool, tool_handler, ErrorData as McpError};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::session::{LangSpec, LspSession};

#[derive(Deserialize, JsonSchema)]
pub struct HoverParams {
    /// Absolute path of the Python (.py) or Rust (.rs) file (must exist
    /// on disk).
    pub file: String,
    /// Zero-based line (LSP convention).
    pub line: u32,
    /// Zero-based character offset in UTF-16 code units (LSP convention).
    pub character: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct FileParams {
    /// Absolute path of the Python (.py) or Rust (.rs) file (must exist
    /// on disk).
    pub file: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EditParams {
    /// Absolute path of the Python (.py) or Rust (.rs) file (must exist
    /// on disk).
    pub file: String,
    /// Full replacement content of the file.
    pub content: String,
}

/// Two independent backend sessions with per-call extension routing.
/// Killing one backend leaves the other fully functional (SM-7).
pub struct Bridge {
    pub py: Arc<LspSession>,
    pub rs: Arc<LspSession>,
}

impl Bridge {
    pub fn new(py_backend_cmd: &str, rs_backend_cmd: &str, root: PathBuf) -> Self {
        Self {
            py: Arc::new(LspSession::new(
                py_backend_cmd,
                root.clone(),
                300,
                LangSpec::python(),
            )),
            rs: Arc::new(LspSession::new(rs_backend_cmd, root, 300, LangSpec::rust())),
        }
    }

    /// Route by file extension (case-sensitive), driven by each
    /// session's LangSpec. Unknown extensions are rejected loudly
    /// with the supported surface listed.
    pub fn session_for(&self, file: &str) -> Result<(Arc<LspSession>, PathBuf), String> {
        let path = PathBuf::from(file);
        let ext = path.extension().and_then(|e| e.to_str());
        let session = if ext == Some(self.py.lang.extension) {
            Arc::clone(&self.py)
        } else if ext == Some(self.rs.lang.extension) {
            Arc::clone(&self.rs)
        } else {
            return Err(format!(
                "unsupported file type .{}: {file} — this bridge serves .py (pyrefly backend) and .rs (rust-analyzer backend)",
                ext.unwrap_or(""),
            ));
        };
        Ok((session, path))
    }

    pub fn shutdown_all(&self) {
        let _ = self.py.shutdown();
        let _ = self.rs.shutdown();
    }
}

#[derive(Clone)]
pub struct LspBridgeServer {
    tool_router: ToolRouter<LspBridgeServer>,
    bridge: Arc<Bridge>,
}

impl LspBridgeServer {
    pub fn new(bridge: Arc<Bridge>) -> Self {
        Self {
            tool_router: Self::build_router(),
            bridge,
        }
    }

    fn build_router() -> ToolRouter<LspBridgeServer> {
        ToolRouter::new()
            .with_route((Self::lsp_status_tool_attr(), Self::lsp_status))
            .with_route((Self::hover_tool_attr(), Self::hover))
            .with_route((Self::check_file_tool_attr(), Self::check_file))
            .with_route((Self::edit_file_tool_attr(), Self::edit_file))
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for LspBridgeServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
}

impl LspBridgeServer {
    #[tool(
        description = "Bridge/backend health: per-backend server info, backend command, open-file count, liveness (py = pyrefly, rs = rust-analyzer). A backend whose binary is missing from PATH reports state=unavailable with install guidance"
    )]
    pub async fn lsp_status(&self) -> Result<CallToolResult, McpError> {
        let b = Arc::clone(&self.bridge);
        let text = tokio::task::spawn_blocking(move || {
            format!("{}\n{}", status_line("py", &b.py), status_line("rs", &b.rs))
        })
        .await
        .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(
        description = "Hover a Python (.py → pyrefly) or Rust (.rs → rust-analyzer) symbol: returns the upstream markdown type signature. file = absolute path (must exist on disk); line/character are zero-based, character counts UTF-16 code units (LSP convention). Files excluded by the repo's .gitignore may return no hover (upstream behavior)"
    )]
    pub async fn hover(
        &self,
        Parameters(HoverParams {
            file,
            line,
            character,
        }): Parameters<HoverParams>,
    ) -> Result<CallToolResult, McpError> {
        let (session, _path) = self
            .bridge
            .session_for(&file)
            .map_err(|e| McpError::invalid_params(e, None))?;
        let text =
            tokio::task::spawn_blocking(move || hover_impl(&session, &file, line, character))
                .await
                .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
                .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(
        description = "Type-check one Python (.py) or Rust (.rs) file: returns its latest diagnostics (severity, code, range, message). file = absolute path. Out-of-band disk edits are picked up automatically. Note (Rust): flycheck/cargo-check diagnostics run on the DISK content — in-memory edits see rust-analyzer's native diagnostics only"
    )]
    pub async fn check_file(
        &self,
        Parameters(FileParams { file }): Parameters<FileParams>,
    ) -> Result<CallToolResult, McpError> {
        let (session, _path) = self
            .bridge
            .session_for(&file)
            .map_err(|e| McpError::invalid_params(e, None))?;
        let text = tokio::task::spawn_blocking(move || check_file_impl(&session, &file))
            .await
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(
        description = "Replace a Python (.py) or Rust (.rs) file's content in the language server's workspace (full-content didChange; the disk file is NOT written). Run check_file afterwards for updated diagnostics"
    )]
    pub async fn edit_file(
        &self,
        Parameters(EditParams { file, content }): Parameters<EditParams>,
    ) -> Result<CallToolResult, McpError> {
        let (session, _path) = self
            .bridge
            .session_for(&file)
            .map_err(|e| McpError::invalid_params(e, None))?;
        let text = tokio::task::spawn_blocking(move || edit_file_impl(&session, &file, &content))
            .await
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

/// POSIX PATH lookup for one backend program — the same resolution
/// `Command::new` would use, probed up front so `lsp_status` can tell
/// "backend binary missing" (wheel machines without rust-analyzer,
/// SM-8) from "not spawned yet". Bare names walk `PATH`; overrides
/// containing a path separator are checked directly. No external
/// `which` dependency.
pub fn backend_available(cmd: &str) -> bool {
    fn executable(p: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(p)
                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            p.is_file()
        }
    }
    if cmd.contains('/') {
        return executable(Path::new(cmd));
    }
    let Ok(search) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&search).any(|dir| executable(&dir.join(cmd)))
}

/// One `lsp_status` line: availability first (missing binary ⇒
/// `state=unavailable` + the LangSpec install hint), then live session
/// state once the backend exists.
pub fn status_line(tag: &str, s: &LspSession) -> String {
    if !backend_available(s.backend_cmd()) {
        return format!(
            "{tag}: backend={} server=n/a open_files=0 state=unavailable (binary not found; install: {})",
            s.backend_cmd(),
            s.lang.install_hint
        );
    }
    format!(
        "{tag}: backend={} server={} open_files={} state={}",
        s.backend_cmd(),
        s.server_info(),
        s.open_files.lock().unwrap().len(),
        if s.is_dead() { "dead" } else { "alive" }
    )
}

pub fn hover_impl(s: &LspSession, file: &str, line: u32, character: u32) -> Result<String, String> {
    let path = PathBuf::from(file);
    s.sync_open(&path)?;
    let uri = LspSession::file_uri(&path);
    // The retry window is per-backend (pyrefly: 500ms; rust-analyzer:
    // 30s — cold-loading a whole cargo workspace takes seconds).
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(s.lang.hover_retry_ms);
    loop {
        let resp = match s.request(
            "textDocument/hover",
            serde_json::json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        ) {
            Ok(r) => r,
            // rust-analyzer returns -32801 "content modified" when the
            // request races its file-change processing — transient,
            // retry like a null hover.
            Err(e) if e.contains("content modified") => {
                if std::time::Instant::now() >= deadline {
                    return Err(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e),
        };
        let value = resp
            .pointer("/result/contents/value")
            .and_then(|v| v.as_str());
        if let Some(v) = value {
            if !v.trim().is_empty() {
                return Ok(v.to_string());
            }
        }
        // Null/empty hover: transient while the backend warms up —
        // bounded retry before concluding "no symbol here" (P1 F-05).
        if std::time::Instant::now() >= deadline {
            return Ok(format!("no hover at {line}:{character}"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn check_file_impl(s: &LspSession, file: &str) -> Result<String, String> {
    let path = PathBuf::from(file);
    let uri = LspSession::file_uri(&path);
    let mut mutation_at = s.sync_open(&path)?;
    let mut overlay_version = s
        .overlay
        .lock()
        .unwrap()
        .get(&path)
        .map(|e| e.version)
        .unwrap_or(1);
    let call_start = std::time::Instant::now();
    let deadline = call_start + std::time::Duration::from_millis(s.lang.slow_timeout_ms);
    let half = std::time::Duration::from_millis(s.lang.slow_timeout_ms / 2);
    let mut reissued = false;
    loop {
        // F1: the freshness basis is the NEWER of this call's own
        // mutation and the overlay entry's last_mutation (stamped at
        // every mutation origin on the session side) — a nudge-path
        // check no longer runs with a None basis and passes poisoned
        // stale entries. `Option<Instant>` max: None < Some — the newer
        // of whichever origins exist. None → fresh below is a defensive
        // default (post-F1 the overlay entry always carries a stamp).
        let overlay_mut = s
            .overlay
            .lock()
            .unwrap()
            .get(&path)
            .and_then(|e| e.last_mutation);
        let basis = mutation_at.max(overlay_mut);
        // Convergence (P1 R-07): the cached push must (a) carry the
        // overlay's current version or newer — this alone answers the
        // no-mutation repeat check from cache, (b) postdate the
        // mutation basis, and (c) be per-URI quiesced. A pure
        // push model means no new push arrives without a mutation, so
        // waiting without these guards serves stale answers.
        let entry = s.diag_cache.lock().unwrap().get(&uri).cloned();
        let fresh = match &entry {
            Some(e) => basis.map(|b| e.last_push > b).unwrap_or(true),
            None => false,
        };
        if let Some(e) = &entry {
            // version must be present and >= the overlay's — the
            // version-less pushes (e.g. a didClose empty push from a
            // concurrent eviction) must never read as converged (P1 F6).
            let version_ok = e.version.map(|v| v >= overlay_version).unwrap_or(false);
            let quiesced = std::time::Instant::now().duration_since(e.last_push) >= s.quiesce;
            if version_ok && fresh && quiesced {
                return Ok(format_diags(&e.diagnostics));
            }
        }
        // Stalled re-issue: a backend can silently drop a push (the
        // probe-verified rust-analyzer didChange drop after a warm
        // hover; also pyrefly dropping a re-open push after an LRU
        // eviction storm — lru_evict regression pin). F2 recasts the
        // stall test in TIME semantics — no push newer than the basis
        // past half the deadline ⇒ stalled ⇒ recover via close+
        // re-open once — because a poisoned eviction push carries
        // version+1 and defeats any version comparison. Absence
        // semantics preserved (no entry / no version ⇒ stalled: the
        // existing recovery path for backends that drop the re-open
        // push). NOT gated on overlay_version > 1: the eviction
        // re-open path replays at version 1 and needs this recovery
        // too (a spurious re-issue on a slow FIRST push costs one
        // close+re-open and still converges — accepted trade).
        if !reissued && std::time::Instant::now() >= deadline - half {
            let stalled = match &entry {
                None => true,
                Some(e) if e.version.is_none() => true,
                Some(_) => {
                    !fresh && std::time::Instant::now() >= basis.unwrap_or(call_start) + half
                }
            };
            if stalled {
                mutation_at = Some(s.force_reopen(&path)?);
                overlay_version = 1;
                reissued = true;
                continue;
            }
        }
        if std::time::Instant::now() >= deadline {
            let partial = s
                .diag_cache
                .lock()
                .unwrap()
                .get(&uri)
                .map(|e| format_diags(&e.diagnostics))
                .unwrap_or_else(|| "no diagnostics received yet".to_string());
            return Ok(format!(
                "{partial}\n[WARN] not converged within the deadline"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn format_diags(diags: &[serde_json::Value]) -> String {
    let mut out = format!("count={}\n", diags.len());
    for d in diags {
        let sev = d.get("severity").and_then(|v| v.as_i64()).unwrap_or(0);
        let code = d
            .get("code")
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_else(|| "-".to_string());
        let line = d
            .pointer("/range/start/line")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let col = d
            .pointer("/range/start/character")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let msg = d.get("message").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!("sev={sev} code={code} {line}:{col} {msg}\n"));
    }
    out
}

pub fn edit_file_impl(s: &LspSession, file: &str, content: &str) -> Result<String, String> {
    let path = PathBuf::from(file);
    s.sync_open(&path)?;
    s.apply_edit(&path, content)?;
    Ok(format!(
        "edited, {} bytes — run check_file for updated diagnostics",
        content.len()
    ))
}
