# Segment B implementation receipt

Status: implemented and affected tests passed; ready for main's independent review. No commit, git mutation, install, deployment or full workspace test. Shared documents belong to main. Admission initially denied the first source patch; resumed only after main issued the documented work order. Original Round 0 evidence is unchanged.

## Accepted decisions implemented

- R0-04: `server.rs::check_file_impl` checks known death at entry, during convergence and before accepting success (including cached diagnostics and partial-warning output). Cached-death regression fails baseline and passes repaired source.
- R0-05: `transport.rs` owns a bounded queue and one serialized writer thread. Writer alone owns stdin; original framing helper is retained. A nonblocking pipe adapter makes partial writes cancellable; child ownership never requires interaction or writer ownership. Server-request replies take the same writer/acknowledgement path. Every transport write/response/interaction timeout invalidates the session. `session.rs` carries absolute deadlines through lazy initialization, writes, acknowledgements and response waits. `check_file` starts its slow_timeout clock at entry, including state/interaction waits and sync/reopen. Shutdown uses a total 10s entry budget, reserving the final 2s for cleanup; cleanup kills/waits the child and joins both reader and writer before returning. Session Drop also performs terminal cleanup.
- R0-09: handshake rejection is terminal synchronously, not a retry promise. The child is killed and waited before the failing call returns; original session cannot respawn, while a separately constructed session can succeed. Missing executable remains a loud spawn error.
- R0-08: the Rust battery opens the document to initialize (no shutdown-as-initialization), compares full release/revision/date identity, hard-fails mismatch in all modes, derives declaration positions and executes both frozen hover comparisons. Frozen expected values were not edited. Negative controls independently change release, revision and date.

Preserved policies: overlay/version/mutation-time/quiescence predicates, per-family diagnostic versions, LRU overlay replay, dropped-push force_reopen recovery. A responsive backend missing diagnostic convergence retains the explicit not-converged warning; this is distinct from transport timeout and does not invalidate the healthy transport.

## Commands and results

All Cargo commands used:

```sh
export CARGO_INCREMENTAL=0
export TMPDIR="$PWD/audit/20260927/tmp"
export GIT_CEILING_DIRECTORIES="$PWD/audit/20260927/tmp"
```

| Command | Exit / evidence |
|---|---|
| `cargo test --locked -p code-reality-lsp-bridge --test lifecycle -- --nocapture` (baseline) | 101, 4 failed / 1 healthy control passed, 12.72s; `red.log`, `red.exit` |
| `BRIDGE_STRICT_BATTERY=1 cargo test --locked -p code-reality-lsp-bridge --test ra_equivalence_battery -- --nocapture` (original battery) | 101, 0 passed / 1 failed, malformed version comparison; `ra-red.log` |
| `cargo test --locked -p code-reality-lsp-bridge --test lifecycle -- --nocapture` (final) | 0, 8 passed / 0 failed / 0 ignored, 16.99s; `lifecycle-final.log` |
| `BRIDGE_STRICT_BATTERY=1 cargo test --locked -p code-reality-lsp-bridge --test ra_equivalence_battery -- --nocapture` (final) | 0, 2 passed / 0 failed / 0 ignored, 8.76s; `ra-final.log` |
| `BRIDGE_STRICT_BATTERY=1 cargo test --locked -p code-reality-lsp-bridge -- --nocapture` | 0, 56 passed / 0 failed / 0 ignored; `suite.log`, `suite.exit`, `verification.json` |
| Scoped `rustfmt --edition 2021 --config skip_children=true --check` on edited Rust source/tests/fixture | 0; `format.log`, `format.exit` |
| `git diff --check -- crates/code-reality-lsp-bridge Cargo.lock` | 0 |
| `git diff --exit-code -- crates/code-reality-lsp-bridge/tests/fixtures/ra_equivalence/ra_hover_baseline.json` | 0, frozen oracle unchanged |

The final sequence was MIN lifecycle → strict RA → affected bridge suite. The affected suite executes Python equivalence and normal checks, poisoned-diagnostic/eviction tests, real RA edit/check, TS process tests and death isolation. RA emits existing missing-optional-config warnings; these are not skipped tests. The version-negative test deliberately catches expected assertion panics. Two extended cold-start fixtures initially failed with undersized startup budgets under load (`extended.log`); the cached control now explicitly warms its backend, while blocked initialization has 3s/5s deadline/tolerance. The already-initialized blocked check still uses 400ms; no production timeout was raised to make tests pass.

## Wall clocks and cleanup

From `lifecycle-final.log`:

| Real subprocess scenario | Observed |
|---|---|
| 4 MiB didOpen into non-reading initialized child, 400ms check policy | 402ms, error, child absent, no rescue |
| check waiting behind pending interaction, 400ms policy | 408ms, both requests fail, child reaped |
| multi-MiB initialization frame into non-reading child, 3000ms policy | 3005ms, terminal failure, child reaped |
| concurrent shutdown against blocked didOpen | 8.003501041s shutdown; 8107ms overall, child absent, no rescue |
| shutdown against multi-MiB request write | 8010ms, request errors, child absent, transport joined |
| shutdown during server-request reply backpressure | 8011ms, child absent, transport joined |

Each process test asserts `kill -0` fails after cleanup (a zombie would still exist). `shutdown_bounds_request_write_and_reader_reply_backpressure` also asserts the backend slot is removed after shutdown returns; `Transport::stop` explicitly joins writer and reader before that return. `verification.json` records a post-suite `ps` readback of logged fixture PIDs. Baseline RED rescue exists solely to release the baseline defect; final runs have no rescue.

## Scope and limitations

Source changes are confined to bridge src, tests, README and Cargo.toml. Cargo.lock adds only the bridge's reference to existing workspace libc (for Unix O_NONBLOCK flags); no new resolved package. New files: `src/transport.rs`, `tests/lifecycle.rs`, `tests/fixtures/lifecycle_backend.rs`. The transport uses Unix descriptors, consistent with the distributed macOS face. No claim bounds arbitrary kernel/filesystem hangs. Graph-derived impact verification was unavailable (no local graph.db and no index build authorized); evidence is positive source tracing plus process and real-backend tests. No source acceptance implies deployment.

Main documentation synchronization points: reader replies share writer transport; failed initialization requires a new session; check deadline begins at entry; shutdown kills/reaps and joins; RA battery version drift hard-fails. Main owns independent review and workspace-wide integration acceptance.
