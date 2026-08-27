//! Pure ruff-AST collectors: no Pyrefly import lives here (the isolation
//! boundary is `api.rs`); these walk any `ruff_python_ast::Mod`.

use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Function,
    Class,
    Variable,
}

/// One lexical scope level enclosing a def site (outermost first).
#[derive(Debug, Clone)]
pub struct ScopeEntry {
    pub name: String,
    pub is_class: bool,
}

#[derive(Debug, Clone)]
pub struct DefSite {
    pub kind: DefKind,
    pub name: String,
    pub name_range: TextRange,
    /// Full node range (whole def/assign stmt) — the SCIP DEF occurrence
    /// range must cover the body for spans-based reference attribution.
    pub node_range: TextRange,
    /// Enclosing scope chain, outermost → innermost, excluding self.
    /// Only module/class scopes are tracked — function locals are not
    /// emitted (SCIP local-symbol form is out of corpus scope).
    pub scope: Vec<ScopeEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// A load-context name or attribute-name position.
    Load,
    /// An import alias position.
    Import,
}

#[derive(Debug, Clone, Copy)]
pub struct RefSite {
    pub kind: RefKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy)]
pub struct CallSite {
    /// Position of the callee NAME — for `obj.method()` the attribute
    /// range start, for `fn()` the name start. Resolving at the whole
    /// `call.func` start would bind attribute calls to the receiver.
    pub name_pos: TextSize,
    pub range: TextRange,
}

pub struct ModuleSites {
    pub defs: Vec<DefSite>,
    pub refs: Vec<RefSite>,
    pub calls: Vec<CallSite>,
    /// (full node range, kind, name) of every def/class/module-or-class
    /// level assign — chain material for resolved targets in this file.
    pub def_nodes: Vec<(TextRange, DefKind, String)>,
}

pub fn collect(module: &ruff_python_ast::ModModule) -> ModuleSites {
    let mut v = Collector {
        scope: Vec::new(),
        fn_depth: 0,
        defs: Vec::new(),
        refs: Vec::new(),
        calls: Vec::new(),
        call_name_ranges: std::collections::HashSet::new(),
        def_nodes: Vec::new(),
    };
    v.visit_body(&module.body);
    // A call func name is emitted as a CallSite — the same position must
    // not ALSO surface as a plain reference (double emission doubled
    // caller sites downstream).
    v.refs.retain(|r| !v.call_name_ranges.contains(&r.range));
    ModuleSites {
        defs: v.defs,
        refs: v.refs,
        calls: v.calls,
        def_nodes: v.def_nodes,
    }
}

struct Collector {
    /// Module/class/function scope stack. Module- and class-level defs
    /// are emitted (function-local bindings are not — SCIP local-symbol
    /// form is out of corpus scope); function entries ride the chain
    /// only so nested def symbols stay face-consistent.
    scope: Vec<ScopeEntry>,
    fn_depth: usize,
    defs: Vec<DefSite>,
    refs: Vec<RefSite>,
    calls: Vec<CallSite>,
    /// Callee-name ranges of collected call sites (dedupe filter).
    call_name_ranges: std::collections::HashSet<TextRange>,
    def_nodes: Vec<(TextRange, DefKind, String)>,
}

impl<'a> SourceOrderVisitor<'a> for Collector {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => {
                self.push_def(DefKind::Function, &f.name, f.name.range(), stmt.range());
                // Function scopes join the scope chain so a nested def's
                // symbol (`outer().inner().`) matches on BOTH faces —
                // the def face (scope stack) and the target face
                // (enclosing_chain over def_nodes, which always included
                // function nodes). Skipping it here made every reference
                // to a nested function mint a symbol no def row carried.
                self.scope.push(ScopeEntry {
                    name: f.name.to_string(),
                    is_class: false,
                });
                self.fn_depth += 1;
                source_order::walk_stmt(self, stmt);
                self.fn_depth -= 1;
                self.scope.pop();
            }
            Stmt::ClassDef(c) => {
                self.push_def(DefKind::Class, &c.name, c.name.range(), stmt.range());
                self.scope.push(ScopeEntry {
                    name: c.name.to_string(),
                    is_class: true,
                });
                source_order::walk_stmt(self, stmt);
                self.scope.pop();
            }
            Stmt::Assign(a) => {
                if self.fn_depth == 0 {
                    if let Some(name) = single_name_target(&a.targets) {
                        self.push_def(DefKind::Variable, &name.id, name.range(), stmt.range());
                    }
                }
                source_order::walk_stmt(self, stmt);
            }
            Stmt::AnnAssign(a) => {
                if self.fn_depth == 0 && a.target.is_name_expr() {
                    let name = a.target.as_name_expr().unwrap();
                    self.push_def(DefKind::Variable, &name.id, name.range(), stmt.range());
                }
                source_order::walk_stmt(self, stmt);
            }
            Stmt::Import(i) => {
                for alias in &i.names {
                    self.refs.push(RefSite {
                        kind: RefKind::Import,
                        range: alias.range(),
                    });
                }
            }
            Stmt::ImportFrom(imp) => {
                for alias in &imp.names {
                    self.refs.push(RefSite {
                        kind: RefKind::Import,
                        range: alias.range(),
                    });
                }
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(call) => {
                let name_range = callee_name_range(&call.func);
                self.calls.push(CallSite {
                    name_pos: name_range.start(),
                    range: call.func.range(),
                });
                self.call_name_ranges.insert(name_range);
                source_order::walk_expr(self, expr);
            }
            Expr::Name(n) => {
                if n.ctx.is_load() {
                    self.refs.push(RefSite {
                        kind: RefKind::Load,
                        range: n.range(),
                    });
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Attribute(a) => {
                if a.ctx.is_load() {
                    // The attribute NAME range — resolution binds to the
                    // attribute, while the base expression is visited
                    // separately as a Name reference below it.
                    self.refs.push(RefSite {
                        kind: RefKind::Load,
                        range: a.attr.range(),
                    });
                }
                source_order::walk_expr(self, expr);
            }
            _ => source_order::walk_expr(self, expr),
        }
    }
}

impl Collector {
    fn push_def(
        &mut self,
        kind: DefKind,
        name: &str,
        name_range: TextRange,
        node_range: TextRange,
    ) {
        self.defs.push(DefSite {
            kind,
            name: name.to_string(),
            name_range,
            node_range,
            scope: self.scope.clone(),
        });
        self.def_nodes.push((node_range, kind, name.to_string()));
    }
}

fn single_name_target(targets: &[Expr]) -> Option<&ruff_python_ast::ExprName> {
    match targets {
        [Expr::Name(n)] => Some(n),
        _ => None,
    }
}

/// Callee name range of a call func expression (the method name for
/// attribute calls — resolving at the receiver would bind to the object).
fn callee_name_range(func: &Expr) -> TextRange {
    match func {
        Expr::Attribute(a) => a.attr.range(),
        _ => func.range(),
    }
}
