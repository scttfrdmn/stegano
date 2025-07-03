//! Core types and traits for the steganography system

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataFormat {
    // Genomics formats
    FASTA,
    FASTQ,
    VCF,
    SAM,
    BAM,
    
    // Image formats
    TIFF,
    PNG,
    MICROSCOPY,
    DICOM,
    
    // Signal formats
    WAV,
    BinarySignal,
    CsvTimeseries,
    
    // Generic
    BINARY,
    TEXT,
}

#[derive(Debug, Clone)]
pub struct CarrierData {
    pub format: DataFormat,
    pub raw_data: Vec<u8>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub dimensions: Option<Dimensions>,
    pub sampling_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
    pub depth: Option<u32>,
    pub channels: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EmbedParams {
    pub encryption_key: Vec<u8>,
    pub error_correction_level: ECLevel,
    pub stealth_level: StealthLevel,
    pub use_gpu: bool,
    pub format_specific: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub enum ECLevel {
    None,
    Low,     // 10% overhead
    Medium,  // 25% overhead  
    High,    // 50% overhead
    Maximum, // 100% overhead
}

#[derive(Debug, Clone, Copy)]
pub enum StealthLevel {
    Maximum,  // Minimal capacity, maximum stealth
    High,     // Balanced approach
    Medium,   // Good capacity with reasonable stealth
    Low,      // Maximum capacity, minimal stealth
}

#[derive(Debug)]
pub struct EmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub carrier_modified: bool,
    pub performance_stats: PerformanceStats,
}

#[derive(Debug)]
pub struct PerformanceStats {
    pub embedding_time_ms: u64,
    pub encryption_time_ms: u64,
    pub gpu_acceleration_used: bool,
    pub memory_usage_bytes: usize,
}

#[derive(Debug)]
pub struct CapacityInfo {
    pub total_bits: usize,
    pub recommended_bits: usize,
    pub steganographic_ratio: f64,
    pub format_specific_info: HashMap<String, serde_json::Value>,
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
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SteganoError>;
