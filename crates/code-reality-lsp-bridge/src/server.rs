//! MCP server face of the bridge (rmcp, stdio). Tools are thin: they
//! hold no LSP state of their own — every interaction goes through the
//! shared `LspSession` (serialized), and blocking work runs on
//! `spawn_blocking` so the async runtime stays free.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::ErrorCode;
use rmcp::{tool, tool_handler, ErrorData as McpError};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::session::LspSession;

#[derive(Deserialize, JsonSchema)]
pub struct HoverParams {
    /// Absolute path of the Python file (must exist on disk).
    pub file: String,
    /// Zero-based line (LSP convention).
    pub line: u32,
    /// Zero-based character offset in UTF-16 code units (LSP convention).
    pub character: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct FileParams {
    /// Absolute path of the Python file (must exist on disk).
    pub file: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EditParams {
    /// Absolute path of the Python file (must exist on disk).
    pub file: String,
    /// Full replacement content of the file.
    pub content: String,
}

#[derive(Clone)]
pub struct LspBridgeServer {
    tool_router: ToolRouter<LspBridgeServer>,
    session: Arc<LspSession>,
}

impl LspBridgeServer {
    pub fn new(session: Arc<LspSession>) -> Self {
        Self {
            tool_router: Self::build_router(),
            session,
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
    #[tool(description = "Bridge/backend health: server info, backend command, open-file count, liveness")]
    pub async fn lsp_status(&self) -> Result<CallToolResult, McpError> {
        let s = Arc::clone(&self.session);
        let text = tokio::task::spawn_blocking(move || {
            format!(
                "backend={} server={} open_files={} state={}",
                s.backend_cmd(),
                s.server_info(),
                s.open_files.lock().unwrap().len(),
                if s.is_dead() { "dead" } else { "alive" }
            )
        })
        .await
        .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(description = "Hover a Python symbol: returns the upstream markdown type signature. file = absolute path (must exist on disk); line/character are zero-based, character counts UTF-16 code units (LSP convention). Files excluded by the repo's .gitignore return no hover (upstream behavior)")]
    pub async fn hover(
        &self,
        Parameters(HoverParams { file, line, character }): Parameters<HoverParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = ensure_py(&file).map_err(|e| McpError::invalid_params(e, None))?;
        let s = Arc::clone(&self.session);
        let file = path.to_string_lossy().to_string();
        let text = tokio::task::spawn_blocking(move || hover_impl(&s, &file, line, character))
            .await
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(description = "Type-check one Python file: returns its latest diagnostics (severity, code, range, message). file = absolute path. Out-of-band disk edits are picked up automatically")]
    pub async fn check_file(
        &self,
        Parameters(FileParams { file }): Parameters<FileParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = ensure_py(&file).map_err(|e| McpError::invalid_params(e, None))?;
        let s = Arc::clone(&self.session);
        let file = path.to_string_lossy().to_string();
        let text = tokio::task::spawn_blocking(move || check_file_impl(&s, &file))
            .await
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(description = "Replace a Python file's content in the language server's workspace (full-content didChange; the disk file is NOT written). Run check_file afterwards for updated diagnostics")]
    pub async fn edit_file(
        &self,
        Parameters(EditParams { file, content }): Parameters<EditParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = ensure_py(&file).map_err(|e| McpError::invalid_params(e, None))?;
        let s = Arc::clone(&self.session);
        let file = path.to_string_lossy().to_string();
        let text = tokio::task::spawn_blocking(move || edit_file_impl(&s, &file, &content))
            .await
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

pub fn ensure_py(file: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(file);
    if path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err(format!(
            "not a Python file: {file} — this bridge only serves .py files"
        ));
    }
    Ok(path)
}

pub fn hover_impl(s: &LspSession, file: &str, line: u32, character: u32) -> Result<String, String> {
    let path = ensure_py(file)?;
    s.sync_open(&path)?;
    let uri = LspSession::file_uri(&path);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        let resp = s.request(
            "textDocument/hover",
            serde_json::json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        )?;
        let value = resp
            .pointer("/result/contents/value")
            .and_then(|v| v.as_str());
        if let Some(v) = value {
            return Ok(v.to_string());
        }
        // Null hover: transient while background indexing warms up —
        // retry briefly before concluding "no symbol here" (EP R-05).
        if std::time::Instant::now() >= deadline {
            return Ok(format!("no hover at {line}:{character}"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn check_file_impl(s: &LspSession, file: &str) -> Result<String, String> {
    let path = ensure_py(file)?;
    let mutation_at = s.sync_open(&path)?;
    let uri = LspSession::file_uri(&path);
    let overlay_version = s
        .overlay
        .lock()
        .unwrap()
        .get(&path)
        .map(|e| e.version)
        .unwrap_or(1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        // Convergence (EP R-07): the cached push must (a) carry the
        // overlay's current version or newer — this alone answers the
        // no-mutation repeat check from cache, (b) postdate any
        // mutation sent THIS call, and (c) be per-URI quiesced. A pure
        // push model means no new push arrives without a mutation, so
        // waiting without these guards serves stale answers.
        let entry = s.diag_cache.lock().unwrap().get(&uri).cloned();
        if let Some(e) = entry {
            // version must be present and >= the overlay's — the
            // version-less pushes (e.g. a didClose empty push from a
            // concurrent eviction) must never read as converged (F6).
            let version_ok = e.version.map(|v| v >= overlay_version).unwrap_or(false);
            let fresh = mutation_at.map(|t| e.last_push > t).unwrap_or(true);
            let quiesced =
                std::time::Instant::now().duration_since(e.last_push) >= s.quiesce;
            if version_ok && fresh && quiesced {
                return Ok(format_diags(&e.diagnostics));
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
            return Ok(format!("{partial}\n[WARN] not converged within 10s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn format_diags(diags: &[serde_json::Value]) -> String {
    let mut out = format!("count={}\n", diags.len());
    for d in diags {
        let sev = d.get("severity").and_then(|v| v.as_i64()).unwrap_or(0);
        let code = d.get("code").and_then(|v| v.as_str()).unwrap_or("-");
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
    let path = ensure_py(file)?;
    s.sync_open(&path)?;
    s.apply_edit(&path, content)?;
    Ok(format!(
        "edited, {} bytes — run check_file for updated diagnostics",
        content.len()
    ))
}
