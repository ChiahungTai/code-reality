//! Shared conventions — the frozen `code_reality/common.py` contract:
//! EDGE_KINDS, anchor pattern, repo relativization, CRG db read-only
//! connection (WAL semantics), mtime tear guard, ordered `_meta` block,
//! plus the D1 JSON serializer and the time foundation (D2).
//!
//! Byte-parity scope: stdout bytes + exit codes. Assertion texts that
//! Python raises as uncaught errors (traceback, exit 1) surface here as
//! `Err(String)` messages — the caller maps them to a crash ToolOutput
//! (empty stdout, exit 1) with the text on stderr (best-effort face).

use rusqlite::Connection;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Structural edge kinds (`common.py:18`) — the "no structural change"
/// boundary in transition and the projection filter in snapshot/graph_csv.
pub const EDGE_KINDS: [&str; 3] = ["IMPORTS_FROM", "CALLS", "INHERITS"];

/// Anchor line → literal-ish line regex (`common.py:21-37`):
/// `^[ \t]*` + fully escaped trimmed line + `[ \t]*$`. `regex::escape` is
/// used for semantic parity with Python `re.escape` (the escape *sets*
/// differ, but both produce a pattern matching exactly the literal line —
/// matching behavior is the contract; pattern-string bytes are emitted
/// only into R5 sidecar files, outside the R4 stdout gate).
pub fn anchor_pattern(line: &str) -> String {
    format!("^[ \\t]*{}[ \\t]*$", regex::escape(line.trim()))
}

/// Absolute path → repo-relative; outside repo → None (`common.py:40-45`).
/// Only `repo_root` is resolved — the path itself keeps symlinks
/// (frozen behavior; `Path::relative_to` needs no component symlink
/// canonicalization when the root is already resolved).
pub fn repo_relative(path: &str, repo_root: &Path) -> Option<String> {
    let root = resolve(repo_root);
    Path::new(path)
        .strip_prefix(&root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// CRG graph.db conventional path (`common.py:48-50`).
pub fn graph_db_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".code-review-graph").join("graph.db")
}

/// Read-only connection — `immutable=1` without a WAL file, `mode=ro`
/// fallback with one (`common.py:53-74`). The mode=ro failure message is
/// the frozen guidance text; an immutable open failure propagates as the
/// raw sqlite error (Python leaves it uncaught → exit 1).
pub fn connect_ro(db_path: &Path) -> Result<Connection, String> {
    let name = db_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let wal = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{}-wal", name));
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_URI
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if !wal.exists() {
        return Connection::open_with_flags(
            format!("file:{}?immutable=1", db_path.display()),
            flags,
        )
        .map_err(|e| format!("{} 開啟失敗（immutable）：{}", name, e));
    }
    Connection::open_with_flags(format!("file:{}?mode=ro", db_path.display()), flags).map_err(
        |_| {
            format!(
                "{} 有 {} 但 mode=ro 開啟失敗（writer crash 後 hot-WAL-無-shm）——先 \
                 `uvx code-review-graph status` 或 build 後重跑",
                name,
                wal.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            )
        },
    )
}

/// `st_mtime_ns` (`common.py:77-78`). Python `.stat()` on a missing file
/// raises (exit 1); here `Err` carries the io error for the same mapping.
pub fn db_mtime_ns(db_path: &Path) -> Result<i64, String> {
    use std::os::unix::fs::MetadataExt;
    let md = std::fs::metadata(db_path)
        .map_err(|e| format!("stat 失敗 {}: {}", db_path.display(), e))?;
    // st_mtime_ns = seconds + subsecond (`mtime()` alone is whole seconds)
    Ok(md.mtime() * 1_000_000_000 + md.mtime_nsec())
}

/// Tear guard (`common.py:81-90`): main file rewritten mid-read → crash.
pub fn assert_db_unchanged(db_path: &Path, mtime_before: i64) -> Result<(), String> {
    let now = db_mtime_ns(db_path)?;
    if now != mtime_before {
        return Err(format!(
            "{} 在讀取期間被改寫（build/update 併發）——快照可能撕裂，重跑本工具",
            db_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
    }
    Ok(())
}

/// `git rev-parse HEAD` with `check=True` semantics — failure is a crash
/// (exit 1), unlike the tolerant R2 `engine::git_head` ([SRC] face).
/// Shared by `make_meta` and `snapshot::head_sha`.
pub fn git_rev_parse_head(repo_root: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|e| format!("git rev-parse HEAD 執行失敗：{}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse HEAD 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Python list repr for message/report faces: `['a', 'b']`. Module names
/// in practice contain no quotes (single-quote-only shape recorded as a
/// known boundary).
pub fn py_list_repr(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{}'", s)).collect();
    format!("[{}]", inner.join(", "))
}

/// Ordered `_meta` block (`common.py:93-115`): repo/commit/created_at/tool
/// then `extra` in given order — insertion order IS the JSON order
/// (serde_json preserve_order). `commit=None` shells out to
/// `git rev-parse HEAD` with Python `check=True` semantics: failure is a
/// crash (exit 1), unlike the tolerant R2 `engine::git_head` ([SRC] face).
pub fn make_meta(
    tool: &str,
    repo_root: &Path,
    commit: Option<&str>,
    extra: Vec<(&str, Value)>,
) -> Result<serde_json::Map<String, Value>, String> {
    let repo_root = resolve(repo_root);
    let commit = match commit {
        Some(c) => c.to_string(),
        None => git_rev_parse_head(&repo_root)?,
    };
    let mut map = serde_json::Map::new();
    map.insert(
        "repo".into(),
        Value::String(
            repo_root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
    );
    map.insert("commit".into(), Value::String(commit));
    map.insert("created_at".into(), Value::String(utc_now_iso_micros()));
    map.insert("tool".into(), Value::String(tool.to_string()));
    for (k, v) in extra {
        map.insert(k.to_string(), v);
    }
    Ok(map)
}

// ---------- D1: JSON serializer ----------

/// `json.dumps(v, ensure_ascii=False, indent=1)` byte face (D1): 1-space
/// PrettyFormatter, non-ASCII unescaped. Python's `indent=1` switches item
/// separators to `",\n"` and `": "` — serde's pretty writer emits exactly
/// that; empty containers print `{}{}`-style flat (`{}` / `[]`), matching
/// Python. Snapshot/transition *file* outputs use the same serializer;
/// their Python `ensure_ascii=True` escape difference is a recorded
/// non-gate deviation (semantic equality, Python can consume).
pub fn to_json_indent1(v: &Value) -> String {
    use serde::Serialize;
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b" "),
    );
    v.serialize(&mut ser).expect("in-memory JSON write");
    String::from_utf8(buf).expect("serde_json writes valid UTF-8")
}

/// Python `json.dumps(v, ensure_ascii=False)` with default (no-indent)
/// separators: `", "` after commas, `": "` after colons — serde's compact
/// writer omits those spaces, so this custom formatter restores them
/// (hub_refs `--json` byte face).
pub fn to_json_py_compact(v: &Value) -> String {
    use serde_json::ser::Formatter;
    struct PyCompact;
    impl Formatter for PyCompact {
        fn begin_array_value<W: ?Sized + std::io::Write>(
            &mut self,
            writer: &mut W,
            first: bool,
        ) -> std::io::Result<()> {
            // begin_array already wrote "["; separators only between items
            if first {
                Ok(())
            } else {
                writer.write_all(b", ")
            }
        }

        fn begin_object_key<W: ?Sized + std::io::Write>(
            &mut self,
            writer: &mut W,
            first: bool,
        ) -> std::io::Result<()> {
            if first {
                Ok(())
            } else {
                writer.write_all(b", ")
            }
        }

        fn begin_object_value<W: ?Sized + std::io::Write>(
            &mut self,
            writer: &mut W,
        ) -> std::io::Result<()> {
            writer.write_all(b": ")
        }
    }
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, PyCompact);
    use serde::Serialize;
    v.serialize(&mut ser).expect("in-memory JSON write");
    String::from_utf8(buf).expect("serde_json writes valid UTF-8")
}

// ---------- D2: time foundation ----------

/// `datetime.now(UTC).isoformat()` (`common.py:112`): microseconds at
/// default `timespec='auto'` — the fraction segment is omitted entirely
/// when micros == 0 (a 1e-6 probability edge recorded in D2; created_at
/// is not a stdout-gate face but feeds the R4-N interop sidecar).
pub fn utc_now_iso_micros() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_to_utc_iso(d.as_secs() as i64, d.subsec_micros(), true)
}

fn epoch_to_utc_iso(secs: i64, micros: u32, omit_zero_micros: bool) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = crate::engine::civil_from_days(days);
    let frac = if micros == 0 && omit_zero_micros {
        String::new()
    } else {
        format!(".{:06}", micros)
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}+00:00",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        frac
    )
}

/// Local-zone UTC offset in effect at the given epoch (seconds east).
/// D2 POC decision (S1): libc `localtime_r` over jiff/shell-out — exact
/// DST-aware semantics with a dependency already in the tree; equivalent
/// to Python's naive-`astimezone()` assumption by construction
/// (wall-clock → gmtoff at that wall-clock).
fn local_gmtoff(t: i64) -> i64 {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = t as libc::time_t;
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i64
    }
}

/// Naive wall-clock components → epoch under the local zone
/// (`datetime.fromisoformat(s).astimezone()` for naive `s`):
/// interpret as UTC, read the gmtoff in effect there, subtract.
/// Round-tripping through `timegm`/`localtime` is the POSIX-correct
/// DST-safe construction.
fn naive_local_to_epoch(y: i64, m: u32, d: u32, hh: i64, mi: i64, ss: i64) -> i64 {
    let days = days_from_civil(y, m, d);
    let as_utc = days * 86400 + hh * 3600 + mi * 60 + ss;
    as_utc - local_gmtoff(as_utc)
}

/// Hinnant inverse of `civil_from_days` (days → y/m/d is in engine.rs).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parsed ISO-8601 timestamp → epoch seconds (`snapshot.py:130`:
/// `datetime.fromisoformat(updated).astimezone()`). Naive forms assume the
/// local zone; offset forms convert directly. Fractional seconds beyond
/// microsecond precision are truncated (Python keeps µs; comparisons here
/// are second-granularity like the stale decision itself).
pub fn parse_iso_to_epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        s.get(r)?.parse::<i64>().ok()
    };
    let y = num(0..4)?;
    let mo = num(5..7)? as u32;
    let d = num(8..10)? as u32;
    if b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b't' && b[10] != b' ') {
        return None;
    }
    let hh = num(11..13)?;
    let mi = num(14..16)?;
    let ss = num(17..19)?;
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    // field-range guard: Python fromisoformat raises ValueError on
    // out-of-range fields (caller falls through to the next stale level);
    // unvalidated fields would compute a bogus epoch here instead.
    // Recorded coarseness vs fromisoformat: per-month day lengths are not
    // checked (2026-02-30 parses here, ValueError in Python), and the
    // date-only form (YYYY-MM-DD) is rejected — this feeds only the
    // `last_updated` fallback level, whose producers emit full ISO stamps
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || hh > 23 || mi > 59 || ss > 59 {
        return None;
    }
    let mut rest = &b[19..];
    // optional fraction
    if rest.first() == Some(&b'.') {
        let digits = rest[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        rest = &rest[1 + digits..];
    }
    // optional offset (Z / ±HH:MM / ±HHMM / ±HH)
    match rest.first() {
        None => Some(naive_local_to_epoch(y, mo, d, hh, mi, ss)),
        Some(&b'Z') | Some(&b'z') if rest.len() == 1 => {
            Some(days_from_civil(y, mo, d) * 86400 + hh * 3600 + mi * 60 + ss)
        }
        Some(&sign @ (b'+' | b'-')) if rest.len() >= 3 => {
            let digits: Vec<u8> = rest[1..]
                .iter()
                .copied()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if digits.len() < 2 {
                return None;
            }
            let oh: i64 = std::str::from_utf8(&digits[0..2]).ok()?.parse().ok()?;
            let om: i64 = match digits.get(2..4) {
                Some(mm) => std::str::from_utf8(mm).ok()?.parse().ok()?,
                None => 0,
            };
            let base = days_from_civil(y, mo, d) * 86400 + hh * 3600 + mi * 60 + ss;
            let off = oh * 3600 + om * 60;
            Some(if sign == b'+' { base - off } else { base + off })
        }
        _ => None,
    }
}

/// Local-wall rendering of an epoch for stale-reason messages
/// (`datetime.fromtimestamp(...).astimezone().isoformat()`): local zone,
/// microseconds at auto timespec. Micros come in nanos/1000.
pub fn local_epoch_to_iso_auto(secs: i64, nanos_part: u32) -> String {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = secs as libc::time_t;
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        let micros = nanos_part / 1000;
        let frac = if micros == 0 {
            String::new()
        } else {
            format!(".{:06}", micros)
        };
        let off = tm.tm_gmtoff as i64;
        let sign = if off < 0 { '-' } else { '+' };
        let aoff = off.abs();
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{}{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
            frac,
            sign,
            aoff / 3600,
            (aoff % 3600) / 60
        )
    }
}

/// Resolve without the symlink dance when the caller already resolved —
/// several call sites pass a just-resolved root; keep a single choke point
/// for the `resolve()` normalization the frozen code performs
/// (`common.py:100`, `snapshot.py:62`).
pub(crate) fn resolve(p: &Path) -> PathBuf {
    dunce_like_canonicalize(p)
}

/// `Path::resolve()` semantics (Python): canonicalize symlinks; fall back
/// to the input when the path does not exist yet (resolve() on a missing
/// tail is permissive in Python while `canonicalize` errors).
fn dunce_like_canonicalize(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| normalize_lexical(p))
}

/// Lexical fallback: drop `.` components and resolve `..` textually.
fn normalize_lexical(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_kinds_pinned() {
        assert_eq!(EDGE_KINDS, ["IMPORTS_FROM", "CALLS", "INHERITS"]);
    }

    #[test]
    fn anchor_pattern_shape() {
        let p = anchor_pattern("  x = 1  ");
        assert!(p.starts_with("^[ \\t]*"));
        assert!(p.ends_with("[ \\t]*$"));
        let re = regex::Regex::new(&p).unwrap();
        assert!(re.is_match("  x = 1  "));
        assert!(!re.is_match("x = 12"));
    }

    #[test]
    fn repo_relative_outside_is_none() {
        let root = Path::new("/tmp/repo");
        assert_eq!(repo_relative("/tmp/repo/a/b.py", root), Some("a/b.py".into()));
        assert_eq!(repo_relative("/elsewhere/x.py", root), None);
    }

    #[test]
    fn json_indent1_matches_python_shapes() {
        // Pinned against the local Python oracle (json.dumps(..., indent=1))
        let v: Value = serde_json::json!({"a": 1, "b": ["x", "y"], "c": {}, "d": []});
        assert_eq!(
            to_json_indent1(&v),
            "{\n \"a\": 1,\n \"b\": [\n  \"x\",\n  \"y\"\n ],\n \"c\": {},\n \"d\": []\n}"
        );
        let u: Value = serde_json::json!({"k": "值"});
        assert_eq!(to_json_indent1(&u), "{\n \"k\": \"值\"\n}");
    }

    #[test]
    fn utc_now_micros_shape() {
        let s = utc_now_iso_micros();
        assert!(s.ends_with("+00:00"));
        assert_eq!(s.len(), 25 + 7); // 19 base + 6 frac + suffix
    }

    #[test]
    fn epoch_to_utc_iso_zero_micros_omits_frac() {
        assert_eq!(
            epoch_to_utc_iso(0, 0, true),
            "1970-01-01T00:00:00+00:00"
        );
        assert_eq!(
            epoch_to_utc_iso(1, 500_000, true),
            "1970-01-01T00:00:01.500000+00:00"
        );
    }

    #[test]
    fn days_from_civil_roundtrips_engine_civil() {
        for days in [-100_000i64, -1, 0, 1, 19_000, 20_000] {
            let (y, m, d) = crate::engine::civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn iso_parse_offset_forms() {
        // aware offsets are exact regardless of local zone
        assert_eq!(parse_iso_to_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_to_epoch("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(
            parse_iso_to_epoch("2026-08-25T13:14:15.123456+08:00"),
            parse_iso_to_epoch("2026-08-25T05:14:15Z")
        );
        // naive forms assume local — equivalence with the libc construction
        let naive = parse_iso_to_epoch("2026-08-25T12:00:00").unwrap();
        let reconstructed = naive_local_to_epoch(2026, 8, 25, 12, 0, 0);
        assert_eq!(naive, reconstructed);
        assert_eq!(parse_iso_to_epoch("not-a-date"), None);
    }

    #[test]
    fn local_epoch_iso_renders_offset_and_auto_frac() {
        let s = local_epoch_to_iso_auto(0, 0);
        assert!(s.ends_with("1970-01-01T08:00:00+08:00"), "Asia/Taipei: {s}");
        assert!(!s.contains('.'), "zero micros omit frac: {s}");
        let t = local_epoch_to_iso_auto(1_000_000, 1_500_000);
        assert!(t.contains(".001500"), "{t}");
    }
}
