# Core API Reference

## Overview

The core module (`stegano::core`) provides fundamental types, traits, and error handling that form the foundation of the stegano library. Understanding these core concepts is essential for effectively using any part of the library.

## Core Types

### `DataFormat`

Enumeration of supported data formats for steganographic operations.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataFormat {
    // Genomics formats
    FASTA,           // FASTA sequence files
    FASTQ,           // FASTQ sequence files with quality scores
    VCF,             // Variant Call Format
    SAM,             // Sequence Alignment/Map format
    BAM,             // Binary Alignment/Map format
    
    // Imagery formats
    PNG,             // Portable Network Graphics
    TIFF,            // Tagged Image File Format
    MICROSCOPY,      // Scientific microscopy formats
    DICOM,           // Digital Imaging and Communications in Medicine
    
    // Signal formats
    WAV,             // Waveform Audio File Format
    BinarySignal,    // Raw binary signal data
    CsvTimeseries,   // CSV-based timeseries data
    
    // Generic formats
    BINARY,          // Generic binary data
    TEXT,            // Plain text data
}
```

**Usage Example:**
```rust
use stegano::core::DataFormat;

// Format detection
match format {
    DataFormat::FASTA | DataFormat::FASTQ => {
        // Handle genomics data
        process_genomics_data(data).await?;
    }
    DataFormat::PNG | DataFormat::TIFF => {
        // Handle image data
        process_image_data(data).await?;
    }
    _ => {
        // Handle other formats
        process_generic_data(data).await?;
    }
}
```

### `CarrierData`

Primary data structure representing the carrier data for steganographic operations.

```rust
#[derive(Debug, Clone)]
pub struct CarrierData {
    /// Detected or specified data format
    pub format: DataFormat,
    
    /// Raw binary data of the carrier
    pub raw_data: Vec<u8>,
    
    /// Format-specific metadata
    pub metadata: HashMap<String, serde_json::Value>,
    
    /// Image/video dimensions (width, height)
    pub dimensions: Option<(u32, u32)>,
    
    /// Audio sampling rate in Hz
    pub sampling_rate: Option<f64>,
}
```

**Key Features:**
- **Format Detection**: Automatic detection based on data content
- **Metadata Storage**: Extensible key-value metadata storage
- **Multi-domain Support**: Handles images, audio, genomics, and binary data
- **Memory Efficient**: Optimized for large data processing

**Usage Example:**
```rust
use stegano::core::{CarrierData, DataFormat};
use std::collections::HashMap;

// Create carrier data for an image
let carrier = CarrierData {
    format: DataFormat::PNG,
    raw_data: image_bytes,
    metadata: HashMap::new(),
    dimensions: Some((1920, 1080)),
    sampling_rate: None,
};

// Create carrier data for audio
let audio_carrier = CarrierData {
    format: DataFormat::WAV,
    raw_data: audio_bytes,
    metadata: HashMap::new(),
    dimensions: None,
    sampling_rate: Some(44100.0),
};
```

### `EmbedParams`

Configuration parameters for steganographic embedding operations.

```rust
#[derive(Debug, Clone)]
pub struct EmbedParams {
    /// Encryption key for payload protection
    pub encryption_key: Vec<u8>,
    
    /// Error correction level
    pub error_correction_level: ECLevel,
    
    /// Stealth level for imperceptibility
    pub stealth_level: StealthLevel,
    
    /// Enable GPU acceleration if available
    pub use_gpu: bool,
    
    /// Format-specific embedding parameters
    pub format_specific: HashMap<String, serde_json::Value>,
}
```

**Constructor:**
```rust
impl EmbedParams {
    pub fn new(encryption_key: Vec<u8>) -> Self {
        Self {
            encryption_key,
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Medium,
            use_gpu: true,
            format_specific: HashMap::new(),
        }
    }
}
```

**Usage Example:**
```rust
use stegano::core::{EmbedParams, ECLevel, StealthLevel};

// Basic parameters
let params = EmbedParams::new(b"my_secret_key".to_vec());

// Advanced configuration
let mut advanced_params = EmbedParams::new(b"advanced_key".to_vec());
advanced_params.error_correction_level = ECLevel::High;
advanced_params.stealth_level = StealthLevel::Maximum;
advanced_params.use_gpu = true;

// Format-specific parameters for genomics
advanced_params.format_specific.insert(
    "quality_threshold".to_string(),
    serde_json::json!(30)
);
```

### `CapacityInfo`

Information about the embedding capacity of carrier data.

```rust
#[derive(Debug, Clone)]
pub struct CapacityInfo {
    /// Total bits available in the carrier
    pub total_bits: usize,
    
    /// Recommended bits for safe embedding
    pub recommended_bits: usize,
    
    /// Ratio of steganographic to total capacity
    pub steganographic_ratio: f64,
    
    /// Format-specific capacity information
    pub format_specific_info: HashMap<String, serde_json::Value>,
}
```

**Methods:**
```rust
impl CapacityInfo {
    pub fn new(total_bits: usize, recommended_bits: usize) -> Self { ... }
    
    /// Get recommended capacity in bytes
    pub fn recommended_bytes(&self) -> usize {
        self.recommended_bits / 8
    }
    
    /// Check if payload fits within recommended capacity
    pub fn can_fit(&self, payload_size: usize) -> bool {
        payload_size * 8 <= self.recommended_bits
    }
}
```

### `EmbedResult`

Result information from embedding operations.

```rust
#[derive(Debug, Clone)]
pub struct EmbedResult {
    /// Whether embedding was successful
    pub success: bool,
    
    /// Number of bytes embedded
    pub bytes_embedded: usize,
    
    /// Whether carrier data was modified
    pub carrier_modified: bool,
    
    /// Performance statistics
    pub performance_stats: PerformanceStats,
}
```

### `PerformanceStats`

Performance metrics for steganographic operations.

```rust
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Embedding operation time in milliseconds
    pub embedding_time_ms: u64,
    
    /// Encryption time in milliseconds
    pub encryption_time_ms: u64,
    
    /// Whether GPU acceleration was used
    pub gpu_acceleration_used: bool,
    
    /// Peak memory usage in bytes
    pub memory_usage_bytes: usize,
}
```

## Enumerations

### `ECLevel` - Error Correction Level

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ECLevel {
    None,     // No error correction
    Low,      // Basic error detection
    Medium,   // Reed-Solomon error correction
    High,     // Advanced error correction with redundancy
}
```

### `StealthLevel` - Imperceptibility Level

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StealthLevel {
    Low,      // Minimal stealth, maximum capacity
    Medium,   // Balanced stealth and capacity
    High,     // High stealth, reduced capacity
    Maximum,  // Maximum stealth, minimal capacity
}
```

## Error Handling

### `SteganoError`

Comprehensive error type covering all failure modes.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SteganoError {
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    
    #[error("Insufficient capacity: needed {needed} bytes, available {available} bytes")]
    InsufficientCapacity { needed: usize, available: usize },
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Unsupported format: {0:?}")]
    UnsupportedFormat(DataFormat),
    
    #[error("Data integrity error: {0}")]
    IntegrityError(String),
    
    #[error("GPU error: {0}")]
    GpuError(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
```

**Error Handling Example:**
```rust
use stegano::core::SteganoError;

match result {
    Ok(embed_result) => {
        println!("Successfully embedded {} bytes", embed_result.bytes_embedded);
    }
    Err(SteganoError::InsufficientCapacity { needed, available }) => {
        eprintln!("Not enough space: need {} bytes, have {} bytes", needed, available);
    }
    Err(SteganoError::UnsupportedFormat(format)) => {
        eprintln!("Format {:?} is not supported", format);
    }
    Err(e) => {
        eprintln!("Embedding failed: {}", e);
    }
}
```

## Result Type

The library uses a standard `Result<T>` type alias:

```rust
pub type Result<T> = std::result::Result<T, SteganoError>;
```

## Utility Functions

### Format Detection

```rust
pub fn detect_format(data: &[u8]) -> Option<DataFormat> {
    // Automatic format detection based on file headers and content
}
```

### Capacity Estimation

```rust
pub fn estimate_capacity(
    data: &[u8], 
    format: &DataFormat
) -> Result<CapacityInfo> {
    // Quick capacity estimation without full handler initialization
}
```

## Best Practices

### Error Handling
```rust
// ✅ Proper error handling
match embed_operation().await {
    Ok(result) => handle_success(result),
    Err(SteganoError::InsufficientCapacity { needed, available }) => {
        // Handle capacity issues specifically
        suggest_compression_or_smaller_payload(needed, available);
    }
    Err(e) => handle_generic_error(e),
}

// ❌ Avoid unwrapping in production
let result = embed_operation().await.unwrap(); // Don't do this!
```

### Resource Management
```rust
// ✅ Efficient memory usage
let mut carrier = CarrierData {
    format: DataFormat::PNG,
    raw_data: Vec::with_capacity(expected_size), // Pre-allocate
    metadata: HashMap::with_capacity(10),
    dimensions: Some((width, height)),
    sampling_rate: None,
};

// ✅ Reuse parameters
let params = EmbedParams::new(encryption_key);
for data in datasets {
    embed_with_params(&mut data, &payload, &params).await?;
}
```

### Performance Optimization
```rust
// ✅ Enable GPU acceleration
let mut params = EmbedParams::new(key);
params.use_gpu = true;

// ✅ Choose appropriate stealth level
params.stealth_level = match requirements {
    Requirements::HighCapacity => StealthLevel::Low,
    Requirements::Balanced => StealthLevel::Medium,
    Requirements::MaxStealth => StealthLevel::Maximum,
};
```

## Integration Examples

### Basic Workflow
```rust
use stegano::core::*;
use stegano::handlers::HandlerRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize registry
    let registry = HandlerRegistry::new();
    
    // Load carrier data
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: std::fs::read("image.png")?,
        metadata: HashMap::new(),
        dimensions: Some((1920, 1080)),
        sampling_rate: None,
    };
    
    // Configure embedding
    let params = EmbedParams::new(b"secret_key".to_vec());
    
    // Get appropriate handler
    let handler = registry.get_handler(&carrier.format)
        .ok_or(SteganoError::UnsupportedFormat(carrier.format.clone()))?;
    
    // Embed payload
    let payload = b"Secret message";
    let result = handler.embed(&mut carrier, payload, &params).await?;
    
    println!("Embedded {} bytes in {}ms", 
             result.bytes_embedded, 
             result.performance_stats.embedding_time_ms);
    
    // Extract payload
    let extracted = handler.extract(&carrier, &params).await?;
    assert_eq!(extracted, payload);
    
    Ok(())
}
```

### Advanced Usage with Error Recovery
```rust
use stegano::core::*;

async fn robust_embed(
    carrier: &mut CarrierData,
    payload: &[u8],
    params: &EmbedParams,
) -> Result<EmbedResult> {
    let registry = HandlerRegistry::new();
    
    // Try primary handler
    if let Some(handler) = registry.get_handler(&carrier.format) {
        match handler.embed(carrier, payload, params).await {
            Ok(result) => return Ok(result),
            Err(SteganoError::InsufficientCapacity { .. }) => {
                // Try with higher compression
                let compressed = compress_payload(payload)?;
                return handler.embed(carrier, &compressed, params).await;
            }
            Err(SteganoError::GpuError(_)) => {
                // Retry without GPU
                let mut cpu_params = params.clone();
                cpu_params.use_gpu = false;
                return handler.embed(carrier, payload, &cpu_params).await;
            }
            Err(e) => return Err(e),
        }
    }
    
    Err(SteganoError::UnsupportedFormat(carrier.format.clone()))
}
```

---

## See Also

- [Handler System API](handlers.md) - Format-specific operations
- [GPU Acceleration API](gpu.md) - High-performance computing
- [Cryptography API](crypto.md) - Security and encryption
- [Getting Started Guide](../guides/getting-started.md) - Basic usage tutorial