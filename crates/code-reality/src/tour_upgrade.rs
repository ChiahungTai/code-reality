//! `tour_upgrade` — the frozen `code_reality/tour_upgrade.py` contract:
//! legacy-format migration (pattern completion, cross-ref revival,
//! bracket sanitization, manifest write-through). Dry-run by default —
//! the curated-protection rule: never blindly overwrite a real corpus.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::tour_manifest::{self, Manifest};
use crate::tour_validate;
use crate::ToolOutput;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--tours-dir",
            short: None,
            kind: Kind::Value {
                metavar: "TOURS_DIR",
            },
        },
        FlagSpec {
            long: "--apply",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: tour_upgrade [-h] [--repo REPO] [--tours-dir TOURS_DIR] [--apply]\n",
    "\n",
    "tour corpus 舊格式遷移（預設 dry-run）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO\n",
    "  --tours-dir TOURS_DIR\n",
    "  --apply               實際寫檔（預設 dry-run）\n",
);

fn decl_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"^\s*(?:pub\s+)?(?:async\s+)?(?:unsafe\s+)?(struct|enum|trait|fn|impl|class|def)\s+([A-Za-z_][A-Za-z0-9_]*)",
        )
        .unwrap()
    })
}

fn backtick_decl() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"`(?:pub\s+)?(?:async\s+)?(struct|enum|trait|fn|impl|class|def)\s+([A-Za-z_][A-Za-z0-9_]*)",
        )
        .unwrap()
    })
}

fn crossref() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[(\d{1,2})\s*-\s*([^\]]+)\]").unwrap())
}

fn bracket() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[([^\]\[]+)\]").unwrap())
}

fn line_pattern(line: &str) -> String {
    format!("^[ \\t]*{}[ \\t]*$", regex::escape(line.trim()))
}

fn step_str(step: &serde_json::Value, key: &str) -> String {
    step.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Buildable pattern for a step (`tour_upgrade.py:31-60`): declaration
/// line → whole-line pattern (nearest hit must equal the original line);
/// otherwise backtick declaration hints from the description with the
/// nearest hit within ±1.
pub fn build_step_pattern(step: &serde_json::Value, repo: &Path) -> Option<String> {
    let f = step.get("file").and_then(|v| v.as_str())?;
    let ln = step.get("line")?.as_i64()?;
    let p = repo.join(f);
    if !p.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&p).ok()?;
    let lines: Vec<&str> = content.split('\n').collect();
    if !(1..=(lines.len() as i64)).contains(&ln) {
        return None;
    }
    let target = lines[(ln - 1) as usize];
    if decl_re().is_match(target) {
        let pat = line_pattern(target);
        let hits = tour_validate::hits(&lines, &pat);
        return if hits == vec![(ln - 1) as usize] {
            Some(pat)
        } else {
            None
        };
    }
    let desc = step
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    for m in backtick_decl().captures_iter(desc) {
        let cand = regex::Regex::new(&format!(
            r"^[ \t]*(?:pub\s+)?(?:async\s+)?(?:unsafe\s+)?{}\s+{}\b",
            &m[1],
            regex::escape(&m[2])
        ))
        .ok()?;
        let hits: Vec<i64> = lines
            .iter()
            .enumerate()
            .filter(|(_, x)| cand.is_match(x))
            .map(|(i, _)| i as i64)
            .collect();
        let near: Vec<i64> = hits
            .into_iter()
            .filter(|i| (i - (ln - 1)).abs() <= 1)
            .collect();
        if near.len() == 1 {
            return Some(line_pattern(lines[near[0] as usize]));
        }
    }
    None
}

/// Unresolved single brackets → full-width brackets (`tour_upgrade.py:66-81`):
/// the player's TOUR_REF misreads them as tour links. Markdown file links
/// (`](`) and double-bracket links (`][`) pass through.
pub fn sanitize_brackets(desc: &str) -> (String, usize) {
    let mut n = 0;
    let mut out = String::new();
    let mut last = 0usize;
    for m in bracket().captures_iter(desc) {
        let whole = m.get(0).unwrap();
        let (start, end) = (whole.start(), whole.end());
        let before = if start > 0 {
            &desc[start - 1..start]
        } else {
            ""
        };
        let after = if end < desc.len() {
            &desc[end..end + 1]
        } else {
            ""
        };
        if "]([".contains(before) || "](".contains(after) {
            continue;
        }
        out.push_str(&desc[last..start]);
        out.push('［');
        out.push_str(&m[1]);
        out.push('］');
        last = end;
        n += 1;
    }
    out.push_str(&desc[last..]);
    (out, n)
}

/// `[N - name]` → `[name][key#1]` (`tour_upgrade.py:84-96`).
pub fn revive_crossrefs(desc: &str, key_by_num: &BTreeMap<i64, String>) -> (String, usize) {
    let mut n = 0;
    let mut out = String::new();
    let mut last = 0usize;
    for m in crossref().captures_iter(desc) {
        let whole = m.get(0).unwrap();
        let num: i64 = m[1].parse().unwrap_or(0);
        let name = m[2].to_string();
        let Some(key) = key_by_num.get(&num) else {
            continue;
        };
        if key.contains(']') || key.contains('#') {
            continue;
        }
        out.push_str(&desc[last..whole.start()]);
        out.push_str(&format!("[{name}][{key}#1]"));
        last = whole.end();
        n += 1;
    }
    out.push_str(&desc[last..]);
    (out, n)
}

fn set_step_desc(step: &mut serde_json::Value, new_desc: &str) {
    step.as_object_mut().unwrap().insert(
        "description".into(),
        serde_json::Value::String(new_desc.to_string()),
    );
}

/// Mutate one tour in place (`tour_upgrade.py:99-123`); returns the
/// per-tour report row.
pub fn upgrade_tour(
    tour: &mut serde_json::Value,
    repo: &Path,
    key_by_num: &BTreeMap<i64, String>,
) -> (usize, usize, usize) {
    let mut pat_add = 0;
    let mut pat_skip = 0;
    let mut refs = 0;
    if let Some(steps) = tour.get_mut("steps").and_then(|s| s.as_array_mut()) {
        for step in steps.iter_mut() {
            if step
                .get("pattern")
                .and_then(|p| p.as_str())
                .map(|p| !p.is_empty())
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(pat) = build_step_pattern(step, repo) {
                step.as_object_mut()
                    .unwrap()
                    .insert("pattern".into(), serde_json::Value::String(pat));
                pat_add += 1;
            } else {
                pat_skip += 1;
            }
        }
        for step in steps.iter_mut() {
            let desc = step_str(step, "description");
            let (new_desc, n) = revive_crossrefs(&desc, key_by_num);
            if n > 0 {
                set_step_desc(step, &new_desc);
                refs += n;
            }
        }
        for step in steps.iter_mut() {
            let desc = step_str(step, "description");
            let (new_desc, n) = sanitize_brackets(&desc);
            if n > 0 {
                set_step_desc(step, &new_desc);
            }
        }
    }
    (pat_add, pat_skip, refs)
}

/// Route a `code-reality tour_upgrade ...` invocation (`tour_upgrade.py:126-176`).
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 tour_upgrade");
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
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let tours_dir = values
        .get("--tours-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tours"));
    let apply = values.contains_key("--apply");

    let mut tours = match tour_validate::iter_tours(&repo, &tours_dir, false) {
        Ok(t) => t,
        Err(e) => return ToolOutput::crash(e),
    };
    if tours.is_empty() {
        return ToolOutput::crash(format!("{} 無 .tour", repo.join(&tours_dir).display()));
    }
    // N → title key (zero-padded prefix strip; key via ts_key — hyphen
    // truncation semantics included)
    let mut key_by_num: BTreeMap<i64, String> = BTreeMap::new();
    let num_re = regex::Regex::new(r"^#?0*(\d+)\s-").unwrap();
    for (_, d) in &tours {
        let title = d.get("title").and_then(|t| t.as_str()).unwrap_or("");
        if let Some(m) = num_re.captures(title) {
            if let Ok(n) = m[1].parse::<i64>() {
                key_by_num.insert(n, tour_validate::ts_key(title));
            }
        }
    }
    let dup: Vec<(i64, String)> = key_by_num
        .iter()
        .filter(|(_, v)| v.contains('-'))
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    let (mut total_add, mut total_skip, mut total_refs) = (0, 0, 0);
    let mut report = Vec::new();
    for (rel, tour) in tours.iter_mut() {
        let (a, s, r) = upgrade_tour(tour, &repo, &key_by_num);
        total_add += a;
        total_skip += s;
        total_refs += r;
        report.push(format!("  {rel}: pattern +{a} skip {s} crossref {r}"));
    }
    let mode = if apply { "APPLY" } else { "DRY-RUN" };
    let mut stdout = format!(
        "[OK] tour_upgrade {mode}: {} tours | pattern +{total_add} skip {total_skip} | crossref {total_refs}\n",
        tours.len()
    );
    for line in &report {
        stdout.push_str(line);
        stdout.push('\n');
    }
    for (k, v) in &dup {
        stdout.push_str(&format!(
            "[WARN] 編號 {k} 的匹配鍵含 '-'（截斷風險）: {}\n",
            truncate(v, 40)
        ));
    }
    if !apply {
        return ToolOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let root = repo.join(&tours_dir);
    for (rel, tour) in &tours {
        let path = repo.join(rel);
        let body = crate::common::to_json_indent1(tour);
        if let Err(e) = std::fs::write(&path, format!("{body}\n")) {
            return ToolOutput::crash(format!("{} 寫入失敗：{}", path.display(), e));
        }
    }
    let (commit, _warn) = tour_manifest::git_head(&repo);
    let mpath = root.join("manifest.toml");
    let mut data: Manifest = tour_manifest::load(&mpath).unwrap_or_default();
    if data.version.is_none() {
        data.version = Some(toml::Value::Integer(1));
    }
    for (rel, _) in &tours {
        let rel_from_root = if tours_dir.to_string_lossy() == "." {
            rel.clone()
        } else {
            Path::new(rel)
                .strip_prefix(&tours_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| rel.clone())
        };
        tour_manifest::upsert(&mut data, &rel_from_root, "manual", &[], &commit);
    }
    if let Err(e) = tour_manifest::dump(&mpath, &data) {
        return ToolOutput::crash(e);
    }
    stdout.push_str(&format!(
        "[OK] manifest 寫入: {}（{} rows, generator=manual=curated）\n",
        mpath.display(),
        tours.len()
    ));
    let validated = tour_validate::validate(&repo, &tours_dir, true);
    stdout.push_str(&validated.stdout);
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: validated.exit_code,
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
