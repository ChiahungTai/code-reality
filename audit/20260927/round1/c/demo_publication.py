"""Independent real CLI acceptance adapted from immutable Round 0 data probe.

Oracle S: accepted EP C. Original protobuf fixture helpers are reused; assertions
observe disk bytes, CLI verdicts and SQLite integrity, not Rust helper results.
All writes stay in this audit directory; no Git mutation or package install.
"""
import fcntl
import json
import os
from pathlib import Path
import re
import runpy
import shlex
import signal
import sqlite3
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[3]
BIN = str(ROOT / "target/debug/code-reality")
ORIGINAL = runpy.run_path(str(ROOT / "audit/20260927/evidence/data/demo_data_boundaries.py"))


def index_with_calls(source):
    """Round 0 encoding plus explicit return-call occurrences for a flow control."""
    field = ORIGINAL["field"]
    varint = ORIGINAL["varint"]
    doc = field(1, "app.py") + field(4, "python")
    for line, text in enumerate(source.splitlines()):
        name = re.match(r"def (\w+)\(", text)[1]
        symbol = f"scip-python python probe 1 app/{name}()."
        occurrence = field(1, b"".join(varint(n) for n in [line, 4, 4 + len(name)]))
        occurrence += field(2, symbol) + varint(24) + varint(1)
        occurrence += field(7, b"".join(varint(n) for n in [line, 0, line, len(text)]))
        doc += field(2, occurrence)
        call = re.search(r"return (\w+)\(", text)
        if call:
            callee = call[1]
            start = call.start(1)
            occurrence = field(1, b"".join(varint(n) for n in [line, start, start + len(callee)]))
            occurrence += field(2, f"scip-python python probe 1 app/{callee}().")
            doc += field(2, occurrence)
    return field(2, doc)


def produce():
    if "--version" in sys.argv:
        sys.stdout.write("round1-independent-deriving-fixture\n")
        return
    repo = Path(sys.argv[sys.argv.index("--repo") + 1])
    out = Path(sys.argv[sys.argv.index("--out") + 1])
    source = (repo / "app.py").read_text()
    (repo / "producer-input.txt").write_text(source)
    out.write_bytes(index_with_calls(source) if (repo / "flow-control").exists() else ORIGINAL["index_for"](source))
    if (repo / "mutate-after-produce").exists():
        (repo / "app.py").write_text(source.replace("old_name", "new_name"))
    if (repo / "mutate-policy").exists():
        (repo / ".code-reality.toml").write_text('exclude = ["nothing/"]\n')
    (repo / "producer-finished").touch()


def hold():
    path = Path(sys.argv[2])
    with path.open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        Path(sys.argv[3]).write_text(str(os.getpid()))
        time.sleep(60)


def main():
    run_dir = BASE / sys.argv[1]
    run_dir.mkdir(exist_ok=False)
    fake = run_dir / "bin"
    fake.mkdir()
    wrapper = fake / "pyrefly-index"
    wrapper.write_text("#!/bin/sh\nexec uv run --no-project python " + shlex.quote(str(Path(__file__).resolve())) + ' --produce "$@"\n')
    wrapper.chmod(0o755)
    env = dict(os.environ, CODE_REALITY_IDENTITY_CACHE="off", PATH=str(fake) + os.pathsep + os.environ["PATH"])
    records = []
    checks = []

    def record(label, args, code, stdout, stderr, elapsed):
        value = dict(label=label, command=shlex.join(args), exit=code,
                     stdout=stdout, stderr=stderr, elapsed=elapsed)
        records.append(value)
        (run_dir / f"{label}.json").write_text(json.dumps(value, indent=2) + "\n")

    def run(label, args, expected=0):
        command = [BIN, *args]
        start = time.monotonic()
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=45)
        record(label, command, result.returncode, result.stdout, result.stderr, time.monotonic() - start)
        if expected is not None:
            assert result.returncode == expected, (label, result.returncode, result.stderr)
        return result

    def fixture(name, source="def old_name(): return 1\ndef stable_name(): return 2\n"):
        repo = run_dir / name
        repo.mkdir()
        (repo / "app.py").write_text(source)
        return repo

    def artifacts(repo):
        return [repo / ".code-reality/scip/index.scip", repo / ".code-reality/scip/index.scip.meta.json", repo / ".code-reality/graph.db"]

    def plane(repo):
        return {str(p): p.read_bytes() for p in artifacts(repo)}

    def verify_graph(repo, expected_nodes):
        with sqlite3.connect(f"file:{repo}/.code-reality/graph.db?mode=ro", uri=True) as conn:
            assert conn.execute("pragma integrity_check").fetchall() == [("ok",)]
            assert conn.execute("select count(*) from nodes").fetchone()[0] == expected_nodes
            counts = {name: conn.execute(f"select count(*) from {name}").fetchone()[0]
                      for name in ("flows", "flow_memberships", "communities")}
            assert counts["communities"] > 0
            assert conn.execute("select count(*) from flow_memberships m left join flows f on f.id=m.flow_id left join nodes n on n.id=m.node_id where f.id is null or n.id is null").fetchone()[0] == 0
            return counts

    try:
        run("version", ["--version"])
        a = fixture("A-production-drift")
        run("A-stable-build", ["build", "--repo", str(a), "--json"])
        healthy = json.loads(run("A-stable-freshness", ["freshness", "--repo", str(a), "--json"]).stdout)
        assert healthy["serves"] == "current-tree"
        before = plane(a)
        (a / "mutate-after-produce").touch()
        run("A-drift-rejected", ["build", "--repo", str(a), "--json"], 2)
        assert plane(a) == before
        assert "old_name" in (a / "producer-input.txt").read_text()
        assert "new_name" in (a / "app.py").read_text()
        checks.append("production edit rejects publication; prior slot/meta/graph byte-identical")

        run("A-manual-stamp", ["scip_refs", "--repo", str(a), "--stamp-meta"])
        stale = json.loads(run("A-restamp-negative", ["freshness", "--repo", str(a), "--json"], 1).stdout)
        assert stale["indexed_source_identity"] == healthy["indexed_source_identity"]
        assert stale["serves"] == "committed-baseline" and "content-drift" in stale["stale_reasons"]
        checks.append("manual restamp retains prior identity and stale content verdict")

        b = fixture("B-graphfail")
        blocker = b / ".code-reality/graph.db"
        blocker.mkdir(parents=True)
        failed = run("B-publish-failure", ["build", "--repo", str(b), "--json"], 1)
        assert "graph.db" in failed.stderr
        blocker.rmdir()
        assert artifacts(b)[0].is_file()
        torn = json.loads(run("B-missing-graph", ["freshness", "--repo", str(b), "--json"], 1).stdout)
        assert "torn-plane" in torn["stale_reasons"]
        run("B-refresh", ["refresh", "--repo", str(b)])
        verify_graph(b, 2)
        run("B-query", ["graph_query", "hub", "--repo", str(b)])
        run("B-recovered", ["freshness", "--repo", str(b), "--json"])
        assert not list((b / ".code-reality").glob(".graph-build-*"))
        checks.append("graph publication failure -> missing/torn -> refresh recovery; own temp cleanup")

        d = fixture("D-concurrent", "".join(f"def fn_{n:05}(): return 1\n" for n in range(5000)))
        run("D-initial", ["build", "--repo", str(d), "--json"])
        for family in ("graph", "mixed"):
            processes = []
            for number in range(4):
                op = ["build"] if family == "mixed" and number % 2 else ["graph_db", "build"]
                args = [BIN, *op, "--repo", str(d), "--json"]
                processes.append((args, subprocess.Popen(args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)))
            for number, (args, proc) in enumerate(processes):
                stdout, stderr = proc.communicate(timeout=45)
                record(f"D-{family}-{number}", args, proc.returncode, stdout, stderr, None)
                assert proc.returncode == 0 or "data-plane writer busy" in stderr, stderr
            counts = verify_graph(d, 5000)
            checks.append(dict(concurrency=family, writers=4, nodes=5000, derived_counts=counts))

        lock = d / ".code-reality/.writer.lock"
        ready = d / "lock-ready"
        holder = subprocess.Popen(["uv", "run", "--no-project", "python", str(Path(__file__).resolve()), "--hold", str(lock), str(ready)], env=env)
        try:
            deadline = time.monotonic() + 10
            while not ready.exists():
                assert time.monotonic() < deadline, "lock holder readiness timeout"
                time.sleep(0.02)
            busy = run("D-busy", ["graph_db", "build", "--repo", str(d), "--json"], None)
            assert busy.returncode != 0 and "data-plane writer busy" in busy.stderr
            assert records[-1]["elapsed"] < 8
        finally:
            if ready.exists():
                os.kill(int(ready.read_text()), signal.SIGKILL)
            else:
                holder.kill()
            holder.wait(timeout=5)
        run("D-owner-death-recovery", ["graph_db", "build", "--repo", str(d), "--json"])
        verify_graph(d, 5000)
        checks.append("bounded busy <8s and OS releases lock after SIGKILL")

        e = fixture("E-derived-flow", "".join(f"def chain_{n}(): return chain_{n+1}()\n" for n in range(7)) + "def chain_7(): return 1\n")
        (e / "flow-control").touch()
        run("E-initial", ["build", "--repo", str(e), "--json"])
        counts = verify_graph(e, 8)
        assert counts["flows"] > 0 and counts["flow_memberships"] >= 8, counts
        args = [BIN, "graph_db", "build", "--repo", str(e), "--json"]
        processes = [subprocess.Popen(args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for _ in range(4)]
        for number, proc in enumerate(processes):
            stdout, stderr = proc.communicate(timeout=45)
            record(f"E-concurrent-{number}", args, proc.returncode, stdout, stderr, None)
            assert proc.returncode == 0, stderr
        assert verify_graph(e, 8) == counts
        checks.append(dict(derived_nonempty=counts, concurrent_writers=4))
    finally:
        (run_dir / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
        (run_dir / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    print(f"[OK] publication acceptance: {len(checks)} checked scenarios")


if __name__ == "__main__":
    if "--produce" in sys.argv:
        produce()
    elif "--hold" in sys.argv:
        hold()
    else:
        main()
