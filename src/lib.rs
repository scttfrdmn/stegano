// SPDX-License-Identifier: Apache-2.0
//! # Scientific Data Steganography System
//!
//! A high-performance, multi-format steganography library designed specifically for protecting
//! scientific data including genomics, imagery, and signal data.
//!
//! ## Overview
//!
//! Stegano provides enterprise-grade steganographic embedding capabilities with advanced features:
//!
//! - **Multi-Format Support**: Specialized handlers for FASTA/FASTQ genomics data, TIFF/PNG imagery, and WAV/binary signals
//! - **Advanced Security**: AES-256-GCM / ChaCha20-Poly1305 authenticated encryption with Argon2id key derivation
//! - **Biological Integrity**: FASTQ quality score embedding with preservation of biological data validity
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
//! - [`crypto`]: Cryptographic layer: Argon2id KDF and AES-256-GCM / ChaCha20-Poly1305 AEAD
//! - [`core`]: Core types, traits, and data structures
//! - `ffi`: Foreign Function Interface for C/C++ integration (native targets only)
//!
//! ## Experimental modules
//!
//! The `gpu`, `ml` and `monitoring` modules are compiled only with the
//! off-by-default `experimental` Cargo feature (implied by `gpu`, `metal` and
//! `cuda`). They are not used by the embedding pipeline, are unsupported, and
//! may change or be removed in any release.

pub mod capacity;
pub mod carrier;
pub mod cli;
pub mod codec;
pub mod core;
pub mod crypto;
pub mod embedding;
pub mod error_correction;
#[cfg(not(target_arch = "wasm32"))]
pub mod ffi;
#[cfg(feature = "experimental")]
pub mod gpu;
pub mod handlers;
pub mod integrity;
#[cfg(feature = "experimental")]
pub mod ml;
#[cfg(feature = "experimental")]
pub mod monitoring;
pub(crate) mod time;
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
#[cfg(feature = "experimental")]
pub use gpu::GpuAccelerator;
pub use handlers::DataHandler;
#[cfg(feature = "experimental")]
pub use ml::{MLOptimizer, StrategyRecommendation};
#[cfg(feature = "experimental")]
pub use monitoring::{MonitoringConfig, MonitoringSystem};
pub use validation::{StatisticalValidationSuite, ValidationReport};

// Re-export serde_json to ensure version consistency
pub use serde_json;
