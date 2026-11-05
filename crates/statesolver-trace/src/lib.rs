//! The StateSolver `.ss` trace format, version 0.
//!
//! A trace is a [`Header`] followed by a gap-free, ordered stream of
//! [`Event`]s: register snapshots and deltas, address-space changes, syscalls
//! with the bytes the kernel wrote (so replay can inject them), signals,
//! allocation lifetimes with per-address [`Epoch`]s, markers, and exit.
//!
//! One data model, two containers:
//!
//! - **JSONL** (`*.jsonl`): one JSON object per line, header first. Used for
//!   tests and hand-written examples.
//! - **`.ss`**: the same JSONL stream, zstd-compressed. Inspect it with
//!   `zstdcat trace.ss | head`.
//!
//! Readers detect the container from the first bytes and stream; nothing is
//! buffered beyond one line. See `docs/trace-format.md`.

pub mod bytes;
pub mod event;
pub mod validate;

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

pub use bytes::Bytes;
pub use event::{
    AllocId, AllocKind, Epoch, Event, EventId, EventKind, FORMAT_NAME, FORMAT_VERSION, Header,
    MapOp, MemWrite, Program, Reg, Region, RrPos,
};
pub use validate::{Rule, Validator, Violation};

/// Lines longer than this are rejected, which bounds memory on untrusted
/// input. Large enough for a hex-encoded 16 MiB syscall payload.
pub const MAX_LINE_BYTES: u64 = 64 << 20;

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Errors from reading or writing traces.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// A line could not be parsed.
    #[error("line {line}: {source}")]
    Json {
        /// 1-based line number.
        line: u64,
        /// Parser error.
        source: serde_json::Error,
    },
    /// A line exceeds [`MAX_LINE_BYTES`].
    #[error("line {line} is longer than {MAX_LINE_BYTES} bytes")]
    LineTooLong {
        /// 1-based line number.
        line: u64,
    },
    /// No header.
    #[error("trace is empty: missing header")]
    MissingHeader,
    /// Wrong `format` field.
    #[error("not a statesolver trace (format = {0:?})")]
    WrongFormat(String),
    /// Newer version than supported.
    #[error("unsupported trace version {found} (this build reads up to {supported})")]
    UnsupportedVersion {
        /// Version in the file.
        found: u16,
        /// Newest supported.
        supported: u16,
    },
}

/// Convenience alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Plain JSON Lines.
    Jsonl,
    /// zstd-compressed JSON Lines (`.ss`).
    Ss,
}

impl Format {
    /// `.jsonl` means plain; anything else means `.ss`.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        {
            Self::Jsonl
        } else {
            Self::Ss
        }
    }
}

fn check_header(h: &Header) -> Result<()> {
    if h.format != FORMAT_NAME {
        return Err(Error::WrongFormat(h.format.clone()));
    }
    if h.version > FORMAT_VERSION {
        return Err(Error::UnsupportedVersion {
            found: h.version,
            supported: FORMAT_VERSION,
        });
    }
    Ok(())
}

/// Streaming reader over either container.
pub struct TraceReader {
    inner: Box<dyn BufRead>,
    header: Header,
    format: Format,
    line: u64,
    buf: Vec<u8>,
}

impl TraceReader {
    /// Detect the container and read the header.
    ///
    /// # Errors
    /// Fails on I/O errors or an invalid header.
    pub fn new(mut inner: Box<dyn BufRead>) -> Result<Self> {
        let format = if inner.fill_buf()?.starts_with(&ZSTD_MAGIC) {
            Format::Ss
        } else {
            Format::Jsonl
        };
        if format == Format::Ss {
            inner = Box::new(BufReader::with_capacity(
                1 << 16,
                zstd::stream::read::Decoder::with_buffer(inner)?,
            ));
        }
        let mut r = Self {
            inner,
            header: Header::new(
                "",
                Program {
                    path: String::new(),
                    argv: Vec::new(),
                    sha256: None,
                    load_base: None,
                },
            ),
            format,
            line: 0,
            buf: Vec::new(),
        };
        loop {
            if !r.read_line()? {
                return Err(Error::MissingHeader);
            }
            if !r.buf.iter().all(u8::is_ascii_whitespace) {
                break;
            }
        }
        let line = r.line;
        r.header = serde_json::from_slice(&r.buf).map_err(|source| Error::Json { line, source })?;
        check_header(&r.header)?;
        Ok(r)
    }

    /// The header.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The detected container.
    #[must_use]
    pub fn format(&self) -> Format {
        self.format
    }

    fn read_line(&mut self) -> Result<bool> {
        self.buf.clear();
        self.line += 1;
        let n = (&mut self.inner)
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut self.buf)?;
        if n == 0 {
            return Ok(false);
        }
        if self.buf.last() == Some(&b'\n') {
            self.buf.pop();
            if self.buf.last() == Some(&b'\r') {
                self.buf.pop();
            }
        } else if n as u64 > MAX_LINE_BYTES {
            return Err(Error::LineTooLong { line: self.line });
        }
        Ok(true)
    }
}

impl Iterator for TraceReader {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.read_line() {
                Err(e) => return Some(Err(e)),
                Ok(false) => return None,
                Ok(true) if self.buf.iter().all(u8::is_ascii_whitespace) => {}
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

/// Streaming writer over either container.
pub struct TraceWriter<W: Write> {
    inner: Sink<W>,
}

enum Sink<W: Write> {
    Plain(W),
    Zstd(zstd::stream::write::Encoder<'static, W>),
}

impl<W: Write> Sink<W> {
    fn get(&mut self) -> &mut dyn Write {
        match self {
            Self::Plain(w) => w,
            Self::Zstd(w) => w,
        }
    }
}

impl<W: Write> TraceWriter<W> {
    /// Start a trace and write the header.
    ///
    /// # Errors
    /// Fails on I/O or serialization errors.
    pub fn new(inner: W, format: Format, header: &Header) -> Result<Self> {
        let sink = match format {
            Format::Jsonl => Sink::Plain(inner),
            Format::Ss => Sink::Zstd(zstd::stream::write::Encoder::new(inner, 3)?),
        };
        let mut w = Self { inner: sink };
        w.line(header)?;
        Ok(w)
    }

    fn line<T: serde::Serialize>(&mut self, v: &T) -> Result<()> {
        let out = self.inner.get();
        serde_json::to_writer(&mut *out, v).map_err(|source| Error::Json { line: 0, source })?;
        out.write_all(b"\n")?;
        Ok(())
    }

    /// Append one event.
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn write(&mut self, event: &Event) -> Result<()> {
        self.line(event)
    }

    /// Flush, finish compression, and return the underlying writer.
    ///
    /// # Errors
    /// Fails if flushing fails.
    pub fn finish(self) -> Result<W> {
        Ok(match self.inner {
            Sink::Plain(mut w) => {
                w.flush()?;
                w
            }
            Sink::Zstd(z) => {
                let mut w = z.finish()?;
                w.flush()?;
                w
            }
        })
    }
}

/// Open a trace file of either container.
///
/// # Errors
/// Fails if the file cannot be opened or its header is invalid.
pub fn open(path: impl AsRef<Path>) -> Result<TraceReader> {
    TraceReader::new(Box::new(BufReader::with_capacity(
        1 << 16,
        File::open(path)?,
    )))
}

/// Create a trace file; `.jsonl` paths are plain, others are `.ss`.
///
/// # Errors
/// Fails if the file cannot be created or the header written.
pub fn create(path: impl AsRef<Path>, header: &Header) -> Result<TraceWriter<BufWriter<File>>> {
    let path = path.as_ref();
    TraceWriter::new(
        BufWriter::with_capacity(1 << 16, File::create(path)?),
        Format::from_path(path),
        header,
    )
}
