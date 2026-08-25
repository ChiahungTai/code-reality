//! `delta_tour` — the frozen `code_reality/delta_tour.py` contract
//! (post 281e07e): transition diff + git hunk anchors → CodeTour.
//! Step-set truth = the claimed git range (`git diff --name-status`);
//! claims three-state guard; deleted files collapse to one summary
//! step; descriptions carry range commit subjects (the mechanical why).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{anchor_pattern, to_json_indent1};
use crate::profile::{is_excluded, load_profile, module_of};
use crate::transition::{
    extract_ep_claims, load_snapshot, render_json_value, summarize,
};
use crate::ToolOutput;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const DEFAULT_TASK: &str = "review";

fn hunk_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?m)^@@ -\d+(?:,\d+)? \+(\d+)").unwrap())
}

fn decl_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"^\s*(?:async\s+def\b|def\b|class\b|fn\b|struct\b|enum\b|impl\b|trait\b|mod\b|func\b|type\b|interface\b)",
        )
        .unwrap()
    })
}

fn code_suffix(p: &str) -> bool {
    [
        ".py", ".rs", ".ts", ".tsx", ".js", ".go", ".java", ".c", ".h", ".cpp",
    ]
    .iter()
    .any(|s| p.to_lowercase().ends_with(s))
}

/// Local date `YYYY-MM-DD` (tour filename + cleanup window semantics —
/// deliberately NOT UTC).
pub fn local_today() -> String {
    // libc localtime → y-m-d
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as libc::time_t;
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday
        )
    }
}

fn git_bytes(repo_root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|e| format!("git {:?} 執行失敗：{e}", args))?;
    if !out.status.success() {
        return Err(format!(
            "git {:?} 失敗：{}",
            args,
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(out.stdout)
}

/// `git diff --unified=0` first positive hunk line per file + git-A set
/// (`delta_tour.py:52-110`). Git failures crash — silent degradation
/// would silently drop steps.
pub fn first_change_lines(
    repo_root: &Path,
    before: &str,
    after: &str,
) -> Result<(BTreeMap<String, i64>, BTreeSet<String>), String> {
    let out = String::from_utf8_lossy(&git_bytes(
        repo_root,
        &[
            "diff", "--name-status", "-z", "--diff-filter=AM", before, after,
        ],
    )?)
    .into_owned();
    let toks: Vec<&str> = out.split('\0').filter(|p| !p.is_empty()).collect();
    // pairwise pairing relies on the AM filter: A/M entries have exactly
    // two fields; R/C are three (R100\0old\0new) and would misparse
    let mut added: BTreeSet<String> = BTreeSet::new();
    let mut files: Vec<String> = Vec::new();
    let mut it = toks.into_iter();
    while let (Some(status), Some(f)) = (it.next(), it.next()) {
        files.push(f.to_string());
        if status == "A" {
            added.insert(f.to_string());
        }
    }
    let mut lines: BTreeMap<String, i64> = BTreeMap::new();
    for f in &files {
        let hunks = String::from_utf8_lossy(&git_bytes(
            repo_root,
            &["diff", "--unified=0", before, after, "--", f],
        )?)
        .into_owned();
        let anchor = hunk_re()
            .captures_iter(&hunks)
            .filter_map(|m| m[1].parse::<i64>().ok())
            .find(|n| *n > 0);
        match anchor {
            Some(n) => {
                lines.insert(f.clone(), n);
            }
            None if !hunks.trim().is_empty() => {
                lines.insert(f.clone(), 1); // missing file beats weak anchor
            }
            None => {}
        }
    }
    Ok((lines, added))
}

/// Full `git diff --name-status -z` for the claimed range — the step-set
/// single source of truth (snapshot file sets drift when the profile or
/// exclusions change between exports).
pub type RangeStatus = (BTreeMap<&'static str, Vec<String>>, BTreeMap<String, String>);

pub fn range_status(
    repo_root: &Path,
    before: &str,
    after: &str,
) -> Result<RangeStatus, String> {
    let out = String::from_utf8_lossy(&git_bytes(
        repo_root,
        &["diff", "--name-status", "-z", before, after],
    )?)
    .into_owned();
    let toks: Vec<&str> = out.split('\0').filter(|t| !t.is_empty()).collect();
    let mut by_status: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    by_status.insert("A", Vec::new());
    by_status.insert("M", Vec::new());
    by_status.insert("D", Vec::new());
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    let mut i = 0usize;
    while i < toks.len() {
        let code = toks[i].chars().next().unwrap_or(' ');
        if code == 'R' || code == 'C' {
            let (old, new) = (toks[i + 1], toks[i + 2]);
            i += 3;
            if code == 'R' {
                renames.insert(new.to_string(), old.to_string());
            } else {
                by_status.get_mut("A").unwrap().push(new.to_string());
            }
        } else {
            let path = toks[i + 1];
            i += 2;
            match code {
                'A' => by_status.get_mut("A").unwrap().push(path.to_string()),
                'D' => by_status.get_mut("D").unwrap().push(path.to_string()),
                _ => by_status.get_mut("M").unwrap().push(path.to_string()), // M / T
            }
        }
    }
    Ok((by_status, renames))
}

/// First declaration line for added/renamed code files (copyright
/// headers are not what a reader wants); line 1 otherwise.
pub fn new_file_anchor(repo_root: &Path, after: &str, path: &str) -> i64 {
    if !code_suffix(path) {
        return 1;
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("show")
        .arg(format!("{after}:{path}"))
        .output();
    let Ok(r) = out else { return 1 };
    if !r.status.success() {
        return 1;
    }
    let Ok(text) = String::from_utf8(r.stdout) else {
        return 1;
    };
    for (idx, line) in text.split('\n').enumerate() {
        if decl_re().is_match(line) {
            return (idx + 1) as i64;
        }
    }
    1
}

/// Commit subjects touching `path` within the range — the cheapest
/// mechanical why for step descriptions.
pub fn file_subjects(repo_root: &Path, before: &str, after: &str, path: &str) -> Vec<String> {
    let Ok(out) = git_bytes(repo_root, &["log", "--format=%s", &format!("{before}..{after}"), "--", path])
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out)
        .split('\n')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn after_lines(
    repo_root: &Path,
    after: &str,
    path: &str,
    cache: &mut HashMap<String, Option<Vec<String>>>,
) -> Option<Vec<String>> {
    if !cache.contains_key(path) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg(format!("{after}:{path}"))
            .output();
        let v = match out {
            Ok(r) if r.status.success() => String::from_utf8(r.stdout)
                .ok()
                .map(|t| t.split('\n').map(|s| s.to_string()).collect()),
            _ => None,
        };
        cache.insert(path.to_string(), v);
    }
    cache.get(path).cloned().flatten()
}

/// transition JSON → CodeTour tour (`delta_tour.py:239-423`).
pub fn build_tour(
    data: &Value,
    repo_root: &Path,
    ep_path: Option<&Path>,
    task: &str,
    stderr: &mut String,
) -> Result<Value, String> {
    let profile = load_profile(repo_root)?;
    let meta = data["_meta"].as_object().cloned().unwrap_or_default();
    let before = meta.get("before").and_then(Value::as_str).unwrap_or("");
    let after = meta.get("after").and_then(Value::as_str).unwrap_or("");
    let (statuses, renames) = range_status(repo_root, before, after)?;
    let (jump, _) = first_change_lines(repo_root, before, after)?;

    let empty: Vec<Value> = Vec::new();
    let claims = data.get("ep_claims").and_then(Value::as_object).cloned().unwrap_or_default();
    let strs = |key: &str| -> Vec<String> {
        claims
            .get(key)
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    };
    let c_hit = strs("claimed_and_changed");
    let c_sur = strs("changed_not_claimed");
    let c_miss = strs("claimed_not_changed");

    // claims three-state: ⚠ "EP didn't mention this" is a serious
    // accusation — only when the comparison actually ran
    let (claims_state, nc_reason): (&str, Option<String>) = if data.get("ep_claims").is_none() {
        ("no_ep", None)
    } else if claims.get("claims_none").and_then(Value::as_bool).unwrap_or(false) {
        (
            "not_compared",
            Some(
                if profile.is_none() {
                    "profile 未載入——--repo 未指到含 .code-reality.toml 的 checkout".to_string()
                } else {
                    "EP 內無 profile 前綴路徑 mention（相對路徑需可解析至前綴下）".to_string()
                },
            ),
        )
    } else if c_hit.is_empty() && !c_sur.is_empty() {
        let reason =
            "宣稱對照 0 命中且有多個變更模組——matcher 異常訊號，整塊降級未比對".to_string();
        stderr.push_str(&format!(
            "[WARN] {reason}（步驟不標 ✓/⚠——避免把抽取失效誤呈為「EP 沒提卻變了」）\n"
        ));
        ("not_compared", Some(reason))
    } else {
        ("compared", None)
    };

    let claim_tag = |module: &str| -> &'static str {
        if claims_state != "compared" {
            return "";
        }
        if c_hit.iter().any(|m| m == module) {
            return "✓宣稱命中";
        }
        if c_sur.iter().any(|m| m == module) {
            return "⚠EP沒提卻變了";
        }
        ""
    };

    let changed_modules: Vec<String> = data
        .get("changed_modules")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let claims_section = match claims_state {
        "compared" => format!(
            "**宣稱對照**——✓ 命中 ({})：{}；\n⚠ EP 沒提卻變了 ({})：{}；\n✗ 宣稱未動 ({})：{}。",
            c_hit.len(),
            if c_hit.is_empty() { "無".into() } else { c_hit.join(", ") },
            c_sur.len(),
            if c_sur.is_empty() { "無".into() } else { c_sur.join(", ") },
            c_miss.len(),
            if c_miss.is_empty() { "無".into() } else { c_miss.join(", ") }
        ),
        "not_compared" => format!(
            "**EP 宣稱對照：未比對**——{}；本 tour 不對步驟標註 ✓/⚠。\n實際變動模組（供判讀）：{}",
            nc_reason.unwrap_or_default(),
            if changed_modules.is_empty() { "無".into() } else { changed_modules.join(", ") }
        ),
        _ => format!(
            "**EP 宣稱**：NONE（未提供 --ep）。\n實際變動模組（供判讀）：{}",
            if changed_modules.is_empty() { "無".into() } else { changed_modules.join(", ") }
        ),
    };

    let noise = |f: &str| -> bool {
        is_excluded(f, profile.as_ref()) || f.starts_with(".kanban/") || f.starts_with(".tours/")
    };
    let a_files: Vec<String> = statuses["A"].iter().filter(|f| !noise(f)).cloned().collect();
    let r_files: Vec<String> = renames.keys().filter(|f| !noise(f)).cloned().collect();
    let m_files: Vec<String> = statuses["M"]
        .iter()
        .filter(|f| !noise(f) && !renames.contains_key(*f))
        .cloned()
        .collect();
    let d_files: Vec<String> = statuses["D"].iter().filter(|f| !noise(f)).cloned().collect();

    // overview counts derive from the same sets as the steps
    let added_n = data.get("added").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let removed_n = data.get("removed").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let summary = format!(
        "before `{}` → after `{}`：+{}/−{} 模組邊、{} 新檔、{} 改名、{} 修改、{} 刪檔。\n\n{}\n\n之後每步一個變動檔（修改錨第一個 hunk、新檔錨第一個宣告行）。",
        before.chars().take(8).collect::<String>(),
        after.chars().take(8).collect::<String>(),
        added_n,
        removed_n,
        a_files.len(),
        r_files.len(),
        m_files.len(),
        d_files.len(),
        claims_section
    );

    let ep_on_disk = ep_path.map(|p| p.exists()).unwrap_or(false);
    let ep_anchor: Option<String> = if ep_on_disk {
        ep_path.map(|p| p.display().to_string())
    } else {
        a_files.first().cloned().or_else(|| r_files.first().cloned())
    };

    let mut lines_cache: HashMap<String, Option<Vec<String>>> = HashMap::new();
    let step_pattern = |f: &str, ln: i64,
                        cache: &mut HashMap<String, Option<Vec<String>>>|
     -> Option<String> {
        let lines = after_lines(repo_root, after, f, cache)?;
        if (ln - 1) >= lines.len() as i64 || lines[(ln - 1) as usize].trim().is_empty() {
            return None;
        }
        Some(anchor_pattern(&lines[(ln - 1) as usize]))
    };

    let mut overview = json!({
        "file": ep_anchor.clone().unwrap_or_else(|| "README.md".into()),
        "line": 1,
        "title": format!("弧總覽：{} → {}", before.chars().take(8).collect::<String>(), after.chars().take(8).collect::<String>()),
        "description": summary,
    });
    let mut steps: Vec<Value> = vec![overview.clone()];
    if !ep_on_disk {
        if let Some(anchor) = &ep_anchor {
            if let Some(pat) = step_pattern(anchor, 1, &mut lines_cache) {
                overview
                    .as_object_mut()
                    .unwrap()
                    .insert("pattern".into(), Value::String(pat));
                steps[0] = overview.clone();
            }
        }
    }

    let mut entries: Vec<(String, &'static str, i64)> = a_files
        .iter()
        .map(|f| (f.clone(), "＋新檔", new_file_anchor(repo_root, after, f)))
        .collect();
    entries.extend(
        r_files
            .iter()
            .map(|f| (f.clone(), "→改名", new_file_anchor(repo_root, after, f))),
    );
    entries.extend(
        m_files
            .iter()
            .map(|f| (f.clone(), "M修改", jump.get(f).copied().unwrap_or(1))),
    );
    for (f, tag, ln) in entries {
        let mod_ = module_of(&f, profile.as_ref());
        let ct = claim_tag(&mod_);
        let subs = file_subjects(repo_root, before, after, &f);
        let mut description = format!("{f} · 模組 `{mod_}`");
        if !ct.is_empty() {
            description.push_str(&format!(" · {ct}"));
        }
        if tag == "→改名" {
            description.push_str(&format!("\n改名自 `{}`。", renames.get(&f).cloned().unwrap_or_default()));
        }
        if !subs.is_empty() {
            description.push_str(&format!("\ncommit: {}", subs[0]));
            if subs.len() > 1 {
                description.push_str(&format!("（range 內共 {} commits）", subs.len()));
            }
        }
        if ln > 1 {
            description.push_str(&format!("\n\n錨：第 {ln} 行。"));
        }
        let title = format!(
            "{tag} {}{}",
            f.rsplit('/').next().unwrap_or(&f),
            if ct.is_empty() { String::new() } else { format!("（{ct}）") }
        );
        let mut step = json!({
            "file": f,
            "line": ln,
            "title": title,
            "description": description,
        });
        if let Some(pat) = step_pattern(&f, ln, &mut lines_cache) {
            step.as_object_mut().unwrap().insert("pattern".into(), Value::String(pat));
        }
        steps.push(step);
    }

    if !d_files.is_empty() {
        let listing: Vec<String> = d_files.iter().map(|f| format!("- {f}")).collect();
        steps.push(json!({
            "file": d_files[0],
            "line": 1,
            "title": format!("−刪檔 ×{}（range 內彙總）", d_files.len()),
            "description": format!("本弧刪除 {} 檔——無法跳轉，僅清單：\n{}", d_files.len(), listing.join("\n")),
        }));
    }

    Ok(json!({
        "title": format!("{task} 變更導覽"),
        "description": summary,
        "steps": steps,
    }))
}

/// stem → ASCII kebab-case task segment (panel filename parsing).
pub fn kebab(name: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"[^A-Za-z0-9]+").unwrap());
    re.replace_all(name, "-").trim_matches('-').to_lowercase()
}

/// Remove delta tours whose filename dates exceed the keep window.
pub fn cleanup_expired(out_dir: &Path, keep_days: i64, today: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(out_dir) else {
        return 0;
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    static DATE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let dr = DATE_RE.get_or_init(|| regex::Regex::new(r"^(\d{4}-\d{2}-\d{2})-").unwrap());
    let mut removed = 0;
    for p in files {
        if !p.is_file() {
            continue;
        }
        if p.extension().map(|e| e != "tour").unwrap_or(true) {
            continue;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(m) = dr.captures(&name) else { continue };
        // date diff in days (ISO strings compare lexicographically)
        let today_days = iso_to_days(today);
        let file_days = iso_to_days(&m[1]);
        let (Some(t), Some(f)) = (today_days, file_days) else { continue };
        if t - f > keep_days {
            let _ = std::fs::remove_file(&p);
            removed += 1;
        }
    }
    removed
}

fn iso_to_days(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    Some(crate::common::days_from_civil(y, m as u32, d as u32))
}

/// Route a `code-reality delta_tour ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 delta_tour");
    };
    let spec = ToolSpec {
        flags: &[
            FlagSpec { long: "--ep", short: None, kind: Kind::Value { metavar: "EP" } },
            FlagSpec { long: "--repo", short: None, kind: Kind::Value { metavar: "REPO" } },
            FlagSpec { long: "--task", short: None, kind: Kind::Value { metavar: "TASK" } },
            FlagSpec { long: "--out-dir", short: None, kind: Kind::Value { metavar: "OUT_DIR" } },
        ],
        positionals: &["snapshot_a", "snapshot_b"],
    };
    let (values, positionals) = match parse(&spec, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: concat!(
                    "usage: delta_tour [-h] [--ep EP] [--repo REPO] [--task TASK]\n",
                    "                 [--out-dir OUT_DIR] snapshot_a snapshot_b\n",
                    "\n",
                    "delta-review tour——transition diff＋git hunk 錨 → CodeTour `.tour`。\n",
                    "\n",
                    "positional arguments:\n",
                    "  snapshot_a            before snapshot（S2 schema）\n",
                    "  snapshot_b            after snapshot（S2 schema）\n",
                    "\n",
                    "options:\n",
                    "  -h, --help            show this help message and exit\n",
                    "  --ep EP               EP markdown（宣稱對照＋總覽步錨點）\n",
                    "  --repo REPO           repo 根（before/after commit 需在此 git 歷史內）\n",
                    "  --task TASK           任務身分（檔名 <date>-<task>.tour 與 title；建議 ASCII kebab-case——panel 檔名慣例，原樣使用；預設 --ep stem kebab 化，無 --ep 時 review）\n",
                    "  --out-dir OUT_DIR     輸出目錄（預設 .tours/delta/）\n",
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, positionals } => (values, positionals),
    };
    let ep = values.get("--ep").and_then(|v| v.clone()).map(PathBuf::from);
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let task = match values.get("--task").and_then(|v| v.clone()) {
        Some(t) => {
            if t.is_empty() {
                return ToolOutput::crash("--task 不得為空字串");
            }
            t
        }
        None => ep
            .as_ref()
            .map(|p| {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let k = kebab(&stem);
                if k.is_empty() { DEFAULT_TASK.to_string() } else { k }
            })
            .unwrap_or_else(|| DEFAULT_TASK.to_string()),
    };
    let out_dir = values
        .get("--out-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tours/delta"));

    let mut stderr = String::new();
    let mut stdout = String::new();
    let sa = match load_snapshot(Path::new(&positionals[0])) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    let sb = match load_snapshot(Path::new(&positionals[1])) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    let profile = match load_profile(&repo) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
    if ep.is_some() && profile.is_none() {
        stdout.push_str(
            "[WARN] claims 恆 NONE——--repo 未指到含 .code-reality.toml 的 repo，宣稱對照不生效（--repo 預設 cwd）\n",
        );
    }
    let claims = match &ep {
        None => None,
        Some(p) => match extract_ep_claims(p, profile.as_ref(), Some(&repo)) {
            Ok(c) => Some(c),
            Err(e) => return ToolOutput::crash(e),
        },
    };
    let (diff, new_files, gone_files) = summarize(&sa, &sb);
    let data = render_json_value(&sa, &sb, claims.as_ref(), &diff, &new_files, &gone_files, profile.as_ref());
    let tour = match build_tour(&data, &repo, ep.as_deref(), &task, &mut stderr) {
        Ok(t) => t,
        Err(e) => return ToolOutput::crash(e),
    };
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return ToolOutput::crash(format!("{} 建立失敗：{e}", out_dir.display()));
    }
    let out_path = out_dir.join(format!("{}-{task}.tour", local_today()));
    if let Err(e) = std::fs::write(&out_path, to_json_indent1(&tour)) {
        return ToolOutput::crash(format!("{} 寫入失敗：{e}", out_path.display()));
    }
    let n_steps = tour["steps"].as_array().map(|a| a.len()).unwrap_or(0);
    stdout.push_str(&format!("[OK] delta tour: {n_steps} steps -> {}\n", out_path.display()));
    let cleaned = cleanup_expired(&out_dir, 7, &local_today());
    if cleaned > 0 {
        stdout.push_str(&format!("[OK] cleaned {cleaned} expired delta tours（>7 天）\n"));
    }
    stdout.push_str("[LOG] CodeTour 擴充載入 .tours/ 走讀（vanilla 或 fork 皆可）\n");
    ToolOutput { stdout, stderr, exit_code: 0 }
}

