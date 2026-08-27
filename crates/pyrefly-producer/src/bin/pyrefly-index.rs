//! `pyrefly-index` — emit a SCIP protobuf index for a Python repo with
//! the linked Pyrefly engine (EP ep-pyrefly-native-producer S1).
//!
//! Usage: pyrefly-index --repo <repo-root> [--out <index.scip>]
//!
//! The default output is the repo-keyed sidecar slot
//! (`~/.mosaic/code-reality/scip/<repo-basename>/index.scip`); `--out`
//! lets probe runs write beside the primary slot without cutover.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut repo: Option<String> = None;
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--repo" => repo = args.next(),
            "--out" => out = args.next(),
            "--help" | "-h" => {
                println!("usage: pyrefly-index --repo <repo-root> [--out <index.scip>]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unrecognized argument {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(repo) = repo else {
        eprintln!("error: --repo is required");
        return ExitCode::FAILURE;
    };
    let repo = Path::new(&repo).to_path_buf();
    match pyrefly_producer::emit(&repo, out.as_deref().map(Path::new)) {
        Ok(r) => {
            println!(
                "[OK] pyrefly-index: {} files, {} defs, {} refs, {} call sites \
                 (dunder collapsed {}, external/local/unchained dropped {}/{}/{}, \
                 unresolved refs/calls {}/{}) -> {} in {:.2}s",
                r.files,
                r.defs,
                r.references,
                r.call_sites,
                r.collapsed_dunder_pairs,
                r.dropped_external_targets,
                r.dropped_local_bindings,
                r.dropped_unchained,
                r.unresolved_refs,
                r.unresolved_calls,
                r.index_path.display(),
                r.elapsed_secs,
            );
            if !r.skipped_no_ast.is_empty() {
                eprintln!(
                    "[WARN] {} files produced no AST: {:?}",
                    r.skipped_no_ast.len(),
                    r.skipped_no_ast
                );
            }
            if !r.finder_errors.is_empty() {
                eprintln!(
                    "[WARN] config-finder errors ({}): {:?}",
                    r.finder_errors.len(),
                    r.finder_errors
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[FAIL] pyrefly-index: {e}");
            ExitCode::FAILURE
        }
    }
}
