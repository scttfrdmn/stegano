//! VCF (Variant Call Format) handler for genomics variant data steganography.
//!
//! # Design
//!
//! The carrier is treated as raw text.  Only the records (and the single
//! column of each record) that a strategy writes to are rewritten; every other
//! line — meta lines, comments, records that fail to parse, records past the
//! end of the payload — is copied byte-for-byte, including its line ending
//! (`\r\n` is preserved), its original QUAL text and its INFO key order.
//!
//! The payload is always passed through [`crate::codec::encode_payload`]
//! (wire format v2, self-framing with its own length).  The embedded *stream*
//! is `[STREAM_MAGIC, strategy code] ‖ encoded payload`, so extraction can
//! identify the strategy without trying every one and decodes the codec
//! payload once.
//!
//! # Strategies (`format_specific["vcf_strategy"]`)
//!
//! | name | aliases | channel |
//! |------|---------|---------|
//! | `info` (default) | `info_fields` | INFO key `STEG=<index>_<hex>`, 32 bytes per record |
//! | `format` | `sample`, `format_fields` | FORMAT key `STEG`, value `<index>_<hex>`, 16 bytes per sample cell |
//! | `id` | — | extra ID item `STEG<index>_<hex>` (`;`-separated, original ID kept), 16 bytes per record |
//! | `quality` | `quality_precision` | 8 bytes per record appended as extra QUAL digits (see below) |
//! | `multi` | — | the same stream in every one of `info`/`format`/`id` that has room |
//!
//! Chunk indices are plain decimal numbers with no width limit, so payloads of
//! any number of chunks are recovered; missing or duplicated chunks are an
//! error rather than silent truncation.
//!
//! When an INFO or FORMAT key is added, a matching `##INFO=<ID=STEG,...>` /
//! `##FORMAT=<ID=STEG,...>` line is inserted before `#CHROM` (unless already
//! declared), so the output stays valid for `bcftools`/htslib.
//!
//! ## Quality strategy
//!
//! Only QUAL values written as plain decimals (`29`, `29.47`) are used; `.`
//! (missing) and any other notation are left untouched.  The original QUAL
//! text is kept as a prefix and a guard digit `0` plus 24 payload digits are
//! appended (`29.47` → `29.470…`, `29` → `29.0…`).  The numeric value therefore
//! grows by strictly less than 0.1 units of the last printed digit, and
//! rounding to the originally printed precision gives back the original text.
//! It is, however, plainly visible as an unusually long QUAL.
//!
//! The format/sample strategy needs sample columns: on a sites-only VCF it is
//! an error.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::time::Instant;

/// Handler for VCF (Variant Call Format) genomics carrier files.
///
/// See the module documentation for the embedding layout.
pub struct VcfHandler;

/// VCF embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VcfEmbedStrategy {
    /// INFO key `STEG` on consecutive records.
    InfoFieldSteganography,

    /// Extra trailing digits on plain-decimal QUAL values (value change is
    /// below the last printed digit; `.` is never touched).
    QualityScoreSteganography,

    /// FORMAT key `STEG` in sample columns (requires at least one sample).
    SampleFormatSteganography,

    /// Extra `;`-separated item in the ID column.
    VariantIdSteganography,

    /// The same stream in every INFO / FORMAT / ID channel with room for it.
    MultiStrategy,
}

impl VcfEmbedStrategy {
    /// Parse a `vcf_strategy` name (case-insensitive).
    ///
    /// Accepted: `info` (`info_fields`), `quality` (`quality_precision`),
    /// `format` (`sample`, `format_fields`), `id`, `multi`.
    pub fn from_name(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "info" | "info_fields" => Ok(Self::InfoFieldSteganography),
            "quality" | "quality_precision" => Ok(Self::QualityScoreSteganography),
            "format" | "sample" | "format_fields" => Ok(Self::SampleFormatSteganography),
            "id" => Ok(Self::VariantIdSteganography),
            "multi" => Ok(Self::MultiStrategy),
            other => Err(SteganoError::InvalidParameter(format!(
                "unknown vcf_strategy '{other}'; expected one of: info, quality, \
                 format (alias: sample), id, multi"
            ))),
        }
    }

    /// Canonical strategy name.
    pub fn name(self) -> &'static str {
        match self {
            Self::InfoFieldSteganography => "info",
            Self::QualityScoreSteganography => "quality",
            Self::SampleFormatSteganography => "format",
            Self::VariantIdSteganography => "id",
            Self::MultiStrategy => "multi",
        }
    }

    pub(crate) fn code(self) -> u8 {
        match self {
            Self::InfoFieldSteganography => 0,
            Self::QualityScoreSteganography => 1,
            Self::SampleFormatSteganography => 2,
            Self::VariantIdSteganography => 3,
            Self::MultiStrategy => 4,
        }
    }

    pub(crate) fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::InfoFieldSteganography),
            1 => Some(Self::QualityScoreSteganography),
            2 => Some(Self::SampleFormatSteganography),
            3 => Some(Self::VariantIdSteganography),
            4 => Some(Self::MultiStrategy),
            _ => None,
        }
    }
}

pub(crate) const VCF_MAGIC_HEADER: &str = "##fileformat=VCF";
/// First byte of every embedded stream.
pub(crate) const STREAM_MAGIC: u8 = b'V';
/// INFO / FORMAT key and ID item prefix.
pub(crate) const STEG_KEY: &str = "STEG";
pub(crate) const INFO_META: &str =
    "##INFO=<ID=STEG,Number=1,Type=String,Description=\"Opaque annotation chunk\">";
const FORMAT_META: &str =
    "##FORMAT=<ID=STEG,Number=1,Type=String,Description=\"Opaque annotation chunk\">";
pub(crate) const INFO_CHUNK_BYTES: usize = 32;
const FORMAT_CHUNK_BYTES: usize = 16;
pub(crate) const ID_CHUNK_BYTES: usize = 16;
pub(crate) const QUAL_CHUNK_BYTES: usize = 8;
/// Guard digit `0` + three decimal digits per byte.
const QUAL_SUFFIX_LEN: usize = 1 + 3 * QUAL_CHUNK_BYTES;
/// Stream prefix (magic + strategy) plus the unencrypted, un-EC codec header.
const MIN_STREAM_OVERHEAD: usize = 2 + 36;

/// Indexed (INFO / FORMAT / ID) channels, in extraction preference order.
const INDEXED_CHANNELS: [VcfEmbedStrategy; 3] = [
    VcfEmbedStrategy::InfoFieldSteganography,
    VcfEmbedStrategy::SampleFormatSteganography,
    VcfEmbedStrategy::VariantIdSteganography,
];

/// One physical line: body without its terminator, and the terminator.
struct Line<'a> {
    body: &'a [u8],
    eol: &'a [u8],
}

/// A VCF file as raw lines.
struct VcfDoc<'a> {
    lines: Vec<Line<'a>>,
    /// Index of the `#CHROM` line.
    chrom_idx: usize,
    /// Number of sample columns declared by `#CHROM`.
    n_samples: usize,
}

/// Pending modifications: replaced record fields and new meta lines.
#[derive(Default)]
struct Edits {
    records: HashMap<usize, Vec<String>>,
    meta: Vec<&'static str>,
}

impl Edits {
    fn fields_mut(&mut self, line: usize, orig: &[&str]) -> &mut Vec<String> {
        self.records
            .entry(line)
            .or_insert_with(|| orig.iter().map(|s| s.to_string()).collect())
    }

    fn declare(&mut self, doc: &VcfDoc, kind: &str, meta: &'static str) {
        if !doc.has_meta(kind, STEG_KEY) && !self.meta.contains(&meta) {
            self.meta.push(meta);
        }
    }
}

/// Split one physical line (as produced by `split_inclusive('\n')` or
/// `read_until(b'\n')`) into its body and its terminator (`\r\n`, `\n` or
/// nothing).
pub(crate) fn split_eol(seg: &[u8]) -> (&[u8], &[u8]) {
    let body_len = if seg.ends_with(b"\r\n") {
        seg.len() - 2
    } else if seg.ends_with(b"\n") {
        seg.len() - 1
    } else {
        seg.len()
    };
    seg.split_at(body_len)
}

fn split_lines(data: &[u8]) -> Vec<Line<'_>> {
    data.split_inclusive(|&b| b == b'\n')
        .map(|seg| {
            let (body, eol) = split_eol(seg);
            Line { body, eol }
        })
        .collect()
}

/// Whether a line after `#CHROM` is a record candidate: non-empty and not a
/// `#` comment.
pub(crate) fn is_data_line(body: &[u8]) -> bool {
    !body.is_empty() && body[0] != b'#'
}

/// Columns of a record usable as a carrier: valid UTF-8 with at least 8
/// tab-separated columns.
pub(crate) fn record_fields(body: &[u8]) -> Option<Vec<&str>> {
    let s = std::str::from_utf8(body).ok()?;
    let fields: Vec<&str> = s.split('\t').collect();
    (fields.len() >= 8).then_some(fields)
}

/// Whether `body` is a `##<kind>=<ID=<id>,...>` meta line.
pub(crate) fn is_meta_line(body: &[u8], kind: &str, id: &str) -> bool {
    std::str::from_utf8(body)
        .ok()
        .and_then(|s| s.strip_prefix("##"))
        .and_then(|s| s.strip_prefix(kind))
        .and_then(|s| s.strip_prefix("=<ID="))
        .and_then(|s| s.strip_prefix(id))
        .is_some_and(|rest| rest.starts_with(',') || rest.starts_with('>'))
}

/// Append `item` to a `;`-separated column, replacing a missing value.
fn push_item(col: &mut String, item: &str) {
    if col.is_empty() || col == "." {
        col.clear();
    } else {
        col.push(';');
    }
    col.push_str(item);
}

/// Add INFO chunk `index` to an INFO column.
pub(crate) fn push_info_chunk(col: &mut String, index: usize, chunk: &[u8]) {
    push_item(col, &format!("{STEG_KEY}={}", chunk_value(index, chunk)));
}

/// Add ID chunk `index` to an ID column.
pub(crate) fn push_id_chunk(col: &mut String, index: usize, chunk: &[u8]) {
    push_item(col, &format!("{STEG_KEY}{}", chunk_value(index, chunk)));
}

/// Append a QUAL chunk (guard digit `0`, then 3 digits per byte, zero-padded
/// to [`QUAL_CHUNK_BYTES`]) to a plain-decimal QUAL value.
pub(crate) fn push_qual_chunk(col: &mut String, chunk: &[u8]) {
    let mut digits = String::with_capacity(QUAL_SUFFIX_LEN);
    digits.push('0'); // guard digit: keeps the change below the last printed digit
    for i in 0..QUAL_CHUNK_BYTES {
        digits.push_str(&format!("{:03}", chunk.get(i).copied().unwrap_or(0)));
    }
    if !col.contains('.') {
        col.push('.');
    }
    col.push_str(&digits);
}

/// Chunks in one INFO column; `Err` if a `STEG=` item is malformed.
pub(crate) fn info_chunks(col: &str) -> Result<Vec<(usize, Vec<u8>)>> {
    let prefix = format!("{STEG_KEY}=");
    col.split(';')
        .filter_map(|item| item.strip_prefix(prefix.as_str()))
        .map(|value| {
            parse_chunk(value)
                .ok_or_else(|| SteganoError::IntegrityError("Malformed VCF INFO chunk".into()))
        })
        .collect()
}

/// Chunks in one ID column.  Items that merely start with "STEG" but are not
/// chunks are ignored.
pub(crate) fn id_chunks(col: &str) -> Vec<(usize, Vec<u8>)> {
    col.split(';')
        .filter_map(|item| item.strip_prefix(STEG_KEY).and_then(parse_chunk))
        .collect()
}

/// FORMAT chunks in the sample columns of one record; `Err` if a sample
/// value is malformed.
pub(crate) fn format_chunks(f: &[&str]) -> Result<Vec<(usize, Vec<u8>)>> {
    let mut chunks = Vec::new();
    if f.len() < 10 {
        return Ok(chunks);
    }
    let Some(pos) = f[8].split(':').position(|k| k == STEG_KEY) else {
        return Ok(chunks);
    };
    for sample in &f[9..] {
        match sample.split(':').nth(pos) {
            None | Some(".") | Some("") => {}
            Some(value) => chunks.push(parse_chunk(value).ok_or_else(|| {
                SteganoError::IntegrityError("Malformed VCF FORMAT chunk".into())
            })?),
        }
    }
    Ok(chunks)
}

/// The QUAL chunk carried by a plain-decimal QUAL value, if any.
pub(crate) fn qual_chunk(qual: &str) -> Option<Vec<u8>> {
    let frac = qual.split_once('.').map(|(_, frac)| frac).unwrap_or("");
    let suffix = frac
        .len()
        .checked_sub(QUAL_SUFFIX_LEN)
        .and_then(|start| frac.get(start..))?;
    let digits = suffix.strip_prefix('0')?;
    (0..QUAL_CHUNK_BYTES)
        .map(|i| {
            digits
                .get(3 * i..3 * i + 3)
                .and_then(|t| t.parse::<u8>().ok())
        })
        .collect()
}

/// Whether a stream whose prefix names strategy `found` may be read from
/// `channel` (`multi` writes the same stream into every indexed channel).
pub(crate) fn plausible_channel(found: VcfEmbedStrategy, channel: VcfEmbedStrategy) -> bool {
    found == channel
        || (found == VcfEmbedStrategy::MultiStrategy
            && channel != VcfEmbedStrategy::QualityScoreSteganography)
}

impl<'a> VcfDoc<'a> {
    fn parse(data: &'a [u8]) -> Result<Self> {
        if !data.starts_with(VCF_MAGIC_HEADER.as_bytes()) {
            return Err(SteganoError::IntegrityError(
                "Invalid VCF format - missing ##fileformat=VCF header".to_string(),
            ));
        }
        let lines = split_lines(data);
        let chrom_idx = lines
            .iter()
            .position(|l| l.body.starts_with(b"#CHROM"))
            .ok_or_else(|| {
                SteganoError::IntegrityError("Invalid VCF format - missing #CHROM line".into())
            })?;
        let n_samples = std::str::from_utf8(lines[chrom_idx].body)
            .map(|s| s.split('\t').count().saturating_sub(9))
            .unwrap_or(0);
        Ok(Self {
            lines,
            chrom_idx,
            n_samples,
        })
    }

    /// Data lines after `#CHROM` (non-empty, not starting with `#`).
    fn data_lines(&self) -> impl Iterator<Item = (usize, &'a [u8])> + '_ {
        self.lines
            .iter()
            .enumerate()
            .skip(self.chrom_idx + 1)
            .filter(|(_, l)| is_data_line(l.body))
            .map(|(i, l)| (i, l.body))
    }

    /// Records usable as carriers: valid UTF-8 with at least 8 columns.
    fn records(&self) -> impl Iterator<Item = (usize, Vec<&'a str>)> + '_ {
        self.data_lines()
            .filter_map(|(i, body)| record_fields(body).map(|f| (i, f)))
    }

    /// Whether a `##<kind>=<ID=<id>,...>` meta line exists.
    fn has_meta(&self, kind: &str, id: &str) -> bool {
        self.lines[..self.chrom_idx]
            .iter()
            .any(|l| is_meta_line(l.body, kind, id))
    }

    fn render(&self, edits: &Edits) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i == self.chrom_idx {
                let eol: &[u8] = if line.eol.is_empty() { b"\n" } else { line.eol };
                for meta in &edits.meta {
                    out.extend_from_slice(meta.as_bytes());
                    out.extend_from_slice(eol);
                }
            }
            match edits.records.get(&i) {
                Some(fields) => out.extend_from_slice(fields.join("\t").as_bytes()),
                None => out.extend_from_slice(line.body),
            }
            out.extend_from_slice(line.eol);
        }
        out
    }

    /// Records with a FORMAT column and at least one sample whose sample
    /// columns are not longer than FORMAT (so an appended key lines up).
    fn format_records(&self) -> impl Iterator<Item = (usize, Vec<&'a str>)> + '_ {
        self.records().filter(|(_, f)| {
            if f.len() < 10 || f[8].is_empty() || f[8] == "." {
                return false;
            }
            let nkeys = f[8].split(':').count();
            f[9..].iter().all(|s| s.split(':').count() <= nkeys)
        })
    }

    /// Raw byte capacity of a channel.
    fn channel_capacity(&self, strategy: VcfEmbedStrategy) -> usize {
        match strategy {
            VcfEmbedStrategy::InfoFieldSteganography => self.records().count() * INFO_CHUNK_BYTES,
            VcfEmbedStrategy::VariantIdSteganography => self.records().count() * ID_CHUNK_BYTES,
            VcfEmbedStrategy::SampleFormatSteganography => {
                self.format_records()
                    .map(|(_, f)| f.len() - 9)
                    .sum::<usize>()
                    * FORMAT_CHUNK_BYTES
            }
            VcfEmbedStrategy::QualityScoreSteganography => {
                self.records()
                    .filter(|(_, f)| is_plain_decimal(f[5]))
                    .count()
                    * QUAL_CHUNK_BYTES
            }
            VcfEmbedStrategy::MultiStrategy => INDEXED_CHANNELS
                .iter()
                .map(|&s| self.channel_capacity(s))
                .filter(|&c| c > 0)
                .min()
                .unwrap_or(0),
        }
    }
}

pub(crate) fn is_plain_decimal(q: &str) -> bool {
    let (int, frac) = match q.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (q, None),
    };
    !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

fn chunk_value(index: usize, chunk: &[u8]) -> String {
    format!("{index}_{}", hex::encode(chunk))
}

/// Parse `<decimal index>_<hex>`; `None` if `s` is not in that form.
pub(crate) fn parse_chunk(s: &str) -> Option<(usize, Vec<u8>)> {
    let (index, hex_part) = s.split_once('_')?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) || hex_part.is_empty() {
        return None;
    }
    Some((index.parse().ok()?, hex::decode(hex_part).ok()?))
}

/// Order chunks by index, requiring exactly 0..n.
fn assemble(mut chunks: Vec<(usize, Vec<u8>)>, channel: &str) -> Result<Option<Vec<u8>>> {
    if chunks.is_empty() {
        return Ok(None);
    }
    chunks.sort_by_key(|(i, _)| *i);
    let mut out = Vec::new();
    for (expected, (index, data)) in chunks.into_iter().enumerate() {
        if index != expected {
            return Err(SteganoError::IntegrityError(format!(
                "VCF {channel} data is incomplete: chunk {expected} missing or duplicated"
            )));
        }
        out.extend_from_slice(&data);
    }
    Ok(Some(out))
}

pub(crate) fn capacity_error(needed_bytes: usize, available_bytes: usize) -> SteganoError {
    SteganoError::InsufficientCapacity {
        needed: needed_bytes * 8,
        available: available_bytes * 8,
    }
}

fn sites_only_error() -> SteganoError {
    SteganoError::InvalidParameter(
        "VCF has no usable sample columns (sites-only VCF); the 'format'/'sample' strategy \
         needs at least one sample column - use 'info' or 'id' instead"
            .to_string(),
    )
}

pub(crate) fn already_embedded_error(channel: &str) -> SteganoError {
    SteganoError::InvalidParameter(format!(
        "VCF carrier already contains stegano data in the {channel} channel"
    ))
}

pub(crate) fn strategy_from_params(params: &EmbedParams) -> Result<Option<VcfEmbedStrategy>> {
    match params.format_specific.get("vcf_strategy") {
        None => Ok(None),
        Some(v) => {
            let name = v.as_str().ok_or_else(|| {
                SteganoError::InvalidParameter("vcf_strategy must be a string".to_string())
            })?;
            VcfEmbedStrategy::from_name(name).map(Some)
        }
    }
}

impl Default for VcfHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VcfHandler {
    pub fn new() -> Self {
        Self
    }

    // ----- embedding ---------------------------------------------------------

    /// Channels `strategy` writes to in `doc`; errors if the carrier cannot
    /// take it (sites-only for `format`, channel already used).
    fn plan_channels(
        &self,
        doc: &VcfDoc,
        strategy: VcfEmbedStrategy,
    ) -> Result<Vec<VcfEmbedStrategy>> {
        let channels: Vec<VcfEmbedStrategy> = match strategy {
            VcfEmbedStrategy::MultiStrategy => INDEXED_CHANNELS
                .into_iter()
                .filter(|&c| doc.channel_capacity(c) > 0)
                .collect(),
            VcfEmbedStrategy::SampleFormatSteganography if doc.channel_capacity(strategy) == 0 => {
                return Err(sites_only_error())
            }
            s => vec![s],
        };
        for &c in &channels {
            if c != VcfEmbedStrategy::QualityScoreSteganography
                && self.read_channel(doc, c).ok().flatten().is_some()
            {
                return Err(already_embedded_error(c.name()));
            }
        }
        Ok(channels)
    }

    /// Write the stream `[STREAM_MAGIC, strategy] ‖ encoded` into `channels`
    /// and render the result.
    fn write_channels(
        &self,
        doc: &VcfDoc,
        strategy: VcfEmbedStrategy,
        channels: &[VcfEmbedStrategy],
        encoded: &[u8],
    ) -> Result<Vec<u8>> {
        let mut stream = vec![STREAM_MAGIC, strategy.code()];
        stream.extend_from_slice(encoded);

        let mut edits = Edits::default();
        let mut written = 0;
        let mut last_err = None;
        for &c in channels {
            match self.embed_channel(doc, c, &stream, &mut edits) {
                Ok(()) => written += 1,
                Err(e) => last_err = Some(e),
            }
        }
        if written == 0 {
            return Err(last_err.unwrap_or_else(|| capacity_error(stream.len(), 0)));
        }
        Ok(doc.render(&edits))
    }

    /// In-memory embed of already codec-encoded bytes (the output of
    /// [`crate::codec::encode_payload`]); used to compare against the
    /// streaming path byte for byte.
    #[cfg(test)]
    pub(crate) fn embed_encoded(
        &self,
        data: &[u8],
        encoded: &[u8],
        strategy: VcfEmbedStrategy,
    ) -> Result<Vec<u8>> {
        let doc = VcfDoc::parse(data)?;
        let channels = self.plan_channels(&doc, strategy)?;
        self.write_channels(&doc, strategy, &channels, encoded)
    }

    fn embed_channel(
        &self,
        doc: &VcfDoc,
        strategy: VcfEmbedStrategy,
        stream: &[u8],
        edits: &mut Edits,
    ) -> Result<()> {
        let available = doc.channel_capacity(strategy);
        if stream.len() > available {
            return Err(capacity_error(stream.len(), available));
        }
        match strategy {
            VcfEmbedStrategy::InfoFieldSteganography => {
                let chunks = stream.chunks(INFO_CHUNK_BYTES).enumerate();
                for ((index, chunk), (line, orig)) in chunks.zip(doc.records()) {
                    push_info_chunk(&mut edits.fields_mut(line, &orig)[7], index, chunk);
                }
                edits.declare(doc, "INFO", INFO_META);
            }
            VcfEmbedStrategy::VariantIdSteganography => {
                let chunks = stream.chunks(ID_CHUNK_BYTES).enumerate();
                for ((index, chunk), (line, orig)) in chunks.zip(doc.records()) {
                    push_id_chunk(&mut edits.fields_mut(line, &orig)[2], index, chunk);
                }
            }
            VcfEmbedStrategy::SampleFormatSteganography => {
                let mut chunks = stream.chunks(FORMAT_CHUNK_BYTES).enumerate().peekable();
                for (line, orig) in doc.format_records() {
                    if chunks.peek().is_none() {
                        break;
                    }
                    let fields = edits.fields_mut(line, &orig);
                    let nkeys = fields[8].split(':').count();
                    fields[8].push(':');
                    fields[8].push_str(STEG_KEY);
                    for sample in fields.iter_mut().skip(9) {
                        let Some((index, chunk)) = chunks.next() else {
                            break;
                        };
                        // Fill dropped trailing sub-fields so STEG lines up with FORMAT.
                        for _ in sample.split(':').count()..nkeys {
                            sample.push_str(":.");
                        }
                        sample.push(':');
                        sample.push_str(&chunk_value(index, chunk));
                    }
                }
                edits.declare(doc, "FORMAT", FORMAT_META);
            }
            VcfEmbedStrategy::QualityScoreSteganography => {
                let carriers = doc.records().filter(|(_, f)| is_plain_decimal(f[5]));
                for (chunk, (line, orig)) in stream.chunks(QUAL_CHUNK_BYTES).zip(carriers) {
                    push_qual_chunk(&mut edits.fields_mut(line, &orig)[5], chunk);
                }
            }
            VcfEmbedStrategy::MultiStrategy => unreachable!("multi is expanded by the caller"),
        }
        Ok(())
    }

    // ----- extraction --------------------------------------------------------

    /// Read the raw stream of one channel (`Ok(None)` if the channel is empty).
    fn read_channel(&self, doc: &VcfDoc, strategy: VcfEmbedStrategy) -> Result<Option<Vec<u8>>> {
        let mut chunks = Vec::new();
        match strategy {
            VcfEmbedStrategy::InfoFieldSteganography => {
                for (_, f) in doc.records() {
                    chunks.extend(info_chunks(f[7])?);
                }
                assemble(chunks, "INFO")
            }
            VcfEmbedStrategy::VariantIdSteganography => {
                for (_, f) in doc.records() {
                    chunks.extend(id_chunks(f[2]));
                }
                assemble(chunks, "ID")
            }
            VcfEmbedStrategy::SampleFormatSteganography => {
                for (_, f) in doc.records() {
                    chunks.extend(format_chunks(&f)?);
                }
                assemble(chunks, "FORMAT")
            }
            VcfEmbedStrategy::QualityScoreSteganography => {
                let mut out = Vec::new();
                for (_, f) in doc.records().filter(|(_, f)| is_plain_decimal(f[5])) {
                    let Some(bytes) = qual_chunk(f[5]) else {
                        break;
                    };
                    out.extend_from_slice(&bytes);
                }
                Ok((!out.is_empty()).then_some(out))
            }
            VcfEmbedStrategy::MultiStrategy => unreachable!("multi is expanded by the caller"),
        }
    }

    /// Validate the stream prefix; return the strategy and codec payload.
    fn split_stream(stream: &[u8]) -> Option<(VcfEmbedStrategy, &[u8])> {
        match stream {
            [STREAM_MAGIC, code, rest @ ..] => Some((VcfEmbedStrategy::from_code(*code)?, rest)),
            _ => None,
        }
    }

    /// Candidate codec payloads, in preference order.
    fn candidate_payloads(
        &self,
        doc: &VcfDoc,
        requested: Option<VcfEmbedStrategy>,
    ) -> Result<Vec<Vec<u8>>> {
        match requested {
            Some(strategy) if strategy != VcfEmbedStrategy::MultiStrategy => {
                let stream = self.read_channel(doc, strategy)?.ok_or_else(|| {
                    SteganoError::IntegrityError(format!(
                        "No stegano data found in the VCF {} channel",
                        strategy.name()
                    ))
                })?;
                let (found, payload) = Self::split_stream(&stream).ok_or_else(|| {
                    SteganoError::IntegrityError("Unrecognised VCF stegano stream".into())
                })?;
                let multi_ok = found == VcfEmbedStrategy::MultiStrategy
                    && strategy != VcfEmbedStrategy::QualityScoreSteganography;
                if found != strategy && !multi_ok {
                    return Err(SteganoError::InvalidParameter(format!(
                        "VCF data was embedded with strategy '{}', not '{}'",
                        found.name(),
                        strategy.name()
                    )));
                }
                Ok(vec![payload.to_vec()])
            }
            _ => {
                let mut channels = INDEXED_CHANNELS.to_vec();
                if requested.is_none() {
                    channels.push(VcfEmbedStrategy::QualityScoreSteganography);
                }
                let mut candidates: Vec<Vec<u8>> = Vec::new();
                let mut last_err = None;
                for channel in channels {
                    match self.read_channel(doc, channel) {
                        Ok(Some(stream)) => {
                            if let Some((found, payload)) = Self::split_stream(&stream) {
                                if plausible_channel(found, channel) && !candidates.iter().any(|c| c == payload) {
                                    candidates.push(payload.to_vec());
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(e) => last_err = Some(e),
                    }
                }
                if candidates.is_empty() {
                    return Err(last_err.unwrap_or_else(|| {
                        SteganoError::IntegrityError("No stegano data found in VCF".to_string())
                    }));
                }
                Ok(candidates)
            }
        }
    }

    // ----- capacity / integrity ----------------------------------------------

    fn estimate_vcf_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        let doc = VcfDoc::parse(data)?;
        let default = VcfEmbedStrategy::InfoFieldSteganography;
        let usable = doc
            .channel_capacity(default)
            .saturating_sub(MIN_STREAM_OVERHEAD);

        let total_bits = data.len() * 8;
        let recommended_bits = (usable * 8).min(total_bits);

        let mut strategies = serde_json::Map::new();
        for s in [
            VcfEmbedStrategy::InfoFieldSteganography,
            VcfEmbedStrategy::SampleFormatSteganography,
            VcfEmbedStrategy::VariantIdSteganography,
            VcfEmbedStrategy::QualityScoreSteganography,
            VcfEmbedStrategy::MultiStrategy,
        ] {
            let bits = doc.channel_capacity(s).saturating_sub(MIN_STREAM_OVERHEAD) * 8;
            strategies.insert(s.name().to_string(), bits.into());
        }

        let mut info = HashMap::new();
        info.insert("method".to_string(), default.name().into());
        info.insert("variants".to_string(), doc.records().count().into());
        info.insert("samples".to_string(), doc.n_samples.into());
        info.insert("estimated_capacity_bytes".to_string(), usable.into());
        info.insert(
            "strategies".to_string(),
            serde_json::Value::Object(strategies),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: if total_bits == 0 {
                0.0
            } else {
                recommended_bits as f64 / total_bits as f64
            },
            format_specific_info: info,
        })
    }

    fn verify_vcf_integrity(&self, data: &[u8]) -> Result<bool> {
        let Ok(doc) = VcfDoc::parse(data) else {
            return Ok(false);
        };
        for (_, body) in doc.data_lines() {
            let Ok(line) = std::str::from_utf8(body) else {
                return Ok(false);
            };
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 8 || f[0].is_empty() || f[1].parse::<u64>().map_or(true, |p| p == 0) {
                return Ok(false);
            }
            let is_bases = |a: &str| {
                !a.is_empty()
                    && a.chars()
                        .all(|c| "ATCGN.-".contains(c.to_ascii_uppercase()))
            };
            if !is_bases(f[3]) {
                return Ok(false);
            }
            for alt in f[4].split(',') {
                let symbolic = alt == "*"
                    || (alt.starts_with('<') && alt.ends_with('>'))
                    || alt.contains('[')
                    || alt.contains(']');
                if !symbolic && !is_bases(alt) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl DataHandler for VcfHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        data.starts_with(VCF_MAGIC_HEADER.as_bytes())
            .then_some(DataFormat::VCF)
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::VCF => self.estimate_vcf_capacity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        if carrier.format != DataFormat::VCF {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        let start_time = Instant::now();
        let strategy =
            strategy_from_params(params)?.unwrap_or(VcfEmbedStrategy::InfoFieldSteganography);

        let output = {
            let doc = VcfDoc::parse(&carrier.raw_data)?;
            // Validated before the (expensive) codec runs.
            let channels = self.plan_channels(&doc, strategy)?;
            let encoded = crate::codec::encode_payload(payload, params)?;
            self.write_channels(&doc, strategy, &channels, &encoded)?
        };
        carrier.raw_data = output;

        Ok(EmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            carrier_modified: true,
            performance_stats: PerformanceStats {
                embedding_time_ms: start_time.elapsed().as_millis() as u64,
                encryption_time_ms: 0,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + payload.len(),
            },
            validation_report: None,
        })
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        if carrier.format != DataFormat::VCF {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        let requested = strategy_from_params(params)?;
        let doc = VcfDoc::parse(&carrier.raw_data)?;
        let mut last_err = None;
        for payload in self.candidate_payloads(&doc, requested)? {
            match crate::codec::decode_payload(&payload, params) {
                Ok(data) => return Ok(data),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err
            .unwrap_or_else(|| SteganoError::IntegrityError("No stegano data found in VCF".into())))
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::VCF => self.verify_vcf_integrity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "##fileformat=VCFv4.2\n##source=test\n";
    const COLS: &str = "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO";
    const COLS_SAMPLES: &str = "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\tS2";

    /// `n` records; with samples when `samples` is true.
    fn make_vcf(n: usize, samples: bool, eol: &str) -> Vec<u8> {
        let mut s = String::new();
        for l in HEADER.lines() {
            s.push_str(l);
            s.push_str(eol);
        }
        s.push_str(if samples { COLS_SAMPLES } else { COLS });
        s.push_str(eol);
        for i in 0..n {
            let qual = match i % 4 {
                0 => "29.47".to_string(),
                1 => format!("{}", 30 + i % 50),
                2 => ".".to_string(),
                _ => "1e3".to_string(),
            };
            let id = if i % 3 == 0 {
                ".".to_string()
            } else {
                format!("rs{i}")
            };
            s.push_str(&format!(
                "chr1\t{}\t{id}\tA\tT\t{qual}\tPASS\tZZ=1;DP={};AA=0.5",
                100 + i,
                i
            ));
            if samples {
                s.push_str("\tGT:DP\t0/1:12\t1|1");
            }
            s.push_str(eol);
        }
        s.into_bytes()
    }

    fn params(strategy: &str, password: &str) -> EmbedParams {
        let mut p = EmbedParams {
            encryption_key: password.as_bytes().to_vec(),
            error_correction_level: ECLevel::None,
            ..Default::default()
        };
        p.format_specific.insert(
            "vcf_strategy".to_string(),
            serde_json::Value::String(strategy.to_string()),
        );
        p
    }

    fn carrier(data: Vec<u8>) -> CarrierData {
        CarrierData::new(DataFormat::VCF, data)
    }

    async fn roundtrip(data: Vec<u8>, payload: &[u8], p: &EmbedParams) -> CarrierData {
        let handler = VcfHandler::new();
        let mut c = carrier(data);
        let r = handler.embed(&mut c, payload, p).await.unwrap();
        assert!(r.success);
        assert_eq!(r.bytes_embedded, payload.len());
        assert_eq!(handler.extract(&c, p).await.unwrap(), payload);
        // Strategy is self-describing: extraction without a strategy works too.
        let mut auto = p.clone();
        auto.format_specific.remove("vcf_strategy");
        assert_eq!(handler.extract(&c, &auto).await.unwrap(), payload);
        assert!(handler.verify_integrity(&c).unwrap());
        c
    }

    fn lines_of(data: &[u8]) -> Vec<&[u8]> {
        data.split_inclusive(|&b| b == b'\n').collect()
    }

    #[test]
    fn test_vcf_detection() {
        let handler = VcfHandler::new();
        assert_eq!(
            handler.detect_format(&make_vcf(1, false, "\n")),
            Some(DataFormat::VCF)
        );
        assert_eq!(handler.detect_format(b"@HD\tVN:1.6"), None);
    }

    #[test]
    fn test_vcf_capacity_estimation() {
        let handler = VcfHandler::new();
        let capacity = handler
            .estimate_capacity(&carrier(make_vcf(20, true, "\n")))
            .unwrap();
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.recommended_bits <= capacity.total_bits);
        assert!(capacity.format_specific_info.contains_key("strategies"));
    }

    #[test]
    fn test_strategy_names() {
        use VcfEmbedStrategy::*;
        assert_eq!(
            VcfEmbedStrategy::from_name("sample").unwrap(),
            SampleFormatSteganography
        );
        assert_eq!(
            VcfEmbedStrategy::from_name("FORMAT").unwrap(),
            SampleFormatSteganography
        );
        assert_eq!(
            VcfEmbedStrategy::from_name("info").unwrap(),
            InfoFieldSteganography
        );
        assert!(VcfEmbedStrategy::from_name("bogus").is_err());
        for s in [
            InfoFieldSteganography,
            QualityScoreSteganography,
            SampleFormatSteganography,
            VariantIdSteganography,
            MultiStrategy,
        ] {
            assert_eq!(VcfEmbedStrategy::from_code(s.code()), Some(s));
            assert_eq!(VcfEmbedStrategy::from_name(s.name()).unwrap(), s);
        }
    }

    #[tokio::test]
    async fn test_every_strategy_roundtrip_with_and_without_password() {
        for strategy in ["info", "quality", "format", "sample", "id", "multi"] {
            for password in ["", "correct horse"] {
                let p = params(strategy, password);
                roundtrip(
                    make_vcf(40, true, "\n"),
                    b"secret genomics variant data",
                    &p,
                )
                .await;
            }
        }
    }

    #[tokio::test]
    async fn test_wrong_password_and_missing_password_fail() {
        let handler = VcfHandler::new();
        let p = params("info", "right");
        let c = roundtrip(make_vcf(10, false, "\n"), b"enc", &p).await;
        assert!(handler.extract(&c, &params("info", "wrong")).await.is_err());
        assert!(handler.extract(&c, &params("info", "")).await.is_err());
    }

    #[tokio::test]
    async fn test_error_correction_roundtrip() {
        let mut p = params("info", "pw");
        p.error_correction_level = ECLevel::High;
        roundtrip(make_vcf(40, false, "\n"), b"ec payload", &p).await;
    }

    #[tokio::test]
    async fn test_unchanged_lines_byte_identical_crlf() {
        // CRLF input with float QUALs, a comment line and an unparseable line.
        let mut data = make_vcf(12, false, "\r\n");
        data.extend_from_slice(b"#a comment line\r\n");
        data.extend_from_slice(b"garbage line without tabs\r\n");
        data.extend_from_slice(b"chr2\t5\t.\tG\tC\t29.47\tPASS\tB=2;A=1\r\n");
        let p = params("info", "");
        let handler = VcfHandler::new();
        let mut c = carrier(data.clone());
        handler.embed(&mut c, b"x", &p).await.unwrap();

        let before = lines_of(&data);
        let after: Vec<&[u8]> = lines_of(&c.raw_data)
            .into_iter()
            .filter(|l| !l.starts_with(b"##INFO=<ID=STEG"))
            .collect();
        assert_eq!(before.len(), after.len());
        let mut changed = 0;
        for (b, a) in before.iter().zip(&after) {
            assert!(a.ends_with(b"\r\n"), "CRLF must be preserved");
            if b != a {
                changed += 1;
                // Only the INFO column was extended; original text kept as prefix.
                let b = std::str::from_utf8(b).unwrap().trim_end();
                let a = std::str::from_utf8(a).unwrap().trim_end();
                assert!(
                    a.starts_with(b) && a[b.len()..].starts_with(";STEG="),
                    "{a}"
                );
            }
        }
        // 1-byte payload + 38-byte overhead = 2 INFO chunks.
        assert_eq!(changed, 2);
        let text = String::from_utf8(c.raw_data.clone()).unwrap();
        assert!(text.contains("\t29.47\tPASS\tZZ=1;DP=0;AA=0.5;STEG=0_"));
        assert!(text.contains("garbage line without tabs\r\n"));
        assert!(text.contains("chr2\t5\t.\tG\tC\t29.47\tPASS\tB=2;A=1\r\n"));
        assert!(text.contains("##INFO=<ID=STEG,Number=1,Type=String,Description=\"Opaque annotation chunk\">\r\n#CHROM"));
    }

    #[tokio::test]
    async fn test_header_declares_every_new_key() {
        let handler = VcfHandler::new();
        for strategy in ["info", "format", "multi", "id"] {
            let mut c = carrier(make_vcf(30, true, "\n"));
            handler
                .embed(&mut c, b"declare me", &params(strategy, ""))
                .await
                .unwrap();
            let doc = VcfDoc::parse(&c.raw_data).unwrap();
            let mut info_keys = std::collections::HashSet::new();
            let mut format_keys = std::collections::HashSet::new();
            for (_, f) in doc.records() {
                for item in f[7].split(';') {
                    info_keys.insert(item.split('=').next().unwrap().to_string());
                }
                for k in f[8].split(':') {
                    format_keys.insert(k.to_string());
                }
            }
            for k in ["ZZ", "DP", "AA"] {
                info_keys.remove(k); // pre-existing (undeclared in the input fixture)
            }
            format_keys.remove("GT");
            format_keys.remove("DP");
            for k in &info_keys {
                assert!(doc.has_meta("INFO", k), "{strategy}: INFO {k} undeclared");
            }
            for k in &format_keys {
                assert!(
                    doc.has_meta("FORMAT", k),
                    "{strategy}: FORMAT {k} undeclared"
                );
            }
            // Declared only once, and only if used.
            let text = String::from_utf8(c.raw_data.clone()).unwrap();
            assert!(text.matches("##INFO=<ID=STEG").count() <= 1);
            assert_eq!(text.contains("##INFO=<ID=STEG"), info_keys.contains("STEG"));
            assert_eq!(
                text.contains("##FORMAT=<ID=STEG"),
                format_keys.contains("STEG")
            );
        }
    }

    #[tokio::test]
    async fn test_existing_declaration_not_duplicated() {
        let mut data = b"##fileformat=VCFv4.2\n".to_vec();
        data.extend_from_slice(INFO_META.as_bytes());
        data.extend_from_slice(b"\n");
        data.extend_from_slice(&make_vcf(5, false, "\n")[21..]);
        let c = roundtrip(data, b"x", &params("info", "")).await;
        let text = String::from_utf8(c.raw_data).unwrap();
        assert_eq!(text.matches("##INFO=<ID=STEG").count(), 1);
    }

    #[tokio::test]
    async fn test_more_than_100_chunks() {
        let payload: Vec<u8> = (0..3500u32).map(|i| (i * 7 % 251) as u8).collect();
        for strategy in ["info", "id", "format", "quality", "multi"] {
            let c = roundtrip(make_vcf(1000, true, "\n"), &payload, &params(strategy, "")).await;
            if strategy == "id" {
                let text = String::from_utf8(c.raw_data).unwrap();
                assert!(text.contains(";STEG150_") || text.contains("\tSTEG150_"));
            }
        }
    }

    #[tokio::test]
    async fn test_missing_chunk_is_an_error() {
        let handler = VcfHandler::new();
        let payload = vec![0xAB; 200];
        let p = params("info", "");
        let mut c = carrier(make_vcf(20, false, "\n"));
        handler.embed(&mut c, &payload, &p).await.unwrap();
        // Drop the record carrying chunk 1.
        let text = String::from_utf8(c.raw_data).unwrap();
        let cut: String = text
            .split_inclusive('\n')
            .filter(|l| !l.contains(";STEG=1_"))
            .collect();
        let err = handler
            .extract(&carrier(cut.into_bytes()), &p)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("chunk 1"), "{err}");
    }

    #[tokio::test]
    async fn test_unicode_ids_do_not_panic() {
        let mut data = make_vcf(0, false, "\n");
        for (i, id) in [
            "rsé",
            "STEGé_ab",
            "STEG٣_00",
            "STEG1_zz",
            "STEGOSAURUS",
            "ü;STEG",
        ]
        .iter()
        .enumerate()
        {
            data.extend_from_slice(
                format!("chr1\t{}\t{id}\tA\tT\t30\tPASS\tK=ü\n", i + 1).as_bytes(),
            );
        }
        let handler = VcfHandler::new();
        // Nothing embedded yet: clean errors, no panic.
        assert!(handler
            .extract(&carrier(data.clone()), &params("id", ""))
            .await
            .is_err());
        assert!(handler
            .extract(&carrier(data.clone()), &EmbedParams::default())
            .await
            .is_err());
        // Embedding into unicode IDs keeps the original ID and round-trips.
        let c = roundtrip(data, b"u", &params("id", "")).await;
        let text = String::from_utf8(c.raw_data).unwrap();
        assert!(text.contains("\trsé;STEG"));
    }

    #[tokio::test]
    async fn test_sites_only_format_strategy_errors() {
        let handler = VcfHandler::new();
        for name in ["format", "sample"] {
            let mut c = carrier(make_vcf(10, false, "\n"));
            let err = handler
                .embed(&mut c, b"x", &params(name, ""))
                .await
                .unwrap_err();
            assert!(err.to_string().contains("sites-only"), "{err}");
        }
    }

    #[tokio::test]
    async fn test_unknown_strategy_and_mismatch_errors() {
        let handler = VcfHandler::new();
        let mut c = carrier(make_vcf(10, false, "\n"));
        assert!(handler
            .embed(&mut c, b"x", &params("bogus", ""))
            .await
            .is_err());
        handler
            .embed(&mut c, b"x", &params("info", ""))
            .await
            .unwrap();
        assert!(handler.extract(&c, &params("id", "")).await.is_err());
        // Re-embedding into the same channel is refused rather than corrupting it.
        assert!(handler
            .embed(&mut c, b"y", &params("info", ""))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_insufficient_capacity() {
        let handler = VcfHandler::new();
        let mut c = carrier(make_vcf(2, false, "\n"));
        let err = handler
            .embed(&mut c, &[0u8; 500], &params("info", ""))
            .await
            .unwrap_err();
        // Units are bits (#61): 2 records x INFO_CHUNK_BYTES bytes.
        match err {
            SteganoError::InsufficientCapacity { needed, available } => {
                assert_eq!(available, 2 * INFO_CHUNK_BYTES * 8);
                assert_eq!(needed % 8, 0);
                assert!(needed > 500 * 8);
            }
            other => panic!("expected InsufficientCapacity, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_quality_strategy_is_sub_digit_and_keeps_missing() {
        let data = make_vcf(40, false, "\n");
        let c = roundtrip(data.clone(), b"q", &params("quality", "")).await;
        let before = VcfDoc::parse(&data).unwrap();
        let after = VcfDoc::parse(&c.raw_data).unwrap();
        let mut modified = 0;
        for ((_, b), (_, a)) in before.records().zip(after.records()) {
            let (bq, aq) = (b[5], a[5]);
            if bq == aq {
                continue;
            }
            modified += 1;
            assert!(
                is_plain_decimal(bq),
                "only plain decimals are touched: {bq}"
            );
            // Original text is a prefix (plus ".") and the next digit is the 0 guard.
            let base = if bq.contains('.') {
                bq.to_string()
            } else {
                format!("{bq}.")
            };
            assert!(
                aq.starts_with(&base) && aq[base.len()..].starts_with('0'),
                "{bq} -> {aq}"
            );
            let places = base.len() - base.find('.').unwrap() - 1;
            let unit = 10f64.powi(-(places as i32));
            let delta = aq.parse::<f64>().unwrap() - bq.parse::<f64>().unwrap();
            assert!((0.0..0.1 * unit).contains(&delta), "{bq} -> {aq}");
            // Rounding to the printed precision gives back the original text.
            assert_eq!(format!("{:.*}", places, aq.parse::<f64>().unwrap()), bq);
        }
        assert!(modified > 0);
        // '.' and non-plain QUALs untouched.
        for ((_, b), (_, a)) in before.records().zip(after.records()) {
            if !is_plain_decimal(b[5]) {
                assert_eq!(b[5], a[5]);
            }
        }
    }

    #[test]
    fn test_vcf_biological_integrity() {
        let handler = VcfHandler::new();
        assert!(handler
            .verify_vcf_integrity(&make_vcf(3, true, "\n"))
            .unwrap());
        let invalid = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tX\tY\t30.0\tPASS\t.\n";
        assert!(!handler.verify_vcf_integrity(invalid).unwrap());
        let symbolic = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\t<DEL>,*\t30.0\tPASS\t.\n";
        assert!(handler.verify_vcf_integrity(symbolic).unwrap());
    }

    #[test]
    fn test_no_trailing_newline_preserved() {
        let data = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t1\t.\tA\tT\t3\tPASS\t.";
        let doc = VcfDoc::parse(data).unwrap();
        assert_eq!(doc.render(&Edits::default()), data.to_vec());
    }
}
