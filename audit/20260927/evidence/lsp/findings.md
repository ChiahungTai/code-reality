# Round 0: LSP lifecycle/error handling candidates

Reviewer/findings only; final disposition belongs to the parent collector.
Baseline: `eaa5d2462ea7821021ad0ea87403484ac0e47538`.
Scope: `crates/code-reality-lsp-bridge` and its tests; all source locations below are relative to that crate. README and `crates/AGENTS.md` lifecycle mechanisms were read first. Source/text-derived, not index-verified; no graph build/heal or structural-reference claim. Scoped source diff against baseline was empty. Workspace baseline build/tests remain owned by main; none were duplicated here.

## LSP-1 — A runtime / B evidence: blocked backend writes bypass check and shutdown deadlines

**Priority candidate: P1; confirmed with an isolated backend.**

- Locations: `src/server.rs:392` calls `sync_open` before starting the check deadline at `src/server.rs:400`; `src/session.rs:631` sends didOpen; `src/session.rs:544` holds the backend mutex while the blocking write at `src/session.rs:549` runs. `src/session.rs:556` needs that same mutex before shutdown can even reach its timed reap loop (`src/session.rs:566`). Request response timing similarly begins only after the write (`src/session.rs:529`, `src/session.rs:532`).
- Reproduction: initialize a cooperative backend, ask it to stop reading stdin while remaining alive, then check a newly opened 4 MiB file. Start shutdown concurrently. The probe's observation window expired at **35,255 ms**, versus the Python check's **20,000 ms** convergence setting; both check and shutdown were still pending. External SIGKILL of this probe-owned child released check with `Broken pipe (os error 32)` and shutdown with `Ok(())`.
- Evidence: `blocked-write.log`; command below exited **0**, asserting the observed pending state and cleanup results.
- Existing defenses: response waits have a 30-second timeout; convergence has a per-language deadline; shutdown has a 10-second child-exit polling window. These do not bound the preceding blocking pipe write/mutex acquisition.
- Test limitation: `tests/bridge.rs:465` (`check_file_timeout_path_warns`) forces an unattainable quiescence window on a responsive backend. That verifies the convergence-loop warning, not blocked writes or teardown. `tests/bridge.rs:47` exercises cooperative shutdown. These specific tests cannot substantiate an end-to-end deadline claim.
- Narrow action for consideration: make transport writes/teardown interruptible or document that these are post-sync convergence deadlines. This probe establishes a finite lower bound and release after an external kill; **it does not prove MCP cancellation behavior, an infinite hang, or automatic bounded exit**. The library probe exercises the production tool body/session/framing/child boundary; MCP transport was not involved in this case.

## LSP-2 — D silent failure: cached check reports success after confirmed backend death

**Priority candidate: P2; confirmed through the actual stdio MCP binary.**

- Locations: the EOF reader records death at `src/session.rs:390`; unchanged open files take the no-notification path at `src/session.rs:649` / `src/session.rs:653`. `src/server.rs:446` accepts the cached diagnostics based on version/freshness/quiescence and returns success at `src/server.rs:447`, without checking session death on this path.
- Reproduction: check one file successfully, kill the probe-owned backend, wait until `lsp_status` explicitly reports `state=dead`, and issue the same `check_file` via MCP. It returns `{"content":[{"type":"text","text":"count=0\n"}],"isError":false}` with no dead-backend or cached-result marker. The direct tool-body probe also returns `Ok("count=0\n")` in **0 ms** after death.
- Evidence: `dead-cache.log`, `mcp.log`, `mcp.stderr.log`; both commands below exited **0**. The actual bridge process exited **0** on MCP stdin closure.
- Existing defenses: `request` and `notify` call `check_alive`; status correctly reports death. Version, mutation-time and quiescence checks protect the cached diagnostics against several stale-push cases. They do not disclose backend unavailability on the cache-only path.
- Test limitation: `tests/bridge.rs:78` checks a request after death; `tests/ts_backend.rs:540` and `tests/ts_backend.rs:551` check hover after death. Those operations require a request and encounter the liveness guard; they do not exercise this warm cached check.
- Narrow action for consideration: surface death before returning a live-check success, or explicitly label a cached/degraded answer. **The unchanged file's prior diagnostics were not shown to be semantically wrong**; the confirmed issue is hidden backend failure and unqualified success. No claim is made about corrupted analysis or all backend-family behavior.

## LSP-3 — A runtime / B evidence: initialization rollback cannot retry in the same session

**Priority candidate: P2; confirmed with a reject-once backend.**

- Locations: `src/session.rs:472`–`src/session.rs:479` removes/kills the half-installed backend and explicitly claims the next call can retry fresh. Its reader then sets the shared dead flag at `src/session.rs:390`. The next spawn attempt hits `check_alive` at `src/session.rs:357`, which returns the restart-required error at `src/session.rs:335`.
- Reproduction: the backend returns an initialization error on its first launch and would initialize successfully on the next launch. After EOF is observed, a second request on the original session fails with `language server backend died ... restart the bridge to recover`, with `dead=true; pid=None`. A newly constructed session using the same backend succeeds. This isolates session state from persistent backend failure.
- Evidence: `retry.log`; command below exited **0** with assertions for initial failure, failed retry, and successful fresh-session control.
- Existing defenses: failed initialization is loud and the half-installed child is killed; normal backend death intentionally requires a bridge restart. The mismatch is specifically the initialization-rollback retry claim. This is not a request for general automatic restart after arbitrary backend death.
- Test limitation: `tests/bridge.rs:68` exercises a missing executable; `tests/ts_backend.rs:283` exercises an initialization failure caused by a fake backend exiting without its required flag. The inspected assertions validate the first error, not same-session recovery after a transient initialization rejection.
- Narrow action for consideration: either implement generation-safe initialization rollback/retry, or narrow/archive the retry claim and explicitly keep restart-required semantics. Merely clearing a shared flag without isolating the old reader would need separate race validation. No such repair was attempted.

## Reproduction commands and exit records

Run from `/Users/ctai/.codex/worktrees/5967/code-reality`.
The probe links main's already-built baseline library; it does not run Cargo or alter crate sources. The exact build command used, **exit 0**:

```sh
rustc --edition=2021 audit/20260927/evidence/lsp/probe.rs -L dependency=target/debug/deps --extern code_reality_lsp_bridge=target/debug/deps/libcode_reality_lsp_bridge-5776dae1339d352e.rlib --extern serde_json=target/debug/deps/libserde_json-dfd3d69bd5424347.rlib -o audit/20260927/evidence/lsp/probe
```

Exact probe invocations, each **exit 0** (success means the defect observation/assertions reproduced):

```sh
audit/20260927/evidence/lsp/probe retry > audit/20260927/evidence/lsp/retry.log 2>&1
audit/20260927/evidence/lsp/probe dead-cache > audit/20260927/evidence/lsp/dead-cache.log 2>&1
audit/20260927/evidence/lsp/probe blocked-write > audit/20260927/evidence/lsp/blocked-write.log 2>&1
uv run --no-project python audit/20260927/evidence/lsp/demo_mcp.py > audit/20260927/evidence/lsp/mcp.log 2>&1
```

`dead-cache` creates the fixture consumed by `demo_mcp.py`; run it first. The MCP probe uses `target/debug/code-reality-lsp-bridge` and executable `backend.sh`. That wrapper's executable bit was set with `chmod +x audit/20260927/evidence/lsp/backend.sh`. The fixture/backend marker/logs/probe binary are all confined to this evidence directory. The Rust dependency hashes above identify the artifacts used in this checkout; if main rebuilds with different hashes, resolve the matching rlibs before recompiling. Existing `probe` remains runnable here.

No further sweeps, source/doc edits, sidecar writes, delegation, or shared findings/progress updates were performed. Findings are baseline-existing candidates, not newly introduced regressions. Cancellation, real language-server stall frequency, and whole-suite coverage remain outside this evidence. The MCP probe additionally confirms cooperative bridge exit after this dead-child scenario; it is not cancellation/deadline evidence.
