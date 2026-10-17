//! Shared helpers for the line-based text handlers ([`csv`](super::csv) and
//! [`text`](super::text)): the embedded bit-stream envelope and the
//! "is this text?" check.
//!
//! # Envelope
//!
//! ```text
//! [magic 4 bytes][strategy u8][len u32 LE][len bytes of codec::encode_payload output]
//! ```
//!
//! The stream is written as bits, least-significant bit of each byte first.
//! The extractor reads the 9-byte header, checks magic and strategy, caps the
//! declared length against the carrier's capacity (when known) before
//! buffering the body, and hands the body to
//! [`crate::codec::decode_payload`] exactly once.

use crate::core::{Result, SteganoError};

/// Envelope header length in bytes: magic (4) + strategy (1) + length (4).
pub(crate) const ENVELOPE_LEN: usize = 9;
/// Envelope header length in bits.
pub(crate) const ENVELOPE_BITS: usize = ENVELOPE_LEN * 8;

/// Build `[magic][strategy][len u32 LE] ‖ encoded`.
pub(crate) fn build(magic: &[u8; 4], strategy: u8, encoded: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(encoded.len()).map_err(|_| {
        SteganoError::InvalidParameter("Payload too large for envelope (> 4 GiB)".into())
    })?;
    let mut s = Vec::with_capacity(ENVELOPE_LEN + encoded.len());
    s.extend_from_slice(magic);
    s.push(strategy);
    s.extend_from_slice(&len.to_le_bytes());
    s.extend_from_slice(encoded);
    Ok(s)
}

/// Bit `i` (LSB-first within each byte) of `bytes`; `0` past the end.
pub(crate) fn bit(bytes: &[u8], i: usize) -> u8 {
    bytes.get(i / 8).map_or(0, |b| (b >> (i % 8)) & 1)
}

/// The standard "nothing embedded here" error.
pub(crate) fn not_found(what: &str) -> SteganoError {
    SteganoError::IntegrityError(format!("No stegano data found in {what} carrier"))
}

/// Require `data` (which starts at byte `base` of the carrier) to be text:
/// valid UTF-8 without NUL bytes.  Checking line by line gives the same
/// result as checking the whole carrier, because `\n` never occurs inside a
/// multi-byte UTF-8 sequence.
pub(crate) fn check_text(data: &[u8], base: usize, what: &str) -> Result<()> {
    if let Some(pos) = data.iter().position(|&b| b == 0) {
        let pos = base + pos;
        return Err(SteganoError::IntegrityError(format!(
            "{what} carrier looks binary (NUL byte at offset {pos}); \
             only UTF-8 text is supported"
        )));
    }
    if let Err(e) = std::str::from_utf8(data) {
        return Err(SteganoError::IntegrityError(format!(
            "{what} carrier is not valid UTF-8 (invalid byte at offset {}); \
             only UTF-8 text is supported",
            base + e.valid_up_to()
        )));
    }
    Ok(())
}

/// Incremental reader for an envelope written as a bit stream.
pub(crate) struct EnvelopeReader {
    magic: [u8; 4],
    strategy: u8,
    /// Largest acceptable declared body length (bytes).
    max_len: usize,
    what: &'static str,
    cur: u8,
    nbits: u32,
    buf: Vec<u8>,
    len: Option<usize>,
}

impl EnvelopeReader {
    /// Reader expecting `magic` and `strategy`, rejecting declared lengths
    /// above `max_len` bytes (pass `usize::MAX` when the capacity is not
    /// known, e.g. while streaming).
    pub(crate) fn new(magic: &[u8; 4], strategy: u8, max_len: usize, what: &'static str) -> Self {
        Self {
            magic: *magic,
            strategy,
            max_len,
            what,
            cur: 0,
            nbits: 0,
            buf: Vec::with_capacity(ENVELOPE_LEN),
            len: None,
        }
    }

    /// Whether the whole envelope has been read.
    pub(crate) fn is_done(&self) -> bool {
        self.len
            .is_some_and(|len| self.buf.len() == ENVELOPE_LEN + len)
    }

    /// Whether the 9-byte header has been read and accepted.
    pub(crate) fn header_read(&self) -> bool {
        self.len.is_some()
    }

    /// Whether the magic and strategy byte have been read and match (true
    /// also after a later length error).
    pub(crate) fn prefix_matches(&self) -> bool {
        self.buf.len() > 4 && self.buf[..4] == self.magic && self.buf[4] == self.strategy
    }

    /// Feed one bit.  Returns `Ok(true)` once the envelope is complete
    /// (further bits must not be pushed), or an error if the header does not
    /// match or declares an impossible length.
    pub(crate) fn push(&mut self, bit: u8) -> Result<bool> {
        debug_assert!(!self.is_done());
        self.cur |= (bit & 1) << self.nbits;
        self.nbits += 1;
        if self.nbits < 8 {
            return Ok(false);
        }
        self.buf.push(self.cur);
        self.cur = 0;
        self.nbits = 0;
        if self.len.is_none() && self.buf.len() == ENVELOPE_LEN {
            if self.buf[..4] != self.magic || self.buf[4] != self.strategy {
                return Err(not_found(self.what));
            }
            let len =
                u32::from_le_bytes([self.buf[5], self.buf[6], self.buf[7], self.buf[8]]) as usize;
            if len > self.max_len {
                return Err(SteganoError::IntegrityError(format!(
                    "Embedded length {len} exceeds {} carrier capacity of {} bytes \
                     (truncated or corrupted carrier)",
                    self.what, self.max_len
                )));
            }
            self.len = Some(len);
        }
        Ok(self.is_done())
    }

    /// The codec bytes, or an error if the envelope is incomplete.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        match self.len {
            None => Err(not_found(self.what)),
            Some(len) if self.buf.len() < ENVELOPE_LEN + len => {
                Err(SteganoError::IntegrityError(format!(
                    "{} payload truncated: header says {len} bytes, found {}",
                    self.what,
                    self.buf.len() - ENVELOPE_LEN
                )))
            }
            Some(_) => {
                let mut buf = self.buf;
                buf.drain(..ENVELOPE_LEN);
                Ok(buf)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(stream: &[u8], strategy: u8, max_len: usize) -> Result<Vec<u8>> {
        let mut r = EnvelopeReader::new(b"TEST", strategy, max_len, "test");
        for i in 0..stream.len() * 8 {
            if r.push(bit(stream, i))? {
                break;
            }
        }
        r.finish()
    }

    #[test]
    fn roundtrip_and_errors() {
        let s = build(b"TEST", 3, b"hello").unwrap();
        assert_eq!(read_all(&s, 3, 100).unwrap(), b"hello");
        assert!(read_all(&s, 4, 100).is_err()); // wrong strategy
        assert!(read_all(&s, 3, 4).is_err()); // length above capacity
        assert!(read_all(&s[..12], 3, 100).is_err()); // truncated
        assert!(read_all(&s[..4], 3, 100).is_err()); // no header
        let empty = build(b"TEST", 3, b"").unwrap();
        assert_eq!(read_all(&empty, 3, 0).unwrap(), b"");
    }

    #[test]
    fn text_check() {
        assert!(check_text("héllo\n".as_bytes(), 0, "x").is_ok());
        assert!(check_text(b"a\0b", 0, "x").is_err());
        assert!(check_text(b"\xff\xfe", 0, "x").is_err());
    }
}
