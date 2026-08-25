//! cli — argument surface and mode routing (assembly layer). The bin is a
//! thin print-and-exit shim over [`run`].
//!
//! Check/route order mirrors the frozen Python main() exactly
//! (scip_refs.py:764-831): mutex family → stamp needs --repo → index/slot
//! resolution → index existence check (with legacy-slot migration hint) →
//! mode routing (stamp → build-cache → query final guard). `--audit` is not
//! wired (R4 scope); the mutex and final-guard texts still carry the
//! verbatim Python strings (transient divergence documented in the EP).
//!
//! Arg parsing mimics argparse (allow_abbrev / negative-number positionals /
//! `--` separator / last-wins repeats / lone `-` positional) — the frozen
//! CLI contract the R7 relay will inherit.

use crate::cache::open_face;
use crate::engine::{
    default_index_path, expand_home, git_head, load_index, meta_path, source_line, utc_now_iso,
    DEFAULT_INDEX_ROOT,
};
use crate::ToolOutput;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// defs map + refs map from a query face.
type QueryResults = (
    BTreeMap<String, Vec<String>>,
    HashMap<String, Vec<String>>,
);

const FLAGS: [&str; 5] = ["--index", "--repo", "--stamp-meta", "--build-cache", "--help"];

/// Python 3.14 argparse negative-number test (empirically pinned against the
/// local oracle): `-` alone is positional; a `-`-prefixed token whose next
/// char is an optional `.` then an ASCII digit is a negative number (prefix
/// match — `-5x` and `-5.5.5` count); space-containing tokens are positional.
fn is_negative_numberish(tok: &str) -> bool {
    if tok == "-" || tok.contains(' ') {
        return true;
    }
    let Some(rest) = tok.strip_prefix('-') else {
        return false;
    };
    let rest = rest.strip_prefix('.').unwrap_or(rest);
    rest.starts_with(|c: char| c.is_ascii_digit())
}

/// Option-looking token (for flag-value refusal): `-`-prefixed, longer than
/// `-`, and not a negative number. Python refuses these as flag values
/// (`argument X: expected one argument`), including exactly `--`.
fn looks_like_option(tok: &str) -> bool {
    tok.starts_with('-') && tok != "-" && !is_negative_numberish(tok)
}

struct Args {
    query: Option<String>,
    index: Option<String>,
    repo: Option<String>,
    stamp_meta: bool,
    build_cache: bool,
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
            name => {
                // --index / --repo: value inline or next token (last wins);
                // option-looking next tokens are refused (argparse parity)
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
                if name == "--index" {
                    args.index = Some(val);
                } else {
                    args.repo = Some(val);
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
                    "                 [--stamp-meta] [--build-cache] [query]\n"
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Parsed::Fail(msg) => return ToolOutput::fail(msg),
    };
    let mut stderr = String::new();

    // Mutex family (Python order; audit rules belong to R4)
    if args.build_cache && (args.stamp_meta || args.query.is_some()) {
        return ToolOutput::fail("--build-cache 與 --stamp-meta/--audit/查詢互斥");
    }
    if args.stamp_meta && args.query.is_some() {
        return ToolOutput::fail("--stamp-meta 與 --audit/查詢互斥");
    }
    if args.stamp_meta && args.repo.is_none() {
        return ToolOutput::fail("--stamp-meta 需 --repo");
    }

    // Index / slot resolution
    let (index_path, default_resolved) = match args.index {
        Some(p) => (PathBuf::from(p), false),
        None => match &args.repo {
            None => {
                return ToolOutput::fail("需 --index（或 --repo 解析 repo-keyed 預設 slot）");
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
                "預設索引不在：{}（--repo {} → repo-keyed slot；生成命令或搬遷見 docstring）",
                index_path.display(),
                args.repo.as_deref().unwrap_or_default()
            )
        } else {
            format!("索引不在：{}", index_path.display())
        };
        if default_resolved {
            let legacy = expand_home(DEFAULT_INDEX_ROOT).join("index.scip");
            if legacy.exists() {
                msg.push_str(&format!(
                    "\n  既有全局 slot 索引可搬遷（免重生成；僅當該索引生成自 --repo 指定的 repo——搬錯 repo 的索引會全域查無）：mkdir -p {} && mv {} {}/",
                    index_path.parent().unwrap().display(),
                    legacy.display(),
                    index_path.parent().unwrap().display()
                ));
            }
        }
        return ToolOutput::fail(msg.trim_end());
    }

    if args.stamp_meta {
        return stamp_meta_mode(&index_path, Path::new(args.repo.as_deref().unwrap()));
    }
    if args.build_cache {
        return build_cache_mode(&index_path, &mut stderr);
    }

    // Query final guard: `is None or empty` (Python truthiness at :825 — an
    // empty-string query bypasses the mutex checks and lands here)
    let query = match args.query.as_deref() {
        None | Some("") => {
            return ToolOutput::fail("需提供查詢或 --audit");
        }
        Some(q) => q,
    };

    let repo = args.repo.as_deref().map(Path::new);
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
/// the query-internal auto-rebuild falls back to protobuf).
fn build_cache_mode(index_path: &Path, stderr: &mut String) -> ToolOutput {
    let db_path = crate::cache::sqlite_path(index_path);
    let head = crate::engine::load_meta(index_path)
        .0
        .and_then(|m| m["head"].as_str().map(str::to_string))
        .unwrap_or_default();
    let result = match load_index(index_path) {
        Ok(loaded) => {
            stderr.push_str(&loaded.stderr);
            crate::cache::build_db(&loaded.index, &db_path, &head)
        }
        Err(msg) => Err(msg),
    };
    match result {
        Ok(stats) => ToolOutput {
            stdout: crate::msg_line(
                "OK",
                &format!(
                    "cache built：{}（{} symbols/{} occurrences）",
                    db_path.display(),
                    stats.symbols,
                    stats.occurrences
                ),
            ),
            stderr: std::mem::take(stderr),
            exit_code: 0,
        },
        Err(e) => ToolOutput::fail(format!(
            "衍生 db 構建失敗：{}：{}",
            db_path.display(),
            e.trim_end()
        )),
    }
}
