//! `hazard` — the frozen `code_reality/hazard.py` contract: dynamic
//! dispatch hazard detection as the hub_refs hazard stage (pure judgment
//! layer). Static callers (CRG / Tree-sitter) cannot see dynamic
//! dispatch; the six rules here feed the "N prod callers ⚠ K dynamic
//! hazards" annotation that guards against "no refs, safe to delete"
//! misjudgments.
//!
//! Layering (frozen): resident AST-level (zero rg cost, every callers
//! query) = registry auto-discovery + strentenum/protocol existence;
//! triggered rg-level (static_prod ≤ 2 or --hazard) = all six rules with
//! counts. rg output lines are `path:line:content` strings.

use crate::common::{assert_db_unchanged, connect_ro, db_mtime_ns, repo_relative};
use crate::profile::{is_excluded, HazardRegistry, Profile};
use std::path::{Path, PathBuf};

/// rg match-line type: `path:line:content` (matches Python `RgRunner`).
pub type RgLine = String;
/// Injected rg runner (tests substitute synthetic line sets).
pub type RgRunner<'a> = &'a dyn Fn(&[&str]) -> Result<Vec<RgLine>, String>;

const STR_ENUM_BASES: [&str; 2] = ["StrEnum", "BaseStrEnum"];
const PROTOCOL_BASES: [&str; 1] = ["Protocol"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SymbolFacts {
    pub name: String,
    pub is_class: bool,
    pub bases: Vec<String>,
    pub is_strentenum: bool,
    pub is_protocol: bool,
    pub enum_values: Vec<String>,
    /// repo-relative defining file (None = outside repo / unresolved)
    pub rel_path: Option<String>,
    pub module: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HazardFinding {
    pub kind: String,
    pub count: i64,
    pub summary: String,
    pub evidence: Vec<String>,
    /// ordered sub-counts (prod/test etc.) — Python dict insertion order
    pub detail: Vec<(String, i64)>,
}

impl HazardFinding {
    fn new(kind: &str, count: i64, summary: String) -> Self {
        Self {
            kind: kind.to_string(),
            count,
            summary,
            evidence: Vec::new(),
            detail: Vec::new(),
        }
    }
}

/// Source string → symbol facts via AST (`hazard.py:88-114`): ClassDef
/// walk (any nesting); bases compared as name strings (import aliases
/// unresolved — recorded gap); SyntaxError → empty facts.
pub fn parse_symbol_facts(source: &str, symbol: &str) -> SymbolFacts {
    let mut facts = SymbolFacts {
        name: symbol.to_string(),
        ..Default::default()
    };
    let Ok(parsed) = ruff_python_parser::parse_module(source) else {
        return facts; // SyntaxError tolerance (hazard.py:96-98)
    };
    // ast.walk + break: FIRST matching ClassDef wins (hazard.py:99-113)
    if let Some(cls) = find_class_defs(parsed.syntax().body.as_slice(), symbol)
        .into_iter()
        .next()
    {
        facts.is_class = true;
        facts.bases = cls.bases().iter().filter_map(dotted_base).collect();
        let has = |n: &str| facts.bases.iter().any(|b| b == n);
        facts.is_strentenum = STR_ENUM_BASES.iter().any(|b| has(b)) || (has("str") && has("Enum"));
        facts.is_protocol = PROTOCOL_BASES.iter().any(|b| has(b));
        facts.enum_values = extract_str_members(&cls.body);
    }
    facts
}

/// Recursive statement walk collecting ClassDefs with the given name —
/// the equivalent of `ast.walk` for statement-bearing nodes.
fn find_class_defs<'a>(
    body: &'a [ruff_python_ast::Stmt],
    name: &str,
) -> Vec<&'a ruff_python_ast::StmtClassDef> {
    let mut out = Vec::new();
    let mut stack: Vec<&'a [ruff_python_ast::Stmt]> = vec![body];
    while let Some(stmts) = stack.pop() {
        for stmt in stmts {
            match stmt {
                ruff_python_ast::Stmt::ClassDef(c) if c.name.as_str() == name => {
                    out.push(c);
                }
                ruff_python_ast::Stmt::ClassDef(c) => stack.push(&c.body),
                ruff_python_ast::Stmt::FunctionDef(f) => stack.push(&f.body),
                ruff_python_ast::Stmt::If(s) => {
                    stack.push(&s.body);
                    for cl in &s.elif_else_clauses {
                        stack.push(&cl.body);
                    }
                }
                ruff_python_ast::Stmt::For(s) => {
                    stack.push(&s.body);
                    stack.push(&s.orelse);
                }
                ruff_python_ast::Stmt::While(s) => {
                    stack.push(&s.body);
                    stack.push(&s.orelse);
                }
                ruff_python_ast::Stmt::With(s) => stack.push(&s.body),
                ruff_python_ast::Stmt::Try(s) => {
                    stack.push(&s.body);
                    stack.push(&s.orelse);
                    stack.push(&s.finalbody);
                    for h in &s.handlers {
                        let ruff_python_ast::ExceptHandler::ExceptHandler(inner) = h;
                        stack.push(&inner.body);
                    }
                }
                ruff_python_ast::Stmt::Match(s) => {
                    for case in &s.cases {
                        stack.push(&case.body);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Base expression → dotted name (`Name.id` or `a.b.c` attribute chain);
/// non-name bases (subscripts/calls) drop out (hazard.py:102-106).
fn dotted_base(expr: &ruff_python_ast::Expr) -> Option<String> {
    match expr {
        ruff_python_ast::Expr::Name(n) => Some(n.id.as_str().to_string()),
        ruff_python_ast::Expr::Attribute(a) => {
            let mut parts: Vec<String> = vec![a.attr.as_str().to_string()];
            let mut cur = a.value.as_ref();
            while let ruff_python_ast::Expr::Attribute(inner) = cur {
                parts.push(inner.attr.as_str().to_string());
                cur = inner.value.as_ref();
            }
            match cur {
                ruff_python_ast::Expr::Name(n) => {
                    parts.push(n.id.as_str().to_string());
                    parts.reverse();
                    Some(parts.join("."))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Class-body top-level `NAME = "value"` string assignments
/// (`hazard.py:128-140`) — StrEnum member values.
fn extract_str_members(body: &[ruff_python_ast::Stmt]) -> Vec<String> {
    let mut values = Vec::new();
    for stmt in body {
        if let ruff_python_ast::Stmt::Assign(a) = stmt {
            if a.targets.len() == 1 {
                if let (ruff_python_ast::Expr::Name(_), ruff_python_ast::Expr::StringLiteral(s)) =
                    (&a.targets[0], a.value.as_ref())
                {
                    values.push(s.value.to_str().to_string());
                }
            }
        }
    }
    values
}

/// `Class.method` / `<path>::Class.method` → method name; bare class →
/// None (`hazard.py:143-146`).
pub fn method_name(symbol: &str) -> Option<String> {
    let bare = symbol.split("::").nth(1).unwrap_or(symbol);
    bare.split_once('.').map(|(_, m)| m.to_string())
}

/// getattr dispatch rg pattern — `getattr(obj, "<symbol>")`
/// (`hazard.py:149-152`).
pub fn build_getattr_pattern(symbol: &str) -> String {
    format!(
        r#"getattr\(\s*[A-Za-z_][A-Za-z0-9_.]*\s*,\s*["']{}["']"#,
        regex::escape(symbol)
    )
}

/// StrEnum member values as rg -F patterns (quote-anchored)
/// (`hazard.py:154-156`).
pub fn build_strentenum_patterns(values: &[String]) -> Vec<String> {
    values.iter().map(|v| format!("\"{v}\"")).collect()
}

/// import_module("<module>") literal pattern (`hazard.py:158-161`).
pub fn build_importlib_pattern(module: &str) -> String {
    format!(r#"import_module\(\s*["']{}["']"#, regex::escape(module))
}

/// rg -n output lines → (prod, test, excluded) (`hazard.py:164-183`):
/// `tests/` prefix split per the hub_refs aggregate heuristic;
/// exclusions from the profile.
pub fn classify_rg_lines(
    lines: &[RgLine],
    profile: Option<&Profile>,
) -> (Vec<RgLine>, Vec<RgLine>, Vec<RgLine>) {
    let mut prod = Vec::new();
    let mut test = Vec::new();
    let mut excluded = Vec::new();
    for ln in lines {
        let rel = ln.split(':').next().unwrap_or("");
        if rel.starts_with("tests/") {
            test.push(ln.clone());
        } else if is_excluded(rel, profile) {
            excluded.push(ln.clone());
        } else {
            prod.push(ln.clone());
        }
    }
    (prod, test, excluded)
}

fn starts_with_rel(ln: &str, rel: &str) -> bool {
    ln.starts_with(rel)
}

/// StrEnum member string-literal consumption count (`hazard.py:186-214`):
/// definition file excluded; prod/test counted separately; short values
/// match non-enum usage (honest total, no semantics).
pub fn detect_strentenum_string_dispatch(
    facts: &SymbolFacts,
    rg: RgRunner,
    profile: Option<&Profile>,
) -> Result<Option<HazardFinding>, String> {
    if !facts.is_strentenum || facts.enum_values.is_empty() {
        return Ok(None);
    }
    let mut owned: Vec<String> = vec!["-F".to_string()];
    for p in build_strentenum_patterns(&facts.enum_values) {
        owned.push("-e".to_string());
        owned.push(p);
    }
    let args: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut lines = rg(&args)?;
    if let Some(rel) = &facts.rel_path {
        lines.retain(|ln| !starts_with_rel(ln, rel));
    }
    let (prod, test, _) = classify_rg_lines(&lines, profile);
    if lines.is_empty() {
        return Ok(None);
    }
    let mut f = HazardFinding::new(
        "strentenum-string-dispatch",
        (prod.len() + test.len()) as i64,
        format!(
            "StrEnum member 字串值 literal 消費 {} 處 prod + {} 處 test（YAML/config/dict key——靜態 callers 圖外）",
            prod.len(),
            test.len()
        ),
    );
    f.evidence = prod.iter().take(5).cloned().collect();
    f.detail = vec![
        ("prod".into(), prod.len() as i64),
        ("test".into(), test.len() as i64),
    ];
    Ok(Some(f))
}

/// getattr(obj, "<symbol>") dynamic-attribute sites (`hazard.py:217-234`).
pub fn detect_getattr_dispatch(
    facts: &SymbolFacts,
    rg: RgRunner,
    profile: Option<&Profile>,
) -> Result<Option<HazardFinding>, String> {
    let pattern = build_getattr_pattern(&facts.name);
    let mut lines = rg(&[&pattern])?;
    if let Some(rel) = &facts.rel_path {
        lines.retain(|ln| !starts_with_rel(ln, rel));
    }
    let (prod, test, _) = classify_rg_lines(&lines, profile);
    if lines.is_empty() {
        return Ok(None);
    }
    let mut f = HazardFinding::new(
        "getattr-string-dispatch",
        (prod.len() + test.len()) as i64,
        format!(
            "getattr(<obj>, \"{}\") 動態取得 {} prod + {} test 處",
            facts.name,
            prod.len(),
            test.len()
        ),
    );
    let mut ev = prod.clone();
    ev.extend(test.clone());
    f.evidence = ev.iter().take(5).cloned().collect();
    f.detail = vec![
        ("prod".into(), prod.len() as i64),
        ("test".into(), test.len() as i64),
    ];
    Ok(Some(f))
}

/// Registry auto-discovery inference (`hazard.py:237-257`): defined under
/// the scan prefix + name suffix match.
pub fn detect_registry_auto_discovery(
    facts: &SymbolFacts,
    registries: &[HazardRegistry],
) -> Option<HazardFinding> {
    if !facts.is_class {
        return None;
    }
    let Some(rel) = &facts.rel_path else {
        return None;
    };
    for reg in registries {
        if rel.starts_with(&reg.package_prefix) && facts.name.ends_with(&reg.suffix) {
            let mut f = HazardFinding::new(
                "registry-auto-discovery",
                1,
                format!(
                    "經 {}() 註冊到 {}——callers 邊不含 registry 字串 spec_name dispatch 點；「0 callers 可刪」判斷對 registry 成員恆為誤導",
                    reg.register_fn, reg.registry
                ),
            );
            if !reg.evidence.is_empty() {
                f.evidence = vec![reg.evidence.clone()];
            }
            return Some(f);
        }
    }
    None
}

/// Protocol annotation/isinstance consumption sites (`hazard.py:260-284`).
pub fn detect_protocol_duck_typing(
    facts: &SymbolFacts,
    rg: RgRunner,
    profile: Option<&Profile>,
) -> Result<Option<HazardFinding>, String> {
    if !facts.is_protocol {
        return Ok(None);
    }
    let pattern = format!(
        r"(?::\s*|->\s*|isinstance\([^,]*,\s*){}\b",
        regex::escape(&facts.name)
    );
    let lines = rg(&[&pattern])?;
    let (prod, test, _) = classify_rg_lines(&lines, profile);
    if lines.is_empty() {
        return Ok(None);
    }
    let mut f = HazardFinding::new(
        "protocol-duck-typing",
        (prod.len() + test.len()) as i64,
        format!(
            "Protocol 型別標註/檢查 {} prod + {} test 處——實作類消費不經繼承邊（structural typing）",
            prod.len(),
            test.len()
        ),
    );
    f.evidence = prod.iter().take(5).cloned().collect();
    f.detail = vec![
        ("prod".into(), prod.len() as i64),
        ("test".into(), test.len() as i64),
    ];
    Ok(Some(f))
}

/// import_module("<literal>") references to the defining module
/// (`hazard.py:287-304`).
pub fn detect_importlib_lazy_load(
    facts: &SymbolFacts,
    rg: RgRunner,
    profile: Option<&Profile>,
) -> Result<Option<HazardFinding>, String> {
    let Some(module) = &facts.module else {
        return Ok(None);
    };
    let pattern = build_importlib_pattern(module);
    let lines = rg(&[&pattern])?;
    if lines.is_empty() {
        return Ok(None);
    }
    let (prod, test, _) = classify_rg_lines(&lines, profile);
    let mut f = HazardFinding::new(
        "importlib-lazy-load",
        (prod.len() + test.len()) as i64,
        format!("import_module(\"{module}\") literal 引用——模組邊經字串"),
    );
    let mut ev = prod.clone();
    ev.extend(test.clone());
    f.evidence = ev.iter().take(5).cloned().collect();
    f.detail = vec![
        ("prod".into(), prod.len() as i64),
        ("test".into(), test.len() as i64),
    ];
    Ok(Some(f))
}

/// rg call-file set − CRG caller-file set (`hazard.py:307-356`): bare
/// class → `\bSym\(`; Class.method → `\.method\(`; `None` baseline
/// (callees direction) skips. Method-form noise: matches any same-name
/// method across classes (gap locating, not precise edges).
pub fn detect_static_edge_gap(
    facts: &SymbolFacts,
    static_caller_files: Option<&std::collections::BTreeSet<String>>,
    rg: RgRunner,
    method: Option<&str>,
) -> Result<Option<HazardFinding>, String> {
    let Some(baseline) = static_caller_files else {
        return Ok(None);
    };
    let pattern = if method.is_none() && !facts.name.contains('.') {
        format!(r"\b{}\(", regex::escape(&facts.name))
    } else if let Some(m) = method {
        format!(r"\.{}\(", regex::escape(m))
    } else {
        return Ok(None);
    };
    let mut lines = rg(&[&pattern])?;
    if let Some(rel) = &facts.rel_path {
        lines.retain(|ln| !starts_with_rel(ln, rel));
    }
    let rg_files: std::collections::BTreeSet<String> = lines
        .iter()
        .map(|ln| ln.split(':').next().unwrap_or("").to_string())
        .collect();
    let prod_missing: Vec<&String> = rg_files
        .difference(baseline)
        .filter(|f| !f.starts_with("tests/"))
        .collect();
    let test_missing: Vec<&String> = rg_files
        .difference(baseline)
        .filter(|f| f.starts_with("tests/"))
        .collect();
    let missing_count = prod_missing.len() + test_missing.len();
    if missing_count == 0 {
        return Ok(None);
    }
    let mut missing_all: Vec<&String> = prod_missing.clone();
    missing_all.extend(test_missing.clone());
    let mut sorted: Vec<String> = missing_all.into_iter().cloned().collect();
    sorted.sort();
    let mut f = HazardFinding::new(
        "static-edge-gap",
        missing_count as i64,
        format!(
            "{} 呼叫檔 {} 個不在 CRG callers（prod {} / test {}）——rg 可見但靜態圖漏邊",
            match method {
                Some(m) => format!(".{m}"),
                None => facts.name.clone(),
            },
            missing_count,
            prod_missing.len(),
            test_missing.len()
        ),
    );
    f.evidence = sorted.iter().take(5).cloned().collect();
    f.detail = vec![
        ("rg_files".into(), rg_files.len() as i64),
        ("crg_files".into(), baseline.len() as i64),
        ("missing_prod".into(), prod_missing.len() as i64),
        ("missing_test".into(), test_missing.len() as i64),
    ];
    Ok(Some(f))
}

/// Resident AST level (`hazard.py:359-393`): zero rg cost, existence
/// signals (count=0 — counts only exist at the rg level).
pub fn resident_findings(facts: &SymbolFacts, registries: &[HazardRegistry]) -> Vec<HazardFinding> {
    let mut findings = Vec::new();
    if facts.is_strentenum && !facts.enum_values.is_empty() {
        let shown: Vec<String> = facts.enum_values.iter().take(3).map(py_repr_str).collect();
        let more = if facts.enum_values.len() > 3 {
            " 等"
        } else {
            ""
        };
        findings.push(HazardFinding::new(
            "strentenum-string-dispatch",
            0,
            format!(
                "StrEnum class（{}{more}）——member 字串值 literal 消費在靜態 callers 圖外",
                shown.join(", ")
            ),
        ));
    }
    if let Some(f) = detect_registry_auto_discovery(facts, registries) {
        findings.push(f);
    }
    if facts.is_protocol {
        findings.push(HazardFinding::new(
            "protocol-duck-typing",
            0,
            "Protocol subclass——實作類消費不經繼承邊（structural typing）".to_string(),
        ));
    }
    findings
}

fn py_repr_str(v: &String) -> String {
    format!("'{}'", v)
}

/// Full six-rule scan (rg level, triggered) (`hazard.py:396-425`).
pub fn full_findings(
    facts: &SymbolFacts,
    registries: &[HazardRegistry],
    rg: RgRunner,
    static_caller_files: Option<&std::collections::BTreeSet<String>>,
    profile: Option<&Profile>,
    method: Option<&str>,
) -> Result<Vec<HazardFinding>, String> {
    let mut findings = Vec::new();
    if let Some(f) = detect_strentenum_string_dispatch(facts, rg, profile)? {
        findings.push(f);
    }
    if let Some(f) = detect_getattr_dispatch(facts, rg, profile)? {
        findings.push(f);
    }
    if let Some(f) = detect_registry_auto_discovery(facts, registries) {
        findings.push(f);
    }
    if let Some(f) = detect_protocol_duck_typing(facts, rg, profile)? {
        findings.push(f);
    }
    if let Some(f) = detect_importlib_lazy_load(facts, rg, profile)? {
        findings.push(f);
    }
    if let Some(f) = detect_static_edge_gap(facts, static_caller_files, rg, method)? {
        findings.push(f);
    }
    Ok(findings)
}

/// §5.4 gate (`hazard.py:428-445`): few static callers (prod ≤ threshold)
/// AND hazard hits → warning line. The threshold is shared with the
/// trigger (a split would silently narrow the warning below the scan).
pub fn hazard_gate_warning(
    static_prod: i64,
    _static_test: i64,
    findings: &[HazardFinding],
    threshold: i64,
) -> Option<String> {
    if static_prod <= threshold && !findings.is_empty() {
        let kinds: Vec<&str> = findings.iter().map(|f| f.kind.as_str()).collect();
        return Some(format!(
            "[WARN] 靜態 prod callers 僅 {static_prod} 但命中 {} 類 dynamic hazard（{}）——「無引用可刪」判斷需先查 hazard 明細",
            findings.len(),
            kinds.join("、")
        ));
    }
    None
}

// ---------- orchestration (hub_refs hazard_stage support) ----------

/// Resolve the symbol's defining file via the nodes table
/// (`hazard.py:453-503`, hub_refs.resolve_qualified conventions) then
/// AST-parse. Advisory stage: resolution failures degrade to name-only
/// facts (rg rules still usable) — never crashes on ambiguity.
pub fn symbol_facts(
    symbol: &str,
    repo_root: &Path,
    profile: Option<&Profile>,
) -> Result<SymbolFacts, String> {
    let bare = symbol.split("::").nth(1).unwrap_or(symbol);
    let cls_name = bare.split('.').next().unwrap_or(bare);
    let facts = SymbolFacts {
        name: cls_name.to_string(),
        ..Default::default()
    };
    let db_path = crate::graph_db::db_path(repo_root);
    if !db_path.exists() {
        return Ok(facts);
    }
    let m0 = db_mtime_ns(&db_path)?;
    let rows = query_nodes(&db_path, bare)?;
    assert_db_unchanged(&db_path, m0)?;
    let mut pairs: Vec<(String, String, String)> = Vec::new();
    for (q, fp, k) in rows {
        if let Some(rel) = repo_relative(&fp, repo_root) {
            if !is_excluded(&rel, profile) {
                pairs.push((q, rel, k));
            }
        }
    }
    if pairs.len() != 1 {
        return Ok(facts);
    }
    let (_, rel, kind) = pairs.pop().unwrap();
    let source = std::fs::read_to_string(repo_root.join(&rel)).unwrap_or_default();
    let mut parsed = parse_symbol_facts(&source, cls_name);
    parsed.rel_path = Some(rel.clone());
    parsed.module = Some(rel.trim_end_matches(".py").replace('/', "."));
    parsed.kind = Some(kind);
    Ok(parsed)
}

fn query_nodes(db_path: &Path, bare: &str) -> Result<Vec<(String, String, String)>, String> {
    let conn = connect_ro(db_path)?;
    let sql = if bare.contains('.') {
        let (parent, name) = bare.rsplit_once('.').unwrap();
        (
            "SELECT qname, file_path, kind FROM nodes WHERE name = ?1 AND parent_name = ?2"
                .to_string(),
            vec![name.to_string(), parent.to_string()],
        )
    } else {
        (
            "SELECT qname, file_path, kind FROM nodes WHERE name = ?1".to_string(),
            vec![bare.to_string()],
        )
    };
    let (sql, params) = sql;
    let rows: Result<Vec<(String, String, String)>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare(&sql)?;
        let mapped = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(mapped)
    })();
    match rows {
        Ok(v) => Ok(v),
        Err(e) => Err(format!(
            "graph.db 讀 nodes 失敗（{e}）：{}——非自有格式？重跑 `code-reality graph_db build --repo <repo>`",
            db_path.display()
        )),
    }
}

/// rg -n --no-heading runner (`hazard.py:513-565`): search scope
/// py+yaml+json+toml; exclusion globs pinned; **cwd=root + path `.`**
/// (an absolute path arg silently defeats the `-g` exclusions — the
/// glob anchors on the path-arg form, empirically proven); output lines
/// are stripped of the `./` prefix for comparability with CRG file sets.
pub fn make_rg_runner(repo_root: &Path) -> impl Fn(&[&str]) -> Result<Vec<RgLine>, String> {
    let root: PathBuf = crate::common::resolve(repo_root);
    move |args: &[&str]| {
        let mut cmd = std::process::Command::new("rg");
        cmd.args([
            "-n",
            "--no-heading",
            "-t",
            "py",
            "-t",
            "yaml",
            "-t",
            "json",
            "-t",
            "toml",
        ])
        .args(args)
        .arg(".")
        .args([
            "-g",
            "!.venv/**",
            "-g",
            "!stubs/**",
            "-g",
            "!ai-analysis/**",
            "-g",
            "!.agent-tmp/**",
            "-g",
            "!.code-reality/**",
        ])
        .current_dir(&root);
        let out = cmd.output().map_err(|e| format!("rg 執行失敗：{e}"))?;
        if !(out.status.success() || out.status.code() == Some(1)) {
            return Err(format!(
                "rg 失敗（exit {}）: {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let mut lines = Vec::new();
        for ln in String::from_utf8_lossy(&out.stdout).split('\n') {
            let ln = ln.strip_prefix("./").unwrap_or(ln);
            if !ln.trim().is_empty() {
                lines.push(ln.to_string());
            }
        }
        Ok(lines)
    }
}
