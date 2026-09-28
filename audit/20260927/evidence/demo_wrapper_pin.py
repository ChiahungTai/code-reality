"""Exercise actual plugin wrappers against version-neighbor executable faces."""
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / 'wrapper-pin'
OUT.mkdir(exist_ok=True)
config = json.loads((ROOT / 'plugin/.mcp.json').read_text())['mcpServers']


def executable(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    path.chmod(0o755)


results = []
for name, face in config.items():
    wrapper = face['args'][1]
    want = re.search(r'want=([^;]+)', wrapper).group(1)
    for suffix in ['+testrev', '0+testrev', '-rc1+testrev']:
        version = want + suffix
        case = OUT / (name + '-' + suffix.replace('+', '_'))
        bins = case / 'bins'
        calls = case / 'uv.log'
        executable(bins / 'uv', '#!/bin/sh\nif [ "$1 $2 $3" = "tool dir --bin" ]; then\n echo "' + str(bins) + '"\nelse\n echo "$*" >> "' + str(calls) + '"\n exit 17\nfi\n')
        for binary in ['code-reality-mcp', 'code-reality-lsp-bridge', 'pyrefly-index']:
            executable(bins / binary, '#!/bin/sh\nif [ "$1" = --version ]; then\n echo "' + version + '"\nelse\n echo "SERVED ' + binary + ' ' + version + '"\nfi\n')
        run = subprocess.run(['/bin/sh', '-c', wrapper], env={'PATH': str(bins)}, capture_output=True, text=True, timeout=5)
        results.append({'face': name, 'wanted': want, 'actual': version, 'exit': run.returncode, 'stdout': run.stdout.strip(), 'stderr': run.stderr.strip(), 'attempted_install': calls.exists()})
(OUT / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
assert all(row['exit'] == 0 and not row['attempted_install'] for row in results), results
print('[OK] wrapper_pin: both real wrappers accept exact, neighbor-patch and prerelease versions; see wrapper-pin/results.json')
