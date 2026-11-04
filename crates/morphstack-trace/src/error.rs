//! Error type for reading and writing traces.

/// Errors produced while reading or writing a trace.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A JSONL line could not be parsed.
    #[error("line {line}: {source}")]
    Json {
        /// 1-based line number.
        line: u64,
        /// Parser error.
        source: serde_json::Error,
    },

    /// A JSONL line exceeds the size limit.
    #[error("line {line} is longer than {max} bytes", max = crate::jsonl::MAX_LINE_BYTES)]
    LineTooLong {
        /// 1-based line number.
        line: u64,
    },

    /// The trace is missing its header.
    #[error("trace is empty: missing header")]
    MissingHeader,

    /// The header names a different format.
    #[error("not a morphstack trace (format = {0:?})")]
    WrongFormat(String),

    /// The header version is newer than this reader supports.
    #[error("unsupported trace version {found} (this build reads up to {supported})")]
    UnsupportedVersion {
        /// Version in the file.
        found: u16,
        /// Newest version this build understands.
        supported: u16,
    },

    /// Binary trace does not start with the expected magic bytes.
    #[error("bad magic bytes: not a binary morphstack trace")]
    BadMagic,

    /// Binary header exceeds the size limit.
    #[error("binary header is {0} bytes, over the {max} byte limit", max = crate::binary::MAX_HEADER_BYTES)]
    HeaderTooLarge(u32),

    /// Binary trace ended in the middle of an event.
    #[error("truncated binary trace at byte {offset}")]
    Truncated {
        /// Byte offset where data ran out.
        offset: u64,
    },

    /// Unknown event tag in a binary trace.
    #[error("invalid event tag {tag:#04x} at byte {offset}")]
    InvalidTag {
        /// Offending tag.
        tag: u8,
        /// Byte offset of the tag.
        offset: u64,
    },

    /// A varint was too long or a value did not fit its field.
    #[error("malformed integer at byte {offset}")]
    BadInteger {
        /// Byte offset where the integer started.
        offset: u64,
    },
}

/// Convenience alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
