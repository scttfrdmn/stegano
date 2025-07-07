//! # Core Types and Traits
//!
//! This module provides the fundamental types, traits, and data structures for the steganography system.
//!
//! ## Key Components
//!
//! ### Data Formats
//! The [`DataFormat`] enum defines all supported data formats, organized by category:
//! - **Genomics**: FASTA, FASTQ, VCF, SAM, BAM
//! - **Imagery**: TIFF, PNG, MICROSCOPY, DICOM
//! - **Signals**: WAV, BinarySignal, CsvTimeseries
//! - **Generic**: BINARY, TEXT
//!
//! ### Carrier Data
//! [`CarrierData`] represents the container for embedding data, including:
//! - Format detection and validation
//! - Raw binary data
//! - Metadata and dimensions
//! - Sampling rate for signal data
//!
//! ### Embedding Parameters
//! [`EmbedParams`] configures the embedding process:
//! - Encryption keys and algorithms
//! - Error correction levels (None to Maximum)
//! - Stealth levels (Maximum stealth to Low)
//! - GPU acceleration options
//! - Format-specific parameters
//!
//! ## Examples
//!
//! ### Creating Carrier Data
//! ```rust
//! use stegano::core::*;
//!
//! let data = std::fs::read("sample.png").unwrap();
//! let mut carrier = CarrierData::new(DataFormat::PNG, data);
//! carrier.add_metadata("width".to_string(), serde_json::Value::Number(800.into()));
//! ```
//!
//! ### Configuring Embedding Parameters
//! ```rust
//! use stegano::core::*;
//!
//! let params = EmbedParams::new(b"secure_key".to_vec())
//!     .with_error_correction(ECLevel::High)
//!     .with_stealth_level(StealthLevel::Medium)
//!     .with_gpu();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Data format enumeration covering all supported steganographic carriers.
///
/// Formats are organized by scientific domain to enable domain-specific optimizations
/// and embedding strategies. Each format has specialized handlers that understand
/// the format's structure and constraints.
///
/// # Genomics Formats
/// - [`FASTA`](DataFormat::FASTA): DNA/RNA/protein sequence data with header metadata embedding
/// - [`FASTQ`](DataFormat::FASTQ): Next-generation sequencing data with advanced quality score steganography
/// - [`VCF`](DataFormat::VCF): Variant Call Format for genomic variations (planned)
/// - [`SAM`](DataFormat::SAM): Sequence Alignment/Map format (planned)
/// - [`BAM`](DataFormat::BAM): Binary Alignment/Map format (planned)
///
/// # Imagery Formats
/// - [`TIFF`](DataFormat::TIFF): Multi-channel scientific imagery with metadata fields
/// - [`PNG`](DataFormat::PNG): Standard images with LSB techniques
/// - [`MICROSCOPY`](DataFormat::MICROSCOPY): Multi-dimensional microscopy stacks (planned)
/// - [`DICOM`](DataFormat::DICOM): Medical imaging format (planned)
///
/// # Signal Formats
/// - [`WAV`](DataFormat::WAV): Audio signals with various embedding techniques
/// - [`BinarySignal`](DataFormat::BinarySignal): Raw signal data
/// - [`CsvTimeseries`](DataFormat::CsvTimeseries): Time series data in CSV format
///
/// # Generic Formats
/// - [`BINARY`](DataFormat::BINARY): Generic binary data with LSB embedding
/// - [`TEXT`](DataFormat::TEXT): Plain text with whitespace steganography
///
/// # Examples
///
/// ```rust
/// use stegano::core::DataFormat;
///
/// // Genomics data
/// let format = DataFormat::FASTQ;
/// assert_eq!(format.is_genomics(), true);
///
/// // Image data
/// let format = DataFormat::PNG;
/// assert_eq!(format.is_imagery(), true);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataFormat {
    // Genomics formats
    /// FASTA sequence format - DNA/RNA/protein sequences with header embedding
    FASTA,
    /// FASTQ format with quality scores - Advanced quality score steganography
    FASTQ,
    /// Variant Call Format - Genomic variation data (planned)
    VCF,
    /// Sequence Alignment/Map format (planned)
    SAM,
    /// Binary Alignment/Map format (planned)
    BAM,

    // Image formats
    /// Tagged Image File Format - Multi-channel scientific imagery
    TIFF,
    /// Portable Network Graphics - Standard image format
    PNG,
    /// Multi-dimensional microscopy data (planned)
    MICROSCOPY,
    /// Digital Imaging and Communications in Medicine (planned)
    DICOM,

    // Signal formats
    /// Waveform Audio File Format
    WAV,
    /// Raw binary signal data
    BinarySignal,
    /// Comma-separated values time series
    CsvTimeseries,

    // Generic
    /// Generic binary data
    BINARY,
    /// Plain text data
    TEXT,
}

impl DataFormat {
    /// Check if this format is a genomics format
    pub fn is_genomics(&self) -> bool {
        matches!(
            self,
            DataFormat::FASTA
                | DataFormat::FASTQ
                | DataFormat::VCF
                | DataFormat::SAM
                | DataFormat::BAM
        )
    }

    /// Check if this format is an imagery format
    pub fn is_imagery(&self) -> bool {
        matches!(
            self,
            DataFormat::TIFF | DataFormat::PNG | DataFormat::MICROSCOPY | DataFormat::DICOM
        )
    }

    /// Check if this format is a signal format
    pub fn is_signal(&self) -> bool {
        matches!(
            self,
            DataFormat::WAV | DataFormat::BinarySignal | DataFormat::CsvTimeseries
        )
    }

    /// Get the typical file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            DataFormat::FASTA => ".fasta",
            DataFormat::FASTQ => ".fastq",
            DataFormat::VCF => ".vcf",
            DataFormat::SAM => ".sam",
            DataFormat::BAM => ".bam",
            DataFormat::TIFF => ".tiff",
            DataFormat::PNG => ".png",
            DataFormat::MICROSCOPY => ".tiff",
            DataFormat::DICOM => ".dcm",
            DataFormat::WAV => ".wav",
            DataFormat::BinarySignal => ".bin",
            DataFormat::CsvTimeseries => ".csv",
            DataFormat::BINARY => ".bin",
            DataFormat::TEXT => ".txt",
        }
    }
}

/// Container for carrier data used in steganographic embedding operations.
///
/// `CarrierData` encapsulates all information about the data that will host hidden information,
/// including format identification, raw binary content, metadata, and format-specific properties.
///
/// # Fields
///
/// * `format` - The detected or specified data format
/// * `raw_data` - The binary content of the carrier
/// * `metadata` - Key-value metadata extracted from or added to the carrier
/// * `dimensions` - Spatial dimensions for image/video data
/// * `sampling_rate` - Sampling rate for audio/signal data
///
/// # Examples
///
/// ```rust
/// use stegano::core::*;
/// use std::collections::HashMap;
///
/// // Create carrier from file data
/// let data = std::fs::read("sample.png").unwrap();
/// let mut carrier = CarrierData::new(DataFormat::PNG, data);
///
/// // Add metadata
/// carrier.add_metadata("width".to_string(), serde_json::Value::Number(800.into()));
/// carrier.add_metadata("height".to_string(), serde_json::Value::Number(600.into()));
///
/// // Set dimensions for image data
/// carrier.dimensions = Some(Dimensions::new_2d(800, 600));
///
/// println!("Carrier size: {} bytes", carrier.size());
/// ```
#[derive(Debug, Clone)]
pub struct CarrierData {
    /// The data format of this carrier
    pub format: DataFormat,
    /// Raw binary data of the carrier
    pub raw_data: Vec<u8>,
    /// Metadata key-value pairs
    pub metadata: HashMap<String, serde_json::Value>,
    /// Spatial dimensions for image/video data
    pub dimensions: Option<Dimensions>,
    /// Sampling rate for audio/signal data in Hz
    pub sampling_rate: Option<f64>,
}

impl CarrierData {
    /// Create new carrier data with minimal fields
    pub fn new(format: DataFormat, data: Vec<u8>) -> Self {
        Self {
            format,
            raw_data: data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        }
    }

    /// Get the size of the carrier data in bytes
    pub fn size(&self) -> usize {
        self.raw_data.len()
    }

    /// Check if carrier data is empty
    pub fn is_empty(&self) -> bool {
        self.raw_data.is_empty()
    }

    /// Add metadata entry
    pub fn add_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
    pub depth: Option<u32>,
    pub channels: Option<u32>,
}

impl Dimensions {
    /// Create 2D dimensions
    pub fn new_2d(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            depth: None,
            channels: None,
        }
    }

    /// Create 3D dimensions
    pub fn new_3d(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth: Some(depth),
            channels: None,
        }
    }

    /// Calculate total pixels/voxels
    pub fn total_elements(&self) -> u64 {
        let base = self.width as u64 * self.height as u64;
        let with_depth = base * self.depth.unwrap_or(1) as u64;
        with_depth * self.channels.unwrap_or(1) as u64
    }
}

/// Parameters controlling the steganographic embedding process.
///
/// `EmbedParams` provides comprehensive configuration for embedding operations,
/// including security, performance, and format-specific options.
///
/// # Security Configuration
/// * `encryption_key` - Encryption key for payload protection
/// * `error_correction_level` - Reed-Solomon error correction strength
/// * `stealth_level` - Balance between capacity and detectability
///
/// # Performance Configuration  
/// * `use_gpu` - Enable GPU acceleration when available
/// * `format_specific` - Format-specific embedding parameters
///
/// # Examples
///
/// ```rust
/// use stegano::core::*;
/// use std::collections::HashMap;
///
/// // Basic configuration
/// let params = EmbedParams::new(b"secure_password".to_vec());
///
/// // Advanced configuration with GPU and high stealth
/// let params = EmbedParams::new(b"my_key".to_vec())
///     .with_error_correction(ECLevel::High)
///     .with_stealth_level(StealthLevel::Maximum)
///     .with_gpu();
///
/// // Format-specific parameters
/// let mut format_params = HashMap::new();
/// format_params.insert("strategy".to_string(), serde_json::Value::String("conservative".to_string()));
/// let params = EmbedParams {
///     encryption_key: b"key".to_vec(),
///     error_correction_level: ECLevel::Medium,
///     stealth_level: StealthLevel::High,
///     use_gpu: false,
///     format_specific: format_params,
/// };
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedParams {
    /// Encryption key for payload protection
    pub encryption_key: Vec<u8>,
    /// Error correction level for data integrity
    pub error_correction_level: ECLevel,
    /// Stealth level balancing capacity vs detectability
    pub stealth_level: StealthLevel,
    /// Enable GPU acceleration
    pub use_gpu: bool,
    /// Format-specific embedding parameters
    pub format_specific: HashMap<String, serde_json::Value>,
}

impl EmbedParams {
    /// Create default embedding parameters
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            encryption_key: key,
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::High,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    }

    /// Create parameters with specific error correction level
    pub fn with_error_correction(key: Vec<u8>, ec_level: ECLevel) -> Self {
        Self {
            encryption_key: key,
            error_correction_level: ec_level,
            stealth_level: StealthLevel::High,
            use_gpu: false,
            format_specific: HashMap::new(),
        }
    }

    /// Enable GPU acceleration
    pub fn with_gpu(mut self) -> Self {
        self.use_gpu = true;
        self
    }

    /// Set stealth level
    pub fn with_stealth_level(mut self, level: StealthLevel) -> Self {
        self.stealth_level = level;
        self
    }
}

/// Error correction levels for protecting embedded data against corruption.
///
/// Error correction uses Reed-Solomon codes to add redundancy that enables
/// recovery from data corruption, transmission errors, or format-specific degradation.
/// Higher levels provide better protection but reduce effective capacity.
///
/// # Overhead Percentages
/// * [`None`](ECLevel::None) - 0% overhead, no protection
/// * [`Low`](ECLevel::Low) - 10% overhead, basic protection  
/// * [`Medium`](ECLevel::Medium) - 25% overhead, good protection
/// * [`High`](ECLevel::High) - 50% overhead, strong protection
/// * [`Maximum`](ECLevel::Maximum) - 100% overhead, maximum protection
///
/// # Examples
///
/// ```rust
/// use stegano::core::ECLevel;
///
/// let level = ECLevel::Medium;
/// assert_eq!(level.overhead_percent(), 0.25);
/// assert_eq!(level.total_size(100), 125); // 100 + 25% overhead
/// ```
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ECLevel {
    /// No error correction (0% overhead)
    None,
    /// Low error correction (10% overhead)
    Low,
    /// Medium error correction (25% overhead)
    Medium,
    /// High error correction (50% overhead)
    High,
    /// Maximum error correction (100% overhead)
    Maximum,
}

impl ECLevel {
    /// Get the overhead percentage for this error correction level
    pub fn overhead_percent(&self) -> f64 {
        match self {
            ECLevel::None => 0.0,
            ECLevel::Low => 0.1,
            ECLevel::Medium => 0.25,
            ECLevel::High => 0.5,
            ECLevel::Maximum => 1.0,
        }
    }

    /// Calculate the total size with error correction overhead
    pub fn total_size(&self, base_size: usize) -> usize {
        base_size + (base_size as f64 * self.overhead_percent()) as usize
    }
}

/// Stealth levels controlling the trade-off between embedding capacity and detectability.
///
/// Stealth levels determine how aggressively data is embedded, balancing the amount
/// of data that can be hidden against the risk of statistical detection. Higher
/// stealth levels use more conservative embedding strategies.
///
/// # Capacity Multipliers
/// * [`Maximum`](StealthLevel::Maximum) - 10% capacity, minimal detectability
/// * [`High`](StealthLevel::High) - 30% capacity, low detectability
/// * [`Medium`](StealthLevel::Medium) - 60% capacity, moderate detectability
/// * [`Low`](StealthLevel::Low) - 100% capacity, higher detectability risk
///
/// # Examples
///
/// ```rust
/// use stegano::core::StealthLevel;
///
/// let level = StealthLevel::High;
/// assert_eq!(level.capacity_multiplier(), 0.3);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StealthLevel {
    /// Maximum stealth, minimal capacity (10% multiplier)
    Maximum,
    /// High stealth, balanced approach (30% multiplier)
    High,
    /// Medium stealth, good capacity (60% multiplier)
    Medium,
    /// Low stealth, maximum capacity (100% multiplier)
    Low,
}

impl StealthLevel {
    /// Get the capacity multiplier for this stealth level
    pub fn capacity_multiplier(&self) -> f64 {
        match self {
            StealthLevel::Maximum => 0.1,
            StealthLevel::High => 0.3,
            StealthLevel::Medium => 0.6,
            StealthLevel::Low => 1.0,
        }
    }
}

#[derive(Debug)]
pub struct EmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub carrier_modified: bool,
    pub performance_stats: PerformanceStats,
}

impl EmbedResult {
    /// Create successful embedding result
    pub fn success(bytes_embedded: usize, stats: PerformanceStats) -> Self {
        Self {
            success: true,
            bytes_embedded,
            carrier_modified: true,
            performance_stats: stats,
        }
    }

    /// Create failed embedding result
    pub fn failure(stats: PerformanceStats) -> Self {
        Self {
            success: false,
            bytes_embedded: 0,
            carrier_modified: false,
            performance_stats: stats,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub embedding_time_ms: u64,
    pub encryption_time_ms: u64,
    pub gpu_acceleration_used: bool,
    pub memory_usage_bytes: usize,
}

impl PerformanceStats {
    /// Create new performance stats
    pub fn new() -> Self {
        Self {
            embedding_time_ms: 0,
            encryption_time_ms: 0,
            gpu_acceleration_used: false,
            memory_usage_bytes: 0,
        }
    }

    /// Calculate total processing time
    pub fn total_time_ms(&self) -> u64 {
        self.embedding_time_ms + self.encryption_time_ms
    }

    /// Calculate throughput in bytes per second
    pub fn throughput_bps(&self, bytes_processed: usize) -> f64 {
        if self.total_time_ms() == 0 {
            return 0.0;
        }
        (bytes_processed as f64 * 1000.0) / self.total_time_ms() as f64
    }
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct CapacityInfo {
    pub total_bits: usize,
    pub recommended_bits: usize,
    pub steganographic_ratio: f64,
    pub format_specific_info: HashMap<String, serde_json::Value>,
}

impl CapacityInfo {
    /// Create new capacity info
    pub fn new(total_bits: usize, recommended_bits: usize) -> Self {
        let ratio = if total_bits > 0 {
            recommended_bits as f64 / total_bits as f64
        } else {
            0.0
        };

        Self {
            total_bits,
            recommended_bits,
            steganographic_ratio: ratio,
            format_specific_info: HashMap::new(),
        }
    }

    /// Get recommended capacity in bytes
    pub fn recommended_bytes(&self) -> usize {
        self.recommended_bits / 8
    }

    /// Check if given payload size fits within recommended capacity
    pub fn can_fit(&self, payload_size: usize) -> bool {
        payload_size * 8 <= self.recommended_bits
    }

    /// Calculate utilization percentage for given payload
    pub fn utilization_percent(&self, payload_size: usize) -> f64 {
        if self.recommended_bits == 0 {
            return 0.0;
        }
        (payload_size * 8) as f64 / self.recommended_bits as f64 * 100.0
    }
}

// Error types
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

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl SteganoError {
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            SteganoError::InsufficientCapacity { .. } | SteganoError::InvalidParameter(_)
        )
    }

    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            SteganoError::UnsupportedFormat(_) => "format",
            SteganoError::InsufficientCapacity { .. } => "capacity",
            SteganoError::EncryptionError(_) => "crypto",
            SteganoError::GpuError(_) => "gpu",
            SteganoError::IntegrityError(_) => "integrity",
            SteganoError::InvalidParameter(_) => "parameter",
            SteganoError::IoError(_) => "io",
            SteganoError::SerializationError(_) => "serialization",
        }
    }
}

pub type Result<T> = std::result::Result<T, SteganoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_format_serialization() {
        let format = DataFormat::FASTA;
        let serialized = serde_json::to_string(&format).unwrap();
        let deserialized: DataFormat = serde_json::from_str(&serialized).unwrap();
        assert_eq!(format, deserialized);
    }

    #[test]
    fn test_carrier_data_creation() {
        let data = vec![1, 2, 3, 4, 5];
        let carrier = CarrierData::new(DataFormat::BINARY, data.clone());

        assert_eq!(carrier.format, DataFormat::BINARY);
        assert_eq!(carrier.raw_data, data);
        assert_eq!(carrier.size(), 5);
        assert!(!carrier.is_empty());
    }

    #[test]
    fn test_carrier_data_metadata() {
        let mut carrier = CarrierData::new(DataFormat::PNG, vec![]);

        carrier.add_metadata("width".to_string(), serde_json::Value::Number(800.into()));
        carrier.add_metadata("height".to_string(), serde_json::Value::Number(600.into()));

        assert_eq!(
            carrier.get_metadata("width"),
            Some(&serde_json::Value::Number(800.into()))
        );
        assert_eq!(
            carrier.get_metadata("height"),
            Some(&serde_json::Value::Number(600.into()))
        );
        assert_eq!(carrier.get_metadata("nonexistent"), None);
    }

    #[test]
    fn test_dimensions() {
        let dim_2d = Dimensions::new_2d(800, 600);
        assert_eq!(dim_2d.width, 800);
        assert_eq!(dim_2d.height, 600);
        assert_eq!(dim_2d.depth, None);
        assert_eq!(dim_2d.total_elements(), 800 * 600);

        let dim_3d = Dimensions::new_3d(100, 100, 50);
        assert_eq!(dim_3d.total_elements(), 100 * 100 * 50);
    }

    #[test]
    fn test_embed_params() {
        let key = b"test_key".to_vec();
        let params = EmbedParams::new(key.clone());

        assert_eq!(params.encryption_key, key);
        assert_eq!(params.error_correction_level, ECLevel::Medium);
        assert_eq!(params.stealth_level, StealthLevel::High);
        assert!(!params.use_gpu);

        let gpu_params = EmbedParams::new(key.clone()).with_gpu();
        assert!(gpu_params.use_gpu);

        let low_stealth = EmbedParams::new(key).with_stealth_level(StealthLevel::Low);
        assert_eq!(low_stealth.stealth_level, StealthLevel::Low);
    }

    #[test]
    fn test_ec_level() {
        assert_eq!(ECLevel::None.overhead_percent(), 0.0);
        assert_eq!(ECLevel::Low.overhead_percent(), 0.1);
        assert_eq!(ECLevel::Medium.overhead_percent(), 0.25);
        assert_eq!(ECLevel::High.overhead_percent(), 0.5);
        assert_eq!(ECLevel::Maximum.overhead_percent(), 1.0);

        assert_eq!(ECLevel::Low.total_size(100), 110);
        assert_eq!(ECLevel::High.total_size(100), 150);
    }

    #[test]
    fn test_stealth_level() {
        assert_eq!(StealthLevel::Maximum.capacity_multiplier(), 0.1);
        assert_eq!(StealthLevel::High.capacity_multiplier(), 0.3);
        assert_eq!(StealthLevel::Medium.capacity_multiplier(), 0.6);
        assert_eq!(StealthLevel::Low.capacity_multiplier(), 1.0);
    }

    #[test]
    fn test_embed_result() {
        let stats = PerformanceStats::new();
        let success = EmbedResult::success(100, stats.clone());

        assert!(success.success);
        assert_eq!(success.bytes_embedded, 100);
        assert!(success.carrier_modified);

        let failure = EmbedResult::failure(stats);
        assert!(!failure.success);
        assert_eq!(failure.bytes_embedded, 0);
        assert!(!failure.carrier_modified);
    }

    #[test]
    fn test_performance_stats() {
        let mut stats = PerformanceStats::new();
        stats.embedding_time_ms = 100;
        stats.encryption_time_ms = 50;

        assert_eq!(stats.total_time_ms(), 150);
        assert_eq!(stats.throughput_bps(1500), 10000.0); // 1500 bytes in 150ms = 10,000 bytes/sec
    }

    #[test]
    fn test_capacity_info() {
        let capacity = CapacityInfo::new(1000, 800);

        assert_eq!(capacity.total_bits, 1000);
        assert_eq!(capacity.recommended_bits, 800);
        assert_eq!(capacity.steganographic_ratio, 0.8);
        assert_eq!(capacity.recommended_bytes(), 100);

        assert!(capacity.can_fit(50)); // 50 bytes = 400 bits < 800
        assert!(!capacity.can_fit(200)); // 200 bytes = 1600 bits > 800

        assert_eq!(capacity.utilization_percent(50), 50.0); // 400 bits / 800 bits = 50%
    }

    #[test]
    fn test_stegano_error() {
        let error = SteganoError::InsufficientCapacity {
            needed: 100,
            available: 50,
        };
        assert!(error.is_recoverable());
        assert_eq!(error.category(), "capacity");

        let format_error = SteganoError::UnsupportedFormat(DataFormat::PNG);
        assert!(!format_error.is_recoverable());
        assert_eq!(format_error.category(), "format");
    }
}
