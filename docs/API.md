# Stegano API Reference

This document provides detailed API documentation for the Stegano scientific data steganography system.

## Core Types

### DataFormat

Enumeration of supported data formats for steganographic operations.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataFormat {
    // Genomics formats
    FASTA,              // DNA/protein sequence data
    FASTQ,              // DNA sequences with quality scores
    VCF,                // Variant Call Format
    SAM,                // Sequence Alignment/Map
    BAM,                // Binary Alignment/Map
    
    // Image formats
    TIFF,               // Tagged Image File Format
    PNG,                // Portable Network Graphics
    MICROSCOPY,         // Scientific microscopy formats
    DICOM,              // Medical imaging
    
    // Signal formats
    WAV,                // Waveform Audio File Format
    BinarySignal,       // Raw binary signal data
    CsvTimeseries,      // CSV time series data
    
    // Generic formats
    BINARY,             // Raw binary data
    TEXT,               // Plain text data
}
```

### CarrierData

Structure representing the carrier data for steganographic operations.

```rust
#[derive(Debug, Clone)]
pub struct CarrierData {
    pub format: DataFormat,                                    // Format type
    pub raw_data: Vec<u8>,                                    // Raw carrier data
    pub metadata: HashMap<String, serde_json::Value>,        // Format metadata
    pub dimensions: Option<Dimensions>,                       // Image/video dimensions
    pub sampling_rate: Option<f64>,                          // Audio sampling rate
}
```

### Dimensions

Image or video dimensions for multi-dimensional data.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,                // Width in pixels
    pub height: u32,               // Height in pixels
    pub depth: Option<u32>,        // Depth for 3D data
    pub channels: Option<u32>,     // Number of channels (RGB, etc.)
}
```

### EmbedParams

Parameters controlling the embedding process.

```rust
#[derive(Debug, Clone)]
pub struct EmbedParams {
    pub encryption_key: Vec<u8>,                              // Encryption key
    pub error_correction_level: ECLevel,                     // Error correction level
    pub stealth_level: StealthLevel,                         // Steganographic stealth
    pub use_gpu: bool,                                       // Enable GPU acceleration
    pub enable_validation: bool,                             // Enable statistical validation
    pub format_specific: HashMap<String, serde_json::Value>, // Format-specific options
}
```

### Error Correction Levels

```rust
#[derive(Debug, Clone, Copy)]
pub enum ECLevel {
    None,      // No error correction
    Low,       // 10% overhead
    Medium,    // 25% overhead  
    High,      // 50% overhead
    Maximum,   // 100% overhead
}
```

### Stealth Levels

```rust
#[derive(Debug, Clone, Copy)]
pub enum StealthLevel {
    Maximum,   // Minimal capacity, maximum stealth
    High,      // Balanced approach
    Medium,    // Good capacity with reasonable stealth
    Low,       // Maximum capacity, minimal stealth
}
```

## Results and Statistics

### EmbedResult

Result of an embedding operation.

```rust
#[derive(Debug)]
pub struct EmbedResult {
    pub success: bool,                           // Operation success
    pub bytes_embedded: usize,                   // Number of bytes embedded
    pub carrier_modified: bool,                  // Whether carrier was modified
    pub performance_stats: PerformanceStats,    // Performance metrics
}
```

### CapacityInfo

Information about embedding capacity for a carrier.

```rust
#[derive(Debug)]
pub struct CapacityInfo {
    pub total_bits: usize,                       // Total bits in carrier
    pub recommended_bits: usize,                 // Recommended embedding bits
    pub steganographic_ratio: f64,               // Ratio of payload to carrier
    pub format_specific_info: HashMap<String, serde_json::Value>, // Format details
}
```

### PerformanceStats

Performance metrics for operations.

```rust
#[derive(Debug)]
pub struct PerformanceStats {
    pub embedding_time_ms: u64,      // Embedding time in milliseconds
    pub encryption_time_ms: u64,     // Encryption time in milliseconds
    pub gpu_acceleration_used: bool, // Whether GPU was used
    pub memory_usage_bytes: usize,   // Peak memory usage
}
```

## Core Traits

### DataHandler

Main trait for implementing format-specific steganographic operations.

```rust
#[async_trait]
pub trait DataHandler: Send + Sync {
    /// Detect if this handler can process the given data
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat>;
    
    /// Estimate embedding capacity for this data
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>;
    
    /// Embed payload into carrier data
    async fn embed(
        &self, 
        carrier: &mut CarrierData, 
        payload: &[u8], 
        params: &EmbedParams
    ) -> Result<EmbedResult>;
    
    /// Extract payload from carrier data
    async fn extract(
        &self, 
        carrier: &CarrierData, 
        params: &EmbedParams
    ) -> Result<Vec<u8>>;
    
    /// Verify data integrity
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>;
}
```

## Cryptographic API

### CryptoLayer

Main cryptographic interface for secure steganography.

```rust
impl CryptoLayer {
    /// Create new crypto layer from password
    pub fn new_from_password(password: &str) -> Result<Self>;

    /// Create crypto layer from raw key (32 bytes)
    pub fn new_from_key(key_bytes: &[u8]) -> Result<Self>;

    /// Encrypt payload with authenticated encryption
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<SecurePayload>;

    /// Decrypt and verify payload
    pub fn decrypt(&self, secure_payload: &SecurePayload) -> Result<Vec<u8>>;

    /// Serialize secure payload to bytes
    pub fn serialize_payload(payload: &SecurePayload) -> Vec<u8>;

    /// Deserialize secure payload from bytes
    pub fn deserialize_payload(data: &[u8]) -> Result<SecurePayload>;
}
```

### SecurePayload

Encrypted payload with integrity protection.

```rust
pub struct SecurePayload {
    pub encrypted_data: Vec<u8>,  // AES-GCM encrypted data
    pub nonce: [u8; 12],          // Encryption nonce
    pub hash: [u8; 32],           // Blake3 integrity hash
}
```

## Handler Registry

### HandlerRegistry

Registry for managing format handlers.

```rust
impl HandlerRegistry {
    /// Create new registry with default handlers
    pub fn new() -> Self;
    
    /// Get handler for specific format
    pub fn get_handler(&self, format: &DataFormat) -> Option<&dyn DataHandler>;
}
```

## GPU Acceleration

### GpuAccelerator

GPU acceleration interface for high-performance operations.

```rust
impl GpuAccelerator {
    /// Initialize GPU accelerator
    pub async fn new() -> Result<Self>;
    
    /// Parallel LSB embedding using GPU compute shaders
    pub async fn parallel_lsb_embed(
        &self, 
        data: &mut [u8], 
        payload_bits: &[u8]
    ) -> Result<()>;
    
    /// GPU-accelerated FFT for frequency domain embedding
    pub async fn gpu_fft(&self, signal: &[f32]) -> Result<Vec<Complex<f32>>>;
    
    /// Parallel Reed-Solomon encoding
    pub async fn parallel_reed_solomon(
        &self, 
        data: &[u8], 
        parity_bytes: usize
    ) -> Result<Vec<u8>>;
}
```

## Error Types

### SteganoError

Main error type for the library.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SteganoError {
    #[error("Unsupported data format: {0:?}")]
    UnsupportedFormat(DataFormat),
    
    #[error("Insufficient capacity: need {needed} bits, have {available}")]
    InsufficientCapacity { needed: usize, available: usize },
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("GPU acceleration error: {0}")]
    GpuError(String),
    
    #[error("Data integrity error: {0}")]
    IntegrityError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SteganoError>;
```

## Usage Examples

### Basic Steganography

```rust
use stegano::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Create carrier
    let carrier_data = std::fs::read("image.png")?;
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    // Setup encryption
    let crypto = crypto::CryptoLayer::new_from_password("password")?;
    let secret = b"Secret data";
    let encrypted = crypto.encrypt(secret)?;
    let payload = crypto::CryptoLayer::serialize_payload(&encrypted);

    // Embed
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG).unwrap();
    
    let params = EmbedParams {
        encryption_key: b"password".to_vec(),
        error_correction_level: ECLevel::Medium,
        stealth_level: StealthLevel::High,
        use_gpu: false,
        enable_validation: false,
        format_specific: HashMap::new(),
    };

    let result = handler.embed(&mut carrier, &payload, &params).await?;
    println!("Embedded {} bytes", result.bytes_embedded);

    Ok(())
}
```

### Format Detection

```rust
use stegano::*;

fn detect_file_format(data: &[u8]) -> Option<DataFormat> {
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::BINARY)?;
    handler.detect_format(data)
}
```

### Capacity Analysis

```rust
use stegano::*;

async fn analyze_capacity(carrier: &CarrierData) -> Result<()> {
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    let capacity = handler.estimate_capacity(carrier)?;
    println!("Total bits: {}", capacity.total_bits);
    println!("Available: {}", capacity.recommended_bits);
    println!("Ratio: {:.2}%", capacity.steganographic_ratio * 100.0);
    
    Ok(())
}
```

## Performance Considerations

### Memory Usage
- Large carriers are processed in streaming fashion when possible
- GPU operations use efficient memory transfer patterns
- Zero-copy operations where applicable

### Throughput
- CPU implementations use SIMD when available
- GPU acceleration provides significant speedup for large data
- Async operations allow concurrent processing

### Security
- Constant-time cryptographic operations
- Secure memory handling for keys
- Side-channel attack mitigation

## Thread Safety

All core types implement `Send + Sync` where appropriate:
- `HandlerRegistry` is thread-safe for read operations
- `CryptoLayer` can be shared across threads
- `CarrierData` requires synchronization for mutations

## Feature Flags

Available Cargo features:

- `gpu`: Enable GPU acceleration support
- `cuda`: Enable NVIDIA CUDA kernels
- `benchmarks`: Include performance benchmarks

```toml
[dependencies]
stegano = { version = "0.1", features = ["gpu", "benchmarks"] }
```