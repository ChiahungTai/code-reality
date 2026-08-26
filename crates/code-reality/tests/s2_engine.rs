//! S2 engine tests: predicate boundaries, display semantics, scan order,
//! report assembly, slot/meta/git/[SRC] variants. Boundary cases mirror the
//! Python matcher docstring (`my_open`/`reopen`) and rich_index() shapes.

use code_reality::engine::*;
use scip::types::{Document, Index, Occurrence};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn occ(symbol: &str, line: i32, is_def: bool, range_len: usize) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.range = if range_len >= 2 {
        vec![line, 0]
    } else {
        vec![]
    };
    o.symbol_roles = if is_def { 1 } else { 0 };
    o
}

fn doc(rel_path: &str, occurrences: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    d.relative_path = rel_path.to_string();
    d.occurrences = occurrences;
    d
}

const IMPL_VARIANT: &str =
    "rust-analyzer cargo nautilus_trader 0.1.0 kernel/impl#[EventStoreLifecycle][KernelEventStore]open().";
const INHERENT: &str =
    "rust-analyzer cargo nautilus_trader 0.1.0 kernel/impl#[EventStoreLifecycle]open().";
const TRAIT_DECL: &str =
    "rust-analyzer cargo nautilus_trader 0.1.0 kernel/EventStoreLifecycle#open().";

#[test]
fn name_pat_boundaries_match_python_docstring() {
    // trailing open(). with non-word char before
    assert!(name_pat_match(IMPL_VARIANT, "open"));
    // `my_open().` / `reopen().` must NOT match bare `open` (word boundary)
    assert!(!name_pat_match("kernel/my_open().", "open"));
    assert!(!name_pat_match("kernel/reopen().", "open"));
    // start-of-string boundary matches
    assert!(name_pat_match("open().", "open"));
    // different tail
    assert!(!name_pat_match(INHERENT, "close"));
    // Unicode word boundary: `é` is a word char (Python \w is Unicode-aware)
    assert!(!name_pat_match("kernel/café_open().", "open"));
    assert!(name_pat_match("kernel/café.open().", "open"));
}

#[test]
fn type_method_matcher_requires_marker_or_trait_decl() {
    let q = Query::parse("EventStoreLifecycle.open");
    assert!(matches_query(IMPL_VARIANT, &q)); // marker [EventStoreLifecycle]
    assert!(matches_query(TRAIT_DECL, &q)); // trait decl Type#method
    assert!(matches_query(INHERENT, &q)); // marker
                                          // name tail matches but neither marker nor trait decl
    assert!(!matches_query(
        "rust-analyzer cargo x 0.1.0 kernel/impl#[Other]open().",
        &q
    ));
    // trait decl preceded by word char (e.g. MyEventStoreLifecycle#) must not match;
    // nor a `#`-preceded form (Python `(?<![\w#])` excludes both)
    assert!(!matches_query(
        "rust-analyzer cargo x 0.1.0 kernel/trait#MyEventStoreLifecycle#open().",
        &q
    ));
    assert!(!matches_query(
        "rust-analyzer cargo x 0.1.0 kernel/trait#EventStoreLifecycle#open().",
        &q
    ));
}

#[test]
fn bare_matcher_is_name_tail_only() {
    let q = Query::parse("open");
    assert!(matches_query(IMPL_VARIANT, &q));
    assert!(matches_query(TRAIT_DECL, &q));
    assert!(!matches_query("kernel/my_open().", &q));
}

#[test]
fn fn_tail_captures_full_trailing_identifier() {
    assert_eq!(fn_tail_name(IMPL_VARIANT), Some("open"));
    assert_eq!(fn_tail_name("kernel/my_foo()."), Some("my_foo"));
    assert_eq!(fn_tail_name("kernel/not_a_fn"), None);
    assert_eq!(fn_tail_name("kernel/()."), None);
}

#[test]
fn tail_takes_last_space_part_when_more_than_four() {
    assert_eq!(
        tail(IMPL_VARIANT),
        "kernel/impl#[EventStoreLifecycle][KernelEventStore]open()."
    );
    assert_eq!(tail("a b c d"), "a b c d"); // 4 parts → whole string
}

#[test]
fn ln_is_one_based_and_negative_when_range_missing() {
    assert_eq!(ln(&occ("s", 5, true, 2)), 6);
    assert_eq!(ln(&occ("s", 0, true, 0)), -1);
    assert_eq!(loc_line("p", -1), "p:?");
    assert_eq!(loc_line("p", 7), "p:7");
}

fn fixture_index() -> Index {
    let mut index = Index::new();
    index.documents = vec![
        doc(
            "src/kernel.rs",
            vec![
                occ(INHERENT, 543, true, 2),                      // DEF inherent
                occ(IMPL_VARIANT, 1349, true, 2),                 // DEF impl variant
                occ(INHERENT, 1355, false, 2),                    // ref 1
                occ(INHERENT, 1741, false, 2),                    // ref 2
                occ("…#KernelEventStore#other().", 99, false, 2), // unrelated
            ],
        ),
        doc(
            "src/other.rs",
            vec![
                occ(INHERENT, 12, false, 2),     // cross-file ref 3
                occ(IMPL_VARIANT, 30, false, 2), // ref of impl variant
            ],
        ),
    ];
    index
}

#[test]
fn find_defs_and_refs_scan_order_and_symbol_sort() {
    let index = fixture_index();
    let q = Query::parse("EventStoreLifecycle.open");
    let defs = find_defs(&index, &q);
    // Byte sort: '[' (0x5B) < 'o' → impl-variant symbol sorts before inherent.
    let symbols: Vec<&String> = defs.keys().collect();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].as_str(), IMPL_VARIANT);
    assert_eq!(symbols[1].as_str(), INHERENT);
    assert_eq!(
        defs.get(INHERENT).unwrap(),
        &vec!["src/kernel.rs:544".to_string()]
    );
    assert_eq!(
        defs.get(IMPL_VARIANT).unwrap(),
        &vec!["src/kernel.rs:1350".to_string()]
    );

    let set: BTreeSet<String> = defs.keys().cloned().collect();
    let refs = find_refs(&index, &set);
    assert_eq!(
        refs.get(INHERENT).unwrap(),
        &vec![
            "src/kernel.rs:1356".to_string(),
            "src/kernel.rs:1742".to_string(),
            "src/other.rs:13".to_string()
        ]
    );
    assert_eq!(
        refs.get(IMPL_VARIANT).unwrap(),
        &vec!["src/other.rs:31".to_string()]
    );
}

#[test]
fn report_assembles_poc_verified_byte_shape() {
    let index = fixture_index();
    let q = Query::parse("EventStoreLifecycle.open");
    let defs = find_defs(&index, &q);
    let refs = find_refs(&index, &BTreeSet::from_iter(defs.keys().cloned()));
    let (out, code) = report(&defs, &refs, Some("[SRC] line"), "EventStoreLifecycle.open");
    assert_eq!(code, 0);
    let expected = concat!(
        "[SRC] line\n",
        "[OK] kernel/impl#[EventStoreLifecycle][KernelEventStore]open().\n",
        "  DEF  src/kernel.rs:1350\n",
        "  refs: 1 處（跨檔）\n",
        "    src/other.rs:31\n",
        "[OK] kernel/impl#[EventStoreLifecycle]open().\n",
        "  DEF  src/kernel.rs:544\n",
        "  refs: 3 處（跨檔）\n",
        "    src/kernel.rs:1356\n",
        "    src/kernel.rs:1742\n",
        "    src/other.rs:13\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn report_empty_defs_is_warn_exit_1() {
    let (out, code) = report(&BTreeMap::new(), &HashMap::new(), None, "nope");
    assert_eq!(code, 1);
    assert_eq!(out, "[WARN] 查無 DEF：nope\n");
}

#[test]
fn slot_resolution_is_repo_basename_keyed() {
    let tmp = tempfile::tempdir().unwrap();
    let p = default_index_path(tmp.path()).unwrap();
    let base = tmp
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(
        p,
        expand_home(DEFAULT_INDEX_ROOT)
            .join(base)
            .join("index.scip")
    );
}

#[test]
fn meta_corrupt_shapes_warn_and_return_none() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = tmp.path().join("index.scip");
    std::fs::write(&idx, b"x").unwrap();
    std::fs::write(meta_path(&idx), "{ not json").unwrap();
    let (v, warns) = load_meta(&idx);
    assert!(v.is_none());
    assert_eq!(warns.len(), 1);
    assert!(warns[0].contains("[WARN] index meta 損壞"));

    std::fs::write(meta_path(&idx), r#"{"no_head": 1}"#).unwrap();
    let (v, warns) = load_meta(&idx);
    assert!(v.is_none());
    assert!(warns[0].contains("形狀非預期"));
}

#[test]
fn git_head_fails_loudly_outside_a_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let res = git_head(tmp.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("[WARN] git rev-parse 失敗"));
}

#[test]
fn source_line_variants() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = tmp.path().join("index.scip");
    std::fs::write(&idx, b"scip").unwrap();

    // No meta, no repo → None
    let (line, _) = source_line(&idx, None);
    assert!(line.is_none());

    // Repo without meta → [SRC] carries only the repo part + 未 stamp WARN
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&idx, b"scip2").unwrap(); // rewrite so index is newest
    let (line, warns) = source_line(&idx, Some(&repo));
    assert!(line.as_deref().unwrap().starts_with("[SRC] repo HEAD @ "));
    assert!(warns.iter().any(|w| w.contains("index meta 未 stamp")));

    // Meta stamped with the live head → both parts, no drift/stale/mismatch warns
    // (meta is written last → newer than index → no stale-stamp warn)
    let head = git_head(&repo).unwrap(); // Ok variant
    std::fs::write(
        meta_path(&idx),
        format!(
            r#"{{"repo": "{}", "head": "{}", "stamped_at": "2026-08-24T13:45:02+00:00", "tool": "code_reality.scip_refs"}}"#,
            repo.display(),
            head
        ),
    )
    .unwrap();
    let (line, warns) = source_line(&idx, Some(&repo));
    assert_eq!(
        line.as_deref().unwrap(),
        format!(
            "[SRC] scip index @ {}（2026-08-24） · repo HEAD @ {}",
            &head[..7],
            &head[..7]
        )
    );
    assert!(warns.is_empty(), "warns should be empty, got: {:?}", warns);

    // Drift: stamp a wrong head → drift WARN, [SRC] still shows both (short) shas
    std::fs::write(
        meta_path(&idx),
        format!(
            r#"{{"repo": "{}", "head": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "stamped_at": "2026-08-24T13:45:02+00:00", "tool": "code_reality.scip_refs"}}"#,
            repo.display()
        ),
    )
    .unwrap();
    let (line, warns) = source_line(&idx, Some(&repo));
    assert!(line.is_some());
    assert!(warns
        .iter()
        .any(|w| w.contains("repo HEAD 已離開 index 生成點")));

    // [SRC] index-only variant (stamped meta, no --repo): scip part alone.
    // Absent stamped_at key → no （date） suffix (Python get-default "").
    std::fs::write(
        meta_path(&idx),
        r#"{"repo": "/x", "head": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "tool": "code_reality.scip_refs"}"#,
    )
    .unwrap();
    let (line, warns) = source_line(&idx, None);
    assert_eq!(line.as_deref(), Some("[SRC] scip index @ deadbee"));
    assert!(warns.is_empty());

    // Explicit null stamped_at → （None） (Python str(None))
    std::fs::write(
        meta_path(&idx),
        r#"{"repo": "/x", "head": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "stamped_at": null, "tool": "code_reality.scip_refs"}"#,
    )
    .unwrap();
    let (line, _) = source_line(&idx, None);
    assert_eq!(line.as_deref(), Some("[SRC] scip index @ deadbee（None）"));
}
