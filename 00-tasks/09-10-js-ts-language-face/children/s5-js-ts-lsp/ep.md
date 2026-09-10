# Implementation EP: JavaScript/TypeScript LSP type face

> **ep_type**: implementation
> **parent**: `00-tasks/09-10-js-ts-language-face/ep.md`
> **parent segment**: S5 — Add JavaScript/TypeScript to the LSP type face
> **baseline**: `5938aee9111b0ed8b2320652bf8252fb10fe3127`
> **depends on**: S1 only for shared user-facing extension vocabulary; structurally independent from M1
> **status**: ready-for-implementation

## Implementation overview

Extend `code-reality-lsp-bridge` from two independent backend sessions (`.py` and `.rs`) to a third JavaScript/TypeScript session serving `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, and `.tsx` through `typescript-language-server`.

The protocol engine is already generic enough to reuse. Planning POC with `typescript-language-server 6.0.0 --stdio` + TypeScript `5.9.2` proved all six extensions can return hover under the bridge's current protocol assumptions, including server→client requests answered with `[]` and range-form full-document `didChange`. A deliberately bad `.ts` edit produced two diagnostics and the corrected edit converged back to zero.

One source-level blocker is explicit: `LspSession` currently executes `Command::new(&backend_cmd)` with no args, while TypeScript language server requires `--stdio`. This EP therefore generalizes the backend from a bare executable string to executable + argument vector and adds per-extension language IDs. It does not introduce TypeScript-specific protocol forks into the generic session loop.

## UC inventory

### Capability updated

**Type face via LSP bridge**:

```text
hover(file,line,character)
check_file(file)
edit_file(file,content)
lsp_status()
```

End state: Python, Rust, and JS/TS backends are independently lazy-spawned and independently unavailable/failable.

### Non-goal

M2 absence must not affect M1 structural graph queries. The bridge can report JS/TS backend unavailable while `code-reality build/scip_refs/graph_query` continue to work.

## Scenario Matrix

| # | Scenario | Trigger | Expected behavior | Checkpoint |
|---|---|---|---|---|
| SM-1 | `.js` hover | JS file | route JS/TS session, language ID `javascript` | non-null hover fixture |
| SM-2 | `.jsx` hover | JSX file | ID `javascriptreact` | non-null hover |
| SM-3 | `.mjs` hover | ESM file | ID `javascript` | non-null hover |
| SM-4 | `.cjs` hover | CommonJS file | ID `javascript` | non-null hover |
| SM-5 | `.ts` hover | TS file | ID `typescript` | signature hover |
| SM-6 | `.tsx` hover | TSX file | ID `typescriptreact` | signature hover |
| SM-7 | TS diagnostic edit | `edit_file` bad→good | diagnostics appear then converge to zero | range-form change |
| SM-8 | Backend absent | no TLS executable | `lsp_status` JS/TS line = unavailable with guidance | server stays up |
| SM-9 | Repo-local backend | `node_modules/.bin/typescript-language-server` | resolve and spawn with `--stdio` | GUI-safe local install |
| SM-10 | Minimal GUI PATH | global tool outside PATH | explicit `CODE_REALITY_NODE_BIN_DIR` supplies root | deterministic resolution |
| SM-11 | JS/TS backend dies | kill third child | JS/TS calls fail explicitly; Python/Rust still answer | session isolation |
| SM-12 | Python backend dies | kill Python child | JS/TS/Rust remain live | reverse isolation |
| SM-13 | LRU eviction | >8 JS/TS files | overlay survives close/reopen exactly as existing contract | edit persistence |
| SM-14 | Out-of-band disk edit | opened JS/TS file changes on disk | session syncs before check/hover using existing overlay rules | convergence |
| SM-15 | Unsupported extension | `.vue/.html/...` | loud unsupported file type lists supported faces | no accidental routing |

## Segment 0 — research and POC results

### Current source anchors

- `crates/code-reality-lsp-bridge/src/session.rs:63` — `LangSpec` holds one fixed `language_id` and one fixed extension.
- `crates/code-reality-lsp-bridge/src/session.rs:110` — `LspSession` stores one backend command string.
- `crates/code-reality-lsp-bridge/src/session.rs:237` — backend spawn is `Command::new(&self.backend_cmd)` with no args.
- `crates/code-reality-lsp-bridge/src/session.rs:288` — all server→client requests get `result: []`.
- `crates/code-reality-lsp-bridge/src/session.rs:654` — `full_change` sends a range-form full-content replacement.
- `crates/code-reality-lsp-bridge/src/server.rs:51` — `Bridge` has only `py` and `rs` sessions.
- `crates/code-reality-lsp-bridge/src/server.rs:72` — `session_for` compares against a single extension per session.
- `crates/code-reality-lsp-bridge/src/server.rs:208` — backend availability is a separate status probe and currently PATH-shaped.
- `crates/code-reality-lsp-bridge/src/bin/code-reality-lsp-bridge.rs:29` — Python override flag.
- `crates/code-reality-lsp-bridge/src/bin/code-reality-lsp-bridge.rs:36` — Rust override flag.

### POC evidence absorbed

From `../../poc/results.md` P7:

- `typescript-language-server 6.0.0 --stdio` works with the current initialize/initialized handshake.
- Empty-array replies to server requests did not freeze the TypeScript server.
- Hover succeeded on all six extension faces.
- Range-form full-document `didChange` produced diagnostics then converged after correction.
- Raw initialize response did not provide useful `serverInfo` (`unknown ?`); status must not require it for correctness.

### Frozen decisions

1. Backend family name in user-facing status is `typescript` / `js-ts`; it serves both JS and TS.
2. Public override flag is `--typescript-backend <executable>`.
3. Standard TypeScript backend always receives `--stdio` through typed backend args, not by embedding shell text in the executable string.
4. The bridge crate remains independent from the `code-reality` crate and from Node libraries; it only spawns an external executable.
5. Repo-local `node_modules/.bin` + `CODE_REALITY_NODE_BIN_DIR` + PATH/root resolution mirrors S2's user contract but is implemented as a small generic resolver inside this independent crate or a zero-dependency shared utility only if architecture review approves such extraction.

## Segment 1 — typed backend command and language routing policy

### Backend command model

Replace the bare command string with a structured process spec:

```rust
#[derive(Clone)]
pub struct BackendCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl BackendCommand {
    fn python(program: String) -> Self { Self { program, args: vec![] } }
    fn rust(program: String) -> Self { Self { program, args: vec![] } }
    fn typescript(program: String) -> Self {
        Self { program, args: vec!["--stdio".into()] }
    }
}
```

`LspSession::ensure_spawned_locked` becomes:

```rust
Command::new(&backend.program)
    .args(&backend.args)
    .stdin(Stdio::piped())
    ...
```

Error/status text renders a safe display form from program + fixed args. Do not execute through a shell.

### Language policy

Generalize `LangSpec` from one extension + one language ID to an extension matcher and language-ID function:

```rust
enum LangFamily { Python, Rust, TypeScript }

impl LangSpec {
    fn supports_extension(&self, ext: &str) -> bool;
    fn language_id(&self, ext: &str) -> Option<&'static str>;
}
```

Mapping:

```text
py   -> python
rs   -> rust
js   -> javascript
mjs  -> javascript
cjs  -> javascript
jsx  -> javascriptreact
ts   -> typescript
tsx  -> typescriptreact
```

The session-opening path must ask `LangSpec` for the per-file language ID instead of reading one fixed field.

### Timeouts

Do not copy Python's 500ms hover retry into the third backend. Start from the existing large-workspace-safe policy class (Rust-like 30s hover/convergence ceiling) and measure real TS workspace behavior before reducing it. POC small-fixture latency is not a production upper bound.

### Validation

- Backend args are passed as argv, never shell-split.
- Python/Rust spawn argv remain byte-for-byte equivalent except internal representation.
- Extension→language ID table exact.
- Unsupported extension fails before backend spawn.

## Segment 2 — third independent session and backend resolution

### Bridge shape

```rust
pub struct Bridge {
    pub py: Arc<LspSession>,
    pub rs: Arc<LspSession>,
    pub ts: Arc<LspSession>,
}

pub fn new(py: BackendCommand, rs: BackendCommand, ts: BackendCommand, root: PathBuf) -> Self;
```

`session_for` routes by each `LangSpec` matcher. `shutdown_all` shuts down all three independently.

### Executable resolution

Keep process resolution deterministic and no-network:

1. explicit absolute/relative executable passed via `--typescript-backend` if valid;
2. `<workspace>/node_modules/.bin/typescript-language-server`;
3. directories from `CODE_REALITY_NODE_BIN_DIR`;
4. normal process PATH and any existing bridge fallback roots that are already part of its product contract.

Do not run `npm`, `npx`, or package installation from `lsp_status` or spawn.

`backend_available` and actual spawn must call the same resolver so status cannot say available and then fail because spawn used a different path rule.

### CLI wiring

Binary defaults:

```text
Python:     pyrefly-lsp
Rust:       rust-analyzer
JavaScript/TypeScript: typescript-language-server + fixed --stdio arg
```

Add:

```text
--typescript-backend <executable>
```

The flag changes only the executable program; the JS/TS `LangSpec` still supplies required `--stdio`.

### Validation

- CLI accepts/rejects missing flag values consistently with existing flags.
- Repo-local fake executable is found under a minimal PATH.
- Explicit node-bin env root is found.
- Missing backend does not prevent bridge startup.

## Segment 3 — `lsp_status`, hover/check/edit, and protocol convergence

### Status

`lsp_status` returns three backend-family lines/objects in stable order: Python, Rust, TypeScript. For the third backend:

- missing executable → `state=unavailable` + Node/TLS install guidance;
- present but not spawned → available/not-spawned state under existing semantics;
- spawned → include server info when available, tolerate `unknown ?`.

Do not conflate missing Node tooling with a server-wide MCP failure.

### Protocol reuse

No TypeScript-specific request loop is added unless an implementation test proves unavoidable. Keep:

- existing initialize payload;
- server→client empty-result behavior;
- didOpen ordering after initialized;
- range-form full-content changes;
- diagnostic cache/version freshness;
- force-close/reopen recovery;
- LRU overlay persistence;
- out-of-band disk sync.

The only language-specific protocol data should be the `languageId`, timeout policy, install hint, and backend argv.

### Hermetic tests

Extend fake LSP fixtures to exercise a third session:

- JS/TS hover response;
- diagnostic push after didChange;
- correction clears diagnostics;
- backend death isolation in all directions;
- LRU close/reopen retains unpersisted edit;
- `lsp_status` missing backend.

These tests must not require Node/npm.

### Real acceptance lane

Run an external-tool acceptance script/POC outside the default suite with fixed observed versions:

```text
typescript-language-server 6.0.0
typescript 5.9.2
```

Verify all six hover faces plus `.ts` bad→good diagnostic convergence. Also run one representative real `.mjs` file from a disposable `delegate-bridge` copy.

## Segment 4 — help/MCP contract and regression closure

Update MCP tool descriptions and binary docs so supported file types list all eight code extensions across three backend families (`.py`, `.rs`, and six JS/TS faces).

Unknown file type errors should list the accepted extension set without implying that the structural graph requires the LSP backend.

Run existing equivalence batteries:

- Python vs pyright/pyrefly baseline;
- Rust vs rust-analyzer baseline;
- backend availability/death tests.

The new JS/TS real-tool POC is an additional acceptance lane, not a replacement for existing Python/Rust equivalence.

## Integration strategy

`baseline: 5938aee9111b0ed8b2320652bf8252fb10fe3127`

1. Land typed backend argv + generalized `LangSpec` with Python/Rust regression tests.
2. Add third session/routing/resolution + hermetic fake server tests.
3. Run real TLS six-extension POC through the implementation.
4. Update status/help descriptions.
5. S7 updates plugin prerequisites and final capability wording.

### Cross-child contract exported

- S7 documents `typescript-language-server`, TypeScript runtime dependency, repo-local resolution, and `CODE_REALITY_NODE_BIN_DIR`.
- M1 remains fully usable when S5 backend is unavailable.
- Parent M2 acceptance includes all six JS/TS extensions, not the earlier four-extension shorthand.

## Verification strategy

1. **MIN** — typed argv unit test, `.mjs/.ts` routing, third unavailable status.
2. **SAMPLE** — six extension routing/language IDs, fake diagnostics/edit, death isolation.
3. **FULL** — all `code-reality-lsp-bridge` tests/equivalence batteries + external TLS acceptance POC, then full workspace cargo test before S7 closure.

## Completion / finalization

- Update `crates/AGENTS.md` type-face architecture only after implementation review converges.
- Root/plugin capability wording lands in S7 so M2 is not advertised early.
- Run `/audit-test` semantics on third-backend tests; verify fake server behavior is not merely mirroring implementation assumptions.

## EP review record

Pending the child-EP independent review cycle for the seven-file set. Review must challenge executable+args resolution, the four JS/TS language IDs, and whether any shared session state can let one backend death poison another.
