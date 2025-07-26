use crate::core::*;
use crate::embedding::SteganoEmbedder;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub async fn handle_embed(
    input: &str,
    payload: &str,
    output: &str,
    password: Option<&str>,
    validate: bool,
    gpu: bool,
    stealth: &str,
) -> Result<()> {
    println!("🔐 Embedding {payload} into {input} -> {output}");

    // Read input files
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read carrier file: {e}")))?;
    let payload_data = fs::read(payload)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read payload file: {e}")))?;

    // Create carrier data structure
    let mut carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    // Parse stealth level
    let stealth_level = match stealth.to_lowercase().as_str() {
        "maximum" => StealthLevel::Maximum,
        "high" => StealthLevel::High,
        "medium" => StealthLevel::Medium,
        "low" => StealthLevel::Low,
        _ => StealthLevel::High, // default
    };

    // Setup parameters
    let key = password
        .map(|p| p.as_bytes().to_vec())
        .unwrap_or_else(|| b"default_key".to_vec());
    let mut params = EmbedParams::new(key).with_stealth_level(stealth_level);

    if gpu {
        params = params.with_gpu();
    }
    if validate {
        params = params.with_validation();
    }

    // Create embedder and check capacity
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

    // Embed data with integrated validation
    let result = embedder.embed(&mut carrier, &payload_data, &params).await?;

    // Write output
    fs::write(output, &carrier.raw_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {e}")))?;

    println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
    println!(
        "📈 Performance: {:.2}ms",
        result.performance_stats.total_time_ms()
    );

    // Display validation results if enabled
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
) -> Result<()> {
    println!("🔍 Extracting from {input} -> {output}");

    // Read input file
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {e}")))?;

    // Create carrier data structure
    let carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    // Setup parameters
    let key = password
        .map(|p| p.as_bytes().to_vec())
        .unwrap_or_else(|| b"default_key".to_vec());
    let mut params = EmbedParams::new(key);

    if gpu {
        params = params.with_gpu();
    }

    // Extract data using embedder
    let embedder = SteganoEmbedder::new();
    let extracted_data = embedder.extract(&carrier, &params).await?;

    // Write output
    fs::write(output, &extracted_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {e}")))?;

    println!("✅ Successfully extracted {} bytes", extracted_data.len());

    Ok(())
}

pub async fn handle_analyze(input: &str, validate: bool) -> Result<()> {
    println!("📊 Analyzing {input}");

    // Read input file
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {e}")))?;

    // Create carrier data structure
    let carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    // Create embedder and analyze capacity
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

    // Check if file contains hidden data
    let has_hidden_data = embedder.verify_integrity(&carrier)?;
    println!(
        "  Contains hidden data: {}",
        if has_hidden_data { "Yes" } else { "No" }
    );

    // Run statistical validation if requested
    if validate {
        println!("\n🔬 Running Statistical Validation Analysis...");

        // For validation analysis, we need original and modified data
        // For analysis mode, we'll run basic statistical tests on the current data
        use crate::validation::StatisticalValidationSuite;
        let validation_suite = StatisticalValidationSuite::new();

        // Convert bytes to frequencies for analysis
        let mut frequencies = vec![0.0; 256];
        for &byte in &carrier.raw_data {
            frequencies[byte as usize] += 1.0;
        }

        // Run basic statistical tests
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

        // Runs test for randomness (convert to bool sequence by checking if above median)
        let median = 127.5; // median for byte values 0-255
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

    // Print format-specific info
    if !capacity.format_specific_info.is_empty() {
        println!("\n📋 Format-specific info:");
        for (key, value) in &capacity.format_specific_info {
            println!("    {key}: {value}");
        }
    }

    Ok(())
}

fn detect_format_from_path(path: &str) -> DataFormat {
    let path = Path::new(path);
    match path.extension().and_then(|s| s.to_str()) {
        Some("fasta") | Some("fa") => DataFormat::FASTA,
        Some("fastq") | Some("fq") => DataFormat::FASTQ,
        Some("vcf") => DataFormat::VCF,
        Some("sam") => DataFormat::SAM,
        Some("bam") => DataFormat::BAM,
        Some("png") => DataFormat::PNG,
        Some("tiff") | Some("tif") => DataFormat::TIFF,
        Some("wav") => DataFormat::WAV,
        Some("csv") => DataFormat::CsvTimeseries,
        Some("txt") => DataFormat::TEXT,
        _ => DataFormat::BINARY,
    }
}
