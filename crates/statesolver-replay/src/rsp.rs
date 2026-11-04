//! Minimal client for the gdb remote serial protocol (RSP), as spoken by
//! `rr replay -s <port>`.
//!
//! Packets are `$<payload>#<two-hex checksum>`. After the `qSupported`
//! handshake the client switches to no-ack mode (`QStartNoAckMode`), so
//! every request is exactly one packet out and one packet back, except for
//! console-output (`O…`) packets, which are skipped.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::{Error, Result};

/// One RSP connection.
pub struct Connection {
    stream: TcpStream,
    buf: Vec<u8>,
    ack: bool,
}

fn checksum(payload: &[u8]) -> u8 {
    payload.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

impl Connection {
    /// Wrap a connected stream and perform the handshake.
    ///
    /// # Errors
    /// Fails on I/O errors or a malformed handshake.
    pub fn new(stream: TcpStream) -> Result<Self> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(600)))?;
        let mut c = Self {
            stream,
            buf: Vec::new(),
            ack: true,
        };
        let sup = c.request("qSupported:multiprocess+;swbreak+;hwbreak+")?;
        if !sup.contains("ReverseContinue+") {
            return Err(Error::Protocol(format!(
                "server does not support reverse execution: {sup}"
            )));
        }
        if sup.contains("QStartNoAckMode+") && c.request("QStartNoAckMode")? == "OK" {
            c.ack = false;
        }
        Ok(c)
    }

    fn send(&mut self, payload: &str) -> Result<()> {
        let mut out = Vec::with_capacity(payload.len() + 4);
        out.push(b'$');
        out.extend_from_slice(payload.as_bytes());
        out.push(b'#');
        out.extend_from_slice(format!("{:02x}", checksum(payload.as_bytes())).as_bytes());
        self.stream.write_all(&out)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<String> {
        loop {
            if let Some(start) = self.buf.iter().position(|&b| b == b'$')
                && let Some(hash) = self.buf[start..].iter().position(|&b| b == b'#')
            {
                let hash = start + hash;
                if self.buf.len() >= hash + 3 {
                    let payload = self.buf[start + 1..hash].to_vec();
                    self.buf.drain(..hash + 3);
                    if self.ack {
                        self.stream.write_all(b"+")?;
                    }
                    return Ok(String::from_utf8_lossy(&unescape(&payload)).into_owned());
                }
            }
            let mut chunk = [0u8; 16 * 1024];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                return Err(Error::Protocol("rr closed the connection".into()));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Send one request and return its reply, skipping console output.
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn request(&mut self, payload: &str) -> Result<String> {
        self.send(payload)?;
        loop {
            let r = self.recv()?;
            if r.len() > 1 && r.starts_with('O') && r != "OK" {
                continue;
            }
            return Ok(r);
        }
    }

    /// Send a packet without waiting for a reply (`k`).
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn send_only(&mut self, payload: &str) -> Result<()> {
        self.send(payload)
    }
}

/// Undo RSP binary escaping (`}` followed by the byte XOR 0x20) and run-length encoding.
fn unescape(p: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.len());
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            b'}' if i + 1 < p.len() => {
                out.push(p[i + 1] ^ 0x20);
                i += 2;
            }
            b'*' if i + 1 < p.len() && !out.is_empty() => {
                let n = usize::from(p[i + 1].saturating_sub(29));
                let last = *out.last().unwrap_or(&0);
                out.extend(std::iter::repeat_n(last, n));
                i += 2;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// Decode a hex string into bytes.
///
/// # Errors
/// Fails on malformed hex.
pub fn decode_hex(s: &str) -> Result<Vec<u8>> {
    statesolver_trace::bytes::from_hex(s).map_err(|e| Error::Protocol(format!("{e}: {s:.60}")))
}

/// A little-endian register value from hex.
///
/// # Errors
/// Fails on malformed hex.
pub fn decode_le(s: &str) -> Result<u64> {
    let b = decode_hex(s)?;
    let mut v = [0u8; 8];
    let n = b.len().min(8);
    v[..n].copy_from_slice(&b[..n]);
    Ok(u64::from_le_bytes(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_and_unescape() {
        assert_eq!(checksum(b"OK"), 0x9a);
        assert_eq!(unescape(b"a}\x03b"), b"a#b");
        assert_eq!(unescape(b"0* "), b"0000");
        assert_eq!(
            decode_le("40c349a68f790000").unwrap(),
            0x0000_798f_a649_c340
        );
    }
}
