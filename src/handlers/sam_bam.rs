//! SAM/BAM handler for sequence alignment carrier files.
//!
//! # Container formats
//!
//! * **SAM** — tab-delimited text.  Lines are kept as raw bytes; every line
//!   that is not modified by an embedding is written back byte-identically.
//! * **BAM** — BGZF-compressed binary (`1f 8b ...` with the `BC` extra
//!   subfield).  Raw, uncompressed `BAM\1` streams are also accepted on
//!   input; output is always BGZF (≤ 64 KiB blocks plus the standard 28-byte
//!   EOF marker block), so `samtools` and other htslib tools can read it.
//!
//! Parsing is strict: a malformed line, record or BGZF block is an error, and
//! records are never dropped.  All BAM length fields are validated against
//! the enclosing block with checked arithmetic.
//!
//! # Embedding channels (`format_specific["sam_strategy"]`)
//!
//! | name        | where                                                          |
//! |-------------|----------------------------------------------------------------|
//! | `optional`  | one `zs:Z:<idx>,<hex>` tag on each record, spread evenly        |
//! | `auxiliary` | `zs:Z:<idx>,<hex>` tags, 255-byte chunks on as few records as possible |
//! | `quality`   | LSB of QUAL values (records whose QUAL is `*` are skipped)      |
//! | `comment`   | a single `@CO\tzs:<hex>` header line                            |
//! | `multi`     | first of `optional`, `quality`, `comment` with enough capacity  |
//!
//! `zs` is a lowercase tag, which the SAM specification reserves for end
//! users, so it cannot collide with tags defined by the spec or by aligners.
//!
//! Every channel carries the same self-describing envelope:
//!
//! ```text
//! "SBM" 0x01 | strategy u8 | len u32 LE | codec payload (len bytes)
//! ```
//!
//! where the codec payload is produced by [`crate::codec::encode_payload`].
//! Extraction locates the envelope (no `sam_strategy` needed), validates the
//! marker and strategy byte, and only then calls
//! [`crate::codec::decode_payload`] once.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::ops::Range;
use crate::time::Instant;

/// Handler for SAM/BAM sequence alignment carrier files.
///
/// Embeds payload data in a `zs` optional tag, in quality-score LSBs or in a
/// `@CO` header line, preserving all mandatory fields so the file remains
/// valid for downstream bioinformatics tools.
pub struct SamBamHandler;

/// SAM/BAM embedding strategies (selected with `format_specific["sam_strategy"]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamBamEmbedStrategy {
    /// `zs:Z:` tag on every record, payload spread evenly (`"optional"`).
    OptionalFields,
    /// LSB of quality scores (`"quality"`).
    QualityScores,
    /// One `@CO` header line (`"comment"`).
    CommentLines,
    /// `zs:Z:` tags packed into as few records as possible (`"auxiliary"`).
    AuxiliaryData,
    /// First of optional → quality → comment with enough capacity (`"multi"`).
    MultiStrategy,
}

impl SamBamEmbedStrategy {
    /// Parse a `sam_strategy` value.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "optional" | "optional_fields" => Self::OptionalFields,
            "quality" | "quality_scores" => Self::QualityScores,
            "comment" | "comment_lines" => Self::CommentLines,
            "auxiliary" | "auxiliary_data" => Self::AuxiliaryData,
            "multi" | "multi_strategy" => Self::MultiStrategy,
            _ => return None,
        })
    }

    /// Byte stored in the envelope (`MultiStrategy` is never stored).
    fn to_u8(self) -> u8 {
        match self {
            Self::OptionalFields => 0,
            Self::QualityScores => 1,
            Self::CommentLines => 2,
            Self::AuxiliaryData => 3,
            Self::MultiStrategy => 4,
        }
    }

    fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::OptionalFields,
            1 => Self::QualityScores,
            2 => Self::CommentLines,
            3 => Self::AuxiliaryData,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BAM_MAGIC: [u8; 4] = [b'B', b'A', b'M', 1];
/// Envelope marker.
const ENV_MAGIC: [u8; 4] = [b'S', b'B', b'M', 1];
/// Envelope header: magic + strategy + u32 length.
const ENV_HEADER_LEN: usize = 9;
/// Stegano tag (lowercase: reserved by the SAM spec for end users).
const TAG: [u8; 2] = *b"zs";
/// Prefix of a SAM optional field carrying a chunk.
const SAM_TAG_PREFIX: &[u8] = b"zs:Z:";
/// Prefix of the `@CO` comment header line carrying the envelope.
const COMMENT_PREFIX: &[u8] = b"@CO\tzs:";
/// Maximum payload bytes (before hex encoding) per `zs` tag.
const MAX_CHUNK: usize = 255;
/// SAM QUAL characters (Phred+33).
const SAM_QUAL_RANGE: (u8, u8) = (33, 126);
/// BAM quality values (raw Phred, limited to what SAM can represent).
const BAM_QUAL_RANGE: (u8, u8) = (0, 93);
/// Codec header length used for capacity estimates (unencrypted, no EC).
const CODEC_MIN_OVERHEAD: usize = crate::codec::HEADER_LEN;

/// BGZF: maximum uncompressed bytes per block written (same as htslib).
const BGZF_BLOCK_DATA: usize = 0xff00;
/// BGZF: maximum total block size.
const BGZF_MAX_BLOCK: usize = 65536;
/// BGZF: header length (gzip header with a 6-byte `BC` extra field).
const BGZF_HEADER_LEN: usize = 18;
/// BGZF: standard empty EOF marker block.
const BGZF_EOF: [u8; 28] = [
    0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
    0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn integrity(msg: impl Into<String>) -> SteganoError {
    SteganoError::IntegrityError(msg.into())
}

// ---------------------------------------------------------------------------
// BGZF
// ---------------------------------------------------------------------------

fn le_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn le_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le_i32(b: &[u8], off: usize) -> Option<i32> {
    le_u32(b, off).map(|v| v as i32)
}

/// True if `data` starts with a gzip header carrying the BGZF `BC` subfield.
fn is_bgzf(data: &[u8]) -> bool {
    bgzf_block_len(data, 0).is_ok()
}

/// Validate the BGZF block header at `off`; return (total block length,
/// compressed-data range).
fn bgzf_block_len(data: &[u8], off: usize) -> Result<(usize, Range<usize>)> {
    let bad = |m: &str| integrity(format!("Invalid BGZF block at offset {off}: {m}"));
    let h = data
        .get(off..off.checked_add(12).ok_or_else(|| bad("overflow"))?)
        .ok_or_else(|| bad("truncated header"))?;
    if h[0] != 0x1f || h[1] != 0x8b || h[2] != 8 || h[3] != 4 {
        return Err(bad("not a BGZF gzip header"));
    }
    let xlen = le_u16(h, 10).ok_or_else(|| bad("truncated header"))? as usize;
    let extra_start = off + 12;
    let extra_end = extra_start + xlen;
    let extra = data
        .get(extra_start..extra_end)
        .ok_or_else(|| bad("truncated extra field"))?;
    let mut bsize = None;
    let mut p = 0usize;
    while p + 4 <= extra.len() {
        let slen = le_u16(extra, p + 2).ok_or_else(|| bad("truncated subfield"))? as usize;
        let body = extra
            .get(p + 4..p + 4 + slen)
            .ok_or_else(|| bad("truncated subfield"))?;
        if extra[p] == b'B' && extra[p + 1] == b'C' && slen == 2 {
            bsize = Some(u16::from_le_bytes([body[0], body[1]]) as usize);
        }
        p += 4 + slen;
    }
    if p != extra.len() {
        return Err(bad("malformed extra field"));
    }
    let total = bsize.ok_or_else(|| bad("missing BC subfield"))? + 1;
    // header (12 + xlen) + CRC32 + ISIZE
    if total < 12 + xlen + 8 {
        return Err(bad("BSIZE too small"));
    }
    let end = off.checked_add(total).ok_or_else(|| bad("overflow"))?;
    if end > data.len() {
        return Err(bad("truncated block"));
    }
    Ok((total, extra_end..end - 8))
}

/// Inflate one BGZF block starting at `off`; returns (block length, data).
fn bgzf_read_block(data: &[u8], off: usize) -> Result<(usize, Vec<u8>)> {
    let (total, cdata) = bgzf_block_len(data, off)?;
    let trailer = cdata.end;
    let crc = le_u32(data, trailer).ok_or_else(|| integrity("BGZF trailer truncated"))?;
    let isize =
        le_u32(data, trailer + 4).ok_or_else(|| integrity("BGZF trailer truncated"))? as usize;
    if isize > BGZF_MAX_BLOCK {
        return Err(integrity("BGZF block ISIZE exceeds 64 KiB"));
    }
    let mut out = Vec::with_capacity(isize);
    DeflateDecoder::new(&data[cdata])
        .take(BGZF_MAX_BLOCK as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| integrity(format!("BGZF inflate failed at offset {off}: {e}")))?;
    if out.len() != isize {
        return Err(integrity(format!(
            "BGZF block at offset {off}: ISIZE mismatch"
        )));
    }
    let mut c = Crc::new();
    c.update(&out);
    if c.sum() != crc {
        return Err(integrity(format!(
            "BGZF block at offset {off}: CRC mismatch"
        )));
    }
    Ok((total, out))
}

/// Decompress a complete BGZF stream.
fn bgzf_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < data.len() {
        let (len, block) = bgzf_read_block(data, off)?;
        out.extend_from_slice(&block);
        off += len;
    }
    Ok(out)
}

/// Compress `data` as a BGZF stream terminated by the EOF marker block.
fn bgzf_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2 + BGZF_EOF.len());
    for chunk in data.chunks(BGZF_BLOCK_DATA) {
        let mut cdata = deflate(chunk, Compression::default())?;
        if cdata.len() + BGZF_HEADER_LEN + 8 > BGZF_MAX_BLOCK {
            cdata = deflate(chunk, Compression::none())?;
        }
        let total = cdata.len() + BGZF_HEADER_LEN + 8;
        let bsize = u16::try_from(total - 1)
            .map_err(|_| integrity("BGZF block too large after compression"))?;
        out.extend_from_slice(&[
            0x1f, 0x8b, 8, 4, 0, 0, 0, 0, 0, 0xff, 6, 0, b'B', b'C', 2, 0,
        ]);
        out.extend_from_slice(&bsize.to_le_bytes());
        out.extend_from_slice(&cdata);
        let mut c = Crc::new();
        c.update(chunk);
        out.extend_from_slice(&c.sum().to_le_bytes());
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(&BGZF_EOF);
    Ok(out)
}

fn deflate(data: &[u8], level: Compression) -> Result<Vec<u8>> {
    let mut enc = DeflateEncoder::new(Vec::new(), level);
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

// ---------------------------------------------------------------------------
// SAM model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum SamLine {
    /// Header line (`@..`), raw bytes (including any trailing `\r`).
    Header(Vec<u8>),
    /// Empty (or `\r`-only) line, preserved verbatim.
    Blank(Vec<u8>),
    /// Alignment record.
    Record(SamRecord),
}

#[derive(Debug, Clone)]
struct SamRecord {
    /// Line bytes without a trailing `\r`.
    line: Vec<u8>,
    /// Line ended with `\r` (CRLF input).
    cr: bool,
    /// Byte range of the QUAL field within `line`.
    qual: Range<usize>,
}

impl SamRecord {
    fn qual_present(&self) -> bool {
        &self.line[self.qual.clone()] != b"*"
    }

    /// Optional fields (`TAG:TYPE:VALUE`) as byte slices.
    fn optional_fields(&self) -> impl Iterator<Item = &[u8]> {
        self.line.split(|&b| b == b'\t').skip(11)
    }
}

#[derive(Debug, Clone)]
struct SamDoc {
    lines: Vec<SamLine>,
    trailing_newline: bool,
}

fn valid_tag(t: &[u8]) -> bool {
    t.len() == 2 && t[0].is_ascii_alphabetic() && t[1].is_ascii_alphanumeric()
}

fn parse_num<T: std::str::FromStr>(field: &[u8]) -> Option<T> {
    std::str::from_utf8(field).ok()?.parse().ok()
}

fn valid_cigar(c: &[u8]) -> bool {
    if c == b"*" {
        return true;
    }
    let mut digits = 0usize;
    for &b in c {
        if b.is_ascii_digit() {
            digits += 1;
        } else if b"MIDNSHP=X".contains(&b) && digits > 0 {
            digits = 0;
        } else {
            return false;
        }
    }
    !c.is_empty() && digits == 0
}

fn parse_sam_record(line: Vec<u8>, cr: bool, lineno: usize) -> Result<SamRecord> {
    let bad = |m: &str| integrity(format!("SAM line {lineno}: {m}"));
    let mut ranges = Vec::with_capacity(16);
    let mut start = 0;
    for (i, &b) in line.iter().enumerate() {
        if b == b'\t' {
            ranges.push(start..i);
            start = i + 1;
        }
    }
    ranges.push(start..line.len());
    if ranges.len() < 11 {
        return Err(bad("fewer than 11 mandatory fields"));
    }
    let f = |i: usize| &line[ranges[i].clone()];
    if f(0).is_empty() || f(2).is_empty() || f(6).is_empty() {
        return Err(bad("empty mandatory field"));
    }
    parse_num::<u16>(f(1)).ok_or_else(|| bad("invalid FLAG"))?;
    let pos = parse_num::<u32>(f(3)).ok_or_else(|| bad("invalid POS"))?;
    parse_num::<u8>(f(4)).ok_or_else(|| bad("invalid MAPQ"))?;
    if !valid_cigar(f(5)) {
        return Err(bad("invalid CIGAR"));
    }
    let pnext = parse_num::<u32>(f(7)).ok_or_else(|| bad("invalid PNEXT"))?;
    parse_num::<i32>(f(8)).ok_or_else(|| bad("invalid TLEN"))?;
    if pos > i32::MAX as u32 || pnext > i32::MAX as u32 {
        return Err(bad("position out of range"));
    }
    let seq = f(9);
    if seq != b"*"
        && (seq.is_empty()
            || !seq
                .iter()
                .all(|b| b.is_ascii_alphabetic() || *b == b'=' || *b == b'.'))
    {
        return Err(bad("invalid SEQ"));
    }
    let qual = f(10);
    if qual != b"*" {
        if seq == b"*" || qual.len() != seq.len() {
            return Err(bad("QUAL length does not match SEQ"));
        }
        if !qual
            .iter()
            .all(|q| (SAM_QUAL_RANGE.0..=SAM_QUAL_RANGE.1).contains(q))
        {
            return Err(bad("QUAL character out of range"));
        }
    }
    for r in &ranges[11..] {
        let field = &line[r.clone()];
        // TAG:TYPE:VALUE — only the first two colons are separators.
        if field.len() < 5 || field[2] != b':' || field[4] != b':' {
            return Err(bad("malformed optional field"));
        }
        if !valid_tag(&field[..2]) || !b"AcCsSiIfZHB".contains(&field[3]) {
            return Err(bad("invalid optional field tag or type"));
        }
    }
    let qual = ranges[10].clone();
    Ok(SamRecord { line, cr, qual })
}

fn parse_sam(data: &[u8]) -> Result<SamDoc> {
    if data.is_empty() {
        return Err(integrity("Empty SAM file"));
    }
    let trailing_newline = data.ends_with(b"\n");
    let body = if trailing_newline {
        &data[..data.len() - 1]
    } else {
        data
    };
    let mut lines = Vec::new();
    let mut seen_record = false;
    for (i, raw) in body.split(|&b| b == b'\n').enumerate() {
        let (content, cr) = match raw.strip_suffix(b"\r") {
            Some(c) => (c, true),
            None => (raw, false),
        };
        if content.is_empty() {
            lines.push(SamLine::Blank(raw.to_vec()));
        } else if content[0] == b'@' {
            if seen_record {
                return Err(integrity(format!(
                    "SAM line {}: header line after alignment records",
                    i + 1
                )));
            }
            lines.push(SamLine::Header(raw.to_vec()));
        } else {
            seen_record = true;
            lines.push(SamLine::Record(parse_sam_record(
                content.to_vec(),
                cr,
                i + 1,
            )?));
        }
    }
    Ok(SamDoc {
        lines,
        trailing_newline,
    })
}

impl SamDoc {
    fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, l) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push(b'\n');
            }
            match l {
                SamLine::Header(b) | SamLine::Blank(b) => out.extend_from_slice(b),
                SamLine::Record(r) => {
                    out.extend_from_slice(&r.line);
                    if r.cr {
                        out.push(b'\r');
                    }
                }
            }
        }
        if self.trailing_newline {
            out.push(b'\n');
        }
        out
    }

    fn records(&self) -> impl Iterator<Item = &SamRecord> {
        self.lines.iter().filter_map(|l| match l {
            SamLine::Record(r) => Some(r),
            _ => None,
        })
    }

    fn records_mut(&mut self) -> impl Iterator<Item = &mut SamRecord> {
        self.lines.iter_mut().filter_map(|l| match l {
            SamLine::Record(r) => Some(r),
            _ => None,
        })
    }

    fn headers(&self) -> impl Iterator<Item = &[u8]> {
        self.lines.iter().filter_map(|l| match l {
            SamLine::Header(h) => Some(h.strip_suffix(b"\r").unwrap_or(h)),
            _ => None,
        })
    }

    fn add_comment(&mut self, line: Vec<u8>) {
        let at = self
            .lines
            .iter()
            .rposition(|l| matches!(l, SamLine::Header(_)))
            .map_or(0, |i| i + 1);
        self.lines.insert(at, SamLine::Header(line));
    }
}

// ---------------------------------------------------------------------------
// BAM model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BamRecord {
    /// Record bytes following `block_size`.
    data: Vec<u8>,
    /// Byte range of the quality scores within `data`.
    qual: Range<usize>,
    /// Offset of the auxiliary data within `data`.
    aux: usize,
}

impl BamRecord {
    fn qual_usable(&self) -> bool {
        let q = &self.data[self.qual.clone()];
        !q.is_empty() && q.iter().all(|&v| v <= BAM_QUAL_RANGE.1)
    }

    /// Iterate auxiliary fields as (tag, type, value bytes).
    fn aux_fields(&self) -> Vec<([u8; 2], u8, &[u8])> {
        // Already validated in parse; this cannot fail.
        parse_aux(&self.data[self.aux..]).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct BamDoc {
    text: Vec<u8>,
    /// `n_ref` plus the reference entries, verbatim.
    refs: Vec<u8>,
    records: Vec<BamRecord>,
}

fn aux_type_size(t: u8) -> Option<usize> {
    Some(match t {
        b'A' | b'c' | b'C' => 1,
        b's' | b'S' => 2,
        b'i' | b'I' | b'f' => 4,
        _ => return None,
    })
}

/// A parsed BAM auxiliary field: (tag, type code, raw value bytes).
type AuxField<'a> = ([u8; 2], u8, &'a [u8]);

/// Parse and validate BAM auxiliary data.
fn parse_aux(aux: &[u8]) -> Result<Vec<AuxField<'_>>> {
    let bad = |m: &str| integrity(format!("BAM auxiliary data: {m}"));
    let mut out = Vec::new();
    let mut p = 0usize;
    while p < aux.len() {
        let h = aux.get(p..p + 3).ok_or_else(|| bad("truncated field"))?;
        let tag = [h[0], h[1]];
        if !valid_tag(&tag) {
            return Err(bad("invalid tag"));
        }
        let ty = h[2];
        p += 3;
        let len = match ty {
            b'Z' | b'H' => {
                let nul = aux[p..]
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| bad("unterminated string"))?;
                nul + 1
            }
            b'B' => {
                let sub = *aux.get(p).ok_or_else(|| bad("truncated array"))?;
                let size = aux_type_size(sub)
                    .filter(|_| sub != b'A')
                    .ok_or_else(|| bad("invalid array subtype"))?;
                let n = le_u32(aux, p + 1).ok_or_else(|| bad("truncated array"))? as usize;
                n.checked_mul(size)
                    .and_then(|b| b.checked_add(5))
                    .ok_or_else(|| bad("array too large"))?
            }
            t => aux_type_size(t).ok_or_else(|| bad("invalid type"))?,
        };
        let end = p.checked_add(len).ok_or_else(|| bad("overflow"))?;
        let value = aux.get(p..end).ok_or_else(|| bad("truncated value"))?;
        out.push((tag, ty, value));
        p = end;
    }
    Ok(out)
}

fn parse_bam_record(block: &[u8], index: usize) -> Result<BamRecord> {
    let bad = |m: &str| integrity(format!("BAM record {index}: {m}"));
    if block.len() < 32 {
        return Err(bad("block_size < 32"));
    }
    let l_read_name = block[8] as usize;
    if l_read_name == 0 {
        return Err(bad("empty read name"));
    }
    let n_cigar = le_u16(block, 12).ok_or_else(|| bad("truncated"))? as usize;
    let l_seq = le_i32(block, 16).ok_or_else(|| bad("truncated"))?;
    let l_seq = usize::try_from(l_seq).map_err(|_| bad("negative l_seq"))?;
    let name_end = 32 + l_read_name;
    let cigar_end = n_cigar
        .checked_mul(4)
        .and_then(|c| name_end.checked_add(c))
        .ok_or_else(|| bad("overflow"))?;
    let seq_end = cigar_end
        .checked_add(l_seq.div_ceil(2))
        .ok_or_else(|| bad("overflow"))?;
    let qual_end = seq_end.checked_add(l_seq).ok_or_else(|| bad("overflow"))?;
    if qual_end > block.len() {
        return Err(bad("variable-length fields exceed block_size"));
    }
    if block[name_end - 1] != 0 {
        return Err(bad("read name not NUL-terminated"));
    }
    parse_aux(&block[qual_end..]).map_err(|e| bad(&e.to_string()))?;
    Ok(BamRecord {
        data: block.to_vec(),
        qual: seq_end..qual_end,
        aux: qual_end,
    })
}

/// Decode raw (decompressed) BAM bytes.
fn parse_bam_raw(data: &[u8]) -> Result<BamDoc> {
    if data.len() < 12 || data[..4] != BAM_MAGIC {
        return Err(integrity("Invalid BAM magic bytes"));
    }
    let l_text = le_u32(data, 4).ok_or_else(|| integrity("BAM header truncated"))? as usize;
    let text_end = 8usize
        .checked_add(l_text)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| integrity("BAM header text truncated"))?;
    let text = data[8..text_end].to_vec();
    let n_ref = le_u32(data, text_end).ok_or_else(|| integrity("BAM n_ref truncated"))? as usize;
    let mut off = text_end + 4;
    for i in 0..n_ref {
        let bad = || integrity(format!("BAM reference {i} truncated or invalid"));
        let l_name = le_u32(data, off).ok_or_else(bad)? as usize;
        if l_name == 0 {
            return Err(bad());
        }
        let name_end = off
            .checked_add(4)
            .and_then(|o| o.checked_add(l_name))
            .filter(|&e| e <= data.len())
            .ok_or_else(bad)?;
        if data[name_end - 1] != 0 {
            return Err(bad());
        }
        le_u32(data, name_end).ok_or_else(bad)?;
        off = name_end + 4;
    }
    let refs = data[text_end..off].to_vec();
    let mut records = Vec::new();
    while off < data.len() {
        let index = records.len();
        let block_size = le_u32(data, off)
            .ok_or_else(|| integrity(format!("BAM record {index}: truncated block_size")))?
            as usize;
        let start = off + 4;
        let end = start
            .checked_add(block_size)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| integrity(format!("BAM record {index}: truncated")))?;
        records.push(parse_bam_record(&data[start..end], index)?);
        off = end;
    }
    Ok(BamDoc {
        text,
        refs,
        records,
    })
}

/// Decode a BAM file (BGZF-compressed or raw `BAM\1`).
fn parse_bam(data: &[u8]) -> Result<BamDoc> {
    if data.starts_with(&BAM_MAGIC) {
        parse_bam_raw(data)
    } else if data.starts_with(&[0x1f, 0x8b]) {
        parse_bam_raw(&bgzf_decompress(data)?)
    } else {
        Err(integrity("Not a BAM file (expected BGZF or BAM\\1 magic)"))
    }
}

impl BamDoc {
    fn serialize(&self) -> Result<Vec<u8>> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&BAM_MAGIC);
        let l_text =
            u32::try_from(self.text.len()).map_err(|_| integrity("BAM header too large"))?;
        raw.extend_from_slice(&l_text.to_le_bytes());
        raw.extend_from_slice(&self.text);
        raw.extend_from_slice(&self.refs);
        for r in &self.records {
            let bs = u32::try_from(r.data.len()).map_err(|_| integrity("BAM record too large"))?;
            raw.extend_from_slice(&bs.to_le_bytes());
            raw.extend_from_slice(&r.data);
        }
        bgzf_compress(&raw)
    }

    /// Header text lines (trailing NULs / `\r` stripped).
    fn header_lines(&self) -> impl Iterator<Item = &[u8]> {
        let end = self.text.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        self.text[..end]
            .split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
    }

    fn add_comment(&mut self, line: Vec<u8>) {
        let end = self.text.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        let pad = self.text.split_off(end);
        if !self.text.is_empty() && !self.text.ends_with(b"\n") {
            self.text.push(b'\n');
        }
        self.text.extend_from_slice(&line);
        self.text.push(b'\n');
        self.text.extend_from_slice(&pad);
    }
}

// ---------------------------------------------------------------------------
// Unified document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Doc {
    Sam(SamDoc),
    Bam(BamDoc),
}

impl Doc {
    fn parse(format: &DataFormat, data: &[u8]) -> Result<Self> {
        match format {
            DataFormat::SAM => Ok(Doc::Sam(parse_sam(data)?)),
            DataFormat::BAM => Ok(Doc::Bam(parse_bam(data)?)),
            f => Err(SteganoError::UnsupportedFormat(f.clone())),
        }
    }

    fn serialize(&self) -> Result<Vec<u8>> {
        match self {
            Doc::Sam(s) => Ok(s.serialize()),
            Doc::Bam(b) => b.serialize(),
        }
    }

    fn record_count(&self) -> usize {
        match self {
            Doc::Sam(s) => s.records().count(),
            Doc::Bam(b) => b.records.len(),
        }
    }

    fn qual_range(&self) -> (u8, u8) {
        match self {
            Doc::Sam(_) => SAM_QUAL_RANGE,
            Doc::Bam(_) => BAM_QUAL_RANGE,
        }
    }

    /// Quality values usable for embedding, in record order.
    fn qual_slices(&self) -> Vec<&[u8]> {
        match self {
            Doc::Sam(s) => s
                .records()
                .filter(|r| r.qual_present())
                .map(|r| &r.line[r.qual.clone()])
                .collect(),
            Doc::Bam(b) => b
                .records
                .iter()
                .filter(|r| r.qual_usable())
                .map(|r| &r.data[r.qual.clone()])
                .collect(),
        }
    }

    fn qual_slices_mut(&mut self) -> Vec<&mut [u8]> {
        match self {
            Doc::Sam(s) => s
                .records_mut()
                .filter(|r| r.qual_present())
                .map(|r| &mut r.line[r.qual.clone()])
                .collect(),
            Doc::Bam(b) => b
                .records
                .iter_mut()
                .filter(|r| r.qual_usable())
                .map(|r| &mut r.data[r.qual.clone()])
                .collect(),
        }
    }

    fn quality_capacity(&self) -> usize {
        self.qual_slices().iter().map(|q| q.len()).sum::<usize>() / 8
    }

    /// Does any record already carry a `zs` tag (of any type)?
    fn has_tag(&self) -> bool {
        match self {
            Doc::Sam(s) => s
                .records()
                .any(|r| r.optional_fields().any(|f| f.starts_with(b"zs:"))),
            Doc::Bam(b) => b
                .records
                .iter()
                .any(|r| r.aux_fields().iter().any(|(t, _, _)| *t == TAG)),
        }
    }

    /// Values of all `zs:Z` tags, in record order.
    fn tag_values(&self) -> Vec<Vec<u8>> {
        match self {
            Doc::Sam(s) => s
                .records()
                .flat_map(|r| {
                    r.optional_fields()
                        .filter_map(|f| f.strip_prefix(SAM_TAG_PREFIX))
                        .map(|v| v.to_vec())
                        .collect::<Vec<_>>()
                })
                .collect(),
            Doc::Bam(b) => b
                .records
                .iter()
                .flat_map(|r| {
                    r.aux_fields()
                        .into_iter()
                        .filter(|(t, ty, _)| *t == TAG && *ty == b'Z')
                        .map(|(_, _, v)| v[..v.len() - 1].to_vec())
                        .collect::<Vec<_>>()
                })
                .collect(),
        }
    }

    /// Attach `zs:Z:<value>` to the `i`-th record.
    fn add_tag(&mut self, i: usize, value: &[u8]) {
        match self {
            Doc::Sam(s) => {
                if let Some(r) = s.records_mut().nth(i) {
                    r.line.push(b'\t');
                    r.line.extend_from_slice(SAM_TAG_PREFIX);
                    r.line.extend_from_slice(value);
                }
            }
            Doc::Bam(b) => {
                if let Some(r) = b.records.get_mut(i) {
                    r.data.extend_from_slice(&TAG);
                    r.data.push(b'Z');
                    r.data.extend_from_slice(value);
                    r.data.push(0);
                }
            }
        }
    }

    /// Payloads of `@CO\tzs:` header lines.
    fn comment_values(&self) -> Vec<Vec<u8>> {
        let lines: Vec<&[u8]> = match self {
            Doc::Sam(s) => s.headers().collect(),
            Doc::Bam(b) => b.header_lines().collect(),
        };
        lines
            .into_iter()
            .filter_map(|l| l.strip_prefix(COMMENT_PREFIX))
            .map(|v| v.to_vec())
            .collect()
    }

    fn add_comment(&mut self, value: &[u8]) {
        let mut line = COMMENT_PREFIX.to_vec();
        line.extend_from_slice(value);
        match self {
            Doc::Sam(s) => s.add_comment(line),
            Doc::Bam(b) => b.add_comment(line),
        }
    }
}

// ---------------------------------------------------------------------------
// Envelope and channels
// ---------------------------------------------------------------------------

fn build_envelope(strategy: SamBamEmbedStrategy, codec: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(codec.len())
        .map_err(|_| SteganoError::InvalidParameter("Payload too large".into()))?;
    let mut env = Vec::with_capacity(ENV_HEADER_LEN + codec.len());
    env.extend_from_slice(&ENV_MAGIC);
    env.push(strategy.to_u8());
    env.extend_from_slice(&len.to_le_bytes());
    env.extend_from_slice(codec);
    Ok(env)
}

/// Parse an envelope header: (strategy, codec length).
fn parse_envelope_header(h: &[u8]) -> Option<(SamBamEmbedStrategy, usize)> {
    if h.len() < ENV_HEADER_LEN || h[..4] != ENV_MAGIC {
        return None;
    }
    let s = SamBamEmbedStrategy::from_u8(h[4])?;
    Some((s, le_u32(h, 5)? as usize))
}

/// Validate a complete envelope; return (strategy, codec bytes).
fn parse_envelope(env: &[u8]) -> Option<(SamBamEmbedStrategy, &[u8])> {
    let (s, len) = parse_envelope_header(env)?;
    let body = env.get(ENV_HEADER_LEN..ENV_HEADER_LEN.checked_add(len)?)?;
    (env.len() == ENV_HEADER_LEN + len).then_some((s, body))
}

fn insufficient(needed_bytes: usize, available_bytes: usize) -> SteganoError {
    SteganoError::InsufficientCapacity {
        needed: needed_bytes.saturating_mul(8),
        available: available_bytes.saturating_mul(8),
    }
}

fn embed_tags(doc: &mut Doc, env: &[u8], spread: bool) -> Result<usize> {
    let n = doc.record_count();
    if env.len() > n.saturating_mul(MAX_CHUNK) {
        return Err(insufficient(env.len(), n * MAX_CHUNK));
    }
    let chunk = if spread {
        env.len().div_ceil(n)
    } else {
        MAX_CHUNK
    };
    let chunks: Vec<&[u8]> = env.chunks(chunk).collect();
    for (i, c) in chunks.iter().enumerate() {
        let value = format!("{i},{}", hex::encode(c));
        doc.add_tag(i, value.as_bytes());
    }
    Ok(chunks.len())
}

fn extract_tags(doc: &Doc) -> Option<Vec<u8>> {
    let mut chunks: Vec<(usize, Vec<u8>)> = Vec::new();
    for v in doc.tag_values() {
        let s = std::str::from_utf8(&v).ok()?;
        let (idx, hex_part) = s.split_once(',')?;
        chunks.push((idx.parse().ok()?, hex::decode(hex_part).ok()?));
    }
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|(i, _)| *i);
    if chunks.iter().enumerate().any(|(k, (i, _))| k != *i) {
        return None;
    }
    Some(chunks.into_iter().flat_map(|(_, c)| c).collect())
}

fn embed_quality(doc: &mut Doc, env: &[u8]) -> Result<usize> {
    let cap = doc.quality_capacity();
    if env.len() > cap {
        return Err(insufficient(env.len(), cap));
    }
    let (lo, hi) = doc.qual_range();
    let mut bits = env
        .iter()
        .flat_map(|&byte| (0..8).rev().map(move |k| (byte >> k) & 1));
    let mut touched = 0;
    'outer: for q in doc.qual_slices_mut() {
        touched += 1;
        for v in q.iter_mut() {
            let Some(bit) = bits.next() else {
                break 'outer;
            };
            let mut nv = (*v & !1) | bit;
            if nv < lo {
                nv += 2;
            } else if nv > hi {
                nv -= 2;
            }
            *v = nv;
        }
    }
    Ok(touched)
}

fn quality_bytes(doc: &Doc, max: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut acc = 0u8;
    let mut n = 0;
    for q in doc.qual_slices() {
        for &v in q {
            acc = (acc << 1) | (v & 1);
            n += 1;
            if n == 8 {
                out.push(acc);
                if out.len() == max {
                    return out;
                }
                acc = 0;
                n = 0;
            }
        }
    }
    out
}

fn extract_quality(doc: &Doc) -> Option<Vec<u8>> {
    let head = quality_bytes(doc, ENV_HEADER_LEN);
    let (s, len) = parse_envelope_header(&head)?;
    if s != SamBamEmbedStrategy::QualityScores || len > doc.quality_capacity() {
        return None;
    }
    let env = quality_bytes(doc, ENV_HEADER_LEN + len);
    (env.len() == ENV_HEADER_LEN + len).then_some(env)
}

fn extract_comment(doc: &Doc) -> Option<Vec<u8>> {
    doc.comment_values()
        .into_iter()
        .find_map(|v| hex::decode(v).ok().filter(|e| parse_envelope(e).is_some()))
}

/// Locate the embedded envelope; returns (strategy, codec bytes).
fn find_envelope(doc: &Doc) -> Option<(SamBamEmbedStrategy, Vec<u8>)> {
    let tagged = extract_tags(doc).and_then(|e| {
        parse_envelope(&e)
            .filter(|(s, _)| {
                matches!(
                    s,
                    SamBamEmbedStrategy::OptionalFields | SamBamEmbedStrategy::AuxiliaryData
                )
            })
            .map(|(s, b)| (s, b.to_vec()))
    });
    tagged
        .or_else(|| {
            extract_comment(doc).and_then(|e| {
                parse_envelope(&e)
                    .filter(|(s, _)| *s == SamBamEmbedStrategy::CommentLines)
                    .map(|(s, b)| (s, b.to_vec()))
            })
        })
        .or_else(|| {
            extract_quality(doc).and_then(|e| parse_envelope(&e).map(|(s, b)| (s, b.to_vec())))
        })
}

fn strategy_from_params(params: &EmbedParams) -> Result<SamBamEmbedStrategy> {
    match params.format_specific.get("sam_strategy") {
        None => Ok(SamBamEmbedStrategy::OptionalFields),
        Some(v) => v
            .as_str()
            .and_then(SamBamEmbedStrategy::from_name)
            .ok_or_else(|| {
                SteganoError::InvalidParameter(format!(
                    "Unknown sam_strategy {v}; expected optional|comment|quality|auxiliary|multi"
                ))
            }),
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

impl Default for SamBamHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SamBamHandler {
    pub fn new() -> Self {
        Self
    }

    fn is_bam(&self, data: &[u8]) -> bool {
        if data.starts_with(&BAM_MAGIC) {
            return true;
        }
        if !is_bgzf(data) {
            return false;
        }
        matches!(bgzf_read_block(data, 0), Ok((_, b)) if b.starts_with(&BAM_MAGIC))
    }

    fn is_sam(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }
        let prefix = &data[..data.len().min(64 * 1024)];
        let text = String::from_utf8_lossy(prefix);
        let lines: Vec<&str> = text.lines().take(10).collect();
        let has_headers = lines.iter().any(|line| line.starts_with('@'));
        let has_alignments = lines
            .iter()
            .any(|line| !line.starts_with('@') && line.split('\t').count() >= 11);
        has_headers || has_alignments
    }

    /// Capacity (envelope bytes) of a channel.
    fn channel_capacity(doc: &Doc, strategy: SamBamEmbedStrategy, data_len: usize) -> usize {
        match strategy {
            SamBamEmbedStrategy::OptionalFields | SamBamEmbedStrategy::AuxiliaryData => {
                doc.record_count().saturating_mul(MAX_CHUNK)
            }
            SamBamEmbedStrategy::QualityScores => doc.quality_capacity(),
            // Unbounded in principle; report a conservative figure.
            SamBamEmbedStrategy::CommentLines => (data_len / 10).max(1024),
            SamBamEmbedStrategy::MultiStrategy => {
                doc.record_count().saturating_mul(MAX_CHUNK)
                    + doc.quality_capacity()
                    + (data_len / 10).max(1024)
            }
        }
    }

    fn estimate_capacity_internal(
        &self,
        data: &[u8],
        format: &DataFormat,
        strategy: SamBamEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let doc = Doc::parse(format, data)?;
        let cap = Self::channel_capacity(&doc, strategy, data.len());
        let usable = cap.saturating_sub(ENV_HEADER_LEN + CODEC_MIN_OVERHEAD);
        let total_bits = cap * 8;
        let recommended_bits = usable * 8;

        let mut format_info = HashMap::new();
        format_info.insert(
            "method".to_string(),
            serde_json::Value::String(format!("{strategy:?}")),
        );
        format_info.insert(
            "alignment_records".to_string(),
            serde_json::Value::Number(doc.record_count().into()),
        );
        format_info.insert(
            "quality_capacity_bytes".to_string(),
            serde_json::Value::Number(doc.quality_capacity().into()),
        );
        format_info.insert(
            "estimated_capacity_bytes".to_string(),
            serde_json::Value::Number(usable.into()),
        );
        format_info.insert(
            "is_bam".to_string(),
            serde_json::Value::Bool(matches!(doc, Doc::Bam(_))),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: if total_bits > 0 {
                recommended_bits as f64 / total_bits as f64
            } else {
                0.0
            },
            format_specific_info: format_info,
        })
    }

    fn embed_doc(
        doc: &mut Doc,
        codec: &[u8],
        strategy: SamBamEmbedStrategy,
    ) -> Result<SamBamEmbedStrategy> {
        if find_envelope(doc).is_some() {
            return Err(SteganoError::InvalidParameter(
                "Carrier already contains an embedded payload".into(),
            ));
        }
        let uses_tag = matches!(
            strategy,
            SamBamEmbedStrategy::OptionalFields | SamBamEmbedStrategy::AuxiliaryData
        );
        if uses_tag && doc.has_tag() {
            return Err(SteganoError::InvalidParameter(
                "Carrier records already carry a 'zs' tag".into(),
            ));
        }
        match strategy {
            SamBamEmbedStrategy::OptionalFields => {
                embed_tags(doc, &build_envelope(strategy, codec)?, true)?;
            }
            SamBamEmbedStrategy::AuxiliaryData => {
                embed_tags(doc, &build_envelope(strategy, codec)?, false)?;
            }
            SamBamEmbedStrategy::QualityScores => {
                embed_quality(doc, &build_envelope(strategy, codec)?)?;
            }
            SamBamEmbedStrategy::CommentLines => {
                let env = build_envelope(strategy, codec)?;
                doc.add_comment(hex::encode(env).as_bytes());
            }
            SamBamEmbedStrategy::MultiStrategy => {
                let need = ENV_HEADER_LEN + codec.len();
                let chosen = if !doc.has_tag() && need <= doc.record_count() * MAX_CHUNK {
                    SamBamEmbedStrategy::OptionalFields
                } else if need <= doc.quality_capacity() {
                    SamBamEmbedStrategy::QualityScores
                } else {
                    SamBamEmbedStrategy::CommentLines
                };
                return Self::embed_doc(doc, codec, chosen);
            }
        }
        Ok(strategy)
    }
}

#[async_trait]
impl DataHandler for SamBamHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_bam(data) {
            Some(DataFormat::BAM)
        } else if self.is_sam(data) {
            Some(DataFormat::SAM)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        self.estimate_capacity_internal(
            &carrier.raw_data,
            &carrier.format,
            SamBamEmbedStrategy::OptionalFields,
        )
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start_time = Instant::now();
        let strategy = strategy_from_params(params)?;
        let mut doc = Doc::parse(&carrier.format, &carrier.raw_data)?;

        let codec_start = Instant::now();
        let codec = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = codec_start.elapsed().as_millis() as u64;

        let used = Self::embed_doc(&mut doc, &codec, strategy)?;
        log::debug!("SAM/BAM: embedded {} bytes via {used:?}", payload.len());
        carrier.raw_data = doc.serialize()?;

        Ok(EmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            carrier_modified: true,
            performance_stats: PerformanceStats {
                embedding_time_ms: start_time.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + payload.len(),
            },
            validation_report: None,
        })
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let doc = Doc::parse(&carrier.format, &carrier.raw_data)?;
        let (_strategy, codec) = find_envelope(&doc)
            .ok_or_else(|| integrity("No steganographic data found in SAM/BAM carrier"))?;
        crate::codec::decode_payload(&codec, params)
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::SAM | DataFormat::BAM => {
                Ok(Doc::parse(&carrier.format, &carrier.raw_data).is_ok())
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noodles_sam::alignment::io::Write as _;
    use noodles_sam::alignment::RecordBuf;
    use rand::{Rng, SeedableRng};

    const STRATEGIES: [&str; 5] = ["optional", "auxiliary", "quality", "comment", "multi"];

    /// Records cover: QUAL `*`, boundary QUAL chars `!`/`~`, a Z value with
    /// colons, unmapped reads, a paired read with `=` RNEXT.
    fn sam_fixture(repeats: usize) -> Vec<u8> {
        let header = "@HD\tVN:1.6\tSO:unsorted\n\
                      @SQ\tSN:chr1\tLN:248956422\n\
                      @SQ\tSN:chr2\tLN:242193529\n\
                      @RG\tID:rg1\tSM:S1\tPL:ILLUMINA\n\
                      @PG\tID:bwa\tPN:bwa\tVN:0.7.17\tCL:bwa mem ref.fa r.fq\n\
                      @CO\tfree text comment: with colons\n";
        let records = [
            "r001\t99\tchr1\t7\t30\t8M2I4M1D3M\t=\t37\t39\tTTAGATAAAGGATACTG\t*\tAS:i:18\tXS:i:12\tMD:Z:6A5",
            "r002\t0\tchr1\t9\t30\t3S6M1P1I4M\t*\t0\t0\tAAAAGATAAGGATA\t!!~~!!~~IIIIII\tXC:Z:a:b:c\tNM:i:1",
            "r003\t0\tchr2\t100\t60\t10M\t*\t0\t0\tGCCTAAGCTA\tIIIIIIIIII\tRG:Z:rg1",
            "r004\t4\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\t#########~",
            "r005\t147\tchr1\t25\t30\t10M\t=\t7\t-25\tGCCTAAGCTA\tHHHHHHHHHH\tAS:i:20\tMD:Z:10",
        ];
        let mut out = header.to_string();
        for _ in 0..repeats {
            for r in &records {
                out.push_str(r);
                out.push('\n');
            }
        }
        out.into_bytes()
    }

    fn params(strategy: Option<&str>, key: &[u8], ec: ECLevel) -> EmbedParams {
        let mut p = EmbedParams {
            encryption_key: key.to_vec(),
            error_correction_level: ec,
            ..Default::default()
        };
        if let Some(s) = strategy {
            p.format_specific
                .insert("sam_strategy".into(), serde_json::Value::String(s.into()));
        }
        p
    }

    // ---- test-only SAM -> BAM encoder ---------------------------------------

    fn reg2bin(beg: i64, end: i64) -> u16 {
        let end = end - 1;
        for (shift, off) in [(14, 4681), (17, 585), (20, 73), (23, 9), (26, 1)] {
            if beg >> shift == end >> shift {
                return (off + (beg >> shift)) as u16;
            }
        }
        0
    }

    /// Encode SAM text as raw (uncompressed) BAM.
    fn sam_to_raw_bam(sam: &[u8]) -> Vec<u8> {
        let text = std::str::from_utf8(sam).unwrap();
        let mut header = String::new();
        let mut refs: Vec<(String, u32)> = Vec::new();
        let mut recs = Vec::new();
        for line in text.lines() {
            if line.starts_with('@') {
                header.push_str(line);
                header.push('\n');
                if let Some(rest) = line.strip_prefix("@SQ\t") {
                    let mut sn = String::new();
                    let mut ln = 0;
                    for f in rest.split('\t') {
                        if let Some(v) = f.strip_prefix("SN:") {
                            sn = v.to_string();
                        } else if let Some(v) = f.strip_prefix("LN:") {
                            ln = v.parse().unwrap();
                        }
                    }
                    refs.push((sn, ln));
                }
            } else {
                recs.push(line);
            }
        }
        let refid = |n: &str| -> i32 {
            if n == "*" {
                -1
            } else {
                refs.iter().position(|(s, _)| s == n).unwrap() as i32
            }
        };
        let mut out = BAM_MAGIC.to_vec();
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(&(refs.len() as u32).to_le_bytes());
        for (n, l) in &refs {
            out.extend_from_slice(&(n.len() as u32 + 1).to_le_bytes());
            out.extend_from_slice(n.as_bytes());
            out.push(0);
            out.extend_from_slice(&l.to_le_bytes());
        }
        for line in recs {
            let f: Vec<&str> = line.split('\t').collect();
            let rid = refid(f[2]);
            let pos = f[3].parse::<i32>().unwrap() - 1;
            let next = match f[6] {
                "=" => rid,
                n => refid(n),
            };
            let mut cigar = Vec::new();
            let mut span = 0i64;
            if f[5] != "*" {
                let mut num = 0u32;
                for c in f[5].bytes() {
                    if c.is_ascii_digit() {
                        num = num * 10 + (c - b'0') as u32;
                    } else {
                        let op = b"MIDNSHP=X".iter().position(|&o| o == c).unwrap() as u32;
                        if b"MDN=X".contains(&c) {
                            span += num as i64;
                        }
                        cigar.push((num << 4) | op);
                        num = 0;
                    }
                }
            }
            let seq: &[u8] = if f[9] == "*" { b"" } else { f[9].as_bytes() };
            let mut rec = Vec::new();
            rec.extend_from_slice(&rid.to_le_bytes());
            rec.extend_from_slice(&pos.to_le_bytes());
            rec.push(f[0].len() as u8 + 1);
            rec.push(f[4].parse::<u8>().unwrap());
            let beg = pos.max(0) as i64;
            let end = if span == 0 { beg + 1 } else { beg + span };
            rec.extend_from_slice(&reg2bin(beg, end).to_le_bytes());
            rec.extend_from_slice(&(cigar.len() as u16).to_le_bytes());
            rec.extend_from_slice(&f[1].parse::<u16>().unwrap().to_le_bytes());
            rec.extend_from_slice(&(seq.len() as u32).to_le_bytes());
            rec.extend_from_slice(&next.to_le_bytes());
            rec.extend_from_slice(&(f[7].parse::<i32>().unwrap() - 1).to_le_bytes());
            rec.extend_from_slice(&f[8].parse::<i32>().unwrap().to_le_bytes());
            rec.extend_from_slice(f[0].as_bytes());
            rec.push(0);
            for c in &cigar {
                rec.extend_from_slice(&c.to_le_bytes());
            }
            let code = |b: u8| {
                b"=ACMGRSVTWYHKDBN"
                    .iter()
                    .position(|&x| x == b.to_ascii_uppercase())
                    .unwrap_or(15) as u8
            };
            for pair in seq.chunks(2) {
                let hi = code(pair[0]) << 4;
                let lo = pair.get(1).map_or(0, |&b| code(b));
                rec.push(hi | lo);
            }
            if f[10] == "*" {
                rec.extend(std::iter::repeat_n(0xff, seq.len()));
            } else {
                rec.extend(f[10].bytes().map(|q| q - 33));
            }
            for opt in &f[11..] {
                let (tag, rest) = opt.split_at(2);
                rec.extend_from_slice(tag.as_bytes());
                let (ty, val) = (rest.as_bytes()[1], &rest[3..]);
                match ty {
                    b'i' => {
                        rec.push(b'i');
                        rec.extend_from_slice(&val.parse::<i32>().unwrap().to_le_bytes());
                    }
                    b'Z' | b'H' => {
                        rec.push(ty);
                        rec.extend_from_slice(val.as_bytes());
                        rec.push(0);
                    }
                    b'A' => {
                        rec.push(b'A');
                        rec.push(val.as_bytes()[0]);
                    }
                    t => panic!("unsupported fixture type {t}"),
                }
            }
            out.extend_from_slice(&(rec.len() as u32).to_le_bytes());
            out.extend_from_slice(&rec);
        }
        out
    }

    fn bam_fixture(repeats: usize) -> Vec<u8> {
        bgzf_compress(&sam_to_raw_bam(&sam_fixture(repeats))).unwrap()
    }

    // ---- independent validation (noodles) -----------------------------------

    fn check_records(recs: &[RecordBuf]) {
        for rec in recs {
            for (tag, _) in rec.data().iter() {
                assert!(valid_tag(tag.as_ref()), "invalid tag {tag:?}");
            }
            let q = rec.quality_scores().as_ref();
            assert!(q.iter().all(|&v| v <= 93), "QUAL out of range");
            assert!(q.is_empty() || q.len() == rec.sequence().len());
        }
    }

    fn noodles_sam(data: &[u8]) -> (noodles_sam::Header, Vec<RecordBuf>) {
        let mut r = noodles_sam::io::Reader::new(data);
        let header = r.read_header().expect("noodles SAM header");
        let recs = r
            .record_bufs(&header)
            .collect::<std::io::Result<Vec<_>>>()
            .expect("noodles SAM records");
        check_records(&recs);
        (header, recs)
    }

    fn noodles_bam(data: &[u8]) -> (noodles_sam::Header, Vec<RecordBuf>) {
        let mut r = noodles_bam::io::Reader::new(data);
        let header = r.read_header().expect("noodles BAM header");
        let recs = r
            .record_bufs(&header)
            .collect::<std::io::Result<Vec<_>>>()
            .expect("noodles BAM records");
        check_records(&recs);
        (header, recs)
    }

    /// Render records as SAM text with noodles, normalising representation
    /// details (e.g. BAM's `C` vs SAM's `i` integer widths) that differ
    /// between the two readers but not in meaning.
    fn as_sam_text(header: &noodles_sam::Header, recs: &[RecordBuf]) -> Vec<u8> {
        let mut w = noodles_sam::io::Writer::new(Vec::new());
        for r in recs {
            w.write_alignment_record(header, r).unwrap();
        }
        w.into_inner()
    }

    fn noodles_bgzf_inflate(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        noodles_bgzf::io::Reader::new(data)
            .read_to_end(&mut out)
            .expect("noodles BGZF");
        out
    }

    /// Raw quality scores per record (empty = absent).
    fn quals(recs: &[RecordBuf]) -> Vec<Vec<u8>> {
        recs.iter()
            .map(|r| r.quality_scores().as_ref().to_vec())
            .collect()
    }

    // ---- BGZF ---------------------------------------------------------------

    #[test]
    fn bgzf_roundtrip_and_noodles_compat() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let data: Vec<u8> = (0..200_000).map(|_| rng.gen_range(0..4u8) + b'A').collect();
        let z = bgzf_compress(&data).unwrap();
        assert!(z.ends_with(&BGZF_EOF));
        assert!(is_bgzf(&z));
        let mut off = 0;
        let mut blocks = 0;
        while off < z.len() {
            let (len, block) = bgzf_read_block(&z, off).unwrap();
            assert!(len <= BGZF_MAX_BLOCK && block.len() <= BGZF_BLOCK_DATA);
            off += len;
            blocks += 1;
        }
        assert_eq!(blocks, 200_000usize.div_ceil(BGZF_BLOCK_DATA) + 1);
        assert_eq!(bgzf_decompress(&z).unwrap(), data);
        assert_eq!(noodles_bgzf_inflate(&z), data);

        // Reading BGZF written by noodles.
        let mut w = noodles_bgzf::io::Writer::new(Vec::new());
        w.write_all(&data).unwrap();
        let theirs = w.finish().unwrap();
        assert_eq!(bgzf_decompress(&theirs).unwrap(), data);
        assert_eq!(bgzf_decompress(&BGZF_EOF).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn bgzf_rejects_corruption() {
        let msg = b"hello world hello world";
        let z = bgzf_compress(msg).unwrap();
        for i in 0..z.len() {
            let mut c = z.clone();
            c[i] ^= 0x55;
            // Never panics; a flip is either rejected or (in ignorable gzip
            // header bytes such as MTIME/OS) harmless.
            if let Ok(d) = bgzf_decompress(&c) {
                assert_eq!(d, msg, "undetected corruption at byte {i}");
            }
        }
        assert!(bgzf_decompress(&z[..z.len() - 30]).is_err());
    }

    // ---- SAM parsing --------------------------------------------------------

    #[test]
    fn sam_parse_preserves_bytes() {
        let sam = sam_fixture(2);
        let doc = parse_sam(&sam).unwrap();
        assert_eq!(doc.serialize(), sam);
        assert_eq!(doc.records().count(), 10);
        // CRLF and a missing final newline are preserved.
        let crlf: &[u8] =
            b"@HD\tVN:1.6\r\nr\t4\t*\t0\t0\t*\t*\t0\t0\tAC\t!!\r\nr2\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*";
        assert_eq!(parse_sam(crlf).unwrap().serialize(), crlf.to_vec());
    }

    #[test]
    fn sam_parse_errors_not_dropped() {
        let bad: &[&[u8]] = &[
            b"@HD\tVN:1.6\nr1\t0\tchr1\t1\t0\t*\t*\t0\t0\tA\t!\nshort\tline\n",
            b"r1\tx\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
            b"r1\t0\t*\t0\t0\t*\t*\t0\t0\tACGT\tII\n",
            b"r1\t0\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII\tZSX0:Z:a\n",
            b"r1\t0\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII\tnotatag\n",
            b"r1\t0\t*\t0\t0\t*\t*\t0\t0\t*\t*\n@HD\tVN:1.6\n",
            b"r1\t0\t*\t0\t0\t5Q\t*\t0\t0\t*\t*\n",
            b"",
        ];
        for b in bad {
            assert!(parse_sam(b).is_err(), "{:?}", String::from_utf8_lossy(b));
        }
    }

    #[test]
    fn sam_z_values_with_colons_kept() {
        let doc = parse_sam(&sam_fixture(1)).unwrap();
        let r = doc.records().nth(1).unwrap();
        assert!(r.optional_fields().any(|f| f == b"XC:Z:a:b:c"));
    }

    // ---- SAM round trips ----------------------------------------------------

    async fn embed_extract(
        format: DataFormat,
        carrier: Vec<u8>,
        payload: &[u8],
        p: &EmbedParams,
    ) -> Vec<u8> {
        let h = SamBamHandler::new();
        let mut c = CarrierData::new(format, carrier);
        let r = h.embed(&mut c, payload, p).await.unwrap();
        assert_eq!(r.bytes_embedded, payload.len());
        assert!(h.verify_integrity(&c).unwrap());
        // Self-describing: extraction needs no strategy.
        let mut q = p.clone();
        q.format_specific.clear();
        assert_eq!(h.extract(&c, &q).await.unwrap(), payload);
        c.raw_data
    }

    #[tokio::test]
    async fn sam_roundtrip_every_strategy() {
        let orig = sam_fixture(40);
        let (_, orig_recs) = noodles_sam(&orig);
        let orig_lines: Vec<&[u8]> = orig.split(|&b| b == b'\n').collect();
        let payload = b"alignment payload \x00\xff with binary".to_vec();
        for s in STRATEGIES {
            let p = params(Some(s), b"", ECLevel::None);
            let out = embed_extract(DataFormat::SAM, orig.clone(), &payload, &p).await;
            let (header, recs) = noodles_sam(&out);
            assert_eq!(recs.len(), orig_recs.len(), "{s}: record count");
            assert!(header.header().is_some(), "{s}: @HD kept");
            assert_eq!(header.programs().as_ref().len(), 1, "{s}: @PG kept");

            let out_lines: Vec<&[u8]> = out.split(|&b| b == b'\n').collect();
            let oq = quals(&orig_recs);
            let nq = quals(&recs);
            match s {
                "comment" => {
                    // Exactly one inserted @CO line, everything else identical.
                    assert_eq!(out_lines.len(), orig_lines.len() + 1);
                    assert!(out_lines[6].starts_with(COMMENT_PREFIX));
                    assert_eq!(&out_lines[..6], &orig_lines[..6]);
                    assert_eq!(&out_lines[7..], &orig_lines[6..]);
                    assert_eq!(header.comments().len(), 2);
                }
                "quality" => {
                    assert_eq!(out_lines.len(), orig_lines.len());
                    let mut changed = 0;
                    for (a, b) in orig_lines.iter().zip(&out_lines) {
                        if a != b {
                            changed += 1;
                            let fa: Vec<_> = a.split(|&c| c == b'\t').collect();
                            let fb: Vec<_> = b.split(|&c| c == b'\t').collect();
                            assert_eq!(fa.len(), fb.len());
                            for i in (0..fa.len()).filter(|&i| i != 10) {
                                assert_eq!(fa[i], fb[i]);
                            }
                        }
                    }
                    assert!(changed > 0);
                    for (a, b) in oq.iter().zip(&nq) {
                        assert_eq!(a.is_empty(), b.is_empty(), "'*' QUAL untouched");
                        assert_eq!(a.len(), b.len());
                        for (x, y) in a.iter().zip(b) {
                            assert!(x.abs_diff(*y) <= 2);
                        }
                    }
                }
                _ => {
                    // optional / auxiliary / multi (-> optional): each line is
                    // the original plus at most one trailing zs tag.
                    assert_eq!(out_lines.len(), orig_lines.len());
                    for (a, b) in orig_lines.iter().zip(&out_lines) {
                        if a != b {
                            assert!(b.starts_with(a));
                            let suffix = &b[a.len()..];
                            assert!(suffix.starts_with(b"\tzs:Z:"));
                            assert!(!suffix[1..].contains(&b'\t'));
                        }
                    }
                    assert_eq!(oq, nq);
                }
            }
        }
    }

    #[tokio::test]
    async fn sam_roundtrip_encrypted_with_ec() {
        let payload = b"secret with EC";
        for s in ["optional", "quality"] {
            let p = params(Some(s), b"pw", ECLevel::Low);
            // Encrypted + RS envelope is ~316 bytes; 'quality' carries 44 bits
            // per fixture repeat, so use enough repeats for that.
            let out = embed_extract(DataFormat::SAM, sam_fixture(80), payload, &p).await;
            let h = SamBamHandler::new();
            let c = CarrierData::new(DataFormat::SAM, out);
            assert!(h
                .extract(&c, &params(None, b"wrong", ECLevel::Low))
                .await
                .is_err());
            assert!(h
                .extract(&c, &params(None, b"", ECLevel::Low))
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn auxiliary_concentrates_optional_spreads() {
        let payload = vec![0xabu8; 600];
        let p = |s| params(Some(s), b"", ECLevel::None);
        let count = |d: &[u8]| {
            parse_sam(d)
                .unwrap()
                .records()
                .filter(|r| r.optional_fields().any(|f| f.starts_with(b"zs:")))
                .count()
        };
        let aux = embed_extract(DataFormat::SAM, sam_fixture(40), &payload, &p("auxiliary")).await;
        let opt = embed_extract(DataFormat::SAM, sam_fixture(40), &payload, &p("optional")).await;
        let env_len = 600 + ENV_HEADER_LEN + CODEC_MIN_OVERHEAD;
        assert_eq!(count(&aux), env_len.div_ceil(MAX_CHUNK));
        assert_eq!(count(&opt), env_len.div_ceil(env_len.div_ceil(200)));
    }

    #[tokio::test]
    async fn embed_errors() {
        let h = SamBamHandler::new();
        let none = params(None, b"", ECLevel::None);
        let mut c = CarrierData::new(DataFormat::SAM, sam_fixture(1));
        assert!(h
            .embed(&mut c, b"x", &params(Some("bogus"), b"", ECLevel::None))
            .await
            .is_err());
        let big = vec![1u8; 5 * MAX_CHUNK];
        for s in ["optional", "auxiliary", "quality"] {
            let e = h
                .embed(&mut c, &big, &params(Some(s), b"", ECLevel::None))
                .await;
            assert!(
                matches!(e, Err(SteganoError::InsufficientCapacity { .. })),
                "{s}"
            );
        }
        // Nothing embedded yet.
        assert!(h.extract(&c, &none).await.is_err());
        // Double embed rejected.
        h.embed(&mut c, b"x", &params(Some("comment"), b"", ECLevel::None))
            .await
            .unwrap();
        assert!(h
            .embed(&mut c, b"y", &params(Some("optional"), b"", ECLevel::None))
            .await
            .is_err());
        // multi falls back to a comment when tags/quality are too small.
        let mut c = CarrierData::new(DataFormat::SAM, sam_fixture(1));
        h.embed(&mut c, &big, &params(Some("multi"), b"", ECLevel::None))
            .await
            .unwrap();
        assert_eq!(h.extract(&c, &none).await.unwrap(), big);
        assert!(parse_sam(&c.raw_data)
            .unwrap()
            .headers()
            .any(|l| l.starts_with(COMMENT_PREFIX)));
    }

    #[test]
    fn capacity_and_detection() {
        let h = SamBamHandler::new();
        let sam = sam_fixture(4);
        assert_eq!(h.detect_format(&sam), Some(DataFormat::SAM));
        let bam = bam_fixture(4);
        assert_eq!(h.detect_format(&bam), Some(DataFormat::BAM));
        assert_eq!(
            h.detect_format(&sam_to_raw_bam(&sam)),
            Some(DataFormat::BAM)
        );
        for (f, d) in [(DataFormat::SAM, sam), (DataFormat::BAM, bam)] {
            let cap = h.estimate_capacity(&CarrierData::new(f, d)).unwrap();
            assert_eq!(cap.total_bits, 20 * MAX_CHUNK * 8);
            assert!(cap.recommended_bits > 0 && cap.recommended_bits < cap.total_bits);
        }
    }

    // ---- BAM ----------------------------------------------------------------

    #[test]
    fn bam_fixture_is_valid() {
        let sam = sam_fixture(3);
        let bam = bam_fixture(3);
        let (ha, a) = noodles_sam(&sam);
        let (hb, b) = noodles_bam(&bam);
        assert_eq!(
            as_sam_text(&ha, &a),
            as_sam_text(&hb, &b),
            "test encoder agrees with noodles' reading of the SAM"
        );
        // Parse + serialise reproduces the raw BAM byte-exactly.
        let again = parse_bam(&bam).unwrap().serialize().unwrap();
        assert_eq!(bgzf_decompress(&again).unwrap(), sam_to_raw_bam(&sam));
        assert_eq!(noodles_bgzf_inflate(&again), sam_to_raw_bam(&sam));
    }

    #[test]
    fn reads_noodles_written_bam() {
        let (header, recs) = noodles_sam(&sam_fixture(3));
        let mut w = noodles_bam::io::Writer::new(Vec::new());
        w.write_header(&header).unwrap();
        for r in &recs {
            w.write_alignment_record(&header, r).unwrap();
        }
        let bam = w.into_inner().finish().unwrap();
        let doc = parse_bam(&bam).unwrap();
        assert_eq!(doc.records.len(), recs.len());
    }

    #[tokio::test]
    async fn bam_roundtrip_every_strategy() {
        let sam = sam_fixture(40);
        let (_, orig_recs) = noodles_sam(&sam);
        let payload = b"BAM payload bytes".to_vec();
        for (input_name, input) in [("bgzf", bam_fixture(40)), ("raw", sam_to_raw_bam(&sam))] {
            for s in STRATEGIES {
                let p = params(Some(s), b"", ECLevel::None);
                let out = embed_extract(DataFormat::BAM, input.clone(), &payload, &p).await;
                assert!(out.starts_with(&[0x1f, 0x8b]), "{input_name}/{s}: BGZF");
                assert!(out.ends_with(&BGZF_EOF));
                let (header, recs) = noodles_bam(&out);
                assert_eq!(recs.len(), orig_recs.len(), "{input_name}/{s}");
                assert!(header.header().is_some());
                assert_eq!(header.programs().as_ref().len(), 1);
                assert_eq!(header.reference_sequences().len(), 2);
                for (ra, rb) in orig_recs.iter().zip(&recs) {
                    let (a, b) = (ra.quality_scores().as_ref(), rb.quality_scores().as_ref());
                    assert_eq!(a.is_empty(), b.is_empty());
                    assert_eq!(a.len(), b.len());
                    assert_eq!(ra.sequence(), rb.sequence());
                    assert_eq!(ra.cigar(), rb.cigar());
                    assert_eq!(ra.alignment_start(), rb.alignment_start());
                    assert_eq!(ra.name(), rb.name());
                    assert_eq!(ra.flags(), rb.flags());
                    if s != "quality" {
                        assert_eq!(a, b);
                    }
                }
                if s == "comment" {
                    assert_eq!(header.comments().len(), 2);
                }
                if s == "optional" || s == "multi" || s == "auxiliary" {
                    assert!(recs.iter().any(|r| r.data().get(b"zs").is_some()));
                }
            }
        }
    }

    // ---- robustness ---------------------------------------------------------

    /// Truncating a BAM anywhere yields an error (or, exactly at a record /
    /// block boundary, a shorter valid file) and never a panic.
    #[test]
    fn truncated_bam_never_panics() {
        let raw = sam_to_raw_bam(&sam_fixture(2));
        let full = parse_bam_raw(&raw).unwrap();
        let mut boundaries = std::collections::HashSet::new();
        let mut off = raw.len();
        for r in full.records.iter().rev() {
            boundaries.insert(off);
            off -= r.data.len() + 4;
        }
        boundaries.insert(off);
        for n in 0..raw.len() {
            let res = parse_bam_raw(&raw[..n]);
            assert_eq!(res.is_ok(), boundaries.contains(&n), "raw truncated at {n}");
        }
        let z = bgzf_compress(&raw).unwrap();
        let first_block = bgzf_block_len(&z, 0).unwrap().0;
        for n in 0..z.len() {
            let res = parse_bam(&z[..n]);
            if n != first_block {
                assert!(res.is_err(), "bgzf truncated at {n}");
            }
        }
    }

    #[tokio::test]
    async fn fuzzed_bam_never_panics() {
        let raw = sam_to_raw_bam(&sam_fixture(2));
        let z = bgzf_compress(&raw).unwrap();
        let h = SamBamHandler::new();
        let p = params(None, b"", ECLevel::None);
        let mut rng = rand::rngs::StdRng::seed_from_u64(46);
        for i in 0..3000 {
            let mut d = if i % 2 == 0 { raw.clone() } else { z.clone() };
            for _ in 0..rng.gen_range(1..5) {
                let at = rng.gen_range(0..d.len());
                d[at] = rng.gen();
            }
            let _ = parse_bam(&d);
            let _ = h.detect_format(&d);
            let mut c = CarrierData::new(DataFormat::BAM, d);
            let _ = h.estimate_capacity(&c);
            let _ = h.extract(&c, &p).await;
            if i % 10 == 0 {
                let _ = h.embed(&mut c, b"x", &p).await;
            }
        }
    }

    #[test]
    fn fuzzed_sam_never_panics() {
        let sam = sam_fixture(2);
        let mut rng = rand::rngs::StdRng::seed_from_u64(64);
        for _ in 0..3000 {
            let mut d = sam.clone();
            for _ in 0..rng.gen_range(1..5) {
                let at = rng.gen_range(0..d.len());
                d[at] = rng.gen();
            }
            if let Ok(doc) = parse_sam(&d) {
                assert_eq!(doc.serialize(), d);
            }
        }
    }

    #[test]
    fn crafted_bam_records() {
        let raw = sam_to_raw_bam(&sam_fixture(1));
        let doc = parse_bam_raw(&raw).unwrap();
        let rs = raw.len() - doc.records.iter().map(|r| r.data.len() + 4).sum::<usize>();
        let with = |f: &dyn Fn(&mut Vec<u8>)| {
            let mut d = raw.clone();
            f(&mut d);
            parse_bam_raw(&d)
        };
        // block_size < 32
        assert!(with(&|d| d[rs..rs + 4].copy_from_slice(&8u32.to_le_bytes())).is_err());
        // l_read_name past end of block / zero
        assert!(with(&|d| d[rs + 4 + 8] = 255).is_err());
        assert!(with(&|d| d[rs + 4 + 8] = 0).is_err());
        // huge n_cigar_op
        assert!(with(&|d| d[rs + 16..rs + 18].copy_from_slice(&[0xff, 0xff])).is_err());
        // negative / huge l_seq
        assert!(with(&|d| d[rs + 20..rs + 24].copy_from_slice(&[0xff; 4])).is_err());
        assert!(with(&|d| d[rs + 20..rs + 24].copy_from_slice(&[0xff, 0xff, 0xff, 0x7f])).is_err());
        // Non-UTF-8 read-name bytes are kept verbatim.
        let named = with(&|d| d[rs + 4 + 32] = 0xff).unwrap();
        assert_eq!(named.records[0].data[32], 0xff);
        let rewritten = bgzf_decompress(&named.serialize().unwrap()).unwrap();
        assert_eq!(rewritten[rs + 4 + 32], 0xff);
        // huge l_text / n_ref
        assert!(with(&|d| d[4..8].copy_from_slice(&[0xff; 4])).is_err());
        let l_text = le_u32(&raw, 4).unwrap() as usize;
        assert!(with(&|d| d[8 + l_text..12 + l_text].copy_from_slice(&[0xff; 4])).is_err());
    }

    /// Writes embedded outputs to `$STEGANO_SAM_OUT` for manual validation
    /// with `samtools view` / `samtools quickcheck`.
    #[tokio::test]
    #[ignore]
    async fn dump_outputs_for_samtools() {
        let Ok(dir) = std::env::var("STEGANO_SAM_OUT") else {
            return;
        };
        let h = SamBamHandler::new();
        std::fs::write(format!("{dir}/orig.sam"), sam_fixture(80)).unwrap();
        std::fs::write(format!("{dir}/orig.bam"), bam_fixture(80)).unwrap();
        for s in STRATEGIES {
            for (f, ext, data) in [
                (DataFormat::SAM, "sam", sam_fixture(80)),
                (DataFormat::BAM, "bam", bam_fixture(80)),
            ] {
                let mut c = CarrierData::new(f, data);
                let p = params(Some(s), b"", ECLevel::Low);
                h.embed(&mut c, b"samtools check", &p).await.unwrap();
                std::fs::write(format!("{dir}/{s}.{ext}"), &c.raw_data).unwrap();
            }
        }
    }
}
