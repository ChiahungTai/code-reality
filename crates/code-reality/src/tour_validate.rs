//! `tour_validate` — the frozen `code_reality/tour_validate.py` contract:
//! mechanical validation of the tour corpus (JSON shape, tour-link keys,
//! file-link paths, anchor three-states, manifest sources). Consumer
//! semantics are reimplemented from codetour's player (regex-verified
//! across three corpora); the anchor "corrected" state is a WARN, not a
//! fail — matching the player.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
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
            long: "--manifest",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: tour_validate [-h] [--repo REPO] [--tours-dir TOURS_DIR]\n",
    "                     [--manifest]\n",
    "\n",
    "tour corpus 機械驗證（.tour 語言契約）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo 根\n",
    "  --tours-dir TOURS_DIR corpus 根（遞迴）\n",
    "  --manifest            驗 manifest source 存在性\n",
);

fn ts_key_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^#?\d+\s-").unwrap())
}

/// codetour getTourTitle reproduction (`tour_validate.py:20-24`): strip
/// the `NN -` prefix (truncate at the FIRST '-').
pub fn ts_key(title: &str) -> String {
    if ts_key_re().is_match(title) {
        return title.split('-').nth(1).unwrap_or("").trim().to_string();
    }
    title.to_string()
}

const EXCLUDED_DIRS: [&str; 2] = ["delta", "dev-fixture"];

pub type TourPair = (String, serde_json::Value);

/// Recursive corpus walk (`tour_validate.py:33-47`); delta/ and
/// dev-fixture/ excluded by default (the key index builds with
/// include_excluded — excluded tours remain legal link targets).
pub fn iter_tours(
    repo: &Path,
    tours_dir: &Path,
    include_excluded: bool,
) -> Result<Vec<TourPair>, String> {
    let root = repo.join(tours_dir);
    let mut files = crate::tour_manifest::glob_tours(&root);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(rel_path) = f.strip_prefix(&root) else {
            continue;
        };
        let parts: Vec<String> = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let dirs = &parts[..parts.len().saturating_sub(1)];
        if !include_excluded && dirs.iter().any(|d| EXCLUDED_DIRS.contains(&d.as_str())) {
            continue;
        }
        let rel = f
            .strip_prefix(repo)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| f.to_string_lossy().into_owned());
        let text =
            std::fs::read_to_string(&f).map_err(|e| format!("{} 讀取失敗：{}", f.display(), e))?;
        let tour: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse: {e}"))?;
        out.push((rel, tour));
    }
    Ok(out)
}

pub fn key_index(tours: &[TourPair]) -> BTreeMap<String, Vec<String>> {
    let mut idx: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (rel, d) in tours {
        idx.entry(ts_key(
            d.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        ))
        .or_default()
        .push(rel.clone());
    }
    idx
}

/// Line hits for an arbitrary pattern; compile failure → no hits
/// (`tour_validate.py:57-62`).
pub fn hits(lines: &[&str], pattern: &str) -> Vec<usize> {
    let Ok(rx) = regex::Regex::new(pattern) else {
        return Vec::new();
    };
    lines
        .iter()
        .enumerate()
        .filter(|(_, ln)| rx.is_match(ln))
        .map(|(i, _)| i)
        .collect()
}

fn tour_ref() -> &'static fancy_regex::Regex {
    // lookahead-bearing consumer pattern — fancy-regex (lookaround port)
    static RE: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        fancy_regex::Regex::new(r"(?:\[([^\]]+)\])?\[(?=\s*[^\]\s])([^\]#]+)?(?:#(\d+))?\](?!\()")
            .unwrap()
    })
}

fn file_ref() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[([^\]]+)\]\((\.[^\)]+)\)").unwrap())
}

struct TourRefMatch {
    text: Option<String>,
    key: Option<String>,
    num: Option<String>,
}

fn find_tour_refs(desc: &str) -> Vec<TourRefMatch> {
    let mut out = Vec::new();
    for m in tour_ref().captures_iter(desc).flatten() {
        let g = |i: usize| m.get(i).map(|g| g.as_str().to_string());
        out.push(TourRefMatch {
            text: g(1),
            key: g(2),
            num: g(3),
        });
    }
    out
}

/// Tour-link checks (`tour_validate.py:65-86`): unresolved single
/// brackets are prose false-positives (WARN); real links must hit exactly
/// one target; step numbers must be in range.
pub fn check_links(
    rel: &str,
    tour: &serde_json::Value,
    idx: &BTreeMap<String, Vec<String>>,
    by_rel: &BTreeMap<String, serde_json::Value>,
    stdout: &mut String,
) -> (Vec<String>, usize) {
    let mut fails = Vec::new();
    let mut n_links = 0;
    let steps = tour
        .get("steps")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    for (i, step) in steps.iter().enumerate() {
        let desc = step
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        for m in find_tour_refs(desc) {
            let key = m.key.clone().unwrap_or_default().trim().to_string();
            let hits = idx.get(&key).cloned().unwrap_or_default();
            if hits.len() != 1 {
                if m.text.is_none() && m.num.is_none() {
                    stdout.push_str(&format!(
                        "[WARN] {rel} 步{} 單括號非 link 文字: [{}]\n",
                        i + 1,
                        truncate(&key, 36)
                    ));
                    continue;
                }
                fails.push(format!(
                    "[FAIL] {rel} 步{} tour link 無/歧義目標: {}",
                    i + 1,
                    truncate(&key, 40)
                ));
                continue;
            }
            n_links += 1;
            if let Some(num) = &m.num {
                let target_steps = by_rel
                    .get(&hits[0])
                    .and_then(|t| t.get("steps"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if num.parse::<usize>().unwrap_or(0) > target_steps {
                    fails.push(format!(
                        "[FAIL] {rel} 步{} 步號越界: {}#{num}",
                        i + 1,
                        truncate(&key, 40)
                    ));
                }
            }
        }
    }
    (fails, n_links)
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Anchor three-state check (`tour_validate.py:89-114`): exact → count;
/// corrected → WARN (player semantics); unverified → FAIL.
pub fn check_anchors(
    rel: &str,
    tour: &serde_json::Value,
    repo: &Path,
    stdout: &mut String,
) -> (Vec<String>, usize, usize) {
    let mut fails = Vec::new();
    let (mut exact, mut corrected) = (0, 0);
    let steps = tour
        .get("steps")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    for (i, step) in steps.iter().enumerate() {
        let f = step.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let ln = step.get("line").and_then(|v| v.as_i64());
        let pat = step.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let (Some(ln), true) = (ln, !f.is_empty()) else {
            continue;
        };
        if pat.is_empty() {
            continue;
        }
        let p = repo.join(f);
        if !p.exists() {
            fails.push(format!("[FAIL] {rel} 步{} 錨檔不存在: {f}", i + 1));
            continue;
        }
        let content = std::fs::read_to_string(&p).unwrap_or_default();
        let lines: Vec<&str> = content.split('\n').collect();
        let rx = regex::Regex::new(pat).ok();
        let anchored = ln >= 1
            && ((ln as usize) <= lines.len())
            && rx
                .map(|r| r.is_match(lines[(ln - 1) as usize]))
                .unwrap_or(false);
        if anchored {
            exact += 1;
        } else {
            let hits = hits(&lines, pat);
            if hits.is_empty() {
                fails.push(format!(
                    "[FAIL] {rel} 步{} pattern 未命中（unverified）: {f}:{ln}",
                    i + 1
                ));
            } else {
                let best = hits
                    .into_iter()
                    .min_by_key(|h| (*h as i64 - (ln - 1)).abs())
                    .unwrap();
                corrected += 1;
                stdout.push_str(&format!(
                    "[WARN] {rel} 步{} 錨 corrected: {f} L{ln}->L{}\n",
                    i + 1,
                    best + 1
                ));
            }
        }
    }
    (fails, exact, corrected)
}

pub fn check_files(rel: &str, tour: &serde_json::Value, repo: &Path) -> Vec<String> {
    let mut fails = Vec::new();
    let steps = tour
        .get("steps")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    for (i, step) in steps.iter().enumerate() {
        let desc = step
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        for m in file_ref().captures_iter(desc) {
            if !repo.join(&m[2]).exists() {
                fails.push(format!(
                    "[FAIL] {rel} 步{} file link 路徑不存在: {}",
                    i + 1,
                    &m[2]
                ));
            }
        }
    }
    fails
}

pub fn check_manifest(
    repo: &Path,
    tours_dir: &Path,
    tours: &[TourPair],
    stdout: &mut String,
) -> Vec<String> {
    let path = repo.join(tours_dir).join("manifest.toml");
    if !path.exists() {
        stdout.push_str(&format!(
            "[WARN] 無 manifest（{}）——source 存在性未驗\n",
            path.display()
        ));
        return Vec::new();
    }
    let data = match crate::tour_manifest::load(&path) {
        Ok(d) => d,
        Err(e) => return vec![e],
    };
    let mut fails = Vec::new();
    for rel in data.tour.keys() {
        if !repo.join(tours_dir).join(rel).exists() {
            fails.push(format!("[FAIL] manifest 列的 tour 檔不存在: {rel}"));
        }
    }
    for (rel, _) in tours {
        let root_rel = if tours_dir.to_string_lossy() == "." {
            rel.clone()
        } else {
            Path::new(rel)
                .strip_prefix(tours_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| rel.clone())
        };
        if !data.tour.contains_key(&root_rel) {
            stdout.push_str(&format!(
                "[WARN] {rel} 不在 manifest（derived/curated 未申報）\n"
            ));
        }
    }
    for (rel, row) in &data.tour {
        if let Some(sources) = row.get("sources").and_then(|v| v.as_array()) {
            for src in sources {
                if let Some(s) = src.as_str() {
                    if !repo.join(s).exists() {
                        fails.push(format!("[FAIL] {rel} manifest source 不存在: {s}"));
                    }
                }
            }
        }
    }
    fails
}

/// Full validation (`tour_validate.py:162-196`); returns (exit_code,
/// stdout, stderr) assembled as ToolOutput.
pub fn validate(repo: &Path, tours_dir: &Path, with_manifest: bool) -> ToolOutput {
    let mut stdout = String::new();
    let tours = match iter_tours(repo, tours_dir, false) {
        Ok(t) => t,
        Err(e) => {
            stdout.push_str(&format!("[FAIL] {e}\n"));
            return ToolOutput {
                stdout,
                stderr: String::new(),
                exit_code: 1,
            };
        }
    };
    if tours.is_empty() {
        stdout.push_str(&format!(
            "[WARN] {} 無 .tour\n",
            repo.join(tours_dir).display()
        ));
        return ToolOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let idx = key_index(&iter_tours(repo, tours_dir, true).unwrap_or_default());
    let by_rel: BTreeMap<String, serde_json::Value> = tours.iter().cloned().collect();
    let mut fails = Vec::new();
    let (mut n_links, mut n_files) = (0, 0);
    for (rel, tour) in &tours {
        let (lf, nl) = check_links(rel, tour, &idx, &by_rel, &mut stdout);
        fails.extend(lf);
        n_links += nl;
        let desc = tour
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        n_files += file_ref().find_iter(desc).count();
        if let Some(steps) = tour.get("steps").and_then(|s| s.as_array()) {
            for step in steps {
                let d = step
                    .get("description")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                n_files += file_ref().find_iter(d).count();
            }
        }
        fails.extend(check_files(rel, tour, repo));
        let (af, _, _) = check_anchors(rel, tour, repo, &mut stdout);
        fails.extend(af);
    }
    if with_manifest {
        fails.extend(check_manifest(repo, tours_dir, &tours, &mut stdout));
    }
    for f in &fails {
        stdout.push_str(f);
        stdout.push('\n');
    }
    stdout.push_str(&format!(
        "[OK] tour validate: {} tours | links={n_links} filelinks={n_files} | fails={}\n",
        tours.len(),
        fails.len()
    ));
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: if fails.is_empty() { 0 } else { 1 },
    }
}

/// Route a `code-reality tour_validate ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 tour_validate");
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
    validate(&repo, &tours_dir, values.contains_key("--manifest"))
}
