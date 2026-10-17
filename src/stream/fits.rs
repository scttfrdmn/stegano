//! Streaming FITS sample-LSB embedding.
//!
//! Same HDU rules, target selection, `BLANK`/NaN skipping, strategies
//! (`fits_strategy` = `pixel_lsb` or `float_mantissa`) and `FTSJ` header as
//! the in-memory [`FitsHandler`](crate::handlers::fits::FitsHandler).  The
//! file is processed one 2880-byte header block, or one bounded chunk of a
//! data unit, at a time: header blocks, other HDUs, padding and trailing
//! bytes are copied through unchanged, and only the target HDU's samples are
//! modified.  Every header is parsed and validated as it passes, so a
//! malformed file is an error exactly as in memory, and the output is
//! byte-identical to the in-memory handler's for the same encoded payload.
//!
//! Extraction reads HDUs in file order and stops at the end of the payload.
//! The in-memory extractor tries the first integer image HDU, then the first
//! float image HDU; the stream extractor takes whichever of the two comes
//! first in the file and carries a matching header.  The two only differ
//! for a file with a payload in *both* HDUs.

use super::binary::{bit, FrameReader, Io, Step};
use super::{StreamHandler, StreamStats};
use crate::core::{EmbedParams, Result, SteganoError};
use crate::handlers::fits::{
    build_stream, malformed, CardReader, FitsHandler, FitsStrategy, Hdu, BLOCK, MAGIC,
};
use std::io::{BufRead, Write};

/// FITS stream handler.
pub(crate) struct FitsStream;

/// Data-unit chunk size: a multiple of every sample width (1, 2, 4, 8).
pub(super) const CHUNK: usize = 64 * BLOCK;

/// Walks the HDUs of a FITS stream, passing header blocks through.
struct Walker<'a> {
    io: Io<'a>,
    /// File offset of the next unread byte.
    pos: usize,
    primary: bool,
    done: bool,
    block: Vec<u8>,
    chunk: Vec<u8>,
}

impl<'a> Walker<'a> {
    fn new(reader: &'a mut dyn BufRead, writer: &'a mut dyn Write) -> Self {
        Self {
            io: Io::new(reader, writer),
            pos: 0,
            primary: true,
            done: false,
            block: vec![0; BLOCK],
            chunk: Vec::new(),
        }
    }

    fn read(&mut self, at: usize, len: usize) -> Result<usize> {
        let n = self.io.read_full(&mut self.block[at..at + len])?;
        self.pos += n;
        Ok(n)
    }

    /// Parse (and copy through) the next HDU's header.  Returns `None` at
    /// the end of the HDU list, after copying any trailing bytes.
    fn next_hdu(&mut self) -> Result<Option<Hdu>> {
        if self.done {
            return Ok(None);
        }
        let start = self.pos;
        let n = self.read(0, 8)?;
        let keyword: &[u8; 8] = if self.primary {
            b"SIMPLE  "
        } else {
            b"XTENSION"
        };
        if n < 8 || &self.block[..8] != keyword {
            if self.primary {
                return Err(malformed("missing SIMPLE keyword at start of file"));
            }
            // FITS special records or other trailing bytes: pass through.
            self.io.write(&self.block[..n])?;
            self.io.copy_rest()?;
            self.done = true;
            return Ok(None);
        }
        let mut cards = CardReader::new(start);
        let mut filled = 8;
        loop {
            filled += self.read(filled, BLOCK - filled)?;
            if filled < BLOCK {
                return Err(malformed(format!(
                    "truncated header at offset {}",
                    self.pos - filled
                )));
            }
            let end = cards.feed(&self.block)?;
            self.io.write(&self.block)?;
            filled = 0;
            if end {
                break;
            }
        }
        let hdu = Hdu::from_cards(&cards.finish(), start, self.pos, self.primary)?;
        self.primary = false;
        Ok(Some(hdu))
    }

    /// Stream `hdu`'s data unit, calling `f` on each chunk (a whole number
    /// of samples) before writing it, then copy its padding.  If `f`
    /// returns `true`, the rest of the data unit is left unread and
    /// `Ok(true)` is returned.
    fn data(&mut self, hdu: &Hdu, mut f: impl FnMut(&mut [u8]) -> Result<bool>) -> Result<bool> {
        let mut left = hdu.data_len;
        if self.chunk.len() < CHUNK.min(left) {
            self.chunk.resize(CHUNK.min(left), 0);
        }
        while left > 0 {
            let want = left.min(CHUNK);
            let n = self.io.read_full(&mut self.chunk[..want])?;
            self.pos += n;
            if n < want {
                return Err(malformed(format!(
                    "data unit at offset {} declares {} bytes but only {} remain",
                    hdu.data_start,
                    hdu.data_len,
                    self.pos - hdu.data_start
                )));
            }
            left -= n;
            let chunk = &mut self.chunk[..n];
            if f(chunk)? {
                return Ok(true);
            }
            self.io.write(chunk)?;
        }
        // Padding (the in-memory parser tolerates a truncated final padding).
        let pad = (hdu.next_start - hdu.data_start - hdu.data_len) as u64;
        let copied = self.io.copy(pad)?;
        self.pos += copied as usize;
        Ok(false)
    }

    fn peak(&self) -> usize {
        self.block.capacity() + self.chunk.capacity()
    }
}

/// Call `f` with each usable sample's last (least significant) byte.
fn for_usable(
    chunk: &mut [u8],
    hdu: &Hdu,
    mut f: impl FnMut(&mut u8) -> Result<bool>,
) -> Result<bool> {
    let w = hdu.width();
    for sample in chunk.chunks_exact_mut(w) {
        if FitsHandler::usable(sample, hdu.bitpix, hdu.blank) && f(&mut sample[w - 1])? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl StreamHandler for FitsStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        FitsHandler::select_strategy(params).map(drop)
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        let strategy = FitsHandler::select_strategy(params)?;
        let stream = build_stream(strategy, encoded)?;
        let needed = stream.len() * 8;
        let mut walker = Walker::new(reader, writer);
        let (mut found, mut float_bitpix) = (false, None);
        let (mut written, mut available) = (0usize, 0usize);
        while let Some(hdu) = walker.next_hdu()? {
            if !found && hdu.is_target(strategy) {
                found = true;
                walker.data(&hdu, |chunk| {
                    for_usable(chunk, &hdu, |lsb| {
                        available += 1;
                        if written < needed {
                            *lsb = (*lsb & 0xFE) | bit(&stream, written);
                            written += 1;
                        }
                        Ok(false)
                    })
                })?;
            } else {
                if float_bitpix.is_none() && hdu.is_target(FitsStrategy::FloatMantissa) {
                    float_bitpix = Some(hdu.bitpix);
                }
                walker.data(&hdu, |_| Ok(false))?;
            }
        }
        if !found {
            return Err(FitsHandler::no_target(strategy, float_bitpix));
        }
        if written < needed {
            return Err(SteganoError::InsufficientCapacity { needed, available });
        }
        Ok(StreamStats {
            bytes_read: walker.io.bytes_read,
            bytes_written: walker.io.bytes_written,
            peak_buffer_bytes: walker.peak() + stream.len(),
        })
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, _params: &EmbedParams) -> Result<Vec<u8>> {
        extract(reader).map(|(raw, _)| raw)
    }
}

/// Extract the codec bytes; also returns the carrier bytes consumed.
pub(crate) fn extract(reader: &mut dyn BufRead) -> Result<(Vec<u8>, u64)> {
    let mut sink = std::io::sink();
    let mut walker = Walker::new(reader, &mut sink);
    let mut tried = [false; 2];
    while let Some(hdu) = walker.next_hdu()? {
        let Some(strategy) = FitsStrategy::ALL
            .into_iter()
            .find(|&s| !tried[s.to_u8() as usize] && hdu.is_target(s))
        else {
            walker.data(&hdu, |_| Ok(false))?;
            continue;
        };
        tried[strategy.to_u8() as usize] = true;
        let mut frame = Some(FrameReader::new(MAGIC, strategy.to_u8(), "FITS"));
        let mut done = false;
        let mut cap = 0usize;
        let stopped = walker.data(&hdu, |chunk| {
            for_usable(chunk, &hdu, |lsb| {
                cap += 1;
                let Some(fr) = frame.as_mut() else {
                    return Ok(false);
                };
                match fr.push(*lsb & 1)? {
                    Step::More => Ok(false),
                    Step::NoHeader => {
                        frame = None;
                        Ok(false)
                    }
                    Step::Done => {
                        done = true;
                        Ok(true)
                    }
                }
            })
        })?;
        match frame {
            Some(fr) if done && stopped => return Ok((fr.into_payload(), walker.io.bytes_read)),
            Some(fr) if fr.header_read() => {
                return Err(SteganoError::IntegrityError(format!(
                    "declared FITS payload length {} exceeds carrier capacity ({cap} bits)",
                    fr.declared_len()
                )))
            }
            _ => {}
        }
    }
    Err(SteganoError::InvalidParameter(
        "No FITS steganographic payload found".to_string(),
    ))
}
