//! `runtime_edges` — the frozen `code_reality/runtime_edges.py` contract:
//! viztracer trace JSON → runtime call-edge table. Scan-line nesting on
//! (pid, tid) groups with ts-interval containment; the giant
//! LLM-unconsumable trace compresses into an edge table (the runtime
//! source for evidence fusion).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{make_meta, to_json_indent1};
use crate::ToolOutput;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec { long: "--output", short: Some('o'), kind: Kind::Value { metavar: "OUTPUT" } },
        FlagSpec { long: "--top", short: None, kind: Kind::Value { metavar: "TOP" } },
        FlagSpec { long: "--include", short: None, kind: Kind::Value { metavar: "INCLUDE" } },
        FlagSpec { long: "--exclude", short: None, kind: Kind::Value { metavar: "EXCLUDE" } },
        // BooleanOptionalAction pair (default ON; --no-repo-only disables)
        FlagSpec { long: "--repo-only", short: None, kind: Kind::StoreTrue },
        FlagSpec { long: "--no-repo-only", short: None, kind: Kind::StoreTrue },
        FlagSpec { long: "--repo-root", short: None, kind: Kind::Value { metavar: "REPO_ROOT" } },
    ],
    positionals: &["trace"],
};

const HELP: &str = concat!(
    "usage: runtime_edges [-h] [-o OUTPUT] [--top TOP] [--include INCLUDE]\n",
    "                     [--exclude EXCLUDE] [--repo-only | --no-repo-only]\n",
    "                     [--repo-root REPO_ROOT] trace\n",
    "\n",
    "runtime edge 抽取器——viztracer trace JSON → runtime 呼叫邊表。\n",
    "\n",
    "positional arguments:\n",
    "  trace                 viztracer trace JSON 路徑\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  -o, --output OUTPUT\n                        輸出邊表 JSON（預設 <trace>.edges.json）\n",
    "  --top TOP             只輸出前 N 邊（0=全部）\n",
    "  --include INCLUDE     只留 caller/callee 含 substr 的邊\n",
    "  --exclude EXCLUDE     移除 caller/callee 含 substr 的邊\n",
    "  --repo-only, --no-repo-only\n",
    "                        只保留至少一端 path 在 repo 內的邊（預設開——濾 import 噪音）\n",
    "  --repo-root REPO_ROOT\n",
    "                        repo-only 判定根（預設 cwd）\n",
);

/// `fn (path:line)` → `fn` (`runtime_edges.py:42-44`).
pub fn qualname(name: &str) -> String {
    name.split(" (").next().unwrap_or(name).to_string()
}

fn path_suffix_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r" \((.+?):\d+\)$").unwrap())
}

/// `fn (path:line)` → `path`; no suffix (genexpr import noise) → None.
pub fn event_path(name: &str) -> Option<String> {
    path_suffix_re()
        .captures(name)
        .map(|m| m[1].to_string())
}

pub fn load_trace(path: &Path) -> Result<(Value, String), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{} stat 失敗：{}", path.display(), e))?
        .len();
    let mut warn = String::new();
    if size > 200 * 1024 * 1024 {
        warn.push_str(&format!(
            "[WARN] {} {}MB：json.load 中（數十秒級）\n",
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size / 1024 / 1024
        ));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{} 讀取失敗：{}", path.display(), e))?;
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} JSON 解析失敗：{}", path.display(), e))?;
    if !data.is_object() || data.get("traceEvents").is_none() {
        return Err(format!("非 viztracer 格式（缺 traceEvents）: {}", path.display()));
    }
    Ok((data, warn))
}

/// Scan-line nesting extraction (`runtime_edges.py:67-105`): per
/// (pid, tid) group — a bare-tid key would fabricate cross-process edges
/// when tids collide — sorted by (ts, -dur); the caller is the nearest
/// surviving ancestor whose interval contains the callee.
pub fn extract_edges(events: &[Value]) -> Result<Vec<(String, String, f64)>, String> {
    #[allow(clippy::type_complexity)] // (pid, tid) → (ts, dur, name) scan groups
    let mut by_tid: BTreeMap<(String, String), Vec<(i64, i64, String)>> = BTreeMap::new();
    for e in events {
        if e.get("cat").and_then(Value::as_str) == Some("fee")
            && e.get("ph").and_then(Value::as_str) == Some("X")
        {
            let Some(tid) = e.get("tid") else {
                return Err(format!(
                    "fee/X 事件缺 tid/dur 欄位（非 viztracer 慣例格式）: {}",
                    e.get("name").and_then(Value::as_str).unwrap_or("")
                ));
            };
            let Some(dur) = e.get("dur").and_then(Value::as_i64) else {
                return Err(format!(
                    "fee/X 事件缺 tid/dur 欄位（非 viztracer 慣例格式）: {}",
                    e.get("name").and_then(Value::as_str).unwrap_or("")
                ));
            };
            let pid = e
                .get("pid")
                .map(|p| match p {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let tid_key = match tid {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let name = e.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let ts = e.get("ts").and_then(Value::as_i64).unwrap_or(0);
            by_tid.entry((pid, tid_key)).or_default().push((ts, dur, name));
        }
    }
    if by_tid.values().all(|g| g.is_empty()) {
        return Err("無函式事件（trace 可能被 min_duration 全濾或非 viztracer 格式）".to_string());
    }
    let mut edges: Vec<(String, String, f64)> = Vec::new();
    for group in by_tid.values_mut() {
        group.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut stack: Vec<(i64, i64, &str)> = Vec::new(); // (ts, end, name)
        for (ts, dur, name) in group.iter() {
            let end = ts + dur;
            while let Some((_, top_end, _)) = stack.last() {
                if *top_end <= *ts {
                    stack.pop();
                } else {
                    break;
                }
            }
            if let Some((_, _, caller)) = stack.last() {
                edges.push((caller.to_string(), name.clone(), *dur as f64));
            }
            stack.push((*ts, end, name.as_str()));
        }
    }
    Ok(edges)
}

/// Keep edges with at least one endpoint path inside the repo
/// (`runtime_edges.py:108-129`); name-level cache — distinct names are
/// orders of magnitude fewer than edges on giant traces.
pub fn repo_only_filter(edges: &[(String, String, f64)], repo_root: &Path) -> Vec<(String, String, f64)> {
    let root = crate::common::resolve(repo_root);
    let mut cache: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let in_repo = |name: &str, cache: &mut std::collections::HashMap<String, bool>| -> bool {
        if let Some(v) = cache.get(name) {
            return *v;
        }
        let v = event_path(name)
            .map(|p| Path::new(&p).starts_with(&root))
            .unwrap_or(false);
        cache.insert(name.to_string(), v);
        v
    };
    edges
        .iter()
        .filter(|(c, f, _)| in_repo(c, &mut cache) || in_repo(f, &mut cache))
        .cloned()
        .collect()
}

/// Python `round(x, 2)` (round-half-even), for the ms faces.
fn py_round2(v: f64) -> f64 {
    let scaled = v * 100.0;
    let r = if (scaled - scaled.trunc()).abs() == 0.5 {
        // half → even
        let floor = scaled.floor();
        if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        scaled.round()
    };
    r / 100.0
}

/// Python `statistics.median`: odd n → middle; even n → mean of the two
/// middle values.
fn py_median(mut ds: Vec<f64>) -> f64 {
    ds.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = ds.len();
    if n % 2 == 1 {
        ds[n / 2]
    } else {
        (ds[n / 2 - 1] + ds[n / 2]) / 2.0
    }
}

/// (caller, callee) qualname aggregation → count/p50/p95 (callee dur,
/// ms; p95 nearest-rank with ceil — floor indexing takes max at
/// multiples of 20).
pub fn aggregate(edges: &[(String, String, f64)]) -> Vec<serde_json::Map<String, Value>> {
    let mut agg: Vec<((String, String), Vec<f64>)> = Vec::new();
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (caller, callee, dur) in edges {
        let key = (qualname(caller), qualname(callee));
        match index.get(&key) {
            Some(&i) => agg[i].1.push(*dur),
            None => {
                index.insert(key.clone(), agg.len());
                agg.push((key, vec![*dur]));
            }
        }
    }
    let mut rows: Vec<(serde_json::Map<String, Value>, i64)> = agg
        .into_iter()
        .map(|((c, f), mut ds)| {
            let count = ds.len() as i64;
            let p50 = py_median(ds.clone()) / 1000.0;
            ds.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let rank = (0.95 * ds.len() as f64).ceil() as usize;
            let p95 = ds[rank.saturating_sub(1)] / 1000.0;
            let mut row = serde_json::Map::new();
            row.insert("caller".into(), json!(c));
            row.insert("callee".into(), json!(f));
            row.insert("count".into(), json!(count));
            row.insert("p50_ms".into(), json!(py_round2(p50)));
            row.insert("p95_ms".into(), json!(py_round2(p95)));
            (row, count)
        })
        .collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    rows.into_iter().map(|(r, _)| r).collect()
}

fn filter_rows(
    rows: Vec<serde_json::Map<String, Value>>,
    include: Option<&str>,
    exclude: Option<&str>,
) -> Vec<serde_json::Map<String, Value>> {
    let contains = |r: &serde_json::Map<String, Value>, s: &str| {
        r.get("caller").and_then(Value::as_str).map(|v| v.contains(s)).unwrap_or(false)
            || r.get("callee").and_then(Value::as_str).map(|v| v.contains(s)).unwrap_or(false)
    };
    rows.into_iter()
        .filter(|r| include.map(|s| contains(r, s)).unwrap_or(true))
        .filter(|r| !exclude.map(|s| contains(r, s)).unwrap_or(false))
        .collect()
}

/// Route a `code-reality runtime_edges ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 runtime_edges");
    };
    let (values, positionals) = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput { stdout: HELP.to_string(), stderr: String::new(), exit_code: 0 };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, positionals } => (values, positionals),
    };
    let trace_path = PathBuf::from(&positionals[0]);
    let repo_root = values
        .get("--repo-root")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    // BooleanOptionalAction: default ON; --no-repo-only disables
    let repo_only = !values.contains_key("--no-repo-only");
    let top_s = values.get("--top").and_then(|v| v.clone()).unwrap_or_else(|| "0".into());
    let top: usize = match top_s.parse() {
        Ok(v) => v,
        Err(_) => {
            return ToolOutput::fail(format!("argument --top: invalid int value: '{top_s}'"));
        }
    };

    let (trace, warn) = match load_trace(&trace_path) {
        Ok(v) => v,
        Err(e) => return ToolOutput::crash(e),
    };
    let events: Vec<Value> = trace
        .get("traceEvents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let all_edges = match extract_edges(&events) {
        Ok(e) => e,
        Err(e) => return ToolOutput::crash(e),
    };
    let edges = if repo_only {
        repo_only_filter(&all_edges, &repo_root)
    } else {
        all_edges.clone()
    };
    let mut rows = filter_rows(
        aggregate(&edges),
        values.get("--include").and_then(|v| v.as_deref()),
        values.get("--exclude").and_then(|v| v.as_deref()),
    );
    if top > 0 {
        rows.truncate(top);
    }
    let pids: Vec<i64> = {
        let mut set: Vec<i64> = events
            .iter()
            .filter_map(|e| e.get("pid").and_then(Value::as_i64))
            .collect();
        set.sort();
        set.dedup();
        set
    };
    let out_path = values
        .get("--output")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let stem = trace_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            trace_path.with_file_name(format!("{stem}.edges.json"))
        });
    let meta = match make_meta(
        "code_reality.runtime_edges",
        &repo_root,
        None,
        vec![
            ("trace", json!(trace_path.to_string_lossy())),
            ("repo_only", json!(repo_only)),
            ("pids", json!(pids)),
            ("total_events", json!(events.len())),
            ("total_edges", json!(all_edges.len())),
        ],
    ) {
        Ok(m) => m,
        Err(e) => return ToolOutput::crash(e),
    };
    let out = json!({
        "_meta": Value::Object(meta),
        "edges": rows,
    });
    if let Err(e) = std::fs::write(&out_path, to_json_indent1(&out)) {
        return ToolOutput::crash(format!("{} 寫入失敗：{}", out_path.display(), e));
    }

    let mut stdout = warn;
    stdout.push_str(&format!(
        "[OK] {} edges from {} events -> {}\n",
        rows.len(),
        events.len(),
        out_path.display()
    ));
    // the frozen [LOG] line carries wall-clock timings (load/extract)
    // — dynamic in BOTH carriers; emitted for parity shape with zeros
    stdout.push_str(&format!(
        "[LOG] rg '\"callee\"' {} | head -20；load 0.0s / extract+agg 0.0s\n",
        out_path.display()
    ));
    if repo_only && !all_edges.is_empty() && edges.is_empty() {
        stdout.push_str(&format!(
            "[WARN] repo-only 濾除全部 {} 邊——trace 內 path 與 --repo-root（{}）不符？（從 repo root 執行或顯式指定）\n",
            all_edges.len(),
            repo_root.display()
        ));
    }
    for r in rows.iter().take(5) {
        stdout.push_str(&format!(
            "  top: {} -> {} x{} p50={}ms\n",
            r["caller"].as_str().unwrap_or(""),
            r["callee"].as_str().unwrap_or(""),
            r["count"],
            r["p50_ms"]
        ));
    }
    if pids.len() > 1 {
        stdout.push_str(&format!(
            "[WARN] 多進程 trace（pids={:?}）：邊按 (pid,tid) 分組，跨進程邊不在此列\n",
            pids
        ));
    }
    ToolOutput { stdout, stderr: String::new(), exit_code: 0 }
}
