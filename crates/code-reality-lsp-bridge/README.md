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
