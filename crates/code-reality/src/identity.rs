//! identity — content-addressed source identity (EP 09-23-source-identity).
//!
//! `identity = sha256("cr-identity-v1\0" + Σ sorted-by-rel
//! "{face}\0{rel}\0{size}\0{content_hash}\0")` — same content ⇒ same
//! identity (touch / rebase / stash round-trips are idempotent); mtime
//! is deliberately NOT part of the identity body (D3), it only gates
//! the per-file hash cache.
//!
//! Hashing happens ONLY at the identity assembly point (D17 lazy):
//! [`crate::engine::walk_sources`] stays pure path+stat and hands out
//! [`SourceRecord`]s without content hashes; path-face walk consumers
//! pay zero hash cost, and legacy slots (no identity keys) never
//! compute an identity at all (D8).
//!
//! Cache policy split (D13): the stamp/build face always recomputes
//! from actual bytes ([`IdentityCachePolicy::Full`]) — the indexed
//! identity can never inherit a poisoned cache. The query face
//! ([`IdentityCachePolicy::WriteBack`]) reuses a per-file hash only on
//! a full `(size, mtime[secs+nanos])` gate hit (D16); the worst a
//! polluted gate escape can do is false-stale (safe direction) or the
//! documented stat-gate residual — never a silent false-fresh.

use crate::js_ts_corpus::normalize_rel;
use crate::language::LanguageFace;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Algorithm id stamped beside every identity value and pinned by the
/// freshness JSON face (`identity_algo`). A stamped meta whose
/// `identity_algo` differs is treated as legacy (not comparable).
pub const IDENTITY_ALGO: &str = "sha256-v1";
/// Identity-concatenation domain separator (v1). Bumping invalidates
/// every stamped identity — a new format is a new algo id.
pub const IDENTITY_HEADER: &[u8] = b"cr-identity-v1\0";
/// Cache payload schema version; a mismatch discards the whole cache.
const IDENTITY_VERSION: u32 = 1;
/// Slot-sibling sidecar carrying per-file content hashes (D4).
pub const IDENTITY_CACHE_NAME: &str = "identity-cache.json";

/// Who may consult the per-file hash cache. Caller-declared (S1 does
/// not self-judge): the stamp face passes [`IdentityCachePolicy::Full`],
/// query faces pass [`IdentityCachePolicy::WriteBack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityCachePolicy {
    /// Stamp/build face (D13): bypass the gate — every file is re-read
    /// and re-hashed from actual bytes; the merged cache is written
    /// back so a rebuild purges poisoned entries in scope.
    Full,
    /// Query face (D11): a full `(size, mtime)` gate hit reuses the
    /// cached per-file hash; a miss reads + hashes, then the merged
    /// cache is written back.
    WriteBack,
    /// Cache-off face (`CODE_REALITY_IDENTITY_CACHE=off`): never read
    /// nor write the cache — "off" means distrust, not read-only reuse.
    ReadOnly,
}

/// `CODE_REALITY_IDENTITY_CACHE=off` degrades any policy to ReadOnly
/// (symmetric with AUTOHEAL=off; resolved inside the identity module).
pub fn resolve_policy(policy: IdentityCachePolicy) -> IdentityCachePolicy {
    if std::env::var("CODE_REALITY_IDENTITY_CACHE").ok().as_deref() == Some("off") {
        IdentityCachePolicy::ReadOnly
    } else {
        policy
    }
}

/// One walked source file. NO content hash here (D17): records are pure
/// path + stat — captured at the walk's existing stat point, zero new
/// syscalls; hashing happens only inside [`compute_identity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    pub face: LanguageFace,
    pub rel: String,
    pub size: u64,
    /// `(secs, nanos)` since the Unix epoch (D16: second-only precision
    /// would let a same-second same-size rewrite slip through the gate).
    /// `(0, 0)` when the walk stat carried no readable mtime — no real
    /// file hashes to that gate, so it can only force re-reads.
    pub mtime: (i64, u32),
}

/// SystemTime → the `(secs, nanos)` cache-gate tuple. A pre-epoch or
/// unreadable mtime maps to `(0, 0)` (gate never hits — safe direction).
pub fn systemtime_tuple(t: Option<std::time::SystemTime>) -> (i64, u32) {
    match t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
        Some(d) => (d.as_secs() as i64, d.subsec_nanos()),
        None => (0, 0),
    }
}

/// The computed source identity: algo id + hex digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub algo: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    size: u64,
    mtime: (i64, u32),
    hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachePayload {
    version: u32,
    algo: String,
    repo: String,
    entries: BTreeMap<String, CacheEntry>,
}

/// Slot-sibling sidecar path for the given slot (`<slot_dir>/identity-cache.json`).
pub fn cache_path_for_slot(slot: &Path) -> PathBuf {
    slot.parent()
        .unwrap_or_else(|| Path::new(""))
        .join(IDENTITY_CACHE_NAME)
}

/// Per-file content-hash cache (D4: slot sibling JSON; D16 hardening:
/// repo-bound payload, `(secs, nanos)` gate; crash-only — anything
/// suspicious is discarded wholesale, never served with a warning).
pub struct IdentityCache {
    path: PathBuf,
    repo: String,
    entries: BTreeMap<String, CacheEntry>,
    dirty: bool,
    disabled: bool,
}

impl IdentityCache {
    /// Load + validate the sidecar. Any inconsistency (parse failure,
    /// version/algo drift, repo mismatch — D16) discards the whole
    /// payload with a stderr WARN; the identity is then recomputed from
    /// actual bytes (crash-only for the cache, TC-10).
    pub fn load(path: PathBuf, repo: &Path) -> IdentityCache {
        let (cache, discard) = Self::load_checked(path, repo);
        if let Some(reason) = discard {
            eprint!("[WARN] identity cache 棄置（{reason}）——全量重算\n");
        }
        cache
    }

    /// [`IdentityCache::load`] minus the stderr face; the discard reason
    /// (if any) is returned for callers that surface it themselves.
    pub fn load_checked(path: PathBuf, repo: &Path) -> (IdentityCache, Option<String>) {
        let repo_s = crate::engine::resolve_repo(repo).to_string_lossy().into_owned();
        let empty = |path: PathBuf| IdentityCache {
            path,
            repo: repo_s.clone(),
            entries: BTreeMap::new(),
            dirty: false,
            disabled: false,
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (empty(path), None)
            }
            Err(e) => {
                let reason = format!("讀取失敗：{e}");
                return (empty(path), Some(reason));
            }
        };
        let discard = |path: PathBuf, reason: String| (empty(path), Some(reason));
        let payload: CachePayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => return discard(path, format!("解析失敗：{e}")),
        };
        if payload.version != IDENTITY_VERSION {
            return discard(path, format!("版本不符（{}）", payload.version));
        }
        if payload.algo != IDENTITY_ALGO {
            return discard(path, format!("演算法不符（{}）", payload.algo));
        }
        if payload.repo != repo_s {
            return discard(path, format!("repo 不符（{}）", payload.repo));
        }
        (
            IdentityCache {
                path,
                repo: repo_s,
                entries: payload.entries,
                dirty: false,
                disabled: false,
            },
            None,
        )
    }

    /// The cache-off face: a cache that never touches disk (ReadOnly —
    /// "off" means distrust, so it does not even read).
    pub fn disabled() -> IdentityCache {
        IdentityCache {
            path: PathBuf::new(),
            repo: String::new(),
            entries: BTreeMap::new(),
            dirty: false,
            disabled: true,
        }
    }

    /// Stat-gate lookup (D16): a hit requires size AND mtime (secs AND
    /// nanos) to match exactly. Only consulted under WriteBack — Full
    /// bypasses the gate by contract, ReadOnly never reads.
    fn hit(&self, key: &str, size: u64, mtime: (i64, u32)) -> Option<String> {
        if self.disabled {
            return None;
        }
        let e = self.entries.get(key)?;
        (e.size == size && e.mtime == mtime).then(|| e.hash.clone())
    }

    fn record(&mut self, key: String, size: u64, mtime: (i64, u32), hash: String) {
        if self.disabled {
            return;
        }
        self.entries.insert(
            key,
            CacheEntry {
                size,
                mtime,
                hash,
            },
        );
        self.dirty = true;
    }

    /// Merged atomic write (tmp+rename): entries still belonging to the
    /// current walk set are kept, same-key values are already
    /// overwritten in place, out-of-set entries are pruned. Write
    /// failure WARNs without blocking (read-only slot dir stays
    /// compatible — t20). Skipped when nothing changed: no fresh hash
    /// was computed AND no entry left the walk set.
    fn store(&mut self, keep: &BTreeSet<String>) {
        if self.disabled {
            return;
        }
        let needs_prune = self.entries.keys().any(|k| !keep.contains(k));
        if !self.dirty && !needs_prune {
            return;
        }
        self.entries.retain(|k, _| keep.contains(k));
        let payload = CachePayload {
            version: IDENTITY_VERSION,
            algo: IDENTITY_ALGO.to_string(),
            repo: self.repo.clone(),
            entries: self.entries.clone(),
        };
        let Ok(text) = serde_json::to_string_pretty(&payload) else {
            return;
        };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_file_name(format!(
            ".{}-tmp-{}",
            IDENTITY_CACHE_NAME,
            std::process::id()
        ));
        let written = std::fs::write(&tmp, &text)
            .and_then(|_| std::fs::rename(&tmp, &self.path));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            eprint!("[WARN] identity cache 寫入失敗（{e}）——略過（不影響身分計算）\n");
        }
        self.dirty = false;
    }
}

/// Compute the content-addressed source identity over `records`
/// (the full walk's merged record map, rel-sorted by BTreeMap order),
/// scoped to `faces`, with the caller-declared cache [`IdentityCachePolicy`].
///
/// Fail-loud (D15): a per-file read failure is `Err`, never a silent
/// skip — a verdict face has no safe fallback answer.
pub fn compute_identity(
    root: &Path,
    records: &BTreeMap<String, SourceRecord>,
    faces: &BTreeSet<LanguageFace>,
    policy: IdentityCachePolicy,
    cache: &mut IdentityCache,
) -> Result<Identity, String> {
    let mut h = Sha256::new();
    h.update(IDENTITY_HEADER);
    let mut keep: BTreeSet<String> = BTreeSet::new();
    for (rel, rec) in records {
        // BTreeMap order = rel-sorted: byte-determinism (D2). Face
        // scoping first — out-of-scope records pay no read and no gate.
        if !faces.contains(&rec.face) {
            continue;
        }
        // normalize_rel applies ONLY here and at the cache key (the
        // walk's raw rel stays the comparison face for doc-set/
        // fingerprint equality — D17 normalization boundary).
        let key = normalize_rel(rel);
        keep.insert(key.clone());
        let chash = content_hash_of(&root.join(&rec.rel), &key, rec.size, rec.mtime, policy, cache)?;
        h.update(format!(
            "{}\0{}\0{}\0{}\0",
            rec.face.meta_name(),
            key,
            rec.size,
            chash
        ));
    }
    if !matches!(policy, IdentityCachePolicy::ReadOnly) {
        cache.store(&keep);
    }
    Ok(Identity {
        algo: IDENTITY_ALGO,
        value: hex(&h.finalize()),
    })
}

/// Streaming per-file sha256 (multi-GB files never load whole — judge
/// R16). Read failure → Err (D15). Only WriteBack consults the stat
/// gate; Full (stamp — D13) and ReadOnly (env-off) always re-read.
fn content_hash_of(
    path: &Path,
    key: &str,
    size: u64,
    mtime: (i64, u32),
    policy: IdentityCachePolicy,
    cache: &mut IdentityCache,
) -> Result<String, String> {
    if let IdentityCachePolicy::WriteBack = policy {
        if let Some(hash) = cache.hit(key, size, mtime) {
            return Ok(hash);
        }
    }
    let mut f =
        std::fs::File::open(path).map_err(|e| format!("讀取 {} 失敗：{e}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).map_err(|e| format!("讀取 {} 失敗：{e}", path.display()))?;
    let hash = hex(&hasher.finalize());
    if !matches!(policy, IdentityCachePolicy::ReadOnly) {
        cache.record(key.to_string(), size, mtime, hash.clone());
    }
    Ok(hash)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tree(root: &Path, files: &[(&str, &str)]) {
        for (rel, content) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
    }

    fn walk_records(root: &Path) -> BTreeMap<String, SourceRecord> {
        crate::engine::walk_sources(root).unwrap().records()
    }

    fn py(rel: &str, size: u64, mtime: (i64, u32)) -> (String, SourceRecord) {
        (
            rel.to_string(),
            SourceRecord {
                face: LanguageFace::Python,
                rel: rel.to_string(),
                size,
                mtime,
            },
        )
    }

    fn faces(f: &[LanguageFace]) -> BTreeSet<LanguageFace> {
        f.iter().copied().collect()
    }

    /// TC-1a: equal-content trees under different roots → equal identity.
    #[test]
    fn identity_equal_across_equivalent_trees() {
        let t1 = tempfile::tempdir().unwrap();
        let t2 = tempfile::tempdir().unwrap();
        let files = [
            ("a.py", "x = 1\n"),
            ("src/deep/b.py", "def f():\n    pass\n"),
            ("src/g.rs", "fn g() {}\n"),
        ];
        write_tree(t1.path(), &files);
        write_tree(t2.path(), &files);
        let all = faces(&[
            LanguageFace::Python,
            LanguageFace::Rust,
            LanguageFace::JavaScript,
            LanguageFace::TypeScript,
        ]);
        let i1 = compute_identity(
            t1.path(),
            &walk_records(t1.path()),
            &all,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        let i2 = compute_identity(
            t2.path(),
            &walk_records(t2.path()),
            &all,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        assert_eq!(i1, i2);
        assert_eq!(i1.algo, "sha256-v1");
    }

    /// TC-1b: the entry format pin — header + face field, byte-exact.
    /// The expected digest is assembled here from the frozen spec
    /// (`"cr-identity-v1\0" + "{face}\0{rel}\0{size}\0{content_hash}\0"`),
    /// with sha2 as the external primitive oracle.
    #[test]
    fn identity_entry_format_pin() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x\n")]);
        let mut rec = BTreeMap::new();
        let (k, v) = py("a.py", 2, (1, 0));
        rec.insert(k, v);
        let f = faces(&[LanguageFace::Python]);
        let id = compute_identity(
            t.path(),
            &rec,
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        let content = hex(&Sha256::digest(b"x\n"));
        let mut h = Sha256::new();
        h.update(IDENTITY_HEADER);
        h.update(format!("python\0a.py\0{}\0{content}\0", 2u64));
        let expected = hex(&h.finalize());
        assert_eq!(id.value, expected, "header + face field + entry format");
    }

    /// TC-2: one flipped byte → different identity.
    #[test]
    fn identity_is_content_sensitive() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n")]);
        let f = faces(&[LanguageFace::Python]);
        let before = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        std::fs::write(t.path().join("a.py"), "x = 2\n").unwrap();
        let after = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        assert_ne!(before, after);
    }

    /// TC-3: touch (mtime moves, content equal) → same identity (D3).
    #[test]
    fn identity_is_touch_idempotent() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n"), ("b.py", "y = 2\n")]);
        let f = faces(&[LanguageFace::Python]);
        let before = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        for rel in ["a.py", "b.py"] {
            std::fs::File::options()
                .write(true)
                .open(t.path().join(rel))
                .unwrap()
                .set_modified(later)
                .unwrap();
        }
        let after = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        assert_eq!(before, after, "mtime must not enter the identity body");
    }

    /// TC-6: the cache path and the full-recompute path agree —
    /// cold, warm-hit (zero changes), and single-file-change all equal
    /// the ReadOnly no-cache answer, entry format included (face field).
    #[test]
    fn cache_path_equals_full_recompute() {
        let t = tempfile::tempdir().unwrap();
        write_tree(
            t.path(),
            &[("a.py", "x = 1\n"), ("src/b.py", "y = 2\n"), ("c.py", "z\n")],
        );
        let f = faces(&[LanguageFace::Python]);
        let full = |root: &Path| {
            compute_identity(
                root,
                &walk_records(root),
                &f,
                IdentityCachePolicy::ReadOnly,
                &mut IdentityCache::disabled(),
            )
            .unwrap()
        };
        let i0 = full(t.path());

        // cold WriteBack: all misses, real reads
        let cache_path = t.path().join("identity-cache.json");
        let mut cache = IdentityCache::load(cache_path.clone(), t.path());
        let cold = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut cache,
        )
        .unwrap();
        assert_eq!(cold, i0, "cold cache path ≡ full recompute");
        assert!(cache_path.exists(), "WriteBack persists the sidecar");

        // warm hit: zero changes, every file gate-hits
        let mut warm = IdentityCache::load(cache_path.clone(), t.path());
        let warm_id = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut warm,
        )
        .unwrap();
        assert_eq!(warm_id, i0, "warm-hit path ≡ full recompute");

        // single-file change: gate misses on that file only, still equal
        std::fs::write(t.path().join("src/b.py"), "y = 22\n").unwrap();
        let mut warm2 = IdentityCache::load(cache_path.clone(), t.path());
        let delta = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut warm2,
        )
        .unwrap();
        assert_eq!(delta, full(t.path()), "changed-file path ≡ full recompute");
    }

    /// TC-6+D12: pruning — a file that leaves the walk set has its cache
    /// entry pruned on the next merged write.
    #[test]
    fn cache_store_prunes_out_of_set_entries() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n"), ("gone.py", "bye\n")]);
        let f = faces(&[LanguageFace::Python]);
        let cache_path = t.path().join("identity-cache.json");
        let mut cache = IdentityCache::load(cache_path.clone(), t.path());
        compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut cache,
        )
        .unwrap();
        let payload: CachePayload =
            serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
        assert_eq!(payload.entries.len(), 2);

        std::fs::remove_file(t.path().join("gone.py")).unwrap();
        let mut cache2 = IdentityCache::load(cache_path.clone(), t.path());
        compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut cache2,
        )
        .unwrap();
        let payload: CachePayload =
            serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
        assert_eq!(
            payload.entries.len(),
            1,
            "out-of-set entry pruned by the merged write"
        );
        assert!(payload.entries.contains_key("a.py"));
    }

    /// TC-10: crash-only cache — corrupt bytes, schema downgrade, and a
    /// transplanted repo binding all discard the payload; the identity
    /// equals the clean no-cache answer.
    #[test]
    fn corrupt_cache_discards_and_recomputes() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n")]);
        let f = faces(&[LanguageFace::Python]);
        let clean = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        let cache_path = t.path().join("identity-cache.json");

        // warm once
        let mut cache = IdentityCache::load(cache_path.clone(), t.path());
        compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::WriteBack,
            &mut cache,
        )
        .unwrap();

        let cases: Vec<(&str, String)> = vec![
            ("corrupt bytes", "{not json".to_string()),
            (
                "version downgrade",
                r#"{"version": 0, "algo": "sha256-v1", "repo": "x", "entries": {}}"#.to_string(),
            ),
            (
                "algo drift",
                r#"{"version": 1, "algo": "sha256-v0", "repo": "x", "entries": {}}"#.to_string(),
            ),
            (
                "repo mismatch",
                r#"{"version": 1, "algo": "sha256-v1", "repo": "/elsewhere", "entries": {}}"#
                    .to_string(),
            ),
        ];
        for (label, payload) in cases {
            std::fs::write(&cache_path, &payload).unwrap();
            let (mut cache, discard) = IdentityCache::load_checked(cache_path.clone(), t.path());
            assert!(discard.is_some(), "{label}: payload must be discarded");
            let id = compute_identity(
                t.path(),
                &walk_records(t.path()),
                &f,
                IdentityCachePolicy::WriteBack,
                &mut cache,
            )
            .unwrap();
            assert_eq!(id, clean, "{label}: identity still correct after discard");
        }
    }

    /// TC-14 (D16): same-second same-size rewrite — the gate must MISS
    /// (nanos differ) so the content swap is detected. A seconds-only
    /// gate would reuse the stale hash and answer equal here.
    #[test]
    fn same_second_same_size_rewrite_is_detected() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "aaaa")]);
        let f = faces(&[LanguageFace::Python]);
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mt = |nanos: u32| {
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(base, nanos)
        };
        std::fs::File::options()
            .write(true)
            .open(t.path().join("a.py"))
            .unwrap()
            .set_modified(mt(100_000_000))
            .unwrap();
        let cache_path = t.path().join("identity-cache.json");
        let id = |policy| {
            let mut cache = IdentityCache::load(cache_path.clone(), t.path());
            compute_identity(
                t.path(),
                &walk_records(t.path()),
                &f,
                policy,
                &mut cache,
            )
            .unwrap()
        };
        let before = id(IdentityCachePolicy::WriteBack);

        // rewrite same size, same SECOND, different nanos
        std::fs::write(t.path().join("a.py"), "bbbb").unwrap();
        std::fs::File::options()
            .write(true)
            .open(t.path().join("a.py"))
            .unwrap()
            .set_modified(mt(200_000_000))
            .unwrap();
        let after = id(IdentityCachePolicy::WriteBack);
        assert_ne!(before, after, "same-second swap must not slip the gate");
        assert_eq!(
            after,
            id(IdentityCachePolicy::ReadOnly),
            "cache-path answer ≡ full recompute"
        );
    }

    /// D15: a per-file read failure is Err — never a silent skip posing
    /// as an identity.
    #[test]
    fn read_failure_fails_loud() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n"), ("locked.py", "s")]);
        std::fs::set_permissions(
            t.path().join("locked.py"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let f = faces(&[LanguageFace::Python]);
        let r = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &f,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        );
        std::fs::set_permissions(
            t.path().join("locked.py"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(r.is_err(), "read failure must surface: {:?}", r.ok());
    }

    /// TC-12 (D6 pin): a symlinked corpus file is invisible to the
    /// identity — `walk_sources` does not follow symlinks (recorded
    /// boundary, never silently drifted). The producer side DOES follow
    /// them — the known asymmetry stays out of scope here.
    #[test]
    fn symlinked_source_is_invisible_to_identity() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n")]);
        let f = faces(&[LanguageFace::Python]);
        let id = |root: &Path| {
            compute_identity(
                root,
                &walk_records(root),
                &f,
                IdentityCachePolicy::ReadOnly,
                &mut IdentityCache::disabled(),
            )
            .unwrap()
        };
        let before = id(t.path());
        std::os::unix::fs::symlink(t.path().join("a.py"), t.path().join("linked.py")).unwrap();
        let w = crate::engine::walk_sources(t.path()).unwrap();
        assert!(
            !w.py.contains_key("linked.py"),
            "walk must skip symlinks (D6 boundary)"
        );
        assert_eq!(before, id(t.path()), "symlink must not enter identity");
    }

    /// Face scoping: records outside `faces` contribute nothing (an
    /// explicit python-only identity ignores rust files entirely).
    #[test]
    fn face_scoping_excludes_other_faces() {
        let t = tempfile::tempdir().unwrap();
        write_tree(t.path(), &[("a.py", "x = 1\n"), ("b.rs", "fn f() {}\n")]);
        let py_only = faces(&[LanguageFace::Python]);
        let before = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &py_only,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        std::fs::write(t.path().join("b.rs"), "fn g() {}\n").unwrap();
        let after = compute_identity(
            t.path(),
            &walk_records(t.path()),
            &py_only,
            IdentityCachePolicy::ReadOnly,
            &mut IdentityCache::disabled(),
        )
        .unwrap();
        assert_eq!(before, after, "out-of-scope face must not enter identity");
    }
}
