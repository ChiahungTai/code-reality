//! LSP base-protocol framing over a child's stdio: `Content-Length`
//! headers (terminated with `\r\n`, header block ended by an empty
//! `\r\n` line) followed by exactly N body bytes. The upstream pyrefly
//! reader requires the `\r\n` terminator and only recognizes
//! `Content-Length` (case-insensitive) — mirror that on the write side
//! and accept it case-insensitively on the read side.

use std::io::{BufRead, Write};

use serde_json::Value;

pub fn write_message<W: Write>(w: &mut W, msg: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Read one framed message. `Ok(None)` = clean EOF (the backend exited).
pub fn read_message<R: BufRead>(r: &mut R) -> std::io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok();
            }
        }
    }
    let len = content_length.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing Content-Length header",
        )
    })?;
    // A corrupt header with a huge value must fail as a protocol error,
    // not attempt a giant allocation (F8).
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Content-Length exceeds 64 MiB cap",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}
