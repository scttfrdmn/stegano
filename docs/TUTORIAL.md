# Stegano Tutorial - Comprehensive Guide

This tutorial provides a complete walkthrough of using the Stegano steganography system, from basic concepts to advanced usage patterns.

## Table of Contents

1. [Introduction](#introduction)
2. [Installation and Setup](#installation-and-setup)
3. [Basic Concepts](#basic-concepts)
4. [Getting Started](#getting-started)
5. [Format-Specific Tutorials](#format-specific-tutorials)
6. [Advanced Features](#advanced-features)
7. [Performance Optimization](#performance-optimization)
8. [Security Best Practices](#security-best-practices)
9. [Troubleshooting](#troubleshooting)

## Introduction

Stegano is a high-performance steganography library designed specifically for scientific data formats. It provides:

- **Multi-format support**: FASTA/FASTQ genomics, TIFF/PNG imagery, WAV/signal data
- **Advanced security**: AES-256-GCM encryption with Blake3 key derivation
- **GPU acceleration**: WGPU compute shaders with Metal backend optimization
- **Biological integrity**: FASTQ quality score embedding with validation
- **Production monitoring**: Health checks, metrics, and telemetry

## Installation and Setup

### Prerequisites

- Rust 1.70 or later
- WGPU-compatible GPU (for acceleration)
- macOS with Metal support (recommended for best performance)

### Installation

```bash
# Clone the repository
git clone https://github.com/scttfrdmn/stegano.git
cd stegano

# Build the library
cargo build --release

# Run tests to verify installation
cargo test

# Build with GPU acceleration
cargo build --release --features gpu
```

### Library Integration

Add to your `Cargo.toml`:

```toml
[dependencies]
stegano = "0.1.2"
```

## Basic Concepts

### Data Formats

Stegano organizes supported formats by scientific domain:

- **Genomics**: FASTA (sequences), FASTQ (quality scores), VCF (variants)
- **Imagery**: TIFF (scientific), PNG (standard), DICOM (medical)
- **Signals**: WAV (audio), CSV (time series), Binary (raw)

### Embedding Parameters

Three key parameters control embedding:

1. **Error Correction Level**: None (0%) to Maximum (100% overhead)
2. **Stealth Level**: Maximum stealth (10% capacity) to Low (100% capacity)
3. **GPU Acceleration**: Enabled/disabled for performance

### Security Model

- **Layer 1**: AES-256-GCM or ChaCha20-Poly1305 encryption
- **Layer 2**: Blake3 cryptographic integrity verification
- **Layer 3**: Reed-Solomon error correction with configurable redundancy

## Getting Started

### Example 1: Basic Binary Embedding

```rust
use stegano::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Load carrier data
    let carrier_data = std::fs::read("document.pdf")?;
    let mut carrier = CarrierData::new(DataFormat::BINARY, carrier_data);
    
    // Configure embedding
    let params = EmbedParams::new(b"secure_password".to_vec())
        .with_stealth_level(StealthLevel::High);
    
    // Get handler and check capacity
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    let capacity = handler.estimate_capacity(&carrier)?;
    
    println!("Available capacity: {} bytes", capacity.recommended_bytes());
    
    // Embed secret data
    let payload = b"Confidential research findings";
    let result = handler.embed(&mut carrier, payload, &params).await?;
    
    println!("Successfully embedded {} bytes", result.bytes_embedded);
    
    // Save modified carrier
    std::fs::write("document_with_secret.pdf", &carrier.raw_data)?;
    
    // Extract and verify
    let extracted = handler.extract(&carrier, &params).await?;
    assert_eq!(extracted, payload);
    
    Ok(())
}
```

### Example 2: CLI Usage

```bash
# Embed data into an image
./target/release/stegano embed \
    --input photo.png \
    --payload secret.txt \
    --output hidden.png \
    --password "secure_key" \
    --stealth high

# Extract hidden data
./target/release/stegano extract \
    --input hidden.png \
    --output recovered.txt \
    --password "secure_key"

# Analyze capacity
./target/release/stegano analyze --input photo.png
```

## Format-Specific Tutorials

### FASTQ Quality Score Steganography

FASTQ format provides the most advanced steganographic capabilities through quality score manipulation:

```rust
use stegano::handlers::genomics::{GenomicsHandler, QualityEmbedStrategy};
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    let handler = GenomicsHandler::new();
    
    // Load FASTQ data
    let fastq_data = std::fs::read("sample.fastq")?;
    let mut carrier = CarrierData::new(DataFormat::FASTQ, fastq_data);
    
    // Test different embedding strategies
    let strategies = vec![
        QualityEmbedStrategy::Conservative, // 1 bit per quality char
        QualityEmbedStrategy::Balanced,     // 2 bits per quality char  
        QualityEmbedStrategy::Aggressive,   // 3+ bits per quality char
    ];
    
    for strategy in strategies {
        let capacity = handler.estimate_fastq_quality_capacity(
            &carrier.raw_data, 
            strategy
        )?;
        
        println!("{:?} strategy: {} bytes capacity", 
                 strategy, capacity.recommended_bytes());
    }
    
    // Embed using balanced strategy
    let payload = b"Genomics research data - Sample ID: GRD-2025-001";
    let result = handler.embed_fastq_qualities(
        &mut carrier.raw_data,
        payload,
        QualityEmbedStrategy::Balanced
    )?;
    
    println!("Embedded {} bytes, biological integrity: {}", 
             result.bytes_embedded, 
             result.biological_integrity_preserved);
    
    // Verify the FASTQ file is still valid
    assert!(handler.verify_fastq_biological_integrity(&carrier.raw_data)?);
    
    // Extract payload
    let extracted = handler.extract_fastq_qualities(
        &carrier.raw_data, 
        QualityEmbedStrategy::Balanced
    )?;
    
    assert_eq!(extracted, payload);
    
    Ok(())
}
```

### Image Steganography with Metadata

```rust
use stegano::*;

#[tokio::main] 
async fn main() -> Result<()> {
    // Load TIFF scientific image
    let image_data = std::fs::read("microscopy.tiff")?;
    let mut carrier = CarrierData::new(DataFormat::TIFF, image_data);
    
    // Add image metadata
    carrier.dimensions = Some(Dimensions::new_2d(1024, 768));
    carrier.add_metadata("instrument".to_string(), 
                        serde_json::Value::String("Zeiss LSM 880".to_string()));
    
    // Configure for scientific imagery
    let params = EmbedParams::new(b"lab_key_2025".to_vec())
        .with_error_correction(ECLevel::High)  // Important for scientific data
        .with_stealth_level(StealthLevel::Medium);
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    // Check capacity considering image properties
    let capacity = handler.estimate_capacity(&carrier)?;
    let utilization = capacity.utilization_percent(1024); // 1KB payload
    
    if utilization > 80.0 {
        println!("Warning: High utilization ({}%), consider reducing payload", utilization);
    }
    
    // Embed experimental data
    let payload = b"Experiment ID: EXP-2025-042\nResults: Cell division rate increased 23% under treatment";
    let result = handler.embed(&mut carrier, payload, &params).await?;
    
    // Verify image integrity
    assert!(handler.verify_integrity(&carrier)?);
    
    Ok(())
}
```

### Audio Signal Steganography

```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Load WAV audio file
    let audio_data = std::fs::read("recording.wav")?;
    let mut carrier = CarrierData::new(DataFormat::WAV, audio_data);
    
    // Set sampling rate for signal processing
    carrier.sampling_rate = Some(44100.0);
    
    // Configure for audio with lower stealth (audio is robust)
    let params = EmbedParams::new(b"audio_secret".to_vec())
        .with_stealth_level(StealthLevel::Low)  // More capacity for audio
        .with_gpu();  // GPU acceleration for signal processing
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    let payload = b"This audio contains hidden research notes";
    let result = handler.embed(&mut carrier, payload, &params).await?;
    
    println!("Embedded in audio with GPU acceleration: {} ms", 
             result.performance_stats.embedding_time_ms);
    
    Ok(())
}
```

## Advanced Features

### GPU Acceleration

Enable GPU acceleration for improved performance:

```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable GPU acceleration in parameters
    let params = EmbedParams::new(b"gpu_key".to_vec())
        .with_gpu()
        .with_stealth_level(StealthLevel::Low); // Allows GPU optimization
    
    // Large file processing benefits most from GPU
    let large_data = vec![0u8; 10 * 1024 * 1024]; // 10MB
    let mut carrier = CarrierData::new(DataFormat::BINARY, large_data);
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    let payload = vec![42u8; 1024 * 1024]; // 1MB payload
    let result = handler.embed(&mut carrier, &payload, &params).await?;
    
    if result.performance_stats.gpu_acceleration_used {
        println!("GPU acceleration enabled - processed in {} ms", 
                 result.performance_stats.embedding_time_ms);
    }
    
    Ok(())
}
```

### Error Correction Strategies

Choose appropriate error correction for your use case:

```rust
use stegano::*;

fn choose_error_correction(environment: &str) -> ECLevel {
    match environment {
        "lab_storage" => ECLevel::Low,        // Controlled environment
        "field_research" => ECLevel::Medium,  // Some environmental factors
        "transmission" => ECLevel::High,      // Network/transmission errors
        "long_term_archive" => ECLevel::Maximum, // Maximum protection
        _ => ECLevel::Medium,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let ec_level = choose_error_correction("long_term_archive");
    
    let params = EmbedParams::new(b"archive_key".to_vec())
        .with_error_correction(ec_level);
    
    println!("Using {} error correction with {}% overhead", 
             match ec_level {
                 ECLevel::None => "no",
                 ECLevel::Low => "low",
                 ECLevel::Medium => "medium", 
                 ECLevel::High => "high",
                 ECLevel::Maximum => "maximum",
             },
             (ec_level.overhead_percent() * 100.0) as u32);
    
    // ... rest of embedding logic
    Ok(())
}
```

### Format-Specific Parameters

Customize embedding for specific formats:

```rust
use stegano::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // FASTQ-specific parameters
    let mut fastq_params = HashMap::new();
    fastq_params.insert("strategy".to_string(), 
                       serde_json::Value::String("conservative".to_string()));
    fastq_params.insert("preserve_quality_range".to_string(),
                       serde_json::Value::Bool(true));
    
    let params = EmbedParams {
        encryption_key: b"genomics_key".to_vec(),
        error_correction_level: ECLevel::High,
        stealth_level: StealthLevel::High,
        use_gpu: false,
        format_specific: fastq_params,
    };
    
    // Image-specific parameters  
    let mut image_params = HashMap::new();
    image_params.insert("channel_preference".to_string(),
                       serde_json::Value::String("blue".to_string()));
    image_params.insert("preserve_histograms".to_string(),
                       serde_json::Value::Bool(true));
    
    // Use appropriate parameters for each format
    Ok(())
}
```

## Performance Optimization

### Benchmarking

```rust
use stegano::*;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let sizes = vec![1024, 10240, 102400, 1048576]; // 1KB to 1MB
    
    for size in sizes {
        let data = vec![0u8; size];
        let mut carrier = CarrierData::new(DataFormat::BINARY, data);
        
        let params = EmbedParams::new(b"benchmark_key".to_vec());
        let payload = vec![42u8; size / 100]; // 1% payload
        
        let registry = handlers::HandlerRegistry::new();
        let handler = registry.get_handler(&carrier.format).unwrap();
        
        let start = Instant::now();
        let result = handler.embed(&mut carrier, &payload, &params).await?;
        let duration = start.elapsed();
        
        let throughput = result.performance_stats.throughput_bps(size);
        
        println!("Size: {}KB, Time: {}ms, Throughput: {:.1} MB/s",
                 size / 1024,
                 duration.as_millis(),
                 throughput / 1_000_000.0);
    }
    
    Ok(())
}
```

### Memory Management

```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // For large files, consider streaming or chunked processing
    let chunk_size = 1024 * 1024; // 1MB chunks
    
    // Monitor memory usage
    let params = EmbedParams::new(b"memory_test".to_vec());
    
    // Process in chunks for large files
    let large_file = std::fs::read("large_dataset.bin")?;
    
    if large_file.len() > chunk_size {
        println!("Large file detected, consider chunked processing");
        // Implement chunked processing logic here
    }
    
    Ok(())
}
```

## Security Best Practices

### Key Management

```rust
use stegano::crypto::CryptoLayer;
use stegano::*;

fn derive_key_from_password(password: &str, salt: &[u8]) -> Vec<u8> {
    // Use proper key derivation (PBKDF2, Argon2, etc.)
    // This is a simplified example
    let mut key = blake3::hash(password.as_bytes()).as_bytes().to_vec();
    key.extend_from_slice(salt);
    blake3::hash(&key).as_bytes()[..32].to_vec()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Use strong password-based key derivation
    let password = "SecurePassword123!@#";
    let salt = b"unique_salt_per_user";
    let key = derive_key_from_password(password, salt);
    
    let params = EmbedParams::new(key)
        .with_error_correction(ECLevel::High);
    
    // ... embedding logic
    Ok(())
}
```

### Secure Payload Handling

```rust
use stegano::crypto::CryptoLayer;
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Use the crypto layer for sensitive data
    let crypto = CryptoLayer::new_from_password("master_password")?;
    
    let sensitive_data = b"Highly confidential research data";
    
    // Encrypt before embedding
    let encrypted_payload = crypto.encrypt(sensitive_data)?;
    let serialized_payload = CryptoLayer::serialize_payload(&encrypted_payload);
    
    // Embed encrypted payload
    let mut carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 10000]);
    let params = EmbedParams::new(vec![]); // No additional encryption needed
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    let result = handler.embed(&mut carrier, &serialized_payload, &params).await?;
    
    // Extract and decrypt
    let extracted = handler.extract(&carrier, &params).await?;
    let encrypted_payload = CryptoLayer::deserialize_payload(&extracted)?;
    let decrypted_data = crypto.decrypt(&encrypted_payload)?;
    
    assert_eq!(decrypted_data, sensitive_data);
    
    Ok(())
}
```

### Integrity Verification

```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 1000]);
    let params = EmbedParams::new(b"integrity_key".to_vec())
        .with_error_correction(ECLevel::High);
    
    let payload = b"Important data requiring integrity verification";
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    // Embed with integrity protection
    let result = handler.embed(&mut carrier, payload, &params).await?;
    
    // Always verify integrity after embedding
    assert!(handler.verify_integrity(&carrier)?, "Integrity check failed");
    
    // Simulate data corruption
    carrier.raw_data[500] ^= 0xFF;
    
    // Extraction may still succeed due to error correction
    match handler.extract(&carrier, &params).await {
        Ok(extracted) => {
            if extracted == payload {
                println!("Error correction successfully recovered corrupted data");
            } else {
                println!("Data corruption detected and partially corrected");
            }
        },
        Err(e) => println!("Corruption too severe to recover: {}", e),
    }
    
    Ok(())
}
```

## Troubleshooting

### Common Issues

#### Insufficient Capacity
```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    let small_carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 100]);
    let large_payload = vec![42u8; 1000];
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&small_carrier.format).unwrap();
    
    // Check capacity before embedding
    let capacity = handler.estimate_capacity(&small_carrier)?;
    
    if !capacity.can_fit(large_payload.len()) {
        println!("Payload too large: {} bytes needed, {} bytes available",
                 large_payload.len(), capacity.recommended_bytes());
        
        // Solutions:
        // 1. Use a larger carrier
        // 2. Compress the payload
        // 3. Use lower stealth level
        // 4. Split across multiple carriers
        
        return Ok(());
    }
    
    Ok(())
}
```

#### Format Detection Issues
```rust
use stegano::*;

fn detect_format_robust(data: &[u8]) -> Option<DataFormat> {
    let registry = handlers::HandlerRegistry::new();
    
    // Try each handler
    for handler in registry.get_handlers() {
        if let Some(format) = handler.detect_format(data) {
            return Some(format);
        }
    }
    
    // Fallback to file extension or magic bytes
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    let unknown_data = std::fs::read("unknown_file")?;
    
    match detect_format_robust(&unknown_data) {
        Some(format) => println!("Detected format: {:?}", format),
        None => {
            println!("Could not detect format, using BINARY");
            // Use BINARY format as fallback
        }
    }
    
    Ok(())
}
```

#### Performance Issues
```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable GPU acceleration for large files
    let params = EmbedParams::new(b"performance_key".to_vec())
        .with_gpu()
        .with_stealth_level(StealthLevel::Low); // Allows more aggressive optimization
    
    // Use lower error correction for performance-critical applications
    let fast_params = EmbedParams::new(b"fast_key".to_vec())
        .with_error_correction(ECLevel::Low)
        .with_gpu();
    
    // Monitor performance
    let mut carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 1_000_000]);
    let payload = vec![42u8; 10_000];
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    let result = handler.embed(&mut carrier, &payload, &fast_params).await?;
    
    let stats = &result.performance_stats;
    println!("Performance: {}ms embedding, {} bytes/sec throughput",
             stats.embedding_time_ms,
             stats.throughput_bps(carrier.size()) as u64);
    
    if stats.gpu_acceleration_used {
        println!("GPU acceleration was utilized");
    }
    
    Ok(())
}
```

### Debug Mode

```bash
# Enable debug logging
RUST_LOG=debug cargo run --example basic_demo

# Enable trace logging for detailed debugging  
RUST_LOG=trace cargo run --example genomics_demo

# Log to file
RUST_LOG=debug cargo run --example basic_demo 2>debug.log
```

## Conclusion

This tutorial covered the essential concepts and practical usage of the Stegano steganography system. Key takeaways:

1. **Choose appropriate formats**: Use specialized formats (FASTQ, TIFF) for maximum capacity and stealth
2. **Balance security and performance**: Higher error correction and stealth reduce capacity but improve security
3. **Leverage GPU acceleration**: Significant performance gains for large files
4. **Always verify integrity**: Use built-in integrity checking for reliable embedding
5. **Follow security best practices**: Proper key derivation and encrypted payloads

For production use, refer to the API documentation and performance benchmarking guides for detailed optimization strategies.