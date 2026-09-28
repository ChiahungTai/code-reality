# Segment B progress

Intermediate: accepted EP, findings, original LSP probe, root/crates AGENTS, TDD and architecture skills read. First source-write attempt rejected by admission guard; no files written. Main has now issued the work order. No delegation, install, commit or shared documentation mutation.

Design: retain framing and overlay/version/time/quiescence policy. Separate child owner from bounded serialized writer queue. Writer acknowledgement, response, initialization, interaction and overlay locks share absolute deadlines. Nonblocking pipe adapter inside writer permits cancellation/join even if a pipe reader remains. Failed initialization terminal and synchronously reaped. Shutdown clock starts at entry.

Source tracing only (no local graph.db; no index build authorized). Preserve force_reopen compensation for dropped pushes, last_mutation freshness basis, LRU overlay replay and per-family diagnostic versions. Historical memory supplies no completion evidence.

Next: subprocess regression RED, implementation, strict frozen RA battery, affected bridge suite. RA anchors must follow declarations; golden content stays unchanged.

RED: `cargo test --locked -p code-reality-lsp-bridge --test lifecycle -- --nocapture` with required env exited 101: 4 failed, 1 healthy control passed, 12.72s. Cached check incorrectly returned count=0; failed init was not synchronously dead; blocked checks required rescue at 2007ms and 12105ms. Source work now in progress. Added existing workspace libc dependency and only corresponding Cargo.lock member entry for nonblocking pipe flags; no new package/install.

Intermediate GREEN: original five regressions/control passed (8.45s); blocked check 405ms against 400ms policy, concurrent shutdown 8.0017s, both child PIDs absent without rescue. Transport cleanup joins writer and reader before removing the backend slot. Strict original RA battery RED exited 101 (bad token comparison); repaired strict battery exited 0, both unchanged frozen hovers executed (write_message 11:9, read_message 19:9), 9.83s. Three version-drift negative controls hard-fail as intended.

Extended test attempt: 6 passed, 2 failed due overly small cold-start fixture budgets under parallel load (400ms init adversary killed before writing its PID; cached control initialization exceeded 1s). Warm the cached control explicitly and give initialization-write test 3s with 5s bound; retain 400ms already-initialized check/interaction tests. Do not interpret this failed run as acceptance.

Final-source MIN passed: lifecycle-final.log, exit 0, 8/8, 16.99s. Check 402ms, interaction wait 408ms, blocked initialization write 3005ms, concurrent shutdown 8.0035s, request-write shutdown 8010ms, reader-reply backpressure shutdown 8011ms. Every child absent, no rescue; cleanup returns after thread joins. Strict RA final passed: ra-final.log exit 0, 2/2, 8.76s, both frozen hovers executed. Affected bridge suite now running under the required environment; no full workspace suite invoked.

Final: affected bridge suite completed exit 0, 56 passed / 0 failed / 0 ignored. Golden diff and scoped diff-whitespace checks exit 0. Receipt: receipt.md; machine-counted suite/child readback: verification.json. Only comment/whitespace updates followed the final compiled code; no behavior changes after acceptance. Ready for main independent review. No active segment B command or child remains.
