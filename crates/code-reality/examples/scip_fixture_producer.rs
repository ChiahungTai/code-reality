//! Test-fixture producer: a scip-typescript/pyrefly-index argv-compatible
//! binary that DERIVES its output through the production corpus helpers
//! instead of mirroring them. Test fakes point at this binary, so no
//! shell/python side-channel ever re-implements the corpus walk, the
//! fixture key, or the SCIP protobuf wire format (the mirror-fake family
//! a review closed).
//!
//! Not shipped: `cargo build --release` (the maturin/wheel face) does not
//! build examples; `cargo test` does.
//!
//! Usage shapes (both accept `--version`):
//!   scip_fixture_producer index --cwd <repo> <config.json> \
//!     --no-progress-bar --max-file-byte-size 1mb --output <path>   (ts mode:
//!     one DEF document per file listed in the CR derived config's `files`)
//!   scip_fixture_producer --repo <repo> --out <path>               (py mode:
//!     one DEF document per on-disk .py from the shared source walk)

use code_reality::engine::walk_sources;
use protobuf::Message;
use scip::types::{Document, Index, Occurrence};
use std::path::{Path, PathBuf};

fn emit(path: &Path, docs: Vec<(String, String)>) -> Result<(), String> {
    let mut index = Index::new();
    for (rel, symbol) in docs {
        let mut d = Document::new();
        d.relative_path = rel.clone();
        // three DEF occurrences per document: a one-file corpus must
        // clear the python leg's 128-byte empty-index guard
        for suffix in ["fn1", "fn2", "fn3"] {
            let mut occ = Occurrence::new();
            occ.symbol = format!("{symbol}{suffix}().");
            occ.symbol_roles = 1;
            occ.range = vec![0, 0, 1];
            occ.enclosing_range = vec![0, 0, 1, 1];
            d.occurrences.push(occ);
        }
        index.documents.push(d);
    }
    std::fs::write(path, index.write_to_bytes().map_err(|e| e.to_string())?)
        .map_err(|e| format!("write {}: {e}", path.display()))
}

fn ts_mode(repo: &Path, config: &Path, output: &Path) -> Result<(), String> {
    // Real scip-typescript symbol shape (dumped from actual 0.4.0 output)
    let cfg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(config).map_err(|e| format!("read config: {e}"))?,
    )
    .map_err(|e| format!("parse config: {e}"))?;
    let files = cfg
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("config has no files list")?;
    let dir = repo
        .canonicalize()
        .unwrap_or_else(|_| repo.to_path_buf())
        .display()
        .to_string();
    let mut docs = Vec::new();
    for f in files {
        let abs = f.as_str().ok_or("files entry not a string")?;
        let rel = abs
            .strip_prefix(&format!("{dir}/"))
            .unwrap_or(abs)
            .to_string();
        let name = PathBuf::from(&rel)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        docs.push((
            rel,
            format!("scip-typescript npm . . {dir}/`{name}`/{name}"),
        ));
    }
    docs.sort();
    emit(output, docs)
}

fn py_mode(repo: &Path, output: &Path) -> Result<(), String> {
    let walk = walk_sources(repo)?;
    let mut docs: Vec<(String, String)> = walk
        .py
        .keys()
        .map(|rel| {
            let name = PathBuf::from(rel)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            (rel.clone(), format!("pyrefly python proj 0.1.0 `m`/{name}"))
        })
        .collect();
    docs.sort();
    emit(output, docs)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("0.4.0-fixture");
        std::process::exit(0);
    }
    // pyrefly-index shape: --repo R --out O
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let result = if let (Some(repo), Some(out)) = (flag("--repo"), flag("--out")) {
        py_mode(Path::new(&repo), Path::new(&out))
    } else {
        // scip-typescript shape: index --cwd R <config> --output O
        let cwd = flag("--cwd").expect("--cwd");
        let out = flag("--output").expect("--output");
        let config = args
            .iter()
            .skip(2)
            .find(|a| !a.starts_with('-') && a.ends_with(".json"))
            .cloned()
            .expect("positional config path");
        ts_mode(Path::new(&cwd), Path::new(&config), Path::new(&out))
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
