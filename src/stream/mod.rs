//! Streaming embed/extract for very large carriers (multi-GB FASTQ, FITS, WAV, ...).
//!
//! Payloads are small and carriers are large, so only the **carrier** is
//! streamed.  The payload is encoded once, in memory, with
//! [`codec::encode_payload`](crate::codec::encode_payload) (wire format v2,
//! unchanged: Argon2id runs exactly once), written into the first units of
//! carrier capacity, and the rest of the carrier is passed through.  The
//! output is byte-identical to what the in-memory [`DataHandler`] writes for
//! the same encoded payload, so the two APIs interoperate in both directions.
//!
//! Memory use is bounded by the carrier's record size (for FASTQ: one 4-line
//! record) plus the encoded payload, independent of the carrier's total size.
//! Extraction stops reading as soon as the embedded frame is complete.
//!
//! # Supported formats
//!
//! | Format | Strategies |
//! |--------|------------|
//! | [`DataFormat::FASTQ`] | quality-score LSB (`genomics_strategy` = `quality`, `conservative`, `balanced`, `aggressive`, or absent) |
//! | [`DataFormat::VCF`] | `vcf_strategy` = `info` (or absent), `id`, `quality`; extraction also reads `format`/`multi` files |
//! | [`DataFormat::SAM`], [`DataFormat::BAM`] | `sam_strategy` = `auxiliary` or `quality` (BAM: BGZF or raw `BAM\1` input; `bam_compression` = `bgzf` (default) or `none`); extraction also reads `optional`/`comment`/`multi` files |
//! | [`DataFormat::CsvTimeseries`] | last-digit parity (`csv_strategy` = `digit` or absent; `csv_delimiter` honoured on embed and extract) |
//! | [`DataFormat::TEXT`] | trailing whitespace, embed only with a fixed `text_bits_per_line` (the automatic choice needs the line count up front); extraction works for any `k` |
//! | [`DataFormat::FITS`] | sample LSB of the first suitable image HDU (`fits_strategy` = `pixel_lsb` (or absent) or `float_mantissa`); other HDUs, headers and padding pass through |
//! | [`DataFormat::WAV`] | time-domain LSB only (`signal_strategy` = `timedomain`, or `StealthLevel::Low` with no `signal_strategy`); `frequency`/`phase`/`spread`/`echo` are in-memory only (channel-major bit order, whole-carrier self-verification), and stream extraction of such a payload returns the "not streamable" error |
//!
//! Anything else returns [`SteganoError::InvalidParameter`]
//! (`"<format>/<strategy> is not streamable; use the in-memory API"`); use
//! [`is_streamable`] to check up front.
//!
//! # Insufficient capacity and partial output
//!
//! A single pass cannot know the carrier's capacity before reaching its end.
//! [`embed_stream`] therefore writes the carrier to `writer` as it goes and,
//! if the payload did not fit, returns [`SteganoError::InsufficientCapacity`]
//! **at EOF, after the whole carrier has been written**.  The same applies to
//! a structural error discovered part-way through the carrier.  **On any
//! error the output written so far is incomplete or carries a truncated
//! payload and must be discarded.**  [`embed_file`] does this for you: it
//! writes to a temporary file in the destination directory and renames it
//! over `out_path` only on success.
//!
//! # Example
//!
//! ```rust,no_run
//! use stegano::{DataFormat, EmbedParams};
//! use stegano::stream::{embed_file, extract_file};
//!
//! let params = EmbedParams::new(b"password".to_vec());
//! embed_file(DataFormat::FASTQ, "reads.fastq", "reads.stego.fastq", b"secret", &params)?;
//! let secret = extract_file(DataFormat::FASTQ, "reads.stego.fastq", &params)?;
//! assert_eq!(secret, b"secret");
//! # Ok::<(), stegano::SteganoError>(())
//! ```
//!
//! [`DataHandler`]: crate::handlers::DataHandler

use crate::core::{DataFormat, EmbedParams, EmbedResult, PerformanceStats, Result, SteganoError};
use crate::time::Instant;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

mod binary;
mod csv;
mod fastq;
mod fits;
mod sam;
mod text;
mod vcf;
mod wav;

/// I/O buffer size used by [`embed_file`] / [`extract_file`].
const FILE_BUF: usize = 256 * 1024;

/// Statistics reported by a [`StreamHandler`] embed.
#[derive(Debug, Clone, Default)]
pub(crate) struct StreamStats {
    /// Carrier bytes read.
    #[cfg_attr(not(test), allow(dead_code))]
    pub bytes_read: u64,
    /// Carrier bytes written.
    #[cfg_attr(not(test), allow(dead_code))]
    pub bytes_written: u64,
    /// Peak bytes held in the handler's internal buffers.
    pub peak_buffer_bytes: usize,
}

/// A format that can be embedded/extracted in a single sequential pass.
pub(crate) trait StreamHandler: Sync {
    /// Validate embed parameters (strategy); must not touch the carrier.
    fn check_params(&self, params: &EmbedParams) -> Result<()>;

    /// Stream `reader` to `writer`, embedding the already codec-encoded
    /// `encoded` bytes.  Writes the whole carrier even if the payload does
    /// not fit, then returns [`SteganoError::InsufficientCapacity`].
    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats>;

    /// Read the codec-encoded bytes from `reader`, stopping as soon as the
    /// frame is complete.  `params` supplies format options that extraction
    /// needs (e.g. the CSV delimiter); the strategy is auto-detected.
    fn extract_encoded(&self, reader: &mut dyn BufRead, params: &EmbedParams) -> Result<Vec<u8>>;
}

/// Name of the strategy requested in `params` (for error messages).
fn strategy_name(params: &EmbedParams) -> String {
    params
        .format_specific
        .iter()
        .find(|(k, _)| k.ends_with("_strategy"))
        .map(|(_, v)| v.as_str().map_or_else(|| v.to_string(), str::to_string))
        .unwrap_or_else(|| "default".to_string())
}

fn not_streamable(format: &DataFormat, params: &EmbedParams) -> SteganoError {
    SteganoError::InvalidParameter(format!(
        "{format:?}/{} is not streamable; use the in-memory API",
        strategy_name(params)
    ))
}

/// Registry: the stream handler for `format`, if any.
fn handler_for(format: &DataFormat) -> Option<&'static dyn StreamHandler> {
    match format {
        DataFormat::FASTQ => Some(&fastq::FastqStream),
        DataFormat::VCF => Some(&vcf::VcfStream),
        DataFormat::SAM => Some(&sam::SAM_STREAM),
        DataFormat::BAM => Some(&sam::BAM_STREAM),
        DataFormat::CsvTimeseries => Some(&csv::CsvStream),
        DataFormat::TEXT => Some(&text::TextStream),
        DataFormat::FITS => Some(&fits::FitsStream),
        DataFormat::WAV => Some(&wav::WavStream),
        _ => None,
    }
}

fn embed_handler(format: &DataFormat, params: &EmbedParams) -> Result<&'static dyn StreamHandler> {
    let h = handler_for(format).ok_or_else(|| not_streamable(format, params))?;
    h.check_params(params)?;
    Ok(h)
}

/// Whether [`embed_stream`] supports `format` with the strategy selected by
/// `params`.  (Extraction ignores the strategy parameter and only needs a
/// streamable format.)
pub fn is_streamable(format: &DataFormat, params: &EmbedParams) -> bool {
    embed_handler(format, params).is_ok()
}

/// Crate-internal entry point: stream-embed pre-encoded codec bytes (the
/// output of [`codec::encode_payload`](crate::codec::encode_payload)).
/// The in-memory handlers take the same bytes, which lets tests compare the
/// two paths byte for byte.
pub(crate) fn embed_encoded_stream(
    format: &DataFormat,
    reader: &mut dyn BufRead,
    writer: &mut dyn Write,
    encoded: &[u8],
    params: &EmbedParams,
) -> Result<StreamStats> {
    let stats = embed_handler(format, params)?.embed_encoded(reader, writer, encoded, params)?;
    writer.flush()?;
    Ok(stats)
}

/// Crate-internal entry point: stream-extract the raw codec bytes (not yet
/// decoded/decrypted).
pub(crate) fn extract_encoded_stream(
    format: &DataFormat,
    reader: &mut dyn BufRead,
    params: &EmbedParams,
) -> Result<Vec<u8>> {
    handler_for(format)
        .ok_or_else(|| not_streamable(format, params))?
        .extract_encoded(reader, params)
}

/// Embed `payload` while streaming the carrier from `reader` to `writer`.
///
/// The output is byte-identical to the in-memory API's for the same encoded
/// payload.  `writer` receives many small writes, so wrap files in a
/// [`BufWriter`]; it is flushed before returning `Ok`.
///
/// # Errors
///
/// * [`SteganoError::InvalidParameter`] if the format/strategy is not
///   streamable (nothing is read or written).
/// * [`SteganoError::InsufficientCapacity`] (units: bits) if the payload did
///   not fit, or a structural error if the carrier is malformed.  These are
///   detected while streaming: **the output written so far is then
///   incomplete and must be discarded** (see [`embed_file`]).
pub fn embed_stream<R: BufRead, W: Write>(
    format: DataFormat,
    mut reader: R,
    mut writer: W,
    payload: &[u8],
    params: &EmbedParams,
) -> Result<EmbedResult> {
    let start = Instant::now();
    // Reject unsupported format/strategy before the (expensive) codec.
    embed_handler(&format, params)?;

    let enc_start = Instant::now();
    let encoded = crate::codec::encode_payload(payload, params)?;
    let encryption_time_ms = if params.encryption_key.is_empty() {
        0
    } else {
        (enc_start.elapsed().as_millis() as u64).max(1)
    };

    let stats = embed_encoded_stream(&format, &mut reader, &mut writer, &encoded, params)?;

    Ok(EmbedResult::success(
        payload.len(),
        PerformanceStats {
            embedding_time_ms: start.elapsed().as_millis() as u64,
            encryption_time_ms,
            gpu_acceleration_used: false,
            memory_usage_bytes: stats.peak_buffer_bytes + encoded.len(),
        },
    ))
}

/// Extract a payload by streaming the carrier from `reader`.
///
/// Reading stops as soon as the embedded frame is complete, so only a
/// prefix of a large carrier is consumed (the rest of `reader` is left
/// unread).  The strategy is auto-detected from the carrier.
pub fn extract_stream<R: BufRead>(
    format: DataFormat,
    mut reader: R,
    params: &EmbedParams,
) -> Result<Vec<u8>> {
    let raw = extract_encoded_stream(&format, &mut reader, params)?;
    crate::codec::decode_payload(&raw, params)
}

/// Removes the temporary file unless disarmed.
struct TempGuard(Option<PathBuf>);

impl Drop for TempGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = fs::remove_file(p);
        }
    }
}

/// Create a fresh temporary file next to `out_path`.
fn create_temp(out_path: &Path) -> Result<(PathBuf, File)> {
    let dir = match out_path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = out_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    for _ in 0..16 {
        let tag = hex::encode(crate::crypto::random_array::<6>());
        let tmp = dir.join(format!(".{name}.stegano-{tag}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => return Ok((tmp, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(SteganoError::IoError(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a unique temporary file",
    )))
}

/// Embed `payload` streaming `in_path` to `out_path`, atomically.
///
/// The output is written to a temporary file in `out_path`'s directory and
/// renamed over `out_path` only if embedding succeeds; on any error
/// (including [`SteganoError::InsufficientCapacity`]) the temporary file is
/// removed and `out_path` is left untouched.  `in_path` and `out_path` may
/// be the same file.
pub fn embed_file<P: AsRef<Path>, Q: AsRef<Path>>(
    format: DataFormat,
    in_path: P,
    out_path: Q,
    payload: &[u8],
    params: &EmbedParams,
) -> Result<EmbedResult> {
    let out_path = out_path.as_ref();
    // Reject unsupported format/strategy before touching the filesystem.
    embed_handler(&format, params)?;
    let reader = BufReader::with_capacity(FILE_BUF, File::open(in_path.as_ref())?);
    let (tmp, file) = create_temp(out_path)?;
    let mut guard = TempGuard(Some(tmp.clone()));

    let mut writer = BufWriter::with_capacity(FILE_BUF, file);
    let result = embed_stream(format, reader, &mut writer, payload, params)?;
    let file = writer.into_inner().map_err(|e| e.into_error())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, out_path)?;
    guard.0 = None;
    Ok(result)
}

/// Extract a payload from the file at `path` (see [`extract_stream`]).
pub fn extract_file<P: AsRef<Path>>(
    format: DataFormat,
    path: P,
    params: &EmbedParams,
) -> Result<Vec<u8>> {
    let reader = BufReader::with_capacity(FILE_BUF, File::open(path.as_ref())?);
    extract_stream(format, reader, params)
}

#[cfg(test)]
mod binary_tests;
#[cfg(test)]
mod tests;
