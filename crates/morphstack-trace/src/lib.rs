//! MorphStack activation-event trace format, version 0.
//!
//! A trace is a [`Header`] followed by a time-ordered stream of [`Event`]s
//! describing context lifecycles (`spawn`, `suspend`, `resume`, `exit`),
//! activation lifecycles (`call`, `return`), and optional resource samples.
//!
//! Two encodings carry the same data model:
//!
//! - **JSONL** ([`Format::Jsonl`]): human-readable, used for tests and examples.
//! - **Binary** ([`Format::Binary`], `.msb`): compact varint encoding for
//!   traces with tens of millions of events.
//!
//! Readers stream; nothing is buffered beyond one event. See
//! `docs/trace-format.md` for the normative specification.

pub mod binary;
pub mod error;
pub mod event;
pub mod jsonl;
pub mod stats;
pub mod validate;

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

pub use error::{Error, Result};
pub use event::{
    ActId, CtxId, Event, EventKind, FORMAT_NAME, FORMAT_VERSION, FnId, FunctionInfo, Header,
};
pub use stats::{StatsCollector, TraceStats};
pub use validate::{Rule, Validator, Violation};

/// On-disk encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON Lines.
    Jsonl,
    /// Compact binary (`.msb`).
    Binary,
}

impl Format {
    /// `.msb` means binary; anything else means JSONL.
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("msb"))
        {
            Self::Binary
        } else {
            Self::Jsonl
        }
    }
}

pub(crate) fn check_header(h: &Header) -> Result<()> {
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

/// A streaming reader over either encoding.
pub enum TraceReader<R> {
    /// JSONL input.
    Jsonl(jsonl::JsonlReader<R>),
    /// Binary input.
    Binary(binary::BinaryReader<R>),
}

impl<R: BufRead> TraceReader<R> {
    /// Detect the encoding from the leading bytes and read the header.
    ///
    /// # Errors
    /// Fails on I/O errors or an invalid header.
    pub fn new(mut inner: R) -> Result<Self> {
        // JSONL starts with `{` or whitespace, so a magic-prefix check is unambiguous.
        if inner.fill_buf()?.starts_with(binary::MAGIC) {
            binary::BinaryReader::new(inner).map(Self::Binary)
        } else {
            jsonl::JsonlReader::new(inner).map(Self::Jsonl)
        }
    }

    /// The trace header.
    #[must_use]
    pub fn header(&self) -> &Header {
        match self {
            Self::Jsonl(r) => r.header(),
            Self::Binary(r) => r.header(),
        }
    }

    /// The detected encoding.
    #[must_use]
    pub fn format(&self) -> Format {
        match self {
            Self::Jsonl(_) => Format::Jsonl,
            Self::Binary(_) => Format::Binary,
        }
    }
}

impl<R: BufRead> Iterator for TraceReader<R> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Jsonl(r) => r.next(),
            Self::Binary(r) => r.next(),
        }
    }
}

/// Open a trace file, detecting its encoding from its contents.
///
/// # Errors
/// Fails if the file cannot be opened or its header is invalid.
pub fn open(path: impl AsRef<Path>) -> Result<TraceReader<BufReader<File>>> {
    TraceReader::new(BufReader::with_capacity(1 << 16, File::open(path)?))
}

/// A streaming writer over either encoding.
pub enum TraceWriter<W: Write> {
    /// JSONL output.
    Jsonl(jsonl::JsonlWriter<W>),
    /// Binary output.
    Binary(binary::BinaryWriter<W>),
}

impl<W: Write> TraceWriter<W> {
    /// Start a trace in the given encoding.
    ///
    /// # Errors
    /// Fails if the header cannot be written.
    pub fn new(inner: W, format: Format, header: &Header) -> Result<Self> {
        match format {
            Format::Jsonl => jsonl::JsonlWriter::new(inner, header).map(Self::Jsonl),
            Format::Binary => binary::BinaryWriter::new(inner, header).map(Self::Binary),
        }
    }

    /// Append one event.
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn write(&mut self, event: &Event) -> Result<()> {
        match self {
            Self::Jsonl(w) => w.write(event),
            Self::Binary(w) => w.write(event),
        }
    }

    /// Flush and return the underlying writer.
    ///
    /// # Errors
    /// Fails if flushing fails.
    pub fn finish(self) -> Result<W> {
        match self {
            Self::Jsonl(w) => w.finish(),
            Self::Binary(w) => w.finish(),
        }
    }
}

/// Create a trace file; `.msb` paths are binary, others JSONL.
///
/// # Errors
/// Fails if the file cannot be created or the header cannot be written.
pub fn create(path: impl AsRef<Path>, header: &Header) -> Result<TraceWriter<BufWriter<File>>> {
    let path = path.as_ref();
    let format = Format::from_path(path);
    TraceWriter::new(
        BufWriter::with_capacity(1 << 16, File::create(path)?),
        format,
        header,
    )
}
