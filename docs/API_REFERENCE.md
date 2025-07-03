# Stegano API Reference

Complete API documentation for Stegano v0.1.0 steganography system.

## 📚 Core Modules

### `stegano::core`

Core data structures and enums for steganography operations.

#### `CarrierData`
Represents the file that will carry hidden data.
```rust
pub struct CarrierData {
    pub format: DataFormat,
    pub raw_data: Vec<u8>,
    pub metadata: HashMap<String, String>,
    pub dimensions: Option<Dimensions>,
    pub sampling_rate: Option<f64>,
}
```

#### `EmbedParams`
Configuration parameters for embedding operations.
```rust
pub struct EmbedParams {
    pub encryption_key: Vec<u8>,
    pub error_correction_level: ECLevel,
    pub stealth_level: StealthLevel,
    pub use_gpu: bool,
    pub format_specific: HashMap<String, String>,
}
```

#### `DataFormat`
Supported file formats for steganography.
```rust
pub enum DataFormat {
    // Genomic formats
    FASTA,
    FASTQ,
    VCF,
    SAM,
    BAM,
    
    // Image formats
    PNG,
    TIFF,
    MICROSCOPY,
    DICOM,
    
    // Audio formats
    WAV,
    
    // Data formats
    CsvTimeseries,
    TEXT,
    BINARY,
}
```

### `stegano::handlers`

Format-specific steganography implementations.

#### `DataHandler` Trait
Core interface for steganography operations.
```rust
#[async_trait]
pub trait DataHandler: Send + Sync {
    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult>;
    
    async fn extract(
        &self,
        carrier: &CarrierData,
        params: &EmbedParams,
    ) -> Result<Vec<u8>>;
    
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>;
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>;
}
```

#### `ImageryHandler`
Advanced image steganography with multiple embedding strategies.
```rust
pub struct ImageryHandler {
    pub strategy: ImageEmbedStrategy,
    pub crypto: Option<CryptoLayer>,
}

pub enum ImageEmbedStrategy {
    SimpleLSB,           // Basic LSB embedding
    AdaptiveLSB,         // Adaptive LSB based on pixel values
    DCTFrequency,        // DCT frequency domain embedding
    PVD,                 // Pixel Value Differencing
    EdgeBased,           // Edge-detection based embedding
}
```

**Key Methods:**
```rust
impl ImageryHandler {
    pub fn new(strategy: ImageEmbedStrategy) -> Self;
    pub fn with_encryption(strategy: ImageEmbedStrategy, key: &[u8]) -> Self;
    pub fn detect_format(data: &[u8]) -> Option<DataFormat>;
    pub fn calculate_edge_map(data: &[u8], width: usize, height: usize) -> Vec<u8>;
}
```

#### `SignalsHandler`
Audio and signal steganography with frequency domain techniques.
```rust
pub struct SignalsHandler {
    pub strategy: SignalEmbedStrategy,
    pub crypto: Option<CryptoLayer>,
}

pub enum SignalEmbedStrategy {
    TimeDomainLSB,       // Time domain LSB embedding
    FrequencyDomain,     // FFT frequency domain embedding
    PhaseModulation,     // Phase modulation technique
    SpreadSpectrum,      // Spread spectrum embedding
    EchoHiding,          // Echo hiding technique
}
```

**Key Methods:**
```rust
impl SignalsHandler {
    pub fn new(strategy: SignalEmbedStrategy) -> Self;
    pub fn with_encryption(strategy: SignalEmbedStrategy, key: &[u8]) -> Self;
    pub fn bytes_to_samples(data: &[u8]) -> Vec<f32>;
    pub fn samples_to_bytes(samples: &[f32]) -> Vec<u8>;
    pub fn apply_fft(samples: &[f32]) -> Vec<Complex<f32>>;
}
```

### `stegano::crypto`

Cryptographic operations for secure steganography.

#### `CryptoLayer`
AES-256-GCM encryption with secure key derivation.
```rust
pub struct CryptoLayer {
    key: [u8; 32],
}

impl CryptoLayer {
    pub fn new(password: &[u8]) -> Result<Self>;
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>>;
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>>;
    pub fn serialize_payload(&self, data: &[u8]) -> Result<Vec<u8>>;
    pub fn deserialize_payload(&self, data: &[u8]) -> Result<Vec<u8>>;
}
```

#### `SecurePayload`
Encrypted payload structure with metadata.
```rust
pub struct SecurePayload {
    pub nonce: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub auth_tag: Vec<u8>,
    pub metadata: HashMap<String, String>,
}
```

### `stegano::gpu`

GPU acceleration for high-performance operations.

#### `MetalAccelerator`
Metal GPU acceleration for Apple Silicon.
```rust
pub struct MetalAccelerator {
    device: Device,
    command_queue: CommandQueue,
    library: Library,
}

impl MetalAccelerator {
    pub fn new() -> Result<Self>;
    pub fn is_available() -> bool;
    pub async fn embed_lsb(
        &self,
        carrier: &[u8],
        payload: &[u8],
        strategy: EmbedStrategy,
    ) -> Result<Vec<u8>>;
    pub async fn extract_lsb(
        &self,
        carrier: &[u8],
        payload_length: usize,
    ) -> Result<Vec<u8>>;
}
```

## 🔧 CLI Interface

### Commands

#### `embed`
Hide secret data inside a carrier file.
```bash
stegano embed [OPTIONS] --input <INPUT> --payload <PAYLOAD> --output <OUTPUT>
```

**Options:**
- `--input <INPUT>`: Carrier file path
- `--payload <PAYLOAD>`: Secret data file path  
- `--output <OUTPUT>`: Output file path
- `--password <PASSWORD>`: Optional encryption password
- `--stealth-level <LEVEL>`: Stealth level (low, medium, high)
- `--error-correction <LEVEL>`: Error correction (low, medium, high)
- `--use-gpu`: Enable GPU acceleration (macOS only)

#### `extract`
Extract hidden data from a steganographic file.
```bash
stegano extract [OPTIONS] --input <INPUT> --output <OUTPUT>
```

**Options:**
- `--input <INPUT>`: File containing hidden data
- `--output <OUTPUT>`: Output file for extracted data
- `--password <PASSWORD>`: Decryption password if used

#### `analyze`
Analyze a file's steganographic capacity and detect hidden data.
```bash
stegano analyze --input <INPUT>
```

**Options:**
- `--input <INPUT>`: File to analyze

## 📊 Data Structures

### `EmbedResult`
Result of embedding operation.
```rust
pub struct EmbedResult {
    pub bytes_embedded: usize,
    pub capacity_used: f64,
    pub performance_stats: PerformanceStats,
    pub integrity_hash: String,
}
```

### `CapacityInfo`
Capacity analysis results.
```rust
pub struct CapacityInfo {
    pub total_bits: usize,
    pub recommended_bits: usize,
    pub overhead_percent: f64,
    pub format_specific_info: HashMap<String, String>,
}
```

### `PerformanceStats`
Performance metrics for operations.
```rust
pub struct PerformanceStats {
    pub embed_time_ms: f64,
    pub crypto_time_ms: f64,
    pub gpu_time_ms: Option<f64>,
    pub total_time_ms: f64,
}
```

### `Dimensions`
Image or data dimensions.
```rust
pub struct Dimensions {
    pub width: usize,
    pub height: usize,
    pub depth: Option<usize>,
    pub channels: Option<usize>,
}
```

## 🔐 Error Handling

### `SteganoError`
Comprehensive error types for all operations.
```rust
pub enum SteganoError {
    InvalidParameter(String),
    InsufficientCapacity { needed: usize, available: usize },
    CryptographicFailure(String),
    FormatNotSupported(DataFormat),
    DataCorrupted(String),
    GpuError(String),
    IoError(String),
}
```

## 🚀 Usage Examples

### Basic Embedding
```rust
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Create carrier data
    let carrier_data = std::fs::read("image.png")?;
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };
    
    // Setup parameters
    let params = EmbedParams {
        encryption_key: b"secure_key_2025".to_vec(),
        error_correction_level: ECLevel::Medium,
        stealth_level: StealthLevel::Low,
        use_gpu: false,
        format_specific: HashMap::new(),
    };
    
    // Get handler and embed
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    let secret_data = b"Secret message";
    
    let result = handler.embed(&mut carrier, secret_data, &params).await?;
    println!("Embedded {} bytes", result.bytes_embedded);
    
    // Save result
    std::fs::write("output.png", &carrier.raw_data)?;
    Ok(())
}
```

### GPU-Accelerated Embedding (macOS)
```rust
use stegano::gpu::MetalAccelerator;

#[tokio::main]
async fn main() -> Result<()> {
    if MetalAccelerator::is_available() {
        let accelerator = MetalAccelerator::new()?;
        
        let carrier = std::fs::read("large_image.png")?;
        let payload = std::fs::read("secret.txt")?;
        
        let result = accelerator.embed_lsb(
            &carrier,
            &payload,
            EmbedStrategy::SimpleLSB,
        ).await?;
        
        std::fs::write("accelerated_output.png", &result)?;
        println!("GPU embedding completed");
    }
    Ok(())
}
```

### Custom Handler Implementation
```rust
use stegano::handlers::DataHandler;
use async_trait::async_trait;

pub struct CustomHandler;

#[async_trait]
impl DataHandler for CustomHandler {
    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        // Custom embedding logic
        todo!("Implement custom embedding strategy")
    }
    
    async fn extract(
        &self,
        carrier: &CarrierData,
        params: &EmbedParams,
    ) -> Result<Vec<u8>> {
        // Custom extraction logic
        todo!("Implement custom extraction strategy")
    }
    
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        // Custom capacity calculation
        todo!("Implement capacity estimation")
    }
    
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        // Custom integrity verification
        todo!("Implement integrity verification")
    }
}
```

## 🔍 Format-Specific Features

### Genomic Data (FASTA/FASTQ)
- Sequence-aware embedding preserving biological meaning
- Quality score manipulation in FASTQ
- Header metadata embedding
- Codon-preserving strategies

### Medical Imaging (DICOM/TIFF)
- HIPAA-compliant metadata embedding
- Lossless embedding preserving diagnostic quality
- Multi-frame support
- Patient data protection

### Audio Signals (WAV)
- Frequency domain embedding
- Phase modulation techniques
- Psychoacoustic masking
- Multi-channel support

## 📈 Performance Characteristics

### Capacity Efficiency
| Format | Capacity Ratio | Example (1MB file) |
|--------|---------------|-------------------|
| Text/Binary | 12.5% | 128KB hidden data |
| PNG Images | 25-37.5% | 256-384KB hidden data |
| WAV Audio | 6.25% | 64KB hidden data |
| FASTA/FASTQ | 8-15% | 80-150KB hidden data |

### Performance Benchmarks (Apple M4 Pro)
| Operation | File Size | Time | Throughput |
|-----------|-----------|------|------------|
| Embed | 1MB | ~20ms | 50MB/s |
| Extract | 1MB | ~15ms | 67MB/s |
| GPU Embed | 10MB | ~180ms | 56MB/s |
| Analyze | 1MB | ~5ms | 200MB/s |

---

For additional examples and advanced usage patterns, see the `/examples` directory in the repository.