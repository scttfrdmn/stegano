//! Streaming WAV time-domain LSB embedding.
//!
//! Same RIFF walk, `fmt ` rules (PCM 8/16/24/32, extensible with
//! `wValidBitsPerSample`, IEEE float 32/64), sample LSB rules and `SGSJ`
//! header as the in-memory
//! [`SignalHandler`](crate::handlers::signals::SignalHandler) with
//! `signal_strategy = "timedomain"` (or `StealthLevel::Low`).  The RIFF
//! header and every chunk other than `data` are copied through unchanged
//! (the file length does not change, so the RIFF and chunk size fields stay
//! valid); the `data` chunk is processed in bounded, frame-aligned buffers.
//! Output is byte-identical to the in-memory handler's for the same encoded
//! payload, and float carriers with a NaN/Inf sample are rejected as in
//! memory.
//!
//! # Why only `timedomain`
//!
//! * `frequency` and `phase` work on 1024-frame FFT chunks, but the bit
//!   order is channel-major (all of channel 0's chunks, then channel 1's),
//!   so for multichannel audio the bits for the first frames depend on the
//!   carrier's total length.  That is global state; a single pass cannot
//!   place them.
//! * `spread` and `echo` have the same channel-major order, and the
//!   in-memory embed self-verifies by re-extracting from the whole quantised
//!   output before committing, failing (with no output) on carriers that
//!   cannot hold the bits.
//!
//! These strategies are therefore not streamable ([`is_streamable`] is
//! false).  Stream extraction recognises a `timedomain` header; for any
//! other WAV payload it returns the "not streamable" error, so callers such
//! as the CLI fall back to the in-memory extractor.
//!
//! Unlike in memory, a `data` chunk that precedes the `fmt ` chunk is not
//! supported (the sample format is not known yet); it returns the "not
//! streamable" error.  Stream extraction stops at the end of the payload, so
//! it does not check float samples after that point.
//!
//! [`is_streamable`]: super::is_streamable

use super::binary::{bit, FrameReader, Io, Step};
use super::{StreamHandler, StreamStats};
use crate::core::{EmbedParams, Result, SteganoError};
use crate::handlers::signals::{
    build_stream, le_u32, SignalEmbedStrategy, SignalHandler, SignalLayout, WavFmt, FMT_PREFIX,
    MAGIC,
};
use std::io::{BufRead, Write};

/// WAV stream handler (time-domain LSB).
pub(crate) struct WavStream;

/// Target `data` buffer size; rounded down to whole frames.
const CHUNK: usize = 256 * 1024;

const STRATEGY: SignalEmbedStrategy = SignalEmbedStrategy::TimeDomainLSB;

fn invalid(msg: impl Into<String>) -> SteganoError {
    SteganoError::InvalidParameter(msg.into())
}

fn not_streamable(why: &str) -> SteganoError {
    invalid(format!(
        "{why}; this WAV is not streamable; use the in-memory API"
    ))
}

/// Walk the RIFF chunks up to the `data` chunk, copying everything through.
/// Returns the parsed `fmt ` chunk and the declared `data` size; the `data`
/// chunk header has been copied.
fn walk_to_data(io: &mut Io<'_>) -> Result<(WavFmt, u64)> {
    let mut riff = [0u8; 12];
    let n = io.read_full(&mut riff)?;
    if n < 12 || &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(invalid("WAV carrier is not a RIFF/WAVE file"));
    }
    io.write(&riff)?;

    // First `fmt ` chunk: its first FMT_PREFIX bytes and declared size.
    let mut fmt: Option<(Vec<u8>, usize)> = None;
    loop {
        let mut hdr = [0u8; 8];
        let n = io.read_full(&mut hdr)?;
        io.write(&hdr[..n])?;
        if n < 8 {
            return Err(invalid(if fmt.is_none() {
                "WAV file has no fmt chunk"
            } else {
                "WAV file has no data chunk"
            }));
        }
        let size = le_u32(&hdr, 4) as u64;
        if &hdr[0..4] == b"data" {
            let Some((body, len)) = fmt else {
                return Err(not_streamable("WAV data chunk precedes the fmt chunk"));
            };
            return Ok((SignalLayout::parse_fmt(&body, len)?, size));
        }
        let mut copied = 0;
        if &hdr[0..4] == b"fmt " && fmt.is_none() {
            let mut body = vec![0u8; (size as usize).min(FMT_PREFIX)];
            let k = io.read_full(&mut body)?;
            io.write(&body[..k])?;
            copied = k as u64;
            fmt = Some((body, size as usize));
        }
        if copied + io.copy(size - copied)? < size {
            return Err(invalid("WAV chunk extends past end of file"));
        }
        // RIFF pad byte for odd sizes (may be missing at EOF).
        io.copy(size & 1)?;
    }
}

/// Stream the `data` chunk payload (`size` bytes, clamped at EOF) in
/// frame-aligned buffers, calling `f` with each buffer's layout and bytes
/// before writing them.  A trailing partial frame is passed to the writer
/// untouched.  If `f` returns `true`, streaming stops (nothing more is read
/// or written).
fn stream_data(
    io: &mut Io<'_>,
    fmt: &WavFmt,
    size: u64,
    peak: &mut usize,
    mut f: impl FnMut(&SignalLayout, &mut [u8]) -> Result<bool>,
) -> Result<bool> {
    let frame = fmt.block_align;
    let cap = (CHUNK / frame).max(1) * frame;
    let mut buf = vec![0u8; (cap as u64).min(size) as usize];
    *peak = (*peak).max(buf.capacity());
    let mut left = size;
    while left > 0 {
        let want = (cap as u64).min(left) as usize;
        let n = io.read_full(&mut buf[..want])?;
        left -= n as u64;
        let whole = n - n % frame;
        let layout = SignalLayout::for_buffer(fmt, whole);
        layout.check_finite(&buf[..whole])?;
        if f(&layout, &mut buf[..whole])? {
            return Ok(true);
        }
        io.write(&buf[..n])?;
        if n < want {
            break; // EOF: data chunk clamped to the file.
        }
    }
    Ok(false)
}

impl StreamHandler for WavStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        match SignalHandler::select_strategy(params)? {
            STRATEGY => Ok(()),
            _ => Err(super::not_streamable(&crate::DataFormat::WAV, params)),
        }
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        _params: &EmbedParams,
    ) -> Result<StreamStats> {
        let stream = build_stream(STRATEGY, encoded)?;
        let needed = stream.len() * 8;
        let mut io = Io::new(reader, writer);
        let (fmt, size) = walk_to_data(&mut io)?;
        let (mut written, mut available, mut peak) = (0usize, 0usize, 0usize);
        stream_data(&mut io, &fmt, size, &mut peak, |layout, buf| {
            let n = layout.num_samples();
            available += n;
            for i in 0..n {
                if written == needed {
                    break;
                }
                layout.write_lsb(buf, i, bit(&stream, written));
                written += 1;
            }
            Ok(false)
        })?;
        // Everything after the data chunk is left alone.
        io.copy_rest()?;
        if written < needed {
            return Err(SteganoError::InsufficientCapacity { needed, available });
        }
        Ok(StreamStats {
            bytes_read: io.bytes_read,
            bytes_written: io.bytes_written,
            peak_buffer_bytes: peak + stream.len(),
        })
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, _params: &EmbedParams) -> Result<Vec<u8>> {
        extract(reader).map(|(raw, _)| raw)
    }
}

/// Extract the codec bytes; also returns the carrier bytes consumed.
pub(crate) fn extract(reader: &mut dyn BufRead) -> Result<(Vec<u8>, u64)> {
    let mut sink = std::io::sink();
    let mut io = Io::new(reader, &mut sink);
    let (fmt, size) = walk_to_data(&mut io)?;
    let mut frame = FrameReader::new(MAGIC, STRATEGY.to_u8(), "signal");
    let (mut cap, mut peak) = (0usize, 0usize);
    let mut no_header = false;
    let done = stream_data(&mut io, &fmt, size, &mut peak, |layout, buf| {
        for i in 0..layout.num_samples() {
            cap += 1;
            match frame.push(layout.read_lsb(buf, i))? {
                Step::More => {}
                Step::NoHeader => {
                    no_header = true;
                    return Ok(true);
                }
                Step::Done => return Ok(true),
            }
        }
        Ok(false)
    })?;
    if done && !no_header {
        return Ok((frame.into_payload(), io.bytes_read));
    }
    if frame.header_read() {
        return Err(SteganoError::IntegrityError(format!(
            "declared payload length {} exceeds carrier capacity ({cap} bits)",
            frame.declared_len()
        )));
    }
    Err(not_streamable("No timedomain payload header in this WAV"))
}
