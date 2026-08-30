//! SCIP protobuf index assembly. Occurrence ranges are
//! `[startLine, startCharacter, endLine, endCharacter]` — **0-based**
//! per the SCIP spec and this pipeline's contract (`engine::ln` adds
//! the +1 on read; characters counted in Unicode scalar values —
//! downstream consumers only read the line).

use std::collections::HashMap;
use std::path::Path;

use ruff_text_size::{TextRange, TextSize};
use scip::types as scip_t;

use crate::symbol;
use crate::walk::RefKind;

pub struct IndexEmitter {
    index: scip_t::Index,
    /// rel path → (source, line-start byte offsets)
    modules: HashMap<String, (String, Vec<usize>)>,
    current: Option<String>,
}

impl IndexEmitter {
    pub fn new() -> Self {
        let mut metadata = scip_t::Metadata::default();
        let mut tool_info = scip_t::ToolInfo::default();
        tool_info.name = symbol::PRODUCER_NAME.to_string();
        tool_info.version = symbol::PRODUCER_VERSION.to_string();
        metadata.tool_info = Some(tool_info).into();
        let mut index = scip_t::Index::default();
        index.metadata = Some(metadata).into();
        Self {
            index,
            modules: HashMap::new(),
            current: None,
        }
    }

    pub fn start_module(&mut self, rel_path: &str, source: &str) {
        let line_starts = line_starts(source.as_bytes());
        self.modules
            .insert(rel_path.to_string(), (source.to_string(), line_starts));
        self.index.documents.push(scip_t::Document {
            relative_path: rel_path.to_string(),
            ..Default::default()
        });
        self.current = Some(rel_path.to_string());
    }

    fn push_occurrence(&mut self, symbol_str: &str, range: TextRange, roles: i32) {
        let Some(rel) = self.current.clone() else {
            return;
        };
        let (src, line_starts) = &self.modules[&rel];
        let occ = scip_t::Occurrence {
            symbol: symbol_str.to_string(),
            range: self.scip_range(src, line_starts, range),
            symbol_roles: roles,
            ..Default::default()
        };
        self.index
            .documents
            .last_mut()
            .expect("start_module pushed a document")
            .occurrences
            .push(occ);
    }

    pub fn push_def(&mut self, symbol_str: &str, name_range: TextRange, node_range: TextRange) {
        let Some(rel) = self.current.clone() else {
            return;
        };
        let (src, line_starts) = &self.modules[&rel];
        let mut occ = scip_t::Occurrence {
            symbol: symbol_str.to_string(),
            range: self.scip_range(src, line_starts, name_range),
            symbol_roles: scip_t::SymbolRole::Definition as i32,
            ..Default::default()
        };
        // The DEF occurrence's range is the name position (cache ingest
        // reads its line); the FULL node range rides enclosing_range —
        // engine::fn_spans builds caller-attribution spans from it.
        occ.enclosing_range = self.scip_range(src, line_starts, node_range);
        self.index
            .documents
            .last_mut()
            .expect("start_module pushed a document")
            .occurrences
            .push(occ);
    }

    pub fn push_reference(&mut self, symbol_str: &str, range: TextRange, kind: RefKind) {
        let roles = match kind {
            RefKind::Load => scip_t::SymbolRole::ReadAccess as i32,
            RefKind::Import => scip_t::SymbolRole::Import as i32,
        };
        self.push_occurrence(symbol_str, range, roles);
    }

    /// A resolved call site: SCIP has no call-role bit — the call is the
    /// callee symbol's reference occurrence at the callee position; the
    /// CALLS-vs-REFERENCES edge split is derived downstream (shared
    /// pipeline, occurrence EP S3-F2 jurisdiction).
    pub fn push_call_reference(&mut self, symbol_str: &str, func_range: TextRange) {
        self.push_occurrence(
            symbol_str,
            func_range,
            scip_t::SymbolRole::ReadAccess as i32,
        );
    }

    fn scip_range(&self, src: &str, line_starts: &[usize], range: TextRange) -> Vec<i32> {
        let (l1, c1) = line_col(src, line_starts, range.start());
        let (l2, c2) = line_col(src, line_starts, range.end());
        vec![l1, c1, l2, c2]
    }

    /// Atomic slot write: tmp-sibling + rename. Concurrent readers (the
    /// query-time heal makes concurrent producers routine) must see either
    /// the old or the new index, never a torn write (`cache.rs build_db` /
    /// `concat_scip` precedent). The dot prefix keeps walk faces away; a
    /// leftover tmp from a crashed run is removed defensively first.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        use protobuf::Message;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let bytes = self
            .index
            .write_to_bytes()
            .map_err(|e| format!("protobuf encode: {e:?}"))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = path.with_file_name(format!(".{name}.tmp"));
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "rename {} → {}: {e}",
                tmp.display(),
                path.display()
            ));
        }
        Ok(())
    }
}

fn line_starts(src: &[u8]) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in src.iter().enumerate() {
        if *b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

/// 0-based (line, character) of a byte offset; characters are Unicode
/// scalar values in the line prefix.
fn line_col(src: &str, line_starts: &[usize], off: TextSize) -> (i32, i32) {
    let off = usize::from(off);
    let line_idx = match line_starts.binary_search(&off) {
        Ok(i) => i,
        Err(i) => i - 1,
    };
    let start = line_starts[line_idx];
    let prefix = &src[start..off.max(start)];
    (line_idx as i32, prefix.chars().count() as i32)
}
