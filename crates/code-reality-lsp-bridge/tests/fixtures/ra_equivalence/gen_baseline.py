"""Generate the rust-analyzer hover baseline for the P2 equivalence
battery (same-engine round-trip consistency — the P2 gate oracle).

Runs `rust-analyzer` (no flags; default stdio is LSP), warms up with a
discarded hover, hovers each battery position directly, normalizes
(leading-whitespace strip + first ```rust fence), and freezes the
result plus serverInfo.version into ra_hover_baseline.json.

Regenerate: uv run python tests/fixtures/ra_equivalence/gen_baseline.py
(when the PATH rust-analyzer version changes, expect the frozen text
to drift — the battery skips loudly on version mismatch.)
"""

import json
import pathlib
import re
import subprocess
import sys
import threading
import time

HERE = pathlib.Path(__file__).parent
CRATE_ROOT = pathlib.Path(__file__).resolve().parents[3]
TARGET = CRATE_ROOT / "src/framing.rs"
OUT = HERE / "ra_hover_baseline.json"

# (label, line, character) — zero-based, mid-identifier (ra's hit-test
# returns empty on some boundary positions), machine-verified against
# `cat -n src/framing.rs`. Deliberately NO type symbols: ra appends
# `// size = N, align = N` lines that vary by target architecture and
# must not be normalized away (EP C-F-06).
POSITIONS = [
    ("write_message", 11, 10),
    ("read_message", 19, 10),
]


def normalize(hover_markdown: str) -> str:
    # ra hover = module-path fence + signature fence (+ optional more).
    # Join ALL ```rust fences in order — the first alone carries only
    # the module path and cannot distinguish symbols.
    body = hover_markdown.strip()
    parts = re.findall(r"```rust\n(.*?)```", body, re.DOTALL)
    return " | ".join(p.strip() for p in parts)


def main() -> int:
    proc = subprocess.Popen(
        ["rust-analyzer"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    inbox: list = []

    def send(msg: dict) -> None:
        body = json.dumps(msg).encode()
        proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        proc.stdin.flush()

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
        end = time.time() + timeout
        while time.time() < end:
            for i, msg in enumerate(inbox):
                if msg.get("id") == rid and ("result" in msg or "error" in msg):
                    inbox.pop(i)
                    return msg
            time.sleep(0.05)
        raise TimeoutError(method)

    uri = TARGET.resolve().as_uri()
    root = CRATE_ROOT
    init = req("initialize", {
        "processId": None,
        "rootUri": root.as_uri(),
        "capabilities": {"textDocument": {"hover": {"contentFormat": ["markdown", "plaintext"]}}},
    })
    version = init["result"].get("serverInfo", {}).get("version", "?")
    print(f"[OK] rust-analyzer: {version}")
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": {"uri": uri, "languageId": "rust", "version": 1, "text": TARGET.read_text()}}})

    # Warm-up hover (discarded): during workspace load the module-path
    # line may be absent — sample only after it stabilizes.
    for _ in range(120):
        res = req("textDocument/hover", {"textDocument": {"uri": uri},
                   "position": {"line": POSITIONS[0][1], "character": POSITIONS[0][2]}})
        value = ((res.get("result") or {}).get("contents") or {}).get("value", "")
        if "framing" in value:
            print("[OK] warmed up")
            break
        time.sleep(1)

    baseline = {"_generator": "rust-analyzer", "_version": version, "_positions": {}}
    ok = True
    for label, line, ch in POSITIONS:
        res = req("textDocument/hover", {"textDocument": {"uri": uri},
                   "position": {"line": line, "character": ch}})
        value = ((res.get("result") or {}).get("contents") or {}).get("value", "")
        norm = normalize(value)
        baseline["_positions"][label] = norm
        status = "OK" if norm else "EMPTY"
        if not norm:
            ok = False
        print(f"[{status}] {label} -> {json.dumps(norm)[:120]}")

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
