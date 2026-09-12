//! `tour` — intent-level tour materialization umbrella (AIR-80).
//!
//! `tour materialize <arcId>` assembles the FULL recipe from the manifest
//! provenance row (`[[delta_arc]]`): snapshot-pair resolution by commit
//! sha8, EP claims gate, `delta_tour::build_tour`, `.tours/delta/` output,
//! and row upsert (`tourPath`). The extension/consumer face (ai-lifecycle)
//! passes intent only — never raw snapshot paths or flag assembly (missed
//! `--primary`-style silent drops are the recorded trap this inverts).
//!
//! Re-materialization of the same arcId OVERWRITES the same tourPath:
//! arcId is the canonical key (idempotent materialization; history lives
//! in git). Rows are retained, never deleted — consumers locate tours by
//! row and read tolerantly (missing field = no trigger UI).

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
    "usage: code-reality tour materialize [-h] [--repo REPO]\n",
    "       [--base BASE_SHA] [--target TARGET_SHA] [--ep EP_MD]\n",
    "       [--card CARD_ID] arcId\n",
    "\n",
    "intent-level delta materialization——依 manifest provenance row 組完整 recipe\n",
    "（snapshot pair 解析＋EP gate＋delta_tour build＋row upsert）。\n",
    "\n",
    "positional arguments:\n",
    "  arcId                 materialization canonical key（同 arcId 重產＝覆蓋同 tourPath）\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO           repo 根（預設 cwd）\n",
    "  --base BASE_SHA       首次註冊用——弧 baseline commit（row 已在時可省）\n",
    "  --target TARGET_SHA   首次註冊用——弧 target commit（row 已在時可省）\n",
    "  --ep EP_MD            EP markdown 路徑（宣稱對照；缺席→quality=degraded）\n",
    "  --card CARD_ID        join 屬性（一卡可多弧；可選）\n",
);

fn row_str(row: &toml::Table, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Locate `<snapshots>/<name>-<sha8>.json` by commit sha prefix (8 chars).
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

fn materialize(argv: &[&str]) -> ToolOutput {
    let (values, positionals) = match parse(&SPEC, argv) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok {
            values,
            positionals,
        } => (values, positionals),
    };
    let Some(arc_id) = positionals.first() else {
        return ToolOutput::fail("需提供 arcId（materialization canonical key）");
    };
    if arc_id.is_empty() {
        return ToolOutput::crash("arcId 不得為空字串");
    }
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = crate::common::resolve(&repo);
    let manifest_path = repo.join(".tours").join("manifest.toml");
    let mut manifest: Manifest = match load(&manifest_path) {
        Ok(m) => m,
        Err(e) => return ToolOutput::crash(e),
    };
    // register form: full args present → authoritative row replace
    let reg_base = values.get("--base").and_then(|v| v.clone());
    let reg_target = values.get("--target").and_then(|v| v.clone());
    let reg_ep = values.get("--ep").and_then(|v| v.clone());
    let card = values.get("--card").and_then(|v| v.clone());
    if reg_base.is_some() || reg_target.is_some() || reg_ep.is_some() || card.is_some() {
        let (Some(base), Some(target)) = (reg_base.clone(), reg_target.clone()) else {
            return ToolOutput::crash("註冊形需 --base 與 --target 成對（--ep/--card 可選）");
        };
        let mut fields: Vec<(&str, toml::Value)> = vec![
            ("arcId", toml::Value::String(arc_id.clone())),
            ("base", toml::Value::String(base)),
            ("target", toml::Value::String(target)),
        ];
        if let Some(ep) = &reg_ep {
            fields.push(("ep", toml::Value::String(ep.clone())));
        }
        if let Some(c) = &card {
            fields.push(("cardId", toml::Value::String(c.clone())));
        }
        upsert_delta_arc(&mut manifest, &fields);
    }
    let row = manifest
        .delta_arc
        .iter()
        .find(|r| row_str(r, "arcId") == *arc_id)
        .cloned();
    let Some(row) = row else {
        return ToolOutput::crash(format!(
            "arcId {arc_id} 無 provenance row——首次 materialize 需 --base/--target（＋--ep/--card）註冊"
        ));
    };
    let base = row_str(&row, "base");
    let target = row_str(&row, "target");
    if base.is_empty() || target.is_empty() {
        return ToolOutput::crash(format!("row {arc_id} 缺 base/target——以註冊形補齊"));
    }
    let ep: Option<PathBuf> = values
        .get("--ep")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .or_else(|| {
            let e = row_str(&row, "ep");
            (!e.is_empty()).then(|| repo.join(e))
        });
    let snapshots = repo.join(".code-reality").join("snapshots");
    let sa = match snapshot_for(&snapshots, &base) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(format!("base {base}: {e}")),
    };
    let sb = match snapshot_for(&snapshots, &target) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(format!("target {target}: {e}")),
    };
    let mut stderr = String::new();
    let mut stdout = String::new();
    let sa = match load_snapshot(&sa) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    let sb = match load_snapshot(&sb) {
        Ok(s) => s,
        Err(e) => return ToolOutput::crash(e),
    };
    let profile = match load_profile(&repo) {
        Ok(p) => p,
        Err(e) => return ToolOutput::crash(e),
    };
    if ep.is_some() && profile.is_none() {
        stdout.push_str(
            "[WARN] claims 恆 NONE——repo 無 .code-reality.toml profile，宣稱對照不生效\n",
        );
    }
    let claims = match &ep {
        None => None,
        Some(p) => match extract_ep_claims(p, profile.as_ref(), Some(&repo)) {
            Ok(c) => Some(c),
            Err(e) => return ToolOutput::crash(e),
        },
    };
    let summary = summarize(&sa, &sb);
    let data = render_json_value(&sa, &sb, &summary, claims.as_ref(), profile.as_ref());
    let tour = match build_tour(&data, &repo, ep.as_deref(), arc_id, &mut stderr) {
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
    // row upsert: tourPath + quality (tool-authoritative full replace)
    let quality = if ep.is_some() { "full" } else { "degraded" };
    let tour_rel = format!(".tours/delta/{arc_id}.tour");
    let mut fields: Vec<(&str, toml::Value)> = vec![
        ("arcId", toml::Value::String(arc_id.clone())),
        ("base", toml::Value::String(base)),
        ("target", toml::Value::String(target)),
        ("quality", toml::Value::String(quality.to_string())),
        ("tourPath", toml::Value::String(tour_rel.clone())),
    ];
    if let Some(e) = ep.as_ref() {
        let e_rel = e
            .strip_prefix(&repo)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| e.to_string_lossy().into_owned());
        fields.push(("ep", toml::Value::String(e_rel)));
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
        Some(&"materialize") => materialize(&toks[1..]),
        _ => ToolOutput::fail(
            "需提供子命令：tour materialize <arcId> [--repo R] [--base/--target/--ep/--card]",
        ),
    }
}
