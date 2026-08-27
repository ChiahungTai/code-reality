//! SCIP symbol forms for the pyrefly producer, mirroring the scip-python
//! convention with a `pyrefly ` leading discriminator (infer_language's
//! third Python prefix — graph_db.rs):
//!
//! - function            `` `pkg.mod`/fn(). ``
//! - nested function     `` `pkg.mod`/outer().inner(). ``
//! - method              `` `pkg.mod`/Class#method(). ``
//! - class               `` `pkg.mod`/Class# ``
//! - module variable     `` `pkg.mod`/NAME.NAME. ``
//! - class attribute     `` `pkg.mod`/Class#NAME. ``
//!
//! Only functions/methods carry the `().` tail the fn_tail_name gate
//! requires — classes and variables are filtered at ingest exactly like
//! the scip-python face (R2-3 parity, deliberate).

use crate::api::ResolvedTarget;
use crate::walk::{DefKind, DefSite};

pub const PRODUCER_NAME: &str = "pyrefly-1.3";
/// The pinned rev rides the version so tool_info identity matches the
/// Cargo.toml git-dep exactly (the tag number alone under-reports it).
pub const PRODUCER_VERSION: &str = "1.3.0-dev.2+1d64c4b";

pub fn discriminator(project: &str, version: &str) -> String {
    format!("pyrefly python {project} {version} ")
}

/// Descriptor suffix of one scope/def entry, scip-python style: classes
/// join with `#`, everything else with `.`; callables get `()`.
/// `module_var` marks a module-level variable, which repeats the name
/// (observed form `VAR.VAR.`); a class-level attribute uses the plain
/// `NAME.` form (observed: `Class#ATTR.`).
fn descriptor(kind: DefKind, name: &str, module_var: bool) -> String {
    match kind {
        DefKind::Class => format!("{name}#"),
        DefKind::Function => format!("{name}()."),
        DefKind::Variable if module_var => format!("{name}.{name}."),
        DefKind::Variable => format!("{name}."),
    }
}

/// Symbol of a def site in `module` (chain = scope + the def itself).
pub fn def_symbol(disc: &str, module: &str, def: &DefSite) -> String {
    let mut s = format!("{disc}`{module}`/");
    for e in &def.scope {
        let kind = if e.is_class {
            DefKind::Class
        } else {
            DefKind::Function
        };
        s.push_str(&descriptor(kind, &e.name, false));
    }
    let module_var = def.kind == DefKind::Variable && def.scope.is_empty();
    s.push_str(&descriptor(def.kind, &def.name, module_var));
    s
}

/// Symbol of a resolved target from a pre-built chain (outermost first,
/// excluding the innermost entry which is passed as `kind`/`name`).
pub fn target_symbol(
    disc: &str,
    module: &str,
    chain: &[(DefKind, String)],
    kind: DefKind,
    name: &str,
) -> String {
    let mut s = format!("{disc}`{module}`/");
    for (k, n) in chain {
        s.push_str(&descriptor(*k, n, false));
    }
    let module_var = kind == DefKind::Variable && chain.is_empty();
    s.push_str(&descriptor(kind, name, module_var));
    s
}

/// Dunder-pair collapse (EP S1, validate POC-3: every multi-target call
/// site is the `__init__`+`__new__` constructor pair). When the target
/// set is exactly that pair, keep `__init__` — the class's own
/// constructor — and drop the (usually inherited object.)`__new__`.
/// Returns (kept, collapsed_count).
pub fn collapse_dunder(targets: &[ResolvedTarget]) -> (Vec<&ResolvedTarget>, usize) {
    let names: Vec<&str> = targets
        .iter()
        .map(|t| t.display_name.as_deref().unwrap_or(""))
        .collect();
    let is_dunder_pair = targets.len() > 1
        && names.iter().all(|n| *n == "__init__" || *n == "__new__")
        && names.contains(&"__init__");
    if !is_dunder_pair {
        return (targets.iter().collect(), 0);
    }
    let kept: Vec<&ResolvedTarget> = targets
        .iter()
        .filter(|t| t.display_name.as_deref() == Some("__init__"))
        .collect();
    let collapsed = targets.len() - kept.len();
    (kept, collapsed)
}
