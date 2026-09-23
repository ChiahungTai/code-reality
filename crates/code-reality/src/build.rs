//! `build` — one-shot data-plane bootstrap for a repo (EP
//! ep-build-umbrella + JS/TS blueprint S1/S2). Orchestration only:
//! detect the language faces → run the selected producer legs
//! (pyrefly-index / rust-analyzer scip / scip-typescript, spawned as
//! sibling or external bins — process spawn is the only legal coupling)
//! → in-process `graph_db build` + `ensure_indexes` → state summary.
//!
//! Language-set orchestration (S1): the binary `RepoKind` branching is
//! gone; detection yields a [`SourceInventory`] over the
//! [`crate::language`] extension mapping, legs run in the frozen
//! `ProducerFamily::ORDERED` order, and EVERY leg stages a validated
//! partial under pid-keyed names. The live slot is published exactly
//! once — `merge_and_publish` — after all requested legs validate, so a
//! failed later leg leaves the pre-build slot byte-identical (the old
//! Python-first path wrote the live slot directly; that hole is closed).
//!
//! Mixed indexes use the protobuf cat-merge trick: concatenating encoded
//! `scip.Index` messages of the same type is a legal merge (repeated
//! fields stack; N-way feasibility POC-verified). CR product code does
//! not consume `Index.metadata` — the sidecar stamp stays the
//! provenance authority — but the merge order is frozen and tested
//! because protobuf singular fields are last-value-wins.
//!
//! Known trap (POC): `rust-analyzer scip` takes the repo DIRECTORY —
//! passing Cargo.toml exits 0 with a metadata-only "empty" index, hence
//! the <128-byte guard below.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{first_output_line, resolve_bin};
use crate::engine::{default_index_path, resolve_repo};
use crate::graph_db;
use crate::language::ProducerFamily;
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
                metavar: "rust|python|typescript",
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

const HELP: &str =
    "usage: code-reality build --repo <repo> [--producer rust|python|typescript] [--json]
  --repo REPO              repo root whose data plane gets bootstrapped
  --producer rust|python|typescript
                           override detection（typescript = JS/TS 共用腿）；
                           mixed repos run all legs by default and cat-merge
                           into one graph
  --json                   machine-readable report
";

/// Producer outputs below this size are metadata-only "empty" indexes
/// (the Cargo.toml-form trap produced 102-122 bytes; a legal minimal
/// crate index is 725 bytes — POC- calibrated).
const EMPTY_INDEX_BYTES: u64 = 128;

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
    /// Content-addressed source identity stamped into the meta (None on
    /// a legacy/preserved stamp) — additive `build --json` key.
    pub source_identity: Option<String>,
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

/// Per-face source counts from the detection walk (S1). `js_ts` counts
/// all six JS/TS extensions — JS and TS are one producer family with
/// two graph-language labels.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceInventory {
    pub py: usize,
    pub rs: usize,
    pub js_ts: usize,
}

pub fn count_sources(repo: &Path) -> Result<SourceInventory, String> {
    // Composed (not duplicated) from the shared corpus list; the build
    // detector additionally skips `target` (OUT_DIR artifacts are not
    // source — the staleness walk keeps .py there for the python face).
    // Detection is deliberately profile-UNFILTERED (superset walk:
    // over-selecting a face degrades to a leg that skips on its empty
    // governed corpus; under-selecting would silently drop sources).
    let mut skips: Vec<&str> = crate::engine::SKIP_DIRS.to_vec();
    skips.push("target");
    let mut stack = vec![repo.to_path_buf()];
    let mut inv = SourceInventory::default();
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
                match crate::language::LanguageFace::from_path(Path::new(&name)) {
                    Some(crate::language::LanguageFace::Python) => inv.py += 1,
                    Some(crate::language::LanguageFace::Rust) => inv.rs += 1,
                    Some(crate::language::LanguageFace::JavaScript)
                    | Some(crate::language::LanguageFace::TypeScript) => inv.js_ts += 1,
                    None => {}
                }
            }
        }
    }
    Ok(inv)
}

/// Detected producer families (S1: detection feeds an ordered set, not a
/// `RepoKind` enum — language three must not grow combinatorial states).
fn detect_families(inv: &SourceInventory) -> BTreeSet<ProducerFamily> {
    let mut out = BTreeSet::new();
    if inv.py > 0 {
        out.insert(ProducerFamily::Python);
    }
    if inv.rs > 0 {
        out.insert(ProducerFamily::Rust);
    }
    if inv.js_ts > 0 {
        out.insert(ProducerFamily::TypeScript);
    }
    out
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

/// Protobuf same-type message concatenation: repeated fields stack, so
/// `read(slot) ++ read(part)` written back is a legal merged Index.
/// Temp-sibling + rename (atomic, graph_db build precedent). Consumed
/// by the `project` overlay merge; `build` itself stages and merges
/// through [`merge_and_publish`].
pub(crate) fn concat_scip(slot: &Path, part: &Path) -> Result<(), String> {
    let a = std::fs::read(slot).map_err(|e| format!("讀 {} 失敗：{e}", slot.display()))?;
    let b = std::fs::read(part).map_err(|e| format!("讀 {} 失敗：{e}", part.display()))?;
    let tmp = slot.with_file_name(".merge-tmp.scip");
    std::fs::write(&tmp, [a, b].concat()).map_err(|e| format!("寫 {} 失敗：{e}", tmp.display()))?;
    std::fs::rename(&tmp, slot)
        .map_err(|e| format!("rename {} → {} 失敗：{e}", tmp.display(), slot.display()))
}

/// A producer partial validated for merge: pid-keyed path unique to one
/// build attempt (a concurrent/failed attempt never overwrites another's
/// part — S1 atomicity prerequisite).
struct StagedLeg {
    family: ProducerFamily,
    path: PathBuf,
}

/// Producer outputs below this size are metadata-only "empty" indexes
/// (the Cargo.toml-form trap produced 102-122 bytes; a legal minimal
/// crate index is 725 bytes — POC- calibrated).
fn validate_partial(path: &Path) -> Result<(), BuildError> {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len < EMPTY_INDEX_BYTES {
        return Err(BuildError::Env(
            "producer 產出空索引（<128 bytes）——workspace 載入可能失敗；rust-analyzer scip 需傳 repo 目錄（非 Cargo.toml）"
                .to_string(),
        ));
    }
    Ok(())
}

/// Outcome of the Python producer leg (TS-leg mirror).
#[derive(Debug)]
enum PyProduceOutcome {
    /// The governed (profile-filtered) Python corpus is empty — the leg
    /// is intentionally absent, not a failure (TS `SkippedByProfile`
    /// precedent; feeds the same empty-convergence terminal).
    SkippedByProfile,
    /// Validated, governed-filtered partial written to the staged path.
    Staged {
        producer_version: Option<String>,
        indexed_docs: usize,
        /// Governed documents the producer did NOT emit (F1 narrowed:
        /// warn, never fail — one unparsable file must not sink the build).
        missing_governed: Vec<String>,
    },
}

/// Governed Python source documents: repo-relative, `/`-normalized —
/// the producer-side corpus authority for the staged partial (AD-11:
/// the same effective policy the freshness walk applies, so a file
/// cannot be excluded from the SCIP face while staying
/// freshness-relevant, or vice versa).
fn collect_py_corpus(repo: &Path) -> Result<BTreeSet<String>, String> {
    let walk = crate::engine::walk_sources(repo)?;
    Ok(walk
        .py
        .keys()
        .map(|p| p.replace('\\', "/"))
        .collect::<BTreeSet<String>>())
}

/// Remove profile-excluded documents from a staged Python partial, in
/// place (AD-7: "a way to remove profile-excluded documents from the
/// final partial index"). Drop-if-excluded — NOT retain-if-governed
/// (unlike the TS exact-set check): the Python producer is spawned with
/// just `--repo`, so its own walk is the corpus authority and a document
/// outside the governed set that is NOT profile-excluded (symlink corpus,
/// path-shape surprises, producer/disk TOCTOU) must survive — only an
/// explicit profile prefix removes a document. Returns the retained
/// document count plus the governed documents the producer did not emit
/// (F1 narrowed: surfaced as a build WARN, never a failure).
fn filter_python_partial(
    part: &Path,
    governed: &BTreeSet<String>,
    profile: Option<&crate::profile::Profile>,
) -> Result<(usize, Vec<String>), String> {
    use protobuf::Message;
    // The staged partial's mtime IS the slot mtime (merge publishes by
    // rename, which preserves it) — and the churn loop-guard compares
    // source mtimes against it. Rewriting the bytes must not refresh the
    // clock past an in-build source edit, or a still-churning repo would
    // read as converged. Snapshot and restore around the rewrite.
    let mtime = std::fs::metadata(part)
        .and_then(|m| m.modified())
        .map_err(|e| format!("stat {} 失敗：{e}", part.display()))?;
    let bytes = std::fs::read(part).map_err(|e| format!("讀 {} 失敗：{e}", part.display()))?;
    let mut index = scip::types::Index::parse_from_bytes(&bytes)
        .map_err(|e| format!("pyrefly-index 產出無法解析（{}）：{e}", part.display()))?;
    index.documents.retain(|d| {
        !crate::profile::is_excluded(
            &crate::js_ts_corpus::normalize_rel(&d.relative_path),
            profile,
        )
    });
    let kept = index.documents.len();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for d in &index.documents {
        seen.insert(crate::js_ts_corpus::normalize_rel(&d.relative_path));
    }
    let missing: Vec<String> = governed
        .iter()
        .filter(|g| !seen.contains(*g))
        .cloned()
        .collect();
    let payload = index
        .write_to_bytes()
        .map_err(|e| format!("SCIP 序列化失敗：{e}"))?;
    std::fs::write(part, payload).map_err(|e| format!("寫 {} 失敗：{e}", part.display()))?;
    std::fs::File::options()
        .write(true)
        .open(part)
        .and_then(|f| f.set_modified(mtime))
        .map_err(|e| format!("還原 {} mtime 失敗：{e}", part.display()))?;
    Ok((kept, missing))
}

/// Python leg, staged (S1: every leg writes an explicit partial — the
/// producer's own sidecar invalidation keys off its output path, so a
/// pid-keyed part cannot touch the live slot's sidecars). The external
/// `pyrefly-index` takes no profile input, so the governed filter lands
/// here at the merge/normalize layer (AD-7), mirroring the TS
/// `filter_to_governed` post-step.
fn stage_python_leg(
    repo: &Path,
    part: &Path,
    roots: &[PathBuf],
) -> Result<PyProduceOutcome, BuildError> {
    let governed = collect_py_corpus(repo).map_err(BuildError::Env)?;
    if governed.is_empty() {
        return Ok(PyProduceOutcome::SkippedByProfile);
    }
    // Loaded once for the drop-if-excluded post-step (the walk above
    // loaded it too — TOML parse cost is trivial beside a producer run).
    let profile = crate::profile::load_profile(repo).map_err(BuildError::Env)?;
    let version = stage_python(repo, part, roots)?;
    validate_partial(part)?;
    let (kept, missing) =
        filter_python_partial(part, &governed, profile.as_ref()).map_err(BuildError::Env)?;
    if kept == 0 {
        return Err(BuildError::Env(format!(
            "pyrefly-index 索引 0 個 governed 文檔（語料 {} 檔）——producer 語料與 governed 語料無交集",
            governed.len()
        )));
    }
    Ok(PyProduceOutcome::Staged {
        producer_version: version,
        indexed_docs: kept,
        missing_governed: missing,
    })
}

/// Raw spawn of the external `pyrefly-index` (no profile input — the
/// governed filter is the caller's post-step, see [`stage_python_leg`]).
fn stage_python(repo: &Path, part: &Path, roots: &[PathBuf]) -> Result<Option<String>, BuildError> {
    let bin = resolve_bin(
        "pyrefly-index",
        roots,
        "安裝：uv tool install pyrefly-producer（或 cargo install --path crates/pyrefly-producer）",
    )
    .map_err(|e| BuildError::Env(format!("{e}\n")))?;
    let version = first_output_line(&bin, &["--version"]);
    let out = Command::new(&bin)
        .arg("--repo")
        .arg(repo)
        .arg("--out")
        .arg(part)
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
    Ok(version)
}

/// Rust leg, staged into an explicit partial (unchanged shape from the
/// pre-S1 two-leg era: `--output` + the empty-index guard).
fn stage_rust(repo: &Path, part: &Path, roots: &[PathBuf]) -> Result<Option<String>, BuildError> {
    let bin = resolve_bin(
        "rust-analyzer",
        roots,
        "安裝：rustup component add rust-analyzer",
    )
    .map_err(|e| BuildError::Env(format!("{e}\n")))?;
    let version = first_output_line(&bin, &["--version"]);
    // current_dir pins the rustup proxy's toolchain resolution to the
    // repo (cwd-based proxy trap); the repo DIRECTORY (not Cargo.toml)
    // is the verified CLI shape.
    let out = Command::new(&bin)
        .arg("scip")
        .arg(repo)
        .arg("--output")
        .arg(part)
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
    validate_partial(part)?;
    Ok(version)
}

/// Best-effort removal of THIS build's staged artifacts only
/// (attempt-keyed names + its stage dir); never touches unrelated
/// sidecar files or another attempt's artifacts.
fn cleanup_staged(slot_dir: &Path, attempt: &str) {
    let part_prefix = format!(".part-{attempt}-");
    let publish_tmp = format!(".publish-{attempt}.scip");
    if let Ok(entries) = std::fs::read_dir(slot_dir) {
        for ent in entries.flatten() {
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with(&part_prefix) || name == publish_tmp {
                let _ = std::fs::remove_file(ent.path());
            }
        }
    }
    let _ = std::fs::remove_dir_all(slot_dir.join(format!(".stage-{attempt}")));
}

/// Validate the candidate then publish the live slot exactly once
/// (atomic sibling rename — concurrent readers never see a torn index).
/// Single leg: the validated part renames onto the slot. Multi-leg:
/// deterministic `ORDERED` concatenation into a sibling tmp, parse the
/// merged candidate, rename. Failure at any point leaves the old slot
/// untouched.
fn merge_and_publish(staged: &[StagedLeg], slot: &Path, attempt: &str) -> Result<(), BuildError> {
    if staged.len() == 1 {
        crate::engine::load_index(&staged[0].path)
            .map_err(|e| BuildError::Env(format!("partial 驗證失敗：{e}")))?;
        return std::fs::rename(&staged[0].path, slot).map_err(|e| {
            BuildError::Core(format!(
                "rename {} → {} 失敗：{e}",
                staged[0].path.display(),
                slot.display()
            ))
        });
    }
    let mut bytes: Vec<u8> = Vec::new();
    for family in ProducerFamily::ORDERED {
        if let Some(leg) = staged.iter().find(|l| l.family == family) {
            let b = std::fs::read(&leg.path)
                .map_err(|e| BuildError::Core(format!("讀 {} 失敗：{e}", leg.path.display())))?;
            bytes.extend_from_slice(&b);
        }
    }
    let tmp = slot.with_file_name(format!(".publish-{attempt}.scip"));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| BuildError::Core(format!("寫 {} 失敗：{e}", tmp.display())))?;
    if let Err(e) = crate::engine::load_index(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(BuildError::Env(format!("merged candidate 驗證失敗：{e}")));
    }
    std::fs::rename(&tmp, slot).map_err(|e| {
        BuildError::Core(format!(
            "rename {} → {} 失敗：{e}",
            tmp.display(),
            slot.display()
        ))
    })
}

/// Core orchestration. `roots` is the bin-search path list (injectable
/// for tests); `producer` overrides the detected face set with exactly
/// one family (a deliberate partial face — omitted detected families are
/// reported, never silently dropped).
pub fn build_repo(
    repo: &Path,
    producer: Option<ProducerFamily>,
    roots: &[PathBuf],
) -> Result<Report, BuildError> {
    let resolved = resolve_repo(repo);
    if !resolved.is_dir() {
        return Err(BuildError::Env(format!(
            "--repo {} 不是目錄——請確認路徑（不建立目錄）",
            repo.display()
        )));
    }
    let inventory = count_sources(&resolved).map_err(BuildError::Core)?;
    let detected = detect_families(&inventory);
    let selected: Vec<ProducerFamily> = match producer {
        None => ProducerFamily::ORDERED
            .into_iter()
            .filter(|f| detected.contains(f))
            .collect(),
        Some(f) => vec![f],
    };
    if selected.is_empty() {
        return Err(BuildError::Env(
            "找不到可索引原始碼（.py／.rs／.js/.jsx/.mjs/.cjs/.ts/.tsx）——build 需要至少一種語言面"
                .to_string(),
        ));
    }
    let slot_dir = resolved.join(".code-reality").join("scip");
    std::fs::create_dir_all(&slot_dir)
        .map_err(|e| BuildError::Env(format!("建立 {} 失敗：{e}", slot_dir.display())))?;
    // gitignore from the earliest write window (a failed leg must not
    // leave an untracked data dir behind — sidecar_migrate precedent)
    crate::engine::write_data_dir_gitignore(&resolved.join(".code-reality"))
        .map_err(BuildError::Env)?;
    let slot = default_index_path(&resolved).map_err(BuildError::Core)?;
    let slot_existed = slot.exists();
    // Per-attempt staging identity: pid + process-local counter. PID
    // alone collides for same-process concurrent builds (MCP
    // spawn_blocking runs two builds in one daemon — a shared
    // `.part-<pid>` namespace lets them overwrite/rename/clean each
    // other's partials, S1 EP "unique to one build attempt"). With
    // unique names, racing builds only race the final rename — last
    // writer publishes its own COMPLETE merged index, never a torn or
    // foreign-partial slot.
    static ATTEMPT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let attempt = format!(
        "{pid}-{}",
        ATTEMPT.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        pid = std::process::id()
    );
    let stage_dir = slot_dir.join(format!(".stage-{attempt}"));

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
        source_identity: None,
    };
    // Effective exclusion set with provenance (additive semantics): the
    // first note, so every report — success or empty-convergence — states
    // what was filtered and where each prefix came from. A malformed
    // profile is not reported here; the legs/walk fail loud with it.
    if let Ok(profile) = crate::profile::load_profile(&resolved) {
        let eff = crate::profile::effective_excludes(profile.as_ref());
        rep.notes.push(format!(
            "排除集：{}",
            eff.iter()
                .map(|(e, s)| format!("{e}（{s}）"))
                .collect::<Vec<_>>()
                .join("、")
        ));
    }

    // ---- stage every requested leg (ORDERED); a later-leg failure must
    // leave the pre-build live slot byte-identical ----
    let mut staged: Vec<StagedLeg> = Vec::new();
    // Did every selected leg resolve to an intentionally-empty governed
    // corpus (profile exclusion)? The legal empty-convergence terminal
    // state (codex blocker 1: an all-excluded corpus must be able to
    // CONVERGE — erroring forever leaves a stale index/graph served as
    // fresh-adjacent forever).
    let mut all_policy_empty = !selected.is_empty();
    for family in &selected {
        let part = slot_dir.join(format!(".part-{attempt}-{}.scip", family.cli_name()));
        match family {
            ProducerFamily::Python => match stage_python_leg(&resolved, &part, roots) {
                Ok(PyProduceOutcome::SkippedByProfile) => {
                    rep.notes.push(
                        "python 語言面：governed 語料為空（profile exclude）——略過此腿".to_string(),
                    );
                    continue;
                }
                Ok(PyProduceOutcome::Staged {
                    producer_version,
                    indexed_docs,
                    missing_governed,
                }) => {
                    all_policy_empty = false;
                    if let Some(v) = producer_version {
                        rep.producers.push(format!("pyrefly-index {v}"));
                    }
                    rep.notes
                        .push(format!("python 腿：{indexed_docs} governed 文檔"));
                    if !missing_governed.is_empty() {
                        rep.notes.push(format!(
                            "[WARN] python 腿未索引 {} 個 governed 文檔（producer 漏產——本次發佈成功但語料不全，例：{}）",
                            missing_governed.len(),
                            missing_governed
                                .iter()
                                .take(3)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("、")
                        ));
                    }
                }
                Err(e) => {
                    cleanup_staged(&slot_dir, &attempt);
                    return Err(e);
                }
            },
            ProducerFamily::Rust => match stage_rust(&resolved, &part, roots) {
                Ok(version) => {
                    all_policy_empty = false;
                    if let Some(v) = version {
                        rep.producers.push(v);
                    }
                }
                Err(e) => {
                    cleanup_staged(&slot_dir, &attempt);
                    return Err(e);
                }
            },
            ProducerFamily::TypeScript => {
                match crate::ts_producer::stage_typescript_leg(&resolved, &stage_dir, &part, roots)
                {
                    Ok(crate::ts_producer::TsProduceOutcome::SkippedByProfile) => {
                        rep.notes.push(
                            "typescript 語言面：governed 語料為空（profile exclude）——略過此腿"
                                .to_string(),
                        );
                        continue;
                    }
                    Ok(crate::ts_producer::TsProduceOutcome::Staged {
                        producer_version,
                        mode,
                        indexed_docs,
                    }) => {
                        all_policy_empty = false;
                        // No 128B size guard here: that heuristic is
                        // rust-analyzer-specific calibration (the
                        // Cargo.toml metadata-only trap); the TS leg's
                        // own validation is stronger and exact (parse +
                        // governed doc-set), and a legal tiny JS corpus
                        // produces a small-but-valid index.
                        if let Some(v) = producer_version {
                            rep.producers.push(format!("scip-typescript {v}"));
                        }
                        rep.notes.push(format!(
                            "typescript 腿：{mode} config、{indexed_docs} governed 文檔"
                        ));
                    }
                    Err(e) => {
                        cleanup_staged(&slot_dir, &attempt);
                        return Err(BuildError::Env(e));
                    }
                }
            }
        }
        staged.push(StagedLeg {
            family: *family,
            path: part,
        });
    }
    if staged.is_empty() {
        if producer.is_none() && all_policy_empty {
            // Empty convergence: auto-detection selected only faces whose
            // governed corpus is entirely profile-excluded — the truthful
            // state is NO index. Remove the slot and graph so freshness
            // converges (next check: Fresh) instead of serving a stale
            // corpus forever. An EXPLICIT override onto an empty face
            // stays a loud error (the operator asked for that face).
            cleanup_staged(&slot_dir, &attempt);
            let _ = std::fs::remove_file(crate::cache::sqlite_path(&slot));
            let _ = std::fs::remove_file(crate::engine::meta_path(&slot));
            let _ = std::fs::remove_file(crate::fndefs::fndefs_path(&slot));
            // D12 second half: the identity cache is NOT on the
            // superseded-sidecar list at the publish point (its entries
            // self-validate via the stat gate), but the empty terminal
            // state removes the whole data plane — the cache goes too.
            let _ = std::fs::remove_file(crate::identity::cache_path_for_slot(&slot));
            let _ = std::fs::remove_file(&slot);
            let _ = std::fs::remove_file(graph_db::db_path(&resolved));
            rep.face = "empty(profile-excluded)".to_string();
            rep.graph_rebuilt = false; // the graph is REMOVED, not rebuilt
            rep.notes.push(
                "governed 語料全數被 profile 排除——收斂為空（index 與 graph 已移除）".to_string(),
            );
            let _ = std::fs::remove_file(churn_marker(&slot));
            return Ok(rep);
        }
        cleanup_staged(&slot_dir, &attempt);
        return Err(BuildError::Env(
            "選定的語言面均未產出索引（governed 語料為空或 producer 略過）".to_string(),
        ));
    }

    // ---- publish exactly once, after every requested leg validated ----
    if let Err(e) = merge_and_publish(&staged, &slot, &attempt) {
        cleanup_staged(&slot_dir, &attempt);
        return Err(e);
    }
    cleanup_staged(&slot_dir, &attempt);
    if staged.len() == 1 {
        rep.face = staged[0].family.face_name().to_string();
    } else {
        let names: Vec<&str> = ProducerFamily::ORDERED
            .iter()
            .filter(|f| staged.iter().any(|l| l.family == **f))
            .map(|f| f.cli_name())
            .collect();
        rep.face = format!("mixed({})", names.join("+"));
    }
    // Superseded derived sidecars of the live slot: the producers'
    // invalidation contract, applied at the publish point (a concurrent
    // query could otherwise build a cache db newer than the new slot and
    // relay the OLD corpus as fresh). Meta is restamped right below.
    let _ = std::fs::remove_file(crate::cache::sqlite_path(&slot));
    let _ = std::fs::remove_file(crate::fndefs::fndefs_path(&slot));
    if slot_existed {
        rep.notes
            .push("覆蓋既有 index.scip（slot 單檔——先前面目已取代）".to_string());
    }

    // Stamp index provenance in-process with the legs that actually ran
    // (face-accurate producer string; relay Finding B's no-unstamped-slot
    // goal — direct lib call, no cli::run indirection so the query-path
    // heal hook cannot re-enter at all). Selection mode: explicit
    // override pins the face scope; auto lets freshness union with
    // newly detected faces (muse P0-1).
    let producer_str = rep.producers.join("; ");
    if let Err(e) = crate::engine::stamp_meta_core(
        &resolved,
        &slot,
        roots,
        if producer_str.is_empty() {
            None
        } else {
            Some(&producer_str)
        },
        Some(if producer.is_some() {
            "explicit"
        } else {
            "auto"
        }),
    ) {
        rep.notes.push(format!(
            "stamp-meta 失敗（{e}）——手動補：code-reality scip_refs --repo {} --stamp-meta",
            resolved.display()
        ));
    }
    // Report what was ACTUALLY stamped (read-back, not a prediction) —
    // None on a legacy/preserved stamp.
    rep.source_identity = crate::engine::load_meta(&slot)
        .0
        .and_then(|m| m["source_identity"].as_str().map(str::to_string));

    if let Some(sel) = producer {
        let omitted: Vec<&str> = ProducerFamily::ORDERED
            .iter()
            .filter(|f| detected.contains(f) && **f != sel)
            .map(|f| f.cli_name())
            .collect();
        if !omitted.is_empty() {
            rep.notes
                .push(format!("未索引：{}（--producer 切換）", omitted.join("、")));
        }
    }
    rep.notes
        .push("全量重產：producer 每次重建（冪等）".to_string());

    let g = graph_db::build_from_cache_at(&resolved, &slot).map_err(BuildError::Core)?;
    rep.nodes = g.nodes;
    rep.edges = g.edges;
    let ir = graph_db::ensure_indexes(&resolved).map_err(BuildError::Core)?;
    rep.indexes_created = ir.created;
    rep.indexes_skipped = ir.skipped;
    // A converged explicit build ends any churn window — later queries
    // heal normally (run_heal_locked clears this too; idempotent).
    let _ = std::fs::remove_file(churn_marker(&slot));
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
/// Churn cooldown default (AIR-33 ③): while an active writer keeps
/// sources newer than the slot, every query would re-burn a
/// minutes-scale heal that cannot converge anyway (its output is stale
/// the moment it lands). See `churn_cooldown_active`.
const HEAL_CHURN_COOLDOWN: Duration = Duration::from_secs(600);

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

/// Flagged-path-only producer drift notes (the steady-state query path
/// is zero-spawn by rule): stamped producer vs installed — an upgrade
/// signal, never a rebuild trigger (rebuilding with the same stale
/// producer changes nothing). Covers the two strict-compare producers:
/// `pyrefly-index` and `scip-typescript` (S4 generalization);
/// rust-analyzer stays excluded — its version floats with the toolchain.
/// Crate-internal: exercised through the heal outcomes that surface it.
pub(crate) fn producer_drift_notes(repo: &Path, slot: &Path, roots: &[PathBuf]) -> Vec<String> {
    let Some(stamped) = crate::engine::load_meta(slot)
        .0
        .and_then(|m| m["producer"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty() && s != "<unresolved>")
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let segments: Vec<&str> = stamped.split("; ").collect();
    if let Some(stamped_py) = segments.iter().find(|s| s.starts_with("pyrefly-index ")) {
        let stamped_v = stamped_py
            .strip_prefix("pyrefly-index ")
            .unwrap_or(stamped_py);
        if let Some(current) = crate::common::producer_version("pyrefly-index", roots) {
            if stamped_v != current {
                out.push(format!(
                    "[WARN] producer 版本錯配（stamp pyrefly-index {stamped_v} ≠ 現裝 {current}）——升級：uv tool install -U pyrefly-producer\n"
                ));
            }
        }
    }
    if let Some(stamped_ts) = segments.iter().find(|s| s.starts_with("scip-typescript ")) {
        let stamped_v = stamped_ts
            .strip_prefix("scip-typescript ")
            .unwrap_or(stamped_ts);
        let ts_roots = crate::ts_producer::node_tool_roots(repo, roots);
        if let Some(current) = crate::common::producer_version("scip-typescript", &ts_roots) {
            if stamped_v != current {
                out.push(format!(
                    "[WARN] producer 版本錯配（stamp scip-typescript {stamped_v} ≠ 現裝 {current}）——升級：npm install --save-dev @sourcegraph/scip-typescript（repo-local node_modules/.bin 優先）——或全域安裝後將 bin 目錄加入 PATH／CODE_REALITY_NODE_BIN_DIR\n"
                ));
            }
        }
    }
    out
}

// ---------- churn cooldown (AIR-33 ③) ----------
//
// The mtime staleness signal is true on EVERY query while a writer keeps
// editing — each full heal is minutes-scale on real corpora and cannot
// converge under continuous edits (its output is stale the moment it
// lands), so a heal that finishes and STILL finds sources newer than the
// slot (the SM-9 loop guard) arms a cooldown marker. Later queries inside
// the window serve the existing index with a WARN instead of re-burning
// the heal. A head drift (commit boundary) always overrides — commits are
// convergence points. Escape hatch: CODE_REALITY_HEAL_COOLDOWN_SECS=0.

fn churn_cooldown_secs() -> u64 {
    std::env::var("CODE_REALITY_HEAL_COOLDOWN_SECS")
        .ok()
        .and_then(|v| match v.parse::<u64>() {
            Ok(secs) => Some(secs),
            Err(_) => {
                eprintln!(
                    "[WARN] CODE_REALITY_HEAL_COOLDOWN_SECS 無法解析（{v}）——採用預設 {}s",
                    HEAL_CHURN_COOLDOWN.as_secs()
                );
                None
            }
        })
        .unwrap_or(HEAL_CHURN_COOLDOWN.as_secs())
}

/// Slot-sibling marker (`.heal-churn`); its mtime is the arming
/// timestamp — the same mtime-as-clock idiom as the abandoned-lock age
/// check.
fn churn_marker(slot: &Path) -> PathBuf {
    slot.parent()
        .unwrap_or_else(|| Path::new(""))
        .join(".heal-churn")
}

fn churn_cooldown_active(slot: &Path) -> bool {
    let secs = churn_cooldown_secs();
    if secs == 0 {
        return false;
    }
    churn_marker(slot)
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() < secs)
}

/// Rewriting an already-fresh marker refreshes the window — protection
/// extends while churn keeps failing to converge.
fn write_churn_marker(slot: &Path) {
    let _ = std::fs::write(churn_marker(slot), b"");
}

fn churn_serve_stale(repo: &Path, slot: &Path, roots: &[PathBuf]) -> HealOutcome {
    let mut lines = vec![
        "[WARN] 活躍編輯中（上次癒合未能收斂）——cooldown 內跳過重癒，本次查詢以現存索引作答\n"
            .to_string(),
    ];
    lines.extend(producer_drift_notes(repo, slot, roots));
    HealOutcome::ServeStale(lines)
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
    let snap = crate::engine::evaluate_staleness(repo, slot, crate::identity::IdentityCachePolicy::WriteBack)?;
    if !snap.needs_rebuild() {
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
                lines.extend(producer_drift_notes(repo, slot, roots));
            }
            Ok(out)
        }
        Ok(rep) => {
            // Empty convergence (all faces policy-excluded) removes the
            // slot — absence IS the converged state; evaluate_staleness
            // would stat-fail on the missing slot.
            if !slot.exists() {
                return Ok(HealOutcome::Healed {
                    secs: t0.elapsed().as_secs_f64(),
                    nodes: rep.nodes,
                    notes: rep.notes,
                });
            }
            // Loop guard (SM-9): a rebuild that still leaves the slot
            // behind warns once and serves — never loops.
            let snap = crate::engine::evaluate_staleness(repo, slot, crate::identity::IdentityCachePolicy::WriteBack)?;
            let doc_delta = match crate::engine::load_index(slot) {
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
            // Fingerprint-only drift (no newer mtime) gets the PRECISE
            // diagnosis first: the producer corpus vs disk mismatch —
            // the generic "sources changed during heal" wording would
            // misdescribe a persistently-omitting producer.
            if !snap.source_newer && snap.needs_rebuild() {
                if let Some(d) = &doc_delta {
                    if d.missing > 0 || d.extra > 0 {
                        write_churn_marker(slot);
                        let mut lines = vec![format!(
                            "[WARN] 偵測與 producer 語料不一致（false-stale：missing={}，extra={}，例：{}）——不迴圈，本次查詢以現存索引作答（cooldown 內後續查詢不再重癒）\n",
                            d.missing, d.extra, d.examples.join("、")
                        )];
                        lines.extend(producer_drift_notes(repo, slot, roots));
                        return Ok(HealOutcome::ServeStale(lines));
                    }
                }
            }
            if snap.needs_rebuild() {
                // Failed convergence — arm the churn cooldown so the next
                // query doesn't re-burn a minutes-scale heal into the same
                // non-convergence (AIR-33 ③).
                write_churn_marker(slot);
                let mut lines = vec![
                    "[WARN] heal 期間原始碼又變動——本次查詢以現存索引作答（cooldown 內後續查詢不再重癒）\n"
                        .to_string(),
                ];
                lines.extend(producer_drift_notes(repo, slot, roots));
                return Ok(HealOutcome::ServeStale(lines));
            }
            // Converged — any churn window is over; re-arm healing.
            let _ = std::fs::remove_file(churn_marker(slot));
            // Residual delta on a mtime-converged rebuild: the indexed
            // corpus still disagrees with disk. Reachable only in the
            // LEGACY keyless-meta shape (with fingerprint keys this
            // fires in the precise branch above) — arm the marker there
            // too, or the next query would call this index Fresh
            // (codex P0-4). Serve with the warning — never loop.
            if let Some(d) = &doc_delta {
                if d.missing > 0 || d.extra > 0 {
                    write_churn_marker(slot);
                    let mut lines = vec![format!(
                        "[WARN] 偵測與 producer 語料不一致（false-stale：missing={}，extra={}，例：{}）——不迴圈，本次查詢以現存索引作答\n",
                        d.missing,
                        d.extra,
                        d.examples.join("、")
                    )];
                    lines.extend(producer_drift_notes(repo, slot, roots));
                    return Ok(HealOutcome::ServeStale(lines));
                }
            }
            let notes = producer_drift_notes(repo, slot, roots);
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
            let snap = crate::engine::evaluate_staleness(repo, slot, crate::identity::IdentityCachePolicy::WriteBack)?;
            // churn guard BEFORE the HealedByPeer return — a peer whose
            // heal failed to converge (armed marker) must not be reported
            // as having fixed it (codex P0-4)
            if snap.head_drift != Some(true) && churn_cooldown_active(slot) {
                return Ok(churn_serve_stale(repo, slot, roots));
            }
            if !snap.needs_rebuild() {
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
            lines.extend(producer_drift_notes(repo, slot, roots));
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
    let snap =
        crate::engine::evaluate_staleness(&repo, &slot, crate::identity::IdentityCachePolicy::WriteBack)?;
    // Churn guard (AIR-33 ③ + codex P0-4): an armed marker means the
    // last heal FAILED TO CONVERGE — checked BEFORE any Fresh
    // short-circuit, because a non-converged heal can leave the slot
    // mtime-fresh with no fingerprint keys (the very state the marker
    // exists to remember). A drifted head (commit boundary) overrides:
    // commits are the stable points worth converging on.
    if snap.head_drift != Some(true) && churn_cooldown_active(&slot) {
        return Ok(churn_serve_stale(&repo, &slot, roots));
    }
    if !snap.needs_rebuild() {
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
            lines.extend(producer_drift_notes(&repo, &slot, roots));
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
            "source_identity": rep.source_identity,
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
    let producer_family = producer.as_deref().map(ProducerFamily::parse_cli);
    if let Some(None) = producer_family {
        return ToolOutput::fail(format!(
            "--producer 需為 rust、python 或 typescript（收到：{}）",
            producer.unwrap_or_default()
        ));
    }
    match build_repo(
        Path::new(&repo),
        producer_family.flatten(),
        &producer_roots(),
    ) {
        Ok(rep) => render(rep, json),
        Err(e) => match e {
            BuildError::Env(m) => ToolOutput::fail(format!("build: {m}")),
            BuildError::Core(m) => ToolOutput::crash(format!("build: {m}")),
        },
    }
}
