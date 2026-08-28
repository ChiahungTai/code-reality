"""Generate the pyright hover baseline for the equivalence battery.

Runs `pyright-langserver --stdio` (the golden-oracle role — pyright is
NOT the production type face; see AGENTS.md), hovers each battery
position, normalizes both engines' hover markdown with ONE shared spec
(EP S5), and freezes the result into pyright_hover_baseline.json.

Regenerate: uv run python tests/fixtures/equivalence/gen_baseline.py

Normalization spec (shared with the Rust battery test):
1. take the FIRST ```python fenced block's content
2. strip a leading kind prefix like ``(variable) `` / ``(function) ``
   (the two engines use different kind token sets)
3. strip a leading ``name: `` prefix (pyrefly signs functions as
   ``scale: def scale(...)``; both engines' variables are signed
   ``name: type`` — the strip is symmetric on both sides)
4. strip a trailing ``: ...`` implementation marker (pyrefly only)
5. fold all whitespace runs (newlines, indentation) into single spaces
"""

import json
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).parent
BATTERY = HERE / "battery.py"
OUT = HERE / "pyright_hover_baseline.json"

# (label, line, character) — zero-based, UTF-16 code units. Verified
# against `cat -n battery.py`: def scale at 0-based line 3, the Box
# annotation reference at line 11, the size attribute reference at
# line 12. Positions land mid-identifier (pyright's hit-test returns
# empty for the exact first character of a name; pyrefly returns empty
# for blank lines where pyright falls back to a nearby symbol — both
# engines' boundaries are avoided). Class reference is EXCLUDED from
# the exact-string parity set: pyrefly shows the constructor signature
# (`(class) Box: def Box() -> Box: ...`) where pyright shows just the
# name (`(class) Box`) — a display-depth difference, not a format one;
# the class kind is asserted pyrefly-side in the bridge tests.
PARITY_POSITIONS = [
    ("count_var", 0, 2),
    ("scale_func", 3, 6),
    ("attr_probe", 12, 14),
]
# Recorded for reference only (not part of the equality contract).
PROBE_ONLY_POSITIONS = [
    ("box_class", 11, 6),
]


def normalize(hover_markdown: str) -> str:
    m = re.search(r"```python\n(.*?)```", hover_markdown, re.DOTALL)
    if not m:
        return ""
    body = m.group(1)
    body = re.sub(r"^\([a-z ]+\)\s*", "", body.strip())
    body = re.sub(r"^[A-Za-z_][A-Za-z0-9_]*:\s*", "", body)
    body = re.sub(r":\s*\.\.\.$", "", body)
    return re.sub(r"\s+", " ", body).strip()


def main() -> int:
    proc = subprocess.Popen(
        ["pyright-langserver", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    inbox: list[dict] = []

    def send(msg: dict) -> None:
        body = json.dumps(msg).encode()
        proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        proc.stdin.flush()

    import threading

    def reader() -> None:
        while True:
            headers = {}
            while True:
                line = proc.stdout.readline()
                if not line:
                    return
                if line in (b"\r\n", b"\n"):
                    break
                k, _, v = line.decode().partition(":")
                headers[k.strip().lower()] = v.strip()
            inbox.append(json.loads(proc.stdout.read(int(headers["content-length"]))))

    threading.Thread(target=reader, daemon=True).start()

    next_id = [0]

    def req(method: str, params, timeout: float = 60.0) -> dict:
        next_id[0] += 1
        rid = next_id[0]
        send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        import time

        end = time.time() + timeout
        while time.time() < end:
            for i, msg in enumerate(inbox):
                if msg.get("id") == rid and ("result" in msg or "error" in msg):
                    inbox.pop(i)
                    return msg
            time.sleep(0.05)
        raise TimeoutError(method)

    text = BATTERY.read_text()
    uri = BATTERY.resolve().as_uri()
    init = req("initialize", {
        "processId": None,
        "rootUri": BATTERY.parent.resolve().as_uri(),
        "capabilities": {"textDocument": {"hover": {"contentFormat": ["markdown"]}}},
    })
    print("[OK] pyright:", json.dumps(init["result"].get("serverInfo", {})))
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": {"uri": uri, "languageId": "python", "version": 1, "text": text}}})

    import time

    time.sleep(3)  # let pyright analyze

    baseline = {"_generator": "pyright-langserver", "_positions": {}, "_probe_only": {}}
    ok = True
    for positions, bucket in [(PARITY_POSITIONS, "_positions"), (PROBE_ONLY_POSITIONS, "_probe_only")]:
        for label, line, ch in positions:
            res = req("textDocument/hover", {"textDocument": {"uri": uri}, "position": {"line": line, "character": ch}})
            raw = ((res.get("result") or {}).get("contents") or {})
            value = raw.get("value", "") if isinstance(raw, dict) else str(raw)
            norm = normalize(value)
            baseline[bucket][label] = norm
            if bucket == "_positions":
                status = "OK" if norm else "EMPTY"
                if not norm:
                    ok = False
                print(f"[{status}] {label} ({line}:{ch}) raw={json.dumps(value)[:120]} -> {json.dumps(norm)[:100]}")
            else:
                print(f"[probe] {label} -> {json.dumps(norm)[:80]}")

    send({"jsonrpc": "2.0", "method": "shutdown", "id": next_id[0] + 1, "params": None})
    send({"jsonrpc": "2.0", "method": "exit"})
    proc.wait(timeout=10)

    if not ok:
        print("[FAIL] some positions had no hover — refusing to write baseline")
        return 1
    OUT.write_text(json.dumps(baseline, indent=2) + "\n")
    print(f"[OK] baseline written: {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
