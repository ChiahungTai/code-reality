# Landing closure — formal deployment readback

Written by the ZCode main session (2026-09-28). The Round-1 landing turn
ended at the account usage limit at 11:49 local with the producer build,
cargo installs and installed-process smoke test still running in the
background; those legs completed but no agent ever read them back, so the
deployment acceptance below was never formally declared. This record
supplies that readback. Machine-checkable evidence for every claim below
is attached in [landing-closure-evidence/](landing-closure-evidence/)
(nine deploy exit+log pairs, version readback, smoke JSON, both-direction
test logs). Provenance: the nineteen deploy/smoke files are byte-copies of
originals in the codex-side audit worktree
`/Users/ctai/.codex/worktrees/5967/code-reality/audit/20260927/round1/`
(evidence contract keeps generated assets there; judge-verified
byte-identical); the three closure-generated files
(`installed-versions.txt` and both test logs) are sole copies with no
worktree original — their content self-identifies origin.

## Verified state (all observations mechanical, 2026-09-28 ~12:00 local)

- All nine `deploy-*.exit` files read `0` — build main/lsp/producer,
  install main/lsp, marketplace, cargo main/lsp/producer
  (`landing-closure-evidence/deploy-*.exit` + `.log`) — the background
  legs the dying turn dispatched finished successfully after its last
  message.
- Canonical main is fast-forwarded to
  `787d29c8ff2343526614434441b86ed709980b38` (`fix: harden lifecycle
  deadlines and data-plane publication`); working tree clean.
- Installed consumer faces, `--version` readback in
  `landing-closure-evidence/installed-versions.txt`:
  - uv (`/Users/ctai/.local/bin`): `code-reality-mcp`, `code-reality-lsp-bridge`
    → `0.9.3+787d29c`
  - cargo (`/Users/ctai/.cargo/bin`): `code-reality`, `pyrefly-index`,
    `overlay-gen` → `0.9.3+787d29c`; `pyrefly-lsp` → `0.9.3+787d29c`
    (engine rev `1d64c4b` unchanged)
- Formal smoke rerun (the leg the dying turn never observed):
  `demo_installed_smoke.py --bin-dir "$(uv tool dir --bin)"` →
  `[OK] installed_smoke: 2 binary records; development=False`,
  `passed=true` for both binaries (initialize handshake + tools/list).
  Result JSON: `landing-closure-evidence/installed-smoke-zcode-closure.json`.

## Residual disposition (this closure arc)

- P3 test robustness (the audit-environment TMPDIR incident, Round-0
  baseline section): `build_stamps_snapshot_metadata` now pins
  `GIT_CEILING_DIRECTORIES` to its own tempdir via a Drop-guard that
  restores any pre-existing value (audit baselines run with an outer
  ceiling) and a static-mutex serialization matching the workspace's
  `churn_env_guard` pattern, so the no-ancestor-git assumption is
  enforced by the test, not inherited from the environment. Verified
  both directions — with `TMPDIR` inside a git worktree (previously
  FAILED) and with the default TMPDIR — logs in
  `landing-closure-evidence/test-tmpdir-in-repo.log` and
  `landing-closure-evidence/test-default-tmpdir.log` (both exit 0; each
  log carries a run-provenance header recording the exact TMPDIR and the
  `git rev-parse --show-toplevel` resolution, so the hostile/benign
  configuration of each run is machine-checkable, not narrated).
- The landing-turn's uncommitted `landing.md` checkpoint section
  (gate-sequencing error disclosure + plugin-deployment record) is
  backfilled into the committed `landing.md` verbatim in this arc.
- Generated fixtures and binaries under the audit worktree's run1–run3
  directories remain uncommitted by the documented evidence contract;
  the audit worktree itself is the codex-side home and is left in place.

## Verdict

Local deployment acceptance: **complete** — all installed faces serve
`0.9.3+787d29c`, formal smoke passed, canonical main carries the fix.
Public distribution (PyPI, marketplace registries) remains unchanged;
the release is a separate, explicitly authorized step.
