//! freshness — the consumer-facing source-identity verdict face (S4,
//! source identity EP). `code-reality freshness --repo <repo> [--json]`
//! answers ONE question for the cross-repo consumer (AIR-135.2): does
//! the indexed source identity equal the requested consumer source
//! identity, dirty working tree included?
//!
//! Verdict face, not a heal face: zero heal is triggered — the only
//! write is the identity cache (D11, same write-back policy as the
//! scip_refs query path). The three exits (D9): fresh=0, stale=1 (a
//! legal answer, never an error — the ToolOutput is constructed
//! directly; `fail` is exit 2 and `crash` is 1+[FAIL]), env/usage/
//! no-slot/check-failure=2 with loud guidance that never claims fresh.
//!
//! Naming neighbors (discrimination line, judge R16/R24): the leaf
//! crate `cr-freshness` and the test file `tests/freshness.rs` are the
//! BINARY version-freshness axis; this subcommand and
//! `tests/source_identity.rs` are the INDEX source-identity axis.

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::engine::{
    default_index_path, evaluate_staleness, load_meta, resolve_repo, StalenessSnapshot,
};
use crate::identity::{resolve_policy, IdentityCachePolicy, IDENTITY_ALGO};
use crate::ToolOutput;
use std::path::Path;
use std::time::Duration;

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--json",
            short: None,
            kind: Kind::StoreTrue,
        },
        FlagSpec {
            long: "--delta-full",
            short: None,
            kind: Kind::StoreTrue,
        },
    ],
    positionals: &[],
};

const HELP: &str = "usage: code-reality freshness --repo <repo> [--json] [--delta-full]
  --repo        repo root whose index slot is judged against its sources
  --json        machine-readable verdict (indexed/current identity pair)
  --delta-full  with --json: enumerate delta paths in full (default take 50;
                counts are always complete)
";

pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((_tool, toks)) = argv.split_first() else {
        return ToolOutput::fail(HELP.trim_end());
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
    let json = values.contains_key("--json");
    let delta_full = values.contains_key("--delta-full");
    let Some(repo) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    freshness_with(Path::new(&repo), json, delta_full)
}

/// The verdict (lib API — returns data, never prints/exits).
pub fn freshness(repo: &Path, json: bool) -> ToolOutput {
    freshness_with(repo, json, false)
}

/// [`freshness`] with the delta-enumeration breadth flag (EP CR-10 S2:
/// bounded path lists take 50 with complete counts; `--delta-full`
/// enumerates in full).
pub fn freshness_with(repo: &Path, json: bool, delta_full: bool) -> ToolOutput {
    let repo = resolve_repo(repo);
    let Ok(slot) = default_index_path(&repo) else {
        // currently infallible; kept for call-site stability
        return ToolOutput::fail("freshness：slot 路徑解析失敗");
    };
    if !slot.exists() {
        // D9 + SM-17: fail loud with build guidance AND the empty-
        // terminal explanation (an all-excluded corpus legitimately
        // converges to slot absence — rebuilding is wrong advice and
        // absence must never read as fresh).
        return ToolOutput::fail(format!(
            "找不到 index slot（{}）——先跑 code-reality build --repo {} 建立索引；若語料全數被 profile 排除，slot 缺席即合法收斂（空終態），無需重建、亦不得視為 fresh",
            slot.display(),
            repo.display()
        ));
    }
    let snap =
        match evaluate_staleness(&repo, &slot, resolve_policy(IdentityCachePolicy::WriteBack)) {
            Ok(s) => s,
            // D15 propagation: a check failure has NO fallback answer on a
            // verdict face — exit 2, never a degraded verdict.
            Err(e) => return ToolOutput::fail(format!("freshness 檢查失敗（{e}）")),
        };
    // D14: head drift never kills fresh (AIR-135.2's LHS has no HEAD);
    // it is disclosed via `head_drift` only.
    let fresh = !snap.needs_rebuild();
    // EP DB-100 S2: a Rust-inclusive upgrade-required slot serves the
    // committed baseline (never legacy-signals/fresh — Cargo-only changes
    // are invisible to its v1 body).
    let serves = if snap.identity_algo_upgrade {
        "committed-baseline"
    } else {
        match snap.identity_drift {
            // legacy 判別優先（R19 observation 1）— 不分 fresh/stale
            None => "legacy-signals",
            Some(false) if fresh => "current-tree",
            // stale and torn-with-matching-identity both serve the
            // committed baseline (graph answers are baseline + live-LSP
            // delta for the working tree)
            Some(_) => "committed-baseline",
        }
    };
    // stale_reasons mapping — fixed order (R24); head_drift never enters.
    let mut reasons: Vec<&str> = Vec::new();
    if snap.graph_lags {
        reasons.push("torn-plane");
    }
    if snap.identity_algo_upgrade {
        reasons.push("identity-algo-upgrade");
    }
    if snap.identity_drift == Some(true) {
        reasons.push("content-drift");
    }
    if snap.doc_set_drift == Some(true) {
        reasons.push("doc-set-drift");
    }
    if snap.corpus_policy_drift == Some(true) {
        reasons.push("policy-drift");
    }
    if snap.identity_drift.is_none() && snap.source_newer {
        reasons.push("legacy-signals");
    }
    let head_drift = snap.head_drift == Some(true);
    let exit_code = if fresh { 0 } else { 1 };
    if json {
        let faces: Vec<&str> = snap.eval_faces.iter().map(|f| f.meta_name()).collect();
        // EP CR-10 S2 delta face: the object is ALWAYS present — members
        // null (with delta_complete:false) exactly when a D15 reason is
        // set. Path lists take 50 unless --delta-full; counts are the
        // complete lengths either way (D13: bounded lists carry complete
        // counts).
        let delta_paths = |paths: &Vec<String>| {
            let take = if delta_full {
                paths.len()
            } else {
                paths.len().min(50)
            };
            serde_json::json!({
                "count": paths.len(),
                "paths": paths[..take],
            })
        };
        let delta = match &snap.delta {
            Some(d) => serde_json::json!({
                "changed": delta_paths(&d.changed),
                "added": delta_paths(&d.added),
                "removed": delta_paths(&d.removed),
                "delta_complete": d.delta_complete,
                "content_verification": d.content_verification.as_str(),
                "out_of_scope_count": d.out_of_scope_count,
                "policy_drift_flags": d.policy_drift_flags.iter().collect::<Vec<_>>(),
            }),
            None => serde_json::json!({
                "changed": null,
                "added": null,
                "removed": null,
                "delta_complete": false,
                "content_verification": null,
                "out_of_scope_count": null,
                "policy_drift_flags": null,
            }),
        };
        let v = serde_json::json!({
            "repo": repo.display().to_string(),
            "slot": slot.display().to_string(),
            "fresh": fresh,
            "stale_reasons": reasons,
            "head_drift": head_drift,
            "faces": faces,
            "indexed_source_identity": snap.stamped_identity,
            "current_source_identity": snap.current_identity,
            "identity_algo": IDENTITY_ALGO,
            "serves": serves,
            "base_commit": snap.base_commit,
            "producer_version": snap.producer_version,
            "delta": delta,
            "delta_identity": snap.delta_identity,
            "delta_unavailable_reason": snap
                .delta_unavailable_reason
                .map(|r| r.as_str()),
        });
        return ToolOutput {
            stdout: format!("{}\n", crate::common::to_json_indent1(&v)),
            stderr: String::new(),
            exit_code,
        };
    }
    let mut stdout = if fresh {
        format!("[OK] fresh——serves {serves}\n")
    } else {
        format!(
            "[WARN] stale——serves {serves}（reasons：{}）\n",
            reasons.join("、")
        )
    };
    if head_drift {
        stdout.push_str(
            "[WARN] head_drift：repo HEAD 已離開 index 生成點——head 不判死，圖事實仍有效\n",
        );
    }
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code,
    }
}

// ---------- per-query serving verdict (EP CR-10 S4) ----------
//
// The query faces' shared annotation source (S4 要點 1): computed ONCE
// at each query-path entry — scip_refs computes one stamp per
// invocation (refs/callers/closure all consume it), graph_query one per
// invocation (all 12 ops consume it), get_community one per call. The
// serving-slot artifact hash is the value evaluate_staleness already
// hashed (D11/fresh-F9: no double compute); the load-time meta snapshot
// supplies the binding check, and a mismatch retries the whole
// computation once before degrading to `indeterminate` (bounded
// consistency — full-timeline ABA linearization is explicitly not
// claimed).

/// Wall-clock ceiling for one verdict computation. Overrun discards the
/// answer and degrades to `indeterminate` + `budget-exceeded` (D15) —
/// the query itself still answers (annotate-only), never the reverse.
/// `CODE_REALITY_VERDICT_BUDGET_MS` overrides (test/gate lever).
pub const DEFAULT_VERDICT_BUDGET_MS: u64 = 1_000;

fn verdict_budget() -> Duration {
    Duration::from_millis(
        std::env::var("CODE_REALITY_VERDICT_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_VERDICT_BUDGET_MS),
    )
}

/// The serving stamp a query's annotations are derived from. `snapshot`
/// is `None` exactly when the verdict computation itself degraded —
/// `reason` then carries the frozen D15 word and every consumer must
/// render `indeterminate` (never infer fresh).
#[derive(Debug, Clone)]
pub struct ServingStamp {
    pub snapshot: Option<StalenessSnapshot>,
    pub reason: Option<crate::engine::DeltaUnavailable>,
}

impl ServingStamp {
    /// The freshness verdict (`freshness --json`'s `fresh` field): the
    /// rebuild decision, head drift excluded (D14).
    pub fn fresh(&self) -> Option<bool> {
        self.snapshot.as_ref().map(|s| !s.needs_rebuild())
    }

    fn degrade(reason: crate::engine::DeltaUnavailable) -> Self {
        Self {
            snapshot: None,
            reason: Some(reason),
        }
    }
}

/// The D11 binding check: the load-time meta must still bind the bytes
/// this snapshot hashed (the meta is the bytes-bound pointer, so a
/// meta/index swap between the verdict and the load is caught here; a
/// bytes-only swap under a stable meta is the disclosed
/// bounded-consistency residual).
fn binding_matches(snapshot: &StalenessSnapshot, slot: &Path) -> bool {
    let (meta, _) = load_meta(slot);
    match (
        &snapshot.serving_artifact_hash,
        meta.as_ref().and_then(|m| m["index_sha256"].as_str()),
    ) {
        (Some(served), Some(bound)) => served == bound,
        // no binding to verify (legacy slot) — other axes degrade on
        // their own terms
        _ => true,
    }
}

/// One verdict computation attempt: evaluate + budget gate + binding
/// check. `Err(reason)` = degraded (the D15 word is final for this
/// query); `Ok(Some(stamp))` = bound and trustworthy; `Ok(None)` =
/// binding mismatch that consumed one retry (the caller stops after
/// two).
fn verdict_attempt(
    repo: &Path,
    slot: &Path,
    started: std::time::Instant,
    budget: Duration,
) -> Result<Option<ServingStamp>, crate::engine::DeltaUnavailable> {
    let snapshot = evaluate_staleness(repo, slot, resolve_policy(IdentityCachePolicy::WriteBack));
    if started.elapsed() > budget {
        return Err(crate::engine::DeltaUnavailable::BudgetExceeded);
    }
    let snapshot = snapshot.map_err(|_| crate::engine::DeltaUnavailable::WalkFailed)?;
    if binding_matches(&snapshot, slot) {
        return Ok(Some(ServingStamp {
            snapshot: Some(snapshot),
            reason: None,
        }));
    }
    if started.elapsed() > budget {
        return Err(crate::engine::DeltaUnavailable::BudgetExceeded);
    }
    Ok(None)
}

/// The per-query serving verdict (EP CR-10 S4/D11). `repo: None` (an
/// explicit `--index` with no source tree) cannot be judged against
/// anything — `indeterminate` + `coverage-unknown`, never a guess; a
/// missing slot likewise degrades (the query may still answer from the
/// graph/cache face — annotate-only, no verdict face here).
pub fn serving_stamp(repo: Option<&Path>, slot: &Path) -> ServingStamp {
    serving_stamp_with_budget(repo, slot, verdict_budget())
}

/// [`serving_stamp`] with an explicit budget (the cost gate's test/gate
/// seam; production callers take the default).
pub fn serving_stamp_with_budget(
    repo: Option<&Path>,
    slot: &Path,
    budget: Duration,
) -> ServingStamp {
    let Some(repo) = repo else {
        return ServingStamp::degrade(crate::engine::DeltaUnavailable::CoverageUnknown);
    };
    let repo = resolve_repo(repo);
    if !slot.exists() {
        return ServingStamp::degrade(crate::engine::DeltaUnavailable::CoverageUnknown);
    }
    let started = std::time::Instant::now();
    // D11 bounded retry: a binding mismatch re-runs the whole
    // computation once (the swap may have settled); a second mismatch
    // means the serving body cannot be attributed — `indeterminate`.
    for attempt in 0..2 {
        match verdict_attempt(&repo, slot, started, budget) {
            Ok(Some(stamp)) => return stamp,
            Ok(None) => {
                if attempt == 1 {
                    return ServingStamp::degrade(
                        crate::engine::DeltaUnavailable::ManifestBindingAuditFailed,
                    );
                }
            }
            Err(reason) => return ServingStamp::degrade(reason),
        }
    }
    unreachable!("retry loop returns on every branch")
}

// ---------- tier vocabulary + per-face resolution (D6/D7/D15) ----------
//
// The frozen vocabulary — `exact | possibly-stale | no-coverage |
// indeterminate`, `unavailable`, and the D15 reason enum. No new words
// may be minted here.

/// What the query found (scip faces): the defs-empty face is the
/// no-coverage axis (D7 — never the negative gate); the two zero-row
/// edge forms (`0 callers` / empty frontier) are the gate's subjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScipResult {
    DefsEmpty,
    EdgesEmpty,
    Populated,
}

/// The scip face's annotation payload: the rendered `[CR_VERDICT]` line
/// plus the D7 exit override (`Some(1)` = typed refusal). Public fields
/// so the display functions stay pure and unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipVerdict {
    pub line: String,
    pub exit_override: Option<i32>,
}

/// Resolve the scip face's tier/exhaustive/refusal triple (frozen D6/D7
/// order) and render the verdict line. Resolution order:
/// indeterminate (degraded verdict) → no-coverage (defs empty —
/// orthogonal to stale) → the negative gate (stale + edge-empty) →
/// possibly-stale → exact.
pub fn scip_verdict(stamp: &ServingStamp, result: ScipResult) -> ScipVerdict {
    let snap = stamp.snapshot.as_ref();
    let stale = snap.is_some_and(|s| s.needs_rebuild()) && snap.is_some_and(|s| s.delta.is_some());
    let (tier, negative, refusal) = if snap.is_none() {
        ("indeterminate", None, false)
    } else if snap.is_some_and(|s| s.delta.is_none()) {
        // legacy/unverifiable provenance (D15) — never exact, never the
        // gate (the gate needs the staleness truth the delta carries)
        ("indeterminate", None, false)
    } else if result == ScipResult::DefsEmpty {
        // defs/refs empty = not-in-graph semantics (D7 no-coverage arm);
        // the family exit-1 face is unchanged
        ("no-coverage", None, false)
    } else if stale && result == ScipResult::EdgesEmpty {
        // D7 hard negative gate: a stale inbound query can never present
        // an empty result as authoritative absence
        ("possibly-stale", Some("unavailable"), true)
    } else if stale {
        ("possibly-stale", None, false)
    } else {
        ("exact", None, false)
    };
    let line = cr_verdict_line(stamp, tier, tier == "exact", negative);
    ScipVerdict {
        line,
        exit_override: refusal.then_some(1),
    }
}

/// The `[CR_VERDICT] v=1 …` line (D8 key set; `delta_paths` is the
/// SM-6/TC-15 observable-intersection carrier — the bounded delta path
/// list, present only when the delta answered and the union is
/// non-empty; `delta_unavailable` is the D15 reason carrier — fields
/// always present, `null` when not applicable).
pub fn cr_verdict_line(
    stamp: &ServingStamp,
    tier: &str,
    exhaustive: bool,
    negative: Option<&str>,
) -> String {
    let snap = stamp.snapshot.as_ref();
    let delta = snap.and_then(|s| s.delta.as_ref());
    let reason = stamp
        .reason
        .or(snap.and_then(|s| s.delta_unavailable_reason))
        .map(|r| r.as_str())
        .unwrap_or("null");
    let (changed, complete, verification) = match delta {
        Some(d) => (
            d.changed.len().to_string(),
            d.delta_complete,
            d.content_verification.as_str().to_string(),
        ),
        None => ("null".to_string(), false, "null".to_string()),
    };
    let producer = snap
        .and_then(|s| s.producer_version.as_deref())
        .unwrap_or("null");
    let mut line = format!(
        "[CR_VERDICT] v=1 tier={tier} exhaustive={exhaustive} negative_verdict={} producer_version={producer} delta_changed={changed} delta_complete={complete} content_verification={verification} delta_unavailable={reason}",
        negative.unwrap_or("null"),
    );
    if let Some(d) = delta {
        let mut paths: Vec<&str> = d
            .changed
            .iter()
            .chain(d.added.iter())
            .chain(d.removed.iter())
            .map(String::as_str)
            .collect();
        if !paths.is_empty() {
            paths.truncate(50);
            line.push_str(&format!(" delta_paths={}", paths.join(",")));
        }
    }
    line
}

// ---------- graph face (D9/D10/D11) ----------

/// The graph generation's provenance, read from the SAME rusqlite
/// connection as the results (D11: tmp+rename keeps an old connection
/// pinned to its own inode, so these values describe the graph the
/// query actually answered from). `None` fields = legacy db (D10).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphProvenance {
    pub snapshot_id: Option<String>,
    pub input_index_sha256: Option<String>,
    pub builder_revision: Option<String>,
}

/// The graph face's resolved annotation. `meta_json` is the compact
/// single-line `[CR-META]` payload (D9 schema v1, always carrying
/// `delta_summary` — D15 fields-never-absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphAnnotation {
    pub tier: &'static str,
    pub exhaustive: bool,
    pub negative_verdict: Option<&'static str>,
    pub meta_json: String,
}

/// Resolve the graph face's annotation. Resolution order: degraded
/// verdict → legacy/unverifiable delta → legacy db (D10:
/// `graph_snapshot_id:null ⇒ indeterminate`) → generation mismatch
/// (SM-10: a query pinned to G1 while the slot moved to S2 — never
/// patched with the new meta) → the negative gate (stale + gated-empty)
/// → possibly-stale → exact. The lsp-harvest face (AM-3) needs no
/// special case: its placeholder index never carries an identity stamp,
/// so it lands in the legacy arm and can never claim fresh/delta.
pub fn graph_annotation(
    stamp: &ServingStamp,
    prov: Option<&GraphProvenance>,
    gated_empty: bool,
) -> GraphAnnotation {
    let snap = stamp.snapshot.as_ref();
    let stale = snap.is_some_and(|s| s.needs_rebuild()) && snap.is_some_and(|s| s.delta.is_some());
    let mut generation_reason = None;
    // degraded verdict and legacy/unverifiable delta resolve alike —
    // indeterminate with the D15 reason carried on the summary
    let (tier, negative) = if snap.is_none_or(|s| s.delta.is_none()) {
        ("indeterminate", None)
    } else if prov.is_some_and(|p| p.snapshot_id.is_none()) {
        // D10: legacy db without the provenance keys
        ("indeterminate", None)
    } else if let (Some(p), Some(served)) =
        (prov, snap.and_then(|s| s.serving_artifact_hash.as_deref()))
    {
        match &p.input_index_sha256 {
            Some(bound) if bound != served => {
                generation_reason = Some(crate::engine::DeltaUnavailable::GraphGenerationMismatch);
                ("indeterminate", None)
            }
            _ => {
                if stale && gated_empty {
                    ("possibly-stale", Some("unavailable"))
                } else if stale {
                    ("possibly-stale", None)
                } else {
                    ("exact", None)
                }
            }
        }
    } else if stale && gated_empty {
        ("possibly-stale", Some("unavailable"))
    } else if stale {
        ("possibly-stale", None)
    } else {
        ("exact", None)
    };
    let exhaustive = tier == "exact";
    let delta_summary = delta_summary_json(stamp, generation_reason);
    let producer_version = prov
        .and_then(|p| p.builder_revision.clone())
        .or_else(|| snap.and_then(|s| s.producer_version.clone()));
    let v = serde_json::json!({
        "v": 1,
        "tier": tier,
        "producer_version": producer_version,
        "graph_snapshot_id": prov.and_then(|p| p.snapshot_id.clone()),
        "exhaustive": exhaustive,
        "negative_verdict": negative,
        "delta_summary": delta_summary,
    });
    GraphAnnotation {
        tier,
        exhaustive,
        negative_verdict: negative,
        meta_json: serde_json::to_string(&v).expect("CR-META serializes"),
    }
}

/// The D9 `delta_summary` object: always present; members null exactly
/// when a D15 reason is set (`generation_reason` overrides — the
/// generation mismatch is the annotation's failure, not the delta's).
fn delta_summary_json(
    stamp: &ServingStamp,
    generation_reason: Option<crate::engine::DeltaUnavailable>,
) -> serde_json::Value {
    let snap = stamp.snapshot.as_ref();
    let reason = generation_reason
        .or(stamp.reason)
        .or(snap.and_then(|s| s.delta_unavailable_reason));
    match snap
        .and_then(|s| s.delta.as_ref())
        .filter(|_| reason.is_none())
    {
        Some(d) => serde_json::json!({
            "changed": d.changed.len(),
            "added": d.added.len(),
            "removed": d.removed.len(),
            "delta_complete": d.delta_complete,
            "content_verification": d.content_verification.as_str(),
            "delta_unavailable": serde_json::Value::Null,
        }),
        None => serde_json::json!({
            "changed": serde_json::Value::Null,
            "added": serde_json::Value::Null,
            "removed": serde_json::Value::Null,
            "delta_complete": false,
            "content_verification": serde_json::Value::Null,
            "delta_unavailable": reason.map(|r| r.as_str()),
        }),
    }
}

#[cfg(test)]
mod verdict_tests {
    use super::*;
    use crate::engine::{ContentVerification, DeltaUnavailable, SourceDelta};

    fn stamp_with(delta: Option<SourceDelta>, stale: bool, hash: Option<&str>) -> ServingStamp {
        let (delta_value, reason) = match delta {
            Some(d) => (Some(d), None),
            None => (None, Some(DeltaUnavailable::LegacyNoManifest)),
        };
        ServingStamp {
            snapshot: Some(StalenessSnapshot {
                source_newer: stale,
                graph_lags: false,
                doc_set_drift: None,
                corpus_policy_drift: None,
                identity_drift: Some(stale),
                identity_algo_upgrade: false,
                head_drift: None,
                eval_faces: Default::default(),
                stamped_identity: None,
                current_identity: None,
                delta: delta_value,
                delta_unavailable_reason: reason,
                delta_identity: None,
                base_commit: None,
                producer_version: Some("pyrefly-index 0.1.0".to_string()),
                serving_artifact_hash: hash.map(str::to_string),
            }),
            reason: None,
        }
    }

    fn delta() -> SourceDelta {
        SourceDelta {
            changed: vec!["crates/a.rs".to_string()],
            added: vec![],
            removed: vec![],
            delta_complete: true,
            content_verification: ContentVerification::FullRehash,
            out_of_scope_count: 0,
            policy_drift_flags: Default::default(),
        }
    }

    #[test]
    fn scip_negative_gate_fires_only_stale_and_edge_empty() {
        let d8 = "[CR_VERDICT] v=1 tier=possibly-stale exhaustive=false negative_verdict=unavailable producer_version=pyrefly-index 0.1.0 delta_changed=1 delta_complete=true content_verification=full-rehash delta_unavailable=null delta_paths=crates/a.rs";
        let stale = stamp_with(Some(delta()), true, Some("h"));
        let v = scip_verdict(&stale, ScipResult::EdgesEmpty);
        assert_eq!(v.line, d8);
        assert_eq!(v.exit_override, Some(1));
        // non-empty stale: success with the non-exhaustive caveat
        let v = scip_verdict(&stale, ScipResult::Populated);
        assert_eq!(v.exit_override, None);
        assert!(v.line.contains("negative_verdict=null"));
        // fresh + empty = authoritative zero, family exit untouched
        let fresh = stamp_with(Some(delta()), false, Some("h"));
        let v = scip_verdict(&fresh, ScipResult::EdgesEmpty);
        assert_eq!(v.exit_override, None);
        assert!(v.line.contains("tier=exact exhaustive=true"));
        // defs empty = the no-coverage axis even when stale (never the gate)
        let v = scip_verdict(&stale, ScipResult::DefsEmpty);
        assert_eq!(v.exit_override, None);
        assert!(v.line.contains("tier=no-coverage"));
        // degraded stamp: indeterminate, no gate
        let bad = ServingStamp::degrade(DeltaUnavailable::BudgetExceeded);
        let v = scip_verdict(&bad, ScipResult::EdgesEmpty);
        assert_eq!(v.exit_override, None);
        assert!(v.line.contains("tier=indeterminate"));
        assert!(v.line.contains("delta_unavailable=budget-exceeded"));
    }

    #[test]
    fn graph_annotation_tiers_and_generation_mismatch() {
        let stale = stamp_with(Some(delta()), true, Some("slot2"));
        let g1 = GraphProvenance {
            snapshot_id: Some("gid".to_string()),
            input_index_sha256: Some("slot1".to_string()),
            builder_revision: Some("rev1".to_string()),
        };
        // SM-10: the query served G1 while the slot moved — never patch
        // G1 with the new meta
        let ann = graph_annotation(&stale, Some(&g1), true);
        assert_eq!(ann.tier, "indeterminate");
        assert_eq!(ann.negative_verdict, None);
        let v: serde_json::Value = serde_json::from_str(&ann.meta_json).unwrap();
        assert_eq!(v["v"], 1);
        assert_eq!(v["graph_snapshot_id"], "gid");
        assert_eq!(
            v["delta_summary"]["delta_unavailable"],
            "graph-generation-mismatch"
        );
        // matching generation: the stale-empty gate fires with the null
        // verdict on the CR-META
        let g2 = GraphProvenance {
            input_index_sha256: Some("slot2".to_string()),
            ..g1.clone()
        };
        let ann = graph_annotation(&stale, Some(&g2), true);
        assert_eq!(ann.tier, "possibly-stale");
        assert_eq!(ann.negative_verdict, Some("unavailable"));
        assert!(!ann.exhaustive);
        // annotate-only op on the same state: no refusal
        let ann = graph_annotation(&stale, Some(&g2), false);
        assert_eq!(ann.negative_verdict, None);
        // legacy db (no keys): D10 indeterminate
        let ann = graph_annotation(&stale, Some(&GraphProvenance::default()), false);
        assert_eq!(ann.tier, "indeterminate");
        // symbols bypass (no conn): snapshot id null, verdict still present
        let fresh = stamp_with(Some(delta()), false, Some("slot2"));
        let ann = graph_annotation(&fresh, None, false);
        assert_eq!(ann.tier, "exact");
        let v: serde_json::Value = serde_json::from_str(&ann.meta_json).unwrap();
        assert!(v["graph_snapshot_id"].is_null());
    }

    #[test]
    fn verdict_line_null_fields_when_delta_unavailable() {
        let legacy = stamp_with(None, true, None);
        let v = scip_verdict(&legacy, ScipResult::Populated);
        assert!(v.line.contains("delta_changed=null"));
        assert!(v.line.contains("delta_complete=false"));
        assert!(v.line.contains("content_verification=null"));
        assert!(v.line.contains("delta_unavailable=legacy-no-manifest"));
        assert!(v.line.contains("producer_version=pyrefly-index 0.1.0"));
        assert!(!v.line.contains("delta_paths"));
    }
}
