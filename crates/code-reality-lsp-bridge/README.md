# code-reality-lsp-bridge

The type-face MCP bridge: routes by file extension across three
independent backend families — `.py` tool calls (hover / check_file /
edit_file / lsp_status) go to a lazily spawned `pyrefly-lsp` backend,
`.rs` calls to `rust-analyzer`, and the six JavaScript/TypeScript faces
(`.js` `.jsx` `.mjs` `.cjs` `.ts` `.tsx`) to
`typescript-language-server --stdio`. One backend session per family,
independent lifecycles; killing one backend never disturbs the others.
The bridge itself does no type analysis, it speaks LSP to the backends.

Backends are system-level installs (`pyrefly-lsp` ships in the
pyrefly-producer distribution; `rustup component add rust-analyzer` for
the Rust face; the JS/TS face needs `typescript-language-server` plus a
TypeScript runtime on Node.js). Missing backends surface as loud tool
errors on first use, and `lsp_status` reports the family as
`state=unavailable` with install guidance — never a server-wide
failure.

The TypeScript executable resolves deterministically without network
access, in this order:

1. the explicit `--typescript-backend <executable>` override;
2. `<workspace>/node_modules/.bin/typescript-language-server`;
3. directories listed in `CODE_REALITY_NODE_BIN_DIR` (path list);
4. the normal process `PATH`.

`npm`/`npx` are never invoked. `lsp_status` availability probing and
the actual spawn use the same resolution, so status can never claim
available and then spawn a different rule. The override changes only
the program — the bridge always supplies the fixed `--stdio` argument
itself (typed argv, no shell).

See the [repository README](../../README.md) for the tool semantics.

Each session uses one bounded serialized writer queue for requests,
notifications, initialization and replies to server requests. A dedicated
thread owns nonblocking stdin; acknowledgement and response waits use the
same absolute deadline as interaction acquisition. Child ownership is
independent of the writer, so shutdown can kill and reap without waiting
for a blocked write. Cleanup joins both transport threads. This transport
uses Unix file descriptors (the distributed platform is macOS).

`check_file` starts its per-language deadline at entry, including lazy
initialization and document synchronization. A transport timeout or partial
frame invalidates the session and reaps its child; failed initialization is
also terminal before returning. Restart the bridge (or construct a new
session) to retry. Known backend death is checked on entry and before
accepting diagnostics, including a warm cached result. A responsive backend
that misses diagnostic convergence still yields the explicit not-converged
warning; overlay, version, mutation-time and quiescence gates are retained.

Shutdown has a total ten-second budget from entry, reserving its last two
seconds for forced cleanup. Application deadlines do not guarantee bounds
on arbitrary kernel or filesystem hangs. The Rust equivalence battery
requires the complete frozen rust-analyzer version and executes both frozen
hover comparisons; a different version is a failure in every mode, never
a passing skip. Declaration anchors follow source movement without changing
the frozen expected hovers.
