//! Basic steganography demonstration

use stegano::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("🔐 Scientific Data Steganography System Demo");
    println!("============================================\n");

    // Create some carrier data (simulating a binary file)
    let mut carrier_data = vec![0u8; 1024]; // 1KB of data
    for (i, byte) in carrier_data.iter_mut().enumerate() {
        *byte = (i % 256) as u8; // Fill with pattern
    }

    let mut carrier = CarrierData {
        format: DataFormat::BINARY,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    println!("📁 Carrier Data:");
    println!("   Format: {:?}", carrier.format);
    println!("   Size: {} bytes", carrier.raw_data.len());

    // Secret payload to embed
    let secret_message = "This is confidential research data that needs to be hidden!".as_bytes();
    println!("\n🔒 Secret Payload:");
    println!("   Message: '{}'", String::from_utf8_lossy(secret_message));
    println!("   Size: {} bytes", secret_message.len());

    // Create crypto layer for encryption
    let crypto = crypto::CryptoLayer::new_from_password("my_secret_password_2025")?;
    println!("\n🔐 Encryption:");
    println!("   Using AES-256-GCM with Blake3 key derivation");

    // Encrypt the payload
    let encrypted_payload = crypto.encrypt(secret_message)?;
    let serialized_payload = crypto::CryptoLayer::serialize_payload(&encrypted_payload);
    println!("   Encrypted size: {} bytes", serialized_payload.len());

    // Set up embedding parameters
    let params = EmbedParams {
        encryption_key: b"my_secret_password_2025".to_vec(),
        error_correction_level: ECLevel::Low,
        stealth_level: StealthLevel::Medium,
        use_gpu: false,
        format_specific: HashMap::new(),
    };

    // Get handler and estimate capacity
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::BINARY)
        .ok_or_else(|| SteganoError::UnsupportedFormat(DataFormat::BINARY))?;

    let capacity = handler.estimate_capacity(&carrier)?;
    println!("\n📊 Capacity Analysis:");
    println!("   Total carrier bits: {}", capacity.total_bits);
    println!("   Available for payload: {} bits ({} bytes)", 
             capacity.recommended_bits, capacity.recommended_bits / 8);
    println!("   Steganographic ratio: {:.2}%", capacity.steganographic_ratio * 100.0);

    if serialized_payload.len() * 8 > capacity.recommended_bits {
        return Err(SteganoError::InsufficientCapacity {
            needed: serialized_payload.len() * 8,
            available: capacity.recommended_bits,
        });
    }

    // Embed the encrypted payload
    println!("\n⚡ Embedding Process:");
    let embed_result = handler.embed(&mut carrier, &serialized_payload, &params).await?;
    println!("   Success: {}", embed_result.success);
    println!("   Bytes embedded: {}", embed_result.bytes_embedded);
    println!("   Embedding time: {} ms", embed_result.performance_stats.embedding_time_ms);
    println!("   Memory usage: {} bytes", embed_result.performance_stats.memory_usage_bytes);

    // Verify integrity
    let integrity_ok = handler.verify_integrity(&carrier)?;
    println!("   Integrity check: {}", if integrity_ok { "✅ PASS" } else { "❌ FAIL" });

    // Extract the payload
    println!("\n🔍 Extraction Process:");
    let extracted_payload = handler.extract(&carrier, &params).await?;
    println!("   Extracted {} bytes", extracted_payload.len());

    // Deserialize and decrypt
    let deserialized_payload = crypto::CryptoLayer::deserialize_payload(&extracted_payload)?;
    let decrypted_message = crypto.decrypt(&deserialized_payload)?;

    println!("   Decrypted message: '{}'", String::from_utf8_lossy(&decrypted_message));

    // Verify the round-trip was successful
    let success = decrypted_message == secret_message;
    println!("\n🎯 Round-trip Test:");
    println!("   Original == Extracted: {}", if success { "✅ SUCCESS" } else { "❌ FAILED" });

    if success {
        println!("\n🚀 Steganography demonstration completed successfully!");
        println!("   The secret data was successfully hidden, extracted, and verified.");
    } else {
        println!("\n❌ Steganography demonstration failed!");
        return Err(SteganoError::IntegrityError("Round-trip verification failed".to_string()));
    }

    // Show some statistics
    println!("\n📈 Performance Statistics:");
    println!("   Overhead: {:.1}% ({} bytes added)", 
             (serialized_payload.len() as f64 / secret_message.len() as f64 - 1.0) * 100.0,
             serialized_payload.len() - secret_message.len());
    println!("   Carrier modification: {:.3}% of bytes affected",
             (serialized_payload.len() * 8) as f64 / carrier.raw_data.len() as f64 * 100.0);

    Ok(())
}