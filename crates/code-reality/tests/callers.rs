//! R3 S1 callers tests: fn-span parsing (4/3-element enc, 0-based→1-based,
//! unexpected-arity WARN), flat refs rows, innermost tie attribution
//! (nesting / same-width first-seen / inclusive bounds / item-level
//! separation), and closure BFS (depth truncation, cycles, per-file
//! aggregation, zero-frontier).

use code_reality::callers::{attribute, closure, Caller, CallersResult};
use code_reality::engine::{fn_spans, refs_rows, FnSpan};
use scip::types::{Document, Index, Occurrence};
use std::collections::BTreeMap;

fn occ(symbol: &str, roles: i32, range: Vec<i32>, enc: Option<Vec<i32>>) -> Occurrence {
    let mut o = Occurrence::new();
    o.symbol = symbol.to_string();
    o.symbol_roles = roles;
    o.range = range;
    if let Some(e) = enc {
        o.enclosing_range = e;
    }
    o
}

fn doc(rel: &str, occs: Vec<Occurrence>) -> Document {
    let mut d = Document::new();
    d.relative_path = rel.to_string();
    d.occurrences = occs;
    d
}

fn index(docs: Vec<Document>) -> Index {
    let mut i = Index::new();
    i.documents = docs;
    i
}

// ---------- fn_spans ----------

#[test]
fn fn_spans_parses_four_and_three_element_enc() {
    let idx = index(vec![doc(
        "a.rs",
        vec![
            occ(
                "cargo x kernel/outer().",
                1,
                vec![9, 0],
                Some(vec![9, 0, 11, 5]),
            ),
            occ(
                "cargo x kernel/macro_fn().",
                1,
                vec![19, 2],
                Some(vec![19, 2, 44]),
            ),
            // non-fn DEF (no (). tail): not a span candidate even with enc
            occ(
                "cargo x kernel/struct.Type",
                1,
                vec![30, 0],
                Some(vec![30, 0, 35, 0]),
            ),
            // ref occurrence with enc: never a span candidate
            occ(
                "cargo x kernel/other().",
                0,
                vec![40, 0],
                Some(vec![40, 0, 45, 0]),
            ),
        ],
    )]);
    let (spans, warns) = fn_spans(&idx);
    assert!(warns.is_empty());
    let a = &spans["a.rs"];
    assert_eq!(a.len(), 2);
    // 4-element [sl,sc,el,ec]=[9,0,11,5] → (10, 12) 1-based inclusive
    assert_eq!((a[0].start_line, a[0].end_line), (10, 12));
    assert_eq!(a[0].symbol, "cargo x kernel/outer().");
    // 3-element [sl,sc,ec]=[19,2,44] → single-line (20, 20) — SM-6
    assert_eq!((a[1].start_line, a[1].end_line), (20, 20));
    assert_eq!(a[0].seq, 0);
    assert_eq!(a[1].seq, 1);
}

#[test]
fn fn_spans_skips_and_warns_on_unexpected_enc_len() {
    let idx = index(vec![doc(
        "a.rs",
        vec![
            occ("cargo x kernel/weird().", 1, vec![1, 0], Some(vec![1, 2])),
            occ(
                "cargo x kernel/ok().",
                1,
                vec![5, 0],
                Some(vec![5, 0, 9, 0]),
            ),
        ],
    )]);
    let (spans, warns) = fn_spans(&idx);
    let a = &spans["a.rs"];
    assert_eq!(a.len(), 1, "2-element enc must be skipped");
    assert_eq!(a[0].symbol, "cargo x kernel/ok().");
    assert_eq!(warns.len(), 1);
    assert!(
        warns[0].contains("weird"),
        "WARN names the skipped symbol: {}",
        warns[0]
    );
}

#[test]
fn fn_spans_absent_enc_skips_silently() {
    let idx = index(vec![doc(
        "a.rs",
        vec![
            occ("cargo x kernel/no_enc().", 1, vec![1, 0], None),
            occ(
                "cargo x kernel/ok().",
                1,
                vec![5, 0],
                Some(vec![5, 0, 9, 0]),
            ),
        ],
    )]);
    let (spans, warns) = fn_spans(&idx);
    assert_eq!(spans["a.rs"].len(), 1, "unset enc → no span");
    assert!(warns.is_empty(), "legal absent enc is silent: {:?}", warns);
}

// ---------- refs_rows ----------

#[test]
fn refs_rows_flat_scan_order_and_non_def_only() {
    let idx = index(vec![
        doc(
            "a.rs",
            vec![
                occ("s1().", 0, vec![9, 0], None),
                occ("s2().", 0, vec![19, 0], None),
                occ("s1().", 1, vec![29, 0], None), // DEF: excluded
            ],
        ),
        doc(
            "b.rs",
            vec![
                occ("s1().", 0, vec![39, 0], None),
                occ("other().", 0, vec![49, 0], None), // not in set
            ],
        ),
    ]);
    let set: std::collections::BTreeSet<String> =
        ["s1().", "s2()."].iter().map(|s| s.to_string()).collect();
    let rows = refs_rows(&idx, &set);
    // global scan order across documents, interleaved across symbols
    assert_eq!(
        rows,
        vec![
            ("s1().".to_string(), "a.rs".to_string(), 10),
            ("s2().".to_string(), "a.rs".to_string(), 20),
            ("s1().".to_string(), "b.rs".to_string(), 40),
        ]
    );
}

// ---------- attribute ----------

fn spans_map(entries: Vec<(&str, Vec<FnSpan>)>) -> BTreeMap<String, Vec<FnSpan>> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn span(symbol: &str, rel: &str, start: i64, end: i64, seq: usize) -> FnSpan {
    FnSpan {
        symbol: symbol.to_string(),
        rel_path: rel.to_string(),
        start_line: start,
        end_line: end,
        seq,
    }
}

fn row(path: &str, line: i64) -> (String, String, i64) {
    ("callee.".to_string(), path.to_string(), line)
}

#[test]
fn attribute_innermost_nested_and_boundaries_inclusive() {
    let spans = spans_map(vec![(
        "a.rs",
        vec![
            span("outer().", "a.rs", 100, 200, 0),
            span("inner().", "a.rs", 120, 130, 1),
        ],
    )]);
    // inside inner → innermost
    let r = attribute(&[row("a.rs", 125)], &spans);
    assert_eq!(r.callers.len(), 1);
    assert_eq!(r.callers[0].symbol, "inner().");
    // only outer contains → outer
    let r = attribute(&[row("a.rs", 110)], &spans);
    assert_eq!(r.callers[0].symbol, "outer().");
    // inclusive bounds: start and end lines of the span attribute to it
    let r = attribute(&[row("a.rs", 120), row("a.rs", 130)], &spans);
    assert_eq!(r.callers.len(), 1);
    assert_eq!(r.callers[0].symbol, "inner().");
    assert_eq!(r.callers[0].sites.len(), 2);
    // span boundary of outer
    let r = attribute(&[row("a.rs", 100), row("a.rs", 200)], &spans);
    assert_eq!(r.callers[0].symbol, "outer().");
}

#[test]
fn attribute_same_width_tie_first_seen_wins() {
    let spans = spans_map(vec![(
        "a.rs",
        vec![
            span("first().", "a.rs", 10, 20, 0),
            span("second().", "a.rs", 10, 20, 1),
        ],
    )]);
    let r = attribute(&[row("a.rs", 15)], &spans);
    assert_eq!(r.callers.len(), 1);
    assert_eq!(
        r.callers[0].symbol, "first().",
        "same-width tie → smaller seq"
    );
}

#[test]
fn attribute_item_level_and_grouping_order() {
    let spans = spans_map(vec![
        ("a.rs", vec![span("one().", "a.rs", 10, 20, 0)]),
        ("b.rs", vec![span("two().", "b.rs", 30, 40, 1)]),
    ]);
    let rows = vec![
        row("a.rs", 12),  // → one
        row("b.rs", 35),  // → two
        row("a.rs", 15),  // → one (second site)
        row("c.rs", 99),  // no span in c.rs → item-level
        row("a.rs", 500), // outside any span → item-level
    ];
    let r = attribute(&rows, &spans);
    assert_eq!(r.callers.len(), 2);
    // first-site scan order: one before two
    assert_eq!(r.callers[0].symbol, "one().");
    assert_eq!(
        r.callers[0].sites,
        vec![("a.rs".to_string(), 12), ("a.rs".to_string(), 15)]
    );
    assert_eq!(r.callers[1].symbol, "two().");
    assert_eq!(r.callers[1].def_path, "b.rs");
    assert_eq!(
        r.item_level,
        vec![("c.rs".to_string(), 99), ("a.rs".to_string(), 500)]
    );
}

// ---------- closure ----------

fn cr(callers: Vec<(&str, &str)>) -> CallersResult {
    CallersResult {
        callers: callers
            .into_iter()
            .map(|(sym, def)| Caller {
                symbol: sym.to_string(),
                def_path: def.to_string(),
                sites: Vec::new(),
            })
            .collect(),
        item_level: Vec::new(),
    }
}

#[test]
fn closure_depth_truncation_and_aggregation() {
    // X ← A (a.rs); A ← B (b.rs)
    let mut table: BTreeMap<String, CallersResult> = BTreeMap::new();
    table.insert("X.".to_string(), cr(vec![("A.", "a.rs")]));
    table.insert("A.".to_string(), cr(vec![("B.", "b.rs")]));
    table.insert("B.".to_string(), cr(vec![]));
    let r = closure(
        &["X.".to_string()],
        |s| {
            cr(table
                .get(s)
                .map(|c| {
                    c.callers
                        .iter()
                        .map(|x| (x.symbol.as_str(), x.def_path.as_str()))
                        .collect()
                })
                .unwrap_or_default())
        },
        1,
    );
    assert_eq!(r.levels.len(), 1);
    assert_eq!(r.levels[0].new_symbols, vec!["A.".to_string()]);
    assert_eq!(r.levels[0].by_file.get("a.rs"), Some(&1));
    assert_eq!(r.cycle_reentries, 0);

    let r = closure(
        &["X.".to_string()],
        |s| {
            cr(table
                .get(s)
                .map(|c| {
                    c.callers
                        .iter()
                        .map(|x| (x.symbol.as_str(), x.def_path.as_str()))
                        .collect()
                })
                .unwrap_or_default())
        },
        2,
    );
    assert_eq!(r.levels.len(), 2);
    assert_eq!(r.levels[1].new_symbols, vec!["B.".to_string()]);
    assert_eq!(r.levels[1].by_file.get("b.rs"), Some(&1));
}

#[test]
fn closure_cycle_detection_no_infinite_loop() {
    // A ↔ B mutual: expanding B yields A (already visited) → cycle
    let expand = |s: &str| -> CallersResult {
        match s {
            "X." => cr(vec![("A.", "a.rs")]),
            "A." => cr(vec![("B.", "b.rs")]),
            "B." => cr(vec![("A.", "a.rs")]),
            _ => cr(vec![]),
        }
    };
    let r = closure(&["X.".to_string()], expand, 5);
    assert_eq!(r.levels[0].new_symbols, vec!["A.".to_string()]);
    assert_eq!(r.levels[1].new_symbols, vec!["B.".to_string()]);
    for lvl in &r.levels[2..] {
        assert!(lvl.new_symbols.is_empty(), "cycle must not rediscover");
    }
    assert_eq!(
        r.cycle_reentries, 1,
        "B re-hits A once; frontier empties after"
    );
    // early termination: no further expansions after the cycle re-entry
}

#[test]
fn closure_zero_frontier_stable_shape() {
    let r = closure(&["X.".to_string()], |_| cr(vec![]), 3);
    assert_eq!(r.levels.len(), 3);
    for lvl in &r.levels {
        assert!(lvl.new_symbols.is_empty());
        assert!(lvl.by_file.is_empty());
    }
    assert_eq!(r.cycle_reentries, 0);
}

#[test]
fn closure_seed_reentry_counts_as_cycle() {
    // self-recursion: A calls itself
    let expand = |s: &str| -> CallersResult {
        match s {
            "X." => cr(vec![("A.", "a.rs")]),
            "A." => cr(vec![("A.", "a.rs")]),
            _ => cr(vec![]),
        }
    };
    let r = closure(&["X.".to_string()], expand, 2);
    assert_eq!(r.levels[0].new_symbols, vec!["A.".to_string()]);
    assert_eq!(
        r.cycle_reentries, 1,
        "A re-hits itself (visited this level)"
    );
}
