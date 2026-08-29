#!/usr/bin/env bash
# Wrapper regression for plugin/.mcp.json (npm-face retirement arc:
# deferred uv bootstrap replaces the embedded face; node_modules stays
# as a deprecation-grace rescue when uv is missing).
#
# Runs the ACTUAL wrapper strings extracted via jq — not a copy. The
# pin under test is extracted from the wrapper string itself, so a
# bump that touches only one surface fails here.
#
# Lockstep guard v3: wrapper pin == plugin version == workspace version
# == BOTH marketplace listings (root marketplace.json is what ZCode reads —
# v2 missed it and the 0.5.0 release shipped with a stale market face).
#
#   T1 pin matches on PATH              -> direct exec, uv never invoked
#   T2 stale bin on PATH + uv           -> bootstrap: --force <pkg>==<pin>
#                                          for all three dists, then exec
#   T3 nothing + no uv + no embedded    -> 127 + uv install guidance
#   T4 CODE_REALITY_BOOTSTRAP=off       -> stale bin exec'd, uv untouched
#   T5 no uv + embedded node_modules    -> grace exec + deprecation notice
#   B1 bridge on PATH                   -> direct exec
#   B2 bridge nowhere + no uv           -> 127 fast (no wait loop)
#   B3 bridge via embedded node_modules -> direct exec
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MCP="$ROOT/plugin/.mcp.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
pass=0
fail=0

wrap="$(jq -r '.mcpServers["code-reality"].args | last' "$MCP")"
wrapb="$(jq -r '.mcpServers["code-reality-lsp-bridge"].args | last' "$MCP")"
[ -n "$wrap" ] && [ -n "$wrapb" ] || { echo "no wrapper strings in $MCP"; exit 1; }

# Lockstep guard v3: pin (in the wrapper) == plugin version == workspace
# == marketplace (ZCode) == marketplace (CC)
pin="$(printf '%s' "$wrap" | sed -n 's/.*want=\([0-9][0-9.]*\).*/\1/p')"
pver="$(jq -r .version "$ROOT/plugin/.claude-plugin/plugin.json")"
wver="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)"
mver="$(jq -r '.plugins[0].version' "$ROOT/marketplace.json")"
cver="$(jq -r '.plugins[0].version' "$ROOT/.claude-plugin/marketplace.json")"
if [ -n "$pin" ] && [ "$pin" = "$pver" ] && [ "$pin" = "$wver" ] && [ "$pin" = "$mver" ] && [ "$pin" = "$cver" ]; then
  pass=$((pass + 1)); printf '  ok   lockstep pin==plugin==workspace==marketplaces (%s)\n' "$pin"
else
  fail=$((fail + 1)); printf '  FAIL lockstep pin=%s plugin=%s workspace=%s mkt=%s cc=%s\n' "$pin" "$pver" "$wver" "$mver" "$cver"
fi

# mkfake <dir> <name> <ver>: bin whose every invocation prints "<ver>+rev"
mkfake() {
  mkdir -p "$1"
  printf '#!/bin/sh\necho "%s+rev"\n' "$3" > "$1/$2"
  chmod +x "$1/$2"
}

# uv stub: records argv; tests assert on the log (or its emptiness)
mkdir -p "$WORK/uvbin"
printf '#!/bin/sh\necho "uv $*" >> "%s/uv.log"\n' "$WORK" > "$WORK/uvbin/uv"
chmod +x "$WORK/uvbin/uv"
: > "$WORK/uv.log"

expect() { # <label> <want_out> <want_rc> <actual_out> <actual_rc> <err>
  if [ "$5" = "$3" ] && printf '%s' "$4" | grep -q "$2"; then
    pass=$((pass + 1)); printf '  ok   %s\n' "$1"
  else
    fail=$((fail + 1)); printf '  FAIL %s — want rc=%s out~%s got rc=%s out=%s err=%s\n' \
      "$1" "$3" "$2" "$5" "$4" "$6"
  fi
}

uv_untouched() { # <label>
  if [ -s "$WORK/uv.log" ]; then
    fail=$((fail + 1)); printf '  FAIL %s — uv was invoked: %s\n' "$1" "$(cat "$WORK/uv.log")"
  else
    pass=$((pass + 1)); printf '  ok   %s (uv untouched)\n' "$1"
  fi
}

mkdir -p "$WORK/home"

# T1: PATH bin at the pin -> direct exec, no bootstrap
mkfake "$WORK/match" code-reality-mcp "$pin"
out="$(env -i PATH="$WORK/match:$WORK/uvbin" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T1 pin match -> direct exec" "$pin+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"
uv_untouched "T1"

# T2: stale bin -> bootstrap all three dists at the pin, then exec
mkfake "$WORK/stale" code-reality-mcp 0.0.0
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/stale:$WORK/uvbin" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T2 stale -> bootstrap + exec" "0.0.0+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"
for spec in "code-reality==$pin" "code-reality-lsp-bridge==$pin" "pyrefly-producer==$pin"; do
  if grep -q -- "--force $spec" "$WORK/uv.log"; then
    pass=$((pass + 1)); printf '  ok   T2 uv --force %s\n' "$spec"
  else
    fail=$((fail + 1)); printf '  FAIL T2 uv missing --force %s\n' "$spec"
  fi
done

# T3: nothing anywhere, no uv -> loud 127 + install guidance
out="$(env -i PATH=/nonexistent HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q "uv not found" && printf '%s' "$err" | grep -q "astral.sh/uv"; then
  pass=$((pass + 1)); printf '  ok   T3 no-uv loud 127 + guidance\n'
else
  fail=$((fail + 1)); printf '  FAIL T3 rc=%s err=%s\n' "$rc" "$err"
fi

# T4: dev escape — stale bin exec'd as-is, no install
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/stale:$WORK/uvbin" CODE_REALITY_BOOTSTRAP=off HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T4 BOOTSTRAP=off -> exec as-is" "0.0.0+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"
uv_untouched "T4"

# T5: no uv but the retired embedded face is populated -> grace exec
mkfake "$WORK/proot/node_modules/.bin" code-reality-mcp 0.3.0
out="$(env -i PATH=/nonexistent CLAUDE_PLUGIN_ROOT="$WORK/proot" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 0 ] && printf '%s' "$out" | grep -q "0.3.0+rev" && printf '%s' "$err" | grep -q "deprecation grace"; then
  pass=$((pass + 1)); printf '  ok   T5 embedded grace exec + notice\n'
else
  fail=$((fail + 1)); printf '  FAIL T5 rc=%s out=%s err=%s\n' "$rc" "$out" "$err"
fi

# B1: bridge on PATH -> direct exec
mkfake "$WORK/bdir" code-reality-lsp-bridge "$pin"
out="$(env -i PATH="$WORK/bdir" HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
expect "B1 bridge on PATH" "$pin+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"

# B2: bridge nowhere + no uv -> fast 127 (the wait loop needs uv)
out="$(env -i PATH=/nonexistent HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q "code-reality server installs it"; then
  pass=$((pass + 1)); printf '  ok   B2 bridge fast 127\n'
else
  fail=$((fail + 1)); printf '  FAIL B2 rc=%s err=%s\n' "$rc" "$err"
fi

# B3: bridge via the embedded grace path
mkfake "$WORK/proot/node_modules/.bin" code-reality-lsp-bridge 0.3.0
out="$(env -i PATH=/nonexistent CLAUDE_PLUGIN_ROOT="$WORK/proot" HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
expect "B3 bridge embedded grace" "0.3.0+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"

printf 'wrapper regression: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" = 0 ]
