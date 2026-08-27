//! Symbol-form unit tests (EP S1): scip-python-mirroring shapes, the
//! fn_tail_name gate contract, and dunder-pair collapse.

use pyrefly_producer::symbol::{collapse_dunder, def_symbol, discriminator};
use pyrefly_producer::walk::{DefKind, DefSite, ScopeEntry};

fn site(kind: DefKind, name: &str, scope: &[(&str, bool)]) -> DefSite {
    DefSite {
        kind,
        name: name.to_string(),
        name_range: ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(0),
            ruff_text_size::TextSize::new(0),
        ),
        node_range: ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(0),
            ruff_text_size::TextSize::new(0),
        ),
        scope: scope
            .iter()
            .map(|(n, c)| ScopeEntry {
                name: n.to_string(),
                is_class: *c,
            })
            .collect(),
    }
}

const DISC: &str = "pyrefly python mini 0.0.0 ";

fn sym(kind: DefKind, name: &str, scope: &[(&str, bool)]) -> String {
    def_symbol(DISC, "pkg.core", &site(kind, name, scope))
}

#[test]
fn function_and_method_forms_pass_fn_tail_gate() {
    let f = sym(DefKind::Function, "top_fn", &[]);
    assert_eq!(f, "pyrefly python mini 0.0.0 `pkg.core`/top_fn().");
    assert_eq!(code_reality::engine::fn_tail_name(&f), Some("top_fn"));

    let m = sym(DefKind::Function, "greet", &[("Greeter", true)]);
    assert_eq!(m, "pyrefly python mini 0.0.0 `pkg.core`/Greeter#greet().");
    assert_eq!(code_reality::engine::fn_tail_name(&m), Some("greet"));
}

#[test]
fn class_and_variable_forms_fail_fn_tail_gate_with_parity() {
    let c = sym(DefKind::Class, "Greeter", &[]);
    assert_eq!(c, "pyrefly python mini 0.0.0 `pkg.core`/Greeter#");
    assert_eq!(code_reality::engine::fn_tail_name(&c), None);

    let v = sym(DefKind::Variable, "CONSTANT", &[]);
    assert_eq!(v, "pyrefly python mini 0.0.0 `pkg.core`/CONSTANT.CONSTANT.");
    assert_eq!(code_reality::engine::fn_tail_name(&v), None);

    let a = sym(DefKind::Variable, "tag", &[("Greeter", true)]);
    assert_eq!(a, "pyrefly python mini 0.0.0 `pkg.core`/Greeter#tag.");
    assert_eq!(code_reality::engine::fn_tail_name(&a), None);
}

#[test]
fn nested_function_chains_descriptors() {
    let n = sym(DefKind::Function, "inner", &[("outer", false)]);
    assert_eq!(n, "pyrefly python mini 0.0.0 `pkg.core`/outer().inner().");
    assert_eq!(code_reality::engine::fn_tail_name(&n), Some("inner"));
}

#[test]
fn discriminator_uses_underscore_free_project_name() {
    assert_eq!(
        discriminator("mosaic-alpha", "1.2.3"),
        "pyrefly python mosaic-alpha 1.2.3 "
    );
}

#[test]
fn collapse_dunder_pair_keeps_init() {
    use pyrefly_producer::api::ResolvedTarget;
    let init = ResolvedTarget {
        module_path: "/x/pkg/core.py".into(),
        def_start: ruff_text_size::TextSize::new(1),
        display_name: Some("__init__".into()),
    };
    let new = ResolvedTarget {
        module_path: "/typeshed/builtins.pyi".into(),
        def_start: ruff_text_size::TextSize::new(2),
        display_name: Some("__new__".into()),
    };
    let pair = [init.clone(), new.clone()];
    let (kept, collapsed) = collapse_dunder(&pair);
    assert_eq!(collapsed, 1);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].display_name.as_deref(), Some("__init__"));

    // Non-dunder multi-target (true ambiguity elsewhere): kept intact.
    let mut other = init.clone();
    other.display_name = Some("greet".into());
    let ambiguous = [init.clone(), other];
    let (kept, collapsed) = collapse_dunder(&ambiguous);
    assert_eq!(collapsed, 0);
    assert_eq!(kept.len(), 2);

    // Single __new__ target (no __init__ in set): untouched.
    let single = [new];
    let (kept, collapsed) = collapse_dunder(&single);
    assert_eq!(collapsed, 0);
    assert_eq!(kept.len(), 1);
}
