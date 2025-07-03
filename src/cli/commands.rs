use crate::core::*;
use crate::handlers::HandlerRegistry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub async fn handle_embed(
    input: &str,
    payload: &str,
    output: &str,
    password: Option<&str>,
) -> Result<()> {
    println!("🔐 Embedding {} into {} -> {}", payload, input, output);
    
    // Read input files
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read carrier file: {}", e)))?;
    let payload_data = fs::read(payload)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read payload file: {}", e)))?;
    
    // Create carrier data structure
    let mut carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };
    
    // Setup parameters
    let params = if let Some(pass) = password {
        EmbedParams {
            encryption_key: pass.as_bytes().to_vec(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    } else {
        EmbedParams {
            encryption_key: b"default_key".to_vec(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    };
    
    // Get appropriate handler
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format)
        .ok_or_else(|| SteganoError::InvalidParameter(format!("Unsupported format: {:?}", carrier.format)))?;
    
    // Check capacity
    let capacity = handler.estimate_capacity(&carrier)?;
    println!("📊 Available capacity: {} bytes", capacity.recommended_bits / 8);
    
    if payload_data.len() > capacity.recommended_bits / 8 {
        return Err(SteganoError::InsufficientCapacity {
            needed: payload_data.len(),
            available: capacity.recommended_bits / 8,
        });
    }
    
    // Embed data
    let result = handler.embed(&mut carrier, &payload_data, &params).await?;
    
    // Write output
    fs::write(output, &carrier.raw_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {}", e)))?;
    
    println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
    println!("📈 Performance: {:.2}ms", result.performance_stats.total_time_ms());
    
    Ok(())
}

pub async fn handle_extract(
    input: &str,
    output: &str,
    password: Option<&str>,
) -> Result<()> {
    println!("🔍 Extracting from {} -> {}", input, output);
    
    // Read input file
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {}", e)))?;
    
    // Create carrier data structure
    let carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };
    
    // Setup parameters
    let params = if let Some(pass) = password {
        EmbedParams {
            encryption_key: pass.as_bytes().to_vec(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    } else {
        EmbedParams {
            encryption_key: b"default_key".to_vec(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    };
    
    // Get appropriate handler
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format)
        .ok_or_else(|| SteganoError::InvalidParameter(format!("Unsupported format: {:?}", carrier.format)))?;
    
    // Extract data
    let extracted_data = handler.extract(&carrier, &params).await?;
    
    // Write output
    fs::write(output, &extracted_data)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to write output file: {}", e)))?;
    
    println!("✅ Successfully extracted {} bytes", extracted_data.len());
    
    Ok(())
}

pub async fn handle_analyze(input: &str) -> Result<()> {
    println!("📊 Analyzing {}", input);
    
    // Read input file
    let carrier_data = fs::read(input)
        .map_err(|e| SteganoError::InvalidParameter(format!("Failed to read input file: {}", e)))?;
    
    // Create carrier data structure
    let carrier = CarrierData {
        format: detect_format_from_path(input),
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };
    
    // Get appropriate handler
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format)
        .ok_or_else(|| SteganoError::InvalidParameter(format!("Unsupported format: {:?}", carrier.format)))?;
    
    // Analyze capacity
    let capacity = handler.estimate_capacity(&carrier)?;
    
    println!("📈 Capacity Analysis for {:?}:", carrier.format);
    println!("  Total capacity: {} bits ({} bytes)", capacity.total_bits, capacity.total_bits / 8);
    println!("  Recommended capacity: {} bits ({} bytes)", capacity.recommended_bits, capacity.recommended_bits / 8);
    
    // Check if file contains hidden data
    let has_hidden_data = handler.verify_integrity(&carrier)?;
    println!("  Contains hidden data: {}", if has_hidden_data { "Yes" } else { "No" });
    
    // Print format-specific info
    if !capacity.format_specific_info.is_empty() {
        println!("  Format-specific info:");
        for (key, value) in &capacity.format_specific_info {
            println!("    {}: {}", key, value);
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