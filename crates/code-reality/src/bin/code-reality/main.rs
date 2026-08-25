//! `code-reality` — umbrella CLI. Subcommand names mirror the frozen
//! Python module names verbatim (relay minimal-diff contract). The bin
//! owns printing and exiting; all behavior comes from the lib as
//! [`code_reality::ToolOutput`].
//!
//! No clap: the frozen Python surface is argparse semantics (abbreviations,
//! negative-number positionals, `--` separator, last-wins repeats) that
//! clap rejects — argv passes through raw and each tool module mimics
//! argparse exactly.
//!
//! Umbrella routing note (R4): this bin is NOT a Python parity face —
//! `--help`/usage texts follow the carrier as subcommands land (the
//! frozen Python has no umbrella at all; per-tool surfaces are the gate).

fn main() {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let output = route(&refs);
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::exit(output.exit_code);
}

fn route(argv: &[&str]) -> code_reality::ToolOutput {
    use code_reality::ToolOutput;
    match argv.first() {
        Some(&"scip_refs") => code_reality::cli::run(argv),
        Some(&"snapshot") => code_reality::snapshot::run(argv),
        Some(&"transition") => code_reality::transition::run(argv),
        Some(&"graph_audit") => code_reality::graph_audit::run(argv),
        Some(&"graph_csv") => code_reality::graph_csv::run(argv),
        Some(&"hub_refs") => code_reality::hub_refs::run(argv),
        Some(&"tour_manifest") => code_reality::tour_manifest::run(argv),
        Some(&"tour_validate") => code_reality::tour_validate::run(argv),
        Some(&"tour_upgrade") => code_reality::tour_upgrade::run(argv),
        Some(&"--help") | Some(&"-h") | Some(&"--version") => ToolOutput {
            stdout: format!(
                "code-reality — toolchain umbrella ({})\n",
                SUBCOMMANDS.join(", ")
            ),
            stderr: String::new(),
            exit_code: 0,
        },
        _ => ToolOutput {
            stdout: String::new(),
            stderr: format!("usage: code-reality <{}> [args]\n", SUBCOMMANDS.join("|")),
            exit_code: 2,
        },
    }
}

const SUBCOMMANDS: [&str; 9] = ["scip_refs", "snapshot", "transition", "graph_audit", "graph_csv", "hub_refs", "tour_manifest", "tour_validate", "tour_upgrade"];
