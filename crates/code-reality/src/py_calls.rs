//! Syntactic Python call-site scanner — the CALLS-vs-REFERENCES edge
//! split (occurrence EP S3-F2, absorbed 2026-08-28: build-side
//! mechanism). SCIP carries no call role and scip-python emits uniformly
//! unmarked references (empirically: 146,867 non-def occurrences, all
//! role=ReadAccess / syntax_kind=0); the pyrefly producer's call
//! occurrences ride the same reference channel. The graph build site is
//! the one place holding both the repo root and the sources, so it
//! re-derives the split: parse each referenced file once, collect
//! (rel_path, 1-based line, callee name); a reference row at that
//! (file, line) whose symbol tail equals a callee name there becomes a
//! CALLS edge. Line+name grain can mis-mark a same-line plain load of
//! the same name — the S5 pair-set metric (frozen grain) absorbs that
//! (same caller, same callee).

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::Expr;
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextSize};

/// (rel_path, 1-based line, callee name) for every call expression.
pub type CallSiteSet = HashSet<(String, i64, String)>;

/// Scan `rels` under `repo_root`. Unreadable or unparseable files
/// contribute no marks (their refs stay REFERENCES) and are reported
/// loudly — degrade, don't fail the build.
pub fn call_sites(repo_root: &Path, rels: &BTreeSet<String>) -> (CallSiteSet, Vec<String>) {
    let mut out = CallSiteSet::new();
    let mut warns = Vec::new();
    for rel in rels {
        let Ok(src) = std::fs::read_to_string(repo_root.join(rel)) else {
            warns.push(format!(
                "[WARN] py_calls 讀取失敗，該檔 refs 全標 REFERENCES：{rel}"
            ));
            continue;
        };
        let parsed = match parse_module(&src) {
            Ok(p) => p,
            Err(e) => {
                warns.push(format!(
                    "[WARN] py_calls 解析失敗（{e}），該檔 refs 全標 REFERENCES：{rel}"
                ));
                continue;
            }
        };
        let line_starts = line_starts(src.as_bytes());
        let mut v = CallVisitor {
            rel,
            line_starts: &line_starts,
            out: &mut out,
        };
        v.visit_body(&parsed.syntax().body);
    }
    (out, warns)
}

struct CallVisitor<'a> {
    rel: &'a str,
    line_starts: &'a [usize],
    out: &'a mut CallSiteSet,
}

impl<'a> SourceOrderVisitor<'a> for CallVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            let name = callee_name(&call.func);
            let line = self.line_of(call.func.range().start());
            self.out.insert((self.rel.to_string(), line, name));
        }
        source_order::walk_expr(self, expr);
    }
}

impl CallVisitor<'_> {
    fn line_of(&self, off: TextSize) -> i64 {
        let off = usize::from(off);
        let idx = match self.line_starts.binary_search(&off) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        idx as i64 + 1
    }
}

/// Callee name of a call func expression — the method name for
/// attribute calls (`obj.method()` → `method`).
fn callee_name(func: &Expr) -> String {
    match func {
        Expr::Attribute(a) => a.attr.to_string(),
        Expr::Name(n) => n.id.to_string(),
        _ => String::new(),
    }
}

fn line_starts(src: &[u8]) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in src.iter().enumerate() {
        if *b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_line_and_callee_name() {
        let dir = std::env::temp_dir().join(format!("pycalls-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(
            dir.join("pkg").join("m.py"),
            "def top():\n    return 1\n\n\ndef user():\n    top()\n    obj.method()\n    x = top\n",
        )
        .unwrap();
        let mut rels = BTreeSet::new();
        rels.insert("pkg/m.py".to_string());
        let (calls, warns) = call_sites(&dir, &rels);
        assert!(warns.is_empty(), "{warns:?}");
        assert!(calls.contains(&("pkg/m.py".to_string(), 6, "top".to_string())));
        assert!(calls.contains(&("pkg/m.py".to_string(), 7, "method".to_string())));
        // `x = top` is a load, not a call.
        assert!(!calls.contains(&("pkg/m.py".to_string(), 8, "top".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_degrades_loud() {
        let mut rels = BTreeSet::new();
        rels.insert("nope.py".to_string());
        let (calls, warns) = call_sites(Path::new("/nonexistent-repo"), &rels);
        assert!(calls.is_empty());
        assert_eq!(warns.len(), 1);
    }
}
