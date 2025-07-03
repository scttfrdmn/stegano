//! Scientific Data Steganography System
//! 
//! Copyright (c) 2025 Scott Friedman. All rights reserved.
//! This software is proprietary and confidential.
//! 
//! High-performance, multi-format steganography library for protecting
//! scientific data including genomics, imagery, and signal data.

pub mod handlers;
pub mod crypto;
pub mod gpu;
pub mod error_correction;
pub mod capacity;
pub mod integrity;
pub mod core;

pub use core::*;

// Re-export main API types
pub use handlers::DataHandler;
pub use crypto::{CryptoLayer, SecurePayload};
pub use gpu::GpuAccelerator;
pub use error_correction::IntegritySystem;
pub use capacity::CapacityEstimator;
