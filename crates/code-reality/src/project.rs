//! `project` — projected-graph orchestrator (EP ep-projected-graph-overlay
//! S2). Compiles an EP's declarative projection plan into a projected
//! world and answers EP-planning queries against it:
//!
//!   plan.toml (+ planned sources) --spawn--> overlay-gen → overlay.scip
//!   concat(real index, overlay) → `.code-reality/projections/<stem>/`
//!   → graft surface / new-symbol reverse chain / HOLE / MISSING report.
//!
//! Every projected edge is a DECLARATION, not evidence — the report
//! labels everything `[projected]` and counts hypothetical edges (the
//! Claim→Evidence→Trust laundering trap guard). Query layer rides the
//! protobuf face only (`engine::load_index` + `fn_spans`): deterministic,
//! face-shape independent (the sqlite face's fn-tail gate hides class
//! DEFs), and zero sidecar writes — the real slot stays byte-identical
//! (non-pollution invariant, test-pinned).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::build::{concat_scip, producer_roots, resolve_bin};
use crate::callers;
use crate::engine::{self, default_index_path};
use crate::ToolOutput;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--plan",
            short: None,
            kind: Kind::Value { metavar: "PLAN" },
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = "usage: code-reality project --repo <repo> --plan <plan.toml> [--json]
  --repo REPO     repo whose real index anchors the projection
  --plan PLAN     projection plan (planned sources live in <plan dir>/sources/)
  --json          machine-readable report
";

#[derive(Debug)]
pub enum ProjectError {
    Env(String),
    Core(String),
}

// ---------- report contract (overlay-gen --report, TOML) ----------

struct Report {
    graph_rev: String,
    project: String,
    version: String,
    minted_edges: usize,
    overlay_files: BTreeSet<String>,
    touched: Vec<(String, String)>, // (module, name) — module keeps bare-name
    // queries from colliding across same-named symbols (review CR-1)
    symbols: Vec<(String, String)>,
    claims: Vec<Claim>,
}

struct Claim {
    to_module: String,
    to_name: String,
    note: String,
}

fn tbl_str(t: &toml::Table, key: &str) -> Option<String> {
    t.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn parse_report(text: &str) -> Result<Report, String> {
    let t: toml::Table = text
        .parse()
        .map_err(|e| format!("overlay report parse 失敗：{e}"))?;
    let mut overlay_files = BTreeSet::new();
    if let Some(arr) = t.get("overlay_files").and_then(|v| v.as_array()) {
        for f in arr.iter().filter_map(|v| v.as_str()) {
            overlay_files.insert(f.to_string());
        }
    }
    let collect = |key: &str| -> Vec<(String, String)> {
        t.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_table())
                    .filter_map(|tt| {
                        Some((
                            tbl_str(tt, "module")?,
                            tbl_str(tt, "name")?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut claims = Vec::new();
    if let Some(arr) = t.get("claims").and_then(|v| v.as_array()) {
        for e in arr.iter().filter_map(|e| e.as_table()) {
            claims.push(Claim {
                to_module: tbl_str(e, "to_module").unwrap_or_default(),
                to_name: tbl_str(e, "to_name").unwrap_or_default(),
                note: tbl_str(e, "note").unwrap_or_default(),
            });
        }
    }
    Ok(Report {
        graph_rev: tbl_str(&t, "graph_rev").unwrap_or_else(|| "unstamped".into()),
        project: tbl_str(&t, "project").unwrap_or_default(),
        version: tbl_str(&t, "version").unwrap_or_default(),
        minted_edges: t
            .get("minted_edges")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as usize,
        overlay_files,
        touched: collect("touched"),
        symbols: collect("symbols"),
        claims,
    })
}

// ---------- query layer (protobuf face only, zero sidecar IO) ----------

struct QueryFace {
    index: scip::types::Index,
    spans: BTreeMap<String, Vec<engine::FnSpan>>,
}

fn load_query_face(index_path: &Path) -> Result<QueryFace, String> {
    let loaded = engine::load_index(index_path)?;
    let (spans, _warns) = engine::fn_spans(&loaded.index);
    Ok(QueryFace {
        index: loaded.index,
        spans,
    })
}

/// Attributed caller sites for `name`, or None when it has no DEF in this
/// index (callers semantics: fn-span containment + item-level remainder).
/// `module` scopes the bare-name query: find_defs matches ANY symbol
/// whose tail equals the name, so same-named symbols across modules
/// would pollute the verdicts without the `` `module`/ `` prefix filter
/// (review CR-1).
fn callers_of(face: &QueryFace, name: &str, module: Option<&str>) -> Option<callers::CallersResult> {
    let parsed = engine::Query::parse(name);
    let defs_all = engine::find_defs(&face.index, &parsed);
    let defs: std::collections::BTreeMap<String, Vec<String>> = match module {
        None => defs_all,
        Some(m) => {
            let prefix = format!("`{m}`/");
            let scoped: std::collections::BTreeMap<String, Vec<String>> = defs_all
                .into_iter()
                .filter(|(sym, _)| sym.contains(&prefix))
                .collect();
            scoped
        }
    };
    if defs.is_empty() {
        return None;
    }
    let symbols: BTreeSet<String> = defs.keys().cloned().collect();
    let rows = engine::refs_rows(&face.index, &symbols);
    Some(callers::attribute(&rows, &face.spans))
}

/// Flat site set: (rel_path, line) → attributed caller symbol ("" for
/// item-level sites).
fn site_map(res: &callers::CallersResult) -> BTreeMap<(String, i64), String> {
    let mut out = BTreeMap::new();
    for c in &res.callers {
        for (rel, line) in &c.sites {
            out.insert((rel.clone(), *line), c.symbol.clone());
        }
    }
    for (rel, line) in &res.item_level {
        out.insert((rel.clone(), *line), String::new());
    }
    out
}

// ---------- core ----------

fn valid_stem(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Claim note rendered inline when present (provenance for the verdict).
fn note_suffix(note: &str) -> String {
    if note.is_empty() {
        String::new()
    } else {
        format!("（note: {note}）")
    }
}

pub fn project_repo(
    repo: &Path,
    plan_path: &Path,
    roots: &[PathBuf],
    json: bool,
) -> Result<String, ProjectError> {
    let env = |m: String| ProjectError::Env(m);
    let core = |m: String| ProjectError::Core(m);

    let real = default_index_path(repo).map_err(core)?;
    if !real.exists() {
        return Err(env(format!(
            "真實 index 不存在（{}）——先跑 code-reality build --repo {}\n",
            real.display(),
            repo.display()
        )));
    }
    let plan_path = plan_path
        .canonicalize()
        .map_err(|e| env(format!("plan {} 無效：{e}", plan_path.display())))?;
    let stem = plan_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| env("plan 檔名無 stem".into()))?;
    if !valid_stem(stem) {
        return Err(env(format!(
            "plan 檔名 stem 非法：{stem}（僅 A-Za-z0-9_-，作為 projection slot 目錄名）"
        )));
    }
    let plan_root = plan_path.parent().unwrap_or(Path::new("."));
    let sources = plan_root.join("sources");
    if !sources.is_dir() {
        return Err(env(format!(
            "planned sources 目錄不存在：{}（plan 同層 sources/ 放假想 .py）",
            sources.display()
        )));
    }

    let slot = repo.join(".code-reality/projections").join(stem);
    // Idempotent rebuild: drop the whole slot (index + sidecars + reports
    // of the previous run) before minting the new one.
    if slot.exists() {
        std::fs::remove_dir_all(&slot).map_err(|e| core(format!("清 slot 失敗：{e}")))?;
    }
    std::fs::create_dir_all(&slot).map_err(|e| core(format!("建 slot 失敗：{e}")))?;

    let bin = resolve_bin(
        "overlay-gen",
        roots,
        "安裝：uv tool install pyrefly-producer（或 cargo install --path crates/pyrefly-producer）",
    )
    .map_err(|e| env(format!("{e}\n")))?;
    let overlay = slot.join("overlay.scip");
    let report_path = slot.join("overlay-report.toml");
    let merged = slot.join("index.scip");
    let out = Command::new(&bin)
        .arg("--plan")
        .arg(&plan_path)
        .arg("--sources")
        .arg(&sources)
        .arg("--out")
        .arg(&overlay)
        .arg("--report")
        .arg(&report_path)
        .output()
        .map_err(|e| env(format!("spawn {} 失敗：{e}", bin.display())))?;
    if !out.status.success() {
        return Err(env(format!(
            "overlay-gen 失敗（{}）：\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let report_text =
        std::fs::read_to_string(&report_path).map_err(|e| core(format!("讀 report 失敗：{e}")))?;
    let rep = parse_report(&report_text).map_err(core)?;

    // cat-merge: the real index bytes + overlay bytes are a legal merged
    // scip.Index (repeated-field stacking; build-umbrella precedent).
    std::fs::copy(&real, &merged).map_err(|e| core(format!("複製真實 index 失敗：{e}")))?;
    concat_scip(&merged, &overlay).map_err(core)?;

    let real_face = load_query_face(&real).map_err(core)?;
    let proj_face = load_query_face(&merged).map_err(core)?;

    // Identity guard: the overlay joins the real index by EXACT symbol ID
    // (`pyrefly python <project> <version> ` prefix). A plan whose [meta]
    // identity differs mints refs that silently join nothing — minted
    // edges counted, yet +0 graft and HOLE-instead-of-WIRED verdicts
    // (found via the ai-rules dogfood relay). Fail loud with the real
    // values; one resolving probe (touched first, then claims) suffices.
    let declared = format!("pyrefly python {} {}", rep.project, rep.version);
    let probes = rep
        .touched
        .iter()
        .map(|(m, n)| (m.clone(), n.clone()))
        .chain(rep.claims.iter().map(|c| (c.to_module.clone(), c.to_name.clone())));
    for (module, name) in probes {
        let parsed = engine::Query::parse(&name);
        let defs = engine::find_defs(&real_face.index, &parsed);
        if let Some(def_sym) = defs.keys().next() {
            // Compare by prefix match, not word-position parsing — the
            // grammar lives in symbol.rs (pinned by symbol_form tests);
            // take(4) here is display-only for the mismatch message.
            let declared_with_space = format!("{declared} ");
            if !def_sym.starts_with(&declared_with_space) {
                let prefix = def_sym.split(' ').take(4).collect::<Vec<_>>().join(" ");
                return Err(env(format!(
                    "plan [meta] project/version 與真實 index 前綴不符：plan=\"{} {}\"、index=\"{prefix}\"——project/version 必須等於目標 repo 的 pyproject identity（symbol ID 是 join 鍵，不符則投影邊全部靜默不歸因）\n",
                    rep.project, rep.version
                )));
            }
            let _ = module;
            break;
        }
    }

    // ---- graft surface: touched real symbols, real vs projected sites
    let mut graft_lines: Vec<String> = Vec::new();
    let mut graft_json: Vec<serde_json::Value> = Vec::new();
    for (module, name) in &rep.touched {
        let proj = callers_of(&proj_face, name, Some(module));
        let real_sites_res = callers_of(&real_face, name, Some(module));
        let Some(proj) = proj else {
            graft_lines.push(format!("[projected][MISSING] {module}/{name} — 不在投影 index（touched 目標無 DEF）"));
            graft_json.push(serde_json::json!({
                "symbol": name, "module": module, "verdict": "MISSING",
            }));
            continue;
        };
        let proj_sites = site_map(&proj);
        let real_sites = real_sites_res.map(|r| site_map(&r)).unwrap_or_default();
        let mut delta: Vec<String> = Vec::new();
        for ((rel, line), caller) in &proj_sites {
            if !real_sites.contains_key(&(rel.clone(), *line)) {
                delta.push(if caller.is_empty() {
                    format!("  + {rel}:{line} ← item-level")
                } else {
                    format!("  + {rel}:{line} ← {}", engine::tail(caller))
                });
            }
        }
        graft_lines.push(format!(
            "[projected] {name}: real {} sites → projected {} sites（+{} 投影）",
            real_sites.len(),
            proj_sites.len(),
            delta.len()
        ));
        graft_lines.extend(delta.iter().cloned());
        graft_json.push(serde_json::json!({
            "symbol": name,
            "real_sites": real_sites.len(),
            "projected_sites": proj_sites.len(),
            "projected_only": delta.len(),
        }));
    }

    // ---- new-symbol reverse chain
    let mut reverse_lines: Vec<String> = Vec::new();
    let mut reverse_json: Vec<serde_json::Value> = Vec::new();
    reverse_lines.push("[projected] 新符號反向鏈（planned symbols 的投影 callers）：".into());
    for (module, name) in &rep.symbols {
        match callers_of(&proj_face, name, Some(module)) {
            None => {
                reverse_lines.push(format!("[projected][MISSING] {module}/{name} — 投影 index 無 DEF"));
                reverse_json.push(serde_json::json!({
                    "symbol": name, "module": module, "verdict": "MISSING",
                }));
            }
            Some(res) => {
                let sites = site_map(&res);
                reverse_lines.push(format!(
                    "[projected] {name}: {} callers（{} sites）",
                    res.callers.len(),
                    sites.len()
                ));
                reverse_json.push(serde_json::json!({
                    "symbol": name,
                    "callers": res.callers.len(),
                    "sites": sites.len(),
                }));
            }
        }
    }

    // ---- claims: HOLE (DEF but no edge from overlay files) / MISSING
    let mut claim_lines: Vec<String> = Vec::new();
    let mut claim_json: Vec<serde_json::Value> = Vec::new();
    for c in &rep.claims {
        match callers_of(&proj_face, &c.to_name, Some(&c.to_module)) {
            None => {
                claim_lines.push(format!(
                    "[projected][MISSING] {} — 不在投影 index（宣稱的符號不存在）{}",
                    c.to_name,
                    note_suffix(&c.note)
                ));
                claim_json.push(serde_json::json!({
                    "claim": c.to_name, "verdict": "MISSING", "note": c.note,
                }));
            }
            Some(res) => {
                let sites = site_map(&res);
                let wired: Vec<&(String, i64)> = sites
                    .keys()
                    .filter(|(rel, _)| rep.overlay_files.contains(rel))
                    .collect();
                if wired.is_empty() {
                    claim_lines.push(format!(
                        "[projected][HOLE] {} — 有 DEF 但計畫檔零呼叫邊（整合宣稱未接線）{}",
                        c.to_name,
                        note_suffix(&c.note)
                    ));
                    claim_json.push(serde_json::json!({
                        "claim": c.to_name, "verdict": "HOLE", "note": c.note,
                    }));
                } else {
                    claim_lines.push(format!(
                        "[projected] {} — 已接線（{} 個計畫檔 site）",
                        c.to_name,
                        wired.len()
                    ));
                    claim_json.push(serde_json::json!({
                        "claim": c.to_name, "verdict": "WIRED", "sites": wired.len(),
                    }));
                }
            }
        }
    }

    // ---- graph-rev cross-check (stamped meta vs plan declaration)
    let stamped = engine::stamped_head(&real);
    let mut rev_note = String::new();
    if rep.graph_rev != "unstamped" && rep.graph_rev != stamped {
        rev_note = if stamped.is_empty() {
            format!(
                "[WARN] graph rev 對照不到：plan 宣告 {}，真實 index 無 stamped meta\n",
                rep.graph_rev
            )
        } else {
            format!(
                "[WARN] graph rev 不一致：plan 宣告 {}，index stamped {}——投影基準可能漂移\n",
                rep.graph_rev, stamped
            )
        };
    }

    let rev_warn = !rev_note.is_empty();
    if json {
        let v = serde_json::json!({
            "projected": true,
            "hypothetical_edges": rep.minted_edges,
            "graft": graft_json,
            "reverse": reverse_json,
            "claims": claim_json,
            "graph_rev": rep.graph_rev,
            "stamped": stamped,
            "rev_warn": rev_warn,
            "slot": slot.display().to_string(),
        });
        return Ok(format!("{}\n", crate::common::to_json_indent1(&v)));
    }

    let mut out = String::new();
    out.push_str(&format!(
        "[projected] graft surface（假想邊 {} 條——宣告，非證據）：\n",
        rep.minted_edges
    ));
    for l in &graft_lines {
        out.push_str(l);
        out.push('\n');
    }
    for l in &reverse_lines {
        out.push_str(l);
        out.push('\n');
    }
    if !rep.claims.is_empty() {
        out.push_str("[projected] 整合宣稱判定：\n");
        for l in &claim_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out.push_str(&rev_note);
    out.push_str(&format!(
        "slot: {}\n（重跑同 plan 冪等重建；舊 projections 請手動清除）\n",
        slot.display()
    ));
    Ok(out)
}

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((_tool, toks)) = argv.split_first() else {
        return ToolOutput::fail(HELP.trim_end());
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
    let json = values.contains_key("--json");
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let Some(plan) = values.get("--plan").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --plan");
    };
    match project_repo(Path::new(&repo), Path::new(&plan), &producer_roots(), json) {
        Ok(stdout) => ToolOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        },
        Err(e) => match e {
            ProjectError::Env(m) => ToolOutput::fail(format!("project: {m}")),
            ProjectError::Core(m) => ToolOutput::crash(format!("project: {m}")),
        },
    }
}
