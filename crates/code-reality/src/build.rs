//! `build` — one-shot data-plane bootstrap for a repo (EP
//! ep-build-umbrella). Orchestration only, zero new production logic:
//! detect the language face → run the matching producer (pyrefly-index
//! / rust-analyzer scip, spawned as sibling bins — the crates are
//! separate dists, so process spawn is the only legal coupling) →
//! in-process `graph_db build` + `ensure_indexes` → state summary.
//!
//! Mixed repos default to BOTH legs with the protobuf cat-merge trick:
//! concatenating two encoded `scip.Index` messages of the same type is
//! a legal merge (repeated fields stack), so the unified graph needs no
//! graph_db changes (POC-verified 2026-08-29: one db serving
//! `scip_refs` for both `src/lib.rs:1` and `app.py:1`).
//!
//! Known trap (POC): `rust-analyzer scip` takes the repo DIRECTORY —
//! passing Cargo.toml exits 0 with a metadata-only "empty" index, hence
//! the <128-byte guard below.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{first_output_line, resolve_bin};
use crate::engine::{default_index_path, resolve_repo};
use crate::graph_db;
use crate::ToolOutput;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--producer",
            short: None,
            kind: Kind::Value {
                metavar: "rust|python",
            },
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = "usage: code-reality build --repo <repo> [--producer rust|python] [--json]
  --repo REPO              repo root whose data plane gets bootstrapped
  --producer rust|python   override detection (mixed repos run both legs
                           by default and cat-merge into one graph)
  --json                   machine-readable report
";

/// Producer outputs below this size are metadata-only "empty" indexes
/// (the Cargo.toml-form trap produced 102-122 bytes; a legal minimal
/// crate index is 725 bytes — POC- calibrated).
const EMPTY_INDEX_BYTES: u64 = 128;

#[derive(Debug, PartialEq, Eq)]
pub enum RepoKind {
    Python,
    Rust,
    Mixed { py: usize, rs: usize },
}

#[derive(Debug)]
pub struct Report {
    pub repo: PathBuf,
    pub face: String,
    pub producers: Vec<String>,
    pub index: PathBuf,
    pub nodes: usize,
    pub edges: usize,
    pub graph_rebuilt: bool,
    pub indexes_created: usize,
    pub indexes_skipped: usize,
    pub notes: Vec<String>,
}

/// Error families map onto different exits (EP review finding 4):
/// `Env` → `fail(2)` (fixable environment: missing bin, child failure,
/// empty index, bad repo path), `Core` → `crash(1)` (graph-build core
/// errors, aligned with graph_db.rs/sidecar_migrate.rs precedent).
#[derive(Debug)]
pub enum BuildError {
    Env(String),
    Core(String),
}

impl BuildError {
    fn msg(&self) -> &str {
        match self {
            BuildError::Env(m) | BuildError::Core(m) => m,
        }
    }
}

pub fn count_sources(repo: &Path) -> Result<(usize, usize), String> {
    // Composed (not duplicated) from the shared corpus list; the build
    // detector additionally skips `target` (OUT_DIR artifacts are not
    // source — the staleness walk keeps .py there for the python face).
    let mut skips: Vec<&str> = crate::engine::SKIP_DIRS.to_vec();
    skips.push("target");
    let mut stack = vec![repo.to_path_buf()];
    let (mut py, mut rs) = (0usize, 0usize);
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("讀取 {} 失敗：{e}", dir.display()))?;
        for ent in entries.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            let name = ent.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                if name.starts_with('.') || skips.contains(&name.as_str()) {
                    continue;
                }
                stack.push(ent.path());
            } else if ft.is_file() {
                if name.ends_with(".py") {
                    py += 1;
                } else if name.ends_with(".rs") {
                    rs += 1;
                }
            }
        }
    }
    Ok((py, rs))
}

fn detect_kind(repo: &Path) -> Result<RepoKind, BuildError> {
    let (py, rs) = count_sources(repo).map_err(BuildError::Core)?;
    match (py, rs) {
        (0, 0) => Err(BuildError::Env(
            "找不到 .py 或 .rs 原始碼——build 需要至少一種語言面".to_string(),
        )),
        (_, 0) => Ok(RepoKind::Python),
        (0, _) => Ok(RepoKind::Rust),
        (py, rs) => Ok(RepoKind::Mixed { py, rs }),
    }
}

pub(crate) fn producer_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        roots.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".cargo/bin"));
    }
    roots
}

fn python_leg(repo: &Path, rep: &mut Report, roots: &[PathBuf]) -> Result<(), BuildError> {
    let bin = resolve_bin(
        "pyrefly-index",
        roots,
        "安裝：uv tool install pyrefly-producer（或 cargo install --path crates/pyrefly-producer）",
    )
    .map_err(|e| BuildError::Env(format!("{e}\n")))?;
    if let Some(v) = first_output_line(&bin, &["--version"]) {
        rep.producers.push(format!("pyrefly-index {v}"));
    }
    // No --out: the producer writes the in-repo slot itself and
    // invalidates superseded sidecar artifacts beside it.
    let out = Command::new(&bin)
        .arg("--repo")
        .arg(repo)
        .current_dir(repo)
        .output()
        .map_err(|e| BuildError::Env(format!("spawn {} 失敗：{e}", bin.display())))?;
    if !out.status.success() {
        return Err(BuildError::Env(format!(
            "pyrefly-index 失敗（{}）：\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

fn rust_leg(
    repo: &Path,
    out_path: &Path,
    rep: &mut Report,
    roots: &[PathBuf],
) -> Result<(), BuildError> {
    let bin = resolve_bin(
        "rust-analyzer",
        roots,
        "安裝：rustup component add rust-analyzer",
    )
    .map_err(|e| BuildError::Env(format!("{e}\n")))?;
    if let Some(v) = first_output_line(&bin, &["--version"]) {
        rep.producers.push(v);
    }
    // current_dir pins the rustup proxy's toolchain resolution to the
    // repo (cwd-based proxy trap); the repo DIRECTORY (not Cargo.toml)
    // is the verified CLI shape.
    let out = Command::new(&bin)
        .arg("scip")
        .arg(repo)
        .arg("--output")
        .arg(out_path)
        .current_dir(repo)
        .output()
        .map_err(|e| {
            BuildError::Env(format!(
                "spawn {} 失敗：{e}——rustup component add rust-analyzer",
                bin.display()
            ))
        })?;
    if !out.status.success() {
        return Err(BuildError::Env(format!(
            "rust-analyzer scip 失敗（{}）：\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let len = std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
    if len < EMPTY_INDEX_BYTES {
        return Err(BuildError::Env(
            "producer 產出空索引（<128 bytes）——workspace 載入可能失敗；rust-analyzer scip 需傳 repo 目錄（非 Cargo.toml）"
                .to_string(),
        ));
    }
    Ok(())
}

/// Protobuf same-type message concatenation: repeated fields stack, so
/// `read(slot) ++ read(part)` written back is a legal merged Index.
/// Temp-sibling + rename (atomic, graph_db build precedent).
pub(crate) fn concat_scip(slot: &Path, part: &Path) -> Result<(), String> {
    let a = std::fs::read(slot).map_err(|e| format!("讀 {} 失敗：{e}", slot.display()))?;
    let b = std::fs::read(part).map_err(|e| format!("讀 {} 失敗：{e}", part.display()))?;
    let tmp = slot.with_file_name(".merge-tmp.scip");
    std::fs::write(&tmp, [a, b].concat()).map_err(|e| format!("寫 {} 失敗：{e}", tmp.display()))?;
    std::fs::rename(&tmp, slot)
        .map_err(|e| format!("rename {} → {} 失敗：{e}", tmp.display(), slot.display()))
}

/// Core orchestration. `roots` is the bin-search path list (injectable
/// for tests); `producer` overrides the detected face.
pub fn build_repo(
    repo: &Path,
    producer: Option<&str>,
    roots: &[PathBuf],
) -> Result<Report, BuildError> {
    let resolved = resolve_repo(repo);
    if !resolved.is_dir() {
        return Err(BuildError::Env(format!(
            "--repo {} 不是目錄——請確認路徑（不建立目錄）",
            repo.display()
        )));
    }
    let kind = detect_kind(&resolved)?;
    let slot_dir = resolved.join(".code-reality").join("scip");
    std::fs::create_dir_all(&slot_dir)
        .map_err(|e| BuildError::Env(format!("建立 {} 失敗：{e}", slot_dir.display())))?;
    // gitignore from the earliest write window (a failed leg must not
    // leave an untracked data dir behind — sidecar_migrate precedent)
    crate::engine::write_data_dir_gitignore(&resolved.join(".code-reality"))
        .map_err(BuildError::Env)?;
    let slot = default_index_path(&resolved).map_err(BuildError::Core)?;
    let slot_existed = slot.exists();

    let (mut run_py, mut run_rs) = match kind {
        RepoKind::Python => (true, false),
        RepoKind::Rust => (false, true),
        RepoKind::Mixed { .. } => (true, true),
    };
    if let Some(p) = producer {
        run_py = p == "python";
        run_rs = p == "rust";
    }

    let mut rep = Report {
        repo: resolved.clone(),
        face: String::new(),
        producers: Vec::new(),
        index: slot.clone(),
        nodes: 0,
        edges: 0,
        graph_rebuilt: graph_db::db_path(&resolved).exists(),
        indexes_created: 0,
        indexes_skipped: 0,
        notes: Vec::new(),
    };

    match (run_py, run_rs) {
        (true, false) => {
            rep.face = "python-face".to_string();
            python_leg(&resolved, &mut rep, roots)?;
            if slot_existed {
                rep.notes
                    .push("覆蓋既有 index.scip（slot 單檔——先前面目已取代）".to_string());
            }
        }
        (false, true) => {
            rep.face = "rust-face".to_string();
            // The leg always writes the sibling part; the single-leg face
            // lands it on the slot via rename so concurrent readers never
            // see a torn index (rust-analyzer would otherwise write the
            // slot in place — the mixed path already had rename via
            // concat_scip).
            let rs_part = slot_dir.join(".rust-part.scip");
            if let Err(e) = rust_leg(&resolved, &rs_part, &mut rep, roots) {
                let _ = std::fs::remove_file(&rs_part);
                return Err(e);
            }
            std::fs::rename(&rs_part, &slot).map_err(|e| {
                BuildError::Core(format!(
                    "rename {} → {} 失敗：{e}",
                    rs_part.display(),
                    slot.display()
                ))
            })?;
            if slot_existed {
                rep.notes
                    .push("覆蓋既有 index.scip（slot 單檔——先前面目已取代）".to_string());
            }
        }
        (true, true) => {
            rep.face = "mixed(rust+python)".to_string();
            // Python writes the slot (plus sidecar invalidation); rust
            // writes a sibling part file, then the two are cat-merged
            // into the slot (one graph serves both languages).
            let rs_part = slot_dir.join(".rust-part.scip");
            python_leg(&resolved, &mut rep, roots)?;
            if let Err(e) = rust_leg(&resolved, &rs_part, &mut rep, roots) {
                let _ = std::fs::remove_file(&rs_part);
                return Err(e);
            }
            concat_scip(&slot, &rs_part).map_err(BuildError::Core)?;
            let _ = std::fs::remove_file(&rs_part);
            rep.notes
                .push("雙語言合一 graph（rust+python 串接）".to_string());
        }
        (false, false) => {
            return Err(BuildError::Env(
                "--producer 內部錯誤：需為 rust 或 python".to_string(),
            ));
        }
    }

    // Stamp index provenance in-process with the legs that actually ran
    // (face-accurate producer string; relay Finding B's no-unstamped-slot
    // goal — direct lib call, no cli::run indirection so the query-path
    // heal hook cannot re-enter at all).
    let producer = rep.producers.join("; ");
    if let Err(e) = crate::engine::stamp_meta_core(
        &resolved,
        &slot,
        roots,
        if producer.is_empty() {
            None
        } else {
            Some(&producer)
        },
    ) {
        rep.notes.push(format!(
            "stamp-meta 失敗（{e}）——手動補：code-reality scip_refs --repo {} --stamp-meta",
            resolved.display()
        ));
    }

    if matches!(kind, RepoKind::Mixed { .. }) && (run_py ^ run_rs) {
        rep.notes.push(format!(
            "未索引：{}（--producer 切換）",
            if run_py { "rust" } else { "python" }
        ));
    }
    rep.notes
        .push("全量重產：producer 每次重建（冪等）".to_string());

    let g = graph_db::build_from_cache_at(&resolved, &slot).map_err(BuildError::Core)?;
    rep.nodes = g.nodes;
    rep.edges = g.edges;
    let ir = graph_db::ensure_indexes(&resolved).map_err(BuildError::Core)?;
    rep.indexes_created = ir.created;
    rep.indexes_skipped = ir.skipped;
    Ok(rep)
}

// ---------- query-time index heal (S3, ep-index-query-time-self-heal) ----------

/// Outcome of a pre-query freshness check. `Fresh` covers "nothing to do"
/// (including head-drift-only — that WARN is source_line's single
/// source); `ServeStale` means the caller answers from the existing
/// index with a loud WARN — a heal failure never blocks the query
/// (open_face "answering beats a traceback" philosophy, one layer up).
#[derive(Debug, Clone, PartialEq)]
pub enum HealOutcome {
    Fresh,
    Healed {
        secs: f64,
        nodes: usize,
        notes: Vec<String>,
    },
    HealedByPeer {
        waited_secs: f64,
    },
    ServeStale(Vec<String>),
}

/// Single-flight lock beside the slot: one healer at a time across the
/// query heal and the post-commit refresh. Drop releases.
struct HealLock {
    path: PathBuf,
}

impl Drop for HealLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Lock age beyond which a crashed holder's lock is stealable (producer
/// runs are seconds-to-a-minute; 10min is generous).
const HEAL_LOCK_MAX_AGE: Duration = Duration::from_secs(600);
/// Wait budget for a peer healer before serving stale.
const HEAL_WAIT_BUDGET: Duration = Duration::from_secs(120);
const HEAL_POLL: Duration = Duration::from_millis(200);

fn acquire_heal_lock(slot_dir: &Path) -> Result<Option<HealLock>, String> {
    let p = slot_dir.join(".heal.lock");
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                use std::io::Write;
                let _ = writeln!(f, "{} {}", std::process::id(), crate::engine::utc_now_iso());
                return Ok(Some(HealLock { path: p }));
            }
            Err(e) => {
                if !p.exists() {
                    // creation failed with no lock present (read-only dir
                    // etc.) — an environment failure, NOT "held"; surfacing
                    // it as held would spin the wait budget for nothing
                    return Err(format!("無法建立 heal lock（{}）：{e}", slot_dir.display()));
                }
                // held — steal only an abandoned lock (mtime past max age)
                let abandoned = p
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|age| age > HEAL_LOCK_MAX_AGE);
                if !abandoned || std::fs::remove_file(&p).is_err() {
                    return Ok(None);
                }
                // stolen — retry the create (a fresh racer just wins it)
            }
        }
    }
}

/// Flagged-path-only producer drift note (the steady-state query path is
/// zero-spawn by rule): stamped producer vs installed — an upgrade
/// signal, never a rebuild trigger (rebuilding with the same stale
/// producer changes nothing).
fn producer_drift_note(slot: &Path, roots: &[PathBuf]) -> Option<String> {
    let stamped = crate::engine::load_meta(slot)
        .0
        .and_then(|m| m["producer"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty() && s != "<unresolved>")?;
    // Only the pyrefly segment is compared: the stamp records the legs
    // that actually ran ("pyrefly-index X; rust-analyzer Y"), and
    // rust-analyzer's version floats with the toolchain — a rust-face
    // stamp carries no pyrefly segment and never warns here.
    let stamped_py = stamped
        .split("; ")
        .find(|seg| seg.starts_with("pyrefly-index "))?
        .to_string();
    let stamped_v = stamped_py
        .strip_prefix("pyrefly-index ")
        .unwrap_or(&stamped_py);
    let current = crate::common::producer_version("pyrefly-index", roots)?;
    (stamped_v != current).then(|| {
        format!(
            "[WARN] producer 版本錯配（stamp pyrefly-index {stamped_v} ≠ 現裝 {current}）——升級：uv tool install -U pyrefly-producer\n"
        )
    })
}

/// Post-rebuild-error outcome (SM-17 half-success): the rebuild failed
/// AFTER the producer may have landed a fresh index — re-evaluate before
/// serving stale, so a graph-only failure is not mislabeled. Public for
/// direct testing (the graph step is not injectable through fake bins).
pub fn heal_outcome_after_rebuild_err(
    repo: &Path,
    slot: &Path,
    err: String,
) -> Result<HealOutcome, String> {
    let snap = crate::engine::evaluate_staleness(repo, slot)?;
    if !snap.source_newer {
        Ok(HealOutcome::Healed {
            secs: 0.0,
            nodes: 0,
            notes: vec![format!("[WARN] graph 未重建（{err}）——下次顯式 build 補\n")],
        })
    } else {
        Ok(HealOutcome::ServeStale(vec![format!(
            "[WARN] {err}——本次查詢以現存索引作答\n"
        )]))
    }
}

fn run_heal_locked(
    repo: &Path,
    slot: &Path,
    roots: &[PathBuf],
    _lock: HealLock,
    t0: Instant,
) -> Result<HealOutcome, String> {
    match build_repo(repo, None, roots) {
        Err(e) => {
            let mut out = heal_outcome_after_rebuild_err(repo, slot, e.msg().to_string())?;
            if let HealOutcome::ServeStale(lines) = &mut out {
                lines.extend(producer_drift_note(slot, roots));
            }
            Ok(out)
        }
        Ok(rep) => {
            // Loop guard (SM-9): a rebuild that still leaves the slot
            // behind warns once and serves — never loops.
            let snap = crate::engine::evaluate_staleness(repo, slot)?;
            if snap.source_newer {
                let mut lines = vec![
                    "[WARN] heal 期間原始碼又變動——本次查詢以現存索引作答（下次查詢自動再癒）\n"
                        .to_string(),
                ];
                lines.extend(producer_drift_note(slot, roots));
                return Ok(HealOutcome::ServeStale(lines));
            }
            let delta = match crate::engine::load_index(slot) {
                Ok(loaded) => {
                    let docs: BTreeSet<String> = loaded
                        .index
                        .documents
                        .iter()
                        .map(|d| d.relative_path.clone())
                        .collect();
                    let walk = crate::engine::walk_sources(repo)?;
                    Some(crate::engine::doc_set_delta(&docs, &walk))
                }
                // unparseable fresh output is the build's own failure face
                Err(_) => None,
            };
            if let Some(d) = &delta {
                if d.missing > 0 {
                    let mut lines = vec![format!(
                        "[WARN] 偵測與 producer 語料不一致（false-stale：missing={}，例：{}）——不迴圈，本次查詢以現存索引作答\n",
                        d.missing,
                        d.examples.join("、")
                    )];
                    lines.extend(producer_drift_note(slot, roots));
                    return Ok(HealOutcome::ServeStale(lines));
                }
            }
            let notes = producer_drift_note(slot, roots).into_iter().collect();
            Ok(HealOutcome::Healed {
                secs: t0.elapsed().as_secs_f64(),
                nodes: rep.nodes,
                notes,
            })
        }
    }
}

fn wait_peer_and_reevaluate(
    repo: &Path,
    slot: &Path,
    roots: &[PathBuf],
    t0: Instant,
) -> Result<HealOutcome, String> {
    let lock_path = slot
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(".heal.lock");
    loop {
        if !lock_path.exists() {
            let snap = crate::engine::evaluate_staleness(repo, slot)?;
            if !snap.source_newer {
                return Ok(HealOutcome::HealedByPeer {
                    waited_secs: t0.elapsed().as_secs_f64(),
                });
            }
            // peer released without fixing it — become the healer
            let slot_dir = slot.parent().unwrap_or_else(|| Path::new("."));
            return match acquire_heal_lock(slot_dir) {
                Ok(Some(lock)) => run_heal_locked(repo, slot, roots, lock, t0),
                Ok(None) => {
                    std::thread::sleep(HEAL_POLL); // raced again; keep polling
                    continue;
                }
                Err(e) => Ok(HealOutcome::ServeStale(vec![format!(
                    "[WARN] {e}——本次查詢以現存索引作答\n"
                )])),
            };
        }
        if t0.elapsed() > HEAL_WAIT_BUDGET {
            let mut lines = vec![
                "[WARN] heal lock 等待逾時（併發 healer 未釋放）——本次查詢以現存索引作答\n"
                    .to_string(),
            ];
            lines.extend(producer_drift_note(slot, roots));
            return Ok(HealOutcome::ServeStale(lines));
        }
        std::thread::sleep(HEAL_POLL);
    }
}

/// Pre-query freshness gate (S3). `Err` = the staleness CHECK itself
/// failed (walk/stat) — the caller warns and answers from the existing
/// index. Missing slot → `Fresh` so the caller's own missing-index FAIL
/// path stays authoritative (SM-11, no bootstrap-on-miss).
pub fn ensure_fresh(repo: &Path, roots: &[PathBuf]) -> Result<HealOutcome, String> {
    let repo = resolve_repo(repo);
    let slot = default_index_path(&repo)?;
    if !slot.exists() {
        return Ok(HealOutcome::Fresh);
    }
    let snap = crate::engine::evaluate_staleness(&repo, &slot)?;
    if !snap.source_newer {
        return Ok(HealOutcome::Fresh);
    }
    let t0 = Instant::now();
    let slot_dir = slot
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo.join(".code-reality/scip"));
    match acquire_heal_lock(&slot_dir) {
        Ok(Some(lock)) => run_heal_locked(&repo, &slot, roots, lock, t0),
        Ok(None) => wait_peer_and_reevaluate(&repo, &slot, roots, t0),
        Err(e) => {
            let mut lines = vec![format!("[WARN] {e}——本次查詢以現存索引作答\n")];
            lines.extend(producer_drift_note(&slot, roots));
            Ok(HealOutcome::ServeStale(lines))
        }
    }
}

fn render(rep: Report, json: bool) -> ToolOutput {
    if json {
        let notes: Vec<String> = rep.notes.clone();
        let v = serde_json::json!({
            "repo": rep.repo.display().to_string(),
            "face": rep.face,
            "producers": rep.producers,
            "index": rep.index.display().to_string(),
            "nodes": rep.nodes,
            "edges": rep.edges,
            "graph_rebuilt": rep.graph_rebuilt,
            "indexes": {"created": rep.indexes_created, "skipped": rep.indexes_skipped},
            "notes": notes,
        });
        return ToolOutput {
            stdout: format!("{}\n", crate::common::to_json_indent1(&v)),
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let mut out = format!(
        "[OK] build: {} [{}]\n  index: {}\n  graph: {} nodes / {} edges{}\n  indexes: {} created / {} skipped\n",
        rep.repo.display(),
        rep.face,
        rep.index.display(),
        rep.nodes,
        rep.edges,
        if rep.graph_rebuilt { "（重建）" } else { "" },
        rep.indexes_created,
        rep.indexes_skipped,
    );
    for p in &rep.producers {
        out.push_str(&format!("  producer: {p}\n"));
    }
    for n in &rep.notes {
        out.push_str(&format!("  note: {n}\n"));
    }
    ToolOutput {
        stdout: out,
        stderr: String::new(),
        exit_code: 0,
    }
}

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((_tool, toks)) = argv.split_first() else {
        return ToolOutput::fail(HELP.trim_end());
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
    let json = values.contains_key("--json");
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let producer = values.get("--producer").and_then(|v| v.clone());
    if let Some(p) = &producer {
        if p != "rust" && p != "python" {
            return ToolOutput::fail("--producer 需為 rust 或 python");
        }
    }
    match build_repo(Path::new(&repo), producer.as_deref(), &producer_roots()) {
        Ok(rep) => render(rep, json),
        Err(e) => match e {
            BuildError::Env(m) => ToolOutput::fail(format!("build: {m}")),
            BuildError::Core(m) => ToolOutput::crash(format!("build: {m}")),
        },
    }
}
