# code-reality

Structural facts and governance audits for AI coding sessions, living
above repositories: symbol truth queries (`scip_refs` — refs/defs,
trait disambiguation), caller edges and closures, completeness audits
with `[SRC]` provenance, graph db build/query, narrative tour tooling,
and the `code-reality-mcp` stdio server.

This crate ships two binaries: `code-reality` (the CLI umbrella) and
`code-reality-mcp` (the MCP server face). Per-repo knowledge lives in
each scanned repo's `.code-reality.toml` — the tool layer embeds no
repo-specific special cases.

See the [repository README](../../README.md) for the full picture
(install, per-repo prerequisites, plugin/marketplace faces).
