//! CR-10 S4 — the MCP freshness tool (TC-10, D12), the scip/graph
//! TOCTOU seams (TC-8, D11), and the MCP refusal envelope (TC-4's MCP
//! leg). Consumes the pub seams `freshness::serving_stamp` /
//! `graph_annotation` / the `freshness` tool that landed with S4; the
//! CLI/graph-face RED receipts live in `query_freshness_annotation.rs`
//! and `.agent-tmp/cr10-s4-red-receipts.md`.

mod support;

use code_reality::build::build_repo;
use code_reality::engine::default_index_path;
use code_reality::freshness::{serving_stamp, serving_stamp_with_budget};
use code_reality::mcp_server::{CodeRealityServer, FreshnessParams, RefsParams};
use rmcp::handler::server::wrapper::Parameters;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// CR-10.3 R5: serializes every test that (transitively) reads
/// CODE_REALITY_VERDICT_BUDGET_MS (via serving_stamp → verdict_budget)
/// or mutates CODE_REALITY_AUTOHEAL — process-global env races under
/// cargo's parallel runner; house precedent: the ENV_LOCK in
/// query_freshness_annotation.rs.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct Lab {
    repo: PathBuf,
    _t: tempfile::TempDir,
    _bindir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

fn lab_py() -> Lab {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("mod_a.py"), "def fn1():\n    pass\n").unwrap();
    support::git_init(&repo);
    let bindir = tempfile::tempdir().unwrap();
    support::install_ts_deriving_fake(bindir.path());
    support::install_py_deriving_fake(bindir.path());
    let roots = vec![bindir.path().to_path_buf()];
    Lab {
        repo,
        _t: t,
        _bindir: bindir,
        roots,
    }
}

impl Lab {
    fn slot(&self) -> PathBuf {
        default_index_path(&self.repo).unwrap()
    }
    fn build(&self) {
        build_repo(&self.repo, None, &self.roots).expect("build");
    }
    fn drift(&self) {
        std::fs::write(self.repo.join("mod_a.py"), "def fn1():\n    pass_changed\n").unwrap();
    }
}

fn content_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default()
}

// ---------- TC-10: the MCP freshness tool (D12 typed verdict) ----------

#[tokio::test]
async fn tc10_freshness_tool_stale_is_typed_success_not_error() {
    let lab = lab_py();
    lab.build();
    lab.drift();
    let server = CodeRealityServer::new();
    let result = server
        .freshness(Parameters(FreshnessParams {
            repo_root: lab.repo.display().to_string(),
        }))
        .await
        .expect("stale is a legal answer, never an MCP error");
    assert_eq!(result.is_error, Some(false), "D12: explicit is_error=false");
    let v: serde_json::Value =
        serde_json::from_str(&content_text(&result)).expect("freshness JSON content");
    assert_eq!(v["fresh"], false, "{v}");
    assert!(
        !v["stale_reasons"].as_array().expect("reasons").is_empty(),
        "{v}"
    );
}

#[tokio::test]
async fn tc10_freshness_tool_fresh_is_error_false() {
    let lab = lab_py();
    lab.build();
    let server = CodeRealityServer::new();
    let result = server
        .freshness(Parameters(FreshnessParams {
            repo_root: lab.repo.display().to_string(),
        }))
        .await
        .expect("fresh answers");
    assert_eq!(result.is_error, Some(false));
    let v: serde_json::Value = serde_json::from_str(&content_text(&result)).unwrap();
    assert_eq!(v["fresh"], true, "{v}");
}

#[tokio::test]
async fn tc10_freshness_tool_check_failure_is_error() {
    // no slot in this repo → the freshness CLI face exits 2 (fail-loud,
    // never claims fresh) → the tool maps it to an MCP error (D12)
    let lab = lab_py();
    let server = CodeRealityServer::new();
    let err = server
        .freshness(Parameters(FreshnessParams {
            repo_root: lab.repo.display().to_string(),
        }))
        .await
        .expect_err("exit-2 check failure stays an error");
    assert!(err.message.contains("找不到 index slot"), "{err}");
}

// ---------- TC-4 (MCP leg): the refusal envelope folds to an error ------

#[tokio::test]
async fn tc4_mcp_callers_refusal_is_error_with_type_in_content() {
    // CR-10.3 R5: this test mutates CODE_REALITY_AUTOHEAL — join the
    // ENV_LOCK convention (same-process readers below)
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    lab.drift();
    let server = CodeRealityServer::new();
    // the MCP callers tool routes through cli::run (autoheal would
    // rebuild the drifted slot before answering — turn it off for the
    // refusal; house precedent: build.rs env discipline, restored after)
    // SAFETY: this binary's other tests do not read AUTOHEAL concurrently
    // (s6/mcp suites either set it on their own Commands or don't read it)
    std::env::set_var("CODE_REALITY_AUTOHEAL", "off");
    let err = server
        .callers(Parameters(RefsParams {
            symbol: "mod_afn1".into(),
            repo_root: lab.repo.display().to_string(),
        }))
        .await
        .expect_err("D7 refusal folds to a per-request MCP error");
    std::env::remove_var("CODE_REALITY_AUTOHEAL");
    assert!(
        err.message.contains("negative_verdict=unavailable"),
        "type rides the text face: {err}"
    );
    // CR-10.3 R9: the folded refusal envelope mints the frozen type
    // literal (EP D7 — bridge DB-101 matches this string on the text face)
    assert!(
        err.message.contains("stale-negative-unavailable"),
        "frozen refusal literal in the folded content: {err}"
    );
}

// ---------- TC-8: the TOCTOU seams (D11) ----------

#[test]
fn tc8_serving_stamp_hashes_the_serving_bytes() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let pre = serving_stamp(Some(&lab.repo), &lab.slot());
    let pre_hash = code_reality::identity::sha256_hex(&std::fs::read(lab.slot()).unwrap());
    assert_eq!(
        pre.snapshot
            .as_ref()
            .and_then(|s| s.serving_artifact_hash.as_deref()),
        Some(pre_hash.as_str()),
        "the stamp shares the hash of the bytes actually on the slot"
    );
    assert!(matches!(pre.fresh(), Some(true)));
    // swap the serving body (drift): the NEXT stamp describes the new
    // reality — stale with the drift observable
    lab.drift();
    let post = serving_stamp(Some(&lab.repo), &lab.slot());
    assert!(matches!(post.fresh(), Some(false)));
    let d = post
        .snapshot
        .as_ref()
        .and_then(|s| s.delta.as_ref())
        .expect("delta answers on a stamped slot");
    assert!(d.changed.iter().any(|p| p.ends_with("mod_a.py")), "{d:?}");
}

#[test]
fn tc8_binding_mismatch_retries_then_degrades_indeterminate() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // forge a meta whose bound index no longer matches the serving bytes:
    // the binding check fails twice (bounded retry) → indeterminate +
    // manifest-binding-audit-failed (D11/D15)
    let meta_path = code_reality::engine::meta_path(&lab.slot());
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!("0".repeat(64));
    std::fs::write(
        &meta_path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
    let stamp = serving_stamp(Some(&lab.repo), &lab.slot());
    assert!(
        stamp.snapshot.is_none(),
        "binding never resolves: {stamp:?}"
    );
    assert_eq!(
        stamp.reason,
        Some(code_reality::engine::DeltaUnavailable::ManifestBindingAuditFailed)
    );
}

#[test]
fn tc8_budget_exceeded_degrades_indeterminate() {
    let lab = lab_py();
    lab.build();
    let stamp = serving_stamp_with_budget(Some(&lab.repo), &lab.slot(), Duration::from_millis(0));
    assert!(stamp.snapshot.is_none());
    assert_eq!(
        stamp.reason,
        Some(code_reality::engine::DeltaUnavailable::BudgetExceeded)
    );
}

#[test]
fn tc8_graph_old_conn_reads_its_own_generation() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    // open the graph BEFORE the slot moves — tmp+rename keeps this
    // connection pinned to the old inode (no WAL)
    let conn = code_reality::graph_engine::open(&lab.repo).unwrap();
    let read = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |r| {
            r.get::<_, Option<String>>(0)
        })
        .ok()
        .flatten()
    };
    let gid = read("graph_snapshot_id").expect("S3 stamps the id");
    // re-seat the slot onto different bytes (rebind the stamped pointers
    // so the binding audit stays valid): the SERVING generation moves
    // while the graph the old connection sees stays G1
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");
    std::fs::copy(fixture, lab.slot()).unwrap();
    let slot_hash = code_reality::identity::sha256_hex(&std::fs::read(lab.slot()).unwrap());
    let manifest_path = lab.repo.join(".code-reality/scip/index.scip.manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["index_sha256"] = serde_json::json!(slot_hash);
    let mtext = format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap());
    let digest = code_reality::identity::sha256_hex(mtext.as_bytes());
    std::fs::write(&manifest_path, &mtext).unwrap();
    let meta_path = code_reality::engine::meta_path(&lab.slot());
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!(slot_hash);
    meta["base_manifest_digest"] = serde_json::json!(digest);
    std::fs::write(
        &meta_path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
    // SM-10: the stamp sees the NEW slot; the old conn still reports the
    // OLD generation — the annotation degrades instead of patching
    let prov = code_reality::freshness::GraphProvenance {
        snapshot_id: read("graph_snapshot_id"),
        input_index_sha256: read("input_index_sha256"),
        builder_revision: read("builder_revision"),
    };
    assert_eq!(prov.snapshot_id.as_deref(), Some(gid.as_str()));
    assert_ne!(
        prov.input_index_sha256.as_deref(),
        Some(slot_hash.as_str()),
        "the old conn must still describe G1"
    );
    let stamp = serving_stamp(Some(&lab.repo), &lab.slot());
    let ann = code_reality::freshness::graph_annotation(&stamp, Some(&prov), false);
    assert_eq!(ann.tier, "indeterminate", "{ann:?}");
    let meta: serde_json::Value = serde_json::from_str(&ann.meta_json).unwrap();
    assert_eq!(
        meta["graph_snapshot_id"],
        gid.as_str(),
        "the SERVED graph's id"
    );
    assert_eq!(
        meta["delta_summary"]["delta_unavailable"],
        "graph-generation-mismatch"
    );
}

// ---------- CR-10.3 R1: the post-load leg of the D11 binding window ----
//
// serving_stamp binds at query entry; the mode loads the face bytes
// later. `stamp_after_load` is the seam the modes run after their load:
// a slot swap inside the stamp→load window DEGRADES to indeterminate —
// the loaded body's generation is unattributable at recheck time (a
// swap before the load yields new bytes the old stamp can't describe; a
// swap after the load yields old bytes a recomputed stamp can't
// describe; the two are indistinguishable here), so the old stamp's
// annotations are never served over unverified bytes (muse re-review
// F1: the earlier rebind-on-settle semantics described bytes the query
// never loaded).

#[test]
fn tc8_post_load_slot_swap_degrades_indeterminate() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let stamp = serving_stamp(Some(&lab.repo), &lab.slot());
    assert!(stamp.snapshot.is_some(), "precondition: a bound stamp");
    // swap the slot AFTER the stamp: forge a meta whose bound hash
    // matches nothing — the post-load re-verification must catch the
    // swap and degrade (the load-window swap may never pass the old
    // stamp off as bound)
    let meta_path = code_reality::engine::meta_path(&lab.slot());
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!("0".repeat(64));
    std::fs::write(
        &meta_path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
    let after = code_reality::freshness::stamp_after_load(&stamp, &lab.slot());
    assert!(
        after.snapshot.is_none(),
        "a load-window swap cannot resolve: {after:?}"
    );
    assert_eq!(
        after.reason,
        Some(code_reality::engine::DeltaUnavailable::ManifestBindingAuditFailed)
    );
}

#[test]
fn tc8_post_load_settled_swap_still_degrades_body_unattributable() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lab = lab_py();
    lab.build();
    let stamp = serving_stamp(Some(&lab.repo), &lab.slot());
    assert!(stamp.snapshot.is_some());
    // a CONSISTENT slot swap (new bytes + rebound meta/manifest, the
    // atomic-publish shape): even a settled new generation DEGRADES —
    // whether the loaded body came from the old or the new bytes is
    // unknowable at this seam, so a re-bound stamp could describe bytes
    // the query never loaded (muse re-review F1); the conservative
    // direction is indeterminate, never a possibly-false exact
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rich.scip");
    std::fs::copy(fixture, lab.slot()).unwrap();
    let slot_hash = code_reality::identity::sha256_hex(&std::fs::read(lab.slot()).unwrap());
    let manifest_path = lab.repo.join(".code-reality/scip/index.scip.manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["index_sha256"] = serde_json::json!(slot_hash);
    let mtext = format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap());
    let digest = code_reality::identity::sha256_hex(mtext.as_bytes());
    std::fs::write(&manifest_path, &mtext).unwrap();
    let meta_path = code_reality::engine::meta_path(&lab.slot());
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["index_sha256"] = serde_json::json!(slot_hash);
    meta["base_manifest_digest"] = serde_json::json!(digest);
    std::fs::write(
        &meta_path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .unwrap();
    let after = code_reality::freshness::stamp_after_load(&stamp, &lab.slot());
    assert!(
        after.snapshot.is_none(),
        "a post-load swap cannot attribute the loaded body: {after:?}"
    );
    assert_eq!(
        after.reason,
        Some(code_reality::engine::DeltaUnavailable::ManifestBindingAuditFailed)
    );
}
