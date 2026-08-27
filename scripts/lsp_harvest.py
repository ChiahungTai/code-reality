"""LSP-harvest adapter (productized; POC-validated 2026-08-26, hardened
2026-08-27 after the mosaic dogfood) -> code-reality cache three-table db.

Contract: rows are the internal triple (defs / occurrences / fn-spans) —
SCIP is just rust-analyzer's serialization form; a different producer
feeds the same pipeline. Symbol strings are synthesized in a shape the
engine parsers accept: "lsp python <rel_path> L<line> <name>()." — the
L<line> middle segment disambiguates same-file same-name defs (mosaic
dogfood bug 1: four `execute()` in daily.py collapsed onto the first
under the symbol UNIQUE constraint). fn_tail_name() reads the trailing
word, so the legacy no-line shape keeps parsing too.

Hardening (dogfood bug 2): references are resolved per def from the
symbol's own selectionRange position (not a hardcoded len("def ")
offset), over ALL repo files (no [:200] truncation), for ALL defs by
default (--sample N restores the POC smoke shape).

Usage: uv run python scripts/lsp_harvest.py --repo <repo-root> [--sample N]
"""
import argparse, json, os, sqlite3, subprocess, sys, time

SIDEAR_HOME = os.path.expanduser("~/.mosaic/code-reality/scip")

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

def name_position(s, line_text):
    """The symbol's own name token, as (line, character):
    1. selectionRange.start (DocumentSymbol — pyright omits it for the
       SymbolInformation shape);
    2. the name's index in the def line (covers SymbolInformation, async
       def, decorated defs — a bare range.start points at the `def`
       keyword and references() resolves nothing);
    3. len("def ") as the legacy last resort."""
    sr = s.get("selectionRange")
    if isinstance(sr, dict):
        return sr["start"]["line"], sr["start"]["character"]
    r = sym_range(s)
    if r is not None and line_text is not None:
        ch = line_text.find(s["name"])
        if ch >= 0:
            return r["start"]["line"], ch
    return r["start"]["line"], len("def ")

def flat_symbols(syms, out):
    for s in syms:
        if isinstance(s, dict):
            out.append(s)
            flat_symbols(s.get("children", []), out)
    return out

def main():
    ap = argparse.ArgumentParser(description="LSP-harvest -> code-reality cache")
    ap.add_argument("--repo", required=True, help="repo root to harvest")
    ap.add_argument("--sample", type=int, default=None,
                    help="POC smoke shape: references for the first N defs only "
                         "(alphabetical); default = ALL defs")
    args = ap.parse_args()
    repo = os.path.abspath(os.path.expanduser(args.repo))
    slot = os.path.join(SIDEAR_HOME, os.path.basename(repo.rstrip("/")))

    t0 = time.time()
    lsp = Lsp(repo)
    files = py_files(repo)
    print(f"[..] {len(files)} py files")
    # defs: (rel, line, char, name) — line/char are the NAME token position
    defs = []
    pos_misses = 0    # name not on the def line (decorated/nested shapes)
    for f in files:
        uri = "file://" + f
        try:
            text = open(f, encoding="utf-8").read()
            lsp.notify("textDocument/didOpen", {"textDocument": {
                "uri": uri, "languageId": "python", "version": 1, "text": text}})
            syms = lsp.req("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        except (RuntimeError, OSError):
            continue
        if not syms:
            continue
        src_lines = text.splitlines()
        for s in flat_symbols(syms, []):
            if s.get("kind") in (3, 6, 12):
                r = sym_range(s)
                if r is not None:
                    dl = r["start"]["line"]
                    line_text = src_lines[dl] if dl < len(src_lines) else None
                    if (line_text is not None and line_text.find(s["name"]) < 0
                            and not isinstance(s.get("selectionRange"), dict)):
                        pos_misses += 1
                    pl, pc = name_position(s, line_text)
                    defs.append((os.path.relpath(f, repo),
                                 pl + 1, pc, s["name"],
                                 dl + 1))
    print(f"[OK] defs harvested: {len(defs)} ({time.time()-t0:.0f}s)")
    if pos_misses:
        print(f"[WARN] {pos_misses} defs: name not on the def line (decorated?) — those fall to the def-offset fallback")

    def sym_of(rel, line, name):
        # L<line> middle segment: disambiguates same-file same-name defs;
        # fn_tail_name() reads the trailing word so both shapes parse
        return f"lsp python {rel} L{line} {name}()."

    # references per def (each def's own name-token position — a hardcoded
    # len("def ") offset misses async/decorated/nested shapes)
    targets = defs
    if args.sample is not None:
        targets = sorted(defs)[: args.sample]
    refs = []          # (def_rel, def_line, def_name, [(rel, line)...])
    req_failures = 0   # a failed request writes zero-refs rows (false negatives); counted loudly, aborted when rampant
    t1 = time.time()
    for i, (rel, line, ch, name, _rl) in enumerate(targets):
        f = os.path.join(repo, rel)
        try:
            locs = lsp.req("textDocument/references", {
                "textDocument": {"uri": "file://" + f},
                "position": {"line": line - 1, "character": ch},
                "context": {"includeDeclaration": False},
            }) or []
        except RuntimeError:
            req_failures += 1
            locs = []
        got = []
        for loc in locs:
            r = os.path.relpath(loc["uri"][7:], repo)
            got.append((r, loc["range"]["start"]["line"] + 1))
        refs.append((rel, line, name, got))
        if (i + 1) % 500 == 0:
            print(f"[..] references {i+1}/{len(targets)} ({time.time()-t1:.0f}s)")
    n_refs = sum(len(g) for *_, g in refs)
    print(f"[OK] references harvested: {len(refs)} defs / {n_refs} sites ({time.time()-t1:.0f}s)")
    if req_failures:
        frac = req_failures / max(len(targets), 1)
        if frac > 0.5:
            raise SystemExit(
                f"[FAIL] references 失敗率 {frac:.0%}（{req_failures}/{len(targets)}）——"
                "LSP 不穩定，中止而不寫壞 cache（重跑本腳本）")
        print(f"[WARN] references failed for {req_failures}/{len(targets)} defs（零 refs 寫入）")

    # write cache three-table db
    os.makedirs(slot, exist_ok=True)
    index_path = os.path.join(slot, "index.scip")
    db_path = os.path.join(slot, "index.scip.db")
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
    for rel, line, ch, name, def_line in defs:
        seq += 1
        sym = sym_of(rel, line, name)
        conn.execute("INSERT INTO occurrences VALUES (?,?,?,?,1)",
                     (seq, sym, rel, def_line))
        conn.execute("INSERT OR REPLACE INTO symbol_tails VALUES (?,?,?)",
                     (sym, name + "().", name))
    for rel, line, name, sites in refs:
        sym = sym_of(rel, line, name)
        for r, ln in sites:
            seq += 1
            conn.execute("INSERT INTO occurrences VALUES (?,?,?,?,0)",
                         (seq, sym, r, ln))
    head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    conn.executemany("INSERT OR REPLACE INTO meta VALUES (?,?)", [
        ("schema", "1"), ("head", head),
        ("producer", "lsp-harvest(pyright-langserver)"),
    ])
    conn.commit()
    conn.close()
    # staleness contract (cache.rs stale_reason): meta sidecar carries the
    # stamped head; the index placeholder exists (face routing); the db is
    # touched LAST so db mtime >= index mtime
    with open(index_path, "w") as fh:
        fh.write(f"lsp-harvest placeholder (producer=pyright-langserver, head={head[:12]})\n")
    meta_path = os.path.join(slot, "index.scip.meta.json")
    with open(meta_path, "w") as fh:
        json.dump({"head": head, "producer": "pyright-langserver"}, fh)
    # touch order: index -> meta(stamp) -> db — every layer newer than the
    # artifact it certifies (stamp-vs-index and db-vs-index guards)
    os.utime(index_path)
    os.utime(meta_path, None)
    os.utime(db_path, None)
    print(f"[OK] cache db: {db_path} ({seq} rows)")
    if args.sample is not None:
        with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "lsp_answers.json"), "w") as fh:
            json.dump({f"{rel}::{name}": sites for rel, _line, name, sites in refs}, fh, indent=1)
        print("[OK] answer key written (sample mode)")

if __name__ == "__main__":
    main()
