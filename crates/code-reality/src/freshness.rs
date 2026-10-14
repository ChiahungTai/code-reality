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
use crate::engine::{default_index_path, evaluate_staleness, resolve_repo};
use crate::identity::{resolve_policy, IdentityCachePolicy, IDENTITY_ALGO};
use crate::ToolOutput;
use std::path::Path;

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
