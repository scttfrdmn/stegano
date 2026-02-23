//! Foreign Function Interface (FFI) for C/C++ integration
//!
//! Copyright (c) 2025 Scott Friedman. All rights reserved.
//! This software is proprietary and confidential.

use crate::core::*;
use crate::handlers::HandlerRegistry;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr;
use std::slice;
use std::sync::OnceLock;

/// Single shared Tokio runtime — avoids creating a new runtime per FFI call.
static TOKIO_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn ffi_runtime() -> &'static tokio::runtime::Runtime {
    TOKIO_RT.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime for FFI")
    })
}

/// C-compatible error codes
#[repr(C)]
pub enum SteganoErrorCode {
    Success = 0,
    InvalidParameter = 1,
    InsufficientCapacity = 2,
    CryptographicFailure = 3,
    FormatNotSupported = 4,
    DataCorrupted = 5,
    GpuError = 6,
    IoError = 7,
    Unknown = 999,
}

/// C-compatible data format enum
#[repr(C)]
pub enum CDataFormat {
    Fasta = 0,
    Fastq = 1,
    Vcf = 2,
    Sam = 3,
    Bam = 4,
    Png = 5,
    Tiff = 6,
    Microscopy = 7,
    Dicom = 8,
    Wav = 9,
    BinarySignal = 10,
    CsvTimeseries = 11,
    Binary = 12,
    Text = 13,
}

/// C-compatible embed result structure
#[repr(C)]
pub struct CEmbedResult {
    pub success: bool,
    pub bytes_embedded: c_uint,
    pub carrier_modified: bool,
    pub embedding_time_ms: c_uint,
    pub memory_usage_bytes: c_uint,
}

/// C-compatible capacity info structure
#[repr(C)]
pub struct CCapacityInfo {
    pub total_bits: c_uint,
    pub recommended_bits: c_uint,
    pub steganographic_ratio: f64,
}

/// Convert Rust DataFormat to C enum
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
    }
}

/// Convert C enum to Rust DataFormat
fn from_c_format(format: CDataFormat) -> DataFormat {
    match format {
        CDataFormat::Fasta => DataFormat::FASTA,
        CDataFormat::Fastq => DataFormat::FASTQ,
        CDataFormat::Vcf => DataFormat::VCF,
        CDataFormat::Sam => DataFormat::SAM,
        CDataFormat::Bam => DataFormat::BAM,
        CDataFormat::Png => DataFormat::PNG,
        CDataFormat::Tiff => DataFormat::TIFF,
        CDataFormat::Microscopy => DataFormat::MICROSCOPY,
        CDataFormat::Dicom => DataFormat::DICOM,
        CDataFormat::Wav => DataFormat::WAV,
        CDataFormat::BinarySignal => DataFormat::BinarySignal,
        CDataFormat::CsvTimeseries => DataFormat::CsvTimeseries,
        CDataFormat::Binary => DataFormat::BINARY,
        CDataFormat::Text => DataFormat::TEXT,
    }
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

/// Initialize the stegano library
/// Returns: Error code (0 = success)
#[no_mangle]
pub extern "C" fn stegano_init() -> SteganoErrorCode {
    // Initialize logging if needed
    SteganoErrorCode::Success
}

/// Detect the format of data
///
/// # Arguments
/// * `data` - Pointer to data buffer
/// * `data_len` - Length of data buffer
/// * `format_out` - Pointer to output format enum
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure `data` points to a valid memory region of at least `data_len` bytes
/// and `format_out` points to a valid CDataFormat that can be written to.
#[no_mangle]
pub unsafe extern "C" fn stegano_detect_format(
    data: *const u8,
    data_len: c_uint,
    format_out: *mut CDataFormat,
) -> SteganoErrorCode {
    if data.is_null() || format_out.is_null() || data_len == 0 {
        return SteganoErrorCode::InvalidParameter;
    }

    let data_slice = unsafe { slice::from_raw_parts(data, data_len as usize) };
    let registry = HandlerRegistry::new();

    // Try each handler to detect format
    for handler in registry.get_handlers() {
        if let Some(format) = handler.detect_format(data_slice) {
            unsafe {
                *format_out = to_c_format(&format);
            }
            return SteganoErrorCode::Success;
        }
    }

    // Default to binary if no specific format detected
    unsafe {
        *format_out = CDataFormat::Binary;
    }
    SteganoErrorCode::Success
}

/// Estimate embedding capacity for data
///
/// # Arguments
/// * `data` - Pointer to carrier data buffer
/// * `data_len` - Length of carrier data buffer
/// * `format` - Data format
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
    data_len: c_uint,
    format: CDataFormat,
    capacity_out: *mut CCapacityInfo,
) -> SteganoErrorCode {
    if data.is_null() || capacity_out.is_null() || data_len == 0 {
        return SteganoErrorCode::InvalidParameter;
    }

    let data_slice = unsafe { slice::from_raw_parts(data, data_len as usize) };
    let rust_format = from_c_format(format);

    let carrier = CarrierData {
        format: rust_format,
        raw_data: data_slice.to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let registry = HandlerRegistry::new();

    match registry.get_handler(&carrier.format) {
        Some(handler) => match handler.estimate_capacity(&carrier) {
            Ok(capacity) => {
                unsafe {
                    *capacity_out = CCapacityInfo {
                        total_bits: capacity.total_bits as c_uint,
                        recommended_bits: capacity.recommended_bits as c_uint,
                        steganographic_ratio: capacity.steganographic_ratio,
                    };
                }
                SteganoErrorCode::Success
            }
            Err(error) => to_c_error(&error),
        },
        None => SteganoErrorCode::FormatNotSupported,
    }
}

/// Embed payload into carrier data
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer (will be modified)
/// * `carrier_len` - Length of carrier data buffer
/// * `payload_data` - Pointer to payload data buffer
/// * `payload_len` - Length of payload data buffer
/// * `format` - Data format
/// * `password` - Password for encryption (null for no encryption)
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
    carrier_len: c_uint,
    payload_data: *const u8,
    payload_len: c_uint,
    format: CDataFormat,
    password: *const c_char,
    result_out: *mut CEmbedResult,
) -> SteganoErrorCode {
    if carrier_data.is_null()
        || payload_data.is_null()
        || result_out.is_null()
        || carrier_len == 0
        || payload_len == 0
    {
        return SteganoErrorCode::InvalidParameter;
    }

    let carrier_slice = unsafe { slice::from_raw_parts_mut(carrier_data, carrier_len as usize) };
    let payload_slice = unsafe { slice::from_raw_parts(payload_data, payload_len as usize) };
    let rust_format = from_c_format(format);

    // Handle password
    let encryption_key = if password.is_null() {
        vec![]
    } else {
        let c_str = unsafe { CStr::from_ptr(password) };
        c_str.to_bytes().to_vec()
    };

    let mut carrier = CarrierData {
        format: rust_format,
        raw_data: carrier_slice.to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let params = EmbedParams {
        encryption_key,
        error_correction_level: ECLevel::Medium,
        stealth_level: StealthLevel::Low,
        use_gpu: false,
        format_specific: HashMap::new(),
        enable_validation: false,
    };

    let registry = HandlerRegistry::new();

    match registry.get_handler(&carrier.format) {
        Some(handler) => {
            // Use the shared static runtime
            let rt = ffi_runtime();

            match rt.block_on(handler.embed(&mut carrier, payload_slice, &params)) {
                Ok(result) => {
                    // Copy modified carrier data back
                    let copy_len = std::cmp::min(carrier.raw_data.len(), carrier_len as usize);
                    unsafe {
                        ptr::copy_nonoverlapping(carrier.raw_data.as_ptr(), carrier_data, copy_len);
                    }

                    unsafe {
                        *result_out = CEmbedResult {
                            success: result.success,
                            bytes_embedded: result.bytes_embedded as c_uint,
                            carrier_modified: result.carrier_modified,
                            embedding_time_ms: result.performance_stats.embedding_time_ms as c_uint,
                            memory_usage_bytes: result.performance_stats.memory_usage_bytes
                                as c_uint,
                        };
                    }
                    SteganoErrorCode::Success
                }
                Err(error) => to_c_error(&error),
            }
        }
        None => SteganoErrorCode::FormatNotSupported,
    }
}

/// Extract payload from carrier data
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer
/// * `carrier_len` - Length of carrier data buffer
/// * `format` - Data format
/// * `password` - Password for decryption (null for no encryption)
/// * `payload_out` - Pointer to output payload buffer (caller must free)
/// * `payload_len_out` - Pointer to output payload length
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// The caller must ensure:
/// - `carrier_data` points to a valid memory region of at least `carrier_len` bytes
/// - `password` is either null or points to a valid, null-terminated C string
/// - `payload_out` points to a valid location that can store a pointer to allocated memory
/// - `payload_len_out` points to a valid c_uint that can be written to
/// - The caller must free the allocated payload memory using `stegano_free`
#[no_mangle]
pub unsafe extern "C" fn stegano_extract(
    carrier_data: *const u8,
    carrier_len: c_uint,
    format: CDataFormat,
    password: *const c_char,
    payload_out: *mut *mut u8,
    payload_len_out: *mut c_uint,
) -> SteganoErrorCode {
    if carrier_data.is_null()
        || payload_out.is_null()
        || payload_len_out.is_null()
        || carrier_len == 0
    {
        return SteganoErrorCode::InvalidParameter;
    }

    let carrier_slice = unsafe { slice::from_raw_parts(carrier_data, carrier_len as usize) };
    let rust_format = from_c_format(format);

    // Handle password
    let encryption_key = if password.is_null() {
        vec![]
    } else {
        let c_str = unsafe { CStr::from_ptr(password) };
        c_str.to_bytes().to_vec()
    };

    let carrier = CarrierData {
        format: rust_format,
        raw_data: carrier_slice.to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let params = EmbedParams {
        encryption_key,
        error_correction_level: ECLevel::Medium,
        stealth_level: StealthLevel::Low,
        use_gpu: false,
        format_specific: HashMap::new(),
        enable_validation: false,
    };

    let registry = HandlerRegistry::new();

    match registry.get_handler(&carrier.format) {
        Some(handler) => {
            // Use the shared static runtime
            let rt = ffi_runtime();

            match rt.block_on(handler.extract(&carrier, &params)) {
                Ok(payload) => {
                    // Allocate memory for payload (caller must free)
                    let payload_ptr = unsafe { libc::malloc(payload.len()) as *mut u8 };

                    if payload_ptr.is_null() {
                        return SteganoErrorCode::Unknown;
                    }

                    unsafe {
                        ptr::copy_nonoverlapping(payload.as_ptr(), payload_ptr, payload.len());
                        *payload_out = payload_ptr;
                        *payload_len_out = payload.len() as c_uint;
                    }
                    SteganoErrorCode::Success
                }
                Err(error) => to_c_error(&error),
            }
        }
        None => SteganoErrorCode::FormatNotSupported,
    }
}

/// Free memory allocated by stegano_extract
///
/// # Arguments
/// * `ptr` - Pointer to memory to free
///
/// # Safety
/// The caller must ensure `ptr` was allocated by a stegano function and is not null.
#[no_mangle]
pub unsafe extern "C" fn stegano_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe {
            libc::free(ptr);
        }
    }
}

/// Get version string
/// Returns: Version string (caller must NOT free)
#[no_mangle]
pub extern "C" fn stegano_version() -> *const c_char {
    static VERSION: &str = "0.3.12\0";
    VERSION.as_ptr() as *const c_char
}

/// Verify data integrity after embedding
///
/// # Arguments
/// * `carrier_data` - Pointer to carrier data buffer
/// * `carrier_len` - Length of carrier data buffer
/// * `format` - Data format
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
    carrier_len: c_uint,
    format: CDataFormat,
    integrity_ok: *mut bool,
) -> SteganoErrorCode {
    if carrier_data.is_null() || integrity_ok.is_null() || carrier_len == 0 {
        return SteganoErrorCode::InvalidParameter;
    }

    let carrier_slice = unsafe { slice::from_raw_parts(carrier_data, carrier_len as usize) };
    let rust_format = from_c_format(format);

    let carrier = CarrierData {
        format: rust_format,
        raw_data: carrier_slice.to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let registry = HandlerRegistry::new();

    match registry.get_handler(&carrier.format) {
        Some(handler) => match handler.verify_integrity(&carrier) {
            Ok(result) => {
                unsafe {
                    *integrity_ok = result;
                }
                SteganoErrorCode::Success
            }
            Err(error) => to_c_error(&error),
        },
        None => SteganoErrorCode::FormatNotSupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_conversion() {
        assert_eq!(
            to_c_format(&DataFormat::FASTA) as u32,
            CDataFormat::Fasta as u32
        );
        assert_eq!(from_c_format(CDataFormat::Fastq), DataFormat::FASTQ);
    }

    #[test]
    fn test_ffi_init() {
        let result = stegano_init();
        assert_eq!(result as u32, SteganoErrorCode::Success as u32);
    }

    #[test]
    fn test_version() {
        let version_ptr = stegano_version();
        assert!(!version_ptr.is_null());

        let version_str = unsafe { CStr::from_ptr(version_ptr) };
        assert_eq!(version_str.to_str().unwrap(), "0.3.12");
    }
}
