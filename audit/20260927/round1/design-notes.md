# Decision preparation (not yet accepted)

Main source readback confirms the Round 0 anchors still match baseline. These notes prepare adjudication; they do not authorize implementation before three-family collection.

## Existing owners and integration points

- Plugin wrappers own binary selection; scripts/test-plugin-wrapper.sh drives the actual JSON shell strings. Preserve canonical uv directory, all-three verification and postinstall checks.
- MCP result mapping owns success/error payload policy. Blocking module execution owns in-flight work independently of response delivery; stdio peer shutdown is not subprocess cancellation.
- LspSession owns the process, pending response channel, writer ordering and diagnostic cache. Reader EOF mutates dead; initialization failure currently kills without a wait. Writes occur at client request, notification, initialized, server-request replies and shutdown. Every write origin needs the same bounded transport treatment. check_file_impl owns convergence but currently starts its clock after sync_open.
- build_repo owns producer staging and publication, but stamp_meta_core is also consumed by CLI manual stamping and refresh head-sync. A build-only check cannot make arbitrary restamping authoritative. Preserve stable old identity rather than hashing new bytes into an old index.
- graph build publishes the database before materialize_derived reads/writes the served path. Merely giving SQLite files unique names does not protect the derived-table phase or producer-cache acquisition. Writer serialization must enclose the full relevant operation, with a lock order that cannot re-enter the existing heal lock.
- Existing heal single-flight is distinct from explicit writer ownership. Do not add recursive acquisition when ensure_fresh calls build_repo.
- Freshness evaluates arbitrary slots, including projected/index-only workflows. The missing-graph rule needs a canonical-main-slot condition or a documented required-artifact distinction.
- Rust hover equivalence currently uses real source line numbers in framing.rs. Transport refactoring must keep the oracle anchored to the intended declarations rather than accidentally hovering a moved blank/comment. The frozen value oracle is not to be regenerated merely to pass.

## Scope and limitations

This is source readback and positive path tracing, not an indexed dependency/zero-consumer proof. No .code-reality/graph.db is present. Root/module AGENTS and the standalone plugin skill describe these contracts; if behavior changes, their affected descriptions must follow. No new board exists at backlog/; the audit report and accepted EP will own this bounded remediation arc. No memory writes are authorized.

Potential split: wrappers/MCP; LSP transport/cache/battery; data-plane identity/recovery/writers. Shared documentation and Cargo.lock changes remain marshal-coordinated. Existing baseline probes supply independent historical failure evidence, and new regressions must first fail on the old behavior. Runtime consumer probes are required in addition to unit tests.
