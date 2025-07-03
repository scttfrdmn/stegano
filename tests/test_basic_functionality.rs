//! Basic functionality integration tests

use stegano::*;
use std::collections::HashMap;

#[tokio::test]
async fn test_basic_steganography_roundtrip() {
    // Create test carrier data
    let mut carrier_data = vec![0x42u8; 2048]; // 2048 bytes of data
    for (i, byte) in carrier_data.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let mut carrier = CarrierData {
        format: DataFormat::BINARY,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let secret_message = b"Test secret message for steganography";
    
    // Setup
    let crypto = crypto::CryptoLayer::new_from_password("test_password").unwrap();
    let encrypted_payload = crypto.encrypt(secret_message).unwrap();
    let serialized_payload = crypto::CryptoLayer::serialize_payload(&encrypted_payload);

    let params = EmbedParams {
        encryption_key: b"test_password".to_vec(),
        error_correction_level: ECLevel::Low,
        stealth_level: StealthLevel::Medium,
        use_gpu: false,
        format_specific: HashMap::new(),
    };

    // Get handler
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::BINARY).unwrap();

    // Test capacity estimation
    let capacity = handler.estimate_capacity(&carrier).unwrap();
    assert!(capacity.total_bits > 0);
    assert!(capacity.recommended_bits <= capacity.total_bits);

    // Test embedding
    let embed_result = handler.embed(&mut carrier, &serialized_payload, &params).await.unwrap();
    assert!(embed_result.success);
    assert_eq!(embed_result.bytes_embedded, serialized_payload.len());

    // Test integrity verification
    let integrity_ok = handler.verify_integrity(&carrier).unwrap();
    assert!(integrity_ok);

    // Test extraction
    let extracted_payload = handler.extract(&carrier, &params).await.unwrap();
    assert_eq!(extracted_payload.len(), serialized_payload.len());

    // Test decryption
    let deserialized_payload = crypto::CryptoLayer::deserialize_payload(&extracted_payload).unwrap();
    let decrypted_message = crypto.decrypt(&deserialized_payload).unwrap();
    
    assert_eq!(decrypted_message, secret_message);
}

#[tokio::test]
async fn test_format_detection() {
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::BINARY).unwrap();

    // Test PNG detection
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(handler.detect_format(&png_header), Some(DataFormat::PNG));

    // Test FASTA detection
    let fasta_data = b">sequence1\nATCGATCG\n";
    assert_eq!(handler.detect_format(fasta_data), Some(DataFormat::FASTA));

    // Test binary fallback
    let binary_data = [0x00, 0x01, 0x02, 0x03];
    assert_eq!(handler.detect_format(&binary_data), Some(DataFormat::BINARY));
}

#[test]
fn test_crypto_layer() {
    let crypto = crypto::CryptoLayer::new_from_password("test_password").unwrap();
    let message = b"Hello, cryptographic world!";
    
    // Test encryption
    let encrypted = crypto.encrypt(message).unwrap();
    assert_ne!(encrypted.encrypted_data, message);
    assert_eq!(encrypted.nonce.len(), 12);
    assert_eq!(encrypted.hash.len(), 32);
    
    // Test decryption
    let decrypted = crypto.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, message);
    
    // Test serialization round-trip
    let serialized = crypto::CryptoLayer::serialize_payload(&encrypted);
    let deserialized = crypto::CryptoLayer::deserialize_payload(&serialized).unwrap();
    let final_decrypted = crypto.decrypt(&deserialized).unwrap();
    assert_eq!(final_decrypted, message);
}