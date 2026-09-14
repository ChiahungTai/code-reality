#!/usr/bin/env bash
# release.sh — single-stage release finalizer.
#
# Born of the v0.5.0 double-miss arc: the release shipped with stale
# marketplace version faces (fixed only at the fourth+fifth sync point)
# and the local dist/marketplace slice was never regenerated — both
# runbook steps lived in memory and were skipped. This script makes the
# whole checklist mechanical; the runbook collapses to running it.
#
# Usage: scripts/release.sh <new-version> [--subject "..."] [--dry-run]
#
# Stages:
#   preflight  clean tree; five version faces in lockstep at the CURRENT
#              version (catches pre-existing drift before bumping)
#   bump       Cargo.toml workspace version, plugin/.claude-plugin/
#              plugin.json, marketplace.json, .claude-plugin/marketplace
#              .json, plugin/.mcp.json want=, Cargo.lock via
#              `cargo update --workspace`
#   guard      scripts/test-plugin-wrapper.sh (five-point lockstep at the
#              NEW version) + re-extract check
#   commit     "chore(release): v<new> — <subject>" (bump-only commit;
#              preflight cleanliness guarantees the isolation)
#   tag+push   annotated v<new>; push origin main + tag
#   wheels     readiness barrier: watch THIS tag's release-wheels run
#              to green, then probe PyPI for all three dists at the
#              new version — the marketplace slice must not expose a
#              plugin pin the registry cannot serve yet (v0.9.1 race,
#              2026-09-14)
#   slice      bash scripts/dist-marketplace.sh (local-market machines
#              see the new version only after this — the second miss)
#   report     CI watch hint + consumer next steps
#
# --dry-run stops after the guard and restores the tree (no commit,
# tag, push, or slice). Invoking WITHOUT --dry-run IS the commit/push
# consent — that's the design: consent moves to the invocation boundary.
#
# macOS-only (BSD sed -i ''). Interruption recovery: if it dies between
# commit and push, finish manually with `git push origin main "v<new>"`;
# if it dies in the wheels barrier (tag already pushed), resolve CI/PyPI
# per the printed command, then finish with
# `bash scripts/dist-marketplace.sh`; if it dies mid-bump (dirty
# half-edited tree), `git checkout --` the five faces + Cargo.lock.
# Rerun after a full success is refused ("already at <ver>") by design.
set -euo pipefail
trap 'rm -f marketplace.json.rel-tmp .claude-plugin/marketplace.json.rel-tmp plugin/.claude-plugin/plugin.json.rel-tmp' EXIT

usage() { awk 'NR==1{next} /^# /{print; next} {exit}' "$0" >&2; exit 2; }

new=""
subject=""
dry_run=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry_run=1 ;;
    --subject) shift; [ $# -gt 0 ] || usage; subject="$1" ;;
    -h|--help) usage ;;
    -*) echo "unknown flag: $1" >&2; usage ;;
    *) [ -z "$new" ] || { echo "one version only" >&2; usage; }; new="$1" ;;
  esac
  shift
done
[ -n "$new" ] || usage

die() { echo "release: FAIL — $*" >&2; exit 1; }

# ---------- face extraction (same shapes as test-plugin-wrapper.sh) ----------
face_cargo() { sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1; }
face_plugin() { jq -r .version plugin/.claude-plugin/plugin.json; }
face_mkt() { jq -r '.plugins[0].version' marketplace.json; }
face_cc() { jq -r '.plugins[0].version' .claude-plugin/marketplace.json; }
face_want() {
  jq -r '.mcpServers["code-reality"].args | last' plugin/.mcp.json \
    | sed -n 's/.*want=\([0-9][0-9.]*\).*/\1/p'
}

FACES=(Cargo.toml plugin/.claude-plugin/plugin.json marketplace.json \
  .claude-plugin/marketplace.json plugin/.mcp.json)

# ---------- preflight ----------
[ -f Cargo.toml ] || die "run from repo root"

if [ -n "$(git status --porcelain)" ]; then
  git status --porcelain >&2
  die "working tree not clean — commit or stash first (release commit must be bump-only)"
fi

cur="$(face_cargo)"
[ "$(git branch --show-current)" = main ] || die "not on main (release pushes origin main)"
for f in "$(face_plugin)" "$(face_mkt)" "$(face_cc)" "$(face_want)"; do
  [ "$f" = "$cur" ] || die "pre-bump lockstep broken: cargo=$cur plugin=$(face_plugin) mkt=$(face_mkt) cc=$(face_cc) want=$(face_want) — fix before releasing"
done

[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must be X.Y.Z (got: $new)"
[ "$new" != "$cur" ] || die "already at $new (rerun after a completed release is refused by design; for a mid-flight retry see the recovery note in the header)"

echo "  preflight ok: five faces locked at $cur -> bumping to $new"

# ---------- bump ----------
# Escape the dots: $cur goes into sed/grep PATTERNS, where '.' is a
# metachar (review finding 3 — defensive; post-bump checks back it up).
esc_cur=${cur//./\\.}
grep -c "^version = \"$esc_cur\"" Cargo.toml | grep -qx 1 \
  || die "Cargo.toml version line count != 1 — refusing blind replace"
sed -i '' "s/^version = \"$esc_cur\"/version = \"$new\"/" Cargo.toml

jq_tmp() { jq --arg v "$new" "$1" "$2" > "$2.rel-tmp" && mv "$2.rel-tmp" "$2"; }
jq_tmp '.version = $v' plugin/.claude-plugin/plugin.json
jq_tmp '.plugins[0].version = $v' marketplace.json
jq_tmp '.plugins[0].version = $v' .claude-plugin/marketplace.json
# want= is a literal in the code-reality wrapper string; replace every
# occurrence (defensive /g — the bridge wrapper carries no want= today).
sed -i '' "s/want=$esc_cur/want=$new/g" plugin/.mcp.json

cargo update --workspace > /dev/null 2>&1 \
  || die "cargo update --workspace failed (Cargo.lock)"

# ---------- guard ----------
bash scripts/test-plugin-wrapper.sh > /dev/null \
  || die "wrapper lockstep guard FAILED after bump — inspect manually"
for f in "$(face_cargo)" "$(face_plugin)" "$(face_mkt)" "$(face_cc)" "$(face_want)"; do
  [ "$f" = "$new" ] || die "post-bump face != $new: $f"
done
echo "  bump ok + guard green: five faces at $new"

if [ "$dry_run" = 1 ]; then
  git checkout -- "${FACES[@]}" Cargo.lock
  echo "  dry-run: tree restored (guard re-check next line)"
  bash scripts/test-plugin-wrapper.sh > /dev/null \
    || die "post-restore guard failed — inspect the tree manually"
  echo "  dry-run clean at $(face_cargo)"
  exit 0
fi

# ---------- commit ----------
[ -n "$subject" ] || subject="release"
git add Cargo.toml Cargo.lock plugin/.claude-plugin/plugin.json \
  marketplace.json .claude-plugin/marketplace.json plugin/.mcp.json
git commit -m "chore(release): v$new — $subject

Five version faces bumped in lockstep (workspace, plugin.json,
marketplace x2, wrapper want=) via scripts/release.sh; wrapper
lockstep guard is five-point."
echo "  committed"

# ---------- tag + push ----------
git tag -a "v$new" -m "release: v$new — $subject"
git push origin main "v$new"
echo "  pushed main + tag v$new"

# ---------- wheels readiness barrier ----------
# The plugin face becomes updatable the moment the marketplace slice
# below is regenerated, but the binaries it exact-pins only exist on
# PyPI once CI finishes (trusted publishing, minutes). Slicing first
# opens a race: a plugin refreshed inside the window pins a version
# the registry cannot serve, and the wrapper fails loud at bootstrap
# (v0.9.1 incident). Order: watch THIS tag's release-wheels run to
# green, then probe the registry itself — CI green proves the publish
# job ran, the probe proves the state the wrapper actually consumes.
# Scope: this gates the local-slice face only; GitHub-marketplace
# consumers see main the moment it is pushed and rely on the same
# fail-loud self-heal, not on this barrier.
run_id=""
for _ in 1 2 3 4 5 6 7 8 9 10; do
  run_id=$(gh run list --workflow release-wheels.yml --branch "v$new" \
    --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
  if [ -n "$run_id" ]; then break; fi
  sleep 3
done
[ -n "$run_id" ] || die "no release-wheels run found for tag v$new — check 'gh run list --workflow release-wheels.yml' before slicing"
echo "  wheels barrier: watching run $run_id (CI build, minutes — hands off the plugin until done)"
gh run watch "$run_id" --interval 30 --exit-status \
  || die "wheels CI FAILED — tag already pushed; inspect 'gh run view $run_id --log-failed', fix forward, and cut the next patch release"
wheels_ok=""
for _ in 1 2 3 4 5 6 7 8 9 10 11 12; do
  if curl -sf -o /dev/null "https://pypi.org/pypi/code-reality/$new/json" \
    && curl -sf -o /dev/null "https://pypi.org/pypi/pyrefly-producer/$new/json" \
    && curl -sf -o /dev/null "https://pypi.org/pypi/code-reality-lsp-bridge/$new/json"; then
    wheels_ok=1
    break
  fi
  sleep 15
done
[ -n "$wheels_ok" ] || die "PyPI does not serve $new for all three dists (CI green but registry missing or lagging). Recovery: verify https://pypi.org/pypi/code-reality/$new/json manually, then finish with: bash scripts/dist-marketplace.sh"
echo "  wheels live on PyPI: all three dists at $new"

# ---------- local slice ----------
bash scripts/dist-marketplace.sh

# ---------- report ----------
echo
echo "[release] done. Next:"
echo "  1. ZCode: refresh the code-reality-market panel -> update plugin -> new session"
echo "  2. wheels verified live on PyPI at $new (uvx consumers see them now)"
