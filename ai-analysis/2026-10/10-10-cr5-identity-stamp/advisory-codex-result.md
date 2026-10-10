# CR-5 — source-identity certification investigation (read-only)

Scope: `code-reality` (target) versus `delegate-bridge` (control). Static source and existing
`index.scip`/meta observations only. No build, tests, git edits, or data-plane mutations.
The concrete culprit is an orphan Rust file under `audit/`: the source-identity walk counts it,
but the Cargo-scoped rust-analyzer producer does not publish its document.

## Certification contract and observed state

- `crates/code-reality/src/engine.rs:618-682`: `walk_sources` traverses ordinary directories,
  accepts every `.rs` except those under `target/`, and **does not apply profile exclusions**
  to Rust. `SourceWalk::paths_for_faces` includes all those Rust paths (`engine.rs:554-569`).
- `engine.rs:1252-1286`: `ProductionIdentity::capture` freezes the walk's source document set
  and identity for the selected faces. `engine.rs:1397-1418` certifies identity only if the
  **published SCIP document paths exactly equal the captured paths** and faces is nonempty;
  otherwise it writes ordinary meta without the identity keys (`engine.rs:1422-1425`).
- `build.rs:798-801,916-934,1009-1044`: capture A is rechecked against later source walks,
  but stage/publish may succeed even when producer coverage differs from that walk. The
  post-publication read-back explicitly issues `[WARN] slot 未取得 source identity 認證`.
- Actual target meta has only `repo/head/stamped_at/tool/producer/selection`; current
  `CODE_REALITY_IDENTITY_CACHE=off code-reality freshness --repo . --json` says
  `fresh=false`, `stale_reasons=["identity-algo-upgrade"]`, `faces=["python","rust"]`,
  `indexed_source_identity=null`.
- The control meta carries `source_faces`, `source_set_fingerprint`,
  `source_identity`, `identity_algo=sha256-v2`, `index_sha256`, and `rust_topology`.
  Its **current** freshness read is `false/content-drift` (with `head_drift=true`), so its
  **successful earlier certification** must not be misreported as currently fresh.

## Hypothesis adjudication

### H1 — rust-analyzer coverage omits walked Rust files: **CONFIRMED-cause**

1. The target `Cargo.toml:1-3` declares `[workspace] members = ["crates/*"]`.
   `audit/20260927/evidence/lsp/probe.rs:1-4` is a real `.rs` file **outside** these Cargo
   members, in a normal (non-dot) directory. `.code-reality.toml:1-5` excludes only
   `.venv/` and `.agent-tmp/`, and the Rust walk would ignore even those profile excludes.
2. The producer invokes `rust-analyzer scip <workspace-dir>` (`build.rs:375-407`), using
   a resolved single root `.` (`rust_projects.rs:242-284`). Its produced document corpus
   follows the Cargo workspace; an unrelated standalone `audit/.../probe.rs` is not a target.
3. Binary-safe, exact-path `rg -a -o -F` on the **existing** target `index.scip` returned
   **0** hits for `audit/20260927/evidence/lsp/probe.rs`, whereas it returned **1** each
   for known positives `crates/code-reality/src/engine.rs` and
   `crates/code-reality-lsp-bridge/examples/fake_lsp_server.rs`. The serialized
   `Document.relative_path` field contains its path as one contiguous string, as consumed
   by `engine.rs:1399-1404`. This is a meaningful negative control, unlike `strings` output.
4. The single-root **coverage gate is disabled**: `build.rs:430-449` sets `gate_fatal`
   only for multiple roots or explicit `rust_projects` configuration, leaving `governed`
   empty for this root. `build.rs:475-484` runs the gate conditionally. Thus production
   and graph rebuild succeed while stamping is silently skipped (`engine.rs:1405`).

At least this one concrete member of `walk.docs - published.docs` establishes a mismatch.
Additional omitted or extra Rust paths may exist; the full SCIP document list was not
decoded in this read-only leg.

### H2 — mismatched path form: **CANNOT-DETERMINE-statically** as a *second* cause

- `build.rs:470-478` runs `normalize_scip_paths` on **every** Rust root, including `.`;
  `build.rs:512-562` rejects absolute paths, `..`, backslashes, missing files, and
  symlink escapes. For root `.`, it adds an empty prefix and preserves already-relative
  paths. Normal Rust paths have positive exact-string hits in the slot (H1 controls).
- Caveat: this normalization does **not** strip an initial `./` from a producer-emitted
  path (`build.rs:528-559`). Exact byte-field decoding is needed to exclude that (or
  other format variants) as an *additional* discrepancy. It is unnecessary to explain
  the verified absence of `audit/.../probe.rs`.
- The graph's absolute `file_path` is a red herring: the builder explicitly computes
  `resolve(&repo_abs.join(rel))` before inserting nodes (`graph_db.rs:783-807`); graph
  absolute paths do not imply absolute `Document.relative_path` values.
- Precise read-only follow-up: decode the existing SCIP protobuf's `Index.documents[*].relative_path`
  verbatim, print the leading bytes of each path, and compare with `walk_sources` paths.

### H3 — Python post-filter/corpus disagreement: **RULED-OUT as the demonstrated cause**

- `build.rs:246-252` derives `collect_py_corpus` from the same `walk_sources.py` used by
  capture. `build.rs:265-297` removes only profile-excluded producer documents and compares
  normalized path names against `governed` (missing governed docs produce a warning).
- The target profile does not exclude `audit/` and an existing `audit/20260927/evidence/demo_mcp_lifecycle.py`
  has a positive exact-path match in SCIP; a test fixture Python path likewise matches.
  The supplied build receipt reports 14 retained Python documents. No evidence ties
  Python filtering to the **proven** omitted Rust audit path.
- Scope: 14 retained documents is a **count**, not a set-equality proof. A secondary
  Python mismatch remains possible until the full sets are decoded; this verdict only
  rules H3 out as the needed cause of this symptom.

### H4 — pyrefly producer emits beyond/short of its explicit corpus: **CANNOT-DETERMINE-statically**

- `crates/pyrefly-producer/src/lib.rs:57-73` canonicalizes the repo and collects `.py`
  files. `lib.rs:134-138` creates one SCIP document for each driven module through
  `emit.rs:39-48`. `api.rs:104-112,148-151` can omit a module when no AST is available.
- `lib.rs:360-380` shares `SKIP_DIRS` with the source walk (`engine.rs:475,639-644`).
  `build.rs:328-342` would announce missing governed Python docs, though that is
  not a complete guarantee against extra non-excluded documents.
- Follow-up: inspect **exact** published Python `Document.relative_path` set and compare
  it to captured `walk.py` (and capture the `missing_governed` receipt). Do not infer
  equality solely from the reported number 14.

### H5 — source walk includes files omitted by a producer: **CONFIRMED-cause**

- Same concrete counterexample as H1: ordinary `audit/20260927/evidence/lsp/probe.rs`
  is counted by `engine.rs:618-682` but is outside Cargo workspace members (`Cargo.toml:3`)
  and absent from the published index (binary-safe exact-path negative check).
- This is **one shared causal mechanism** (H1 and H5), not two independent defects.
  Other cases (Cargo-unreferenced `.rs` under member directories, fixtures, unattached
  `.rs` files) require a complete document-set comparison; no claim is made that all
  such files were inspected.

## Causal verdict and control explanation

`walk_sources` treats every non-`target/` `.rs` as a certifiable Rust source -> capture A
records `audit/.../probe.rs` -> rust-analyzer indexes the Cargo workspace `crates/*`
and omits that unrelated audit probe -> the single-root coverage gate does not reject
the omission -> the candidate index/graph publish -> `docs != a.docs` at
`stamp_meta_locked` -> identity fields are absent -> freshness reports
`identity-algo-upgrade` indefinitely. **Confidence: high**, backed by the specific
orphan, walk/stamp control flow and index positive/negative path checks; exact entire
set difference is unmeasured.

`delegate-bridge` differs in both observed outcome and safeguard: its meta has the
triple, hence the equality gate passed when it stamped. Its roots are `dutymail` and
`rust` (control producer field; `rust/Cargo.toml:1-2`, `dutymail/Cargo.toml:4-5`),
so `gate_fatal` is on (`build.rs:439-449`), making missing walked Rust documents a
build-time failure rather than a silent uncertified publication. Multi-root layout
alone is **not** an explanation for certification; the control's source/index set
matched then, whereas the target has a concrete uncovered orphan now. The control's
subsequent content drift is a distinct later freshness state.

## Fix recommendation and regression design

1. **Immediate local remedy (separate authorized implementation):** adjudicate
   `audit/.../probe.rs` as a non-governed standalone diagnostic and explicitly exclude
   that **Rust-only path** using a deliberately designed corpus policy, or relocate
   it into a legitimate Cargo example/test target if it must be indexed. Do not
   silently exclude all of `audit/`, which contains indexed Python evidence.
2. **Durable owner fix:** make Rust source identity and Rust producer coverage share an
   explicit **Cargo-aware governed corpus policy**. A file outside recognized targets
   must be a visible, policy-owned exclusion (or fail-loud orphan). Preserve any
   independent detection of new source/target arrivals and Cargo topology changes;
   never derive the trusted identity *solely* from the just-produced index because
   that would certify silent producer omissions. Apply matching policy when building
   `ProductionIdentity` and when checking produced document paths.
3. **Diagnostic/guard fix:** expose `walk.docs - index.docs` and `index.docs - walk.docs`
   (counts plus first N exact paths), including face and root, at certification. Apply
   an equivalent coverage gate to the single-root path before publication, with a
   controlled nonzero result instead of an exit-0 graph that can never be certified.
   `build.rs:628-654` already contains a coverage implementation for multi-root mode.
4. **Do not relax strict set equality** to subset/superset: a missing producer document
   would then be falsely certified as consumed; allowing extras would certify unknown
   source/scope bytes. That would invalidate freshness's consumption-evidence semantics.

Regression fixture: create a temporary single-root Cargo workspace with a member
`crates/mini/src/lib.rs`, an independent `audit/lsp/probe.rs`, and two Python files
(one under `audit/`). Reproduce the exact orphan delta and assert a diagnostic or
fail-loud status, without a certified triple. Exercise the intended **explicit**
resolution (Cargo target or Rust-only exclusion), then run the normal build and
assert `source_identity`, `identity_algo=sha256-v2`, `index_sha256`, and `source_faces`
are present and that `index_sha256` matches the published bytes; require
`freshness --json` to answer `fresh=true` on the unchanged fixture. Mutate a governed
Rust source and assert `content-drift` until rebuilt; add an unexpected orphan and
assert it cannot silently pass as certified. Mirror the same invariant in a two-root
control fixture. Candidate suites: `crates/code-reality/tests/source_identity.rs`,
`crates/code-reality/tests/multi_rust_root_e2e.rs`. **Tests were not run here.**

## Five-line summary

1. CONFIRMED: `audit/20260927/evidence/lsp/probe.rs` is walked but missing from Cargo-scoped SCIP.
2. The single-root mode skips Rust coverage gating, so build succeeds and stamp equality fails.
3. Graph absolute paths are an insertion transformation, not an explanation of index paths.
4. The multi-root control stamped successfully; its currently observed `content-drift` is separate.
5. Align Cargo-aware governed sources, keep strict equality, report exact deltas, and test the stamp.
