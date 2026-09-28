# Authorized landing and local deployment

AUTH: user said "commit／整合／部署 你可以做" and then asked to finish directly.

This supersedes the preparation-only stop in CR-1 and the handoff EP for this arc. Original findings are already complete: eight fixes and two truthful contract amendments. No speculative feature implementation is added.

Target: commit the 29 reviewed product files plus board and selected durable audit artifacts, fast-forward the clean local main checkout at /Users/ctai/Github/code-reality, then build/install local macOS wheels into the existing uv tool face and update existing local plugin consumer files with backups/readback. No public registry publication or remote push is planned. No HTTP service is registered with launchd.

Preflight: main HEAD equals eaa5d2462ea7821021ad0ea87403484ac0e47538 and working tree is clean. Product hashes match final-verification.json; scoped rustfmt --check and git diff --check pass. Full workspace test/build and independent review evidence are reused at identical source identity. The repository is Rust; Python lint/type gates do not apply to product code. Audit reproduction assets are preserved evidence, not new production Python.

Hooks: core.hooksPath=.githooks. The unmanaged post-commit hook installs crates from the canonical checkout and refreshes the committed worktree. Do not bypass it; collect completion and independently verify final binaries after main integration. Existing Python audit probes stay under audit by explicit user evidence contract; generated fixtures, cache, nested repositories and binaries are excluded from staging. Foreign adjudication-zcode-main.md remains untouched/uncommitted.

Receipt: classification=boundary; review=Muse fresh plus GLM intent completed, main adjudication and native docs followup passed; session-freshness=final file hashes matched before staging; deployment-surfaces=local deployment pending. Public distribution remains unchanged.

## Landing checkpoint

Committed source and selected evidence as 787d29c8ff2343526614434441b86ed709980b38; canonical main fast-forward completed (integration.exit=0). Source-scoped whitespace check passed. Aggregate staged whitespace check reported blank terminal lines in raw logs and context-space lines inside archived diff evidence; the shell continued to commit after that failed check. This was a gate-sequencing error, not a passing aggregate check. Raw evidence remains byte-preserved; no product whitespace failure was found by the subsequent explicit product-path check.

Plugin deployment: backed up and replaced .mcp.json, README.md and consumer SKILL.md in the existing ZCode/Claude/Codex installations; all nine files match committed source (plugin-deployment.json). Canonical local marketplace slice regenerated successfully. No registry metadata or published version was changed; these are local 0.9.3+source-revision builds, not a PyPI release. Already-running MCP processes are not force-killed; fresh processes use updated binaries/configuration.

The unmanaged hook logged an install start but supplied no verified completion in this tool lifetime. Explicit synchronous installs and fresh-process probes own deployment acceptance; hook start is not success evidence. Main/LSP wheels built and uv installs passed. Producer wheel and all three canonical-path cargo installs are collecting; do not yet claim deployment complete.
