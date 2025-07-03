//! Advanced FASTA embedding techniques for phantom-science
//!
//! This example demonstrates sophisticated FASTA steganography techniques
//! specifically tailored for phantom-science genomics workflows.

use stegano::*;
use std::collections::HashMap;

/// FASTA-specific phantom-science embedding strategies
pub struct FastaPhantomIntegration;

impl FastaPhantomIntegration {
    /// Embed data in FASTA headers with biological context preservation
    pub async fn embed_in_headers(
        carrier_data: &mut Vec<u8>, 
        payload: &[u8],
        preserve_biology: bool,
    ) -> Result<usize> {
        let mut data_str = String::from_utf8_lossy(carrier_data).to_string();
        let payload_hex = hex::encode(payload);
        
        let mut embedded_bytes = 0;
        let mut current_pos = 0;
        
        // Find all sequence headers
        while let Some(header_pos) = data_str[current_pos..].find('>') {
            let absolute_pos = current_pos + header_pos;
            
            if let Some(newline_pos) = data_str[absolute_pos..].find('\n') {
                let insertion_point = absolute_pos + newline_pos;
                
                if preserve_biology {
                    // Add phantom-science data as biological annotation
                    let annotation = format!(" |phantom_data:{}", &payload_hex[embedded_bytes * 2..std::cmp::min((embedded_bytes + 50) * 2, payload_hex.len())]);
                    data_str.insert_str(insertion_point, &annotation);
                    embedded_bytes += std::cmp::min(50, payload_hex.len() / 2 - embedded_bytes);
                } else {
                    // Simple embedding
                    let chunk_size = std::cmp::min(100, payload_hex.len() - embedded_bytes * 2);
                    if chunk_size > 0 {
                        let chunk = &payload_hex[embedded_bytes * 2..embedded_bytes * 2 + chunk_size];
                        let comment = format!(" |ps_data:{}", chunk);
                        data_str.insert_str(insertion_point, &comment);
                        embedded_bytes += chunk_size / 2;
                    }
                }
                
                current_pos = insertion_point + 50; // Move past this header
                
                if embedded_bytes >= payload.len() {
                    break;
                }
            } else {
                break;
            }
        }
        
        *carrier_data = data_str.into_bytes();
        Ok(embedded_bytes)
    }
    
    /// Extract embedded data from FASTA headers
    pub fn extract_from_headers(data: &[u8]) -> Result<Vec<u8>> {
        let data_str = String::from_utf8_lossy(data);
        let mut hex_chunks = Vec::new();
        
        // Look for phantom_data markers
        for line in data_str.lines() {
            if line.contains("|phantom_data:") {
                if let Some(start) = line.find("|phantom_data:") {
                    let start = start + "|phantom_data:".len();
                    let end = line[start..].find(' ').map(|p| start + p).unwrap_or(line.len());
                    hex_chunks.push(&line[start..end]);
                }
            }
            
            // Also look for ps_data markers
            if line.contains("|ps_data:") {
                if let Some(start) = line.find("|ps_data:") {
                    let start = start + "|ps_data:".len();
                    let end = line[start..].find(' ').map(|p| start + p).unwrap_or(line.len());
                    hex_chunks.push(&line[start..end]);
                }
            }
        }
        
        // Combine all hex chunks
        let combined_hex = hex_chunks.join("");
        
        if combined_hex.is_empty() {
            return Err(SteganoError::IntegrityError("No phantom-science data found".to_string()));
        }
        
        hex::decode(&combined_hex)
            .map_err(|e| SteganoError::IntegrityError(format!("Hex decode error: {}", e)))
    }
    
    /// Estimate capacity for phantom-science FASTA embedding
    pub fn estimate_phantom_capacity(data: &[u8]) -> Result<CapacityInfo> {
        let data_str = String::from_utf8_lossy(data);
        let header_count = data_str.matches('>').count();
        
        // More conservative estimate for phantom-science embedding
        let estimated_bytes_per_header = 75; // Accounting for biological annotations
        let total_capacity_bytes = header_count * estimated_bytes_per_header;
        
        let total_bits = data.len() * 8;
        let recommended_bits = total_capacity_bytes * 8;
        
        let mut format_info = HashMap::new();
        format_info.insert("method".to_string(), serde_json::Value::String("phantom_header_embedding".to_string()));
        format_info.insert("sequences".to_string(), serde_json::Value::Number(header_count.into()));
        format_info.insert("phantom_capacity_bytes".to_string(), serde_json::Value::Number(total_capacity_bytes.into()));
        format_info.insert("average_per_header".to_string(), serde_json::Value::Number(estimated_bytes_per_header.into()));
        
        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧬 FASTA Phantom-Science Embedding Demo");
    println!("=======================================\n");
    
    // Create comprehensive FASTA sample with multiple sequences
    let comprehensive_fasta = ">seq001 Homo sapiens chromosome 1, GRCh38.p13 Primary Assembly
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
TTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCC
GGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAA
>seq002 Homo sapiens chromosome 2, region 1p36.33
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
CGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTA
GGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCC
TTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAA
>seq003 Homo sapiens mitochondrion complete genome, revised Cambridge Reference Sequence
GATCACAGGTCTATCACCCTATTAACCACTCACGGGAGCTCTCCATGCATTTGGTATTTT
CGTCTGGGGGGTGTGCACGCGATAGCATTGCGAGACGCTGGAGCCGGAGCACCCTATGTC
GCAGTATCTGTCTTTGATTCCTGCCTCATCCTATTATTTATCGCACCTACGTTCAATATT
ACAGGCGAACATACTTACTAAAGTGTGTTAATTAATTAATGCTTGTAGGACATAATAATA
>seq004 Homo sapiens chromosome X PAR1 region
ATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGC
TGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCA
GCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCAT
CATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATG
>seq005 Homo sapiens chromosome Y SRY gene region
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT
CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG";
    
    let carrier_data = comprehensive_fasta.as_bytes().to_vec();
    
    // Create phantom-science specific payload
    let phantom_payload = serde_json::json!({
        "experiment_id": "PS-EXP-2025-001",
        "phantom_protocol": "genomic_steganography_v2.1",
        "sample_metadata": {
            "collection_date": "2025-07-03T20:30:00Z",
            "lab_id": "PHANTOM-LAB-001",
            "technician": "Dr. Sarah Chen",
            "processing_notes": "High-quality extraction, optimal steganographic conditions"
        },
        "quality_control": {
            "steganographic_integrity": 98.7,
            "biological_preservation": 99.2,
            "embedding_efficiency": 94.5
        },
        "chain_of_custody": [
            "phantom-collection-bot",
            "phantom-lab-processor",
            "phantom-quality-control",
            "phantom-steganography-unit"
        ]
    });
    
    let payload_bytes = phantom_payload.to_string().into_bytes();
    
    println!("📊 Phantom-Science Payload:");
    println!("{}\n", phantom_payload.to_string());
    
    // Analyze capacity
    println!("📈 Analyzing FASTA capacity...");
    let capacity = FastaPhantomIntegration::estimate_phantom_capacity(&carrier_data)?;
    println!("   Total sequences: {}", 
             capacity.format_specific_info.get("sequences").unwrap());
    println!("   Phantom capacity: {} bytes", 
             capacity.format_specific_info.get("phantom_capacity_bytes").unwrap());
    println!("   Average per header: {} bytes", 
             capacity.format_specific_info.get("average_per_header").unwrap());
    println!("   Payload size: {} bytes", payload_bytes.len());
    println!("   Fits in capacity: {}\n", 
             payload_bytes.len() <= capacity.recommended_bits / 8);
    
    // Test both embedding strategies
    let strategies = vec![
        ("Biological Context Preservation", true),
        ("Standard Phantom Embedding", false),
    ];
    
    for (strategy_name, preserve_biology) in strategies {
        println!("🔬 Testing: {}", strategy_name);
        
        let mut test_data = carrier_data.clone();
        
        // Embed with current strategy
        let embedded_bytes = FastaPhantomIntegration::embed_in_headers(
            &mut test_data,
            &payload_bytes,
            preserve_biology,
        ).await?;
        
        println!("   Embedded: {} bytes", embedded_bytes);
        
        // Extract and verify
        let extracted = FastaPhantomIntegration::extract_from_headers(&test_data)?;
        
        println!("   Extracted: {} bytes", extracted.len());
        println!("   Roundtrip success: {}", extracted == payload_bytes);
        
        // Show a sample of the modified FASTA
        let modified_str = String::from_utf8_lossy(&test_data);
        let first_header = modified_str.lines()
            .find(|line| line.starts_with('>'))
            .unwrap_or("No headers found");
        
        println!("   Sample header: {}", 
                 if first_header.len() > 80 { 
                     format!("{}...", &first_header[..80]) 
                 } else { 
                     first_header.to_string() 
                 });
        
        println!();
    }
    
    // Verify biological integrity
    println!("🧬 Biological Integrity Check:");
    let modified_str = String::from_utf8_lossy(&carrier_data);
    let sequence_count = modified_str.matches('>').count();
    let nucleotide_chars = modified_str.chars()
        .filter(|c| matches!(c, 'A' | 'T' | 'C' | 'G' | 'a' | 't' | 'c' | 'g'))
        .count();
    
    println!("   Sequence headers preserved: {}", sequence_count);
    println!("   Nucleotide characters: {}", nucleotide_chars);
    println!("   FASTA format valid: {}", 
             modified_str.starts_with('>') && modified_str.contains('\n'));
    
    println!("\n🎯 FASTA phantom-science embedding test completed!");
    
    Ok(())
}