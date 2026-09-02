//! `transition` — the snapshot-diff DOMAIN module (frozen
//! `code_reality/transition.py` semantics, post canonical-sync
//! 281e07e): snapshot pair set-diff (B1 reversed = added direction),
//! EP claims comparison (regex ∪ relative path-token normalization
//! with repo-root existence verification), degenerate/cross-face
//! guards, json render. The CLI/report face was retired (S4,
//! 2026-08-29 user adjudication — no real consumers; the machine
//! comparison rides delta_tour): delta_tour is the sole diff
//! interface, consuming `load_snapshot`/`summarize`/
//! `extract_ep_claims`/`render_json_value`.

use crate::common::utc_now_iso_micros;
use crate::profile::{claims_regex, module_of, Profile};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub type Edge = (String, String, String);

#[derive(Debug, Clone)]
pub struct LoadedSnapshot {
    pub path: PathBuf,
    pub meta: Map<String, Value>,
    pub files: BTreeSet<String>,
    pub module_edges: BTreeSet<Edge>,
}

pub fn load_snapshot(path: &Path) -> Result<LoadedSnapshot, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("{} 讀取失敗：{}", path.display(), e))?;
    let data: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "非 S2 snapshot 格式（缺 _meta/module_edges）: {}（{e}）",
            path.display()
        )
    })?;
    let obj = data.as_object().ok_or_else(|| {
        format!(
            "非 S2 snapshot 格式（缺 _meta/module_edges）: {}",
            path.display()
        )
    })?;
    if !obj.contains_key("_meta") || !obj.contains_key("module_edges") {
        return Err(format!(
            "非 S2 snapshot 格式（缺 _meta/module_edges）: {}",
            path.display()
        ));
    }
    let edges_raw = obj["module_edges"].as_array().ok_or_else(|| {
        format!(
            "module_edges 元素非 [src, dst, kind] 三元組: {}",
            path.display()
        )
    })?;
    let mut module_edges = BTreeSet::new();
    for e in edges_raw {
        let arr = e.as_array().ok_or_else(|| {
            format!(
                "module_edges 元素非 [src, dst, kind] 三元組: {}",
                path.display()
            )
        })?;
        if arr.len() != 3 || !arr.iter().all(Value::is_string) {
            return Err(format!(
                "module_edges 元素非 [src, dst, kind] 三元組: {}",
                path.display()
            ));
        }
        module_edges.insert((
            arr[0].as_str().unwrap().to_string(),
            arr[1].as_str().unwrap().to_string(),
            arr[2].as_str().unwrap().to_string(),
        ));
    }
    let files: BTreeSet<String> = obj
        .get("files")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Ok(LoadedSnapshot {
        path: path.to_path_buf(),
        meta: obj["_meta"].as_object().cloned().unwrap_or_default(),
        files,
        module_edges,
    })
}

pub struct EdgeDiff {
    pub added: Vec<Edge>,
    pub removed: Vec<Edge>,
    /// always added-direction (B1)
    pub reversed: Vec<(String, String)>,
    pub changed_modules: BTreeSet<String>,
}

/// Pair set-diff (`transition.py:72-86`): the tuple-diff projection ≠
/// pair set-diff under multi-kind duplicates — the pair projection is
/// the correct reversal test.
pub fn diff_edges(a: &BTreeSet<Edge>, b: &BTreeSet<Edge>) -> EdgeDiff {
    let removed: BTreeSet<&Edge> = a.difference(b).collect();
    let added: BTreeSet<&Edge> = b.difference(a).collect();
    let removed_pairs: BTreeSet<(&str, &str)> = removed
        .iter()
        .map(|(s, d, _)| (s.as_str(), d.as_str()))
        .collect();
    let added_pairs: BTreeSet<(&str, &str)> = added
        .iter()
        .map(|(s, d, _)| (s.as_str(), d.as_str()))
        .collect();
    let mut reversed: Vec<(String, String)> = added_pairs
        .intersection(&removed_pairs.iter().map(|&(s, d)| (d, s)).collect())
        .map(|&(s, d)| (s.to_string(), d.to_string()))
        .collect();
    reversed.sort();
    let mut changed = BTreeSet::new();
    for (s, d) in removed_pairs.iter().chain(added_pairs.iter()) {
        changed.insert(s.to_string());
        changed.insert(d.to_string());
    }
    EdgeDiff {
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        reversed,
        changed_modules: changed,
    }
}

/// Pair summary (S4): the diff faces plus the degenerate marking —
/// marked at summarize time so every consumer (delta_tour today, any
/// future one) inherits the guard without re-deriving it.
pub struct Summary {
    pub diff: EdgeDiff,
    pub new_files: Vec<String>,
    pub gone_files: Vec<String>,
    pub degenerate: Option<String>,
}

pub fn summarize(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> Summary {
    let diff = diff_edges(&sa.module_edges, &sb.module_edges);
    let new_files: Vec<String> = sb.files.difference(&sa.files).cloned().collect();
    let gone_files: Vec<String> = sa.files.difference(&sb.files).cloned().collect();
    let degenerate = degenerate_pair(sa, sb);
    Summary {
        diff,
        new_files,
        gone_files,
        degenerate,
    }
}

/// Degenerate-pair guard: a snapshot whose participating-file set is empty
/// (the REFERENCES-only collapse) cannot support file-face conclusions.
/// ANY side empty → `Some(warning)` distinguishing which side's list is
/// untrustworthy; the consumer must not draw a "no structural change"
/// conclusion from a both-empty pair (a healthy→degenerate pair still
/// carries its — falsely massive — file lists, marked by the warning
/// instead of erased).
pub fn degenerate_pair(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> Option<String> {
    match (sa.files.is_empty(), sb.files.is_empty()) {
        (true, true) => {
            Some("兩側 snapshot files 皆空（退化快照）——diff 無意義，勿下「無結構變化」結論".into())
        }
        (true, false) => Some("before 側 snapshot files 空（退化）——gone-files 清單不可信".into()),
        (false, true) => Some("after 側 snapshot files 空（退化）——added-files 清單不可信".into()),
        _ => None,
    }
}

fn files_face(s: &LoadedSnapshot) -> Option<&str> {
    s.meta.get("files_face").and_then(Value::as_str)
}

/// Cross-face guard (S2): the files face is comparable only when both
/// sides carry the same `files_face` meta (absent ≡ the pre-widening
/// structural-only face). module_edges stay comparable either way — the
/// structural kind set never changed.
fn face_mismatch(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> Option<String> {
    match (files_face(sa), files_face(sb)) {
        (Some(a), Some(b)) if a != b => Some(format!(
            "files 面不同（before={a}／after={b}）——files diff 跨面不可比；module_edges 仍可比（kind 集不變）"
        )),
        (None, Some(_)) | (Some(_), None) => Some(
            "files 面跨版本不可比（一方缺 files_face＝舊 structural-only 面）——files diff 不可信；module_edges 仍可比（kind 集不變）".into(),
        ),
        _ => None,
    }
}

fn crg_generation(s: &LoadedSnapshot) -> Option<&str> {
    s.meta.get("crg_last_updated").and_then(Value::as_str)
}

fn crg_raw_edges(s: &LoadedSnapshot) -> Option<u64> {
    s.meta.get("crg_raw_edges").and_then(Value::as_u64)
}

/// Cross-generation guard (MOS-4 symptom 2): the pair spans two
/// graph.db generations — a rebuild happened between the two snapshot
/// points, and a corpus-shrinking rebuild masquerades as mass phantom
/// deletions in the files diff. Loose semantics: either side missing
/// the `crg_last_updated` fingerprint (pre-fingerprint snapshot)
/// skips the check — old snapshots never false-warn. `last_updated`
/// has a single writer, the graph build path, so a value difference
/// means a real rebuild (head-sync refresh touches no meta).
fn generation_mismatch(sa: &LoadedSnapshot, sb: &LoadedSnapshot) -> Option<String> {
    let (ga, gb) = (crg_generation(sa)?, crg_generation(sb)?);
    if ga == gb {
        return None;
    }
    let edges_note = match (crg_raw_edges(sa), crg_raw_edges(sb)) {
        (Some(na), Some(nb)) => format!("；raw edges {na}→{nb}"),
        _ => String::new(),
    };
    Some(format!(
        "before/after 跨 graph 世代（before {ga}／after {gb}{edges_note}）——graph.db 曾在兩次 snapshot 之間重建，檔案集收縮可能是重建造成而非真實刪檔（phantom 刪檔風險）；delta 僅供參考，建議重建後雙端重 snapshot。不擋執行。"
    ))
}

/// File-path token regex (`transition.py:100`).
fn file_token_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[A-Za-z0-9_][\w./+-]*\.[A-Za-z0-9]+").unwrap())
}

/// Relative path tokens → module claims with existence verification
/// (`transition.py:103-132`, sync 281e07e): prefix-direct hits resolve
/// as-is; bare-relative tokens resolve only when
/// `repo_root/<prefix>/<first-segment>` is a real directory — a grounded
/// mapping, no guessing.
pub fn path_token_claims(text: &str, profile: &Profile, repo_root: &Path) -> BTreeSet<String> {
    let mut claims = BTreeSet::new();
    for tok in file_token_re().find_iter(text).map(|m| m.as_str()) {
        if !tok.contains('/') {
            continue; // bare filenames cannot be mapped to a module
        }
        let mut resolved: Option<String> = None;
        for rule in &profile.modules {
            if tok.starts_with(&rule.prefix) {
                resolved = Some(tok.to_string());
                break;
            }
        }
        if resolved.is_none() {
            let seg = tok.split('/').next().unwrap_or("");
            for rule in &profile.modules {
                if repo_root.join(&rule.prefix).join(seg).is_dir() {
                    resolved = Some(format!("{}{}", rule.prefix, tok));
                    break;
                }
            }
        }
        if let Some(r) = resolved {
            let m = module_of(&r, Some(profile));
            if !m.is_empty() {
                claims.insert(m);
            }
        }
    }
    claims
}

/// EP claims (`transition.py:135-150`): regex findall ∪ path tokens when
/// `repo_root` is provided. Missing EP file → crash (SM-12: NONE means
/// the file exists but has no mentions).
pub fn extract_ep_claims(
    ep_path: &Path,
    profile: Option<&Profile>,
    repo_root: Option<&Path>,
) -> Result<BTreeSet<String>, String> {
    if !ep_path.is_file() {
        return Err(format!(
            "EP 檔不存在或非檔案：{}（SM-12——NONE 是檔在但無 mention）",
            ep_path.display()
        ));
    }
    let text = std::fs::read_to_string(ep_path)
        .map_err(|e| format!("{} 讀取失敗：{}", ep_path.display(), e))?;
    let mut claims: BTreeSet<String> = claims_regex(profile)
        .find_iter(&text)
        .map(|m| m.as_str().to_string())
        .collect();
    if let (Some(root), Some(p)) = (repo_root, profile) {
        claims.extend(path_token_claims(&text, p, root));
    }
    Ok(claims)
}

pub struct ClaimsCompare {
    pub claimed_and_changed: Vec<String>,
    pub changed_not_claimed: Vec<String>,
    pub claimed_not_changed: Vec<String>,
    pub claims_none: bool,
}

pub fn compare_claims(claims: &BTreeSet<String>, changed: &BTreeSet<String>) -> ClaimsCompare {
    ClaimsCompare {
        claimed_and_changed: claims.intersection(changed).cloned().collect(),
        changed_not_claimed: changed.difference(claims).cloned().collect(),
        claimed_not_changed: claims.difference(changed).cloned().collect(),
        claims_none: claims.is_empty(),
    }
}

/// Actual changed modules = edge topology ∪ file add/remove owners
/// (`transition.py:169-179`).
fn changed_modules_all(
    diff: &EdgeDiff,
    new_files: &[String],
    gone_files: &[String],
    profile: Option<&Profile>,
) -> BTreeSet<String> {
    let mut out = diff.changed_modules.clone();
    for f in new_files.iter().chain(gone_files) {
        out.insert(module_of(f, profile));
    }
    out
}

/// Render the json diff face from a `Summary` (single derivation
/// source: `degenerate` marked once in `summarize`, consumed here —
/// delta_tour pipes this into `build_tour`).
pub fn render_json_value(
    sa: &LoadedSnapshot,
    sb: &LoadedSnapshot,
    summary: &Summary,
    claims: Option<&BTreeSet<String>>,
    profile: Option<&Profile>,
) -> Value {
    let diff = &summary.diff;
    let new_files = &summary.new_files;
    let gone_files = &summary.gone_files;
    let changed_set = changed_modules_all(diff, new_files, gone_files, profile);
    let changed: Vec<String> = changed_set.iter().cloned().collect();
    let mut out = json!({
        "_meta": {
            "tool": "code_reality.transition",
            "created_at": utc_now_iso_micros(),
            "before": sa.meta.get("commit").cloned().unwrap_or(Value::Null),
            "after": sb.meta.get("commit").cloned().unwrap_or(Value::Null),
            "repo": sb.meta.get("repo").cloned().unwrap_or(Value::Null),
        },
        "added": diff.added.iter().map(|(s, d, k)| json!([s, d, k])).collect::<Vec<_>>(),
        "removed": diff.removed.iter().map(|(s, d, k)| json!([s, d, k])).collect::<Vec<_>>(),
        "reversed": diff.reversed.iter().map(|(s, d)| json!([s, d])).collect::<Vec<_>>(),
        "changed_modules": changed,
        "new_files": new_files,
        "gone_files": gone_files,
    });
    // degenerate-pair guard on the json face: the warning field rides
    // alongside the diff faces — delta_tour's build_tour reads it and
    // injects the warning into the tour description (S4).
    if let Some(w) = &summary.degenerate {
        out.as_object_mut()
            .unwrap()
            .insert("degenerate_warning".into(), json!(w));
    }
    if let Some(w) = face_mismatch(sa, sb) {
        out.as_object_mut()
            .unwrap()
            .insert("files_face_warning".into(), json!(w));
    }
    if let Some(w) = generation_mismatch(sa, sb) {
        out.as_object_mut()
            .unwrap()
            .insert("generation_warning".into(), json!(w));
    }
    if let Some(c) = claims {
        let cmp = compare_claims(c, &changed_set);
        out.as_object_mut().unwrap().insert(
            "ep_claims".into(),
            json!({
                "claims": c.iter().cloned().collect::<Vec<_>>(),
                "claims_none": cmp.claims_none,
                "claimed_and_changed": cmp.claimed_and_changed,
                "changed_not_claimed": cmp.changed_not_claimed,
                "claimed_not_changed": cmp.claimed_not_changed,
            }),
        );
    }
    out
}
