//! `code-reality` — umbrella CLI. Subcommand names mirror the frozen Python
//! module names verbatim (relay minimal-diff contract). The bin owns printing
//! and exiting; all behavior comes from the lib as [`code_reality::ToolOutput`].
//!
//! No clap: the frozen Python surface is argparse semantics (abbreviations,
//! negative-number positionals, `--` separator, last-wins repeats) that clap
//! rejects — argv passes through raw and `cli::run` mimics argparse exactly.

fn main() {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    match refs.first() {
        Some(&"scip_refs") => {}
        Some(&"--help") | Some(&"-h") | Some(&"--version") => {
            println!("code-reality — toolchain umbrella (scip_refs)");
            std::process::exit(0);
        }
        _ => {
            eprintln!("usage: code-reality <scip_refs> [args]");
            std::process::exit(2);
        }
    }
    let output = code_reality::cli::run(&refs);
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::exit(output.exit_code);
}
