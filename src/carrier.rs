// SPDX-License-Identifier: Apache-2.0
//! Shared carrier construction and binding defaults.
//!
//! Every front end (CLI, C FFI, Go via the FFI, Python, WebAssembly) builds
//! its [`CarrierData`] through this module so that format detection, image
//! dimensions and audio sample rate are populated the same way everywhere,
//! and so that every front end uses the same default embedding parameters.
//!
//! A carrier embedded through one front end can therefore be extracted
//! through any other one using the defaults.

use crate::core::{
    CarrierData, CipherKind, DataFormat, Dimensions, ECLevel, EmbedParams, StealthLevel,
};
use crate::handlers::HandlerRegistry;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Defaults (single source of truth for all front ends)
// ---------------------------------------------------------------------------

/// Default stealth level used by the CLI, FFI, Go, Python and WASM bindings.
pub const DEFAULT_STEALTH_LEVEL: StealthLevel = StealthLevel::High;
/// String form of [`DEFAULT_STEALTH_LEVEL`] (for CLI / binding signatures).
pub const DEFAULT_STEALTH_NAME: &str = "high";

/// Default error-correction level used by the CLI, FFI, Go, Python and WASM
/// bindings.
///
/// `None` keeps small carriers usable (Reed-Solomon framing adds at least one
/// 255-byte block). Note that whether a password-less payload is run through
/// the codec is decided by the *caller's* EC level on both embed and extract,
/// so the embed and extract sides must agree on "none" vs. "not none"; the
/// actual EC strength is read from the payload frame.
pub const DEFAULT_EC_LEVEL: ECLevel = ECLevel::None;
/// String form of [`DEFAULT_EC_LEVEL`].
pub const DEFAULT_EC_NAME: &str = "none";

/// Default cipher (only used when a password is supplied).
pub const DEFAULT_CIPHER_KIND: CipherKind = CipherKind::Aes256Gcm;
/// String form of [`DEFAULT_CIPHER_KIND`].
pub const DEFAULT_CIPHER_NAME: &str = "aes256gcm";

/// Build [`EmbedParams`] with the shared front-end defaults.
///
/// This intentionally differs from [`EmbedParams::new`] (which uses
/// `ECLevel::Medium`): the front ends use [`DEFAULT_EC_LEVEL`].
pub fn default_params(key: Vec<u8>) -> EmbedParams {
    EmbedParams {
        encryption_key: key,
        error_correction_level: DEFAULT_EC_LEVEL,
        stealth_level: DEFAULT_STEALTH_LEVEL,
        use_gpu: false,
        format_specific: HashMap::new(),
        enable_validation: false,
        cipher_kind: DEFAULT_CIPHER_KIND,
    }
}

/// Build [`EmbedParams`] from string options as used by the Python and WASM
/// bindings. An empty `password` disables encryption. Unknown names are
/// rejected with a descriptive message.
pub fn params_from_names(
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> std::result::Result<EmbedParams, String> {
    let mut params = default_params(password.as_bytes().to_vec());
    params.stealth_level = parse_stealth_level(stealth).ok_or_else(|| {
        format!("unknown stealth level '{stealth}' (expected low, medium, high or maximum)")
    })?;
    params.error_correction_level = parse_ec_level(ec_level).ok_or_else(|| {
        format!(
            "unknown error-correction level '{ec_level}' (expected none, low, medium, high or maximum)"
        )
    })?;
    params.cipher_kind = parse_cipher(cipher)
        .ok_or_else(|| format!("unknown cipher '{cipher}' (expected aes256gcm or chacha20)"))?;
    Ok(params)
}

/// Parse a stealth level name (`low`, `medium`, `high`, `maximum`),
/// case-insensitively. Returns `None` for unknown names.
pub fn parse_stealth_level(s: &str) -> Option<StealthLevel> {
    match s.trim().to_lowercase().as_str() {
        "low" => Some(StealthLevel::Low),
        "medium" => Some(StealthLevel::Medium),
        "high" => Some(StealthLevel::High),
        "maximum" | "max" => Some(StealthLevel::Maximum),
        _ => None,
    }
}

/// Parse an error-correction level name (`none`, `low`, `medium`, `high`,
/// `maximum`), case-insensitively. Returns `None` for unknown names.
pub fn parse_ec_level(s: &str) -> Option<ECLevel> {
    match s.trim().to_lowercase().as_str() {
        "none" | "off" => Some(ECLevel::None),
        "low" => Some(ECLevel::Low),
        "medium" => Some(ECLevel::Medium),
        "high" => Some(ECLevel::High),
        "maximum" | "max" => Some(ECLevel::Maximum),
        _ => None,
    }
}

/// Parse a cipher name (`aes256gcm`, `chacha20`, `chacha20poly1305`;
/// `-`/`_` are ignored), case-insensitively. Returns `None` for unknown names.
pub fn parse_cipher(s: &str) -> Option<CipherKind> {
    match s.trim().to_lowercase().replace(['-', '_'], "").as_str() {
        "aes256gcm" | "aes" | "aesgcm" => Some(CipherKind::Aes256Gcm),
        "chacha20" | "chacha20poly1305" | "chacha" => Some(CipherKind::ChaCha20Poly1305),
        _ => None,
    }
}

/// The `format_specific` key a handler reads its strategy name from, or
/// `None` if the format has no selectable strategy.
pub fn strategy_key(format: &DataFormat) -> Option<&'static str> {
    Some(match format {
        DataFormat::VCF => "vcf_strategy",
        DataFormat::SAM | DataFormat::BAM => "sam_strategy",
        DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY => "imagery_strategy",
        DataFormat::DICOM => "dicom_strategy",
        DataFormat::WAV => "signal_strategy",
        DataFormat::FASTA | DataFormat::FASTQ => "genomics_strategy",
        DataFormat::FITS => "fits_strategy",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// CarrierData construction
// ---------------------------------------------------------------------------

/// Build a fully-populated `CarrierData` from a file path and its bytes.
///
/// Detects the format (extension first, then magic bytes, then handler
/// probing), then reads image dimensions and audio sample rate directly from
/// the file header so that handlers that need those fields work correctly.
pub fn build_carrier(path: &str, raw_data: Vec<u8>) -> CarrierData {
    let format = detect_format(path, &raw_data);
    carrier_from_bytes(format, raw_data)
}

/// Build a fully-populated `CarrierData` for bytes of a known format
/// (dimensions and sample rate are parsed from the header when applicable).
pub fn carrier_from_bytes(format: DataFormat, raw_data: Vec<u8>) -> CarrierData {
    let dimensions = detect_image_dimensions(&raw_data, &format);
    let sampling_rate = detect_wav_sample_rate(&raw_data, &format);
    CarrierData {
        format,
        raw_data,
        metadata: HashMap::new(),
        dimensions,
        sampling_rate,
    }
}

/// Build a fully-populated `CarrierData` from bytes alone (no file name),
/// detecting the format with [`detect_format_from_content`].
pub fn carrier_from_content(raw_data: Vec<u8>) -> CarrierData {
    let format = detect_format_from_content(&raw_data);
    carrier_from_bytes(format, raw_data)
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Detect the data format for `path`, using:
/// 1. File extension (fast and reliable when present)
/// 2. Content detection ([`detect_format_from_content`]) as fallback
pub fn detect_format(path: &str, data: &[u8]) -> DataFormat {
    if let Some(fmt) = detect_format_from_extension(path) {
        return fmt;
    }
    detect_format_from_content(data)
}

/// Detect the data format from bytes only: magic bytes first, then each
/// registered handler's `detect_format`, falling back to `BINARY`.
pub fn detect_format_from_content(data: &[u8]) -> DataFormat {
    if let Some(fmt) = detect_format_from_magic(data) {
        return fmt;
    }
    let registry = HandlerRegistry::new();
    for handler in registry.get_handlers() {
        if let Some(fmt) = handler.detect_format(data) {
            return fmt;
        }
    }
    DataFormat::BINARY
}

/// Map a file extension to a [`DataFormat`], or `None` if unknown.
pub fn detect_format_from_extension(path: &str) -> Option<DataFormat> {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())?
        .to_lowercase();

    Some(match ext.as_str() {
        "fasta" | "fa" => DataFormat::FASTA,
        "fastq" | "fq" => DataFormat::FASTQ,
        "vcf" => DataFormat::VCF,
        "sam" => DataFormat::SAM,
        "bam" => DataFormat::BAM,
        "png" => DataFormat::PNG,
        "tiff" | "tif" => DataFormat::TIFF,
        "dcm" | "dicom" => DataFormat::DICOM,
        "wav" => DataFormat::WAV,
        "csv" => DataFormat::CsvTimeseries,
        "txt" => DataFormat::TEXT,
        "fits" | "fit" | "fts" => DataFormat::FITS,
        _ => return None,
    })
}

/// Detect format from magic bytes (file signatures).
pub fn detect_format_from_magic(data: &[u8]) -> Option<DataFormat> {
    if data.len() < 4 {
        return None;
    }

    // PNG: 8-byte signature
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(DataFormat::PNG);
    }
    // TIFF little-endian ("II\x2A\x00") or big-endian ("MM\x00\x2A")
    if data.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(DataFormat::TIFF);
    }
    // DICOM: 128-byte preamble then "DICM"
    if data.len() >= 132 && &data[128..132] == b"DICM" {
        return Some(DataFormat::DICOM);
    }
    // WAV: "RIFF....WAVE"
    if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WAVE" {
        return Some(DataFormat::WAV);
    }
    // BAM: "BAM\1"
    if data.starts_with(&[b'B', b'A', b'M', 1]) {
        return Some(DataFormat::BAM);
    }
    // FASTA/FASTQ: text-based, detect by first byte
    if data.first() == Some(&b'>') {
        return Some(DataFormat::FASTA);
    }
    if data.first() == Some(&b'@') {
        // Could be FASTQ or SAM; check for SAM header tags
        if data.starts_with(b"@HD") || data.starts_with(b"@SQ") || data.starts_with(b"@PG") {
            return Some(DataFormat::SAM);
        }
        return Some(DataFormat::FASTQ);
    }
    if data.starts_with(b"##fileformat=VCF") {
        return Some(DataFormat::VCF);
    }
    // FITS: "SIMPLE  " (8-byte keyword, space-padded)
    if data.len() >= 8 && &data[..8] == b"SIMPLE  " {
        return Some(DataFormat::FITS);
    }

    None
}

// ---------------------------------------------------------------------------
// Image dimension detection
// ---------------------------------------------------------------------------

/// Read image dimensions from raw file bytes.
/// Returns `None` if the format doesn't use dimensions or the header is unreadable.
pub fn detect_image_dimensions(data: &[u8], format: &DataFormat) -> Option<Dimensions> {
    match format {
        DataFormat::PNG => detect_png_dimensions(data),
        DataFormat::TIFF | DataFormat::MICROSCOPY => detect_tiff_dimensions(data),
        _ => None,
    }
}

/// Parse width and height from a PNG IHDR chunk.
///
/// Layout: `[8-byte magic][4-byte chunk_len][4-byte "IHDR"][4-byte width BE][4-byte height BE]...`
pub fn detect_png_dimensions(data: &[u8]) -> Option<Dimensions> {
    if data.len() < 24 {
        return None;
    }
    if !data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return None;
    }
    if &data[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if width == 0 || height == 0 {
        return None;
    }
    Some(Dimensions::new_2d(width, height))
}

/// Parse ImageWidth (tag 256) and ImageLength (tag 257) from a TIFF IFD.
///
/// All offset arithmetic is checked, so hostile headers cannot cause a panic
/// (including on 32-bit targets such as wasm32).
pub fn detect_tiff_dimensions(data: &[u8]) -> Option<Dimensions> {
    if data.len() < 8 {
        return None;
    }
    let little_endian = match &data[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };

    // Endian-aware reads; `None` on out-of-range or overflowing offsets.
    let u16_at = |off: usize| -> Option<u32> {
        let b = data.get(off..off.checked_add(2)?)?;
        Some(if little_endian {
            u16::from_le_bytes([b[0], b[1]]) as u32
        } else {
            u16::from_be_bytes([b[0], b[1]]) as u32
        })
    };
    let u32_at = |off: usize| -> Option<u32> {
        let b = data.get(off..off.checked_add(4)?)?;
        Some(if little_endian {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    };

    // Magic check (42).
    if u16_at(2)? != 42 {
        return None;
    }

    let ifd_offset = usize::try_from(u32_at(4)?).ok()?;
    let num_entries = u16_at(ifd_offset)? as usize;
    let entries_start = ifd_offset.checked_add(2)?;

    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;

    for i in 0..num_entries {
        let entry = entries_start.checked_add(i.checked_mul(12)?)?;
        if entry.checked_add(12)? > data.len() {
            break;
        }
        let tag = u16_at(entry)?;
        let field_type = u16_at(entry + 2)?;

        // Read the value field (4 bytes at entry+8); for SHORT it's in the first 2 bytes.
        let value = match field_type {
            1 | 6 => data[entry + 8] as u32, // BYTE / SBYTE
            3 | 8 => u16_at(entry + 8)?,     // SHORT / SSHORT
            4 | 9 => u32_at(entry + 8)?,     // LONG / SLONG
            _ => continue,
        };

        match tag {
            256 => width = Some(value),  // ImageWidth
            257 => height = Some(value), // ImageLength
            _ => {}
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }

    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some(Dimensions::new_2d(w, h)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Audio sample-rate detection
// ---------------------------------------------------------------------------

/// Read the sample rate from a WAV fmt chunk.
///
/// Layout: `RIFF[4] size[4] WAVE[4] fmt [4] chunk_size[4] audio_fmt[2] channels[2] sample_rate[4]...`
pub fn detect_wav_sample_rate(data: &[u8], format: &DataFormat) -> Option<f64> {
    if !matches!(format, DataFormat::WAV) {
        return None;
    }
    if data.len() < 28 {
        return None;
    }
    if !data.starts_with(b"RIFF") || &data[8..12] != b"WAVE" || &data[12..16] != b"fmt " {
        return None;
    }
    let rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    if rate == 0 {
        return None;
    }
    Some(rate as f64)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- defaults / parsing ----

    #[test]
    fn test_default_params_use_shared_defaults() {
        let p = default_params(vec![]);
        assert_eq!(p.stealth_level, DEFAULT_STEALTH_LEVEL);
        assert_eq!(p.error_correction_level, DEFAULT_EC_LEVEL);
        assert_eq!(p.cipher_kind, DEFAULT_CIPHER_KIND);
        assert_eq!(
            parse_stealth_level(DEFAULT_STEALTH_NAME),
            Some(DEFAULT_STEALTH_LEVEL)
        );
        assert_eq!(parse_ec_level(DEFAULT_EC_NAME), Some(DEFAULT_EC_LEVEL));
        assert_eq!(parse_cipher(DEFAULT_CIPHER_NAME), Some(DEFAULT_CIPHER_KIND));
    }

    #[test]
    fn test_parse_levels() {
        assert_eq!(parse_stealth_level("Maximum"), Some(StealthLevel::Maximum));
        assert_eq!(parse_stealth_level("LOW"), Some(StealthLevel::Low));
        assert_eq!(parse_stealth_level("ultra"), None);
        assert_eq!(parse_ec_level("Medium"), Some(ECLevel::Medium));
        assert_eq!(parse_ec_level("none"), Some(ECLevel::None));
        assert_eq!(parse_ec_level("42"), None);
        assert_eq!(
            parse_cipher("ChaCha20-Poly1305"),
            Some(CipherKind::ChaCha20Poly1305)
        );
        assert_eq!(parse_cipher("aes256gcm"), Some(CipherKind::Aes256Gcm));
        assert_eq!(parse_cipher("xor"), None);
    }

    #[test]
    fn test_params_from_names() {
        let p = params_from_names(
            "",
            DEFAULT_STEALTH_NAME,
            DEFAULT_EC_NAME,
            DEFAULT_CIPHER_NAME,
        )
        .unwrap();
        assert!(p.encryption_key.is_empty());
        assert_eq!(p.stealth_level, DEFAULT_STEALTH_LEVEL);
        let p = params_from_names("pw", "maximum", "medium", "chacha20-poly1305").unwrap();
        assert_eq!(p.encryption_key, b"pw".to_vec());
        assert_eq!(p.stealth_level, StealthLevel::Maximum);
        assert_eq!(p.error_correction_level, ECLevel::Medium);
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
        assert!(params_from_names("", "ultra", "none", "aes256gcm").is_err());
        assert!(params_from_names("", "high", "extreme", "aes256gcm").is_err());
        assert!(params_from_names("", "high", "none", "xorcrypt").is_err());
    }

    #[test]
    fn test_strategy_keys() {
        assert_eq!(strategy_key(&DataFormat::VCF), Some("vcf_strategy"));
        assert_eq!(strategy_key(&DataFormat::SAM), Some("sam_strategy"));
        assert_eq!(strategy_key(&DataFormat::BAM), Some("sam_strategy"));
        assert_eq!(strategy_key(&DataFormat::PNG), Some("imagery_strategy"));
        assert_eq!(strategy_key(&DataFormat::TIFF), Some("imagery_strategy"));
        assert_eq!(strategy_key(&DataFormat::DICOM), Some("dicom_strategy"));
        assert_eq!(strategy_key(&DataFormat::WAV), Some("signal_strategy"));
        assert_eq!(strategy_key(&DataFormat::FASTA), Some("genomics_strategy"));
        assert_eq!(strategy_key(&DataFormat::FITS), Some("fits_strategy"));
        assert_eq!(strategy_key(&DataFormat::BINARY), None);
    }

    // ---- format detection from extension ----

    #[test]
    fn test_format_from_extension() {
        let cases = [
            ("photo.png", DataFormat::PNG),
            ("scan.tiff", DataFormat::TIFF),
            ("scan.tif", DataFormat::TIFF),
            ("mri.dcm", DataFormat::DICOM),
            ("mri.dicom", DataFormat::DICOM),
            ("audio.wav", DataFormat::WAV),
            ("reads.fasta", DataFormat::FASTA),
            ("reads.fa", DataFormat::FASTA),
            ("reads.fastq", DataFormat::FASTQ),
            ("reads.fq", DataFormat::FASTQ),
            ("variants.vcf", DataFormat::VCF),
            ("align.sam", DataFormat::SAM),
            ("align.bam", DataFormat::BAM),
            ("sky.fits", DataFormat::FITS),
        ];
        for (path, want) in cases {
            assert_eq!(detect_format_from_extension(path), Some(want), "{path}");
        }
        assert_eq!(detect_format_from_extension("file.xyz"), None);
        assert_eq!(detect_format_from_extension("noextension"), None);
    }

    // ---- format detection from magic bytes ----

    #[test]
    fn test_magic_png() {
        let data = make_png_header(10, 20);
        assert_eq!(detect_format_from_magic(&data), Some(DataFormat::PNG));
    }

    #[test]
    fn test_magic_tiff() {
        let mut le = vec![0u8; 8];
        le[0..4].copy_from_slice(&[b'I', b'I', 0x2A, 0x00]);
        assert_eq!(detect_format_from_magic(&le), Some(DataFormat::TIFF));
        let mut be = vec![0u8; 8];
        be[0..4].copy_from_slice(&[b'M', b'M', 0x00, 0x2A]);
        assert_eq!(detect_format_from_magic(&be), Some(DataFormat::TIFF));
    }

    #[test]
    fn test_magic_misc() {
        let mut wav = vec![0u8; 12];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        assert_eq!(detect_format_from_magic(&wav), Some(DataFormat::WAV));
        assert_eq!(
            detect_format_from_magic(b">chr1 sequence\nACGT"),
            Some(DataFormat::FASTA)
        );
        assert_eq!(
            detect_format_from_magic(b"@read1\nACGT\n+\nIIII"),
            Some(DataFormat::FASTQ)
        );
        assert_eq!(
            detect_format_from_magic(
                b"@HD\tVN:1.6\nread1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tIIII"
            ),
            Some(DataFormat::SAM)
        );
        assert_eq!(
            detect_format_from_magic(b"##fileformat=VCFv4.2\n#CHROM\t..."),
            Some(DataFormat::VCF)
        );
        assert_eq!(detect_format_from_magic(b"hello world"), None);
        assert_eq!(detect_format_from_magic(b"\x00\x01\x02"), None);
    }

    #[test]
    fn test_content_detection_falls_back_to_binary() {
        assert_eq!(
            detect_format_from_content(b"this is definitely not a known scientific format"),
            DataFormat::BINARY
        );
    }

    // ---- PNG dimension detection ----

    fn make_png_header(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[8, 2, 0, 0, 0]);
        data
    }

    #[test]
    fn test_png_dimensions() {
        let dims = detect_png_dimensions(&make_png_header(640, 480)).unwrap();
        assert_eq!((dims.width, dims.height), (640, 480));
        assert!(detect_png_dimensions(&[0u8; 10]).is_none());
        let mut bad = make_png_header(100, 100);
        bad[0] = 0xFF;
        assert!(detect_png_dimensions(&bad).is_none());
        assert!(detect_png_dimensions(&make_png_header(0, 100)).is_none());
    }

    // ---- TIFF dimension detection ----

    fn make_tiff_le(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"II");
        data.extend_from_slice(&42u16.to_le_bytes());
        data.extend_from_slice(&8u32.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        for (tag, value) in [(256u16, width), (257u16, height)] {
            data.extend_from_slice(&tag.to_le_bytes());
            data.extend_from_slice(&4u16.to_le_bytes());
            data.extend_from_slice(&1u32.to_le_bytes());
            data.extend_from_slice(&value.to_le_bytes());
        }
        data
    }

    #[test]
    fn test_tiff_dimensions() {
        let dims = detect_tiff_dimensions(&make_tiff_le(1920, 1080)).unwrap();
        assert_eq!((dims.width, dims.height), (1920, 1080));
        assert!(detect_tiff_dimensions(&[0u8; 4]).is_none());
        let mut bad = make_tiff_le(100, 100);
        bad[2] = 0xFF;
        assert!(detect_tiff_dimensions(&bad).is_none());
    }

    #[test]
    fn test_tiff_hostile_offset_does_not_panic() {
        let mut data = make_tiff_le(1, 1);
        data[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(detect_tiff_dimensions(&data).is_none());
    }

    // ---- WAV sample rate detection ----

    fn make_wav_header(sample_rate: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(b"WAVE");
        data.extend_from_slice(b"fmt ");
        data.extend_from_slice(&16u32.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&sample_rate.to_le_bytes());
        data
    }

    #[test]
    fn test_wav_sample_rate() {
        assert_eq!(
            detect_wav_sample_rate(&make_wav_header(44100), &DataFormat::WAV),
            Some(44100.0)
        );
        assert_eq!(
            detect_wav_sample_rate(&make_wav_header(48000), &DataFormat::WAV),
            Some(48000.0)
        );
        assert!(detect_wav_sample_rate(&make_wav_header(44100), &DataFormat::PNG).is_none());
        assert!(detect_wav_sample_rate(&[0u8; 20], &DataFormat::WAV).is_none());
    }

    #[test]
    fn test_carrier_from_bytes_populates_rate_and_dims() {
        let wav = carrier_from_bytes(DataFormat::WAV, make_wav_header(48000));
        assert_eq!(wav.sampling_rate, Some(48000.0));
        let png = carrier_from_content(make_png_header(32, 16));
        assert_eq!(png.format, DataFormat::PNG);
        let dims = png.dimensions.unwrap();
        assert_eq!((dims.width, dims.height), (32, 16));
    }
}
