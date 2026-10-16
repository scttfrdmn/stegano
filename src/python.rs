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
//! # Class-based API
//! s = stegano.Stegano()
//! modified = s.embed(carrier, b"secret", password="mykey", stealth="maximum")
//! payload  = s.extract(modified, password="mykey")
//! info     = s.estimate_capacity(carrier)
//! print(info["total_bits"], info["format"])
//! ```
//!
//! Defaults (`stealth`, `ec_level`, `cipher`) are the shared front-end
//! defaults from [`crate::carrier`], identical to the CLI, C FFI, Go and
//! WASM bindings. All calls share one Tokio runtime and release the GIL
//! while embedding/extracting.

// pyo3 0.22's #[pyfunction]/#[pymethods] expansion trips this lint on every
// `PyResult` return with recent clippy; it is not in our code.
#![allow(clippy::useless_conversion)]

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use std::sync::OnceLock;

use crate::carrier::{
    self, carrier_from_content, DEFAULT_CIPHER_NAME, DEFAULT_EC_NAME, DEFAULT_STEALTH_NAME,
};
#[cfg(test)]
use crate::core::{CipherKind, ECLevel, StealthLevel};
use crate::core::{DataFormat, EmbedParams};
use crate::embedding::SteganoEmbedder;
use crate::handlers::HandlerRegistry;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// One Tokio runtime shared by every Python call.
static RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> = OnceLock::new();

fn runtime() -> PyResult<&'static tokio::runtime::Runtime> {
    RUNTIME
        .get_or_init(|| tokio::runtime::Runtime::new().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to create runtime: {e}")))
}

/// Detect DataFormat from raw bytes (shared detection, see [`crate::carrier`]).
#[cfg(test)]
fn detect_format(data: &[u8]) -> DataFormat {
    carrier::detect_format_from_content(data)
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
/// Unknown names raise `ValueError`.
fn make_params(
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> std::result::Result<EmbedParams, String> {
    carrier::params_from_names(password, stealth, ec_level, cipher)
}

fn embed_bytes(
    carrier: Vec<u8>,
    payload: &[u8],
    params: &EmbedParams,
) -> crate::core::Result<Vec<u8>> {
    let rt = tokio_runtime_for_worker()?;
    let mut carrier_data = carrier_from_content(carrier);
    rt.block_on(SteganoEmbedder::new().embed(&mut carrier_data, payload, params))?;
    Ok(carrier_data.raw_data)
}

fn extract_bytes(carrier: Vec<u8>, params: &EmbedParams) -> crate::core::Result<Vec<u8>> {
    let rt = tokio_runtime_for_worker()?;
    let carrier_data = carrier_from_content(carrier);
    rt.block_on(SteganoEmbedder::new().extract(&carrier_data, params))
}

/// The shared runtime, as a library error (used inside `allow_threads`).
fn tokio_runtime_for_worker() -> crate::core::Result<&'static tokio::runtime::Runtime> {
    RUNTIME
        .get_or_init(|| tokio::runtime::Runtime::new().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| crate::core::SteganoError::InvalidParameter(format!("runtime: {e}")))
}

// ---------------------------------------------------------------------------
// Python class: Stegano
// ---------------------------------------------------------------------------

/// Main stegano class providing embed / extract operations.
///
/// All instances share one Tokio runtime; the class exists for API
/// compatibility and convenience.
///
/// Example::
///
///     s = stegano.Stegano()
///     carrier = open("carrier.png", "rb").read()
///     modified = s.embed(carrier, b"secret", password="key")
///     payload  = s.extract(modified, password="key")
#[pyclass]
pub struct Stegano;

#[pymethods]
impl Stegano {
    /// Create a new ``Stegano`` instance.
    #[new]
    pub fn new() -> PyResult<Self> {
        runtime()?;
        Ok(Self)
    }

    /// Embed *payload* into *carrier* bytes.
    ///
    /// Args:
    ///     carrier:  Carrier file bytes (PNG, FASTA, WAV, FITS, …).
    ///     payload:  Secret bytes to embed.
    ///     password: Encryption password.  Empty string disables encryption.
    ///     stealth:  ``"low"`` | ``"medium"`` | ``"high"`` (default) | ``"maximum"``.
    ///     ec_level: ``"none"`` (default) | ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
    ///     cipher:   ``"aes256gcm"`` (default) | ``"chacha20"``.
    ///
    /// Returns:
    ///     Modified carrier bytes (``bytes``) with the payload embedded.
    ///     The result may be longer than *carrier* (SAM, VCF, DICOM).
    #[pyo3(signature = (carrier, payload, password="", stealth=DEFAULT_STEALTH_NAME, ec_level=DEFAULT_EC_NAME, cipher=DEFAULT_CIPHER_NAME))]
    #[allow(clippy::too_many_arguments)]
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
        embed(py, carrier, payload, password, stealth, ec_level, cipher)
    }

    /// Extract the embedded payload from *carrier* bytes.
    ///
    /// Args:
    ///     carrier:  Carrier bytes containing the embedded payload.
    ///     password: Decryption password (must match the one used during embed).
    ///     ec_level: Use any non-``"none"`` value if EC was used at embed time
    ///               without a password (the actual level is read from the payload).
    ///     cipher:   Cipher hint; auto-detected from embedded header in practice.
    ///
    /// Returns:
    ///     Extracted payload as ``bytes``.
    #[pyo3(signature = (carrier, password="", ec_level=DEFAULT_EC_NAME, cipher=DEFAULT_CIPHER_NAME))]
    pub fn extract<'py>(
        &self,
        py: Python<'py>,
        carrier: &[u8],
        password: &str,
        ec_level: &str,
        cipher: &str,
    ) -> PyResult<Bound<'py, PyBytes>> {
        extract(py, carrier, password, ec_level, cipher)
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
        estimate_capacity(py, carrier)
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
/// Args:
///     carrier:  Carrier file bytes (PNG, FASTA, WAV, FITS, …).
///     payload:  Secret bytes to embed.
///     password: Encryption password.  Empty string disables encryption.
///     stealth:  ``"low"`` | ``"medium"`` | ``"high"`` (default) | ``"maximum"``.
///     ec_level: ``"none"`` (default) | ``"low"`` | ``"medium"`` | ``"high"`` | ``"maximum"``.
///     cipher:   ``"aes256gcm"`` (default) | ``"chacha20"``.
///
/// Returns:
///     Modified carrier bytes (``bytes``) with the payload embedded.
#[pyfunction]
#[pyo3(signature = (carrier, payload, password="", stealth=DEFAULT_STEALTH_NAME, ec_level=DEFAULT_EC_NAME, cipher=DEFAULT_CIPHER_NAME))]
pub fn embed<'py>(
    py: Python<'py>,
    carrier: &[u8],
    payload: &[u8],
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let params = make_params(password, stealth, ec_level, cipher).map_err(PyValueError::new_err)?;
    runtime()?;
    let carrier = carrier.to_vec();
    let payload = payload.to_vec();
    let out = py
        .allow_threads(move || embed_bytes(carrier, &payload, &params))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(PyBytes::new_bound(py, &out))
}

/// Extract the embedded payload from *carrier* bytes.
///
/// Args:
///     carrier:  Carrier bytes containing the embedded payload.
///     password: Decryption password (empty = no decryption).
///     ec_level: Use any non-``"none"`` value if EC was used at embed time
///               without a password.
///     cipher:   Cipher hint; auto-detected from embedded header in practice.
///
/// Returns:
///     Extracted payload as ``bytes``.
#[pyfunction]
#[pyo3(signature = (carrier, password="", ec_level=DEFAULT_EC_NAME, cipher=DEFAULT_CIPHER_NAME))]
pub fn extract<'py>(
    py: Python<'py>,
    carrier: &[u8],
    password: &str,
    ec_level: &str,
    cipher: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let params = make_params(password, DEFAULT_STEALTH_NAME, ec_level, cipher)
        .map_err(PyValueError::new_err)?;
    runtime()?;
    let carrier = carrier.to_vec();
    let out = py
        .allow_threads(move || extract_bytes(carrier, &params))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(PyBytes::new_bound(py, &out))
}

/// Estimate the embedding capacity of *carrier* bytes.
///
/// Returns:
///     ``dict`` with keys ``total_bits``, ``recommended_bits``, and ``format``.
#[pyfunction]
pub fn estimate_capacity<'py>(py: Python<'py>, carrier: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let carrier_data = carrier_from_content(carrier.to_vec());
    let registry = HandlerRegistry::new();
    let handler = registry
        .get_handler(&carrier_data.format)
        .ok_or_else(|| PyRuntimeError::new_err("Unsupported carrier format"))?;

    let cap = handler
        .estimate_capacity(&carrier_data)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let dict = PyDict::new_bound(py);
    dict.set_item("total_bits", cap.total_bits)?;
    dict.set_item("recommended_bits", cap.recommended_bits)?;
    dict.set_item("format", format_name(&carrier_data.format))?;
    Ok(dict)
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
        let p = make_params(
            "",
            DEFAULT_STEALTH_NAME,
            DEFAULT_EC_NAME,
            DEFAULT_CIPHER_NAME,
        )
        .unwrap();
        assert!(p.encryption_key.is_empty());
        assert_eq!(p.stealth_level, StealthLevel::High);
        assert_eq!(p.error_correction_level, ECLevel::None);
        assert_eq!(p.cipher_kind, CipherKind::Aes256Gcm);
    }

    #[test]
    fn test_make_params_chacha20() {
        let p = make_params("mypassword", "maximum", "medium", "chacha20").unwrap();
        assert_eq!(p.encryption_key, b"mypassword".to_vec());
        assert_eq!(p.stealth_level, StealthLevel::Maximum);
        assert_eq!(p.error_correction_level, ECLevel::Medium);
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_make_params_chacha20_hyphenated() {
        // Hyphen/underscore normalised
        let p = make_params("k", "low", "low", "chacha20-poly1305").unwrap();
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_make_params_unknown_values_rejected() {
        assert!(make_params("", "ultra", "none", "aes256gcm").is_err());
        assert!(make_params("", "high", "extreme", "aes256gcm").is_err());
        assert!(make_params("", "high", "none", "xorcrypt").is_err());
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

    /// Roundtrips through the same helpers the Python functions call
    /// (inside `allow_threads`), without needing the GIL.
    #[test]
    fn test_binding_embed_extract_roundtrip() {
        let payload = b"Python binding roundtrip test payload";
        let params = make_params("", "high", "none", "aes256gcm").unwrap();
        let out = embed_bytes(generic_carrier(4096), payload, &params).expect("embed");
        let extracted = extract_bytes(out, &params).expect("extract");
        assert_eq!(extracted, payload.to_vec());
    }

    /// Test roundtrip with encryption (AES-256-GCM).
    #[test]
    fn test_binding_embed_extract_encrypted() {
        let payload = b"Encrypted Python binding test";
        let params = make_params("super_secret", "high", "none", "aes256gcm").unwrap();
        let out = embed_bytes(generic_carrier(8192), payload, &params).expect("embed");
        let extracted = extract_bytes(out, &params).expect("extract");
        assert_eq!(extracted, payload.to_vec());
    }

    /// Test roundtrip with ChaCha20-Poly1305.
    #[test]
    fn test_binding_embed_extract_chacha20() {
        let payload = b"ChaCha20 Python binding test";
        let params = make_params("chacha_key", "high", "none", "chacha20").unwrap();
        let out = embed_bytes(generic_carrier(8192), payload, &params).expect("embed");
        let extracted = extract_bytes(out, &params).expect("extract");
        assert_eq!(extracted, payload.to_vec());
    }
}
