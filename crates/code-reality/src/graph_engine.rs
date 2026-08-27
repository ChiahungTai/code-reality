//! Graph-engine substrate for the v1+ engine parity ops
//! (`ep-v1plus-engine-parity.md` S1): read-only loaders over the
//! self-owned `.code-reality/graph.db` (v1+ S4 flip). Ordering contract:
//! the CRG-era `get_all_nodes` / `get_all_edges` / `load_flow_adjacency`
//! parity relied on natural rowid (insertion) order — loaders here keep
//! the same reliance, now over the graph_db build/import insertion order.

use crate::common::connect_ro;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Projection of `nodes` rows the engine ops consume. `symbol` is the
/// join key (producer native string — legacy-imported nodes mint
/// qname-keyed symbols, so layer-2 fixtures reconcile exactly);
/// `qualified_name` is the display column (never a key). `extra` is the
/// parsed JSON column (decorators etc.).
#[derive(Debug, Clone)]
pub struct GraphNodeLite {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub symbol: String,
    pub file_path: String,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub language: Option<String>,
    pub parent_name: Option<String>,
    pub is_test: bool,
    pub community_id: Option<i64>,
    pub extra: Value,
}

/// Edge projection: (kind, caller, callee) — the only fields the engine
/// ops read; endpoints are node symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdgeLite {
    pub kind: String,
    pub caller_symbol: String,
    pub callee_symbol: String,
}

/// Open the repo's self-owned graph.db read-only (missing db is a loud
/// failure with a build hint — the caller surfaces it as env-level exit 2).
pub fn open(repo_root: &Path) -> Result<Connection, String> {
    let db = crate::graph_db::db_path(repo_root);
    if !db.exists() {
        return Err(format!(
            "graph.db 不在：{}——先跑 `code-reality graph_db build --repo <repo>`（舊庫在場再加 `graph_db import_legacy`）",
            db.display()
        ));
    }
    connect_ro(&db)
}

/// All nodes, File nodes excluded unless asked; natural rowid order.
pub fn load_nodes(conn: &Connection, exclude_files: bool) -> Result<Vec<GraphNodeLite>, String> {
    let sql = if exclude_files {
        "SELECT id, kind, name, qname, symbol, file_path, line_start, line_end, \
         language, parent_name, is_test, community_id, extra FROM nodes \
         WHERE kind != 'File'"
    } else {
        "SELECT id, kind, name, qname, symbol, file_path, line_start, line_end, \
         language, parent_name, is_test, community_id, extra FROM nodes"
    };
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("nodes 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map([], row_to_node)
        .map_err(|e| format!("nodes 查詢失敗：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("nodes 讀取失敗：{e}"))
}

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphNodeLite> {
    let extra_text: String = row.get(12)?;
    Ok(GraphNodeLite {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        qualified_name: row.get(3)?,
        symbol: row.get(4)?,
        file_path: row.get(5)?,
        line_start: row.get(6)?,
        line_end: row.get(7)?,
        language: row.get(8)?,
        parent_name: row.get(9)?,
        is_test: row.get::<_, i64>(10)? != 0,
        community_id: row.get(11)?,
        extra: serde_json::from_str(&extra_text).unwrap_or(Value::Null),
    })
}

/// All edges in rowid order.
pub fn load_edges(conn: &Connection) -> Result<Vec<GraphEdgeLite>, String> {
    let mut stmt = conn
        .prepare("SELECT kind, caller_symbol, callee_symbol FROM edges")
        .map_err(|e| format!("edges 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(GraphEdgeLite {
                kind: r.get(0)?,
                caller_symbol: r.get(1)?,
                callee_symbol: r.get(2)?,
            })
        })
        .map_err(|e| format!("edges 查詢失敗：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("edges 讀取失敗：{e}"))
}

/// ALL nodes (File included) keyed by symbol/id; CALLS edges append into
/// `calls_out` (duplicates and order preserved — BFS dedup happens at
/// traversal); TESTED_BY records its *source* (the tested production
/// node, #515).
pub struct FlowAdjacency {
    pub calls_out: HashMap<String, Vec<String>>,
    pub has_tested_by: HashSet<String>,
    pub nodes_by_key: HashMap<String, GraphNodeLite>,
    pub nodes_by_id: HashMap<i64, GraphNodeLite>,
}

pub fn load_flow_adjacency(conn: &Connection) -> Result<FlowAdjacency, String> {
    let nodes = load_nodes(conn, false)?;
    let mut adj = FlowAdjacency {
        calls_out: HashMap::new(),
        has_tested_by: HashSet::new(),
        nodes_by_key: HashMap::with_capacity(nodes.len()),
        nodes_by_id: HashMap::with_capacity(nodes.len()),
    };
    for n in nodes {
        adj.nodes_by_id.insert(n.id, n.clone());
        adj.nodes_by_key.insert(n.symbol.clone(), n);
    }
    let mut stmt = conn
        .prepare(
            "SELECT kind, caller_symbol, callee_symbol FROM edges \
             WHERE kind IN ('CALLS', 'TESTED_BY')",
        )
        .map_err(|e| format!("flow edges 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("flow edges 查詢失敗：{e}"))?;
    for (kind, src, tgt) in rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("flow edges 讀取失敗：{e}"))?
    {
        if kind == "CALLS" {
            adj.calls_out.entry(src).or_default().push(tgt);
        } else {
            adj.has_tested_by.insert(src);
        }
    }
    Ok(adj)
}

/// Python `round(x, n)` parity: decimal-correct rounding (half-even on
/// the true binary value). `(x*10^n).round()/10^n` differs at exact
/// .5 boundaries (48/10359 NT flow criticalities were off by 1e-4).
pub fn py_round(x: f64, places: usize) -> f64 {
    format!("{:.1$}", x, places).parse().unwrap_or(x)
}

// ---------- shared sanitization (graph.py _sanitize_name) ----------

/// Strip ASCII control characters (keep \t and \n) and truncate to 256 —
/// the CRG prompt-injection guard applied to every name flowing into
/// tool output.
pub fn sanitize_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| *c == '\t' || *c == '\n' || *c >= ' ')
        .collect();
    let mut out = String::new();
    for ch in cleaned.chars().take(256) {
        out.push(ch);
    }
    out
}

// ---------- S2: hub / bridge (analysis.py:14 / analysis.py:60) ----------

/// CRG impact weights (constants.py:56) with default 0.5 — shared by the
/// bridge parallel-edge dedup (strongest kind wins).
fn impact_weight(kind: &str) -> f64 {
    match kind {
        "CALLS" => 1.0,
        "INHERITS" | "OVERRIDES" | "IMPLEMENTS" => 0.9,
        "TESTED_BY" => 0.7,
        "REFERENCES" | "DEPENDS_ON" => 0.6,
        "IMPORTS_FROM" => 0.5,
        "CONTAINS" => 0.3,
        _ => 0.5,
    }
}

/// `find_hub_nodes` (analysis.py:14): degree over ALL edge rows (duplicate
/// rows count), non-File nodes with degree > 0, stable sort by total
/// degree desc (ties keep rowid order), top_n.
pub fn find_hub_nodes(conn: &Connection, top_n: usize) -> Result<Vec<Value>, String> {
    find_hub_nodes_with(conn, &load_edges(conn)?, top_n)
}

/// Edge-injected core (callers pass the full-graph edge slice).
pub fn find_hub_nodes_with(
    conn: &Connection,
    edges: &[GraphEdgeLite],
    top_n: usize,
) -> Result<Vec<Value>, String> {
    let mut in_deg: HashMap<String, i64> = HashMap::new();
    let mut out_deg: HashMap<String, i64> = HashMap::new();
    for e in edges {
        *out_deg.entry(e.caller_symbol.clone()).or_default() += 1;
        *in_deg.entry(e.callee_symbol.clone()).or_default() += 1;
    }
    let mut scored: Vec<Value> = Vec::new();
    for n in load_nodes(conn, true)? {
        let ind = in_deg.get(&n.symbol).copied().unwrap_or(0);
        let outd = out_deg.get(&n.symbol).copied().unwrap_or(0);
        let total = ind + outd;
        if total == 0 {
            continue;
        }
        scored.push(serde_json::json!({
            "name": sanitize_name(&n.name),
            "qualified_name": n.qualified_name,
            "kind": n.kind,
            "file": n.file_path,
            "in_degree": ind,
            "out_degree": outd,
            "total_degree": total,
            "community_id": n.community_id,
        }));
    }
    scored.sort_by(|a, b| b["total_degree"].as_i64().cmp(&a["total_degree"].as_i64()));
    scored.truncate(top_n);
    Ok(scored)
}

/// `find_bridge_nodes` (analysis.py:60): Brandes betweenness on the
/// directed multigraph collapsed to its strongest parallel edge per pair
/// (graph.py `_build_networkx_graph`), normalized by 1/((n-1)(n-2))
/// (networkx digraph convention), score > 0 only, non-File nodes,
/// betweenness rounded to 6 decimals, stable sort desc, top_n.
/// Deviation (recorded): above 5000 nodes networkx samples k=500 sources
/// with its own RNG; we use a fixed-seed LCG sample — exact parity holds
/// only for the unsampled path (statistical parity when sampled).
pub fn find_bridge_nodes(conn: &Connection, top_n: usize) -> Result<Vec<Value>, String> {
    find_bridge_nodes_with(conn, &load_edges(conn)?, top_n)
}

/// Edge-injected core (callers pass the full-graph edge slice).
pub fn find_bridge_nodes_with(
    conn: &Connection,
    edges: &[GraphEdgeLite],
    top_n: usize,
) -> Result<Vec<Value>, String> {
    // strongest parallel edge per (src, tgt) pair
    let mut best: HashMap<(String, String), f64> = HashMap::new();
    for e in edges {
        let w = impact_weight(&e.kind);
        let key = (e.caller_symbol.clone(), e.callee_symbol.clone());
        match best.get(&key) {
            Some(existing) if w <= *existing => {}
            _ => {
                best.insert(key, w);
            }
        }
    }
    // deterministic id assignment: HashMap iteration order is per-process
    // random (std RandomState) — sort the pairs so every run of the same
    // db yields identical ids, tie ordering and fp summation
    let mut pairs: Vec<(&String, &String)> = best.keys().map(|(s, t)| (s, t)).collect();
    pairs.sort();
    let mut node_ids: Vec<String> = Vec::new();
    let mut id_of: HashMap<&str, usize> = HashMap::new();
    let mut adj: Vec<Vec<usize>> = Vec::new();
    for (src, tgt) in &pairs {
        let s = *id_of.entry(src.as_str()).or_insert_with(|| {
            node_ids.push((*src).clone());
            adj.push(Vec::new());
            node_ids.len() - 1
        });
        let t = *id_of.entry(tgt.as_str()).or_insert_with(|| {
            node_ids.push((*tgt).clone());
            adj.push(Vec::new());
            node_ids.len() - 1
        });
        adj[s].push(t);
    }
    let n = node_ids.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    // source set: all nodes, or a fixed-seed k-sample above 5000
    let sources: Vec<usize> = if n > 5000 {
        let k = 500.min(n);
        // deterministic sample: stride through a fixed-seed LCG permutation
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (state >> 33) as usize % (i + 1);
            order.swap(i, j);
        }
        order.truncate(k);
        order
    } else {
        (0..n).collect()
    };
    // Brandes (directed, unweighted)
    let mut between: Vec<f64> = vec![0.0; n];
    for &s in &sources {
        let mut stack: Vec<usize> = Vec::new();
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut sigma = vec![0.0f64; n];
        let mut dist = vec![-1i64; n];
        sigma[s] = 1.0;
        dist[s] = 0;
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            stack.push(v);
            for &w in &adj[v] {
                if dist[w] < 0 {
                    dist[w] = dist[v] + 1;
                    queue.push_back(w);
                }
                if dist[w] == dist[v] + 1 {
                    sigma[w] += sigma[v];
                    preds[w].push(v);
                }
            }
        }
        let mut delta = vec![0.0f64; n];
        while let Some(v) = stack.pop() {
            for &p in &preds[v] {
                delta[p] += sigma[p] / sigma[v] * (1.0 + delta[v]);
            }
            if v != s {
                between[v] += delta[v];
            }
        }
    }
    let norm = ((n as f64 - 1.0) * (n as f64 - 2.0)).max(1.0);
    let nodes_all = load_nodes(conn, false)?;
    let node_by_key: HashMap<&str, &GraphNodeLite> = nodes_all
        .iter()
        .map(|nd| (nd.symbol.as_str(), nd))
        .collect();
    let mut results: Vec<Value> = Vec::new();
    for (i, qn) in node_ids.iter().enumerate() {
        let score = between[i] / norm;
        if score <= 0.0 {
            continue;
        }
        let Some(node) = node_by_key.get(qn.as_str()) else {
            continue;
        };
        if node.kind == "File" {
            continue;
        }
        let rounded = py_round(score, 6);
        results.push(serde_json::json!({
            "name": sanitize_name(&node.name),
            "qualified_name": node.qualified_name,
            "kind": node.kind,
            "file": node.file_path,
            "betweenness": rounded,
            "community_id": node.community_id,
        }));
    }
    results.sort_by(|a, b| {
        b["betweenness"]
            .as_f64()
            .partial_cmp(&a["betweenness"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_n);
    Ok(results)
}

// ---------- S3: flows family (flows.py) ----------

use std::sync::LazyLock;

fn pats(src: &[&str]) -> Vec<regex::Regex> {
    src.iter()
        .map(|p| regex::Regex::new(p).expect("flow pattern"))
        .collect()
}

/// Framework-decorator patterns (flows.py:27-72).
static FRAMEWORK_DECORATOR_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    pats(&[
        r"(?i)router\.(get|post|put|delete|patch|route)",
        r"(?i)blueprint\.(route|before_request|after_request)",
        r"(?i)(before|after)_(request|response)",
        r"(?i)click\.(command|group)",
        r"(?i)\w+\.(command|group)\b",
        r"(?i)(field|model)_(serializer|validator)",
        r"(?i)(celery\.)?(task|shared_task|periodic_task)",
        r"(?i)receiver",
        r"(?i)api_view",
        r"(?i)\baction\b",
        r"pytest\.(fixture|mark)",
        r"(?i)(override_settings|modify_settings)",
        r"(?i)(event\.)?listens_for",
        r"(?i)(Get|Post|Put|Delete|Patch|RequestMapping)Mapping",
        r"(?i)(Scheduled|EventListener|Bean|Configuration)",
        r"(?i)KafkaListener",
        r"(?i)(WorkflowMethod|ActivityMethod)",
        r"(?i)(Component|Injectable|Controller|Module|Guard|Pipe)",
        r"(?i)(Subscribe|Mutation|Query|Resolver)",
        r"(app|router)\.(get|post|put|delete|patch|use|all)\b",
        r"(?i)@(Override|OnLifecycleEvent|Composable)",
        r"(?i)(HiltViewModel|AndroidEntryPoint|Inject)",
        r"(?i)\w+\.(tool|tool_plain|system_prompt|result_validator)\b",
        r"^tool\b",
        r"(?i)\w+\.(middleware|exception_handler|on_exception)\b",
        r"(?i)\w+\.route\b",
    ])
});

/// Conventional entry-name patterns (flows.py:74-114).
static ENTRY_NAME_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    pats(&[
        r"^main$",
        r"^__main__$",
        r"^test_",
        r"^Test[A-Z]",
        r"^on_",
        r"^handle_",
        r"^handler$",
        r"^handle$",
        r"^lambda_handler$",
        r"^upgrade$",
        r"^downgrade$",
        r"^lifespan$",
        r"^get_db$",
        r"^on(Create|Start|Resume|Pause|Stop|Destroy|Bind|Receive)",
        r"^do(Get|Post|Put|Delete)$",
        r"^do_(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)$",
        r"^log_message$",
        r"^(middleware|errorHandler)$",
        r"^ng(OnInit|OnChanges|OnDestroy|DoCheck|AfterContentInit|AfterContentChecked|AfterViewInit|AfterViewChecked)$",
        r"^(transform|writeValue|registerOnChange|registerOnTouched|setDisabledState)$",
        r"^(canActivate|canDeactivate|canActivateChild|canLoad|canMatch|resolve)$",
        r"^(componentDidMount|componentDidUpdate|componentWillUnmount|shouldComponentUpdate|render)$",
    ])
});

/// Language-scoped entry names (flows.py:116-120) — PHP only.
static PHP_ENTRY_PATTERNS: LazyLock<Vec<regex::Regex>> =
    LazyLock::new(|| pats(&[r"^(boot|register)$", r"^__invoke$"]));

static TEST_FILE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"([\\/]__tests__[\\/]|\.spec\.[jt]sx?$|\.test\.[jt]sx?$|[\\/]test_[^/\\]*\.py$)",
    )
    .expect("test-file pattern")
});

/// SECURITY_KEYWORDS (constants.py:32) — substring match on lowered
/// name/qualified_name.
const SECURITY_KEYWORDS: [&str; 25] = [
    "auth",
    "login",
    "password",
    "token",
    "session",
    "crypt",
    "secret",
    "credential",
    "permission",
    "sql",
    "query",
    "execute",
    "connect",
    "socket",
    "request",
    "http",
    "sanitize",
    "validate",
    "encrypt",
    "decrypt",
    "hash",
    "sign",
    "verify",
    "admin",
    "privilege",
];

fn is_test_file(file_path: &str) -> bool {
    TEST_FILE_RE.is_match(file_path)
}

fn has_framework_decorator(node: &GraphNodeLite) -> bool {
    let Some(decs) = node.extra.get("decorators") else {
        return false;
    };
    let strings: Vec<String> = match decs {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => return false,
    };
    if strings.is_empty() {
        return false;
    }
    strings
        .iter()
        .any(|d| FRAMEWORK_DECORATOR_PATTERNS.iter().any(|p| p.is_match(d)))
}

fn matches_entry_name(node: &GraphNodeLite) -> bool {
    if ENTRY_NAME_PATTERNS.iter().any(|p| p.is_match(&node.name)) {
        return true;
    }
    if node.language.as_deref() == Some("php") {
        return PHP_ENTRY_PATTERNS.iter().any(|p| p.is_match(&node.name));
    }
    false
}

const NODE_COLUMNS: &str = "id, kind, name, qname, symbol, file_path, line_start, \
     line_end, language, parent_name, is_test, community_id, extra";

/// `detect_entry_points` (flows.py:164): Function/Test nodes that are true
/// roots (no incoming CALLS from non-File sources), framework-decorated,
/// or conventionally named; tests excluded unless asked.
pub fn detect_entry_points(
    conn: &Connection,
    include_tests: bool,
) -> Result<Vec<GraphNodeLite>, String> {
    let mut called: HashSet<String> = HashSet::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e.callee_symbol FROM edges e \
                 LEFT JOIN nodes n ON n.symbol = e.caller_symbol \
                 WHERE e.kind = 'CALLS' AND (n.kind IS NULL OR n.kind != 'File')",
            )
            .map_err(|e| format!("call targets 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("call targets 查詢失敗：{e}"))?;
        for qn in rows {
            called.insert(qn.map_err(|e| format!("call targets 讀取失敗：{e}"))?);
        }
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {NODE_COLUMNS} FROM nodes WHERE kind IN ('Function', 'Test')"
        ))
        .map_err(|e| format!("candidates 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map([], row_to_node)
        .map_err(|e| format!("candidates 查詢失敗：{e}"))?;
    let mut out = Vec::new();
    for node in rows {
        let node = node.map_err(|e| format!("candidates 讀取失敗：{e}"))?;
        if !include_tests && (node.is_test || is_test_file(&node.file_path)) {
            continue;
        }
        if node.extra.get("verilog_kind").is_some() {
            continue;
        }
        let is_entry = !called.contains(&node.symbol)
            || has_framework_decorator(&node)
            || matches_entry_name(&node);
        if is_entry {
            out.push(node);
        }
    }
    Ok(out)
}

/// `compute_criticality` (flows.py:324): five weighted factors.
fn compute_criticality(path_ids: &[i64], depth: i64, adj: &FlowAdjacency) -> f64 {
    if path_ids.is_empty() {
        return 0.0;
    }
    let nodes: Vec<&GraphNodeLite> = path_ids
        .iter()
        .filter_map(|id| adj.nodes_by_id.get(id))
        .collect();
    if nodes.is_empty() {
        return 0.0;
    }
    // file spread: 1 file -> 0, 5+ files -> 1
    let mut files: HashSet<&str> = HashSet::new();
    for n in &nodes {
        files.insert(n.file_path.as_str());
    }
    let file_spread = if files.len() > 1 {
        ((files.len() - 1) as f64 / 4.0).min(1.0)
    } else {
        0.0
    };
    // external calls: call targets absent from the graph
    let mut external = 0usize;
    for n in &nodes {
        if let Some(targets) = adj.calls_out.get(&n.symbol) {
            for t in targets {
                if !adj.nodes_by_key.contains_key(t) {
                    external += 1;
                }
            }
        }
    }
    let external_score = (external as f64 / 5.0).min(1.0);
    // security sensitivity: node hits any keyword (each node once)
    let security_hits = nodes
        .iter()
        .filter(|n| {
            let name_lower = n.name.to_lowercase();
            let qn_lower = n.qualified_name.to_lowercase();
            SECURITY_KEYWORDS
                .iter()
                .any(|kw| name_lower.contains(kw) || qn_lower.contains(kw))
        })
        .count();
    let security_score = (security_hits as f64 / nodes.len().max(1) as f64).min(1.0);
    // test coverage gap
    let tested = nodes
        .iter()
        .filter(|n| adj.has_tested_by.contains(&n.symbol))
        .count();
    let coverage = tested as f64 / nodes.len().max(1) as f64;
    let test_gap = 1.0 - coverage;
    // depth: 10+ -> 1
    let depth_score = (depth as f64 / 10.0).min(1.0);
    let criticality = file_spread * 0.30
        + external_score * 0.20
        + security_score * 0.25
        + test_gap * 0.15
        + depth_score * 0.10;
    py_round((criticality).clamp(0.0, 1.0), 4)
}

/// `trace_flows` (flows.py:284): forward BFS from every entry point,
/// sorted by criticality desc (stable). Each flow maps to the CRG dict
/// shape (name/entry_point/path/depth/node_count/file_count/files/
/// criticality). `files` uses first-seen order (CRG's set-to-list order
/// is CPython-internal; the parity bar compares multisets, not this
/// list's order).
pub fn trace_flows(
    conn: &Connection,
    max_depth: usize,
    include_tests: bool,
) -> Result<Vec<Value>, String> {
    let entry_points = detect_entry_points(conn, include_tests)?;
    if entry_points.is_empty() {
        return Ok(Vec::new());
    }
    let adj = load_flow_adjacency(conn)?;
    let mut flows: Vec<(f64, Value)> = Vec::new();
    for ep in &entry_points {
        // BFS (flows.py:222)
        let mut path_ids: Vec<i64> = vec![ep.id];
        let mut path_keys: Vec<String> = vec![ep.symbol.clone()];
        let mut visited: HashSet<String> = HashSet::from([ep.symbol.clone()]);
        let mut queue: std::collections::VecDeque<(String, usize)> =
            std::collections::VecDeque::from([(ep.symbol.clone(), 0)]);
        let mut actual_depth: i64 = 0;
        while let Some((current, depth)) = queue.pop_front() {
            if depth as i64 > actual_depth {
                actual_depth = depth as i64;
            }
            if depth >= max_depth {
                continue;
            }
            if let Some(targets) = adj.calls_out.get(&current) {
                for target in targets {
                    if visited.contains(target) {
                        continue;
                    }
                    let Some(node) = adj.nodes_by_key.get(target) else {
                        continue;
                    };
                    visited.insert(target.clone());
                    path_ids.push(node.id);
                    path_keys.push(target.clone());
                    queue.push_back((target.clone(), depth + 1));
                }
            }
        }
        if path_ids.len() < 2 {
            continue;
        }
        let mut files: Vec<String> = Vec::new();
        for key in &path_keys {
            if let Some(n) = adj.nodes_by_key.get(key) {
                if !files.contains(&n.file_path) {
                    files.push(n.file_path.clone());
                }
            }
        }
        let file_count = files.len();
        let criticality = compute_criticality(&path_ids, actual_depth, &adj);
        flows.push((
            criticality,
            serde_json::json!({
                "name": sanitize_name(&ep.name),
                "entry_point": ep.qualified_name,
                "entry_point_id": ep.id,
                "path": path_ids,
                "depth": actual_depth,
                "node_count": path_ids.len(),
                "file_count": file_count,
                "files": files,
                "criticality": criticality,
            }),
        ));
    }
    flows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(flows.into_iter().map(|(_, v)| v).collect())
}

/// `get_affected_flows` (flows.py:674), computed fresh: flows whose node
/// files intersect the changed set.
pub fn affected_flows(
    conn: &Connection,
    changed_files: &[String],
    max_depth: usize,
    include_tests: bool,
) -> Result<Vec<Value>, String> {
    if changed_files.is_empty() {
        return Ok(Vec::new());
    }
    let flows = trace_flows(conn, max_depth, include_tests)?;
    let adj = load_flow_adjacency(conn)?;
    let changed: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let mut affected = Vec::new();
    for flow in flows {
        let hit = flow["path"]
            .as_array()
            .map(|ids| {
                ids.iter().any(|id| {
                    adj.nodes_by_id
                        .get(&id.as_i64().unwrap_or(i64::MIN))
                        .map(|n| changed.contains(n.file_path.as_str()))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if hit {
            affected.push(flow);
        }
    }
    Ok(affected)
}

// ---------- S4: impact_radius (graph.py:771 get_impact_radius_sql) ----------

const IMPACT_DEPTH_DECAY: f64 = 0.6;
const IMPACT_SCORE_FLOOR: f64 = 0.05;

/// CRG `node_to_dict` shape (graph.py) — sanitized name fields.
fn node_dict(n: &GraphNodeLite) -> Value {
    serde_json::json!({
        "id": n.id,
        "kind": n.kind,
        "name": sanitize_name(&n.name),
        "qualified_name": sanitize_name(&n.qualified_name),
        "file_path": n.file_path,
        "line_start": n.line_start,
        "line_end": n.line_end,
        "language": n.language,
        "parent_name": n.parent_name.as_ref().map(|p| sanitize_name(p)),
        "is_test": n.is_test,
    })
}

/// `impact_radius` (graph.py:771): bounded best-score relaxation from the
/// nodes of `changed_files`, both edge directions, per-kind weights with
/// 0.5 default, 0.6 per-depth decay, 0.05 floor, seeded at 1.0; final
/// selection excludes seeds and verilog nodes, orders by (score desc,
/// qname asc) with a max_nodes+1 sentinel for truncation.
pub fn impact_radius(
    conn: &Connection,
    changed_files: &[String],
    max_depth: usize,
    max_nodes: usize,
) -> Result<Value, String> {
    impact_radius_with(
        conn,
        &load_edges(conn)?,
        changed_files,
        max_depth,
        max_nodes,
    )
}

/// Edge-injected core (callers pass the full-graph edge slice).
pub fn impact_radius_with(
    conn: &Connection,
    edges: &[GraphEdgeLite],
    changed_files: &[String],
    max_depth: usize,
    max_nodes: usize,
) -> Result<Value, String> {
    let empty = serde_json::json!({
        "changed_nodes": [], "impacted_nodes": [], "impacted_files": [],
        "edges": [], "truncated": false, "total_impacted": 0,
        "impact_scores": {},
    });
    if changed_files.is_empty() {
        return Ok(empty);
    }
    // seeds: every node whose file_path is changed (File nodes included —
    // get_nodes_by_file has no kind filter)
    let nodes_all = load_nodes(conn, false)?;
    let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let seed_nodes: Vec<&GraphNodeLite> = nodes_all
        .iter()
        .filter(|n| changed_set.contains(n.file_path.as_str()))
        .collect();
    if seed_nodes.is_empty() {
        return Ok(empty);
    }
    let seeds: HashSet<String> = seed_nodes.iter().map(|n| n.symbol.clone()).collect();

    // bidirectional weighted adjacency over all edge kinds (injected)
    let mut adj_out: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let mut adj_in: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for e in edges {
        let w = impact_weight(&e.kind);
        adj_out
            .entry(e.caller_symbol.clone())
            .or_default()
            .push((e.callee_symbol.clone(), w));
        adj_in
            .entry(e.callee_symbol.clone())
            .or_default()
            .push((e.caller_symbol.clone(), w));
    }

    let mut best: HashMap<String, f64> = HashMap::new();
    let mut frontier: HashMap<String, f64> = HashMap::new();
    for qn in &seeds {
        best.insert(qn.clone(), 1.0);
        frontier.insert(qn.clone(), 1.0);
    }
    for _ in 0..max_depth {
        let mut next: HashMap<String, f64> = HashMap::new();
        for (qn, fscore) in &frontier {
            let expand = |dir: &HashMap<String, Vec<(String, f64)>>,
                          next: &mut HashMap<String, f64>| {
                if let Some(neigh) = dir.get(qn) {
                    for (other, w) in neigh {
                        let cand = fscore * w * IMPACT_DEPTH_DECAY;
                        if cand > IMPACT_SCORE_FLOOR {
                            let slot = next.entry(other.clone()).or_insert(0.0);
                            if cand > *slot {
                                *slot = cand;
                            }
                        }
                    }
                }
            };
            expand(&adj_out, &mut next);
            expand(&adj_in, &mut next);
        }
        // drop candidates that don't beat the known best
        next.retain(|k, v| *v > best.get(k).copied().unwrap_or(0.0));
        if next.is_empty() {
            break;
        }
        for (k, v) in &next {
            best.insert(k.clone(), *v);
        }
        frontier = next;
    }

    // final selection: join nodes (canonical), drop seeds + verilog,
    // order (score desc, symbol asc), cap with sentinel
    let by_key: HashMap<&str, &GraphNodeLite> =
        nodes_all.iter().map(|n| (n.symbol.as_str(), n)).collect();
    let mut rows: Vec<(String, f64)> = best
        .into_iter()
        .filter(|(qn, _)| !seeds.contains(qn))
        .filter(|(qn, _)| by_key.contains_key(qn.as_str()))
        .filter(|(qn, _)| by_key[qn.as_str()].extra.get("verilog_kind").is_none())
        .collect();
    rows.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let total_impacted = rows.len();
    let truncated = total_impacted > max_nodes;
    rows.truncate(max_nodes);

    let score_by_qn: HashMap<&str, f64> = rows.iter().map(|(qn, s)| (qn.as_str(), *s)).collect();
    let mut impacted_nodes: Vec<Value> = rows
        .iter()
        .map(|(qn, _)| node_dict(by_key[qn.as_str()]))
        .collect();
    let mut impacted_files: Vec<String> = Vec::new();
    for (qn, _) in &rows {
        let f = &by_key[qn.as_str()].file_path;
        if !impacted_files.contains(f) {
            impacted_files.push(f.clone());
        }
    }
    let mut impact_scores = serde_json::Map::new();
    for (qn, s) in &rows {
        impact_scores.insert(qn.clone(), Value::from(py_round(*s, 4)));
    }

    // edges among seeds + impacted (batched IN, CRG get_edges_among)
    let mut all_qns: Vec<String> = seeds.iter().cloned().collect();
    for (qn, _) in &rows {
        all_qns.push(qn.clone());
    }
    let mut relevant: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, caller_symbol, callee_symbol, file_path, \
                 line, confidence, confidence_tier FROM edges \
                 WHERE caller_symbol IN (SELECT value FROM json_each(?1))",
            )
            .map_err(|e| format!("edges among 查詢失敗：{e}"))?;
        let arr = Value::Array(
            all_qns
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        );
        let rows_out = stmt
            .query_map([arr.to_string()], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, f64>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| format!("edges among 查詢失敗：{e}"))?;
        for row in rows_out {
            let (id, kind, src, tgt, fp, line, conf, tier) =
                row.map_err(|e| format!("edges among 讀取失敗：{e}"))?;
            if !all_qns.contains(&tgt) {
                continue;
            }
            relevant.push(serde_json::json!({
                "id": id, "kind": kind,
                "source": sanitize_name(&src), "target": sanitize_name(&tgt),
                "file_path": fp, "line": line,
                "confidence": conf, "confidence_tier": tier,
            }));
        }
    }
    impacted_nodes.sort_by(|a, b| {
        let sa = score_by_qn
            .get(a["qualified_name"].as_str().unwrap_or(""))
            .copied()
            .unwrap_or(0.0);
        let sb = score_by_qn
            .get(b["qualified_name"].as_str().unwrap_or(""))
            .copied()
            .unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a["qualified_name"]
                    .as_str()
                    .cmp(&b["qualified_name"].as_str())
            })
    });

    let changed_nodes: Vec<Value> = seed_nodes.iter().map(|n| node_dict(n)).collect();
    Ok(serde_json::json!({
        "changed_nodes": changed_nodes,
        "impacted_nodes": impacted_nodes,
        "impacted_files": impacted_files,
        "edges": relevant,
        "truncated": truncated,
        "total_impacted": total_impacted,
        "impact_scores": impact_scores,
    }))
}

// ---------- S5: communities Tier 0 + architecture_overview ----------
// communities.py — igraph is never present in the base install, so the
// live CRG path is `_detect_file_based` (directory grouping) with
// `_split_oversized` a no-op and `_dedupe_community_names` still active.

const SLUG_MAX_LEN: usize = 30;
const COMMON_WORDS: [&str; 50] = [
    "get", "set", "self", "init", "new", "create", "update", "delete", "add", "remove", "make",
    "build", "from", "to", "for", "with", "the", "and", "test", "main", "run", "do", "is", "has",
    "on", "of", "in", "at", "by", "my", "this", "that", "all", "none", "should", "when", "then",
    "given", "return", "returns", "raise", "raises", "expect", "expected", "assert", "tests", "be",
    "it", "if", "not",
];

/// Counter.most_common parity: count desc, ties keep first-seen order.
struct OrderedCounter {
    counts: Vec<(String, usize)>,
    index: HashMap<String, usize>,
}

impl OrderedCounter {
    fn new() -> Self {
        Self {
            counts: Vec::new(),
            index: HashMap::new(),
        }
    }
    fn bump(&mut self, key: &str) {
        match self.index.get(key) {
            Some(&i) => self.counts[i].1 += 1,
            None => {
                self.index.insert(key.to_string(), self.counts.len());
                self.counts.push((key.to_string(), 1));
            }
        }
    }
    fn most_common(&self, n: usize) -> Vec<String> {
        let mut idx: Vec<usize> = (0..self.counts.len()).collect();
        idx.sort_by(|&a, &b| self.counts[b].1.cmp(&self.counts[a].1));
        idx.truncate(n);
        idx.into_iter().map(|i| self.counts[i].0.clone()).collect()
    }
}

/// `_split_name` (communities.py:168): camelCase boundary then split on
/// `[_\-.\s]+`.
fn split_name(name: &str) -> Vec<String> {
    let mut s = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();
    for i in 0..chars.len() {
        if i > 0 && chars[i - 1].is_ascii_lowercase() && chars[i].is_ascii_uppercase() {
            s.push('_');
        }
        s.push(chars[i]);
    }
    s.split(|c: char| c == '_' || c == '-' || c == '.' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// `_to_slug` (communities.py:175).
fn to_slug(s: &str) -> String {
    let normalized: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<String> = split_name(&normalized)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect();
    let slug = words.join("-");
    if slug.chars().count() <= SLUG_MAX_LEN {
        return slug;
    }
    // last '-' within the first 31 chars, else hard cut at 30
    let chars: Vec<char> = slug.chars().collect();
    let mut boundary = None;
    for (i, c) in chars.iter().enumerate().take(SLUG_MAX_LEN + 1) {
        if *c == '-' {
            boundary = Some(i);
        }
    }
    match boundary {
        Some(b) if b > 0 => chars[..b].iter().collect(),
        _ => chars[..SLUG_MAX_LEN].iter().collect(),
    }
}

fn is_common_word(w: &str) -> bool {
    COMMON_WORDS.contains(&w)
}

/// `_extract_file_prefix` (communities.py:127): most common parent-dir
/// component (or file stem).
fn extract_file_prefix(file_paths: &[&str]) -> String {
    if file_paths.is_empty() {
        return String::new();
    }
    let mut counter = OrderedCounter::new();
    for fp in file_paths {
        let normalized = fp.replace('\\', "/");
        let segments: Vec<&str> = normalized.split('/').collect();
        let part = if segments.len() >= 2 {
            segments[segments.len() - 2].to_string()
        } else {
            let last = segments[segments.len() - 1];
            last.rsplit_once('.')
                .map(|(stem, _)| stem.to_string())
                .unwrap_or_else(|| last.to_string())
        };
        counter.bump(&part);
    }
    to_slug(&counter.most_common(1)[0])
}

/// `_extract_keywords` (communities.py:145).
fn extract_keywords(members: &[&GraphNodeLite]) -> Vec<String> {
    let mut counter = OrderedCounter::new();
    for m in members {
        if matches!(m.kind.as_str(), "Function" | "Class" | "Test" | "Type") {
            for w in split_name(&m.name) {
                let wl = w.to_lowercase();
                if !is_common_word(&wl) && wl.chars().count() > 1 {
                    counter.bump(&wl);
                }
            }
        }
    }
    if counter.counts.is_empty() {
        return Vec::new();
    }
    counter.most_common(5)
}

fn is_test_node(n: &GraphNodeLite) -> bool {
    n.kind == "Test" || n.is_test
}

/// `_generate_community_name` (communities.py:79).
fn generate_community_name(members: &[&GraphNodeLite]) -> String {
    if members.is_empty() {
        return "empty".into();
    }
    let production: Vec<&&GraphNodeLite> = members.iter().filter(|m| !is_test_node(m)).collect();
    let naming: Vec<&GraphNodeLite> = if production.is_empty() {
        members.to_vec()
    } else {
        production.into_iter().copied().collect()
    };
    let prefix = extract_file_prefix(
        &naming
            .iter()
            .map(|m| m.file_path.as_str())
            .collect::<Vec<_>>(),
    );
    // dominant class (>40%)
    let mut class_counter = OrderedCounter::new();
    for m in &naming {
        if m.kind == "Class" {
            class_counter.bump(&m.name);
        }
    }
    if !class_counter.counts.is_empty() {
        let top = &class_counter.most_common(1)[0];
        let top_count = class_counter.counts[class_counter.index[top]].1;
        if top_count > naming.len() * 40 / 100 {
            let slug = to_slug(top);
            return if prefix.is_empty() {
                slug
            } else {
                format!("{prefix}-{slug}")
            };
        }
    }
    let keywords = extract_keywords(&naming);
    let keyword = keywords.first().cloned().unwrap_or_default();
    if !prefix.is_empty() && !keyword.is_empty() {
        return format!("{prefix}-{keyword}");
    }
    if !prefix.is_empty() {
        return prefix;
    }
    if !keyword.is_empty() {
        return keyword;
    }
    "cluster".into()
}

/// `_compute_cohesion_batch` (communities.py:187): internal/(internal+
/// external), external counted at both endpoints.
fn compute_cohesion_batch(
    member_sets: &[HashSet<String>],
    all_edges: &[GraphEdgeLite],
) -> Vec<f64> {
    let mut qn_to_idx: HashMap<&str, usize> = HashMap::new();
    for (idx, members) in member_sets.iter().enumerate() {
        for qn in members {
            qn_to_idx.insert(qn.as_str(), idx);
        }
    }
    let n = member_sets.len();
    let mut internal = vec![0usize; n];
    let mut external = vec![0usize; n];
    for e in all_edges {
        let sc = qn_to_idx.get(e.caller_symbol.as_str());
        let tc = qn_to_idx.get(e.callee_symbol.as_str());
        match (sc, tc) {
            (None, None) => {}
            (Some(&s), Some(&t)) if s == t => internal[s] += 1,
            (s, t) => {
                if let Some(&s) = s {
                    external[s] += 1;
                }
                if let Some(&t) = t {
                    external[t] += 1;
                }
            }
        }
    }
    (0..n)
        .map(|i| {
            let total = internal[i] + external[i];
            if total > 0 {
                py_round(internal[i] as f64 / total as f64, 4)
            } else {
                0.0
            }
        })
        .collect()
}

/// `detect_communities` Tier 0 (communities.py:798 → `_detect_file_based`
/// :474 + `_dedupe_community_names` :732). Returns CRG-shaped community
/// dicts (name/level/size/cohesion/dominant_language/description/members).
pub fn detect_communities(conn: &Connection, min_size: usize) -> Result<Vec<Value>, String> {
    detect_communities_with(conn, &load_edges(conn)?, min_size)
}

/// Edge-injected core (callers pass the full-graph edge slice).
pub fn detect_communities_with(
    conn: &Connection,
    edges: &[GraphEdgeLite],
    min_size: usize,
) -> Result<Vec<Value>, String> {
    let all_edges = edges;
    let nodes = load_nodes(conn, true)?;
    // -- directory grouping (adaptive depth, communities.py:474) --
    let dir_parts: Vec<Vec<String>> = nodes
        .iter()
        .map(|n| {
            n.file_path
                .replace('\\', "/")
                .split('/')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .split_last()
                .map(|(_, dirs)| dirs.to_vec())
                .unwrap_or_default()
        })
        .collect();
    // longest common prefix over dir parts
    let mut prefix_len = 0usize;
    if !dir_parts.is_empty() {
        let shortest = dir_parts.iter().map(|p| p.len()).min().unwrap();
        'outer: for i in 0..shortest {
            let seg = &dir_parts[0][i];
            for p in &dir_parts {
                if &p[i] != seg {
                    break 'outer;
                }
            }
            prefix_len = i + 1;
        }
    }
    let group_at = |depth: usize| -> Vec<(String, Vec<usize>)> {
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, parts) in dir_parts.iter().enumerate() {
            let remainder = &parts[prefix_len.min(parts.len())..];
            let key = if !remainder.is_empty() {
                remainder[..depth.min(remainder.len())].join("/")
            } else {
                // file at the prefix itself: file stem fallback
                let segs: Vec<&str> = nodes[i].file_path.split('/').collect();
                segs.last()
                    .map(|f| {
                        f.rsplit_once('.')
                            .map(|(stem, _)| stem.to_string())
                            .unwrap_or_else(|| f.to_string())
                    })
                    .unwrap_or_else(|| "root".into())
            };
            if !groups.contains_key(&key) {
                order.push(key.clone());
            }
            groups.entry(key).or_default().push(i);
        }
        order
            .into_iter()
            .map(|k| {
                let v = groups.remove(&k).unwrap();
                (k, v)
            })
            .collect()
    };
    let max_depth = dir_parts
        .iter()
        .map(|p| p.len().saturating_sub(prefix_len))
        .max()
        .unwrap_or(0);
    let mut best_groups = group_at(1);
    for depth in 1..=max_depth {
        let groups = group_at(depth);
        let qualifying = groups.iter().filter(|(_, v)| v.len() >= min_size).count();
        best_groups = groups;
        if qualifying >= 10 {
            break;
        }
    }
    // min_size filter + batch cohesion
    let mut pending: Vec<(String, Vec<usize>)> = best_groups
        .into_iter()
        .filter(|(_, v)| v.len() >= min_size)
        .collect();
    let member_sets: Vec<HashSet<String>> = pending
        .iter()
        .map(|(_, idxs)| idxs.iter().map(|&i| nodes[i].symbol.clone()).collect())
        .collect();
    let cohesions = compute_cohesion_batch(&member_sets, all_edges);
    let mut communities: Vec<Value> = pending
        .iter_mut()
        .enumerate()
        .map(|(ci, (dir_path, idxs))| {
            let members: Vec<&GraphNodeLite> = idxs.iter().map(|&i| &nodes[i]).collect();
            let mut lang_counter = OrderedCounter::new();
            for m in &members {
                if let Some(lang) = &m.language {
                    lang_counter.bump(lang);
                }
            }
            let dominant = lang_counter
                .most_common(1)
                .first()
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "name": generate_community_name(&members),
                "level": 0,
                "size": members.len(),
                "cohesion": cohesions[ci],
                "dominant_language": dominant,
                "description": format!("Directory-based community: {dir_path}"),
                "members": members.iter().map(|m| m.symbol.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    // -- dedupe names: shared with the Leiden tier (F3 drift guard) --
    dedupe_community_names(&mut communities, &nodes);
    Ok(communities)
}

static TEST_COMMUNITY_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(^test[-/]|[-/]test([:/]|$)|it:should|describe:|spec[-/]|[-/]spec$)")
        .expect("test-community pattern")
});

fn is_test_community(name: &str) -> bool {
    TEST_COMMUNITY_RE.is_match(name)
}

/// `get_architecture_overview` (communities.py:1020): communities +
/// cross-community edges (TESTED_BY excluded) + high-coupling warnings
/// (>10 edges, test-dominated pairs skipped). Uses fresh detection
/// (CRG reads its stored table — same values on a freshly built db).
pub fn architecture_overview(
    conn: &Connection,
    max_results: usize,
    minimal: bool,
) -> Result<Value, String> {
    let communities_full = detect_communities(conn, 2)?;
    let mut node_to_community: HashMap<&str, i64> = HashMap::new();
    for (ci, c) in communities_full.iter().enumerate() {
        for qn in c["members"].as_array().unwrap() {
            node_to_community.insert(qn.as_str().unwrap_or(""), (ci + 1) as i64);
        }
    }
    let all_edges = load_edges(conn)?;
    let mut cross_edges: Vec<Value> = Vec::new();
    let mut pair_counts = OrderedCounter::new();
    let mut pair_order: Vec<String> = Vec::new();
    let mut pair_set: HashSet<String> = HashSet::new();
    let mut pair_kinds: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for e in &all_edges {
        if e.kind == "TESTED_BY" {
            continue;
        }
        let sc = node_to_community.get(e.caller_symbol.as_str());
        let tc = node_to_community.get(e.callee_symbol.as_str());
        if let (Some(&s), Some(&t)) = (sc, tc) {
            if s != t {
                let key = format!("{}:{}", s.min(t), s.max(t));
                if !pair_set.contains(&key) {
                    pair_set.insert(key.clone());
                    pair_order.push(key.clone());
                }
                pair_counts.bump(&key);
                *pair_kinds
                    .entry(key.clone())
                    .or_default()
                    .entry(e.kind.clone())
                    .or_insert(0) += 1;
                if !minimal {
                    cross_edges.push(serde_json::json!({
                        "source_community": s,
                        "target_community": t,
                        "edge_kind": e.kind,
                        "source": sanitize_name(&e.caller_symbol),
                        "target": sanitize_name(&e.callee_symbol),
                    }));
                }
            }
        }
    }
    // warnings: count desc (stable), skip test-dominated names
    let mut warnings: Vec<String> = Vec::new();
    let mut ranked: Vec<(String, usize)> = pair_order
        .iter()
        .map(|k| {
            let count = pair_counts.counts[pair_counts.index[k]].1;
            (k.clone(), count)
        })
        .collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (key, count) in ranked {
        if count <= 10 {
            break;
        }
        let (c1, c2) = key.split_once(':').unwrap();
        let (c1, c2): (usize, usize) = (c1.parse().unwrap(), c2.parse().unwrap());
        let name1 = communities_full[c1 - 1]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let name2 = communities_full[c2 - 1]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if is_test_community(&name1) || is_test_community(&name2) {
            continue;
        }
        warnings.push(format!(
            "High coupling ({count} edges) between '{name1}' and '{name2}'"
        ));
        if warnings.len() >= max_results {
            break;
        }
    }
    // CRG _MINIMAL_COMMUNITY_FIELDS parity — no member lists (the NT
    // overview is 12.6MB with them, <5KB-class without)
    let communities: Vec<Value> = if minimal {
        communities_full
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c["name"],
                    "size": c["size"],
                    "cohesion": c["cohesion"],
                    "dominant_language": c["dominant_language"],
                    "description": c["description"],
                })
            })
            .collect()
    } else {
        communities_full.clone()
    };
    let cross_community_edges: Value = if minimal {
        let id_to_name = |i: usize| -> String {
            communities_full
                .get(i.wrapping_sub(1))
                .and_then(|c| c["name"].as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("community-{i}"))
        };
        let mut pairs: Vec<(String, usize)> = pair_order
            .iter()
            .map(|k| {
                let count = pair_counts.counts[pair_counts.index[k]].1;
                (k.clone(), count)
            })
            .collect();
        pairs.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        pairs
            .iter()
            .map(|(key, count)| {
                let (c1, c2) = key.split_once(':').unwrap();
                let (c1, c2): (usize, usize) = (c1.parse().unwrap(), c2.parse().unwrap());
                let mut kinds: Vec<(String, usize)> = pair_kinds[key]
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                kinds.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
                serde_json::json!({
                    "source_community": id_to_name(c1),
                    "target_community": id_to_name(c2),
                    "edge_count": count,
                    "top_kinds": kinds.iter().take(3).map(|(k, _)| k).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>()
            .into()
    } else {
        cross_edges.truncate(max_results);
        cross_edges.into()
    };
    Ok(serde_json::json!({
        "communities": communities,
        "cross_community_edges": cross_community_edges,
        "warnings": warnings,
    }))
}

/// Single-community drill-down (CRG get_community parity): partial name
/// match, members opt-in — the consumption exit for the minimal list
/// faces (a standard-detail list on a big repo is not consumable).
pub fn get_community(
    conn: &Connection,
    needle: &str,
    include_members: bool,
) -> Result<Value, String> {
    let cs = detect_communities(conn, 2)?;
    let hits: Vec<&Value> = cs
        .iter()
        .filter(|c| {
            c["name"]
                .as_str()
                .map(|n| n.to_lowercase().contains(&needle.to_lowercase()))
                .unwrap_or(false)
        })
        .collect();
    match hits.len() {
        0 => Err(format!("community 未命中：{needle}")),
        1 => {
            let c = hits[0];
            let mut out = serde_json::json!({
                "name": c["name"],
                "size": c["size"],
                "cohesion": c["cohesion"],
                "dominant_language": c["dominant_language"],
                "description": c["description"],
            });
            if include_members {
                out.as_object_mut()
                    .unwrap()
                    .insert("members".into(), c["members"].clone());
            }
            Ok(out)
        }
        n => Err(format!(
            "命中 {n} 個 community（{}）——請縮小名稱",
            hits.iter()
                .filter_map(|c| c["name"].as_str())
                .take(5)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

// ---------- S6: detect_changes + risk (changes.py) ----------

/// `_parse_unified_diff` (changes.py:136): `+++ b/<path>` headers +
/// `@@ ... +start[,count] @@` hunks; count=0 pure-deletion keeps the
/// position; `/dev/null` headers never match `+++ b/` so deleted files
/// are skipped. First-seen key order preserved (BTreeMap is fine — CRG
/// dict order only feeds per-file node lookups).
pub fn parse_unified_diff(diff_text: &str) -> std::collections::BTreeMap<String, Vec<(i64, i64)>> {
    static FILE_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^\+\+\+ b/(.+)$").unwrap());
    static HUNK_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^@@ .+? \+(\d+)(?:,(\d+))? @@").unwrap());
    let mut ranges: std::collections::BTreeMap<String, Vec<(i64, i64)>> =
        std::collections::BTreeMap::new();
    let mut current_file: Option<String> = None;
    for line in diff_text.lines() {
        if let Some(m) = FILE_RE.captures(line) {
            current_file = Some(m.get(1).unwrap().as_str().to_string());
            continue;
        }
        if let Some(m) = HUNK_RE.captures(line) {
            if current_file.is_some() {
                let start: i64 = m.get(1).unwrap().as_str().parse().unwrap_or(0);
                let count: i64 = m.get(2).and_then(|c| c.as_str().parse().ok()).unwrap_or(1);
                let end = if count == 0 { start } else { start + count - 1 };
                ranges
                    .entry(current_file.clone().unwrap())
                    .or_default()
                    .push((start, end));
            }
        }
    }
    ranges
}

/// `parse_git_diff_ranges` (changes.py:33): `git diff --unified=0 <base>`
/// with a safe-ref guard. Recorded deviation: no 30s subprocess timeout
/// (std Command has none; a hung local git hangs the tool — same exposure
/// class as the repo's other git shell-outs).
pub fn diff_ranges(
    repo_root: &Path,
    base: &str,
) -> Result<std::collections::BTreeMap<String, Vec<(i64, i64)>>, String> {
    static SAFE_REF: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[A-Za-z0-9_.~^/@{}\-]+$").unwrap());
    if !SAFE_REF.is_match(base) {
        return Err(format!("Invalid git ref rejected: {base}"));
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--unified=0", base, "--"])
        .output()
        .map_err(|e| format!("git diff 執行失敗：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git diff 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(parse_unified_diff(&String::from_utf8_lossy(&out.stdout)))
}

/// `map_changes_to_nodes` (changes.py:267): exact file_path lookup, then
/// LIKE-suffix fallback; overlap on node [line_start, line_end].
pub fn map_changes_to_nodes(
    conn: &Connection,
    changed_ranges: &std::collections::BTreeMap<String, Vec<(i64, i64)>>,
) -> Result<Vec<GraphNodeLite>, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<GraphNodeLite> = Vec::new();
    for (file_path, ranges) in changed_ranges {
        let mut nodes: Vec<GraphNodeLite> = Vec::new();
        {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {NODE_COLUMNS} FROM nodes WHERE file_path = ?1"
                ))
                .map_err(|e| format!("nodes by file 查詢失敗：{e}"))?;
            let rows = stmt
                .query_map([file_path.as_str()], row_to_node)
                .map_err(|e| format!("nodes by file 查詢失敗：{e}"))?;
            for n in rows {
                nodes.push(n.map_err(|e| format!("nodes by file 讀取失敗：{e}"))?);
            }
        }
        if nodes.is_empty() {
            // suffix fallback: graph may store absolute paths
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {NODE_COLUMNS} FROM nodes WHERE file_path LIKE ?1"
                ))
                .map_err(|e| format!("suffix 查詢失敗：{e}"))?;
            let pat = format!("%{file_path}");
            let rows = stmt
                .query_map([pat.as_str()], row_to_node)
                .map_err(|e| format!("suffix 查詢失敗：{e}"))?;
            for n in rows {
                nodes.push(n.map_err(|e| format!("suffix 讀取失敗：{e}"))?);
            }
        }
        for node in nodes {
            if seen.contains(&node.symbol) {
                continue;
            }
            let (Some(ls), Some(le)) = (node.line_start, node.line_end) else {
                continue;
            };
            if ranges.iter().any(|&(s, e)| ls <= e && le >= s) {
                seen.insert(node.symbol.clone());
                result.push(node);
            }
        }
    }
    Ok(result)
}

/// `compute_risk_score` (changes.py:312) — six factors. Recorded
/// deviation: `get_transitive_tests`' evidence-gated bare-name fallback
/// (legacy minimal graphs) is not ported — direct TESTED_BY + class
/// CONTAINS expansion + 1-hop CALLS frontier (50) is the live path on
/// modern graphs.
pub fn compute_risk_score(
    conn: &Connection,
    node: &GraphNodeLite,
    churn_counts: Option<&HashMap<String, i64>>,
) -> Result<f64, String> {
    let mut score = 0.0f64;
    // flow participation (cap 0.25): sum of flow criticalities, else count*0.05
    let crits: Vec<f64> = {
        let mut stmt = conn
            .prepare(
                "SELECT f.criticality FROM flows f JOIN flow_memberships fm \
                 ON fm.flow_id = f.id WHERE fm.node_id = ?1",
            )
            .map_err(|e| format!("flow criticalities 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([node.id], |r| r.get::<_, f64>(0))
            .map_err(|e| format!("flow criticalities 查詢失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("flow criticalities 讀取失敗：{e}"))?
    };
    if !crits.is_empty() {
        score += crits.iter().sum::<f64>().min(0.25);
    } else {
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM flow_memberships WHERE node_id = ?1",
                [node.id],
                |r| r.get(0),
            )
            .map_err(|e| format!("flow memberships 查詢失敗：{e}"))?;
        score += (cnt as f64 * 0.05).min(0.25);
    }
    // community crossing (cap 0.15): distinct CALLS callers from another community
    let caller_qns: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT caller_symbol FROM edges \
                 WHERE kind = 'CALLS' AND callee_symbol = ?1",
            )
            .map_err(|e| format!("callers 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([node.symbol.as_str()], |r| r.get::<_, String>(0))
            .map_err(|e| format!("callers 查詢失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("callers 讀取失敗：{e}"))?
    };
    let caller_count = caller_qns.len() as f64;
    let mut crossing = 0usize;
    if node.community_id.is_some() && !caller_qns.is_empty() {
        for qn in &caller_qns {
            let cid: Option<i64> = conn
                .query_row(
                    "SELECT community_id FROM nodes WHERE symbol = ?1",
                    [qn.as_str()],
                    |r| r.get(0),
                )
                .unwrap_or(None);
            if let Some(cid) = cid {
                if Some(cid) != node.community_id {
                    crossing += 1;
                }
            }
        }
    }
    score += (crossing as f64 * 0.05).min(0.15);
    // test coverage: transitive tests count / 5 scaling
    let test_count = transitive_test_count(conn, &node.symbol)?;
    score += 0.30 - (test_count as f64 / 5.0).min(1.0) * 0.25;
    // security sensitivity
    let name_lower = node.name.to_lowercase();
    let qn_lower = node.qualified_name.to_lowercase();
    if SECURITY_KEYWORDS
        .iter()
        .any(|kw| name_lower.contains(kw) || qn_lower.contains(kw))
    {
        score += 0.20;
    }
    // caller count (cap 0.10)
    score += (caller_count / 20.0).min(0.10);
    // churn (opt-in, cap 0.15)
    if let Some(churn) = churn_counts {
        if !node.file_path.is_empty() {
            let commits = churn.get(&node.file_path).copied().unwrap_or(0);
            score += (commits as f64 / 10.0).min(1.0) * 0.15;
        }
    }
    Ok(py_round(score.clamp(0.0, 1.0), 4))
}

/// Live-path `get_transitive_tests` count: class CONTAINS expansion +
/// direct TESTED_BY + 1-hop CALLS callee TESTED_BY (frontier cap 50).
fn transitive_test_count(conn: &Connection, symbol: &str) -> Result<usize, String> {
    let mut input_qns = vec![symbol.to_string()];
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM nodes WHERE symbol = ?1", [symbol], |r| {
            r.get(0)
        })
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|e| format!("kind 查詢失敗：{e}"))?;
    if kind.as_deref() == Some("Class") {
        let mut stmt = conn
            .prepare(
                "SELECT callee_symbol FROM edges \
                 WHERE caller_symbol = ?1 AND kind = 'CONTAINS'",
            )
            .map_err(|e| format!("contains 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([symbol], |r| r.get::<_, String>(0))
            .map_err(|e| format!("contains 查詢失敗：{e}"))?;
        for r in rows {
            input_qns.push(r.map_err(|e| format!("contains 讀取失敗：{e}"))?);
        }
    }
    let tested_by_of = |qn: &str| -> Result<Vec<String>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT callee_symbol FROM edges \
                 WHERE caller_symbol = ?1 AND kind = 'TESTED_BY'",
            )
            .map_err(|e| format!("tested_by 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([qn], |r| r.get::<_, String>(0))
            .map_err(|e| format!("tested_by 查詢失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("tested_by 讀取失敗：{e}"))
    };
    let mut seen: HashSet<String> = HashSet::new();
    for qn in &input_qns {
        for t in tested_by_of(qn)? {
            seen.insert(t);
        }
    }
    // 1-hop CALLS frontier (cap 50 per hop)
    for qn in &input_qns {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT callee_symbol FROM edges \
                 WHERE kind = 'CALLS' AND caller_symbol = ?1 LIMIT 50",
            )
            .map_err(|e| format!("calls 查詢失敗：{e}"))?;
        let rows = stmt
            .query_map([qn.as_str()], |r| r.get::<_, String>(0))
            .map_err(|e| format!("calls 查詢失敗：{e}"))?;
        for callee in rows {
            let callee = callee.map_err(|e| format!("calls 讀取失敗：{e}"))?;
            for t in tested_by_of(&callee)? {
                seen.insert(t);
            }
        }
    }
    Ok(seen.len())
}

/// `analyze_changes` (changes.py:381) — composition face. `ranges`
/// pre-parsed (the CLI/MCP face runs [`diff_ranges`] first and remaps
/// keys to repo-absolute, mirroring the #528 remap).
pub fn detect_changes(
    conn: &Connection,
    repo_root: Option<&Path>,
    changed_files: &[String],
    changed_ranges: Option<&std::collections::BTreeMap<String, Vec<(i64, i64)>>>,
    include_churn_args: Option<&HashMap<String, i64>>,
) -> Result<Value, String> {
    let _ = include_churn_args;
    // node mapping: ranges if provided, else all nodes of changed files
    let changed_nodes: Vec<GraphNodeLite> = if let Some(ranges) = changed_ranges {
        if ranges.is_empty() {
            Vec::new()
        } else {
            map_changes_to_nodes(conn, ranges)?
        }
    } else {
        let mut out = Vec::new();
        for fp in changed_files {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {NODE_COLUMNS} FROM nodes WHERE file_path = ?1"
                ))
                .map_err(|e| format!("nodes by file 查詢失敗：{e}"))?;
            let rows = stmt
                .query_map([fp.as_str()], row_to_node)
                .map_err(|e| format!("nodes by file 查詢失敗：{e}"))?;
            for n in rows {
                out.push(n.map_err(|e| format!("nodes by file 讀取失敗：{e}"))?);
            }
        }
        out
    };
    let mut changed_funcs: Vec<GraphNodeLite> = changed_nodes
        .into_iter()
        .filter(|n| matches!(n.kind.as_str(), "Function" | "Test" | "Class"))
        .filter(|n| n.extra.get("verilog_kind").is_none())
        .collect();
    let funcs_truncated = changed_funcs.len() > 500;
    changed_funcs.truncate(500);

    let churn: Option<HashMap<String, i64>> = None; // churn opt-in face not wired (CRG default off)
    let _ = repo_root;
    let mut node_risks: Vec<(f64, Value)> = Vec::new();
    for node in &changed_funcs {
        let risk = compute_risk_score(conn, node, churn.as_ref())?;
        let mut d = node_dict(node);
        d.as_object_mut()
            .unwrap()
            .insert("risk_score".into(), Value::from(risk));
        node_risks.push((risk, d));
    }
    let overall_risk = node_risks.iter().map(|(r, _)| *r).fold(0.0f64, f64::max);
    // affected flows: fresh trace over changed files
    let affected = affected_flows(conn, changed_files, 15, false)?;
    // test gaps: production funcs with no outgoing TESTED_BY
    let mut test_gaps: Vec<Value> = Vec::new();
    for node in &changed_funcs {
        if node.is_test {
            continue;
        }
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE caller_symbol = ?1 \
                 AND kind = 'TESTED_BY'",
                [node.symbol.as_str()],
                |r| r.get(0),
            )
            .map_err(|e| format!("tested_by 計數失敗：{e}"))?;
        if cnt == 0 {
            test_gaps.push(serde_json::json!({
                "name": sanitize_name(&node.name),
                "qualified_name": sanitize_name(&node.qualified_name),
                "file": node.file_path,
                "line_start": node.line_start,
                "line_end": node.line_end,
            }));
        }
    }
    let mut priorities: Vec<(f64, Value)> = node_risks.clone();
    priorities.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    priorities.truncate(10);

    let mut summary_parts = vec![
        format!("Analyzed {} changed file(s):", changed_files.len()),
        format!("  - {} changed function(s)/class(es)", changed_funcs.len()),
        format!("  - {} affected flow(s)", affected.len()),
        format!("  - {} test gap(s)", test_gaps.len()),
        format!("  - Overall risk score: {overall_risk:.2}"),
    ];
    if !test_gaps.is_empty() {
        let mut seen_names: HashSet<&str> = HashSet::new();
        let mut gap_names: Vec<&str> = Vec::new();
        for g in &test_gaps {
            let n = g["name"].as_str().unwrap_or("");
            if seen_names.contains(n) {
                continue;
            }
            seen_names.insert(n);
            gap_names.push(n);
            if gap_names.len() >= 5 {
                break;
            }
        }
        summary_parts.push(format!("  - Untested: {}", gap_names.join(", ")));
    }
    if funcs_truncated {
        summary_parts.push(
            "  - Warning: analysis capped at 500 functions (set CRG_MAX_CHANGED_FUNCS to adjust)"
                .into(),
        );
    }
    Ok(serde_json::json!({
        "summary": summary_parts.join("\n"),
        "risk_score": overall_risk,
        "changed_functions": node_risks.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "affected_flows": affected,
        "test_gaps": test_gaps,
        "review_priorities": priorities.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "functions_truncated": funcs_truncated,
    }))
}

// ---------- S8: keyword search (graph.py:695 search_nodes) ----------

/// `search_nodes`: FTS5 (`nodes_fts` MATCH, single-phrase quoted /
/// multi-word AND-quoted, JOIN nodes ON rowid, LIMIT) falling back to
/// LIKE substring on name/qualified_name when FTS yields nothing.
/// Returns (nodes, method) — method marks the face actually used.
pub fn search_nodes(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<(Vec<Value>, &'static str), String> {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return Ok((Vec::new(), "empty"));
    }
    // Phase 1: FTS5 (missing table or syntax error -> fall through, like
    // CRG's blanket except)
    let fts_query = if words.len() == 1 {
        format!("\"{}\"", words[0].replace('"', "\"\""))
    } else {
        words
            .iter()
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    let fts_ok: Result<Vec<GraphNodeLite>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.kind, n.name, n.qname, n.symbol, n.file_path, \
             n.line_start, n.line_end, n.language, n.parent_name, n.is_test, \
             n.community_id, n.extra \
             FROM nodes_fts f JOIN nodes n ON f.rowid = n.id \
             WHERE nodes_fts MATCH ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![fts_query, limit as i64], row_to_node)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })();
    if let Ok(nodes) = fts_ok {
        if !nodes.is_empty() {
            return Ok((nodes.iter().map(node_dict).collect(), "fts5"));
        }
    }
    // Phase 2: LIKE fallback
    let conds: Vec<String> = words
        .iter()
        .map(|_| "(LOWER(name) LIKE ? OR LOWER(qname) LIKE ?)".to_string())
        .collect();
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes WHERE {} LIMIT {}",
        conds.join(" AND "),
        limit
    );
    let params: Vec<String> = words
        .iter()
        .flat_map(|w| {
            vec![
                format!("%{}%", w.to_lowercase()),
                format!("%{}%", w.to_lowercase()),
            ]
        })
        .collect();
    let refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("LIKE 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map(refs.as_slice(), row_to_node)
        .map_err(|e| format!("LIKE 查詢失敗：{e}"))?;
    let nodes = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("LIKE 讀取失敗：{e}"))?;
    Ok((nodes.iter().map(node_dict).collect(), "like"))
}

/// `semantic_search` live face (embeddings.py:1120): with an empty
/// embeddings table — the only state this machine has ever had — the
/// result is the keyword fallback, marked as such in the envelope.
pub fn semantic_search(conn: &Connection, query: &str, limit: usize) -> Result<Value, String> {
    let (nodes, method) = search_nodes(conn, query, limit)?;
    Ok(serde_json::json!({
        "method": method,
        "note": "embeddings face not adopted (empty table) — keyword fallback, the live CRG behavior",
        "results": nodes,
    }))
}

// ---------- S7: minimal_context + review_context (tools/context.py, tools/review.py) ----------

fn risk_band(score: f64) -> &'static str {
    if score > 0.7 {
        "high"
    } else if score > 0.4 {
        "medium"
    } else {
        "low"
    }
}

fn task_suggestions(task: &str) -> Vec<&'static str> {
    let t = task.to_lowercase();
    let hit = |ws: &[&str]| ws.iter().any(|w| t.contains(w));
    if hit(&["review", "pr", "merge", "diff"]) {
        vec!["detect_changes", "get_affected_flows", "get_review_context"]
    } else if hit(&["debug", "bug", "error", "fix"]) {
        vec!["semantic_search_nodes", "query_graph", "get_flow"]
    } else if hit(&["refactor", "rename", "dead", "clean"]) {
        vec![
            "refactor",
            "find_large_functions",
            "get_architecture_overview",
        ]
    } else if hit(&["onboard", "understand", "explore", "arch"]) {
        vec![
            "get_architecture_overview",
            "list_communities",
            "list_flows",
        ]
    } else {
        vec![
            "detect_changes",
            "semantic_search_nodes",
            "get_architecture_overview",
        ]
    }
}

/// `get_minimal_context` (tools/context.py:37): stats + risk band +
/// top-3 stored communities/flows + task-keyword tool suggestions in the
/// compact envelope. Risk analysis uses the explicit changed-files face
/// (the auto-detect variant is the caller's job).
pub fn get_minimal_context(
    conn: &Connection,
    task: &str,
    changed_files: &[String],
) -> Result<Value, String> {
    let total_nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .map_err(|e| format!("stats 查詢失敗：{e}"))?;
    let total_edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .map_err(|e| format!("stats 查詢失敗：{e}"))?;
    let files_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE kind = 'File'", [], |r| {
            r.get(0)
        })
        .map_err(|e| format!("stats 查詢失敗：{e}"))?;
    let mut risk = "unknown";
    let mut risk_score = 0.0f64;
    let mut top_affected: Vec<String> = Vec::new();
    let mut test_gap_count = 0usize;
    if !changed_files.is_empty() {
        let analysis = detect_changes(conn, None, changed_files, None, None)?;
        risk_score = analysis["risk_score"].as_f64().unwrap_or(0.0);
        risk = risk_band(risk_score);
        top_affected = analysis["changed_functions"]
            .as_array()
            .unwrap()
            .iter()
            .take(5)
            .map(|f| f["name"].as_str().unwrap_or("").to_string())
            .collect();
        test_gap_count = analysis["test_gaps"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
    }
    let communities: Vec<String> = conn
        .prepare("SELECT name FROM communities ORDER BY size DESC LIMIT 3")
        .ok()
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .ok()?
                .collect::<Result<Vec<_>, _>>()
                .ok()
        })
        .unwrap_or_default();
    let flows: Vec<String> = conn
        .prepare("SELECT name FROM flows ORDER BY criticality DESC LIMIT 3")
        .ok()
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .ok()?
                .collect::<Result<Vec<_>, _>>()
                .ok()
        })
        .unwrap_or_default();
    let mut summary_parts = vec![format!(
        "{total_nodes} nodes, {total_edges} edges across {files_count} files."
    )];
    if risk != "unknown" {
        summary_parts.push(format!("Risk: {risk} ({risk_score:.2})."));
    }
    if test_gap_count > 0 {
        summary_parts.push(format!("{test_gap_count} test gaps."));
    }
    let mut out = serde_json::json!({
        "summary": summary_parts.join(" "),
        "risk": risk,
        "next_tool_suggestions": task_suggestions(task),
    });
    let obj = out.as_object_mut().unwrap();
    if !top_affected.is_empty() {
        obj.insert("key_entities".into(), Value::from(top_affected));
    }
    if !communities.is_empty() {
        obj.insert("communities".into(), Value::from(communities));
    }
    if !flows.is_empty() {
        obj.insert("flows_affected".into(), Value::from(flows));
    }
    Ok(out)
}

/// `get_review_context` (tools/review.py:25) — structural composition:
/// impact radius + numbered source snippets (capped) + guidance + next
/// suggestions. EP S7 bar is structural-key parity, not line-faithful
/// snippet extraction.
pub fn get_review_context(
    conn: &Connection,
    repo_root: &Path,
    changed_files: &[String],
    max_depth: usize,
    include_source: bool,
    max_lines_per_file: usize,
) -> Result<Value, String> {
    let impact = impact_radius(conn, changed_files, max_depth, 500)?;
    let changed_count = impact["changed_nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let impacted_count = impact["impacted_nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let file_count = impact["impacted_files"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let gaps = detect_changes(conn, None, changed_files, None, None)?;
    let gap_count = gaps["test_gaps"].as_array().map(|a| a.len()).unwrap_or(0);
    let mut snippets = serde_json::Map::new();
    if include_source {
        for f in changed_files {
            let p = repo_root.join(f);
            match std::fs::read_to_string(&p) {
                Ok(text) => {
                    let numbered = text
                        .lines()
                        .take(max_lines_per_file)
                        .enumerate()
                        .map(|(i, l)| format!("{}: {}", i + 1, l))
                        .collect::<Vec<_>>()
                        .join("\n");
                    snippets.insert(f.clone(), Value::String(numbered));
                }
                Err(_) => {
                    snippets.insert(f.clone(), Value::String("(could not read file)".into()));
                }
            }
        }
    }
    let guidance = if gap_count > 0 {
        format!("{gap_count} changed/impacted functions have no test coverage — prioritize tests for the highest-risk ones.")
    } else {
        "No test gaps among changed functions; focus review on impacted-node coupling.".into()
    };
    let summary = format!(
        "Review context for {} changed file(s):\n  - {changed_count} directly changed nodes\n  - {impacted_count} impacted nodes in {file_count} files\n\nReview guidance:\n{guidance}",
        changed_files.len()
    );
    Ok(serde_json::json!({
        "status": "ok",
        "summary": summary,
        "context": {
            "changed_files": changed_files,
            "impact": impact,
            "test_gaps": gaps["test_gaps"],
            "source_snippets": snippets,
            "review_guidance": guidance,
        },
        "next_tool_suggestions": ["detect_changes", "get_affected_flows", "semantic_search_nodes"],
    }))
}

// ---------- S9: CLI face (`graph_query <op>`) ----------
// One umbrella subcommand for the ten engine ops (EP deviation recorded:
// EP sketched per-op subcommands; the consumer face is MCP — a single
// argv-encoded subcommand keeps the argparse surface proportionate while
// MCP tools build argv per op, same CLI=MCP-single-backend premise).

fn gq_fail(msg: String) -> crate::ToolOutput {
    crate::ToolOutput::fail(msg)
}

/// `graph_query <op> --repo R [--files a,b] [--depth N] [--limit N]
/// [--max-results N] [--task T] [--base B] [--include-source]
/// [--max-lines N] [--max-nodes N] [--query Q]` — ops:
/// impact_radius / detect_changes / hub / bridge / communities /
/// arch_overview / flows / affected_flows / review_context /
/// minimal_context / search. JSON output (indent-1) on stdout.
pub fn run(argv: &[&str]) -> crate::ToolOutput {
    let Some((&_sub, rest)) = argv.split_first() else {
        return gq_fail("需提供子命令 graph_query".into());
    };
    let Some((op, flag_toks)) = rest.split_first() else {
        return gq_fail("需提供操作名（impact_radius/detect_changes/hub/bridge/communities/arch_overview/flows/affected_flows/review_context/minimal_context/search）".into());
    };
    let mut repo: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    let mut depth: Option<usize> = None;
    let mut max_nodes: Option<usize> = None;
    let mut limit: Option<usize> = None;
    let mut max_results: Option<usize> = None;
    let mut task = String::new();
    let mut base = String::new();
    let mut query = String::new();
    let mut include_source = false;
    let mut use_leiden = false;
    let mut use_minimal = false; // CLI default standard; MCP passes minimal
    let mut seed: u64 = 42;
    let mut max_lines: Option<usize> = None;
    let mut i = 0usize;
    while i < flag_toks.len() {
        let t = flag_toks[i];
        let value = |i: &mut usize| -> Option<String> {
            *i += 1;
            flag_toks.get(*i).map(|s| s.to_string())
        };
        let num = |i: &mut usize, name: &str| -> Result<Option<usize>, crate::ToolOutput> {
            match value(i) {
                None => Ok(None),
                Some(v) => match v.parse::<usize>() {
                    Ok(n) => Ok(Some(n)),
                    Err(_) => Err(gq_fail(format!("{name} 非非負整數：{v}"))),
                },
            }
        };
        match t {
            "--repo" => repo = value(&mut i),
            "--files" => {
                if let Some(v) = value(&mut i) {
                    files = v.split(',').map(|s| s.to_string()).collect();
                }
            }
            "--query" => query = value(&mut i).unwrap_or_default(),
            "--task" => task = value(&mut i).unwrap_or_default(),
            "--base" => base = value(&mut i).unwrap_or_default(),
            "--depth" => match num(&mut i, "--depth") {
                Ok(v) => depth = v,
                Err(o) => return o,
            },
            "--max-nodes" => match num(&mut i, "--max-nodes") {
                Ok(v) => max_nodes = v,
                Err(o) => return o,
            },
            "--limit" => match num(&mut i, "--limit") {
                Ok(v) => limit = v,
                Err(o) => return o,
            },
            "--max-results" => match num(&mut i, "--max-results") {
                Ok(v) => max_results = v,
                Err(o) => return o,
            },
            "--max-lines" => match num(&mut i, "--max-lines") {
                Ok(v) => max_lines = v,
                Err(o) => return o,
            },
            "--include-source" => include_source = true,
            "--union" => {
                return gq_fail(
                    "--union 已退休（v1+ S4）：聯集邊於 graph_db build 時物化進 .code-reality/graph.db，查詢預設全量".to_string(),
                );
            }
            "--leiden" => use_leiden = true,
            "--detail-level" => match value(&mut i) {
                Some(v) if v == "minimal" => use_minimal = true,
                Some(v) if v == "standard" => use_minimal = false,
                other => {
                    return gq_fail(format!(
                        "--detail-level 須為 minimal|standard，收到：{}",
                        other.unwrap_or_default()
                    ));
                }
            },
            "--seed" => match num(&mut i, "--seed") {
                Ok(v) => seed = v.unwrap_or(42) as u64,
                Err(o) => return o,
            },
            "-h" | "--help" => {
                return crate::ToolOutput {
                    stdout: concat!(
                        "usage: graph_query [-h] <op> --repo REPO [--files FILES] ",
                        "[--leiden] [--seed N] ",
                        "[--depth N] [--limit N] [--max-results N] [--task TASK] ",
                        "[--base BASE] [--query QUERY] [--include-source] ",
                        "[--max-lines N] [--max-nodes N]\n",
                        "ops: impact_radius detect_changes hub bridge communities ",
                        "arch_overview flows affected_flows review_context ",
                        "minimal_context search symbols\n"
                    )
                    .to_string(),
                    stderr: String::new(),
                    exit_code: 0,
                }
            }
            other => {
                return gq_fail(format!("unrecognized arguments: {other}"));
            }
        }
        i += 1;
    }
    let Some(repo) = repo else {
        return gq_fail("--repo 為必要參數（graph_query 不猜 cwd）".into());
    };
    // comma-join argv encoding (MCP face): the guard below rejects EMPTY
    // segments (double comma); paths containing a comma are rejected at
    // the MCP layer where the list is still structured
    if let Some(bad) = files.iter().position(|f| f.is_empty()) {
        return gq_fail(format!(
            "--files 第 {bad} 個路徑為空（逗號分隔編碼限制）——改傳單一路徑或多次呼叫"
        ));
    }
    let repo_path = std::path::PathBuf::from(&repo);
    // cache-only face: document_symbols reads the SCIP cache, not
    // graph.db — an index-only repo must not be blocked by a missing CRG
    // graph (delta-review F1)
    if *op == "symbols" {
        return match document_symbols(&repo_path, &query) {
            Ok(v) => crate::ToolOutput {
                stdout: format!(
                    "[OK] graph_query symbols\n{}",
                    crate::common::to_json_indent1(&v)
                ),
                stderr: String::new(),
                exit_code: 0,
            },
            Err(e) => gq_fail(e),
        };
    }
    let conn = match open(&repo_path) {
        Ok(c) => c,
        Err(e) => return gq_fail(e),
    };
    let op: &str = op;
    let res: Result<Value, String> = match op {
        "impact_radius" => {
            impact_radius(&conn, &files, depth.unwrap_or(2), max_nodes.unwrap_or(500))
        }
        "detect_changes" => {
            let ranges = if !base.is_empty() {
                match diff_ranges(&repo_path, &base) {
                    Ok(r) => {
                        // remap keys to repo-absolute (CRG #528)
                        let root = crate::common::resolve(&repo_path);
                        let remapped: std::collections::BTreeMap<String, Vec<(i64, i64)>> = r
                            .into_iter()
                            .map(|(k, v)| (root.join(k).to_string_lossy().into_owned(), v))
                            .collect();
                        Some(remapped)
                    }
                    Err(e) => return gq_fail(e),
                }
            } else {
                None
            };
            detect_changes(&conn, Some(&repo_path), &files, ranges.as_ref(), None)
        }
        "hub" => {
            if use_minimal {
                return gq_fail("--detail-level 僅作用於 communities/arch_overview".to_string());
            }
            find_hub_nodes(&conn, limit.unwrap_or(10)).map(Value::Array)
        }
        "bridge" => find_bridge_nodes(&conn, limit.unwrap_or(10)).map(Value::Array),
        "communities" => if use_leiden {
            detect_communities_leiden(&conn, 2, seed)
        } else {
            detect_communities(&conn, 2)
        }
        .map(|mut cs| {
            if use_minimal {
                // CRG list_communities minimal parity: summary fields only
                for c in cs.iter_mut() {
                    if let Some(obj) = c.as_object_mut() {
                        obj.remove("members");
                    }
                }
            }
            Value::Array(cs)
        }),
        "flows" => {
            if use_minimal {
                return gq_fail("--detail-level 僅作用於 communities/arch_overview".to_string());
            }
            trace_flows(&conn, 15, false)
                .map(|mut fs| {
                    // CRG list_flows parity: limit defaults to 50 (NT full
                    // output is 7.6MB); a large --limit keeps CLI full output
                    let cap = limit.unwrap_or(50);
                    if fs.len() > cap {
                        eprintln!(
                            "[WARN] flows truncated to {cap} of {} — pass a larger --limit for full output",
                            fs.len()
                        );
                    }
                    fs.truncate(cap);
                    Value::Array(fs)
                })
        }
        "arch_overview" => architecture_overview(&conn, max_results.unwrap_or(100), use_minimal),
        "affected_flows" => affected_flows(&conn, &files, 15, false).map(Value::Array),
        "review_context" => get_review_context(
            &conn,
            &repo_path,
            &files,
            depth.unwrap_or(2),
            include_source,
            max_lines.unwrap_or(200),
        ),
        "minimal_context" => get_minimal_context(&conn, &task, &files),
        "search" => semantic_search(&conn, &query, limit.unwrap_or(20)),
        other => return gq_fail(format!("未知操作：{other}")),
    };
    // route Result errors to exit 2 (env-level) per family convention
    let res = match res {
        Ok(v) => v,
        Err(e) => return gq_fail(e),
    };
    crate::ToolOutput {
        stdout: format!(
            "[OK] graph_query {op}\n{}",
            crate::common::to_json_indent1(&res)
        ),
        stderr: String::new(),
        exit_code: 0,
    }
}

// ---------- S10: Leiden Tier 1 (single-clustering, seeded deterministic) ----------

/// Tier-1 communities via seeded Leiden (single-clustering 0.7, BSD-3;
/// fixed seed = bit-for-bit deterministic). Edge weights follow CRG's
/// clustering-affinity table (communities.py:60: CALLS 1.0 / INHERITS
/// 0.8 / IMPLEMENTS 0.7 / TESTED_BY 0.4 / DEPENDS_ON 0.6 / IMPORTS_FROM
/// 0.5 / CONTAINS 0.3, default 0.5) with max-per-undirected-pair dedup;
/// resolution scales as `max(0.05, 1/log10(n))` (communities.py:405 —
/// deliberate deviations from the cited oracle: CRG floors n at 10
/// (1.0 below that); CRG dedups undirected pairs first-wins, we keep the
/// max weight; CRG additionally reassigns test nodes via TESTED_BY votes
/// (`_reassign_test_nodes`) — none ported for this non-frozen face).
pub fn detect_communities_leiden(
    conn: &Connection,
    min_size: usize,
    seed: u64,
) -> Result<Vec<Value>, String> {
    use single_clustering::community_search::leiden::{
        leiden, modularity, LeidenConfig, ObjectiveKind,
    };
    use single_clustering::network::CSRNetwork;
    let edges = load_edges(conn)?;
    let nodes = load_nodes(conn, true)?;
    let idx_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.symbol.as_str(), i))
        .collect();
    fn cluster_weight(kind: &str) -> f64 {
        match kind {
            "CALLS" => 1.0,
            "INHERITS" => 0.8,
            "IMPLEMENTS" => 0.7,
            "TESTED_BY" => 0.4,
            "DEPENDS_ON" => 0.6,
            "IMPORTS_FROM" => 0.5,
            "CONTAINS" => 0.3,
            _ => 0.5,
        }
    }
    let mut weights: HashMap<(usize, usize), f64> = HashMap::new();
    for e in &edges {
        let (Some(&s), Some(&t)) = (
            idx_of.get(e.caller_symbol.as_str()),
            idx_of.get(e.callee_symbol.as_str()),
        ) else {
            continue;
        };
        if s == t {
            continue;
        }
        let key = (s.min(t), s.max(t));
        let w = cluster_weight(&e.kind);
        let slot = weights.entry(key).or_insert(0.0);
        if w > *slot {
            *slot = w;
        }
    }
    let n = nodes.len();
    if n == 0 || weights.is_empty() {
        return detect_communities(conn, min_size);
    }
    // deterministic construction: sort so CSR build order is stable
    let mut tuples: Vec<(usize, usize, f64)> =
        weights.iter().map(|(&(a, b), &w)| (a, b, w)).collect();
    tuples.sort_by_key(|(a, b, _)| (*a, *b));
    let graph =
        CSRNetwork::from_edges(n, &tuples).map_err(|e| format!("Leiden 圖建構失敗：{e}"))?;
    let resolution = (1.0f64 / (n as f64).log10()).max(0.05);
    let config = LeidenConfig {
        objective: ObjectiveKind::Rb { resolution },
        seed: Some(seed),
        refine: true,
        ..Default::default()
    };
    let clustering = leiden(&graph, &config).map_err(|e| format!("Leiden 執行失敗：{e}"))?;
    let labels = clustering.labels();
    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        clusters.entry(label).or_default().push(i);
    }
    let mut ordered: Vec<Vec<usize>> = clusters.into_values().collect();
    ordered.sort_by_key(|c| c[0]); // deterministic order (min member idx)
                                   // node metadata + cohesion like Tier 0
    let member_sets: Vec<HashSet<String>> = ordered
        .iter()
        .map(|idxs| idxs.iter().map(|&i| nodes[i].symbol.clone()).collect())
        .collect();
    let cohesions = compute_cohesion_batch(&member_sets, &edges);
    let modq = modularity(&graph, labels, resolution);
    let mut communities: Vec<Value> = ordered
        .iter()
        .enumerate()
        .filter(|(_, idxs)| idxs.len() >= min_size)
        .map(|(ci, idxs)| {
            let members: Vec<&GraphNodeLite> =
                idxs.iter().map(|&i| &nodes[i]).collect();
            let mut lang_counter = OrderedCounter::new();
            for m in &members {
                if let Some(lang) = &m.language {
                    lang_counter.bump(lang);
                }
            }
            let dominant = lang_counter
                .most_common(1)
                .first()
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "name": generate_community_name(&members),
                "level": 0,
                "size": members.len(),
                "cohesion": cohesions[ci],
                "dominant_language": dominant,
                "description": format!(
                    "Leiden community (modularity {modq:.4}, resolution {resolution:.4}, seed {seed})"
                ),
                "members": members.iter().map(|m| m.symbol.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    // name dedupe reuse (same rule as Tier 0)
    dedupe_community_names(&mut communities, &nodes);
    Ok(communities)
}

/// Shared name-dedupe (communities.py:732) extracted for both tiers.
fn dedupe_community_names(communities: &mut [Value], nodes: &[GraphNodeLite]) {
    let nodes_by_key: HashMap<&str, &GraphNodeLite> =
        nodes.iter().map(|n| (n.symbol.as_str(), n)).collect();
    let mut taken: HashSet<String> = communities
        .iter()
        .map(|c| c["name"].as_str().unwrap_or("").to_string())
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (pos, c) in communities.iter().enumerate() {
        let name = c["name"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        if !by_name.contains_key(&name) {
            order.push(name.clone());
        }
        by_name.entry(name).or_default().push(pos);
    }
    for base_name in order {
        let dupes = &by_name[&base_name];
        if dupes.len() <= 1 {
            continue;
        }
        let mut ranked = dupes.clone();
        ranked.sort_by(|&a, &b| {
            let sa = communities[a]["size"].as_i64().unwrap_or(0);
            let sb = communities[b]["size"].as_i64().unwrap_or(0);
            sb.cmp(&sa).then(a.cmp(&b))
        });
        let base_words: HashSet<&str> = base_name.split('-').collect();
        for &pos in &ranked[1..] {
            let member_nodes: Vec<&GraphNodeLite> = communities[pos]["members"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|qn| nodes_by_key.get(qn.as_str().unwrap_or("")))
                .copied()
                .collect();
            let naming: Vec<&GraphNodeLite> = {
                let prod: Vec<&&GraphNodeLite> =
                    member_nodes.iter().filter(|m| !is_test_node(m)).collect();
                if prod.is_empty() {
                    member_nodes.clone()
                } else {
                    prod.into_iter().copied().collect()
                }
            };
            let mut candidate = String::new();
            for keyword in extract_keywords(&naming) {
                let suffix = to_slug(&keyword);
                if suffix.is_empty() || base_words.contains(suffix.as_str()) {
                    continue;
                }
                let cand = format!("{base_name}-{suffix}");
                if !taken.contains(&cand) {
                    candidate = cand;
                    break;
                }
            }
            if candidate.is_empty() {
                let mut k = 2;
                candidate = format!("{base_name}-{k}");
                while taken.contains(&candidate) {
                    k += 1;
                    candidate = format!("{base_name}-{k}");
                }
            }
            taken.insert(candidate.clone());
            communities[pos]["name"] = Value::String(candidate);
        }
    }
}

// ---------- LSP-aligned face: document_symbols (SCIP cache outline) ----------

/// File outline from the SCIP cache's defining occurrences — the
/// documentSymbol-alike face. Hover/type signatures are NOT in SCIP
/// data; those stay LSP-only (recorded boundary).
pub fn document_symbols(repo_root: &Path, file_rel: &str) -> Result<Value, String> {
    let index = crate::engine::default_index_path(repo_root)?;
    document_symbols_at(&index, file_rel)
}

/// Core over an explicit index path (test seam; the repo-keyed home is
/// global by basename — delta-review F4).
pub fn document_symbols_at(index_path: &Path, file_rel: &str) -> Result<Value, String> {
    let cache_db = crate::cache::sqlite_path(index_path);
    if !cache_db.exists() {
        return Err(format!(
            "SCIP cache 不在：{}（先 `code-reality scip_refs --build-cache --repo <repo>`）",
            cache_db.display()
        ));
    }
    let conn = crate::common::connect_ro(&cache_db)?;
    let mut stmt = conn
        .prepare(
            "SELECT symbol, line FROM occurrences WHERE rel_path = ?1 AND is_def = 1 \
             ORDER BY line",
        )
        .map_err(|e| format!("outline 查詢失敗：{e}"))?;
    let rows = stmt
        .query_map([file_rel], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("outline 查詢失敗：{e}"))?;
    let mut symbols = Vec::new();
    for r in rows {
        let (symbol, line) = r.map_err(|e| format!("outline 讀取失敗：{e}"))?;
        let name =
            crate::engine::fn_tail_name(&symbol).unwrap_or_else(|| crate::engine::tail(&symbol));
        symbols.push(serde_json::json!({
            "name": name,
            "line": line,
        }));
    }
    Ok(serde_json::json!({
        "file": file_rel,
        "symbols": symbols,
        "note": "hover/type signatures are LSP-only (not in SCIP data)",
    }))
}
