//! Generic data format handlers

use crate::core::*;
use super::DataHandler;
use async_trait::async_trait;
use std::time::Instant;

pub struct GenericHandler;

impl GenericHandler {
    pub fn new() -> Self {
        Self
    }

    /// Basic LSB embedding - replaces least significant bits of carrier data
    fn lsb_embed(&self, carrier: &mut [u8], payload: &[u8]) -> Result<usize> {
        let payload_bits = self.bytes_to_bits(payload);
        let max_capacity = carrier.len(); // 1 bit per byte
        
        if payload_bits.len() > max_capacity {
            return Err(SteganoError::InsufficientCapacity {
                needed: payload_bits.len(),
                available: max_capacity,
            });
        }

        // Embed payload length first (32 bits)
        let payload_len = payload.len() as u32;
        let len_bits = self.u32_to_bits(payload_len);
        
        if len_bits.len() + payload_bits.len() > max_capacity {
            return Err(SteganoError::InsufficientCapacity {
                needed: len_bits.len() + payload_bits.len(),
                available: max_capacity,
            });
        }

        // Embed length header
        for (i, &bit) in len_bits.iter().enumerate() {
            carrier[i] = (carrier[i] & 0xFE) | bit;
        }

        // Embed payload data
        for (i, &bit) in payload_bits.iter().enumerate() {
            let idx = 32 + i; // Skip length header
            carrier[idx] = (carrier[idx] & 0xFE) | bit;
        }

        Ok(32 + payload_bits.len())
    }

    /// Extract payload from LSB-embedded data
    fn lsb_extract(&self, carrier: &[u8]) -> Result<Vec<u8>> {
        if carrier.len() < 32 {
            return Err(SteganoError::IntegrityError("Carrier too small for header".to_string()));
        }

        // Extract payload length from first 32 bits
        let mut len_bits = Vec::with_capacity(32);
        for i in 0..32 {
            len_bits.push(carrier[i] & 1);
        }
        let payload_len = self.bits_to_u32(&len_bits);

        if payload_len == 0 {
            return Ok(Vec::new());
        }

        let payload_bits_needed = payload_len as usize * 8;
        if 32 + payload_bits_needed > carrier.len() {
            return Err(SteganoError::IntegrityError(
                "Carrier too small for declared payload".to_string()
            ));
        }

        // Extract payload bits
        let mut payload_bits = Vec::with_capacity(payload_bits_needed);
        for i in 32..(32 + payload_bits_needed) {
            payload_bits.push(carrier[i] & 1);
        }

        Ok(self.bits_to_bytes(&payload_bits))
    }

    /// Convert bytes to bit array
    fn bytes_to_bits(&self, bytes: &[u8]) -> Vec<u8> {
        let mut bits = Vec::with_capacity(bytes.len() * 8);
        for &byte in bytes {
            for i in 0..8 {
                bits.push((byte >> i) & 1);
            }
        }
        bits
    }

    /// Convert bit array back to bytes
    fn bits_to_bytes(&self, bits: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(bits.len() / 8);
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                byte |= bit << i;
            }
            bytes.push(byte);
        }
        bytes
    }

    /// Convert u32 to 32-bit array
    fn u32_to_bits(&self, value: u32) -> Vec<u8> {
        let mut bits = Vec::with_capacity(32);
        for i in 0..32 {
            bits.push(((value >> i) & 1) as u8);
        }
        bits
    }

    /// Convert 32-bit array to u32
    fn bits_to_u32(&self, bits: &[u8]) -> u32 {
        let mut value = 0u32;
        for (i, &bit) in bits.iter().enumerate() {
            if i >= 32 { break; }
            value |= (bit as u32) << i;
        }
        value
    }
}

#[async_trait]
impl DataHandler for GenericHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        // Basic format detection based on magic bytes
        if data.len() < 4 {
            return Some(DataFormat::BINARY);
        }

        match &data[0..4] {
            [0x89, 0x50, 0x4E, 0x47] => Some(DataFormat::PNG),
            [0x49, 0x49, 0x2A, 0x00] | [0x4D, 0x4D, 0x00, 0x2A] => Some(DataFormat::TIFF),
            [0x52, 0x49, 0x46, 0x46] => {
                // RIFF format - check for WAV
                if data.len() >= 12 && &data[8..12] == b"WAVE" {
                    Some(DataFormat::WAV)
                } else {
                    Some(DataFormat::BINARY)
                }
            },
            _ => {
                // Check for FASTA format
                if data[0] == b'>' {
                    Some(DataFormat::FASTA)
                } else {
                    Some(DataFormat::BINARY)
                }
            }
        }
    }
    
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        let data_size = carrier.raw_data.len();
        
        // Reserve 32 bits for payload length header
        let available_bits = if data_size > 32 { data_size - 32 } else { 0 };
        let available_bytes = available_bits / 8;
        
        // LSB embedding uses 1 bit per carrier byte
        let total_bits = data_size;
        let recommended_bits = available_bits;
        let steganographic_ratio = if data_size > 0 {
            available_bits as f64 / (data_size * 8) as f64
        } else {
            0.0
        };

        let mut format_info = std::collections::HashMap::new();
        format_info.insert(
            "embedding_method".to_string(),
            serde_json::Value::String("LSB".to_string())
        );
        format_info.insert(
            "available_bytes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(available_bytes))
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio,
            format_specific_info: format_info,
        })
    }
    
    async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], _params: &EmbedParams) -> Result<EmbedResult> {
        let start_time = Instant::now();
        
        let _bits_embedded = self.lsb_embed(&mut carrier.raw_data, payload)?;
        
        let embedding_time = start_time.elapsed().as_millis() as u64;
        
        Ok(EmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            carrier_modified: true,
            performance_stats: PerformanceStats {
                embedding_time_ms: embedding_time,
                encryption_time_ms: 0, // No encryption in basic implementation
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + payload.len(),
            },
        })
    }
    
    async fn extract(&self, carrier: &CarrierData, _params: &EmbedParams) -> Result<Vec<u8>> {
        self.lsb_extract(&carrier.raw_data)
    }
    
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        // Basic integrity check - verify we can extract a valid payload length
        if carrier.raw_data.len() < 32 {
            return Ok(false);
        }

        let mut len_bits = Vec::with_capacity(32);
        for i in 0..32 {
            len_bits.push(carrier.raw_data[i] & 1);
        }
        let payload_len = self.bits_to_u32(&len_bits);

        // Check if declared payload length is reasonable
        let max_possible_payload = (carrier.raw_data.len() - 32) / 8;
        Ok(payload_len as usize <= max_possible_payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_generic_handler_creation() {
        let handler = GenericHandler::new();
        // Test that we can create a handler instance
        assert_eq!(std::mem::size_of_val(&handler), 0); // Zero-sized struct
    }

    #[test]
    fn test_bytes_to_bits_conversion() {
        let handler = GenericHandler::new();
        let bytes = vec![0x55, 0xAA]; // 01010101, 10101010
        let bits = handler.bytes_to_bits(&bytes);
        
        // Check first byte: 0x55 = 01010101 -> [1,0,1,0,1,0,1,0] (LSB first)
        assert_eq!(bits[0], 1);
        assert_eq!(bits[1], 0);
        assert_eq!(bits[2], 1);
        assert_eq!(bits[3], 0);
        assert_eq!(bits[4], 1);
        assert_eq!(bits[5], 0);
        assert_eq!(bits[6], 1);
        assert_eq!(bits[7], 0);
        
        // Check second byte: 0xAA = 10101010 -> [0,1,0,1,0,1,0,1] (LSB first)
        assert_eq!(bits[8], 0);
        assert_eq!(bits[9], 1);
        assert_eq!(bits[10], 0);
        assert_eq!(bits[11], 1);
    }

    #[test]
    fn test_bits_to_bytes_conversion() {
        let handler = GenericHandler::new();
        let bits = vec![1,0,1,0,1,0,1,0, 0,1,0,1,0,1,0,1];
        let bytes = handler.bits_to_bytes(&bits);
        
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[1], 0xAA);
    }

    #[test]
    fn test_u32_to_bits_conversion() {
        let handler = GenericHandler::new();
        let value = 0x12345678u32;
        let bits = handler.u32_to_bits(value);
        
        assert_eq!(bits.len(), 32);
        
        // Verify LSB first: 0x12345678 = ...01111000 -> bits[0..7] = [0,0,0,1,1,1,1,0]
        assert_eq!(bits[0], 0); // LSB
        assert_eq!(bits[1], 0);
        assert_eq!(bits[2], 0);
        assert_eq!(bits[3], 1);
    }

    #[test]
    fn test_bits_to_u32_conversion() {
        let handler = GenericHandler::new();
        let mut bits = vec![0; 32];
        bits[3] = 1; // Bit 3 set
        bits[7] = 1; // Bit 7 set
        
        let value = handler.bits_to_u32(&bits);
        assert_eq!(value, (1 << 3) | (1 << 7)); // 0x88
    }

    #[test]
    fn test_bits_to_u32_overflow() {
        let handler = GenericHandler::new();
        let bits = vec![1; 40]; // More than 32 bits
        let value = handler.bits_to_u32(&bits);
        assert_eq!(value, 0xFFFFFFFF); // Only first 32 bits used
    }

    #[test]
    fn test_empty_data_conversions() {
        let handler = GenericHandler::new();
        
        let empty_bytes = vec![];
        let bits = handler.bytes_to_bits(&empty_bytes);
        assert_eq!(bits.len(), 0);
        
        let empty_bits = vec![];
        let bytes = handler.bits_to_bytes(&empty_bits);
        assert_eq!(bytes.len(), 0);
    }

    #[test]
    fn test_format_detection_png() {
        let handler = GenericHandler::new();
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(handler.detect_format(&png_data), Some(DataFormat::PNG));
    }

    #[test]
    fn test_format_detection_tiff() {
        let handler = GenericHandler::new();
        let tiff_le = vec![0x49, 0x49, 0x2A, 0x00]; // Little endian TIFF
        let tiff_be = vec![0x4D, 0x4D, 0x00, 0x2A]; // Big endian TIFF
        
        assert_eq!(handler.detect_format(&tiff_le), Some(DataFormat::TIFF));
        assert_eq!(handler.detect_format(&tiff_be), Some(DataFormat::TIFF));
    }

    #[test]
    fn test_format_detection_fasta() {
        let handler = GenericHandler::new();
        let fasta_data = b">sequence1\nATCG\n".to_vec();
        assert_eq!(handler.detect_format(&fasta_data), Some(DataFormat::FASTA));
    }

    #[test]
    fn test_format_detection_wav() {
        let handler = GenericHandler::new();
        let mut wav_data = b"RIFF".to_vec();
        wav_data.extend_from_slice(&[0, 0, 0, 0]); // File size placeholder
        wav_data.extend_from_slice(b"WAVE");
        assert_eq!(handler.detect_format(&wav_data), Some(DataFormat::WAV));
    }

    #[test]
    fn test_format_detection_binary() {
        let handler = GenericHandler::new();
        let binary_data = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(handler.detect_format(&binary_data), Some(DataFormat::BINARY));
    }

    #[test]
    fn test_format_detection_small_data() {
        let handler = GenericHandler::new();
        let small_data = vec![0x00, 0x01]; // Less than 4 bytes
        assert_eq!(handler.detect_format(&small_data), Some(DataFormat::BINARY));
    }

    #[test]
    fn test_capacity_estimation() {
        let handler = GenericHandler::new();
        let data = vec![0; 1000]; // 1000 bytes
        let carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert_eq!(capacity.total_bits, 1000);
        assert_eq!(capacity.recommended_bits, 968); // 1000 - 32 (header)
        assert!(capacity.steganographic_ratio > 0.0);
        assert!(capacity.format_specific_info.contains_key("embedding_method"));
    }

    #[test]
    fn test_capacity_estimation_small_carrier() {
        let handler = GenericHandler::new();
        let data = vec![0; 20]; // Smaller than header size
        let carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert_eq!(capacity.recommended_bits, 0); // Too small for any payload
    }

    #[tokio::test]
    async fn test_lsb_embedding_roundtrip() {
        let handler = GenericHandler::new();
        let carrier_data = vec![0x80; 1000]; // 1000 bytes, MSB set
        let payload = b"Hello, steganography world!";
        
        let mut carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: carrier_data.clone(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
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
        assert_eq!(result.bytes_embedded, payload.len());
        
        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
        
        // Verify integrity
        assert!(handler.verify_integrity(&carrier).unwrap());
    }

    #[tokio::test]
    async fn test_empty_payload_embedding() {
        let handler = GenericHandler::new();
        let mut carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: vec![0; 100],
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let params = EmbedParams::new(vec![]);
        let empty_payload = b"";
        
        let result = handler.embed(&mut carrier, empty_payload, &params).await.unwrap();
        assert!(result.success);
        
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, empty_payload);
    }

    #[test]
    fn test_lsb_embed_insufficient_capacity() {
        let handler = GenericHandler::new();
        let mut carrier = vec![0; 50]; // Small carrier
        let payload = vec![0; 100]; // Large payload
        
        let result = handler.lsb_embed(&mut carrier, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SteganoError::InsufficientCapacity { .. }));
    }

    #[test]
    fn test_lsb_extract_invalid_carrier() {
        let handler = GenericHandler::new();
        let small_carrier = vec![0; 20]; // Too small for header
        
        let result = handler.lsb_extract(&small_carrier);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SteganoError::IntegrityError(_)));
    }

    #[test]
    fn test_lsb_extract_malformed_header() {
        let handler = GenericHandler::new();
        let mut carrier = vec![0; 100];
        
        // Set a payload length that's too large for the carrier
        let large_len = 1000u32;
        let len_bits = handler.u32_to_bits(large_len);
        for (i, &bit) in len_bits.iter().enumerate() {
            carrier[i] = (carrier[i] & 0xFE) | bit;
        }
        
        let result = handler.lsb_extract(&carrier);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SteganoError::IntegrityError(_)));
    }

    #[test]
    fn test_verify_integrity_small_carrier() {
        let handler = GenericHandler::new();
        let carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: vec![0; 20], // Too small
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        assert!(!handler.verify_integrity(&carrier).unwrap());
    }

    #[test]
    fn test_verify_integrity_invalid_payload_length() {
        let handler = GenericHandler::new();
        let mut data = vec![0; 100];
        
        // Set payload length to exceed carrier capacity
        let invalid_len = 1000u32;
        let len_bits = handler.u32_to_bits(invalid_len);
        for (i, &bit) in len_bits.iter().enumerate() {
            data[i] = (data[i] & 0xFE) | bit;
        }
        
        let carrier = CarrierData {
            format: DataFormat::BINARY,
            raw_data: data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        assert!(!handler.verify_integrity(&carrier).unwrap());
    }

    #[test]
    fn test_single_byte_operations() {
        let handler = GenericHandler::new();
        
        // Test single byte conversion
        let single_byte = vec![0xFF];
        let bits = handler.bytes_to_bits(&single_byte);
        assert_eq!(bits.len(), 8);
        assert_eq!(bits, vec![1, 1, 1, 1, 1, 1, 1, 1]);
        
        let converted_back = handler.bits_to_bytes(&bits);
        assert_eq!(converted_back, single_byte);
    }

    #[test]
    fn test_partial_bit_chunks() {
        let handler = GenericHandler::new();
        
        // Test with bit array not divisible by 8
        let partial_bits = vec![1, 0, 1]; // Only 3 bits
        let bytes = handler.bits_to_bytes(&partial_bits);
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0x05); // 101 in LSB = 5
    }
}