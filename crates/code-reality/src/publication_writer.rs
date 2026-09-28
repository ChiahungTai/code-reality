//! Data-plane writer ownership. Lock order: heal -> publication writer.
//! Separate open file descriptions serialize threads as well as processes.
//! The persistent lock inode is never unlinked; the OS releases ownership on
//! process death. Readers do not take this lock. Non-cooperating writers and
//! source edits are outside its scope.
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(crate) struct WriterGuard {
    _file: File,
}

impl WriterGuard {
    pub(crate) fn acquire(repo: &Path) -> Result<Self, String> {
        let dir = crate::engine::resolve_repo(repo).join(".code-reality");
        std::fs::create_dir_all(&dir).map_err(|e| format!("writer directory: {e}"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(".writer.lock"))
            .map_err(|e| format!("writer lock: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: file owns a valid descriptor for this entire guard.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Self { _file: file });
            }
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::WouldBlock
                && e.kind() != std::io::ErrorKind::Interrupted
            {
                return Err(format!("writer lock: {e}"));
            }
            if Instant::now() >= deadline {
                return Err("data-plane writer busy (5s wait budget); retry after the active writer finishes".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Only this attempt's temporary files are removed, including SQLite journals.
pub(crate) struct TempDb(pub(crate) PathBuf);
impl TempDb {
    pub(crate) fn new(dir: &Path) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let p = dir.join(format!(
                ".graph-build-{}-{}.db",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(_) => return Ok(Self(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("graph temporary file: {e}")),
            }
        }
    }
}
impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        for suffix in ["-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}
