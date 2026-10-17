//! `tour_reanchor` — CR-15: tour step re-anchor proposals driven by SCIP
//! symbol truth. Steps carrying `x-codeReality.symbol` resolve against
//! graph.db's current definition address (freshness gated — a non-
//! `current-tree` graph degrades the whole run to a WARN + skips);
//! `--bootstrap-symbols` backfills symbol-less steps through the shared
//! same-file candidate resolution (R1/R2 in `symbol_resolve`).
//!
//! Dry-run by default — proposals and skips only; the tour corpus and
//! the manifest see ZERO writes. (The one write a run may still make is
//! the freshness identity cache — the freshness face's standing
//! query-side WriteBack convention, see `identity.rs`;
//! `IdentityCachePolicy` itself is unchanged.) `--apply` writes ONLY
//! the verified-safe corrections: the step `line` (and `file` when the
//! symbol moved files) plus the bootstrap symbol backfill — a proposal
//! whose pattern does not hit the new line is never written (the player
//! consumes file/line/pattern; pattern updates belong to
//! `tour_upgrade`). The proposal face itself is double-gated
//! (round-2): the new target file must actually READ — an
//! absent/unreadable target skips the step with no proposal at all
//! (an empty-string pool would let a line-1 `^`-shaped anchor
//! false-verify and `--apply` relocate onto a nonexistent file) — and
//! the summary's `corrected` counts only pattern-verified proposals;
//! dead-pattern proposals carry their own `unverified` column. The
//! manifest, generator, sources and
//! anchored_commit are NEVER touched; unknown step fields and key order
//! survive the rewrite.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::symbol_resolve::{
    best_ident_filtered, FillCandidate, FillOutcome, SymbolGate, SymbolLoc,
};
use crate::ToolOutput;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--tours-dir",
            short: None,
            kind: Kind::Value {
                metavar: "TOURS_DIR",
            },
        },
        FlagSpec {
            long: "--bootstrap-symbols",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--apply",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: tour_reanchor [-h] [--repo REPO] [--tours-dir TOURS_DIR]\n",
    "                     [--bootstrap-symbols] [--apply]\n",
    "\n",
    "tour step 重錨提案工具（預設 dry-run 零寫入）——SCIP symbol 真值面\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO\n",
    "  --tours-dir TOURS_DIR corpus 根（遞迴；預設 .tours）\n",
    "  --bootstrap-symbols   對缺 symbol 的 step 以同檔同名候選回填\n",
    "                        x-codeReality.symbol（僅唯一候選；R1/R2）\n",
    "  --apply               實際寫檔（僅已驗證可安全修正：line/file+\n",
    "                        bootstrap symbol；manifest 永不觸碰）\n",
);

/// Def-keyword tokens excluded from the bootstrap ident pool BEFORE
/// ranking (CR-15 M7): fed to
/// [`crate::symbol_resolve::best_ident_filtered`], so a keyword can
/// never win by call-position/length/stable-order and mask the next
/// non-keyword token — `^def run\(` takes `run`, and so does the
/// paren-less `def run` (where `def` would win the stable sort
/// unfiltered).
fn is_keyword_ident(s: &str) -> bool {
    matches!(
        s,
        "def"
            | "fn"
            | "class"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "pub"
            | "async"
            | "unsafe"
            | "const"
            | "static"
            | "let"
            | "type"
            | "return"
            | "self"
    )
}

/// Bootstrap ident extraction (CR-15 decision 5): title first
/// (chain_tour step titles carry the frame symbol text), then the
/// pattern — both through the shared
/// [`crate::symbol_resolve::best_ident_filtered`] with the keyword
/// exclusion (M7: keywords are dropped pre-ranking, the best surviving
/// token wins). `None` when neither yields a token.
fn bootstrap_ident(step: &serde_json::Value) -> Option<(String, String)> {
    for src in [
        step.get("title").and_then(|v| v.as_str()).unwrap_or(""),
        step.get("pattern").and_then(|v| v.as_str()).unwrap_or(""),
    ] {
        let (last, full) = best_ident_filtered(src, &is_keyword_ident);
        if !last.is_empty() {
            return Some((last, full));
        }
    }
    None
}

fn step_str(step: &serde_json::Value, key: &str) -> String {
    step.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn step_symbol(step: &serde_json::Value) -> Option<&str> {
    step.get("x-codeReality")
        .and_then(|x| x.get("symbol"))
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
}

/// Does the compiled pattern hit line `ln` (1-based) of `lines`?
fn pattern_at(rx: &Option<regex::Regex>, lines: &[&str], ln: i64) -> bool {
    ln >= 1
        && ((ln as usize) <= lines.len())
        && rx
            .as_ref()
            .map(|r| r.is_match(lines[(ln - 1) as usize]))
            .unwrap_or(false)
}

/// R3-style candidate rendering for the proposal output (same shapes as
/// chain_tour's hint line: descriptor + line + kind, ≤4 + ellipsis).
fn cand_hint(cands: &[FillCandidate]) -> String {
    let mut parts: Vec<String> = cands
        .iter()
        .take(4)
        .map(|c| format!("{} :{}（{}）", c.descriptor, c.line, c.kind))
        .collect();
    if cands.len() > 4 {
        parts.push("…".into());
    }
    parts.join("／")
}

/// Route a `code-reality tour_reanchor ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 tour_reanchor");
    };
    let values = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, .. } => values,
    };
    let repo = crate::argparse::repo_root(&values);
    let tours_dir = values
        .get("--tours-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tours"));
    let bootstrap = values.contains_key("--bootstrap-symbols");
    let apply = values.contains_key("--apply");
    reanchor(&repo, &tours_dir, bootstrap, apply)
}

/// The reanchor pass (lib API): proposals on dry-run, verified-safe
/// writes on `--apply`. Exit 0 unless a corpus/graph read fails loud.
pub fn reanchor(repo: &Path, tours_dir: &Path, bootstrap: bool, apply: bool) -> ToolOutput {
    let mut stdout = String::new();
    let mut tours = match crate::tour_validate::iter_tours(repo, tours_dir, false) {
        Ok(t) => t,
        Err(e) => return ToolOutput::crash(e),
    };
    if tours.is_empty() {
        return ToolOutput::crash(format!("{} 無 .tour", repo.join(tours_dir).display()));
    }
    // The symbol-coordinate gate, once per run (CR-12): freshness
    // `current-tree` + readable graph.db, else every step skips and the
    // single WARN names the degrade.
    let (gate, gate_warn) = SymbolGate::for_repo(repo);
    if let Some(w) = gate_warn {
        stdout.push_str(&w);
    }
    let (mut corrected, mut bootstrap_filled) = (0usize, 0usize);
    let (mut unverified, mut ambiguous, mut skipped, mut exact) =
        (0usize, 0usize, 0usize, 0usize);
    let mut written = 0usize;
    for (rel, tour) in tours.iter_mut() {
        let mut tour_changed = false;
        let Some(steps) = tour.get_mut("steps").and_then(|s| s.as_array_mut()) else {
            continue;
        };
        for (i, step) in steps.iter_mut().enumerate() {
            let label = format!("  {rel} 步{}: ", i + 1);
            let f = step_str(step, "file");
            let ln = step.get("line").and_then(|v| v.as_i64());
            let pat = step_str(step, "pattern");
            if f.is_empty() || ln.is_none() || pat.is_empty() {
                skipped += 1;
                stdout.push_str(&format!("{label}跳過（缺 file/line/pattern）\n"));
                continue;
            }
            let ln = ln.unwrap_or(0);
            let p = repo.join(&f);
            let rx = regex::Regex::new(&pat).ok();
            let Some(loc) = gate.locator() else {
                // degraded gate — legacy face: the recorded anchor's file
                // must exist for any line/pattern work (existence first,
                // matching the healthy-gate priority)
                skipped += 1;
                if p.exists() {
                    stdout.push_str(&format!("{label}跳過（graph 退化——symbol 驗證不可用）\n"));
                } else {
                    stdout.push_str(&format!("{label}跳過（錨檔不存在 {f}）\n"));
                }
                continue;
            };
            // ---- symbol path (H1: locate BEFORE the old-file existence
            // check — a unique address decides even when the recorded
            // file is gone) ----
            if let Some(sym) = step_symbol(step) {
                match loc.locate(sym) {
                    Err(e) => {
                        // M4: a db error is never a silent miss
                        skipped += 1;
                        stdout.push_str(&format!(
                            "{label}跳過（graph.db symbol 查詢失敗——{e}）\n"
                        ));
                    }
                    Ok(SymbolLoc::Located {
                        file_path,
                        line_start,
                    }) => {
                        let Some(sf) = loc.rel_file(&file_path) else {
                            skipped += 1;
                            stdout.push_str(&format!(
                                "{label}跳過（symbol 檔在 repo 外——保留現行 line/pattern 錨）\n"
                            ));
                            continue;
                        };
                        let moved = sf != f;
                        if !moved && !p.exists() {
                            // the graph still points at the recorded file
                            // but the file is gone — no relocation target,
                            // nothing to verify against (H1)
                            skipped += 1;
                            stdout.push_str(&format!("{label}跳過（錨檔不存在 {f}）\n"));
                            continue;
                        }
                        if !moved && line_start == ln {
                            exact += 1;
                            continue;
                        }
                        // unique symbol, stale coordinates — the
                        // proposal is verified against the target's
                        // actual content BEFORE anything is proposed or
                        // counted (round-2): an unreadable target skips
                        // with no proposal (an empty-string pool would
                        // let a line-1 `^`-shaped anchor false-verify,
                        // and --apply would relocate onto a nonexistent
                        // file), and only a pattern-verified proposal
                        // counts corrected — dead-pattern proposals
                        // carry the separate unverified column
                        let target = if moved { repo.join(&sf) } else { p.clone() };
                        let Ok(verify_content) = std::fs::read_to_string(&target) else {
                            skipped += 1;
                            let note = if moved {
                                format!("新檔不可讀 {sf}")
                            } else {
                                format!("檔案不可讀 {f}")
                            };
                            stdout.push_str(&format!("{label}跳過（{note}）\n"));
                            continue;
                        };
                        let new_lines: Vec<&str> = verify_content.split('\n').collect();
                        let pat_at_new = pattern_at(&rx, &new_lines, line_start);
                        if moved {
                            stdout.push_str(&format!(
                                "{label}{f} -> {sf} L{ln}->L{line_start}（依據 SCIP symbol）\n"
                            ));
                        } else {
                            stdout.push_str(&format!(
                                "{label}L{ln}->L{line_start}（依據 SCIP symbol）\n"
                            ));
                        }
                        if !pat_at_new {
                            unverified += 1;
                            stdout.push_str(&format!(
                                "{label}  pattern 未命中新行——apply 不寫（pattern 更新超出本工具範疇）\n"
                            ));
                            continue;
                        }
                        corrected += 1;
                        if apply {
                            let obj = step.as_object_mut().unwrap();
                            if moved {
                                obj.insert(
                                    "file".into(),
                                    serde_json::Value::String(sf.clone()),
                                );
                            }
                            obj.insert("line".into(), serde_json::json!(line_start));
                            tour_changed = true;
                        }
                    }
                    Ok(SymbolLoc::Ambiguous(rows)) => {
                        if !p.exists() {
                            // H1: miss/ambiguous walk the old-file
                            // existence check first
                            skipped += 1;
                            stdout.push_str(&format!("{label}跳過（錨檔不存在 {f}）\n"));
                            continue;
                        }
                        ambiguous += 1;
                        let list = rows
                            .iter()
                            .take(4)
                            .map(|(fp, l)| {
                                let rf = loc.rel_file(fp).unwrap_or_else(|| fp.clone());
                                match l {
                                    Some(l) => format!("{rf}:{l}"),
                                    None => rf,
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("／");
                        stdout.push_str(&format!(
                            "{label}歧義（{} 列同名 symbol）：{list}\n",
                            rows.len()
                        ));
                    }
                    Ok(SymbolLoc::Miss) => {
                        // never read as "definition deleted" — the
                        // recorded line/pattern anchor stands (H1: a gone
                        // old file reports the existence skip, not a
                        // symbol verdict)
                        skipped += 1;
                        if p.exists() {
                            stdout.push_str(&format!(
                                "{label}跳過（symbol miss——保留現行 line/pattern 錨）\n"
                            ));
                        } else {
                            stdout.push_str(&format!("{label}跳過（錨檔不存在 {f}）\n"));
                        }
                    }
                }
                continue;
            }
            // no symbol — bootstrap face; the recorded anchor's file must
            // exist (the fill queries this file's candidates and verifies
            // the pattern against its content)
            if !p.exists() {
                skipped += 1;
                stdout.push_str(&format!("{label}跳過（錨檔不存在 {f}）\n"));
                continue;
            }
            if bootstrap {
                let content = std::fs::read_to_string(&p).unwrap_or_default();
                let lines: Vec<&str> = content.split('\n').collect();
                let Some((ident, ident_full)) = bootstrap_ident(step) else {
                    skipped += 1;
                    stdout.push_str(&format!(
                        "{label}跳過（bootstrap 無法抽 ident——title/pattern 皆無）\n"
                    ));
                    continue;
                };
                match loc.same_file_fill(&f, &ident, &ident_full) {
                    Err(e) => return ToolOutput::crash(e),
                    Ok(FillOutcome::Filled(node_line, Some(symbol))) => {
                        // defensive uniqueness re-check (decision 7
                        // discipline): the backfilled symbol must itself
                        // resolve to exactly one row
                        match loc.locate(&symbol) {
                            Ok(SymbolLoc::Ambiguous(rows)) => {
                                ambiguous += 1;
                                stdout.push_str(&format!(
                                    "{label}歧義（{} 列同名 symbol）——跳過\n",
                                    rows.len()
                                ));
                                continue;
                            }
                            Err(e) => {
                                skipped += 1;
                                stdout.push_str(&format!(
                                    "{label}跳過（graph.db symbol 查詢失敗——{e}）\n"
                                ));
                                continue;
                            }
                            _ => {}
                        }
                        // H2: the candidate must corroborate the recorded
                        // anchor BEFORE the backfill — a still-hitting
                        // pattern pins the candidate to the recorded line;
                        // a broken anchor needs the pattern to hit the
                        // candidate's line. Anything else means the
                        // title-derived ident and the line/pattern anchor
                        // disagree (title names `beta`, pattern/line pin
                        // `alpha`) — skip with the candidate listed, never
                        // backfill.
                        let old_hits = pattern_at(&rx, &lines, ln);
                        let consistent = if old_hits {
                            node_line == ln
                        } else {
                            pattern_at(&rx, &lines, node_line)
                        };
                        if !consistent {
                            skipped += 1;
                            stdout.push_str(&format!(
                                "{label}跳過（bootstrap 候選與既有錨不符：{symbol} :{node_line}）\n"
                            ));
                            continue;
                        }
                        // M5: a non-object x-codeReality cannot take the
                        // symbol without clobbering author content
                        if step
                            .get("x-codeReality")
                            .is_some_and(|v| !v.is_object())
                        {
                            skipped += 1;
                            stdout.push_str(&format!(
                                "{label}跳過（x-codeReality 非 object——回填會覆寫既有值）\n"
                            ));
                            continue;
                        }
                        bootstrap_filled += 1;
                        // Line moves ONLY in H2's broken-anchor arm — the
                        // consistency check already proved there that the
                        // pattern hits the candidate's line; a
                        // still-hitting pattern keeps its line (decision 5).
                        let line_moves = !old_hits && node_line != ln;
                        let note = if line_moves {
                            format!(" L{ln}->L{node_line}")
                        } else {
                            String::new()
                        };
                        stdout.push_str(&format!(
                            "{label}補 symbol {symbol}（bootstrap）{note}\n"
                        ));
                        if apply {
                            let obj = step.as_object_mut().unwrap();
                            match obj.get("x-codeReality").cloned() {
                                Some(serde_json::Value::Object(map)) => {
                                    // M5: merge — sibling keys survive
                                    let mut m = map;
                                    m.insert(
                                        "symbol".into(),
                                        serde_json::Value::String(symbol.clone()),
                                    );
                                    obj.insert(
                                        "x-codeReality".into(),
                                        serde_json::Value::Object(m),
                                    );
                                }
                                _ => {
                                    obj.insert(
                                        "x-codeReality".into(),
                                        serde_json::json!({ "symbol": symbol }),
                                    );
                                }
                            }
                            tour_changed = true;
                            if line_moves {
                                obj.insert("line".into(), serde_json::json!(node_line));
                            }
                        }
                    }
                    Ok(FillOutcome::Filled(_, None)) => {
                        skipped += 1;
                        stdout.push_str(&format!("{label}跳過（唯一候選無 symbol）\n"));
                    }
                    Ok(FillOutcome::NoAnchor(reason, cands)) if reason == "ambiguous" => {
                        ambiguous += 1;
                        stdout.push_str(&format!(
                            "{label}歧義（{} 候選）：{}\n",
                            cands.len(),
                            cand_hint(&cands)
                        ));
                    }
                    Ok(FillOutcome::NoAnchor(reason, _)) => {
                        skipped += 1;
                        stdout.push_str(&format!("{label}跳過（{reason}）\n"));
                    }
                }
                continue;
            }
            skipped += 1;
            stdout.push_str(&format!("{label}跳過（無 symbol；未開 --bootstrap-symbols）\n"));
        }
        if apply && tour_changed {
            let path = repo.join(rel);
            let body = crate::common::to_json_indent1(tour);
            if let Err(e) = std::fs::write(&path, format!("{body}\n")) {
                return ToolOutput::crash(format!("{} 寫入失敗：{}", path.display(), e));
            }
            written += 1;
        }
    }
    let mode = if apply { "APPLY" } else { "DRY-RUN" };
    let written_note = if apply {
        format!(" | written {written}")
    } else {
        String::new()
    };
    stdout.push_str(&format!(
        "[OK] tour_reanchor {mode}: {} tours | corrected {corrected} unverified {unverified} bootstrap {bootstrap_filled} ambiguous {ambiguous} skipped {skipped} exact {exact}{written_note}\n",
        tours.len()
    ));
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}
