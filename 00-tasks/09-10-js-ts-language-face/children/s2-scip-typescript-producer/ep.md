# Implementation EP: `scip-typescript` producer and governed JS/TS corpus

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S2 — Add the `scip-typescript` producer leg and governed JS/TS corpus construction
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **depends on**: S1 language-set build orchestration
> **status**: ready-for-implementation

## Implementation overview

Add the real `ProducerFamily::TypeScript` leg behind S1's staged-output contract. The producer must index JavaScript and TypeScript without modifying target repository configuration, must honor repo-owned CR exclusions, and must fail loudly when its executable/runtime is unavailable.

The decisive planning POC is stronger than the original blueprint assumption: a generic `include=["src/**/*"]` config omitted `.jsx`, while a CR-owned config with an explicit governed `files` list indexed all six supported extensions. The same explicit file list worked when the config lived under `.code-reality/scip/...`, so JS fallback can be both complete and non-invasive.

Existing `tsconfig.json` files remain authoritative for compiler/module/workspace semantics when they produce a usable index. Presence alone is not enough: the real acceptance corpus has root `tsconfig.json = {}`, and `scip-typescript 0.4.0` exits `1` with `error: no files got indexed`. A zero-file existing-project outcome may fall back to the governed derived config; unrelated compiler/indexer failures remain loud and do not silently switch semantics.

## UC inventory

### Capability updated

**Build and maintain a first-class JS/TS structural graph** — this EP supplies the batch producer and exact source corpus; S3/S4 complete graph semantics/lifecycle.

### Consumer contract

```text
code-reality build --repo <repo>
code-reality build --repo <repo> --producer typescript
```

No `npx`, npm install, or network download is performed by those product paths.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | Usable TS project | root `tsconfig.json` indexes governed files | Run existing project config, filter to governed corpus, stage valid SCIP | target config untouched |
| SM-2 | Empty/unusable root config | `{}` + real JS/TS sources | Recognize zero-file outcome, retry with CR-derived explicit-files config | one bounded fallback |
| SM-3 | No config JS repo | `.js/.mjs/...` only | Generate sidecar config with exact `files` list | no repo-root file write |
| SM-4 | Six-extension corpus | `.js/.jsx/.mjs/.cjs/.ts/.tsx` | All governed files appear in partial SCIP | exact doc-set equality |
| SM-5 | Profile excludes generated tree | `exclude=["dist/"]` | Excluded docs absent even if project config would include them | producer/freshness share source-set helper |
| SM-6 | All JS/TS excluded | detected files exist but governed set empty | Omit/skip TypeScript leg with explicit profile note; do not call it producer failure | no empty-index false error |
| SM-7 | Missing producer | no resolvable `scip-typescript` | Environment failure with install/path guidance | S1 old live slot unchanged |
| SM-8 | Missing Node/runtime | producer cannot execute | Environment failure naming Node prerequisite | old slot unchanged |
| SM-9 | Indexer real error | invalid compiler config/module resolution failure | Fail loud; do not reinterpret as zero-file fallback | stderr preserved |
| SM-10 | Repo-local tool install | `node_modules/.bin/scip-typescript` exists | Resolve it before requiring global PATH | GUI-friendly local project path |
| SM-11 | GUI/minimal PATH global install | tool outside normal roots | Honor explicit `CODE_REALITY_NODE_BIN_DIR` search root and show guidance | deterministic, no `npm prefix` spawn |
| SM-12 | Producer output contains excluded docs | existing project includes `dist/` | Decode/filter documents to exact governed set before S1 merge | no generated duplicate definitions |
| SM-13 | Project references/workspace | root config or package workspace | Use scip-typescript project/workspace behavior when verified; if unsupported fail with actionable guidance | no silent single-project truncation |
| SM-14 | Version drift | stamped `scip-typescript` differs from installed | S4 flagged-path drift note can compare version string | steady-state query remains zero-spawn |

## Segment 0 — research and POC results

### Current source anchors

- `crates/code-reality/src/common.rs:191` — generic `resolve_bin`.
- `crates/code-reality/src/common.rs:227` — `producer_version(name, roots)` executes `<bin> --version` and takes the first line.
- `crates/code-reality/src/build.rs:145` — current producer roots are PATH + `~/.local/bin` + `~/.cargo/bin`.
- `crates/code-reality/src/build.rs:186` — Rust leg demonstrates staged producer output.
- `crates/code-reality/src/profile.rs:47` — `Profile`.
- `crates/code-reality/src/profile.rs:56` — `load_profile`.
- `crates/code-reality/src/profile.rs:289` — `is_excluded`.

### POC evidence absorbed

From `../../poc/results.md`:

- `scip-typescript --version` → `0.4.0`; existing `producer_version` shape is compatible.
- Empty `{}` config → exit `1`, `no files got indexed`.
- Source-only acceptance corpus → 28 exact SCIP documents, 1,376,364 bytes.
- Six-extension glob config → five docs, `.jsx` omitted.
- Six-extension explicit `files` config → six docs.
- Explicit absolute `files` also works when config lives under CR-owned `.code-reality/scip/...`.
- Current normal PATH has neither `scip-typescript` nor the LSP backend; external prerequisite behavior is real, not theoretical.

### Frozen architecture decisions

1. **No repo mutation.** Generated config lives inside the current build's CR-owned staging directory.
2. **Fallback uses explicit files.** The `files` array is generated from CR's governed JS/TS source set; do not use a broad glob as the fallback source authority.
3. **Existing configs get first opportunity, not unconditional trust.** If the producer succeeds, post-filter to the governed set and require at least one governed document when the governed set is non-empty. A known zero-file result may retry once with derived config. Other failures are errors.
4. **Profile filtering is a postcondition.** Even an authoritative project config may include generated/excluded docs, so the staged SCIP is decoded and `documents` retained only when their normalized relative path is in the governed set.
5. **No implicit package-manager discovery process in normal resolution.** Search repo-local `node_modules/.bin`, explicit `CODE_REALITY_NODE_BIN_DIR`, then existing roots. This is deterministic in GUI harnesses and avoids calling `npm prefix -g` on build/heal.
6. **External version policy is observational.** Initial acceptance pins POC evidence on `scip-typescript 0.4.0`; runtime does not reject every other version solely by number. The exact version is stamped and later drift is visible.
7. **Node is a prerequisite, not bundled.** Error guidance states Node >=18 and the validated tool version. Node 24.20.0 is locally proven compatible but is not presented as an upstream guarantee.

## Segment 1 — JS/TS governed source set

### Context

S2 and S4 need the same answer to one question: which JS/TS source documents belong to this repo's CR corpus? This helper is a single source for those two paths. Python/Rust corpus behavior stays outside this helper and is not changed here.

### Files

```text
crates/code-reality/src/
├── js_ts_corpus.rs      # new: exact JS/TS file collection + normalized rel paths
├── language.rs          # S1 extension mapping reused
├── profile.rs           # existing exclusion authority
└── build.rs             # consumes governed set for TypeScript leg
```

### Core implementation

```rust
struct JsTsCorpus {
    files: BTreeSet<String>,       // repo-relative, normalized '/'
    newest: Option<SystemTime>,
}

fn collect_js_ts_corpus(repo: &Path) -> Result<JsTsCorpus, String> {
    let profile = load_profile(repo)?;
    walk repo using existing dot-dir/SKIP_DIR discipline;
    for file:
        if LanguageFace::from_path(file) is JavaScript or TypeScript
           && !is_excluded(rel, profile.as_ref()) {
            insert normalized rel;
            update newest;
        }
    return corpus;
}
```

Do not hard-code `dist/`. The acceptance repo gets source-only behavior only when its own profile excludes that path.

If the raw detector from S1 sees JS/TS files but `JsTsCorpus.files` is empty, the TypeScript leg is treated as intentionally absent-by-profile, with an explicit report note. This avoids a false environment failure from a deliberately empty governed corpus.

### Validation

- All six extensions recognized.
- Profile prefix exclusion removes generated path but not same-name sibling.
- Windows separator normalization fixture if supported by existing path helpers.
- Real-file fixture where all JS/TS is excluded.
- Exact set equality fixture, not count-only.

## Segment 2 — external tool resolution and producer command

### Context

JS ecosystems commonly install CLIs in project-local `node_modules/.bin`, while plugin-spawned GUI processes may have a reduced PATH. Product code must find deterministic local installs without silently invoking npm or downloading packages.

### Core implementation

Add a small Node-tool root helper without changing generic Rust/Python producer semantics:

```rust
fn node_tool_roots(repo: &Path, base_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = vec![repo.join("node_modules/.bin")];
    if let Some(explicit) = env::var_os("CODE_REALITY_NODE_BIN_DIR") {
        roots.extend(split_paths(explicit));
    }
    roots.extend(base_roots.iter().cloned());
    stable_dedup(roots)
}

fn resolve_scip_typescript(repo, roots) -> Result<PathBuf, BuildError> {
    resolve_bin("scip-typescript", &node_tool_roots(repo, roots), INSTALL_HINT)
}
```

The install hint should offer both deterministic faces:

```text
npm install --save-dev @sourcegraph/scip-typescript
# or install globally and expose its bin directory through PATH / CODE_REALITY_NODE_BIN_DIR
```

Before the first index spawn, verify a resolvable `node` runtime or let the executable failure surface with an augmented Node >=18 hint. Do not implement auto-install.

Version observation uses the existing `first_output_line(bin, ["--version"])`; POC proves `0.4.0` emits a compatible plain line.

### Validation

- Fake repo-local `node_modules/.bin/scip-typescript` wins over global roots.
- Explicit node bin root works with a minimal test PATH.
- Missing binary error contains install + PATH/env guidance.
- `--version` failure does not make a working producer impossible unless current build contract already treats version absence as fatal; preserve existing producer semantics.

## Segment 3 — project selection, fallback config, and SCIP corpus filtering

### Context

This segment turns the governed file set into a staged SCIP partial while preserving authoritative project semantics when they work.

### Project plan

Represent selection explicitly:

```rust
enum JsTsProjectPlan {
    Existing { project: PathBuf, workspace_flags: Vec<OsString> },
    DerivedFiles { config: PathBuf },
}

struct JsTsProduceResult {
    staged_index: PathBuf,
    mode: JsTsProjectPlan,
    indexed_docs: usize,
}
```

Selection order:

1. Collect governed source set. Empty → profile-skipped result.
2. Discover a root project/workspace configuration using explicit, bounded rules: root `tsconfig.json`/`jsconfig.json`, plus package-manager workspace markers only where `scip-typescript` has a verified CLI flag.
3. If an existing project is selected, run it into a **staged candidate**, never the live slot.
4. If it succeeds, filter candidate documents to the governed source set. If at least one governed document remains, accept it.
5. If the producer returns the pinned/recognized zero-file condition (`no files got indexed`) or a successful candidate filters to zero documents while the governed set is non-empty, retry **once** with `DerivedFiles`.
6. Any other producer/config/compiler error fails loud. Do not use fallback to hide a malformed real TypeScript project.
7. Derived mode creates a config under this build's staging directory with compiler defaults proven in POC and an **absolute explicit `files` array** for every governed document.
8. Run `scip-typescript index --cwd <repo> <derived-config> --no-progress-bar --output <part>`.
9. Decode the final part, retain only governed `Document.relative_path` values, serialize to a second staged file, and validate non-empty/parseable output before returning it to S1.

Derived compiler settings start from the POC-safe baseline:

```json
{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": false,
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "target": "ES2022",
    "jsx": "preserve",
    "noEmit": true
  },
  "files": ["<absolute governed files>"]
}
```

The implementation may adjust module settings from package metadata when required by a failing fixture, but such logic must be evidence-driven and covered by a scenario; do not build a second TypeScript project resolver in Rust.

### SCIP filtering details

Normalize each `Document.relative_path` before membership comparison. Excluded documents are removed. External symbols may remain; they do not create repo-owned definitions without a retained document. After filtering:

- governed set non-empty + retained docs empty → failure/fallback condition;
- every retained doc must belong to governed set;
- output must parse after serialization;
- report retained/excluded counts for diagnostics without dumping the full file list.

### Workspace boundary

The child implementation must POC scip-typescript's current `--pnpm-workspaces` / `--yarn-workspaces` behavior before enabling those flags. If a workspace shape cannot be indexed correctly, return actionable unsupported/project-discovery guidance. Do not silently index only one package and call the repo complete.

### Validation

Hermetic fake-producer tests:

- existing config success;
- existing config zero-file → exactly one derived retry;
- existing config unrelated failure → no fallback;
- successful candidate with generated/excluded docs → filter exactness;
- all governed files excluded → no producer call;
- malformed/empty staged output → S1 live slot untouched.

Real POC/acceptance lane:

- `delegate-bridge` copy with repo-owned `dist/` exclusion: 28 governed docs, exact source set.
- Six-extension fixture: all six documents through derived explicit-files mode.
- At least one typed TS config with imports/project semantics through existing-config mode.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. Implement governed corpus helper first; S4 will reuse it unchanged.
2. Add tool resolution and fake binary contract.
3. Wire project selection/derived config/filtering into S1's staged TypeScript leg.
4. Run S1 combination/atomicity tests after the real leg is connected.

### Cross-child contract exported

- S3 receives a staged/final SCIP whose JS/TS documents already obey the governed corpus.
- S4 must call the same `collect_js_ts_corpus` helper for source mtime/doc-set convergence; it may not reimplement exclusion rules.
- S7 documents repo-local/global tool resolution and `CODE_REALITY_NODE_BIN_DIR` exactly as implemented here.

## Verification strategy

1. **MIN** — empty `{}` config fallback, six-extension derived config, missing binary.
2. **SAMPLE** — profile filtering, repo-local resolution, existing-config success/error split, all-excluded repo.
3. **FULL** — affected build suites + S1 integration suite; then real `delegate-bridge` copy POC and a typed TS corpus. Network-backed npm execution remains outside default cargo tests.

## Completion / finalization

- Record the chosen/validated scip-typescript version and workspace flags in the parent evidence, without turning the planning snapshot into a forever hard-coded version claim.
- Update consumer prerequisites only in S7 after M1 semantics are complete.
- Run `/audit-test` semantics on fake producer/corpus tests.
- Do not advertise M1 from S2 alone.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Cross-child source-corpus changes discovered by review must be synchronized with S4 before implementation starts.
