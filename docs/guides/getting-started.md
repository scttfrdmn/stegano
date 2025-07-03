# Getting Started with Stegano

## Overview

This guide will help you get up and running with the stegano scientific steganography library. You'll learn how to perform basic embedding and extraction operations, understand the core concepts, and explore the library's capabilities.

## Installation

### Adding to Your Project

Add stegano to your `Cargo.toml`:

```toml
[dependencies]
stegano = "0.2.0"
tokio = { version = "1.0", features = ["full"] }  # Required for async support
```

### Feature Flags

Enable optional features based on your needs:

```toml
[dependencies]
stegano = { version = "0.2.0", features = ["gpu", "metal", "enterprise"] }
```

Available features:
- `gpu` - GPU acceleration via WGPU (enabled by default)
- `metal` - Metal acceleration on macOS (enabled by default on macOS)
- `enterprise` - Enterprise features (REST API, monitoring)
- `python` - Python bindings
- `wasm` - WebAssembly support

## Your First Steganographic Operation

Let's start with a simple example that embeds a secret message into an image:

```rust
use stegano::{
    core::{CarrierData, DataFormat, EmbedParams},
    handlers::HandlerRegistry,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Load your carrier data (image, audio, genomics file, etc.)
    let image_data = std::fs::read("examples/data/sample.png")?;
    
    // Step 2: Create a CarrierData structure
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: image_data,
        metadata: HashMap::new(),
        dimensions: Some((1920, 1080)), // Optional: specify dimensions
        sampling_rate: None,
    };
    
    // Step 3: Prepare your secret payload
    let secret_message = b"This is my secret message!";
    
    // Step 4: Configure embedding parameters
    let params = EmbedParams::new(b"my_encryption_key".to_vec());
    
    // Step 5: Get the appropriate handler for your data format
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format)
        .expect("PNG format should be supported");
    
    // Step 6: Embed the secret message
    let embed_result = handler.embed(&mut carrier, secret_message, &params).await?;
    
    println!("✅ Successfully embedded {} bytes!", embed_result.bytes_embedded);
    println!("⏱️  Embedding took {}ms", embed_result.performance_stats.embedding_time_ms);
    
    // Step 7: Save the modified carrier (now contains hidden data)
    std::fs::write("output_with_secret.png", &carrier.raw_data)?;
    
    // Step 8: Extract the secret message (verification)
    let extracted_message = handler.extract(&carrier, &params).await?;
    
    println!("🔓 Extracted message: {}", String::from_utf8_lossy(&extracted_message));
    
    Ok(())
}
```

## Core Concepts

### 1. Carrier Data

**Carrier data** is the host file or data that will contain your hidden information. Stegano supports multiple formats:

- **Images**: PNG, TIFF, DICOM, Microscopy
- **Audio**: WAV, binary signals, CSV timeseries
- **Genomics**: FASTA, FASTQ, VCF, SAM, BAM
- **Generic**: Binary data, text files

```rust
// Creating carrier data for different formats
use stegano::core::{CarrierData, DataFormat};

// Image carrier
let image_carrier = CarrierData {
    format: DataFormat::PNG,
    raw_data: image_bytes,
    dimensions: Some((width, height)),
    ..Default::default()
};

// Audio carrier
let audio_carrier = CarrierData {
    format: DataFormat::WAV,
    raw_data: audio_bytes,
    sampling_rate: Some(44100.0),
    ..Default::default()
};

// Genomics carrier
let fasta_carrier = CarrierData {
    format: DataFormat::FASTA,
    raw_data: fasta_bytes,
    ..Default::default()
};
```

### 2. Embedding Parameters

Control how your data is hidden using `EmbedParams`:

```rust
use stegano::core::{EmbedParams, ECLevel, StealthLevel};

let mut params = EmbedParams::new(b"encryption_key".to_vec());

// Configure error correction
params.error_correction_level = ECLevel::High;  // Protect against data corruption

// Configure stealth level
params.stealth_level = StealthLevel::Maximum;   // Maximize imperceptibility

// Enable GPU acceleration
params.use_gpu = true;

// Add format-specific parameters
params.format_specific.insert(
    "embedding_strategy".to_string(),
    serde_json::json!("adaptive_lsb")
);
```

### 3. Handler System

Stegano uses a handler system where each data format has specialized embedding techniques:

```rust
use stegano::handlers::HandlerRegistry;

let registry = HandlerRegistry::new();

// Get handler by format
let png_handler = registry.get_handler(&DataFormat::PNG);

// Or let the system auto-detect
let handler = registry.get_handler_by_data(&file_bytes);
```

## Working with Different Data Types

### Image Steganography

```rust
use stegano::core::*;

#[tokio::main]
async fn image_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: std::fs::read("photo.png")?,
        dimensions: Some((1024, 768)),
        ..Default::default()
    };
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG).unwrap();
    
    // Configure for high-quality images
    let mut params = EmbedParams::new(b"photo_key".to_vec());
    params.stealth_level = StealthLevel::High;  // Minimize visual artifacts
    
    let secret_data = b"Hidden in plain sight";
    let result = handler.embed(&mut carrier, secret_data, &params).await?;
    
    std::fs::write("photo_with_secret.png", &carrier.raw_data)?;
    Ok(())
}
```

### Audio Steganography

```rust
#[tokio::main]
async fn audio_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut carrier = CarrierData {
        format: DataFormat::WAV,
        raw_data: std::fs::read("music.wav")?,
        sampling_rate: Some(44100.0),
        ..Default::default()
    };
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::WAV).unwrap();
    
    // Configure for audio quality preservation
    let mut params = EmbedParams::new(b"audio_key".to_vec());
    params.stealth_level = StealthLevel::Medium;
    
    // Add audio-specific parameters
    params.format_specific.insert(
        "frequency_range".to_string(),
        serde_json::json!({"min": 2000, "max": 8000})
    );
    
    let message = b"Secret audio message";
    handler.embed(&mut carrier, message, &params).await?;
    
    std::fs::write("music_with_secret.wav", &carrier.raw_data)?;
    Ok(())
}
```

### Genomics Steganography

```rust
#[tokio::main]
async fn genomics_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut carrier = CarrierData {
        format: DataFormat::FASTA,
        raw_data: std::fs::read("sequences.fasta")?,
        ..Default::default()
    };
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::FASTA).unwrap();
    
    // Genomics data requires careful handling
    let mut params = EmbedParams::new(b"bio_key".to_vec());
    params.error_correction_level = ECLevel::High;  // Protect scientific data
    
    let research_data = b"Experimental results: positive correlation";
    handler.embed(&mut carrier, research_data, &params).await?;
    
    std::fs::write("sequences_with_data.fasta", &carrier.raw_data)?;
    Ok(())
}
```

## Capacity Estimation

Before embedding, check if your payload will fit:

```rust
use stegano::handlers::HandlerRegistry;

async fn check_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: std::fs::read("large_image.png")?,
        dimensions: Some((4096, 3072)),
        ..Default::default()
    };
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG).unwrap();
    
    // Estimate capacity
    let capacity = handler.estimate_capacity(&carrier)?;
    
    println!("📊 Capacity Analysis:");
    println!("   Total bits available: {}", capacity.total_bits);
    println!("   Recommended capacity: {} bytes", capacity.recommended_bytes());
    println!("   Steganographic ratio: {:.2}%", capacity.steganographic_ratio * 100.0);
    
    // Check if your payload fits
    let payload_size = 1024; // 1KB
    if capacity.can_fit(payload_size) {
        println!("✅ Payload will fit comfortably");
    } else {
        println!("❌ Payload too large - consider compression");
    }
    
    Ok(())
}
```

## GPU Acceleration

Enable high-performance processing with GPU acceleration:

```rust
use stegano::gpu::GpuAccelerator;

#[tokio::main]
async fn gpu_example() -> Result<(), Box<dyn std::error::Error>> {
    // Check GPU availability
    if !GpuAccelerator::is_available() {
        println!("⚠️  GPU acceleration not available, using CPU");
    }
    
    // Create GPU accelerator
    let gpu = GpuAccelerator::new().await?;
    println!("🚀 Using GPU: {}", gpu.device_info());
    
    // Configure parameters for GPU acceleration
    let mut params = EmbedParams::new(b"gpu_key".to_vec());
    params.use_gpu = true;
    
    // Large data that benefits from GPU acceleration
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: std::fs::read("4k_image.png")?,
        dimensions: Some((3840, 2160)),
        ..Default::default()
    };
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG).unwrap();
    
    let large_payload = vec![0u8; 100_000]; // 100KB payload
    let result = handler.embed(&mut carrier, &large_payload, &params).await?;
    
    if result.performance_stats.gpu_acceleration_used {
        println!("🎯 GPU acceleration was used!");
        println!("⚡ Completed in {}ms", result.performance_stats.embedding_time_ms);
    }
    
    Ok(())
}
```

## Error Handling

Handle different types of errors gracefully:

```rust
use stegano::core::SteganoError;

async fn robust_embedding() -> Result<(), Box<dyn std::error::Error>> {
    let mut carrier = load_carrier_data()?;
    let payload = b"Secret message";
    let params = EmbedParams::new(b"key".to_vec());
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    match handler.embed(&mut carrier, payload, &params).await {
        Ok(result) => {
            println!("✅ Embedding successful: {} bytes", result.bytes_embedded);
        }
        Err(SteganoError::InsufficientCapacity { needed, available }) => {
            println!("❌ Not enough space: need {} bytes, have {} bytes", needed, available);
            println!("💡 Try: reducing payload size or using compression");
        }
        Err(SteganoError::UnsupportedFormat(format)) => {
            println!("❌ Format {:?} is not supported", format);
            println!("💡 Supported formats: PNG, WAV, FASTA, FASTQ, etc.");
        }
        Err(SteganoError::GpuError(msg)) => {
            println!("⚠️  GPU error: {}", msg);
            println!("💡 Retrying with CPU...");
            
            // Retry without GPU
            let mut cpu_params = params;
            cpu_params.use_gpu = false;
            handler.embed(&mut carrier, payload, &cpu_params).await?;
        }
        Err(e) => {
            println!("❌ Embedding failed: {}", e);
        }
    }
    
    Ok(())
}
```

## Performance Optimization Tips

### 1. Choose the Right Stealth Level
```rust
// Maximum capacity, minimal stealth
params.stealth_level = StealthLevel::Low;

// Balanced approach (recommended)
params.stealth_level = StealthLevel::Medium;

// Maximum stealth, reduced capacity
params.stealth_level = StealthLevel::Maximum;
```

### 2. Enable GPU for Large Data
```rust
// Enable GPU for files > 1MB
params.use_gpu = carrier.raw_data.len() > 1_000_000;
```

### 3. Use Appropriate Error Correction
```rust
// For critical data
params.error_correction_level = ECLevel::High;

// For performance-critical applications
params.error_correction_level = ECLevel::Low;
```

### 4. Reuse Objects
```rust
let registry = HandlerRegistry::new(); // Create once
let params = EmbedParams::new(key);    // Reuse for multiple operations

for file in files {
    let handler = registry.get_handler(&file.format).unwrap();
    handler.embed(&mut file.data, &payload, &params).await?;
}
```

## Command Line Usage

Stegano also provides a command-line interface:

```bash
# Embed a file
stegano embed --carrier image.png --payload secret.txt --output image_with_secret.png --key mykey

# Extract a file
stegano extract --carrier image_with_secret.png --output extracted_secret.txt --key mykey

# Check capacity
stegano capacity --carrier large_image.png

# List supported formats
stegano formats
```

## Next Steps

Now that you understand the basics, explore these advanced topics:

1. **[Performance Optimization](performance-optimization.md)** - GPU acceleration and optimization strategies
2. **[Security Best Practices](security-best-practices.md)** - Cryptographic security and threat mitigation
3. **[Format-Specific Guides](formats/)** - Deep dive into genomics, imagery, and signal processing
4. **[Advanced Examples](../examples/advanced/)** - ML-driven optimization and anti-detection techniques

## Common Troubleshooting

### "Insufficient Capacity" Error
- **Solution**: Use a larger carrier file or compress your payload
- **Alternative**: Use `StealthLevel::Low` for maximum capacity

### GPU Acceleration Not Working
- **Check**: Ensure GPU drivers are installed and up to date
- **Fallback**: The library automatically falls back to CPU processing

### Format Not Detected
- **Solution**: Manually specify the format when creating `CarrierData`
- **Check**: Ensure file headers are intact and not corrupted

### Poor Performance
- **Enable GPU**: Set `params.use_gpu = true` for large files
- **Optimize**: Use appropriate stealth and error correction levels
- **Profile**: Check `performance_stats` for bottlenecks

## Support

- **Documentation**: [Full API Reference](../api/)
- **Examples**: [Example Code](../examples/)
- **Issues**: [GitHub Issues](https://github.com/stegano/stegano/issues)
- **Discussions**: [GitHub Discussions](https://github.com/stegano/stegano/discussions)

Happy steganography! 🕵️‍♂️