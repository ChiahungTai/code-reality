//! js_ts_corpus — the governed JavaScript/TypeScript source set (S2).
//!
//! Single source for the question "which JS/TS documents belong to this
//! repo's CR corpus?": the producer leg (document set authority for the
//! staged partial) and the freshness walk (doc-set convergence) must
//! never answer it twice with different rules (AD-11). The underlying
//! walk is [`crate::engine::walk_sources`] — extension mapping from
//! [`crate::language`], exclusions from the repo-owned profile. No
//! directory name (`dist/`, `build/`, …) is special-cased here: a
//! generated tree is excluded only when the repo's own profile says so.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::SystemTime;

use crate::language::LanguageFace;

/// Governed JS/TS source documents: repo-relative, `/`-normalized.
#[derive(Debug, Default, Clone)]
pub struct JsTsCorpus {
    pub files: BTreeSet<String>,
    pub newest: Option<SystemTime>,
}

/// Collect the governed corpus. Fail-loud: profile parse failures and
/// walk errors propagate (a malformed profile is a crash-face input, not
/// a silent generic fallback).
pub fn collect_js_ts_corpus(repo: &Path) -> Result<JsTsCorpus, String> {
    let walk = crate::engine::walk_sources(repo)?;
    // Normalize separators at collection so the doc says what it does:
    // `/`-normalized everywhere (the producer's normalize_rel then only
    // strips a leading "./" — the exact-set check cannot silently break
    // on backslash-separated walks, muse P2-10).
    let mut files = walk
        .js
        .iter()
        .chain(walk.ts.iter())
        .map(|p| p.replace('\\', "/"))
        .collect::<BTreeSet<String>>();
    let files2 = std::mem::take(&mut files);
    let newest = [LanguageFace::JavaScript, LanguageFace::TypeScript]
        .iter()
        .filter_map(|f| walk.newest_by_face.get(f).copied())
        .max();
    Ok(JsTsCorpus {
        files: files2,
        newest,
    })
}

/// Normalize a producer-side document path to repo-relative `/` form:
/// strip a leading `./`, unify separators. Absolute paths pass through
/// unchanged (membership against repo-relative paths then fails loud at
/// the exact-set check instead of silently dropping documents).
pub fn normalize_rel(p: &str) -> String {
    let p = p.replace('\\', "/");
    p.strip_prefix("./").map(str::to_string).unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_extensions_and_skips() {
        let t = tempfile::tempdir().unwrap();
        for rel in [
            "a.js",
            "b.jsx",
            "c.mjs",
            "d.cjs",
            "e.ts",
            "f.tsx",
            "src/deep/g.mjs",
        ] {
            let p = t.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        // skipped: dot-dir, node_modules, target, non-JS
        for rel in [".hidden/h.js", "node_modules/pkg/i.js", "target/gen.js"] {
            let p = t.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        std::fs::write(t.path().join("notes.txt"), "x").unwrap();

        let c = collect_js_ts_corpus(t.path()).unwrap();
        let expect: BTreeSet<String> = [
            "a.js",
            "b.jsx",
            "c.mjs",
            "d.cjs",
            "e.ts",
            "f.tsx",
            "src/deep/g.mjs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(c.files, expect);
        assert!(c.newest.is_some());
    }

    #[test]
    fn profile_exclusion_removes_generated_tree_only() {
        let t = tempfile::tempdir().unwrap();
        for rel in ["dist/a.mjs", "distx/a.mjs", "src/a.mjs"] {
            let p = t.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        std::fs::write(
            t.path().join(".code-reality.toml"),
            "exclude = [\"dist/\"]\n",
        )
        .unwrap();
        let c = collect_js_ts_corpus(t.path()).unwrap();
        let expect: BTreeSet<String> = ["distx/a.mjs", "src/a.mjs"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(c.files, expect, "prefix-granular exclusion only");
    }

    #[test]
    fn all_excluded_is_empty_not_error() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("gen/a.mjs");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
        std::fs::write(
            t.path().join(".code-reality.toml"),
            "exclude = [\"gen/\"]\n",
        )
        .unwrap();
        let c = collect_js_ts_corpus(t.path()).unwrap();
        assert!(c.files.is_empty());
        assert!(c.newest.is_none());
    }

    #[test]
    fn malformed_profile_fails_loud() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.mjs"), "x").unwrap();
        std::fs::write(t.path().join(".code-reality.toml"), "exclude = 3\n").unwrap();
        assert!(collect_js_ts_corpus(t.path()).is_err());
    }

    #[test]
    fn normalize_rel_shapes() {
        assert_eq!(normalize_rel("./src/a.ts"), "src/a.ts");
        assert_eq!(normalize_rel("src\\a.ts"), "src/a.ts");
        assert_eq!(normalize_rel("src/a.ts"), "src/a.ts");
    }
}
