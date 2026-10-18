//! CR-15 — tour symbol-anchor verification + reanchor:
//!
//! - T-A: `tour_validate` symbol-aware three-state (unique symbol +
//!   matching coords → exact; unique symbol + stale coords → corrected
//!   WARN with the symbol-derived address; ambiguous/miss → legacy
//!   line/pattern path unchanged);
//! - T-B: the freshness gate (no graph.db / non-`current-tree` → one
//!   degrade WARN, behavior byte-identical to the legacy face);
//! - T-C: `tour_reanchor` dry-run zero-writes / apply writes only the
//!   verified-safe corrections (manifest bytes untouched, unknown
//!   fields + key order preserved, pattern-miss proposals never
//!   written);
//! - T-D: bootstrap backfill (unique candidate fills; multi-candidate
//!   lists hints and skips; zero candidates skips);
//! - T-E: chain_tour behavior is guarded by its existing suites (the
//!   extraction to `symbol_resolve` is behavior-frozen).
//!
//! T-F — CR-15 patch legs (dual-review, all adopted): H1 symbol-first
//! relocation when the recorded file is gone (validate + reanchor);
//! H2 bootstrap candidate↔anchor consistency (title/pattern conflict
//! never backfills); H3 pattern dead at the unique new address is an
//! unverified FAIL, never corrected; M4 graph.db errors fail loud
//! (schema probe over symbol/file_path/line_start + `Result` locate);
//! M5 x-codeReality merge without clobbering siblings; M7 keyword
//! exclusion before ident ranking.
//!
//! T-R — CR-15 patch round-2 (both findings adopted): the relocation
//! target must actually READ — a unique symbol addressing a deleted
//! new file is an unverified FAIL (validate) / a zero-write skip with
//! no proposal (reanchor), never an empty-string `^`-pool corrected
//! WARN or an `--apply` relocation onto a nonexistent file; the
//! reanchor summary splits corrected (pattern-verified proposals)
//! from unverified (dead-pattern proposals).
//!
//! The gate runs the REAL freshness face: each lab builds a true slot
//! via `build_repo` + the deriving py fake (fresh, `current-tree`), then
//! replaces graph.db with a synthetic fixture (mtime newer than the slot
//! — not torn; graph.db carries no identity binding). Degraded cases
//! remove the graph / key the meta. H1's deleted-old-file labs delete
//! the source and RE-BUILD first — the slot identity must cover the
//! post-deletion tree or the gate would degrade and the symbol face
//! would never run.

mod graph_db_fixture;
mod support;

use code_reality::build::build_repo;
use code_reality::engine::{default_index_path, meta_path};
use code_reality::tour_reanchor;
use code_reality::tour_validate;
use graph_db_fixture::{make_graph_db, GraphDbSpec, NodeAttr, NodeSeed};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const SRC: &str = "\
import os


def alpha():
    return 1


def beta():
    return 2


class Gamma:
    pass


class A:

    def run(self):
        return 10


class B:

    def run(self):
        return 20
";

const OTHER: &str = "\
def beta():
    return 9
";

// SCIP-shaped symbols (descriptor region after the last ` / cut).
const SYM_ALPHA: &str = "pyrefly python proj 1.0 `m`/alpha().";
const SYM_BETA: &str = "pyrefly python proj 1.0 `m`/beta().";
const SYM_BETA_MOVED: &str = "pyrefly python proj 1.0 `m2`/beta().";
const SYM_GAMMA_TYPE: &str = "pyrefly python proj 1.0 `m`/Gamma#";
const SYM_GAMMA_CTOR: &str = "pyrefly python proj 1.0 `m`/Gamma().";
const SYM_RUN_A: &str = "pyrefly python proj 1.0 `m`/A#run().";
const SYM_RUN_B: &str = "pyrefly python proj 1.0 `m`/B#run().";
const SYM_GHOST: &str = "pyrefly python proj 1.0 `m`/ghost().";

const MANIFEST_TOML: &str = "version = 1\n\n[tour.\"10-x/01.tour\"]\ngenerator = \"chain_tour\"\nsources = [\"chain.md\"]\n";

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
}

fn abs(repo: &Path, rel: &str) -> String {
    repo.join(rel).display().to_string()
}

/// The synthetic graph the tour fixtures address. file_path values are
/// absolute against the CANONICALIZED repo root (the locator resolves
/// the repo root before comparing).
fn graph_spec(repo: &Path) -> GraphDbSpec {
    let py = abs(repo, "src_mod.py");
    let other = abs(repo, "other_mod.py");
    let seed = |name: &str, symbol: &str, file: &str| NodeSeed {
        name: name.into(),
        parent: None,
        qname: symbol.into(),
        file_path: file.into(),
    };
    let mut spec = GraphDbSpec {
        nodes: vec![
            seed("alpha", SYM_ALPHA, &py),
            seed("beta", SYM_BETA, &py),
            seed("beta", SYM_BETA_MOVED, &other),
            seed("Gamma", SYM_GAMMA_TYPE, &py),
            seed("Gamma", SYM_GAMMA_CTOR, &py),
            seed("run", SYM_RUN_A, &py),
            seed("run", SYM_RUN_B, &py),
        ],
        ..Default::default()
    };
    let attr = |kind: &'static str| NodeAttr {
        kind,
        language: "python",
        is_test: 0,
        community_id: None,
    };
    spec.node_attrs = vec![
        (SYM_ALPHA.into(), attr("Function")),
        (SYM_BETA.into(), attr("Function")),
        (SYM_BETA_MOVED.into(), attr("Function")),
        (SYM_GAMMA_TYPE.into(), attr("Type")),
        (SYM_GAMMA_CTOR.into(), attr("Function")),
        (SYM_RUN_A.into(), attr("Function")),
        (SYM_RUN_B.into(), attr("Function")),
    ];
    spec.node_lines = vec![
        (SYM_ALPHA.into(), 4),
        (SYM_BETA.into(), 8),
        (SYM_BETA_MOVED.into(), 1),
        (SYM_GAMMA_TYPE.into(), 12),
        (SYM_GAMMA_CTOR.into(), 12),
        (SYM_RUN_A.into(), 18),
        (SYM_RUN_B.into(), 24),
    ];
    spec
}

impl Lab {
    /// Replace the build's graph.db with the synthetic fixture (mtime
    /// newer than the slot — the torn-plane guard stays quiet; the slot
    /// identity is untouched, so the gate still answers current-tree).
    fn install_graph(&self, spec: &GraphDbSpec) {
        let db = self.repo.join(".code-reality/graph.db");
        std::fs::remove_file(&db).unwrap();
        make_graph_db(&db, spec).unwrap();
    }

    /// A minimal non-UNIQUE nodes table — the only way to pin the
    /// defensive same-symbol-ambiguous branch (the self-owned schema
    /// enforces symbol UNIQUE; production dbs can't carry it).
    fn install_dup_symbol_graph(&self, symbol: &str, lines: &[i64]) {
        let db = self.repo.join(".code-reality/graph.db");
        std::fs::remove_file(&db).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (symbol TEXT, file_path TEXT, line_start INTEGER);",
        )
        .unwrap();
        for l in lines {
            conn.execute(
                "INSERT INTO nodes (symbol, file_path, line_start) VALUES (?1, ?2, ?3)",
                (symbol, abs(&self.repo, "src_mod.py"), l),
            )
            .unwrap();
        }
    }

    /// Break the identity comparability: a keyless meta degrades the
    /// slot to the legacy face (`serves: legacy-signals`) without
    /// touching any source byte.
    fn key_meta(&self) {
        std::fs::write(meta_path(&default_index_path(&self.repo).unwrap()), "{}\n").unwrap();
    }
}

fn fresh_lab(tag: &str) -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(t.path()).unwrap();
    std::fs::write(repo.join("src_mod.py"), SRC).unwrap();
    std::fs::write(repo.join("other_mod.py"), OTHER).unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_py_deriving_fake(bindir.path());
    build_repo(&repo, None, &[bindir.path().to_path_buf()]).expect("build_repo");
    let lab = Lab {
        repo,
        _t: t,
        _bindir: bindir,
    };
    lab.install_graph(&graph_spec(&lab.repo));
    lab
}

fn write_tour(repo: &Path, rel: &str, steps: Value) -> PathBuf {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let tour = json!({
        "title": "測試場景 01",
        "description": "CR-15 fixture",
        "steps": steps,
    });
    std::fs::write(&p, format!("{}\n", code_reality::common::to_json_indent1(&tour))).unwrap();
    p
}

fn step(file: &str, line: i64, title: &str, pattern: &str) -> Value {
    json!({
        "file": file,
        "line": line,
        "title": title,
        "description": "步",
        "pattern": pattern,
    })
}

fn with_symbol(mut s: Value, symbol: &str) -> Value {
    s.as_object_mut()
        .unwrap()
        .insert("x-codeReality".into(), json!({ "symbol": symbol }));
    s
}

fn run_reanchor(repo: &Path, extra: &[&str]) -> code_reality::ToolOutput {
    let mut argv = vec![
        "tour_reanchor".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--tours-dir".to_string(),
        ".tours".to_string(),
    ];
    argv.extend(extra.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    tour_reanchor::run(&refs)
}

fn read_bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

fn written_tour(repo: &Path) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo.join(".tours/10-x/01.tour")).unwrap(),
    )
    .unwrap()
}

// ---------- T-A: validate symbol three-state (fresh gate) ----------

#[test]
fn ta1_unique_symbol_matching_coords_is_exact() {
    let lab = fresh_lab("cr15-ta1");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 4, "alpha()", "def alpha"), SYM_ALPHA)]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    assert!(out.stdout.contains("[OK] tour validate"), "{}", out.stdout);
    assert!(
        !out.stdout.contains("corrected"),
        "exact anchor must stay quiet: {}",
        out.stdout
    );
}

#[test]
fn ta2_unique_symbol_stale_line_corrected_warn_with_new_coords() {
    let lab = fresh_lab("cr15-ta2");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 5, "alpha()", "def alpha"), SYM_ALPHA)]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    // corrected is a WARN (player semantics), never a fail
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let want = "[WARN] .tours/10-x/01.tour 步1 錨 corrected（source: SCIP symbol）: src_mod.py L5->L4";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    assert!(!out.stdout.contains("[FAIL]"), "{}", out.stdout);
}

#[test]
fn ta3_unique_symbol_moved_file_corrected_warn_carries_file() {
    let lab = fresh_lab("cr15-ta3");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 8, "beta()", "def beta"), SYM_BETA_MOVED)]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let want = "錨 corrected（source: SCIP symbol）: src_mod.py -> other_mod.py L8->L1";
    assert!(out.stdout.contains(want), "{}", out.stdout);
}

#[test]
fn ta4_symbol_ambiguous_falls_back_to_legacy() {
    let lab = fresh_lab("cr15-ta4");
    lab.install_dup_symbol_graph("pyrefly python proj 1.0 `m`/dup().", &[4, 8]);
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 999, "dup()", "def alpha"),
            "pyrefly python proj 1.0 `m`/dup()."
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    // advisory ambiguous WARN, then the legacy nearest-hit face
    assert!(
        out.stdout.contains("symbol 歧義（2 列同名 symbol）"),
        "{}",
        out.stdout
    );
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L999->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
}

#[test]
fn ta5_symbol_miss_falls_back_to_legacy_no_ambiguous_warn() {
    let lab = fresh_lab("cr15-ta5");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 999, "ghost()", "def alpha"), SYM_GHOST)]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    assert!(!out.stdout.contains("symbol 歧義"), "{}", out.stdout);
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L999->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
    assert!(!out.stdout.contains("SCIP symbol"), "{}", out.stdout);
}

#[test]
fn ta6_old_file_deleted_unique_symbol_new_address_still_corrected() {
    let lab = fresh_lab("cr15-ta6");
    // H1: the recorded file is GONE while the symbol uniquely addresses
    // other_mod.py — a relocation corrected WARN, not an existence FAIL.
    // Delete + re-produce keeps the gate current-tree (the slot identity
    // must cover the post-deletion tree or the symbol face never runs).
    std::fs::remove_file(lab.repo.join("src_mod.py")).unwrap();
    build_repo(&lab.repo, None, &[lab._bindir.path().to_path_buf()]).expect("rebuild");
    lab.install_graph(&graph_spec(&lab.repo));
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 8, "beta()", "def beta"),
            SYM_BETA_MOVED
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    let want = "錨 corrected（source: SCIP symbol）: src_mod.py -> other_mod.py L8->L1";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    assert!(!out.stdout.contains("錨檔不存在"), "{}", out.stdout);
}

#[test]
fn ta7_symbol_unique_pattern_dead_at_new_address_is_unverified_fail() {
    let lab = fresh_lab("cr15-ta7");
    // H3: the symbol uniquely addresses other_mod.py L1, but the
    // recorded pattern (`return 2` — still hits the RECORDED src_mod.py
    // L9) is dead in the new file — an unverified FAIL, never a
    // corrected WARN
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 9, "beta()", "return 2"),
            SYM_BETA_MOVED
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 1, "{}", out.stdout);
    let want = "[FAIL] .tours/10-x/01.tour 步1 symbol 定址（source: SCIP symbol）: src_mod.py -> other_mod.py L9->L1；pattern 未命中新座標（unverified）";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    assert!(
        !out.stdout.contains("corrected（source: SCIP symbol）"),
        "{}",
        out.stdout
    );
}

// ---------- T-B: freshness gate degrades ----------

#[test]
fn tb1_missing_graph_db_degrades_with_one_warn_legacy_behavior() {
    let lab = fresh_lab("cr15-tb1");
    std::fs::remove_file(lab.repo.join(".code-reality/graph.db")).unwrap();
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 5, "alpha()", "def alpha")]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    assert!(
        out.stdout.contains("無 graph.db"),
        "degrade WARN must name the missing db: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("symbol 驗證退化 line/pattern"),
        "{}",
        out.stdout
    );
    // behavior identical to today: legacy corrected face, no symbol talk
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L5->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
    assert!(!out.stdout.contains("SCIP symbol"), "{}", out.stdout);
}

#[test]
fn tb2_non_current_tree_gate_warns_once_per_validate() {
    let lab = fresh_lab("cr15-tb2");
    lab.key_meta();
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            step("src_mod.py", 5, "alpha()", "def alpha"),
            step("src_mod.py", 8, "beta()", "def beta"),
        ]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 0, "{}", out.stdout);
    assert!(
        out.stdout.contains("graph stale（serves legacy-signals）"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("symbol 驗證退化 line/pattern"),
        "{}",
        out.stdout
    );
    // ONE corpus-level WARN despite two steps
    assert_eq!(
        out.stdout.matches("symbol 驗證退化 line/pattern").count(),
        1,
        "{}",
        out.stdout
    );
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L5->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
}

// ---------- T-C: reanchor writes ----------

#[test]
fn tc1_dry_run_zero_writes() {
    let lab = fresh_lab("cr15-tc1");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            with_symbol(step("src_mod.py", 5, "alpha()", "def alpha"), SYM_ALPHA),
            with_symbol(step("src_mod.py", 8, "beta()", "def beta"), SYM_BETA),
        ]),
    );
    let manifest = lab.repo.join(".tours/manifest.toml");
    std::fs::write(&manifest, MANIFEST_TOML).unwrap();
    let (tour_before, manifest_before) = (read_bytes(&tour), read_bytes(&manifest));
    let out = run_reanchor(&lab.repo, &[]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("DRY-RUN"), "{}", out.stdout);
    assert!(out.stdout.contains("L5->L4"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), tour_before, "dry-run must not write");
    assert_eq!(read_bytes(&manifest), manifest_before);
}

#[test]
fn tc2_apply_updates_line_and_moves_file_manifest_untouched() {
    let lab = fresh_lab("cr15-tc2");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            with_symbol(step("src_mod.py", 5, "alpha()", "def alpha"), SYM_ALPHA),
            with_symbol(step("src_mod.py", 8, "beta()", "def beta"), SYM_BETA_MOVED),
        ]),
    );
    // unknown step field must survive
    let mut v = serde_json::from_str::<Value>(
        &std::fs::read_to_string(&tour).unwrap(),
    )
    .unwrap();
    v["steps"][0]["note"] = json!("keepme");
    std::fs::write(&tour, format!("{}\n", code_reality::common::to_json_indent1(&v)))
        .unwrap();
    let manifest = lab.repo.join(".tours/manifest.toml");
    std::fs::write(&manifest, MANIFEST_TOML).unwrap();
    let manifest_before = read_bytes(&manifest);

    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("APPLY"), "{}", out.stdout);
    assert!(out.stdout.contains("written 1"), "{}", out.stdout);
    // manifest bytes identical — the reanchor face never touches it
    assert_eq!(read_bytes(&manifest), manifest_before);

    let after = written_tour(&lab.repo);
    assert_eq!(after["steps"][0]["line"], json!(4));
    assert_eq!(after["steps"][0]["note"], json!("keepme"));
    assert_eq!(after["steps"][1]["file"], json!("other_mod.py"));
    assert_eq!(after["steps"][1]["line"], json!(1));
    // symbol identity untouched on the symbol path
    assert_eq!(
        after["steps"][0]["x-codeReality"]["symbol"],
        json!(SYM_ALPHA)
    );
    // key order preserved: "file" directly precedes "line" in both steps
    let raw = std::fs::read_to_string(&tour).unwrap();
    assert!(
        raw.contains("\"file\": \"src_mod.py\",\n   \"line\": 4,"),
        "key order/face drift: {}",
        raw
    );
    assert!(
        raw.contains("\"file\": \"other_mod.py\",\n   \"line\": 1,"),
        "{}",
        raw
    );
}

#[test]
fn tc3_apply_never_writes_pattern_miss_proposals() {
    let lab = fresh_lab("cr15-tc3");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 5, "alpha()", "return 999"), SYM_ALPHA)]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("pattern 未命中新行"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before, "unverified proposal must not write");
}

#[test]
fn tc4_degraded_gate_skips_every_step_and_writes_nothing() {
    let lab = fresh_lab("cr15-tc4");
    // content drift → the slot answers committed-baseline → gate off
    let mut src = SRC.to_string();
    src.push_str("\n# drift\n");
    std::fs::write(lab.repo.join("src_mod.py"), src).unwrap();
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(step("src_mod.py", 5, "alpha()", "def alpha"), SYM_ALPHA)]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("symbol 驗證退化 line/pattern"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("跳過（graph 退化"), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn tc5_old_file_deleted_unique_symbol_proposes_and_applies_relocation() {
    let lab = fresh_lab("cr15-tc5");
    // H1 reanchor face: the recorded file is gone, the symbol uniquely
    // addresses other_mod.py — the relocation is proposed (dry-run) and
    // applied (--apply; the pattern hits the new address so it is
    // verified-safe). Same delete + re-produce leg as ta6.
    std::fs::remove_file(lab.repo.join("src_mod.py")).unwrap();
    build_repo(&lab.repo, None, &[lab._bindir.path().to_path_buf()]).expect("rebuild");
    lab.install_graph(&graph_spec(&lab.repo));
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 8, "beta()", "def beta"),
            SYM_BETA_MOVED
        )]),
    );
    let out = run_reanchor(&lab.repo, &[]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    let want = "src_mod.py -> other_mod.py L8->L1（依據 SCIP symbol）";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    assert!(!out.stdout.contains("錨檔不存在"), "{}", out.stdout);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("written 1"), "{}", out.stdout);
    let after = written_tour(&lab.repo);
    assert_eq!(after["steps"][0]["file"], json!("other_mod.py"));
    assert_eq!(after["steps"][0]["line"], json!(1));
}

// ---------- T-D: bootstrap backfill ----------

#[test]
fn td1_bootstrap_unique_fills_symbol_keeps_hitting_line() {
    let lab = fresh_lab("cr15-td1");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 12, "Gamma", "class Gamma")]),
    );
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    // R1: Type + pseudo-ctor pair → the TYPE symbol, line stays 12
    // (the pattern still hits the recorded line)
    assert!(out.stdout.contains("補 symbol"), "{}", out.stdout);
    assert!(out.stdout.contains(SYM_GAMMA_TYPE), "{}", out.stdout);
    assert!(out.stdout.contains("written 1"), "{}", out.stdout);
    let after = written_tour(&lab.repo);
    assert_eq!(
        after["steps"][0]["x-codeReality"]["symbol"],
        json!(SYM_GAMMA_TYPE)
    );
    assert_eq!(after["steps"][0]["line"], json!(12));
}

#[test]
fn td2_bootstrap_aligns_line_only_when_recorded_anchor_broken() {
    let lab = fresh_lab("cr15-td2");
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 2, "alpha", "def alpha")]),
    );
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("L2->L4"), "{}", out.stdout);
    let after = written_tour(&lab.repo);
    assert_eq!(after["steps"][0]["line"], json!(4));
    assert_eq!(
        after["steps"][0]["x-codeReality"]["symbol"],
        json!(SYM_ALPHA)
    );
}

#[test]
fn td3_bootstrap_ambiguous_lists_candidates_and_skips() {
    let lab = fresh_lab("cr15-td3");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 18, "run", "def run")]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("歧義（2 候選）"), "{}", out.stdout);
    assert!(out.stdout.contains("A#run()."), "{}", out.stdout);
    assert!(out.stdout.contains("B#run()."), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn td4_bootstrap_zero_candidates_skips() {
    let lab = fresh_lab("cr15-td4");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 4, "ghost", "def alpha")]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("跳過（not-in-graph）"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn td5_no_symbol_without_bootstrap_flag_skips() {
    let lab = fresh_lab("cr15-td5");
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 5, "alpha()", "def alpha")]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("跳過（無 symbol；未開 --bootstrap-symbols）"),
        "{}",
        out.stdout
    );
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn td6_bootstrap_merges_into_existing_xcoder_reality_object() {
    let lab = fresh_lab("cr15-td6");
    // M5: an existing x-codeReality OBJECT takes the symbol as a merge —
    // sibling keys survive the backfill
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([{
            "file": "src_mod.py",
            "line": 12,
            "title": "Gamma",
            "description": "步",
            "pattern": "class Gamma",
            "x-codeReality": { "source": "manual" },
        }]),
    );
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("補 symbol"), "{}", out.stdout);
    let after = written_tour(&lab.repo);
    let x = &after["steps"][0]["x-codeReality"];
    assert_eq!(
        x["source"],
        json!("manual"),
        "sibling key must survive: {}",
        out.stdout
    );
    assert_eq!(x["symbol"], json!(SYM_GAMMA_TYPE));
}

#[test]
fn td7_bootstrap_non_object_xcoder_reality_skips_with_note() {
    let lab = fresh_lab("cr15-td7");
    // M5: a non-object x-codeReality cannot take the symbol without
    // clobbering author content — the step skips with a note, zero
    // writes
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([{
            "file": "src_mod.py",
            "line": 12,
            "title": "Gamma",
            "description": "步",
            "pattern": "class Gamma",
            "x-codeReality": "manual",
        }]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("跳過（x-codeReality 非 object"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("補 symbol"), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn td8_bootstrap_ident_keyword_never_wins_next_token_taken() {
    let lab = fresh_lab("cr15-td8");
    // M7: the title is empty; the ident must come from the pattern with
    // keywords excluded BEFORE ranking. `^def run\(` (marshal-specified)
    // and the paren-less `def run` (where `def` would win the stable
    // sort unfiltered) must both take `run` — observable via the
    // same-file candidates (A#run/B#run) the query then reports.
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            {
                "file": "src_mod.py",
                "line": 18,
                "title": "",
                "description": "步",
                "pattern": "^def run\\(",
            },
            {
                "file": "src_mod.py",
                "line": 18,
                "title": "",
                "description": "步",
                "pattern": "def run",
            },
        ]),
    );
    let out = run_reanchor(&lab.repo, &["--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout.matches("A#run().").count(),
        2,
        "both steps must resolve ident=run: {}",
        out.stdout
    );
    assert!(!out.stdout.contains("not-in-graph"), "{}", out.stdout);
    assert!(!out.stdout.contains("無法抽 ident"), "{}", out.stdout);
}

#[test]
fn td9_bootstrap_candidate_anchor_conflict_skips_with_candidate() {
    let lab = fresh_lab("cr15-td9");
    // H2 negative (marshal): the title names beta while the pattern/line
    // anchor pins alpha — the beta candidate (L8) must NOT be backfilled
    // into an alpha-anchored step. Second step: a broken anchor whose
    // pattern hits neither its recorded line nor the candidate's line
    // skips too.
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            step("src_mod.py", 4, "beta", "def alpha"),
            step("src_mod.py", 2, "alpha", "class Gamma"),
        ]),
    );
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("跳過（bootstrap 候選與既有錨不符"),
        "{}",
        out.stdout
    );
    // the conflicting candidate is listed with its line
    assert!(out.stdout.contains("beta(). :8"), "{}", out.stdout);
    assert!(out.stdout.contains("alpha(). :4"), "{}", out.stdout);
    assert!(!out.stdout.contains("補 symbol"), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
}

// ---------- T-M: M4 db errors fail loud, never silent Miss ----------

/// A nodes table whose schema probe passes but whose row decode fails
/// (file_path NULL) — `locate` must return Err.
fn install_null_file_path_graph(lab: &Lab) {
    let db = lab.repo.join(".code-reality/graph.db");
    std::fs::remove_file(&db).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE nodes (symbol TEXT, file_path TEXT, line_start INTEGER);
         INSERT INTO nodes VALUES ('pyrefly python proj 1.0 `m`/ghost().', NULL, 4);",
    )
    .unwrap();
}

#[test]
fn tm4_nodes_missing_file_path_column_fails_loud_not_miss() {
    let lab = fresh_lab("cr15-tm4");
    // M4: a legacy-schema nodes table (no file_path column) must fail
    // the schema probe loud — with the symbol-only probe, every lookup
    // silently degraded to Miss
    let db = lab.repo.join(".code-reality/graph.db");
    std::fs::remove_file(&db).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE nodes (symbol TEXT);").unwrap();
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 999, "ghost()", "def alpha"),
            SYM_GHOST
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert!(out.stdout.contains("非自有格式"), "{}", out.stdout);
    assert!(out.stdout.contains("file_path"), "{}", out.stdout);
    assert!(
        out.stdout.contains("symbol 驗證退化 line/pattern"),
        "{}",
        out.stdout
    );
    // the legacy face serves — a silent Miss would look identical minus
    // the degrade WARN, so the WARN is the discriminator
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L999->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
}

#[test]
fn tm4b_symbol_query_row_error_warns_and_degrades_legacy_not_miss() {
    let lab = fresh_lab("cr15-tm4b");
    install_null_file_path_graph(&lab);
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 999, "ghost()", "def alpha"),
            SYM_GHOST
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert!(
        out.stdout.contains("graph.db symbol 查詢失敗——退化 line/pattern"),
        "{}",
        out.stdout
    );
    let legacy = "[WARN] .tours/10-x/01.tour 步1 錨 corrected: src_mod.py L999->L4";
    assert!(out.stdout.contains(legacy), "{}", out.stdout);
}

#[test]
fn tm4c_reanchor_symbol_query_error_skips_loud() {
    let lab = fresh_lab("cr15-tm4c");
    install_null_file_path_graph(&lab);
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 999, "ghost()", "def alpha"),
            SYM_GHOST
        )]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("跳過（graph.db symbol 查詢失敗"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

// ---------- T-R: round-2 (unreadable target gate + corrected count) ----

#[test]
fn tr1_unreadable_new_file_validates_fail_not_corrected() {
    let lab = fresh_lab("cr15-tr1");
    // round-2 R1: SYM_BETA_MOVED uniquely addresses other_mod.py, but
    // that NEW file is gone (the graph fixture points at a deleted
    // file). A `^` pattern would false-verify against the empty-string
    // pool the old `unwrap_or_default` minted (line 1 of [""]) — the
    // step must FAIL unverified instead of a corrected WARN.
    std::fs::remove_file(lab.repo.join("other_mod.py")).unwrap();
    build_repo(&lab.repo, None, &[lab._bindir.path().to_path_buf()]).expect("rebuild");
    lab.install_graph(&graph_spec(&lab.repo));
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 8, "beta()", "^"),
            SYM_BETA_MOVED
        )]),
    );
    let out = tour_validate::validate(&lab.repo, Path::new(".tours"), false);
    assert_eq!(out.exit_code, 1, "{}", out.stdout);
    let want = "[FAIL] .tours/10-x/01.tour 步1 symbol 定址（source: SCIP symbol）: src_mod.py -> other_mod.py L8->L1；新檔不可讀（unverified）";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    assert!(
        !out.stdout.contains("corrected（source: SCIP symbol）"),
        "{}",
        out.stdout
    );
}

#[test]
fn tr2_reanchor_unreadable_new_file_skips_zero_writes() {
    let lab = fresh_lab("cr15-tr2");
    // round-2 R1 reanchor face: no proposal is opened and nothing is
    // written — the old face proposed (and `--apply` wrote) a
    // relocation onto a file that does not exist, counting it corrected
    std::fs::remove_file(lab.repo.join("other_mod.py")).unwrap();
    build_repo(&lab.repo, None, &[lab._bindir.path().to_path_buf()]).expect("rebuild");
    lab.install_graph(&graph_spec(&lab.repo));
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([with_symbol(
            step("src_mod.py", 8, "beta()", "^"),
            SYM_BETA_MOVED
        )]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("跳過（新檔不可讀 other_mod.py）"),
        "{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("（依據 SCIP symbol）"),
        "no proposal may be opened: {}",
        out.stdout
    );
    assert!(out.stdout.contains("corrected 0"), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn tr3_pattern_miss_counts_unverified_not_corrected() {
    let lab = fresh_lab("cr15-tr3");
    // round-2 R2: the summary's corrected counts ONLY pattern-verified
    // proposals — step 1 verifies (def alpha hits L4), step 2's pattern
    // is dead at L8 and must land in the unverified column
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([
            with_symbol(step("src_mod.py", 5, "alpha()", "def alpha"), SYM_ALPHA),
            with_symbol(step("src_mod.py", 9, "beta()", "return 999"), SYM_BETA),
        ]),
    );
    let out = run_reanchor(&lab.repo, &[]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("L5->L4"), "{}", out.stdout);
    assert!(out.stdout.contains("L9->L8"), "{}", out.stdout);
    assert!(out.stdout.contains("pattern 未命中新行"), "{}", out.stdout);
    assert!(
        out.stdout.contains("corrected 1 unverified 1"),
        "{}",
        out.stdout
    );
}

// ---------- faces ----------

#[test]
fn tf1_help_face_and_argparse_shape() {
    let out = tour_reanchor::run(&["tour_reanchor", "-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("usage: tour_reanchor"), "{}", out.stdout);
    assert!(out.stdout.contains("--bootstrap-symbols"), "{}", out.stdout);
    // unknown flag → argparse usage error (exit 2)
    let out = tour_reanchor::run(&["tour_reanchor", "--nope"]);
    assert_eq!(out.exit_code, 2);
}

#[test]
fn tf2_empty_corpus_fails_loud() {
    let t = tempfile::tempdir().unwrap();
    let repo = std::fs::canonicalize(t.path()).unwrap();
    std::fs::create_dir_all(repo.join(".tours")).unwrap();
    let out = run_reanchor(&repo, &[]);
    assert_eq!(out.exit_code, 1, "{} {}", out.stdout, out.stderr);
    assert!(out.stderr.contains("無 .tour"), "{}", out.stderr);
}

// ---------- T-Q (CR-18): qualifier titles through the bootstrap ----------

const QFMT_DEBUG: &str = "rust-analyzer cargo demo 0.1.0 pkg/impl#[Price][Debug]fmt().";
const QFMT_DISPLAY: &str = "rust-analyzer cargo demo 0.1.0 pkg/impl#[Price][Display]fmt().";

/// Same-file same-name `fmt` collision pair in src_mod.py: Display sits
/// on line 4 (`def alpha():` — the recorded anchor's pattern-hitting
/// line), Debug on line 8. The bootstrap must consume the title's
/// qualifier through the SAME parser/selector as chain_tour — the
/// pre-CR-18 best_ident extraction picked `Display` (the trait name)
/// off the title and missed the graph entirely.
fn qualifier_graph_spec(repo: &Path) -> GraphDbSpec {
    let py = abs(repo, "src_mod.py");
    let seed = |symbol: &str| NodeSeed {
        name: "fmt".into(),
        parent: None,
        qname: symbol.into(),
        file_path: py.clone(),
    };
    let attr = || NodeAttr {
        kind: "Function",
        language: "rust",
        is_test: 0,
        community_id: None,
    };
    let mut spec = GraphDbSpec {
        nodes: vec![seed(QFMT_DEBUG), seed(QFMT_DISPLAY)],
        ..Default::default()
    };
    spec.node_attrs = vec![(QFMT_DEBUG.into(), attr()), (QFMT_DISPLAY.into(), attr())];
    spec.node_lines = vec![(QFMT_DEBUG.into(), 8), (QFMT_DISPLAY.into(), 4)];
    spec
}

#[test]
fn tq1_bootstrap_qualifier_title_backfills_declared_identity() {
    let lab = fresh_lab("cr18-tq1");
    lab.install_graph(&qualifier_graph_spec(&lab.repo));
    write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 4, "Price::fmt[Display]", "def alpha")]),
    );
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("補 symbol"), "{}", out.stdout);
    assert!(out.stdout.contains(QFMT_DISPLAY), "{}", out.stdout);
    assert!(out.stdout.contains("written 1"), "{}", out.stdout);
    let after = written_tour(&lab.repo);
    assert_eq!(
        after["steps"][0]["x-codeReality"]["symbol"],
        json!(QFMT_DISPLAY)
    );
    // pattern still hits the recorded line — the line never moves
    assert_eq!(after["steps"][0]["line"], json!(4));
}

#[test]
fn tq2_bootstrap_malformed_qualifier_title_refuses_backfill() {
    let lab = fresh_lab("cr18-tq2");
    lab.install_graph(&qualifier_graph_spec(&lab.repo));
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 4, "Price::fmt[Display", "def alpha")]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("qualifier 語法錯誤"),
        "malformed qualifier title must be refused explicitly: {}",
        out.stdout
    );
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}

#[test]
fn tq3_bootstrap_qualifier_no_hit_skips_with_reason() {
    let lab = fresh_lab("cr18-tq3");
    lab.install_graph(&qualifier_graph_spec(&lab.repo));
    let tour = write_tour(
        &lab.repo,
        ".tours/10-x/01.tour",
        json!([step("src_mod.py", 4, "Price::fmt[ToString]", "def alpha")]),
    );
    let before = read_bytes(&tour);
    let out = run_reanchor(&lab.repo, &["--apply", "--bootstrap-symbols"]);
    assert_eq!(out.exit_code, 0, "{} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("qualifier_no_hit"), "{}", out.stdout);
    assert!(out.stdout.contains("written 0"), "{}", out.stdout);
    assert_eq!(read_bytes(&tour), before);
}
