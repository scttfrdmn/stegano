use crate::carrier::{self, build_carrier};
use crate::cli::files;
use crate::cli::{PasswordArgs, PASSWORD_ENV};
use crate::core::*;
use crate::embedding::SteganoEmbedder;
use std::fs;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub async fn handle_embed(
    input: &str,
    payload: &str,
    output: &str,
    password: &PasswordArgs,
    validate: bool,
    gpu: bool,
    stealth: &str,
    ec_level: &str,
    strategy: Option<&str>,
    cipher: &str,
    no_stream: bool,
) -> Result<()> {
    println!("🔐 Embedding {payload} into {input} -> {output}");

    let stealth_level = parse_stealth_level(stealth)?;
    let ec = parse_ec_level(ec_level)?;
    let cipher_kind = carrier::parse_cipher(cipher).ok_or_else(|| {
        SteganoError::InvalidParameter(format!(
            "unknown cipher '{cipher}' (expected aes256gcm or chacha20)"
        ))
    })?;
    let key = resolve_password(password, true)?.unwrap_or_default();

    let payload_data = read_file(payload, "payload")?;

    // Warn when ECLevel::Low is chosen with a small payload (RS block is always 255 bytes).
    if matches!(ec, ECLevel::Low) && payload_data.len() < 200 {
        println!("⚠️  ECLevel::Low uses a 255-byte RS block regardless of payload size.");
        println!("   Consider --ec-level medium for payloads smaller than ~200 bytes.");
    }

    let mut params = carrier::default_params(key)
        .with_stealth_level(stealth_level)
        .with_ec_level(ec);
    params.cipher_kind = cipher_kind;

    if gpu {
        params = params.with_gpu();
    }

    // Streaming path: decided from the extension / magic bytes only, so a
    // large carrier is never read into memory.
    if let Some(format) = streaming_format(input, no_stream, validate, "embed")? {
        let mut params = params.clone();
        if let Some(s) = strategy {
            apply_strategy(&mut params, &format, s);
        }
        if files::embed_streamable(&format, &params) {
            println!("🌊 Streaming {format:?} carrier (constant memory; --no-stream to disable)");
            let result = crate::stream::embed_file(format, input, output, &payload_data, &params)
                .map_err(|e| with_path_context(e, input, output))?;
            println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
            println!(
                "📈 Performance: {:.2}ms",
                result.performance_stats.total_time_ms()
            );
            return Ok(());
        }
    }
    println!("📦 In-memory embedding");

    if validate {
        params = params.with_validation();
    }

    let carrier_data = read_file(input, "carrier")?;
    let mut carrier = build_carrier(input, carrier_data);

    if let Some(s) = strategy {
        apply_strategy(&mut params, &carrier.format, s);
    }

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    // `None`: the handler reports the precise error itself (e.g. float FITS
    // data without the `float_mantissa` opt-in).
    if let Some(available) = usable_capacity_bits(&carrier.format, &capacity, &params) {
        println!(
            "📊 Available capacity: {available} bits ({} bytes)",
            available / 8
        );

        // Both values in bits, matching the `InsufficientCapacity` message.
        let needed_bits = payload_data.len().saturating_mul(8);
        if needed_bits > available {
            return Err(SteganoError::InsufficientCapacity {
                needed: needed_bits,
                available,
            });
        }
    }

    let result = embedder.embed(&mut carrier, &payload_data, &params).await?;

    write_file(output, &carrier.raw_data)?;

    println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
    println!(
        "📈 Performance: {:.2}ms",
        result.performance_stats.total_time_ms()
    );

    if let Some(validation_report) = result.validation_report {
        println!("\n🔬 Statistical Validation Results:");
        match validation_report.overall_score() {
            Some(score) => println!("   Detection-resistance score: {score}/10 (heuristic)"),
            None => println!("   Detection-resistance score: n/a (no LSB detector ran)"),
        }
        println!(
            "   Tests Completed: {}",
            validation_report.test_results().len()
        );

        for test_name in validation_report.test_results().keys() {
            println!("   ✓ {test_name}");
        }

        match validation_report.overall_score() {
            Some(s) if s >= 8 => {
                println!("   LSB detectors see little or no evidence of embedding.")
            }
            Some(s) if s >= 5 => println!("   ⚠️  Some LSB detectors flag the output."),
            Some(_) => println!("   ❌ LSB detectors flag the output - embedding is detectable."),
            None => {}
        }
        println!("   (Score covers two LSB-replacement detectors only; not a security guarantee.)");
    }

    Ok(())
}

pub async fn handle_extract(
    input: &str,
    output: &str,
    password: &PasswordArgs,
    gpu: bool,
    ec_level: &str,
    strategy: Option<&str>,
    no_stream: bool,
) -> Result<()> {
    println!("🔍 Extracting from {input} -> {output}");

    let ec = parse_ec_level(ec_level)?;
    let key = resolve_password(password, false)?.unwrap_or_default();

    let mut params = carrier::default_params(key).with_ec_level(ec);

    if gpu {
        params = params.with_gpu();
    }

    if let Some(format) = streaming_format(input, no_stream, false, "extract")? {
        // Stream extraction auto-detects the channel, so --strategy is not
        // needed (and not consulted) here.
        if let Some(r) = files::try_extract_stream(&format, Path::new(input), &params) {
            println!("🌊 Streaming {format:?} carrier (stops at the end of the payload; --no-stream to disable)");
            let extracted_data = r.map_err(|e| with_path_context(e, input, output))?;
            write_file(output, &extracted_data)?;
            println!("✅ Successfully extracted {} bytes", extracted_data.len());
            return Ok(());
        }
    }
    println!("📦 In-memory extraction");

    let carrier_data = read_file(input, "input")?;
    let carrier = build_carrier(input, carrier_data);

    if let Some(s) = strategy {
        apply_strategy(&mut params, &carrier.format, s);
    }

    let embedder = SteganoEmbedder::new();
    let extracted_data = embedder.extract(&carrier, &params).await?;

    write_file(output, &extracted_data)?;

    println!("✅ Successfully extracted {} bytes", extracted_data.len());

    Ok(())
}

pub async fn handle_analyze(input: &str, validate: bool) -> Result<()> {
    handle_analyze_with(input, validate, None, None).await
}

/// [`handle_analyze`] with the FASTQ quality-alphabet options:
/// `quality_bins` declares the alphabet (see
/// [`QualityAlphabet::parse`](crate::validation::fastq_quality::QualityAlphabet::parse))
/// and `quality_reference` names an unmodified FASTQ to compare the quality
/// histogram with.  Both are rejected for non-FASTQ input.
pub async fn handle_analyze_with(
    input: &str,
    validate: bool,
    quality_bins: Option<&str>,
    quality_reference: Option<&str>,
) -> Result<()> {
    println!("📊 Analyzing {input}");

    let carrier_data = read_file(input, "input")?;

    let carrier = build_carrier(input, carrier_data);

    let is_fastq = carrier.format == DataFormat::FASTQ;
    if !is_fastq && (quality_bins.is_some() || quality_reference.is_some()) {
        return Err(SteganoError::InvalidParameter(format!(
            "--quality-bins and --quality-reference apply to FASTQ input only (got {:?})",
            carrier.format
        )));
    }
    let quality_options = if is_fastq {
        Some(fastq_quality_options(quality_bins, quality_reference)?)
    } else {
        None
    };

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

    if let Some(options) = quality_options {
        print_fastq_quality_report(&carrier.raw_data, &options)?;
    }

    if !capacity.format_specific_info.is_empty() {
        println!("\n📋 Format-specific info:");
        for (key, value) in &capacity.format_specific_info {
            println!("    {key}: {value}");
        }
    }

    Ok(())
}

fn fastq_quality_options(
    quality_bins: Option<&str>,
    quality_reference: Option<&str>,
) -> Result<crate::validation::fastq_quality::FastqQualityOptions> {
    use crate::validation::fastq_quality::*;
    let alphabet = quality_bins.map(QualityAlphabet::parse).transpose()?;
    let reference = match quality_reference {
        Some(path) => Some(QualityHistogram::from_fastq(&read_file(
            path,
            "reference",
        )?)?),
        None => None,
    };
    Ok(FastqQualityOptions {
        alphabet,
        reference,
        ..Default::default()
    })
}

/// Print the FASTQ quality-alphabet section of `analyze`.
fn print_fastq_quality_report(
    data: &[u8],
    options: &crate::validation::fastq_quality::FastqQualityOptions,
) -> Result<()> {
    use crate::validation::fastq_quality::*;
    let r = analyze_fastq_qualities(data, options)?;
    let h = &r.histogram;

    println!("\n🧬 FASTQ quality alphabet:");
    println!(
        "  Records: {}, quality characters: {}, distinct values: {}",
        r.records,
        h.total(),
        h.distinct()
    );
    if h.invalid() > 0 {
        println!("  Characters outside Phred+33: {}", h.invalid());
    }
    let hist: Vec<String> = (0..PHRED_VALUES)
        .filter(|&q| h.counts()[q] > 0)
        .map(|q| format!("Q{q}={}", h.counts()[q]))
        .collect();
    println!("  Histogram: {}", hist.join(" "));

    match (&r.alphabet, r.alphabet_source) {
        (Some(alphabet), Some(source)) => {
            let bins: Vec<String> = alphabet.bins().iter().map(|q| format!("Q{q}")).collect();
            let how = match source {
                AlphabetSource::Declared => "declared".to_string(),
                AlphabetSource::Inferred => format!(
                    "inferred, {:.2}% of characters on bins",
                    r.best_scheme.1 * 100.0
                ),
            };
            println!(
                "  Alphabet: {} ({}) [{how}]",
                alphabet.name(),
                bins.join(",")
            );
            println!(
                "  Off-bin values: {} ({:.4}%)",
                r.off_bin_count,
                r.off_bin_fraction * 100.0
            );
            if r.off_bin_count > 0 {
                let first: Vec<String> = r
                    .off_bin_samples
                    .iter()
                    .map(|s| format!("record {} col {} Q{}", s.record + 1, s.column + 1, s.phred))
                    .collect();
                println!("  First off-bin: {}", first.join("; "));
                println!(
                    "  ⚠️  Quality values were changed after binning (stegano's FASTQ quality \
                     strategy does this)"
                );
            }
        }
        _ => println!(
            "  Alphabet: no known binning scheme matched (best: {}, {:.2}% of characters on \
             bins); pass --quality-bins to check against a known alphabet",
            r.best_scheme.0.name(),
            r.best_scheme.1 * 100.0
        ),
    }

    if let Some(d) = r.divergence {
        println!(
            "  Reference divergence: chi-square {:.2} (df {}), p-value {:.6}, KL {:.6} nats",
            d.chi_square, d.degrees_of_freedom, d.p_value, d.kl_divergence
        );
    }
    Ok(())
}

/// Report whether `input` contains hidden data.
///
/// Returns `Ok(true)` when a payload header is found and `Ok(false)` when
/// not; the binary maps these to exit codes 0 and 1, and errors to 3
/// (see [`crate::cli::exit_code`]).
pub async fn handle_detect(input: &str, validate: bool) -> Result<bool> {
    println!("🔍 Detecting hidden data in {input}");

    let carrier_data = read_file(input, "input")?;

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
        let bool_sequence: Vec<bool> = carrier
            .raw_data
            .iter()
            .map(|&b| b as f64 > median)
            .collect();
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

    Ok(has_data)
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

/// Read a file, reporting failures as `SteganoError::IoError` with the path.
fn read_file(path: &str, what: &str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| {
        SteganoError::IoError(std::io::Error::new(
            e.kind(),
            format!("failed to read {what} file '{path}': {e}"),
        ))
    })
}

/// Write a file atomically (special files such as `/dev/stdout` are written
/// directly), reporting failures as `SteganoError::IoError` with the path.
fn write_file(path: &str, data: &[u8]) -> Result<()> {
    let special = fs::metadata(path).is_ok_and(|m| !m.is_file());
    let result = if special {
        fs::write(path, data).map_err(SteganoError::from)
    } else {
        files::write_atomic(Path::new(path), data)
    };
    result.map_err(|e| match e {
        SteganoError::IoError(e) => SteganoError::IoError(std::io::Error::new(
            e.kind(),
            format!("failed to write output file '{path}': {e}"),
        )),
        other => other,
    })
}

/// The format to try the streaming path with, or `None` for in-memory.
///
/// Detection uses only the extension and the magic bytes of the file head
/// ([`files::detect_format_cheap`]); the caller still checks that the
/// format/strategy pair is streamable.
fn streaming_format(
    input: &str,
    no_stream: bool,
    validate: bool,
    what: &str,
) -> Result<Option<DataFormat>> {
    if no_stream {
        println!("📦 --no-stream: using the in-memory path");
        return Ok(None);
    }
    if validate {
        // Validation compares the whole original and embedded carriers.
        println!("📦 --validate needs the whole carrier: using the in-memory path");
        return Ok(None);
    }
    files::detect_format_cheap(Path::new(input)).map_err(|e| match e {
        SteganoError::IoError(e) => SteganoError::IoError(std::io::Error::new(
            e.kind(),
            format!("failed to read {what} input file '{input}': {e}"),
        )),
        other => other,
    })
}

/// Add the file names to an I/O error from the stream API.
fn with_path_context(e: SteganoError, input: &str, output: &str) -> SteganoError {
    match e {
        SteganoError::IoError(e) => SteganoError::IoError(std::io::Error::new(
            e.kind(),
            format!("I/O error streaming '{input}' -> '{output}': {e}"),
        )),
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Password handling
// ---------------------------------------------------------------------------

/// Resolve the password from (in order) `--password-file`, `--password`
/// (with a warning), `STEGANO_PASSWORD`, or an interactive prompt when
/// `--password-prompt` is given. `confirm` asks twice (used for embed).
///
/// Returns `None` when no password source was given (no encryption).
pub fn resolve_password(args: &PasswordArgs, confirm: bool) -> Result<Option<Vec<u8>>> {
    resolve_password_with_env(args, confirm, std::env::var(PASSWORD_ENV).ok())
}

fn resolve_password_with_env(
    args: &PasswordArgs,
    confirm: bool,
    env_value: Option<String>,
) -> Result<Option<Vec<u8>>> {
    if let Some(path) = &args.password_file {
        let text = fs::read_to_string(path).map_err(|e| {
            SteganoError::IoError(std::io::Error::new(
                e.kind(),
                format!("failed to read password file '{path}': {e}"),
            ))
        })?;
        let line = text.lines().next().unwrap_or("").trim_end_matches('\r');
        if line.is_empty() {
            return Err(SteganoError::InvalidParameter(format!(
                "password file '{path}' is empty"
            )));
        }
        return Ok(Some(line.as_bytes().to_vec()));
    }
    if let Some(p) = &args.password {
        eprintln!(
            "⚠️  --password is visible to other users (ps) and saved in shell history; \
             prefer --password-file, {PASSWORD_ENV} or --password-prompt."
        );
        return Ok(Some(p.as_bytes().to_vec()));
    }
    if let Some(p) = env_value.filter(|p| !p.is_empty()) {
        return Ok(Some(p.into_bytes()));
    }
    if args.password_prompt {
        return prompt_password(confirm).map(Some);
    }
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
fn prompt_password(confirm: bool) -> Result<Vec<u8>> {
    let first = rpassword::prompt_password("Password: ")?;
    if first.is_empty() {
        return Err(SteganoError::InvalidParameter("empty password".to_string()));
    }
    if confirm {
        let second = rpassword::prompt_password("Confirm password: ")?;
        if first != second {
            return Err(SteganoError::InvalidParameter(
                "passwords do not match".to_string(),
            ));
        }
    }
    Ok(first.into_bytes())
}

#[cfg(target_arch = "wasm32")]
fn prompt_password(_confirm: bool) -> Result<Vec<u8>> {
    Err(SteganoError::InvalidParameter(
        "interactive password prompt is not available on this platform".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Parameter helpers
// ---------------------------------------------------------------------------

fn parse_stealth_level(s: &str) -> Result<StealthLevel> {
    carrier::parse_stealth_level(s).ok_or_else(|| {
        SteganoError::InvalidParameter(format!(
            "unknown stealth level '{s}' (expected low, medium, high or maximum)"
        ))
    })
}

fn parse_ec_level(s: &str) -> Result<ECLevel> {
    carrier::parse_ec_level(s).ok_or_else(|| {
        SteganoError::InvalidParameter(format!(
            "unknown error-correction level '{s}' (expected none, low, medium, high or maximum)"
        ))
    })
}

/// Capacity (in bits) available to the strategy selected in `params`.
///
/// `estimate_capacity` reports the default strategy only. For FITS, the
/// opt-in `float_mantissa` capacity is in `format_specific_info`. Returns
/// `None` when the CLI should not pre-check and leave the error to the
/// handler: a float-only FITS file without the opt-in, or `float_mantissa`
/// on a file with no float image.
fn usable_capacity_bits(
    format: &DataFormat,
    capacity: &CapacityInfo,
    params: &EmbedParams,
) -> Option<usize> {
    if *format != DataFormat::FITS {
        return Some(capacity.recommended_bits);
    }
    let info = &capacity.format_specific_info;
    let float_bits = info
        .get("float_mantissa_capacity_bits")
        .and_then(serde_json::Value::as_u64);
    let float_opt_in = params
        .format_specific
        .get("fits_strategy")
        .and_then(serde_json::Value::as_str)
        == Some("float_mantissa");
    if float_opt_in {
        let overhead_bits = info
            .get("overhead_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .saturating_mul(8);
        return float_bits.map(|b| b.saturating_sub(overhead_bits) as usize);
    }
    if capacity.total_bits == 0 && float_bits.is_some() {
        return None;
    }
    Some(capacity.recommended_bits)
}

/// Insert the strategy string into `params.format_specific` under the key
/// the handler for `format` reads (see [`carrier::strategy_key`]). Formats
/// without a selectable strategy are ignored with a warning.
fn apply_strategy(params: &mut EmbedParams, format: &DataFormat, strategy: &str) {
    let Some(key) = carrier::strategy_key(format) else {
        eprintln!("⚠️  --strategy is ignored for {format:?} carriers");
        return;
    };
    params.format_specific.insert(
        key.to_string(),
        serde_json::Value::String(strategy.to_string()),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- stealth / EC level parsing ----

    #[test]
    fn test_parse_stealth_level() {
        assert!(matches!(
            parse_stealth_level("maximum"),
            Ok(StealthLevel::Maximum)
        ));
        assert!(matches!(
            parse_stealth_level("Maximum"),
            Ok(StealthLevel::Maximum)
        ));
        assert!(matches!(
            parse_stealth_level("high"),
            Ok(StealthLevel::High)
        ));
        assert!(matches!(
            parse_stealth_level("medium"),
            Ok(StealthLevel::Medium)
        ));
        assert!(matches!(parse_stealth_level("low"), Ok(StealthLevel::Low)));
        assert!(parse_stealth_level("unknown").is_err());
    }

    #[test]
    fn test_parse_ec_level_all_variants() {
        assert!(matches!(parse_ec_level("none"), Ok(ECLevel::None)));
        assert!(matches!(parse_ec_level("low"), Ok(ECLevel::Low)));
        assert!(matches!(parse_ec_level("medium"), Ok(ECLevel::Medium)));
        assert!(matches!(parse_ec_level("high"), Ok(ECLevel::High)));
        assert!(matches!(parse_ec_level("maximum"), Ok(ECLevel::Maximum)));
        assert!(matches!(parse_ec_level("NONE"), Ok(ECLevel::None)));
        assert!(matches!(parse_ec_level("Medium"), Ok(ECLevel::Medium)));
    }

    #[test]
    fn test_parse_ec_level_unknown_is_error() {
        assert!(parse_ec_level("").is_err());
        assert!(parse_ec_level("unknown").is_err());
        assert!(parse_ec_level("42").is_err());
    }

    // ---- password resolution ----

    #[test]
    fn test_password_none_by_default() {
        let args = PasswordArgs::default();
        assert_eq!(resolve_password_with_env(&args, false, None).unwrap(), None);
    }

    #[test]
    fn test_password_from_env() {
        let args = PasswordArgs::default();
        assert_eq!(
            resolve_password_with_env(&args, false, Some("envpw".into())).unwrap(),
            Some(b"envpw".to_vec())
        );
        // An empty variable counts as unset.
        assert_eq!(
            resolve_password_with_env(&args, false, Some(String::new())).unwrap(),
            None
        );
    }

    #[test]
    fn test_password_flag_beats_env() {
        let args = PasswordArgs {
            password: Some("flagpw".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_password_with_env(&args, false, Some("envpw".into())).unwrap(),
            Some(b"flagpw".to_vec())
        );
    }

    #[test]
    fn test_password_file_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pw.txt");
        fs::write(&path, "filepw\r\nsecond line\n").unwrap();
        let args = PasswordArgs {
            password_file: Some(path.to_str().unwrap().to_string()),
            ..Default::default()
        };
        assert_eq!(
            resolve_password_with_env(&args, false, Some("envpw".into())).unwrap(),
            Some(b"filepw".to_vec())
        );
    }

    #[test]
    fn test_password_file_missing_is_io_error() {
        let args = PasswordArgs {
            password_file: Some("/nonexistent/stegano/pw".into()),
            ..Default::default()
        };
        assert!(matches!(
            resolve_password_with_env(&args, false, None),
            Err(SteganoError::IoError(_))
        ));
    }

    #[test]
    fn test_read_file_missing_is_io_error() {
        assert!(matches!(
            read_file("/nonexistent/stegano/carrier.bin", "carrier"),
            Err(SteganoError::IoError(_))
        ));
    }

    // ---- apply_strategy ----

    fn make_params() -> EmbedParams {
        carrier::default_params(vec![])
    }

    fn strategy_of(p: &EmbedParams, key: &str) -> Option<String> {
        p.format_specific
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    #[test]
    fn test_apply_strategy_keys_per_format() {
        let cases = [
            (DataFormat::VCF, "format", "vcf_strategy"),
            (DataFormat::SAM, "optional", "sam_strategy"),
            (DataFormat::BAM, "comment", "sam_strategy"),
            (DataFormat::PNG, "adaptive", "imagery_strategy"),
            (DataFormat::TIFF, "lsb", "imagery_strategy"),
            (DataFormat::DICOM, "pixel_lsb", "dicom_strategy"),
            (DataFormat::WAV, "frequency", "signal_strategy"),
            (DataFormat::FASTA, "codon", "genomics_strategy"),
            (DataFormat::FASTQ, "quality", "genomics_strategy"),
            (DataFormat::FITS, "float_mantissa", "fits_strategy"),
        ];
        for (format, strategy, key) in cases {
            let mut p = make_params();
            apply_strategy(&mut p, &format, strategy);
            assert_eq!(
                strategy_of(&p, key).as_deref(),
                Some(strategy),
                "{format:?}"
            );
            assert_eq!(p.format_specific.len(), 1);
        }
    }

    #[test]
    fn test_apply_strategy_dicom_not_imagery() {
        let mut p = make_params();
        apply_strategy(&mut p, &DataFormat::DICOM, "metadata");
        assert!(!p.format_specific.contains_key("imagery_strategy"));
    }

    #[test]
    fn test_apply_strategy_unmapped_format_noop() {
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
            strategy_of(&p, "imagery_strategy").as_deref(),
            Some("adaptive")
        );
    }

    // ---- usable_capacity_bits ----

    #[test]
    fn test_usable_capacity_bits_fits_float() {
        let mut cap = CapacityInfo::new(0, 0);
        cap.format_specific_info
            .insert("float_mantissa_capacity_bits".into(), 1000.into());
        cap.format_specific_info
            .insert("overhead_bytes".into(), 10.into());
        let mut p = make_params();
        // No opt-in on a float-only file: leave the error to the handler.
        assert_eq!(usable_capacity_bits(&DataFormat::FITS, &cap, &p), None);
        apply_strategy(&mut p, &DataFormat::FITS, "float_mantissa");
        assert_eq!(usable_capacity_bits(&DataFormat::FITS, &cap, &p), Some(920));
        // Other formats use the reported capacity.
        assert_eq!(usable_capacity_bits(&DataFormat::PNG, &cap, &p), Some(0));
        let int_cap = CapacityInfo::new(800, 720);
        assert_eq!(
            usable_capacity_bits(&DataFormat::FITS, &int_cap, &make_params()),
            Some(720)
        );
    }
}
