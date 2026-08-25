//! `hub_refs` — the frozen `code_reality/hub_refs.py` contract: CRG
//! callers_of/callees_of aggregated per directory with test/prod split,
//! plus the dynamic-dispatch hazard safety net (§5.4). Symbol
//! resolution goes through exact nodes-table matching (the CRG CLI's
//! ambiguous candidates are fuzzy substrings without the precise class
//! node — the nodes table is the only reliable resolver).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{
    assert_db_unchanged, db_mtime_ns, graph_db_path, repo_relative,
    to_json_py_compact,
};
use crate::hazard::{
    full_findings, hazard_gate_warning, make_rg_runner, method_name, resident_findings,
    symbol_facts, HazardFinding,
};
use crate::profile::{is_excluded, load_profile, Profile};
use crate::ToolOutput;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CRG_TIMEOUT_S: u64 = 120;
/// static prod callers at or below this triggers the rg-level hazard scan
pub const RG_TRIGGER_PROD: i64 = 2;

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--direction",
            short: None,
            kind: Kind::Value { metavar: "DIRECTION" },
        },
        FlagSpec {
            long: "--top",
            short: None,
            kind: Kind::Value { metavar: "TOP" },
        },
        FlagSpec {
            long: "--hazard",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &["symbol"],
};

const HELP: &str = concat!(
    "usage: hub_refs [-h] [--repo REPO] [--direction {callers,callees}]\n",
    "                [--top TOP] [--hazard] [--json] symbol\n",
    "\n",
    "hub-refs 聚合器——CRG callers_of/callees_of 按檔聚合＋test/prod 切分。\n",
    "\n",
    "positional arguments:\n",
    "  symbol                qualified name 或裸名（自動解析）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo 根（CRG cwd）\n",
    "  --direction DIRECTION\n",
    "                        refs 方向\n",
    "  --top TOP             每欄最多列 N 目錄\n",
    "  --hazard              強制全規則 hazard 掃描（常規為觸發式：static_prod ≤ RG_TRIGGER_PROD=2 才掃）\n",
    "  --json                機器可讀輸出（hazard_findings 欄）\n",
);

/// `uvx code-review-graph query <pattern> <target>` subprocess
/// (`hub_refs.py:70-99`): 120s timeout; nonzero exit → crash; stdout
/// must be JSON.
pub fn crg_query(pattern: &str, target: &str, repo_root: &Path) -> Result<Value, String> {
    let mut child = std::process::Command::new("uvx")
        .arg("code-review-graph")
        .arg("query")
        .arg(pattern)
        .arg(target)
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "uvx 不在 PATH——先安裝 uv: uvx".to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(CRG_TIMEOUT_S);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "CRG CLI 逾時（>{CRG_TIMEOUT_S}s）: uvx code-review-graph query {pattern} {target}"
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(format!("CRG CLI wait 失敗：{e}")),
        }
    };
    let mut out_pipe = child.stdout.take().unwrap();
    let mut err_pipe = child.stderr.take().unwrap();
    let (mut stdout_buf, mut stderr_buf) = (Vec::new(), Vec::new());
    use std::io::Read;
    let t1 = std::thread::spawn(move || {
        out_pipe.read_to_end(&mut stdout_buf).ok();
        stdout_buf
    });
    let t2 = std::thread::spawn(move || {
        err_pipe.read_to_end(&mut stderr_buf).ok();
        stderr_buf
    });
    let stdout = String::from_utf8_lossy(&t1.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&t2.join().unwrap_or_default()).into_owned();
    if !status.success() {
        let tail: String = stderr.chars().rev().take(500).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!(
            "CRG CLI 失敗（exit {}）: query {pattern} {target}\n{tail}",
            status.code().unwrap_or(-1)
        ));
    }
    serde_json::from_str(&stdout).map_err(|_| {
        format!(
            "CRG CLI stdout 非 JSON（可能混入 warnings）: {} | stderr: {}",
            stdout.chars().take(200).collect::<String>(),
            stderr.chars().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect::<String>()
        )
    })
}

/// status≠ok output face (`hub_refs.py:102-114`): `[FAIL]` + candidate
/// lines on STDOUT, then exit 1 — a silent `[OK] 0 refs` for a typo'd
/// qualified name would read as "no refs, safe to delete".
/// Test hook for the not-found/ambiguous output faces (stdout [FAIL] +
/// candidates, stderr message, exit 1 — the anti-false-negative design).
pub fn require_ok_test_hook(resp: &Value) -> Result<(), ToolOutput> {
    require_ok(resp)
}

fn require_ok(resp: &Value) -> Result<(), ToolOutput> {
    if resp.get("status").and_then(Value::as_str) == Some("ok") {
        return Ok(());
    }
    let status = resp.get("status").and_then(Value::as_str).unwrap_or("");
    let summary = resp.get("summary").and_then(Value::as_str).unwrap_or("(無 summary)");
    let mut stdout = format!("[FAIL] CRG {status}: {summary}\n");
    if let Some(cands) = resp.get("candidates").and_then(Value::as_array) {
        for c in cands.iter().take(10) {
            stdout.push_str(&format!(
                "  候選: {}  (is_test={})\n",
                c.get("qualified_name").and_then(Value::as_str).unwrap_or(""),
                match c.get("is_test") {
                    Some(Value::Bool(b)) => b.to_string(),
                    _ => "None".to_string(),
                }
            ));
        }
    }
    Err(ToolOutput {
        stdout,
        stderr: format!("CRG query {status}: {summary}\n"),
        exit_code: 1,
    })
}

/// symbol → qualified name via exact nodes-table matching
/// (`hub_refs.py:117-168`): `::` passthrough; `.` → name+parent_name;
/// unique → qname; multiple → `[FAIL]` + list + exit 1; zero →
/// `symbol not found` exit 1.
pub fn resolve_qualified(symbol: &str, repo_root: &Path) -> Result<String, ToolOutput> {
    if symbol.contains("::") {
        return Ok(symbol.to_string());
    }
    let db_path = graph_db_path(repo_root);
    let profile = match load_profile(repo_root) {
        Ok(p) => p,
        Err(e) => return Err(ToolOutput::crash(e)),
    };
    if !db_path.exists() {
        return Err(ToolOutput::crash(format!(
            "graph.db 不存在：{}——先跑 `uvx code-review-graph build`",
            db_path.display()
        )));
    }
    let m0 = match db_mtime_ns(&db_path) {
        Ok(v) => v,
        Err(e) => return Err(ToolOutput::crash(e)),
    };
    let rows = query_nodes_pairs(&db_path, symbol).map_err(ToolOutput::crash)?;
    if let Err(e) = assert_db_unchanged(&db_path, m0) {
        return Err(ToolOutput::crash(e));
    }
    let mut kept: Vec<String> = Vec::new();
    for (q, fp) in rows {
        if let Some(rel) = repo_relative(&fp, repo_root) {
            if !is_excluded(&rel, profile.as_ref()) {
                kept.push(q);
            }
        }
    }
    if kept.len() == 1 {
        return Ok(kept.pop().unwrap());
    }
    if kept.len() > 1 {
        let mut stdout = format!(
            "[FAIL] '{symbol}' 匹配 {} 個 node（用 qualified_name 重跑）：\n",
            kept.len()
        );
        for q in kept.iter().take(10) {
            stdout.push_str(&format!("  {q}\n"));
        }
        return Err(ToolOutput {
            stdout,
            stderr: format!("ambiguous symbol: {symbol}\n"),
            exit_code: 1,
        });
    }
    Err(ToolOutput {
        stdout: String::new(),
        stderr: format!("symbol not found: {symbol}——試完整 qualified name（<abs>::Class.method）\n"),
        exit_code: 1,
    })
}

fn query_nodes_pairs(db_path: &Path, symbol: &str) -> Result<Vec<(String, String)>, String> {
    let conn = crate::common::connect_ro(db_path)?;
    let (sql, params): (String, Vec<String>) = if let Some((cls, method)) = symbol.rsplit_once('.') {
        (
            "SELECT qualified_name, file_path FROM nodes WHERE name = ?1 AND parent_name = ?2"
                .to_string(),
            vec![method.to_string(), cls.to_string()],
        )
    } else {
        (
            "SELECT qualified_name, file_path FROM nodes WHERE name = ?1".to_string(),
            vec![symbol.to_string()],
        )
    };
    let rows: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare(&sql)?;
        let mapped = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(mapped)
    })();
    rows.map_err(|e| {
        format!(
            "非 CRG graph.db（讀 nodes 失敗：{e}）：{}——先跑 `uvx code-review-graph build`",
            db_path.display()
        )
    })
}

/// Name resolution + query (`hub_refs.py:171-181`).
pub fn resolve_symbol(
    symbol: &str,
    repo_root: &Path,
    direction: &str,
) -> Result<Value, ToolOutput> {
    let pattern = if direction == "callers" { "callers_of" } else { "callees_of" };
    let qname = resolve_qualified(symbol, repo_root)?;
    let resp = crg_query(pattern, &qname, repo_root).map_err(ToolOutput::crash)?;
    require_ok(&resp)?;
    Ok(resp)
}

#[derive(Debug, Clone, Default)]
pub struct AggResult {
    pub prod: Vec<(String, i64)>,
    pub test: Vec<(String, i64)>,
    pub total_prod: i64,
    pub total_test: i64,
    pub excluded: i64,
    pub outside: i64,
}

/// Insertion-ordered counter (Python `Counter`): `most_common(top)`
/// returns counts descending with ties in first-seen order.
#[derive(Default)]
struct OrderedCounter {
    order: Vec<String>,
    counts: std::collections::HashMap<String, i64>,
}

impl OrderedCounter {
    fn bump(&mut self, key: &str) {
        if !self.counts.contains_key(key) {
            self.order.push(key.to_string());
        }
        *self.counts.entry(key.to_string()).or_insert(0) += 1;
    }

    fn total(&self) -> i64 {
        self.counts.values().sum()
    }

    fn most_common(&self, top: usize) -> Vec<(String, i64)> {
        let mut rows: Vec<(usize, &String, i64)> = self
            .order
            .iter()
            .enumerate()
            .map(|(i, k)| (i, k, self.counts[k]))
            .collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        rows.into_iter()
            .take(top)
            .map(|(_, k, v)| (k.clone(), v))
            .collect()
    }
}

/// Per-directory (filename stripped) ref counts, is_test split,
/// exclusions filtered (`hub_refs.py:184-219`). The `tests/` path
/// heuristic patches CRG's is_test under-labeling.
pub fn aggregate(results: &[Value], repo_root: &Path, top: usize) -> Result<AggResult, String> {
    let repo_root = crate::common::resolve(repo_root);
    let profile = load_profile(&repo_root)?;
    let mut prod_counts = OrderedCounter::default();
    let mut test_counts = OrderedCounter::default();
    let mut excluded = 0i64;
    let mut outside = 0i64;
    for r in results {
        let Some(fp) = r.get("file_path").and_then(Value::as_str) else {
            continue;
        };
        if fp.is_empty() {
            continue;
        }
        let Ok(rel) = Path::new(fp).strip_prefix(&repo_root) else {
            outside += 1; // outside repo (venv/other checkout) — counted, not silent
            continue;
        };
        let rel = rel.to_string_lossy().into_owned();
        if is_excluded(&rel, profile.as_ref()) {
            excluded += 1;
            continue;
        }
        let d = Path::new(&rel)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_test_flag = matches!(r.get("is_test"), Some(Value::Bool(true)));
        if is_test_flag || rel.starts_with("tests/") {
            test_counts.bump(&d);
        } else {
            prod_counts.bump(&d);
        }
    }
    Ok(AggResult {
        prod: prod_counts.most_common(top),
        test: test_counts.most_common(top),
        total_prod: prod_counts.total(),
        total_test: test_counts.total(),
        excluded,
        outside,
    })
}

/// CRG refs → repo-relative caller file set (static-edge-gap baseline,
/// `hub_refs.py:222-239`).
pub fn caller_files_of(
    results: &[Value],
    repo_root: &Path,
    profile: Option<&Profile>,
) -> BTreeSet<String> {
    let repo_root = crate::common::resolve(repo_root);
    let mut files = BTreeSet::new();
    for r in results {
        let Some(fp) = r.get("file_path").and_then(Value::as_str) else {
            continue;
        };
        if let Ok(rel) = Path::new(fp).strip_prefix(&repo_root) {
            let rel = rel.to_string_lossy().into_owned();
            if !is_excluded(&rel, profile) {
                files.insert(rel);
            }
        }
    }
    files
}

/// §5.4 hazard safety net (`hub_refs.py:242-288`): resident AST level
/// always; rg-level full scan when forced or (callers direction,
/// static_prod ≤ RG_TRIGGER_PROD). Returns (findings, gate warning,
/// level) — level feeds `--json` consumers distinguishing existence
/// signals from counts.
pub fn hazard_stage(
    symbol: &str,
    repo_root: &Path,
    direction: &str,
    total_prod: i64,
    total_test: i64,
    results: &[Value],
    force: bool,
) -> Result<(Vec<HazardFinding>, Option<String>, &'static str), String> {
    let profile = load_profile(repo_root)?;
    let registries: Vec<crate::profile::HazardRegistry> = profile
        .as_ref()
        .map(|p| p.hazard_registries.clone())
        .unwrap_or_default();
    let facts = symbol_facts(symbol, repo_root, profile.as_ref())?;
    let triggered = force || (direction == "callers" && total_prod <= RG_TRIGGER_PROD);
    let (findings, level) = if triggered {
        let rg = make_rg_runner(repo_root);
        let baseline = if direction == "callers" {
            Some(caller_files_of(results, repo_root, profile.as_ref()))
        } else {
            None
        };
        (
            full_findings(
                &facts,
                &registries,
                &rg,
                baseline.as_ref(),
                profile.as_ref(),
                method_name(symbol).as_deref(),
            )?,
            "full",
        )
    } else {
        (resident_findings(&facts, &registries), "resident")
    };
    let warn = if direction == "callers" {
        hazard_gate_warning(total_prod, total_test, &findings, RG_TRIGGER_PROD)
    } else {
        None
    };
    Ok((findings, warn, level))
}

/// `--json` payload (`hub_refs.py:291-319`): key order preserved
/// (serde_json preserve_order); `detail` insertion-ordered.
#[allow(clippy::too_many_arguments)] // frozen parameter shape (hub_refs.py:291)
pub fn json_payload(
    args_symbol: &str,
    target: &str,
    direction: &str,
    agg: &AggResult,
    findings: &[HazardFinding],
    warn: Option<&str>,
    results_omitted: i64,
    hazard_level: &str,
) -> Value {
    let findings_v: Vec<Value> = findings
        .iter()
        .map(|f| {
            let mut m = Map::new();
            m.insert("kind".into(), json!(f.kind));
            m.insert("count".into(), json!(f.count));
            m.insert("summary".into(), json!(f.summary));
            m.insert("evidence".into(), json!(f.evidence));
            m.insert(
                "detail".into(),
                Value::Object(
                    f.detail
                        .iter()
                        .map(|(k, v)| (k.clone(), json!(v)))
                        .collect(),
                ),
            );
            Value::Object(m)
        })
        .collect();
    json!({
        "symbol": args_symbol,
        "target": target,
        "direction": direction,
        "results_omitted": results_omitted,
        "aggregate": {
            "prod": agg.prod.iter().map(|(d, n)| json!([d, n])).collect::<Vec<_>>(),
            "test": agg.test.iter().map(|(d, n)| json!([d, n])).collect::<Vec<_>>(),
            "total_prod": agg.total_prod,
            "total_test": agg.total_test,
            "excluded": agg.excluded,
            "outside": agg.outside,
        },
        "hazard_findings": findings_v,
        "hazard_level": hazard_level,
        "hazard_gate": warn,
    })
}

/// Route a `code-reality hub_refs ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 hub_refs");
    };
    let (values, positionals) = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, positionals } => (values, positionals),
    };
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let direction = values.get("--direction").and_then(|v| v.clone()).unwrap_or_else(|| "callers".into());
    if direction != "callers" && direction != "callees" {
        return ToolOutput::fail(format!(
            "argument --direction: invalid choice: '{direction}' (choose from 'callers', 'callees')"
        ));
    }
    let top_s = values.get("--top").and_then(|v| v.clone()).unwrap_or_else(|| "20".into());
    let top: usize = match top_s.parse() {
        Ok(v) => v,
        Err(_) => {
            return ToolOutput::fail(format!("argument --top: invalid int value: '{top_s}'"));
        }
    };
    let force_hazard = values.contains_key("--hazard");
    let as_json = values.contains_key("--json");
    let symbol = positionals[0].clone();

    let resp = match resolve_symbol(&symbol, &repo, &direction) {
        Ok(v) => v,
        Err(out) => return out,
    };
    let results: Vec<Value> = resp
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let agg = match aggregate(&results, &repo, top) {
        Ok(a) => a,
        Err(e) => return ToolOutput::crash(e),
    };
    let target = resp
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or(&symbol)
        .to_string();
    let omitted = resp.get("results_omitted").and_then(Value::as_i64).unwrap_or(0);

    let mut findings: Vec<HazardFinding> = Vec::new();
    let mut warn: Option<String> = None;
    let mut level = "resident";
    if direction == "callers" || force_hazard {
        match hazard_stage(
            &symbol,
            &repo,
            &direction,
            agg.total_prod,
            agg.total_test,
            &results,
            force_hazard,
        ) {
            Ok((f, w, l)) => {
                findings = f;
                warn = w;
                level = l;
            }
            Err(e) => return ToolOutput::crash(e),
        }
    }

    if as_json {
        let mut body = to_json_py_compact(&json_payload(
            &symbol,
            &target,
            &direction,
            &agg,
            &findings,
            warn.as_deref(),
            omitted,
            level,
        ));
        body.push('\n'); // Python print()
        return ToolOutput {
            stdout: body,
            stderr: String::new(),
            exit_code: 0,
        };
    }

    let mut stdout = String::new();
    stdout.push_str(&format!(
        "[OK] {direction} of {target}: {} prod / {} test refs（omitted {omitted}，excluded {}，outside {}）\n",
        agg.total_prod, agg.total_test, agg.excluded, agg.outside
    ));
    stdout.push_str("prod:\n");
    for (d, n) in &agg.prod {
        stdout.push_str(&format!("  {d} ({n})\n"));
    }
    stdout.push_str("test:\n");
    for (d, n) in &agg.test {
        stdout.push_str(&format!("  {d} ({n})\n"));
    }
    if !findings.is_empty() {
        stdout.push_str(&format!("⚠ {} dynamic hazards:\n", findings.len()));
        for f in &findings {
            stdout.push_str(&format!("  [{}] {}\n", f.kind, f.summary));
            for ev in f.evidence.iter().take(3) {
                stdout.push_str(&format!("      {ev}\n"));
            }
        }
    }
    if let Some(w) = &warn {
        stdout.push_str(w);
        stdout.push('\n');
    }
    stdout.push_str(
        "[WARN] 註腳：CRG（Tree-sitter）缺 instance-attr 邊（R2）——跨檔 self._x.method() 呼叫不在本清單；邊真相用 LSP findReferences\n",
    );
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}
