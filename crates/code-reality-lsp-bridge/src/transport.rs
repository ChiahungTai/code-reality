//! One bounded writer owns stdin. Child ownership never depends on a pipe write.
//! Nonblocking descriptors keep both transport threads joinable on cancellation.
use std::io::{self, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::framing::{read_message, write_message};
use crate::session::{DiagEntry, PendingSlot};

const TICK: Duration = Duration::from_millis(2);
struct Frame {
    message: Value,
    deadline: Instant,
    ack: mpsc::SyncSender<Result<(), String>>,
}

#[derive(Clone)]
pub(crate) struct Writer {
    tx: mpsc::SyncSender<Frame>,
    dead: Arc<AtomicBool>,
}

impl Writer {
    pub(crate) fn send(&self, message: Value, deadline: Instant) -> Result<(), String> {
        let (ack, rx) = mpsc::sync_channel(1);
        let mut frame = Frame {
            message,
            deadline,
            ack,
        };
        loop {
            remaining(&self.dead, deadline, "transport write")?;
            match self.tx.try_send(frame) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(f)) => frame = f,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("transport writer exited".into())
                }
            }
            std::thread::sleep(TICK);
        }
        loop {
            let wait = remaining(&self.dead, deadline, "transport write acknowledgement")?;
            match rx.recv_timeout(wait.min(TICK)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("transport writer exited".into())
                }
            }
        }
    }
}

pub(crate) fn remaining(
    dead: &AtomicBool,
    deadline: Instant,
    stage: &str,
) -> Result<Duration, String> {
    if dead.load(Ordering::SeqCst) {
        return Err("language server backend died — restart the bridge to recover".into());
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| format!("timeout waiting for {stage}"))
}

/// Preserve framing's write_all/read_exact semantics while making WouldBlock
/// interruptible. No partial frame is ever retried on a new session.
struct Pipe<T> {
    io: T,
    dead: Arc<AtomicBool>,
    deadline: Option<Instant>,
}
impl<T> Pipe<T> {
    fn check(&self) -> io::Result<()> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "session terminated",
            ));
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "transport write deadline",
            ));
        }
        Ok(())
    }
}
impl<T: Read> Read for Pipe<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            self.check()?;
            match self.io.read(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(TICK),
                result => return result,
            }
        }
    }
}
impl<T: Write> Write for Pipe<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            self.check()?;
            match self.io.write(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(TICK),
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        self.io.flush()
    }
}
fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    // SAFETY: the live ChildStdin/ChildStdout owns this descriptor; fcntl
    // changes flags without closing it or transferring ownership.
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
fn reap(child: &Mutex<Child>) {
    let mut child = child.lock().unwrap();
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

pub(crate) struct Transport {
    child: Arc<Mutex<Child>>,
    pub(crate) writer: Writer,
    reader: Option<JoinHandle<()>>,
    writer_thread: Option<JoinHandle<()>>,
}
impl Transport {
    pub(crate) fn start(
        mut child: Child,
        dead: Arc<AtomicBool>,
        pending: PendingSlot,
        diagnostics: Arc<Mutex<std::collections::HashMap<String, DiagEntry>>>,
    ) -> Result<Self, String> {
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        if let Err(e) = nonblocking(&stdin).and_then(|()| nonblocking(&stdout)) {
            let _ = child.kill();
            let _ = child.wait();
            dead.store(true, Ordering::SeqCst);
            return Err(e.to_string());
        }
        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = mpsc::sync_channel::<Frame>(8);
        let writer = Writer {
            tx,
            dead: dead.clone(),
        };
        let writer_dead = dead.clone();
        let writer_thread = std::thread::spawn(move || {
            let mut pipe = Pipe {
                io: stdin,
                dead: writer_dead.clone(),
                deadline: None,
            };
            while !writer_dead.load(Ordering::SeqCst) {
                let frame = match rx.recv_timeout(TICK) {
                    Ok(f) => f,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                pipe.deadline = Some(frame.deadline);
                let result = write_message(&mut pipe, &frame.message).map_err(|e| e.to_string());
                let failed = result.is_err();
                let _ = frame.ack.try_send(result);
                if failed {
                    writer_dead.store(true, Ordering::SeqCst);
                    break;
                }
            }
        });
        let reply_writer = writer.clone();
        let reader_child = child.clone();
        let reader = std::thread::spawn(move || {
            let mut input = BufReader::new(Pipe {
                io: stdout,
                dead: dead.clone(),
                deadline: None,
            });
            while let Ok(Some(msg)) = read_message(&mut input) {
                let id = msg.get("id");
                if msg.get("method").is_none() {
                    let mut slot = pending.lock().unwrap();
                    if slot
                        .as_ref()
                        .is_some_and(|(want, _)| id.and_then(Value::as_i64) == Some(*want))
                    {
                        let (_, tx) = slot.take().unwrap();
                        let _ = tx.try_send(msg);
                    }
                } else if let Some(id) = id {
                    if reply_writer
                        .send(
                            json!({"jsonrpc":"2.0", "id":id, "result":[]}),
                            Instant::now() + Duration::from_secs(30),
                        )
                        .is_err()
                    {
                        break;
                    }
                } else if msg["method"] == "textDocument/publishDiagnostics" {
                    if let Some(params) = msg.get("params") {
                        let uri = params["uri"].as_str().unwrap_or_default().to_string();
                        diagnostics.lock().unwrap().insert(
                            uri,
                            DiagEntry {
                                version: params["version"].as_i64(),
                                diagnostics: params["diagnostics"]
                                    .as_array()
                                    .cloned()
                                    .unwrap_or_default(),
                                last_push: Instant::now(),
                            },
                        );
                    }
                }
            }
            dead.store(true, Ordering::SeqCst);
            if let Some((_, tx)) = pending.lock().unwrap().take() {
                let _ = tx.try_send(json!({"error":{"code":-32603,"message":"backend exited"}}));
            }
            reap(&reader_child);
        });
        Ok(Self {
            child,
            writer,
            reader: Some(reader),
            writer_thread: Some(writer_thread),
        })
    }
    pub(crate) fn pid(&self) -> u32 {
        self.child.lock().unwrap().id()
    }
    pub(crate) fn exited(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(Some(_)))
    }
    pub(crate) fn stop(&mut self) {
        self.writer.dead.store(true, Ordering::SeqCst);
        reap(&self.child);
        if let Some(handle) = self.writer_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.stop();
    }
}
