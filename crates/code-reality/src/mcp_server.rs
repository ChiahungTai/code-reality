//! `mcp_server` — the R6 unified MCP interface (AD-2/AD-3): one HTTP
//! resident on `127.0.0.1:8200` serving every repo via a per-call
//! `repo_root` parameter (no session/workspace binding — repo is a
//! parameter, not topology). Tools thin-wrap the SAME lib the umbrella
//! CLI uses (in-process; drift is a compile error), with per-request
//! isolation: tool errors surface as loud MCP errors and lib panics on
//! hostile sidecars are caught at the handler boundary (SM-14) so one
//! poisoned repo never takes the daemon down.
//!
//! Tool surface v0 (the SCIP family four — snapshot/transition/tours
//! stay CLI; skills subprocess-consume them, YAGNI):
//! `refs(symbol, repo_root)` / `callers(symbol, repo_root)` /
//! `closure(symbol, repo_root, depth)` / `audit(repo_root)`.
//! Responses carry `[SRC]` passthrough and a `[STDERR]` section.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorCode};
use rmcp::{tool, tool_handler, ErrorData as McpError};

// Typed tool-parameter structs — rmcp 3.1.4 routes params through
// FromContextPart, which bare primitives don't implement; Parameters<T>
// is the supported extraction wrapper (schema derives from the struct).
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct RefsParams {
    pub symbol: String,
    pub repo_root: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ClosureParams {
    pub symbol: String,
    pub repo_root: String,
    pub depth: Option<u32>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct AuditParams {
    pub repo_root: String,
}

#[derive(Clone, Default)]
pub struct CodeRealityServer {
    tool_router: ToolRouter<CodeRealityServer>,
}

/// ToolOutput → MCP content: stdout text + a `[STDERR]` section when the
/// management face has content (visibility rule).
fn to_tool_result(out: crate::ToolOutput) -> Result<CallToolResult, McpError> {
    let mut text = out.stdout;
    if !out.stderr.is_empty() {
        text.push_str("[STDERR]\n");
        text.push_str(&out.stderr);
    }
    // non-exhaustive struct — build via success() then set is_error
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    if out.exit_code != 0 {
        result.is_error = Some(true);
    }
    Ok(result)
}

fn map_tool_output(out: crate::ToolOutput) -> Result<CallToolResult, McpError> {
    // Per-request isolation (SM-14): exit != 0 maps to an MCP tool error
    // (loud, per-request) — the daemon itself stays alive.
    if out.exit_code != 0 {
        let mut text = String::new();
        if !out.stderr.is_empty() {
            text.push_str("[STDERR]\n");
            text.push_str(&out.stderr);
        }
        return Err(McpError::new(
            ErrorCode::INTERNAL_ERROR,
            format!("工具退出碼 {}：{}", out.exit_code, text.trim()),
            None,
        ));
    }
    to_tool_result(out)
}

impl CodeRealityServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::build_router(),
        }
    }

    fn build_router() -> ToolRouter<Self> {
        ToolRouter::new()
            .with_route((Self::refs_tool_attr(), Self::refs))
            .with_route((Self::callers_tool_attr(), Self::callers))
            .with_route((Self::closure_tool_attr(), Self::closure))
            .with_route((Self::audit_tool_attr(), Self::audit))
    }

    async fn run_refs_like(&self, args: Vec<String>) -> Result<CallToolResult, McpError> {
        // SM-14 isolation in two layers: blocking I/O leaves the async
        // runtime free, and catch_unwind maps data-driven panics (hostile
        // sidecars) to per-request loud errors — the daemon survives
        let out = tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::cli::run(&refs)
            }))
        })
        .await
        .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, format!("任務 join 失敗：{e}"), None))?
        .map_err(|_| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                "lib panic（毒化 sidecar？）——已隔離為單請求錯誤",
                None,
            )
        })?;
        map_tool_output(out)
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for CodeRealityServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
}

// The macro-generated router needs the tools defined on the handler type;
// rmcp's #[tool] places them in an impl block — define a second impl with
// the four tools.
impl CodeRealityServer {
    /// Symbol truth query: refs/defs with trait disambiguation. Same
    /// lib call as `code-reality scip_refs <symbol> --repo <repo_root>`.
    #[tool(description = "Symbol truth query (refs/defs, trait disambiguation) over the repo's SCIP index")]
    pub async fn refs(
        &self,
        Parameters(RefsParams { symbol, repo_root }): Parameters<RefsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = vec![
            "scip_refs".to_string(),
            symbol,
            "--repo".to_string(),
            repo_root,
        ];
        self.run_refs_like(args).await
    }

    /// Caller-edge query: callers with site lines (call_edges set).
    #[tool(description = "Caller edges of a symbol (sites included; item-level refs noted)")]
    pub async fn callers(
        &self,
        Parameters(RefsParams { symbol, repo_root }): Parameters<RefsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = vec![
            "scip_refs".to_string(),
            symbol,
            "--callers".to_string(),
            "--repo".to_string(),
            repo_root,
        ];
        self.run_refs_like(args).await
    }

    /// Transitive closure of caller edges (BFS, default depth 2).
    #[tool(description = "Closure of caller edges (BFS; depth default 2, max 10000)")]
    pub async fn closure(
        &self,
        Parameters(ClosureParams { symbol, repo_root, depth }): Parameters<ClosureParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "scip_refs".to_string(),
            symbol,
            "--closure".to_string(),
            "--repo".to_string(),
            repo_root,
        ];
        if let Some(d) = depth {
            args.push("--depth".to_string());
            args.push(d.to_string());
        }
        self.run_refs_like(args).await
    }

    /// Completeness governance: graph_audit missing list reconciled
    /// against SCIP refs (in-process two-pass).
    #[tool(description = "Completeness audit: graph_audit gaps × SCIP refs (two-pass)")]
    pub async fn audit(
        &self,
        Parameters(AuditParams { repo_root }): Parameters<AuditParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = vec![
            "scip_refs".to_string(),
            "--audit".to_string(),
            "--repo".to_string(),
            repo_root,
        ];
        self.run_refs_like(args).await
    }
}

/// Bin entry helper: serve streamable-http on the given port. The
/// session manager is in-memory (single-user daemon; launchd owns the
/// lifecycle).
pub async fn serve(port: u16) -> Result<(), String> {
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService,
    };
    use std::sync::Arc;

    let service = StreamableHttpService::new(
        || Ok(CodeRealityServer::new()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    // axum serving mirrors the upstream rmcp test pattern (nest at /mcp —
    // the conventional MCP endpoint path)
    let app = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{port} 失敗：{e}"))?;
    eprintln!("[OK] code-reality MCP listening on 127.0.0.1:{port}/mcp");
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("serve 失敗：{e}"))
}

