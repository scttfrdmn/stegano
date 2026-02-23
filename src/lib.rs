//! # Scientific Data Steganography System
//!
//! Copyright (c) 2025 Scott Friedman. All rights reserved.
//! This software is proprietary and confidential.
//!
//! A high-performance, multi-format steganography library designed specifically for protecting
//! scientific data including genomics, imagery, and signal data.
//!
//! ## Overview
//!
//! Stegano provides enterprise-grade steganographic embedding capabilities with advanced features:
//!
//! - **Multi-Format Support**: Specialized handlers for FASTA/FASTQ genomics data, TIFF/PNG imagery, and WAV/binary signals
//! - **Advanced Security**: AES-256-GCM encryption with Blake3 key derivation and authenticated encryption
//! - **GPU Acceleration**: WGPU-based compute shaders with Metal backend optimization for Apple Silicon
//! - **Biological Integrity**: FASTQ quality score embedding with preservation of biological data validity
//! - **Production Monitoring**: Comprehensive health checks, metrics, and telemetry for enterprise deployment
//! - **ML Optimization**: Neural networks and genetic algorithms for adaptive embedding strategies
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use stegano::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Create carrier data
//!     let mut carrier = CarrierData::new(
//!         DataFormat::BINARY,
//!         std::fs::read("data.bin")?
//!     );
//!
//!     // Setup embedding parameters with validation
//!     let params = EmbedParams::new(b"secure_password".to_vec())
//!         .with_stealth_level(StealthLevel::High)
//!         .with_validation();
//!
//!     // Create embedder and embed data with integrated validation
//!     let embedder = SteganoEmbedder::new();
//!     let payload = b"Secret research data";
//!     let result = embedder.embed(&mut carrier, payload, &params).await?;
//!     
//!     println!("Embedded {} bytes", result.bytes_embedded);
//!     if let Some(report) = result.validation_report {
//!         println!("Validation score: {}/10", report.overall_score());
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Core Modules
//!
//! - [`handlers`]: Format-specific embedding and extraction implementations
//! - [`crypto`]: Cryptographic layer with AES-GCM and Blake3
//! - [`gpu`]: GPU acceleration using WGPU compute shaders
//! - [`core`]: Core types, traits, and data structures
//! - [`ffi`]: Foreign Function Interface for C/C++ integration

pub mod capacity;
pub mod cli;
pub mod codec;
pub mod core;
pub mod crypto;
pub mod embedding;
pub mod error_correction;
pub mod ffi;
pub mod gpu;
pub mod handlers;
pub mod integrity;
pub mod ml;
pub mod monitoring;
pub mod validation;

#[cfg(feature = "python")]
pub mod python;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "python")]
use pyo3::prelude::*;

/// Python extension module entry point.
///
/// Activated only when compiled with `--features python` (e.g. via maturin).
#[cfg(feature = "python")]
#[pymodule]
fn stegano(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::init_module(m)
}

pub use core::*;

// Re-export main API types
pub use capacity::CapacityEstimator;
pub use crypto::{CryptoLayer, SecurePayload};
pub use embedding::SteganoEmbedder;
pub use error_correction::IntegritySystem;
pub use gpu::GpuAccelerator;
pub use handlers::DataHandler;
pub use ml::{MLOptimizer, StrategyRecommendation};
pub use monitoring::{MonitoringConfig, MonitoringSystem};
pub use validation::{StatisticalValidationSuite, ValidationReport};

// Re-export serde_json to ensure version consistency
pub use serde_json;
