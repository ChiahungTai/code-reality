"""S5-P2: LSP-harvest adapter (productized home; POC-validated 2026-08-26 (pyright-langserver) -> code-reality
cache three-table db (ep-v1plus-graph-engine.md S5; P1 scip-python is
dead: PyPI 404 + archived git layout).

Contract per EP: rows are the internal triple (defs / occurrences /
fn-spans) — SCIP is just rust-analyzer's serialization form; a different
producer feeds the same pipeline. Symbol strings are synthesized in a
shape engine parsers accept: "lsp python <rel_path> <name>().".

Pass-bar (pinned at POC design): N=20 sampled symbols, engine refs ==
LSP references sets (100%), +5 hand checks against source.
"""
import json, os, sqlite3, subprocess, sys, time

REPO = os.path.expanduser("~/Github/ai-rules")
SLOT = os.path.expanduser("~/.mosaic/code-reality/scip/ai-rules")
SAMPLE = 20

class Lsp:
    def __init__(self, root):
        self.p = subprocess.Popen(
            ["pyright-langserver", "--stdio"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, bufsize=0)
        self.root = root
        self.id = 0
        self.req("initialize", {
            "processId": os.getpid(),
            "rootUri": "file://" + root,
            "capabilities": {},
        })
        self.notify("initialized", {})

    def _send(self, obj):
        body = json.dumps(obj).encode()
        self.p.stdin.write(
            f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.p.stdin.flush()

    def _recv(self):
        headers = {}
        while True:
            line = self.p.stdout.readline().decode().strip()
            if not line:
                break
            k, v = line.split(":", 1)
            headers[k.lower()] = v.strip()
        n = int(headers["content-length"])
        body = b""
        while len(body) < n:
            chunk = self.p.stdout.read(n - len(body))
            if not chunk:
                raise RuntimeError("LSP stream closed")
            body += chunk
        return json.loads(body)

    def req(self, method, params):
        self.id += 1
        rid = self.id
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        while True:
            msg = self._recv()
            if msg.get("id") == rid:
                if "error" in msg:
                    raise RuntimeError(msg["error"])
                return msg.get("result")

    def notify(self, method, params):
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

def py_files(root):
    out = []
    for dp, dns, fns in os.walk(root):
        dns[:] = [d for d in dns if d not in (".git", ".venv", "node_modules", "__pycache__")]
        for f in fns:
            if f.endswith(".py"):
                out.append(os.path.join(dp, f))
    return sorted(out)

def sym_range(s):
    """Both LSP shapes: DocumentSymbol (s.range + children) and
    SymbolInformation (s.location.range, flat)."""
    if isinstance(s.get("range"), dict):
        return s["range"]
    loc = s.get("location")
    if isinstance(loc, dict) and isinstance(loc.get("range"), dict):
        return loc["range"]
    return None

def flat_symbols(syms, out):
    for s in syms:
        if isinstance(s, dict):
            out.append(s)
            flat_symbols(s.get("children", []), out)
    return out

def main():
    lsp = Lsp(REPO)
    files = py_files(REPO)[:200]
    print(f"[..] {len(files)} py files")
    defs = []          # (rel, line, name, kind)
    for f in files:
        uri = "file://" + f
        try:
            text = open(f, encoding="utf-8").read()
            lsp.notify("textDocument/didOpen", {"textDocument": {
                "uri": uri, "languageId": "python", "version": 1, "text": text}})
            syms = lsp.req("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        except RuntimeError:
            continue
        if not syms:
            continue
        for s in flat_symbols(syms, []):
            if s.get("kind") in (3, 6, 12):
                r = sym_range(s)
                if r is not None:
                    defs.append((os.path.relpath(f, REPO), r["start"]["line"] + 1, s["name"]))
    print(f"[OK] defs harvested: {len(defs)}")

    # sample references
    by_name = {}
    for rel, line, name in defs:
        by_name.setdefault((rel, name), line)
    sample = sorted(by_name.items())[:SAMPLE]
    refs = {}           # (rel,name) -> [(rel,line)]
    for (rel, name), line in sample:
        f = os.path.join(REPO, rel)
        try:
            locs = lsp.req("textDocument/references", {
                "textDocument": {"uri": "file://" + f},
                "position": {"line": line - 1, "character": len("def ")},
                "context": {"includeDeclaration": True},
            }) or []
        except RuntimeError:
            locs = []
        got = []
        for loc in locs:
            r = os.path.relpath(loc["uri"][7:], REPO)
            got.append((r, loc["range"]["start"]["line"] + 1))
        refs[(rel, name)] = got
    n_refs = sum(len(v) for v in refs.values())
    print(f"[OK] references harvested: {SAMPLE} symbols / {n_refs} sites")

    # write cache three-table db
    os.makedirs(SLOT, exist_ok=True)
    index_path = os.path.join(SLOT, "index.scip")
    db_path = os.path.join(SLOT, "index.scip.db")
    if os.path.exists(db_path):
        os.remove(db_path)
    conn = sqlite3.connect(db_path)
    conn.executescript("""
      CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
      CREATE TABLE occurrences (
        seq INTEGER PRIMARY KEY, symbol TEXT NOT NULL, rel_path TEXT NOT NULL,
        line INTEGER NOT NULL, is_def INTEGER NOT NULL);
      CREATE TABLE symbol_tails (symbol TEXT PRIMARY KEY, tail TEXT NOT NULL, method TEXT);
      CREATE INDEX idx_symbol_tails_method ON symbol_tails(method);
    """)
    seq = 0
    def sym_of(rel, name):
        return f"lsp python {rel} {name}()."
    for rel, line, name in defs:
        seq += 1
        conn.execute("INSERT INTO occurrences VALUES (?,?,?,?,1)",
                     (seq, sym_of(rel, name), rel, line))
        conn.execute("INSERT OR REPLACE INTO symbol_tails VALUES (?,?,?)",
                     (sym_of(rel, name), name + "().", name))
    for (rel, name), sites in refs.items():
        for r, line in sites:
            seq += 1
            conn.execute("INSERT INTO occurrences VALUES (?,?,?,?,0)",
                         (seq, sym_of(rel, name), r, line))
    head = subprocess.run(["git", "-C", REPO, "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    conn.executemany("INSERT OR REPLACE INTO meta VALUES (?,?)", [
        ("schema", "1"), ("head", head),
        ("producer", "lsp-harvest-poc(pyright-langserver)"),
    ])
    conn.commit()
    conn.close()
    # staleness contract (cache.rs stale_reason): meta sidecar carries the
    # stamped head; the index placeholder exists (face routing); the db is
    # touched LAST so db mtime >= index mtime
    with open(index_path, "w") as fh:
        fh.write(f"lsp-harvest placeholder (producer=pyright-langserver, head={head[:12]})\n")
    meta_path = os.path.join(SLOT, "index.scip.meta.json")
    with open(meta_path, "w") as fh:
        json.dump({"head": head, "producer": "pyright-langserver"}, fh)
    # touch order: index -> meta(stamp) -> db — every layer newer than
    # the artifact it certifies (stamp-vs-index and db-vs-index guards)
    os.utime(index_path)
    os.utime(meta_path, None)
    os.utime(db_path, None)
    print(f"[OK] cache db: {db_path} ({seq} rows)")
    # dump LSP answer key for the engine comparison step
    with open(os.path.join(os.path.dirname(__file__), "lsp_answers.json"), "w") as fh:
        json.dump({f"{rel}::{name}": v for (rel, name), v in refs.items()}, fh, indent=1)
    print("[OK] answer key written")

main()
