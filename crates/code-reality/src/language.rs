//! language — the source-language / producer-family model for the data
//! plane (JS/TS blueprint S1). Two deliberately separate concepts:
//! [`LanguageFace`] is a document/source language (JavaScript and
//! TypeScript are distinct labels on graph nodes), while
//! [`ProducerFamily`] is an executable producer leg (both JS and TS map
//! to the one `scip-typescript` family). Replaces the binary
//! `RepoKind::{Python,Rust,Mixed}` branching that turns combinatorial at
//! language three.

use std::path::Path;

/// A source-document language. The extension mapping is the single
/// source shared by build detection (S1), graph language labels (S3),
/// and the freshness walk (S4) — no second extension list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LanguageFace {
    Python,
    Rust,
    JavaScript,
    TypeScript,
}

/// An executable producer leg. Orchestration order is the explicit
/// total order [`ProducerFamily::ORDERED`]; merge determinism never
/// depends on hash/map iteration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProducerFamily {
    Python,
    Rust,
    TypeScript,
}

/// The six JS/TS source extensions, in the frozen face order.
pub const JS_TS_EXTS: [&str; 6] = ["js", "jsx", "mjs", "cjs", "ts", "tsx"];

impl LanguageFace {
    /// Document extension → language. Case-sensitive (toolchain faces
    /// are case-sensitive; parity with the old `.ends_with(".py")` walk).
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension().and_then(|e| e.to_str())?;
        Some(match ext {
            "py" => Self::Python,
            "rs" => Self::Rust,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "tsx" => Self::TypeScript,
            _ => return None,
        })
    }

    /// Graph node `language` label (S3).
    pub fn graph_label(self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
        }
    }

    /// The producer leg that owns this language. JS + TS share the
    /// `scip-typescript` family (AD-3).
    pub fn producer(self) -> ProducerFamily {
        match self {
            Self::Python => ProducerFamily::Python,
            Self::Rust => ProducerFamily::Rust,
            Self::JavaScript | Self::TypeScript => ProducerFamily::TypeScript,
        }
    }

    /// Stable lowercase name used in sidecar metadata (`source_faces`).
    pub fn meta_name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Rust => "rust",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
        }
    }
}

impl ProducerFamily {
    /// Deterministic leg/merge order: Python, Rust, TypeScript.
    pub const ORDERED: [Self; 3] = [Self::Python, Self::Rust, Self::TypeScript];

    /// CLI/MCP `--producer` spelling. `typescript` selects the shared
    /// JS/TS family; `javascript` is not a second value (S1 AD-3).
    pub fn cli_name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
        }
    }

    /// Parse the CLI value; unknown → None (the boundary reports the
    /// legal set).
    pub fn parse_cli(s: &str) -> Option<Self> {
        Self::ORDERED.into_iter().find(|f| f.cli_name() == s)
    }

    /// Report `face` fragment for this family's single-leg build.
    pub fn face_name(self) -> &'static str {
        match self {
            Self::Python => "python-face",
            Self::Rust => "rust-face",
            Self::TypeScript => "typescript-face",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_table_for_all_eight_faces() {
        let f = |p: &str| LanguageFace::from_path(Path::new(p));
        assert_eq!(f("a.py"), Some(LanguageFace::Python));
        assert_eq!(f("src/b.rs"), Some(LanguageFace::Rust));
        for ext in ["js", "jsx", "mjs", "cjs"] {
            assert_eq!(
                f(format!("x.{ext}").as_str()),
                Some(LanguageFace::JavaScript),
                "{ext}"
            );
        }
        for ext in ["ts", "tsx"] {
            assert_eq!(
                f(format!("x.{ext}").as_str()),
                Some(LanguageFace::TypeScript),
                "{ext}"
            );
        }
        assert_eq!(f("x.go"), None);
        assert_eq!(f("x.PY"), None, "case-sensitive by design");
        assert_eq!(f("noext"), None);
        assert_eq!(f("x.ts.bak"), None);
    }

    #[test]
    fn producer_mapping_and_order() {
        assert_eq!(
            LanguageFace::JavaScript.producer(),
            ProducerFamily::TypeScript
        );
        assert_eq!(
            LanguageFace::TypeScript.producer(),
            ProducerFamily::TypeScript
        );
        assert_eq!(
            ProducerFamily::ORDERED,
            [
                ProducerFamily::Python,
                ProducerFamily::Rust,
                ProducerFamily::TypeScript
            ]
        );
        assert_eq!(
            ProducerFamily::parse_cli("typescript"),
            Some(ProducerFamily::TypeScript)
        );
        assert_eq!(ProducerFamily::parse_cli("javascript"), None);
        assert_eq!(ProducerFamily::parse_cli("go"), None);
    }

    #[test]
    fn graph_labels_and_meta_names() {
        assert_eq!(LanguageFace::JavaScript.graph_label(), "JavaScript");
        assert_eq!(LanguageFace::TypeScript.graph_label(), "TypeScript");
        assert_eq!(LanguageFace::TypeScript.meta_name(), "typescript");
    }
}
