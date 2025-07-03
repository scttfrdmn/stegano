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
//! ```rust
//! use stegano::*;
//! use std::collections::HashMap;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Create carrier data
//!     let mut carrier = CarrierData::new(
//!         DataFormat::BINARY, 
//!         std::fs::read("data.bin")?
//!     );
//!
//!     // Setup embedding parameters
//!     let params = EmbedParams::new(b"secure_password".to_vec())
//!         .with_stealth_level(StealthLevel::High);
//!
//!     // Get appropriate handler and embed data
//!     let registry = handlers::HandlerRegistry::new();
//!     let handler = registry.get_handler(&carrier.format).unwrap();
//!     
//!     let payload = b"Secret research data";
//!     let result = handler.embed(&mut carrier, payload, &params).await?;
//!     println!("Embedded {} bytes", result.bytes_embedded);
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

pub mod handlers;
pub mod crypto;
pub mod gpu;
pub mod ml;
pub mod error_correction;
pub mod capacity;
pub mod integrity;
pub mod core;
pub mod cli;
pub mod ffi;
pub mod monitoring;

pub use core::*;

// Re-export main API types
pub use handlers::DataHandler;
pub use crypto::{CryptoLayer, SecurePayload};
pub use gpu::GpuAccelerator;
pub use ml::{MLOptimizer, StrategyRecommendation};
pub use error_correction::IntegritySystem;
pub use capacity::CapacityEstimator;
pub use monitoring::{MonitoringSystem, MonitoringConfig};
