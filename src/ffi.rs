// SPDX-License-Identifier: Apache-2.0
//! Foreign Function Interface (FFI) for C/C++ (and Go via cgo) integration.
//!
//! The C header for this module is generated with cbindgen and checked in at
//! `include/stegano.h`; regenerate it with `cbindgen --config cbindgen.toml
//! --output include/stegano.h src/ffi.rs` after changing any signature.
//!
//! # Conventions
//!
//! * Every function returns a [`SteganoErrorCode`]; `0` is success.
//! * Formats are passed as plain `uint32_t` values (see [`CDataFormat`] for
//!   the numbering). Unknown values return `InvalidParameter` instead of
//!   being reinterpreted as a Rust enum.
//! * A null or empty `password` means no encryption: the payload is then
//!   stored in plaintext inside the carrier.
//! * Buffers returned through `uint8_t **` out-parameters are allocated by
//!   the library and must be released with [`stegano_free`].
//! * Defaults (stealth level, error-correction level, cipher) are the shared
//!   front-end defaults in [`crate::carrier`]; the `*_v2` functions accept a
//!   JSON object that can override them (`"stealth"`, `"ec_level"`,
//!   `"cipher"`) and pass format-specific keys through to the handlers
//!   (e.g. `"sam_strategy"`).
//!
//! * `stegano_embed_file` / `stegano_extract_file` work on file paths and
//!   have no 32-bit size limit: streamable formats/strategies (see
//!   [`crate::stream::is_streamable`]) are streamed with constant memory,
//!   anything else is read into memory. Embedded output is written
//!   atomically (temporary file + rename).
//!
//! # Panics
//!
//! Every exported function body runs inside [`std::panic::catch_unwind`]; a
//! panic is reported as [`SteganoErrorCode::Panic`] instead of unwinding into
//! the foreign caller (which is undefined behaviour). This requires
//! `panic = "unwind"`, which the crate's `release` profile sets. If you build
//! the library with `panic = "abort"` yourself, a panic aborts the host
//! process instead. Inputs are validated up front either way, so malformed
//! arguments are reported as error codes rather than reaching a panic.
//!
//! # Async runtime
//!
//! The library keeps one shared Tokio runtime. If an FFI function is called
//! from a thread that is already inside a Tokio runtime (where `block_on`
//! would panic), the work is run on a short-lived helper thread instead.

use crate::carrier::{self, carrier_from_bytes};
use crate::cli::files;
use crate::core::*;
use crate::handlers::HandlerRegistry;
use std::collections::HashMap;
use std::ffi::CStr;
use std::future::Future;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::sync::OnceLock;

/// Single shared Tokio runtime — avoids creating a new runtime per FFI call.
static TOKIO_RT: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();

fn ffi_runtime() -> FfiResult<&'static tokio::runtime::Runtime> {
    TOKIO_RT
        .get_or_init(|| tokio::runtime::Runtime::new().ok())
        .as_ref()
        .ok_or(SteganoErrorCode::Unknown)
}

/// Run `fut` to completion on the shared runtime.
///
/// If the calling thread is already inside a Tokio runtime, `block_on` would
/// panic, so the future is driven from a scoped helper thread instead.
fn run_blocking<F>(fut: F) -> FfiResult<F::Output>
where
    F: Future + Send,
    F::Output: Send,
{
    let rt = ffi_runtime()?;
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|s| s.spawn(|| rt.block_on(fut)).join())
            .map_err(|_| SteganoErrorCode::Panic)
    } else {
        Ok(rt.block_on(fut))
    }
}

/// Run an FFI body, converting a panic into [`SteganoErrorCode::Panic`].
fn guard<F: FnOnce() -> SteganoErrorCode>(f: F) -> SteganoErrorCode {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(SteganoErrorCode::Panic)
}

/// C-compatible error codes returned by every exported function.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteganoErrorCode {
    /// The call succeeded.
    Success = 0,
    /// A null pointer, empty buffer, unknown format number, invalid UTF-8,
    /// malformed JSON options, an unknown option value, or a format/strategy
    /// combination the handler does not accept.
    InvalidParameter = 1,
    /// The carrier is too small for the encoded payload.
    InsufficientCapacity = 2,
    /// Encryption or decryption failed: wrong password, a password given for
    /// an unencrypted payload (or none for an encrypted one), or the payload
    /// or its header was modified (AEAD authentication failure).
    CryptographicFailure = 3,
    /// The handler cannot process this carrier (unsupported layout or
    /// format).
    FormatNotSupported = 4,
    /// No valid stegano payload was found, or it is damaged beyond what the
    /// error-correction level can repair.
    DataCorrupted = 5,
    /// Reserved. Maps `SteganoError::GpuError`, which only the experimental
    /// GPU modules produce; the embedding functions exported here do not
    /// use the GPU and do not return it.
    GpuError = 6,
    /// A file could not be read or written (path-based functions).
    IoError = 7,
    /// `stegano_embed` (v1) only: the embedded carrier has a different length
    /// than the input (SAM, BAM, VCF and DICOM output can grow), so it cannot
    /// be written back in place. The input buffer is left untouched; use
    /// `stegano_embed_v2`, which returns a library-allocated output buffer.
    OutputSizeChanged = 8,
    /// A Rust panic was caught at the FFI boundary (only possible when the
    /// library is built with `panic = "unwind"`).
    Panic = 9,
    /// Any other failure (for example, the shared Tokio runtime could not be
    /// created).
    Unknown = 999,
}

/// Data format numbering used by the `uint32_t format` parameters.
///
/// Functions take the format as a plain `uint32_t` (not this enum) so that
/// out-of-range values from C or Go are rejected with `InvalidParameter`
/// rather than causing undefined behaviour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CDataFormat {
    /// FASTA sequences (payload in a header-line suffix; not covert).
    Fasta = 0,
    /// FASTQ reads (quality-score LSBs).
    Fastq = 1,
    /// Variant Call Format.
    Vcf = 2,
    /// SAM alignments (text).
    Sam = 3,
    /// BAM alignments (BGZF or raw `BAM\1`).
    Bam = 4,
    /// PNG image.
    Png = 5,
    /// TIFF image.
    Tiff = 6,
    /// Microscopy image: handled exactly like PNG/TIFF (the container is
    /// detected from its magic bytes). No microscopy-specific container
    /// (e.g. OME-TIFF stacks, multi-page TIFF) is supported.
    Microscopy = 7,
    /// DICOM medical image.
    Dicom = 8,
    /// RIFF/WAVE audio.
    Wav = 9,
    /// Raw signal: the bytes are treated as headerless mono 16-bit
    /// little-endian samples (no file structure is parsed).
    BinarySignal = 10,
    /// CSV: one bit per unquoted numeric cell in the parity of its last
    /// digit; header, text cells, quotes and line endings are unchanged, so
    /// the output stays valid CSV.
    CsvTimeseries = 11,
    /// Generic binary data (LSB of every byte). Also the result of format
    /// detection for unrecognised data.
    Binary = 12,
    /// Plain UTF-8 text: trailing spaces/tabs per line; the visible text is
    /// unchanged.
    Text = 13,
    /// FITS astronomical image.
    Fits = 14,
}

/// Embed statistics written to the `result_out` parameter of the embed
/// functions. Values larger than `UINT32_MAX` are saturated.
#[repr(C)]
pub struct CEmbedResult {
    /// `true` when the payload was embedded (always `true` when the call
    /// returns `Success`).
    pub success: bool,
    /// Number of payload bytes embedded, as reported by the format handler.
    pub bytes_embedded: u32,
    /// `true` when the carrier bytes were changed.
    pub carrier_modified: bool,
    /// Wall-clock time spent embedding, in milliseconds.
    pub embedding_time_ms: u32,
    /// Handler's rough estimate of the working memory used, in bytes (not a
    /// measurement).
    pub memory_usage_bytes: u32,
}

/// Capacity estimate written by `stegano_estimate_capacity`. Values larger
/// than `UINT32_MAX` are saturated.
#[repr(C)]
pub struct CCapacityInfo {
    /// Raw capacity of the carrier, in bits, including the handler's own
    /// framing header.
    pub total_bits: u32,
    /// Bits the handler recommends for the encoded payload. The encoded
    /// payload is larger than your data: it adds a 36-byte header, a 16-byte
    /// tag when a password is used, and Reed-Solomon parity when error
    /// correction is on (see `docs/WIRE_FORMAT.md`).
    pub recommended_bits: u32,
    /// `recommended_bits / total_bits` (0.0 when `total_bits` is 0).
    pub steganographic_ratio: f64,
}

/// Convert Rust DataFormat to its C numbering.
fn to_c_format(format: &DataFormat) -> CDataFormat {
    match format {
        DataFormat::FASTA => CDataFormat::Fasta,
        DataFormat::FASTQ => CDataFormat::Fastq,
        DataFormat::VCF => CDataFormat::Vcf,
        DataFormat::SAM => CDataFormat::Sam,
        DataFormat::BAM => CDataFormat::Bam,
        DataFormat::PNG => CDataFormat::Png,
        DataFormat::TIFF => CDataFormat::Tiff,
        DataFormat::MICROSCOPY => CDataFormat::Microscopy,
        DataFormat::DICOM => CDataFormat::Dicom,
        DataFormat::WAV => CDataFormat::Wav,
        DataFormat::BinarySignal => CDataFormat::BinarySignal,
        DataFormat::CsvTimeseries => CDataFormat::CsvTimeseries,
        DataFormat::BINARY => CDataFormat::Binary,
        DataFormat::TEXT => CDataFormat::Text,
        DataFormat::FITS => CDataFormat::Fits,
    }
}

/// Validate and convert a C format number to a Rust DataFormat.
fn format_from_u32(format: u32) -> Option<DataFormat> {
    Some(match format {
        0 => DataFormat::FASTA,
        1 => DataFormat::FASTQ,
        2 => DataFormat::VCF,
        3 => DataFormat::SAM,
        4 => DataFormat::BAM,
        5 => DataFormat::PNG,
        6 => DataFormat::TIFF,
        7 => DataFormat::MICROSCOPY,
        8 => DataFormat::DICOM,
        9 => DataFormat::WAV,
        10 => DataFormat::BinarySignal,
        11 => DataFormat::CsvTimeseries,
        12 => DataFormat::BINARY,
        13 => DataFormat::TEXT,
        14 => DataFormat::FITS,
        _ => return None,
    })
}

/// Convert Rust error to C error code
fn to_c_error(error: &SteganoError) -> SteganoErrorCode {
    match error {
        SteganoError::InvalidParameter(_) => SteganoErrorCode::InvalidParameter,
        SteganoError::InsufficientCapacity { .. } => SteganoErrorCode::InsufficientCapacity,
        SteganoError::EncryptionError(_) => SteganoErrorCode::CryptographicFailure,
        SteganoError::UnsupportedFormat(_) => SteganoErrorCode::FormatNotSupported,
        SteganoError::IntegrityError(_) => SteganoErrorCode::DataCorrupted,
        SteganoError::GpuError(_) => SteganoErrorCode::GpuError,
        SteganoError::IoError(_) => SteganoErrorCode::IoError,
        SteganoError::SerializationError(_) => SteganoErrorCode::DataCorrupted,
    }
}

type FfiResult<T> = std::result::Result<T, SteganoErrorCode>;

/// Borrow `len` bytes at `ptr`, rejecting null pointers, empty buffers and
/// lengths that `slice::from_raw_parts` does not allow.
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes for the lifetime `'a`.
unsafe fn input_slice<'a>(ptr: *const u8, len: u32) -> FfiResult<&'a [u8]> {
    let len = len as usize;
    if ptr.is_null() || len == 0 || len > isize::MAX as usize {
        return Err(SteganoErrorCode::InvalidParameter);
    }
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

/// Read an optional NUL-terminated password; null means "no encryption".
///
/// # Safety
/// `password` must be null or a valid NUL-terminated C string.
unsafe fn read_password(password: *const c_char) -> Vec<u8> {
    if password.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(password) }.to_bytes().to_vec()
    }
}

/// Parse an optional NUL-terminated JSON object; null or "" means "no options".
///
/// # Safety
/// `json` must be null or a valid NUL-terminated C string.
unsafe fn read_options(json: *const c_char) -> FfiResult<HashMap<String, serde_json::Value>> {
    if json.is_null() {
        return Ok(HashMap::new());
    }
    let s = unsafe { CStr::from_ptr(json) }
        .to_str()
        .map_err(|_| SteganoErrorCode::InvalidParameter)?;
    if s.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str(s).map_err(|_| SteganoErrorCode::InvalidParameter)
}

/// Build EmbedParams from the shared defaults plus the JSON overrides
/// `"stealth"`, `"ec_level"` and `"cipher"`. All keys are also passed
/// through as `format_specific` so handlers see e.g. `"sam_strategy"`.
fn build_params(
    key: Vec<u8>,
    format_specific: HashMap<String, serde_json::Value>,
) -> FfiResult<EmbedParams> {
    fn str_opt<'a>(
        map: &'a HashMap<String, serde_json::Value>,
        key: &str,
    ) -> FfiResult<Option<&'a str>> {
        match map.get(key) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
            Some(_) => Err(SteganoErrorCode::InvalidParameter),
        }
    }

    let mut params = carrier::default_params(key);
    if let Some(s) = str_opt(&format_specific, "stealth")? {
        params.stealth_level =
            carrier::parse_stealth_level(s).ok_or(SteganoErrorCode::InvalidParameter)?;
    }
    if let Some(s) = str_opt(&format_specific, "ec_level")? {
        params.error_correction_level =
            carrier::parse_ec_level(s).ok_or(SteganoErrorCode::InvalidParameter)?;
    }
    if let Some(s) = str_opt(&format_specific, "cipher")? {
        params.cipher_kind = carrier::parse_cipher(s).ok_or(SteganoErrorCode::InvalidParameter)?;
    }
    params.format_specific = format_specific;
    Ok(params)
}

/// Read a required NUL-terminated file path (null or empty is rejected).
///
/// On Unix the bytes are used as-is (paths need not be UTF-8); elsewhere
/// the path must be valid UTF-8.
///
/// # Safety
/// `path` must be null or a valid NUL-terminated C string.
unsafe fn read_path(path: *const c_char) -> FfiResult<PathBuf> {
    if path.is_null() {
        return Err(SteganoErrorCode::InvalidParameter);
    }
    let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
    if bytes.is_empty() {
        return Err(SteganoErrorCode::InvalidParameter);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes)
            .map(PathBuf::from)
            .map_err(|_| SteganoErrorCode::InvalidParameter)
    }
}

/// Borrow a payload buffer of `len` (`size_t`) bytes, rejecting null
/// pointers, empty buffers and lengths `slice::from_raw_parts` disallows.
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes for the lifetime `'a`.
unsafe fn input_slice_sized<'a>(ptr: *const u8, len: usize) -> FfiResult<&'a [u8]> {
    if ptr.is_null() || len == 0 || len > isize::MAX as usize {
        return Err(SteganoErrorCode::InvalidParameter);
    }
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

/// Copy `data` into a fresh `malloc` buffer (released with `stegano_free`).
fn alloc_output(data: &[u8]) -> FfiResult<(*mut u8, u32)> {
    let len = u32::try_from(data.len()).map_err(|_| SteganoErrorCode::InvalidParameter)?;
    Ok((alloc_output_sized(data)?, len))
}

/// Copy `data` into a fresh `malloc` buffer of any size.
fn alloc_output_sized(data: &[u8]) -> FfiResult<*mut u8> {
    // malloc(0) may return NULL; always allocate at least one byte.
    let p = unsafe { libc::malloc(data.len().max(1)) as *mut u8 };
    if p.is_null() {
        return Err(SteganoErrorCode::Unknown);
    }
    unsafe { ptr::copy_nonoverlapping(data.as_ptr(), p, data.len()) };
    Ok(p)
}

fn sat_u32(v: impl TryInto<u32>) -> u32 {
    v.try_into().unwrap_or(u32::MAX)
}

/// Shared embed implementation: returns the full (possibly grown) carrier.
fn embed_impl(
    carrier: &[u8],
    payload: &[u8],
    format: u32,
    params: EmbedParams,
) -> FfiResult<(Vec<u8>, CEmbedResult)> {
    let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
    let mut carrier = carrier_from_bytes(format, carrier.to_vec());
    let registry = HandlerRegistry::new();
    let handler = registry
        .get_handler(&carrier.format)
        .ok_or(SteganoErrorCode::FormatNotSupported)?;

    let result =
        run_blocking(handler.embed(&mut carrier, payload, &params))?.map_err(|e| to_c_error(&e))?;
    let c_result = CEmbedResult {
        success: result.success,
        bytes_embedded: sat_u32(result.bytes_embedded),
        carrier_modified: result.carrier_modified,
        embedding_time_ms: sat_u32(result.performance_stats.embedding_time_ms),
        memory_usage_bytes: sat_u32(result.performance_stats.memory_usage_bytes),
    };
    Ok((carrier.raw_data, c_result))
}

/// Shared extract implementation.
fn extract_impl(carrier: &[u8], format: u32, params: EmbedParams) -> FfiResult<Vec<u8>> {
    let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
    let carrier = carrier_from_bytes(format, carrier.to_vec());
    let registry = HandlerRegistry::new();
    let handler = registry
        .get_handler(&carrier.format)
        .ok_or(SteganoErrorCode::FormatNotSupported)?;
    run_blocking(handler.extract(&carrier, &params))?.map_err(|e| to_c_error(&e))
}

fn into_code(r: FfiResult<()>) -> SteganoErrorCode {
    r.err().unwrap_or(SteganoErrorCode::Success)
}

/// Initialize the stegano library
/// Returns: Error code (0 = success)
#[no_mangle]
pub extern "C" fn stegano_init() -> SteganoErrorCode {
    guard(|| {
        let _ = ffi_runtime();
        SteganoErrorCode::Success
    })
}

/// Detect the format of data
///
/// # Arguments
/// * `data` - Pointer to data buffer
/// * `data_len` - Length of data buffer
/// * `format_out` - Receives the detected format number (see `CDataFormat`);
///   unrecognised data is reported as `CDataFormat_Binary`.
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure `data` points to a valid memory region of at least `data_len` bytes
/// and `format_out` points to a writable `uint32_t`.
#[no_mangle]
pub unsafe extern "C" fn stegano_detect_format(
    data: *const u8,
    data_len: u32,
    format_out: *mut u32,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if format_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            let data = unsafe { input_slice(data, data_len) }?;
            let format = carrier::detect_format_from_content(data);
            unsafe { *format_out = to_c_format(&format) as u32 };
            Ok(())
        })())
    })
}

/// Estimate embedding capacity for data
///
/// # Arguments
/// * `data` - Pointer to carrier data buffer
/// * `data_len` - Length of carrier data buffer
/// * `format` - Data format number (see `CDataFormat`)
/// * `capacity_out` - Pointer to output capacity info
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure `data` points to a valid memory region of at least `data_len` bytes
/// and `capacity_out` points to a valid CCapacityInfo structure that can be written to.
#[no_mangle]
pub unsafe extern "C" fn stegano_estimate_capacity(
    data: *const u8,
    data_len: u32,
    format: u32,
    capacity_out: *mut CCapacityInfo,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if capacity_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            let data = unsafe { input_slice(data, data_len) }?;
            let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
            let carrier = carrier_from_bytes(format, data.to_vec());
            let registry = HandlerRegistry::new();
            let handler = registry
                .get_handler(&carrier.format)
                .ok_or(SteganoErrorCode::FormatNotSupported)?;
            let capacity = handler
                .estimate_capacity(&carrier)
                .map_err(|e| to_c_error(&e))?;
            unsafe {
                *capacity_out = CCapacityInfo {
                    total_bits: sat_u32(capacity.total_bits),
                    recommended_bits: sat_u32(capacity.recommended_bits),
                    steganographic_ratio: capacity.steganographic_ratio,
                };
            }
            Ok(())
        })())
    })
}

/// Embed payload into carrier data in place (v1).
///
/// Uses the shared default parameters. If the embedded carrier would have a
/// different length than `carrier_len` (SAM, BAM, VCF and DICOM output can
/// grow), the input buffer is left untouched and `OutputSizeChanged` is
/// returned; use `stegano_embed_v2` for those formats.
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer (modified on success)
/// * `carrier_len` - Length of carrier data buffer
/// * `payload_data` - Pointer to payload data buffer
/// * `payload_len` - Length of payload data buffer
/// * `format` - Data format number (see `CDataFormat`)
/// * `password` - Password for encryption (null or empty: no encryption,
///   the payload is stored in plaintext)
/// * `result_out` - Pointer to output embed result
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure:
/// - `carrier_data` points to a valid, mutable memory region of at least `carrier_len` bytes
/// - `payload_data` points to a valid memory region of at least `payload_len` bytes
/// - `password` is either null or points to a valid, null-terminated C string
/// - `result_out` points to a valid CEmbedResult structure that can be written to
#[no_mangle]
pub unsafe extern "C" fn stegano_embed(
    carrier_data: *mut u8,
    carrier_len: u32,
    payload_data: *const u8,
    payload_len: u32,
    format: u32,
    password: *const c_char,
    result_out: *mut CEmbedResult,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if result_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            let carrier = unsafe { input_slice(carrier_data, carrier_len) }?;
            let payload = unsafe { input_slice(payload_data, payload_len) }?;
            let params = build_params(unsafe { read_password(password) }, HashMap::new())?;
            let (output, result) = embed_impl(carrier, payload, format, params)?;
            if output.len() != carrier.len() {
                return Err(SteganoErrorCode::OutputSizeChanged);
            }
            unsafe {
                ptr::copy_nonoverlapping(output.as_ptr(), carrier_data, output.len());
                *result_out = result;
            }
            Ok(())
        })())
    })
}

/// Extract payload from carrier data (v1, shared default parameters).
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer
/// * `carrier_len` - Length of carrier data buffer
/// * `format` - Data format number (see `CDataFormat`)
/// * `password` - Password for decryption (null or empty if the payload
///   was embedded without one)
/// * `payload_out` - Receives a library-allocated payload buffer
/// * `payload_len_out` - Receives the payload length
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure:
/// - `carrier_data` points to a valid memory region of at least `carrier_len` bytes
/// - `password` is either null or points to a valid, null-terminated C string
/// - `payload_out` points to a valid location that can store a pointer to allocated memory
/// - `payload_len_out` points to a valid `uint32_t` that can be written to
/// - The caller must free the allocated payload memory using `stegano_free`
#[no_mangle]
pub unsafe extern "C" fn stegano_extract(
    carrier_data: *const u8,
    carrier_len: u32,
    format: u32,
    password: *const c_char,
    payload_out: *mut *mut u8,
    payload_len_out: *mut u32,
) -> SteganoErrorCode {
    unsafe {
        stegano_extract_v2(
            carrier_data,
            carrier_len,
            format,
            password,
            ptr::null(),
            payload_out,
            payload_len_out,
        )
    }
}

/// Embed payload into carrier data (v2: returns a new output buffer and
/// accepts JSON options).
///
/// The input carrier is not modified. On success `*output_out` receives a
/// library-allocated buffer holding the complete embedded carrier and
/// `*output_len_out` its length, which may differ from `carrier_len` (SAM,
/// BAM, VCF and DICOM output grows). Free the buffer with `stegano_free`.
///
/// `format_specific_json` is an optional null-terminated UTF-8 JSON object.
/// Recognised keys:
/// - `"stealth"`: `low` | `medium` | `high` | `maximum`
/// - `"ec_level"`: `none` | `low` | `medium` | `high` | `maximum`
/// - `"cipher"`: `aes256gcm` | `chacha20`
/// - format strategy keys, passed through to the handler:
///   `imagery_strategy`, `vcf_strategy`, `sam_strategy`, `dicom_strategy`,
///   `signal_strategy`, `genomics_strategy`, `fits_strategy`, and other
///   handler keys such as `bam_compression`
///
/// Pass NULL or `{}` for the shared defaults.
///
/// # Safety
/// - `carrier_data` must point to at least `carrier_len` readable bytes
/// - `payload_data` must point to at least `payload_len` readable bytes
/// - `password` and `format_specific_json` must each be null or a valid
///   NUL-terminated C string
/// - `output_out` and `output_len_out` must be valid writable pointers
/// - `result_out` must be null or point to a writable `CEmbedResult`
#[no_mangle]
pub unsafe extern "C" fn stegano_embed_v2(
    carrier_data: *const u8,
    carrier_len: u32,
    payload_data: *const u8,
    payload_len: u32,
    format: u32,
    password: *const c_char,
    format_specific_json: *const c_char,
    output_out: *mut *mut u8,
    output_len_out: *mut u32,
    result_out: *mut CEmbedResult,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if output_out.is_null() || output_len_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            unsafe {
                *output_out = ptr::null_mut();
                *output_len_out = 0;
            }
            let carrier = unsafe { input_slice(carrier_data, carrier_len) }?;
            let payload = unsafe { input_slice(payload_data, payload_len) }?;
            let options = unsafe { read_options(format_specific_json) }?;
            let params = build_params(unsafe { read_password(password) }, options)?;
            let (output, result) = embed_impl(carrier, payload, format, params)?;
            let (p, len) = alloc_output(&output)?;
            unsafe {
                *output_out = p;
                *output_len_out = len;
                if !result_out.is_null() {
                    *result_out = result;
                }
            }
            Ok(())
        })())
    })
}

/// Extract payload from carrier data (v2: accepts JSON options).
///
/// `format_specific_json` accepts the same keys as `stegano_embed_v2` and
/// should match the options used for embedding (e.g. the same
/// `sam_strategy`, and a non-`none` `ec_level` if EC was used without a
/// password). Pass NULL for the shared defaults.
///
/// # Safety
/// Same as `stegano_extract`, plus:
/// - `format_specific_json` is either null or a valid null-terminated C string
///   containing a JSON object.
#[no_mangle]
pub unsafe extern "C" fn stegano_extract_v2(
    carrier_data: *const u8,
    carrier_len: u32,
    format: u32,
    password: *const c_char,
    format_specific_json: *const c_char,
    payload_out: *mut *mut u8,
    payload_len_out: *mut u32,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if payload_out.is_null() || payload_len_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            unsafe {
                *payload_out = ptr::null_mut();
                *payload_len_out = 0;
            }
            let carrier = unsafe { input_slice(carrier_data, carrier_len) }?;
            let options = unsafe { read_options(format_specific_json) }?;
            let params = build_params(unsafe { read_password(password) }, options)?;
            let payload = extract_impl(carrier, format, params)?;
            let (p, len) = alloc_output(&payload)?;
            unsafe {
                *payload_out = p;
                *payload_len_out = len;
            }
            Ok(())
        })())
    })
}

/// Detect the format of the file at `path` without reading all of it: the
/// file extension first, then the first 4 KiB (magic bytes, then handler
/// probing). Unrecognised files are reported as `CDataFormat_Binary`.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string and `format_out` must
/// point to a writable `uint32_t`.
#[no_mangle]
pub unsafe extern "C" fn stegano_detect_file_format(
    path: *const c_char,
    format_out: *mut u32,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if format_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            let path = unsafe { read_path(path) }?;
            let format = files::detect_file_format(&path).map_err(|e| to_c_error(&e))?;
            unsafe { *format_out = to_c_format(&format) as u32 };
            Ok(())
        })())
    })
}

/// Embed a payload into the carrier file `in_path`, writing the result to
/// `out_path` (path-based; no 32-bit limit on the carrier size).
///
/// Streamable format/strategy pairs (e.g. FASTQ, VCF `info`/`id`/`quality`, SAM/BAM `auxiliary`/`quality`)
/// are streamed with constant memory; anything else is read into memory.
/// The output is written to a temporary file in `out_path`'s directory and
/// renamed over `out_path` only on success, so on error `out_path` is left
/// untouched. `in_path` and `out_path` may be the same file.
///
/// `format_specific_json` accepts the same keys as `stegano_embed_v2`
/// (plus any handler key, e.g. `"bam_compression"`). `result_out` may be
/// NULL.
///
/// # Safety
/// - `in_path` and `out_path` must be valid NUL-terminated C strings
/// - `payload_data` must point to at least `payload_len` readable bytes
/// - `password` and `format_specific_json` must each be null or a valid
///   NUL-terminated C string
/// - `result_out` must be null or point to a writable `CEmbedResult`
#[no_mangle]
pub unsafe extern "C" fn stegano_embed_file(
    in_path: *const c_char,
    out_path: *const c_char,
    payload_data: *const u8,
    payload_len: usize,
    format: u32,
    password: *const c_char,
    format_specific_json: *const c_char,
    result_out: *mut CEmbedResult,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            let in_path = unsafe { read_path(in_path) }?;
            let out_path = unsafe { read_path(out_path) }?;
            let payload = unsafe { input_slice_sized(payload_data, payload_len) }?;
            let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
            let options = unsafe { read_options(format_specific_json) }?;
            let params = build_params(unsafe { read_password(password) }, options)?;
            let (result, _mode) = run_blocking(files::embed_path(
                Some(format),
                &in_path,
                &out_path,
                payload,
                &params,
                true,
            ))?
            .map_err(|e| to_c_error(&e))?;
            if !result_out.is_null() {
                unsafe {
                    *result_out = CEmbedResult {
                        success: result.success,
                        bytes_embedded: sat_u32(result.bytes_embedded),
                        carrier_modified: result.carrier_modified,
                        embedding_time_ms: sat_u32(result.performance_stats.embedding_time_ms),
                        memory_usage_bytes: sat_u32(result.performance_stats.memory_usage_bytes),
                    };
                }
            }
            Ok(())
        })())
    })
}

/// Extract the payload from the carrier file at `path` (path-based; no
/// 32-bit limit on the carrier size).
///
/// Streamable formats are streamed, stopping as soon as the payload is
/// complete; anything else is read into memory. On success `*output_out`
/// receives a library-allocated buffer (free it with `stegano_free`) and
/// `*output_len_out` its length. `format_specific_json` is as for
/// `stegano_extract_v2`.
///
/// # Safety
/// - `path` must be a valid NUL-terminated C string
/// - `password` and `format_specific_json` must each be null or a valid
///   NUL-terminated C string
/// - `output_out` and `output_len_out` must be valid writable pointers
#[no_mangle]
pub unsafe extern "C" fn stegano_extract_file(
    path: *const c_char,
    format: u32,
    password: *const c_char,
    format_specific_json: *const c_char,
    output_out: *mut *mut u8,
    output_len_out: *mut usize,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if output_out.is_null() || output_len_out.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            unsafe {
                *output_out = ptr::null_mut();
                *output_len_out = 0;
            }
            let path = unsafe { read_path(path) }?;
            let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
            let options = unsafe { read_options(format_specific_json) }?;
            let params = build_params(unsafe { read_password(password) }, options)?;
            let (payload, _mode) =
                run_blocking(files::extract_path(Some(format), &path, &params, true))?
                    .map_err(|e| to_c_error(&e))?;
            let p = alloc_output_sized(&payload)?;
            unsafe {
                *output_out = p;
                *output_len_out = payload.len();
            }
            Ok(())
        })())
    })
}

/// Free a buffer that the library returned through a `uint8_t **`
/// out-parameter (`stegano_extract`, `stegano_extract_v2`,
/// `stegano_extract_file`, `stegano_embed_v2`). Passing NULL is a no-op.
///
/// Every such buffer must be released exactly once with this function, not
/// with your own allocator's `free` (the library may use a different C
/// runtime, e.g. on Windows). Do not pass the string from `stegano_version`,
/// which is static.
///
/// # Arguments
/// * `ptr` - Pointer to memory to free
///
/// # Safety
/// The caller must ensure `ptr` is null or was returned by a stegano
/// function and has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn stegano_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe {
            libc::free(ptr);
        }
    }
}

/// Get the library version string (the crate version, e.g. `"0.8.0"`).
/// Returns: static NUL-terminated string (caller must NOT free)
#[no_mangle]
pub extern "C" fn stegano_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

/// Check whether carrier data contains an embedded stegano payload.
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer
/// * `carrier_len` - Length of carrier data buffer
/// * `format` - Data format number (see `CDataFormat`)
/// * `integrity_ok` - Pointer to output integrity result
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure `carrier_data` points to a valid memory region of at least
/// `carrier_len` bytes and `integrity_ok` points to a valid bool that can be written to.
#[no_mangle]
pub unsafe extern "C" fn stegano_verify_integrity(
    carrier_data: *const u8,
    carrier_len: u32,
    format: u32,
    integrity_ok: *mut bool,
) -> SteganoErrorCode {
    guard(|| {
        into_code((|| {
            if integrity_ok.is_null() {
                return Err(SteganoErrorCode::InvalidParameter);
            }
            let data = unsafe { input_slice(carrier_data, carrier_len) }?;
            let format = format_from_u32(format).ok_or(SteganoErrorCode::InvalidParameter)?;
            let carrier = carrier_from_bytes(format, data.to_vec());
            let registry = HandlerRegistry::new();
            let handler = registry
                .get_handler(&carrier.format)
                .ok_or(SteganoErrorCode::FormatNotSupported)?;
            let ok = handler
                .verify_integrity(&carrier)
                .map_err(|e| to_c_error(&e))?;
            unsafe { *integrity_ok = ok };
            Ok(())
        })())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    const BINARY: u32 = CDataFormat::Binary as u32;
    const SAM: u32 = CDataFormat::Sam as u32;

    fn binary_carrier(size: usize) -> Vec<u8> {
        (0u8..=255).cycle().take(size).collect()
    }

    fn sam_carrier(records: usize) -> Vec<u8> {
        let mut s = String::from("@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:100000\n");
        for i in 0..records {
            s.push_str(&format!(
                "read{i}\t0\tchr1\t{}\t60\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n",
                i + 1
            ));
        }
        s.into_bytes()
    }

    unsafe fn embed_v2(
        carrier: &[u8],
        payload: &[u8],
        format: u32,
        password: Option<&str>,
        json: Option<&str>,
    ) -> FfiResult<Vec<u8>> {
        let pw = password.map(|p| CString::new(p).unwrap());
        let js = json.map(|j| CString::new(j).unwrap());
        let mut out: *mut u8 = ptr::null_mut();
        let mut out_len: u32 = 0;
        let code = unsafe {
            stegano_embed_v2(
                carrier.as_ptr(),
                carrier.len() as u32,
                payload.as_ptr(),
                payload.len() as u32,
                format,
                pw.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                js.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                &mut out,
                &mut out_len,
                ptr::null_mut(),
            )
        };
        if code != SteganoErrorCode::Success {
            return Err(code);
        }
        let v = unsafe { slice::from_raw_parts(out, out_len as usize) }.to_vec();
        unsafe { stegano_free(out as *mut c_void) };
        Ok(v)
    }

    unsafe fn extract_v2(
        carrier: &[u8],
        format: u32,
        password: Option<&str>,
        json: Option<&str>,
    ) -> FfiResult<Vec<u8>> {
        let pw = password.map(|p| CString::new(p).unwrap());
        let js = json.map(|j| CString::new(j).unwrap());
        let mut out: *mut u8 = ptr::null_mut();
        let mut out_len: u32 = 0;
        let code = unsafe {
            stegano_extract_v2(
                carrier.as_ptr(),
                carrier.len() as u32,
                format,
                pw.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                js.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                &mut out,
                &mut out_len,
            )
        };
        if code != SteganoErrorCode::Success {
            return Err(code);
        }
        let v = unsafe { slice::from_raw_parts(out, out_len as usize) }.to_vec();
        unsafe { stegano_free(out as *mut c_void) };
        Ok(v)
    }

    #[test]
    fn test_format_conversion() {
        assert_eq!(
            to_c_format(&DataFormat::FASTA) as u32,
            CDataFormat::Fasta as u32
        );
        assert_eq!(
            format_from_u32(CDataFormat::Fastq as u32),
            Some(DataFormat::FASTQ)
        );
        for v in 0..=14u32 {
            let f = format_from_u32(v).unwrap();
            assert_eq!(to_c_format(&f) as u32, v);
        }
        assert_eq!(format_from_u32(15), None);
        assert_eq!(format_from_u32(99), None);
    }

    #[test]
    fn test_ffi_init() {
        assert_eq!(stegano_init(), SteganoErrorCode::Success);
    }

    #[test]
    fn test_version() {
        let version_ptr = stegano_version();
        assert!(!version_ptr.is_null());
        let version_str = unsafe { CStr::from_ptr(version_ptr) };
        assert_eq!(version_str.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_guard_catches_panic() {
        assert_eq!(guard(|| panic!("boom")), SteganoErrorCode::Panic);
    }

    #[test]
    fn test_guard_catches_panic_inside_runtime_future() {
        let code = guard(|| match run_blocking(async { panic!("boom in future") }) {
            Ok(()) => SteganoErrorCode::Success,
            Err(e) => e,
        });
        assert_eq!(code, SteganoErrorCode::Panic);
        assert_eq!(SteganoErrorCode::Panic as i32, 9);
    }

    /// `catch_unwind` is a no-op under `panic = "abort"`, so the shipped
    /// cdylib must be built with unwinding (#62). Test binaries always unwind,
    /// so check the release profile in the manifest directly.
    #[test]
    fn test_release_profile_unwinds() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        let release = manifest
            .split("[profile.release]")
            .nth(1)
            .expect("[profile.release] section");
        let section = release.split("\n[").next().unwrap();
        let panic_lines: Vec<&str> = section
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("panic"))
            .collect();
        assert_eq!(panic_lines, ["panic = \"unwind\""]);
    }

    #[test]
    fn test_invalid_format_rejected() {
        let carrier = binary_carrier(4096);
        let mut info = CCapacityInfo {
            total_bits: 0,
            recommended_bits: 0,
            steganographic_ratio: 0.0,
        };
        let code = unsafe {
            stegano_estimate_capacity(carrier.as_ptr(), carrier.len() as u32, 99, &mut info)
        };
        assert_eq!(code, SteganoErrorCode::InvalidParameter);
        assert_eq!(
            unsafe { embed_v2(&carrier, b"x", 99, None, None) },
            Err(SteganoErrorCode::InvalidParameter)
        );
    }

    #[test]
    fn test_null_and_bad_json_rejected() {
        let carrier = binary_carrier(4096);
        assert_eq!(
            unsafe { embed_v2(&carrier, b"x", BINARY, None, Some("not json")) },
            Err(SteganoErrorCode::InvalidParameter)
        );
        assert_eq!(
            unsafe { embed_v2(&carrier, b"x", BINARY, None, Some(r#"{"cipher":"rot13"}"#)) },
            Err(SteganoErrorCode::InvalidParameter)
        );
        let mut fmt: u32 = 0;
        assert_eq!(
            unsafe { stegano_detect_format(ptr::null(), 10, &mut fmt) },
            SteganoErrorCode::InvalidParameter
        );
    }

    #[test]
    fn test_v2_roundtrip_binary_with_cipher() {
        let carrier = binary_carrier(8192);
        let payload = b"ffi chacha roundtrip";
        let json = Some(r#"{"cipher":"chacha20"}"#);
        let out = unsafe { embed_v2(&carrier, payload, BINARY, Some("pw"), json) }.unwrap();
        assert_eq!(out.len(), carrier.len());
        // Cipher is auto-detected on extract.
        let got = unsafe { extract_v2(&out, BINARY, Some("pw"), None) }.unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn test_v2_cipher_key_selects_chacha() {
        let params =
            build_params(b"k".to_vec(), [("cipher".into(), "chacha20".into())].into()).unwrap();
        assert_eq!(params.cipher_kind, CipherKind::ChaCha20Poly1305);
        let params = build_params(vec![], HashMap::new()).unwrap();
        assert_eq!(params.stealth_level, carrier::DEFAULT_STEALTH_LEVEL);
        assert_eq!(params.error_correction_level, carrier::DEFAULT_EC_LEVEL);
    }

    #[test]
    fn test_v2_grown_output_not_truncated() {
        let carrier = sam_carrier(200);
        let payload = b"SAM payload that makes the file grow";
        let out = unsafe { embed_v2(&carrier, payload, SAM, None, None) }.unwrap();
        assert!(out.len() > carrier.len(), "SAM output should grow");
        let got = unsafe { extract_v2(&out, SAM, None, None) }.unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn test_v1_embed_reports_size_change() {
        let mut carrier = sam_carrier(200);
        let original = carrier.clone();
        let payload = b"SAM payload";
        let mut result = CEmbedResult {
            success: false,
            bytes_embedded: 0,
            carrier_modified: false,
            embedding_time_ms: 0,
            memory_usage_bytes: 0,
        };
        let code = unsafe {
            stegano_embed(
                carrier.as_mut_ptr(),
                carrier.len() as u32,
                payload.as_ptr(),
                payload.len() as u32,
                SAM,
                ptr::null(),
                &mut result,
            )
        };
        assert_eq!(code, SteganoErrorCode::OutputSizeChanged);
        assert_eq!(carrier, original, "input must be left untouched");
    }

    #[test]
    fn test_v1_roundtrip_binary() {
        let mut carrier = binary_carrier(4096);
        let payload = b"v1 in-place roundtrip";
        let mut result = CEmbedResult {
            success: false,
            bytes_embedded: 0,
            carrier_modified: false,
            embedding_time_ms: 0,
            memory_usage_bytes: 0,
        };
        let code = unsafe {
            stegano_embed(
                carrier.as_mut_ptr(),
                carrier.len() as u32,
                payload.as_ptr(),
                payload.len() as u32,
                BINARY,
                ptr::null(),
                &mut result,
            )
        };
        assert_eq!(code, SteganoErrorCode::Success);
        assert!(result.success);
        let got = unsafe { extract_v2(&carrier, BINARY, None, None) }.unwrap();
        assert_eq!(got, payload);
    }

    fn fastq_carrier(records: usize) -> Vec<u8> {
        let mut s = String::new();
        for i in 0..records {
            s.push_str(&format!("@r{i}\nACGTACGTACGTACGT\n+\nIIIIIIIIIIIIIIII\n"));
        }
        s.into_bytes()
    }

    fn cpath(p: &std::path::Path) -> CString {
        CString::new(p.to_str().unwrap()).unwrap()
    }

    unsafe fn embed_file(
        input: &std::path::Path,
        output: &std::path::Path,
        payload: &[u8],
        format: u32,
        json: Option<&str>,
    ) -> SteganoErrorCode {
        let js = json.map(|j| CString::new(j).unwrap());
        unsafe {
            stegano_embed_file(
                cpath(input).as_ptr(),
                cpath(output).as_ptr(),
                payload.as_ptr(),
                payload.len(),
                format,
                c"pw".as_ptr(),
                js.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                ptr::null_mut(),
            )
        }
    }

    unsafe fn extract_file(path: &std::path::Path, format: u32) -> FfiResult<Vec<u8>> {
        let mut out: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let code = unsafe {
            stegano_extract_file(
                cpath(path).as_ptr(),
                format,
                c"pw".as_ptr(),
                ptr::null(),
                &mut out,
                &mut out_len,
            )
        };
        if code != SteganoErrorCode::Success {
            return Err(code);
        }
        let v = unsafe { slice::from_raw_parts(out, out_len) }.to_vec();
        unsafe { stegano_free(out as *mut c_void) };
        Ok(v)
    }

    #[test]
    fn test_file_roundtrip_streamed_and_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let cases = [
            ("in.fastq", fastq_carrier(300), CDataFormat::Fastq as u32),
            ("in.sam", sam_carrier(200), SAM),
            ("in.bin", binary_carrier(8192), BINARY),
        ];
        for (name, data, format) in cases {
            let input = dir.path().join(name);
            let output = dir.path().join(format!("out-{name}"));
            fs_write(&input, &data);
            let code = unsafe { embed_file(&input, &output, b"file payload", format, None) };
            assert_eq!(code, SteganoErrorCode::Success, "{name}");
            let got = unsafe { extract_file(&output, format) }.unwrap();
            assert_eq!(got, b"file payload", "{name}");
        }
    }

    fn fs_write(p: &std::path::Path, data: &[u8]) {
        std::fs::write(p, data).unwrap();
    }

    #[test]
    fn test_file_errors_leave_output_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("small.fastq");
        let output = dir.path().join("out.fastq");
        fs_write(&input, &fastq_carrier(2));
        fs_write(&output, b"keep me");
        let code = unsafe { embed_file(&input, &output, &[7u8; 4000], 1, None) };
        assert_eq!(code, SteganoErrorCode::InsufficientCapacity);
        assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);

        let missing = dir.path().join("missing.fastq");
        assert_eq!(
            unsafe { embed_file(&missing, &output, b"x", 1, None) },
            SteganoErrorCode::IoError
        );
        assert_eq!(
            unsafe { embed_file(&input, &output, b"x", 99, None) },
            SteganoErrorCode::InvalidParameter
        );
        assert_eq!(
            unsafe { embed_file(&input, &output, b"x", 1, Some("[1]")) },
            SteganoErrorCode::InvalidParameter
        );
        let code = unsafe {
            stegano_embed_file(
                ptr::null(),
                cpath(&output).as_ptr(),
                b"x".as_ptr(),
                1,
                1,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
            )
        };
        assert_eq!(code, SteganoErrorCode::InvalidParameter);
        assert_eq!(
            unsafe { extract_file(&missing, 1) },
            Err(SteganoErrorCode::IoError)
        );
    }

    #[test]
    fn test_detect_file_format() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("reads.dat");
        fs_write(&p, &fastq_carrier(3));
        let mut fmt = 99u32;
        let code = unsafe { stegano_detect_file_format(cpath(&p).as_ptr(), &mut fmt) };
        assert_eq!(code, SteganoErrorCode::Success);
        assert_eq!(fmt, CDataFormat::Fastq as u32);
        let code = unsafe { stegano_detect_file_format(ptr::null(), &mut fmt) };
        assert_eq!(code, SteganoErrorCode::InvalidParameter);
    }

    #[tokio::test]
    async fn test_callable_from_inside_tokio_runtime() {
        let carrier = binary_carrier(4096);
        let payload = b"called from async context";
        let out = unsafe { embed_v2(&carrier, payload, BINARY, None, None) }.unwrap();
        let got = unsafe { extract_v2(&out, BINARY, None, None) }.unwrap();
        assert_eq!(got, payload);
    }
}
