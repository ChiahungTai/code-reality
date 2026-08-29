#!/usr/bin/env bash
# Wrapper regression for plugin/.mcp.json (EP: ep-npm-embedded-face S3).
#
# Runs the ACTUAL wrapper strings extracted from plugin/.mcp.json via jq
# — not a copy: if the JSON changes, this test follows it or fails. Each
# server entry is exercised through the full candidate chain under a
# controlled environment (env -i):
#
#   Q1  PATH has the bin            -> PATH face (no prepend)
#   Q2  only plugin node_modules    -> embedded face (PATH prepended with
#                                      node_modules/.bin — the lsp-bridge
#                                      backend resolution depends on it)
#   Q3  only ~/.local/bin           -> uv fallback face (PATH prepended)
#   Q4  nothing anywhere            -> fail-loud guidance, exit 127
#   Q4b as Q4 but CLAUDE_PLUGIN_ROOT unset (the ZCode shape — empty
#                                      expansion degrades identically)
#   Q5  PATH and node_modules both  -> PATH wins (uv main face precedence)
#
# An unset/empty CLAUDE_PLUGIN_ROOT must degrade gracefully (ZCode has no
# expansion mechanism — Q3/Q4 cover that path).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MCP="$ROOT/plugin/.mcp.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
pass=0
fail=0

# mkfake <dir> <name>: executable that identifies itself and the PATH head
# it was exec'd with (asserts the wrapper's PATH prepend behavior).
mkfake() {
  mkdir -p "$1"
  printf '#!/bin/sh\necho "FAKE %s HEAD=${PATH%%:*}"\n' "$2" > "$1/$2"
  chmod +x "$1/$2"
}

# expect <label> <want_out> <want_rc> <actual_out> <actual_rc> <err>
expect() {
  if [ "$5" = "$3" ] && printf '%s' "$4" | grep -q "$2"; then
    pass=$((pass + 1)); printf '  ok   %s\n' "$1"
  else
    fail=$((fail + 1))
    printf '  FAIL %s — want rc=%s out~%s got rc=%s out=%s err=%s\n' \
      "$1" "$3" "$2" "$5" "$4" "$6"
  fi
}

for server in code-reality code-reality-lsp-bridge; do
  case "$server" in
    code-reality)          bin=code-reality-mcp ;;
    code-reality-lsp-bridge) bin=code-reality-lsp-bridge ;;
  esac
  wrapper="$(jq -r --arg s "$server" '.mcpServers[$s].args | last' "$MCP")"
  [ -n "$wrapper" ] || { echo "no wrapper string for $server in $MCP"; exit 1; }

  # fixtures
  fakepath="$WORK/pathbin";  mkfake "$fakepath" "$bin"
  proot="$WORK/proot";       mkfake "$proot/node_modules/.bin" "$bin"
  fakehome="$WORK/home";     mkfake "$fakehome/.local/bin" "$bin"
  emptyhome="$WORK/emptyhome"; mkdir -p "$emptyhome"

  out="$(env -i PATH="$fakepath" HOME="$emptyhome" CLAUDE_PLUGIN_ROOT="$WORK/none" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  expect "$server Q1 PATH face"        "FAKE $bin HEAD=$fakepath" 0 "$out" "$rc" "$(cat "$WORK/err")"

  out="$(env -i PATH=/nonexistent HOME="$emptyhome" CLAUDE_PLUGIN_ROOT="$proot" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  expect "$server Q2 node_modules face" "FAKE $bin HEAD=$proot/node_modules/.bin" 0 "$out" "$rc" "$(cat "$WORK/err")"

  out="$(env -i PATH=/nonexistent HOME="$fakehome" CLAUDE_PLUGIN_ROOT="$WORK/none" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  expect "$server Q3 ~/.local/bin face" "FAKE $bin HEAD=$fakehome/.local/bin" 0 "$out" "$rc" "$(cat "$WORK/err")"

  out="$(env -i PATH=/nonexistent HOME="$emptyhome" CLAUDE_PLUGIN_ROOT="$WORK/none" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  err="$(cat "$WORK/err")"
  if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q "uv tool install" \
     && printf '%s' "$err" | grep -q "update the code-reality plugin"; then
    pass=$((pass + 1)); printf '  ok   %s\n' "$server Q4 fail-loud"
  else
    fail=$((fail + 1)); printf '  FAIL %s — rc=%s err=%s\n' "$server Q4 fail-loud" "$rc" "$err"
  fi

  # Q4b: same, but CLAUDE_PLUGIN_ROOT genuinely unset (ZCode shape)
  out="$(env -i PATH=/nonexistent HOME="$emptyhome" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  err="$(cat "$WORK/err")"
  if [ "$rc" = 127 ] && printf '%s' "$err" | grep -q "uv tool install"; then
    pass=$((pass + 1)); printf '  ok   %s\n' "$server Q4b fail-loud (unset)"
  else
    fail=$((fail + 1)); printf '  FAIL %s — rc=%s err=%s\n' "$server Q4b fail-loud (unset)" "$rc" "$err"
  fi

  # Q5: PATH and node_modules both present -> PATH face wins (SM-4)
  out="$(env -i PATH="$fakepath" HOME="$emptyhome" CLAUDE_PLUGIN_ROOT="$proot" \
    /bin/sh -c "$wrapper" 2>"$WORK/err")"; rc=$?
  expect "$server Q5 PATH beats node_modules" "FAKE $bin HEAD=$fakepath" 0 "$out" "$rc" "$(cat "$WORK/err")"
done

printf 'wrapper regression: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" = 0 ]
