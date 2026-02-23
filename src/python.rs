//! Python bindings for the stegano library via PyO3.
//!
//! ## Build & Install
//!
//! Install [maturin](https://www.maturin.rs/) and run:
//! ```sh
//! pip install maturin
//! maturin develop --features python
//! ```
//! Or build a wheel for distribution:
//! ```sh
//! maturin build --release --features python
//! ```
//!
//! ## Python Usage
//!
//! ```python
//! import stegano
//!
//! # Module-level one-shot functions
//! with open("carrier.png", "rb") as f:
//!     carrier = f.read()
//!
//! modified = stegano.embed(carrier, b"secret payload", password="mykey")
//! payload  = stegano.extract(modified, password="mykey")
//! assert payload == b"secret payload"
//!
//! # Class-based API (reuses the Tokio runtime)
//! s = stegano.Stegano()
//! modified = s.embed(carrier, b"secret", password="mykey", stealth="maximum")
//! payload  = s.extract(modified, password="mykey")
//! info     = s.estimate_capacity(carrier)
//! print(info["total_bits"], info["format"])
//! ```

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use std::collections::HashMap;

use crate::core::{
    CarrierData, CipherKind, DataFormat, ECLevel, EmbedParams, StealthLevel,
};
use crate::embedding::SteganoEmbedder;
use crate::handlers::HandlerRegistry;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Detect DataFormat from raw bytes by asking each handler in order.
/// Falls back to `DataFormat::BINARY` for unknown data.
fn detect_format(data: &[u8]) -> DataFormat {
    let registry = HandlerRegistry::new();
    for handler in registry.get_handlers() {
        if let Some(fmt) = handler.detect_format(data) {
            return fmt;
        }
    }
    DataFormat::BINARY
}

/// Return a static human-readable name for a DataFormat variant.
fn format_name(fmt: &DataFormat) -> &'static str {
    match fmt {
        DataFormat::FASTA => "FASTA",
        DataFormat::FASTQ => "FASTQ",
        DataFormat::VCF => "VCF",
        DataFormat::SAM => "SAM",
        DataFormat::BAM => "BAM",
        DataFormat::PNG => "PNG",
        DataFormat::TIFF => "TIFF",
        DataFormat::MICROSCOPY => "MICROSCOPY",
        DataFormat::DICOM => "DICOM",
        DataFormat::WAV => "WAV",
        DataFormat::BinarySignal => "BinarySignal",
        DataFormat::CsvTimeseries => "CsvTimeseries",
        DataFormat::BINARY => "BINARY",
        DataFormat::TEXT => "TEXT",
        DataFormat::FITS => "FITS",
    }
}

/// Build an [`EmbedParams`] from string options coming from Python.
///
/// Invalid / unknown values silently fall back to sensible defaults:
/// - stealth  → `High`
/// - ec_level → `None`
/// - cipher   → `Aes256Gcm`
fn make_params(password: &str, stealth: &str, ec_level: &str, cipher: &str) -> EmbedParams {
    let key = if password.is_empty() {
        vec![]
    } else {
        password.as_bytes().to_vec()
    };

    let stealth_level = match stealth.to_lowercase().as_str() {
        "low" => StealthLevel::Low,
        "medium" => StealthLevel::Medium,
        "high" => StealthLevel::High,
        "maximum" => StealthLevel::Maximum,
        _ => StealthLevel::High,
    };

    let error_correction_level = match ec_level.to_lowercase().as_str() {
        "none" => ECLevel::None,
        "low" => ECLevel::Low,
        "medium" => ECLevel::Medium,
        "high" => ECLevel::High,
        "maximum" => ECLevel::Maximum,
        _ => ECLevel::None,
    };

    let cipher_kind = match cipher
        .to_lowercase()
        .replace('-', "")
        .replace('_', "")
        .as_str()
    {
        "chacha20" | "chacha20poly1305" => CipherKind::ChaCha20Poly1305,
        _ => CipherKind::Aes256Gcm,
    };

    EmbedParams {
        encryption_key: key,
        error_correction_level,
        stealth_level,
        use_gpu: false,
        enable_validation: false,
        format_specific: HashMap::new(),
        cipher_kind,
    }
}

// ---------------------------------------------------------------------------
// Python class: Stegano
// ---------------------------------------------------------------------------

/// Main stegano class providing embed / extract operations.
///
/// Maintains a reusable Tokio runtime so creating one instance and calling
/// it repeatedly is more efficient than the module-level one-shot functions.
///
/// Example::
///
///     s = stegano.Stegano()
///     carrier = open("carrier.png", "rb").read()
///     modified = s.embed(carrier, b"secret", password="key")
///     payload  = s.extract(modified, password="key")
#[pyclass]
pub struct Stegano {
    embedder: SteganoEmbedder,
    rt: tokio::runtime::Runtime,
}

#[pymethods]
impl Stegano {
    /// Create a new ``Stegano`` instance.
    #[new]
    pub fn new() -> PyResult<Self> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create runtime: {e}")))?;
        Ok(Self {
            embedder: SteganoEmbedder::new(),
            rt,
        })
    }

    /// Embed *payload* into *carrier* bytes.
    ///
    /// Args:
    ///     carrier:  Carrier file bytes (PNG, FASTA, WAV, FITS, …).
    ///     payload:  Secret bytes to embed.
    ///     password: Encryption password.  Empty string disables encryption.
    ///     stealth:  ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
    ///     ec_level: ``"none"`` | ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
    ///     cipher:   ``"aes256gcm"`` (default) | ``"chacha20"``.
    ///
    /// Returns:
    ///     Modified carrier bytes (``bytes``) with the payload embedded.
    #[pyo3(signature = (carrier, payload, password="", stealth="high", ec_level="none", cipher="aes256gcm"))]
    pub fn embed<'py>(
        &self,
        py: Python<'py>,
        carrier: &[u8],
        payload: &[u8],
        password: &str,
        stealth: &str,
        ec_level: &str,
        cipher: &str,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let params = make_params(password, stealth, ec_level, cipher);
        let format = detect_format(carrier);
        let mut carrier_data = CarrierData {
            format,
            raw_data: carrier.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        self.rt
            .block_on(self.embedder.embed(&mut carrier_data, payload, &params))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyBytes::new_bound(py, &carrier_data.raw_data))
    }

    /// Extract the embedded payload from *carrier* bytes.
    ///
    /// Args:
    ///     carrier:  Carrier bytes containing the embedded payload.
    ///     password: Decryption password (must match the one used during embed).
    ///     ec_level: Error-correction level used during embedding.
    ///     cipher:   Cipher hint; auto-detected from embedded header in practice.
    ///
    /// Returns:
    ///     Extracted payload as ``bytes``.
    #[pyo3(signature = (carrier, password="", ec_level="none", cipher="aes256gcm"))]
    pub fn extract<'py>(
        &self,
        py: Python<'py>,
        carrier: &[u8],
        password: &str,
        ec_level: &str,
        cipher: &str,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let params = make_params(password, "high", ec_level, cipher);
        let format = detect_format(carrier);
        let carrier_data = CarrierData {
            format,
            raw_data: carrier.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let result = self
            .rt
            .block_on(self.embedder.extract(&carrier_data, &params))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyBytes::new_bound(py, &result))
    }

    /// Estimate the embedding capacity of *carrier* bytes.
    ///
    /// Returns:
    ///     ``dict`` with keys ``total_bits``, ``recommended_bits``, and ``format``.
    pub fn estimate_capacity<'py>(
        &self,
        py: Python<'py>,
        carrier: &[u8],
    ) -> PyResult<Bound<'py, PyDict>> {
        let format = detect_format(carrier);
        let carrier_data = CarrierData {
            format: format.clone(),
            raw_data: carrier.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let registry = HandlerRegistry::new();
        let handler = registry
            .get_handler(&format)
            .ok_or_else(|| PyRuntimeError::new_err("Unsupported carrier format"))?;

        let cap = handler
            .estimate_capacity(&carrier_data)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        let dict = PyDict::new_bound(py);
        dict.set_item("total_bits", cap.total_bits)?;
        dict.set_item("recommended_bits", cap.recommended_bits)?;
        dict.set_item("format", format_name(&format))?;
        Ok(dict)
    }

    fn __repr__(&self) -> &'static str {
        "Stegano()"
    }
}

// ---------------------------------------------------------------------------
// Module-level convenience functions
// ---------------------------------------------------------------------------

/// Embed *payload* into *carrier* bytes.
///
/// One-shot convenience wrapper around :class:`Stegano`.
///
/// Args:
///     carrier:  Carrier file bytes (PNG, FASTA, WAV, FITS, …).
///     payload:  Secret bytes to embed.
///     password: Encryption password.  Empty string disables encryption.
///     stealth:  ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
///     ec_level: ``"none"`` | ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
///     cipher:   ``"aes256gcm"`` (default) | ``"chacha20"``.
///
/// Returns:
///     Modified carrier bytes (``bytes``) with the payload embedded.
#[pyfunction]
#[pyo3(signature = (carrier, payload, password="", stealth="high", ec_level="none", cipher="aes256gcm"))]
pub fn embed<'py>(
    py: Python<'py>,
    carrier: &[u8],
    payload: &[u8],
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let s = Stegano::new()?;
    s.embed(py, carrier, payload, password, stealth, ec_level, cipher)
}

/// Extract the embedded payload from *carrier* bytes.
///
/// One-shot convenience wrapper around :class:`Stegano`.
///
/// Args:
///     carrier:  Carrier bytes containing the embedded payload.
///     password: Decryption password (empty = no decryption).
///     ec_level: Error-correction level used during embedding.
///     cipher:   Cipher hint; auto-detected from embedded header in practice.
///
/// Returns:
///     Extracted payload as ``bytes``.
#[pyfunction]
#[pyo3(signature = (carrier, password="", ec_level="none", cipher="aes256gcm"))]
pub fn extract<'py>(
    py: Python<'py>,
    carrier: &[u8],
    password: &str,
    ec_level: &str,
    cipher: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let s = Stegano::new()?;
    s.extract(py, carrier, password, ec_level, cipher)
}

/// Estimate the embedding capacity of *carrier* bytes.
///
/// Returns:
///     ``dict`` with keys ``total_bits``, ``recommended_bits``, and ``format``.
#[pyfunction]
pub fn estimate_capacity<'py>(
    py: Python<'py>,
    carrier: &[u8],
) -> PyResult<Bound<'py, PyDict>> {
    let s = Stegano::new()?;
    s.estimate_capacity(py, carrier)
}

/// Return the stegano library version string.
#[pyfunction]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------------------------------------------------------------------------
// Module initialiser — called from the #[pymodule] entry point in lib.rs
// ---------------------------------------------------------------------------

/// Register all Python-visible items into *m*.
pub fn init_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Stegano>()?;
    m.add_function(wrap_pyfunction!(embed, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_capacity, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (run with: cargo test --features python)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal carrier: a repeating byte sequence (BINARY format).
    fn generic_carrier(size: usize) -> Vec<u8> {
        (0u8..=255).cycle().take(size).collect()
    }

    #[test]
    fn test_detect_format_binary_fallback() {
        let data = b"this is definitely not a known scientific format";
        assert_eq!(detect_format(data), DataFormat::BINARY);
    }

    #[test]
    fn test_detect_format_fits() {
        // Minimal FITS: starts with "SIMPLE  " (8 bytes, space-padded keyword)
        let mut fits = b"SIMPLE  =                    T / synthetic".to_vec();
        fits.resize(80, b' ');
        assert_eq!(detect_format(&fits), DataFormat::FITS);
    }

    #[test]
    fn test_detect_format_fasta() {
        let fasta = b">seq1\nACGTACGT\n";
        assert_eq!(detect_format(fasta), DataFormat::FASTA);
    }

    #[test]
    fn test_make_params_defaults() {
        let p = make_params("", "high", "none", "aes256gcm");
        assert!(p.encryption_key.is_empty());
        assert_eq!(p.stealth_level, StealthLevel::High);
        assert_eq!(p.error_correction_level, ECLevel::None);
        assert_eq!(p.cipher_kind, CipherKind::Aes256Gcm);
    }

    #[test]
    fn test_make_params_chacha20() {
        let p = make_params("mypassword", "maximum", "medium", "chacha20");
        assert_eq!(p.encryption_key, b"mypassword".to_vec());
        assert_eq!(p.stealth_level, StealthLevel::Maximum);
        assert_eq!(p.error_correction_level, ECLevel::Medium);
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_make_params_chacha20_hyphenated() {
        // Hyphen/underscore normalised
        let p = make_params("k", "low", "low", "chacha20-poly1305");
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_format_name_all_variants() {
        for fmt in [
            DataFormat::FASTA,
            DataFormat::FASTQ,
            DataFormat::VCF,
            DataFormat::SAM,
            DataFormat::BAM,
            DataFormat::PNG,
            DataFormat::TIFF,
            DataFormat::MICROSCOPY,
            DataFormat::DICOM,
            DataFormat::WAV,
            DataFormat::BinarySignal,
            DataFormat::CsvTimeseries,
            DataFormat::BINARY,
            DataFormat::TEXT,
            DataFormat::FITS,
        ] {
            assert!(!format_name(&fmt).is_empty());
        }
    }

    /// Test the embed/extract roundtrip using the same logic the Python
    /// bindings invoke, but without needing the PyO3 GIL.
    #[tokio::test]
    async fn test_binding_embed_extract_roundtrip() {
        let carrier_bytes = generic_carrier(4096);
        let payload = b"Python binding roundtrip test payload";
        let params = make_params("", "high", "none", "aes256gcm");
        let format = detect_format(&carrier_bytes);

        let mut carrier_data = CarrierData {
            format,
            raw_data: carrier_bytes,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let embedder = SteganoEmbedder::new();
        embedder
            .embed(&mut carrier_data, payload, &params)
            .await
            .expect("embed should succeed");

        let extracted = embedder
            .extract(&carrier_data, &params)
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }

    /// Test roundtrip with encryption (AES-256-GCM).
    #[tokio::test]
    async fn test_binding_embed_extract_encrypted() {
        let carrier_bytes = generic_carrier(8192);
        let payload = b"Encrypted Python binding test";
        let params = make_params("super_secret", "high", "none", "aes256gcm");
        let format = detect_format(&carrier_bytes);

        let mut carrier_data = CarrierData {
            format,
            raw_data: carrier_bytes,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let embedder = SteganoEmbedder::new();
        embedder
            .embed(&mut carrier_data, payload, &params)
            .await
            .expect("embed should succeed");

        let extracted = embedder
            .extract(&carrier_data, &params)
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }

    /// Test roundtrip with ChaCha20-Poly1305.
    #[tokio::test]
    async fn test_binding_embed_extract_chacha20() {
        let carrier_bytes = generic_carrier(8192);
        let payload = b"ChaCha20 Python binding test";
        let params = make_params("chacha_key", "high", "none", "chacha20");
        let format = detect_format(&carrier_bytes);

        let mut carrier_data = CarrierData {
            format,
            raw_data: carrier_bytes,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let embedder = SteganoEmbedder::new();
        embedder
            .embed(&mut carrier_data, payload, &params)
            .await
            .expect("embed should succeed");

        let extracted = embedder
            .extract(&carrier_data, &params)
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }
}
