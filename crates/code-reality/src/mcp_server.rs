//! `mcp_server` — the R6 unified MCP interface (AD-2/AD-3): one HTTP
//! resident on `127.0.0.1:8200` serving every repo via a per-call
//! `repo_root` parameter (no session/workspace binding — repo is a
//! parameter, not topology). Tools thin-wrap the SAME lib the umbrella
//! CLI uses (in-process; drift is a compile error), with per-request
//! isolation: tool errors surface as loud MCP errors and lib panics on
//! hostile sidecars are caught at the handler boundary (SM-14) so one
//! poisoned repo never takes the daemon down.
//!
//! Tool surface v1: the SCIP family (`refs`/`callers`/`closure`/
//! `audit`), the graph-engine parity family, and — since 0.6.0
//! (ep-mcp-data-plane-tools) — the data-plane four (`build` /
//! `snapshot` / `delta_tour` / `project`). The original read-only
//! "snapshot/tours stay CLI; skills subprocess-consume them" YAGNI
//! stance was re-adjudicated 2026-08-29: the EP/build loop now drives
//! the data plane in-session, so the MCP face carries write side
//! effects — every data-plane tool description states its write
//! target, and build's minutes-level no-progress semantics live in its
//! description too. Responses carry `[SRC]` passthrough and a
//! `[STDERR]` section.

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

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqRepoParams {
    pub repo_root: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqFilesOnlyParams {
    pub repo_root: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqFilesParams {
    pub repo_root: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqLimitParams {
    pub repo_root: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqCommunitiesParams {
    pub repo_root: String,
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    /// Deprecated (v1+ S4): union edges materialize at `graph_db build`
    /// time — queries are always full-graph now; the flag is a no-op.
    pub use_union: Option<bool>,
    #[serde(default)]
    /// "minimal" (default): summary fields only; "standard": member lists.
    pub detail_level: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqUnionParams {
    pub repo_root: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    /// Deprecated (v1+ S4): union edges materialize at `graph_db build`
    /// time — queries are always full-graph now; the flag is a no-op.
    pub use_union: Option<bool>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqUnionLimitParams {
    pub repo_root: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    /// Deprecated (v1+ S4): union edges materialize at `graph_db build`
    /// time — queries are always full-graph now; the flag is a no-op.
    pub use_union: Option<bool>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqDetailParams {
    pub repo_root: String,
    /// flows cap (default 50; CRG list_flows parity)
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqCommunityParams {
    pub repo_root: String,
    /// partial community name (from list_communities)
    pub community_name: String,
    /// include the member list (default false — CRG parity)
    #[serde(default)]
    pub include_members: Option<bool>,
}

/// architecture_overview params (rmcp takes ONE Parameters wrapper —
/// merged instead of a second wrapper)
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqArchOverviewParams {
    pub repo_root: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub detail_level: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqDocumentSymbolsParams {
    pub repo_root: String,
    pub file: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqSearchParams {
    pub repo_root: String,
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GqTaskParams {
    pub repo_root: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub files: Vec<String>,
}

// ---------- data-plane family (EP ep-mcp-data-plane-tools) ----------

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct BuildParams {
    pub repo_root: String,
    #[serde(default)]
    pub producer: Option<String>,
    #[serde(default)]
    pub json: Option<bool>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SnapshotParams {
    pub repo_root: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub out_dir: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DeltaTourParams {
    pub repo_root: String,
    pub snapshot_a: String,
    pub snapshot_b: String,
    #[serde(default)]
    pub ep: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub out_dir: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ProjectParams {
    pub repo_root: String,
    pub plan: String,
    #[serde(default)]
    pub json: Option<bool>,
}

#[derive(Clone, Default)]
pub struct CodeRealityServer {
    tool_router: ToolRouter<CodeRealityServer>,
}

/// ToolOutput → MCP content: stdout text + a `[STDERR]` section when the
/// management face has content (visibility rule).
/// MCP single-frame byte cap (backstop — the payload-shape fixes
/// (detail_level/limit) are the real defense; this catches anything that
/// still slips through, before the client tears the connection down).
const MCP_TEXT_CAP: usize = 1 << 20;

fn apply_text_cap(mut text: String) -> String {
    if text.len() > MCP_TEXT_CAP {
        // cut at a UTF-8 char boundary
        let mut cut = MCP_TEXT_CAP;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str(&format!(
            "\n[TRUNCATED] output exceeded {} bytes — pass a smaller detail_level/limit, or use the CLI face (graph_query ... | grep/jq)\n",
            MCP_TEXT_CAP
        ));
    }
    text
}

fn to_tool_result(out: crate::ToolOutput) -> Result<CallToolResult, McpError> {
    let mut text = out.stdout;
    if !out.stderr.is_empty() {
        text.push_str("[STDERR]\n");
        text.push_str(&out.stderr);
    }
    text = apply_text_cap(text);
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
        // WARN/FAIL faces print to stdout (e.g. "[WARN] 查無 DEF：<sym>") —
        // an error text built from stderr alone surfaces as an empty
        // reason (2026-08-29 battery)
        let mut text = out.stdout;
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
            .with_route((Self::impact_radius_tool_attr(), Self::impact_radius))
            .with_route((Self::detect_changes_tool_attr(), Self::detect_changes))
            .with_route((Self::hub_nodes_tool_attr(), Self::hub_nodes))
            .with_route((Self::bridge_nodes_tool_attr(), Self::bridge_nodes))
            .with_route((Self::list_communities_tool_attr(), Self::list_communities))
            .with_route((
                Self::architecture_overview_tool_attr(),
                Self::architecture_overview,
            ))
            .with_route((Self::list_flows_tool_attr(), Self::list_flows))
            .with_route((Self::affected_flows_tool_attr(), Self::affected_flows))
            .with_route((
                Self::get_minimal_context_tool_attr(),
                Self::get_minimal_context,
            ))
            .with_route((
                Self::get_review_context_tool_attr(),
                Self::get_review_context,
            ))
            .with_route((Self::semantic_search_tool_attr(), Self::semantic_search))
            .with_route((Self::document_symbols_tool_attr(), Self::document_symbols))
            .with_route((Self::get_community_tool_attr(), Self::get_community))
            .with_route((Self::build_tool_attr(), Self::build))
            .with_route((Self::snapshot_tool_attr(), Self::snapshot))
            .with_route((Self::delta_tour_tool_attr(), Self::delta_tour))
            .with_route((Self::project_tool_attr(), Self::project))
    }

    /// Shared per-request isolation (SM-14): blocking I/O leaves the
    /// async runtime free, and catch_unwind maps data-driven panics
    /// (hostile sidecars) to per-request loud errors — the daemon
    /// survives. Every tool family funnels through here with its
    /// module's argv `run` (refs via `cli::run`, graph via
    /// `graph_engine::run`, data-plane via the module `run`s); panic
    /// payloads are surfaced (the former gq form — strictly more
    /// informative than the refs-era fixed string).
    async fn run_module<F>(f: F, args: Vec<String>) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(&[&str]) -> crate::ToolOutput + Send + 'static,
    {
        let out = tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&refs)))
        })
        .await
        .map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("任務 join 失敗：{e}"),
                None,
            )
        })?
        .map_err(|payload| {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".into());
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("lib panic：{msg}——已隔離為單請求錯誤"),
                None,
            )
        })?;
        map_tool_output(out)
    }

    async fn run_refs_like(&self, args: Vec<String>) -> Result<CallToolResult, McpError> {
        Self::run_module(crate::cli::run, args).await
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
    #[tool(
        description = "Symbol truth query (refs/defs, trait disambiguation) over the repo's SCIP index"
    )]
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
        Parameters(ClosureParams {
            symbol,
            repo_root,
            depth,
        }): Parameters<ClosureParams>,
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

    fn reject_comma_files(files: &[String]) -> Result<(), McpError> {
        if let Some(bad) = files.iter().find(|f| f.contains(',')) {
            return Err(McpError::new(
                ErrorCode::INVALID_PARAMS,
                format!("files 元素含逗號（comma-join argv 編碼限制）：{bad}"),
                None,
            ));
        }
        Ok(())
    }

    async fn gq(&self, args: Vec<String>) -> Result<CallToolResult, McpError> {
        Self::run_module(crate::graph_engine::run, args).await
    }

    /// gq + a loud (non-breaking) deprecation notice when the client asked
    /// for the retired union flag — silent no-ops teach callers nothing.
    async fn gq_union_deprecated(
        &self,
        args: Vec<String>,
        requested_union: bool,
    ) -> Result<CallToolResult, McpError> {
        let mut res = self.gq(args).await?;
        if requested_union {
            res.content.insert(
                0,
                ContentBlock::text(
                    "[DEPRECATED] use_union is a no-op (v1+ S4): union edges materialize at `graph_db build` time — queries are full-graph",
                ),
            );
        }
        Ok(res)
    }

    #[tool(
        description = "Blast radius of changed files: impacted nodes/files with best-path impact scores"
    )]
    pub async fn impact_radius(
        &self,
        Parameters(GqUnionParams {
            repo_root,
            files,
            max_depth,
            use_union,
        }): Parameters<GqUnionParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "impact_radius".into(),
            "--repo".into(),
            repo_root,
        ];
        if !files.is_empty() {
            Self::reject_comma_files(&files)?;
            args.push("--files".into());
            args.push(files.join(","));
        }
        if let Some(d) = max_depth {
            args.push("--depth".into());
            args.push(d.to_string());
        }
        self.gq_union_deprecated(args, use_union.unwrap_or(false))
            .await
    }

    #[tool(
        description = "Risk-scored review guidance from changed files: changed functions, test gaps, priorities"
    )]
    pub async fn detect_changes(
        &self,
        Parameters(GqFilesOnlyParams { repo_root, files }): Parameters<GqFilesOnlyParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "detect_changes".into(),
            "--repo".into(),
            repo_root,
        ];
        if !files.is_empty() {
            Self::reject_comma_files(&files)?;
            args.push("--files".into());
            args.push(files.join(","));
        }
        self.gq(args).await
    }

    #[tool(description = "Most connected nodes (in+out degree, all edge kinds)")]
    pub async fn hub_nodes(
        &self,
        Parameters(GqUnionLimitParams {
            repo_root,
            limit,
            use_union,
        }): Parameters<GqUnionLimitParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = vec![
            "graph_query".into(),
            "hub".into(),
            "--repo".into(),
            repo_root,
            "--limit".into(),
            limit.unwrap_or(10).to_string(),
        ];
        self.gq_union_deprecated(args, use_union.unwrap_or(false))
            .await
    }

    #[tool(description = "Architectural chokepoints by (sampled) betweenness centrality")]
    pub async fn bridge_nodes(
        &self,
        Parameters(GqUnionLimitParams {
            repo_root,
            limit,
            use_union,
        }): Parameters<GqUnionLimitParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = vec![
            "graph_query".into(),
            "bridge".into(),
            "--repo".into(),
            repo_root,
            "--limit".into(),
            limit.unwrap_or(10).to_string(),
        ];
        self.gq_union_deprecated(args, use_union.unwrap_or(false))
            .await
    }

    #[tool(
        description = "Communities: directory (CRG Tier-0 parity) or seeded Leiden; detail_level=minimal by default (summary fields only — use standard for member lists)"
    )]
    pub async fn list_communities(
        &self,
        Parameters(GqCommunitiesParams {
            repo_root,
            algorithm,
            use_union,
            detail_level,
        }): Parameters<GqCommunitiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "communities".into(),
            "--repo".into(),
            repo_root,
        ];
        if detail_level.as_deref() != Some("standard") {
            args.push("--detail-level".into());
            args.push("minimal".into());
        }
        match algorithm.as_deref() {
            None | Some("directory") => {}
            Some("leiden") => {
                args.push("--leiden".into());
            }
            Some(other) => {
                return Err(McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("algorithm 須為 directory|leiden，收到：{other}"),
                    None,
                ));
            }
        }
        self.gq_union_deprecated(args, use_union.unwrap_or(false))
            .await
    }

    #[tool(
        description = "Communities + cross-community edge pairs + high-coupling warnings; detail_level=minimal by default (CRG _minimal_overview parity)"
    )]
    pub async fn architecture_overview(
        &self,
        Parameters(GqArchOverviewParams {
            repo_root,
            limit,
            detail_level,
        }): Parameters<GqArchOverviewParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "arch_overview".into(),
            "--repo".into(),
            repo_root,
        ];
        if let Some(l) = limit {
            args.push("--max-results".into());
            args.push(l.to_string());
        }
        match detail_level.as_deref() {
            None | Some("minimal") => {
                args.push("--detail-level".into());
                args.push("minimal".into());
            }
            Some("standard") => {}
            Some(other) => {
                return Err(McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("detail_level 須為 minimal|standard，收到：{other}"),
                    None,
                ));
            }
        }
        self.gq(args).await
    }

    #[tool(
        description = "Execution flows from entry points (forward BFS over CALLS, criticality-sorted); limit defaults to 50 — pass a larger one for more"
    )]
    pub async fn list_flows(
        &self,
        Parameters(GqDetailParams { repo_root, limit }): Parameters<GqDetailParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "flows".into(),
            "--repo".into(),
            repo_root,
        ];
        if let Some(l) = limit {
            args.push("--limit".into());
            args.push(l.to_string());
        }
        self.gq(args).await
    }

    #[tool(
        description = "Single-community drill-down by partial name; include_members=false by default (CRG get_community parity)"
    )]
    pub async fn get_community(
        &self,
        Parameters(GqCommunityParams {
            repo_root,
            community_name,
            include_members,
        }): Parameters<GqCommunityParams>,
    ) -> Result<CallToolResult, McpError> {
        // in-process call (no CLI op — the drill-down is an MCP face)
        let repo = repo_root.clone();
        let needle = community_name.clone();
        let with_members = include_members.unwrap_or(false);
        let out = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let conn = crate::graph_engine::open(std::path::Path::new(&repo))?;
                crate::graph_engine::get_community(&conn, &needle, with_members)
            }))
        })
        .await
        .map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("任務 join 失敗：{e}"),
                None,
            )
        })?
        .map_err(|payload| {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".into());
            McpError::new(ErrorCode::INTERNAL_ERROR, format!("lib panic：{msg}"), None)
        })?;
        let v = out.map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e, None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            apply_text_cap(crate::common::to_json_indent1(&v)),
        )]))
    }

    #[tool(description = "Flows whose path touches the changed files")]
    pub async fn affected_flows(
        &self,
        Parameters(GqFilesOnlyParams { repo_root, files }): Parameters<GqFilesOnlyParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "affected_flows".into(),
            "--repo".into(),
            repo_root,
        ];
        if !files.is_empty() {
            Self::reject_comma_files(&files)?;
            args.push("--files".into());
            args.push(files.join(","));
        }
        self.gq(args).await
    }

    #[tool(
        description = "Ultra-compact entry context: stats, risk band, top communities/flows, next-tool suggestions"
    )]
    pub async fn get_minimal_context(
        &self,
        Parameters(GqTaskParams {
            repo_root,
            task,
            files,
        }): Parameters<GqTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "minimal_context".into(),
            "--repo".into(),
            repo_root,
        ];
        if !task.is_empty() {
            args.push("--task".into());
            args.push(task);
        }
        if !files.is_empty() {
            Self::reject_comma_files(&files)?;
            args.push("--files".into());
            args.push(files.join(","));
        }
        self.gq(args).await
    }

    #[tool(description = "Focused review context: impact + source snippets + guidance")]
    pub async fn get_review_context(
        &self,
        Parameters(GqFilesParams {
            repo_root,
            files,
            max_depth,
        }): Parameters<GqFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "graph_query".into(),
            "review_context".into(),
            "--repo".into(),
            repo_root,
        ];
        if !files.is_empty() {
            Self::reject_comma_files(&files)?;
            args.push("--files".into());
            args.push(files.join(","));
        }
        if let Some(d) = max_depth {
            args.push("--depth".into());
            args.push(d.to_string());
        }
        self.gq(args).await
    }

    #[tool(description = "Keyword search (FTS5 with LIKE fallback; embeddings face not adopted)")]
    pub async fn semantic_search(
        &self,
        Parameters(GqSearchParams {
            repo_root,
            query,
            limit,
        }): Parameters<GqSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        self.gq(vec![
            "graph_query".into(),
            "search".into(),
            "--repo".into(),
            repo_root,
            "--query".into(),
            query,
            "--limit".into(),
            limit.unwrap_or(20).to_string(),
        ])
        .await
    }

    #[tool(
        description = "File outline from SCIP defining occurrences (documentSymbol-alike; hover/signatures stay LSP-only)"
    )]
    pub async fn document_symbols(
        &self,
        Parameters(GqDocumentSymbolsParams { repo_root, file }): Parameters<
            GqDocumentSymbolsParams,
        >,
    ) -> Result<CallToolResult, McpError> {
        self.gq(vec![
            "graph_query".into(),
            "symbols".into(),
            "--repo".into(),
            repo_root,
            "--query".into(),
            file,
        ])
        .await
    }

    // ---------- data-plane family (EP ep-mcp-data-plane-tools) ----------
    // The face carries WRITE side effects (re-adjudicated 2026-08-29):
    // each description states its write target and runtime shape so the
    // caller knows what a call does to the repo's data plane.

    /// One-shot data-plane build. Same lib as `code-reality build`.
    #[tool(
        description = "One-shot data-plane build: detect language face, spawn producers (pyrefly-index / rust-analyzer scip), rebuild graph.db + indexes. WRITES <repo>/.code-reality/ (index slot, graph.db). LONG-RUNNING: minutes-level on large repos, no progress reporting — the call blocks until done; set your client timeout accordingly. Same lib as `code-reality build --repo <repo>`"
    )]
    pub async fn build(
        &self,
        Parameters(BuildParams {
            repo_root,
            producer,
            json,
        }): Parameters<BuildParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(p) = producer.as_deref() {
            if p != "rust" && p != "python" {
                return Err(McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("producer 須為 rust 或 python，收到：{p}"),
                    None,
                ));
            }
        }
        let mut args = vec!["build".to_string(), "--repo".to_string(), repo_root];
        if let Some(p) = producer {
            args.push("--producer".to_string());
            args.push(p);
        }
        if json.unwrap_or(false) {
            args.push("--json".to_string());
        }
        Self::run_module(crate::build::run, args).await
    }

    /// Boundary snapshot. Same lib as `code-reality snapshot`.
    #[tool(
        description = "Boundary snapshot of the current graph.db state (files + module edges + staleness meta). WRITES a dated snapshot file under <repo>/.code-reality/snapshots/ (or out_dir). Requires an existing graph.db (run build first). Seconds-level. Same lib as `code-reality snapshot --repo <repo>`"
    )]
    pub async fn snapshot(
        &self,
        Parameters(SnapshotParams {
            repo_root,
            label,
            out_dir,
        }): Parameters<SnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec!["snapshot".to_string(), "--repo".to_string(), repo_root];
        if let Some(l) = label {
            args.push("--label".to_string());
            args.push(l);
        }
        if let Some(d) = out_dir {
            args.push("--out-dir".to_string());
            args.push(d);
        }
        Self::run_module(crate::snapshot::run, args).await
    }

    /// Delta-review CodeTour. Same lib as `code-reality delta_tour`.
    #[tool(
        description = "Diff two snapshots into a delta-review CodeTour (git hunk anchors, EP claims comparison). WRITES <repo>/.tours/delta/<date>-<task>.tour — MCP default is in-repo (unlike the CLI's cwd-relative default); pass out_dir to override. Pass ABSOLUTE snapshot paths (the snapshot tool's report carries them). Seconds-level. Same lib as `code-reality delta_tour <a> <b> --repo <repo>`"
    )]
    pub async fn delta_tour(
        &self,
        Parameters(DeltaTourParams {
            repo_root,
            snapshot_a,
            snapshot_b,
            ep,
            task,
            out_dir,
        }): Parameters<DeltaTourParams>,
    ) -> Result<CallToolResult, McpError> {
        // MCP callers have no meaningful server cwd — default the tour
        // tree into the repo (CodeTour consumers open the repo root),
        // unlike the CLI's cwd-relative default.
        let out_dir = out_dir.unwrap_or_else(|| format!("{repo_root}/.tours/delta"));
        let mut args = vec![
            "delta_tour".to_string(),
            snapshot_a,
            snapshot_b,
            "--repo".to_string(),
            repo_root,
            "--out-dir".to_string(),
            out_dir,
        ];
        if let Some(e) = ep {
            args.push("--ep".to_string());
            args.push(e);
        }
        if let Some(t) = task {
            args.push("--task".to_string());
            args.push(t);
        }
        Self::run_module(crate::delta_tour::run, args).await
    }

    /// Projected-graph overlay. Same lib as `code-reality project`.
    #[tool(
        description = "Projected-graph overlay for EP planning: compile a declarative plan.toml via overlay-gen, report graft surface + claim verdicts ([projected] = declarations, not evidence). WRITES <repo>/.code-reality/projections/<plan-stem>/; the real index slot stays untouched. Needs overlay-gen resolvable (uv tool install pyrefly-producer). Pass an ABSOLUTE plan path (the server's cwd is meaningless to you). Seconds-level. Same lib as `code-reality project --repo <repo> --plan <plan.toml>`"
    )]
    pub async fn project(
        &self,
        Parameters(ProjectParams {
            repo_root,
            plan,
            json,
        }): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = vec![
            "project".to_string(),
            "--repo".to_string(),
            repo_root,
            "--plan".to_string(),
            plan,
        ];
        if json.unwrap_or(false) {
            args.push("--json".to_string());
        }
        Self::run_module(crate::project::run, args).await
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

/// Stdio serving mode (open-source default, `--stdio`): the AI harness
/// spawns and owns this process — zero daemon, zero port, works on any
/// OS. The HTTP resident mode (launchd/`serve`) remains for
/// multi-harness sharing on a single machine.
pub async fn serve_stdio() -> Result<(), String> {
    use rmcp::ServiceExt;
    let server = CodeRealityServer::new();
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let peer = server
        .serve((stdin, stdout))
        .await
        .map_err(|e| format!("stdio serve 失敗：{e}"))?;
    peer.waiting()
        .await
        .map_err(|e| format!("stdio session 結束：{e}"))?;
    Ok(())
}
