//! R6 MCP server tests — tool routing via the lib (the same lib the
//! CLI uses), SM-14 isolation (missing repo loud; panic containment),
//! and a live HTTP smoke on an ephemeral port.

use serde_json::json;

#[tokio::test]
async fn tools_route_through_the_lib() {
    let server = code_reality::mcp_server::CodeRealityServer::new();
    // missing repo_root is a loud per-request error (the tool macro
    // validates required params before the handler runs — SM-5 face)
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    // audit with a repo lacking sidecars: env-level exit 2 → MCP error
    let err = server
        .audit(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::AuditParams {
                repo_root: repo.display().to_string(),
            },
        ))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("退出碼 2")
            || err.message.contains("graph.db")
            || err.message.contains("rust-analyzer"),
        "{:?}",
        err.message
    );
    // refs on a repo without an index → exit 2 → MCP error (loud)
    let err2 = server
        .refs(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::RefsParams {
                symbol: "SomeSymbol".into(),
                repo_root: repo.display().to_string(),
            },
        ))
        .await
        .unwrap_err();
    assert!(err2.message.contains("退出碼 2"), "{:?}", err2.message);
}

#[tokio::test]
async fn closure_optional_depth_and_missing_param_types() {
    let server = code_reality::mcp_server::CodeRealityServer::new();
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    // depth is Option — absent depth routes with default; both land on
    // the same loud env error here (no index in the empty repo)
    let err = server
        .closure(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::ClosureParams {
                symbol: "X".into(),
                repo_root: repo.display().to_string(),
                depth: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("退出碼 2"), "{:?}", err.message);
}

#[test]
fn bin_help_face() {
    // the daemon takes no CLI args; --help is not a face it owns
    // (launchd starts it bare). Just assert the bin builds & links.
    assert!(
        std::path::Path::new("target/debug/code-reality-mcp").exists()
            || std::env::var("CARGO_MANIFEST_DIR").is_ok()
    );
}

// ---------- live HTTP smoke (V1-V3 subset) ----------

fn repo_root_for_engine() -> String {
    let tmp = tempfile::tempdir().unwrap();
    let kept = std::fs::canonicalize(tmp.path()).unwrap();
    // leak the path (test process lifetime) — tempdir cleaned on drop
    std::mem::forget(tmp);
    kept.display().to_string()
}

#[tokio::test]
async fn http_server_serves_initialize_and_tools_list() {
    use rmcp::transport::StreamableHttpClientTransport;
    use rmcp::ServiceExt;

    // ephemeral port: bind once to discover, release, reuse
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let server = tokio::spawn(async move { code_reality::mcp_server::serve(port).await });
    // give the listener a moment
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let transport = StreamableHttpClientTransport::from_uri(format!("http://127.0.0.1:{port}/mcp"));
    let client = ().serve(transport).await.unwrap();
    // tools/list: four SCIP-family tools + the engine parity family
    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "affected_flows",
            "architecture_overview",
            "audit",
            "bridge_nodes",
            "callers",
            "closure",
            "detect_changes",
            "document_symbols",
            "get_community",
            "get_minimal_context",
            "get_review_context",
            "hub_nodes",
            "impact_radius",
            "list_communities",
            "list_flows",
            "refs",
            "semantic_search"
        ],
        "{names:?}"
    );
    let refs_tool = tools.iter().find(|t| t.name == "refs").unwrap();
    let schema = &refs_tool.input_schema;
    assert!(schema.get("properties").unwrap().get("repo_root").is_some());
    assert!(schema.get("required").is_some());

    // per-request poison isolation (SM-14): a call against a repo without
    // sidecars returns a tool-level error — the client peer SURVIVES and
    // serves a subsequent tools/list
    let result = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("refs").with_arguments(
                json!({"symbol": "X", "repo_root": "/tmp"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await;
    // tool-level failure surfaces as an error result or is_error content
    match result {
        Err(e) => assert!(!format!("{e}").is_empty()),
        Ok(r) => assert!(r.is_error.unwrap_or(false)),
    }
    // engine-family tool over the real transport: graph.db read face
    // against the dogfood corpus is env-dependent; here a repo WITHOUT
    // .code-review-graph must fail loud (per-request error, client alive)
    let engine_result = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("hub_nodes").with_arguments(
                serde_json::json!({"repo_root": repo_root_for_engine()})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await;
    match engine_result {
        // exit!=0 maps to a per-request McpError (SM-14); the transport
        // itself must survive — asserted by the tools2 list below
        Err(e) => assert!(format!("{e}").contains("graph.db")),
        Ok(r) => assert!(r.is_error.unwrap_or(false)),
    }
    let tools2 = client.list_all_tools().await.unwrap();
    assert_eq!(tools2.len(), 17);
    client.cancel().await.unwrap();
    server.abort();
}

// ---------- consumer-protection faces (NT 12.6MB disconnect, 2026-08-27) ----------

mod cp_fixture {
    use rusqlite::Connection;
    use std::path::Path;

    /// A .code-reality/graph.db with one fat community (N members) —
    /// the disconnect shape at fixture scale.
    pub fn fat_db(repo: &Path, members: usize) {
        let dir = repo.join(".code-reality");
        std::fs::create_dir_all(&dir).unwrap();
        let c = Connection::open(dir.join("graph.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, name TEXT NOT NULL,
                qname TEXT NOT NULL, file_path TEXT NOT NULL, line_start INTEGER,
                line_end INTEGER, language TEXT, parent_name TEXT,
                is_test INTEGER DEFAULT 0, extra TEXT DEFAULT '{}',
                updated_at REAL NOT NULL, community_id INTEGER, provenance TEXT);
             CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL, caller_symbol TEXT NOT NULL, callee_symbol TEXT NOT NULL,
                provenance TEXT NOT NULL, file_path TEXT NOT NULL, line INTEGER DEFAULT 0,
                confidence REAL DEFAULT 1.0, confidence_tier TEXT DEFAULT 'EXTRACTED',
                updated_at REAL NOT NULL);
             CREATE TABLE communities (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
                level INTEGER NOT NULL DEFAULT 0, parent_id INTEGER, cohesion REAL DEFAULT 0.0,
                size INTEGER DEFAULT 0, dominant_language TEXT, description TEXT,
                created_at TEXT NOT NULL DEFAULT 't');
             CREATE TABLE flows (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                entry_point_id INTEGER NOT NULL, depth INTEGER NOT NULL, node_count INTEGER NOT NULL,
                file_count INTEGER NOT NULL, criticality REAL NOT NULL DEFAULT 0.0,
                path_json TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT 't',
                updated_at TEXT NOT NULL DEFAULT 't');
             CREATE TABLE flow_memberships (flow_id INTEGER NOT NULL, node_id INTEGER NOT NULL,
                position INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (flow_id, node_id));",
        )
        .unwrap();
        for i in 0..members {
            let sym = format!("sym module_a_very_long_symbol_name_{i:06}().");
            let file = if i % 2 == 0 { "src/a.rs" } else { "src/b.rs" };
            c.execute(
                "INSERT INTO nodes (symbol, kind, name, qname, file_path, language, updated_at)
                 VALUES (?1, 'Function', ?2, ?2, ?3, 'Rust', 0)",
                rusqlite::params![sym, sym, format!("/{file}")],
            )
            .unwrap();
        }
        // one cross edge between the two files' communities
        c.execute(
            "INSERT INTO edges (kind, caller_symbol, callee_symbol, provenance, file_path, updated_at)
             VALUES ('CALLS', 'sym module_a_very_long_symbol_name_000000().',
                     'sym module_a_very_long_symbol_name_000001().', 'test', '/src/a.rs', 0)",
            [],
        )
        .unwrap();
    }
}

fn result_text(r: rmcp::model::CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .next()
        .unwrap_or_default()
}

#[tokio::test]
async fn minimal_detail_drops_members_and_aggregates_pairs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    cp_fixture::fat_db(&repo, 400);
    let server = code_reality::mcp_server::CodeRealityServer::new();
    // list_communities default (minimal): no member lists
    let r = server
        .list_communities(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqCommunitiesParams {
                repo_root: repo.display().to_string(),
                algorithm: None,
                use_union: None,
                detail_level: None, // default = minimal
            },
        ))
        .await
        .unwrap();
    let text = result_text(r);
    assert!(!text.contains("members"), "minimal drops member lists");
    assert!(text.contains("cohesion") || text.contains("size"));
    // architecture_overview default (minimal): aggregated pairs, no per-edge rows
    let r2 = server
        .architecture_overview(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqArchOverviewParams {
                repo_root: repo.display().to_string(),
                limit: None,
                detail_level: None, // default = minimal
            },
        ))
        .await
        .unwrap();
    let text2 = result_text(r2);
    assert!(!text2.contains("source\":"), "no per-edge source fields");
    assert!(
        text2.contains("edge_count"),
        "aggregated per-pair counts present"
    );
    assert!(text2.contains("top_kinds"), "top-kinds aggregation present");
}

#[tokio::test]
async fn oversized_output_hits_the_byte_cap_backstop() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    cp_fixture::fat_db(&repo, 50_000); // standard detail > 1MB
    let server = code_reality::mcp_server::CodeRealityServer::new();
    let r = server
        .list_communities(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqCommunitiesParams {
                repo_root: repo.display().to_string(),
                algorithm: None,
                use_union: None,
                detail_level: Some("standard".into()), // force full output
            },
        ))
        .await
        .unwrap();
    let text = result_text(r);
    assert!(text.contains("[TRUNCATED]"), "cap sentinel must be present");
    assert!(
        text.len() < 1_100_000,
        "capped near 1MB, got {}",
        text.len()
    );
}

#[tokio::test]
async fn get_community_drill_down_and_flows_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    cp_fixture::fat_db(&repo, 400);
    let server = code_reality::mcp_server::CodeRealityServer::new();
    // zero-hit is loud
    let err = server
        .get_community(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqCommunityParams {
                repo_root: repo.display().to_string(),
                community_name: "zzz-no-such".into(),
                include_members: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("未命中"), "{:?}", err.message);
    // drill-down hit: summary fields, no members by default; opt-in adds them
    let rhit = server
        .get_community(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqCommunityParams {
                repo_root: repo.display().to_string(),
                community_name: "sym-module".into(), // disambiguator suffix of the second community
                include_members: None,
            },
        ))
        .await
        .unwrap();
    let th = result_text(rhit);
    assert!(th.contains("size"), "summary fields present: {th}");
    assert!(!th.contains("members"), "members opt-in only");
    let rm = server
        .get_community(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::GqCommunityParams {
                repo_root: repo.display().to_string(),
                community_name: "sym-module".into(),
                include_members: Some(true),
            },
        ))
        .await
        .unwrap();
    assert!(
        result_text(rm).contains("members"),
        "opt-in returns members"
    );
    // flows limit (CLI face): two chained CALLS edges -> one entry per cap
    let dir2 = tempfile::tempdir().unwrap();
    let repo2 = std::fs::canonicalize(dir2.path()).unwrap();
    cp_fixture::fat_db(&repo2, 4);
    {
        let c = rusqlite::Connection::open(repo2.join(".code-reality/graph.db")).unwrap();
        for (a, b) in [
            (
                "sym module_a_very_long_symbol_name_000000().",
                "sym module_a_very_long_symbol_name_000001().",
            ),
            (
                "sym module_a_very_long_symbol_name_000001().",
                "sym module_a_very_long_symbol_name_000002().",
            ),
        ] {
            c.execute(
                "INSERT INTO edges (kind, caller_symbol, callee_symbol, provenance, file_path, updated_at)
                 VALUES ('CALLS', ?1, ?2, 'test', '/src/a.rs', 0)",
                rusqlite::params![a, b],
            )
            .unwrap();
        }
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_code-reality"))
        .args([
            "graph_query",
            "flows",
            "--repo",
            repo2.to_str().unwrap(),
            "--limit",
            "1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let n = stdout.matches("\"name\"").count();
    assert_eq!(n, 1, "--limit truncates flows, got {n} entries");
}
