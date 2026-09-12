//! `tour` — intent-level tour materialization umbrella (AIR-80).
//!
//! Two-phase contract (codex review R1): `tour register` persists the
//! provenance row (`[[delta_arc]]`) and returns — the ask-once "skip" path
//! leaves a PENDING row (no tourPath) so consumers can still trigger later;
//! `tour materialize <arcId>` assembles the FULL recipe from the row
//! (snapshot-pair resolution, EP claims gate, `delta_tour::build_tour`,
//! `.tours/delta/` output) and only then upserts `tourPath`. Rows are
//! tool-owned authoritative full replaces keyed on arcId — re-registering
//! must re-supply optional fields (cardId/ep); unknown keys are NOT
//! preserved in delta rows (explicit contract, not an accident).
//!
//! Materialization semantics: commit-ish (7-char short shas included) are
//! resolved via `git rev-parse --verify` before snapshot lookup; any
//! snapshot carrying a non-empty `_meta.stale` fails loud (the doctrine
//! gate is "in place AND non-stale"); a relative EP resolves against
//! `--repo` for both FS access (absolute, for claims) and the tour anchor
//! (repo-relative, never absolute paths in step files). Same arcId
//! re-materialization OVERWRITES the same tourPath; history lives in git.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::to_json_indent1;
use crate::delta_tour::build_tour;
use crate::profile::load_profile;
use crate::tour_manifest::{dump, load, upsert_delta_arc, Manifest};
use crate::transition::{extract_ep_claims, load_snapshot, render_json_value, summarize};
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
            long: "--base",
            short: None,
            kind: Kind::Value {
                metavar: "BASE_SHA",
            },
        },
        FlagSpec {
            long: "--target",
            short: None,
            kind: Kind::Value {
                metavar: "TARGET_SHA",
            },
        },
        FlagSpec {
            long: "--ep",
            short: None,
            kind: Kind::Value { metavar: "EP_MD" },
        },
        FlagSpec {
            long: "--card",
            short: None,
            kind: Kind::Value { metavar: "CARD_ID" },
        },
    ],
    positionals: &["arcId"],
};

const HELP: &str = concat!(
    "usage: code-reality tour <register|materialize> [-h] [--repo REPO]\n",
    "       [--base BASE_SHA] [--target TARGET_SHA] [--ep EP_MD]\n",
    "       [--card CARD_ID] arcId\n",
    "\n",
    "intent-level delta tour：register＝持久化 provenance row（pending，ask-once",
    " 略過路徑用）；materialize＝依 row 組完整 recipe 產 tour（snapshot pair 解析",
    "＋EP gate＋row 補 tourPath）。\n",
    "\n",
    "positional arguments:\n",
    "  arcId                 materialization canonical key（同 arcId 重產＝覆蓋同 tourPath）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo 根（預設 cwd）\n",
    "  --base BASE_SHA       弧 baseline commit（commit-ish，內部 rev-parse 解析）\n",
    "  --target TARGET_SHA   弧 target commit（commit-ish）\n",
    "  --ep EP_MD            EP markdown 路徑（僅註冊形；repo-relative canonical 化；缺席→quality=degraded）\n",
    "  --card CARD_ID        join 屬性（僅註冊形；row-driven materialize 帶旗標＝fail-loud）\n",
);

fn row_str(row: &toml::Table, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Resolve a commit-ish (short sha / branch / HEAD~1 …) to the full commit
/// sha via git — snapshot files are `<repo>-<sha8>.json`, so 7-char inputs
/// must canonicalize before suffix matching (codex review R3).
fn resolve_commit(repo: &Path, commit_ish: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("--verify")
        .arg("--quiet")
        .arg(format!("{commit_ish}^{{commit}}"))
        .output()
        .map_err(|e| format!("git rev-parse 執行失敗：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "commit-ish 解析失敗：{commit_ish}（{repo:?} 歷史內不存在？）"
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Locate `<snapshots>/*-<sha8>.json` by commit sha prefix (8 chars of a
/// RESOLVED full sha).
pub fn snapshot_for(snapshots: &Path, commit: &str) -> Result<PathBuf, String> {
    let sha8: String = commit.chars().take(8).collect();
    if sha8.is_empty() {
        return Err("commit sha 為空".to_string());
    }
    let suffix = format!("-{sha8}.json");
    let mut hits: Vec<PathBuf> = std::fs::read_dir(snapshots)
        .map_err(|e| format!("{} 讀取失敗：{e}", snapshots.display()))?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(&suffix))
                .unwrap_or(false)
        })
        .collect();
    hits.sort();
    match hits.len() {
        0 => Err(format!(
            "snapshot 缺席：{}/ *{suffix}——先在對應 commit 跑 `code-reality snapshot --repo <repo>`（ask-once 未確認弧保留 inputs 即為此用）",
            snapshots.display()
        )),
        1 => Ok(hits.remove(0)),
        _ => Err(format!(
            "snapshot 歧義（{suffix} 命中 {}）：{:?}",
            hits.len(),
            hits
        )),
    }
}

type Values = std::collections::HashMap<&'static str, Option<String>>;

/// Parse the shared arc argument face; returns (arcId, values).
fn parse_arc_args(toks: &[&str]) -> Result<(String, Values), ToolOutput> {
    match parse(&SPEC, toks) {
        Outcome::Help => Err(ToolOutput {
            stdout: HELP.to_string(),
            stderr: String::new(),
            exit_code: 0,
        }),
        Outcome::Err(msg) => Err(ToolOutput::fail(msg)),
        Outcome::Ok {
            values,
            positionals,
        } => {
            let Some(arc_id) = positionals.first() else {
                return Err(ToolOutput::fail(
                    "需提供 arcId（materialization canonical key）",
                ));
            };
            if arc_id.is_empty() {
                return Err(ToolOutput::crash("arcId 不得為空字串"));
            }
            Ok((arc_id.clone(), values))
        }
    }
}

fn repo_of(values: &Values) -> PathBuf {
    crate::common::resolve(
        &values
            .get("--repo")
            .and_then(|v| v.clone())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    )
}

/// Base/target from flags; fail unless paired. Both resolved to full shas.
fn resolved_pair(repo: &Path, values: &Values) -> Result<(String, String), String> {
    let base_ish = values.get("--base").and_then(|v| v.clone());
    let target_ish = values.get("--target").and_then(|v| v.clone());
    match (base_ish, target_ish) {
        (Some(b), Some(t)) => {
            let base = resolve_commit(repo, &b)?;
            let target = resolve_commit(repo, &t)?;
            Ok((base, target))
        }
        (Some(_), None) | (None, Some(_)) => {
            Err("--base 與 --target 需成對（row-driven materialize 兩者都可省）".to_string())
        }
        (None, None) => Err("需 --base/--target".to_string()),
    }
}

/// Lexical normalization (no filesystem access): collapses `.`/`..` so
/// containment checks cannot be escaped by `repo/../outside.md` (codex
/// final review N1 residual — strip_prefix alone is prefix-lexical and a
/// joined `..` component slips through).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() && !out.as_os_str().is_empty() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

/// Canonical ep provenance string: repo-relative when the resolved (and
/// normalized) path is inside the repo, absolute otherwise (unambiguous
/// repo-root anchoring; `..` escapes normalize to the real location).
fn canonical_ep(repo: &Path, spec: &str) -> String {
    let pb = PathBuf::from(spec);
    let abs = normalize(&if pb.is_absolute() { pb } else { repo.join(&pb) });
    match abs.strip_prefix(normalize(repo)) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string_lossy().into_owned(),
    }
}

fn arc_fields(
    arc_id: &str,
    base: &str,
    target: &str,
    repo: &Path,
    values: &Values,
) -> Vec<(&'static str, toml::Value)> {
    let mut fields: Vec<(&'static str, toml::Value)> = vec![
        ("arcId", toml::Value::String(arc_id.to_string())),
        ("base", toml::Value::String(base.to_string())),
        ("target", toml::Value::String(target.to_string())),
    ];
    if let Some(ep) = values.get("--ep").and_then(|v| v.clone()) {
        fields.push(("ep", toml::Value::String(canonical_ep(repo, &ep))));
    }
    if let Some(c) = values.get("--card").and_then(|v| v.clone()) {
        fields.push(("cardId", toml::Value::String(c)));
    }
    fields
}

/// Persist the provenance row IMMEDIATELY (codex review R1): registration
/// must survive a later materialize failure — ask-once "skip" leaves a
/// pending row so consumers can still trigger.
fn register(argv: &[&str]) -> ToolOutput {
    let (arc_id, values) = match parse_arc_args(argv) {
        Ok(v) => v,
        Err(out) => return out,
    };
    let repo = repo_of(&values);
    let manifest_path = repo.join(".tours").join("manifest.toml");
    let mut manifest: Manifest = match load(&manifest_path) {
        Ok(m) => m,
        Err(e) => return ToolOutput::crash(e),
    };
    let (base, target) = match resolved_pair(&repo, &values) {
        Ok(v) => v,
        Err(e) => return ToolOutput::crash(e),
    };
    upsert_delta_arc(
        &mut manifest,
        &arc_fields(&arc_id, &base, &target, &repo, &values),
    );
    if let Err(e) = std::fs::create_dir_all(repo.join(".tours")) {
        return ToolOutput::crash(format!(".tours 建立失敗：{e}"));
    }
    if let Err(e) = dump(&manifest_path, &manifest) {
        return ToolOutput::crash(e);
    }
    ToolOutput {
        stdout: format!("[OK] registered arc {arc_id}（pending row——materialize 時補 tourPath）\n"),
        stderr: String::new(),
        exit_code: 0,
    }
}

/// Stale gate (codex review R4): the doctrine premise is "snapshots in
/// place AND non-stale" — a stale pair fails loud instead of shipping a
/// silently-degraded tour (intent-only callers bypass no gate).
fn stale_of(meta: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    match meta.get("stale") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn materialize(argv: &[&str]) -> ToolOutput {
    let (arc_id, values) = match parse_arc_args(argv) {
        Ok(v) => v,
        Err(out) => return out,
    };
    let repo = repo_of(&values);
    let manifest_path = repo.join(".tours").join("manifest.toml");
    let mut manifest: Manifest = match load(&manifest_path) {
        Ok(m) => m,
        Err(e) => return ToolOutput::crash(e),
    };
    // flag face (codex re-review N2): --card/--ep belong to the registration
    // form only — accepted without --base/--target they would be silently
    // dropped, so fail loud instead.
    if (values.contains_key("--card") || values.contains_key("--ep"))
        && !values.contains_key("--base")
        && !values.contains_key("--target")
    {
        return ToolOutput::crash(
            "--card/--ep 僅註冊形有效——row-driven materialize 需與 --base/--target 成對，否則忽略即 silent drop",
        );
    }
    // registration form (base/target on flags): resolve and PERSIST first —
    // a later failure must not erase the row (R1)
    if values.contains_key("--base") || values.contains_key("--target") {
        let (base, target) = match resolved_pair(&repo, &values) {
            Ok(v) => v,
            Err(e) => return ToolOutput::crash(e),
        };
        upsert_delta_arc(
            &mut manifest,
            &arc_fields(&arc_id, &base, &target, &repo, &values),
        );
        if let Err(e) = std::fs::create_dir_all(repo.join(".tours")) {
            return ToolOutput::crash(format!(".tours 建立失敗：{e}"));
        }
        if let Err(e) = dump(&manifest_path, &manifest) {
            return ToolOutput::crash(e);
        }
    }
    let Some(row) = manifest
        .delta_arc
        .iter()
        .find(|r| row_str(r, "arcId") == arc_id)
        .cloned()
    else {
        return ToolOutput::crash(format!(
            "arcId {arc_id} 無 provenance row——先 `tour register`（或帶 --base/--target 註冊形）"
        ));
    };
    let base = row_str(&row, "base");
    let target = row_str(&row, "target");
    if base.is_empty() || target.is_empty() {
        return ToolOutput::crash(format!("row {arc_id} 缺 base/target——以 register 補齊"));
    }
    // EP provenance vs tour anchor split (codex re-review N1): the row
    // persists a CANONICAL ep spec (repo-relative when inside the repo,
    // absolute otherwise) so row-driven re-materialization can always
    // rebuild ep_fs; ep_tour (step anchor) only exists for in-repo EPs —
    // absolute paths never enter tour steps. quality = claims completeness.
    let ep_spec = row_str(&row, "ep");
    let (ep_fs, ep_tour): (Option<PathBuf>, Option<String>) = if ep_spec.is_empty() {
        (None, None)
    } else {
        let pb = PathBuf::from(&ep_spec);
        let abs = if pb.is_absolute() { pb } else { repo.join(&pb) };
        let abs = normalize(&abs);
        match abs.strip_prefix(normalize(&repo)) {
            Ok(rel) => (
                Some(abs.clone()),
                Some(rel.to_string_lossy().replace('\\', "/")),
            ),
            Err(_) => (Some(abs), None),
        }
    };
    let snapshots = repo.join(".code-reality").join("snapshots");
    let sa_path = match snapshot_for(&snapshots, &base) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(format!("base {base}: {e}")),
    };
    let sb_path = match snapshot_for(&snapshots, &target) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(format!("target {target}: {e}")),
    };
    let sa = match load_snapshot(&sa_path) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    let sb = match load_snapshot(&sb_path) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    if let Some(w) = stale_of(&sa.meta) {
        return ToolOutput::crash(format!(
            "base snapshot stale（fail-loud）：{w}——重跑 `code-reality snapshot --repo <repo>` 後再 materialize"
        ));
    }
    if let Some(w) = stale_of(&sb.meta) {
        return ToolOutput::crash(format!(
            "target snapshot stale（fail-loud）：{w}——重跑 `code-reality snapshot --repo <repo>` 後再 materialize"
        ));
    }
    let mut stderr = String::new();
    let mut stdout = String::new();
    let profile = match load_profile(&repo) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
    if ep_fs.is_some() && profile.is_none() {
        stdout.push_str(
            "[WARN] claims 恆 NONE——repo 無 .code-reality.toml profile，宣稱對照不生效\n",
        );
    }
    let claims = match &ep_fs {
        None => None,
        Some(p) => match extract_ep_claims(p, profile.as_ref(), Some(&repo)) {
            Ok(c) => Some(c),
            Err(e) => return ToolOutput::crash(e),
        },
    };
    let summary = summarize(&sa, &sb);
    let data = render_json_value(&sa, &sb, &summary, claims.as_ref(), profile.as_ref());
    let tour = match build_tour(
        &data,
        &repo,
        ep_tour.as_deref().map(Path::new),
        &arc_id,
        &mut stderr,
    ) {
        Ok(t) => t,
        Err(e) => return ToolOutput::crash(e),
    };
    let delta_dir = repo.join(".tours").join("delta");
    if let Err(e) = std::fs::create_dir_all(&delta_dir) {
        return ToolOutput::crash(format!("{} 建立失敗：{e}", delta_dir.display()));
    }
    let out_path = delta_dir.join(format!("{arc_id}.tour"));
    if let Err(e) = std::fs::write(&out_path, to_json_indent1(&tour)) {
        return ToolOutput::crash(format!("{} 寫入失敗：{e}", out_path.display()));
    }
    // row upsert (tool-owned full replace keyed on arcId): quality = claims
    // completeness; ep provenance (canonical spec) and cardId are preserved
    // so intent-only re-materialization is idempotent (codex re-review N1)
    let quality = if ep_spec.is_empty() {
        "degraded"
    } else {
        "full"
    };
    let tour_rel = format!(".tours/delta/{arc_id}.tour");
    let mut fields: Vec<(&'static str, toml::Value)> = vec![
        ("arcId", toml::Value::String(arc_id.clone())),
        ("base", toml::Value::String(base)),
        ("target", toml::Value::String(target)),
        ("quality", toml::Value::String(quality.to_string())),
        ("tourPath", toml::Value::String(tour_rel.clone())),
    ];
    if !ep_spec.is_empty() {
        fields.push(("ep", toml::Value::String(ep_spec)));
    }
    let card_id = row_str(&row, "cardId");
    if !card_id.is_empty() {
        fields.push(("cardId", toml::Value::String(card_id)));
    }
    upsert_delta_arc(&mut manifest, &fields);
    if let Err(e) = dump(&manifest_path, &manifest) {
        return ToolOutput::crash(e);
    }
    let n_steps = tour["steps"].as_array().map(|a| a.len()).unwrap_or(0);
    stdout.push_str(&format!(
        "[OK] materialized arc {arc_id}: {n_steps} steps -> {}\n",
        out_path.display()
    ));
    stdout.push_str(&format!(
        "[OK] manifest row upsert: arcId={arc_id} quality={quality} tourPath={tour_rel}\n"
    ));
    stdout.push_str(
        "[LOG] delta tour 帶 tour-level ref（after commit）——歷史快照，永不 living-reanchor\n",
    );
    ToolOutput {
        stdout,
        stderr,
        exit_code: 0,
    }
}

/// Route a `code-reality tour ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_tour, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 tour");
    };
    match toks.first() {
        Some(&"register") => register(&toks[1..]),
        Some(&"materialize") => materialize(&toks[1..]),
        _ => ToolOutput::fail(
            "需提供子命令：tour register <arcId> --base X --target Y [--ep E] [--card C] ／ tour materialize <arcId>",
        ),
    }
}
