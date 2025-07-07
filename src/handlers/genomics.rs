//! Genomics data format handlers with FASTA/FASTQ specific embedding

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Instant;

pub struct GenomicsHandler;

/// FASTQ quality score embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityEmbedStrategy {
    /// Conservative: 1 bit per quality character, minimal detection risk
    Conservative,
    /// Balanced: 2 bits per quality character, moderate capacity
    Balanced,
    /// Aggressive: 3-4 bits per quality character, maximum capacity
    Aggressive,
}

/// FASTQ record structure for parsing
#[derive(Debug, Clone)]
pub struct FastqRecord {
    pub header: String,
    pub sequence: String,
    pub separator: String,
    pub quality: String,
}

/// Result of FASTQ quality score embedding
#[derive(Debug, Clone)]
pub struct QualityEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: QualityEmbedStrategy,
    pub quality_chars_modified: usize,
    pub biological_integrity_preserved: bool,
}

/// Constants for quality score steganography
const MIN_QUALITY_ASCII: u8 = 33; // '!'
const MAX_QUALITY_ASCII: u8 = 126; // '~'
const TYPICAL_HIGH_QUALITY_MIN: u8 = 64; // High quality range start
const TYPICAL_HIGH_QUALITY_MAX: u8 = 105; // High quality range end

impl GenomicsHandler {
    pub fn new() -> Self {
        Self
    }

    /// FASTA-specific embedding in comment fields and sequence gaps
    fn embed_fasta(&self, data: &mut Vec<u8>, payload: &[u8]) -> Result<usize> {
        let mut data_str = String::from_utf8_lossy(data).to_string();
        let payload_hex = hex::encode(payload);

        // Find the first sequence header
        if let Some(header_pos) = data_str.find('>') {
            if let Some(newline_pos) = data_str[header_pos..].find('\n') {
                let insertion_point = header_pos + newline_pos;

                // Insert payload as a comment in the header
                let comment = format!(" | embedded_data:{}", payload_hex);
                data_str.insert_str(insertion_point, &comment);

                // Update the original data by replacing it
                *data = data_str.into_bytes();
                return Ok(payload.len());
            }
        }

        Err(SteganoError::InsufficientCapacity {
            needed: payload.len(),
            available: 0,
        })
    }

    /// Extract payload from FASTA comment fields
    fn extract_fasta(&self, data: &[u8]) -> Result<Vec<u8>> {
        let data_str = String::from_utf8_lossy(data);

        // Look for embedded data marker
        if let Some(marker_pos) = data_str.find("embedded_data:") {
            let start = marker_pos + "embedded_data:".len();

            // Find the end of the hex string (newline or space)
            let end = data_str[start..]
                .find(|c: char| c.is_whitespace())
                .map(|pos| start + pos)
                .unwrap_or(data_str.len());

            let hex_str = &data_str[start..end];

            // Decode hex payload
            if let Ok(payload) = hex::decode(hex_str) {
                return Ok(payload);
            }
        }

        Err(SteganoError::IntegrityError(
            "No embedded data found".to_string(),
        ))
    }

    /// Estimate capacity for FASTA files based on header space
    fn estimate_fasta_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        let data_str = String::from_utf8_lossy(data);
        let header_count = data_str.matches('>').count();

        // Each header can accommodate roughly 100 bytes of hex-encoded data
        let estimated_bytes = header_count * 50; // Conservative estimate
        let total_bits = data.len() * 8;
        let recommended_bits = estimated_bytes * 8;

        let mut format_info = HashMap::new();
        format_info.insert(
            "method".to_string(),
            serde_json::Value::String("header_comment".to_string()),
        );
        format_info.insert(
            "sequences".to_string(),
            serde_json::Value::Number(header_count.into()),
        );
        format_info.insert(
            "estimated_capacity_bytes".to_string(),
            serde_json::Value::Number(estimated_bytes.into()),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }

    /// Estimate capacity for FASTQ files based on quality score steganography
    fn estimate_fastq_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        self.estimate_fastq_quality_capacity(data, QualityEmbedStrategy::Balanced)
    }

    /// Parse FASTQ data into structured records
    fn parse_fastq(&self, data: &[u8]) -> Result<Vec<FastqRecord>> {
        let data_str = String::from_utf8_lossy(data);
        let lines: Vec<&str> = data_str.lines().collect();
        let mut records = Vec::new();

        if lines.len() < 4 {
            return Err(SteganoError::IntegrityError(
                "Invalid FASTQ: insufficient lines".to_string(),
            ));
        }

        let mut i = 0;
        while i + 3 < lines.len() {
            // FASTQ format: 4 lines per record
            if !lines[i].starts_with('@') {
                return Err(SteganoError::IntegrityError(format!(
                    "Invalid FASTQ header at line {}",
                    i + 1
                )));
            }
            if !lines[i + 2].starts_with('+') {
                return Err(SteganoError::IntegrityError(format!(
                    "Invalid FASTQ separator at line {}",
                    i + 3
                )));
            }

            records.push(FastqRecord {
                header: lines[i].to_string(),
                sequence: lines[i + 1].to_string(),
                separator: lines[i + 2].to_string(),
                quality: lines[i + 3].to_string(),
            });

            i += 4;
        }

        if records.is_empty() {
            return Err(SteganoError::IntegrityError(
                "No valid FASTQ records found".to_string(),
            ));
        }

        Ok(records)
    }

    /// Estimate capacity for FASTQ quality score steganography with specific strategy
    fn estimate_fastq_quality_capacity(
        &self,
        data: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let records = self.parse_fastq(data)?;
        let mut total_quality_chars = 0;

        for record in &records {
            total_quality_chars += record.quality.len();
        }

        let bits_per_char = match strategy {
            QualityEmbedStrategy::Conservative => 1,
            QualityEmbedStrategy::Balanced => 2,
            QualityEmbedStrategy::Aggressive => 3,
        };

        let total_bits = data.len() * 8;
        let recommended_bits = total_quality_chars * bits_per_char;
        let estimated_bytes = recommended_bits / 8;

        let mut format_info = HashMap::new();
        format_info.insert(
            "method".to_string(),
            serde_json::Value::String("fastq_quality_score_steganography".to_string()),
        );
        format_info.insert(
            "strategy".to_string(),
            serde_json::Value::String(format!("{:?}", strategy)),
        );
        format_info.insert(
            "records".to_string(),
            serde_json::Value::Number(records.len().into()),
        );
        format_info.insert(
            "total_quality_chars".to_string(),
            serde_json::Value::Number(total_quality_chars.into()),
        );
        format_info.insert(
            "bits_per_char".to_string(),
            serde_json::Value::Number(bits_per_char.into()),
        );
        format_info.insert(
            "estimated_capacity_bytes".to_string(),
            serde_json::Value::Number(estimated_bytes.into()),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }

    /// Embed payload in FASTQ quality scores
    pub fn embed_fastq_qualities(
        &self,
        data: &mut Vec<u8>,
        payload: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<QualityEmbedResult> {
        let mut records = self.parse_fastq(data)?;

        // Create a payload with length header (4 bytes for length + payload)
        let mut full_payload = Vec::new();
        let payload_len = payload.len() as u32;
        full_payload.extend_from_slice(&payload_len.to_le_bytes());
        full_payload.extend_from_slice(payload);

        // Convert full payload to bits
        let mut payload_bits = Vec::new();
        for byte in &full_payload {
            for i in 0..8 {
                payload_bits.push((byte >> i) & 1);
            }
        }

        let bits_per_char = match strategy {
            QualityEmbedStrategy::Conservative => 1,
            QualityEmbedStrategy::Balanced => 2,
            QualityEmbedStrategy::Aggressive => 3,
        };

        // Calculate total available capacity
        let total_capacity = records
            .iter()
            .map(|r| r.quality.len() * bits_per_char)
            .sum::<usize>();

        if payload_bits.len() > total_capacity {
            return Err(SteganoError::InsufficientCapacity {
                needed: payload_bits.len(),
                available: total_capacity,
            });
        }

        let mut bit_index = 0;
        let mut quality_chars_modified = 0;

        // Embed bits in quality scores
        for record in &mut records {
            let mut new_quality = String::new();

            for c in record.quality.chars() {
                let ascii_val = c as u8;

                // Check if we have more bits to embed
                if bit_index < payload_bits.len() {
                    // Validate quality score range
                    if ascii_val < MIN_QUALITY_ASCII || ascii_val > MAX_QUALITY_ASCII {
                        new_quality.push(c);
                        continue;
                    }

                    // Embed bits based on strategy
                    let mut modified_val = ascii_val;
                    let bits_to_embed =
                        std::cmp::min(bits_per_char, payload_bits.len() - bit_index);

                    // Clear the LSBs we'll use for embedding
                    modified_val &= !(((1 << bits_to_embed) - 1) as u8);

                    // Embed the payload bits
                    for i in 0..bits_to_embed {
                        if bit_index + i < payload_bits.len() {
                            modified_val |= (payload_bits[bit_index + i] << i) as u8;
                        }
                    }

                    // Ensure the modified quality score is still valid
                    if modified_val >= MIN_QUALITY_ASCII && modified_val <= MAX_QUALITY_ASCII {
                        new_quality.push(modified_val as char);
                        quality_chars_modified += 1;
                        bit_index += bits_to_embed;
                    } else {
                        // If modification would make invalid quality score, skip
                        new_quality.push(c);
                    }
                } else {
                    new_quality.push(c);
                }
            }

            record.quality = new_quality;
        }

        // Reconstruct FASTQ data
        let mut new_fastq = String::new();
        for record in &records {
            new_fastq.push_str(&record.header);
            new_fastq.push('\n');
            new_fastq.push_str(&record.sequence);
            new_fastq.push('\n');
            new_fastq.push_str(&record.separator);
            new_fastq.push('\n');
            new_fastq.push_str(&record.quality);
            new_fastq.push('\n');
        }

        *data = new_fastq.into_bytes();

        // Verify biological integrity
        let integrity_preserved = self.verify_fastq_biological_integrity(data)?;

        Ok(QualityEmbedResult {
            success: bit_index >= payload_bits.len(),
            bytes_embedded: payload.len(),
            strategy_used: strategy,
            quality_chars_modified,
            biological_integrity_preserved: integrity_preserved,
        })
    }

    /// Extract payload from FASTQ quality scores
    pub fn extract_fastq_qualities(
        &self,
        data: &[u8],
        strategy: QualityEmbedStrategy,
    ) -> Result<Vec<u8>> {
        let records = self.parse_fastq(data)?;

        let bits_per_char = match strategy {
            QualityEmbedStrategy::Conservative => 1,
            QualityEmbedStrategy::Balanced => 2,
            QualityEmbedStrategy::Aggressive => 3,
        };

        let mut extracted_bits = Vec::new();

        // Extract bits from quality scores
        for record in &records {
            for c in record.quality.chars() {
                let ascii_val = c as u8;

                // Validate quality score range
                if ascii_val < MIN_QUALITY_ASCII || ascii_val > MAX_QUALITY_ASCII {
                    continue;
                }

                // Extract embedded bits
                for i in 0..bits_per_char {
                    let bit = (ascii_val >> i) & 1;
                    extracted_bits.push(bit);
                }
            }
        }

        // Convert bits back to bytes
        let mut full_payload = Vec::new();
        for chunk in extracted_bits.chunks(8) {
            if chunk.len() == 8 {
                let mut byte = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    byte |= (bit << i) as u8;
                }
                full_payload.push(byte);
            }
        }

        // Extract payload length from first 4 bytes
        if full_payload.len() < 4 {
            return Err(SteganoError::IntegrityError(
                "Insufficient data to extract payload length".to_string(),
            ));
        }

        let payload_len = u32::from_le_bytes([
            full_payload[0],
            full_payload[1],
            full_payload[2],
            full_payload[3],
        ]) as usize;

        // Validate payload length
        if payload_len + 4 > full_payload.len() {
            return Err(SteganoError::IntegrityError(
                "Invalid payload length in extracted data".to_string(),
            ));
        }

        // Extract the actual payload (skip the 4-byte length header)
        let payload = full_payload[4..4 + payload_len].to_vec();

        Ok(payload)
    }

    /// Verify biological integrity of FASTQ data after quality score modification
    fn verify_fastq_biological_integrity(&self, data: &[u8]) -> Result<bool> {
        let records = self.parse_fastq(data)?;

        for record in &records {
            // Check that sequence and quality have same length
            if record.sequence.len() != record.quality.len() {
                return Ok(false);
            }

            // Check quality score range
            for c in record.quality.chars() {
                let ascii_val = c as u8;
                if ascii_val < MIN_QUALITY_ASCII || ascii_val > MAX_QUALITY_ASCII {
                    return Ok(false);
                }
            }

            // Check that sequence contains only valid nucleotides
            for c in record.sequence.chars() {
                if !"ATCGN".contains(c.to_ascii_uppercase()) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Check if data is FASTA format
    fn is_fasta(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        let text = String::from_utf8_lossy(data);
        text.starts_with('>') && text.contains('\n')
    }

    /// Check if data is FASTQ format
    fn is_fastq(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        let text = String::from_utf8_lossy(data);
        text.starts_with('@') && text.contains('\n') && text.contains('+')
    }
}

#[async_trait]
impl DataHandler for GenomicsHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_fasta(data) {
            Some(DataFormat::FASTA)
        } else if self.is_fastq(data) {
            Some(DataFormat::FASTQ)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::FASTA => self.estimate_fasta_capacity(&carrier.raw_data),
            DataFormat::FASTQ => self.estimate_fastq_capacity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        _params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start_time = Instant::now();

        let bytes_embedded = match carrier.format {
            DataFormat::FASTA => self.embed_fasta(&mut carrier.raw_data, payload)?,
            DataFormat::FASTQ => {
                // Use FASTQ quality score steganography for better capacity and stealth
                let result = self.embed_fastq_qualities(
                    &mut carrier.raw_data,
                    payload,
                    QualityEmbedStrategy::Balanced,
                )?;
                result.bytes_embedded
            }
            _ => return Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        };

        let embedding_time = start_time.elapsed().as_millis() as u64;

        Ok(EmbedResult {
            success: true,
            bytes_embedded,
            carrier_modified: true,
            performance_stats: PerformanceStats {
                embedding_time_ms: embedding_time,
                encryption_time_ms: 0,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + payload.len(),
            },
        })
    }

    async fn extract(&self, carrier: &CarrierData, _params: &EmbedParams) -> Result<Vec<u8>> {
        match carrier.format {
            DataFormat::FASTA => self.extract_fasta(&carrier.raw_data),
            DataFormat::FASTQ => {
                // Use FASTQ quality score extraction
                self.extract_fastq_qualities(&carrier.raw_data, QualityEmbedStrategy::Balanced)
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        // Check if the genomics file is still valid after embedding
        match carrier.format {
            DataFormat::FASTA => {
                let data_str = String::from_utf8_lossy(&carrier.raw_data);
                // Verify FASTA format integrity
                Ok(data_str.starts_with('>') && data_str.contains('\n'))
            }
            DataFormat::FASTQ => {
                // Use comprehensive FASTQ biological integrity verification
                self.verify_fastq_biological_integrity(&carrier.raw_data)
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_fasta_detection() {
        let handler = GenomicsHandler::new();
        let fasta_data = b">seq1\nATCGATCG\n>seq2\nGCTAGCTA\n";
        assert_eq!(handler.detect_format(fasta_data), Some(DataFormat::FASTA));
    }

    #[test]
    fn test_fastq_detection() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1\nATCGATCG\n+\nIIIIIIII\n";
        assert_eq!(handler.detect_format(fastq_data), Some(DataFormat::FASTQ));
    }

    #[test]
    fn test_fasta_capacity_estimation() {
        let handler = GenomicsHandler::new();
        let fasta_data = b">seq1 description\nATCGATCG\n>seq2 another\nGCTAGCTA\n";

        let carrier = CarrierData {
            format: DataFormat::FASTA,
            raw_data: fasta_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_fasta_embedding_roundtrip() {
        let handler = GenomicsHandler::new();
        let fasta_data = b">seq1 description line here with much longer content for embedding capacity\nATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG\n>seq2 another long sequence header for testing\nGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA\n>seq3 yet another sequence with plenty of space\nTTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGG\n";

        let mut carrier = CarrierData {
            format: DataFormat::FASTA,
            raw_data: fasta_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"secret genomics data";
        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);

        // Verify integrity
        assert!(handler.verify_integrity(&carrier).unwrap());
    }

    #[tokio::test]
    async fn test_fastq_quality_score_embedding_roundtrip() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1/1
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
+
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
@seq2/1
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
+
JJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJ
@seq3/1
TTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGG
+
KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK
";

        let mut carrier = CarrierData {
            format: DataFormat::FASTQ,
            raw_data: fastq_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"secret genomics quality data";
        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };

        // Test capacity estimation
        let capacity = handler.estimate_capacity(&carrier).unwrap();
        println!(
            "FASTQ quality capacity: {} bytes",
            capacity.recommended_bits / 8
        );
        assert!(capacity.recommended_bits >= payload.len() * 8);

        // Embed using quality score steganography
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_embedded, payload.len());

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);

        // Verify biological integrity
        assert!(handler.verify_integrity(&carrier).unwrap());

        // Verify that the FASTQ structure is preserved
        let data_str = String::from_utf8_lossy(&carrier.raw_data);
        assert!(data_str.starts_with('@'));
        assert!(data_str.contains('+'));

        // Count records to ensure structure is preserved
        let lines: Vec<&str> = data_str.lines().collect();
        assert_eq!(lines.len() % 4, 0, "FASTQ should have multiple of 4 lines");
    }

    #[test]
    fn test_fastq_capacity_estimation_strategies() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1/1
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
+
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
@seq2/1
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
+
JJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJ
";

        // Test different embedding strategies
        let conservative = handler
            .estimate_fastq_quality_capacity(fastq_data, QualityEmbedStrategy::Conservative)
            .unwrap();
        let balanced = handler
            .estimate_fastq_quality_capacity(fastq_data, QualityEmbedStrategy::Balanced)
            .unwrap();
        let aggressive = handler
            .estimate_fastq_quality_capacity(fastq_data, QualityEmbedStrategy::Aggressive)
            .unwrap();

        // Conservative should have lowest capacity, aggressive highest
        assert!(conservative.recommended_bits < balanced.recommended_bits);
        assert!(balanced.recommended_bits < aggressive.recommended_bits);

        println!("Conservative: {} bits", conservative.recommended_bits);
        println!("Balanced: {} bits", balanced.recommended_bits);
        println!("Aggressive: {} bits", aggressive.recommended_bits);
    }

    #[test]
    fn test_fastq_parsing() {
        let handler = GenomicsHandler::new();
        let fastq_data = b"@seq1/1
ATCGATCG
+
IIIIIIII
@seq2/1
GCTAGCTA
+
JJJJJJJJ
";

        let records = handler.parse_fastq(fastq_data).unwrap();
        assert_eq!(records.len(), 2);

        assert_eq!(records[0].header, "@seq1/1");
        assert_eq!(records[0].sequence, "ATCGATCG");
        assert_eq!(records[0].separator, "+");
        assert_eq!(records[0].quality, "IIIIIIII");

        assert_eq!(records[1].header, "@seq2/1");
        assert_eq!(records[1].sequence, "GCTAGCTA");
        assert_eq!(records[1].separator, "+");
        assert_eq!(records[1].quality, "JJJJJJJJ");
    }

    #[test]
    fn test_fastq_biological_integrity_verification() {
        let handler = GenomicsHandler::new();

        // Valid FASTQ
        let valid_fastq = b"@seq1/1
ATCGATCG
+
IIIIIIII
";
        assert!(handler
            .verify_fastq_biological_integrity(valid_fastq)
            .unwrap());

        // Invalid: mismatched sequence/quality lengths
        let invalid_fastq = b"@seq1/1
ATCGATCG
+
III
";
        assert!(!handler
            .verify_fastq_biological_integrity(invalid_fastq)
            .unwrap());

        // Invalid: bad nucleotides
        let invalid_nucleotides = b"@seq1/1
ATCGXYZ
+
IIIIIII
";
        assert!(!handler
            .verify_fastq_biological_integrity(invalid_nucleotides)
            .unwrap());
    }
}
