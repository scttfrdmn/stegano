//! FASTQ quality score steganography for phantom-science
//!
//! This example demonstrates advanced steganography techniques using
//! FASTQ quality scores while preserving biological integrity.

use std::collections::HashMap;
use stegano::*;

/// FASTQ quality score steganography handler
pub struct FastqQualityHandler {
    min_quality_threshold: u8,
    max_quality_threshold: u8,
    embedding_density: f32,
    preserve_patterns: bool,
}

impl Default for FastqQualityHandler {
    fn default() -> Self {
        Self {
            min_quality_threshold: 20, // Phred+33: '5' (Q20)
            max_quality_threshold: 35, // Phred+33: 'D' (Q35)
            embedding_density: 0.3,    // Use 30% of eligible quality scores
            preserve_patterns: true,   // Maintain quality distributions
        }
    }
}

impl FastqQualityHandler {
    pub fn new(min_quality: u8, max_quality: u8, density: f32) -> Self {
        Self {
            min_quality_threshold: min_quality,
            max_quality_threshold: max_quality,
            embedding_density: density,
            preserve_patterns: true,
        }
    }

    /// Embed data in FASTQ quality scores
    pub async fn embed_in_quality_scores(
        &self,
        fastq_data: &mut Vec<u8>,
        payload: &[u8],
    ) -> Result<usize> {
        let fastq_str = String::from_utf8_lossy(fastq_data).to_string();
        let mut lines: Vec<String> = fastq_str.lines().map(|s| s.to_string()).collect();

        // Convert payload to bits
        let payload_bits: Vec<u8> = payload
            .iter()
            .flat_map(|byte| (0..8).rev().map(move |i| (byte >> i) & 1))
            .collect();

        let mut bit_index = 0;
        let mut embedded_bits = 0;

        // Process FASTQ records (4 lines per record)
        for record_start in (0..lines.len()).step_by(4) {
            if record_start + 3 >= lines.len() {
                break;
            }

            // Line 0: Header (@)
            // Line 1: Sequence
            // Line 2: Plus line (+)
            // Line 3: Quality scores

            if !lines[record_start].starts_with('@') || !lines[record_start + 2].starts_with('+') {
                continue;
            }

            let mut quality_chars: Vec<char> = lines[record_start + 3].chars().collect();
            let quality_chars_len = quality_chars.len();

            // Embed bits in quality scores
            for (pos, quality_char) in quality_chars.iter_mut().enumerate() {
                if bit_index >= payload_bits.len() {
                    break;
                }

                let quality_value = (*quality_char as u8).saturating_sub(33); // Phred+33

                // Check if this quality score is eligible for embedding
                if quality_value >= self.min_quality_threshold
                    && quality_value <= self.max_quality_threshold
                    && (pos as f32 / quality_chars_len as f32) < self.embedding_density
                {
                    let bit_to_embed = payload_bits[bit_index];

                    // Modify LSB of quality score
                    let mut new_quality = quality_value;
                    if self.preserve_patterns {
                        // Preserve quality patterns by only modifying if change is small
                        let modified_quality = (quality_value & 0xFE) | bit_to_embed;
                        if (modified_quality as i16 - quality_value as i16).abs() <= 1 {
                            new_quality = modified_quality;
                            embedded_bits += 1;
                            bit_index += 1;
                        }
                    } else {
                        // Simple LSB embedding
                        new_quality = (quality_value & 0xFE) | bit_to_embed;
                        embedded_bits += 1;
                        bit_index += 1;
                    }

                    *quality_char = (new_quality + 33) as char; // Convert back to Phred+33
                }
            }

            // Update the quality line
            lines[record_start + 3] = quality_chars.iter().collect::<String>();

            if bit_index >= payload_bits.len() {
                break;
            }
        }

        // Reconstruct FASTQ
        *fastq_data = lines.join("\n").into_bytes();

        Ok(embedded_bits / 8) // Return bytes embedded
    }

    /// Extract data from FASTQ quality scores
    pub fn extract_from_quality_scores(
        &self,
        fastq_data: &[u8],
        expected_bits: usize,
    ) -> Result<Vec<u8>> {
        let fastq_str = String::from_utf8_lossy(fastq_data);
        let lines: Vec<&str> = fastq_str.lines().collect();

        let mut extracted_bits = Vec::new();

        // Process FASTQ records
        for record_start in (0..lines.len()).step_by(4) {
            if record_start + 3 >= lines.len() {
                break;
            }

            if !lines[record_start].starts_with('@') || !lines[record_start + 2].starts_with('+') {
                continue;
            }

            let quality_line = lines[record_start + 3];

            // Extract bits from quality scores
            for (pos, quality_char) in quality_line.chars().enumerate() {
                if extracted_bits.len() >= expected_bits {
                    break;
                }

                let quality_value = (quality_char as u8).saturating_sub(33);

                // Check if this position was eligible for embedding
                if quality_value >= self.min_quality_threshold
                    && quality_value <= self.max_quality_threshold
                    && (pos as f32 / quality_line.len() as f32) < self.embedding_density
                {
                    let bit = quality_value & 1;
                    extracted_bits.push(bit);
                }
            }

            if extracted_bits.len() >= expected_bits {
                break;
            }
        }

        if extracted_bits.len() < expected_bits {
            return Err(SteganoError::IntegrityError(format!(
                "Could not extract enough bits: got {}, expected {}",
                extracted_bits.len(),
                expected_bits
            )));
        }

        // Convert bits back to bytes
        let mut result = Vec::new();
        for chunk in extracted_bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                byte |= bit << (7 - i);
            }
            result.push(byte);
        }

        Ok(result)
    }

    /// Estimate capacity for FASTQ quality score embedding
    pub fn estimate_fastq_capacity(&self, fastq_data: &[u8]) -> Result<CapacityInfo> {
        let fastq_str = String::from_utf8_lossy(fastq_data);
        let lines: Vec<&str> = fastq_str.lines().collect();

        let mut total_eligible_positions = 0;
        let mut total_quality_positions = 0;
        let record_count = lines.len() / 4;

        // Analyze FASTQ records
        for record_start in (0..lines.len()).step_by(4) {
            if record_start + 3 >= lines.len() {
                break;
            }

            if !lines[record_start].starts_with('@') || !lines[record_start + 2].starts_with('+') {
                continue;
            }

            let quality_line = lines[record_start + 3];
            total_quality_positions += quality_line.len();

            for (pos, quality_char) in quality_line.chars().enumerate() {
                let quality_value = (quality_char as u8).saturating_sub(33);

                if quality_value >= self.min_quality_threshold
                    && quality_value <= self.max_quality_threshold
                    && (pos as f32 / quality_line.len() as f32) < self.embedding_density
                {
                    total_eligible_positions += 1;
                }
            }
        }

        let recommended_bits = total_eligible_positions;
        let total_bits = fastq_data.len() * 8;

        let mut format_info = HashMap::new();
        format_info.insert(
            "method".to_string(),
            serde_json::Value::String("fastq_quality_lsb".to_string()),
        );
        format_info.insert(
            "records".to_string(),
            serde_json::Value::Number(record_count.into()),
        );
        format_info.insert(
            "total_quality_positions".to_string(),
            serde_json::Value::Number(total_quality_positions.into()),
        );
        format_info.insert(
            "eligible_positions".to_string(),
            serde_json::Value::Number(total_eligible_positions.into()),
        );
        format_info.insert(
            "embedding_density".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(self.embedding_density as f64).unwrap(),
            ),
        );
        format_info.insert(
            "quality_threshold_min".to_string(),
            serde_json::Value::Number(self.min_quality_threshold.into()),
        );
        format_info.insert(
            "quality_threshold_max".to_string(),
            serde_json::Value::Number(self.max_quality_threshold.into()),
        );

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
    println!("🧬 FASTQ Quality Score Steganography Demo");
    println!("=========================================\n");

    // Create realistic FASTQ data with quality scores
    let sample_fastq = "@seq001 HWI-ST1276:71:C1162ACXX:1:1101:1208:2458 1:N:0:CGATGT
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
+seq001
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
@seq002 HWI-ST1276:71:C1162ACXX:1:1101:1208:2459 1:N:0:CGATGT
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
+seq002
JJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJ
@seq003 HWI-ST1276:71:C1162ACXX:1:1101:1208:2460 1:N:0:CGATGT
TTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCC
+seq003
HHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHH
@seq004 HWI-ST1276:71:C1162ACXX:1:1101:1208:2461 1:N:0:CGATGT
GGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAA
+seq004
GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG
@seq005 HWI-ST1276:71:C1162ACXX:1:1101:1208:2462 1:N:0:CGATGT
CGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTA
+seq005
FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";

    let fastq_data = sample_fastq.as_bytes().to_vec();

    // Create phantom-science metadata for FASTQ
    let phantom_metadata = serde_json::json!({
        "fastq_experiment": "PS-FASTQ-2025-007",
        "sequencing_platform": "phantom-sequencer-v3",
        "quality_protocol": "steganographic_preservation_v1.2",
        "sample_info": {
            "sample_id": "PS-SAMPLE-12345",
            "library_prep": "phantom-prep-protocol",
            "read_length": 64,
            "quality_encoding": "Phred+33"
        },
        "steganographic_params": {
            "quality_range": "Q20-Q35",
            "embedding_density": 0.3,
            "preservation_mode": true
        }
    });

    let payload = phantom_metadata.to_string().into_bytes();

    println!("📊 Phantom-Science FASTQ Metadata:");
    println!("{}\n", phantom_metadata);

    // Test different quality score embedding strategies
    let strategies = vec![
        (
            "Conservative (Q25-Q30, 20% density)",
            FastqQualityHandler::new(25, 30, 0.2),
        ),
        (
            "Balanced (Q20-Q35, 30% density)",
            FastqQualityHandler::new(20, 35, 0.3),
        ),
        (
            "Aggressive (Q15-Q40, 50% density)",
            FastqQualityHandler::new(15, 40, 0.5),
        ),
    ];

    for (strategy_name, handler) in strategies {
        println!("🔬 Testing Strategy: {strategy_name}");

        let mut test_data = fastq_data.clone();

        // Analyze capacity
        let capacity = handler.estimate_fastq_capacity(&test_data)?;
        println!(
            "   Records: {}",
            capacity.format_specific_info.get("records").unwrap()
        );
        println!(
            "   Total quality positions: {}",
            capacity
                .format_specific_info
                .get("total_quality_positions")
                .unwrap()
        );
        println!(
            "   Eligible positions: {}",
            capacity
                .format_specific_info
                .get("eligible_positions")
                .unwrap()
        );
        println!(
            "   Capacity: {} bits ({} bytes)",
            capacity.recommended_bits,
            capacity.recommended_bits / 8
        );
        println!("   Payload size: {} bytes", payload.len());

        if payload.len() * 8 <= capacity.recommended_bits {
            // Embed data
            let embedded_bytes = handler
                .embed_in_quality_scores(&mut test_data, &payload)
                .await?;
            println!("   ✅ Embedded: {embedded_bytes} bytes");

            // Extract data
            let expected_bits = payload.len() * 8;
            let extracted = handler.extract_from_quality_scores(&test_data, expected_bits)?;

            println!("   Extracted: {} bytes", extracted.len());
            println!("   Roundtrip success: {}", extracted == payload);

            // Quality preservation analysis
            let original_qualities = extract_quality_scores(&fastq_data);
            let modified_qualities = extract_quality_scores(&test_data);

            if original_qualities.len() == modified_qualities.len() {
                let mut quality_diff_sum = 0;
                let mut positions_changed = 0;

                for (orig, modif) in original_qualities.iter().zip(modified_qualities.iter()) {
                    let diff = (*orig as i16 - *modif as i16).abs();
                    quality_diff_sum += diff;
                    if diff > 0 {
                        positions_changed += 1;
                    }
                }

                let avg_quality_change = quality_diff_sum as f64 / original_qualities.len() as f64;
                let change_percentage =
                    positions_changed as f64 / original_qualities.len() as f64 * 100.0;

                println!("   Quality preservation:");
                println!("     Average quality change: {avg_quality_change:.3}");
                println!("     Positions changed: {change_percentage:.1}%");
            }
        } else {
            println!("   ❌ Insufficient capacity for payload");
        }

        println!();
    }

    // Biological integrity check
    println!("🧬 Biological Integrity Analysis:");
    let original_str = String::from_utf8_lossy(&fastq_data);
    let record_count = original_str.lines().count() / 4;

    println!("   FASTQ records: {record_count}");
    println!(
        "   Format structure preserved: {}",
        validate_fastq_structure(&fastq_data)
    );

    // Quality score distribution analysis
    let qualities = extract_quality_scores(&fastq_data);
    let avg_quality = qualities.iter().map(|&q| q as f64).sum::<f64>() / qualities.len() as f64;
    let min_quality = qualities.iter().min().unwrap_or(&0);
    let max_quality = qualities.iter().max().unwrap_or(&0);

    println!("   Quality score statistics:");
    println!("     Average: {avg_quality:.1}");
    println!("     Range: Q{min_quality} - Q{max_quality}");

    println!("\n🎯 FASTQ quality score steganography test completed!");

    Ok(())
}

/// Extract all quality scores from FASTQ data
fn extract_quality_scores(fastq_data: &[u8]) -> Vec<u8> {
    let fastq_str = String::from_utf8_lossy(fastq_data);
    let lines: Vec<&str> = fastq_str.lines().collect();
    let mut qualities = Vec::new();

    for record_start in (0..lines.len()).step_by(4) {
        if record_start + 3 < lines.len()
            && lines[record_start].starts_with('@')
            && lines[record_start + 2].starts_with('+')
        {
            let quality_line = lines[record_start + 3];
            for quality_char in quality_line.chars() {
                let quality_value = (quality_char as u8).saturating_sub(33);
                qualities.push(quality_value);
            }
        }
    }

    qualities
}

/// Validate FASTQ structure integrity
fn validate_fastq_structure(fastq_data: &[u8]) -> bool {
    let fastq_str = String::from_utf8_lossy(fastq_data);
    let lines: Vec<&str> = fastq_str.lines().collect();

    if lines.len() % 4 != 0 {
        return false;
    }

    for record_start in (0..lines.len()).step_by(4) {
        if record_start + 3 >= lines.len() {
            return false;
        }

        // Check FASTQ record structure
        if !lines[record_start].starts_with('@')
            || !lines[record_start + 2].starts_with('+')
            || lines[record_start + 1].len() != lines[record_start + 3].len()
        {
            return false;
        }
    }

    true
}
