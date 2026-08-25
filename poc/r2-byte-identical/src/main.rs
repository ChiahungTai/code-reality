//! POC: R2 致命假設——跨語言 byte-identical stdout（Rust scip crate 查詢 vs Python scip_refs）
//!
//! 驗證: ①`scip` crate 可解 rust-analyzer 產出的 NT 真索引（275MB protobuf）
//!       ②Rust 端到端單符號查詢（matcher/DEF/refs/顯示截斷/[SRC]）stdout 與
//!         Python 基準 `cmp` 逐位元組相同＋exit code 一致
//! EP 段落: R2（ep-rust-migration.md 風險清單「致命｜未驗」項）
//! 風險: 致命（不過則整個 Rust 路線重議）
//! 來源: Python 基準 = `uv run python -m code_reality.scip_refs
//!        EventStoreLifecycle.open --repo ~/Github/nautilus_trader`
//!        （sqlite face 回應；本 POC 走 protobuf face 掃描序——跨 face 等價
//!         是額外強化的證據形態）
//!
//! 語意移植自 code_reality/scip_refs.py: _matcher(:135)/find_defs(:162)/
//! find_refs(:173)/report(:182)/ln(:116)/tail(:129)/source_line(:640)。
//! crate 形態：scip 0.9＝rust-protobuf（型別在 scip::types、經 protobuf::Message 解碼）。

use protobuf::Message;
use scip::types::{Index, Occurrence};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const INDEX: &str = "~/.mosaic/code-reality/scip/nautilus_trader/index.scip";
const REPO: &str = "/Users/ctai/Github/nautilus_trader";
const QUERY: &str = "EventStoreLifecycle.open";

fn expand(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("{}/{}", std::env::var("HOME").unwrap(), rest),
        None => p.to_string(),
    }
}

/// Python \w（Unicode word char）的近似——symbol 實務上全 ASCII
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// (?<!\w)open\(\)\.$ —— 字尾 "open()." 且前一碼非 word（或起始處）
fn name_pat_match(s: &str, method: &str) -> bool {
    let needle = format!("{}().", method);
    match s.strip_suffix(&needle) {
        Some(before) => before.chars().next_back().map_or(true, |c| !is_word(c)),
        None => false,
    }
}

/// (?<![\w#])Type# —— "Type#" 前一碼非 word 且非 '#'（或起始處）
fn trait_decl_match(s: &str, type_name: &str) -> bool {
    let pat = format!("{}#", type_name);
    let mut from = 0usize;
    while let Some(pos) = s[from..].find(&pat) {
        let abs = from + pos;
        let ok = abs == 0 || {
            let c = s[..abs].chars().next_back().unwrap();
            !is_word(c) && c != '#'
        };
        if ok {
            return true;
        }
        from = abs + pat.len();
    }
    false
}

fn matcher(s: &str, type_name: &str, method: &str) -> bool {
    name_pat_match(s, method)
        && (s.contains(&format!("[{}]", type_name)) || trait_decl_match(s, type_name))
}

/// tail(): symbol.split(" ") 段數 >4 取末段，否則原字串
fn tail(symbol: &str) -> &str {
    let parts: Vec<&str> = symbol.split(' ').collect();
    if parts.len() > 4 { parts[parts.len() - 1] } else { symbol }
}

fn loc_line(rel_path: &str, line: i64) -> String {
    if line <= 0 {
        format!("{}:?", rel_path)
    } else {
        format!("{}:{}", rel_path, line)
    }
}

fn ln(occ: &Occurrence) -> i64 {
    if occ.range.len() >= 2 {
        occ.range[0] as i64 + 1
    } else {
        -1
    }
}

fn git_head(repo: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let t0 = Instant::now();
    let index_path = expand(INDEX);
    let bytes = std::fs::read(&index_path).unwrap_or_else(|e| {
        eprintln!("[FAIL] 讀索引失敗 {}: {}", index_path, e);
        std::process::exit(2);
    });
    let index = Index::parse_from_bytes(&bytes).unwrap_or_else(|e| {
        eprintln!("[FAIL] 索引解析失敗（損壞/截斷？）：{}", e);
        std::process::exit(2);
    });
    eprintln!(
        "[LOG] decode {} bytes / {} docs in {:?}",
        bytes.len(),
        index.documents.len(),
        t0.elapsed()
    );

    // find_defs：DEF（roles&1）且 matcher 命中；掃描序 append
    let (type_name, method) = QUERY.rsplit_once('.').unwrap();
    let mut defs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 != 0 && matcher(&occ.symbol, type_name, method) {
                defs.entry(occ.symbol.clone())
                    .or_default()
                    .push(loc_line(&d.relative_path, ln(occ)));
            }
        }
    }
    if defs.is_empty() {
        println!("[WARN] 查無 DEF：{}", QUERY);
        std::process::exit(1);
    }

    // find_refs：非 DEF occ、symbol 屬 defs 集
    let symbols: BTreeSet<String> = defs.keys().cloned().collect();
    let mut refs: HashMap<String, Vec<String>> =
        symbols.iter().map(|s| (s.clone(), Vec::new())).collect();
    for d in &index.documents {
        for occ in &d.occurrences {
            if occ.symbol_roles & 1 == 0 && symbols.contains(&occ.symbol) {
                refs.entry(occ.symbol.clone())
                    .or_default()
                    .push(loc_line(&d.relative_path, ln(occ)));
            }
        }
    }

    // [SRC]：meta.head[:7]＋stamped_at[:10]＋live git HEAD[:7]（stdout 面不含 WARN）
    let meta_path = format!("{}.meta.json", index_path);
    let meta: Option<serde_json::Value> =
        std::fs::read_to_string(&meta_path).ok().and_then(|t| serde_json::from_str(&t).ok());
    let idx_sha = meta.as_ref().and_then(|m| m["head"].as_str()).map(str::to_string);
    let repo_sha = git_head(REPO);
    let mut parts: Vec<String> = Vec::new();
    if let Some(sha) = &idx_sha {
        let stamped = meta
            .as_ref()
            .and_then(|m| m["stamped_at"].as_str())
            .unwrap_or("");
        parts.push(format!("scip index @ {}（{}）", &sha[..7], &stamped[..10]));
    }
    if let Some(sha) = &repo_sha {
        parts.push(format!("repo HEAD @ {}", &sha[..7]));
    }
    if !parts.is_empty() {
        println!("[SRC] {}", parts.join(" · "));
    }

    // report()：sorted(defs)；每符號 [OK]/DEF/refs 標題/前 6 refs/截斷行
    for symbol in defs.keys() {
        let r_list = &refs[symbol];
        println!("[OK] {}", tail(symbol));
        for loc_str in &defs[symbol] {
            println!("  DEF  {}", loc_str);
        }
        println!("  refs: {} 處（跨檔）", r_list.len());
        for r in r_list.iter().take(6) {
            println!("    {}", r);
        }
        if r_list.len() > 6 {
            println!("    ...共 {} 處", r_list.len());
        }
    }
    eprintln!("[LOG] total {:?}", t0.elapsed());
}
