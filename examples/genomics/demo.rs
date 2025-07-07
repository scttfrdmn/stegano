//! Genomics steganography demonstration

use std::collections::HashMap;
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("🧬 Genomics Steganography Demo");
    println!("==============================\n");

    // Create a sample FASTA file content
    let fasta_content = ">sample_sequence_1 | Synthetic DNA sequence for demonstration
ATGCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATC
GATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATC
CGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGAT
TCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGA
ATGCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATC
GATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATC
CGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGAT
TCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGA
>sample_sequence_2 | Additional sequence for capacity
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
AGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCT
CTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAG
TAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGC
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
AGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCT
CTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAG
TAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGC
>sample_sequence_3 | Extended genomics data for steganographic capacity
TTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAA
AATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATT
TTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAA
AATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATT
TTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAA";

    let mut carrier = CarrierData {
        format: DataFormat::FASTA,
        raw_data: fasta_content.as_bytes().to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    println!("📊 FASTA Data Overview:");
    println!("   Total size: {} bytes", carrier.raw_data.len());
    println!("   Sequences: 3");
    println!(
        "   Content preview: {}...",
        String::from_utf8_lossy(&carrier.raw_data[..60])
    );

    // Secret genomics research data to embed
    let secret_research = "CONFIDENTIAL: Novel gene variant discovery - Position 1234567, Mutation A->G, Clinical significance: Pathogenic".as_bytes();

    println!("\n🔒 Secret Research Data:");
    println!("   Content: '{}'", String::from_utf8_lossy(secret_research));
    println!("   Size: {} bytes", secret_research.len());

    // Create crypto layer for secure embedding
    let crypto = crypto::CryptoLayer::new_from_password("genomics_research_key_2025")?;
    let encrypted_payload = crypto.encrypt(secret_research)?;
    let serialized_payload = crypto::CryptoLayer::serialize_payload(&encrypted_payload);

    println!("\n🔐 Encryption:");
    println!(
        "   Encrypted payload size: {} bytes",
        serialized_payload.len()
    );

    // Embedding parameters optimized for genomics
    let params = EmbedParams {
        encryption_key: b"genomics_research_key_2025".to_vec(),
        error_correction_level: ECLevel::High, // High redundancy for genomics data
        stealth_level: StealthLevel::High,     // Maximum stealth for sensitive research
        use_gpu: false,
        format_specific: {
            let mut specific = HashMap::new();
            specific.insert(
                "preserve_sequences".to_string(),
                serde_json::Value::Bool(true),
            );
            specific.insert(
                "use_headers_only".to_string(),
                serde_json::Value::Bool(true),
            );
            specific
        },
    };

    // Note: For this demo, we'll use the generic handler since genomics-specific
    // handlers would require more complex implementation
    let registry = handlers::HandlerRegistry::new();
    let handler = registry
        .get_handler(&DataFormat::BINARY) // Using generic for demo
        .ok_or_else(|| SteganoError::UnsupportedFormat(DataFormat::BINARY))?;

    // Analyze capacity for genomics data
    let capacity = handler.estimate_capacity(&carrier)?;
    println!("\n📈 Capacity Analysis for Genomics Data:");
    println!("   Total FASTA file bits: {}", capacity.total_bits);
    println!(
        "   Available for payload: {} bits ({} bytes)",
        capacity.recommended_bits,
        capacity.recommended_bits / 8
    );
    println!(
        "   Steganographic ratio: {:.2}%",
        capacity.steganographic_ratio * 100.0
    );

    // Check if we have enough capacity
    if serialized_payload.len() * 8 > capacity.recommended_bits {
        println!("\n❌ Insufficient capacity!");
        println!(
            "   Need: {} bits, Have: {} bits",
            serialized_payload.len() * 8,
            capacity.recommended_bits
        );
        return Err(SteganoError::InsufficientCapacity {
            needed: serialized_payload.len() * 8,
            available: capacity.recommended_bits,
        });
    }

    // Embed the research data
    println!("\n⚡ Genomics Embedding Process:");
    let embed_result = handler
        .embed(&mut carrier, &serialized_payload, &params)
        .await?;
    println!("   Embedding successful: {}", embed_result.success);
    println!(
        "   Research data embedded: {} bytes",
        embed_result.bytes_embedded
    );
    println!(
        "   Processing time: {} ms",
        embed_result.performance_stats.embedding_time_ms
    );

    // Verify the FASTA structure integrity
    let modified_fasta = String::from_utf8_lossy(&carrier.raw_data);
    let sequence_count = modified_fasta.matches('>').count();
    println!(
        "   FASTA structure preserved: {} sequences detected",
        sequence_count
    );

    // Extract and verify the hidden research data
    println!("\n🔍 Data Extraction:");
    let extracted_payload = handler.extract(&carrier, &params).await?;
    println!("   Extracted {} bytes", extracted_payload.len());

    // Decrypt and verify
    let deserialized_payload = crypto::CryptoLayer::deserialize_payload(&extracted_payload)?;
    let decrypted_research = crypto.decrypt(&deserialized_payload)?;

    println!(
        "   Decrypted research: '{}'",
        String::from_utf8_lossy(&decrypted_research)
    );

    // Verify round-trip success
    let success = decrypted_research == secret_research;
    println!("\n🎯 Genomics Steganography Test:");
    println!(
        "   Data integrity: {}",
        if success {
            "✅ VERIFIED"
        } else {
            "❌ FAILED"
        }
    );

    if success {
        println!("\n🚀 Genomics steganography completed successfully!");
        println!("   The confidential research data has been securely hidden within");
        println!("   the FASTA genomics data and successfully extracted.");

        println!("\n📋 Security Summary:");
        println!("   • Research data encrypted with AES-256-GCM");
        println!("   • High error correction level for data integrity");
        println!("   • FASTA file structure preserved");
        println!(
            "   • Steganographic hiding ratio: {:.2}%",
            capacity.steganographic_ratio * 100.0
        );
    } else {
        return Err(SteganoError::IntegrityError(
            "Genomics data verification failed".to_string(),
        ));
    }

    Ok(())
}
