//! `transition` — the frozen `code_reality/transition.py` contract
//! (post canonical-sync 281e07e): snapshot pair set-diff (B1 reversed =
//! added direction), EP claims comparison (regex ∪ relative path-token
//! normalization with repo-root existence verification), md/json dual
//! output. stdout faces (byte gate): the profile-less WARN, `[OK]`
//! transition line, baseline `[LOG]`, tail `[LOG]`.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{to_json_indent1, utc_now_iso_micros};
use crate::profile::{claims_regex, load_profile, module_of, Profile};
use crate::ToolOutput;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--ep",
            short: None,
            kind: Kind::Value { metavar: "EP" },
        },
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--output-prefix",
            short: Some('o'),
            kind: Kind::Value {
                metavar: "OUTPUT_PREFIX",
            },
        },
    ],
    positionals: &["snapshot_a", "snapshot_b"],
};

const HELP: &str = concat!(
    "usage: transition [-h] [--ep EP] [--repo REPO]\n",
    "                  [-o OUTPUT_PREFIX]\n",
    "                  snapshot_a snapshot_b\n",
    "\n",
    "transition diff——兩 snapshot module-edge 集差異＋「EP 宣稱 vs 實際」對照。\n",
    "\n",
    "positional arguments:\n",
    "  snapshot_a            before snapshot（S2 schema）\n",
    "  snapshot_b            after snapshot（S2 schema）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --ep EP               EP markdown（宣稱模組對照）\n",
    "  --repo REPO           repo 根（profile 載入——module/claims 規則來源）\n",
    "  -o, --output-prefix OUTPUT_PREFIX\n",
    "                        輸出前綴（預設 transition-<a>..<b>）\n",
);

pub type Edge = (String, String, String);

#[derive(Debug, Clone)]
pub struct LoadedSnapshot {
    pub path: PathBuf,
    pub meta: Map<String, Value>,
    pub files: BTreeSet<String>,
    pub module_edges: BTreeSet<Edge>,
}

pub fn load_snapshot(path: &Path) -> Result<LoadedSnapshot, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{} 讀取失敗：{}", path.display(), e))?;
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("非 S2 snapshot 格式（缺 _meta/module_edges）: {}（{e}）", path.display()))?;
    let obj = data.as_object().ok_or_else(|| {
        format!("非 S2 snapshot 格式（缺 _meta/module_edges）: {}", path.display())
    })?;
    if !obj.contains_key("_meta") || !obj.contains_key("module_edges") {
        return Err(format!(
            "非 S2 snapshot 格式（缺 _meta/module_edges）: {}",
            path.display()
        ));
    }
    let edges_raw = obj["module_edges"].as_array().ok_or_else(|| {
        format!("module_edges 元素非 [src, dst, kind] 三元組: {}", path.display())
    })?;
    let mut module_edges = BTreeSet::new();
    for e in edges_raw {
        let arr = e.as_array().ok_or_else(|| {
            format!("module_edges 元素非 [src, dst, kind] 三元組: {}", path.display())
        })?;
        if arr.len() != 3 || !arr.iter().all(Value::is_string) {
            return Err(format!(
                "module_edges 元素非 [src, dst, kind] 三元組: {}",
                path.display()
            ));
        }
        module_edges.insert((
            arr[0].as_str().unwrap().to_string(),
            arr[1].as_str().unwrap().to_string(),
            arr[2].as_str().unwrap().to_string(),
        ));
    }
    let files: BTreeSet<String> = obj
        .get("files")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Ok(LoadedSnapshot {
        path: path.to_path_buf(),
        meta: obj["_meta"].as_object().cloned().unwrap_or_default(),
        files,
        module_edges,
    })
}

pub struct EdgeDiff {
    pub added: Vec<Edge>,
    pub removed: Vec<Edge>,
    /// always added-direction (B1)
    pub reversed: Vec<(String, String)>,
    pub changed_modules: BTreeSet<String>,
}

/// Pair set-diff (`transition.py:72-86`): the tuple-diff projection ≠
/// pair set-diff under multi-kind duplicates — the pair projection is
/// the correct reversal test.
pub fn diff_edges(a: &BTreeSet<Edge>, b: &BTreeSet<Edge>) -> EdgeDiff {
    let removed: BTreeSet<&Edge> = a.difference(b).collect();
    let added: BTreeSet<&Edge> = b.difference(a).collect();
    let removed_pairs: BTreeSet<(&str, &str)> =
        removed.iter().map(|(s, d, _)| (s.as_str(), d.as_str())).collect();
    let added_pairs: BTreeSet<(&str, &str)> =
        added.iter().map(|(s, d, _)| (s.as_str(), d.as_str())).collect();
    let mut reversed: Vec<(String, String)> = added_pairs
        .intersection(&removed_pairs.iter().map(|&(s, d)| (d, s)).collect())
        .map(|&(s, d)| (s.to_string(), d.to_string()))
        .collect();
    reversed.sort();
    let mut changed = BTreeSet::new();
    for (s, d) in removed_pairs.iter().chain(added_pairs.iter()) {
        changed.insert(s.to_string());
        changed.insert(d.to_string());
    }
    EdgeDiff {
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        reversed,
        changed_modules: changed,
    }
}

pub fn summarize(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> (EdgeDiff, Vec<String>, Vec<String>) {
    let diff = diff_edges(&sa.module_edges, &sb.module_edges);
    let new_files: Vec<String> = sb.files.difference(&sa.files).cloned().collect();
    let gone_files: Vec<String> = sa.files.difference(&sb.files).cloned().collect();
    (diff, new_files, gone_files)
}

/// File-path token regex (`transition.py:100`).
fn file_token_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"[A-Za-z0-9_][\w./+-]*\.[A-Za-z0-9]+").unwrap()
    })
}

/// Relative path tokens → module claims with existence verification
/// (`transition.py:103-132`, sync 281e07e): prefix-direct hits resolve
/// as-is; bare-relative tokens resolve only when
/// `repo_root/<prefix>/<first-segment>` is a real directory — a grounded
/// mapping, no guessing.
pub fn path_token_claims(
    text: &str,
    profile: &Profile,
    repo_root: &Path,
) -> BTreeSet<String> {
    let mut claims = BTreeSet::new();
    for tok in file_token_re().find_iter(text).map(|m| m.as_str()) {
        if !tok.contains('/') {
            continue; // bare filenames cannot be mapped to a module
        }
        let mut resolved: Option<String> = None;
        for rule in &profile.modules {
            if tok.starts_with(&rule.prefix) {
                resolved = Some(tok.to_string());
                break;
            }
        }
        if resolved.is_none() {
            let seg = tok.split('/').next().unwrap_or("");
            for rule in &profile.modules {
                if repo_root.join(&rule.prefix).join(seg).is_dir() {
                    resolved = Some(format!("{}{}", rule.prefix, tok));
                    break;
                }
            }
        }
        if let Some(r) = resolved {
            let m = module_of(&r, Some(profile));
            if !m.is_empty() {
                claims.insert(m);
            }
        }
    }
    claims
}

/// EP claims (`transition.py:135-150`): regex findall ∪ path tokens when
/// `repo_root` is provided. Missing EP file → crash (SM-12: NONE means
/// the file exists but has no mentions).
pub fn extract_ep_claims(
    ep_path: &Path,
    profile: Option<&Profile>,
    repo_root: Option<&Path>,
) -> Result<BTreeSet<String>, String> {
    if !ep_path.is_file() {
        return Err(format!(
            "EP 檔不存在或非檔案：{}（SM-12——NONE 是檔在但無 mention）",
            ep_path.display()
        ));
    }
    let text = std::fs::read_to_string(ep_path)
        .map_err(|e| format!("{} 讀取失敗：{}", ep_path.display(), e))?;
    let mut claims: BTreeSet<String> = claims_regex(profile)
        .find_iter(&text)
        .map(|m| m.as_str().to_string())
        .collect();
    if let (Some(root), Some(p)) = (repo_root, profile) {
        claims.extend(path_token_claims(&text, p, root));
    }
    Ok(claims)
}

/// EP header `**baseline**: <hash>` (`transition.py:153-157`) — the bold
/// literal is load-bearing (a bare `baseline:` line must NOT match).
pub fn extract_baseline(ep_path: &Path) -> Result<Option<String>, String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"\*\*baseline\*\*:\s*([0-9a-f]{7,40})").unwrap()
    });
    if !ep_path.is_file() {
        return Err(format!("EP 檔不存在或非檔案：{}", ep_path.display()));
    }
    let text = std::fs::read_to_string(ep_path)
        .map_err(|e| format!("{} 讀取失敗：{}", ep_path.display(), e))?;
    Ok(re.captures(&text).map(|c| c[1].to_string()))
}

pub struct ClaimsCompare {
    pub claimed_and_changed: Vec<String>,
    pub changed_not_claimed: Vec<String>,
    pub claimed_not_changed: Vec<String>,
    pub claims_none: bool,
}

pub fn compare_claims(claims: &BTreeSet<String>, changed: &BTreeSet<String>) -> ClaimsCompare {
    ClaimsCompare {
        claimed_and_changed: claims.intersection(changed).cloned().collect(),
        changed_not_claimed: changed.difference(claims).cloned().collect(),
        claimed_not_changed: claims.difference(changed).cloned().collect(),
        claims_none: claims.is_empty(),
    }
}

/// Actual changed modules = edge topology ∪ file add/remove owners
/// (`transition.py:169-179`).
fn changed_modules_all(
    diff: &EdgeDiff,
    new_files: &[String],
    gone_files: &[String],
    profile: Option<&Profile>,
) -> BTreeSet<String> {
    let mut out = diff.changed_modules.clone();
    for f in new_files.iter().chain(gone_files) {
        out.insert(module_of(f, profile));
    }
    out
}

fn fmt_edges(edges: &[Edge], limit: usize) -> Vec<String> {
    let mut lines: Vec<String> = edges
        .iter()
        .take(limit)
        .map(|(s, d, k)| format!("- `{s} -> {d}` ({k})"))
        .collect();
    if edges.len() > limit {
        lines.push(format!("- ... +{} more", edges.len() - limit));
    }
    lines
}

fn trunc_lines(entries: &[String], limit: usize) -> Vec<String> {
    let mut lines: Vec<String> = entries
        .iter()
        .take(limit)
        .map(|e| format!("- {e}"))
        .collect();
    if entries.len() > limit {
        lines.push(format!("- ... +{} more", entries.len() - limit));
    }
    lines
}

fn meta_str(meta: &Map<String, Value>, key: &str) -> String {
    meta.get(key).and_then(Value::as_str).unwrap_or("?").to_string()
}

fn meta_commit8(meta: &Map<String, Value>) -> String {
    meta_str(meta, "commit").chars().take(8).collect()
}

pub fn render_report(
    sa: &LoadedSnapshot,
    sb: &LoadedSnapshot,
    claims: Option<&BTreeSet<String>>,
    diff: &EdgeDiff,
    new_files: &[String],
    gone_files: &[String],
    profile: Option<&Profile>,
) -> String {
    let a8 = meta_commit8(&sa.meta);
    let b8 = meta_commit8(&sb.meta);
    let mut lines: Vec<String> = vec![
        format!("# Transition Report: {}", meta_str(&sb.meta, "repo")),
        String::new(),
        format!("- before: `{a8}`（{}）", sa.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
        format!("- after: `{b8}`（{}）", sb.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
        format!(
            "- module edges: {} -> {}（+{} / -{} / reversed {}）",
            sa.module_edges.len(),
            sb.module_edges.len(),
            diff.added.len(),
            diff.removed.len(),
            diff.reversed.len()
        ),
        format!(
            "- files: {} -> {}（+{} / -{}）",
            sa.files.len(),
            sb.files.len(),
            new_files.len(),
            gone_files.len()
        ),
        String::new(),
    ];
    if diff.added.is_empty()
        && diff.removed.is_empty()
        && diff.reversed.is_empty()
        && new_files.is_empty()
        && gone_files.is_empty()
    {
        lines.push("## 無結構變化".into());
        lines.push(String::new());
        lines.push("兩 snapshot 邊集與檔案集相同（同 commit 或無結構變動）。".into());
        lines.push(String::new());
        return lines.join("\n");
    }
    lines.push("## 邊變化".into());
    lines.push(String::new());
    if !diff.added.is_empty() {
        lines.push(format!("### added ({})", diff.added.len()));
        lines.extend(fmt_edges(&diff.added, 20));
        lines.push(String::new());
    }
    if !diff.removed.is_empty() {
        lines.push(format!("### removed ({})", diff.removed.len()));
        lines.extend(fmt_edges(&diff.removed, 20));
        lines.push(String::new());
    }
    if !diff.reversed.is_empty() {
        lines.push(format!("### reversed ({})——added 方向", diff.reversed.len()));
        let items: Vec<String> = diff
            .reversed
            .iter()
            .map(|(s, d)| format!("`{s} <-> {d}`"))
            .collect();
        lines.extend(trunc_lines(&items, 20));
        lines.push(String::new());
    }
    if !new_files.is_empty() {
        lines.push(format!("### new files ({})", new_files.len()));
        lines.extend(trunc_lines(new_files, 20));
        lines.push(String::new());
    }
    if !gone_files.is_empty() {
        lines.push(format!("### gone files ({})", gone_files.len()));
        lines.extend(trunc_lines(gone_files, 20));
        lines.push(String::new());
    }
    if !diff.removed.is_empty() && !diff.added.is_empty() {
        lines.push("> 已知未覆蓋：rename 偵測（module 改名表現為 remove+add）。".into());
        lines.push(String::new());
    }
    lines.push("## EP 宣稱 vs 實際變動".into());
    lines.push(String::new());
    let changed = changed_modules_all(diff, new_files, gone_files, profile);
    let changed_sorted: Vec<String> = changed.iter().cloned().collect();
    match claims {
        None => lines.push("未提供 `--ep`（EP 宣稱模組路徑對照省略）。".into()),
        Some(c) if c.is_empty() => {
            lines.push("claims: **NONE**——EP 內無 profile prefix 路徑 mention。".into());
            lines.push(format!(
                "- 實際變動模組（供判讀，無宣稱可比對）：{}",
                crate::common::py_list_repr(&changed_sorted)
            ));
        }
        Some(c) => {
            let cmp = compare_claims(c, &changed);
            lines.push(format!(
                "- 宣稱命中 ({})：{}",
                cmp.claimed_and_changed.len(),
                crate::common::py_list_repr(&cmp.claimed_and_changed)
            ));
            lines.push(format!(
                "- 實際超出——EP 沒提卻變了 ({})：{}",
                cmp.changed_not_claimed.len(),
                crate::common::py_list_repr(&cmp.changed_not_claimed)
            ));
            lines.push(format!(
                "- 宣稱未動——EP 說要動但沒變 ({})：{}",
                cmp.claimed_not_changed.len(),
                crate::common::py_list_repr(&cmp.claimed_not_changed)
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_json_value(
    sa: &LoadedSnapshot,
    sb: &LoadedSnapshot,
    claims: Option<&BTreeSet<String>>,
    diff: &EdgeDiff,
    new_files: &[String],
    gone_files: &[String],
    profile: Option<&Profile>,
) -> Value {
    let changed_set = changed_modules_all(diff, new_files, gone_files, profile);
    let changed: Vec<String> = changed_set.iter().cloned().collect();
    let mut out = json!({
        "_meta": {
            "tool": "code_reality.transition",
            "created_at": utc_now_iso_micros(),
            "before": sa.meta.get("commit").cloned().unwrap_or(Value::Null),
            "after": sb.meta.get("commit").cloned().unwrap_or(Value::Null),
            "repo": sb.meta.get("repo").cloned().unwrap_or(Value::Null),
        },
        "added": diff.added.iter().map(|(s, d, k)| json!([s, d, k])).collect::<Vec<_>>(),
        "removed": diff.removed.iter().map(|(s, d, k)| json!([s, d, k])).collect::<Vec<_>>(),
        "reversed": diff.reversed.iter().map(|(s, d)| json!([s, d])).collect::<Vec<_>>(),
        "changed_modules": changed,
        "new_files": new_files,
        "gone_files": gone_files,
    });
    if let Some(c) = claims {
        let cmp = compare_claims(c, &changed_set);
        out.as_object_mut().unwrap().insert(
            "ep_claims".into(),
            json!({
                "claims": c.iter().cloned().collect::<Vec<_>>(),
                "claims_none": cmp.claims_none,
                "claimed_and_changed": cmp.claimed_and_changed,
                "changed_not_claimed": cmp.changed_not_claimed,
                "claimed_not_changed": cmp.claimed_not_changed,
            }),
        );
    }
    out
}

/// Route a `code-reality transition ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 transition");
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
    // D3 crash face — but the frozen main() prints the profile-less WARN
    // to stdout BEFORE a later crash (e.g. missing EP): the accumulated
    // stdout is part of the byte face and must survive the crash
    let mut stdout = String::new();
    macro_rules! crash {
        ($msg:expr) => {{
            return ToolOutput {
                stdout: stdout.clone(),
                stderr: crate::msg_line("FAIL", &$msg),
                exit_code: 1,
            };
        }};
    }
    let sa = match load_snapshot(Path::new(&positionals[0])) {
        Ok(s) => s,
        Err(e) => crash!(e),
    };
    let sb = match load_snapshot(Path::new(&positionals[1])) {
        Ok(s) => s,
        Err(e) => crash!(e),
    };
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let profile = match load_profile(&repo) {
        Ok(p) => p,
        Err(e) => crash!(e),
    };
    let ep = values.get("--ep").and_then(|v| v.clone());
    if ep.is_some() && profile.is_none() {
        stdout.push_str(
            "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，宣稱對照不生效（--repo 預設 cwd）\n",
        );
    }
    let claims: Option<BTreeSet<String>> = match &ep {
        None => None,
        Some(ep) => match extract_ep_claims(Path::new(ep), profile.as_ref(), Some(&repo)) {
            Ok(c) => Some(c),
            Err(e) => crash!(e),
        },
    };
    let a8 = meta_commit8(&sa.meta);
    let b8 = meta_commit8(&sb.meta);
    let prefix = values
        .get("--output-prefix")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("transition-{a8}..{b8}")));
    if let Some(parent) = prefix.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            crash!(format!("{} 建立失敗：{}", parent.display(), e));
        }
    }
    let stem = prefix
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let md_path = prefix.with_file_name(format!("{stem}.md"));
    let json_path = prefix.with_file_name(format!("{stem}.json"));
    let (diff, new_files, gone_files) = summarize(&sa, &sb);
    let md_body = render_report(&sa, &sb, claims.as_ref(), &diff, &new_files, &gone_files, profile.as_ref());
    if let Err(e) = std::fs::write(&md_path, md_body) {
        crash!(format!("{} 寫入失敗：{}", md_path.display(), e));
    }
    let json_body = to_json_indent1(&render_json_value(
        &sa,
        &sb,
        claims.as_ref(),
        &diff,
        &new_files,
        &gone_files,
        profile.as_ref(),
    ));
    if let Err(e) = std::fs::write(&json_path, json_body) {
        crash!(format!("{} 寫入失敗：{}", json_path.display(), e));
    }
    stdout.push_str(&format!(
        "[OK] transition {a8} -> {b8}: +{} / -{} / reversed {} -> {} + {}\n",
        diff.added.len(),
        diff.removed.len(),
        diff.reversed.len(),
        md_path.display(),
        json_path.display()
    ));
    if let Some(ep) = &ep {
        match extract_baseline(Path::new(ep)) {
            Ok(Some(baseline)) => {
                stdout.push_str(&format!(
                    "[LOG] EP baseline={baseline}（diff before 應錨定此 commit）\n"
                ));
            }
            Ok(None) => {}
            Err(e) => crash!(e),
        }
    }
    stdout.push_str(&format!(
        "[LOG] rg 'changed_not_claimed' {}\n",
        json_path.display()
    ));
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}
