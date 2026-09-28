"""Isolated Round 0 probes; all runtime writes stay beside this script."""

import json
import os
from pathlib import Path
import re
import shlex
import sqlite3
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
BIN = str(BASE.parents[3] / "target/debug/code-reality")


def varint(value):
    result = bytearray()
    while value > 127:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def field(number, value):
    if isinstance(value, str):
        value = value.encode()
    return varint(number * 8 + 2) + varint(len(value)) + value


def index_for(source):
    doc = field(1, "app.py") + field(4, "python")
    for line, text in enumerate(source.splitlines()):
        match = re.match(r"def (\w+)\(", text)
        if not match:
            continue
        name = match[1]
        symbol = f"scip-python python probe 1 app/{name}()."
        occurrence = field(1, b"".join(varint(n) for n in [line, 4, 4 + len(name)]))
        occurrence += field(2, symbol) + varint(24) + varint(1)
        occurrence += field(7, b"".join(varint(n) for n in [line, 0, line, len(text)]))
        doc += field(2, occurrence)
    return field(2, doc)


def produce():
    if "--version" in sys.argv:
        sys.stdout.write("round0-deriving-fixture\n")
        return
    repo = Path(sys.argv[sys.argv.index("--repo") + 1])
    out = Path(sys.argv[sys.argv.index("--out") + 1])
    source = (repo / "app.py").read_text()
    (repo / "producer-input.txt").write_text(source)
    out.write_bytes(index_for(source))
    if (repo / "mutate-after-produce").exists():
        time.sleep(0.03)
        (repo / "app.py").write_text(source.replace("old_name", "new_name"))


def main():
    run_dir = BASE / (sys.argv[1] if len(sys.argv) > 1 else "run2")
    run_dir.mkdir(exist_ok=False)
    fake = run_dir / "bin"
    fake.mkdir()
    wrapper = fake / "pyrefly-index"
    wrapper.write_text("#!/bin/sh\nexec uv run --no-project python " + shlex.quote(str(Path(__file__).resolve())) + ' --produce "$@"\n')
    wrapper.chmod(0o755)
    env = dict(os.environ, CODE_REALITY_IDENTITY_CACHE="off", PATH=str(fake) + os.pathsep + os.environ["PATH"])
    records = []

    def run(label, args):
        completed = subprocess.run([BIN, *args], env=env, capture_output=True, text=True)
        record = dict(label=label, command=shlex.join([BIN, *args]), env_overrides={"CODE_REALITY_IDENTITY_CACHE": "off", "PATH_prefix": str(fake)}, exit=completed.returncode, stdout=completed.stdout, stderr=completed.stderr)
        records.append(record)
        (run_dir / f"{label}.json").write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n")
        return completed

    def fixture(name, source="def old_name(): return 1\ndef stable_name(): return 2\n"):
        repo = run_dir / name
        repo.mkdir()
        (repo / "app.py").write_text(source)
        return repo

    def nodes(repo):
        with sqlite3.connect(f"file:{repo}/.code-reality/graph.db?mode=ro", uri=True) as conn:
            return conn.execute("select name from nodes order by name").fetchall()

    run("version", ["--version"])
    a = fixture("A-in-build-mutation")
    (a / "mutate-after-produce").touch()
    run("A-build", ["build", "--repo", str(a), "--json"])
    run("A-freshness", ["freshness", "--repo", str(a), "--json"])
    run("A-refresh", ["refresh", "--repo", str(a)])
    (run_dir / "A-observation.json").write_text(json.dumps(dict(producer_input=(a / "producer-input.txt").read_text(), current_source=(a / "app.py").read_text(), graph_nodes=nodes(a)), indent=2) + "\n")

    b = fixture("B-first-build-failure")
    blocker = b / ".code-reality/graph.db.tmp-build"
    blocker.mkdir(parents=True)
    run("B-build-failure", ["build", "--repo", str(b), "--json"])
    assert not (b / ".code-reality/graph.db").exists()
    blocker.rmdir()
    run("B-freshness", ["freshness", "--repo", str(b), "--json"])
    run("B-refresh", ["refresh", "--repo", str(b)])
    (run_dir / "B-observation.json").write_text(json.dumps(dict(graph_exists_after_refresh=(b / ".code-reality/graph.db").exists(), slot_exists=(b / ".code-reality/scip/index.scip").exists())) + "\n")
    run("B-query", ["graph_query", "hub", "--repo", str(b)])
    run("B-explicit-recovery", ["build", "--repo", str(b), "--json"])
    run("B-recovery-freshness", ["freshness", "--repo", str(b), "--json"])

    d = fixture("D-concurrent-graph", "".join(f"def fn_{n:05}(): return 1\n" for n in range(5000)))
    slot = d / ".code-reality/scip/index.scip"
    slot.parent.mkdir(parents=True)
    slot.write_bytes(index_for((d / "app.py").read_text()))
    run("D-serial-control", ["graph_db", "build", "--repo", str(d), "--json"])
    args = [BIN, "graph_db", "build", "--repo", str(d), "--json"]
    processes = [subprocess.Popen(args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for _ in range(4)]
    for number, process in enumerate(processes):
        stdout, stderr = process.communicate()
        record = dict(label=f"D-concurrent-{number}", command=shlex.join(args), exit=process.returncode, stdout=stdout, stderr=stderr)
        records.append(record)
    try:
        final = dict(node_count=len(nodes(d)))
        with sqlite3.connect(f"file:{d}/.code-reality/graph.db?mode=ro", uri=True) as conn:
            final["integrity_check"] = conn.execute("pragma integrity_check").fetchall()
    except sqlite3.Error as error:
        final = dict(error=str(error))
    (run_dir / "D-observation.json").write_text(json.dumps(final, indent=2) + "\n")
    (run_dir / "commands.json").write_text(json.dumps(records, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    if "--produce" in sys.argv:
        produce()
    else:
        main()
