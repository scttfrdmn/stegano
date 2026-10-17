//! Helpers shared by the binary (sample-LSB) stream handlers, FITS and WAV.
//!
//! Both in-memory handlers use the same 10-byte bit-stream header,
//! `magic(4) ‖ len u32 LE ‖ strategy u8 ‖ version u8`, followed by the codec
//! bytes, written LSB first; [`FrameReader`] decodes it incrementally.

use crate::core::{Result, SteganoError};
use std::io::{self, BufRead, Read, Write};

/// Header length: magic(4) + length(4) + strategy(1) + version(1).
const HEADER_LEN: usize = 10;
/// Wire-format version stored in header byte 9.
const FORMAT_VERSION: u8 = 2;

/// Sequential carrier I/O with byte counters.
pub(crate) struct Io<'a> {
    reader: &'a mut dyn BufRead,
    writer: &'a mut dyn Write,
    pub(crate) bytes_read: u64,
    pub(crate) bytes_written: u64,
}

impl<'a> Io<'a> {
    pub(crate) fn new(reader: &'a mut dyn BufRead, writer: &'a mut dyn Write) -> Self {
        Self {
            reader,
            writer,
            bytes_read: 0,
            bytes_written: 0,
        }
    }

    /// Read until `buf` is full or EOF; returns the number of bytes read.
    pub(crate) fn read_full(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut n = 0;
        while n < buf.len() {
            match self.reader.read(&mut buf[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        self.bytes_read += n as u64;
        Ok(n)
    }

    pub(crate) fn write(&mut self, buf: &[u8]) -> Result<()> {
        self.writer.write_all(buf)?;
        self.bytes_written += buf.len() as u64;
        Ok(())
    }

    /// Copy up to `n` bytes unchanged; returns the number copied (less
    /// than `n` only at EOF).
    pub(crate) fn copy(&mut self, n: u64) -> Result<u64> {
        let copied = io::copy(&mut (&mut *self.reader).take(n), &mut *self.writer)?;
        self.bytes_read += copied;
        self.bytes_written += copied;
        Ok(copied)
    }

    /// Copy everything up to EOF unchanged.
    pub(crate) fn copy_rest(&mut self) -> Result<()> {
        self.copy(u64::MAX).map(drop)
    }
}

/// Outcome of feeding one bit to a [`FrameReader`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// More bits are needed.
    More,
    /// The first 10 bytes are not a header for this magic and strategy.
    NoHeader,
    /// The frame is complete.
    Done,
}

/// Incremental reader for the `magic ‖ len ‖ strategy ‖ version ‖ codec`
/// bit stream.
pub(crate) struct FrameReader {
    magic: [u8; 4],
    strategy: u8,
    /// Name used in the version error ("FITS", "signal").
    what: &'static str,
    buf: Vec<u8>,
    cur: u8,
    nbits: u32,
    total: Option<usize>,
}

impl FrameReader {
    pub(crate) fn new(magic: &[u8; 4], strategy: u8, what: &'static str) -> Self {
        Self {
            magic: *magic,
            strategy,
            what,
            buf: Vec::with_capacity(HEADER_LEN),
            cur: 0,
            nbits: 0,
            total: None,
        }
    }

    /// Feed one bit (must not be called again after `NoHeader`/`Done`).
    pub(crate) fn push(&mut self, bit: u8) -> Result<Step> {
        self.cur |= (bit & 1) << self.nbits;
        self.nbits += 1;
        if self.nbits < 8 {
            return Ok(Step::More);
        }
        self.buf.push(self.cur);
        self.cur = 0;
        self.nbits = 0;
        if self.total.is_none() && self.buf.len() == HEADER_LEN {
            if self.buf[..4] != self.magic || self.buf[8] != self.strategy {
                return Ok(Step::NoHeader);
            }
            if self.buf[9] != FORMAT_VERSION {
                return Err(SteganoError::IntegrityError(format!(
                    "unsupported {} payload version {} (expected {FORMAT_VERSION})",
                    self.what, self.buf[9]
                )));
            }
            self.total = Some(HEADER_LEN + self.declared_len());
        }
        Ok(if Some(self.buf.len()) == self.total {
            Step::Done
        } else {
            Step::More
        })
    }

    /// The payload length declared by the header (valid once it is read).
    pub(crate) fn declared_len(&self) -> usize {
        u32::from_le_bytes([self.buf[4], self.buf[5], self.buf[6], self.buf[7]]) as usize
    }

    /// Whether the header has been read and accepted.
    pub(crate) fn header_read(&self) -> bool {
        self.total.is_some()
    }

    /// The codec bytes of a complete frame.
    pub(crate) fn into_payload(mut self) -> Vec<u8> {
        self.buf.drain(..HEADER_LEN);
        self.buf
    }
}

/// Bit `i` (LSB first within each byte) of `bytes`.
pub(crate) fn bit(bytes: &[u8], i: usize) -> u8 {
    (bytes[i / 8] >> (i % 8)) & 1
}
