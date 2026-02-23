//! Image data format handlers with advanced steganography techniques

use super::DataHandler;
use crate::core::*;
use crate::crypto::CryptoLayer;
use async_trait::async_trait;
use std::time::Instant;

pub struct ImageryHandler {
    crypto: Option<CryptoLayer>,
}

#[derive(Debug, Clone)]
pub enum ImageEmbedStrategy {
    /// Simple LSB embedding in least significant bits
    SimpleLSB,
    /// Advanced LSB with adaptive selection of channels
    AdaptiveLSB,
    /// DCT-based embedding in frequency domain
    DCTFrequency,
    /// Pixel value differencing
    PVD,
    /// Edge-based embedding for higher imperceptibility
    EdgeBased,
}

impl ImageEmbedStrategy {
    fn to_u8(&self) -> u8 {
        match self {
            ImageEmbedStrategy::SimpleLSB => 0,
            ImageEmbedStrategy::AdaptiveLSB => 1,
            ImageEmbedStrategy::DCTFrequency => 2,
            ImageEmbedStrategy::PVD => 3,
            ImageEmbedStrategy::EdgeBased => 4,
        }
    }

    #[allow(dead_code)]
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ImageEmbedStrategy::SimpleLSB),
            1 => Some(ImageEmbedStrategy::AdaptiveLSB),
            2 => Some(ImageEmbedStrategy::DCTFrequency),
            3 => Some(ImageEmbedStrategy::PVD),
            4 => Some(ImageEmbedStrategy::EdgeBased),
            _ => None,
        }
    }
}

impl ImageryHandler {
    pub fn new() -> Self {
        Self { crypto: None }
    }

    pub fn with_encryption(password: &str) -> Result<Self> {
        Ok(Self {
            crypto: Some(CryptoLayer::new_from_password(password)?),
        })
    }

    /// Detect image format from file headers
    fn detect_image_format(&self, data: &[u8]) -> Option<DataFormat> {
        if data.len() < 8 {
            return None;
        }

        // PNG magic bytes
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some(DataFormat::PNG);
        }

        // JPEG magic bytes
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(DataFormat::BINARY); // Treat as binary for now
        }

        // TIFF magic bytes (little endian)
        if data.starts_with(&[0x49, 0x49, 0x2A, 0x00]) {
            return Some(DataFormat::TIFF);
        }

        // TIFF magic bytes (big endian)
        if data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
            return Some(DataFormat::TIFF);
        }

        // BMP magic bytes
        if data.starts_with(&[0x42, 0x4D]) {
            return Some(DataFormat::BINARY); // Treat as binary for now
        }

        None
    }

    /// Estimate capacity using different strategies
    fn estimate_image_capacity(
        &self,
        carrier: &CarrierData,
        strategy: &ImageEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let dimensions = carrier
            .dimensions
            .as_ref()
            .ok_or(SteganoError::InvalidParameter(
                "Missing image dimensions".to_string(),
            ))?;
        let total_pixels = dimensions.width * dimensions.height;

        let (capacity_bits, overhead) = match strategy {
            ImageEmbedStrategy::SimpleLSB => {
                // 1 bit per channel (RGB = 3 bits per pixel)
                (total_pixels * 3, 64) // 64 bits for header
            }
            ImageEmbedStrategy::AdaptiveLSB => {
                // Variable bits per pixel based on local complexity
                (total_pixels * 2, 96) // Conservative estimate with larger header
            }
            ImageEmbedStrategy::DCTFrequency => {
                // DCT blocks (8x8), embedding in mid-frequency coefficients
                let blocks = (dimensions.width / 8) * (dimensions.height / 8);
                (blocks * 12, 128) // 12 coefficients per block usable
            }
            ImageEmbedStrategy::PVD => {
                // Pixel value differencing
                (total_pixels / 2, 96) // Pairs of pixels
            }
            ImageEmbedStrategy::EdgeBased => {
                // Only embed in edge regions (estimate 30% of image)
                (total_pixels * 3 / 10, 128)
            }
        };

        let _capacity_bytes = if capacity_bits > overhead {
            (capacity_bits - overhead) / 8
        } else {
            0
        };

        let recommended_bits = capacity_bits.saturating_sub(overhead);
        let mut info = CapacityInfo::new(capacity_bits as usize, recommended_bits as usize);
        info.format_specific_info.insert(
            "overhead_bytes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(overhead / 8)),
        );
        info.format_specific_info.insert(
            "strategy".to_string(),
            serde_json::Value::String(format!("{strategy:?}")),
        );
        Ok(info)
    }

    /// LSB embedding with optional encryption
    async fn embed_lsb(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        strategy: &ImageEmbedStrategy,
    ) -> Result<EmbedResult> {
        let embed_start = Instant::now();

        // Prepare payload with encryption if enabled
        let (final_payload, encryption_time_ms) = if let Some(ref crypto) = self.crypto {
            use crate::crypto::CryptoLayer;
            let enc_start = Instant::now();
            let secure_payload = crypto.encrypt(payload)?;
            // Ceiling to nearest ms; at least 1 when encryption ran
            let enc_ns = enc_start.elapsed().as_nanos().max(1) as u64;
            let enc_ms = enc_ns.div_ceil(1_000_000);
            (CryptoLayer::serialize_payload(&secure_payload), enc_ms)
        } else {
            (payload.to_vec(), 0)
        };

        // Create header
        let mut header = Vec::new();
        header.extend_from_slice(b"IMSJ"); // Image Steganography header
        header.extend_from_slice(&(final_payload.len() as u32).to_le_bytes());
        header.push(strategy.to_u8());
        header.push(if self.crypto.is_some() { 1 } else { 0 }); // Encryption flag

        let total_data = [header, final_payload].concat();
        let bits_needed = total_data.len() * 8;

        // Check capacity
        let capacity = self.estimate_image_capacity(carrier, strategy)?;
        if bits_needed > capacity.total_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: bits_needed,
                available: capacity.recommended_bits / 8,
            });
        }

        // Perform embedding based on strategy
        match strategy {
            ImageEmbedStrategy::SimpleLSB | ImageEmbedStrategy::AdaptiveLSB => {
                self.embed_lsb_pixels(&mut carrier.raw_data, &total_data, strategy)?;
            }
            ImageEmbedStrategy::EdgeBased => {
                self.embed_edge_based(
                    &mut carrier.raw_data,
                    &total_data,
                    carrier.dimensions.as_ref().unwrap().clone(),
                )?;
            }
            _ => {
                // For now, fallback to simple LSB for other strategies
                self.embed_lsb_pixels(
                    &mut carrier.raw_data,
                    &total_data,
                    &ImageEmbedStrategy::SimpleLSB,
                )?;
            }
        }

        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: embed_start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: total_data.len() * 2,
            },
        ))
    }

    /// Simple LSB embedding in pixel data
    fn embed_lsb_pixels(
        &self,
        image_data: &mut [u8],
        payload: &[u8],
        strategy: &ImageEmbedStrategy,
    ) -> Result<()> {
        let payload_bits: Vec<u8> = payload
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect();

        match strategy {
            ImageEmbedStrategy::SimpleLSB => {
                // Simple sequential embedding
                for (i, &bit) in payload_bits.iter().enumerate() {
                    if i >= image_data.len() {
                        break;
                    }
                    image_data[i] = (image_data[i] & 0xFE) | bit;
                }
            }
            ImageEmbedStrategy::AdaptiveLSB => {
                // Embed preferentially in high-variation areas
                let mut bit_index = 0;
                for i in 0..image_data.len() - 2 {
                    if bit_index >= payload_bits.len() {
                        break;
                    }

                    // Calculate local variation
                    let variation = (image_data[i] as i16 - image_data[i + 1] as i16).abs()
                        + (image_data[i + 1] as i16 - image_data[i + 2] as i16).abs();

                    // Only embed if variation is above threshold
                    if variation > 10 {
                        image_data[i] = (image_data[i] & 0xFE) | payload_bits[bit_index];
                        bit_index += 1;
                    }
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Edge-based embedding for better imperceptibility
    fn embed_edge_based(
        &self,
        image_data: &mut [u8],
        payload: &[u8],
        dimensions: Dimensions,
    ) -> Result<()> {
        let payload_bits: Vec<u8> = payload
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect();

        let width = dimensions.width as usize;
        let height = dimensions.height as usize;
        let channels = 3; // Assume RGB

        let mut bit_index = 0;

        // Simple edge detection using Sobel-like operator
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                if bit_index >= payload_bits.len() {
                    break;
                }

                let pixel_idx = (y * width + x) * channels;
                if pixel_idx + channels >= image_data.len() {
                    continue;
                }

                // Check bounds for gradient calculation
                if pixel_idx < width * channels || pixel_idx + width * channels >= image_data.len()
                {
                    continue;
                }

                // Calculate gradient magnitude for edge detection
                let gx = (image_data[pixel_idx + channels] as i16)
                    - (image_data[pixel_idx - channels] as i16);
                let gy = (image_data[pixel_idx + width * channels] as i16)
                    - (image_data[pixel_idx - width * channels] as i16);
                let gradient = ((gx * gx + gy * gy) as f64).sqrt();

                // Only embed in edge regions (high gradient)
                if gradient > 20.0 {
                    image_data[pixel_idx] =
                        (image_data[pixel_idx] & 0xFE) | payload_bits[bit_index];
                    bit_index += 1;
                }
            }
            if bit_index >= payload_bits.len() {
                break;
            }
        }

        Ok(())
    }

    /// Extract data using the detected strategy
    async fn extract_with_strategy(
        &self,
        carrier: &CarrierData,
        strategy: &ImageEmbedStrategy,
    ) -> Result<Vec<u8>> {
        // First extract header to determine payload size and encryption
        let mut header_bits = Vec::new();
        let header_bytes_needed = 10; // 4 magic + 4 size + 1 strategy + 1 encryption

        match strategy {
            ImageEmbedStrategy::SimpleLSB => {
                for i in 0..header_bytes_needed * 8 {
                    if i >= carrier.raw_data.len() {
                        break;
                    }
                    header_bits.push(carrier.raw_data[i] & 1);
                }
            }
            ImageEmbedStrategy::EdgeBased => {
                // Extract from edge regions
                let dimensions =
                    carrier
                        .dimensions
                        .as_ref()
                        .ok_or(SteganoError::InvalidParameter(
                            "Missing image dimensions".to_string(),
                        ))?;
                header_bits = self.extract_edge_bits(
                    &carrier.raw_data,
                    dimensions.clone(),
                    header_bytes_needed * 8,
                )?;
            }
            _ => {
                // Fallback to simple LSB
                for i in 0..header_bytes_needed * 8 {
                    if i >= carrier.raw_data.len() {
                        break;
                    }
                    header_bits.push(carrier.raw_data[i] & 1);
                }
            }
        }

        // Convert bits back to bytes
        let header_bytes: Vec<u8> = header_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        if header_bytes.len() < header_bytes_needed || &header_bytes[0..4] != b"IMSJ" {
            return Err(SteganoError::InvalidParameter(
                "Invalid header magic".to_string(),
            ));
        }

        let payload_size = u32::from_le_bytes([
            header_bytes[4],
            header_bytes[5],
            header_bytes[6],
            header_bytes[7],
        ]) as usize;

        let is_encrypted = header_bytes[9] == 1;

        // Extract payload
        let total_bits_needed = (header_bytes_needed + payload_size) * 8;
        let mut all_bits = Vec::new();

        match strategy {
            ImageEmbedStrategy::SimpleLSB => {
                for i in 0..total_bits_needed.min(carrier.raw_data.len()) {
                    all_bits.push(carrier.raw_data[i] & 1);
                }
            }
            ImageEmbedStrategy::EdgeBased => {
                all_bits = self.extract_edge_bits(
                    &carrier.raw_data,
                    carrier.dimensions.as_ref().unwrap().clone(),
                    total_bits_needed,
                )?;
            }
            _ => {
                for i in 0..total_bits_needed.min(carrier.raw_data.len()) {
                    all_bits.push(carrier.raw_data[i] & 1);
                }
            }
        }

        let all_bytes: Vec<u8> = all_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        if all_bytes.len() < header_bytes_needed + payload_size {
            return Err(SteganoError::InvalidParameter(
                "Insufficient data for payload".to_string(),
            ));
        }

        let encrypted_payload = &all_bytes[header_bytes_needed..header_bytes_needed + payload_size];

        // Decrypt if necessary
        if is_encrypted {
            if let Some(ref crypto) = self.crypto {
                use crate::crypto::CryptoLayer;
                let secure_payload = CryptoLayer::deserialize_payload(encrypted_payload)?;
                crypto.decrypt(&secure_payload)
            } else {
                Err(SteganoError::EncryptionError(
                    "No decryption key available".to_string(),
                ))
            }
        } else {
            Ok(encrypted_payload.to_vec())
        }
    }

    /// Extract bits from edge regions
    fn extract_edge_bits(
        &self,
        image_data: &[u8],
        dimensions: Dimensions,
        bits_needed: usize,
    ) -> Result<Vec<u8>> {
        let width = dimensions.width as usize;
        let height = dimensions.height as usize;
        let channels = 3;
        let mut bits = Vec::new();

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                if bits.len() >= bits_needed {
                    break;
                }

                let pixel_idx = (y * width + x) * channels;
                if pixel_idx + channels >= image_data.len()
                    || pixel_idx < width * channels
                    || pixel_idx + width * channels >= image_data.len()
                {
                    continue;
                }

                let gx = (image_data[pixel_idx + channels] as i16)
                    - (image_data[pixel_idx - channels] as i16);
                let gy = (image_data[pixel_idx + width * channels] as i16)
                    - (image_data[pixel_idx - width * channels] as i16);
                let gradient = ((gx * gx + gy * gy) as f64).sqrt();

                if gradient > 20.0 {
                    bits.push(image_data[pixel_idx] & 1);
                }
            }
            if bits.len() >= bits_needed {
                break;
            }
        }

        Ok(bits)
    }
}

#[async_trait]
impl DataHandler for ImageryHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        self.detect_image_format(data)
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        // Use adaptive LSB as default strategy for capacity estimation
        self.estimate_image_capacity(carrier, &ImageEmbedStrategy::AdaptiveLSB)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let strategy = match params.stealth_level {
            StealthLevel::Low => ImageEmbedStrategy::SimpleLSB,
            StealthLevel::Medium => ImageEmbedStrategy::AdaptiveLSB,
            StealthLevel::High => ImageEmbedStrategy::EdgeBased,
            StealthLevel::Maximum => ImageEmbedStrategy::EdgeBased,
        };

        self.embed_lsb(carrier, payload, &strategy).await
    }

    async fn extract(&self, carrier: &CarrierData, _params: &EmbedParams) -> Result<Vec<u8>> {
        // Try different strategies in order of likelihood
        let strategies = [
            ImageEmbedStrategy::AdaptiveLSB,
            ImageEmbedStrategy::SimpleLSB,
            ImageEmbedStrategy::EdgeBased,
        ];

        for strategy in &strategies {
            if let Ok(result) = self.extract_with_strategy(carrier, strategy).await {
                return Ok(result);
            }
        }

        Err(SteganoError::InvalidParameter(
            "No valid extraction strategy found".to_string(),
        ))
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        // Basic integrity check - ensure we can detect the header
        if carrier.raw_data.len() < 80 {
            // Minimum for header
            return Ok(false);
        }

        // Try to extract header
        let mut header_bits = Vec::new();
        for i in 0..32 {
            // First 4 bytes for magic
            if i >= carrier.raw_data.len() {
                return Ok(false);
            }
            header_bits.push(carrier.raw_data[i] & 1);
        }

        let header_bytes: Vec<u8> = header_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        Ok(header_bytes.len() >= 4 && &header_bytes[0..4] == b"IMSJ")
    }
}

impl Default for ImageryHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_imagery_handler_creation() {
        let handler = ImageryHandler::new();
        assert!(handler.crypto.is_none());
    }

    #[test]
    fn test_imagery_handler_with_encryption() {
        let handler = ImageryHandler::with_encryption("test_password").unwrap();
        assert!(handler.crypto.is_some());
    }

    #[test]
    fn test_image_format_detection() {
        let handler = ImageryHandler::new();

        // PNG magic bytes
        let png_data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(handler.detect_format(&png_data), Some(DataFormat::PNG));

        // JPEG magic bytes
        let jpeg_data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46]; // 8 bytes minimum
        assert_eq!(handler.detect_format(&jpeg_data), Some(DataFormat::BINARY));

        // TIFF magic bytes (little endian)
        let tiff_data = [0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]; // 8 bytes minimum
        assert_eq!(handler.detect_format(&tiff_data), Some(DataFormat::TIFF));

        // Unknown format
        let unknown_data = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(handler.detect_format(&unknown_data), None);
    }

    #[test]
    fn test_capacity_estimation() {
        let handler = ImageryHandler::new();
        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0u8; 1000],
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(100, 100)),
            sampling_rate: None,
        };

        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.total_bits > 0);
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_lsb_embedding_roundtrip() {
        let handler = ImageryHandler::new();
        let payload = b"Test payload for image steganography";

        let mut carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0xAAu8; 10000], // Large enough for payload
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(100, 100)),
            sampling_rate: None,
        };

        let mut params = EmbedParams::new(b"test_key".to_vec());
        params.stealth_level = StealthLevel::Low; // Use SimpleLSB

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert_eq!(result.bytes_embedded, payload.len());
        assert!(result.success);

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
    }

    #[tokio::test]
    async fn test_encrypted_embedding_roundtrip() {
        let handler = ImageryHandler::with_encryption("test_password_123").unwrap();
        let payload = b"Secret message in image";

        let mut carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0x80u8; 15000], // Large enough for encrypted payload
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(120, 120)),
            sampling_rate: None,
        };

        let mut params = EmbedParams::new(b"key".to_vec());
        params.stealth_level = StealthLevel::Low; // Use SimpleLSB

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert_eq!(result.bytes_embedded, payload.len());
        assert!(result.success);
        assert!(result.performance_stats.encryption_time_ms > 0);

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
    }

    #[test]
    fn test_integrity_verification() {
        let handler = ImageryHandler::new();

        // Create carrier with valid header
        let mut carrier_data = vec![0u8; 1000];
        let header_bits = [b'I', b'M', b'S', b'J']
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect::<Vec<u8>>();

        for (i, &bit) in header_bits.iter().enumerate() {
            if i < carrier_data.len() {
                carrier_data[i] = (carrier_data[i] & 0xFE) | bit;
            }
        }

        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: carrier_data,
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(32, 32)),
            sampling_rate: None,
        };

        assert!(handler.verify_integrity(&carrier).unwrap());

        // Test with invalid header
        let invalid_carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0xFFu8; 1000],
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(32, 32)),
            sampling_rate: None,
        };

        assert!(!handler.verify_integrity(&invalid_carrier).unwrap());
    }

    #[test]
    fn test_different_stealth_levels() {
        let handler = ImageryHandler::new();

        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0u8; 5000],
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(64, 64)),
            sampling_rate: None,
        };

        // Test capacity with different strategies
        let simple_capacity = handler
            .estimate_image_capacity(&carrier, &ImageEmbedStrategy::SimpleLSB)
            .unwrap();
        let adaptive_capacity = handler
            .estimate_image_capacity(&carrier, &ImageEmbedStrategy::AdaptiveLSB)
            .unwrap();
        let edge_capacity = handler
            .estimate_image_capacity(&carrier, &ImageEmbedStrategy::EdgeBased)
            .unwrap();

        // Simple LSB should have highest capacity
        assert!(simple_capacity.recommended_bits >= adaptive_capacity.recommended_bits);
        assert!(adaptive_capacity.recommended_bits >= edge_capacity.recommended_bits);
    }

    #[test]
    fn test_insufficient_capacity() {
        let handler = ImageryHandler::new();
        let large_payload = vec![0u8; 10000]; // Too large

        let mut small_carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0u8; 100], // Too small
            metadata: HashMap::new(),
            dimensions: Some(Dimensions::new_2d(10, 10)),
            sampling_rate: None,
        };

        let params = EmbedParams::new(b"key".to_vec());
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(handler.embed(&mut small_carrier, &large_payload, &params));
        assert!(matches!(
            result,
            Err(SteganoError::InsufficientCapacity { .. })
        ));
    }
}
