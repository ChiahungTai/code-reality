//! `tour_manifest` — the frozen `code_reality/tour_manifest.py` contract:
//! `.tours/manifest.toml` read/write (the derived/curated split as
//! source×generator×anchored_commit). dump roundtrips UNKNOWN keys —
//! rebuilding only the known fields would silently delete hand-written
//! entries (the NT `audience` incident, twice).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::ToolOutput;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--tours-dir",
            short: None,
            kind: Kind::Value {
                metavar: "TOURS_DIR",
            },
        },
        FlagSpec {
            long: "--init-scan",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: tour_manifest [-h] [--repo REPO] [--tours-dir TOURS_DIR]\n",
    "                     [--init-scan]\n",
    "\n",
    "manifest 讀寫／--init-scan 骨架生成\n",
    "\n",
    "options:\n",
    "  -h, --help            show this help message and exit\n",
    "  --repo REPO\n",
    "  --tours-dir TOURS_DIR\n",
    "  --init-scan           掃 corpus 生成 manifest 骨架\n",
);

/// git HEAD with the WARN fallback (`tour_manifest.py:20-32`): non-git
/// repo → "[WARN] ... anchored_commit 記 unknown" on stdout, "unknown".
pub fn git_head(repo: &Path) -> (String, String) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("HEAD")
        .output();
    match out {
        Ok(o) if o.status.success() => (
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::new(),
        ),
        _ => (
            "unknown".to_string(),
            format!(
                "[WARN] git HEAD 取不到（{} 非 git repo？）——anchored_commit 記 unknown\n",
                repo.display()
            ),
        ),
    }
}

/// Manifest data model: `version`, `tour` (rel → row), plus unknown
/// top-level keys preserved for roundtripping.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub version: Option<toml::Value>,
    pub tour: BTreeMap<String, toml::Table>,
    pub extra: Vec<(String, toml::Value)>, // insertion-irrelevant: dump sorts
}

pub fn load(path: &Path) -> Result<Manifest, String> {
    if !path.exists() {
        return Ok(Manifest::default());
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("{} 讀取失敗：{}", path.display(), e))?;
    let data: toml::Value =
        toml::from_str(&text).map_err(|e| format!("{} TOML 解析失敗：{}", path.display(), e))?;
    let table = data.as_table().cloned().unwrap_or_default();
    let mut m = Manifest::default();
    for (k, v) in table {
        match k.as_str() {
            "version" => m.version = Some(v),
            "tour" => {
                if let Some(t) = v.as_table() {
                    for (rel, row) in t {
                        if let Some(row_t) = row.as_table() {
                            m.tour.insert(rel.clone(), row_t.clone());
                        }
                    }
                }
            }
            other => m.extra.push((other.to_string(), v)),
        }
    }
    Ok(m)
}

pub fn upsert(m: &mut Manifest, rel: &str, generator: &str, sources: &[String], commit: &str) {
    let mut row = toml::Table::new();
    row.insert(
        "generator".into(),
        toml::Value::String(generator.to_string()),
    );
    row.insert(
        "sources".into(),
        toml::Value::Array(
            sources
                .iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        ),
    );
    row.insert(
        "anchored_commit".into(),
        toml::Value::String(commit.to_string()),
    );
    m.tour.insert(rel.to_string(), row);
}

fn toml_key(key: &str) -> String {
    // bare key ([A-Za-z0-9_-]+) verbatim; otherwise JSON-quoted
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_string()
    } else {
        serde_json::to_string(&serde_json::Value::String(key.to_string())).unwrap()
    }
}

/// Scalar / scalar-list TOML serialization (`tour_manifest.py:71-89`);
/// unsupported types are loud — silently dropping data is worse.
fn toml_value(v: &toml::Value) -> Result<String, String> {
    match v {
        toml::Value::Boolean(b) => Ok(b.to_string()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        toml::Value::Float(f) => {
            if !f.is_finite() {
                return Err(
                    "manifest 頂層鍵含非有限 float（inf/nan）——TOML 無此字面，寫出即非法"
                        .to_string(),
                );
            }
            Ok(format!("{}", f))
        }
        toml::Value::String(s) => {
            // json escaping is a TOML basic-string subset; U+007F differs
            // between the two — escape it or the file is unparseable
            Ok(serde_json::to_string(&serde_json::Value::String(s.clone()))
                .unwrap()
                .replace('\u{7f}', "\\u007f"))
        }
        toml::Value::Array(items) => {
            let mut parts = Vec::new();
            for x in items {
                match x {
                    toml::Value::Boolean(_)
                    | toml::Value::Integer(_)
                    | toml::Value::Float(_)
                    | toml::Value::String(_) => parts.push(toml_value(x)?),
                    _ => {
                        return Err(
                            "manifest 頂層鍵型別不支援保存（list 內非 scalar）——只支援 scalar／scalar list"
                                .to_string(),
                        );
                    }
                }
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
        other => Err(format!(
            "manifest 頂層鍵型別不支援保存（{:?}）——只支援 scalar／scalar list",
            other.type_str()
        )),
    }
}

pub fn dump(path: &Path, m: &Manifest) -> Result<(), String> {
    let mut lines = Vec::new();
    let version = m.version.clone().unwrap_or(toml::Value::Integer(1));
    lines.push(format!("version = {}", toml_value(&version)?));
    // unknown top-level keys roundtrip (sorted)
    let mut extra = m.extra.clone();
    extra.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in &extra {
        lines.push(format!("{} = {}", toml_key(k), toml_value(v)?));
    }
    for (rel, row) in &m.tour {
        lines.push(format!("\n[tour.\"{rel}\"]"));
        lines.push(format!(
            "generator = \"{}\"",
            row.get("generator").and_then(|v| v.as_str()).unwrap_or("")
        ));
        let srcs: Vec<String> = row
            .get("sources")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .map(|s| format!("\"{}\"", s.as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();
        lines.push(format!("sources = [{}]", srcs.join(", ")));
        lines.push(format!(
            "anchored_commit = \"{}\"",
            row.get("anchored_commit")
                .and_then(|v| v.as_str())
                .unwrap_or("")
        ));
        // unknown row keys roundtrip (sorted) — untouched rows must not
        // lose data; upserted rows are tool-authoritative full replaces
        let mut unknown: Vec<(&String, &toml::Value)> = row
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "generator" | "sources" | "anchored_commit"))
            .collect();
        unknown.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in unknown {
            lines.push(format!("{} = {}", toml_key(k), toml_value(v)?));
        }
    }
    std::fs::write(path, format!("{}\n", lines.join("\n")))
        .map_err(|e| format!("{} 寫入失敗：{}", path.display(), e))
}

/// Walk up from out_dir to a `.tours` root (`tour_manifest.py:114-119`);
/// the caller judges "not a corpus tree" by `name != ".tours"`.
pub fn tours_root_of(out_dir: &Path) -> PathBuf {
    let mut p = crate::common::resolve(out_dir);
    while !p.file_name().unwrap_or_default().is_empty()
        && p.file_name().unwrap_or_default() != ".tours"
        && p.parent() != Some(&p)
    {
        p = p.parent().unwrap_or(&p).to_path_buf();
    }
    p
}

/// Scan the corpus filling ONLY missing rows (`tour_manifest.py:122-148`):
/// existing generator-written sources are preserved; generator guessed
/// from filename convention (`chain-*` or bare `NN.tour` → chain_tour).
pub fn init_scan(repo: &Path, tours_dir: &Path) -> Result<(Manifest, String), String> {
    let path = repo.join(tours_dir).join("manifest.toml");
    let mut data = load(&path)?;
    if data.version.is_none() {
        data.version = Some(toml::Value::Integer(1));
    }
    let (commit, warn) = git_head(repo);
    let root = repo.join(tours_dir);
    let mut files: Vec<PathBuf> = glob_tours(&root);
    files.sort();
    for f in files {
        let Ok(rel_path) = f.strip_prefix(&root) else {
            continue;
        };
        let parts: Vec<String> = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let dirs = &parts[..parts.len().saturating_sub(1)];
        if dirs.iter().any(|d| d == "delta" || d == "dev-fixture") {
            continue; // regenerable time layer / dev fixtures — out of scope
        }
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        if data.tour.contains_key(&rel) {
            continue;
        }
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = f
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let gen = if name.starts_with("chain-")
            || (stem.is_ascii() && stem.chars().all(|c| c.is_ascii_digit()))
        {
            "chain_tour"
        } else {
            "manual"
        };
        upsert(&mut data, &rel, gen, &[], &commit);
    }
    Ok((data, warn))
}

pub(crate) fn glob_tours(root: &Path) -> Vec<PathBuf> {
    let mut opts = glob::MatchOptions::new();
    opts.require_literal_leading_dot = false;
    let pattern = root.join("**/*.tour").to_string_lossy().into_owned();
    glob::glob_with(&pattern, opts)
        .into_iter()
        .flatten()
        .filter_map(|r| r.ok())
        .collect()
}

/// Route a `code-reality tour_manifest ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 tour_manifest");
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
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let tours_dir = values
        .get("--tours-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tours"));
    let path = repo.join(&tours_dir).join("manifest.toml");
    if !values.contains_key("--init-scan") {
        return ToolOutput {
            stdout: format!(
                "[OK] manifest path: {}（exists={}）\n",
                path.display(),
                path.exists()
            ),
            stderr: String::new(),
            exit_code: 0,
        };
    }
    let (data, warn) = match init_scan(&repo, &tours_dir) {
        Ok(v) => v,
        Err(e) => return ToolOutput::crash(e),
    };
    if let Err(e) = dump(&path, &data) {
        return ToolOutput::crash(e);
    }
    ToolOutput {
        stdout: format!(
            "{}[OK] manifest init: {} rows -> {}\n",
            warn,
            data.tour.len(),
            path.display()
        ),
        stderr: String::new(),
        exit_code: 0,
    }
}
