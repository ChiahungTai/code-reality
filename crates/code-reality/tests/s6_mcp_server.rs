//! R6 MCP server tests — tool routing via the lib (the same lib the
//! CLI uses), SM-14 isolation (missing repo loud; panic containment),
//! and a live HTTP smoke on an ephemeral port.

mod graph_db_fixture;

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
    // The bin owns its diagnostic faces: --help answers on stdout with
    // exit 0, unknown args reject loud (exit 2) — the membership-test
    // era of "takes no CLI args" ended with the version-face fix.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_code-reality-mcp"))
        .arg("--help")
        .env("CR_REPO", "/nonexistent")
        .output()
        .expect("spawn code-reality-mcp bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--stdio"), "help face: {stdout}");
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
            "build",
            "callers",
            "closure",
            "delta_tour",
            "detect_changes",
            "document_symbols",
            "get_community",
            "get_minimal_context",
            "get_review_context",
            "hub_nodes",
            "impact_radius",
            "list_communities",
            "list_flows",
            "project",
            "refs",
            "semantic_search",
            "snapshot"
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
    // a graph.db must fail loud (per-request error, client alive)
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
    assert_eq!(tools2.len(), 21);
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

// ---------- data-plane family (EP ep-mcp-data-plane-tools) ----------

fn dp_git(repo: &std::path::Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?} failed");
}

fn dp_ok_path(text: &str, marker: &str) -> String {
    text.lines()
        .find(|l| l.starts_with(marker))
        .and_then(|l| l.split("-> ").last())
        .unwrap()
        .trim()
        .to_string()
}

#[tokio::test]
async fn data_plane_tools_route_loud_errors() {
    let server = code_reality::mcp_server::CodeRealityServer::new();
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let rr = repo.display().to_string();

    // SM-2: build on a repo without any source face
    let err = server
        .build(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::BuildParams {
                repo_root: rr.clone(),
                producer: None,
                json: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("找不到 .py 或 .rs"), "{:?}", err.message);

    // SM-3: producer validation surfaces as INVALID_PARAMS before any lib
    // call — pinned on the code, not just the message text (the lib's own
    // exit-2 arm produces a near-identical message; only the code
    // distinguishes the pre-validation route)
    let err = server
        .build(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::BuildParams {
                repo_root: rr.clone(),
                producer: Some("go".into()),
                json: None,
            },
        ))
        .await
        .unwrap_err();
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "{:?}",
        err.message
    );
    assert!(err.message.contains("rust 或 python"), "{:?}", err.message);

    // SM-6: snapshot without a graph.db
    let err = server
        .snapshot(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::SnapshotParams {
                repo_root: rr.clone(),
                label: None,
                out_dir: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("graph.db"), "{:?}", err.message);

    // SM-8: delta_tour with missing snapshot files
    let err = server
        .delta_tour(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::DeltaTourParams {
                repo_root: rr.clone(),
                snapshot_a: "/nonexistent-a.json".into(),
                snapshot_b: "/nonexistent-b.json".into(),
                ep: None,
                task: None,
                out_dir: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("讀取失敗"), "{:?}", err.message);

    // SM-11: project without an index slot
    let err = server
        .project(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::ProjectParams {
                repo_root: rr.clone(),
                plan: "/nonexistent-plan.toml".into(),
                json: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("真實 index 不存在"),
        "{:?}",
        err.message
    );

    // SM-10: with an index placeholder in place, the invalid plan path is
    // the error (project_repo checks the real index BEFORE the plan)
    std::fs::create_dir_all(repo.join(".code-reality/scip")).unwrap();
    std::fs::write(repo.join(".code-reality/scip/index.scip"), b"placeholder").unwrap();
    let err = server
        .project(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::ProjectParams {
                repo_root: rr,
                plan: "/nonexistent-plan.toml".into(),
                json: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("plan") && err.message.contains("無效"),
        "{:?}",
        err.message
    );
}

#[tokio::test]
async fn mcp_snapshot_and_delta_tour_end_to_end() {
    let server = code_reality::mcp_server::CodeRealityServer::new();
    let tmp = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(tmp.path()).unwrap();
    let rr = repo.display().to_string();

    std::fs::create_dir_all(repo.join("pkg")).unwrap();
    std::fs::write(
        repo.join(".code-reality.toml"),
        "[[module]]\nprefix = \"pkg/\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("pkg/mod.py"), "# header\ndef keep():\n    pass\n").unwrap();
    dp_git(&repo, &["init", "-q"]);
    dp_git(&repo, &["add", "."]);
    dp_git(&repo, &["commit", "-qm", "base"]);

    let db = repo.join(".code-reality/graph.db");
    let mk_db = |edges: usize| {
        // make_graph_db CREATEs tables — drop the previous db between the
        // two graph states (simulated rebuilds)
        let _ = std::fs::remove_file(&db);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let mut spec = graph_db_fixture::GraphDbSpec::default();
        spec.metadata
            .push(("git_head_sha".into(), "deadbeefdeadbeef".into()));
        for i in 0..edges {
            spec.edges.push((
                "CALLS".into(),
                graph_db_fixture::qualified(&repo, "pkg/mod.py", "keep"),
                graph_db_fixture::qualified(&repo, &format!("pkg/other{i}.py"), "fn_b"),
            ));
        }
        graph_db_fixture::make_graph_db(&db, &spec).unwrap();
    };
    mk_db(1);

    // before snapshot (MCP write face); the fixture db's pinned sha
    // (deadbeef) != real HEAD rides a stale WARN at exit 0
    let r1 = server
        .snapshot(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::SnapshotParams {
                repo_root: rr.clone(),
                label: Some("before".into()),
                out_dir: None,
            },
        ))
        .await
        .unwrap();
    let t1 = result_text(r1);
    assert!(t1.contains("[WARN] graph stale"), "stale warn expected: {t1}");
    assert!(t1.contains("[OK] snapshot: 2 files"), "{t1}");
    let path1 = dp_ok_path(&t1, "[OK] snapshot");
    assert!(std::path::Path::new(&path1).is_file(), "written: {path1}");

    // second commit (source change anchors the git hunks) + db gains an edge
    std::fs::write(repo.join("pkg/mod.py"), "# header\ndef keep():\n    return 42\n").unwrap();
    dp_git(&repo, &["add", "."]);
    dp_git(&repo, &["commit", "-qm", "change"]);
    mk_db(2);

    let r2 = server
        .snapshot(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::SnapshotParams {
                repo_root: rr.clone(),
                label: Some("after".into()),
                out_dir: None,
            },
        ))
        .await
        .unwrap();
    let t2 = result_text(r2);
    assert!(t2.contains("[OK] snapshot: 3 files"), "{t2}");
    let path2 = dp_ok_path(&t2, "[OK] snapshot");
    assert_ne!(path1, path2, "sha8-suffixed snapshot names differ");

    // delta tour via MCP: omitted out_dir defaults in-repo (<repo>/.tours/delta)
    let rt = server
        .delta_tour(rmcp::handler::server::wrapper::Parameters(
            code_reality::mcp_server::DeltaTourParams {
                repo_root: rr,
                snapshot_a: path1,
                snapshot_b: path2,
                ep: None,
                task: Some("mcp-e2e".into()),
                out_dir: None,
            },
        ))
        .await
        .unwrap();
    let tt = result_text(rt);
    assert!(tt.contains("[OK] delta tour:"), "{tt}");
    assert!(tt.contains(".tours/delta/"), "in-repo default out_dir: {tt}");
    let tour_path = dp_ok_path(&tt, "[OK] delta tour");
    // the diff is real (an added edge + a changed file): steps must be >= 1
    let steps: usize = tt
        .lines()
        .find(|l| l.starts_with("[OK] delta tour"))
        .and_then(|l| l.split(": ").nth(1))
        .and_then(|s| s.split(' ').next())
        .and_then(|s| s.parse().ok())
        .expect("step count in [OK] line");
    assert!(steps >= 1, "expected non-degenerate tour, got {steps} steps: {tt}");
    assert!(
        std::path::Path::new(&tour_path).is_file(),
        "tour written: {tour_path}"
    );
    assert!(
        tour_path.ends_with("-mcp-e2e.tour"),
        "task name in file: {tour_path}"
    );
}
