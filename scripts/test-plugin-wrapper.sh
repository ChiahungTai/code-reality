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
#   T1 pinned uv face + stale PATH      -> direct uv-face exec, no install
#   T2 pinned PATH + stale uv face      -> bootstrap all three dists, then
#                                          exec the newly pinned uv face
#   T2b uv install exits 0 but stays stale -> fail loud after postcheck
#   T2c only a side face (bridge) stale -> postcheck fails: the three-face
#                                          conjunction is load-bearing
#   T2d fresh machine, empty uv bin dir -> install converges, uv-face exec
#   T3 nothing + no uv + no embedded    -> 127 + uv install guidance
#   T4 CODE_REALITY_BOOTSTRAP=off       -> stale bin exec'd, uv untouched
#   T5 no uv + embedded node_modules    -> grace exec + deprecation notice
#   B1 bridge uv face beats PATH        -> direct pinned uv-face exec
#   B2 bridge nowhere + no uv           -> 127 fast (no wait loop)
#   B3 bridge via embedded node_modules -> direct exec
#   B4 bridge BOOTSTRAP=off             -> developer PATH face wins
#
# Exec-source identity (F2): uv-face fixtures are tagged "+uvface" so the
# expected output proves WHICH binary exec'd — a wrapper that regresses to
# exec'ing the PATH binary cannot pass T1/T2/B1.
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
bpin="$(printf '%s' "$wrapb" | sed -n 's/.*want=\([0-9][0-9.]*\).*/\1/p')"
pver="$(jq -r .version "$ROOT/plugin/.claude-plugin/plugin.json")"
wver="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)"
mver="$(jq -r '.plugins[0].version' "$ROOT/marketplace.json")"
cver="$(jq -r '.plugins[0].version' "$ROOT/.claude-plugin/marketplace.json")"
if [ -n "$pin" ] && [ "$pin" = "$bpin" ] && [ "$pin" = "$pver" ] && [ "$pin" = "$wver" ] && [ "$pin" = "$mver" ] && [ "$pin" = "$cver" ]; then
  pass=$((pass + 1)); printf '  ok   lockstep pin==plugin==workspace==marketplaces (%s)\n' "$pin"
else
  fail=$((fail + 1)); printf '  FAIL lockstep mcp-pin=%s bridge-pin=%s plugin=%s workspace=%s mkt=%s cc=%s\n' "$pin" "$bpin" "$pver" "$wver" "$mver" "$cver"
fi

# mkfake <dir> <name> <ver> [tag]: bin whose every invocation prints
# "<ver>+rev<tag>" — the tag marks the fixture's origin (exec-source identity)
mkfake() {
  mkdir -p "$1"
  printf '#!/bin/sh\necho "%s+rev%s"\n' "$3" "${4:-}" > "$1/$2"
  chmod +x "$1/$2"
}

# uv stubs expose a canonical tool bin dir. The installing variant copies
# prepared pinned binaries there; the noop variant simulates `uv` returning 0
# without actually converging the executable face.
mkdir -p "$WORK/uvface" "$WORK/pinned"
mkfake "$WORK/pinned" code-reality-mcp "$pin" "+uvface"
mkfake "$WORK/pinned" code-reality-lsp-bridge "$pin" "+uvface"
mkfake "$WORK/pinned" pyrefly-index "$pin" "+uvface"

mk_uv_stub() { # <dir> <mode: install|noop>
  mkdir -p "$1"
  cat > "$1/uv" <<EOF
#!/bin/sh
if [ "\$1" = tool ] && [ "\$2" = dir ] && [ "\$3" = --bin ]; then
  echo "$WORK/uvface"
  exit 0
fi
echo "uv \$*" >> "$WORK/uv.log"
if [ "$2" = install ] && [ "\$1" = tool ] && [ "\$2" = install ]; then
  case "\$4" in
    code-reality==*) /bin/cp "$WORK/pinned/code-reality-mcp" "$WORK/uvface/code-reality-mcp" ;;
    code-reality-lsp-bridge==*) /bin/cp "$WORK/pinned/code-reality-lsp-bridge" "$WORK/uvface/code-reality-lsp-bridge" ;;
    pyrefly-producer==*) /bin/cp "$WORK/pinned/pyrefly-index" "$WORK/uvface/pyrefly-index" ;;
  esac
fi
exit 0
EOF
  chmod +x "$1/uv"
}

mk_uv_stub "$WORK/uvinstall" install
mk_uv_stub "$WORK/uvnoop" noop
: > "$WORK/uv.log"

expect() { # <label> <want_out> <want_rc> <actual_out> <actual_rc> <err>
  if [ "$5" = "$3" ] && printf '%s' "$4" | grep -q "$2"; then
    pass=$((pass + 1)); printf '  ok   %s\n' "$1"
  else
    fail=$((fail + 1)); printf '  FAIL %s — want rc=%s out~%s got rc=%s out=%s err=%s\n' \
      "$1" "$3" "$2" "$5" "$4" "$6"
  fi
}

no_install() { # <label> — zero install side effects (uv tool dir --bin IS allowed)
  if [ -s "$WORK/uv.log" ]; then
    fail=$((fail + 1)); printf '  FAIL %s — uv was invoked: %s\n' "$1" "$(cat "$WORK/uv.log")"
  else
    pass=$((pass + 1)); printf '  ok   %s (no install)\n' "$1"
  fi
}

mkdir -p "$WORK/home"

# T1: bootstrap-on consumer face is uv. A stale PATH binary must not matter.
mkfake "$WORK/path-stale" code-reality-mcp 0.0.0
mkfake "$WORK/uvface" code-reality-mcp "$pin" "+uvface"
mkfake "$WORK/uvface" code-reality-lsp-bridge "$pin"
mkfake "$WORK/uvface" pyrefly-index "$pin"
out="$(env -i PATH="$WORK/path-stale:$WORK/uvnoop" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T1 pinned uv face beats stale PATH" "$pin+rev+uvface" 0 "$out" "$rc" "$(cat "$WORK/err")"
no_install "T1"

# T2: even a pinned PATH/cargo face cannot prove the uv consumer face is fresh.
mkfake "$WORK/path-pin" code-reality-mcp "$pin"
mkfake "$WORK/uvface" code-reality-mcp 0.0.0
mkfake "$WORK/uvface" code-reality-lsp-bridge "$pin"
mkfake "$WORK/uvface" pyrefly-index "$pin"
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/path-pin:$WORK/uvinstall" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T2 pinned PATH cannot mask stale uv face" "$pin+rev+uvface" 0 "$out" "$rc" "$(cat "$WORK/err")"
for spec in "code-reality==$pin" "code-reality-lsp-bridge==$pin" "pyrefly-producer==$pin"; do
  if grep -q -- "--force $spec" "$WORK/uv.log"; then
    pass=$((pass + 1)); printf '  ok   T2 uv --force %s\n' "$spec"
  else
    fail=$((fail + 1)); printf '  FAIL T2 uv missing --force %s\n' "$spec"
  fi
done

# T2b: package-manager exit 0 is insufficient; post-install binary state is
# the invariant. Keep the uv face stale and require a loud failure.
mkfake "$WORK/uvface" code-reality-mcp 0.0.0
mkfake "$WORK/uvface" code-reality-lsp-bridge "$pin"
mkfake "$WORK/uvface" pyrefly-index "$pin"
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/path-pin:$WORK/uvnoop" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q -e "still stale" -e "post-install"; then
  pass=$((pass + 1)); printf '  ok   T2b uv exit 0 + stale binary fails postcheck\n'
else
  fail=$((fail + 1)); printf '  FAIL T2b rc=%s out=%s err=%s\n' "$rc" "$out" "$err"
fi

# T2c: the three-face conjunction is load-bearing — one stale SIDE face must
# block startup even with the MCP face pinned (guards a regression to
# checking only the exec'd binary).
mkfake "$WORK/uvface" code-reality-mcp "$pin"
mkfake "$WORK/uvface" code-reality-lsp-bridge 0.0.0
mkfake "$WORK/uvface" pyrefly-index "$pin"
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/path-pin:$WORK/uvnoop" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q -e "still stale" -e "post-install"; then
  pass=$((pass + 1)); printf '  ok   T2c stale side face (bridge) fails postcheck\n'
else
  fail=$((fail + 1)); printf '  FAIL T2c rc=%s out=%s err=%s\n' "$rc" "$out" "$err"
fi

# T2d: fresh consumer machine — uv present, empty tool bin dir -> the
# install leg converges all three faces before the first exec.
rm -f "$WORK/uvface/code-reality-mcp" "$WORK/uvface/code-reality-lsp-bridge" "$WORK/uvface/pyrefly-index"
: > "$WORK/uv.log"
out="$(env -i PATH="$WORK/path-pin:$WORK/uvinstall" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T2d fresh empty uv bin dir -> install converges" "$pin+rev+uvface" 0 "$out" "$rc" "$(cat "$WORK/err")"

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
out="$(env -i PATH="$WORK/path-stale:$WORK/uvnoop" CODE_REALITY_BOOTSTRAP=off HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
expect "T4 BOOTSTRAP=off -> exec as-is" "0.0.0+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"
no_install "T4"

# T5: no uv but the retired embedded face is populated -> grace exec
mkfake "$WORK/proot/node_modules/.bin" code-reality-mcp 0.3.0
out="$(env -i PATH=/nonexistent CLAUDE_PLUGIN_ROOT="$WORK/proot" HOME="$WORK/home" /bin/sh -c "$wrap" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 0 ] && printf '%s' "$out" | grep -q "0.3.0+rev" && printf '%s' "$err" | grep -q "deprecation grace"; then
  pass=$((pass + 1)); printf '  ok   T5 embedded grace exec + notice\n'
else
  fail=$((fail + 1)); printf '  FAIL T5 rc=%s out=%s err=%s\n' "$rc" "$out" "$err"
fi

# B1: bootstrap-on bridge must use the same canonical uv face as the server.
mkfake "$WORK/bdir" code-reality-lsp-bridge 9.9.9
mkfake "$WORK/uvface" code-reality-lsp-bridge "$pin" "+uvface"
out="$(env -i PATH="$WORK/bdir:$WORK/uvnoop" HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
expect "B1 bridge uv face beats PATH" "$pin+rev+uvface" 0 "$out" "$rc" "$(cat "$WORK/err")"

# B2: bridge nowhere + no uv -> fast 127 (nothing can bootstrap it)
out="$(env -i PATH=/nonexistent HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
err="$(cat "$WORK/err")"
if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q "uv is missing"; then
  pass=$((pass + 1)); printf '  ok   B2 bridge fast 127\n'
else
  fail=$((fail + 1)); printf '  FAIL B2 rc=%s err=%s\n' "$rc" "$err"
fi

# B3: bridge via the embedded grace path
mkfake "$WORK/proot/node_modules/.bin" code-reality-lsp-bridge 0.3.0
out="$(env -i PATH=/nonexistent CLAUDE_PLUGIN_ROOT="$WORK/proot" HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
expect "B3 bridge embedded grace" "0.3.0+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"

# B4: developer override intentionally returns to normal PATH resolution.
out="$(env -i PATH="$WORK/bdir:$WORK/uvnoop" CODE_REALITY_BOOTSTRAP=off HOME="$WORK/home" /bin/sh -c "$wrapb" 2>"$WORK/err")"; rc=$?
expect "B4 bridge BOOTSTRAP=off uses PATH" "9.9.9+rev" 0 "$out" "$rc" "$(cat "$WORK/err")"

printf 'wrapper regression: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" = 0 ]
