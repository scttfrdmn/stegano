//! WebAssembly bindings for the stegano library via wasm-bindgen.
//!
//! ## Build
//!
//! Install [wasm-pack](https://rustwasm.github.io/wasm-pack/) and run:
//! ```sh
//! cargo install wasm-pack
//! ./scripts/build-wasm.sh   # assembles the stegano-wasm npm package in pkg/
//! # Build an ES module bundle for modern browsers:
//! wasm-pack build --target web --no-default-features --features wasm
//! # Build for Node.js:
//! wasm-pack build --target nodejs --no-default-features --features wasm
//! ```
//!
//! ## JavaScript / TypeScript Usage
//!
//! ```javascript
//! import init, {
//!     stegano_embed,
//!     stegano_extract,
//!     stegano_estimate_capacity,
//!     stegano_version,
//! } from 'stegano-wasm'; // npm install stegano-wasm (see npm/README.md)
//!
//! await init();
//!
//! // Embed a secret message into a PNG carrier
//! const carrier = new Uint8Array(await fetch('carrier.png').then(r => r.arrayBuffer()));
//! const payload = new TextEncoder().encode('secret message');
//!
//! const modified = await stegano_embed(carrier, payload, 'password', 'high', 'none', 'aes256gcm');
//! const extracted = await stegano_extract(modified, 'password', 'none', 'aes256gcm');
//! console.log(new TextDecoder().decode(extracted)); // 'secret message'
//!
//! // Inspect carrier capacity
//! const info = stegano_estimate_capacity(carrier);
//! console.log(`Format: ${info.format()}, Total bits: ${info.total_bits}`);
//!
//! console.log(stegano_version()); // crate version, e.g. '0.7.0'
//! ```

#[cfg(any(target_arch = "wasm32", test))]
use crate::carrier::{self, carrier_from_content};
#[cfg(any(target_arch = "wasm32", test))]
use crate::core::{DataFormat, EmbedParams};
#[cfg(any(target_arch = "wasm32", test))]
use crate::embedding::SteganoEmbedder;
#[cfg(target_arch = "wasm32")]
use crate::handlers::HandlerRegistry;

// ---------------------------------------------------------------------------
// Internal helpers — compiled for wasm32 and for test builds
// ---------------------------------------------------------------------------

/// Detect DataFormat from raw bytes (shared detection, see [`crate::carrier`]).
#[cfg(test)]
fn detect_format(data: &[u8]) -> DataFormat {
    carrier::detect_format_from_content(data)
}

/// Return a static human-readable name for a DataFormat variant.
#[cfg(any(target_arch = "wasm32", test))]
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

/// Build an [`EmbedParams`] from string options provided by JavaScript.
///
/// Empty strings select the shared front-end defaults (see
/// [`crate::carrier`]); unknown names are rejected.
#[cfg(any(target_arch = "wasm32", test))]
fn make_params(
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> anyhow::Result<EmbedParams> {
    fn or_default<'a>(s: &'a str, default: &'a str) -> &'a str {
        if s.trim().is_empty() {
            default
        } else {
            s
        }
    }
    carrier::params_from_names(
        password,
        or_default(stealth, carrier::DEFAULT_STEALTH_NAME),
        or_default(ec_level, carrier::DEFAULT_EC_NAME),
        or_default(cipher, carrier::DEFAULT_CIPHER_NAME),
    )
    .map_err(anyhow::Error::msg)
}

/// Embed `payload` into `carrier` — shared async implementation used by both
/// the WASM exports and the native unit tests. The result may be longer than
/// `carrier` (SAM, VCF, DICOM).
#[cfg(any(target_arch = "wasm32", test))]
async fn embed_internal(
    carrier: &[u8],
    payload: &[u8],
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> anyhow::Result<Vec<u8>> {
    let params = make_params(password, stealth, ec_level, cipher)?;
    let mut carrier_data = carrier_from_content(carrier.to_vec());
    let embedder = SteganoEmbedder::new();
    embedder.embed(&mut carrier_data, payload, &params).await?;
    Ok(carrier_data.raw_data)
}

/// Extract the embedded payload from `carrier` — shared async implementation.
#[cfg(any(target_arch = "wasm32", test))]
async fn extract_internal(
    carrier: &[u8],
    password: &str,
    ec_level: &str,
    cipher: &str,
) -> anyhow::Result<Vec<u8>> {
    let params = make_params(password, carrier::DEFAULT_STEALTH_NAME, ec_level, cipher)?;
    let carrier_data = carrier_from_content(carrier.to_vec());
    let embedder = SteganoEmbedder::new();
    Ok(embedder.extract(&carrier_data, &params).await?)
}

// ---------------------------------------------------------------------------
// WASM exports — only compiled when targeting wasm32
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsError;

/// Embedding capacity information returned by [`stegano_estimate_capacity`].
///
/// Access via the public fields `total_bits` and `recommended_bits`, and the
/// `format()` getter method.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WasmCapacityInfo {
    /// Total bits available in the carrier for steganographic embedding.
    pub total_bits: usize,
    /// Recommended maximum payload size in bits (conservative estimate).
    pub recommended_bits: usize,
    format_str: String,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WasmCapacityInfo {
    /// Human-readable format name of the detected carrier
    /// (e.g. `"PNG"`, `"FASTA"`, `"WAV"`, `"BINARY"`).
    pub fn format(&self) -> String {
        self.format_str.clone()
    }
}

/// Embed *payload* into *carrier* bytes and return the modified carrier.
///
/// Returns a `Promise<Uint8Array>` that resolves with the stego-carrier on
/// success, or rejects with a JavaScript `Error` on failure.
///
/// # Arguments
/// - `carrier`  — Carrier file bytes (PNG, FASTA, WAV, FITS, DICOM, …).
/// - `payload`  — Secret bytes to embed.
/// - `password` — Encryption password. An empty string disables encryption:
///   the payload is then stored in plaintext.
/// - `stealth`  — `"low"` | `"medium"` | `"high"` | `"maximum"` (`""` = shared default, `"high"`).
/// - `ec_level` — `"none"` | `"low"` | `"medium"` | `"high"` | `"maximum"` (`""` = shared default, `"none"`).
/// - `cipher`   — `"aes256gcm"` (default) | `"chacha20"`.
///
/// The result may be longer than `carrier` (SAM, VCF, DICOM). Unknown
/// option names reject the promise.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn stegano_embed(
    carrier: &[u8],
    payload: &[u8],
    password: &str,
    stealth: &str,
    ec_level: &str,
    cipher: &str,
) -> Result<Uint8Array, JsError> {
    let result = embed_internal(carrier, payload, password, stealth, ec_level, cipher)
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Uint8Array::from(result.as_slice()))
}

/// Extract the embedded payload from *carrier* bytes.
///
/// Returns a `Promise<Uint8Array>` that resolves with the extracted payload on
/// success, or rejects with a JavaScript `Error` on failure.
///
/// # Arguments
/// - `carrier`  — Carrier bytes that contain an embedded payload.
/// - `password` — Decryption password (must match the one used during embed).
/// - `ec_level` — Only matters without a password: use any non-`"none"` value
///   if EC was used at embed time (the level itself is read from the payload).
///   `""` = shared default.
/// - `cipher`   — Cipher hint; the header is usually sufficient for auto-detection.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn stegano_extract(
    carrier: &[u8],
    password: &str,
    ec_level: &str,
    cipher: &str,
) -> Result<Uint8Array, JsError> {
    let result = extract_internal(carrier, password, ec_level, cipher)
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Uint8Array::from(result.as_slice()))
}

/// Estimate the steganographic capacity of *carrier* bytes.
///
/// Returns a [`WasmCapacityInfo`] object synchronously — no `await` needed.
///
/// # Arguments
/// - `carrier` — Carrier file bytes to inspect.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn stegano_estimate_capacity(carrier: &[u8]) -> Result<WasmCapacityInfo, JsError> {
    let carrier_data = carrier_from_content(carrier.to_vec());

    let registry = HandlerRegistry::new();
    let handler = registry
        .get_handler(&carrier_data.format)
        .ok_or_else(|| JsError::new("Unsupported carrier format"))?;

    let cap = handler
        .estimate_capacity(&carrier_data)
        .map_err(|e| JsError::new(&e.to_string()))?;

    Ok(WasmCapacityInfo {
        total_bits: cap.total_bits,
        recommended_bits: cap.recommended_bits,
        format_str: format_name(&carrier_data.format).to_string(),
    })
}

/// Return the stegano library version string (the crate version).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn stegano_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// Tests — run with: cargo test --features wasm --lib
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Generic binary carrier of a given size.
    fn generic_carrier(size: usize) -> Vec<u8> {
        (0u8..=255).cycle().take(size).collect()
    }

    // --- format detection ---

    #[test]
    fn test_detect_format_binary_fallback() {
        let data = b"this is definitely not a known scientific format";
        assert_eq!(detect_format(data), DataFormat::BINARY);
    }

    #[test]
    fn test_detect_format_fasta() {
        let fasta = b">seq1\nACGTACGT\n";
        assert_eq!(detect_format(fasta), DataFormat::FASTA);
    }

    #[test]
    fn test_detect_format_fits() {
        let mut fits = b"SIMPLE  =                    T / synthetic".to_vec();
        fits.resize(80, b' ');
        assert_eq!(detect_format(&fits), DataFormat::FITS);
    }

    // --- make_params ---

    use crate::core::{CipherKind, ECLevel, StealthLevel};

    #[test]
    fn test_make_params_defaults() {
        let p = make_params("", "high", "none", "aes256gcm").unwrap();
        assert!(p.encryption_key.is_empty());
        assert_eq!(p.stealth_level, StealthLevel::High);
        assert_eq!(p.error_correction_level, ECLevel::None);
        assert_eq!(p.cipher_kind, CipherKind::Aes256Gcm);
        // Empty strings select the shared defaults.
        let d = make_params("", "", "", "").unwrap();
        assert_eq!(d.stealth_level, carrier::DEFAULT_STEALTH_LEVEL);
        assert_eq!(d.error_correction_level, carrier::DEFAULT_EC_LEVEL);
        assert_eq!(d.cipher_kind, carrier::DEFAULT_CIPHER_KIND);
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
        let p = make_params("k", "low", "low", "chacha20-poly1305").unwrap();
        assert_eq!(p.cipher_kind, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_make_params_unknown_values_rejected() {
        assert!(make_params("", "ultra", "none", "aes256gcm").is_err());
        assert!(make_params("", "high", "extreme", "aes256gcm").is_err());
        assert!(make_params("", "high", "none", "xorcrypt").is_err());
    }

    // --- format_name ---

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

    // --- embed/extract roundtrips via internal helpers ---

    #[tokio::test]
    async fn test_wasm_embed_extract_roundtrip() {
        let carrier = generic_carrier(4096);
        let payload = b"WASM binding roundtrip test";

        let modified = embed_internal(&carrier, payload, "", "high", "none", "aes256gcm")
            .await
            .expect("embed should succeed");

        let extracted = extract_internal(&modified, "", "none", "aes256gcm")
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }

    #[tokio::test]
    async fn test_wasm_embed_extract_encrypted() {
        let carrier = generic_carrier(8192);
        let payload = b"Encrypted WASM binding test";

        let modified = embed_internal(
            &carrier,
            payload,
            "wasm_secret",
            "high",
            "none",
            "aes256gcm",
        )
        .await
        .expect("embed should succeed");

        let extracted = extract_internal(&modified, "wasm_secret", "none", "aes256gcm")
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }

    #[tokio::test]
    async fn test_wasm_embed_extract_chacha20() {
        let carrier = generic_carrier(8192);
        let payload = b"ChaCha20 WASM binding test";

        let modified = embed_internal(&carrier, payload, "chacha_key", "high", "none", "chacha20")
            .await
            .expect("embed should succeed");

        let extracted = extract_internal(&modified, "chacha_key", "none", "chacha20")
            .await
            .expect("extract should succeed");

        assert_eq!(extracted, payload.to_vec());
    }

    #[tokio::test]
    async fn test_wasm_embed_extract_stealth_levels() {
        let carrier = generic_carrier(8192);
        let payload = b"stealth level test";

        for stealth in &["low", "medium", "high", "maximum"] {
            let modified = embed_internal(&carrier, payload, "", stealth, "none", "aes256gcm")
                .await
                .unwrap_or_else(|e| panic!("embed failed for stealth={stealth}: {e}"));

            let extracted = extract_internal(&modified, "", "none", "aes256gcm")
                .await
                .unwrap_or_else(|e| panic!("extract failed for stealth={stealth}: {e}"));

            assert_eq!(
                extracted,
                payload.to_vec(),
                "roundtrip failed for stealth={stealth}"
            );
        }
    }
}
