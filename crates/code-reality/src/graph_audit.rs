//! `graph_audit` — the frozen `code_reality/graph_audit.py` contract:
//! D1 risk scan (same-name methods across ≥2 impl blocks) + D2
//! rust-analyzer reconciliation against graph.db nodes. Exit family
//! 0/1/2 (D3): 0 clean | 1 missing found | 2 environment error
//! (rust-analyzer absent / db missing / vacuous all-zero guard).
//! `--json` four keys (risk_files/audited_files/missing/errors) are the
//! governance-hook contract face — stdout bytes gated.

use crate::argparse::{parse, required, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{connect_ro, graph_db_path};
use crate::profile::{is_excluded, load_profile, scan_roots};
use crate::ToolOutput;
use rusqlite::Connection;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--all",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--graph",
            short: None,
            kind: Kind::Value { metavar: "GRAPH" },
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: graph_audit [-h] --repo REPO [--all] [--json] [--graph GRAPH]\n",
    "\n",
    "CRG graph.db Rust 完整度稽核\n",
    "\n",
    "options:\n",
    "  -h, --help     show this help message and exit\n",
    "  --repo REPO    掃描目標 repo 根\n",
    "  --all          對帳全部 .rs（預設僅風險檔）\n",
    "  --json         機器可讀輸出（治理鉤子契約）\n",
    "  --graph GRAPH  覆寫 graph.db 路徑（預設 <repo>/.code-review-graph/graph.db）\n",
);

fn impl_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"^\s*(?:unsafe\s+)?impl(?:<[^{]*>)?\s+(?:(?:\w+::)*\w+\s+for\s+)?((?:dyn\s+)?[A-Z]\w*)",
        )
        .unwrap()
    })
}

fn fn_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern(?:\s*"[^"]*")?\s+)*fn\s+(\w+)"#,
        )
        .unwrap()
    })
}

fn ra_kind_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"kind: SymbolKind\((\w+)\)"#).unwrap())
}

fn ra_label_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"label: "([^"]*)""#).unwrap())
}

const RA_FN_KINDS: [&str; 2] = ["Function", "Method"];

/// Insertion-ordered counter (Python `Counter` iteration semantics):
/// the `--json` `missing` array order = scope file order × first-seen
/// label order within each file (D4).
#[derive(Default)]
pub struct OrderedCounter {
    order: Vec<String>,
    counts: HashMap<String, usize>,
}

impl OrderedCounter {
    pub fn bump(&mut self, key: &str) {
        if !self.counts.contains_key(key) {
            self.order.push(key.to_string());
        }
        *self.counts.entry(key.to_string()).or_insert(0) += 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, usize)> {
        self.order
            .iter()
            .map(|k| (k.as_str(), self.counts[k]))
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Scan set (`graph_audit.py:68-79`): profile `[[scan_root]]` path globs;
/// without scan roots the whole repo `*.rs` via exclusions. pathlib glob
/// matches dotfiles — glob crate default `require_literal_leading_dot =
/// false` aligns (sorted + dedup as in Python's set collection).
/// Profile load errors propagate (crash-only — Python `graph_audit.py:71`
/// calls `load_profile` bare; swallowing would silently fall back to the
/// generic whole-repo scan on a broken TOML).
pub fn scan_files(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let profile = load_profile(repo)?;
    let roots = scan_roots(profile.as_ref());
    let mut out: Vec<PathBuf> = if !roots.is_empty() {
        let mut opts = glob::MatchOptions::new();
        opts.require_literal_leading_dot = false;
        roots
            .iter()
            .filter_map(|sr| {
                glob::glob_with(repo.join(&sr.path).to_string_lossy().as_ref(), opts).ok()
            })
            .flatten()
            .filter_map(|r| r.ok())
            .collect()
    } else {
        let mut opts = glob::MatchOptions::new();
        opts.require_literal_leading_dot = false;
        let pattern = repo.join("**/*.rs").to_string_lossy().into_owned();
        glob::glob_with(&pattern, opts)
            .into_iter()
            .flatten()
            .filter_map(|r| r.ok())
            .filter(|p| {
                p.strip_prefix(repo)
                    .map(|rel| !is_excluded(&rel.to_string_lossy(), profile.as_ref()))
                    .unwrap_or(true)
            })
            .collect()
    };
    out.sort();
    out.dedup();
    Ok(out)
}

/// D1 risk scan (`graph_audit.py:82-118`): per-block counting (NOT the
/// global intersection — single-method impls like Drop would empty it);
/// impl closure = `}` at indent ≤ the impl keyword's.
pub fn risk_scan(files: &[PathBuf]) -> Vec<(PathBuf, String, Vec<String>)> {
    let mut at_risk = Vec::new();
    for f in files {
        let text = std::fs::read(f)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        // impls: (type, [fn names], indent)
        let mut impls: Vec<(String, Vec<String>, usize)> = Vec::new();
        let mut cur: Option<usize> = None; // index into impls
        for line in text.split('\n') {
            if let Some(m) = impl_re().captures(line) {
                impls.push((
                    m[1].to_string(),
                    Vec::new(),
                    indent_of(line),
                ));
                cur = Some(impls.len() - 1);
                continue;
            }
            let Some(idx) = cur else { continue };
            let stripped = line.trim();
            if stripped == "}" {
                if indent_of(line) <= impls[idx].2 {
                    cur = None;
                }
                continue;
            }
            if let Some(fm) = fn_re().captures(line) {
                impls[idx].1.push(fm[1].to_string());
            }
        }
        // per-type aggregated block counts, insertion order preserved
        let mut block_counts: Vec<(String, OrderedCounter)> = Vec::new();
        for (t, names, _) in &impls {
            let entry = match block_counts.iter_mut().find(|(et, _)| et == t) {
                Some((_, c)) => c,
                None => {
                    block_counts.push((t.clone(), OrderedCounter::default()));
                    &mut block_counts.last_mut().unwrap().1
                }
            };
            let mut seen: std::collections::BTreeSet<&str> = Default::default();
            for n in names {
                if seen.insert(n) {
                    entry.bump(n);
                }
            }
        }
        for (t, counts) in &block_counts {
            let mut overlap: Vec<String> = counts
                .iter()
                .filter(|&(_, c)| c >= 2)
                .map(|(n, _)| n.to_string())
                .collect();
            overlap.sort(); // graph_audit.py:115 — sorted() on the overlap face
            if !overlap.is_empty() {
                at_risk.push((f.clone(), t.clone(), overlap));
            }
        }
    }
    at_risk
}

/// rust-analyzer `symbols` stdout → label counts (fn kinds only).
pub fn parse_ra_symbols(stdout_text: &str) -> OrderedCounter {
    let mut counts = OrderedCounter::default();
    for line in stdout_text.split('\n') {
        let Some(kind) = ra_kind_re().captures(line) else {
            continue;
        };
        if !RA_FN_KINDS.contains(&&kind[1]) {
            continue;
        }
        if let Some(label) = ra_label_re().captures(line) {
            counts.bump(&label[1]);
        }
    }
    counts
}

/// rust-analyzer stdin mode (`graph_audit.py:134-151`): the file is passed
/// as stdin (an opened fd — no writer side, so no pipe deadlock), stdout
/// parsed lossily, `check=False`. Timeout (60s) → `Ok(None)` (caller
/// records an error); spawn failure is the crash family (Python would
/// raise FileNotFoundError after the env gate).
pub fn ra_symbols(path: &Path) -> Result<Option<OrderedCounter>, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("{} 開啟失敗：{}", path.display(), e))?;
    let mut child = std::process::Command::new("rust-analyzer")
        .arg("symbols")
        .stdin(std::process::Stdio::from(file))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("rust-analyzer spawn 失敗：{}", e))?;
    // drain stdout concurrently so a large symbol list can't fill the pipe
    let mut out_pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        out_pipe.read_to_end(&mut buf).ok();
        buf
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Ok(st),
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => break Err(e),
        }
    };
    let buf = reader.join().unwrap_or_default();
    match status {
        Ok(_) => Ok(Some(parse_ra_symbols(&String::from_utf8_lossy(&buf)))),
        Err(e) => Err(format!("rust-analyzer wait 失敗：{}", e)),
    }
}

/// graph.db nodes per-name counts (`graph_audit.py:154-161`): DB side kind
/// must include 'Test' (missing it flags every test fn as a false gap).
pub fn db_functions(conn: &Connection, path: &Path) -> HashMap<String, usize> {
    let resolved = crate::common::resolve(path);
    let Ok(mut stmt) = conn.prepare(
        "SELECT name, COUNT(*) FROM nodes WHERE file_path = ?1 \
         AND kind IN ('Function', 'Test') GROUP BY name",
    ) else {
        return HashMap::new();
    };
    let Ok(rows) = stmt.query_map([resolved.to_string_lossy().as_ref()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    }) else {
        return HashMap::new();
    };
    rows.filter_map(|r| r.ok())
        .map(|(n, c)| (n, c as usize))
        .collect()
}

#[derive(Debug, Clone)]
pub struct MissingItem {
    pub file: String,
    pub symbol: String,
    pub ra_count: usize,
    pub db_count: usize,
}

/// Test injection point (mirrors the Python `ra_lookup` parameter).
pub type RaLookup<'a> = &'a dyn Fn(&Path) -> Result<Option<OrderedCounter>, String>;

/// D1+D2 main flow (`graph_audit.py:164-213`): returns
/// `(risk, audited_count, missing, errors, total_ra, stderr_warns)`.
/// The per-file vacuous WARN lines go to the returned stderr buffer (the
/// lib never prints); the aggregate all-zero guard lives in the CLI.
#[allow(clippy::type_complexity)] // mirror of the frozen Python tuple shape
pub fn audit(
    repo: &Path,
    graph: &Path,
    all_files: bool,
    ra_lookup: Option<RaLookup>,
) -> Result<
    (
        Vec<(PathBuf, String, Vec<String>)>,
        usize,
        Vec<MissingItem>,
        Vec<String>,
        usize,
        Vec<String>,
    ),
    String,
> {
    let default_lookup = |p: &Path| ra_symbols(p);
    let lookup = ra_lookup.unwrap_or(&default_lookup);
    let files = scan_files(repo)?;
    let risk = risk_scan(&files);
    let scope: Vec<PathBuf> = if all_files {
        files
    } else {
        let mut s: Vec<PathBuf> = risk.iter().map(|(f, _, _)| f.clone()).collect();
        s.sort();
        s.dedup();
        s
    };
    let conn = connect_ro(graph)?;
    let mut missing = Vec::new();
    let mut errors = Vec::new();
    let mut warns = Vec::new();
    let mut total_ra: usize = 0;
    for f in &scope {
        let ra = match lookup(f) {
            Ok(Some(ra)) => ra,
            Ok(None) => {
                errors.push(format!("{}: rust-analyzer 逾時/失敗（跳過）", f.display()));
                continue;
            }
            Err(e) => return Err(e),
        };
        total_ra += ra.total();
        let db = db_functions(&conn, f);
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        if ra.is_empty() && size > 0 {
            warns.push(format!(
                "[WARN] rust-analyzer 對 {} 零輸出（格式 drift 或單檔 parse fail）——該檔對帳 vacuous，勿當乾淨讀\n",
                f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ));
        }
        for (name, ra_count) in ra.iter() {
            let db_count = db.get(name).copied().unwrap_or(0);
            if db_count < ra_count {
                missing.push(MissingItem {
                    file: f.display().to_string(),
                    symbol: name.to_string(),
                    ra_count,
                    db_count,
                });
            }
        }
    }
    Ok((risk, scope.len(), missing, errors, total_ra, warns))
}

/// The three env-gate failure messages, shared verbatim by the
/// `graph_audit` CLI and the `scip_refs --audit` wrapper (copying the
/// strings would drift on the first edit).
pub(crate) fn env_gate_messages() -> [&'static str; 3] {
    [
        "rust-analyzer 不在 PATH——rustup component add rust-analyzer",
        "graph.db 不存在：{graph}（完整度稽核需要它；新鮮度指標不保證存在）",
        "全部檔案 rust-analyzer 符號數為 0——輸出格式漂移或環境錯誤",
    ]
}

pub(crate) fn which(bin: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| {
            std::fs::metadata(p)
                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
}

/// Route a `code-reality graph_audit ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 graph_audit");
    };
    let values = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, .. } => values,
    };
    let repo = match required(&values, "--repo") {
        Ok(r) => PathBuf::from(r),
        Err(msg) => return ToolOutput::fail(msg),
    };
    let all_files = values.contains_key("--all");
    let as_json = values.contains_key("--json");
    let mut graph = values
        .get("--graph")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| graph_db_path(&repo));
    if graph.as_os_str().is_empty() {
        // Python Path("") == Path(".") — exists() is true there, and the
        // failure lands in connect_ro (exit 1), not the env gate (exit 2)
        graph = PathBuf::from(".");
    }

    let mut stderr = String::new();
    let gates = env_gate_messages();
    if which("rust-analyzer").is_none() {
        stderr.push_str(&crate::msg_line("FAIL", gates[0]));
        return ToolOutput { stdout: String::new(), stderr, exit_code: 2 };
    }
    if !graph.exists() {
        stderr.push_str(&crate::msg_line(
            "FAIL",
            &gates[1].replace("{graph}", &graph.display().to_string()),
        ));
        return ToolOutput { stdout: String::new(), stderr, exit_code: 2 };
    }

    let (risk, audited, missing, errors, total_ra, warns) = match audit(&repo, &graph, all_files, None)
    {
        Ok(v) => v,
        Err(e) => {
            // Python crashes uncaught here (exit 1, empty stdout)
            let mut out = ToolOutput::crash(e);
            out.stderr.insert_str(0, &stderr);
            return out;
        }
    };
    for w in warns {
        stderr.push_str(&w);
    }
    // aggregate vacuous guard: all-zero symbols across an audited scope
    if audited > 0 && total_ra == 0 {
        stderr.push_str(&crate::msg_line("FAIL", gates[2]));
        return ToolOutput { stdout: String::new(), stderr, exit_code: 2 };
    }

    let stdout = if as_json {
        let risk_files: Vec<serde_json::Value> = risk
            .iter()
            .map(|(f, t, o)| {
                serde_json::json!({
                    "file": f.display().to_string(),
                    "type": t,
                    "overlap": o,
                })
            })
            .collect();
        let missing_v: Vec<serde_json::Value> = missing
            .iter()
            .map(|m| {
                serde_json::json!({
                    "file": m.file,
                    "symbol": m.symbol,
                    "ra_count": m.ra_count,
                    "db_count": m.db_count,
                })
            })
            .collect();
        let errors_v: Vec<&String> = errors.iter().collect();
        // Python `print(json.dumps(...))` — the trailing newline is part of
        // the stdout byte face
        let mut body = crate::common::to_json_indent1(&serde_json::json!({
            "risk_files": risk_files,
            "audited_files": audited,
            "missing": missing_v,
            "errors": errors_v,
        }));
        body.push('\n');
        body
    } else {
        let mut out = String::new();
        out.push_str(&format!(
            "[OK] D1 風險掃描：{} 檔（同名方法 ≥2 impl 塊）\n",
            risk.len()
        ));
        out.push_str(&format!(
            "[OK] D2 對帳：{} 檔（rust-analyzer vs graph.db，{} 符號）\n",
            audited, total_ra
        ));
        for e in &errors {
            out.push_str(&format!("[WARN] {e}\n"));
        }
        if !missing.is_empty() {
            out.push_str(&format!(
                "[WARN] DB 缺差 {} 項（同鍵去重吃掉——head_matches_build 不反映此項）：\n",
                missing.len()
            ));
            let mut by_file: Vec<(String, Vec<&MissingItem>)> = Vec::new();
            for m in &missing {
                match by_file.iter_mut().find(|(f, _)| f == &m.file) {
                    Some((_, items)) => items.push(m),
                    None => by_file.push((m.file.clone(), vec![m])),
                }
            }
            by_file.sort_by(|a, b| a.0.cmp(&b.0));
            for (f, items) in &by_file {
                let syms: Vec<String> = items
                    .iter()
                    .map(|m| format!("{}({}/{})", m.symbol, m.db_count, m.ra_count))
                    .collect();
                out.push_str(&format!("  {f}: {}\n", syms.join(", ")));
            }
        } else {
            out.push_str("[OK] 無缺差\n");
        }
        out
    };
    let exit_code = if missing.is_empty() { 0 } else { 1 };
    ToolOutput { stdout, stderr, exit_code }
}
