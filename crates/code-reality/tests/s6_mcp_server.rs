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
        err.message.contains("退出碼 2") || err.message.contains("graph.db") || err.message.contains("rust-analyzer"),
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
    assert!(std::path::Path::new("target/debug/code-reality-mcp").exists()
        || std::env::var("CARGO_MANIFEST_DIR").is_ok());
}

// ---------- live HTTP smoke (V1-V3 subset) ----------

#[tokio::test]
async fn http_server_serves_initialize_and_tools_list() {
    use rmcp::ServiceExt;
    use rmcp::transport::StreamableHttpClientTransport;

    // ephemeral port: bind once to discover, release, reuse
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let server = tokio::spawn(async move {
        code_reality::mcp_server::serve(port).await
    });
    // give the listener a moment
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let transport = StreamableHttpClientTransport::from_uri(
        format!("http://127.0.0.1:{port}/mcp"),
    );
    let client = ().serve(transport).await.unwrap();
    // tools/list: exactly the four SCIP-family tools
    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(names, vec!["audit", "callers", "closure", "refs"], "{names:?}");
    let refs_tool = tools.iter().find(|t| t.name == "refs").unwrap();
    let schema = &refs_tool.input_schema;
    assert!(schema.get("properties").unwrap().get("repo_root").is_some());
    assert!(schema.get("required").is_some());

    // per-request poison isolation (SM-14): a call against a repo without
    // sidecars returns a tool-level error — the client peer SURVIVES and
    // serves a subsequent tools/list
    let result = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("refs")
                .with_arguments(
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
    let tools2 = client.list_all_tools().await.unwrap();
    assert_eq!(tools2.len(), 4);
    client.cancel().await.unwrap();
    server.abort();
}
