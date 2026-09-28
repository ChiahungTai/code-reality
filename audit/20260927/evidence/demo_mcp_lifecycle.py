"""Drive real stdio MCP with controlled external producer processes."""
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / 'mcp-lifecycle'
OUT.mkdir(exist_ok=True)
BIN = ROOT / 'target/debug/code-reality-mcp'


def run_case(mode):
    case = OUT / mode
    case.mkdir(exist_ok=True)
    repo = case / 'repo'
    repo.mkdir(exist_ok=True)
    (repo / 'example.py').write_text('def example():\n    return 1\n')
    started = case / 'started'
    completed = case / 'completed'
    started.unlink(missing_ok=True)
    completed.unlink(missing_ok=True)
    producer = case / 'pyrefly-index'
    body = ('/bin/sleep 3\nprintf done > "' + str(completed) + '"\n') if mode != 'large-error' else ('/usr/bin/head -c 1200000 /dev/zero | /usr/bin/tr "\\000" E >&2\n')
    producer.write_text('#!/bin/sh\nif [ "$1" = --version ]; then echo 0.9.3; exit 0; fi\nprintf started > "' + str(started) + '"\n' + body + 'exit 2\n')
    producer.chmod(0o755)
    env = dict(os.environ, PATH=str(case) + ':' + os.environ['PATH'], CR_REPO='/nonexistent', CODE_REALITY_AUTOHEAL='off')
    lines = queue.Queue()
    with (case / 'stderr.log').open('w') as err:
        process = subprocess.Popen([str(BIN), '--stdio'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True, env=env, start_new_session=True)
        reader = threading.Thread(target=lambda: [lines.put(line) for line in process.stdout], daemon=True)
        reader.start()

        def send(message):
            process.stdin.write(json.dumps({'jsonrpc': '2.0', **message}) + '\n')
            process.stdin.flush()

        def receive(request_id, timeout=15):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                line = lines.get(timeout=max(0.01, deadline-time.monotonic()))
                value = json.loads(line)
                if value.get('id') == request_id:
                    return line, value
            raise TimeoutError(request_id)

        try:
            send({'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'round0-audit', 'version': '1'}}})
            receive(1)
            send({'method': 'notifications/initialized'})
            send({'id': 2, 'method': 'tools/call', 'params': {'name': 'build', 'arguments': {'repo_root': str(repo), 'producer': 'python'}}})
            deadline = time.monotonic() + 10
            while not started.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            assert started.exists(), 'producer did not start'
            result = {'mode': mode, 'producer_started': True}
            if mode == 'large-error':
                line, value = receive(2)
                (case / 'response.json').write_text(line)
                result.update(response_bytes=len(line.encode()), error_code=value.get('error', {}).get('code'), error_message_bytes=len(value.get('error', {}).get('message', '').encode()), truncated_marker='[TRUNCATED]' in line)
            elif mode == 'cancel':
                send({'method': 'notifications/cancelled', 'params': {'requestId': 2, 'reason': 'audit cancellation'}})
                send({'id': 3, 'method': 'tools/list'})
                receive(3)
                result['server_responsive_after_cancel'] = True
                time.sleep(3.5)
                result['producer_completed_after_cancel'] = completed.exists()
                result['pending_response_ids'] = [json.loads(lines.get_nowait()).get('id') for _ in range(lines.qsize())]
            else:
                process.stdin.close()
                before = time.monotonic()
                try:
                    process.wait(timeout=0.5)
                    result['alive_after_eof_500ms'] = False
                except subprocess.TimeoutExpired:
                    result['alive_after_eof_500ms'] = True
                process.wait(timeout=8)
                result['exit_elapsed_after_eof_seconds'] = round(time.monotonic() - before, 3)
                result['producer_completed_after_eof'] = completed.exists()
            if not process.stdin.closed:
                process.stdin.close()
            process.wait(timeout=8)
            result['server_exit'] = process.returncode
            return result
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


results = [run_case(mode) for mode in ['large-error', 'cancel', 'eof']]
(OUT / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
print('[OK] mcp_lifecycle: collected real stdio error-size, cancellation and EOF observations')
