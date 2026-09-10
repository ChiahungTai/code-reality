//! ts_producer — the `scip-typescript` producer leg (S2).
//!
//! Indexes the governed six-extension corpus without mutating the
//! target repository: an existing root TS project config gets the first
//! opportunity (compiler/module/workspace semantics stay repo-owned),
//! and the fallback is a CR-owned sidecar config whose explicit absolute
//! `files` list IS the governed corpus (POC: glob/include inference
//! omits `.jsx`; explicit `files` indexes all six extensions exactly).
//!
//! Resolution is deterministic and offline: repo-local
//! `node_modules/.bin` → `CODE_REALITY_NODE_BIN_DIR` → the base roots.
//! No `npx -y`, no implicit download, no `npm prefix` spawn (AD-5).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use protobuf::Message;
use scip::types::Index;

use crate::js_ts_corpus::{collect_js_ts_corpus, normalize_rel};

const INSTALL_HINT: &str = "安裝：npm install --save-dev @sourcegraph/scip-typescript（repo-local node_modules/.bin 優先）——或全域安裝後將 bin 目錄加入 PATH／CODE_REALITY_NODE_BIN_DIR";
const NODE_HINT: &str = "scip-typescript 需要 Node >= 18 執行環境（npm 套件）——請先安裝 Node";

/// Outcome of the TypeScript producer leg.
#[derive(Debug)]
pub enum TsProduceOutcome {
    /// Raw detection saw JS/TS files but the governed (profile-filtered)
    /// corpus is empty — the leg is intentionally absent, not a failure
    /// (S2 SM-6: no empty-index false error).
    SkippedByProfile,
    /// Validated, governed-filtered partial written to the staged path.
    Staged {
        producer_version: Option<String>,
        /// "existing" (repo project config) or "derived" (CR sidecar).
        mode: &'static str,
        indexed_docs: usize,
    },
}

/// Node-tool bin search roots: repo-local `node_modules/.bin` first
/// (project-local installs win), then the explicit GUI-safe
/// `CODE_REALITY_NODE_BIN_DIR` entries, then the caller's base roots.
/// Deterministic order, stable dedup.
pub fn node_tool_roots(repo: &Path, base_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = vec![repo.join("node_modules").join(".bin")];
    if let Some(explicit) = std::env::var_os("CODE_REALITY_NODE_BIN_DIR") {
        roots.extend(std::env::split_paths(&explicit));
    }
    roots.extend(base_roots.iter().cloned());
    let mut seen = std::collections::BTreeSet::new();
    roots.retain(|r| seen.insert(r.clone()));
    roots
}

/// Root project config candidates, in preference order (bounded rule:
/// root configs only — workspace/package discovery flags are not enabled
/// until verified; a workspace repo without a root config takes the
/// derived explicit-files path).
fn find_root_project(repo: &Path) -> Option<PathBuf> {
    for name in ["tsconfig.json", "jsconfig.json"] {
        let p = repo.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// `--max-file-byte-size` covers the largest governed file so the
/// producer's 1mb default cannot silently drop corpus documents (the
/// derived-mode exact-set check would fail loud; this avoids tripping it
/// on legitimate large files). Ceiling at 1gb.
fn max_file_size_arg(repo: &Path, corpus: &BTreeSet<String>) -> Result<String, String> {
    let mut max: u64 = 0;
    for rel in corpus {
        let len = std::fs::metadata(repo.join(rel))
            .map_err(|e| format!("stat {} 失敗：{e}", rel))?
            .len();
        max = max.max(len);
    }
    let mb = (max.div_ceil(1024 * 1024)).clamp(1, 1024);
    Ok(format!("{mb}mb"))
}

fn write_derived_config(
    stage_dir: &Path,
    repo: &Path,
    corpus: &BTreeSet<String>,
) -> Result<PathBuf, String> {
    let cfg = serde_json::json!({
        "compilerOptions": {
            "allowJs": true,
            "checkJs": false,
            "module": "NodeNext",
            "moduleResolution": "NodeNext",
            "target": "ES2022",
            "jsx": "preserve",
            "noEmit": true
        },
        "files": corpus.iter()
            .map(|rel| repo.join(rel).display().to_string())
            .collect::<Vec<String>>()
    });
    let path = stage_dir.join("cr-tsconfig.json");
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&cfg).unwrap()),
    )
    .map_err(|e| format!("寫入 derived config 失敗（{}）：{e}", path.display()))?;
    Ok(path)
}

/// Narrow seam around the external `scip-typescript` process. The
/// producer policy (corpus/config/fallback/filtering) stays in this
/// module; tests can replace only the process boundary instead of
/// growing an argv-parsing subprocess runtime of their own.
trait IndexerRunner {
    fn version(&self, bin: &Path) -> Option<String>;

    fn run(
        &self,
        bin: &Path,
        repo: &Path,
        config: &Path,
        candidate: &Path,
        max_bytes: &str,
    ) -> Result<(), String>;
}

struct ProcessIndexer;

impl IndexerRunner for ProcessIndexer {
    fn version(&self, bin: &Path) -> Option<String> {
        crate::common::first_output_line(bin, &["--version"])
    }

    fn run(
        &self,
        bin: &Path,
        repo: &Path,
        config: &Path,
        candidate: &Path,
        max_bytes: &str,
    ) -> Result<(), String> {
        let out = std::process::Command::new(bin)
            .arg("index")
            .arg("--cwd")
            .arg(repo)
            .arg(config)
            .arg("--no-progress-bar")
            .arg("--max-file-byte-size")
            .arg(max_bytes)
            .arg("--output")
            .arg(candidate)
            .output()
            .map_err(|e| format!("spawn {} 失敗：{e}", bin.display()))?;
        if !out.status.success() {
            return Err(format!(
                "scip-typescript 失敗（{}）：\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    }
}

/// The pinned zero-file condition (POC: `error: no files got indexed`,
/// exit 1) — the one existing-config outcome allowed to fall back to the
/// derived config. Any other producer/config/compiler error fails loud;
/// fallback must never mask a malformed real project (S2 SM-9).
fn is_zero_file_failure(err: &str) -> bool {
    err.contains("no files got indexed")
}

/// Append the Node prerequisite hint when the failure smells like a
/// missing/unusable Node runtime (npm shims exec `node`; a missing node
/// surfaces as env/spawn 127 noise rather than a compiler error).
fn augment_node_hint(mut err: String) -> String {
    if err.contains("node:") || err.contains("env: node") || err.contains("No such file") {
        err.push_str(NODE_HINT);
        err.push('\n');
    }
    err
}

/// Decode `candidate`, retain exactly the governed documents, serialize
/// to `out`. Returns the retained count and the governed files the
/// candidate did NOT carry (empty = exact coverage).
fn filter_to_governed(
    candidate: &Path,
    governed: &BTreeSet<String>,
    out: &Path,
) -> Result<(usize, Vec<String>), String> {
    let bytes =
        std::fs::read(candidate).map_err(|e| format!("讀 {} 失敗：{e}", candidate.display()))?;
    let mut index = Index::parse_from_bytes(&bytes).map_err(|e| {
        format!(
            "scip-typescript 產出無法解析（{}）：{e}",
            candidate.display()
        )
    })?;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    index
        .documents
        .retain(|d| governed.contains(&normalize_rel(&d.relative_path)));
    for d in &index.documents {
        seen.insert(normalize_rel(&d.relative_path));
    }
    let missing: Vec<String> = governed
        .iter()
        .filter(|g| !seen.contains(*g))
        .cloned()
        .collect();
    let kept = index.documents.len();
    let payload = index
        .write_to_bytes()
        .map_err(|e| format!("SCIP 序列化失敗：{e}"))?;
    std::fs::write(out, payload).map_err(|e| format!("寫 {} 失敗：{e}", out.display()))?;
    Ok((kept, missing))
}

/// Stage the TypeScript leg. `stage_dir` is this build's CR-owned
/// staging directory (config + raw candidate live and die there); the
/// filtered partial lands on `part` for S1's merge/publish.
pub fn stage_typescript_leg(
    repo: &Path,
    stage_dir: &Path,
    part: &Path,
    roots: &[PathBuf],
) -> Result<TsProduceOutcome, String> {
    stage_typescript_leg_with(repo, stage_dir, part, roots, &ProcessIndexer)
}

fn stage_typescript_leg_with<R: IndexerRunner>(
    repo: &Path,
    stage_dir: &Path,
    part: &Path,
    roots: &[PathBuf],
    runner: &R,
) -> Result<TsProduceOutcome, String> {
    let corpus = collect_js_ts_corpus(repo)?;
    if corpus.files.is_empty() {
        return Ok(TsProduceOutcome::SkippedByProfile);
    }
    let ts_roots = node_tool_roots(repo, roots);
    let bin = resolve_bin_with_hint(&ts_roots).map_err(|e| format!("{e}\n"))?;
    let version = runner.version(&bin);
    std::fs::create_dir_all(stage_dir)
        .map_err(|e| format!("建立 {} 失敗：{e}", stage_dir.display()))?;
    let max_bytes = max_file_size_arg(repo, &corpus.files)?;
    let candidate = stage_dir.join("candidate.scip");

    let existing = find_root_project(repo);
    let mut mode: &'static str = if existing.is_some() {
        "existing"
    } else {
        "derived"
    };
    // Existing → derived is a bounded one-shot fallback; derived never
    // falls back again (its files list IS the corpus authority).
    for _ in 0..2 {
        let config = match mode {
            "existing" => existing.clone().expect("existing mode implies a project"),
            _ => write_derived_config(stage_dir, repo, &corpus.files)?,
        };
        if let Err(run_err) = runner.run(&bin, repo, &config, &candidate, &max_bytes) {
            if mode == "existing" && is_zero_file_failure(&run_err) {
                mode = "derived";
                continue;
            }
            return Err(augment_node_hint(run_err));
        }
        let (kept, missing) = filter_to_governed(&candidate, &corpus.files, part)?;
        if mode == "existing" && (kept == 0 || !missing.is_empty()) {
            // Usable-config predicate (parent EP): an existing project is
            // authoritative only when it COVERS the governed corpus. A
            // partial corpus (kept ≥ 1 but missing > 0) would publish an
            // index whose doc set ⊊ governed disk — the S4 fingerprint
            // contract can never hold, degrading freshness to mtime-only
            // (delete/rename blind spot) and making every heal land in
            // the missing>0 non-convergence branch. Fall back to the
            // derived config, which must converge exactly.
            mode = "derived";
            continue;
        }
        if kept == 0 {
            return Err(format!(
                "scip-typescript derived config 索引 0 個 governed 文檔（語料 {} 檔）",
                corpus.files.len()
            ));
        }
        if !missing.is_empty() {
            return Err(format!(
                "derived 語料集不符：indexed {kept} ≠ governed {}（例缺：{}）",
                corpus.files.len(),
                missing
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("、")
            ));
        }
        // Postcondition: parseable, non-empty, and every retained doc
        // belongs to the governed set (re-read from the staged file —
        // trust the bytes that will be merged, not the in-memory copy).
        let bytes = std::fs::read(part).map_err(|e| format!("讀 {} 失敗：{e}", part.display()))?;
        let index = Index::parse_from_bytes(&bytes)
            .map_err(|e| format!("staged partial 無法解析（{}）：{e}", part.display()))?;
        if index.documents.is_empty() {
            return Err("staged partial 0 文檔".to_string());
        }
        for d in &index.documents {
            if !corpus.files.contains(&normalize_rel(&d.relative_path)) {
                return Err(format!(
                    "staged partial 含非 governed 文檔：{}",
                    d.relative_path
                ));
            }
        }
        return Ok(TsProduceOutcome::Staged {
            producer_version: version,
            mode,
            indexed_docs: kept,
        });
    }
    Err("scip-typescript 內部錯誤：fallback 迴圈未收斂".to_string())
}

fn resolve_bin_with_hint(roots: &[PathBuf]) -> Result<PathBuf, String> {
    crate::common::resolve_bin("scip-typescript", roots, INSTALL_HINT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::Message;
    use scip::types::{Document, Occurrence};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::os::unix::fs::PermissionsExt;

    #[derive(Clone, Debug)]
    enum FakeReply {
        Docs(Vec<String>),
        Err(String),
    }

    #[derive(Default)]
    struct FakeIndexer {
        replies: RefCell<VecDeque<FakeReply>>,
        configs: RefCell<Vec<PathBuf>>,
    }

    impl FakeIndexer {
        fn with(replies: impl IntoIterator<Item = FakeReply>) -> Self {
            Self {
                replies: RefCell::new(replies.into_iter().collect()),
                configs: RefCell::new(Vec::new()),
            }
        }

        fn run_count(&self) -> usize {
            self.configs.borrow().len()
        }
    }

    impl IndexerRunner for FakeIndexer {
        fn version(&self, _bin: &Path) -> Option<String> {
            Some("0.4.0-in-process".to_string())
        }

        fn run(
            &self,
            _bin: &Path,
            _repo: &Path,
            config: &Path,
            candidate: &Path,
            _max_bytes: &str,
        ) -> Result<(), String> {
            self.configs.borrow_mut().push(config.to_path_buf());
            match self
                .replies
                .borrow_mut()
                .pop_front()
                .expect("fake reply for every producer run")
            {
                FakeReply::Err(e) => Err(e),
                FakeReply::Docs(paths) => {
                    let mut index = Index::new();
                    for rel in paths {
                        let mut doc = Document::new();
                        doc.relative_path = rel.clone();
                        let mut occ = Occurrence::new();
                        occ.symbol = format!(
                            "scip-typescript npm . . /fixture/`{}`/item().",
                            rel.replace('/', "_")
                        );
                        occ.symbol_roles = 1;
                        occ.range = vec![0, 0, 1];
                        doc.occurrences.push(occ);
                        index.documents.push(doc);
                    }
                    std::fs::write(candidate, index.write_to_bytes().unwrap()).unwrap();
                    Ok(())
                }
            }
        }
    }

    fn executable_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("scip-typescript");
        std::fs::write(&bin, b"fixture: never executed\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    fn stage_with(
        repo: &Path,
        runner: &FakeIndexer,
    ) -> Result<(TsProduceOutcome, tempfile::TempDir), String> {
        let root = executable_root();
        let stage = repo.join(".code-reality/test-stage");
        let part = repo.join(".code-reality/test-part.scip");
        let out =
            stage_typescript_leg_with(repo, &stage, &part, &[root.path().to_path_buf()], runner)?;
        Ok((out, root))
    }

    #[test]
    fn in_process_zero_file_existing_config_retries_derived_once() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("tsconfig.json"), "{}\n").unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
        let runner = FakeIndexer::with([
            FakeReply::Err("scip-typescript failed: no files got indexed".to_string()),
            FakeReply::Docs(vec!["src/a.ts".to_string()]),
        ]);

        let (out, _bin) = stage_with(repo.path(), &runner).unwrap();
        assert!(matches!(
            out,
            TsProduceOutcome::Staged {
                mode: "derived",
                indexed_docs: 1,
                ..
            }
        ));
        assert_eq!(runner.run_count(), 2);
        let configs = runner.configs.borrow();
        assert_eq!(configs[0], repo.path().join("tsconfig.json"));
        assert_eq!(
            configs[1].file_name().and_then(|s| s.to_str()),
            Some("cr-tsconfig.json")
        );
    }

    #[test]
    fn in_process_partial_existing_config_falls_back_to_exact_derived() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("tsconfig.json"), "{}\n").unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
        std::fs::write(repo.path().join("src/b.ts"), "export const b = 1;\n").unwrap();
        let runner = FakeIndexer::with([
            FakeReply::Docs(vec!["src/a.ts".to_string()]),
            FakeReply::Docs(vec!["src/a.ts".to_string(), "src/b.ts".to_string()]),
        ]);

        let (out, _bin) = stage_with(repo.path(), &runner).unwrap();
        assert!(matches!(
            out,
            TsProduceOutcome::Staged {
                mode: "derived",
                indexed_docs: 2,
                ..
            }
        ));
        assert_eq!(runner.run_count(), 2);
    }

    #[test]
    fn in_process_unrelated_existing_error_does_not_fallback() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("tsconfig.json"), "{}\n").unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
        let runner = FakeIndexer::with([FakeReply::Err(
            "scip-typescript failed: TS2322 compiler failure".to_string(),
        )]);

        let err = stage_with(repo.path(), &runner).unwrap_err();
        assert!(err.contains("TS2322"), "{err}");
        assert_eq!(runner.run_count(), 1, "must not take derived fallback");
    }

    #[test]
    fn in_process_filter_drops_non_governed_documents() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::create_dir_all(repo.path().join("dist")).unwrap();
        std::fs::write(repo.path().join("src/a.mjs"), "export const a = 1;\n").unwrap();
        std::fs::write(repo.path().join("src/b.ts"), "export const b = 1;\n").unwrap();
        std::fs::write(repo.path().join("dist/mirror.mjs"), "generated\n").unwrap();
        std::fs::write(
            repo.path().join(".code-reality.toml"),
            "exclude = [\"dist/\"]\n",
        )
        .unwrap();
        let runner = FakeIndexer::with([FakeReply::Docs(vec![
            "src/a.mjs".to_string(),
            "src/b.ts".to_string(),
            "dist/mirror.mjs".to_string(),
        ])]);

        let (out, _bin) = stage_with(repo.path(), &runner).unwrap();
        assert!(matches!(
            out,
            TsProduceOutcome::Staged {
                mode: "derived",
                indexed_docs: 2,
                ..
            }
        ));
        let part = repo.path().join(".code-reality/test-part.scip");
        let index = Index::parse_from_bytes(&std::fs::read(part).unwrap()).unwrap();
        let docs: BTreeSet<_> = index
            .documents
            .iter()
            .map(|d| d.relative_path.as_str())
            .collect();
        assert_eq!(docs, BTreeSet::from(["src/a.mjs", "src/b.ts"]));
    }
}
