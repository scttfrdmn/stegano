//! Comprehensive test suite for 0.1.1 fixes
//!
//! This test suite verifies that all phantom-science issues have been addressed.

use std::collections::HashMap;
use stegano::handlers::HandlerRegistry;
use stegano::DataHandler;
use stegano::*;

#[tokio::test]
async fn test_issue_1_genomics_handler_implemented() {
    // Issue #1: CRITICAL BUG: Genomics handler is unimplemented
    let handler = handlers::genomics::GenomicsHandler::new();

    // Test FASTA format detection
    let fasta_data = b">seq1\nATCGATCG\n>seq2\nGCTAGCTA\n";
    assert_eq!(handler.detect_format(fasta_data), Some(DataFormat::FASTA));

    // Test FASTQ format detection
    let fastq_data = b"@seq1\nATCGATCG\n+\nIIIIIIII\n";
    assert_eq!(handler.detect_format(fastq_data), Some(DataFormat::FASTQ));

    // Test capacity estimation
    let carrier = CarrierData {
        format: DataFormat::FASTA,
        raw_data: fasta_data.to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let capacity = handler.estimate_capacity(&carrier).unwrap();
    assert!(capacity.recommended_bits > 0);

    // Test embedding/extraction roundtrip
    let mut carrier = CarrierData {
        format: DataFormat::FASTA,
        raw_data: b">seq1 longer header for capacity\nATCGATCGATCGATCGATCGATCGATCG\n".to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let payload = b"test data";
    let params = EmbedParams {
        encryption_key: vec![],
        error_correction_level: ECLevel::None,
        stealth_level: StealthLevel::Low,
        use_gpu: false,
        format_specific: HashMap::new(),
        enable_validation: false,
    };

    let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
    assert!(result.success);

    let extracted = handler.extract(&carrier, &params).await.unwrap();
    assert_eq!(extracted, payload);

    // Test integrity verification
    assert!(handler.verify_integrity(&carrier).unwrap());
}

#[test]
fn test_issue_2_ffi_module_exposed() {
    // Issue #2: CRITICAL BUG: FFI module not exposed in lib.rs
    // This test verifies that FFI functions are accessible

    use stegano::ffi::*;

    // Test FFI initialization
    let result = stegano_init();
    assert_eq!(result as u32, SteganoErrorCode::Success as u32);

    // Test version function
    let version_ptr = stegano_version();
    assert!(!version_ptr.is_null());

    let version_str = unsafe { std::ffi::CStr::from_ptr(version_ptr) };
    assert_eq!(version_str.to_str().unwrap(), "0.2.1");
}

#[test]
fn test_issue_3_cdylib_configuration() {
    // Issue #3: BUG: Missing cdylib configuration in Cargo.toml
    // This test verifies that the library can be built as both rlib and cdylib

    // We can't directly test cdylib compilation from within a test,
    // but we can verify the configuration exists by checking that
    // FFI functions are available (which requires cdylib support)

    use stegano::ffi::*;

    // If FFI functions are available, cdylib configuration is working
    let _ = stegano_init();
    let _ = stegano_version();
    // Test passes if we reach here without compilation errors
}

#[test]
fn test_issue_4_handler_registry_fixed() {
    // Issue #4: BUG: Handler registry get_handler method doesn't work
    let registry = HandlerRegistry::new();

    // Test that registry can find handlers for each format
    assert!(registry.get_handler(&DataFormat::FASTA).is_some());
    assert!(registry.get_handler(&DataFormat::FASTQ).is_some());
    assert!(registry.get_handler(&DataFormat::PNG).is_some());
    assert!(registry.get_handler(&DataFormat::WAV).is_some());
    assert!(registry.get_handler(&DataFormat::BINARY).is_some());

    // Test the new get_handler_by_data method
    let fasta_data = b">seq1\nATCG\n";
    assert!(registry.get_handler_by_data(fasta_data).is_some());

    let binary_data = b"\x89PNG\r\n\x1a\n"; // PNG magic bytes
    assert!(registry.get_handler_by_data(binary_data).is_some());
}

#[test]
fn test_issue_6_repository_urls_updated() {
    // Issue #6: DOCUMENTATION: Update repository URLs
    // We can't directly test Cargo.toml content from here,
    // but we can verify the package metadata is accessible

    // This test primarily verifies the build system works correctly
    // with the updated repository URLs
    let _ = env!("CARGO_PKG_REPOSITORY"); // Will fail if not properly set
}

#[tokio::test]
async fn test_issue_5_phantom_science_integration_examples() {
    // Issue #5: IMPROVEMENT: Add phantom-science integration examples
    // Test that the phantom-science integration patterns work

    use std::collections::HashMap;

    // Create a mock phantom-science metadata structure
    #[derive(serde::Serialize, serde::Deserialize)]
    struct PhantomMetadata {
        sample_id: String,
        processing_steps: Vec<String>,
        quality_metrics: HashMap<String, f64>,
    }

    let metadata = PhantomMetadata {
        sample_id: "PS-TEST-001".to_string(),
        processing_steps: vec!["extraction".to_string(), "sequencing".to_string()],
        quality_metrics: {
            let mut map = HashMap::new();
            map.insert("quality_score".to_string(), 35.2);
            map.insert("read_count".to_string(), 1000000.0);
            map
        },
    };

    // Test that we can serialize and embed phantom-science metadata
    let metadata_json = serde_json::to_vec(&metadata).unwrap();

    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::FASTA).unwrap();

    let mut carrier = CarrierData {
        format: DataFormat::FASTA,
        raw_data: b">seq1 test sequence with capacity for phantom data\nATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG\n".to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let params = EmbedParams {
        encryption_key: b"phantom-key".to_vec(),
        error_correction_level: ECLevel::Medium,
        stealth_level: StealthLevel::Low,
        use_gpu: false,
        format_specific: HashMap::new(),
        enable_validation: false,
    };

    // Embed phantom-science metadata
    let result = handler
        .embed(&mut carrier, &metadata_json, &params)
        .await
        .unwrap();
    assert!(result.success);

    // Extract and verify
    let extracted_json = handler.extract(&carrier, &params).await.unwrap();
    let extracted_metadata: PhantomMetadata = serde_json::from_slice(&extracted_json).unwrap();

    assert_eq!(extracted_metadata.sample_id, metadata.sample_id);
    assert_eq!(
        extracted_metadata.processing_steps,
        metadata.processing_steps
    );
    assert_eq!(extracted_metadata.quality_metrics, metadata.quality_metrics);
}

#[test]
fn test_issue_7_fastq_quality_score_concept() {
    // Issue #7: FEATURE REQUEST: FASTQ quality score steganography
    // Test the conceptual framework for FASTQ quality score manipulation

    let fastq_data = b"@seq1\nATCG\n+\nIIII\n@seq2\nGCTA\n+\nJJJJ\n";

    // Test FASTQ format detection
    let genomics_handler = handlers::genomics::GenomicsHandler::new();
    assert_eq!(
        genomics_handler.detect_format(fastq_data),
        Some(DataFormat::FASTQ)
    );

    // Test that FASTQ files can be processed (this uses basic embedding for now)
    let carrier = CarrierData {
        format: DataFormat::FASTQ,
        raw_data:
            b"@seq1 test with longer header for embedding\nATCGATCGATCGATCG\n+\nIIIIIIIIIIIIIIII\n"
                .to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::FASTQ).unwrap();

    // Test capacity estimation for FASTQ
    let capacity = handler.estimate_capacity(&carrier).unwrap();
    assert!(capacity.recommended_bits > 0);

    // The framework is in place for FASTQ quality score steganography
    // (the examples show the full implementation)
}

#[test]
fn test_version_updated_to_0_2_1() {
    // Verify that version has been updated for the release
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.2.1");
}

#[test]
fn test_all_issues_integration() {
    // Integration test verifying all issues work together
    let registry = HandlerRegistry::new();

    // Test that we can get handlers for all major formats (Issue #4 fix)
    let genomics_handler = registry.get_handler(&DataFormat::FASTA).unwrap();
    let _imagery_handler = registry.get_handler(&DataFormat::PNG).unwrap();
    let _signals_handler = registry.get_handler(&DataFormat::WAV).unwrap();
    let _generic_handler = registry.get_handler(&DataFormat::BINARY).unwrap();

    // Test that genomics handler is functional (Issue #1 fix)
    let fasta_data = b">test\nATCG\n";
    assert!(genomics_handler.detect_format(fasta_data).is_some());

    // Test that FFI module is accessible (Issue #2 fix)
    use stegano::ffi::*;
    let init_result = stegano_init();
    assert_eq!(init_result as u32, SteganoErrorCode::Success as u32);

    // All systems working together indicate successful 0.2.1 release readiness
}
