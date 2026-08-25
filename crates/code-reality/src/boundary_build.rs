//! `boundary_build` — the frozen `code_reality/boundary_build.py` contract:
//! PyO3 boundary extractor. Scans the target repo's crates `*.rs` pyo3
//! declarations and reconciles them against the Python `.pyi` contract
//! tree, writing a commit-anchored sidecar DB
//! (`~/.mosaic/code-reality/boundary/<nt-short-sha>.db`, idempotent
//! overwrite). Query consumer: `boundary.rs`.
//!
//! Known Gaps are documented in the frozen module and carried as-is
//! (macro-hidden declarations, credential field omission, empty stubs,
//! non-string-aware brace counting).

use crate::common::{connect_ro, make_meta};
use crate::profile::{load_profile, scan_roots};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const TOOL: &str = "code_reality.boundary_build";
pub const DEFAULT_OUT_DIR: &str = "~/.mosaic/code-reality/boundary";

fn re_fn() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#"^(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:unsafe\s+)?(?:extern\s+"C"\s+)?fn\s+(\w+)"#,
        )
        .unwrap()
    })
}

fn re_struct() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?struct\s+(\w+)").unwrap())
}

fn re_enum() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)").unwrap())
}

fn re_impl() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^impl\b").unwrap())
}

fn re_kv_str() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap())
}

fn re_field() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^\s*pub\s+(?:\(crate\)\s+)?(\w+)\s*:").unwrap())
}

fn re_variant() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(\w+)\s*(?:\([^)]*\))?\s*,?\s*$").unwrap())
}

#[derive(Debug, Clone)]
pub struct RsClass {
    pub rs_path: String,
    pub line: i64,
    pub rust_name: String,
    pub exposed: String,
    pub py_module: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RsMethod {
    pub rs_path: String,
    pub line: i64,
    pub rust_class: String,
    pub rust_fn: String,
    pub exposed: String,
    pub kind: String, // method|getter|setter|new|staticmethod|classmethod|dunder|variant|field_property
    pub renamed: bool,
}

#[derive(Debug, Clone)]
pub struct RsFunction {
    pub rs_path: String,
    pub line: i64,
    pub rust_fn: String,
    pub exposed: String,
    pub py_module: Option<String>,
}

fn balanced(s: &str) -> bool {
    s.matches('(').count() == s.matches(')').count()
        && s.matches('[').count() == s.matches(']').count()
}

/// Collect consecutive attribute lines from i (`boundary_build.py:108-122`).
fn collect_attrs(lines: &[&str], mut i: usize) -> (Vec<String>, usize) {
    let mut attrs = Vec::new();
    while i < lines.len() {
        let stripped = lines[i].trim();
        if !stripped.starts_with("#[") {
            break;
        }
        let mut text = stripped.to_string();
        let mut j = i;
        while !balanced(&text) && j + 1 < lines.len() {
            j += 1;
            text.push(' ');
            text.push_str(lines[j].trim());
        }
        attrs.push(text);
        i = j + 1;
    }
    (attrs, i)
}

fn skip_doc_blank(lines: &[&str], mut i: usize) -> usize {
    while i < lines.len() {
        let s = lines[i].trim();
        if s.is_empty() || s.starts_with("///") || s.starts_with("//!") || s.starts_with("//") {
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn word_re(inner: &str, word: &str) -> bool {
    // \b<word>\b without lookaround: split-scan equivalence
    regex::Regex::new(&format!(r"\b{}\b", regex::escape(word)))
        .unwrap()
        .is_match(inner)
}

fn starts_word_re(inner: &str, prefix: &str) -> bool {
    regex::Regex::new(&format!(r"^{}", regex::escape(prefix)))
        .unwrap()
        .is_match(inner)
}

/// Attribute classification (`boundary_build.py:135-159`).
fn attr_kind(a: &str) -> Option<&'static str> {
    let inner = if a.ends_with(']') { &a[2..a.len() - 1] } else { &a[2..] };
    if word_re(inner, "pyclass") {
        return Some("pyclass");
    }
    if word_re(inner, "pymethods") {
        return Some("pymethods");
    }
    if word_re(inner, "pyfunction") {
        return Some("pyfunction");
    }
    if word_re(inner, "pymodule") {
        return Some("pymodule");
    }
    if inner.contains("gen_stub_pyclass") {
        return Some("gen_stub_pyclass");
    }
    if inner.contains("gen_stub_pyfunction") {
        return Some("gen_stub_pyfunction");
    }
    if starts_word_re(inner, "getter") || starts_word_re(inner, "pyo3::getter") {
        return Some("getter");
    }
    if starts_word_re(inner, "setter") || starts_word_re(inner, "pyo3::setter") {
        return Some("setter");
    }
    if inner == "new" || inner == "pyo3::new" {
        return Some("new");
    }
    if inner == "staticmethod" || inner == "pyo3::staticmethod" {
        return Some("staticmethod");
    }
    if inner == "classmethod" || inner == "pyo3::classmethod" {
        return Some("classmethod");
    }
    if starts_word_re(inner, "pyo3") && inner.starts_with("pyo3") && inner[4..].starts_with('(') {
        return Some("pyo3");
    }
    None
}

/// `key = "value"` extraction with multi-attr scan semantics
/// (`boundary_build.py:162-168`).
fn attr_kv(a: &str, key: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(m) = re_kv_str().find_at(a, search_from) {
        let g = &a[m.start()..m.end()];
        if let Some(caps) = re_kv_str().captures(g) {
            if &caps[1] == key {
                return Some(caps[2].to_string());
            }
        }
        search_from = m.end();
    }
    None
}

/// `impl <T> Foo<T> where ... {` → `Foo`; trait impl (` for `) → None
/// (`boundary_build.py:171-189`).
fn impl_self_type(header: &str) -> Option<String> {
    let mut body = header.split('{').next().unwrap_or("");
    let pos = body.find("impl")?;
    body = &body[pos + 4..];
    if body.contains(" for ") {
        return None;
    }
    if body.starts_with('<') {
        let mut depth = 0i32;
        for (k, ch) in body.char_indices() {
            if ch == '<' {
                depth += 1;
            } else if ch == '>' {
                depth -= 1;
                if depth == 0 {
                    body = &body[k + 1..];
                    break;
                }
            }
        }
    }
    let seg = body.trim().split('<').next().unwrap_or("").trim();
    let name = seg.rsplit("::").next().unwrap_or("").trim();
    let is_ident = !name.is_empty()
        && name.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
        && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if is_ident {
        Some(name.to_string())
    } else {
        None
    }
}

/// Brace-balanced body end (`boundary_build.py:192-201`): `//` comments
/// stripped, `'{'`/`'}'` char literals neutralized (string-unaware — the
/// documented known gap).
fn impl_body_end(lines: &[&str], brace_line: usize) -> usize {
    static LINE_COMMENT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let strip = LINE_COMMENT.get_or_init(|| regex::Regex::new(r"//.*$").unwrap());
    let mut depth = 0i64;
    for (k, line) in lines.iter().enumerate().skip(brace_line) {
        let no_comment = strip.replace(line, "");
        let code = no_comment.replace("'{'", "X").replace("'}'", "X");
        depth += code.matches('{').count() as i64 - code.matches('}').count() as i64;
        if depth == 0 {
            return k;
        }
    }
    lines.len() - 1
}

/// CamelCase → SCREAMING_SNAKE_CASE (`boundary_build.py:204-214`; the
/// lookbehind-bearing rules via fancy-regex — naive `.upper()` never
/// matches `UsdM`↔`USD_M`).
pub fn screaming_snake(name: &str) -> String {
    static R1: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    static R2: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    let r1 = R1.get_or_init(|| fancy_regex::Regex::new(r"(?<=[a-z0-9])(?=[A-Z])").unwrap());
    let r2 = R2.get_or_init(|| fancy_regex::Regex::new(r"(?<=[A-Z])(?=[A-Z][a-z])").unwrap());
    let mut s = r1.replace_all(name, "_").into_owned();
    s = r2.replace_all(&s, "_").into_owned();
    s.to_uppercase()
}

/// Python-exposed method name (`boundary_build.py:217-234`): rename >
/// getter/setter get_/set_ strip > py_ strip > original.
fn exposed_method_name(fn_name: &str, kind: &str, rename: Option<&str>) -> String {
    if let Some(r) = rename {
        return r.to_string();
    }
    if kind == "new" {
        return "__new__".to_string();
    }
    if (kind == "getter" || kind == "setter")
        && (fn_name.starts_with("get_") || fn_name.starts_with("set_"))
    {
        return fn_name[4..].to_string();
    }
    if let Some(stripped) = fn_name.strip_prefix("py_") {
        return stripped.to_string();
    }
    fn_name.to_string()
}

/// One `*.rs` file scan (`boundary_build.py:237-426`): pymethods impl
/// bodies, pyclass struct/enum with variant/field synthesis, pyfunction.
pub fn scan_rust_file(path: &Path, repo: &Path) -> (Vec<RsClass>, Vec<RsMethod>, Vec<RsFunction>) {
    let rel = path.strip_prefix(repo).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let raw = std::fs::read(path).unwrap_or_default();
    let content = String::from_utf8_lossy(&raw);
    let lines: Vec<&str> = content.split('\n').collect();
    let (mut classes, mut methods, mut functions) = (Vec::new(), Vec::new(), Vec::new());

    let mut i = 0usize;
    while i < lines.len() {
        let (attrs, mut j) = collect_attrs(&lines, i);
        let kinds: Vec<Option<&str>> = attrs.iter().map(|a| attr_kind(a)).collect();
        j = skip_doc_blank(&lines, j);
        if j >= lines.len() {
            break;
        }
        let line = lines[j];
        let stripped = line.trim();

        // pymethods impl body scan
        if kinds.contains(&Some("pymethods")) && re_impl().is_match(stripped) {
            let mut header = stripped.to_string();
            let mut k = j;
            while !header.contains('{') && k + 1 < lines.len() {
                k += 1;
                header.push(' ');
                header.push_str(lines[k].trim());
            }
            let cls_name = impl_self_type(&header);
            let end = impl_body_end(&lines, k);
            let mut m = j + 1;
            while m < end {
                let s = lines[m].trim();
                if s.is_empty() || s.starts_with("///") || s.starts_with("//!") || s.starts_with("//") {
                    m += 1;
                    continue;
                }
                let (inner_attrs, mut n) = collect_attrs(&lines, m);
                let inner_kinds: Vec<Option<&str>> =
                    inner_attrs.iter().map(|a| attr_kind(a)).collect();
                n = skip_doc_blank(&lines, n);
                if n >= end {
                    break;
                }
                if let Some(fn_m) = re_fn().captures(lines[n].trim()) {
                    let fn_name = fn_m[1].to_string();
                    let mut rename: Option<String> = None;
                    for (a, kd) in inner_attrs.iter().zip(inner_kinds.iter()) {
                        if *kd == Some("pyo3") {
                            // name= and signature= commonly coexist — only a
                            // present name= overwrites; a later signature
                            // attr must not clear the rename (last-wins fix)
                            if let Some(v) = attr_kv(a, "name") {
                                rename = Some(v);
                            }
                        }
                    }
                    let kind = if inner_kinds.contains(&Some("new")) {
                        "new"
                    } else if inner_kinds.contains(&Some("getter")) {
                        "getter"
                    } else if inner_kinds.contains(&Some("setter")) {
                        "setter"
                    } else if inner_kinds.contains(&Some("staticmethod")) {
                        "staticmethod"
                    } else if inner_kinds.contains(&Some("classmethod")) {
                        "classmethod"
                    } else if fn_name.starts_with("__") && fn_name.ends_with("__") {
                        "dunder"
                    } else {
                        "method"
                    };
                    let exposed = exposed_method_name(&fn_name, kind, rename.as_deref());
                    methods.push(RsMethod {
                        rs_path: rel.clone(),
                        line: (n + 1) as i64,
                        rust_class: cls_name.clone().unwrap_or_else(|| "?".into()),
                        rust_fn: fn_name,
                        exposed,
                        kind: kind.to_string(),
                        renamed: rename.is_some(),
                    });
                    m = n + 1;
                } else {
                    m += 1; // attrs not followed by fn — conservative step
                }
            }
            i = end + 1;
            continue;
        }

        // pyclass struct/enum (with variant / field_property synthesis)
        let struct_m = re_struct().captures(stripped);
        let enum_m = re_enum().captures(stripped);
        if (kinds.contains(&Some("pyclass")) || kinds.contains(&Some("gen_stub_pyclass")))
            && (struct_m.is_some() || enum_m.is_some())
        {
            let is_enum = enum_m.is_some();
            let sm = struct_m.or(enum_m).unwrap();
            let rust_name = sm[1].to_string();
            let mut module: Option<String> = None;
            let mut name_attr: Option<String> = None;
            for (a, kd) in attrs.iter().zip(kinds.iter()) {
                if *kd == Some("pyclass") || *kd == Some("gen_stub_pyclass") {
                    if let Some(v) = attr_kv(a, "module") {
                        module = Some(v);
                    }
                    if let Some(v) = attr_kv(a, "name") {
                        name_attr = Some(v);
                    }
                }
            }
            classes.push(RsClass {
                rs_path: rel.clone(),
                line: (i + 1) as i64, // attr block start — the pyclass anchor
                exposed: name_attr.clone().unwrap_or_else(|| rust_name.clone()),
                rust_name: rust_name.clone(),
                py_module: module,
            });
            let attr_blob = attrs.join(" ");
            let mut body_start = j;
            while !lines[body_start].contains('{') && body_start + 1 < lines.len() && body_start < j + 3 {
                body_start += 1;
            }
            if !lines[body_start].contains('{') {
                // tuple struct — no body, skip synthesis
                i = j + 1;
                continue;
            }
            let body_end = impl_body_end(&lines, body_start);
            if is_enum {
                let mut k2 = body_start + 1;
                let mut pending_rename: Option<String> = None;
                while k2 < body_end {
                    let s2 = lines[k2].trim();
                    if s2.is_empty() || s2.starts_with("///") || s2.starts_with("//") {
                        k2 += 1;
                        continue;
                    }
                    if s2.starts_with("#[") {
                        if let Some(nm) = attr_kv(s2, "name") {
                            pending_rename = Some(nm);
                        }
                        k2 += 1;
                        continue;
                    }
                    if let Some(vm) = re_variant().captures(s2) {
                        let exposed = pending_rename
                            .take()
                            .unwrap_or_else(|| screaming_snake(&vm[1]));
                        methods.push(RsMethod {
                            rs_path: rel.clone(),
                            line: (k2 + 1) as i64,
                            rust_class: rust_name.clone(),
                            rust_fn: vm[1].to_string(),
                            exposed,
                            kind: "variant".into(),
                            renamed: pending_rename.is_some(),
                        });
                        pending_rename = None;
                    }
                    k2 += 1;
                }
            } else if attr_blob.contains("from_py_object") || attr_blob.contains("get_all") {
                for (k2, l2) in lines.iter().enumerate().take(body_end).skip(body_start + 1) {
                    if let Some(fm) = re_field().captures(l2) {
                        methods.push(RsMethod {
                            rs_path: rel.clone(),
                            line: (k2 + 1) as i64,
                            rust_class: rust_name.clone(),
                            rust_fn: fm[1].to_string(),
                            exposed: fm[1].to_string(),
                            kind: "field_property".into(),
                            renamed: false,
                        });
                    }
                }
            }
            i = body_end + 1;
            continue;
        }

        // pyfunction
        if (kinds.contains(&Some("pyfunction")) || kinds.contains(&Some("gen_stub_pyfunction")))
            && re_fn().is_match(stripped)
        {
            let mut module: Option<String> = None;
            let mut rename: Option<String> = None;
            for (a, kd) in attrs.iter().zip(kinds.iter()) {
                if *kd == Some("pyfunction") || *kd == Some("gen_stub_pyfunction") {
                    if let Some(v) = attr_kv(a, "module") {
                        module = Some(v);
                    }
                }
                if *kd == Some("pyo3") {
                    if let Some(v) = attr_kv(a, "name") {
                        rename = Some(v);
                    }
                }
            }
            let fn_name = re_fn().captures(stripped).unwrap()[1].to_string();
            let exposed = rename
                .clone()
                .unwrap_or_else(|| fn_name.strip_prefix("py_").unwrap_or(&fn_name).to_string());
            functions.push(RsFunction {
                rs_path: rel.clone(),
                line: (j + 1) as i64,
                exposed,
                rust_fn: fn_name,
                py_module: module,
            });
            i = j + 1;
            continue;
        }

        i = if !attrs.is_empty() { j + 1 } else { i + 1 };
    }
    (classes, methods, functions)
}

// ---------- .pyi parsing (ruff AST) ----------

#[derive(Debug, Clone, Default)]
pub struct PyClass {
    pub pyi_path: String,
    pub line: i64,
    pub name: String,
    pub is_enum: bool,
    /// exposed name → lineno (property expansion included; last def wins)
    pub methods: BTreeMap<String, i64>,
    /// enum member → lineno (first assignment wins)
    pub members: BTreeMap<String, i64>,
}

#[derive(Debug, Clone)]
pub struct PyFunction {
    pub pyi_path: String,
    pub line: i64,
    pub name: String,
}

/// Byte-offset → 1-based line via precomputed line starts.
struct LineMap {
    starts: Vec<usize>,
}

impl LineMap {
    fn new(src: &str) -> Self {
        let mut starts = vec![0usize];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { starts }
    }

    fn line(&self, offset: usize) -> i64 {
        match self.starts.binary_search(&offset) {
            Ok(i) => i as i64 + 1,
            Err(i) => i as i64,
        }
    }
}

/// `python/nautilus_trader/common/__init__.pyi` → `nautilus_trader.common`
/// (`boundary_build.py:451-465`; rindex — the repo dir shares the package
/// name, a plain index would hit the repo segment).
pub fn pyi_module(pyi_path: &str) -> Result<String, String> {
    let parts: Vec<&str> = pyi_path.split('/').collect();
    let idx = parts
        .iter()
        .rev()
        .position(|p| *p == "nautilus_trader")
        .map(|r| parts.len() - 1 - r)
        .ok_or_else(|| format!("pyi 路徑不含 nautilus_trader package 段：{pyi_path}"))?;
    let mut mod_parts: Vec<&str> = parts[idx..parts.len() - 1].to_vec();
    if parts[parts.len() - 1] != "__init__.pyi" {
        let stem = parts[parts.len() - 1].trim_end_matches(".pyi");
        mod_parts.push(stem);
    }
    Ok(mod_parts.join("."))
}

/// Parse one .pyi file (`boundary_build.py:468-511`): top-level classes
/// (enum-ness from bases, method/member tables) and functions.
pub fn parse_pyi(path: &Path, repo: &Path) -> Result<(Vec<PyClass>, Vec<PyFunction>), String> {
    let rel = path.strip_prefix(repo).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let src = std::fs::read_to_string(path).map_err(|e| format!("{} 讀取失敗：{e}", path.display()))?;
    let line_map = LineMap::new(&src);
    let parsed = ruff_python_parser::parse_module(&src)
        .map_err(|e| format!("{} pyi 解析失敗：{e}", path.display()))?;
    let (mut classes, mut functions) = (Vec::new(), Vec::new());
    for stmt in parsed.syntax().body.iter() {
        match stmt {
            ruff_python_ast::Stmt::ClassDef(node) => {
                let is_enum = node.bases().iter().any(|b| match b {
                    ruff_python_ast::Expr::Name(n) => {
                        matches!(n.id.as_str(), "Enum" | "StrEnum" | "IntEnum")
                    }
                    ruff_python_ast::Expr::Attribute(a) => {
                        matches!(a.attr.as_str(), "Enum" | "StrEnum" | "IntEnum")
                    }
                    _ => false,
                });
                let mut pc = PyClass {
                    pyi_path: rel.clone(),
                    line: line_map.line(node.range.start().to_usize()),
                    name: node.name.as_str().to_string(),
                    is_enum,
                    ..Default::default()
                };
                for sub in node.body.iter() {
                    match sub {
                        ruff_python_ast::Stmt::FunctionDef(f) => {
                            add_pyi_method(&mut pc, f, &line_map);
                        }
                        ruff_python_ast::Stmt::ClassDef(_)
                        | ruff_python_ast::Stmt::Pass(_) => {}
                        ruff_python_ast::Stmt::Assign(a) => {
                            for t in &a.targets {
                                if let ruff_python_ast::Expr::Name(n) = t {
                                    // first assignment wins (setdefault)
                                    pc.members.entry(n.id.as_str().to_string()).or_insert(
                                        line_map.line(a.range.start().to_usize()),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                classes.push(pc);
            }
            ruff_python_ast::Stmt::FunctionDef(f) => {
                functions.push(PyFunction {
                    pyi_path: rel.clone(),
                    line: line_map.line(f.range.start().to_usize()),
                    name: f.name.as_str().to_string(),
                });
            }
            _ => {}
        }
    }
    Ok((classes, functions))
}

fn add_pyi_method(pc: &mut PyClass, f: &ruff_python_ast::StmtFunctionDef, lm: &LineMap) {
    let name = f.name.as_str().to_string();
    let lineno = lm.line(f.range.start().to_usize());
    pc.methods.insert(name.clone(), lineno);
    // defensive @property get_x expansion (POC verbatim; zero hits on the
    // current NT corpus)
    let has_property = f.decorator_list.iter().any(|d| match &d.expression {
        ruff_python_ast::Expr::Name(n) => n.id.as_str() == "property",
        ruff_python_ast::Expr::Attribute(a) => a.attr.as_str() == "property",
        _ => false,
    });
    if has_property && name.starts_with("get_") {
        pc.methods.insert(name[4..].to_string(), lineno);
    }
}

// ---------- reconciliation ----------

fn crate_of(rs_path: &str) -> String {
    let parts: Vec<&str> = rs_path.split('/').collect();
    if parts.len() > 1 && parts[0] == "crates" {
        parts[1].to_string()
    } else {
        parts[0].to_string()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Edge {
    pub level: &'static str,
    pub py_symbol: String,
    pub pyi_path: String,
    pub pyi_line: i64,
    pub rs_symbol: String,
    pub rs_path: String,
    pub rs_line: i64,
    pub match_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_kind: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Coverage {
    pub classes: ClassCov,
    pub methods: MethodCov,
    pub functions: FnCov,
    pub unresolved_module: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ClassCov {
    pub rs_pyclass_total: i64,
    pub matched: i64,
    pub rs_only: i64,
    pub pyi_total: i64,
    pub pyi_only: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MethodCov {
    pub rs_methods_on_matched_classes: i64,
    pub matched: i64,
    pub rs_only: i64,
    pub pyi_only: i64,
    pub unresolved_class: i64,
    pub rs_only_by_kind: BTreeMap<String, i64>,
    pub dunder_rs_only: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FnCov {
    pub rs_functions: i64,
    pub matched: i64,
    pub rs_only: i64,
    pub pyi_only: i64,
}

/// Full reconciliation (`boundary_build.py:530-715`): class/method/
/// function joins with crate-limited keys (cross-crate same-name joins
/// would mis-bind — bare-name fallback only when globally unique).
pub fn build_boundary(
    classes: &[RsClass],
    methods: &[RsMethod],
    functions: &[RsFunction],
    py_classes: &[(String, PyClass)],
    py_functions: &[(String, PyFunction)],
) -> (Vec<Edge>, Coverage) {
    let mut class_by_key: BTreeMap<(String, String), RsClass> = BTreeMap::new();
    let mut classes_by_rust: BTreeMap<String, Vec<RsClass>> = BTreeMap::new();
    for c in classes {
        class_by_key
            .entry((crate_of(&c.rs_path), c.rust_name.clone()))
            .or_insert_with(|| c.clone());
        classes_by_rust.entry(c.rust_name.clone()).or_default().push(c.clone());
    }
    let mut py_class_index: BTreeMap<(String, String), PyClass> = BTreeMap::new();
    for (pmod, pc) in py_classes {
        py_class_index
            .entry((pmod.clone(), pc.name.clone()))
            .or_insert_with(|| pc.clone());
    }
    let mut py_fn_index: BTreeMap<(String, String), PyFunction> = BTreeMap::new();
    for (pmod, pfn) in py_functions {
        py_fn_index
            .entry((pmod.clone(), pfn.name.clone()))
            .or_insert_with(|| pfn.clone());
    }

    let mut edges: Vec<Edge> = Vec::new();
    let mut matched_keys: std::collections::BTreeSet<(String, String)> = Default::default();
    let mut cov = Coverage::default();
    cov.classes.rs_pyclass_total = classes.len() as i64;
    cov.classes.pyi_total = py_classes.len() as i64;
    cov.functions.rs_functions = functions.len() as i64;
    let mut rs_only_classes = 0i64;

    for c in classes {
        let Some(py_module) = &c.py_module else {
            cov.unresolved_module += 1;
            rs_only_classes += 1;
            continue;
        };
        if let Some(py) = py_class_index.get(&(py_module.clone(), c.exposed.clone())) {
            matched_keys.insert((crate_of(&c.rs_path), c.rust_name.clone()));
            cov.classes.matched += 1;
            edges.push(Edge {
                level: "class",
                py_symbol: format!("{py_module}.{}", c.exposed),
                pyi_path: py.pyi_path.clone(),
                pyi_line: py.line,
                rs_symbol: c.rust_name.clone(),
                rs_path: c.rs_path.clone(),
                rs_line: c.line,
                match_kind: if c.exposed == c.rust_name { "NAME_MATCH" } else { "PYCLASS_NAME_RENAME" },
                method_kind: None,
            });
        } else {
            rs_only_classes += 1;
        }
    }
    cov.classes.rs_only = rs_only_classes;

    let rs_exposed_keys: std::collections::BTreeSet<(String, String)> = classes
        .iter()
        .filter_map(|c| c.py_module.clone().map(|m| (m, c.exposed.clone())))
        .collect();
    cov.classes.pyi_only = py_classes
        .iter()
        .filter(|(m, c)| !rs_exposed_keys.contains(&(m.clone(), c.name.clone())))
        .count() as i64;

    // method reconciliation (only on class-matched pairs)
    let mut rs_method_keys: std::collections::BTreeSet<(String, String, String)> = Default::default();
    for m in methods {
        let mut mkey0 = (crate_of(&m.rs_path), m.rust_class.clone());
        let mut rc = class_by_key.get(&mkey0).cloned();
        if rc.is_none() {
            let candidates = classes_by_rust.get(&m.rust_class).cloned().unwrap_or_default();
            if candidates.len() == 1 {
                rc = Some(candidates[0].clone());
                mkey0 = (crate_of(&candidates[0].rs_path), candidates[0].rust_name.clone());
            }
        }
        let Some(rc) = rc else {
            cov.methods.unresolved_class += 1;
            continue;
        };
        if !matched_keys.contains(&mkey0) {
            continue;
        }
        let py_module = rc.py_module.clone().unwrap();
        cov.methods.rs_methods_on_matched_classes += 1;
        let py = py_class_index.get(&(py_module.clone(), rc.exposed.clone())).unwrap();
        let mut exposed = m.exposed.clone();
        if m.kind == "new" {
            // pyo3-stub-gen version difference: __init__ vs __new__ both seen
            exposed = if py.methods.contains_key("__init__") {
                "__init__".to_string()
            } else if py.methods.contains_key("__new__") {
                "__new__".to_string()
            } else {
                m.exposed.clone()
            };
        }
        rs_method_keys.insert((py_module.clone(), rc.exposed.clone(), exposed.clone()));
        let target_line = if m.kind == "variant" {
            py.members.get(&exposed)
        } else {
            py.methods.get(&exposed)
        };
        match target_line {
            Some(&pyi_line) => {
                cov.methods.matched += 1;
                let kind_str: &'static str = if m.renamed {
                    "PYO3_NAME_RENAME"
                } else if m.kind == "getter" || m.kind == "setter" {
                    "GETTER_PROPERTY"
                } else if m.kind == "field_property" {
                    "FIELD_PROPERTY"
                } else if m.kind == "variant" {
                    "ENUM_VARIANT"
                } else {
                    "NAME_MATCH"
                };
                edges.push(Edge {
                    level: "method",
                    py_symbol: format!("{py_module}.{}.{}", rc.exposed, exposed),
                    pyi_path: py.pyi_path.clone(),
                    pyi_line,
                    rs_symbol: format!("{}::{}", m.rust_class, m.rust_fn),
                    rs_path: m.rs_path.clone(),
                    rs_line: m.line,
                    match_kind: kind_str,
                    method_kind: Some(m.kind.clone()),
                });
            }
            None if m.kind == "dunder" => {
                cov.methods.dunder_rs_only += 1;
            }
            None => {
                cov.methods.rs_only += 1;
                *cov.methods.rs_only_by_kind.entry(m.kind.clone()).or_insert(0) += 1;
            }
        }
    }
    for (pmod, pc) in py_classes {
        if !rs_exposed_keys.contains(&(pmod.clone(), pc.name.clone())) {
            continue;
        }
        for name in pc.methods.keys() {
            if !rs_method_keys.contains(&(pmod.clone(), pc.name.clone(), name.clone()))
                && !name.starts_with("__")
            {
                cov.methods.pyi_only += 1;
            }
        }
    }

    // function reconciliation
    for f in functions {
        let Some(py_module) = &f.py_module else {
            cov.unresolved_module += 1;
            continue;
        };
        match py_fn_index.get(&(py_module.clone(), f.exposed.clone())) {
            Some(pf) => {
                cov.functions.matched += 1;
                edges.push(Edge {
                    level: "function",
                    py_symbol: format!("{py_module}.{}", f.exposed),
                    pyi_path: pf.pyi_path.clone(),
                    pyi_line: pf.line,
                    rs_symbol: f.rust_fn.clone(),
                    rs_path: f.rs_path.clone(),
                    rs_line: f.line,
                    match_kind: if f.exposed != f.rust_fn { "PYO3_NAME_RENAME" } else { "NAME_MATCH" },
                    method_kind: None,
                });
            }
            None => cov.functions.rs_only += 1,
        }
    }
    let rs_fn_keys: std::collections::BTreeSet<(String, String)> = functions
        .iter()
        .filter_map(|f| f.py_module.clone().map(|m| (m, f.exposed.clone())))
        .collect();
    cov.functions.pyi_only = py_functions
        .iter()
        .filter(|(m, f)| !rs_fn_keys.contains(&(m.clone(), f.name.clone())))
        .count() as i64;

    (edges, cov)
}

// ---------- sidecar ----------

/// NT repo HEAD (`boundary_build.py:723-731`) — check=True crash family.
pub fn nt_head_sha(nt_repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(nt_repo)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|e| format!("git rev-parse HEAD 執行失敗：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse HEAD 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CoverageSummary {
    pub class_matched: i64,
    pub class_total: i64,
    pub class_pct: f64,
    pub method_matched: i64,
    pub method_total: i64,
    pub method_pct: f64,
    pub function_matched: i64,
    pub function_total: i64,
    pub function_pct: f64,
}

/// Python round(x, 1) (half-even) for the pct faces.
fn py_round1(v: f64) -> f64 {
    let scaled = v * 10.0;
    let r = if (scaled - scaled.trunc()).abs() == 0.5 {
        let floor = scaled.floor();
        if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }
    } else {
        scaled.round()
    };
    r / 10.0
}

fn pct(matched: i64, total: i64) -> f64 {
    if total == 0 {
        0.0
    } else {
        py_round1(matched as f64 / total as f64 * 100.0)
    }
}

pub fn coverage_summary(coverage: &Coverage) -> CoverageSummary {
    CoverageSummary {
        class_matched: coverage.classes.matched,
        class_total: coverage.classes.rs_pyclass_total,
        class_pct: pct(coverage.classes.matched, coverage.classes.rs_pyclass_total),
        method_matched: coverage.methods.matched,
        method_total: coverage.methods.rs_methods_on_matched_classes,
        method_pct: pct(coverage.methods.matched, coverage.methods.rs_methods_on_matched_classes),
        function_matched: coverage.functions.matched,
        function_total: coverage.functions.rs_functions,
        function_pct: pct(coverage.functions.matched, coverage.functions.rs_functions),
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct KnownGaps {
    pub pyi_only_class_custom_data_macro_est: i64,
    pub rs_only_class_declared_not_stubbed: i64,
    pub rs_only_method_field_property: i64,
    pub rs_only_method_getter_empty_stub_est: i64,
    pub rs_only_method_variant_residual: i64,
}

pub fn known_gaps_of(coverage: &Coverage) -> KnownGaps {
    KnownGaps {
        pyi_only_class_custom_data_macro_est: coverage.classes.pyi_only,
        rs_only_class_declared_not_stubbed: coverage.classes.rs_only,
        rs_only_method_field_property: *coverage.methods.rs_only_by_kind.get("field_property").unwrap_or(&0),
        rs_only_method_getter_empty_stub_est: *coverage.methods.rs_only_by_kind.get("getter").unwrap_or(&0),
        rs_only_method_variant_residual: *coverage.methods.rs_only_by_kind.get("variant").unwrap_or(&0),
    }
}

/// Write the sidecar DB (`boundary_build.py:771-840`): `<sha8>.db`
/// idempotent overwrite, meta table (nt_commit is the sole authority key
/// — the generic commit key is dropped), boundary_edges with indices.
pub fn write_sidecar(
    nt_repo: &Path,
    nt_commit: &str,
    edges: &[Edge],
    coverage: &Coverage,
    out_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("{} 建立失敗：{}", out_dir.display(), e))?;
    let db = out_dir.join(format!("{}.db", &nt_commit[..8]));
    let _ = std::fs::remove_file(&db);
    let mut meta = make_meta(TOOL, nt_repo, Some(nt_commit), vec![])?;
    meta.shift_remove("commit"); // nt_commit is the authority key
    meta.insert("nt_commit".into(), serde_json::Value::String(nt_commit.to_string()));
    meta.insert("edges_count".into(), serde_json::Value::String(edges.len().to_string()));
    meta.insert(
        "coverage_summary".into(),
        serde_json::Value::String(serde_json::to_string(&coverage_summary(coverage)).unwrap()),
    );
    meta.insert(
        "known_gaps".into(),
        serde_json::Value::String(serde_json::to_string(&known_gaps_of(coverage)).unwrap()),
    );
    let conn = Connection::open(&db).map_err(|e| format!("{} 開啟失敗：{e}", db.display()))?;
    conn.execute_batch(
        "CREATE TABLE boundary_edges (
            id INTEGER PRIMARY KEY,
            level TEXT,
            py_symbol TEXT,
            pyi_path TEXT, pyi_line INTEGER,
            rs_symbol TEXT,
            rs_path TEXT, rs_line INTEGER,
            match_kind TEXT,
            method_kind TEXT
        );
        CREATE INDEX idx_edges_py ON boundary_edges(py_symbol);
        CREATE INDEX idx_edges_rs ON boundary_edges(rs_path, rs_line);
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);",
    )
    .map_err(|e| format!("schema 建立失敗：{e}"))?;
    for e in edges {
        conn.execute(
            "INSERT INTO boundary_edges (level,py_symbol,pyi_path,pyi_line,rs_symbol,rs_path,rs_line,match_kind,method_kind) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            (
                e.level,
                &e.py_symbol,
                &e.pyi_path,
                e.pyi_line,
                &e.rs_symbol,
                &e.rs_path,
                e.rs_line,
                e.match_kind,
                e.method_kind.as_deref(),
            ),
        )
        .map_err(|err| format!("edge 寫入失敗：{err}"))?;
    }
    for (k, v) in &meta {
        let val = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        conn.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", (k, val))
            .map_err(|err| format!("meta 寫入失敗：{err}"))?;
    }
    drop(conn);
    Ok(db)
}

/// literal prefix directory of a glob (existence validation).
fn glob_base(glob: &str) -> PathBuf {
    let mut parts = PathBuf::new();
    for seg in glob.split('/') {
        if seg.contains('*') || seg.contains('?') || seg.contains('[') {
            break;
        }
        parts.push(seg);
    }
    parts
}

fn assert_repo(repo: &Path, roots: &[crate::profile::ScanRoot]) -> Result<(), String> {
    if !repo.is_dir() {
        return Err(format!("repo 不存在：{}", repo.display()));
    }
    for sr in roots {
        for g in [&sr.path, &sr.pyi] {
            let base = repo.join(glob_base(g));
            if !base.is_dir() {
                return Err(format!(
                    "scan_root base 不存在：{}（profile glob '{g}'）——檢查 .code-reality.toml [[scan_root]] 與 --repo 是否同 repo",
                    base.display()
                ));
            }
        }
    }
    Ok(())
}

fn glob_repo(repo: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut opts = glob::MatchOptions::new();
    opts.require_literal_leading_dot = false;
    let full = repo.join(pattern).to_string_lossy().into_owned();
    let mut out: Vec<PathBuf> = glob::glob_with(&full, opts)
        .into_iter()
        .flatten()
        .filter_map(|r| r.ok())
        .collect();
    out.sort();
    out
}

/// Full pipeline (`boundary_build.py:870-904`): validate → scan →
/// reconcile → sidecar. Returns the DB path.
pub fn build_sidecar(repo: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let repo = crate::common::resolve(repo);
    let profile = load_profile(&repo)?;
    let roots = scan_roots(profile.as_ref()).to_vec();
    if roots.is_empty() {
        return Err(format!(
            "{} 無 boundary 掃描根——需 repo profile（.code-reality.toml）定義 [[scan_root]]（path=rs glob、pyi=pyi glob）＋顯式 --repo（SM-1b：不內建任何 repo 預設）",
            repo.display()
        ));
    }
    assert_repo(&repo, &roots)?;
    let sha = nt_head_sha(&repo)?;

    let (mut classes, mut methods, mut functions) = (Vec::new(), Vec::new(), Vec::new());
    for p in roots.iter().flat_map(|sr| glob_repo(&repo, &sr.path)) {
        let (c, m, f) = scan_rust_file(&p, &repo);
        classes.extend(c);
        methods.extend(m);
        functions.extend(f);
    }
    let mut py_classes: Vec<(String, PyClass)> = Vec::new();
    let mut py_functions: Vec<(String, PyFunction)> = Vec::new();
    for p in roots.iter().flat_map(|sr| glob_repo(&repo, &sr.pyi)) {
        let pstr = p.to_string_lossy().into_owned();
        let pabs = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        let repo_canon = std::fs::canonicalize(&repo).unwrap_or_else(|_| repo.clone());
        let rel_for_module = pabs
            .strip_prefix(&repo_canon)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or(pstr);
        let module = pyi_module(&rel_for_module)?;
        let (pcs, pfs) = parse_pyi(&pabs, &repo_canon)?;
        py_classes.extend(pcs.into_iter().map(|c| (module.clone(), c)));
        py_functions.extend(pfs.into_iter().map(|f| (module.clone(), f)));
    }
    let (edges, coverage) = build_boundary(&classes, &methods, &functions, &py_classes, &py_functions);
    write_sidecar(&repo, &sha, &edges, &coverage, out_dir)
}

/// Route a `code-reality boundary_build ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 boundary_build");
    };
    let spec = ToolSpec {
        flags: &[
            FlagSpec { long: "--repo", short: None, kind: Kind::Value { metavar: "REPO" } },
            FlagSpec { long: "--out-dir", short: None, kind: Kind::Value { metavar: "OUT_DIR" } },
        ],
        positionals: &[],
    };
    let values = match parse(&spec, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: concat!(
                    "usage: boundary_build [-h] --repo REPO [--out-dir OUT_DIR]\n",
                    "\n",
                    "PyO3 boundary extractor（sidecar build）\n",
                    "\n",
                    "options:\n",
                    "  -h, --help            show this help message and exit\n",
                    "  --repo REPO           掃描目標 repo 根（唯讀；需該 repo 的 .code-reality.toml 定義 [[scan_root]]）\n",
                    "  --out-dir OUT_DIR\n",
                )
                .to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, .. } => values,
    };
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let out_dir = values
        .get("--out-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::engine::expand_home(DEFAULT_OUT_DIR));
    let db = match build_sidecar(Path::new(&repo), &out_dir) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
    // read meta back for the report faces
    let conn = match connect_ro(&db) {
        Ok(c) => c,
        Err(e) => return ToolOutput::crash(e),
    };
    let meta: BTreeMap<String, String> = conn
        .prepare("SELECT key, value FROM meta")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map(|i| i.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    drop(conn);
    let summary: CoverageSummary = serde_json::from_str(
        meta.get("coverage_summary").map(String::as_str).unwrap_or("{}"),
    )
    .unwrap_or_default();
    let gaps: KnownGaps =
        serde_json::from_str(meta.get("known_gaps").map(String::as_str).unwrap_or("{}")).unwrap_or_default();
    let nt_commit = meta.get("nt_commit").cloned().unwrap_or_default();
    let mut stdout = format!(
        "[OK] boundary sidecar: {} edges -> {}（NT {}）\n",
        meta.get("edges_count").map(|s| s.as_str()).unwrap_or("?"),
        db.display(),
        nt_commit.chars().take(8).collect::<String>()
    );
    stdout.push_str(&format!(
        "  class: {}/{}（{}%）｜method: {}/{}（{}%）｜function: {}/{}（{}%）\n",
        summary.class_matched,
        summary.class_total,
        summary.class_pct,
        summary.method_matched,
        summary.method_total,
        summary.method_pct,
        summary.function_matched,
        summary.function_total,
        summary.function_pct
    ));
    stdout.push_str(&format!(
        "  known gaps: pyi-only class {}（custom_data! 巨集估）、rs-only class {}（declared-not-stubbed）、field_property {}（credential 省略）、getter {}（空白 stub 估）、variant {}（轉換殘餘）\n",
        gaps.pyi_only_class_custom_data_macro_est,
        gaps.rs_only_class_declared_not_stubbed,
        gaps.rs_only_method_field_property,
        gaps.rs_only_method_getter_empty_stub_est,
        gaps.rs_only_method_variant_residual
    ));
    stdout.push_str(&format!(
        "[LOG] 查詢：uv run --project ~/Github/ai-rules python -m code_reality.boundary <symbol> --repo <repo>｜裸 sqlite：sqlite3 {} 'SELECT * FROM boundary_edges WHERE py_symbol LIKE \"%LiveNode%\"'\n",
        db.display()
    ));
    ToolOutput { stdout, stderr: String::new(), exit_code: 0 }
}

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::ToolOutput;
