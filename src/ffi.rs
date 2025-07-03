//! FFI (Foreign Function Interface) for exposing stegano library to other languages
//! 
//! This module provides C-compatible functions that can be called from Go or other languages
//! that support calling C libraries.

use crate::core::*;
use crate::handlers::HandlerRegistry;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar};
use std::ptr;
use std::slice;
use tokio::runtime::Runtime;

/// FFI result codes
#[repr(C)]
#[derive(PartialEq)]
pub enum SteganoResult {
    Success = 0,
    ErrorInvalidInput = -1,
    ErrorInsufficientCapacity = -2,
    ErrorCorruptedData = -3,
    ErrorTamperDetected = -4,
    ErrorLayerNotSupported = -5,
    ErrorCryptographicFailure = -6,
    ErrorUnknown = -99,
}

/// FFI structure for embedding parameters
#[repr(C)]
pub struct EmbedParamsFFI {
    pub encryption_key: *const c_uchar,
    pub encryption_key_len: c_int,
    pub error_correction_level: c_int,  // 0=None, 1=Low, 2=Medium, 3=High
    pub stealth_level: c_int,           // 0=Low, 1=Medium, 2=High, 3=Maximum
    pub use_gpu: c_int,                 // 0=false, 1=true
}

/// FFI structure for embedding results
#[repr(C)]
pub struct EmbedResultFFI {
    pub success: c_int,
    pub bytes_embedded: c_int,
    pub carrier_modified: c_int,
    pub embedding_time_ms: c_int,
    pub memory_usage_bytes: c_int,
}

/// FFI structure for genomics fingerprint embedding
#[repr(C)]
pub struct GenomicsFingerprintFFI {
    pub customer_id: *const c_char,
    pub dataset_id: *const c_char,
    pub timestamp: i64,
    pub metadata_json: *const c_char,
}

/// Global runtime for async operations
static mut RUNTIME: Option<Runtime> = None;

/// Initialize the stegano library for FFI usage
#[no_mangle]
pub extern "C" fn stegano_init() -> SteganoResult {
    unsafe {
        if RUNTIME.is_none() {
            match Runtime::new() {
                Ok(rt) => {
                    RUNTIME = Some(rt);
                    SteganoResult::Success
                }
                Err(_) => SteganoResult::ErrorUnknown,
            }
        } else {
            SteganoResult::Success
        }
    }
}

/// Cleanup the stegano library
#[no_mangle]
pub extern "C" fn stegano_cleanup() {
    unsafe {
        RUNTIME = None;
    }
}

/// Embed fingerprint data in FASTQ quality scores
#[no_mangle]
pub extern "C" fn stegano_embed_fastq_qualities(
    quality_scores: *const c_uchar,
    quality_len: c_int,
    fingerprint: *const GenomicsFingerprintFFI,
    params: *const EmbedParamsFFI,
    output_buffer: *mut c_uchar,
    output_buffer_size: c_int,
    result: *mut EmbedResultFFI,
) -> SteganoResult {
    if quality_scores.is_null() || fingerprint.is_null() || params.is_null() || 
       output_buffer.is_null() || result.is_null() || quality_len <= 0 {
        return SteganoResult::ErrorInvalidInput;
    }

    unsafe {
        let runtime = match &RUNTIME {
            Some(rt) => rt,
            None => return SteganoResult::ErrorUnknown,
        };

        // Convert input parameters
        let quality_slice = slice::from_raw_parts(quality_scores, quality_len as usize);
        let quality_data = quality_slice.to_vec();

        let customer_id = match CStr::from_ptr((*fingerprint).customer_id).to_str() {
            Ok(s) => s,
            Err(_) => return SteganoResult::ErrorInvalidInput,
        };

        let dataset_id = if (*fingerprint).dataset_id.is_null() {
            ""
        } else {
            match CStr::from_ptr((*fingerprint).dataset_id).to_str() {
                Ok(s) => s,
                Err(_) => return SteganoResult::ErrorInvalidInput,
            }
        };

        // Create fingerprint payload
        let fingerprint_data = format!("{}:{}:{}", customer_id, dataset_id, (*fingerprint).timestamp);
        let payload = fingerprint_data.as_bytes();

        // Convert embedding parameters
        let encryption_key = if (*params).encryption_key.is_null() {
            vec![]
        } else {
            slice::from_raw_parts((*params).encryption_key, (*params).encryption_key_len as usize).to_vec()
        };

        let error_correction_level = match (*params).error_correction_level {
            0 => ECLevel::None,
            1 => ECLevel::Low,
            2 => ECLevel::Medium,
            3 => ECLevel::High,
            _ => ECLevel::None,
        };

        let stealth_level = match (*params).stealth_level {
            0 => StealthLevel::Low,
            1 => StealthLevel::Medium,
            2 => StealthLevel::High,
            3 => StealthLevel::Maximum,
            _ => StealthLevel::Medium,
        };

        let embed_params = EmbedParams {
            encryption_key,
            error_correction_level,
            stealth_level,
            use_gpu: (*params).use_gpu != 0,
            format_specific: HashMap::new(),
        };

        // Create carrier data
        let mut carrier = CarrierData {
            format: DataFormat::FASTQ,
            raw_data: quality_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        // Perform embedding
        let registry = HandlerRegistry::new();
        let handler = match registry.get_handler(&DataFormat::FASTQ) {
            Some(h) => h,
            None => return SteganoResult::ErrorLayerNotSupported,
        };

        let embed_result = match runtime.block_on(handler.embed(&mut carrier, payload, &embed_params)) {
            Ok(r) => r,
            Err(e) => {
                return match e {
                    SteganoError::InsufficientCapacity { .. } => SteganoResult::ErrorInsufficientCapacity,
                    SteganoError::UnsupportedFormat(_) => SteganoResult::ErrorLayerNotSupported,
                    SteganoError::IntegrityError(_) => SteganoResult::ErrorCorruptedData,
                    _ => SteganoResult::ErrorUnknown,
                };
            }
        };

        // Copy result to output buffer
        let output_size = carrier.raw_data.len().min(output_buffer_size as usize);
        ptr::copy_nonoverlapping(carrier.raw_data.as_ptr(), output_buffer, output_size);

        // Fill result structure
        (*result).success = if embed_result.success { 1 } else { 0 };
        (*result).bytes_embedded = embed_result.bytes_embedded as c_int;
        (*result).carrier_modified = if embed_result.carrier_modified { 1 } else { 0 };
        (*result).embedding_time_ms = embed_result.performance_stats.embedding_time_ms as c_int;
        (*result).memory_usage_bytes = embed_result.performance_stats.memory_usage_bytes as c_int;

        SteganoResult::Success
    }
}

/// Extract fingerprint data from FASTQ quality scores
#[no_mangle]
pub extern "C" fn stegano_extract_fastq_qualities(
    quality_scores: *const c_uchar,
    quality_len: c_int,
    params: *const EmbedParamsFFI,
    customer_id_buffer: *mut c_char,
    customer_id_buffer_size: c_int,
    dataset_id_buffer: *mut c_char,
    dataset_id_buffer_size: c_int,
    timestamp: *mut i64,
) -> SteganoResult {
    if quality_scores.is_null() || params.is_null() || customer_id_buffer.is_null() || 
       dataset_id_buffer.is_null() || timestamp.is_null() || quality_len <= 0 {
        return SteganoResult::ErrorInvalidInput;
    }

    unsafe {
        let runtime = match &RUNTIME {
            Some(rt) => rt,
            None => return SteganoResult::ErrorUnknown,
        };

        // Convert input parameters
        let quality_slice = slice::from_raw_parts(quality_scores, quality_len as usize);

        let encryption_key = if (*params).encryption_key.is_null() {
            vec![]
        } else {
            slice::from_raw_parts((*params).encryption_key, (*params).encryption_key_len as usize).to_vec()
        };

        let error_correction_level = match (*params).error_correction_level {
            0 => ECLevel::None,
            1 => ECLevel::Low,
            2 => ECLevel::Medium,
            3 => ECLevel::High,
            _ => ECLevel::None,
        };

        let stealth_level = match (*params).stealth_level {
            0 => StealthLevel::Low,
            1 => StealthLevel::Medium,
            2 => StealthLevel::High,
            3 => StealthLevel::Maximum,
            _ => StealthLevel::Medium,
        };

        let embed_params = EmbedParams {
            encryption_key,
            error_correction_level,
            stealth_level,
            use_gpu: (*params).use_gpu != 0,
            format_specific: HashMap::new(),
        };

        // Create carrier data
        let carrier = CarrierData {
            format: DataFormat::FASTQ,
            raw_data: quality_slice.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        // Perform extraction
        let registry = HandlerRegistry::new();
        let handler = match registry.get_handler(&DataFormat::FASTQ) {
            Some(h) => h,
            None => return SteganoResult::ErrorLayerNotSupported,
        };

        let extracted_data = match runtime.block_on(handler.extract(&carrier, &embed_params)) {
            Ok(data) => data,
            Err(e) => {
                return match e {
                    SteganoError::IntegrityError(_) => SteganoResult::ErrorCorruptedData,
                    SteganoError::UnsupportedFormat(_) => SteganoResult::ErrorLayerNotSupported,
                    _ => SteganoResult::ErrorUnknown,
                };
            }
        };

        // Parse extracted fingerprint data
        let fingerprint_str = match String::from_utf8(extracted_data) {
            Ok(s) => s,
            Err(_) => return SteganoResult::ErrorCorruptedData,
        };

        let parts: Vec<&str> = fingerprint_str.split(':').collect();
        if parts.len() < 3 {
            return SteganoResult::ErrorCorruptedData;
        }

        // Copy customer ID
        let customer_id_bytes = parts[0].as_bytes();
        let customer_copy_len = customer_id_bytes.len().min((customer_id_buffer_size - 1) as usize);
        ptr::copy_nonoverlapping(customer_id_bytes.as_ptr(), customer_id_buffer as *mut u8, customer_copy_len);
        *customer_id_buffer.add(customer_copy_len) = 0; // Null terminator

        // Copy dataset ID
        let dataset_id_bytes = parts[1].as_bytes();
        let dataset_copy_len = dataset_id_bytes.len().min((dataset_id_buffer_size - 1) as usize);
        ptr::copy_nonoverlapping(dataset_id_bytes.as_ptr(), dataset_id_buffer as *mut u8, dataset_copy_len);
        *dataset_id_buffer.add(dataset_copy_len) = 0; // Null terminator

        // Parse timestamp
        *timestamp = parts[2].parse::<i64>().unwrap_or(0);

        SteganoResult::Success
    }
}

/// Verify fingerprint integrity in FASTQ quality scores
#[no_mangle]
pub extern "C" fn stegano_verify_fastq_qualities(
    quality_scores: *const c_uchar,
    quality_len: c_int,
    expected_customer_id: *const c_char,
    params: *const EmbedParamsFFI,
) -> SteganoResult {
    if quality_scores.is_null() || expected_customer_id.is_null() || params.is_null() || quality_len <= 0 {
        return SteganoResult::ErrorInvalidInput;
    }

    unsafe {
        let expected_customer = match CStr::from_ptr(expected_customer_id).to_str() {
            Ok(s) => s,
            Err(_) => return SteganoResult::ErrorInvalidInput,
        };

        // First extract the fingerprint
        let mut customer_buffer: [c_char; 256] = [0; 256];
        let mut dataset_buffer: [c_char; 256] = [0; 256];
        let mut timestamp: i64 = 0;

        let extract_result = stegano_extract_fastq_qualities(
            quality_scores,
            quality_len,
            params,
            customer_buffer.as_mut_ptr(),
            256,
            dataset_buffer.as_mut_ptr(),
            256,
            &mut timestamp,
        );

        if extract_result != SteganoResult::Success {
            return extract_result;
        }

        // Verify customer ID matches
        let extracted_customer = match CStr::from_ptr(customer_buffer.as_ptr()).to_str() {
            Ok(s) => s,
            Err(_) => return SteganoResult::ErrorCorruptedData,
        };

        if extracted_customer == expected_customer {
            SteganoResult::Success
        } else {
            SteganoResult::ErrorTamperDetected
        }
    }
}

/// Generate a cryptographic fingerprint for a dataset
#[no_mangle]
pub extern "C" fn stegano_generate_fingerprint(
    customer_id: *const c_char,
    dataset_id: *const c_char,
    timestamp: i64,
    fingerprint_buffer: *mut c_char,
    fingerprint_buffer_size: c_int,
) -> SteganoResult {
    if customer_id.is_null() || fingerprint_buffer.is_null() || fingerprint_buffer_size <= 0 {
        return SteganoResult::ErrorInvalidInput;
    }

    unsafe {
        let customer = match CStr::from_ptr(customer_id).to_str() {
            Ok(s) => s,
            Err(_) => return SteganoResult::ErrorInvalidInput,
        };

        let dataset = if dataset_id.is_null() {
            ""
        } else {
            match CStr::from_ptr(dataset_id).to_str() {
                Ok(s) => s,
                Err(_) => return SteganoResult::ErrorInvalidInput,
            }
        };

        let fingerprint = format!("{}:{}:{}", customer, dataset, timestamp);
        let fingerprint_bytes = fingerprint.as_bytes();
        
        let copy_len = fingerprint_bytes.len().min((fingerprint_buffer_size - 1) as usize);
        ptr::copy_nonoverlapping(fingerprint_bytes.as_ptr(), fingerprint_buffer as *mut u8, copy_len);
        *fingerprint_buffer.add(copy_len) = 0; // Null terminator

        SteganoResult::Success
    }
}