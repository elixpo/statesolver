//! JSON Lines encoding: a header object on the first line, then one event per line.

use std::io::{BufRead, Read, Write};

use crate::error::{Error, Result};
use crate::event::{Event, Header};

/// Lines longer than this are rejected, which bounds memory on untrusted input.
pub const MAX_LINE_BYTES: u64 = 1 << 20;

/// Streaming JSONL trace reader.
pub struct JsonlReader<R> {
    inner: R,
    header: Header,
    line: u64,
    buf: Vec<u8>,
}

impl<R: BufRead> JsonlReader<R> {
    /// Read and check the header line.
    ///
    /// # Errors
    /// Fails on I/O errors, a missing or malformed header, or an unsupported version.
    pub fn new(mut inner: R) -> Result<Self> {
        let mut buf = Vec::new();
        let mut line = 0;
        loop {
            if !read_line(&mut inner, &mut buf, &mut line)? {
                return Err(Error::MissingHeader);
            }
            if !is_blank(&buf) {
                break;
            }
        }
        let header: Header =
            serde_json::from_slice(&buf).map_err(|source| Error::Json { line, source })?;
        crate::check_header(&header)?;
        Ok(Self {
            inner,
            header,
            line,
            buf,
        })
    }

    /// The trace header.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }
}

impl<R: BufRead> Iterator for JsonlReader<R> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match read_line(&mut self.inner, &mut self.buf, &mut self.line) {
                Err(e) => return Some(Err(e)),
                Ok(false) => return None,
                Ok(true) if is_blank(&self.buf) => {}
                Ok(true) => {
                    let line = self.line;
                    return Some(
                        serde_json::from_slice(&self.buf)
                            .map_err(|source| Error::Json { line, source }),
                    );
                }
            }
        }
    }
}

/// Read one line into `buf`, without its terminator. Returns `false` at EOF.
fn read_line<R: BufRead>(inner: &mut R, buf: &mut Vec<u8>, line: &mut u64) -> Result<bool> {
    buf.clear();
    *line += 1;
    let n = inner
        .by_ref()
        .take(MAX_LINE_BYTES + 1)
        .read_until(b'\n', buf)?;
    if n == 0 {
        return Ok(false);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    } else if n as u64 > MAX_LINE_BYTES {
        return Err(Error::LineTooLong { line: *line });
    }
    Ok(true)
}

fn is_blank(buf: &[u8]) -> bool {
    buf.iter().all(u8::is_ascii_whitespace)
}

/// Streaming JSONL trace writer.
pub struct JsonlWriter<W: Write> {
    inner: W,
}

impl<W: Write> JsonlWriter<W> {
    /// Write the header line.
    ///
    /// # Errors
    /// Fails on I/O or serialization errors.
    pub fn new(mut inner: W, header: &Header) -> Result<Self> {
        serde_json::to_writer(&mut inner, header)
            .map_err(|source| Error::Json { line: 1, source })?;
        inner.write_all(b"\n")?;
        Ok(Self { inner })
    }

    /// Append one event.
    ///
    /// # Errors
    /// Fails on I/O or serialization errors.
    pub fn write(&mut self, event: &Event) -> Result<()> {
        serde_json::to_writer(&mut self.inner, event)
            .map_err(|source| Error::Json { line: 0, source })?;
        self.inner.write_all(b"\n")?;
        Ok(())
    }

    /// Flush and return the underlying writer.
    ///
    /// # Errors
    /// Fails if flushing fails.
    pub fn finish(mut self) -> Result<W> {
        self.inner.flush()?;
        Ok(self.inner)
    }
}
