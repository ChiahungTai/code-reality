//! js_ts_calls — syntax-aware JavaScript/TypeScript call-site collector
//! (S3). SCIP occurrence roles do not encode "this reference is a call";
//! the graph build re-derives the CALLS-vs-REFERENCES split by re-parsing
//! sources, exactly like [`crate::py_calls`] does for Python. Regex-only
//! classification is rejected by the blueprint (AD-8) — Tree-sitter is
//! the locked parser family (POC: direct/method/optional-chain/new/JSX/
//! TSX/non-call cases all classified exactly across the six extensions).
//!
//! Marks are `(rel_path, 1-based line, 0-based start column of the
//! callee NAME, callee tail name)` — the column grain disambiguates a
//! same-line plain load from a real call (`const a = f; f();` — the
//! post-codex-review identity; line+name grain would pollute both to
//! CALLS). The column is the tail identifier's start, matching where
//! SCIP anchors the occurrence (`obj.method()` anchors at `method`, not
//! `obj`). Column units are tree-sitter BYTES vs SCIP UTF-16 code
//! units — identical on ASCII lines; a non-ASCII prefix degrades that
//! line's marks conservatively (references stay REFERENCES).
//! Computed/dynamic forms (`obj[key]()`) mint NO guessed name.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use tree_sitter::{Node, Parser};

/// (rel_path, 1-based line, 0-based column, callee name) per static
/// call site — column is the callee name's start.
pub type CallSiteSet = HashSet<(String, i64, i64, String)>;

/// Scan `rels` under `repo_root`. Unreadable or unparseable files
/// contribute no marks (their refs stay REFERENCES) and are reported
/// loudly — degrade, don't fail the build. A tree with ANY error node
/// contributes no marks either: a missing mark is safe (REFERENCES), a
/// speculative mark from a broken parse would be a false CALLS.
pub fn call_sites(repo_root: &Path, rels: &BTreeSet<String>) -> (CallSiteSet, Vec<String>) {
    let mut out = CallSiteSet::new();
    let mut warns = Vec::new();
    let mut parsers = JsTsParsers::new();
    for rel in rels {
        let Some(ext) = rel.rsplit_once('.').map(|(_, e)| e) else {
            continue;
        };
        let Some(parser) = parsers.for_ext(ext) else {
            continue;
        };
        let Ok(src) = std::fs::read_to_string(repo_root.join(rel)) else {
            warns.push(format!(
                "[WARN] js_ts_calls 讀取失敗，該檔 refs 全標 REFERENCES：{rel}"
            ));
            continue;
        };
        let Some(tree) = parser.parse(&src, None) else {
            warns.push(format!(
                "[WARN] js_ts_calls 解析失敗（無樹），該檔 refs 全標 REFERENCES：{rel}"
            ));
            continue;
        };
        let root = tree.root_node();
        if root.has_error() {
            warns.push(format!(
                "[WARN] js_ts_calls 剖析含錯誤節點，該檔 refs 全標 REFERENCES（live edit？）：{rel}"
            ));
            continue;
        }
        collect(root, &src, rel, &mut out);
    }
    (out, warns)
}

struct JsTsParsers {
    js: Parser,
    ts: Parser,
    tsx: Parser,
}

impl JsTsParsers {
    fn new() -> Self {
        let mut js = Parser::new();
        js.set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("tree-sitter-javascript grammar loads");
        let mut ts = Parser::new();
        ts.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("tree-sitter-typescript grammar loads");
        let mut tsx = Parser::new();
        tsx.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("tree-sitter-tsx grammar loads");
        Self { js, ts, tsx }
    }

    fn for_ext(&mut self, ext: &str) -> Option<&mut Parser> {
        match ext {
            "js" | "jsx" | "mjs" | "cjs" => Some(&mut self.js),
            "ts" => Some(&mut self.ts),
            "tsx" => Some(&mut self.tsx),
            _ => None,
        }
    }
}

fn collect(node: Node, src: &str, rel: &str, out: &mut CallSiteSet) {
    let kind = node.kind();
    if kind == "call_expression" || kind == "new_expression" {
        let field = if kind == "call_expression" {
            "function"
        } else {
            "constructor"
        };
        if let Some(callee) = node.child_by_field_name(field) {
            if let Some(tail) = static_callee_tail_node(callee, 0) {
                let pos = tail.start_position();
                out.insert((
                    rel.to_string(),
                    pos.row as i64 + 1,
                    pos.column as i64,
                    tail.utf8_text(src.as_bytes())
                        .unwrap_or_default()
                        .to_string(),
                ));
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect(child, src, rel, out);
        }
    }
}

/// Terminal static callee tail NODE (identifier / property_identifier):
/// `foo()` → `foo`; `obj.foo()` → `foo`; `obj?.foo?.()` → `foo`;
/// `new Foo()` → `Foo`; `(foo)()` → `foo`; `await track<T>()` → `track`
/// (the grammar hangs await/type-args around the callee). Computed /
/// dynamic forms (`obj[key]()` → subscript_expression) return None — no
/// guessed names.
fn static_callee_tail_node<'a>(node: Node<'a>, depth: u8) -> Option<Node<'a>> {
    if depth > 3 {
        return None;
    }
    match node.kind() {
        "identifier" | "property_identifier" => Some(node),
        "member_expression" => {
            let prop = node.child_by_field_name("property")?;
            static_callee_tail_node(prop, depth + 1)
        }
        "parenthesized_expression" | "await_expression" => {
            let inner = node.named_child(0)?;
            static_callee_tail_node(inner, depth + 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(dir: &Path, files: &[(&str, &str)]) -> (CallSiteSet, Vec<String>) {
        for (rel, content) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        let rels: BTreeSet<String> = files.iter().map(|(r, _)| r.to_string()).collect();
        call_sites(dir, &rels)
    }

    #[test]
    fn direct_method_optional_and_new() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[(
                "a.mjs",
                "import { helper } from './lib.mjs';\n\
                 helper();\n\
                 client.push(helper);\n\
                 obj.method();\n\
                 chain?.deep?.run();\n\
                 const g = new Greeter();\n\
                 const ref = helper;\n",
            )],
        );
        assert!(warns.is_empty(), "{warns:?}");
        assert!(marks.contains(&("a.mjs".into(), 2, 0, "helper".into())));
        assert!(marks.contains(&("a.mjs".into(), 3, 7, "push".into())));
        assert!(marks.contains(&("a.mjs".into(), 4, 4, "method".into())));
        assert!(marks.contains(&("a.mjs".into(), 5, 13, "run".into())));
        assert!(marks.contains(&("a.mjs".into(), 6, 14, "Greeter".into())));
        // plain loads/property reads are not calls
        assert!(!marks.iter().any(|(_, l, _, _)| *l == 7));
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn six_extension_routing() {
        let t = tempfile::tempdir().unwrap();
        let files = [
            ("x.js", "f();\n"),
            ("x.jsx", "export const V = () => render();\n"),
            ("x.mjs", "f();\n"),
            ("x.cjs", "f();\n"),
            ("x.ts", "function g(): void {}\ng();\n"),
            ("x.tsx", "export const C = () => <div>{chip()}</div>;\n"),
        ];
        let (marks, warns) = scan(t.path(), &files);
        assert!(warns.is_empty(), "{warns:?}");
        for (rel, _) in &files {
            assert!(
                marks.iter().any(|(r, _, _, _)| r == rel),
                "no mark in {rel}: {marks:?}"
            );
        }
        assert!(marks.contains(&("x.ts".into(), 2, 0, "g".into())));
        assert!(marks.contains(&("x.tsx".into(), 1, 29, "chip".into())));
        assert!(marks.contains(&("x.jsx".into(), 1, 23, "render".into())));
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn computed_dynamic_and_dynamic_import_no_guess() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[("dyn.cjs", "obj[key]();\nimport('x').then(m => m.run());\n")],
        );
        assert!(warns.is_empty(), "{warns:?}");
        assert!(
            !marks.iter().any(|(_, _, _, n)| n == "key"),
            "computed access must not mint a name: {marks:?}"
        );
        // line 1 (obj[key]()) has NO mark at all
        assert!(!marks.iter().any(|(_, l, _, _)| *l == 1), "{marks:?}");
        // dynamic import is a distinct grammar node (no call mark — no
        // SCIP symbol carries the name "import" anyway); the .then()
        // method call and the inner run() DO carry marks (probe-verified)
        assert!(!marks.iter().any(|(_, _, _, n)| n == "import"), "{marks:?}");
        assert!(marks.contains(&("dyn.cjs".into(), 2, 12, "then".into())));
        assert!(marks.contains(&("dyn.cjs".into(), 2, 24, "run".into())));
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn type_only_and_import_references_are_not_calls() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[(
                "types.ts",
                "import type { Named } from './lib';\n\
                 export interface Shape extends Named {\n\
                   area(): number;\n\
                 }\n\
                 const s: Named = {} as Named;\n\
                 declare function area(): number;\n",
            )],
        );
        assert!(warns.is_empty(), "{warns:?}");
        // `area(): number` in an interface signature is not a call
        assert!(marks.is_empty(), "{marks:?}");
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn decorator_tagged_template_and_jsx_element_forms() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[(
                "forms.tsx",
                "function dec(_t: any, _k?: any) {}\n\
                 class Box {\n\
                   @dec() prop: number = 1;\n\
                   render() { return <Widget onClick={handler} />; }\n\
                   tag() { return html`<b>x</b>`; }\n\
                 }\n\
                 function handler() {}\n\
                 function html(s: TemplateStringsArray) { return s; }\n",
            )],
        );
        assert!(warns.is_empty(), "{warns:?}");
        // decorator @dec() is a real runtime invocation → mark
        assert!(
            marks.contains(&("forms.tsx".into(), 3, 1, "dec".into())),
            "{marks:?}"
        );
        // JSX attribute onClick={handler} is a plain reference — no mark
        assert!(
            !marks.iter().any(|(_, _, _, n)| n == "handler"),
            "{marks:?}"
        );
        // tagged template html`...` invokes the tag function — a real
        // call mark at the tag position (probe-verified on the grammar)
        assert!(
            marks.contains(&("forms.tsx".into(), 5, 15, "html".into())),
            "{marks:?}"
        );
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn broken_parse_degrades_loud_no_marks() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[("broken.ts", "function unclosed(: string {\n  return f();\n")],
        );
        assert!(marks.is_empty(), "{marks:?}");
        assert_eq!(warns.len(), 1, "{warns:?}");
        assert!(warns[0].contains("broken.ts"), "{warns:?}");
        let _ = std::fs::remove_dir_all(t.path());
    }

    #[test]
    fn missing_file_degrades_loud() {
        let mut rels = BTreeSet::new();
        rels.insert("nope.mjs".to_string());
        let (marks, warns) = call_sites(Path::new("/nonexistent-repo"), &rels);
        assert!(marks.is_empty());
        assert_eq!(warns.len(), 1);
    }

    #[test]
    fn generic_and_await_calls() {
        let t = tempfile::tempdir().unwrap();
        let (marks, warns) = scan(
            t.path(),
            &[(
                "gen.ts",
                "async function go(): Promise<void> {\n  await track<number>(1);\n  const m = await obj.fetch();\n}\n",
            )],
        );
        assert!(warns.is_empty(), "{warns:?}");
        assert!(marks.contains(&("gen.ts".into(), 2, 8, "track".into())));
        assert!(marks.contains(&("gen.ts".into(), 3, 22, "fetch".into())));
        let _ = std::fs::remove_dir_all(t.path());
    }
}
