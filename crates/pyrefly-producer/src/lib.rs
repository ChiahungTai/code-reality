//! pyrefly-producer — Rust-native Python occurrence producer (EP
//! ep-pyrefly-native-producer S1). Reads a repo with the linked Pyrefly
//! engine, resolves every def / reference / call site, and emits a SCIP
//! protobuf index into the repo-keyed sidecar slot; the existing
//! `--stamp-meta` → `--build-cache` → `graph_db build` pipeline consumes
//! it unchanged (SCIP face — cache schema untouched).

pub mod api;
pub mod emit;
pub mod symbol;
pub mod walk;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ruff_text_size::{TextRange, TextSize};

#[derive(Debug, Default)]
pub struct EmitReport {
    pub files: usize,
    pub defs: usize,
    pub references: usize,
    pub call_sites: usize,
    pub collapsed_dunder_pairs: usize,
    pub dropped_external_targets: usize,
    pub dropped_local_bindings: usize,
    pub dropped_unchained: usize,
    pub unresolved_refs: usize,
    pub unresolved_calls: usize,
    /// B7b: constructor calls minted in fn shape (`Cls().`). Split so
    /// idempotence is machine-checkable — refs >= defs and defs equals
    /// the number of distinct called classes.
    pub minted_pseudo_ctor_refs: usize,
    pub minted_pseudo_ctor_defs: usize,
    pub skipped_no_ast: Vec<String>,
    /// Config-finder errors from `Handles::all` (P-8 loud list).
    pub finder_errors: Vec<String>,
    /// Sidecar artifacts (cache db / stamped meta) that existed beside
    /// the slot and were superseded by this run — a stale lsp-harvest
    /// cache left in place would be silently trusted by
    /// `graph_db build`'s lsp fast-path (silent bad-db, 2026-08-28).
    pub invalidated_sidecars: Vec<String>,
    pub index_path: PathBuf,
    pub elapsed_secs: f64,
}

/// Emit a SCIP index for `repo_root` into its sidecar slot (or `out`).
pub fn emit(repo_root: &Path, out: Option<&Path>) -> Result<EmitReport, String> {
    let t0 = Instant::now();
    let mut files = Vec::new();
    collect_py_files(repo_root, &mut files);
    if files.is_empty() {
        return Err(format!("no .py files under {}", repo_root.display()));
    }
    let (project, version) = project_identity(repo_root);
    let disc = symbol::discriminator(&project, &version);

    let driven = api::drive(repo_root, files)?;

    let index_path = match out {
        Some(p) => p.to_path_buf(),
        None => code_reality::engine::default_index_path(repo_root)?,
    };

    let mut report = EmitReport {
        files: driven.modules.len(),
        unresolved_refs: driven.unresolved_refs,
        unresolved_calls: driven.unresolved_calls,
        skipped_no_ast: driven.skipped_no_ast.clone(),
        finder_errors: driven.finder_errors.clone(),
        index_path: index_path.clone(),
        ..Default::default()
    };

    // Chain material for target-symbol minting: rel path → def nodes.
    let mut def_nodes_by_path: HashMap<&str, &Vec<(TextRange, walk::DefKind, String)>> =
        HashMap::with_capacity(driven.modules.len());
    for m in &driven.modules {
        def_nodes_by_path.insert(m.rel_path.as_str(), &m.def_nodes);
    }

    let mut emitter = emit::IndexEmitter::new();

    // Pass 1 (B7b) — pure data scan, no emission: collect the
    // pseudo-constructor symbols of classes hit by a constructor call.
    // A call site that also resolved to a corpus `__init__` (B7a shape)
    // is excluded — its edge already exists in method grain and must not
    // be double-minted.
    let mut called_classes: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for m in &driven.modules {
        for c in &m.calls {
            let (kept, _collapsed) = symbol::collapse_dunder(&c.targets);
            let (targets, _dropped) =
                mint_targets(&kept, &disc, repo_root, &def_nodes_by_path, true);
            if targets
                .iter()
                .any(|t| t.kind == walk::DefKind::Function && t.name == "__init__")
            {
                continue;
            }
            for t in targets.iter().filter(|t| t.kind == walk::DefKind::Class) {
                if let Some(pseudo) = symbol::pseudo_ctor_symbol(&t.symbol) {
                    called_classes.insert(pseudo);
                }
            }
        }
    }

    for m in &driven.modules {
        let src = std::fs::read_to_string(repo_root.join(&m.rel_path))
            .map_err(|e| format!("read {}: {e}", m.rel_path))?;
        emitter.start_module(&m.rel_path, &src);

        // Module identity is derived from the rel path on BOTH sides
        // (defs and resolved targets): pyrefly's handle naming is
        // fallback-shaped for non-project corpora (`core` for
        // pkg/core.py) while import resolution says `pkg.core` — using
        // either pyrefly name splits symbol identity in two.
        let module_id = module_of_rel(&m.rel_path);
        for d in &m.defs {
            let sym = symbol::def_symbol(&disc, &module_id, d);
            emitter.push_def(&sym, d.name_range, d.node_range);
            report.defs += 1;
            // B7b def backfill: classes that receive a constructor call
            // get ONE pseudo-constructor DEF occurrence so the minted
            // call reference survives the graph build's def-symbol gate.
            // Emission stays inside the defs loop — the set is membership
            // -only (iterating it would leak HashSet order into the
            // byte-determinism contract).
            if d.kind == walk::DefKind::Class {
                if let Some(pseudo) = symbol::pseudo_ctor_symbol(&sym) {
                    if called_classes.contains(&pseudo) {
                        emitter.push_def(&pseudo, d.name_range, d.node_range);
                        report.minted_pseudo_ctor_defs += 1;
                    }
                }
            }
        }
        for r in &m.refs {
            let (kept, collapsed) = symbol::collapse_dunder(&r.targets);
            report.collapsed_dunder_pairs += collapsed;
            let (targets, dropped) =
                mint_targets(&kept, &disc, repo_root, &def_nodes_by_path, false);
            report.dropped_external_targets += dropped.external;
            report.dropped_local_bindings += dropped.local_binding;
            report.dropped_unchained += dropped.unchained;
            for t in &targets {
                emitter.push_reference(&t.symbol, r.range, r.kind);
                report.references += 1;
            }
        }
        for c in &m.calls {
            let (kept, collapsed) = symbol::collapse_dunder(&c.targets);
            report.collapsed_dunder_pairs += collapsed;
            let (targets, dropped) =
                mint_targets(&kept, &disc, repo_root, &def_nodes_by_path, true);
            report.dropped_external_targets += dropped.external;
            report.dropped_local_bindings += dropped.local_binding;
            report.dropped_unchained += dropped.unchained;
            let b7a_site = targets
                .iter()
                .any(|t| t.kind == walk::DefKind::Function && t.name == "__init__");
            for t in &targets {
                // B7b: a constructor call resolved to the class itself
                // (dataclass / plain object-inherit — no corpus
                // `__init__`) is minted in fn shape; a B7a site (corpus
                // `__init__` present) keeps its method-grain edge.
                let sym = if !b7a_site && t.kind == walk::DefKind::Class {
                    match symbol::pseudo_ctor_symbol(&t.symbol) {
                        Some(p) => {
                            report.minted_pseudo_ctor_refs += 1;
                            p
                        }
                        None => t.symbol.clone(),
                    }
                } else {
                    t.symbol.clone()
                };
                emitter.push_call_reference(&sym, c.site.range);
                report.call_sites += 1;
            }
        }
    }
    emitter.write(&index_path)?;
    // Invalidate sidecar artifacts derived from the PREVIOUS index: the
    // cache db, stamped meta, and fn-defs sidecar are all keyed to this
    // slot, so leaving them behind hands every downstream consumer a
    // superseded producer face. A concurrent remove of the same file is
    // the target state, not an error (review R1).
    for stale in [
        code_reality::cache::sqlite_path(&index_path),
        code_reality::engine::meta_path(&index_path),
        code_reality::fndefs::fndefs_path(&index_path),
    ] {
        match std::fs::remove_file(&stale) {
            Ok(()) => report.invalidated_sidecars.push(stale.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "remove {} (index 本身已寫入 {}: {e})",
                    stale.display(),
                    index_path.display()
                ))
            }
        }
    }
    report.elapsed_secs = t0.elapsed().as_secs_f64();
    Ok(report)
}

/// Dotted module id of a corpus rel path (`pkg/core.py` → `pkg.core`,
/// `pkg/__init__.py` → `pkg`).
pub fn module_of_rel(rel: &str) -> String {
    let stem = rel.strip_suffix(".py").unwrap_or(rel);
    let stem = stem.strip_suffix("/__init__").unwrap_or(stem);
    stem.replace('/', ".")
}

/// One in-corpus minted target of a reference/call site. `kind`/`name`
/// ride along so the B7b constructor-mint can distinguish class targets
/// (`Cls#`) from B7a initializer targets (`Cls#__init__().`).
#[derive(Debug, Clone)]
pub struct MintedTarget {
    pub symbol: String,
    pub def_range: TextRange,
    pub kind: walk::DefKind,
    pub name: String,
}

/// Drop-reason breakdown for the emit report — external targets,
/// local-binding guard hits, and in-corpus positions with no collected
/// def node are different phenomena and must not share one counter.
#[derive(Debug, Default)]
pub struct DropCounts {
    pub external: usize,
    pub local_binding: usize,
    pub unchained: usize,
}

/// Drop targets outside the corpus and mint SCIP symbols for the rest.
/// `call_site` marks a CALL resolution: only there is a Class-kind
/// display mismatch import aliasing (see the local-binding guard).
fn mint_targets(
    targets: &[&api::ResolvedTarget],
    disc: &str,
    repo_root: &Path,
    def_nodes_by_path: &HashMap<&str, &Vec<(TextRange, walk::DefKind, String)>>,
    call_site: bool,
) -> (Vec<MintedTarget>, DropCounts) {
    let mut out = Vec::new();
    let mut dropped = DropCounts::default();
    for t in targets {
        // Exact corpus keying: strip the repo root off the resolved
        // module path (external targets — stdlib/site-packages — fail
        // the strip). Suffix matching would mis-key same-tailed external
        // paths onto corpus files and mint from the wrong def-node table.
        let module_rel: Option<&str> = t
            .module_path
            .strip_prefix(repo_root)
            .ok()
            .and_then(|p| p.to_str())
            .filter(|rel| def_nodes_by_path.contains_key(*rel));
        let Some(rel) = module_rel else {
            dropped.external += 1;
            continue;
        };
        let nodes = def_nodes_by_path[rel];
        match enclosing_chain(nodes, t.def_start) {
            Some((kind, name, chain)) => {
                if std::env::var_os("PYREFLY_PRODUCER_DEBUG").is_some() {
                    eprintln!(
                        "[debug] target {} @ {}:{:?} -> {:?} in {:?}",
                        t.display_name.as_deref().unwrap_or("?"),
                        t.module_path.display(),
                        usize::from(t.def_start),
                        (kind, &name),
                        chain
                    );
                }
                // Local-binding guard: a resolved name whose display
                // differs from the innermost collected def (parameters,
                // function locals, comprehension bindings) is not a
                // reference to that def — minting it would fabricate a
                // reference to the enclosing function. Exception, CALL
                // SITES ONLY: a Class-kind innermost target on a call is
                // import aliasing (`from m import Plain as P; P()`) —
                // but a plain-load resolution landing anywhere in a
                // class body (lambda/walrus/comprehension bindings are
                // not def nodes) must stay dropped.
                if let Some(dn) = t.display_name.as_deref() {
                    if dn != name && !(call_site && kind == walk::DefKind::Class) {
                        dropped.local_binding += 1;
                        continue;
                    }
                }
                out.push(MintedTarget {
                    symbol: symbol::target_symbol(disc, &module_of_rel(rel), &chain, kind, &name),
                    def_range: TextRange::new(t.def_start, t.def_start),
                    kind,
                    name: name.clone(),
                });
            }
            // In-corpus module but the def position matches no collected
            // def node (e.g. a comprehension binding): count, don't
            // silently mint a wrong symbol.
            None => dropped.unchained += 1,
        }
    }
    (out, dropped)
}

/// Innermost def-node chain containing `pos`: outermost → innermost
/// (the innermost — tightest — entry is returned separately with its
/// kind/name).
fn enclosing_chain(
    nodes: &[(TextRange, walk::DefKind, String)],
    pos: TextSize,
) -> Option<(walk::DefKind, String, Vec<(walk::DefKind, String)>)> {
    let mut containing: Vec<&(TextRange, walk::DefKind, String)> =
        nodes.iter().filter(|(r, _, _)| r.contains(pos)).collect();
    containing.sort_by_key(|(r, _, _)| r.len());
    // Ascending by span: the FIRST entry is the tightest (innermost).
    let innermost = containing.first().cloned()?;
    let chain = containing[1..]
        .iter()
        .rev()
        .map(|(_, k, n)| (*k, n.clone()))
        .collect();
    Some((innermost.1, innermost.2.clone(), chain))
}

/// Walk .py files, skipping dot-dirs, `__pycache__`, and the common
/// non-dot dependency dirs (`venv`-shaped envs, `node_modules`) whose
/// inclusion would flood the corpus with site-packages definitions.
pub fn collect_py_files(root: &Path, out: &mut Vec<PathBuf>) {
    const SKIP_DIRS: [&str; 3] = ["__pycache__", "venv", "node_modules"];
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && !SKIP_DIRS.contains(&name) {
                collect_py_files(&p, out);
            }
        } else if p.extension().and_then(|e| e.to_str()) == Some("py") {
            out.push(p);
        }
    }
}

/// (project, version) from pyproject.toml [project]; fallback
/// (repo-basename, "0.0.0") for repos without Python packaging metadata.
fn project_identity(repo_root: &Path) -> (String, String) {
    let pyproject = repo_root.join("pyproject.toml");
    let Ok(text) = std::fs::read_to_string(&pyproject) else {
        let name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        return (name, "0.0.0".to_string());
    };
    let mut in_project = false;
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_project = t == "[project]";
            continue;
        }
        if !in_project {
            continue;
        }
        if let Some(rest) = t.strip_prefix("name") {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                name = Some(unquote(v));
            }
        } else if let Some(rest) = t.strip_prefix("version") {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                version = Some(unquote(v));
            }
        }
    }
    let fallback = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    (
        name.unwrap_or(fallback).replace('_', "-"),
        version.unwrap_or_else(|| "0.0.0".to_string()),
    )
}

fn unquote(v: &str) -> String {
    // Cut an inline TOML comment first (`version = "1.0" # note`) so the
    // tail can't survive into the discriminator.
    let v = v.split('#').next().unwrap_or("").trim_end();
    v.trim().trim_matches('"').trim_matches('\'').to_string()
}
