//! Multi-Cargo-workspace root resolution (EP DB-100 S1; frozen decisions
//! Q1/Q6/Q8). One git repo may contain several independent Cargo
//! workspaces — the resolver discovers their roots so the Rust producer
//! can stage one partial per workspace.
//!
//! Two faces (frozen Q8): [`resolve_structural`] is pure TOML — the sole
//! source for the query/freshness face, it never spawns `cargo metadata`
//! (the ≈40 ms steady-state freshness gate must not regress). It reads
//! `[workspace]` table presence as the workspace-root candidate signal.
//! [`validate_authoritative`] runs `cargo metadata --no-deps` and is the
//! build face's authority (workspace_root == candidate), executed once
//! before staging; a root it rejects never enters the topology
//! descriptor, keeping the stamped and query-face root sets byte-equal.
//!
//! Zero-candidate contract (frozen Q6): `resolve_structural` returns an
//! EMPTY root set when the repo carries no Cargo.toml at all — the
//! legacy-parity fallback (treat the whole repo as the single "." root)
//! is the CALLER's decision, not the resolver's.
//!
//! Error codes CR-RUST-ROOT-01..08 are a frozen contract (EP TC-8).

use crate::profile::Profile;
use std::path::{Path, PathBuf};

/// Resolved workspace roots: repo-relative, canonical-unique,
/// non-overlapping, sorted deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProjects {
    pub roots: Vec<String>,
    /// Discovery-mode truth (P2): `"config"` when the roots came from
    /// the explicit `rust_projects` list, `"auto"` from structural
    /// discovery. The caller-side legacy-parity `["."]` fallback only
    /// ever consumes an empty root set, which config mode never
    /// produces — so the fallback keeps `"auto"` by construction.
    pub mode: &'static str,
}

fn cr_err(code: &str, msg: &str) -> String {
    format!("CR-RUST-ROOT-{code}: {msg}")
}

/// Resolve workspace roots from the structural face (pure TOML, no
/// cargo spawns). Config mode (`rust_projects` in `.code-reality.toml`)
/// is authoritative; absence falls back to conservative auto-discovery.
pub fn resolve_structural(
    repo: &Path,
    profile: Option<&Profile>,
) -> Result<RustProjects, String> {
    // Canonicalize the repo base once: containment checks (starts_with)
    // must compare against the same resolved prefix as the canonicalized
    // children — a symlinked path prefix (/var → /private/var on macOS)
    // would otherwise flag every child as an escape.
    let repo = repo
        .canonicalize()
        .map_err(|e| cr_err("02", &format!("repo root canonicalize failed: {e}")))?;
    match profile.and_then(|p| p.rust_projects.clone()) {
        Some(list) => from_config(&repo, list),
        None => auto_discover(&repo),
    }
}

/// Authoritative workspace-root check (build staging only): every root
/// must satisfy `cargo metadata` `workspace_root == candidate`. Fails
/// loud CR-RUST-ROOT-05 with the metadata stderr on mismatch — the
/// rejected root never reaches the topology descriptor.
pub fn validate_authoritative(repo: &Path, projects: &RustProjects) -> Result<(), String> {
    for root in &projects.roots {
        let dir = repo.join(root);
        let out = std::process::Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(&dir)
            .output()
            .map_err(|e| cr_err("05", &format!("rust project '{root}' cargo spawn failed: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(cr_err(
                "05",
                &format!(
                    "rust project '{root}' is not a Cargo workspace root (cargo metadata failed): {}",
                    stderr.trim()
                ),
            ));
        }
        let json: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| cr_err("05", &format!("rust project '{root}' metadata parse failed: {e}")))?;
        let ws_root = json["workspace_root"]
            .as_str()
            .ok_or_else(|| cr_err("05", &format!("rust project '{root}' metadata missing workspace_root")))?;
        let canon_ws = PathBuf::from(ws_root)
            .canonicalize()
            .map_err(|e| cr_err("05", &format!("rust project '{root}' workspace_root canonicalize failed: {e}")))?;
        let canon_dir = dir.canonicalize().map_err(|e| {
            cr_err("02", &format!("rust project '{root}' cannot canonicalize inside repo '{}': {e}", repo.display()))
        })?;
        if canon_ws != canon_dir {
            return Err(cr_err(
                "05",
                &format!(
                    "rust project '{root}' is not a Cargo workspace root (cargo workspace_root='{}')",
                    ws_root
                ),
            ));
        }
    }
    Ok(())
}

fn from_config(repo: &Path, list: Vec<String>) -> Result<RustProjects, String> {
    if list.is_empty() {
        return Err(cr_err(
            "01",
            "rust_projects is present but empty — remove the key for auto-discovery or list at least one root",
        ));
    }
    let mut canonical: Vec<(String, PathBuf)> = Vec::new();
    for entry in &list {
        let rel = Path::new(entry);
        let bad_shape = entry.is_empty()
            || rel.is_absolute()
            || entry.split('/').any(|c| c == "..")
            || entry.split('\\').any(|c| c == "..");
        if bad_shape {
            return Err(cr_err(
                "01",
                &format!("rust_projects entry '{entry}' must be a nonempty repo-relative directory ('.' allowed; absolute and '..' forbidden)"),
            ));
        }
        let joined = repo.join(rel);
        let canon = joined.canonicalize().map_err(|e| {
            cr_err(
                "02",
                &format!("rust project '{entry}' cannot canonicalize inside repo '{}': {e}", repo.display()),
            )
        })?;
        if !canon.starts_with(repo) {
            return Err(cr_err(
                "02",
                &format!("rust project '{entry}' canonicalizes outside the repo root (symlink escape)"),
            ));
        }
        let manifest = canon.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|_| {
            cr_err(
                "04",
                &format!("rust project '{entry}' missing or unreadable Cargo.toml"),
            )
        })?;
        let _parsed: toml::Value = toml::from_str(&text).map_err(|e| {
            cr_err(
                "04",
                &format!("rust project '{entry}' has an invalid Cargo.toml: {e}"),
            )
        })?;
        // The root that leaves this function is the CANONICAL
        // repo-relative rel — cosmetic spellings ("./rust", "rust/",
        // "a/./b") fold here so every downstream consumer (walk-side
        // rel prefixing, the coverage gate, the topology marker) sees
        // one normal form. The "." entry keeps its "." spelling (the
        // whole-repo root's contract shape).
        let normalized = match canon.strip_prefix(repo) {
            Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => {
                // Unreachable after the starts_with containment check
                // above (same component semantics) — fail loud anyway.
                return Err(cr_err(
                    "02",
                    &format!("rust project '{entry}' canonicalizes outside the repo root (symlink escape)"),
                ));
            }
        };
        canonical.push((normalized, canon));
    }
    canonical.sort_by(|a, b| a.1.cmp(&b.1));
    for w in canonical.windows(2) {
        if w[0].1 == w[1].1 {
            return Err(cr_err(
                "03",
                &format!(
                    "rust projects '{}' and '{}' resolve to the same directory",
                    w[0].0, w[1].0
                ),
            ));
        }
        if w[1].1.starts_with(&w[0].1) {
            return Err(cr_err(
                "03",
                &format!(
                    "rust projects '{}' and '{}' overlap (ancestor/descendant)",
                    w[0].0, w[1].0
                ),
            ));
        }
    }
    scan_unlisted_workspaces(repo, &canonical)?;
    let roots = canonical.into_iter().map(|(e, _)| e).collect();
    Ok(RustProjects {
        roots,
        mode: "config",
    })
}

/// Config-mode inventory gate: an immediate child declaring its own
/// `[workspace]` but absent from the configured list is a coverage hole
/// (adding a third sibling must not silently retain old coverage).
fn scan_unlisted_workspaces(
    repo: &Path,
    configured: &[(String, PathBuf)],
) -> Result<(), String> {
    for ent in std::fs::read_dir(repo).map_err(|e| e.to_string())?.flatten() {
        let ok = ent.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !ok {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let manifest = ent.path().join("Cargo.toml");
        let Some(text) = std::fs::read_to_string(&manifest).ok() else {
            continue;
        };
        let Ok(v) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        if v.get("workspace").is_none() {
            continue;
        }
        let canon = ent.path().canonicalize().unwrap_or_else(|_| ent.path());
        if !configured.iter().any(|(_, c)| c == &canon) {
            return Err(cr_err(
                "07",
                &format!(
                    "unconfigured independent Cargo workspace '{name}'; add it to rust_projects or correct the project layout"
                ),
            ));
        }
    }
    Ok(())
}

fn auto_discover(repo: &Path) -> Result<RustProjects, String> {
    if repo.join("Cargo.toml").is_file() {
        // "." is only valid when no OTHER independent workspace exists —
        // a sibling workspace beside a root manifest is the same
        // silent-coverage-loss shape this EP exists to fix, so fail loud
        // (config cannot express this layout either: ["."] trips 07 and
        // [".", "sib"] trips 03 overlap — EP Q1 keeps nested disjoint
        // roots deliberately out of scope).
        let mut siblings: Vec<String> = Vec::new();
        for ent in std::fs::read_dir(repo).map_err(|e| e.to_string())?.flatten() {
            let ok = ent.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !ok {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let manifest = ent.path().join("Cargo.toml");
            let Some(text) = std::fs::read_to_string(&manifest).ok() else {
                continue;
            };
            let Ok(v) = toml::from_str::<toml::Value>(&text) else {
                continue;
            };
            if v.get("workspace").is_some() {
                siblings.push(name);
            }
        }
        if !siblings.is_empty() {
            siblings.sort();
            return Err(cr_err(
                "06",
                &format!(
                    "ambiguous Rust auto-discovery candidates [., {}]; a root manifest cannot coexist with an independent child workspace — declare rust_projects in .code-reality.toml or restructure",
                    siblings.join(", ")
                ),
            ));
        }
        return Ok(RustProjects {
            roots: vec![".".to_string()],
            mode: "auto",
        });
    }
    // Immediate-child scan (mirrors rust-analyzer's own child-scan — the
    // current "more than one project" failure condition becomes the
    // discovery trigger). Candidates: children carrying a Cargo.toml.
    let mut candidates: Vec<(String, bool)> = Vec::new(); // (rel, explicit_workspace)
    for ent in std::fs::read_dir(repo).map_err(|e| e.to_string())?.flatten() {
        let ok = ent.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !ok {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let manifest = ent.path().join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest)
            .map_err(|e| cr_err("04", &format!("rust project '{name}': {e}")))?;
        let parsed: toml::Value = toml::from_str(&text)
            .map_err(|e| cr_err("04", &format!("rust project '{name}' has an invalid Cargo.toml: {e}")))?;
        let explicit = parsed.get("workspace").is_some();
        candidates.push((name, explicit));
    }
    match candidates.len() {
        0 => Ok(RustProjects {
            roots: Vec::new(),
            mode: "auto",
        }),
        1 => {
            let roots = vec![candidates.remove(0).0];
            Ok(RustProjects { roots, mode: "auto" })
        }
        _ => {
            if candidates.iter().all(|(_, explicit)| *explicit) {
                let mut roots: Vec<String> = candidates.into_iter().map(|(n, _)| n).collect();
                roots.sort();
                Ok(RustProjects {
                    roots,
                    mode: "auto",
                })
            } else {
                let mut names: Vec<&str> = candidates.iter().map(|(n, _)| n.as_str()).collect();
                names.sort();
                Err(cr_err(
                    "06",
                    &format!(
                        "ambiguous Rust auto-discovery candidates [{}]; declare rust_projects in .code-reality.toml",
                        names.join(", ")
                    ),
                ))
            }
        }
    }
}
