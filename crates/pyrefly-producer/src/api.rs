//! Single-point isolation of every Pyrefly import (EP S1 isolation rule):
//! upgrading the pinned git rev (or any internal-API break) touches only
//! this file. Everything outside sees plain data types.
//!
//! `drive` builds the engine exactly like the S0 spike: the construction
//! chain is `default_config_finder(None)` → `State::new(cf, AllThreads)` →
//! `new_transaction` → `checkpoint(Ok(iter))` → `Handles::new(files).all`
//! → **`transaction.run(...)` (the scheduler — every getter is pure-read
//! and returns None without it)** → walk ASTs → `find_definition`.

use std::convert::Infallible;
use std::path::{Path, PathBuf};

use pyrefly::commands::check::Handles;
use pyrefly::commands::config_finder::default_config_finder;
use pyrefly::state::lsp::FindPreference;
use pyrefly::state::require::Require;
use pyrefly::state::state::State;
use pyrefly_util::thread_pool::ThreadCount;
use ruff_text_size::{TextRange, TextSize};

use crate::walk;

/// One resolved definition target of a reference/call site.
#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub module_path: PathBuf,
    pub def_start: TextSize,
    pub display_name: Option<String>,
}

#[derive(Debug)]
pub struct RefResolution {
    pub kind: walk::RefKind,
    pub range: TextRange,
    pub targets: Vec<ResolvedTarget>,
}

#[derive(Debug)]
pub struct CallResolution {
    pub site: walk::CallSite,
    pub targets: Vec<ResolvedTarget>,
}

#[derive(Debug)]
pub struct ModuleData {
    pub rel_path: String,
    pub defs: Vec<walk::DefSite>,
    /// Every def/class/module-level-assign node in the file, used to build
    /// the qualified-name chain of a resolved target living in this module.
    pub def_nodes: Vec<(TextRange, walk::DefKind, String)>,
    pub refs: Vec<RefResolution>,
    pub calls: Vec<CallResolution>,
}

#[derive(Debug, Default)]
pub struct DriveResult {
    pub modules: Vec<ModuleData>,
    /// Files the engine loaded but produced no AST for (P-8 loud list).
    pub skipped_no_ast: Vec<String>,
    /// Config-finder errors surfaced by `Handles::all`. NOTE: at the
    /// pinned rev `1d64c4b` `all()` unconditionally returns an empty
    /// vector — this stays plumbed so a rev upgrade surfaces errors
    /// loudly (P-8) instead of silently changing behavior.
    pub finder_errors: Vec<String>,
    pub unresolved_refs: usize,
    pub unresolved_calls: usize,
}

/// Resolve every def / name-reference / call site of `files` against the
/// Pyrefly engine and return the buffered result. Call positions use the
/// CALLEE name position (for `obj.method()` the attribute range), not the
/// receiver start — resolving at the receiver would bind to the object.
pub fn drive(repo_root: &Path, files: Vec<PathBuf>) -> Result<DriveResult, String> {
    let mut result = DriveResult::default();

    let config_finder = default_config_finder(None);
    let checked = config_finder
        .checkpoint(Ok::<_, Infallible>(files.into_iter()))
        .map_err(|e| format!("config checkpoint failed: {e:?}"))?;
    let state = State::new(config_finder, ThreadCount::AllThreads);
    let mut transaction = state.new_transaction(Require::Everything, None);
    let handles = Handles::new(checked);
    let (handles, _search_paths, errs) = handles.all(state.config_finder());
    result.finder_errors = errs.iter().map(|e| e.get_message()).collect();

    transaction.run(&handles, Require::Everything, None);

    let preference = FindPreference {
        prefer_pyi: false,
        ..Default::default()
    };

    for handle in &handles {
        let path = handle.path().as_path().to_path_buf();
        let rel_path = relativize(repo_root, &path);
        let Some(ast) = transaction.get_ast(handle) else {
            result.skipped_no_ast.push(rel_path);
            continue;
        };
        let sites = walk::collect(&ast);

        let resolve = |pos| match transaction.find_definition(handle, pos, preference) {
            Ok(items) => items
                .iter()
                .map(|it| ResolvedTarget {
                    module_path: it.module.path().as_path().to_path_buf(),
                    def_start: it.definition_range.start(),
                    display_name: it.display_name.clone(),
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        let mut refs = Vec::with_capacity(sites.refs.len());
        for r in &sites.refs {
            let targets = resolve(r.range.start());
            if targets.is_empty() {
                result.unresolved_refs += 1;
            }
            refs.push(RefResolution {
                kind: r.kind,
                range: r.range,
                targets,
            });
        }
        let mut calls = Vec::with_capacity(sites.calls.len());
        for c in &sites.calls {
            let targets = resolve(c.name_pos);
            if targets.is_empty() {
                result.unresolved_calls += 1;
            }
            calls.push(CallResolution { site: *c, targets });
        }

        result.modules.push(ModuleData {
            rel_path,
            defs: sites.defs,
            def_nodes: sites.def_nodes,
            refs,
            calls,
        });
    }
    // pyrefly's `Handles` iterates an internal HashSet — the caller's
    // file order is destroyed and the emit would be run-to-run
    // non-deterministic (document order, cache seq, span tie-breaks).
    // Sort by rel path: module-internal AST walks are already ordered.
    result.modules.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(result)
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
