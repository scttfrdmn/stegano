//! Streaming SAM/BAM embedding (`sam_strategy` = `auxiliary` or `quality`).
//!
//! Same envelope (`SBM\x01` + strategy + length), channels, validation and
//! byte-preservation rules as the in-memory
//! [`SamBamHandler`](crate::handlers::sam_bam::SamBamHandler), processed one
//! alignment record at a time:
//!
//! * **SAM** is read line by line; header and blank lines pass through.
//! * **BAM** is read block by block (BGZF, or a raw `BAM\1` stream), the
//!   decompressed bytes are parsed into records across block boundaries, and
//!   the output is re-blocked exactly as the in-memory compressor does it
//!   (`0xff00`-byte chunks, then the EOF marker block), or written raw with
//!   `bam_compression` = `none`.
//!
//! Memory is bounded by one BGZF block in each direction plus the largest
//! record (and the BAM header), independent of the file size.
//!
//! Embedding validates the whole carrier exactly as the in-memory handler
//! does, so a streaming embed succeeds iff the in-memory one would, with
//! byte-identical output for the same encoded payload.  The one exception:
//! a carrier that already carries `zs:Z` tags whose chunk indices are not in
//! file order cannot be checked for an existing payload in one pass, and is
//! rejected with [`SteganoError::InvalidParameter`] by the `quality`
//! strategy (and by extraction).
//!
//! Extraction auto-detects the channel (`zs` tags, `@CO` comment or quality
//! LSBs) and stops reading as soon as the envelope is complete.  If a
//! carrier holds more than one envelope (neither API produces such files)
//! the first one completed in file order wins, whereas the in-memory
//! handler prefers tags, then the comment, then quality values.

use super::{not_streamable, StreamHandler, StreamStats};
use crate::codec::{frame_len, MAX_HEADER_BLOCK};
use crate::core::{DataFormat, EmbedParams, Result, SteganoError};
use crate::handlers::sam_bam::{
    bam_compression_from_params, bam_header_lines, bgzf_header_at, bgzf_inflate_at,
    bgzf_write_block, build_envelope, comment_envelope, env_bit, insufficient, le_u32,
    next_bam_record, parse_bam_header, parse_envelope, parse_envelope_header, parse_sam_line,
    parse_tag_chunk, set_qual_lsb, strategy_from_params, tag_chunk_value, write_bam_header,
    AlignmentRecord, BamCompression, BamRecord, QualBits, SamBamEmbedStrategy, SamLine, SamRecord,
    BAM_MAGIC, BAM_QUAL_RANGE, BGZF_BLOCK_DATA, BGZF_EOF, ENV_HEADER_LEN, MAX_CHUNK,
    SAM_QUAL_RANGE,
};
use std::io::{self, BufRead, Cursor, Read, Write};

/// SAM or BAM stream handler.
pub(crate) struct SamBamStream {
    bam: bool,
}

pub(crate) static SAM_STREAM: SamBamStream = SamBamStream { bam: false };
pub(crate) static BAM_STREAM: SamBamStream = SamBamStream { bam: true };

/// Read granularity for length-prefixed fields (bounds allocation by the
/// bytes actually present, not by a possibly bogus length).
const READ_STEP: usize = 64 * 1024;

fn integrity(msg: &str) -> SteganoError {
    SteganoError::IntegrityError(msg.to_string())
}

fn not_found() -> SteganoError {
    integrity("No steganographic data found in SAM/BAM carrier")
}

fn unordered_tags() -> SteganoError {
    SteganoError::InvalidParameter(
        "Carrier has 'zs' tag chunks out of index order, which cannot be checked in a single \
         pass; use the in-memory API"
            .into(),
    )
}

impl SamBamStream {
    fn format(&self) -> DataFormat {
        if self.bam {
            DataFormat::BAM
        } else {
            DataFormat::SAM
        }
    }

    fn strategy(&self, params: &EmbedParams) -> Result<SamBamEmbedStrategy> {
        match strategy_from_params(params)? {
            s @ (SamBamEmbedStrategy::AuxiliaryData | SamBamEmbedStrategy::QualityScores) => Ok(s),
            _ => Err(not_streamable(&self.format(), params)),
        }
    }

    fn open<'a>(&self, reader: &'a mut dyn BufRead) -> Result<Box<dyn Source + 'a>> {
        Ok(if self.bam {
            Box::new(BamSource::open(reader)?)
        } else {
            Box::new(SamSource::new(reader))
        })
    }
}

// ---------------------------------------------------------------------------
// Byte sources (raw or BGZF-decompressed)
// ---------------------------------------------------------------------------

/// Sequential byte input.
trait ByteSource {
    /// Fill `buf`; returns fewer bytes only at EOF.
    fn read_full(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Bytes held in internal buffers.
    fn held(&self) -> usize {
        0
    }
}

/// Append up to `n` bytes from `src` to `out`; fewer only at EOF.
fn read_append(src: &mut (impl ByteSource + ?Sized), n: usize, out: &mut Vec<u8>) -> Result<usize> {
    let mut done = 0;
    while done < n {
        let step = (n - done).min(READ_STEP);
        let start = out.len();
        out.resize(start + step, 0);
        let got = src.read_full(&mut out[start..])?;
        out.truncate(start + got);
        done += got;
        if got < step {
            break;
        }
    }
    Ok(done)
}

/// An uncompressed source.
struct Raw<R>(R);

impl<R: Read> ByteSource for Raw<R> {
    fn read_full(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut filled = 0;
        while filled < buf.len() {
            match self.0.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(filled)
    }
}

/// Decompresses a BGZF stream one block at a time.
struct Bgzf<R> {
    inner: Raw<R>,
    /// Current compressed block.
    cblock: Vec<u8>,
    /// Current decompressed block and read position.
    block: Vec<u8>,
    pos: usize,
    /// Compressed offset of the next block (for error messages).
    off: usize,
}

impl<R: Read> Bgzf<R> {
    fn new(inner: R) -> Self {
        Self {
            inner: Raw(inner),
            cblock: Vec::new(),
            block: Vec::new(),
            pos: 0,
            off: 0,
        }
    }

    /// Load the next block; `false` at a clean EOF.
    fn next_block(&mut self) -> Result<bool> {
        let off = self.off;
        self.cblock.clear();
        let n = read_append(&mut self.inner, 12, &mut self.cblock)?;
        if n == 0 {
            return Ok(false);
        }
        if n == 12 && self.cblock[..4] == [0x1f, 0x8b, 8, 4] {
            let xlen = u16::from_le_bytes([self.cblock[10], self.cblock[11]]) as usize;
            read_append(&mut self.inner, xlen, &mut self.cblock)?;
        }
        // Errors here (truncated / not BGZF) are the in-memory ones.
        let (total, _) = bgzf_header_at(&self.cblock, off)?;
        let rest = total - self.cblock.len();
        read_append(&mut self.inner, rest, &mut self.cblock)?;
        let (len, data) = bgzf_inflate_at(&self.cblock, off)?;
        self.off += len;
        self.block = data;
        self.pos = 0;
        Ok(true)
    }
}

impl<R: Read> ByteSource for Bgzf<R> {
    fn read_full(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut filled = 0;
        while filled < buf.len() {
            if self.pos == self.block.len() {
                if !self.next_block()? {
                    break;
                }
                continue;
            }
            let n = (buf.len() - filled).min(self.block.len() - self.pos);
            buf[filled..filled + n].copy_from_slice(&self.block[self.pos..self.pos + n]);
            self.pos += n;
            filled += n;
        }
        Ok(filled)
    }

    fn held(&self) -> usize {
        self.cblock.capacity() + self.block.capacity()
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Output sink: plain, or BGZF with the in-memory compressor's blocking.
enum Out<'a> {
    Plain(&'a mut dyn Write),
    Bgzf {
        inner: &'a mut dyn Write,
        /// Pending uncompressed bytes (< `BGZF_BLOCK_DATA`).
        buf: Vec<u8>,
        block: Vec<u8>,
    },
}

impl<'a> Out<'a> {
    fn new(inner: &'a mut dyn Write, bgzf: bool) -> Self {
        if bgzf {
            Out::Bgzf {
                inner,
                buf: Vec::with_capacity(BGZF_BLOCK_DATA),
                block: Vec::new(),
            }
        } else {
            Out::Plain(inner)
        }
    }

    fn emit_block(inner: &mut dyn Write, buf: &mut Vec<u8>, block: &mut Vec<u8>) -> Result<()> {
        block.clear();
        bgzf_write_block(buf, block)?;
        inner.write_all(block)?;
        buf.clear();
        Ok(())
    }

    /// Write any partial block and the BGZF EOF marker.
    fn finish(&mut self) -> Result<()> {
        if let Out::Bgzf { inner, buf, block } = self {
            if !buf.is_empty() {
                Self::emit_block(&mut **inner, buf, block)?;
            }
            inner.write_all(&BGZF_EOF)?;
        }
        Ok(())
    }

    fn held(&self) -> usize {
        match self {
            Out::Plain(_) => 0,
            Out::Bgzf { buf, block, .. } => buf.capacity() + block.capacity(),
        }
    }
}

impl Write for Out<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self {
            Out::Plain(w) => w.write(data),
            Out::Bgzf { inner, buf, block } => {
                let n = (BGZF_BLOCK_DATA - buf.len()).min(data.len());
                buf.extend_from_slice(&data[..n]);
                if buf.len() == BGZF_BLOCK_DATA {
                    Self::emit_block(&mut **inner, buf, block)
                        .map_err(|e| io::Error::other(e.to_string()))?;
                }
                Ok(n)
            }
        }
    }

    /// Flushes the underlying writer only: a partial block is never emitted
    /// early, so block boundaries match the in-memory compressor.
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Out::Plain(w) => w.flush(),
            Out::Bgzf { inner, .. } => inner.flush(),
        }
    }
}

struct CountingReader<'a> {
    inner: &'a mut dyn BufRead,
    n: u64,
}

impl Read for CountingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.n += n as u64;
        Ok(n)
    }
}

impl BufRead for CountingReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.n += amt as u64;
        self.inner.consume(amt);
    }
}

struct CountingWriter<'a> {
    inner: &'a mut dyn Write,
    n: u64,
}

impl Write for CountingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.n += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// ---------------------------------------------------------------------------
// Record sources
// ---------------------------------------------------------------------------

enum Item {
    Eof,
    /// Pass-through bytes (SAM header or blank line).
    Other,
    Record,
}

/// A SAM or BAM file read one item at a time.
trait Source {
    fn next(&mut self) -> Result<Item>;
    /// The current record (after [`Item::Record`]).
    fn record(&self) -> &dyn AlignmentRecord;
    fn record_mut(&mut self) -> &mut dyn AlignmentRecord;
    /// First well-formed `@CO\tzs:` envelope among the header lines so far.
    fn comment(&self) -> Option<&[u8]>;
    fn qual_range(&self) -> (u8, u8);
    /// Write what precedes the first item (the BAM header).
    fn write_prelude(&self, out: &mut dyn Write) -> Result<()>;
    /// Write the current item (possibly modified).
    fn write_current(&self, out: &mut dyn Write) -> Result<()>;
    fn held(&self) -> usize;
}

struct SamSource<'a> {
    reader: &'a mut dyn BufRead,
    /// Current line, terminator included.
    line: Vec<u8>,
    lineno: usize,
    seen_record: bool,
    /// Current record, with whether its line ended in `\n`.
    rec: Option<(SamRecord, bool)>,
    comment: Option<Vec<u8>>,
}

impl<'a> SamSource<'a> {
    fn new(reader: &'a mut dyn BufRead) -> Self {
        Self {
            reader,
            line: Vec::new(),
            lineno: 0,
            seen_record: false,
            rec: None,
            comment: None,
        }
    }
}

impl Source for SamSource<'_> {
    fn next(&mut self) -> Result<Item> {
        self.line.clear();
        self.rec = None;
        if self.reader.read_until(b'\n', &mut self.line)? == 0 {
            if self.lineno == 0 {
                return Err(integrity("Empty SAM file"));
            }
            return Ok(Item::Eof);
        }
        self.lineno += 1;
        let nl = self.line.ends_with(b"\n");
        let raw = &self.line[..self.line.len() - usize::from(nl)];
        match parse_sam_line(raw, self.lineno, &mut self.seen_record)? {
            SamLine::Header(h) => {
                if self.comment.is_none() {
                    self.comment = comment_envelope(h.strip_suffix(b"\r").unwrap_or(&h));
                }
                Ok(Item::Other)
            }
            SamLine::Blank(_) => Ok(Item::Other),
            SamLine::Record(r) => {
                self.rec = Some((r, nl));
                Ok(Item::Record)
            }
        }
    }

    fn record(&self) -> &dyn AlignmentRecord {
        &self.rec.as_ref().expect("current record").0
    }

    fn record_mut(&mut self) -> &mut dyn AlignmentRecord {
        &mut self.rec.as_mut().expect("current record").0
    }

    fn comment(&self) -> Option<&[u8]> {
        self.comment.as_deref()
    }

    fn qual_range(&self) -> (u8, u8) {
        SAM_QUAL_RANGE
    }

    fn write_prelude(&self, _out: &mut dyn Write) -> Result<()> {
        Ok(())
    }

    fn write_current(&self, out: &mut dyn Write) -> Result<()> {
        match &self.rec {
            None => out.write_all(&self.line)?,
            Some((r, nl)) => {
                out.write_all(&r.line)?;
                if r.cr {
                    out.write_all(b"\r")?;
                }
                if *nl {
                    out.write_all(b"\n")?;
                }
            }
        }
        Ok(())
    }

    fn held(&self) -> usize {
        self.line.capacity() + self.rec.as_ref().map_or(0, |(r, _)| r.line.capacity())
    }
}

struct BamSource<'a> {
    src: Box<dyn ByteSource + 'a>,
    text: Vec<u8>,
    refs: Vec<u8>,
    /// Current `block_size` + record bytes.
    buf: Vec<u8>,
    index: usize,
    rec: Option<BamRecord>,
    comment: Option<Vec<u8>>,
}

impl<'a> BamSource<'a> {
    fn open(reader: &'a mut dyn BufRead) -> Result<Self> {
        let mut head = Vec::new();
        read_append(&mut Raw(&mut *reader), 4, &mut head)?;
        let compressed = if head.starts_with(&BAM_MAGIC) {
            false
        } else if head.starts_with(&[0x1f, 0x8b]) {
            true
        } else {
            return Err(integrity("Not a BAM file (expected BGZF or BAM\\1 magic)"));
        };
        let input = Cursor::new(head).chain(reader);
        let mut src: Box<dyn ByteSource + 'a> = if compressed {
            Box::new(Bgzf::new(input))
        } else {
            Box::new(Raw(input))
        };

        // Read exactly the bytes the header parser consumes (stopping early
        // on anything it would reject), then let it validate them.
        let mut hdr = Vec::new();
        Self::read_header(&mut *src, &mut hdr)?;
        let (text, refs, end) = parse_bam_header(&hdr)?;
        debug_assert_eq!(end, hdr.len());
        let comment = bam_header_lines(&text).find_map(comment_envelope);
        Ok(Self {
            src,
            text,
            refs,
            buf: Vec::new(),
            index: 0,
            rec: None,
            comment,
        })
    }

    fn read_header(src: &mut dyn ByteSource, hdr: &mut Vec<u8>) -> Result<()> {
        let mut field = |n: usize, hdr: &mut Vec<u8>| -> Result<bool> {
            Ok(read_append(&mut *src, n, hdr)? == n)
        };
        if !field(8, hdr)? || hdr[..4] != BAM_MAGIC {
            return Ok(());
        }
        let l_text = le_u32(hdr, 4).unwrap_or(0) as usize;
        if !field(l_text, hdr)? || !field(4, hdr)? {
            return Ok(());
        }
        let n_ref = le_u32(hdr, hdr.len() - 4).unwrap_or(0);
        for _ in 0..n_ref {
            if !field(4, hdr)? {
                return Ok(());
            }
            let l_name = le_u32(hdr, hdr.len() - 4).unwrap_or(0) as usize;
            if l_name == 0 || !field(l_name.saturating_add(4), hdr)? {
                return Ok(());
            }
        }
        Ok(())
    }
}

impl Source for BamSource<'_> {
    fn next(&mut self) -> Result<Item> {
        self.buf.clear();
        self.rec = None;
        let n = read_append(&mut *self.src, 4, &mut self.buf)?;
        if n == 0 {
            return Ok(Item::Eof);
        }
        if n == 4 {
            let block_size = le_u32(&self.buf, 0).unwrap_or(0) as usize;
            read_append(&mut *self.src, block_size, &mut self.buf)?;
        }
        let (rec, _) = next_bam_record(&self.buf, 0, self.index)?;
        self.index += 1;
        self.rec = Some(rec);
        Ok(Item::Record)
    }

    fn record(&self) -> &dyn AlignmentRecord {
        self.rec.as_ref().expect("current record")
    }

    fn record_mut(&mut self) -> &mut dyn AlignmentRecord {
        self.rec.as_mut().expect("current record")
    }

    fn comment(&self) -> Option<&[u8]> {
        self.comment.as_deref()
    }

    fn qual_range(&self) -> (u8, u8) {
        BAM_QUAL_RANGE
    }

    fn write_prelude(&self, out: &mut dyn Write) -> Result<()> {
        write_bam_header(&self.text, &self.refs, out)
    }

    fn write_current(&self, out: &mut dyn Write) -> Result<()> {
        match &self.rec {
            Some(r) => r.write_to(out),
            None => Ok(()),
        }
    }

    fn held(&self) -> usize {
        self.src.held()
            + self.text.capacity()
            + self.refs.capacity()
            + self.buf.capacity()
            + self.rec.as_ref().map_or(0, |_| self.buf.capacity())
    }
}

// ---------------------------------------------------------------------------
// Channel scanners
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum TagState {
    /// Chunk indices seen so far are 0, 1, 2, ...
    #[default]
    Ordered,
    /// Indices out of order: undecidable without buffering every chunk.
    Unordered,
    /// Cannot form an envelope (the in-memory handler finds none).
    Invalid,
}

/// Incremental version of the in-memory `zs:Z` tag reassembly.
#[derive(Debug, Default)]
struct TagScan {
    state: TagState,
    /// Chunks seen.
    chunks: usize,
    /// Envelope bytes reassembled (all of them, or only the header).
    bytes: Vec<u8>,
    keep_all: bool,
    /// Total envelope bytes in the in-order chunks.
    total: usize,
}

/// What the tags hold.
enum TagEnvelope<'a> {
    None,
    Unknown,
    Found(&'a [u8]),
}

impl TagScan {
    fn push(&mut self, v: &[u8]) {
        if self.state == TagState::Invalid {
            return;
        }
        let Some((i, chunk)) = parse_tag_chunk(v) else {
            self.state = TagState::Invalid;
            return;
        };
        if self.state == TagState::Ordered {
            if i == self.chunks {
                self.total = self.total.saturating_add(chunk.len());
                let keep = if self.keep_all {
                    chunk.len()
                } else {
                    ENV_HEADER_LEN
                        .saturating_sub(self.bytes.len())
                        .min(chunk.len())
                };
                self.bytes.extend_from_slice(&chunk[..keep]);
                self.settle();
            } else {
                self.state = TagState::Unordered;
            }
        }
        self.chunks += 1;
    }

    /// Envelope (strategy, codec length) declared by the in-order chunks.
    fn header(&self) -> Option<usize> {
        match parse_envelope_header(&self.bytes)? {
            (SamBamEmbedStrategy::OptionalFields | SamBamEmbedStrategy::AuxiliaryData, len) => {
                ENV_HEADER_LEN.checked_add(len)
            }
            _ => None,
        }
    }

    /// Once the header is in, an invalid header or too many bytes can never
    /// become a valid envelope (more chunks only add bytes).
    fn settle(&mut self) {
        if self.bytes.len() >= ENV_HEADER_LEN && self.header().is_none_or(|t| self.total > t) {
            self.state = TagState::Invalid;
            self.bytes = Vec::new();
        }
    }

    fn envelope(&self) -> TagEnvelope<'_> {
        match self.state {
            TagState::Invalid => TagEnvelope::None,
            TagState::Unordered => TagEnvelope::Unknown,
            TagState::Ordered => match self.header() {
                Some(t) if self.total == t => {
                    TagEnvelope::Found(self.bytes.get(ENV_HEADER_LEN..).unwrap_or_default())
                }
                _ => TagEnvelope::None,
            },
        }
    }
}

/// Quality-LSB envelope reader.
#[derive(Debug, Default)]
struct QualScan {
    bits: QualBits,
    /// Stream bytes (envelope header + codec) needed, once known.
    target: Option<usize>,
    frame_checked: bool,
    dead: bool,
    /// Usable quality values seen (embed-time existence check).
    values: usize,
}

impl QualScan {
    /// Feed one record's usable quality values, accumulating at most
    /// `limit` bytes.
    fn push(&mut self, q: &[u8], limit: usize) {
        self.values += q.len();
        if self.dead || self.bits.bytes.len() >= limit.min(self.target.unwrap_or(usize::MAX)) {
            return;
        }
        for &v in q {
            self.bits.push(v);
        }
        let have = self.bits.bytes.len();
        if self.target.is_none() && have >= ENV_HEADER_LEN {
            match parse_envelope_header(&self.bits.bytes) {
                Some((SamBamEmbedStrategy::QualityScores, len)) => {
                    self.target = Some(ENV_HEADER_LEN.saturating_add(len));
                }
                _ => {
                    self.dead = true;
                    self.bits = QualBits::default();
                    return;
                }
            }
        }
        if let (Some(t), false) = (self.target, self.frame_checked) {
            // The codec frame's own header gives its exact length; stop
            // there even if the envelope length is corrupt.
            let need = MAX_HEADER_BLOCK.min(t - ENV_HEADER_LEN);
            if have >= ENV_HEADER_LEN + need {
                self.frame_checked = true;
                let frame = &self.bits.bytes[ENV_HEADER_LEN..ENV_HEADER_LEN + need];
                let n = frame_len(frame).unwrap_or(need);
                self.target = Some(t.min(ENV_HEADER_LEN.saturating_add(n)));
            }
        }
    }

    /// Codec bytes, once complete.
    fn codec(&self) -> Option<&[u8]> {
        let t = self.target?;
        self.bits.bytes.get(ENV_HEADER_LEN..t)
    }

    /// Whether the in-memory handler would find a quality envelope (the
    /// header parses and the declared envelope fits the whole channel).
    fn exists(&self) -> bool {
        matches!(
            parse_envelope_header(&self.bits.bytes),
            Some((SamBamEmbedStrategy::QualityScores, len))
                if ENV_HEADER_LEN.saturating_add(len) <= self.values / 8
        )
    }
}

fn is_comment_envelope(env: Option<&[u8]>) -> bool {
    matches!(
        env.and_then(parse_envelope),
        Some((SamBamEmbedStrategy::CommentLines, _))
    )
}

// ---------------------------------------------------------------------------
// StreamHandler
// ---------------------------------------------------------------------------

impl StreamHandler for SamBamStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        self.strategy(params)?;
        bam_compression_from_params(params)?;
        Ok(())
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        let strategy = self.strategy(params)?;
        let compression = bam_compression_from_params(params)?;
        let env = build_envelope(strategy, encoded)?;
        let aux = strategy == SamBamEmbedStrategy::AuxiliaryData;
        let chunks: Vec<&[u8]> = if aux {
            env.chunks(MAX_CHUNK).collect()
        } else {
            Vec::new()
        };
        let total_bits = if aux { 0 } else { env.len() * 8 };

        let mut cr = CountingReader {
            inner: reader,
            n: 0,
        };
        let mut cw = CountingWriter {
            inner: writer,
            n: 0,
        };
        let mut out = Out::new(&mut cw, self.bam && compression == BamCompression::Bgzf);

        let mut tags = TagScan::default();
        let mut qual = QualScan::default();
        let mut any_zs = false;
        let mut records = 0usize;
        let mut placed = 0usize; // aux chunks written
        let mut pos = 0usize; // quality bits written
        let mut peak = 0usize;
        let comment;
        {
            let mut src = self.open(&mut cr)?;
            src.write_prelude(&mut out)?;
            let range = src.qual_range();
            loop {
                match src.next()? {
                    Item::Eof => break,
                    Item::Other => {}
                    Item::Record => {
                        records += 1;
                        // Inspect the original record first.
                        let rec = src.record_mut();
                        any_zs |= rec.has_zs_tag();
                        for v in rec.zs_values() {
                            tags.push(v);
                        }
                        if let Some(q) = rec.usable_qual() {
                            qual.push(q, ENV_HEADER_LEN);
                        }
                        if aux {
                            if let Some(c) = chunks.get(placed) {
                                rec.add_zs_tag(tag_chunk_value(placed, c).as_bytes());
                                placed += 1;
                            }
                        } else if pos < total_bits {
                            if let Some(q) = rec.usable_qual_mut() {
                                for v in q.iter_mut().take(total_bits - pos) {
                                    *v = set_qual_lsb(*v, env_bit(&env, pos), range);
                                    pos += 1;
                                }
                            }
                        }
                    }
                }
                src.write_current(&mut out)?;
                peak = peak.max(src.held() + out.held());
            }
            out.finish()?;
            comment = is_comment_envelope(src.comment());
        }

        // Same checks, in the same order, as the in-memory handler.
        let tagged = match tags.envelope() {
            TagEnvelope::Found(_) => true,
            TagEnvelope::None => false,
            TagEnvelope::Unknown if aux => false, // rejected below (has a zs tag)
            TagEnvelope::Unknown => return Err(unordered_tags()),
        };
        if tagged || comment || qual.exists() {
            return Err(SteganoError::InvalidParameter(
                "Carrier already contains an embedded payload".into(),
            ));
        }
        if aux && any_zs {
            return Err(SteganoError::InvalidParameter(
                "Carrier records already carry a 'zs' tag".into(),
            ));
        }
        if aux && placed < chunks.len() {
            return Err(insufficient(env.len(), records.saturating_mul(MAX_CHUNK)));
        }
        if !aux && pos < total_bits {
            return Err(insufficient(env.len(), qual.values / 8));
        }
        Ok(StreamStats {
            bytes_read: cr.n,
            bytes_written: cw.n,
            peak_buffer_bytes: peak + env.len(),
        })
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, _params: &EmbedParams) -> Result<Vec<u8>> {
        extract(self, reader).map(|(raw, _)| raw)
    }
}

/// Extract the codec bytes; also returns the carrier bytes consumed.
fn extract(h: &SamBamStream, reader: &mut dyn BufRead) -> Result<(Vec<u8>, u64)> {
    let mut cr = CountingReader {
        inner: reader,
        n: 0,
    };
    let mut tags = TagScan {
        keep_all: true,
        ..Default::default()
    };
    let mut qual = QualScan::default();
    let mut comment_checked = false;
    let mut src = h.open(&mut cr)?;
    let result = loop {
        // The first well-formed comment envelope decides the comment channel.
        if !comment_checked {
            if let Some(c) = src.comment() {
                comment_checked = true;
                if let Some((SamBamEmbedStrategy::CommentLines, codec)) = parse_envelope(c) {
                    break Some(codec.to_vec());
                }
            }
        }
        match src.next()? {
            Item::Eof => break None,
            Item::Other => {}
            Item::Record => {
                let rec = src.record();
                for v in rec.zs_values() {
                    tags.push(v);
                }
                if let TagEnvelope::Found(codec) = tags.envelope() {
                    break Some(codec.to_vec());
                }
                if let Some(q) = rec.usable_qual() {
                    qual.push(q, usize::MAX);
                }
                // Prefer tags (as in memory) while a tag envelope is pending.
                if tags.chunks == 0 || tags.state == TagState::Invalid {
                    if let Some(codec) = qual.codec() {
                        break Some(codec.to_vec());
                    }
                }
            }
        }
    };
    drop(src);
    let raw = match result {
        Some(raw) => raw,
        None if tags.state == TagState::Unordered => return Err(unordered_tags()),
        None => qual.codec().ok_or_else(not_found)?.to_vec(),
    };
    Ok((raw, cr.n))
}

#[cfg(test)]
mod tests;
