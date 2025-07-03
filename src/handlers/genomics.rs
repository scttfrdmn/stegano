//! Genomics data format handlers with FASTA/FASTQ specific embedding

use crate::core::*;
use super::DataHandler;
use async_trait::async_trait;
use std::time::Instant;
use std::collections::HashMap;

pub struct GenomicsHandler;

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
            let end = data_str[start..].find(|c: char| c.is_whitespace())
                .map(|pos| start + pos)
                .unwrap_or(data_str.len());
                
            let hex_str = &data_str[start..end];
            
            // Decode hex payload
            if let Ok(payload) = hex::decode(hex_str) {
                return Ok(payload);
            }
        }
        
        Err(SteganoError::IntegrityError("No embedded data found".to_string()))
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
        format_info.insert("method".to_string(), serde_json::Value::String("header_comment".to_string()));
        format_info.insert("sequences".to_string(), serde_json::Value::Number(header_count.into()));
        format_info.insert("estimated_capacity_bytes".to_string(), serde_json::Value::Number(estimated_bytes.into()));
        
        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }

    /// Estimate capacity for FASTQ files based on header space
    fn estimate_fastq_capacity(&self, data: &[u8]) -> Result<CapacityInfo> {
        let data_str = String::from_utf8_lossy(data);
        let header_count = data_str.matches('@').count();
        
        // FASTQ headers are typically shorter than FASTA, but still usable
        let estimated_bytes = header_count * 40; // More conservative for FASTQ
        let total_bits = data.len() * 8;
        let recommended_bits = estimated_bytes * 8;
        
        let mut format_info = HashMap::new();
        format_info.insert("method".to_string(), serde_json::Value::String("fastq_header_comment".to_string()));
        format_info.insert("reads".to_string(), serde_json::Value::Number(header_count.into()));
        format_info.insert("estimated_capacity_bytes".to_string(), serde_json::Value::Number(estimated_bytes.into()));
        
        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
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
    
    async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], _params: &EmbedParams) -> Result<EmbedResult> {
        let start_time = Instant::now();
        
        let bytes_embedded = match carrier.format {
            DataFormat::FASTA => self.embed_fasta(&mut carrier.raw_data, payload)?,
            DataFormat::FASTQ => {
                // For FASTQ, use similar strategy but be more conservative
                self.embed_fasta(&mut carrier.raw_data, payload)?
            },
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
            DataFormat::FASTA | DataFormat::FASTQ => {
                self.extract_fasta(&carrier.raw_data)
            },
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
            },
            DataFormat::FASTQ => {
                let data_str = String::from_utf8_lossy(&carrier.raw_data);
                // Verify FASTQ format integrity (simplified check)
                Ok(data_str.starts_with('@') && data_str.contains('\n') && data_str.contains('+'))
            },
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
}