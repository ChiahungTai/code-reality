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

/// Leading two-plus-digit prefix (greedy — `100-x` is family 100, not 10);
/// the family-number face shared by family dirs, family titles, and the
/// chain_tour out-dir basename. Single source (CR-2 leg-3 F3) — the
/// pattern once lived as drifting literals in three files.
pub(crate) fn nn_prefix_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(\d{2,})").unwrap())
}

/// Family-number capture over [`nn_prefix_re`] — `None` when the string
/// carries no family number.
pub(crate) fn nn_prefix(s: &str) -> Option<String> {
    nn_prefix_re().captures(s).map(|c| c[1].to_string())
}

/// `^(\d{2,})\s-` — the family-title prefix face: the `\s-` separator
/// distinguishes `NN - ` titles from bare `NN` dirnames. Single source
/// alongside [`nn_prefix_re`] (CR-2 leg-3 F3).
pub(crate) fn family_title_prefix_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(\d{2,})\s-").unwrap())
}

/// CR-2 family template `{NN} - {label}｜{heading}`: an `NN - ` prefix
/// plus the U+FF5C separator (legacy `SS - ` titles never carry ｜).
/// Returns the claimed family number.
fn family_title_nn(title: &str) -> Option<String> {
    if !title.contains('｜') {
        return None;
    }
    family_title_prefix_re()
        .captures(title)
        .map(|c| c[1].to_string())
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
///
/// CR-15 symbol-aware refinement: a step carrying `x-codeReality.symbol`
/// resolves against graph.db FIRST — before the recorded file's
/// existence check — behind the freshness gate
/// ([`crate::symbol_resolve::SymbolGate`] — `current-tree` only). A
/// UNIQUE symbol decides even when the recorded file is gone
/// (relocation, H1): the corrected WARN carries the symbol-derived
/// coordinates — but only when the recorded pattern also hits the new
/// address; a pattern miss at the unique new address is an unverified
/// FAIL (H3 — a proposal the player could not follow is never
/// "corrected"). The relocation target must actually READ: an
/// absent/unreadable new address is an unverified FAIL too — an empty
/// line pool would let a line-1 `^`-shaped anchor false-verify
/// (round-2 R1). Miss / ambiguous / absent symbol / disabled gate all
/// fall back to the legacy path INCLUDING the old-file existence check
/// — a miss is never read as "definition deleted".
pub fn check_anchors(
    rel: &str,
    tour: &serde_json::Value,
    repo: &Path,
    stdout: &mut String,
) -> (Vec<String>, usize, usize) {
    let (gate, warn) = crate::symbol_resolve::SymbolGate::for_repo(repo);
    if let Some(w) = warn {
        stdout.push_str(&w);
    }
    check_anchors_gated(rel, tour, repo, &gate, stdout)
}

/// [`check_anchors`] with the caller-supplied corpus gate — `validate`
/// computes the freshness verdict once per run and threads it through
/// every tour (CR-12: one staleness evaluation per query).
pub fn check_anchors_gated(
    rel: &str,
    tour: &serde_json::Value,
    repo: &Path,
    gate: &crate::symbol_resolve::SymbolGate,
    stdout: &mut String,
) -> (Vec<String>, usize, usize) {
    use crate::symbol_resolve::SymbolLoc;
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
        let rx = regex::Regex::new(pat).ok();
        // ---- CR-15 symbol path (gate enabled + x-codeReality.symbol) --
        // H1: the symbol is consulted BEFORE the recorded file's
        // existence check — a unique address decides even when the
        // recorded file is gone
        let mut symbol_decided = false;
        if let Some(loc) = gate.locator() {
            let sym = step
                .get("x-codeReality")
                .and_then(|x| x.get("symbol"))
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty());
            if let Some(sym) = sym {
                match loc.locate(sym) {
                    Err(e) => {
                        // M4: a db error is never a silent Miss — one
                        // WARN, then the legacy face
                        stdout.push_str(&format!(
                            "[WARN] {rel} 步{} graph.db symbol 查詢失敗——退化 line/pattern: {e}\n",
                            i + 1
                        ));
                    }
                    Ok(SymbolLoc::Located {
                        file_path,
                        line_start,
                    }) => {
                        if let Some(sf) = loc.rel_file(&file_path) {
                            let moved = sf != f;
                            // when the graph still points at the recorded
                            // file, that file must exist for anything the
                            // symbol path could verify — the existence
                            // check below governs (H1)
                            if moved || p.exists() {
                                if !moved && line_start == ln {
                                    // coordinates already match the graph
                                    // — the legacy verdict has the final
                                    // word (exact / nearest-hit corrected /
                                    // unverified)
                                } else {
                                    // unique symbol, stale coordinates —
                                    // verified against the NEW address's
                                    // content (H3): the recorded pattern
                                    // must hit the new line for a
                                    // corrected WARN; a miss there is an
                                    // unverified FAIL. The target must
                                    // actually READ — an absent/unreadable
                                    // file is an unverified FAIL, never an
                                    // empty-string pool a line-1
                                    // `^`-shaped anchor could
                                    // false-verify against (round-2 R1)
                                    let target = if moved {
                                        repo.join(&sf)
                                    } else {
                                        p.clone()
                                    };
                                    let new_content =
                                        match std::fs::read_to_string(&target) {
                                            Ok(c) => c,
                                            Err(_) => {
                                                let fail = if moved {
                                                    format!(
                                                        "[FAIL] {rel} 步{} symbol 定址（source: SCIP symbol）: {f} -> {sf} L{ln}->L{line_start}；新檔不可讀（unverified）",
                                                        i + 1
                                                    )
                                                } else {
                                                    format!(
                                                        "[FAIL] {rel} 步{} symbol 定址（source: SCIP symbol）: {f} L{ln}->L{line_start}；檔案不可讀（unverified）",
                                                        i + 1
                                                    )
                                                };
                                                fails.push(fail);
                                                continue;
                                            }
                                        };
                                    let new_lines: Vec<&str> =
                                        new_content.split('\n').collect();
                                    let pat_at_new = line_start >= 1
                                        && ((line_start as usize) <= new_lines.len())
                                        && rx
                                            .as_ref()
                                            .map(|r| {
                                                r.is_match(new_lines[(line_start - 1) as usize])
                                            })
                                            .unwrap_or(false);
                                    if pat_at_new {
                                        corrected += 1;
                                        let msg = if moved {
                                            format!(
                                                "[WARN] {rel} 步{} 錨 corrected（source: SCIP symbol）: {f} -> {sf} L{ln}->L{line_start}",
                                                i + 1
                                            )
                                        } else {
                                            format!(
                                                "[WARN] {rel} 步{} 錨 corrected（source: SCIP symbol）: {f} L{ln}->L{line_start}",
                                                i + 1
                                            )
                                        };
                                        stdout.push_str(&msg);
                                        stdout.push('\n');
                                    } else {
                                        let fail = if moved {
                                            format!(
                                                "[FAIL] {rel} 步{} symbol 定址（source: SCIP symbol）: {f} -> {sf} L{ln}->L{line_start}；pattern 未命中新座標（unverified）",
                                                i + 1
                                            )
                                        } else {
                                            format!(
                                                "[FAIL] {rel} 步{} symbol 定址（source: SCIP symbol）: {f} L{ln}->L{line_start}；pattern 未命中新座標（unverified）",
                                                i + 1
                                            )
                                        };
                                        fails.push(fail);
                                    }
                                    symbol_decided = true;
                                }
                            }
                        }
                        // rel-ification failed (db from another checkout):
                        // conservative legacy fallback
                    }
                    Ok(SymbolLoc::Ambiguous(rows)) => {
                        if p.exists() {
                            stdout.push_str(&format!(
                                "[WARN] {rel} 步{} symbol 歧義（{} 列同名 symbol）——退化 line/pattern: {f}:{ln}\n",
                                i + 1,
                                rows.len()
                            ));
                            // fall through to the legacy path
                        }
                        // recorded file gone (H1): the existence check
                        // below governs — a degrade note would announce a
                        // pattern face that cannot run
                    }
                    Ok(SymbolLoc::Miss) => {
                        // absence of a graph row is never read as
                        // "definition deleted" — the legacy path,
                        // including the old-file existence check (H1),
                        // governs unchanged
                    }
                }
            }
        }
        if symbol_decided {
            continue;
        }
        // ---- legacy line/pattern path (frozen player semantics) ------
        if !p.exists() {
            fails.push(format!("[FAIL] {rel} 步{} 錨檔不存在: {f}", i + 1));
            continue;
        }
        let content = std::fs::read_to_string(&p).unwrap_or_default();
        let lines: Vec<&str> = content.split('\n').collect();
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
        let root = repo.join(tours_dir);
        if root.exists()
            && iter_tours(repo, tours_dir, true)
                .unwrap_or_default()
                .is_empty()
        {
            // Corpus dir present but zero *.tour on disk — layout or
            // extension drift is an anomaly, not a skip. Contract note:
            // deliberate deviation from the frozen tour_validate.py
            // empty-exit-0 face (adjudicated 2026-08-30; tour_upgrade
            // already crashed on this state — the family semantics now
            // agree).
            stdout.push_str(&format!(
                "[FAIL] {} 有 corpus 目錄但零 .tour 匹配——版面/副檔/路徑異常\n",
                root.display()
            ));
            return ToolOutput {
                stdout,
                stderr: String::new(),
                exit_code: 1,
            };
        }
        stdout.push_str(&format!("[WARN] {} 無 .tour\n", root.display()));
        // corpus-less repo = legitimate skip; if this repo SHOULD have a
        // corpus, check the --repo target first.
        return ToolOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let all_tours = iter_tours(repo, tours_dir, true).unwrap_or_default();
    let idx = key_index(&all_tours);
    let by_rel: BTreeMap<String, serde_json::Value> = tours.iter().cloned().collect();
    // CR-15: the symbol-coordinate gate is computed ONCE per run —
    // freshness verdict + graph.db presence decide the whole corpus;
    // degraded runs print the single WARN and behave exactly as before.
    let (gate, gate_warn) = crate::symbol_resolve::SymbolGate::for_repo(repo);
    if let Some(w) = gate_warn {
        stdout.push_str(&w);
    }
    let mut fails = Vec::new();
    let (mut n_links, mut n_files) = (0, 0);
    for (rel, tour) in &tours {
        // CR-2 dir-prefix gate: a family-template title must claim the
        // family number of the directory it lives in (tree aggregation
        // address vs declaration)
        let title = tour.get("title").and_then(|t| t.as_str()).unwrap_or("");
        if let Some(nn) = family_title_nn(title) {
            let dir = Path::new(rel)
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir_nn = nn_prefix(&dir);
            if dir_nn.as_deref() != Some(nn.as_str()) {
                fails.push(format!(
                    "[FAIL] {rel} 族號制 title NN（{nn}）≠ 目錄族號前綴（{}）",
                    dir_nn.unwrap_or(dir)
                ));
            }
        }
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
        let (af, _, _) = check_anchors_gated(rel, tour, repo, &gate, &mut stdout);
        fails.extend(af);
    }
    // CR-2 ts_key uniqueness (tiered, over the full index including
    // excluded dirs): family-template collisions FAIL (the player
    // silently lands on the first match); legacy `SS - ` collisions stay
    // WARN through the transition corpus and may graduate to FAIL once
    // fully migrated.
    let title_of: BTreeMap<&String, &str> = all_tours
        .iter()
        .map(|(rel, t)| (rel, t.get("title").and_then(|x| x.as_str()).unwrap_or("")))
        .collect();
    for (key, rels) in &idx {
        if rels.len() < 2 {
            continue;
        }
        let title_is = |r: &String, pred: &dyn Fn(&str) -> bool| {
            title_of.get(r).map(|t| pred(t)).unwrap_or(false)
        };
        if rels
            .iter()
            .any(|r| title_is(r, &|t| family_title_nn(t).is_some()))
        {
            fails.push(format!(
                "[FAIL] ts_key 撞鍵（族號制）: {} × {}——{:?}（player 撞鍵靜默落第一條）",
                truncate(key, 40),
                rels.len(),
                rels
            ));
        } else if rels
            .iter()
            .any(|r| title_is(r, &|t| ts_key_re().is_match(t)))
        {
            stdout.push_str(&format!(
                "[WARN] ts_key 撞鍵（legacy 過渡期）: {} × {}——{:?}；全遷移族號制後此級升 FAIL\n",
                truncate(key, 40),
                rels.len(),
                rels
            ));
        } else {
            // Third state (CR-2 leg-3 F2): bare titles with no prefix at
            // all — no link-key semantics of their own, so FAIL would
            // red-flag a legitimate legacy corpus; the player collision
            // harm still needs to be visible.
            stdout.push_str(&format!(
                "[WARN] ts_key 撞鍵（裸 title 無前綴）: {} × {}——{:?}（player 撞鍵靜默落第一條；裸 title 無連結鍵語義，故不升 FAIL）\n",
                truncate(key, 40),
                rels.len(),
                rels
            ));
        }
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
    let repo = crate::argparse::repo_root(&values);
    let tours_dir = values
        .get("--tours-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tours"));
    validate(&repo, &tours_dir, values.contains_key("--manifest"))
}
