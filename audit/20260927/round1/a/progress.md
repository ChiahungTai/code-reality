# Segment A progress

Intermediate checkpoint: accepted EP and R0-01/02/03 read; root/crates guidance and TDD skill read. Source baseline clean (audit assets untracked). No delegation, installs, git mutations or writes outside this worktree.

Scope: plugin/.mcp.json, scripts/test-plugin-wrapper.sh, crates/code-reality/src/mcp_server.rs, narrowly named MCP regression test; evidence here. Documentation elsewhere belongs to main.

Next: add failing version-boundary and final error-cap regressions, execute RED, implement scoped fixes, execute targeted GREEN and retain lifecycle limitations. Existing cancellation/EOF historical evidence remains immutable.

## Blocked checkpoint

The first source/test apply_patch was rejected by PreToolUse marshal admission guard: this non-canonical worktree requires a valid `.agent-tmp/work-order.json` issued by marshal; credential is absent or invalid. The guard names `crates/code-reality/src/mcp_server.rs`. No credential was fabricated and no alternate write mechanism was attempted.

No source/test patch applied. The wrapper RED and Cargo RED commands placed after apply_patch in the orchestration were not executed because the tool call threw. Tests executed: 0; no RED/GREEN evidence or passing counts claimed.

Read-only verification: `git diff --stat -- plugin/.mcp.json scripts/test-plugin-wrapper.sh crates/code-reality/src/mcp_server.rs` printed no changes. `test -f .agent-tmp/work-order.json` returned exit 1. Audit directory creation returned exit 0; progress file creation/update succeeded.

Resume prerequisite: main/marshal supplies a valid scoped work order through its authorized issuance mechanism. Resume with tests-first changes: both wrapper predicates (bare pin and +rev positive controls, neighboring release/prerelease rejected on all three main faces and bridge, unsuccessful post-install convergence), combined/final error cap including panic messages, real stdio oversized error plus subsequent tools/list. Preserve historical cancellation/EOF observations and disclose retained behavior in tool descriptions. Do not claim implementation complete.

## Resumed implementation checkpoint

Main issued the scoped credential; read `.agent-tmp/work-order.json` and `/Users/ctai/Github/ai-guide/hooks/work-order-schema.md`. Original source ownership remains unchanged. Main's concurrent README edits are preserved.

RED: wrapper shell exit 1, 26 passed / 8 failed (neighbor/prerelease accepted); MCP cap unit exit 101, 0 passed / 2 failed (ordinary and panic errors lack marker). Real stdio RED exit 101, 0 passed / 2 failed: oversized error lacks marker; lifecycle test initially overclaimed no response after EOF. Corrected that test assertion to apply only to cancellation, consistent with the accepted EP; EOF only promises possible waiting and continued work. All logs/exit sidecars are in this directory.

Changes: both actual wrapper predicates accept the bare release or +rev boundary; all existing bootstrap and escape paths retained. MCP internal-error construction caps complete text (including exit/panic prefix and combined streams), retaining -32603 and UTF-8. The community-specific error path shares the same helper. Four data-plane descriptions disclose cancellation delivery suppression, continuing publication and EOF waiting.

GREEN so far: wrapper shell exit 0, 34 passed / 0 failed; MCP cap unit exit 0, 2 passed / 0 failed. Targeted integration suites in progress. Only the owned Rust files were formatted.

Next: collect targeted integration results; inspect scoped diff and syntax; report A completion before the user-authorized root AGENTS exact-pin sentence correction. No full-workspace tests or outward actions.

## Final segment A receipt

Completed local implementation and targeted verification. Reported A completion before taking the additional, explicitly assigned AGENTS correction. Root AGENTS now describes exact release with optional +rev; only that phrase changed. Main's README lifecycle/cap and plugin/README exact-pin additions were read and preserved. Instruction-writing skill loaded; this change documents the accepted, tested product contract, with no new agent authority or workflow gate. Independent final review remains main-owned.

Owned changed files:
- `plugin/.mcp.json`
- `scripts/test-plugin-wrapper.sh`
- `crates/code-reality/src/mcp_server.rs`
- `crates/code-reality/tests/mcp_error_delivery_regression.rs` (new)
- `AGENTS.md` (additional user-authorized phrase only)
- this progress file and sibling evidence logs/exit files

### Commands and results

All Cargo invocations below used the exact prefix `CARGO_INCREMENTAL=0 TMPDIR="$PWD/audit/20260927/tmp" GIT_CEILING_DIRECTORIES="$PWD/audit/20260927/tmp"`. Logs were redirected into this directory, with the command status saved to the matching `.exit` sidecar and propagated by the shell.

| Command (after Cargo environment prefix where applicable) | Evidence stem | Exit | Counts |
|---|---|---|---|
| `TMPDIR="$PWD/audit/20260927/tmp" bash scripts/test-plugin-wrapper.sh` | wrapper-red | 1 | 26 passed, 8 failed |
| `cargo test -p code-reality --locked --lib error_cap_regression` | mcp-red | 101 | 0 passed, 2 failed |
| `cargo test -p code-reality --locked --test mcp_error_delivery_regression` | stdio-red | 101 | 0 passed, 2 failed; includes corrected EOF test overclaim described above |
| `TMPDIR="$PWD/audit/20260927/tmp" bash scripts/test-plugin-wrapper.sh` | wrapper-green | 0 | 34 passed, 0 failed |
| `cargo test -p code-reality --locked --lib error_cap_regression` | mcp-green | 0 | 2 passed, 0 failed |
| `cargo test -p code-reality --locked --test mcp_error_delivery_regression --test s6_mcp_server --test plugin_wrapper` | targeted-green | 0 | 2 + 9 + 1 passed, 0 failed |
| `cargo test -p code-reality --locked --test mcp_error_delivery_regression` | stdio-cleanup-green | 0 | 2 passed, 0 failed |
| `cargo test -p code-reality --locked --lib error_cap_regression` | mcp-final | 0 | 2 passed, 0 failed; combined final sizes cap-1/cap/cap+1 plus ASCII/multibyte and panic |
| `cargo test -p code-reality --locked --test mcp_error_delivery_regression` | stdio-final | 0 | 3 passed, 0 failed; adds intentional unwind cleanup |

All GREEN Cargo suites reported zero ignored tests. Counts are per invocation, not unique cumulative totals. Shell wrapper's 34 checks are nested under the single Cargo mount test.

Static checks (each exit 0): `bash -n scripts/test-plugin-wrapper.sh`; `jq empty plugin/.mcp.json`; `git diff --check -- plugin/.mcp.json scripts/test-plugin-wrapper.sh crates/code-reality/src/mcp_server.rs`; `rustfmt --edition 2021 --check crates/code-reality/src/mcp_server.rs crates/code-reality/tests/mcp_error_delivery_regression.rs`; `git diff --check -- AGENTS.md`. Formatting command was `rustfmt --edition 2021 crates/code-reality/src/mcp_server.rs crates/code-reality/tests/mcp_error_delivery_regression.rs`, limited to owned files, never workspace fmt.

### Acceptance and limits

Both version predicates are covered through the actual manifest strings: bare release/+rev positives; neighboring release/prerelease negatives across all three main faces and bridge; bad install postcondition rejects after three fake installs. Existing wrapper regression mechanisms remain exercised. No actual package installation occurred.

Final combined error text, including the error prefix and both streams, is capped with a UTF-8 boundary and marker, preserving -32603; small messages stay unchanged. Panic and community-specific internal errors share the helper. Existing success-cap regression still passes. Cap is text bytes plus marker, not a strict JSON frame limit or an in-memory producer-output limit.

Real stdio test observes -32603 + bounded error, then successful tools/list and all four lifecycle descriptions. Cancellation suppresses the cancelled response but producer completion still occurs; EOF waits while started work finishes. This matches retained Round 0 behavior. These controlled producers exit 2 and write completion markers; they do not demonstrate successful post-cancel graph publication or general rollback/cancellation.

Fixture ownership now includes a fresh Unix process group. Drop closes stdin, kills the group (including producers), kills/waits the server and joins the reader; an intentional panic after producer start exercises cleanup without explicit close. The new subprocess regression is Unix-only; no Windows lifecycle claim. No full workspace test, commit, git mutation, install, deployment, other-repo write or delegation. Concurrent data/LSP/Cargo.lock changes are not owned or reverted. Main owns final independent review and integrated verification.

Next: main collects segment A and performs its integration/review gates. No segment A implementation work remains.
