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
//! Deep-layout discovery (CR-19): when the repo root carries no
//! Cargo.toml AND the immediate-child scan finds zero candidates, a
//! bounded recursive scan (depth ≤ 3, dot-dirs/`target`/`node_modules`
//! skipped, profile `exclude` constraining the range) classifies
//! manifests — `[workspace]` ⇒ root, bare `[package]` under a workspace
//! ancestor ⇒ member, bare `[package]` with no ancestor ⇒ standalone
//! root. Depth-1 candidate behavior is byte-unchanged.
//!
//! Zero-candidate contract (frozen Q6): `resolve_structural` returns an
//! EMPTY root set when no manifest is discoverable in range — the
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
pub fn resolve_structural(repo: &Path, profile: Option<&Profile>) -> Result<RustProjects, String> {
    // Canonicalize the repo base once: containment checks (starts_with)
    // must compare against the same resolved prefix as the canonicalized
    // children — a symlinked path prefix (/var → /private/var on macOS)
    // would otherwise flag every child as an escape.
    let repo = repo
        .canonicalize()
        .map_err(|e| cr_err("02", &format!("repo root canonicalize failed: {e}")))?;
    match profile.and_then(|p| p.rust_projects.clone()) {
        Some(list) => from_config(&repo, list, profile),
        None => auto_discover(&repo, profile),
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
            .map_err(|e| {
                cr_err(
                    "05",
                    &format!("rust project '{root}' cargo spawn failed: {e}"),
                )
            })?;
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
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| {
            cr_err(
                "05",
                &format!("rust project '{root}' metadata parse failed: {e}"),
            )
        })?;
        let ws_root = json["workspace_root"].as_str().ok_or_else(|| {
            cr_err(
                "05",
                &format!("rust project '{root}' metadata missing workspace_root"),
            )
        })?;
        let canon_ws = PathBuf::from(ws_root).canonicalize().map_err(|e| {
            cr_err(
                "05",
                &format!("rust project '{root}' workspace_root canonicalize failed: {e}"),
            )
        })?;
        let canon_dir = dir.canonicalize().map_err(|e| {
            cr_err(
                "02",
                &format!(
                    "rust project '{root}' cannot canonicalize inside repo '{}': {e}",
                    repo.display()
                ),
            )
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

fn from_config(
    repo: &Path,
    list: Vec<String>,
    profile: Option<&Profile>,
) -> Result<RustProjects, String> {
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
                &format!(
                    "rust project '{entry}' cannot canonicalize inside repo '{}': {e}",
                    repo.display()
                ),
            )
        })?;
        if !canon.starts_with(repo) {
            return Err(cr_err(
                "02",
                &format!(
                    "rust project '{entry}' canonicalizes outside the repo root (symlink escape)"
                ),
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
    scan_unlisted_workspaces(repo, profile, &canonical)?;
    let roots = canonical.into_iter().map(|(e, _)| e).collect();
    Ok(RustProjects {
        roots,
        mode: "config",
    })
}

/// A manifest collected by the bounded discovery walk (CR-19): the
/// repo-relative DIRECTORY holding the Cargo.toml plus its parse
/// outcome (`None` = readable but invalid TOML — the auto face fails
/// loud 04 on that, the config inventory face stays lenient like its
/// depth-1 form; unreadable manifests never enter the list).
struct ScannedManifest {
    rel: String,
    has_workspace: Option<bool>,
}

/// Bounded deep discovery walk (CR-19): every Cargo.toml whose directory
/// sits at depth ≤ 3 relative to the repo root. Never descends into
/// dot-dirs, `target` or `node_modules` — a scan-local skip list, NOT a
/// reuse of the rust-input sweep's directory hygiene (that sweep skips
/// `__pycache__`/`venv`/`node_modules`; only `node_modules` is shared
/// with it). The profile `exclude` table constrains the range —
/// excluded directories are pruned from the traversal itself (an
/// unreadable excluded subtree cannot abort the scan) and an excluded
/// path contributes no candidate, keeping discovery and governance on
/// one scope. Manifest CONTENT read failures skip the
/// entry (only readable-but-invalid TOML is fatal, and only on the auto
/// face); ENUMERATION failures (a directory the scan cannot list or
/// stat) fail loud — a broken walk is not an empty one.
fn scan_manifests_deep(
    repo: &Path,
    profile: Option<&Profile>,
) -> Result<Vec<ScannedManifest>, String> {
    const MAX_DEPTH: usize = 3;
    let mut found: Vec<ScannedManifest> = Vec::new();
    // (dir, repo-relative path of dir, component depth of dir)
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(repo.to_path_buf(), String::new(), 0)];
    while let Some((dir, rel, depth)) = stack.pop() {
        // Enumeration errors propagate (CR-19 review R2): silently
        // skipping an unlistable directory would report zero
        // candidates from a broken walk — the frozen skip rule covers
        // manifest CONTENT only.
        let dir_face = if rel.is_empty() {
            ".".to_string()
        } else {
            rel.clone()
        };
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("deep discovery scan cannot list directory '{dir_face}': {e}"))?;
        for ent in entries {
            let ent = ent.map_err(|e| {
                format!("deep discovery scan failed inside directory '{dir_face}': {e}")
            })?;
            let name = ent.file_name().to_string_lossy().into_owned();
            let entry_face = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let ft = ent
                .file_type()
                .map_err(|e| format!("deep discovery scan cannot stat '{entry_face}': {e}"))?;
            if ft.is_dir() {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                // Descend only while the child dir can still host an
                // in-range manifest (its own depth ≤ MAX_DEPTH).
                if depth < MAX_DEPTH {
                    let child_rel = if rel.is_empty() {
                        name
                    } else {
                        format!("{rel}/{name}")
                    };
                    // Prune excluded subtrees at the traversal level
                    // (CR-19 review R7): excluded = out of governance,
                    // so an unreadable excluded directory must never
                    // reach the fail-loud enumeration path. The
                    // trailing '/' aligns prefix semantics with the
                    // manifest-layer check below (exclude "poc/" hits
                    // the "poc" directory, not only its files); the
                    // manifest-layer check stays as defense-in-depth.
                    if crate::profile::is_excluded(&format!("{child_rel}/"), profile) {
                        continue;
                    }
                    stack.push((ent.path(), child_rel, depth + 1));
                }
            } else if ft.is_file() && name == "Cargo.toml" {
                let manifest_rel = if rel.is_empty() {
                    "Cargo.toml".to_string()
                } else {
                    format!("{rel}/Cargo.toml")
                };
                if crate::profile::is_excluded(&manifest_rel, profile) {
                    continue;
                }
                // Read failure → skip (frozen CR-19 rule: only
                // readable-but-invalid TOML is a fatal 04 on the auto
                // face — a manifest we cannot read is not evidence).
                let Ok(text) = std::fs::read_to_string(ent.path()) else {
                    continue;
                };
                let has_workspace = toml::from_str::<toml::Value>(&text)
                    .ok()
                    .map(|v| v.get("workspace").is_some());
                found.push(ScannedManifest {
                    rel: rel.clone(),
                    has_workspace,
                });
            }
        }
    }
    // Deterministic classification order: the walk is LIFO over a
    // filesystem-dependent read_dir sequence, and the 06/07 error faces
    // name the FIRST offending manifest — sort so a given tree always
    // produces the same message.
    found.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(found)
}

/// Config-mode inventory gate: any `[workspace]` manifest inside the
/// discovery range (CR-19: the SAME bounded deep scan the auto face
/// uses) that is absent from the configured list is a coverage hole
/// (adding a sibling — depth-1 or deep — must not silently retain old
/// coverage). Member/invalid/unreadable manifests stay lenient: the
/// inventory keys on the `[workspace]` table only, matching the
/// depth-1 form's semantics.
fn scan_unlisted_workspaces(
    repo: &Path,
    profile: Option<&Profile>,
    configured: &[(String, PathBuf)],
) -> Result<(), String> {
    for m in scan_manifests_deep(repo, profile)? {
        if m.has_workspace != Some(true) {
            continue;
        }
        let dir = repo.join(&m.rel);
        let canon = dir.canonicalize().unwrap_or(dir);
        if !configured.iter().any(|(_, c)| c == &canon) {
            // Display face: the repo-root manifest scans as rel "" —
            // name it "." so the 07 message matches the config face's
            // own spelling for the whole-repo root.
            let display = if m.rel.is_empty() { "." } else { &m.rel };
            return Err(cr_err(
                "07",
                &format!(
                    "unconfigured independent Cargo workspace '{display}'; add it to rust_projects or correct the project layout"
                ),
            ));
        }
    }
    Ok(())
}

fn auto_discover(repo: &Path, profile: Option<&Profile>) -> Result<RustProjects, String> {
    if repo.join("Cargo.toml").is_file() {
        // "." is only valid when no OTHER independent workspace exists —
        // a sibling workspace beside a root manifest is the same
        // silent-coverage-loss shape this EP exists to fix, so fail loud
        // (config cannot express this layout either: ["."] trips 07 and
        // [".", "sib"] trips 03 overlap — EP Q1 keeps nested disjoint
        // roots deliberately out of scope).
        let mut siblings: Vec<String> = Vec::new();
        for ent in std::fs::read_dir(repo)
            .map_err(|e| e.to_string())?
            .flatten()
        {
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
    // CR-19: zero candidates here hand off to the bounded deep scan
    // instead of terminating at empty roots; any depth-1 candidate keeps
    // this face's behavior byte-unchanged.
    let mut candidates: Vec<(String, bool)> = Vec::new(); // (rel, explicit_workspace)
    for ent in std::fs::read_dir(repo)
        .map_err(|e| e.to_string())?
        .flatten()
    {
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
        let parsed: toml::Value = toml::from_str(&text).map_err(|e| {
            cr_err(
                "04",
                &format!("rust project '{name}' has an invalid Cargo.toml: {e}"),
            )
        })?;
        let explicit = parsed.get("workspace").is_some();
        candidates.push((name, explicit));
    }
    match candidates.len() {
        0 => deep_discover(repo, profile),
        1 => {
            let roots = vec![candidates.remove(0).0];
            Ok(RustProjects {
                roots,
                mode: "auto",
            })
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

/// Deep-layout classification (CR-19, frozen codex-adjudicated rules —
/// pure TOML, still zero cargo spawns, frozen Q8 unchanged):
///
/// - a manifest carrying `[workspace]` is a ROOT, unconditionally;
/// - a bare `[package]` with a `[workspace]` ancestor is a MEMBER —
///   owned from above, never a root of its own;
/// - a bare `[package]` with NO ancestor manifest is a STANDALONE root
///   candidate (the mosaic_alpha oracle mixes workspace roots and
///   standalone packages — a strict "mixed kinds" 06 would deadlock
///   real consumers);
/// - a bare `[package]` whose ancestors are ALL non-workspace manifests
///   is an OWNERSHIP AMBIGUITY (06): no `[workspace]` can own it as a
///   member, and promoting it to a root would overlap its package
///   ancestor — fail loud instead of silently classifying it as a
///   member nobody covers;
/// - ancestor/descendant ROOT overlap (a `[workspace]` nested inside
///   another) fails 03.
///
/// An empty scan keeps the frozen Q6 contract: EMPTY roots, the caller
/// owns the legacy-parity `["."]` fallback (and the build face's CR-19
/// A guard vetoes the non-empty-governed corner before any spawn).
fn deep_discover(repo: &Path, profile: Option<&Profile>) -> Result<RustProjects, String> {
    let scanned = scan_manifests_deep(repo, profile)?;
    for m in &scanned {
        if m.has_workspace.is_none() {
            return Err(cr_err(
                "04",
                &format!(
                    "rust project '{}' has an invalid Cargo.toml (deep discovery scan)",
                    m.rel
                ),
            ));
        }
    }
    let mut roots: Vec<String> = Vec::new();
    for m in &scanned {
        if m.has_workspace == Some(true) {
            roots.push(m.rel.clone());
            continue;
        }
        let ancestors: Vec<&str> = scanned
            .iter()
            .filter(|a| a.rel != m.rel && m.rel.starts_with(&format!("{}/", a.rel)))
            .filter(|a| a.has_workspace != Some(true))
            .map(|a| a.rel.as_str())
            .collect();
        let workspace_ancestor = scanned.iter().any(|a| {
            a.rel != m.rel
                && m.rel.starts_with(&format!("{}/", a.rel))
                && a.has_workspace == Some(true)
        });
        if workspace_ancestor {
            continue; // member: owned by a workspace above — not a root
        }
        if ancestors.is_empty() {
            roots.push(m.rel.clone()); // standalone package
        } else {
            let mut chain = ancestors;
            chain.sort_unstable();
            return Err(cr_err(
                "06",
                &format!(
                    "ambiguous Rust ownership: '{}' is a bare [package] under non-workspace manifest(s) [{}]; no [workspace] can own it as a member — declare rust_projects in .code-reality.toml or restructure",
                    m.rel,
                    chain.join(", ")
                ),
            ));
        }
    }
    // Component-wise Path order keeps an ancestor adjacent to its
    // minimal descendant, so the windows(2) overlap gate below is
    // complete — String byte order ('-' < '/') can interleave e.g.
    // "crates/a-b" between "crates/a" and "crates/a/sub" and the pair
    // is never checked.
    roots.sort_by(|a, b| Path::new(a).cmp(Path::new(b)));
    for w in roots.windows(2) {
        if w[1].starts_with(&format!("{}/", w[0])) {
            return Err(cr_err(
                "03",
                &format!(
                    "auto-discovered Rust roots '{}' and '{}' overlap (ancestor/descendant); declare rust_projects in .code-reality.toml or restructure",
                    w[0], w[1]
                ),
            ));
        }
    }
    Ok(RustProjects {
        roots,
        mode: "auto",
    })
}
