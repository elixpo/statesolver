//! Compact binary encoding (`.msb`).
//!
//! Layout:
//!
//! ```text
//! magic        8 bytes   "MSTRACE\0"
//! version      u16 LE
//! header_len   u32 LE
//! header       header_len bytes of JSON (same object as the JSONL header)
//! events       until EOF
//! ```
//!
//! Each event is a one-byte tag, the zigzag-encoded wrapping difference from
//! the previous timestamp as a LEB128 varint, then the payload fields as LEB128
//! varints. See `docs/trace-format.md` for the per-tag payloads.

use std::io::{BufRead, ErrorKind, Read, Write};

use crate::error::{Error, Result};
use crate::event::{ActId, CtxId, Event, EventKind, FORMAT_VERSION, FnId, Header};

/// File magic.
pub const MAGIC: &[u8; 8] = b"MSTRACE\0";

/// Header size limit, which bounds memory on untrusted input.
pub const MAX_HEADER_BYTES: u32 = 16 << 20;

const TAG_SPAWN: u8 = 1;
const TAG_CALL: u8 = 2;
const TAG_RETURN: u8 = 3;
const TAG_SUSPEND: u8 = 4;
const TAG_RESUME: u8 = 5;
const TAG_EXIT: u8 = 6;
const TAG_SAMPLE: u8 = 7;

const SAMPLE_RSS: u8 = 1 << 0;
const SAMPLE_CYCLES: u8 = 1 << 1;
const SAMPLE_LLC: u8 = 1 << 2;
const SAMPLE_DTLB: u8 = 1 << 3;
const SAMPLE_KNOWN: u8 = SAMPLE_RSS | SAMPLE_CYCLES | SAMPLE_LLC | SAMPLE_DTLB;

/// Streaming binary trace writer.
pub struct BinaryWriter<W: Write> {
    inner: W,
    prev_ts: u64,
    scratch: Vec<u8>,
}

impl<W: Write> BinaryWriter<W> {
    /// Write the magic and header.
    ///
    /// # Errors
    /// Fails on I/O errors or if the header is larger than [`MAX_HEADER_BYTES`].
    pub fn new(mut inner: W, header: &Header) -> Result<Self> {
        let json = serde_json::to_vec(header).map_err(|source| Error::Json { line: 0, source })?;
        let len = u32::try_from(json.len()).unwrap_or(u32::MAX);
        if len > MAX_HEADER_BYTES {
            return Err(Error::HeaderTooLarge(len));
        }
        inner.write_all(MAGIC)?;
        inner.write_all(&FORMAT_VERSION.to_le_bytes())?;
        inner.write_all(&len.to_le_bytes())?;
        inner.write_all(&json)?;
        Ok(Self {
            inner,
            prev_ts: 0,
            scratch: Vec::with_capacity(48),
        })
    }

    /// Append one event.
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn write(&mut self, event: &Event) -> Result<()> {
        let b = &mut self.scratch;
        b.clear();
        let delta = event.ts.wrapping_sub(self.prev_ts).cast_signed();
        self.prev_ts = event.ts;
        let head = |tag: u8, b: &mut Vec<u8>| {
            b.push(tag);
            put_varint(b, zigzag(delta));
        };
        match &event.kind {
            EventKind::Spawn { ctx, parent } => {
                head(TAG_SPAWN, b);
                put_varint(b, ctx.0);
                match parent {
                    None => b.push(0),
                    Some(p) => {
                        b.push(1);
                        put_varint(b, p.0);
                    }
                }
            }
            EventKind::Call {
                ctx,
                act,
                func,
                frame_bytes,
            } => {
                head(TAG_CALL, b);
                put_varint(b, ctx.0);
                put_varint(b, act.0);
                put_varint(b, u64::from(func.0));
                put_varint(b, u64::from(*frame_bytes));
            }
            EventKind::Return { ctx, act } => {
                head(TAG_RETURN, b);
                put_varint(b, ctx.0);
                put_varint(b, act.0);
            }
            EventKind::Suspend { ctx } => {
                head(TAG_SUSPEND, b);
                put_varint(b, ctx.0);
            }
            EventKind::Resume { ctx } => {
                head(TAG_RESUME, b);
                put_varint(b, ctx.0);
            }
            EventKind::Exit { ctx } => {
                head(TAG_EXIT, b);
                put_varint(b, ctx.0);
            }
            EventKind::Sample {
                rss_bytes,
                cycles,
                llc_misses,
                dtlb_misses,
            } => {
                head(TAG_SAMPLE, b);
                let fields = [
                    (SAMPLE_RSS, rss_bytes),
                    (SAMPLE_CYCLES, cycles),
                    (SAMPLE_LLC, llc_misses),
                    (SAMPLE_DTLB, dtlb_misses),
                ];
                let flags = fields
                    .iter()
                    .filter(|(_, v)| v.is_some())
                    .fold(0u8, |acc, (bit, _)| acc | bit);
                b.push(flags);
                for v in fields.iter().filter_map(|(_, v)| **v) {
                    put_varint(b, v);
                }
            }
        }
        self.inner.write_all(b)?;
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

/// Streaming binary trace reader.
pub struct BinaryReader<R> {
    inner: R,
    header: Header,
    prev_ts: u64,
    offset: u64,
    done: bool,
}

impl<R: BufRead> BinaryReader<R> {
    /// Read and check the magic and header.
    ///
    /// # Errors
    /// Fails on I/O errors, bad magic, an oversized or malformed header, or an
    /// unsupported version.
    pub fn new(mut inner: R) -> Result<Self> {
        let mut magic = [0u8; 8];
        read_exact_or(&mut inner, &mut magic, Error::BadMagic)?;
        if &magic != MAGIC {
            return Err(Error::BadMagic);
        }
        let mut v = [0u8; 2];
        read_exact_or(&mut inner, &mut v, Error::Truncated { offset: 8 })?;
        let version = u16::from_le_bytes(v);
        if version > FORMAT_VERSION {
            return Err(Error::UnsupportedVersion {
                found: version,
                supported: FORMAT_VERSION,
            });
        }
        let mut l = [0u8; 4];
        read_exact_or(&mut inner, &mut l, Error::Truncated { offset: 10 })?;
        let len = u32::from_le_bytes(l);
        if len > MAX_HEADER_BYTES {
            return Err(Error::HeaderTooLarge(len));
        }
        let mut json = vec![0u8; len as usize];
        read_exact_or(&mut inner, &mut json, Error::Truncated { offset: 14 })?;
        let header: Header =
            serde_json::from_slice(&json).map_err(|source| Error::Json { line: 0, source })?;
        crate::check_header(&header)?;
        Ok(Self {
            inner,
            header,
            prev_ts: 0,
            offset: 14 + u64::from(len),
            done: false,
        })
    }

    /// The trace header.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    fn byte(&mut self) -> Result<Option<u8>> {
        let mut b = [0u8; 1];
        loop {
            match self.inner.read(&mut b) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    self.offset += 1;
                    return Ok(Some(b[0]));
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn need_byte(&mut self) -> Result<u8> {
        self.byte()?.ok_or(Error::Truncated {
            offset: self.offset,
        })
    }

    fn varint(&mut self) -> Result<u64> {
        let start = self.offset;
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.need_byte()?;
            let low = u64::from(b & 0x7f);
            if shift == 63 && low > 1 {
                return Err(Error::BadInteger { offset: start });
            }
            value |= low << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::BadInteger { offset: start })
    }

    fn varint_u32(&mut self) -> Result<u32> {
        let start = self.offset;
        u32::try_from(self.varint()?).map_err(|_| Error::BadInteger { offset: start })
    }

    fn opt(&mut self, flags: u8, bit: u8) -> Result<Option<u64>> {
        if flags & bit == 0 {
            Ok(None)
        } else {
            self.varint().map(Some)
        }
    }

    fn read_event(&mut self) -> Result<Option<Event>> {
        let tag_offset = self.offset;
        let Some(tag) = self.byte()? else {
            return Ok(None);
        };
        if !(TAG_SPAWN..=TAG_SAMPLE).contains(&tag) {
            return Err(Error::InvalidTag {
                tag,
                offset: tag_offset,
            });
        }
        let delta = unzigzag(self.varint()?);
        let ts = self.prev_ts.wrapping_add(delta.cast_unsigned());
        self.prev_ts = ts;
        let kind = match tag {
            TAG_SPAWN => {
                let ctx = CtxId(self.varint()?);
                let at = self.offset;
                let parent = match self.need_byte()? {
                    0 => None,
                    1 => Some(CtxId(self.varint()?)),
                    _ => return Err(Error::BadInteger { offset: at }),
                };
                EventKind::Spawn { ctx, parent }
            }
            TAG_CALL => EventKind::Call {
                ctx: CtxId(self.varint()?),
                act: ActId(self.varint()?),
                func: FnId(self.varint_u32()?),
                frame_bytes: self.varint_u32()?,
            },
            TAG_RETURN => EventKind::Return {
                ctx: CtxId(self.varint()?),
                act: ActId(self.varint()?),
            },
            TAG_SUSPEND => EventKind::Suspend {
                ctx: CtxId(self.varint()?),
            },
            TAG_RESUME => EventKind::Resume {
                ctx: CtxId(self.varint()?),
            },
            TAG_EXIT => EventKind::Exit {
                ctx: CtxId(self.varint()?),
            },
            _ => {
                let at = self.offset;
                let flags = self.need_byte()?;
                if flags & !SAMPLE_KNOWN != 0 {
                    return Err(Error::BadInteger { offset: at });
                }
                EventKind::Sample {
                    rss_bytes: self.opt(flags, SAMPLE_RSS)?,
                    cycles: self.opt(flags, SAMPLE_CYCLES)?,
                    llc_misses: self.opt(flags, SAMPLE_LLC)?,
                    dtlb_misses: self.opt(flags, SAMPLE_DTLB)?,
                }
            }
        };
        Ok(Some(Event { ts, kind }))
    }
}

impl<R: BufRead> Iterator for BinaryReader<R> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let r = self.read_event().transpose();
        if !matches!(r, Some(Ok(_))) {
            self.done = true;
        }
        r
    }
}

fn read_exact_or<R: Read>(r: &mut R, buf: &mut [u8], on_eof: Error) -> Result<()> {
    match r.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Err(on_eof),
        Err(e) => Err(e.into()),
    }
}

fn put_varint(b: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        // Truncation is the point: keep the low seven bits.
        #[allow(clippy::cast_possible_truncation)]
        b.push((v as u8) | 0x80);
        v >>= 7;
    }
    #[allow(clippy::cast_possible_truncation)]
    b.push(v as u8);
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)).cast_unsigned()
}

fn unzigzag(v: u64) -> i64 {
    (v >> 1).cast_signed() ^ -((v & 1).cast_signed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_round_trips_extremes() {
        for v in [0, 1, -1, i64::MAX, i64::MIN, 12345, -12345] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
    }

    #[test]
    fn varint_round_trips_extremes() {
        for v in [0, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            let mut b = Vec::new();
            put_varint(&mut b, v);
            let mut data = Vec::new();
            data.extend_from_slice(MAGIC);
            data.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
            let h = serde_json::to_vec(&Header::new("t")).unwrap();
            data.extend_from_slice(&u32::try_from(h.len()).unwrap().to_le_bytes());
            data.extend_from_slice(&h);
            data.extend_from_slice(&b);
            let mut r = BinaryReader::new(&data[..]).unwrap();
            assert_eq!(r.varint().unwrap(), v);
        }
    }
}
