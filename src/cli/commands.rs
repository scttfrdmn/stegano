use crate::core::*;
use crate::embedding::SteganoEmbedder;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub async fn handle_embed(
    input: &str,
    payload: &str,
    output: &str,
    password: Option<&str>,
    validate: bool,
    gpu: bool,
    stealth: &str,
    ec_level: &str,
    strategy: Option<&str>,
    cipher: &str,
) -> Result<()> {
    println!("🔐 Embedding {payload} into {input} -> {output}");

    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read carrier file: {e}")))?;
    let payload_data = fs::read(payload)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read payload file: {e}")))?;

    let mut carrier = build_carrier(input, carrier_data);

    let stealth_level = parse_stealth_level(stealth);
    let ec = parse_ec_level(ec_level);

    // Warn when ECLevel::Low is chosen with a small payload (RS block is always 255 bytes).
    if matches!(ec, ECLevel::Low) && payload_data.len() < 200 {
        println!("⚠️  ECLevel::Low uses a 255-byte RS block regardless of payload size.");
        println!("   Consider --ec-level medium for payloads smaller than ~200 bytes.");
    }

    let key = password.map(|p| p.as_bytes().to_vec()).unwrap_or_default();
    let mut params = EmbedParams::new(key)
        .with_stealth_level(stealth_level)
        .with_ec_level(ec);

    if cipher.eq_ignore_ascii_case("chacha20") || cipher.eq_ignore_ascii_case("chacha20poly1305") {
        params = params.with_chacha20();
    }

    if gpu {
        params = params.with_gpu();
    }
    if validate {
        params = params.with_validation();
    }

    if let Some(s) = strategy {
        apply_strategy(&mut params, &carrier.format, s);
    }

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!(
        "📊 Available capacity: {} bytes",
        capacity.recommended_bits / 8
    );

    if payload_data.len() > capacity.recommended_bits / 8 {
        return Err(SteganoError::InsufficientCapacity {
            needed: payload_data.len(),
            available: capacity.recommended_bits / 8,
        });
    }

    let result = embedder.embed(&mut carrier, &payload_data, &params).await?;

    fs::write(output, &carrier.raw_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {e}")))?;

    println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
    println!(
        "📈 Performance: {:.2}ms",
        result.performance_stats.total_time_ms()
    );

    if let Some(validation_report) = result.validation_report {
        println!("\n🔬 Statistical Validation Results:");
        println!("   Overall Score: {}/10", validation_report.overall_score());
        println!(
            "   Tests Completed: {}",
            validation_report.test_results().len()
        );

        for test_name in validation_report.test_results().keys() {
            println!("   ✓ {test_name}");
        }

        if validation_report.overall_score() >= 8 {
            println!("   🎉 Excellent validation score - embedding is statistically robust!");
        } else if validation_report.overall_score() >= 6 {
            println!("   ⚠️  Good validation score - embedding passed most tests");
        } else {
            println!("   ❌ Low validation score - embedding may be detectable");
        }
    }

    Ok(())
}

pub async fn handle_extract(
    input: &str,
    output: &str,
    password: Option<&str>,
    gpu: bool,
    strategy: Option<&str>,
) -> Result<()> {
    println!("🔍 Extracting from {input} -> {output}");

    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {e}")))?;

    let carrier = build_carrier(input, carrier_data);

    let key = password.map(|p| p.as_bytes().to_vec()).unwrap_or_default();
    let mut params = EmbedParams::new(key);

    if gpu {
        params = params.with_gpu();
    }

    if let Some(s) = strategy {
        apply_strategy(&mut params, &carrier.format, s);
    }

    let embedder = SteganoEmbedder::new();
    let extracted_data = embedder.extract(&carrier, &params).await?;

    fs::write(output, &extracted_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {e}")))?;

    println!("✅ Successfully extracted {} bytes", extracted_data.len());

    Ok(())
}

pub async fn handle_analyze(input: &str, validate: bool) -> Result<()> {
    println!("📊 Analyzing {input}");

    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {e}")))?;

    let carrier = build_carrier(input, carrier_data);

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;

    println!("📈 Capacity Analysis for {:?}:", carrier.format);
    println!(
        "  Total capacity: {} bits ({} bytes)",
        capacity.total_bits,
        capacity.total_bits / 8
    );
    println!(
        "  Recommended capacity: {} bits ({} bytes)",
        capacity.recommended_bits,
        capacity.recommended_bits / 8
    );

    if let Some(ref dims) = carrier.dimensions {
        println!("  Image dimensions: {}×{}", dims.width, dims.height);
    }
    if let Some(rate) = carrier.sampling_rate {
        println!("  Sample rate: {} Hz", rate as u32);
    }

    let has_hidden_data = embedder.verify_integrity(&carrier)?;
    println!(
        "  Contains hidden data: {}",
        if has_hidden_data { "Yes" } else { "No" }
    );

    if validate {
        println!("\n🔬 Running Statistical Validation Analysis...");

        use crate::validation::StatisticalValidationSuite;
        let validation_suite = StatisticalValidationSuite::new();

        let mut frequencies = vec![0.0; 256];
        for &byte in &carrier.raw_data {
            frequencies[byte as usize] += 1.0;
        }

        if let Ok(chi_square_result) = validation_suite.classical_tests().chi_square_test(
            &frequencies,
            &vec![carrier.raw_data.len() as f64 / 256.0; 256],
        ) {
            println!(
                "  Chi-square test p-value: {:.6}",
                chi_square_result.p_value
            );
            println!(
                "  Chi-square significant: {}",
                chi_square_result.is_significant
            );
        }

        let median = 127.5;
        let bool_sequence: Vec<bool> = carrier
            .raw_data
            .iter()
            .map(|&b| b as f64 > median)
            .collect();
        if let Ok(runs_result) = validation_suite.classical_tests().runs_test(&bool_sequence) {
            println!("  Runs test p-value: {:.6}", runs_result.p_value);
            println!("  Randomness test passed: {}", runs_result.is_random);
        }

        println!("  ✅ Statistical validation analysis complete");
    }

    if !capacity.format_specific_info.is_empty() {
        println!("\n📋 Format-specific info:");
        for (key, value) in &capacity.format_specific_info {
            println!("    {key}: {value}");
        }
    }

    Ok(())
}

pub async fn handle_detect(input: &str, validate: bool) -> Result<()> {
    println!("🔍 Detecting hidden data in {input}");

    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {e}")))?;

    let carrier = build_carrier(input, carrier_data);

    println!("   Format: {:?}", carrier.format);
    if let Some(ref dims) = carrier.dimensions {
        println!("   Dimensions: {}×{}", dims.width, dims.height);
    }
    if let Some(rate) = carrier.sampling_rate {
        println!("   Sample rate: {} Hz", rate as u32);
    }

    let embedder = SteganoEmbedder::new();
    let has_data = embedder.verify_integrity(&carrier)?;

    if has_data {
        println!("   Contains hidden data: Yes");
    } else {
        println!("   Contains hidden data: No");
    }

    if validate {
        println!("\n🔬 Statistical Analysis:");

        use crate::validation::StatisticalValidationSuite;
        let validation_suite = StatisticalValidationSuite::new();

        let mut frequencies = vec![0.0; 256];
        for &byte in &carrier.raw_data {
            frequencies[byte as usize] += 1.0;
        }

        if let Ok(chi_square_result) = validation_suite.classical_tests().chi_square_test(
            &frequencies,
            &vec![carrier.raw_data.len() as f64 / 256.0; 256],
        ) {
            println!(
                "   Chi-square p-value: {:.6}  ({})",
                chi_square_result.p_value,
                if chi_square_result.is_significant {
                    "significant — possible steganographic content"
                } else {
                    "not significant"
                }
            );
        }

        let median = 127.5;
        let bool_sequence: Vec<bool> =
            carrier.raw_data.iter().map(|&b| b as f64 > median).collect();
        if let Ok(runs_result) = validation_suite.classical_tests().runs_test(&bool_sequence) {
            println!(
                "   Runs test p-value: {:.6}  ({})",
                runs_result.p_value,
                if runs_result.is_random {
                    "random"
                } else {
                    "non-random — possible steganographic content"
                }
            );
        }

        if has_data {
            println!("   ⚠️  Steganographic header detected — statistical tests may reflect embedded content");
        } else {
            println!("   ✅ No steganographic indicators detected");
        }
    }

    // Exit with code 1 when no hidden data found (enables shell scripting).
    if !has_data {
        std::process::exit(1);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// CarrierData construction
// ---------------------------------------------------------------------------

/// Build a fully-populated `CarrierData` from a file path and its bytes.
///
/// Detects the format (extension first, then magic bytes), then reads
/// image dimensions and audio sample rate directly from the file header so
/// that handlers that require those fields (imagery, signals) work correctly.
fn build_carrier(path: &str, raw_data: Vec<u8>) -> CarrierData {
    let format = detect_format(path, &raw_data);
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

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Detect the data format for `path`, using:
/// 1. File extension (fast and reliable when present)
/// 2. Magic bytes as fallback (handles missing/wrong extensions)
fn detect_format(path: &str, data: &[u8]) -> DataFormat {
    if let Some(fmt) = detect_format_from_extension(path) {
        return fmt;
    }
    detect_format_from_magic(data).unwrap_or(DataFormat::BINARY)
}

fn detect_format_from_extension(path: &str) -> Option<DataFormat> {
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
fn detect_format_from_magic(data: &[u8]) -> Option<DataFormat> {
    if data.len() < 4 {
        return None;
    }

    // PNG: 8-byte signature
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(DataFormat::PNG);
    }
    // TIFF little-endian ("II\x2A\x00") or big-endian ("MM\x00\x2A")
    if data.starts_with(&[0x49, 0x49, 0x2A, 0x00])
        || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
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
fn detect_image_dimensions(data: &[u8], format: &DataFormat) -> Option<Dimensions> {
    match format {
        DataFormat::PNG => detect_png_dimensions(data),
        DataFormat::TIFF | DataFormat::MICROSCOPY => detect_tiff_dimensions(data),
        _ => None,
    }
}

/// Parse width and height from a PNG IHDR chunk.
///
/// Layout: `[8-byte magic][4-byte chunk_len][4-byte "IHDR"][4-byte width BE][4-byte height BE]...`
fn detect_png_dimensions(data: &[u8]) -> Option<Dimensions> {
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
fn detect_tiff_dimensions(data: &[u8]) -> Option<Dimensions> {
    if data.len() < 8 {
        return None;
    }
    let little_endian = match &data[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };

    // Closures for endian-aware reads.
    let u16_at = |off: usize| -> Option<u32> {
        data.get(off..off + 2).map(|b| {
            if little_endian {
                u16::from_le_bytes([b[0], b[1]]) as u32
            } else {
                u16::from_be_bytes([b[0], b[1]]) as u32
            }
        })
    };
    let u32_at = |off: usize| -> Option<u32> {
        data.get(off..off + 4).map(|b| {
            if little_endian {
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            } else {
                u32::from_be_bytes([b[0], b[1], b[2], b[3]])
            }
        })
    };

    // Magic check (42).
    if u16_at(2)? != 42 {
        return None;
    }

    let ifd_offset = u32_at(4)? as usize;
    let num_entries = u16_at(ifd_offset)? as usize;

    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;

    for i in 0..num_entries {
        let entry = ifd_offset + 2 + i * 12;
        if entry + 12 > data.len() {
            break;
        }
        let tag = u16_at(entry)?;
        let field_type = u16_at(entry + 2)?;

        // Read the value field (4 bytes at entry+8); for SHORT it's in the first 2 bytes.
        let value = match field_type {
            1 | 6 => data[entry + 8] as u32,          // BYTE / SBYTE
            3 | 8 => u16_at(entry + 8)?,               // SHORT / SSHORT
            4 | 9 => u32_at(entry + 8)?,               // LONG / SLONG
            _ => continue,
        };

        match tag {
            256 => width = Some(value),   // ImageWidth
            257 => height = Some(value),  // ImageLength
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
fn detect_wav_sample_rate(data: &[u8], format: &DataFormat) -> Option<f64> {
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
// Helpers
// ---------------------------------------------------------------------------

fn parse_stealth_level(s: &str) -> StealthLevel {
    match s.to_lowercase().as_str() {
        "maximum" => StealthLevel::Maximum,
        "high" => StealthLevel::High,
        "medium" => StealthLevel::Medium,
        "low" => StealthLevel::Low,
        _ => StealthLevel::High,
    }
}

fn parse_ec_level(s: &str) -> ECLevel {
    match s.to_lowercase().as_str() {
        "none" => ECLevel::None,
        "low" => ECLevel::Low,
        "medium" => ECLevel::Medium,
        "high" => ECLevel::High,
        "maximum" => ECLevel::Maximum,
        _ => ECLevel::None,
    }
}

/// Insert the strategy string into `params.format_specific` under the key
/// appropriate for the given `format`.  Unknown formats are silently ignored
/// (the handlers will use their own defaults).
fn apply_strategy(params: &mut EmbedParams, format: &DataFormat, strategy: &str) {
    let key = match format {
        DataFormat::VCF => "vcf_strategy",
        DataFormat::SAM | DataFormat::BAM => "sam_strategy",
        DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY | DataFormat::DICOM => {
            "imagery_strategy"
        }
        DataFormat::WAV => "signal_strategy",
        DataFormat::FASTA | DataFormat::FASTQ => "genomics_strategy",
        DataFormat::FITS => "fits_strategy",
        _ => return,
    };
    params
        .format_specific
        .insert(key.to_string(), serde_json::Value::String(strategy.to_string()));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- format detection from extension ----

    #[test]
    fn test_format_from_extension_png() {
        assert_eq!(
            detect_format_from_extension("photo.png"),
            Some(DataFormat::PNG)
        );
    }

    #[test]
    fn test_format_from_extension_tiff() {
        assert_eq!(
            detect_format_from_extension("scan.tiff"),
            Some(DataFormat::TIFF)
        );
        assert_eq!(
            detect_format_from_extension("scan.tif"),
            Some(DataFormat::TIFF)
        );
    }

    #[test]
    fn test_format_from_extension_dicom() {
        assert_eq!(
            detect_format_from_extension("mri.dcm"),
            Some(DataFormat::DICOM)
        );
        assert_eq!(
            detect_format_from_extension("mri.dicom"),
            Some(DataFormat::DICOM)
        );
    }

    #[test]
    fn test_format_from_extension_wav() {
        assert_eq!(
            detect_format_from_extension("audio.wav"),
            Some(DataFormat::WAV)
        );
    }

    #[test]
    fn test_format_from_extension_genomics() {
        assert_eq!(
            detect_format_from_extension("reads.fasta"),
            Some(DataFormat::FASTA)
        );
        assert_eq!(
            detect_format_from_extension("reads.fa"),
            Some(DataFormat::FASTA)
        );
        assert_eq!(
            detect_format_from_extension("reads.fastq"),
            Some(DataFormat::FASTQ)
        );
        assert_eq!(
            detect_format_from_extension("reads.fq"),
            Some(DataFormat::FASTQ)
        );
        assert_eq!(
            detect_format_from_extension("variants.vcf"),
            Some(DataFormat::VCF)
        );
        assert_eq!(
            detect_format_from_extension("align.sam"),
            Some(DataFormat::SAM)
        );
        assert_eq!(
            detect_format_from_extension("align.bam"),
            Some(DataFormat::BAM)
        );
    }

    #[test]
    fn test_format_from_extension_unknown() {
        assert_eq!(detect_format_from_extension("file.xyz"), None);
        assert_eq!(detect_format_from_extension("noextension"), None);
    }

    // ---- format detection from magic bytes ----

    #[test]
    fn test_magic_png() {
        let data: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic
            0, 0, 0, 13, // chunk length
            0x49, 0x48, 0x44, 0x52, // "IHDR"
            0, 0, 0, 10, // width = 10
            0, 0, 0, 20, // height = 20
        ];
        assert_eq!(detect_format_from_magic(&data), Some(DataFormat::PNG));
    }

    #[test]
    fn test_magic_tiff_le() {
        let mut data = vec![0u8; 8];
        data[0..2].copy_from_slice(b"II");
        data[2] = 0x2A;
        data[3] = 0x00;
        data[4..8].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(detect_format_from_magic(&data), Some(DataFormat::TIFF));
    }

    #[test]
    fn test_magic_tiff_be() {
        let mut data = vec![0u8; 8];
        data[0..2].copy_from_slice(b"MM");
        data[2] = 0x00;
        data[3] = 0x2A;
        assert_eq!(detect_format_from_magic(&data), Some(DataFormat::TIFF));
    }

    #[test]
    fn test_magic_wav() {
        let mut data = vec![0u8; 12];
        data[0..4].copy_from_slice(b"RIFF");
        data[8..12].copy_from_slice(b"WAVE");
        assert_eq!(detect_format_from_magic(&data), Some(DataFormat::WAV));
    }

    #[test]
    fn test_magic_fasta() {
        let data = b">chr1 sequence\nACGT";
        assert_eq!(detect_format_from_magic(data), Some(DataFormat::FASTA));
    }

    #[test]
    fn test_magic_fastq() {
        let data = b"@read1\nACGT\n+\nIIII";
        assert_eq!(detect_format_from_magic(data), Some(DataFormat::FASTQ));
    }

    #[test]
    fn test_magic_sam() {
        let data = b"@HD\tVN:1.6\nread1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tIIII";
        assert_eq!(detect_format_from_magic(data), Some(DataFormat::SAM));
    }

    #[test]
    fn test_magic_vcf() {
        let data = b"##fileformat=VCFv4.2\n#CHROM\t...";
        assert_eq!(detect_format_from_magic(data), Some(DataFormat::VCF));
    }

    #[test]
    fn test_magic_unknown() {
        assert_eq!(detect_format_from_magic(b"hello world"), None);
        assert_eq!(detect_format_from_magic(b"\x00\x01\x02"), None);
    }

    // ---- PNG dimension detection ----

    fn make_png_header(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // PNG magic
        data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR chunk length (13 bytes)
        data.extend_from_slice(&13u32.to_be_bytes());
        // "IHDR"
        data.extend_from_slice(b"IHDR");
        // width and height (big-endian)
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        // bit depth, color type, ... (5 bytes filler)
        data.extend_from_slice(&[8, 2, 0, 0, 0]);
        data
    }

    #[test]
    fn test_png_dimensions_basic() {
        let data = make_png_header(640, 480);
        let dims = detect_png_dimensions(&data).unwrap();
        assert_eq!(dims.width, 640);
        assert_eq!(dims.height, 480);
    }

    #[test]
    fn test_png_dimensions_too_short() {
        let data = vec![0u8; 10];
        assert!(detect_png_dimensions(&data).is_none());
    }

    #[test]
    fn test_png_dimensions_wrong_magic() {
        let mut data = make_png_header(100, 100);
        data[0] = 0xFF; // corrupt magic
        assert!(detect_png_dimensions(&data).is_none());
    }

    #[test]
    fn test_png_dimensions_zero_rejected() {
        let data = make_png_header(0, 100);
        assert!(detect_png_dimensions(&data).is_none());
    }

    // ---- TIFF dimension detection ----

    fn make_tiff_le(width: u32, height: u32) -> Vec<u8> {
        // Minimal little-endian TIFF with one IFD containing ImageWidth and ImageLength
        let mut data = Vec::new();
        // Header: byte order, magic, IFD offset
        let ifd_offset: u32 = 8;
        data.extend_from_slice(b"II");
        data.extend_from_slice(&42u16.to_le_bytes());
        data.extend_from_slice(&ifd_offset.to_le_bytes());
        // IFD: 2 entries
        data.extend_from_slice(&2u16.to_le_bytes());
        // Entry 1: tag=256 (ImageWidth), type=4 (LONG), count=1, value=width
        data.extend_from_slice(&256u16.to_le_bytes());
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&width.to_le_bytes());
        // Entry 2: tag=257 (ImageLength), type=4 (LONG), count=1, value=height
        data.extend_from_slice(&257u16.to_le_bytes());
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data
    }

    #[test]
    fn test_tiff_dimensions_le() {
        let data = make_tiff_le(1920, 1080);
        let dims = detect_tiff_dimensions(&data).unwrap();
        assert_eq!(dims.width, 1920);
        assert_eq!(dims.height, 1080);
    }

    #[test]
    fn test_tiff_dimensions_too_short() {
        assert!(detect_tiff_dimensions(&[0u8; 4]).is_none());
    }

    #[test]
    fn test_tiff_dimensions_wrong_magic() {
        let mut data = make_tiff_le(100, 100);
        data[2] = 0xFF; // corrupt magic value (should be 42)
        assert!(detect_tiff_dimensions(&data).is_none());
    }

    // ---- WAV sample rate detection ----

    fn make_wav_header(sample_rate: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&100u32.to_le_bytes()); // file size
        data.extend_from_slice(b"WAVE");
        data.extend_from_slice(b"fmt ");
        data.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        data.extend_from_slice(&1u16.to_le_bytes());  // PCM audio format
        data.extend_from_slice(&2u16.to_le_bytes());  // 2 channels
        data.extend_from_slice(&sample_rate.to_le_bytes());
        data
    }

    #[test]
    fn test_wav_sample_rate_44100() {
        let data = make_wav_header(44100);
        let rate = detect_wav_sample_rate(&data, &DataFormat::WAV).unwrap();
        assert_eq!(rate, 44100.0);
    }

    #[test]
    fn test_wav_sample_rate_48000() {
        let data = make_wav_header(48000);
        let rate = detect_wav_sample_rate(&data, &DataFormat::WAV).unwrap();
        assert_eq!(rate, 48000.0);
    }

    #[test]
    fn test_wav_sample_rate_ignored_for_non_wav() {
        let data = make_wav_header(44100);
        assert!(detect_wav_sample_rate(&data, &DataFormat::PNG).is_none());
    }

    #[test]
    fn test_wav_sample_rate_too_short() {
        let data = vec![0u8; 20];
        assert!(detect_wav_sample_rate(&data, &DataFormat::WAV).is_none());
    }

    // ---- stealth level parsing ----

    #[test]
    fn test_parse_stealth_level() {
        assert!(matches!(parse_stealth_level("maximum"), StealthLevel::Maximum));
        assert!(matches!(parse_stealth_level("Maximum"), StealthLevel::Maximum));
        assert!(matches!(parse_stealth_level("high"), StealthLevel::High));
        assert!(matches!(parse_stealth_level("medium"), StealthLevel::Medium));
        assert!(matches!(parse_stealth_level("low"), StealthLevel::Low));
        // default
        assert!(matches!(parse_stealth_level("unknown"), StealthLevel::High));
    }

    // ---- EC level parsing ----

    #[test]
    fn test_parse_ec_level_all_variants() {
        assert!(matches!(parse_ec_level("none"), ECLevel::None));
        assert!(matches!(parse_ec_level("low"), ECLevel::Low));
        assert!(matches!(parse_ec_level("medium"), ECLevel::Medium));
        assert!(matches!(parse_ec_level("high"), ECLevel::High));
        assert!(matches!(parse_ec_level("maximum"), ECLevel::Maximum));
    }

    #[test]
    fn test_parse_ec_level_case_insensitive() {
        assert!(matches!(parse_ec_level("NONE"), ECLevel::None));
        assert!(matches!(parse_ec_level("Medium"), ECLevel::Medium));
        assert!(matches!(parse_ec_level("HIGH"), ECLevel::High));
    }

    #[test]
    fn test_parse_ec_level_default() {
        // Unknown strings fall back to ECLevel::None
        assert!(matches!(parse_ec_level(""), ECLevel::None));
        assert!(matches!(parse_ec_level("unknown"), ECLevel::None));
        assert!(matches!(parse_ec_level("42"), ECLevel::None));
    }

    // ---- apply_strategy ----

    fn make_params() -> EmbedParams {
        EmbedParams::new(vec![])
    }

    #[test]
    fn test_apply_strategy_vcf() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::VCF, "multi");
        assert_eq!(
            p.format_specific.get("vcf_strategy").and_then(|v| v.as_str()),
            Some("multi")
        );
    }

    #[test]
    fn test_apply_strategy_sam() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::SAM, "optional");
        assert_eq!(
            p.format_specific.get("sam_strategy").and_then(|v| v.as_str()),
            Some("optional")
        );
    }

    #[test]
    fn test_apply_strategy_bam() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::BAM, "comment");
        assert_eq!(
            p.format_specific.get("sam_strategy").and_then(|v| v.as_str()),
            Some("comment")
        );
    }

    #[test]
    fn test_apply_strategy_png() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::PNG, "adaptive");
        assert_eq!(
            p.format_specific.get("imagery_strategy").and_then(|v| v.as_str()),
            Some("adaptive")
        );
    }

    #[test]
    fn test_apply_strategy_tiff() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::TIFF, "lsb");
        assert_eq!(
            p.format_specific.get("imagery_strategy").and_then(|v| v.as_str()),
            Some("lsb")
        );
    }

    #[test]
    fn test_apply_strategy_wav() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::WAV, "frequency");
        assert_eq!(
            p.format_specific.get("signal_strategy").and_then(|v| v.as_str()),
            Some("frequency")
        );
    }

    #[test]
    fn test_apply_strategy_fasta() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::FASTA, "codon");
        assert_eq!(
            p.format_specific.get("genomics_strategy").and_then(|v| v.as_str()),
            Some("codon")
        );
    }

    #[test]
    fn test_apply_strategy_fastq() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::FASTQ, "quality");
        assert_eq!(
            p.format_specific.get("genomics_strategy").and_then(|v| v.as_str()),
            Some("quality")
        );
    }

    #[test]
    fn test_apply_strategy_unmapped_format_noop() {
        // Formats without a mapped strategy key (e.g. BINARY, TEXT) are silently ignored
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::BINARY, "anything");
        assert!(p.format_specific.is_empty());
    }

    #[test]
    fn test_apply_strategy_overwrites_previous() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::PNG, "lsb");
        apply_strategy(&mut p, &DataFormat::PNG, "adaptive");
        assert_eq!(
            p.format_specific.get("imagery_strategy").and_then(|v| v.as_str()),
            Some("adaptive")
        );
    }
}
