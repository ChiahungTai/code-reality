//! Shared argparse mimic — the frozen CLI contract (allow_abbrev /
//! negative-number positionals / `--` separator / last-wins repeats /
//! `-h` on stdout) that the R2 scip_refs parser established, extracted
//! so the graph-family tools share one implementation. Byte faces:
//! `-h` prints the tool's pinned usage/help on stdout (exit 0); usage
//! errors print on stderr (not gated) with exit 2.
//!
//! `cli.rs` (scip_refs) deliberately keeps its own parser for now: its
//! hand-tuned surface (query positional + negative-number oracle pins)
//! predates this module and R2/R3 parity tests pin its edges — migrate
//! it onto `ToolSpec` only with a dedicated parity-anchored refactor.
//!
//! Known gaps vs argparse (unreachable with the current flag tables):
//! short StoreTrue clusters like `-jj` (argparse splits them; we refuse),
//! multi-token error listings, and some `-o`-family message wordings
//! (stderr best-effort face).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    StoreTrue,
    /// Value flag with the argparse metavar (usage rendering only).
    Value { metavar: &'static str },
}

#[derive(Debug, Clone, Copy)]
pub struct FlagSpec {
    pub long: &'static str,
    pub short: Option<char>,
    pub kind: Kind,
}

/// Tool argument surface: flags (help registered implicitly), positional
/// names, and how many positionals are required.
pub struct ToolSpec {
    pub flags: &'static [FlagSpec],
    /// Positional names in declaration order; all of them required
    /// (argparse `nargs` default) — max positionals = names.len().
    pub positionals: &'static [&'static str],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok {
        /// resolved long name → value (StoreTrue → None presence)
        values: HashMap<&'static str, Option<String>>,
        positionals: Vec<String>,
    },
    Help,
    /// Usage error: exit 2, stderr message (best-effort face).
    Err(String),
}

/// Python 3.14 argparse negative-number test (pinned against the local
/// oracle in R2): `-` alone is positional; a `-`-prefixed token whose
/// next char is an optional `.` then an ASCII digit is a negative number
/// (prefix match — `-5x` and `-5.5.5` count); space-containing tokens
/// are positional.
pub fn is_negative_numberish(tok: &str) -> bool {
    if tok == "-" || tok.contains(' ') {
        return true;
    }
    let Some(rest) = tok.strip_prefix('-') else {
        return false;
    };
    let rest = rest.strip_prefix('.').unwrap_or(rest);
    rest.starts_with(|c: char| c.is_ascii_digit())
}

/// Option-looking token (for flag-value refusal): `-`-prefixed, longer
/// than `-`, and not a negative number. Python refuses these as flag
/// values (`argument X: expected one argument`), including exactly `--`.
pub fn looks_like_option(tok: &str) -> bool {
    tok.starts_with('-') && tok != "-" && !is_negative_numberish(tok)
}

fn resolve_long<'a>(spec: &'a ToolSpec, name: &str) -> Resolution<'a> {
    // exact match wins even when it is a prefix of another flag
    if name == "--help" {
        return Resolution::Help;
    }
    if let Some(f) = spec.flags.iter().find(|f| f.long == name) {
        return Resolution::Flag(f);
    }
    let prefix = &name[2..];
    let flag_matches: Vec<&FlagSpec> = spec
        .flags
        .iter()
        .filter(|f| f.long[2..].starts_with(prefix))
        .collect();
    // --help participates in abbreviation matching like any registered flag
    let help_matches = "help".starts_with(prefix);
    match (flag_matches.len(), help_matches) {
        (1, false) => Resolution::Flag(flag_matches[0]),
        (0, true) => Resolution::Help,
        (0, false) => Resolution::Unknown,
        _ => Resolution::Ambiguous,
    }
}

enum Resolution<'a> {
    Flag(&'a FlagSpec),
    Help,
    Unknown,
    Ambiguous,
}

/// argparse-style parse of the tokens AFTER the subcommand.
pub fn parse(spec: &ToolSpec, toks: &[&str]) -> Outcome {
    let mut values: HashMap<&'static str, Option<String>> = HashMap::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut positional_only = false;
    let max_pos = spec.positionals.len();
    let mut i = 0usize;
    macro_rules! fail {
        ($msg:expr) => {
            return Outcome::Err($msg.to_string())
        };
    }
    macro_rules! push_positional {
        ($tok:expr) => {{
            if positionals.len() == max_pos {
                fail!(format!("unrecognized arguments: {}", $tok));
            }
            positionals.push($tok.to_string());
        }};
    }
    while i < toks.len() {
        let tok = toks[i];
        if positional_only {
            push_positional!(tok);
            i += 1;
            continue;
        }
        if tok == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if tok == "-h" {
            return Outcome::Help;
        }
        if let Some(rest) = tok.strip_prefix("--") {
            let (name, inline_val) = match rest.split_once('=') {
                Some((n, v)) => (format!("--{}", n), Some(v.to_string())),
                None => (tok.to_string(), None),
            };
            match resolve_long(spec, &name) {
                Resolution::Help => return Outcome::Help,
                Resolution::Flag(f) => match f.kind {
                    Kind::StoreTrue => {
                        if let Some(v) = inline_val {
                            fail!(format!(
                                "argument {}: ignored explicit argument '{}'",
                                f.long, v
                            ));
                        }
                        values.insert(f.long, None);
                    }
                    Kind::Value { .. } => {
                        let val = match inline_val {
                            Some(v) => v,
                            None => match toks.get(i + 1) {
                                Some(v) if !looks_like_option(v) => {
                                    i += 1;
                                    v.to_string()
                                }
                                _ => fail!(format!(
                                    "argument {}: expected one argument",
                                    f.long
                                )),
                            },
                        };
                        values.insert(f.long, Some(val)); // last wins
                    }
                },
                Resolution::Unknown => fail!(format!("unrecognized arguments: {}", tok)),
                Resolution::Ambiguous => {
                    fail!(format!("argument {}: ambiguous option", name))
                }
            }
            i += 1;
            continue;
        }
        if let Some(short_seq) = tok.strip_prefix('-') {
            // short options: matched only when registered; `-o=v`/`-ovalue`
            // carry the value (argparse splits on `=` for shorts too)
            let mut chars = short_seq.chars();
            let Some(c) = chars.next() else {
                // bare "-"
                push_positional!(tok);
                i += 1;
                continue;
            };
            if let Some(f) = spec.flags.iter().find(|f| f.short == Some(c)) {
                let rest: String = chars.by_ref().collect();
                match f.kind {
                    Kind::StoreTrue => {
                        if !rest.is_empty() {
                            fail!(format!(
                                "argument {}: ignored explicit argument '{}'",
                                f.long, rest
                            ));
                        }
                        values.insert(f.long, None);
                    }
                    Kind::Value { .. } => {
                        let val = if let Some(v) = rest.strip_prefix('=') {
                            v.to_string()
                        } else if !rest.is_empty() {
                            rest
                        } else {
                            match toks.get(i + 1) {
                                Some(v) if !looks_like_option(v) => {
                                    i += 1;
                                    v.to_string()
                                }
                                _ => fail!(format!(
                                    "argument {}: expected one argument",
                                    f.long
                                )),
                            }
                        };
                        values.insert(f.long, Some(val));
                    }
                }
                i += 1;
                continue;
            }
            if !looks_like_option(tok) {
                // negative numberish or bare "-"
                push_positional!(tok);
                i += 1;
                continue;
            }
            fail!(format!("unrecognized arguments: {}", tok));
        }
        push_positional!(tok);
        i += 1;
    }
    if positionals.len() < max_pos {
        let missing: Vec<&str> = spec.positionals[positionals.len()..].to_vec();
        fail!(format!(
            "the following arguments are required: {}",
            missing.join(", ")
        ));
    }
    Outcome::Ok {
        values,
        positionals,
    }
}

/// Required-value extraction with the argparse missing-required message.
pub fn required<'a>(
    values: &'a HashMap<&'static str, Option<String>>,
    long: &'static str,
) -> Result<&'a str, String> {
    values
        .get(long)
        .and_then(|v| v.as_deref())
        .ok_or_else(|| format!("the following arguments are required: {}", long))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: ToolSpec = ToolSpec {
        flags: &[
            FlagSpec {
                long: "--repo",
                short: None,
                kind: Kind::Value { metavar: "REPO" },
            },
            FlagSpec {
                long: "--json",
                short: None,
                kind: Kind::StoreTrue,
            },
            FlagSpec {
                long: "--output-prefix",
                short: Some('o'),
                kind: Kind::Value {
                    metavar: "OUTPUT_PREFIX",
                },
            },
        ],
        positionals: &["snapshot_a", "snapshot_b"],
    };

    #[test]
    fn basics_flags_and_positionals() {
        let out = parse(
            &SPEC,
            &["a.json", "b.json", "--repo", "/r", "--json", "-o", "pre"],
        );
        match out {
            Outcome::Ok {
                values,
                positionals,
            } => {
                assert_eq!(positionals, vec!["a.json", "b.json"]);
                assert_eq!(values.get("--repo"), Some(&Some("/r".into())));
                assert_eq!(values.get("--json"), Some(&None));
                assert_eq!(values.get("--output-prefix"), Some(&Some("pre".into())));
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn abbrev_and_inline_and_last_wins() {
        let out = parse(&SPEC, &["--re=/first", "--repo=/second", "a", "b"]);
        match out {
            Outcome::Ok { values, .. } => {
                assert_eq!(values.get("--repo"), Some(&Some("/second".into())));
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn short_inline_forms() {
        for args in [
            &["-o=x", "a", "b"][..],
            &["-ox", "a", "b"][..],
            &["-o", "x", "a", "b"][..],
        ] {
            let out = parse(&SPEC, args);
            match out {
                Outcome::Ok { values, .. } => {
                    assert_eq!(values.get("--output-prefix"), Some(&Some("x".into())));
                }
                other => panic!("{:?} for {:?}", other, args),
            }
        }
    }

    #[test]
    fn ddash_separator_and_negative_positional() {
        let out = parse(&SPEC, &["--", "-weird.json", "b"]);
        match out {
            Outcome::Ok { positionals, .. } => {
                assert_eq!(positionals, vec!["-weird.json", "b"]);
            }
            other => panic!("{:?}", other),
        }
        let out = parse(&SPEC, &["-5", "b"]);
        match out {
            Outcome::Ok { positionals, .. } => {
                assert_eq!(positionals, vec!["-5", "b"]);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn error_family() {
        assert_eq!(
            parse(&SPEC, &["a"]),
            Outcome::Err("the following arguments are required: snapshot_b".into())
        );
        assert_eq!(
            parse(&SPEC, &["--nope", "a", "b"]),
            Outcome::Err("unrecognized arguments: --nope".into())
        );
        assert_eq!(
            parse(&SPEC, &["--repo", "--json", "a", "b"]),
            Outcome::Err("argument --repo: expected one argument".into())
        );
        assert_eq!(
            parse(&SPEC, &["--json=1", "a", "b"]),
            Outcome::Err("argument --json: ignored explicit argument '1'".into())
        );
        assert_eq!(
            parse(&SPEC, &["a", "b", "c"]),
            Outcome::Err("unrecognized arguments: c".into())
        );
    }

    #[test]
    fn help_faces() {
        assert_eq!(parse(&SPEC, &["-h"]), Outcome::Help);
        assert_eq!(parse(&SPEC, &["--help"]), Outcome::Help);
        assert_eq!(parse(&SPEC, &["--he"]), Outcome::Help);
        assert_eq!(parse(&SPEC, &["a", "-h"]), Outcome::Help);
    }
}
