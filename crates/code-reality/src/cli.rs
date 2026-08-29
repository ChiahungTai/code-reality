//! cli — argument surface and mode routing (assembly layer). The bin is a
//! thin print-and-exit shim over [`run`].
//!
//! Check/route order mirrors the frozen Python main() exactly
//! (scip_refs.py:764-831): mutex family → audit guards (before index
//! resolution) → index/slot resolution → index existence check (with
//! legacy-slot migration hint) → mode routing (stamp → build-cache →
//! audit/query final guard). `--audit` runs the graph_audit first pass
//! in-process (D7) and reconciles the missing list against SCIP refs.
//!
//! Arg parsing mimics argparse (allow_abbrev / negative-number positionals /
//! `--` separator / last-wins repeats / lone `-` positional) — the frozen
//! CLI contract the R7 relay will inherit.

use crate::argparse::looks_like_option;
use crate::cache::{open_face, Face};
use crate::engine::{
    default_index_path, expand_home, git_head, load_index, meta_path, source_line, utc_now_iso,
};
use crate::ToolOutput;
use scip::types::Index;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// defs map + refs map from a query face.
type QueryResults = (BTreeMap<String, Vec<String>>, HashMap<String, Vec<String>>);

const FLAGS: [&str; 9] = [
    "--index",
    "--repo",
    "--stamp-meta",
    "--build-cache",
    "--help",
    "--callers",
    "--closure",
    "--depth",
    "--audit",
];

struct Args {
    query: Option<String>,
    index: Option<String>,
    repo: Option<String>,
    stamp_meta: bool,
    build_cache: bool,
    callers: bool,
    closure: bool,
    depth: Option<String>,
    audit: bool,
}

/// argparse-style parse of the tokens AFTER the subcommand.
enum Parsed {
    Args(Args),
    Help,
    Fail(String),
}

fn parse_tokens(toks: &[&str]) -> Parsed {
    let mut args = Args {
        query: None,
        index: None,
        repo: None,
        stamp_meta: false,
        build_cache: false,
        callers: false,
        closure: false,
        depth: None,
        audit: false,
    };
    let mut positional_only = false;
    let mut i = 0usize;
    macro_rules! fail {
        ($msg:expr) => {{
            return Parsed::Fail($msg);
        }};
    }
    while i < toks.len() {
        let tok = toks[i];
        if positional_only {
            if args.query.is_none() {
                args.query = Some(tok.to_string());
            } else {
                fail!(format!("unrecognized arguments: {}", tok));
            }
            i += 1;
            continue;
        }
        if tok == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if tok == "--help" || tok == "-h" {
            return Parsed::Help;
        }
        let (flag, inline_val) = if let Some(rest) = tok.strip_prefix("--") {
            match rest.split_once('=') {
                Some((name, val)) => (format!("--{}", name), Some(val.to_string())),
                None => (tok.to_string(), None),
            }
        } else if looks_like_option(tok) {
            fail!(format!("unrecognized arguments: {}", tok));
        } else {
            // positional: bare "-", negative numbers, plain/spacey tokens
            if args.query.is_none() {
                args.query = Some(tok.to_string());
            } else {
                fail!(format!("unrecognized arguments: {}", tok));
            }
            i += 1;
            continue;
        };
        // long flag resolution with abbreviation inference (allow_abbrev)
        let resolved = if FLAGS.contains(&flag.as_str()) {
            Some(flag)
        } else {
            let prefix = &flag[2..];
            let matches: Vec<&str> = FLAGS
                .iter()
                .filter(|f| f[2..].starts_with(prefix))
                .copied()
                .collect();
            match matches.len() {
                1 => Some(matches[0].to_string()),
                0 => fail!(format!("unrecognized arguments: {}", tok)),
                _ => fail!(format!("argument {}: ambiguous option", tok)),
            }
        };
        match resolved.unwrap().as_str() {
            "--help" => return Parsed::Help,
            "--stamp-meta" => {
                if let Some(v) = inline_val {
                    fail!(format!("ignored explicit argument '{}'", v));
                }
                args.stamp_meta = true;
                i += 1;
            }
            "--build-cache" => {
                if let Some(v) = inline_val {
                    fail!(format!("ignored explicit argument '{}'", v));
                }
                args.build_cache = true;
                i += 1;
            }
            "--callers" => {
                if let Some(v) = inline_val {
                    fail!(format!("ignored explicit argument '{}'", v));
                }
                args.callers = true;
                i += 1;
            }
            "--closure" => {
                if let Some(v) = inline_val {
                    fail!(format!("ignored explicit argument '{}'", v));
                }
                args.closure = true;
                i += 1;
            }
            "--audit" => {
                if let Some(v) = inline_val {
                    fail!(format!("ignored explicit argument '{}'", v));
                }
                args.audit = true;
                i += 1;
            }
            name => {
                // --index / --repo / --depth: value inline or next token
                // (last wins); option-looking next tokens are refused
                let val = match inline_val.clone() {
                    Some(v) => v,
                    None => match toks.get(i + 1) {
                        Some(v) if !looks_like_option(v) => {
                            i += 1;
                            v.to_string()
                        }
                        _ => fail!(format!("argument {}: expected one argument", name)),
                    },
                };
                match name {
                    "--index" => args.index = Some(val),
                    "--repo" => args.repo = Some(val),
                    _ => args.depth = Some(val),
                }
                i += 1;
            }
        }
    }
    Parsed::Args(args)
}

/// Route a `code-reality scip_refs ...` invocation. Returns ToolOutput
/// (stdout bytes + stderr bytes + exit code); never prints, never exits.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 scip_refs");
    };
    let args = match parse_tokens(toks) {
        Parsed::Args(args) => args,
        Parsed::Help => {
            return ToolOutput {
                stdout: concat!(
                    "usage: scip_refs [-h] [--index INDEX] [--audit] [--repo REPO]\n",
                    "                 [--stamp-meta] [--build-cache]\n",
                    "                 [--callers | --closure [--depth N]] [query]\n"
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Parsed::Fail(msg) => return ToolOutput::fail(msg),
    };
    let mut stderr = String::new();

    // Mutex family (Python order, scip_refs.py:764-790). The R3 query modes
    // join the existing conditions; the audit guards run BEFORE index
    // resolution (:785-790 — do not break the mirror order).
    if args.build_cache
        && (args.stamp_meta || args.audit || args.query.is_some() || args.callers || args.closure)
    {
        return ToolOutput::fail("--build-cache 與 --stamp-meta/--audit/查詢互斥");
    }
    if args.stamp_meta && (args.audit || args.query.is_some() || args.callers || args.closure) {
        return ToolOutput::fail("--stamp-meta 與 --audit/查詢互斥");
    }
    if args.stamp_meta && args.repo.is_none() {
        return ToolOutput::fail("--stamp-meta 需 --repo");
    }
    if args.audit && args.query.is_some() {
        return ToolOutput::fail("--audit 與查詢字串互斥");
    }
    if args.audit && args.repo.is_none() {
        return ToolOutput::fail("--audit 需 --repo（graph_audit 目標）");
    }
    // R3 mode guards
    if args.callers && args.closure {
        return ToolOutput::fail("--callers 與 --closure 互斥");
    }
    if args.depth.is_some() && !args.closure {
        return ToolOutput::fail("--depth 僅伴 --closure 使用");
    }
    // Upper bound: closure has no empty-frontier early exit (the stable
    // output shape prints every level), so an unbounded depth is a
    // user-reachable unbounded loop (adversarial review, 2026-08-25).
    const MAX_CLOSURE_DEPTH: usize = 10_000;
    let depth: Option<usize> = match &args.depth {
        None => None,
        Some(v) => match v.parse::<usize>() {
            Ok(n) if (1..=MAX_CLOSURE_DEPTH).contains(&n) => Some(n),
            _ => {
                return ToolOutput::fail(format!(
                    "--depth 需正整數（1-{}）：{}",
                    MAX_CLOSURE_DEPTH, v
                ))
            }
        },
    };

    // Index / slot resolution
    let (index_path, default_resolved) = match args.index {
        Some(p) => (PathBuf::from(p), false),
        None => match &args.repo {
            None => {
                return ToolOutput::fail("需 --index（或 --repo 解析 in-repo 預設 slot）");
            }
            Some(repo) => match default_index_path(Path::new(repo)) {
                Ok(p) => (p, true),
                Err(msg) => return ToolOutput::fail(msg),
            },
        },
    };

    // Existence check BEFORE mode routing (stamping a missing index fails)
    if !index_path.exists() {
        let mut msg = if default_resolved {
            format!(
                "預設索引不在：{}（--repo {} → in-repo slot；生成命令見 docstring）",
                index_path.display(),
                args.repo.as_deref().unwrap_or_default()
            )
        } else {
            format!("索引不在：{}", index_path.display())
        };
        if default_resolved {
            // Transitional bridge (data-plane unification SM-9): the
            // retired home sidecar keyed by basename — point its
            // orphans at migration instead of the dead global-slot hint.
            let repo = Path::new(args.repo.as_deref().unwrap_or_default());
            let home = expand_home(crate::sidecar_migrate::RETIRED_HOME_ROOT);
            if let Some(old) = crate::sidecar_migrate::old_slot_index(&home, repo) {
                msg.push_str(&format!(
                    "\n  偵測到舊 home slot（{}）——先跑 code-reality sidecar_migrate --repo {}",
                    old.display(),
                    args.repo.as_deref().unwrap_or_default()
                ));
            }
        }
        return ToolOutput::fail(msg.trim_end());
    }

    // Write modes of the manual rust flow (stamp / build-cache) ensure
    // the data-dir self-ignore on the DEFAULT slot — the capability's
    // "zero consumer gitignore setup" covers this flow too. An explicit
    // --index target is the user's choice and is never touched.
    if default_resolved && (args.stamp_meta || args.build_cache) {
        if let Some(root) = index_path.parent().and_then(|p| p.parent()) {
            if let Err(e) = crate::engine::write_data_dir_gitignore(root) {
                return ToolOutput::crash(format!("資料目錄自帶 ignore 失敗：{e}"));
            }
        }
    }

    if args.stamp_meta {
        return stamp_meta_mode(&index_path, Path::new(args.repo.as_deref().unwrap()));
    }
    if args.build_cache {
        return build_cache_mode(&index_path, &mut stderr);
    }

    // Final guard (Python :825 truthiness — an empty-string query bypasses
    // the mutex checks and lands here; audit satisfies the requirement)
    if !args.audit {
        match args.query.as_deref() {
            None | Some("") => {
                return ToolOutput::fail("需提供查詢或 --audit");
            }
            Some(_) => {}
        }
    }

    let repo = args.repo.as_deref().map(Path::new);

    if args.audit {
        // src_line precedes the mode branch (Python :828)
        let (src_line, src_warns) = source_line(&index_path, repo);
        stderr.push_str(&src_warns.concat());
        let repo = repo.expect("audit guard guarantees --repo");
        return audit_mode(&index_path, repo, src_line, &mut stderr);
    }

    let query = args.query.as_deref().unwrap_or("");

    if args.callers {
        return callers_mode(&index_path, repo, query, &mut stderr);
    }
    if args.closure {
        const DEFAULT_CLOSURE_DEPTH: usize = 2; // EP S3 要點 (CLI contract)
        return closure_mode(
            &index_path,
            repo,
            query,
            depth.unwrap_or(DEFAULT_CLOSURE_DEPTH),
            &mut stderr,
        );
    }

    let (src_line, src_warns) = source_line(&index_path, repo);
    stderr.push_str(&src_warns.concat());

    let parsed = crate::engine::Query::parse(query);
    let (defs, refs) = match query_defs_refs(&index_path, &parsed, &mut stderr) {
        Ok(v) => v,
        Err(fail_msg) => return ToolOutput::fail(fail_msg),
    };
    let (stdout, exit_code) = crate::engine::report(&defs, &refs, src_line.as_deref(), query);
    ToolOutput {
        stdout,
        stderr,
        exit_code,
    }
}

/// Face selection + defs/refs with the documented divergence for a corrupt-
/// but-meta-intact db: the frozen Python crashes with an uncaught
/// sqlite3.OperationalError (exit 1, empty stdout); Rust WARNs on stderr and
/// falls back to the protobuf face so the query still answers correctly.
fn query_defs_refs(
    index_path: &Path,
    parsed: &crate::engine::Query,
    stderr: &mut String,
) -> Result<QueryResults, String> {
    let (face, face_stderr) = open_face(index_path)?;
    stderr.push_str(&face_stderr.concat());
    let defs = match face.defs(parsed) {
        Ok(d) => d,
        Err(e) => {
            stderr.push_str(&format!(
                "[WARN] 衍生 db 查詢失敗——本次查詢改走 protobuf 全量解析：{}\n",
                e
            ));
            return protobuf_answers(index_path, parsed);
        }
    };
    let set: BTreeSet<String> = defs.keys().cloned().collect();
    let refs = match face.refs(&set) {
        Ok(r) => r,
        Err(e) => {
            stderr.push_str(&format!(
                "[WARN] 衍生 db 查詢失敗——本次查詢改走 protobuf 全量解析：{}\n",
                e
            ));
            return protobuf_answers(index_path, parsed);
        }
    };
    Ok((defs, refs))
}

fn protobuf_answers(
    index_path: &Path,
    parsed: &crate::engine::Query,
) -> Result<QueryResults, String> {
    let loaded = load_index(index_path)?;
    let defs = crate::engine::find_defs(&loaded.index, parsed);
    let set: BTreeSet<String> = defs.keys().cloned().collect();
    let refs = crate::engine::find_refs(&loaded.index, &set);
    Ok((defs, refs))
}

/// --stamp-meta (scip_refs.py:696-722): idempotent version sidecar write.
fn stamp_meta_mode(index_path: &Path, repo: &Path) -> ToolOutput {
    let head = match git_head(repo) {
        Ok(head) => head,
        Err(warn) => {
            // Python prints the git-failure WARN line before the FAIL
            let mut out = ToolOutput::fail("取不到 repo HEAD——meta 未 stamp");
            out.stderr.insert_str(0, &warn);
            return out;
        }
    };
    let sidecar = meta_path(index_path);
    // Key order mirrors Python json.dumps of the dict: repo/head/stamped_at/tool
    // (serde_json preserve_order keeps insertion order).
    let payload = serde_json::json!({
        "repo": repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()).display().to_string(),
        "head": head,
        "stamped_at": utc_now_iso(),
        "tool": "code_reality.scip_refs",
    });
    let text = format!("{}\n", serde_json::to_string_pretty(&payload).unwrap());
    if let Err(e) = std::fs::write(&sidecar, &text) {
        return ToolOutput::fail(format!("sidecar 寫入失敗：{}", e));
    }
    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    ToolOutput {
        stdout: crate::msg_line(
            "OK",
            &format!(
                "meta stamped：{}（{} @ {}）",
                sidecar.display(),
                repo_name,
                head.chars().take(7).collect::<String>()
            ),
        ),
        stderr: String::new(),
        exit_code: 0,
    }
}

/// --build-cache (scip_refs.py:408-419). Build failure here IS exit 2 (only
/// the query-internal auto-rebuild falls back to protobuf). R3: also builds
/// the fn_defs sidecar — its message goes to stderr only (stdout bytes are
/// the frozen parity face); an explicit-build sidecar failure is likewise
/// exit 2 (same explicit-mode semantics as the three-table build).
fn build_cache_mode(index_path: &Path, stderr: &mut String) -> ToolOutput {
    let db_path = crate::cache::sqlite_path(index_path);
    let head = crate::engine::stamped_head(index_path);
    let loaded = match load_index(index_path) {
        Ok(loaded) => {
            stderr.push_str(&loaded.stderr);
            loaded
        }
        Err(msg) => return ToolOutput::fail(msg),
    };
    match crate::cache::build_db(&loaded.index, &db_path, &head) {
        Ok(stats) => {
            let fndefs_db = crate::fndefs::fndefs_path(index_path);
            match crate::fndefs::build_sidecar(&loaded.index, &fndefs_db, &head) {
                Ok((n, warns)) => {
                    for w in warns {
                        stderr.push_str(&w);
                    }
                    stderr.push_str(&crate::msg_line(
                        "OK",
                        &format!(
                            "fn_defs sidecar built：{}（{} spans）",
                            fndefs_db.display(),
                            n
                        ),
                    ));
                    ToolOutput {
                        stdout: crate::msg_line(
                            "OK",
                            &format!(
                                "cache built：{}（{} symbols/{} occurrences）",
                                db_path.display(),
                                stats.symbols,
                                stats.occurrences
                            ),
                        ),
                        stderr: {
                            // Frozen-stdout parity face: the new counter
                            // rides stderr (like the fn_defs sidecar
                            // line) — zero keeps stdout byte-identical.
                            if stats.docs_fully_filtered > 0 {
                                stderr.push_str(&crate::msg_line(
                                    "WARN",
                                    &format!(
                                        "{} docs fully filtered by fn-tail gate（class/variable-only——R2-3 設計行為，B8 列冊）",
                                        stats.docs_fully_filtered
                                    ),
                                ));
                            }
                            std::mem::take(stderr)
                        },
                        exit_code: 0,
                    }
                }
                Err(e) => ToolOutput::fail(format!(
                    "fn_defs sidecar 構建失敗：{}：{}",
                    fndefs_db.display(),
                    e.trim_end()
                )),
            }
        }
        Err(e) => ToolOutput::fail(format!(
            "衍生 db 構建失敗：{}：{}",
            db_path.display(),
            e.trim_end()
        )),
    }
}

// ---------- R3 caller-edge modes ----------

/// Rows oracle for the caller-edge modes: the active face, or a protobuf
/// index swapped in after a face error (WARN + fallback, mirroring
/// query_defs_refs). `Face::Protobuf` is normalized to `Index` on arrival —
/// the index then also feeds the spans ladder without a second decode.
enum RowsOracle {
    Face(Face),
    Index(Index),
}

impl RowsOracle {
    fn defs(&self, query: &crate::engine::Query) -> Result<BTreeMap<String, Vec<String>>, String> {
        match self {
            RowsOracle::Face(f) => f.defs(query),
            RowsOracle::Index(i) => Ok(crate::engine::find_defs(i, query)),
        }
    }

    fn rows(&self, symbols: &BTreeSet<String>) -> Result<Vec<(String, String, i64)>, String> {
        match self {
            RowsOracle::Face(f) => f.refs_rows(symbols),
            RowsOracle::Index(i) => Ok(crate::engine::refs_rows(i, symbols)),
        }
    }

    fn index(&self) -> Option<&Index> {
        match self {
            RowsOracle::Face(_) => None,
            RowsOracle::Index(i) => Some(i),
        }
    }
}

/// Resolve defs via the face ladder (face errors → WARN + protobuf
/// fallback) for the caller-edge modes.
fn resolve_for_callers(
    index_path: &Path,
    parsed: &crate::engine::Query,
    stderr: &mut String,
) -> Result<(BTreeMap<String, Vec<String>>, RowsOracle), String> {
    let (face, face_stderr) = open_face(index_path)?;
    stderr.push_str(&face_stderr.concat());
    let mut oracle = match face {
        Face::Protobuf { index } => RowsOracle::Index(index),
        Face::Sqlite(conn) => RowsOracle::Face(Face::Sqlite(conn)),
    };
    let defs = match oracle.defs(parsed) {
        Ok(d) => d,
        Err(e) => {
            stderr.push_str(&format!(
                "[WARN] 衍生 db 查詢失敗——本次查詢改走 protobuf 全量解析：{}\n",
                e
            ));
            let loaded = load_index(index_path)?;
            stderr.push_str(&loaded.stderr);
            let defs = crate::engine::find_defs(&loaded.index, parsed);
            oracle = RowsOracle::Index(loaded.index);
            defs
        }
    };
    Ok((defs, oracle))
}

/// Payload of [`callers_front`] on a DEF hit.
type CallersFront = (
    BTreeMap<String, Vec<String>>,
    RowsOracle,
    BTreeMap<String, Vec<crate::engine::FnSpan>>,
);

/// Shared front for both modes: [SRC] line, defs resolution, spans ladder.
/// Returns `(src_line, Some((defs, oracle, spans)))` on a DEF hit;
/// `(src_line, None)` on no DEF (caller emits `[WARN] 查無 DEF` exit 1).
fn callers_front(
    index_path: &Path,
    repo: Option<&Path>,
    query: &str,
    stderr: &mut String,
) -> Result<(Option<String>, Option<CallersFront>), String> {
    let (src_line, src_warns) = source_line(index_path, repo);
    stderr.push_str(&src_warns.concat());
    let parsed = crate::engine::Query::parse(query);
    let (defs, oracle) = resolve_for_callers(index_path, &parsed, stderr)?;
    if defs.is_empty() {
        return Ok((src_line, None));
    }
    let (spans, span_stderr) = crate::fndefs::spans_source(index_path, oracle.index())?;
    for l in span_stderr {
        stderr.push_str(&l);
    }
    Ok((src_line, Some((defs, oracle, spans))))
}

fn no_def_output(src_line: &Option<String>, query: &str, stderr: String) -> ToolOutput {
    let (stdout, exit_code) = crate::engine::no_def_lines(src_line.as_deref(), query);
    ToolOutput {
        stdout,
        stderr,
        exit_code,
    }
}

fn callers_mode(
    index_path: &Path,
    repo: Option<&Path>,
    query: &str,
    stderr: &mut String,
) -> ToolOutput {
    let (src_line, front) = match callers_front(index_path, repo, query, stderr) {
        Ok(v) => v,
        Err(msg) => return ToolOutput::fail(msg),
    };
    let Some((defs, oracle, spans)) = front else {
        return no_def_output(&src_line, query, std::mem::take(stderr));
    };
    let symbols: BTreeSet<String> = defs.keys().cloned().collect();
    let rows = match oracle.rows(&symbols) {
        Ok(r) => r,
        Err(e) => {
            stderr.push_str(&format!(
                "[WARN] 衍生 db 查詢失敗——本次查詢改走 protobuf 全量解析：{}\n",
                e
            ));
            let loaded = match load_index(index_path) {
                Ok(l) => l,
                Err(msg) => return ToolOutput::fail(msg),
            };
            stderr.push_str(&loaded.stderr);
            crate::engine::refs_rows(&loaded.index, &symbols)
        }
    };
    let result = crate::callers::attribute(&rows, &spans);
    let (stdout, exit_code) = crate::callers::format_callers(query, &result, src_line.as_deref());
    ToolOutput {
        stdout,
        stderr: std::mem::take(stderr),
        exit_code,
    }
}

fn closure_mode(
    index_path: &Path,
    repo: Option<&Path>,
    query: &str,
    depth: usize,
    stderr: &mut String,
) -> ToolOutput {
    let (src_line, front) = match callers_front(index_path, repo, query, stderr) {
        Ok(v) => v,
        Err(msg) => return ToolOutput::fail(msg),
    };
    let Some((defs, oracle, spans)) = front else {
        return no_def_output(&src_line, query, std::mem::take(stderr));
    };
    let seeds: Vec<String> = defs.keys().cloned().collect();
    // Mid-BFS face errors: count skipped symbols, WARN once with the count
    // (degraded frontier), never a silent empty answer without a trace.
    let mut bfs_skipped: usize = 0;
    let mut bfs_first_err: Option<String> = None;
    let expand = |sym: &str| -> crate::callers::CallersResult {
        let set: BTreeSet<String> = std::iter::once(sym.to_string()).collect();
        match oracle.rows(&set) {
            Ok(rows) => crate::callers::attribute(&rows, &spans),
            Err(e) => {
                if bfs_first_err.is_none() {
                    bfs_first_err = Some(e);
                }
                bfs_skipped += 1;
                crate::callers::attribute(&[], &spans)
            }
        }
    };
    let result = crate::callers::closure(&seeds, expand, depth);
    if bfs_skipped > 0 {
        stderr.push_str(&format!(
            "[WARN] 衍生 db 查詢失敗——closure 層級擴展跳過 {} 符號（首例：{}）\n",
            bfs_skipped,
            bfs_first_err.as_deref().unwrap_or_default()
        ));
    }
    let (stdout, exit_code) =
        crate::callers::format_closure(query, depth, &result, src_line.as_deref());
    ToolOutput {
        stdout,
        stderr: std::mem::take(stderr),
        exit_code,
    }
}

// ---------- R4 audit mode ----------

/// `--audit` two-pass mode (scip_refs.py:254-316), first pass in-process
/// (D7): the Python subprocess shell was its frozen implementation detail;
/// the 600s total budget and empty-stdout/JSON guards were subprocess-
/// boundary artifacts (accepted deviation — env gates and the vacuous
/// all-zero guard survive, per-file 60s ra timeouts remain).
fn audit_mode(
    index_path: &Path,
    repo: &Path,
    src_line: Option<String>,
    stderr: &mut String,
) -> ToolOutput {
    let repo = crate::common::resolve(repo);
    let graph = crate::graph_db::db_path(&repo);
    // graph_audit main()'s env gates, surfaced through the wrapper's
    // "環境錯誤" face (Python: stderr passthrough, exit 2); the inner
    // texts are the shared gate constants (single source, no string copy)
    let gates = crate::graph_audit::env_gate_messages();
    if crate::graph_audit::which("rust-analyzer").is_none() {
        return ToolOutput::fail(format!("graph_audit 環境錯誤：[FAIL] {}", gates[0]));
    }
    if !graph.exists() {
        return ToolOutput::fail(format!(
            "graph_audit 環境錯誤：[FAIL] {}",
            gates[1].replace("{graph}", &graph.display().to_string())
        ));
    }
    let (_risk, audited, missing, errors, total_ra, _warns) =
        match crate::graph_audit::audit(&repo, &graph, false, None) {
            Ok(v) => v,
            Err(e) => {
                // Python: graph_audit crash (exit 1, empty stdout) surfaces
                // as 異常退出 exit 2 here
                return ToolOutput::fail(format!("graph_audit 異常退出：{e}"));
            }
        };
    if audited > 0 && total_ra == 0 {
        return ToolOutput::fail(format!("graph_audit 環境錯誤：[FAIL] {}", gates[2]));
    }
    for e in &errors {
        stderr.push_str(&format!("[WARN] {e}\n"));
    }

    let mut stdout = String::new();
    if let Some(src) = &src_line {
        stdout.push_str(src);
        stdout.push('\n');
    }
    stdout.push_str(&format!(
        "[OK] graph_audit 缺差 {} 項 → 逐項 SCIP refs 對照：\n",
        missing.len()
    ));
    let (face, face_stderr) = match open_face(index_path) {
        Ok(v) => v,
        Err(msg) => return ToolOutput::fail(msg),
    };
    stderr.push_str(&face_stderr.concat());

    // files_by_name (name → defining rel paths); repo-external paths warn
    // loudly — a false "0 refs" must never mix in silently
    let mut files_by_name: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut rels: Vec<(String, crate::graph_audit::MissingItem)> = Vec::new();
    for m in &missing {
        let p = Path::new(&m.file);
        let rel = match p.strip_prefix(&repo) {
            Ok(r) => r.to_string_lossy().into_owned(),
            Err(_) => {
                stderr.push_str(&format!(
                    "[WARN] 缺差項路徑不在 repo 下（歸屬失敗，refs 將報 0）：{}\n",
                    p.display()
                ));
                p.to_string_lossy().replace('\\', "/")
            }
        };
        files_by_name
            .entry(m.symbol.clone())
            .or_default()
            .insert(rel.clone());
        rels.push((rel, m.clone()));
    }
    let target_symbols = match face.audit_targets(&files_by_name) {
        Ok(t) => t,
        Err(e) => return ToolOutput::fail(e),
    };
    let keys: BTreeSet<String> = target_symbols.keys().cloned().collect();
    let refs_count = match face.refs(&keys) {
        Ok(r) => r,
        Err(e) => return ToolOutput::fail(e),
    };
    let mut with_refs = 0usize;
    for (rel, m) in &rels {
        let n: usize = target_symbols
            .iter()
            .filter(|(_, (d_file, d_name))| d_file == rel && d_name == &m.symbol)
            .map(|(sym, _)| refs_count.get(sym).map(|v| v.len()).unwrap_or(0))
            .sum();
        if n > 0 {
            with_refs += 1;
        }
        stdout.push_str(&format!(
            "  {}: {}({}/{}) → SCIP refs {}\n",
            rel, m.symbol, m.db_count, m.ra_count, n
        ));
    }
    stdout.push_str(&format!(
        "[OK] {}/{} 項在 SCIP 有 refs（非零 callers）\n",
        with_refs,
        missing.len()
    ));
    ToolOutput {
        stdout,
        stderr: std::mem::take(stderr),
        exit_code: 0,
    }
}
