//! Repo profile — the frozen `code_reality/profile.py` + `exclusions.py`
//! contract: `.code-reality.toml` is the single source of module rules,
//! claims prefixes, exclusions, and boundary scan roots. The tool layer
//! embeds no repo-specific special cases.
//!
//! Crash-only loading (`profile.py:53-117`): missing file → `Ok(None)`
//! (generic fallback); broken TOML / unknown keys / missing required
//! fields / prefix without slash / depth not a ≥1 non-bool integer →
//! `Err` (the caller maps to crash ToolOutput, exit 1 + empty stdout).
//! The `hazard_registry` key is parsed here as ①-foundation (② consumes
//! the rules engine); not modeling it would make a Python-legal profile
//! hit an unknown-key crash on the Rust side.

use std::collections::BTreeSet;
use std::path::Path;

pub const PROFILE_FILENAME: &str = ".code-reality.toml";
pub const DEFAULT_EXCLUDE: [&str; 1] = [".venv/"];

const VALID_KEYS: [&str; 6] = [
    "module",
    "exclude",
    "scan_root",
    "hazard_registry",
    "rust_projects",
    "rust",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRule {
    pub prefix: String,
    pub depth: i64,
}

/// `pyi` is a `.pyi` contract-tree glob string (repo-relative) — the
/// frozen face is `str`, not a bool (`profile.py:30-31`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRoot {
    pub path: String,
    pub pyi: String,
}

/// Registry auto-discovery facts for the hazard rules (②) — repo facts
/// belong to the repo (`profile.py:34-42`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HazardRegistry {
    pub package_prefix: String,
    pub suffix: String,
    pub register_fn: String,
    pub registry: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub modules: Vec<ModuleRule>,
    pub exclude: Vec<String>,
    pub scan_roots: Vec<ScanRoot>,
    pub hazard_registries: Vec<HazardRegistry>,
    /// Configured Cargo-workspace roots (EP DB-100 Q1): `None` = key
    /// absent (conservative auto-discovery); `Some(list)` = config mode
    /// (authoritative — an empty list is a resolver error, not auto).
    pub rust_projects: Option<Vec<String>>,
    /// Global cargo features for the rust producer (CR-16): `Some(list)`
    /// applies the list to EVERY resolved workspace root. Mutually
    /// exclusive with `rust_features_by_root` (parse-time error).
    pub rust_features: Option<Vec<String>>,
    /// Per-root cargo features (CR-16): `[rust.features_by_root]` — the
    /// authoritative map when present (a root it does not list injects
    /// nothing). Vec of `(root_key, features)` pairs carrying the
    /// parser's table order (deterministic for a given file; the
    /// identity helper re-sorts), so multi-workspace repos keep one
    /// profile face. Mutually exclusive with `rust_features`.
    pub rust_features_by_root: Option<Vec<(String, Vec<String>)>>,
}

/// Load the profile at repo root; missing file → `Ok(None)` (generic
/// fallback). Everything malformed → `Err(crash message)`.
pub fn load_profile(repo_root: &Path) -> Result<Option<Profile>, String> {
    let path = repo_root.join(PROFILE_FILENAME);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("{} 讀取失敗：{}", path.display(), e))?;
    let data: toml::Value =
        toml::from_str(&text).map_err(|e| format!("{} TOML 解析失敗：{}", path.display(), e))?;
    let table = data
        .as_table()
        .ok_or_else(|| format!("{} TOML 頂層非表：{:?}", path.display(), data))?;
    let unknown: Vec<String> = table
        .keys()
        .filter(|k| !VALID_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        let mut sorted = unknown;
        sorted.sort();
        return Err(format!(
            "{} 含未知鍵 {}——拼錯 section 名會靜默退化 generic fallback（合法鍵：module／exclude／scan_root／hazard_registry／rust_projects／rust）",
            path.display(),
            py_list_repr(&sorted)
        ));
    }
    let arr = |key: &str| -> Result<Vec<toml::Table>, String> {
        match table.get(key) {
            None => Ok(Vec::new()),
            Some(toml::Value::Array(a)) => a
                .iter()
                .map(|v| {
                    v.as_table().cloned().ok_or_else(|| {
                        format!(
                            "{} schema 不合（缺 '{}'）——[[{}]] 項需為表",
                            path.display(),
                            key.strip_suffix('s').unwrap_or(key),
                            key
                        )
                    })
                })
                .collect(),
            Some(_) => Err(format!(
                "{} schema 不合（[[]] 需陣列）：{}",
                path.display(),
                key
            )),
        }
    };
    let get_str = |t: &toml::Table, k: &str| -> Result<String, String> {
        match t.get(k) {
            Some(toml::Value::String(s)) => Ok(s.clone()),
            Some(other) => Err(schema_msg(&path, &py_repr(other))),
            None => Err(schema_msg(&path, &format!("'{}'", k))),
        }
    };
    let rust_projects = match table.get("rust_projects") {
        None => None,
        Some(toml::Value::Array(a)) => Some(a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("{} rust_projects 項需字串：{:?}", path.display(), v))
            })
            .collect::<Result<Vec<String>, _>>()?),
        Some(_) => {
            return Err(format!("{} rust_projects 需字串陣列", path.display()));
        }
    };
    // [rust] section (CR-16): cargo features for the rust producer.
    // `features` (global list) and `features_by_root` (per-root table)
    // are mutually exclusive — two silent-precedence rules would be
    // worse than one loud error; unknown keys fail loud per the :81
    // convention (a typo'd key would otherwise silently inject nothing).
    let (rust_features, rust_features_by_root) = match table.get("rust") {
        None => (None, None),
        Some(toml::Value::Table(t)) => {
            let unknown: Vec<String> = t
                .keys()
                .filter(|k| k.as_str() != "features" && k.as_str() != "features_by_root")
                .cloned()
                .collect();
            if !unknown.is_empty() {
                let mut sorted = unknown;
                sorted.sort();
                return Err(format!(
                    "{} [rust] 含未知鍵 {}——拼錯鍵會靜默不注入（合法鍵：features／features_by_root）",
                    path.display(),
                    py_list_repr(&sorted)
                ));
            }
            let features = match t.get("features") {
                None => None,
                Some(toml::Value::Array(a)) => Some(a
                    .iter()
                    .map(|v| {
                        v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                            format!("{} rust.features 項需字串：{:?}", path.display(), v)
                        })
                    })
                    .collect::<Result<Vec<String>, _>>()?),
                Some(_) => {
                    return Err(format!("{} rust.features 需字串陣列", path.display()));
                }
            };
            let by_root = match t.get("features_by_root") {
                None => None,
                Some(toml::Value::Table(m)) => Some(m
                    .iter()
                    .map(|(k, v)| -> Result<(String, Vec<String>), String> {
                        let arr = v.as_array().ok_or_else(|| {
                            format!(
                                "{} rust.features_by_root['{k}'] 需字串陣列",
                                path.display()
                            )
                        })?;
                        let feats = arr
                            .iter()
                            .map(|f| {
                                f.as_str().map(|s| s.to_string()).ok_or_else(|| {
                                    format!(
                                        "{} rust.features_by_root['{k}'] 項需字串：{:?}",
                                        path.display(),
                                        f
                                    )
                                })
                            })
                            .collect::<Result<Vec<String>, _>>()?;
                        Ok((k.clone(), feats))
                    })
                    .collect::<Result<Vec<(String, Vec<String>)>, _>>()?),
                Some(_) => {
                    return Err(format!(
                        "{} rust.features_by_root 需表（key=root 路徑、value=字串陣列）",
                        path.display()
                    ));
                }
            };
            if features.is_some() && by_root.is_some() {
                return Err(format!(
                    "{} [rust] features 與 features_by_root 互斥——全域清單或 per-root 表擇一",
                    path.display()
                ));
            }
            (features, by_root)
        }
        Some(_) => {
            return Err(format!("{} rust 需為表（[rust]）", path.display()));
        }
    };
    let mut modules = Vec::new();
    for m in arr("module")? {
        let prefix = get_str(&m, "prefix")?;
        let depth = match m.get("depth") {
            None => 1,
            Some(toml::Value::Integer(i)) => *i,
            Some(other) => {
                return Err(format!(
                    "{} [[module]] depth={} 須為 >= 1 的整數",
                    path.display(),
                    py_repr(other)
                ));
            }
        };
        modules.push(ModuleRule { prefix, depth });
    }
    let exclude: Vec<String> = match table.get("exclude") {
        None => DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("{} exclude 項需字串：{:?}", path.display(), v))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => {
            return Err(format!("{} exclude 需陣列", path.display()));
        }
    };
    let mut scan_roots = Vec::new();
    for s in arr("scan_root")? {
        scan_roots.push(ScanRoot {
            path: get_str(&s, "path")?,
            pyi: get_str(&s, "pyi")?,
        });
    }
    let mut registries = Vec::new();
    for r in arr("hazard_registry")? {
        registries.push(HazardRegistry {
            package_prefix: get_str(&r, "package_prefix")?,
            suffix: get_str(&r, "suffix")?,
            register_fn: get_str(&r, "register_fn")?,
            registry: get_str(&r, "registry")?,
            evidence: match r.get("evidence") {
                None => String::new(),
                Some(toml::Value::String(s)) => s.clone(),
                Some(other) => {
                    return Err(schema_msg(&path, &py_repr(other)));
                }
            },
        });
    }
    for rule in &modules {
        if !rule.prefix.ends_with('/') {
            return Err(format!(
                "{} [[module]] prefix='{}' 須以 / 結尾（目錄粒度）",
                path.display(),
                rule.prefix
            ));
        }
        if rule.depth < 1 {
            return Err(format!(
                "{} [[module]] depth={} 須為 >= 1 的整數",
                path.display(),
                rule.depth
            ));
        }
    }
    for entry in &exclude {
        if entry.is_empty() {
            // Directory entries carry a trailing slash, file entries name
            // the full path — both prefix-match under is_excluded. The
            // dropped trailing-slash assert used to reject the empty
            // string incidentally; under starts_with matching an empty
            // entry would exclude EVERYTHING, so it stays loud.
            return Err(format!(
                "{} exclude 項不可為空字串（會排除全部）",
                path.display()
            ));
        }
    }
    for reg in &registries {
        if !reg.package_prefix.ends_with('/') {
            return Err(format!(
                "{} [[hazard_registry]] package_prefix='{}' 須以 / 結尾（目錄粒度）",
                path.display(),
                reg.package_prefix
            ));
        }
    }
    Ok(Some(Profile {
        modules,
        exclude,
        scan_roots,
        hazard_registries: registries,
        rust_projects,
        rust_features,
        rust_features_by_root,
    }))
}

fn schema_msg(path: &Path, missing: &str) -> String {
    format!(
        "{} schema 不合（缺 {}）——[[module]] 需 prefix（depth 可選）、[[scan_root]] 需 path＋pyi、[[hazard_registry]] 需 package_prefix＋suffix＋register_fn＋registry（evidence 可選）",
        path.display(),
        missing
    )
}

/// Python list repr for message faces: `['a', 'b']`.
fn py_list_repr(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{}'", s)).collect();
    format!("[{}]", inner.join(", "))
}

/// Python `repr()` for the TOML value types that can appear in the
/// assertion messages (int / float / bool / str).
fn py_repr(v: &toml::Value) -> String {
    match v {
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => {
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                format!("{}", f)
            }
        }
        toml::Value::Boolean(b) => if *b { "True" } else { "False" }.to_string(),
        toml::Value::String(s) => format!("'{}'", s),
        other => format!("{:?}", other),
    }
}

/// Repo-relative path → module name (`profile.py:120-138`): first matching
/// rule in order; `depth` directory levels under the prefix; a path
/// segment containing `.` (file at prefix root) maps to the base (F6).
/// No rules / no profile → top-level directory (root files → file name).
pub fn module_of(rel_path: &str, profile: Option<&Profile>) -> String {
    if let Some(p) = profile {
        for rule in &p.modules {
            if let Some(rest) = rel_path.strip_prefix(&rule.prefix) {
                let base = rule.prefix.trim_end_matches('/');
                if rest.is_empty() {
                    return base.to_string();
                }
                let segments: Vec<&str> =
                    rest.split('/').take(rule.depth.max(0) as usize).collect();
                if segments.iter().any(|s| s.contains('.')) {
                    return base.to_string();
                }
                return format!("{}/{}", base, segments.join("/"));
            }
        }
    }
    rel_path.split('/').next().unwrap_or(rel_path).to_string()
}

/// EP-claims regex from `[[module]]` prefixes (`profile.py:141-153`).
/// No rules → never-matching sentinel (the Python `(?!x)x` lookahead is
/// not portable to the regex crate; `[^\s\S]` is the semantic equivalent
/// — an empty complement class can match nothing).
pub fn claims_regex(profile: Option<&Profile>) -> regex::Regex {
    let Some(p) = profile else {
        return regex::Regex::new("[^\\s\\S]").unwrap();
    };
    if p.modules.is_empty() {
        return regex::Regex::new("[^\\s\\S]").unwrap();
    }
    let alts: Vec<String> = p
        .modules
        .iter()
        .map(|r| regex::escape(r.prefix.trim_end_matches('/')))
        .collect();
    regex::Regex::new(&format!("(?:{})/[a-z_0-9]+", alts.join("|"))).unwrap()
}

/// Boundary scan roots; no profile / no `[[scan_root]]` → empty.
pub fn scan_roots(profile: Option<&Profile>) -> &[ScanRoot] {
    profile.map(|p| p.scan_roots.as_slice()).unwrap_or(&[])
}

/// Shared exclusion layer (`exclusions.py:13-16`): directory
/// (trailing-slash) or exact-file entries, both prefix-matched — under
/// starts_with, a full file path is an exact-file match. ADDITIVE
/// semantics (user ruling 2026-09-11 — was
/// replace-on-profile before): builtin [`DEFAULT_EXCLUDE`] ∪
/// `profile.exclude`. A repo profile can only widen the set, never
/// narrow the builtin (the old replace rule silently dropped `.venv/`
/// for every repo carrying a profile).
pub fn is_excluded(rel_path: &str, profile: Option<&Profile>) -> bool {
    // borrow, no Vec rebuild — this sits on per-edge hot loops
    if DEFAULT_EXCLUDE.iter().any(|x| rel_path.starts_with(x)) {
        return true;
    }
    match profile {
        Some(p) => p.exclude.iter().any(|x| rel_path.starts_with(x.as_str())),
        None => false,
    }
}

/// Effective exclusion set with per-entry provenance for the build
/// report: builtins first, then profile order; a prefix listed in both
/// collapses to the builtin source.
pub fn effective_excludes(profile: Option<&Profile>) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    for b in DEFAULT_EXCLUDE {
        out.push((b.to_string(), "內建"));
    }
    if let Some(p) = profile {
        for x in &p.exclude {
            if !out.iter().any(|(e, _)| e == x) {
                out.push((x.clone(), "profile"));
            }
        }
    }
    out
}

/// The cargo features the rust producer runs under (CR-16), as the
/// identity body's domain-extras pairs: `(root_key, "feat1,feat2")`,
/// features sorted+deduplicated per pair and pairs sorted by key — a
/// canonical set encoding shared by the stamp face and the freshness
/// re-evaluation (one helper, two callers; changing only one side would
/// open a false-fresh hole). `features_by_root` is authoritative when
/// present: one pair per LISTED root, an unlisted root injects nothing.
/// Otherwise the global `features` list applies to every root — the
/// helper is root-agnostic (resolved roots are a build-face concern),
/// so the global face is one `"*"`-keyed pair. Empty/absent config →
/// empty Vec → the identity body stays byte-identical to the pre-CR-16
/// layout (no header or algo bump).
pub fn effective_rust_features(profile: Option<&Profile>) -> Vec<(String, String)> {
    let Some(p) = profile else {
        return Vec::new();
    };
    let canon = |feats: &[String]| -> String {
        let set: BTreeSet<&str> = feats.iter().map(|s| s.as_str()).collect();
        set.into_iter().collect::<Vec<_>>().join(",")
    };
    if let Some(by_root) = &p.rust_features_by_root {
        let mut pairs: Vec<(String, String)> = by_root
            .iter()
            .filter(|(_, f)| !f.is_empty())
            .map(|(r, f)| (r.clone(), canon(f)))
            .collect();
        pairs.sort();
        return pairs;
    }
    match &p.rust_features {
        Some(f) if !f.is_empty() => vec![("*".to_string(), canon(f))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_profile(dir: &Path, text: &str) -> std::path::PathBuf {
        let p = dir.join(PROFILE_FILENAME);
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn missing_profile_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load_profile(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn unknown_key_message_lists_sorted_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write_profile(
            tmp.path(),
            "zzz = 1\n[[module]]\nprefix = \"a/\"\n[[whatever]]\n",
        );
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(err.contains(&format!("{}", p.display())), "{err}");
        assert!(err.contains("含未知鍵 ['whatever', 'zzz']"), "{err}");
    }

    #[test]
    fn prefix_without_slash_crashes() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "[[module]]\nprefix = \"src\"\n");
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(err.contains("prefix='src' 須以 / 結尾"), "{err}");
    }

    #[test]
    fn depth_float_and_bool_crash_with_py_repr() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "[[module]]\nprefix = \"a/\"\ndepth = 1.5\n");
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(err.contains("depth=1.5 須為 >= 1 的整數"), "{err}");
        write_profile(tmp.path(), "[[module]]\nprefix = \"a/\"\ndepth = true\n");
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(err.contains("depth=True 須為 >= 1 的整數"), "{err}");
    }

    #[test]
    fn missing_scan_root_key_is_schema_crash() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "[[scan_root]]\npath = \"crates\"\n");
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(err.contains("schema 不合（缺 'pyi'）"), "{err}");
    }

    #[test]
    fn hazard_registry_parsed_and_prefix_checked() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(
            tmp.path(),
            "[[hazard_registry]]\npackage_prefix = \"mosaic_alpha/domain/\"\nsuffix = \"Condition\"\nregister_fn = \"register\"\nregistry = \"REG\"\n",
        );
        let p = load_profile(tmp.path()).unwrap().unwrap();
        assert_eq!(p.hazard_registries.len(), 1);
        assert_eq!(p.hazard_registries[0].suffix, "Condition");
        assert_eq!(p.hazard_registries[0].evidence, "");
        write_profile(
            tmp.path(),
            "[[hazard_registry]]\npackage_prefix = \"mosaic_alpha\"\nsuffix = \"C\"\nregister_fn = \"r\"\nregistry = \"R\"\n",
        );
        let err = load_profile(tmp.path()).unwrap_err();
        assert!(
            err.contains("package_prefix='mosaic_alpha' 須以 / 結尾"),
            "{err}"
        );
    }

    #[test]
    fn module_of_f6_and_depth_and_generic() {
        let p = Profile {
            modules: vec![ModuleRule {
                prefix: "crates/".into(),
                depth: 2,
            }],
            exclude: vec![],
            scan_roots: vec![],
            hazard_registries: vec![],
            rust_projects: None,
            rust_features: None,
            rust_features_by_root: None,
        };
        assert_eq!(module_of("crates/a/b/c/mod.rs", Some(&p)), "crates/a/b");
        assert_eq!(module_of("crates/lib.rs", Some(&p)), "crates"); // F6 root file
                                                                    // F6: a file segment inside the depth window maps to the base
                                                                    // (verified against the Python oracle: segments[:2]=["a","mod.rs"]
                                                                    // contains an extension → base)
        assert_eq!(module_of("crates/a/mod.rs", Some(&p)), "crates");
        assert_eq!(module_of("top/x.py", Some(&p)), "top");
        assert_eq!(module_of("rootfile.py", None), "rootfile.py");
        assert_eq!(module_of("a/b.py", None), "a");
    }

    #[test]
    fn claims_regex_never_matches_without_rules() {
        let re = claims_regex(None);
        assert_eq!(re.find_iter("crates/a and crates/b").count(), 0);
        let p = Profile {
            modules: vec![ModuleRule {
                prefix: "crates/".into(),
                depth: 1,
            }],
            exclude: vec![],
            scan_roots: vec![],
            hazard_registries: vec![],
            rust_projects: None,
            rust_features: None,
            rust_features_by_root: None,
        };
        let re = claims_regex(Some(&p));
        let hits: Vec<&str> = re
            .find_iter("see crates/engine and crates/cache here")
            .map(|m| m.as_str())
            .collect();
        assert_eq!(hits, vec!["crates/engine", "crates/cache"]);
    }

    #[test]
    fn exclusion_default_and_profile() {
        assert!(is_excluded(".venv/x.py", None));
        assert!(!is_excluded(".venv-setup.py", None));
        let p = Profile {
            modules: vec![],
            exclude: vec!["ai-analysis/".into()],
            scan_roots: vec![],
            hazard_registries: vec![],
            rust_projects: None,
            rust_features: None,
            rust_features_by_root: None,
        };
        assert!(is_excluded("ai-analysis/ep.md", Some(&p)));
        // Additive (2026-09-11 ruling): a profile widens, never narrows —
        // the builtin survives alongside profile entries.
        assert!(is_excluded(".venv/x.py", Some(&p)));
        assert!(!is_excluded(".venv-setup.py", Some(&p)));
    }

    #[test]
    fn effective_excludes_provenance_and_dedupe() {
        assert_eq!(
            effective_excludes(None),
            vec![(".venv/".to_string(), "內建")]
        );
        let p = Profile {
            modules: vec![],
            exclude: vec![".venv/".into(), "dist/".into()],
            scan_roots: vec![],
            hazard_registries: vec![],
            rust_projects: None,
            rust_features: None,
            rust_features_by_root: None,
        };
        assert_eq!(
            effective_excludes(Some(&p)),
            vec![
                (".venv/".to_string(), "內建"),
                ("dist/".to_string(), "profile"),
            ]
        );
    }
}
