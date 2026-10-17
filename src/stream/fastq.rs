//! Streaming FASTQ quality-score embedding.
//!
//! Same scheme, eligibility rule, `GNSJ` envelope and byte-preservation
//! rules as the in-memory [`GenomicsHandler`](crate::handlers::genomics),
//! processed one 4-line record at a time.  The whole carrier is validated
//! during embedding exactly as the in-memory handler does, so streaming
//! embed succeeds iff the in-memory embed would, with identical output.

use super::{StreamHandler, StreamStats};
use crate::codec::{frame_len, MAX_HEADER_BLOCK};
use crate::core::{EmbedParams, Result, SteganoError};
use crate::handlers::genomics::{
    build_stream, check_fastq_header, check_fastq_record, content_len, not_found,
    parse_stream_header, GenomicsHandler, QualityEmbedStrategy, HEADER_BITS, HEADER_LEN,
};
use std::io::{BufRead, Write};

/// FASTQ stream handler (quality-score LSB).
pub(crate) struct FastqStream;

fn no_records() -> SteganoError {
    SteganoError::IntegrityError("No complete FASTQ records found".to_string())
}

/// Reads one record at a time into reusable line buffers, tracking the
/// state needed to reproduce [`locate_fastq`]'s rules for trailing blank
/// lines and a trailing incomplete record.
///
/// [`locate_fastq`]: crate::handlers::genomics
struct RecordReader<'a> {
    reader: &'a mut dyn BufRead,
    /// The four lines of the current record, terminators included.
    lines: [Vec<u8>; 4],
    /// 0-based index of the next line to be read.
    line_no: usize,
    /// Index of the first line of a run of blank lines at a record boundary.
    blank_run: Option<usize>,
    bytes_read: u64,
    peak: usize,
}

/// What the reader produced.
enum Item {
    /// A complete, validated record in `lines[0..4]`.
    Record,
    /// Pass-through bytes (blank line or trailing incomplete record) in
    /// `lines[0..n]`.
    PassThrough(usize),
    Eof,
}

impl<'a> RecordReader<'a> {
    fn new(reader: &'a mut dyn BufRead) -> Self {
        Self {
            reader,
            lines: Default::default(),
            line_no: 0,
            blank_run: None,
            bytes_read: 0,
            peak: 0,
        }
    }

    /// Read one line (with terminator) into `lines[i]`; `false` at EOF.
    fn read_line(&mut self, i: usize) -> Result<bool> {
        self.lines[i].clear();
        let n = self.reader.read_until(b'\n', &mut self.lines[i])?;
        self.bytes_read += n as u64;
        if n > 0 {
            self.line_no += 1;
        }
        Ok(n > 0)
    }

    fn content(&self, i: usize) -> &[u8] {
        &self.lines[i][..content_len(&self.lines[i])]
    }

    fn next(&mut self) -> Result<Item> {
        let item = self.next_inner()?;
        let held: usize = self.lines.iter().map(Vec::capacity).sum();
        self.peak = self.peak.max(held);
        Ok(item)
    }

    fn next_inner(&mut self) -> Result<Item> {
        let first = self.line_no;
        if !self.read_line(0)? {
            return Ok(Item::Eof);
        }
        if self.content(0).is_empty() {
            // Blank line at a record boundary: valid only if every remaining
            // line is blank too (checked when a non-blank line shows up).
            self.blank_run.get_or_insert(first);
            return Ok(Item::PassThrough(1));
        }
        if let Some(b) = self.blank_run {
            return check_fastq_header(b, b"").map(|_| Item::Eof);
        }
        check_fastq_header(first, self.content(0))?;
        for i in 1..4 {
            if !self.read_line(i)? {
                return Ok(Item::PassThrough(i)); // trailing incomplete record
            }
        }
        check_fastq_record(first, self.content(1), self.content(2), self.content(3))?;
        Ok(Item::Record)
    }

    fn write(&self, writer: &mut dyn Write, n: usize, written: &mut u64) -> Result<()> {
        for l in &self.lines[..n] {
            writer.write_all(l)?;
            *written += l.len() as u64;
        }
        Ok(())
    }
}

/// Bit `i` (LSB-first within each byte) of `bytes`.
fn bit(bytes: &[u8], i: usize) -> u8 {
    (bytes[i / 8] >> (i % 8)) & 1
}

impl StreamHandler for FastqStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        GenomicsHandler::fastq_strategy(params).map(|_| ())
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        let strategy = GenomicsHandler::fastq_strategy(params)?;
        let stream = build_stream(strategy.to_u8(), encoded)?;
        let k = strategy.bits_per_char() as usize;
        let total_bits = stream.len() * 8;
        let mut pos = 0usize; // next stream bit to write
        let mut available = 0usize; // capacity seen so far, in bits
        let mut records = 0usize;
        let mut written = 0u64;

        let mut rr = RecordReader::new(reader);
        loop {
            match rr.next()? {
                Item::Eof => break,
                Item::PassThrough(n) => rr.write(writer, n, &mut written)?,
                Item::Record => {
                    records += 1;
                    if pos < total_bits {
                        let qlen = content_len(&rr.lines[3]);
                        for q in &mut rr.lines[3][..qlen] {
                            if !strategy.is_eligible(*q) {
                                continue;
                            }
                            available += k;
                            if pos < total_bits {
                                let n = k.min(total_bits - pos);
                                let v = (0..n).fold(0u8, |acc, j| acc | bit(&stream, pos + j) << j);
                                *q = strategy.embed_char(*q, v, n);
                                pos += n;
                            }
                        }
                    }
                    rr.write(writer, 4, &mut written)?;
                }
            }
        }
        if records == 0 {
            return Err(no_records());
        }
        if pos < total_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: total_bits,
                available,
            });
        }
        Ok(StreamStats {
            bytes_read: rr.bytes_read,
            bytes_written: written,
            peak_buffer_bytes: rr.peak + stream.len(),
        })
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, _params: &EmbedParams) -> Result<Vec<u8>> {
        extract(reader).map(|(raw, _)| raw)
    }
}

/// Accumulates bits read under one candidate strategy.
struct Candidate {
    strategy: QualityEmbedStrategy,
    bytes: Vec<u8>,
    nbits: usize,
}

impl Candidate {
    fn push_char(&mut self, q: u8) {
        for j in 0..self.strategy.bits_per_char() {
            if self.nbits.is_multiple_of(8) {
                self.bytes.push(0);
            }
            *self.bytes.last_mut().expect("pushed") |= ((q >> j) & 1) << (self.nbits % 8);
            self.nbits += 1;
        }
    }

    /// Complete bytes accumulated so far.
    fn full_bytes(&self) -> usize {
        self.nbits / 8
    }
}

/// Extract the codec bytes; also returns the carrier bytes consumed.
///
/// Strategy detection mirrors the in-memory handler: Conservative, Balanced
/// and Aggressive are tried in that order, and the first whose stream header
/// (magic + strategy byte) matches wins.  All candidates accumulate bits in
/// parallel until that is decided, so the carrier is read once.
pub(crate) fn extract(reader: &mut dyn BufRead) -> Result<(Vec<u8>, u64)> {
    let mut cands: Vec<Candidate> = QualityEmbedStrategy::ALL
        .into_iter()
        .map(|strategy| Candidate {
            strategy,
            bytes: Vec::new(),
            nbits: 0,
        })
        .collect();
    let mut selected = false;
    // Number of stream bytes (header + codec bytes) needed once selected.
    let mut target = usize::MAX;
    let mut frame_checked = false;
    let mut records = 0usize;

    let mut rr = RecordReader::new(reader);
    loop {
        let eof = match rr.next()? {
            Item::Eof => true,
            Item::PassThrough(_) => continue,
            Item::Record => {
                records += 1;
                let qlen = content_len(&rr.lines[3]);
                for &q in &rr.lines[3][..qlen] {
                    for c in cands.iter_mut() {
                        if c.strategy.is_eligible(q) {
                            c.push_char(q);
                        }
                    }
                }
                false
            }
        };
        if eof && records == 0 {
            return Err(no_records());
        }

        if !selected {
            // Resolve candidates in priority order.  At EOF a candidate
            // without a complete header has no stream (as in memory).
            while let Some(c) = cands.first() {
                if c.nbits < HEADER_BITS {
                    if eof {
                        cands.remove(0);
                        continue;
                    }
                    break; // highest-priority candidate still undecided
                }
                match parse_stream_header(&c.bytes, c.strategy.to_u8()) {
                    Some(len) => {
                        cands.truncate(1);
                        selected = true;
                        target = HEADER_LEN.saturating_add(len);
                        break;
                    }
                    None => {
                        cands.remove(0);
                    }
                }
            }
            if cands.is_empty() {
                return Err(not_found());
            }
        }
        if selected {
            let c = &cands[0];
            let have = c.full_bytes();
            // Once enough of the codec frame is in, its own header gives the
            // exact length; stop there (decode_payload ignores trailing bytes).
            if !frame_checked {
                let need = MAX_HEADER_BLOCK.min(target - HEADER_LEN);
                if have >= HEADER_LEN + need {
                    frame_checked = true;
                    match frame_len(&c.bytes[HEADER_LEN..HEADER_LEN + need]) {
                        Some(n) => target = target.min(HEADER_LEN.saturating_add(n)),
                        // Not a valid codec frame: hand what we have to the
                        // decoder, which reports the precise error.
                        None => target = HEADER_LEN + need,
                    }
                }
            }
            if have >= target {
                let raw = c.bytes[HEADER_LEN..target].to_vec();
                return Ok((raw, rr.bytes_read));
            }
        }
        if eof {
            break;
        }
    }

    Err(SteganoError::IntegrityError(
        "Carrier ended before the payload (truncated or corrupted carrier)".to_string(),
    ))
}
