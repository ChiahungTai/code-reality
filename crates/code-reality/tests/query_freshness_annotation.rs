//! CR-10 S4 — per-query freshness annotation + negative gate + the MCP
//! freshness tool (EP `ai-analysis/execution-plans/2026-10-14-cr-delta-freshness`,
//! frozen TC rows TC-3b/4/5/6/8/9/14/15/20/21; TC-10/13 live on the MCP
//! seam — `mcp_freshness_tool.rs` + the in-src cap regression mod).
//!
//! Contract under test (D6/D7/D8/D9/D11/D13/D15):
//! - every successful scip_refs query (refs/callers/closure, the two
//!   zero-row forms included) carries the `[CR_VERDICT] v=1 …` line
//!   directly beneath the `[SRC]` line (D8);
//! - the hard negative gate (D7): stale + callers/closure EMPTY flips the
//!   exit-0 success form to a typed refusal — exit 1 with
//!   `negative_verdict=unavailable`; fresh + empty stays the authoritative
//!   zero (`exhaustive=true`, family exit unchanged);
//! - graph_query grows the `[CR-META]` header line on every op (D9),
//!   gated ops (search/impact_radius/affected_flows) refuse stale-empty
//!   with a null result, and no `[CR-META]` line must never be read as
//!   fresh (gq_fail paths);
//! - annotations derive ONLY from the actually-serving artifact (D11):
//!   after a rebuild the annotation describes the new generation, and a
//!   cost-gate overrun degrades to `indeterminate` + `budget-exceeded`
//!   (D15) instead of guessing.
//!
//! The tests drive only public faces (cli::run / graph_engine::run +
//! the filesystem), so this file compiles against the pre-S4 binary and
//! fails there (RED). Stale slots are reached through explicit
//! `--index <slot> --repo <repo>` (explicit indexes are never healed),
//! which keeps these tests free of process-global env mutation.

mod support;

use code_reality::cli::run;
use code_reality::engine::{default_index_path, meta_path};
use code_reality::graph_engine;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;

const MANIFEST_REL: &str = ".code-reality/scip/index.scip.manifest.json";

// ---------- labs ----------

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

/// Python-face lab: one module file, fn names fn1/fn2/fn3 from the
/// deriving fake (DEF-only index — callers/closure are structurally
/// empty, the TC-4/TC-6 zero forms).
fn lab_py() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("mod_a.py"), "def fn1():\n    pass\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

/// Rust-face lab matching the shared fixture docs (crates/a.rs +
/// crates/b.rs) — the slot-swap lab below re-seats the slot onto
/// `rich_callers.scip`, which carries real caller edges (TC-5's
/// non-empty baseline).
fn lab_cargo() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(repo.join("Cargo.lock"), "version = 3\n").unwrap();
    std::fs::create_dir_all(repo.join("crates")).unwrap();
    std::fs::write(repo.join("crates/a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo.join("crates/b.rs"), "pub fn b() {}\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    support::install_rust_fixture_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

impl Lab {
    fn slot(&self) -> PathBuf {
        default_index_path(&self.repo).unwrap()
    }
    fn build(&self) {
        code_reality::build::build_repo(&self.repo, None, &self.roots).expect("build");
    }
    fn write(&self, rel: &str, content: &str) {
        let p = self.repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    /// scip_refs via the lib CLI face. Explicit `--index` + `--repo`:
    /// the explicit slot is never healed, so a dirty source tree stays
    /// stale for the query (no env lever, no cross-test interference).
    fn scip(&self, args: &[&str]) -> code_reality::ToolOutput {
        let mut argv = vec!["scip_refs".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        argv.push("--repo".to_string());
        argv.push(self.repo.display().to_string());
        argv.push("--index".to_string());
        argv.push(self.slot().display().to_string());
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run(&refs)
    }
    fn gq(&self, args: &[&str]) -> code_reality::ToolOutput {
        let mut argv = vec!["graph_query".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        argv.push("--repo".to_string());
        argv.push(self.repo.display().to_string());
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        graph_engine::run(&refs)
    }
}

// ---------- line parsing helpers ----------

/// The `[CR_VERDICT]` line: the first output line starting with the tag.
fn cr_verdict_line(stdout: &str) -> Option<&str> {
    stdout.lines().find(|l| l.starts_with("[CR_VERDICT] "))
}

/// key=value map of the verdict line (the leading `[CR_VERDICT] ` tag
/// stripped; `v=1` included). CR-10.3 R7 quoting rule: a value
/// containing whitespace or a comma is double-quoted with inner `"`
/// escaped as `\"` — the splitter honors quotes when dividing on
/// spaces; token values keep their verbatim spelling (quotes included)
/// so assertions can pin the exact wire form.
fn verdict_fields(line: &str) -> std::collections::BTreeMap<String, String> {
    let body = line.trim_start_matches("[CR_VERDICT] ");
    let mut out = std::collections::BTreeMap::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for c in body.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else {
            match c {
                '\\' if in_quotes => {
                    cur.push('\\');
                    escaped = true;
                }
                '"' => in_quotes = !in_quotes,
                ' ' if !in_quotes => {
                    if !cur.is_empty() {
                        tokens.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(c),
            }
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    for tok in tokens {
        if let Some((k, v)) = tok.split_once('=') {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

/// Everything after the `[OK]` and `[CR-META]` header lines — the
/// pretty-printed JSON body as one string.
fn gq_body(stdout: &str) -> &str {
    stdout.splitn(3, '\n').nth(2).unwrap_or("")
}

/// The `[CR-META]` line: must be output line 2 (D9 header system).
fn cr_meta_json(stdout: &str) -> Option<Value> {
    let line = stdout.lines().nth(1)?;
    let payload = line.strip_prefix("[CR-META] ")?;
    serde_json::from_str(payload).ok()
}

/// Re-seat the slot onto the caller-edges fixture and re-bind the
/// stamped manifest/meta pointers to the new slot bytes — the binding
/// audit (legs ①③④) stays valid while the served generation now carries
/// refs (identity is source-side and untouched, so the lab answers FRESH
/// over the swapped slot; a later source edit then drifts it stale with
/// a valid delta — the TC-5/TC-15 baseline).
fn reseat_slot_with_callers_fixture(lab: &Lab) {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rich_callers.scip"
    );
    std::fs::copy(fixture, lab.slot()).unwrap();
    let slot_hash = code_reality::identity::sha256_hex(&std::fs::read(lab.slot()).unwrap());
    let manifest_path = lab.repo.join(MANIFEST_REL);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["index_sha256"] = serde_json::json!(slot_hash);
    let mtext = format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("manifest serializes")
    );
    let digest = code_reality::identity::sha256_hex(mtext.as_bytes());
    std::fs::write(&manifest_path, &mtext).unwrap();
    let meta_path = meta_path(&lab.slot());
    let mut meta: Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!(slot_hash);
    meta["base_manifest_digest"] = serde_json::json!(digest);
    std::fs::write(
        &meta_path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
}

/// Reduce the slot's meta to the lsp_harvest face (D16 shape: `{head,
/// producer}` only) — no identity triple, no manifest pointer → the
/// stamp binds (nothing to compare) but the delta face is `None`
/// (legacy-no-manifest) while staleness stays observable (source mtime).
fn strip_meta_to_legacy(lab: &Lab) {
    let mp = meta_path(&lab.slot());
    let meta = serde_json::json!({"head": "", "producer": "lsp_harvest 0.0.0"});
    std::fs::write(
        &mp,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
}

// ---------- TC-3b: untracked-file symbol query = no-coverage ----------

#[test]
fn tc3b_untracked_file_symbol_query_is_no_coverage() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // a governed-face file the indexed generation never saw (untracked)
    lab.write("late_mod.py", "def late_modfn1():\n    pass\n");
    let out = lab.scip(&["late_modfn1"]);
    assert_eq!(
        out.exit_code, 1,
        "no-DEF family exit stays 1: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("v").map(String::as_str), Some("1"));
    assert_eq!(f.get("tier").map(String::as_str), Some("no-coverage"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("false"));
    assert_eq!(f.get("negative_verdict").map(String::as_str), Some("null"));
    // the delta summary keeps the SM-3 dual label observable: the file IS
    // in the added set while the direct query answers no-coverage
    assert!(
        line.contains("late_mod.py"),
        "delta_paths carries the added file (SM-3): {line}"
    );
    assert!(
        out.stdout.contains("[WARN] 查無 DEF：late_modfn1"),
        "{}",
        out.stdout
    );
}

// ---------- TC-4: stale inbound empty = typed refusal (per-face) ----------

#[test]
fn tc4_stale_callers_empty_is_typed_refusal_exit1() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let out = lab.scip(&["mod_afn1", "--callers"]);
    assert_eq!(out.exit_code, 1, "D7 hard gate: {}", out.stdout);
    // CR-10.3 R12: the refusal arm's body line wears the typed refusal
    // label, never the [OK] success tag (exit-1 semantics consistency)
    assert!(out.stdout.contains("[REFUSED]"), "{}", out.stdout);
    assert!(!out.stdout.contains("[OK]"), "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(
        f.get("negative_verdict").map(String::as_str),
        Some("unavailable")
    );
    assert_eq!(f.get("tier").map(String::as_str), Some("possibly-stale"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("false"));
}

#[test]
fn tc4_stale_closure_empty_frontier_is_typed_refusal_exit1() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let out = lab.scip(&["mod_afn1", "--closure", "--depth", "2"]);
    assert_eq!(out.exit_code, 1, "D7 hard gate: {}", out.stdout);
    // CR-10.3 R12: refusal label on the closure arm too
    assert!(out.stdout.contains("[REFUSED]"), "{}", out.stdout);
    assert!(!out.stdout.contains("[OK]"), "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(
        f.get("negative_verdict").map(String::as_str),
        Some("unavailable")
    );
}

// ---------- CR-13: graph-only torn plane no longer refuses the scip face

/// Torn the data plane WITHOUT touching the sources: rewrite the slot
/// bytes in place (same content — the binding audit and the stamped
/// identity stay valid) so the slot mtime passes graph.db's — engine
/// `graph_lags`, the torn-plane shape (tc13 recipe, source_identity.rs).
fn bump_slot_mtime_past_graph(lab: &Lab) {
    let bytes = std::fs::read(lab.slot()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(lab.slot(), &bytes).unwrap();
}

#[test]
fn cr13_graph_only_torn_plane_scip_serves_exact_explicit_index() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    bump_slot_mtime_past_graph(&lab);
    // explicit --index is never healed: the serving slot IS the torn
    // one — a scip query must answer exact (the scip face does not
    // consume graph.db), never a D7 refusal over a graph-only lag
    let out = lab.scip(&["mod_afn1", "--callers"]);
    assert_eq!(
        out.exit_code, 0,
        "graph-only lag is not scip-face stale: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("exact"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("true"));
    assert_eq!(f.get("negative_verdict").map(String::as_str), Some("null"));
    assert!(out.stdout.contains("0 callers"), "{}", out.stdout);
    // the closure format rides the same per-face base
    let out = lab.scip(&["mod_afn1", "--closure", "--depth", "2"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    assert!(line.contains("tier=exact"), "{line}");
    // contrast arm: the same torn plane with a DRIFTED source still
    // refuses — the split moved graph_lags only, never the source base
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let out = lab.scip(&["mod_afn1", "--callers"]);
    assert_eq!(
        out.exit_code, 1,
        "source-stale keeps the D7 gate: {}",
        out.stdout
    );
    assert!(out.stdout.contains("[REFUSED]"), "{}", out.stdout);
}

#[test]
fn cr13_graph_only_torn_plane_autoheal_off_serves_exact() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    bump_slot_mtime_past_graph(&lab);
    // default-slot face with the heal escaped (AUTOHEAL=off): the scip
    // query serves exact over the torn plane — the heal obligation
    // stays with `freshness`/`ensure_fresh`, not the scip answer
    // SAFETY: single-threaded w.r.t. other env readers via ENV_LOCK
    std::env::set_var("CODE_REALITY_AUTOHEAL", "off");
    let out = run(&[
        "scip_refs",
        "mod_afn1",
        "--callers",
        "--repo",
        lab.repo.to_str().unwrap(),
    ]);
    std::env::remove_var("CODE_REALITY_AUTOHEAL");
    assert_eq!(
        out.exit_code, 0,
        "no refusal under AUTOHEAL=off on a graph-only lag: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    assert!(line.contains("tier=exact"), "{line}");
    assert!(out.stdout.contains("0 callers"), "{}", out.stdout);
}

// ---------- TC-5 / TC-15: stale inbound with a baseline hit ----------

#[test]
fn tc5_stale_callers_hit_is_success_non_exhaustive() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_cargo();
    lab.build();
    reseat_slot_with_callers_fixture(&lab);
    // content drift (identity is content-addressed — mtime alone lies)
    lab.write("crates/a.rs", "pub fn a_changed() {}\n");
    let out = lab.scip(&["EventStoreLifecycle.open", "--callers"]);
    assert_eq!(
        out.exit_code, 0,
        "positive evidence still serves: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("possibly-stale"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("false"));
    assert_eq!(f.get("negative_verdict").map(String::as_str), Some("null"));
    assert!(out.stdout.contains("8 callers"), "{}", out.stdout);
}

#[test]
fn tc15_stale_defs_query_delta_paths_observe_intersection() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_cargo();
    lab.build();
    reseat_slot_with_callers_fixture(&lab);
    // anchor file (crates/b.rs — the caller sites) unchanged; the drift
    // lives in crates/a.rs → the delta summary names a.rs, not b.rs
    lab.write("crates/a.rs", "pub fn a_changed() {}\n");
    let out = lab.scip(&["EventStoreLifecycle.open"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("possibly-stale"));
    assert!(
        line.contains("crates/a.rs"),
        "delta summary names the drifted file: {line}"
    );
    // SM-6: the intersection judgement is observable from the path list —
    // the queried symbol's defining file must NOT be claimed as drifted
    let paths = f.get("delta_paths").cloned().unwrap_or_default();
    assert!(
        !paths.contains("crates/b.rs"),
        "unchanged anchor stays out of the delta list: {paths}"
    );

    // now drift the anchor itself → the list must carry it
    lab.write("crates/b.rs", "pub fn b_changed() {}\n");
    let out = lab.scip(&["EventStoreLifecycle.open"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    assert!(
        line.contains("crates/b.rs"),
        "delta summary carries the drifted anchor: {line}"
    );
}

// ---------- TC-6: fresh inbound empty = authoritative zero ----------

#[test]
fn tc6_fresh_callers_empty_is_authoritative_zero() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let out = lab.scip(&["mod_afn1", "--callers"]);
    assert_eq!(
        out.exit_code, 0,
        "0-callers family exit stays 0: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("exact"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("true"));
    assert_eq!(f.get("negative_verdict").map(String::as_str), Some("null"));
    assert!(out.stdout.contains("0 callers"), "{}", out.stdout);
}

// ---------- TC-8: annotation follows the actually-serving generation ----------

#[test]
fn tc8_annotation_follows_served_generation_across_rebuild() {
    // CR-10.3 R5: both queries transitively read CODE_REALITY_VERDICT_BUDGET_MS
    // (cli::run → serving_stamp) — join the ENV_LOCK convention so the
    // tc14 env mutation can never race this file's readers
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // drift the source without rebuilding: the serving slot goes stale
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let stale = lab.scip(&["mod_afn1"]);
    let stale_line = cr_verdict_line(&stale.stdout).expect("annotated");
    assert!(stale_line.contains("tier=possibly-stale"), "{stale_line}");
    assert!(stale_line.contains("mod_a.py"), "{stale_line}");
    // rebuild: the NEW generation serves fresh — the same query must now
    // annotate the rebuilt body (clean delta), never the pinned-old
    // optimistic stale values
    code_reality::build::build_repo(&lab.repo, None, &lab.roots).expect("rebuild");
    let after = lab.scip(&["mod_afn1"]);
    let after_line = cr_verdict_line(&after.stdout).expect("annotated");
    assert!(
        after_line.contains("tier=exact"),
        "annotation describes the rebuilt serving body: {after_line}"
    );
    assert!(
        !after_line.contains("delta_paths="),
        "the healed generation carries no drift: {after_line}"
    );
    assert!(after_line.contains("delta_changed=0"), "{after_line}");
}

// ---------- TC-14: the per-query cost gate (D15 budget-exceeded) ----------

/// Serializes every env-mutating test in this binary (process-global env
/// races under cargo's parallel runner; house precedent: the HTTP e2e
/// mutex in s6_mcp_server.rs).
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn tc14_budget_gate_degrades_annotation_to_indeterminate() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // SAFETY: single-threaded w.r.t. other env readers via ENV_LOCK
    std::env::set_var("CODE_REALITY_VERDICT_BUDGET_MS", "0");
    let out = lab.scip(&["mod_afn1"]);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("indeterminate"));
    assert_eq!(
        f.get("delta_unavailable").map(String::as_str),
        Some("budget-exceeded")
    );
    let gq = lab.gq(&["hub", "--limit", "3"]);
    let meta = cr_meta_json(&gq.stdout).expect("[CR-META] header");
    assert_eq!(meta["tier"], "indeterminate", "{}", gq.stdout);
    assert_eq!(
        meta["delta_summary"]["delta_unavailable"], "budget-exceeded",
        "{}",
        gq.stdout
    );
    std::env::remove_var("CODE_REALITY_VERDICT_BUDGET_MS");
}

// ---------- TC-20: serve-stale annotation matches the served body ----------

#[test]
fn tc20_served_stale_slot_annotates_stale_not_optimistic_fresh() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    // the explicit --index path never heals: the stale slot IS the
    // serving body, and the annotation must say so (no optimistic fresh)
    let out = lab.scip(&["mod_afn1"]);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(f.get("tier").map(String::as_str), Some("possibly-stale"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("false"));
    // the drift is observable (SM-6)
    assert!(line.contains("mod_a.py"), "{line}");
}

// ---------- TC-9: the [CR-META] header covers every graph op ----------

#[test]
fn tc9_graph_query_all_ops_carry_cr_meta_header() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // document_symbols reads the SCIP cache — build it for the bypass leg
    let cache = run(&[
        "scip_refs",
        "--build-cache",
        "--repo",
        lab.repo.to_str().unwrap(),
    ]);
    assert_eq!(cache.exit_code, 0, "{}", cache.stderr);
    let cases: Vec<Vec<&str>> = vec![
        vec!["hub"],
        vec!["bridge"],
        vec!["communities"],
        vec!["arch_overview"],
        vec!["flows"],
        vec!["impact_radius", "--files", "mod_a.py"],
        vec!["detect_changes", "--files", "mod_a.py"],
        vec!["affected_flows", "--files", "mod_a.py"],
        vec!["review_context", "--files", "mod_a.py"],
        vec!["minimal_context", "--task", "t"],
        vec!["search", "--query", "fn1"],
        vec!["symbols", "--query", "mod_a.py"],
    ];
    for args in cases {
        let out = lab.gq(&args);
        assert_eq!(out.exit_code, 0, "op {:?}: {}", args, out.stderr);
        let meta = cr_meta_json(&out.stdout).unwrap_or_else(|| {
            panic!(
                "op {:?} lacks a parseable [CR-META] line 2: {}",
                args, out.stdout
            )
        });
        assert_eq!(meta["v"], 1, "op {:?}: {}", args, meta);
        assert!(
            meta.get("tier").is_some(),
            "op {:?}: tier key present: {}",
            args,
            meta
        );
        assert!(
            meta.get("graph_snapshot_id").is_some(),
            "op {:?}: graph_snapshot_id key present: {}",
            args,
            meta
        );
        assert!(
            meta.get("delta_summary").is_some(),
            "op {:?}: delta_summary always present (D15): {}",
            args,
            meta
        );
    }
}

#[test]
fn tc9_gq_fail_carries_no_cr_meta_line() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let out = lab.gq(&["no_such_op"]);
    assert_ne!(out.exit_code, 0);
    assert!(
        !out.stdout.contains("[CR-META]"),
        "no [CR-META] line = unannotated, never infer fresh (D9): {}",
        out.stdout
    );
}

// ---------- TC-21: graph_query gated-op refusals (stale + empty) ----------

#[test]
fn tc21_stale_gated_ops_refuse_with_null_results() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    // search: stale + zero hits → negative_verdict=unavailable + null
    let out = lab.gq(&["search", "--query", "zzz_no_such_symbol"]);
    assert_eq!(
        out.exit_code, 0,
        "graph_query refusal stays exit 0: {}",
        out.stdout
    );
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_eq!(meta["negative_verdict"], "unavailable", "{}", out.stdout);
    assert_eq!(meta["exhaustive"], false, "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert!(
        body["results"].is_null(),
        "results nulled, no readable []: {body}"
    );

    // impact_radius: stale + no impacted nodes → refusal
    let out = lab.gq(&["impact_radius", "--files", "mod_a.py", "--depth", "1"]);
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_eq!(meta["negative_verdict"], "unavailable", "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert!(body["impacted_nodes"].is_null(), "{body}");

    // affected_flows: stale + empty array → whole result null
    let out = lab.gq(&["affected_flows", "--files", "mod_a.py"]);
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_eq!(meta["negative_verdict"], "unavailable", "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert!(body.is_null(), "no readable []: {body}");
}

#[test]
fn tc21_annotate_only_ops_stay_arrays_on_stale() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    // hub is NOT in the D7 gate set — annotate-only, exempt explicitly
    let out = lab.gq(&["hub", "--limit", "3"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_ne!(meta["negative_verdict"], "unavailable", "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert!(
        body.is_array(),
        "annotate-only keeps the readable []: {body}"
    );
}

#[test]
fn tc21_fresh_gated_empty_is_authoritative_zero() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let out = lab.gq(&["search", "--query", "zzz_no_such_symbol"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_ne!(meta["negative_verdict"], "unavailable", "{}", out.stdout);
    assert_eq!(meta["exhaustive"], true, "{}", out.stdout);
    assert_eq!(meta["tier"], "exact", "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
}

// ---------- CR-10.3 R3: the gate is staleness-truth-bound, not delta-bound

#[test]
fn r3_deltaless_stale_keeps_the_negative_gate_scip() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // legacy-shaped meta (D16 lsp_harvest face): the delta face answers
    // None (legacy-no-manifest) while needs_rebuild() stays observable —
    // the gate must fire on the staleness truth alone (D7 hard gate,
    // CR-10.3 R3); the tier stays indeterminate (no possibly-stale
    // summary to carry) but the refusal face (exit 1 + unavailable)
    // still fires
    strip_meta_to_legacy(&lab);
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let out = lab.scip(&["mod_afn1", "--callers"]);
    assert_eq!(
        out.exit_code, 1,
        "delta-less stale must still refuse the authoritative empty: {}",
        out.stdout
    );
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(
        f.get("negative_verdict").map(String::as_str),
        Some("unavailable"),
        "{line}"
    );
    assert_eq!(f.get("tier").map(String::as_str), Some("indeterminate"));
    assert_eq!(f.get("exhaustive").map(String::as_str), Some("false"));
    assert!(out.stdout.contains("[REFUSED]"), "{}", out.stdout);
}

#[test]
fn r3_deltaless_stale_keeps_the_negative_gate_graph() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    strip_meta_to_legacy(&lab);
    lab.write("mod_a.py", "def fn1():\n    pass_changed\n");
    let out = lab.gq(&["search", "--query", "zzz_no_such_symbol"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let meta = cr_meta_json(&out.stdout).expect("[CR-META] header");
    assert_eq!(
        meta["negative_verdict"], "unavailable",
        "graph gate also delta-independent: {}",
        out.stdout
    );
    assert_eq!(meta["tier"], "indeterminate", "{}", out.stdout);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert!(body["results"].is_null(), "no readable []: {body}");
}

// ---------- CR-10.3 R6: D13 count-pairing on the verdict line ----------

#[test]
fn r6_delta_paths_truncation_carries_the_complete_total() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    // 60 governed files in the indexed generation…
    for i in 0..60 {
        lab.write(
            &format!("mod_{i:02}.py"),
            &format!("def fn{i:02}():\n    pass\n"),
        );
    }
    lab.build();
    // …then drift 55 of them: the delta union (55) exceeds the 50-entry
    // path cap — the complete count must survive (D13 bounded lists
    // carry complete counts; R6 count-pairing mirrors the S2 JSON face)
    for i in 0..55 {
        lab.write(
            &format!("mod_{i:02}.py"),
            &format!("def fn{i:02}():\n    pass_v2\n"),
        );
    }
    let out = lab.scip(&["mod_afn1"]);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let line = cr_verdict_line(&out.stdout).expect("[CR_VERDICT] line present");
    let f = verdict_fields(line);
    assert_eq!(
        f.get("delta_paths_total").map(String::as_str),
        Some("55"),
        "{line}"
    );
    assert_eq!(
        f.get("delta_paths_truncated").map(String::as_str),
        Some("true"),
        "{line}"
    );
    // exactly the capped 50 entries ride the line
    assert_eq!(line.matches("mod_").count(), 50, "{line}");
}

// ---------- CR-10.3 R14: live-source snippet labeling ----------

#[test]
fn r14_review_context_labels_live_source_snippets() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let out = lab.gq(&["review_context", "--files", "mod_a.py", "--include-source"]);
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    let body: Value = serde_json::from_str(gq_body(&out.stdout)).unwrap();
    assert_eq!(
        body["context"]["source_snippets_basis"], "current-tree",
        "review intent-F9: snippets read the live tree, label them: {body}"
    );
}
