//! `overlay-gen` — compile a declarative projection plan (TOML) into a
//! small overlay SCIP index (EP ep-projected-graph-overlay S1).
//!
//! Usage: overlay-gen --plan <plan.toml> --sources <dir> --out <overlay.scip>
//!                    [--report <report.toml>]
//!
//! Spawned backend (no stale WARN — pyrefly-lsp precedent). Symbol IDs go
//! through the crate's single-source constructors (`symbol::def_symbol` /
//! `target_symbol` / `pseudo_ctor_symbol`), never hand-formatted. Every
//! declared edge is gated against the planned source text at the same
//! (file, 1-based line, callee-name) grain the graph build's py_calls
//! split uses — a declared edge the planned code does not actually call
//! fails loud instead of minting a fabricated edge.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pyrefly_producer::emit::IndexEmitter;
use pyrefly_producer::symbol;
use pyrefly_producer::walk::{DefKind, DefSite, ScopeEntry};
use ruff_text_size::{TextRange, TextSize};

// ---------- plan model ----------

struct PlanMeta {
    name: String,
    graph_rev: String,
    /// pyproject identity of the TARGET repo — the SCIP symbol prefix is
    /// keyed to it, so the plan must restate it (POC: `pyrefly python
    /// <project> <version> ` prefixes alias real symbols only on match).
    project: String,
    version: String,
}

struct PlanSymbol {
    rel_path: String,
    kind: DefKind,
    name: String,
    /// (name, is_class), outermost first — is_class picks the `#` vs `.`
    /// descriptor join.
    scope: Vec<(String, bool)>,
}

struct PlanEdge {
    file: String,
    needle: String,
    to_module: String,
    to_kind: DefKind,
    to_name: String,
}

struct PlanClaim {
    to_module: String,
    to_kind: DefKind,
    to_name: String,
    note: String,
}

struct Plan {
    meta: PlanMeta,
    symbols: Vec<PlanSymbol>,
    edges: Vec<PlanEdge>,
    claims: Vec<PlanClaim>,
}

pub struct Stats {
    pub defs: usize,
    pub edges: usize,
    pub out: PathBuf,
}

fn kind_of(raw: &str, ctx: &str) -> Result<DefKind, String> {
    match raw {
        "class" => Ok(DefKind::Class),
        "function" => Ok(DefKind::Function),
        other => Err(format!("plan {ctx} kind 非法：{other}（僅 class|function）")),
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

fn expect_str(t: &toml::Table, key: &str, ctx: &str) -> Result<String, String> {
    t.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("plan {ctx} 缺字串欄位 {key}"))
}

fn parse_scope(raw: Option<&toml::Value>, ctx: &str) -> Result<Vec<(String, bool)>, String> {
    let mut out = Vec::new();
    if let Some(v) = raw {
        let arr = v.as_array().ok_or_else(|| format!("plan {ctx} scope 需陣列"))?;
        for (i, e) in arr.iter().enumerate() {
            let t = e
                .as_table()
                .ok_or_else(|| format!("plan {ctx} scope[{i}] 需 table（name/class）"))?;
            let name = expect_str(t, "name", &format!("{ctx} scope[{i}]"))?;
            let is_class = t.get("class").and_then(|v| v.as_bool()).unwrap_or(false);
            out.push((name, is_class));
        }
    }
    Ok(out)
}

fn parse_plan(text: &str) -> Result<Plan, String> {
    let t: toml::Table = text
        .parse()
        .map_err(|e| format!("plan TOML parse 失敗：{e}"))?;
    let meta_t = t
        .get("meta")
        .and_then(|v| v.as_table())
        .ok_or("plan 缺 [meta] 區塊")?;
    let name = expect_str(meta_t, "name", "[meta]")?;
    if !valid_name(&name) {
        return Err(format!("plan [meta] name 非法：{name}（僅 A-Za-z0-9_-）"));
    }
    let meta = PlanMeta {
        name,
        graph_rev: meta_t
            .get("graph_rev")
            .and_then(|v| v.as_str())
            .unwrap_or("unstamped")
            .to_string(),
        project: expect_str(meta_t, "project", "[meta]")?,
        version: expect_str(meta_t, "version", "[meta]")?,
    };

    let mut symbols = Vec::new();
    if let Some(v) = t.get("symbols").and_then(|v| v.as_array()) {
        for (i, e) in v.iter().enumerate() {
            let ctx = format!("[[symbols]][{i}]");
            let e = e.as_table().ok_or_else(|| format!("plan {ctx} 需 table"))?;
            symbols.push(PlanSymbol {
                rel_path: expect_str(e, "rel_path", &ctx)?,
                kind: kind_of(&expect_str(e, "kind", &ctx)?, &ctx)?,
                name: expect_str(e, "name", &ctx)?,
                scope: parse_scope(e.get("scope"), &ctx)?,
            });
        }
    }

    let mut edges = Vec::new();
    if let Some(v) = t.get("edges").and_then(|v| v.as_array()) {
        for (i, e) in v.iter().enumerate() {
            let ctx = format!("[[edges]][{i}]");
            let e = e.as_table().ok_or_else(|| format!("plan {ctx} 需 table"))?;
            edges.push(PlanEdge {
                file: expect_str(e, "file", &ctx)?,
                needle: expect_str(e, "needle", &ctx)?,
                to_module: expect_str(e, "to_module", &ctx)?,
                to_kind: kind_of(&expect_str(e, "to_kind", &ctx)?, &ctx)?,
                to_name: expect_str(e, "to_name", &ctx)?,
            });
        }
    }

    let mut claims = Vec::new();
    if let Some(v) = t.get("claims").and_then(|v| v.as_array()) {
        for (i, e) in v.iter().enumerate() {
            let ctx = format!("[[claims]][{i}]");
            let e = e.as_table().ok_or_else(|| format!("plan {ctx} 需 table"))?;
            claims.push(PlanClaim {
                to_module: expect_str(e, "to_module", &ctx)?,
                to_kind: kind_of(&expect_str(e, "to_kind", &ctx)?, &ctx)?,
                to_name: expect_str(e, "to_name", &ctx)?,
                note: e
                    .get("note")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }

    // B7b pairing constraint: a constructor edge (to_kind=class) needs the
    // pseudo-ctor DEF, which is minted from the class's own [[symbols]]
    // entry — a real-repo class target would have no DEF anywhere in the
    // merged index and the graph build's def-symbol gate would drop the
    // ref. Planned classes only (recorded MVP boundary).
    for (i, e) in edges.iter().enumerate() {
        if e.to_kind == DefKind::Class
            && !symbols.iter().any(|s| {
                s.kind == DefKind::Class
                    && s.name == e.to_name
                    // CR-7: the pairing must land in the SAME module the
                    // edge declares, or the minted pseudo-ctor ref points
                    // at a symbol with no DEF anywhere.
                    && pyrefly_producer::module_of_rel(&s.rel_path) == e.to_module
            })
        {
            return Err(format!(
                "plan [[edges]][{i}] ctor 邊目標 class {} 未宣告於 [[symbols]]（B7b 配對僅支援同 module 的 planned class）",
                e.to_name
            ));
        }
    }
    for (i, c) in claims.iter().enumerate() {
        if c.to_kind == DefKind::Class {
            return Err(format!(
                "plan [[claims]][{i}] 不支援 class 目標 {}——B7b 限制下 class 宣稱無法判定 WIRED（偽 ctor DEF 僅在 planned class mint）；請以 function 目標宣稱",
                c.to_name
            ));
        }
    }

    Ok(Plan {
        meta,
        symbols,
        edges,
        claims,
    })
}

// ---------- helpers ----------

fn first_occurrence(src: &str, needle: &str) -> Option<usize> {
    src.find(needle)
}

fn line_of(src: &str, off: usize) -> i64 {
    1 + src[..off].bytes().filter(|&b| b == b'\n').count() as i64
}

fn rng(off: usize, len: usize) -> TextRange {
    TextRange::new(TextSize::from(off as u32), TextSize::from((off + len) as u32))
}

/// Stmt start = first non-whitespace char of the def's line (covers
/// `class X` / `def f` and indented methods).
fn stmt_start(src: &str, name_off: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = name_off;
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    while i < name_off && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

// ---------- core ----------

pub fn generate(
    plan_path: &Path,
    sources: &Path,
    out: &Path,
    report_path: Option<&Path>,
) -> Result<Stats, String> {
    let text = std::fs::read_to_string(plan_path)
        .map_err(|e| format!("讀 plan {} 失敗：{e}", plan_path.display()))?;
    let plan = parse_plan(&text)?;

    // Load planned sources once; rels drive the py_calls scan set.
    let mut rels: BTreeSet<String> = BTreeSet::new();
    for s in &plan.symbols {
        rels.insert(s.rel_path.clone());
    }
    for e in &plan.edges {
        rels.insert(e.file.clone());
    }
    let mut srcs: Vec<(String, String)> = Vec::new();
    for rel in &rels {
        let text = std::fs::read_to_string(sources.join(rel))
            .map_err(|e| format!("planned source 缺檔 {}（--sources {}）：{e}", rel, sources.display()))?;
        srcs.push((rel.clone(), text));
    }
    let src_of = |rel: &str| -> Result<&str, String> {
        srcs.iter()
            .find(|(r, _)| r == rel)
            .map(|(_, t)| t.as_str())
            .ok_or_else(|| format!("internal: rel {rel} 未載入"))
    };

    // Consistency gate: declared edges must land on real call sites in the
    // planned text ((file, 1-based line, callee name) — the py_calls grain).
    let (sites, warns) = code_reality::py_calls::call_sites(sources, &rels);
    let mut failures: Vec<String> = Vec::new();
    for (i, e) in plan.edges.iter().enumerate() {
        let src = src_of(&e.file)?;
        let Some(off) = first_occurrence(src, &e.needle) else {
            failures.push(format!(
                "  [[edges]][{i}] needle 不在 {}：{}",
                e.file, e.needle
            ));
            continue;
        };
        // The callee name must sit inside the needle (occurrence range).
        if e.needle.find(&e.to_name).is_none() {
            failures.push(format!(
                "  [[edges]][{i}] needle {} 不含 callee 名 {}",
                e.needle, e.to_name
            ));
            continue;
        }
        let line = line_of(src, off);
        if !sites.contains(&(e.file.clone(), line, e.to_name.clone())) {
            failures.push(format!(
                "  [[edges]][{i}] {}：{} 位於行 {line}，非 {} 的 call site（宣告與程式碼不符）",
                e.file, e.needle, e.to_name
            ));
        }
    }
    for w in &warns {
        failures.push(format!("  [py_calls] {w}"));
    }
    if !failures.is_empty() {
        return Err(format!(
            "一致性 gate 失敗（{} 條宣告邊 / {} warns）——不產出 overlay：\n{}",
            plan.edges.len(),
            warns.len(),
            failures.join("\n")
        ));
    }

    // Mint: one document per file (group by rel_path, plan order for the
    // files, declaration order within a file), symbol IDs via the crate's
    // single-source constructors.
    let disc = symbol::discriminator(&plan.meta.project, &plan.meta.version);
    let mut em = IndexEmitter::new();

    // Ordered unique file list with per-file symbol indices.
    let mut file_order: Vec<String> = Vec::new();
    for s in &plan.symbols {
        if !file_order.contains(&s.rel_path) {
            file_order.push(s.rel_path.clone());
        }
    }
    for e in &plan.edges {
        if !file_order.contains(&e.file) {
            return Err(format!(
                "[[edges]] file {} 無對應 [[symbols]]（邊所在檔需有 planned 符號）",
                e.file
            ));
        }
    }

    let mut defs_minted = 0usize;
    for rel in &file_order {
        let src = src_of(rel)?;
        em.start_module(rel, src);
        let idxs: Vec<usize> = plan
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| &s.rel_path == rel)
            .map(|(i, _)| i)
            .collect();
        // Node ranges: stmt start → next symbol's stmt start (or trimmed
        // EOF) — spans-based caller attribution reads the enclosing range.
        let starts: Vec<usize> = idxs
            .iter()
            .map(|&i| {
                let s = &plan.symbols[i];
                let off = first_occurrence(src, &s.name)
                    .ok_or_else(|| format!("symbol {} 不在 {} 中", s.name, rel))?;
                Ok(stmt_start(src, off))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let end_all = src.trim_end().len();
        // CR-3: node ranges derive from starts[k+1] - starts[k]; the plan
        // must declare symbols in source order or the subtraction would
        // underflow (a plan-author-triggerable crash, not a plan error).
        for w in starts.windows(2) {
            if w[1] <= w[0] {
                return Err(format!(
                    "plan [[symbols]] 於 {rel} 未按 source 出現順序宣告（stmt offsets 需嚴格遞增）"
                ));
            }
        }
        for (k, &i) in idxs.iter().enumerate() {
            let s = &plan.symbols[i];
            let name_off = first_occurrence(src, &s.name).expect("checked above");
            let node_end = if k + 1 < starts.len() { starts[k + 1] } else { end_all };
            let scope: Vec<ScopeEntry> = s
                .scope
                .iter()
                .map(|(n, is_class)| ScopeEntry {
                    name: n.clone(),
                    is_class: *is_class,
                })
                .collect();
            let def = DefSite {
                kind: s.kind,
                name: s.name.clone(),
                name_range: rng(name_off, s.name.len()),
                node_range: rng(starts[k], node_end - starts[k]),
                scope,
            };
            let sym = symbol::def_symbol(&disc, &pyrefly_producer::module_of_rel(rel), &def);
            em.push_def(&sym, def.name_range, def.node_range);
            defs_minted += 1;
            if s.kind == DefKind::Class {
                // B7b: the pseudo-constructor DEF must exist or the graph
                // build's def-symbol gate drops the minted ctor-call ref.
                let pseudo = symbol::pseudo_ctor_symbol(&sym).expect("class symbol");
                em.push_def(&pseudo, def.name_range, def.node_range);
                defs_minted += 1;
            }
        }
        // Edges live in this file's document — emit while it is current.
        for e in &plan.edges {
            if &e.file != rel {
                continue;
            }
            let src = src_of(&e.file)?;
            let needle_off = first_occurrence(src, &e.needle).expect("gate checked");
            let name_in_needle = e.needle.find(&e.to_name).expect("gate checked");
            let call_range = rng(needle_off + name_in_needle, e.to_name.len());
            // Planned-class ctor edges (validated above) ref the pseudo-
            // ctor form — its DEF was minted with the class (B7b pairing).
            let target = match e.to_kind {
                DefKind::Class => {
                    let cls_sym = symbol::target_symbol(
                        &disc,
                        &e.to_module,
                        &[],
                        DefKind::Class,
                        &e.to_name,
                    );
                    symbol::pseudo_ctor_symbol(&cls_sym).expect("class symbol")
                }
                DefKind::Function | DefKind::Variable => symbol::target_symbol(
                    &disc,
                    &e.to_module,
                    &[],
                    DefKind::Function,
                    &e.to_name,
                ),
            };
            em.push_call_reference(&target, call_range);
        }
    }

    em.write(out).map_err(|e| format!("寫 overlay 失敗：{e}"))?;

    // Report (TOML): the orchestrator's content contract — S2 parses this
    // instead of the plan (zero schema knowledge outside this bin).
    let mut rep = String::new();
    rep.push_str(&format!(
        "name = \"{}\"\nproject = \"{}\"\nversion = \"{}\"\ngraph_rev = \"{}\"\n",
        toml_escape(&plan.meta.name),
        toml_escape(&plan.meta.project),
        toml_escape(&plan.meta.version),
        toml_escape(&plan.meta.graph_rev)
    ));
    rep.push_str(&format!("minted_defs = {defs_minted}\n"));
    rep.push_str(&format!("minted_edges = {}\n", plan.edges.len()));
    rep.push_str(&format!(
        "overlay_files = [{}]\n",
        file_order
            .iter()
            .map(|f| format!("\"{}\"", toml_escape(f)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    // Deduped touched targets (graft-surface query set).
    let mut touched: Vec<(&str, &str, &str)> = Vec::new();
    for e in &plan.edges {
        let key = (e.to_module.as_str(), "function", e.to_name.as_str());
        if e.to_kind == DefKind::Class {
            continue; // ctor edges surface via the class's reverse chain
        }
        if !touched.iter().any(|(m, _, n)| *m == key.0 && *n == key.2) {
            touched.push(key);
        }
    }
    for (module, kind, name) in &touched {
        rep.push_str(&format!(
            "\n[[touched]]\nmodule = \"{}\"\nkind = \"{kind}\"\nname = \"{}\"\n",
            toml_escape(module),
            toml_escape(name)
        ));
    }
    for s in &plan.symbols {
        rep.push_str(&format!(
            "\n[[symbols]]\nmodule = \"{}\"\nkind = \"{}\"\nname = \"{}\"\n",
            toml_escape(&pyrefly_producer::module_of_rel(&s.rel_path)),
            match s.kind {
                DefKind::Class => "class",
                DefKind::Function => "function",
                DefKind::Variable => "variable",
            },
            toml_escape(&s.name)
        ));
    }
    for c in &plan.claims {
        rep.push_str(&format!(
            "\n[[claims]]\nto_module = \"{}\"\nto_kind = \"{}\"\nto_name = \"{}\"\nnote = \"{}\"\n",
            toml_escape(&c.to_module),
            match c.to_kind {
                DefKind::Class => "class",
                DefKind::Function => "function",
                DefKind::Variable => "variable",
            },
            toml_escape(&c.to_name),
            toml_escape(&c.note)
        ));
    }
    if let Some(rp) = report_path {
        std::fs::write(rp, &rep).map_err(|e| format!("寫 report {} 失敗：{e}", rp.display()))?;
    }

    Ok(Stats {
        defs: defs_minted,
        edges: plan.edges.len(),
        out: out.to_path_buf(),
    })
}

fn main() -> ExitCode {
    let mut plan: Option<String> = None;
    let mut sources: Option<String> = None;
    let mut out: Option<String> = None;
    let mut report: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--plan" => plan = args.next(),
            "--sources" => sources = args.next(),
            "--out" => out = args.next(),
            "--report" => report = args.next(),
            "--help" | "-h" => {
                println!("usage: overlay-gen --plan <plan.toml> --sources <dir> --out <overlay.scip> [--report <report.toml>]");
                return ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                let rev = option_env!("CR_BUILD_REV");
                let face = match rev {
                    Some(r) => format!("{}+{}", env!("CARGO_PKG_VERSION"), r),
                    None => env!("CARGO_PKG_VERSION").to_string(),
                };
                println!("{face}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unrecognized argument {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let (Some(plan), Some(sources), Some(out)) = (plan, sources, out) else {
        eprintln!("error: --plan/--sources/--out are required");
        return ExitCode::FAILURE;
    };
    match generate(
        Path::new(&plan),
        Path::new(&sources),
        Path::new(&out),
        report.as_deref().map(Path::new),
    ) {
        Ok(s) => {
            println!(
                "[OK] overlay-gen: {} defs, {} edges, gate {}/{} -> {}",
                s.defs,
                s.edges,
                s.edges,
                s.edges,
                s.out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[FAIL] overlay-gen: {e}");
            ExitCode::FAILURE
        }
    }
}
