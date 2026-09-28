"""Exercise the real stdio MCP face with an isolated LSP backend."""
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import threading
import time

root = Path.cwd()
evidence = root / "audit/20260927/evidence/lsp"
responses = queue.Queue()
with (evidence / "mcp.stderr.log").open("w") as errors:
    proc = subprocess.Popen(
        [str(root / "target/debug/code-reality-lsp-bridge"), "--stdio", "--lsp-command", str(evidence / "backend.sh")],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, text=True,
    )
    def read_lines():
        for line in proc.stdout:
            responses.put(json.loads(line))
    threading.Thread(target=read_lines, daemon=True).start()
    def send(message):
        proc.stdin.write(json.dumps(message) + "\n")
        proc.stdin.flush()
    def request(rid, method, params):
        send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        deadline = time.monotonic() + 10
        while True:
            response = responses.get(timeout=max(0.001, deadline - time.monotonic()))
            if response.get("id") == rid:
                print(json.dumps(response), flush=True)
                return response
    try:
        request(1, "initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "lsp-audit", "version": "1"}})
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        params = {"name": "check_file", "arguments": {"file": str(evidence / "cache-fixture.py")}}
        first = request(2, "tools/call", params)
        pid = int((evidence / "mcp-backend.pid").read_text())
        os.kill(pid, signal.SIGKILL)
        for attempt in range(20):
            status = request(10 + attempt, "tools/call", {"name": "lsp_status", "arguments": {}})
            if "state=dead" in status["result"]["content"][0]["text"]:
                break
            time.sleep(0.05)
        else:
            raise AssertionError("backend death not observed")
        after = request(50, "tools/call", params)
        assert after["result"].get("isError") is False, after
        assert after["result"]["content"] == first["result"]["content"], after
        print("CONFIRMED: MCP check_file succeeds with unchanged count=0 after lsp_status reports dead", flush=True)
    finally:
        proc.stdin.close()
        try:
            code = proc.wait(timeout=5)
            print(f"bridge_exit={code}", flush=True)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            print("bridge required external cleanup", flush=True)
