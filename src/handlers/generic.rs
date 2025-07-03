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
            _ => {
                // Check for FASTA format
                if data[0] == b'>' {
                    Some(DataFormat::FASTA)
                } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WAVE" {
                    Some(DataFormat::WAV)
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
        
        let bits_embedded = self.lsb_embed(&mut carrier.raw_data, payload)?;
        
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