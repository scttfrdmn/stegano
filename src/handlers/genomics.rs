//! Genomics data format handlers (FASTQ quality-score LSB, FASTA header suffix).
//!
//! # Embedded stream
//!
//! Both formats carry the same self-describing byte stream:
//!
//! ```text
//! [magic "GNSJ"][strategy u8][len u32 LE][len bytes of codec::encode_payload output]
//! ```
//!
//! The strategy byte identifies how the stream was written
//! ([`QualityEmbedStrategy::to_u8`] for FASTQ, [`FASTA_STRATEGY_BYTE`] for
//! FASTA), so extraction auto-detects the strategy from the carrier and runs
//! the (expensive, Argon2id-based) payload decoder exactly once.  There is no
//! decode path for payloads written by stegano < 0.6.
//!
//! # FASTQ: quality-score LSBs
//!
//! Only 4-line records are supported (`@header`, sequence, `+` line, quality
//! line with the same length as the sequence).  Headers, sequences and `+`
//! lines are never modified; only the low `k` bits of quality characters
//! change (`k` = 1, 2 or 3 for Conservative / Balanced / Aggressive).
//!
//! A quality character `q` is **eligible** iff, with `b = q & !(2^k - 1)`
//! (the value with the embedding bits masked off):
//!
//! ```text
//! b >= FASTQ_MIN_MASKED_QUALITY (36, '$')   and   b + 2^k - 1 <= 126 ('~')
//! ```
//!
//! Because the rule depends only on the masked high bits, which embedding
//! never changes, the extractor sees exactly the same eligible positions as
//! the embedder.  Every written character stays in `b..=b + 2^k - 1`, i.e.
//! inside `'$'..='~'`, so output is always valid Phred+33.  The very common
//! low-quality characters `!`, `"` and `#` (Q0-Q2; Illumina uses `#` as a
//! quality-control indicator) are never eligible, never modified and never
//! produced.  Capacity is exactly `k x (number of eligible characters)` bits,
//! minus the 72-bit stream header.
//!
//! # FASTA: header suffix (plaintext-visible)
//!
//! FASTA has no redundant bits, so the stream is hex-encoded and split into
//! chunks of at most [`FASTA_CHUNK_BYTES`] bytes; chunk `i` is appended to the
//! `i`-th header line as ` stg=<hex>` (before its line terminator).  **This is
//! not covert**: anyone looking at the headers sees the suffix.  Use a
//! password if confidentiality matters.  Capacity is exactly
//! `headers x FASTA_CHUNK_BYTES` bytes minus the 9-byte stream header.
//! Sequence lines (and therefore their wrapping) are never touched.
//!
//! # Byte preservation
//!
//! Line terminators (`\n` or `\r\n`, per line), the presence or absence of a
//! final newline, trailing blank lines and a trailing incomplete FASTQ record
//! are all preserved byte for byte.  Records/headers that do not carry
//! payload are byte-identical to the input.
//!
//! # Strategy selection
//!
//! `format_specific["genomics_strategy"]`:
//! * FASTQ: `"conservative"`, `"balanced"`, `"aggressive"`, or `"quality"` /
//!   absent (derive from [`EmbedParams::stealth_level`]: Low -> Aggressive,
//!   Medium -> Balanced, High/Maximum -> Conservative).
//! * FASTA: `"header"` or absent.
//!
//! Any other value is rejected with [`SteganoError::InvalidParameter`].
//! Extraction ignores the key and uses the strategy byte in the carrier.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use crate::time::Instant;

/// Handler for genomics carrier files (FASTA, FASTQ).
///
/// See the [module documentation](self) for the embedding schemes.
pub struct GenomicsHandler;

/// FASTQ quality score embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityEmbedStrategy {
    /// Conservative: 1 bit per eligible quality character
    Conservative,
    /// Balanced: 2 bits per eligible quality character
    Balanced,
    /// Aggressive: 3 bits per eligible quality character
    Aggressive,
}

impl QualityEmbedStrategy {
    const ALL: [QualityEmbedStrategy; 3] = [
        QualityEmbedStrategy::Conservative,
        QualityEmbedStrategy::Balanced,
        QualityEmbedStrategy::Aggressive,
    ];

    /// Number of low bits used per eligible quality character.
    pub fn bits_per_char(self) -> u32 {
        match self {
            QualityEmbedStrategy::Conservative => 1,
            QualityEmbedStrategy::Balanced => 2,
            QualityEmbedStrategy::Aggressive => 3,
        }
    }

    /// Strategy byte stored in the stream header.
    pub fn to_u8(self) -> u8 {
        self.bits_per_char() as u8
    }

    /// Inverse of [`to_u8`](Self::to_u8).
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.to_u8() == v)
    }

    fn mask(self) -> u8 {
        (1u8 << self.bits_per_char()) - 1
    }

    /// Whether quality character `q` can carry bits.  Depends only on the
    /// masked high bits, so it is invariant under embedding.
    pub fn is_eligible(self, q: u8) -> bool {
        let mask = self.mask();
        let base = q & !mask;
        base >= FASTQ_MIN_MASKED_QUALITY && base as u16 + mask as u16 <= MAX_QUALITY_ASCII as u16
    }
}

/// FASTQ record structure for parsing (line contents without terminators).
#[derive(Debug, Clone)]
pub struct FastqRecord {
    pub header: String,
    pub sequence: String,
    pub separator: String,
    pub quality: String,
}

/// Result of FASTQ quality score embedding
#[derive(Debug, Clone)]
pub struct QualityEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: QualityEmbedStrategy,
    pub quality_chars_modified: usize,
    pub biological_integrity_preserved: bool,
}

const MIN_QUALITY_ASCII: u8 = 33; // '!'
const MAX_QUALITY_ASCII: u8 = 126; // '~'

/// Minimum masked quality value (`q & !mask`) for a character to be eligible.
/// 36 = `'$'` (Q3), so `!`, `"` and `#` are never touched.
pub const FASTQ_MIN_MASKED_QUALITY: u8 = 36;

/// Stream magic.
const MAGIC: &[u8; 4] = b"GNSJ";
/// Stream header: magic (4) + strategy (1) + length u32 LE (4).
const HEADER_LEN: usize = 9;
const HEADER_BITS: usize = HEADER_LEN * 8;

/// Strategy byte for the FASTA header-suffix scheme.
pub const FASTA_STRATEGY_BYTE: u8 = 0x10;
/// Maximum stream bytes carried by one FASTA header (hex-encoded, so the
/// suffix adds at most `5 + 2 x FASTA_CHUNK_BYTES` characters).
pub const FASTA_CHUNK_BYTES: usize = 128;
/// FASTA header suffix tag.
const FASTA_TAG: &[u8] = b" stg=";

const STRATEGY_KEY: &str = "genomics_strategy";

// ---------------------------------------------------------------------------
// Line handling (byte exact)
// ---------------------------------------------------------------------------

/// A line: content `[start, end)` and terminator `[end, term_end)`
/// (`""`, `"\n"` or `"\r\n"`).
#[derive(Debug, Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    term_end: usize,
}

fn split_lines(data: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < data.len() {
        match data[start..].iter().position(|&b| b == b'\n') {
            Some(off) => {
                let nl = start + off;
                let end = if nl > start && data[nl - 1] == b'\r' {
                    nl - 1
                } else {
                    nl
                };
                lines.push(Line {
                    start,
                    end,
                    term_end: nl + 1,
                });
                start = nl + 1;
            }
            None => {
                lines.push(Line {
                    start,
                    end: data.len(),
                    term_end: data.len(),
                });
                start = data.len();
            }
        }
    }
    lines
}

/// Location of one complete FASTQ record: indices into the line list.
#[derive(Debug, Clone, Copy)]
struct FastqLoc {
    header: usize,
    sequence: usize,
    separator: usize,
    quality: usize,
}

/// Parse FASTQ structure.  Complete 4-line records are validated strictly;
/// trailing blank lines or a trailing incomplete record are left as
/// pass-through bytes (they carry no payload).
fn locate_fastq(data: &[u8], lines: &[Line]) -> Result<Vec<FastqLoc>> {
    let content = |i: usize| &data[lines[i].start..lines[i].end];
    let mut recs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if (i..lines.len()).all(|j| content(j).is_empty()) {
            break; // trailing blank lines
        }
        if !content(i).starts_with(b"@") {
            return Err(SteganoError::IntegrityError(format!(
                "Invalid FASTQ header at line {}",
                i + 1
            )));
        }
        if i + 3 >= lines.len() {
            break; // trailing incomplete record: preserved, not used
        }
        if !content(i + 2).starts_with(b"+") {
            return Err(SteganoError::IntegrityError(format!(
                "Invalid FASTQ separator at line {} (only 4-line records are supported)",
                i + 3
            )));
        }
        if content(i + 1).len() != content(i + 3).len() {
            return Err(SteganoError::IntegrityError(format!(
                "FASTQ record at line {}: quality length {} != sequence length {} \
                 (multi-line FASTQ is not supported)",
                i + 1,
                content(i + 3).len(),
                content(i + 1).len()
            )));
        }
        recs.push(FastqLoc {
            header: i,
            sequence: i + 1,
            separator: i + 2,
            quality: i + 3,
        });
        i += 4;
    }
    if recs.is_empty() {
        return Err(SteganoError::IntegrityError(
            "No complete FASTQ records found".to_string(),
        ));
    }
    Ok(recs)
}

/// Byte positions of every quality character, in file order.
fn quality_positions<'a>(
    lines: &'a [Line],
    recs: &'a [FastqLoc],
) -> impl Iterator<Item = usize> + 'a {
    recs.iter()
        .flat_map(move |r| lines[r.quality].start..lines[r.quality].end)
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|b| (0..8).map(move |i| (b >> i) & 1))
        .collect()
}

fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    bits.as_chunks::<8>()
        .0
        .iter()
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u8, |acc, (i, &b)| acc | (b << i))
        })
        .collect()
}

fn build_stream(strategy_byte: u8, encoded: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(encoded.len()).map_err(|_| {
        SteganoError::InvalidParameter("Payload too large for genomics header (> 4 GiB)".into())
    })?;
    let mut s = Vec::with_capacity(HEADER_LEN + encoded.len());
    s.extend_from_slice(MAGIC);
    s.push(strategy_byte);
    s.extend_from_slice(&len.to_le_bytes());
    s.extend_from_slice(encoded);
    Ok(s)
}

/// Parse a stream header; returns the payload length if magic and strategy match.
fn parse_stream_header(h: &[u8], strategy_byte: u8) -> Option<usize> {
    if h.len() < HEADER_LEN || &h[0..4] != MAGIC || h[4] != strategy_byte {
        return None;
    }
    Some(u32::from_le_bytes([h[5], h[6], h[7], h[8]]) as usize)
}

fn not_found() -> SteganoError {
    SteganoError::IntegrityError("No embedded data found".to_string())
}

impl Default for GenomicsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl GenomicsHandler {
    pub fn new() -> Self {
        Self
    }

    // -----------------------------------------------------------------------
    // Strategy selection
    // -----------------------------------------------------------------------

    fn strategy_param(params: &EmbedParams) -> Result<Option<&str>> {
        match params.format_specific.get(STRATEGY_KEY) {
            None => Ok(None),
            Some(v) => v.as_str().map(Some).ok_or_else(|| {
                SteganoError::InvalidParameter(format!("{STRATEGY_KEY} must be a string"))
            }),
        }
    }

    fn fastq_strategy(params: &EmbedParams) -> Result<QualityEmbedStrategy> {
        Ok(match Self::strategy_param(params)? {
            None | Some("quality") => match params.stealth_level {
                StealthLevel::Low => QualityEmbedStrategy::Aggressive,
                StealthLevel::Medium => QualityEmbedStrategy::Balanced,
                StealthLevel::High | StealthLevel::Maximum => QualityEmbedStrategy::Conservative,
            },
            Some("conservative") => QualityEmbedStrategy::Conservative,
            Some("balanced") => QualityEmbedStrategy::Balanced,
            Some("aggressive") => QualityEmbedStrategy::Aggressive,
            Some(other) => {
                return Err(SteganoError::InvalidParameter(format!(
                    "Unknown FASTQ {STRATEGY_KEY} '{other}' \
                     (expected quality|conservative|balanced|aggressive)"
                )))
            }
        })
    }

    fn check_fasta_strategy(params: &EmbedParams) -> Result<()> {
        match Self::strategy_param(params)? {
            None | Some("header") => Ok(()),
            Some(other) => Err(SteganoError::InvalidParameter(format!(
                "Unknown FASTA {STRATEGY_KEY} '{other}' (expected header)"
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // FASTA
    // -----------------------------------------------------------------------

    fn fasta_headers(data: &[u8], lines: &[Line]) -> Vec<usize> {
        (0..lines.len())
            .filter(|&i| data[lines[i].start..lines[i].end].starts_with(b">"))
            .collect()
    }

    /// If header content ends with ` stg=<hex>`, return the decoded chunk.
    fn fasta_chunk(content: &[u8]) -> Option<Vec<u8>> {
        let pos = content
            .windows(FASTA_TAG.len())
            .rposition(|w| w == FASTA_TAG)?;
        let hex_part = &content[pos + FASTA_TAG.len()..];
        if hex_part.is_empty() || hex_part.len() > 2 * FASTA_CHUNK_BYTES {
            return None;
        }
        hex::decode(hex_part).ok()
    }

    /// Embed a stream into FASTA header suffixes (plaintext-visible).
    fn embed_fasta(&self, data: &mut Vec<u8>, stream: &[u8]) -> Result<()> {
        let lines = split_lines(data);
        let headers = Self::fasta_headers(data, &lines);
        let available = headers.len() * FASTA_CHUNK_BYTES;
        if stream.len() > available {
            return Err(SteganoError::InsufficientCapacity {
                needed: stream.len() * 8,
                available: available * 8,
            });
        }
        if headers
            .iter()
            .any(|&h| Self::fasta_chunk(&data[lines[h].start..lines[h].end]).is_some())
        {
            return Err(SteganoError::InvalidParameter(
                "FASTA carrier already contains ' stg=' payload suffixes".to_string(),
            ));
        }

        let mut out = Vec::with_capacity(data.len() + 2 * stream.len() + 8 * headers.len());
        let mut chunks = stream.chunks(FASTA_CHUNK_BYTES);
        let mut next_header = headers.iter().peekable();
        for (i, l) in lines.iter().enumerate() {
            out.extend_from_slice(&data[l.start..l.end]);
            if next_header.peek() == Some(&&i) {
                next_header.next();
                if let Some(chunk) = chunks.next() {
                    out.extend_from_slice(FASTA_TAG);
                    out.extend_from_slice(hex::encode(chunk).as_bytes());
                }
            }
            out.extend_from_slice(&data[l.end..l.term_end]);
        }
        *data = out;
        Ok(())
    }

    /// Extract the raw stream payload (codec bytes) from FASTA header suffixes.
    fn extract_fasta(&self, data: &[u8]) -> Result<Vec<u8>> {
        let lines = split_lines(data);
        let mut stream = Vec::new();
        for h in Self::fasta_headers(data, &lines) {
            match Self::fasta_chunk(&data[lines[h].start..lines[h].end]) {
                Some(c) => stream.extend_from_slice(&c),
                None => break,
            }
            if stream.len() >= HEADER_LEN {
                if let Some(len) = parse_stream_header(&stream, FASTA_STRATEGY_BYTE) {
                    if stream.len() >= HEADER_LEN + len {
                        break;
                    }
                }
            }
        }
        let len = parse_stream_header(&stream, FASTA_STRATEGY_BYTE).ok_or_else(not_found)?;
        if stream.len() < HEADER_LEN + len {
            return Err(SteganoError::IntegrityError(format!(
                "FASTA payload truncated: header says {len} bytes, found {}",
                stream.len() - HEADER_LEN
            )));
        }
        Ok(stream[HEADER_LEN..HEADER_LEN + len].to_vec())
    }

    /// Exact capacity for FASTA header-suffix embedding.
    fn estimate_fasta_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        let headers = Self::fasta_headers(data, &split_lines(data)).len();
        let capacity_bytes = headers * FASTA_CHUNK_BYTES;
        let recommended_bits = capacity_bytes.saturating_sub(HEADER_LEN) * 8;
        let mut info = CapacityInfo::new(data.len() * 8, recommended_bits);
        let fi = &mut info.format_specific_info;
        fi.insert("method".into(), "header_suffix_hex".into());
        fi.insert("plaintext_visible".into(), true.into());
        fi.insert("sequences".into(), headers.into());
        fi.insert("chunk_bytes_per_header".into(), FASTA_CHUNK_BYTES.into());
        fi.insert("overhead_bytes".into(), HEADER_LEN.into());
        fi.insert(
            "estimated_capacity_bytes".into(),
            (recommended_bits / 8).into(),
        );
        Ok(info)
    }

    // -----------------------------------------------------------------------
    // FASTQ
    // -----------------------------------------------------------------------

    /// Parse FASTQ data into structured records (complete records only).
    pub fn parse_fastq(&self, data: &[u8]) -> Result<Vec<FastqRecord>> {
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        let s =
            |i: usize| String::from_utf8_lossy(&data[lines[i].start..lines[i].end]).into_owned();
        Ok(recs
            .iter()
            .map(|r| FastqRecord {
                header: s(r.header),
                sequence: s(r.sequence),
                separator: s(r.separator),
                quality: s(r.quality),
            })
            .collect())
    }

    /// Exact number of payload-carrying bits (including the 72-bit stream
    /// header) for `strategy`.
    pub fn fastq_capacity_bits(
        &self,
        data: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<usize> {
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        let eligible = quality_positions(&lines, &recs)
            .filter(|&p| strategy.is_eligible(data[p]))
            .count();
        Ok(eligible * strategy.bits_per_char() as usize)
    }

    /// Estimate capacity for FASTQ files (Balanced strategy as main value).
    fn estimate_fastq_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        let mut info =
            self.estimate_fastq_quality_capacity(data, QualityEmbedStrategy::Balanced)?;
        for s in QualityEmbedStrategy::ALL {
            let bits = self.fastq_capacity_bits(data, s)?;
            info.format_specific_info.insert(
                format!("{s:?}_recommended_bits").to_lowercase(),
                bits.saturating_sub(HEADER_BITS).into(),
            );
        }
        Ok(info)
    }

    /// Exact capacity for one strategy.  `recommended_bits` excludes the
    /// 72-bit stream header and is the maximum size (in bits) of the
    /// codec-encoded payload.
    fn estimate_fastq_quality_capacity(
        &self,
        data: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        let total_quality_chars: usize = recs
            .iter()
            .map(|r| lines[r.quality].end - lines[r.quality].start)
            .sum();
        let capacity_bits = self.fastq_capacity_bits(data, strategy)?;
        let recommended_bits = capacity_bits.saturating_sub(HEADER_BITS);
        let mut info = CapacityInfo::new(data.len() * 8, recommended_bits);
        let fi = &mut info.format_specific_info;
        fi.insert("method".into(), "fastq_quality_score_steganography".into());
        fi.insert("strategy".into(), format!("{strategy:?}").into());
        fi.insert("records".into(), recs.len().into());
        fi.insert("total_quality_chars".into(), total_quality_chars.into());
        fi.insert(
            "eligible_quality_chars".into(),
            (capacity_bits / strategy.bits_per_char() as usize).into(),
        );
        fi.insert("bits_per_char".into(), strategy.bits_per_char().into());
        fi.insert("overhead_bytes".into(), HEADER_LEN.into());
        fi.insert(
            "estimated_capacity_bytes".into(),
            (recommended_bits / 8).into(),
        );
        Ok(info)
    }

    /// Embed `payload` (raw bytes, wrapped in the `GNSJ` stream header) in
    /// FASTQ quality scores.  Returns
    /// [`SteganoError::InsufficientCapacity`] if it does not fit; never
    /// writes a partial payload.
    pub fn embed_fastq_qualities(
        &self,
        data: &mut [u8],
        payload: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<QualityEmbedResult> {
        let stream = build_stream(strategy.to_u8(), payload)?;
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        let k = strategy.bits_per_char() as usize;
        let mask = strategy.mask();

        let positions: Vec<usize> = quality_positions(&lines, &recs)
            .filter(|&p| strategy.is_eligible(data[p]))
            .collect();
        let bits = bytes_to_bits(&stream);
        let available = positions.len() * k;
        if bits.len() > available {
            return Err(SteganoError::InsufficientCapacity {
                needed: bits.len(),
                available,
            });
        }

        let mut modified = 0;
        for (chunk, &p) in bits.chunks(k).zip(&positions) {
            let mut v = 0u8;
            for (i, &b) in chunk.iter().enumerate() {
                v |= b << i;
            }
            // A final partial chunk keeps the original value of its unused bits.
            let used = ((1u16 << chunk.len()) - 1) as u8;
            let new = (data[p] & !(mask & used)) | v;
            debug_assert!(strategy.is_eligible(new));
            if new != data[p] {
                modified += 1;
            }
            data[p] = new;
        }

        Ok(QualityEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: strategy,
            quality_chars_modified: modified,
            biological_integrity_preserved: self.verify_fastq_biological_integrity(data)?,
        })
    }

    /// Read the stream for `strategy`; `Ok(None)` if no matching header.
    fn read_fastq_stream(
        data: &[u8],
        lines: &[Line],
        recs: &[FastqLoc],
        strategy: QualityEmbedStrategy,
    ) -> Result<Option<Vec<u8>>> {
        let k = strategy.bits_per_char() as usize;
        let mut positions =
            quality_positions(lines, recs).filter(|&p| strategy.is_eligible(data[p]));
        let mut read_bytes = |n: usize| -> Option<Vec<u8>> {
            let mut bits = Vec::with_capacity(n * 8 + k);
            while bits.len() < n * 8 {
                let q = data[positions.next()?];
                bits.extend((0..k).map(|i| (q >> i) & 1));
            }
            Some(bits)
        };
        // Bits are consumed per character, so read header+payload as one bit run.
        let Some(hbits) = read_bytes(HEADER_LEN) else {
            return Ok(None);
        };
        let header = bits_to_bytes(&hbits[..HEADER_BITS]);
        let Some(len) = parse_stream_header(&header, strategy.to_u8()) else {
            return Ok(None);
        };
        let capacity = positions_count(data, lines, recs, strategy) * k;
        if len.saturating_mul(8) > capacity.saturating_sub(HEADER_BITS) {
            return Err(SteganoError::IntegrityError(format!(
                "Embedded length {len} exceeds FASTQ capacity (truncated or corrupted carrier)"
            )));
        }
        let leftover = hbits[HEADER_BITS..].to_vec();
        let need = (len * 8).saturating_sub(leftover.len());
        let mut body = leftover;
        if need > 0 {
            let more = read_bytes(need.div_ceil(8)).ok_or_else(|| {
                SteganoError::IntegrityError("Carrier ended before the payload".to_string())
            })?;
            body.extend_from_slice(&more);
        }
        body.truncate(len * 8);
        Ok(Some(bits_to_bytes(&body)))
    }

    /// Extract the raw stream payload from FASTQ quality scores written with
    /// `strategy`.
    pub fn extract_fastq_qualities(
        &self,
        data: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<Vec<u8>> {
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        Self::read_fastq_stream(data, &lines, &recs, strategy)?.ok_or_else(not_found)
    }

    /// Extract with strategy auto-detection (from the stream header).
    fn extract_fastq_auto(&self, data: &[u8]) -> Result<Vec<u8>> {
        let lines = split_lines(data);
        let recs = locate_fastq(data, &lines)?;
        for s in QualityEmbedStrategy::ALL {
            if let Some(raw) = Self::read_fastq_stream(data, &lines, &recs, s)? {
                return Ok(raw);
            }
        }
        Err(not_found())
    }

    fn has_fastq_stream(&self, data: &[u8]) -> bool {
        self.extract_fastq_auto(data).is_ok()
    }

    /// Verify FASTQ structure: 4-line records, quality length == sequence
    /// length, all quality characters in `'!'..='~'`, IUPAC nucleotides.
    fn verify_fastq_biological_integrity(&self, data: &[u8]) -> Result<bool> {
        let lines = split_lines(data);
        let recs = match locate_fastq(data, &lines) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        let content = |i: usize| &data[lines[i].start..lines[i].end];
        for r in &recs {
            if !content(r.quality)
                .iter()
                .all(|q| (MIN_QUALITY_ASCII..=MAX_QUALITY_ASCII).contains(q))
            {
                return Ok(false);
            }
            if !content(r.sequence)
                .iter()
                .all(|c| b"ACGTUNRYKMSWBDHV.-".contains(&c.to_ascii_uppercase()))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Check if data is FASTA format
    fn is_fasta(&self, data: &[u8]) -> bool {
        data.starts_with(b">") && data.contains(&b'\n')
    }

    /// Check if data is FASTQ format
    fn is_fastq(&self, data: &[u8]) -> bool {
        data.starts_with(b"@") && data.contains(&b'\n') && data.contains(&b'+')
    }
}

fn positions_count(
    data: &[u8],
    lines: &[Line],
    recs: &[FastqLoc],
    strategy: QualityEmbedStrategy,
) -> usize {
    quality_positions(lines, recs)
        .filter(|&p| strategy.is_eligible(data[p]))
        .count()
}

#[async_trait]
impl DataHandler for GenomicsHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_fasta(data) {
            Some(DataFormat::FASTA)
        } else if self.is_fastq(data) {
            Some(DataFormat::FASTQ)
        } else {
            None
        }
    }

    /// Exact capacity.  `recommended_bits` is the maximum size of the
    /// codec-encoded payload (the 9-byte stream header is already excluded).
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::FASTA => self.estimate_fasta_capacity(&carrier.raw_data),
            DataFormat::FASTQ => self.estimate_fastq_capacity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start_time = Instant::now();

        // Validate strategy and structure before the (expensive) codec.
        let fastq_strategy = match carrier.format {
            DataFormat::FASTA => {
                Self::check_fasta_strategy(params)?;
                None
            }
            DataFormat::FASTQ => {
                let s = Self::fastq_strategy(params)?;
                locate_fastq(&carrier.raw_data, &split_lines(&carrier.raw_data))?;
                Some(s)
            }
            _ => return Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        };

        let enc_start = Instant::now();
        let encoded = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };

        let mut new_data = carrier.raw_data.clone();
        match fastq_strategy {
            None => {
                let stream = build_stream(FASTA_STRATEGY_BYTE, &encoded)?;
                self.embed_fasta(&mut new_data, &stream)?;
            }
            Some(s) => {
                self.embed_fastq_qualities(&mut new_data, &encoded, s)?;
            }
        }
        carrier.raw_data = new_data;

        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: start_time.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + encoded.len(),
            },
        ))
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let raw = match carrier.format {
            DataFormat::FASTA => self.extract_fasta(&carrier.raw_data)?,
            DataFormat::FASTQ => self.extract_fastq_auto(&carrier.raw_data)?,
            _ => return Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        };
        crate::codec::decode_payload(&raw, params)
    }

    /// `true` iff the carrier is structurally valid and contains a `GNSJ`
    /// stream header (no decryption is attempted).
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::FASTA => {
                Ok(self.is_fasta(&carrier.raw_data)
                    && self.extract_fasta(&carrier.raw_data).is_ok())
            }
            DataFormat::FASTQ => Ok(self.verify_fastq_biological_integrity(&carrier.raw_data)?
                && self.has_fastq_stream(&carrier.raw_data)),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn carrier(format: DataFormat, data: &[u8]) -> CarrierData {
        CarrierData {
            format,
            raw_data: data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        }
    }

    fn params(stealth: StealthLevel, password: &[u8]) -> EmbedParams {
        EmbedParams {
            encryption_key: password.to_vec(),
            error_correction_level: ECLevel::None,
            stealth_level: stealth,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    fn with_strategy(mut p: EmbedParams, s: &str) -> EmbedParams {
        p.format_specific
            .insert(STRATEGY_KEY.to_string(), serde_json::Value::from(s));
        p
    }

    /// Deterministic pseudo-random generator for test data.
    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed >> 33
    }

    /// Synthetic FASTQ with qualities drawn from `alphabet`; Illumina-style
    /// trailing runs of '#' on every third read.
    fn synth_fastq(records: usize, len: usize, alphabet: &[u8], eol: &str) -> Vec<u8> {
        let mut seed = 42u64;
        let mut out = String::new();
        for r in 0..records {
            let seq: String = (0..len)
                .map(|_| b"ACGTN"[(lcg(&mut seed) % 5) as usize] as char)
                .collect();
            let mut qual: Vec<u8> = (0..len)
                .map(|_| alphabet[(lcg(&mut seed) % alphabet.len() as u64) as usize])
                .collect();
            if r % 3 == 0 {
                for q in qual.iter_mut().skip(len * 2 / 3) {
                    *q = b'#';
                }
            }
            out.push_str(&format!("@read{r} 1:N:0:ACGT{eol}{seq}{eol}+{eol}"));
            out.push_str(std::str::from_utf8(&qual).unwrap());
            out.push_str(eol);
        }
        out.into_bytes()
    }

    /// Structural FASTQ validation of handler output against the original.
    fn assert_valid_fastq(original: &[u8], modified: &[u8]) {
        let lo = split_lines(original);
        let lm = split_lines(modified);
        assert_eq!(lo.len(), lm.len(), "line count changed");
        let ro = locate_fastq(original, &lo).unwrap();
        let rm = locate_fastq(modified, &lm).unwrap();
        assert_eq!(ro.len(), rm.len());
        let c = |d: &'_ [u8], l: Line| d[l.start..l.term_end].to_vec();
        for (a, b) in ro.iter().zip(&rm) {
            for (x, y) in [
                (a.header, b.header),
                (a.sequence, b.sequence),
                (a.separator, b.separator),
            ] {
                assert_eq!(c(original, lo[x]), c(modified, lm[y]));
            }
            let qo = &original[lo[a.quality].start..lo[a.quality].end];
            let qm = &modified[lm[b.quality].start..lm[b.quality].end];
            let sm = &modified[lm[b.sequence].start..lm[b.sequence].end];
            assert!(modified[lm[b.header].start] == b'@');
            assert!(modified[lm[b.separator].start] == b'+');
            assert_eq!(qm.len(), sm.len());
            assert!(
                qm.iter().all(|q| (33..=126).contains(q)),
                "quality out of range"
            );
            for (o, m) in qo.iter().zip(qm) {
                if *o <= b'#' {
                    assert_eq!(o, m, "low quality char '{}' was modified", *o as char);
                }
                assert!(*m > b'#' || o == m, "produced '{}'", *m as char);
            }
            // Terminators are unchanged.
            assert_eq!(
                &original[lo[a.quality].end..lo[a.quality].term_end],
                &modified[lm[b.quality].end..lm[b.quality].term_end]
            );
        }
    }

    #[test]
    fn test_fasta_detection() {
        let handler = GenomicsHandler::new();
        let fasta_data = b">seq1\nATCGATCG\n>seq2\nGCTAGCTA\n";
        assert_eq!(handler.detect_format(fasta_data), Some(DataFormat::FASTA));
    }

    #[test]
    fn test_fastq_detection() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1\nATCGATCG\n+\nIIIIIIII\n";
        assert_eq!(handler.detect_format(fastq_data), Some(DataFormat::FASTQ));
    }

    #[test]
    fn test_eligibility_rule() {
        use QualityEmbedStrategy::*;
        for s in QualityEmbedStrategy::ALL {
            for q in 0u8..=255 {
                let elig = s.is_eligible(q);
                // Invariant under any change of the low bits.
                for v in 0..=s.mask() {
                    let m = (q & !s.mask()) | v;
                    assert_eq!(s.is_eligible(m), elig);
                    if elig {
                        assert!((36..=126).contains(&m));
                    }
                }
            }
            for &q in b"!\"#" {
                assert!(!s.is_eligible(q));
            }
        }
        assert!(Conservative.is_eligible(b'$'));
        assert!(!Conservative.is_eligible(b'~'));
        assert!(Balanced.is_eligible(b'F'));
        assert!(Aggressive.is_eligible(b','));
        assert!(!Aggressive.is_eligible(b'$'));
        for s in QualityEmbedStrategy::ALL {
            assert_eq!(QualityEmbedStrategy::from_u8(s.to_u8()), Some(s));
        }
    }

    #[test]
    fn test_fasta_capacity_estimation() {
        let handler = GenomicsHandler::new();
        let fasta_data = b">seq1 description\nATCGATCG\n>seq2 another\nGCTAGCTA\n";
        let capacity = handler
            .estimate_capacity(&carrier(DataFormat::FASTA, fasta_data))
            .unwrap();
        assert_eq!(
            capacity.recommended_bits,
            (2 * FASTA_CHUNK_BYTES - HEADER_LEN) * 8
        );
        assert!(capacity.steganographic_ratio > 0.0);
    }

    const WRAPPED_FASTA: &[u8] = b">seq1 description line\nATCGATCGATCGATCGATCG\nATCGATCGATCGATCGATCG\nATCG\n>seq2 another\nGCTAGCTAGCTAGCTAGCTA\nGCTA\n>seq3 third\nTTTTAAAACCCCGGGG\n>seq4\nACGT\n";

    #[tokio::test]
    async fn test_fasta_embedding_roundtrip_preserves_wrapping() {
        let handler = GenomicsHandler::new();
        for pw in [&b""[..], b"pw"] {
            let mut c = carrier(DataFormat::FASTA, WRAPPED_FASTA);
            let payload = b"secret genomics data that spans several headers ".repeat(4);
            let p = params(StealthLevel::Low, pw);
            handler.embed(&mut c, &payload, &p).await.unwrap();
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload);
            assert!(handler.verify_integrity(&c).unwrap());

            // Sequence lines identical; headers keep their prefix; same line count.
            let a: Vec<&[u8]> = WRAPPED_FASTA.split(|&b| b == b'\n').collect();
            let b: Vec<&[u8]> = c.raw_data.split(|&b| b == b'\n').collect();
            assert_eq!(a.len(), b.len());
            let mut suffixed = 0;
            for (x, y) in a.iter().zip(&b) {
                if x.starts_with(b">") {
                    assert!(y.starts_with(x));
                    if y.len() > x.len() {
                        assert!(y[x.len()..].starts_with(FASTA_TAG));
                        suffixed += 1;
                    }
                } else {
                    assert_eq!(x, y);
                }
            }
            assert!(suffixed >= 2, "payload should spread over headers");
        }
    }

    #[tokio::test]
    async fn test_fasta_insufficient_capacity_errors() {
        let handler = GenomicsHandler::new();
        let mut c = carrier(DataFormat::FASTA, b">only\nACGT\n");
        let before = c.raw_data.clone();
        let payload = vec![7u8; FASTA_CHUNK_BYTES];
        let err = handler
            .embed(&mut c, &payload, &params(StealthLevel::Low, b""))
            .await
            .unwrap_err();
        assert!(matches!(err, SteganoError::InsufficientCapacity { .. }));
        assert_eq!(c.raw_data, before, "carrier must be untouched on error");
    }

    #[tokio::test]
    async fn test_fasta_exact_capacity() {
        let handler = GenomicsHandler::new();
        let c0 = carrier(DataFormat::FASTA, WRAPPED_FASTA);
        let cap = handler.estimate_capacity(&c0).unwrap().recommended_bits / 8;
        let overhead = crate::codec::encode_payload(b"", &params(StealthLevel::Low, b""))
            .unwrap()
            .len();
        let p = params(StealthLevel::Low, b"");
        let mut c = c0.clone();
        let fits = vec![1u8; cap - overhead];
        handler.embed(&mut c, &fits, &p).await.unwrap();
        assert_eq!(handler.extract(&c, &p).await.unwrap(), fits);
        let mut c = c0.clone();
        let too_big = vec![1u8; cap - overhead + 1];
        assert!(handler.embed(&mut c, &too_big, &p).await.is_err());
    }

    #[tokio::test]
    async fn test_fasta_crlf_and_no_final_newline() {
        let handler = GenomicsHandler::new();
        let data = b">s1 a\r\nACGTACGT\r\nACGT\r\n>s2 b\r\nGGGG\r\n>s3\r\nTTTT";
        let mut c = carrier(DataFormat::FASTA, data);
        let p = params(StealthLevel::Low, b"");
        let payload = vec![0xA5u8; 200];
        handler.embed(&mut c, &payload, &p).await.unwrap();
        assert_eq!(handler.extract(&c, &p).await.unwrap(), payload);
        let out = &c.raw_data;
        assert!(out.ends_with(b"\r\nTTTT"));
        // Every '\n' is still preceded by '\r'.
        let lf = out.iter().filter(|&&b| b == b'\n').count();
        let crlf = out.windows(2).filter(|w| w == b"\r\n").count();
        assert_eq!(lf, crlf);
        assert_eq!(lf, data.iter().filter(|&&b| b == b'\n').count());
        // Header with only the remainder: s3 header unmodified (2 chunks needed).
        assert!(out.windows(8).any(|w| w == b"\r\n>s3\r\nT"));
    }

    #[tokio::test]
    async fn test_fasta_rejects_unknown_strategy_and_double_embed() {
        let handler = GenomicsHandler::new();
        let mut c = carrier(DataFormat::FASTA, WRAPPED_FASTA);
        let bad = with_strategy(params(StealthLevel::Low, b""), "codon");
        assert!(matches!(
            handler.embed(&mut c, b"x", &bad).await,
            Err(SteganoError::InvalidParameter(_))
        ));
        let p = with_strategy(params(StealthLevel::Low, b""), "header");
        handler.embed(&mut c, b"x", &p).await.unwrap();
        assert!(handler.embed(&mut c, b"y", &p).await.is_err());
        assert_eq!(handler.extract(&c, &p).await.unwrap(), b"x");
    }

    #[tokio::test]
    async fn test_fasta_no_payload() {
        let handler = GenomicsHandler::new();
        let c = carrier(DataFormat::FASTA, WRAPPED_FASTA);
        assert!(!handler.verify_integrity(&c).unwrap());
        assert!(handler
            .extract(&c, &params(StealthLevel::Low, b""))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_fastq_roundtrip_all_strategies_with_and_without_password() {
        let handler = GenomicsHandler::new();
        let orig = synth_fastq(40, 100, b"#,:FFFFIJ<AE", "\n");
        for stealth in [StealthLevel::Low, StealthLevel::Medium, StealthLevel::High] {
            for pw in [&b""[..], b"hunter2"] {
                let mut c = carrier(DataFormat::FASTQ, &orig);
                let p = params(stealth, pw);
                let payload = b"secret genomics quality data".repeat(3);
                handler.embed(&mut c, &payload, &p).await.unwrap();
                assert_valid_fastq(&orig, &c.raw_data);
                assert!(handler.verify_integrity(&c).unwrap());
                // Extraction auto-detects the strategy (stealth level irrelevant).
                let p2 = params(StealthLevel::Maximum, pw);
                assert_eq!(handler.extract(&c, &p2).await.unwrap(), payload);
            }
        }
    }

    #[tokio::test]
    async fn test_fastq_illumina_low_quality_heavy() {
        // Many '#' and '!' qualities: the old implementation desynchronised here.
        let handler = GenomicsHandler::new();
        let orig = synth_fastq(60, 150, b"!!!###\"#$%&'()*+5?IJ", "\n");
        for s in ["conservative", "balanced", "aggressive"] {
            let mut c = carrier(DataFormat::FASTQ, &orig);
            let p = with_strategy(params(StealthLevel::Medium, b""), s);
            let payload: Vec<u8> = (0..200u32).map(|i| (i * 37 % 251) as u8).collect();
            handler.embed(&mut c, &payload, &p).await.unwrap();
            assert_valid_fastq(&orig, &c.raw_data);
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload, "{s}");
        }
    }

    #[tokio::test]
    async fn test_fastq_binned_qualities() {
        // NovaSeq-style 4-level binning: only 'F', ':', ',' and '#'.
        let handler = GenomicsHandler::new();
        let orig = synth_fastq(50, 120, b"FFFFFFFF::,#", "\n");
        for s in ["conservative", "balanced", "aggressive"] {
            let mut c = carrier(DataFormat::FASTQ, &orig);
            let p = with_strategy(params(StealthLevel::Medium, b"pw"), s);
            let payload = b"binned".repeat(20);
            handler.embed(&mut c, &payload, &p).await.unwrap();
            assert_valid_fastq(&orig, &c.raw_data);
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload, "{s}");
        }
    }

    #[tokio::test]
    async fn test_fastq_exact_capacity_and_error() {
        let handler = GenomicsHandler::new();
        let orig = synth_fastq(10, 50, b"#!FIJ:,", "\n");
        let p = with_strategy(params(StealthLevel::Low, b""), "balanced");
        let overhead = crate::codec::encode_payload(b"", &p).unwrap().len();
        let cap_bits = handler
            .estimate_fastq_quality_capacity(&orig, QualityEmbedStrategy::Balanced)
            .unwrap()
            .recommended_bits;
        let max_plain = cap_bits / 8 - overhead;

        let mut c = carrier(DataFormat::FASTQ, &orig);
        let fits = vec![0x5Au8; max_plain];
        handler.embed(&mut c, &fits, &p).await.unwrap();
        assert_eq!(handler.extract(&c, &p).await.unwrap(), fits);

        let mut c = carrier(DataFormat::FASTQ, &orig);
        let err = handler
            .embed(&mut c, &vec![0u8; max_plain + 1], &p)
            .await
            .unwrap_err();
        // Units are bits (#61).
        match err {
            SteganoError::InsufficientCapacity { needed, available } => {
                // Both include the 9-byte genomics framing header.
                assert_eq!(available, cap_bits + HEADER_LEN * 8);
                assert_eq!(needed, (HEADER_LEN + max_plain + 1 + overhead) * 8);
            }
            other => panic!("expected InsufficientCapacity, got {other:?}"),
        }
        assert_eq!(c.raw_data, orig, "carrier must be untouched on error");
    }

    #[tokio::test]
    async fn test_fastq_crlf_trailing_partial_record_and_no_final_newline() {
        let handler = GenomicsHandler::new();
        let mut orig = synth_fastq(30, 80, b"IJF:,#", "\r\n");
        // Trailing incomplete record without a final newline.
        orig.extend_from_slice(b"@partial\r\nACGT");
        let mut c = carrier(DataFormat::FASTQ, &orig);
        let p = params(StealthLevel::Medium, b"");
        let payload = b"crlf payload".to_vec();
        handler.embed(&mut c, &payload, &p).await.unwrap();
        assert_eq!(c.raw_data.len(), orig.len());
        assert!(c.raw_data.ends_with(b"\r\n@partial\r\nACGT"));
        let lf = c.raw_data.iter().filter(|&&b| b == b'\n').count();
        let crlf = c.raw_data.windows(2).filter(|w| w == b"\r\n").count();
        assert_eq!(lf, crlf);
        assert_valid_fastq(&orig, &c.raw_data);
        assert_eq!(handler.extract(&c, &p).await.unwrap(), payload);

        // Last complete record without final newline, plus trailing blank lines.
        for tail in [&b""[..], b"\n\n"] {
            let mut o = synth_fastq(30, 80, b"IJF:,#", "\n");
            o.pop();
            o.extend_from_slice(tail);
            let mut c = carrier(DataFormat::FASTQ, &o);
            handler.embed(&mut c, &payload, &p).await.unwrap();
            assert_eq!(c.raw_data.len(), o.len());
            assert_eq!(c.raw_data.ends_with(b"\n\n"), !tail.is_empty());
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload);
        }
    }

    #[tokio::test]
    async fn test_fastq_unmodified_records_byte_identical() {
        let handler = GenomicsHandler::new();
        let orig = synth_fastq(100, 100, b"IJF", "\n");
        let mut c = carrier(DataFormat::FASTQ, &orig);
        let p = params(StealthLevel::Low, b"");
        handler.embed(&mut c, b"tiny", &p).await.unwrap();
        // Payload is ~49 bytes x 8 / 3 bits ~ 131 chars: only the first 2 records change.
        let n = orig.len() / 2;
        assert_eq!(&orig[n..], &c.raw_data[n..]);
    }

    #[tokio::test]
    async fn test_fastq_rejects_invalid_structure_and_strategy() {
        let handler = GenomicsHandler::new();
        let p = params(StealthLevel::Low, b"");
        let mut c = carrier(DataFormat::FASTQ, b"@r\nACGT\n+\nIII\n");
        assert!(handler.embed(&mut c, b"x", &p).await.is_err());
        let mut c = carrier(DataFormat::FASTQ, &synth_fastq(10, 50, b"I", "\n"));
        let bad = with_strategy(p, "codon");
        assert!(matches!(
            handler.embed(&mut c, b"x", &bad).await,
            Err(SteganoError::InvalidParameter(_))
        ));
    }

    #[tokio::test]
    async fn test_fastq_no_payload() {
        let handler = GenomicsHandler::new();
        let c = carrier(DataFormat::FASTQ, &synth_fastq(10, 50, b"IJF:", "\n"));
        assert!(!handler.verify_integrity(&c).unwrap());
        assert!(handler
            .extract(&c, &params(StealthLevel::Low, b""))
            .await
            .is_err());
    }

    #[test]
    fn test_fastq_capacity_estimation_strategies() {
        let handler = GenomicsHandler::new();
        let fastq_data = synth_fastq(4, 60, b"IJKFH", "\n");
        let caps: Vec<usize> = QualityEmbedStrategy::ALL
            .iter()
            .map(|&s| {
                handler
                    .estimate_fastq_quality_capacity(&fastq_data, s)
                    .unwrap()
                    .recommended_bits
            })
            .collect();
        assert!(caps[0] < caps[1] && caps[1] < caps[2]);
    }

    #[test]
    fn test_fastq_parsing() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1/1\r\nATCGATCG\r\n+\r\nIIIIIIII\r\n@seq2/1\nGCTAGCTA\n+\nJJJJJJJJ";
        let records = handler.parse_fastq(fastq_data).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].header, "@seq1/1");
        assert_eq!(records[0].sequence, "ATCGATCG");
        assert_eq!(records[0].separator, "+");
        assert_eq!(records[0].quality, "IIIIIIII");
        assert_eq!(records[1].header, "@seq2/1");
        assert_eq!(records[1].quality, "JJJJJJJJ");
    }

    #[test]
    fn test_fastq_biological_integrity_verification() {
        let handler = GenomicsHandler::new();
        assert!(handler
            .verify_fastq_biological_integrity(b"@seq1/1\nATCGATCG\n+\nIIIIIIII\n")
            .unwrap());
        assert!(!handler
            .verify_fastq_biological_integrity(b"@seq1/1\nATCGATCG\n+\nIII\n")
            .unwrap());
        assert!(!handler
            .verify_fastq_biological_integrity(b"@seq1/1\nATCGXYZ\n+\nIIIIIII\n")
            .unwrap());
    }

    #[test]
    fn test_split_lines_roundtrip() {
        for d in [
            &b""[..],
            b"a",
            b"a\n",
            b"a\r\nb",
            b"\n\n",
            b"\r\n",
            b"a\rb\n",
        ] {
            let lines = split_lines(d);
            let mut out = Vec::new();
            for l in &lines {
                out.extend_from_slice(&d[l.start..l.term_end]);
            }
            assert_eq!(out, d);
        }
    }
}
