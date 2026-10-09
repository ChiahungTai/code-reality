All files read. Here is the full map, organized by your numbering. All paths are under `/Users/ctai/Github/code-reality/crates/code-reality/src/`.

# Part A — build.rs

## 1. File header (protobuf cat-merge legality note)

`build.rs:17-26` (verbatim):

```rust
//! Mixed indexes use the protobuf cat-merge trick: concatenating encoded
//! `scip.Index` messages of the same type is a legal merge (repeated
//! fields stack; N-way feasibility POC-verified). CR product code does
//! not consume `Index.metadata` — the sidecar stamp stays the
//! provenance authority — but the merge order is frozen and tested
//! because protobuf singular fields are last-value-wins.
//!
//! Known trap (POC): `rust-analyzer scip` takes the repo DIRECTORY —
//! passing Cargo.toml exits 0 with a metadata-only "empty" index, hence
//! the <128-byte guard below.
```

## 2. `concat_scip` — build.rs:192-199

Signature + body verbatim:

```rust
pub(crate) fn concat_scip(slot: &Path, part: &Path) -> Result<(), String> {
    let a = std::fs::read(slot).map_err(|e| format!("讀 {} 失敗：{e}", slot.display()))?;
    let b = std::fs::read(part).map_err(|e| format!("讀 {} 失敗：{e}", part.display()))?;
    let tmp = slot.with_file_name(".merge-tmp.scip");
    std::fs::write(&tmp, [a, b].concat()).map_err(|e| format!("寫 {} 失敗：{e}", tmp.display()))?;
    std::fs::rename(&tmp, slot)
        .map_err(|e| format!("rename {} → {} 失敗：{e}", tmp.display(), slot.display()))
}
```

- **Pure byte-concat, no protobuf parse.** Doc comment (187-191): "Protobuf same-type message concatenation: repeated fields stack, so `read(slot) ++ read(part)` written back is a legal merged Index. Temp-sibling + rename (atomic ...). Consumed by the `project` overlay merge; `build` itself stages and merges through [`merge_and_publish`]."
- **metadata / producer version strings: NOT handled at all.** Raw concat means singular protobuf fields are last-value-wins (per header, lines 21-22); the sidecar `.meta.json` stamp (not `Index.metadata`) is the provenance authority.
- **No dedupe by relative_path — documents blindly stack** (repeated field concatenation).
- Contrast: protobuf IS parsed elsewhere in the file — `filter_python_partial` uses `scip::types::Index::parse_from_bytes` (build.rs:279, `use protobuf::Message;` at 269) to retain/drop documents, but that is the Python leg's profile filter, not the merge.

## 3. `struct StagedLeg` — build.rs:204-207

```rust
struct StagedLeg {
    family: ProducerFamily,
    path: PathBuf,
}
```

Doc (201-203): "A producer partial validated for merge: pid-keyed path unique to one build attempt (a concurrent/failed attempt never overwrites another's part — S1 atomicity prerequisite)."

**Constructed in exactly ONE place**: build.rs:652-655, at the bottom of the per-family staging loop in `build_repo`:

```rust
        staged.push(StagedLeg {
            family: *family,
            path: part,
        });
```

where `part` is built at build.rs:566: `let part = slot_dir.join(format!(".part-{attempt}-{}.scip", family.cli_name()));` — i.e., **one part file per family per attempt**; the part filename itself is family-keyed (`.part-<pid>-<n>-rust.scip`).

## 4. `stage_rust` — build.rs:374-407 (whole function)

```rust
fn stage_rust(repo: &Path, part: &Path, roots: &[PathBuf]) -> Result<Option<String>, BuildError> {
    let bin = resolve_bin(
        "rust-analyzer",
        roots,
        "安裝：rustup component add rust-analyzer",
    )
    .map_err(|e| BuildError::Env(format!("{e}\n")))?;
    let version = first_output_line(&bin, &["--version"]);
    // current_dir pins the rustup proxy's toolchain resolution to the
    // repo (cwd-based proxy trap); the repo DIRECTORY (not Cargo.toml)
    // is the verified CLI shape.
    let out = Command::new(&bin)
        .arg("scip")
        .arg(repo)
        .arg("--output")
        .arg(part)
        .current_dir(repo)
        .output()
        ...
    if !out.status.success() { ...Env error... }
    validate_partial(part)?;
    Ok(version)
}
```

- **Exact command**: `rust-analyzer scip <repo> --output <part>` with `.current_dir(repo)`. The **project argument is `repo` itself (the resolved repo root)** — not `roots[0]`, not any Cargo.toml. This is the exact spot where a two-child-workspace repo fails ("more than one project" from rust-analyzer): the single repo root is the only project path ever passed.
- **`roots` is ONLY the bin-search path list** for `resolve_bin` (common.rs:191-208: "PATH-style resolution over injectable roots. Unix exec-bit checked.") — it has nothing to do with Cargo roots. Produced by `producer_roots()` (build.rs:174-185): `$PATH` entries + `$HOME/.local/bin` + `$HOME/.cargo/bin`.
- **Part file location**: written by rust-analyzer itself via `--output`, to `slot_dir/.part-<attempt>-rust.scip` (path composed at build.rs:566).
- **No path normalization/rewriting of the produced index** — bytes go from rust-analyzer straight to `validate_partial` then to merge. (Only the Python leg rewrites its partial: `filter_python_partial` re-serializes and restores mtime, build.rs:264-307.)
- `validate_partial` (build.rs:212-221): `< EMPTY_INDEX_BYTES` (128, build.rs:75) → `BuildError::Env` "producer 產出空索引（<128 bytes）…rust-analyzer scip 需傳 repo 目錄（非 Cargo.toml）".

## 5. `merge_and_publish` — build.rs:432-466 (full flow)

Single leg (433-443): validate via `crate::engine::load_index(&staged[0].path)` then atomic rename part→slot (Core error on rename failure).

Multi-leg (444-465), verbatim key loop:

```rust
    let mut bytes: Vec<u8> = Vec::new();
    for family in ProducerFamily::ORDERED {
        if let Some(leg) = staged.iter().find(|l| l.family == family) {
            let b = std::fs::read(&leg.path)
                .map_err(|e| BuildError::Core(format!("讀 {} 失敗：{e}", leg.path.display())))?;
            bytes.extend_from_slice(&b);
        }
    }
    let tmp = slot.with_file_name(format!(".publish-{attempt}.scip"));
    std::fs::write(&tmp, &bytes) ... ;
    if let Err(e) = crate::engine::load_index(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(BuildError::Env(format!("merged candidate 驗證失敗：{e}")));
    }
    std::fs::rename(&tmp, slot) ...
```

- **Confirmed: `staged.iter().find(|l| l.family == family)` takes only the FIRST leg of each family — a second Rust-family leg would be silently dropped.** Today duplicates cannot occur because `selected` is one entry per family (see item 6), but any multi-Cargo-workspace design that stages N rust partials must change this loop (and the single-leg `staged.len() == 1` rename path).
- Producer-string handling is NOT here; it is in `build_repo`: `let producer_str = rep.producers.join("; ");` (build.rs:750), passed to `stamp_meta_locked` (751-767) as `Some(&producer_str)` plus selection mode `Some("explicit"|"auto")` and `Some(&production_a)`.
- **After merge** (in `build_repo`, build.rs:717-822, in order):
  1. `merge_and_publish(&staged, &slot, &attempt)` (718) then `cleanup_staged` (722);
  2. face string: single → `staged[0].family.face_name()`, multi → `mixed(py+rust+...)` in `ORDERED` order (723-732);
  3. superseded sidecars removed: `crate::cache::sqlite_path(&slot)`, `crate::fndefs::fndefs_path(&slot)` (737-738);
  4. `stamp_meta_locked(... Some(&production_a), &writer)` (751-767) — writes `source_identity`, `identity_algo`, `index_sha256` into the meta (see item 13/Part B);
  5. read-back `rep.source_identity` from `load_meta(&slot)` (775-777);
  6. `--producer` omission note (779-789); "全量重產" note (790);
  7. remove identity cache of superseded generation (795);
  8. **graph build**: `graph_db::build_from_cache_locked(&resolved, &slot, &writer)` (797, Core error) — so yes, graph build happens here under WriterGuard;
  9. `graph_db::ensure_indexes_locked(&resolved, &writer)` (800);
  10. repopulate disposable identity cache: `walk_sources` + `compute_identity(... IdentityCachePolicy::Full ...)` (806-818);
  11. remove churn marker (821).

## 6. Orchestration `build_repo` — build.rs:472-823

- Signature: `pub fn build_repo(repo: &Path, producer: Option<ProducerFamily>, roots: &[PathBuf]) -> Result<Report, BuildError>` (472-476). `roots` doc (468-471): "bin-search path list (injectable for tests)".
- **Detection**: `count_sources(&resolved)` (484) → `SourceInventory { py, rs, js_ts }` counts; `detect_families(&inventory)` (485) → BTreeSet. **Detection is extension-based on FILE CONTENTS, not manifests/child dirs**: `count_sources` (122-156) walks the repo skipping dot-dirs, `SKIP_DIRS` (`__pycache__`, `venv`, `node_modules`, engine.rs:475) plus `"target"` (build.rs:129-130), and classifies each file via `crate::language::LanguageFace::from_path` (145-151). It does NOT look for Cargo.toml at all. Doc (126-128): "Detection is deliberately profile-UNFILTERED (superset walk: over-selecting a face degrades to a leg that skips on its empty governed corpus; under-selecting would silently drop sources)."
- **Selection** (486-492): `None` → `ProducerFamily::ORDERED.into_iter().filter(|f| detected.contains(f))` — i.e., **Python → Rust → TypeScript frozen order** (language.rs:83: `pub const ORDERED: [Self; 3] = [Self::Python, Self::Rust, Self::TypeScript];`). `Some(f)` → `vec![f]`.
- **Staging loop** (565-656): `for family in &selected` → dispatch: `stage_python_leg` (568), `stage_rust` (604), `crate::ts_producer::stage_typescript_leg` (617). Each success pushes one `StagedLeg` (652). `all_policy_empty` tracks whether every leg hit `SkippedByProfile` (564, 580, 606, 631).
- **Attempt identity**: pid + static counter (515-520); `stage_dir = slot_dir.join(format!(".stage-{attempt}"))` (521).
- **Where profile exclusions apply during build**:
  - Python: `stage_python_leg` → `collect_py_corpus(repo)` = `walk_sources` keys (245-252; walk itself profile-filters) and `filter_python_partial` drop-if-excluded post-step (264-307, `index.documents.retain(|d| !crate::profile::is_excluded(&normalize_rel(&d.relative_path), profile))` at 281-286).
  - TS: inside `ts_producer::stage_typescript_leg` (not read here).
  - **Rust: NO profile filtering anywhere** — `stage_rust` never loads the profile; the freshness walk likewise exempts Rust (engine.rs:510-515 "Rust keeps its historical corpus rules (no profile filter ...)").
- Empty-convergence terminal (684-715): if `staged.is_empty() && producer.is_none() && all_policy_empty`, remove slot + graph + all sidecars and return Ok.
- Publication guard (659-683): see item 7.

## 7. Observation A (source identity + policy capture)

- **Capture before production** — build.rs:551-554:

```rust
    // Capture A before any producer. Endpoint validation is not an immutable
    // snapshot: ABA and edits after the final observation remain possible.
    let production_a =
        crate::engine::ProductionIdentity::capture(&resolved, producer).map_err(BuildError::Env)?;
```

- **Double re-check** (659-683): capture `after` (660), compare `after != production_a` → Err "source identity/policy changed during production; prior data plane preserved" (661-665); then `WriterGuard::acquire(&resolved)` (667), capture a third time under the lock (668), compare again (669-674) → "…changed before publication…". All under `publication_writer::WriterGuard`.
- **Binding to index bytes**: `stamp_meta_locked` receives `Some(&production_a)`; it computes `let index_hash = crate::identity::artifact_hash(index_path)` (engine.rs:1161) and stamps `payload["index_sha256"] = serde_json::json!(a.identity.fingerprint binding)` — precisely engine.rs:1194-1205: requires `docs == a.docs && !a.faces.is_empty()` then writes `source_faces`, `source_set_fingerprint`, `js_ts_profile_fingerprint` (JS/TS only), `source_identity` (a.identity.value), `identity_algo` (a.identity.algo), `index_sha256` (index_hash).
- The `index_sha256` is re-verified at freshness time: engine.rs:956-959 `identity_drift = Some(current.value != *stamped_val || crate::identity::artifact_hash(slot)? != bound_index);`.

## 8. Exit-code mapping

- `BuildError` enum + doc, build.rs:94-102:

```rust
/// Error families map onto different exits (EP review finding 4):
/// `Env` → `fail(2)` (fixable environment: missing bin, child failure,
/// empty index, bad repo path), `Core` → `crash(1)` (graph-build core
/// errors, aligned with graph_db.rs/sidecar_migrate.rs precedent).
```

- Mapping at build.rs:1323-1326: `BuildError::Env(m) => ToolOutput::fail(...)`, `BuildError::Core(m) => ToolOutput::crash(...)`.
- The exits live in lib.rs:25-43: `fail` → `exit_code: 2` ("Environment-level loud failure"), `crash` → `exit_code: 1` ("Uncaught-Python crash face (D3)").

# Part B — engine.rs

## 9. `walk_sources` — engine.rs:610-700

- Walks `resolve_repo(repo)` (611) — the single repo root. Profile loaded once per walk (614).
- Stack of `(dir, under_target)` (624); skips dot-dirs and `SKIP_DIRS` (632); `target` stays walkable but tracked via `under_target` (635).
- Files filtered by `crate::language::LanguageFace::from_path` (637); rel path via `strip_prefix(&root)` (640-644).
- **Returns `SourceWalk`** (engine.rs:516-528): `py/rs/js/ts: BTreeMap<String, SourceRecord>` + `newest: Option<SystemTime>` + `newest_by_face: BTreeMap<LanguageFace, SystemTime>`. `SourceRecord` (identity.rs:72-82) = `{face, rel, size, mtime: (i64 secs, u32 nanos)}` — **no content hash** (D17; "one stat, already paid — size+mtime feed the identity cache gate", 645-647).
- Face-specific policy: Python profile-excluded at walk (664-669); Rust skipped when `under_target` (670-675, OUT_DIR artifacts); JS/TS skipped when `under_target || profile.is_excluded` (676-686).
- `records()` (536-544) merges all four maps into one rel-keyed BTreeMap ("globally unique — the face is a function of the extension").
- `fingerprint_for_faces` (580-603): FNV-1a 64 over sorted `"<face>\0<path>\0"` pairs — the doc-set fingerprint.
- Callers (seen): `collect_py_corpus` (build.rs:246), `ProductionIdentity::capture` (engine.rs:1049), `evaluate_staleness` (engine.rs:817), build's cache repopulation (build.rs:806), `run_heal_locked` doc-delta (build.rs:1083), `source_line` drift check (engine.rs:1401).

## 10. `LanguageFace::from_path` — language.rs:39-48

```rust
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
```

Case-sensitive (doc at 37-38). Also `pub const JS_TS_EXTS: [&str; 6] = ["js", "jsx", "mjs", "cjs", "ts", "tsx"];` (language.rs:34). `meta_name()` values (identity row face field): `"python" | "rust" | "javascript" | "typescript"` (language.rs:71-78). `ProducerFamily::ORDERED = [Python, Rust, TypeScript]` (language.rs:83).

## 11. `ProductionIdentity` — engine.rs:1034-1078

```rust
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProductionIdentity {
    pub(crate) faces: BTreeSet<crate::language::LanguageFace>,
    pub(crate) docs: BTreeSet<String>,
    pub(crate) identity: crate::identity::Identity,
    fingerprint: String,
    profile: Option<String>,
    js_ts_profile: String,
}
```

`capture(repo, producer)` (1045-1078): `walk_sources` → `records()`; faces = record faces filtered by producer (1051-1055); `compute_identity(repo, &records, &faces, IdentityCachePolicy::ReadOnly, &mut IdentityCache::disabled())` (1056-1062) — **full uncached hashes, cache disabled**; then hashes the profile file itself: `let profile_path = repo.join(".code-reality.toml"); ... Some(crate::identity::artifact_hash(&profile_path)?)` (1063-1068); plus `fingerprint` and `js_ts_profile: js_ts_profile_fingerprint(repo)`.

**Callers: build only** — build.rs:554 (`production_a`), build.rs:660 (`after`), build.rs:668 (under WriterGuard). NOT used by freshness (which calls `evaluate_staleness`), stamp-meta, or auto-heal (heal reuses `build_repo`).

## 12. `needs_rebuild` — engine.rs:746-754 (full decision)

```rust
    pub fn needs_rebuild(&self) -> bool {
        self.identity_drift == Some(true)
            || self.graph_lags
            || (self.identity_drift.is_none()
                && (self.source_newer
                    || self.doc_set_drift == Some(true)
                    || self.corpus_policy_drift == Some(true)))
    }
```

- **identity_drift path**: when `Some(true)` → rebuild, and mtime-newness / doc-set / policy drifts are demoted to report-only ("mtime-newness is no longer fatal (touch is idempotent, D3)", doc at 741-745).
- **legacy keyless trio**: when `identity_drift.is_none()` → baseline `source_newer || doc_set_drift == Some(true) || corpus_policy_drift == Some(true)`.
- **torn-plane guard**: `graph_lags` fires in BOTH modes unconditionally (doc 716-718: "stays an UNCONDITIONAL rebuild trigger (muse P1-3 torn-plane guard ... never short-circuited by a matching identity)"). Computed at engine.rs:892-898: main-slot only; `graph.db` missing or `gm < slot_m`.
- **identity_algo mismatch — exact comparison**, engine.rs:931-939:

```rust
    if let (Some(_stamped), Some(eval), Some(stamped_val)) =
        (stamped_faces.as_ref(), &eval_faces, &stamped_identity_val)
    {
        if stamped_algo.as_deref() == Some(crate::identity::IDENTITY_ALGO)
            && meta
                .as_ref()
                .and_then(|m| m["index_sha256"].as_str())
                .is_some()
        {
```

**A stamped algo string that differs (or is absent) fails this `if`, leaving `identity_drift = None` → the slot is treated as legacy/keyless and the baseline trio (mtime-newer etc.) decides.** Doc at 910-913: "computed ONLY when the meta carries the full comparable triple (source_faces + source_identity + identity_algo) — any absence or algo mismatch is a legacy slot and pays ZERO hash cost (D8/R22...)". Also pinned at identity.rs:31-32: "A stamped meta whose `identity_algo` differs is treated as legacy (not comparable)." Note: it degrades to legacy signals, not an unconditional baseline rebuild — the legacy trio still must trip. The freshness JSON face always reports the CURRENT algo constant, not the meta's: `"identity_algo": IDENTITY_ALGO` (freshness.rs:135).

## 13. `--stamp-meta` path

- Parse: cli.rs:29 flag `--stamp-meta`, cli.rs:130-136; guard "--stamp-meta 需 --repo" (225-227); route at cli.rs:312-314 → `stamp_meta_mode` (cli.rs:468-504) → `crate::engine::stamp_meta_core(repo, index_path, &crate::build::producer_roots(), None, None)` (cli.rs:469-475).
- `stamp_meta_core` (engine.rs:1101-1111): acquires WriterGuard, delegates to `stamp_meta_locked(..., validated: None, &writer)`. **It performs NO walk / identity harvest** — doc (1097-1100): "Manual/head-sync stamp: preserve byte-bound provenance, never certify current sources as consumed."
- `stamp_meta_locked` (engine.rs:1113-1214): `git_head`; producer string: explicit wins, else prior stamp preserved, else `producer_version("pyrefly-index", ...)` fallback (1127-1136); `selection` preserved similarly (1143-1148); payload keys repo/head/stamped_at/tool/producer/selection (1150-1157).
- **Manual-restamp provenance rules — verbatim**, engine.rs:1158-1185:

```rust
    // Manual/head-sync stamps cannot establish consumption evidence. Preserve
    // prior identity only for identical index bytes and the same repo/scope.
    let prior = load_meta(index_path).0;
    let index_hash = crate::identity::artifact_hash(index_path).map_err(StampError::Write)?;
    let preserve_prior_keys = |payload: &mut serde_json::Value| {
        if let Some(m) = &prior {
            if m["index_sha256"].as_str() != Some(index_hash.as_str())
                || m["repo"] != payload["repo"]
                || m["selection"] != payload["selection"]
            {
                return;
            }
            for key in [
                "source_faces",
                "source_set_fingerprint",
                "js_ts_profile_fingerprint",
                "source_identity",
                "identity_algo",
                "index_sha256",
            ] { ... payload[key] = v.clone(); }
        }
    };
```

  i.e., the six identity/provenance keys are carried over ONLY when the prior meta's `index_sha256` equals the current on-disk index hash AND repo AND selection match; otherwise they are dropped (degraded to legacy). The validated branch (1186-1206) is build-only (item 7).
- Other `stamp_meta_core` caller: refresh.rs:123 (post-commit head-sync re-stamp, `None, None`, message "meta head 已同步（source provenance retained, not recertified）" at 124).

## 14. `default_index_path` + slot layout

engine.rs:427-432:

```rust
pub fn default_index_path(repo: &Path) -> Result<PathBuf, String> {
    Ok(resolve_repo(repo)
        .join(".code-reality")
        .join("scip")
        .join("index.scip"))
}
```

Slot siblings: meta via `meta_path` → `index.scip.meta.json` (engine.rs:16 `META_SUFFIX`, 434-443); cache sqlite `crate::cache::sqlite_path(&slot)`; fndefs `crate::fndefs::fndefs_path(&slot)`; identity cache `identity-cache.json` (identity.rs:39, 116-120 `cache_path_for_slot`); staging artifacts `.part-<attempt>-<family>.scip`, `.publish-<attempt>.scip`, `.stage-<attempt>/` (build.rs:521, 566, 452); locks/markers `.heal.lock`, `.heal-churn` (build.rs:871, 981-985).

# Part C — identity.rs

## 15. Identity construction, algo string, cache, tests

- Constants — identity.rs:32-39:

```rust
pub const IDENTITY_ALGO: &str = "sha256-v1";
pub const IDENTITY_HEADER: &[u8] = b"cr-identity-v1\0";
const IDENTITY_VERSION: u32 = 1;
pub const IDENTITY_CACHE_NAME: &str = "identity-cache.json";
```

- Spec — identity.rs:3-7 (header doc): "`identity = sha256("cr-identity-v1\0" + Σ sorted-by-rel "{face}\0{rel}\0{size}\0{content_hash}\0")` — same content ⇒ same identity …; mtime is deliberately NOT part of the identity body (D3), it only gates the per-file hash cache."
- Core loop — identity.rs:278-307: `h.update(IDENTITY_HEADER);` then per record (face-scoped at 284-286, `normalize_rel` at key/cache-key only, 287-291):

```rust
        h.update(format!(
            "{}\0{}\0{}\0{}\0",
            rec.face.meta_name(),
            key,
            rec.size,
            chash
        ));
```

  Sorting: "BTreeMap order = rel-sorted: byte-determinism (D2)" (282-283) — the caller passes `walk.records()` which is a rel-keyed BTreeMap.
- **mtime-gated per-file hash cache**: `IdentityCachePolicy` (44-57): `Full` (stamp/build — bypass gate, always re-read, D13), `WriteBack` (query — gate hit reuses hash, D11), `ReadOnly` (`CODE_REALITY_IDENTITY_CACHE=off`, resolve_policy 61-67). Gate at 208-214: hit requires `e.size == size && e.mtime == mtime` with full `(secs, nanos)` precision (D16, doc 77-81). `content_hash_of` (320-342) streams sha256; `cache.store` prunes out-of-set entries, tmp+rename atomic write (230-262).
- Version/compat tests: `identity_entry_format_pin` (431-453, byte-exact frozen spec pin with sha2 oracle), `identity_equal_across_equivalent_trees` (391-425), `identity_is_content_sensitive` (457-479), `identity_is_touch_idempotent` (482-513), `cache_path_equals_full_recompute` (519-580), `cache_store_prunes_out_of_set_entries` (585-621), `corrupt_cache_discards_and_recomputes` (627-682 — version downgrade / algo drift / repo mismatch discard), `same_second_same_size_rewrite_is_detected` (688-726), `read_failure_fails_loud` (731-754), `symlinked_source_is_invisible_to_identity` (761-783), `face_scoping_excludes_other_faces` (788-810).

## 16. What can enter the identity input

- **Only language-face files.** Records come exclusively from `walk_sources`, which drops every file whose extension is not py/rs/js/jsx/mjs/cjs/ts/tsx (engine.rs:637-639). There is NO special-casing for Cargo.toml, package.json, tsconfig, or any manifest/config inside the identity string — a repo-root Cargo.toml change does not move the identity.
- The ONLY non-face input nearby is in `ProductionIdentity` (not the identity string): the `.code-reality.toml` profile is hashed separately into the `profile` field (engine.rs:1063-1068) and the JS/TS exclusion-policy fingerprint `js_ts_profile_fingerprint` (engine.rs:760-777) — policy A components compared by `PartialEq` on `ProductionIdentity`, and the latter is also stamped as `js_ts_profile_fingerprint` in the meta.

# Quick extras

## 17. CLI flag parsing for `build` (argparse.rs)

- `build` uses the shared `argparse::parse` with `const SPEC: ToolSpec` at build.rs:39-60 — flags `--repo` (`Kind::Value { metavar: "REPO" }`), `--producer` (`Kind::Value { metavar: "rust|python|typescript" }`), `--json` (`Kind::StoreTrue`); `positionals: &[]`. Run loop at build.rs:1290-1328 reads `values.get("--producer")` (1309) and validates via `ProducerFamily::parse_cli` (1310-1316).
- **Repeatable-flag precedent: NONE.** `Kind` is only `StoreTrue` or `Value` (argparse.rs:21-28); values land in `HashMap<&'static str, Option<String>>` where repeats are explicitly last-wins — argparse.rs:175 `values.insert(f.long, Some(val)); // last wins`. (Note: build.rs itself derives its per-family names from `--producer` singular; the umbrella bin routes subcommands before this.) Also note cli.rs (scip_refs) keeps its OWN hand-rolled parser (cli.rs:57-188) — only add the flag there if scip_refs needs it; `build` would use the shared `ToolSpec`.
- **Where `--rust-root` would land**: a new `FlagSpec { long: "--rust-root", kind: Kind::Value { .. } }` in `build.rs` `SPEC` (39-60) + `HELP` (62-70) + extraction in `run` (after 1309). If it must be repeatable, `Kind` needs a new variant (e.g. `Append`) plus a values representation change — there is no existing append machinery to reuse. `--repo=X` inline, `--rust-root` prefix-abbreviation, and last-wins are the frozen semantics to preserve.

## 18. `.code-reality.toml` profile struct

Owned by `profile.rs`. `PROFILE_FILENAME: &str = ".code-reality.toml"` (profile.rs:16), `DEFAULT_EXCLUDE: [&str; 1] = [".venv/"]` (17), `VALID_KEYS = ["module", "exclude", "scan_root", "hazard_registry"]` (19). Struct (46-52):

```rust
pub struct Profile {
    pub modules: Vec<ModuleRule>,        // { prefix: String, depth: i64 } (22-25)
    pub exclude: Vec<String>,
    pub scan_roots: Vec<ScanRoot>,       // { path: String, pyi: String } (30-33)
    pub hazard_registries: Vec<HazardRegistry>, // { package_prefix, suffix, register_fn, registry, evidence } (38-44)
}
```

Loader `load_profile(repo_root) -> Result<Option<Profile>, String>` (56-205, crash-only, missing → `Ok(None)`). Exclusion predicate `is_excluded` (293-302, additive builtin ∪ profile), `effective_excludes` (307-320).

# Design-relevant synthesis for multi-Cargo-workspace support

1. The single point of failure is `build.rs:385-391` — `Command::new(&bin).arg("scip").arg(repo)` passes the repo root as the sole project path; with `rust/` + `dutymail/` child workspaces rust-analyzer sees two projects and fails.
2. `StagedLeg` is family-keyed and `merge_and_publish`'s `find(|l| l.family == family)` (build.rs:446) plus the family-keyed part filename (build.rs:566) assume exactly one Rust leg per build; multiple Rust partials would need either per-root `StagedLeg`s (new loop) or an intra-family pre-merge.
3. Identity/walk are already root-relative and face-based (`walk_sources` walks everything under the repo; rel paths like `rust/crates/...` and `dutymail/src/...` share one namespace), so the freshness/identity pipeline would not need structural change — but note the rust face skips nothing by profile and skips `target/` only (engine.rs:670-675), and `count_sources` skips `target` entirely for detection (build.rs:129-130), so both child workspaces' `.rs` files are already detected and identity-tracked.
4. Merge legality: the cat-merge is byte-level with documents stacking blindly and `Index.metadata` last-value-wins (build.rs:17-22) — N-way concat of two rust-analyzer indexes plus ordering concerns are already the tested pattern, provided each partial is produced with `--output` at the repo cwd so relative paths stay repo-relative.