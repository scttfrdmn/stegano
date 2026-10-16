//! FITS (Flexible Image Transport System) format handler.
//!
//! FITS is the standard format for astronomical images and data from
//! telescopes, satellites, and observatories.  This handler implements
//! pixel-LSB steganography in the data section that follows the header.
//!
//! # FITS format overview
//! A FITS file consists of one or more Header-Data Units (HDUs).  Each
//! header is a sequence of 80-byte fixed-width "cards" grouped into
//! 2880-byte blocks.  The header ends with an `END` card.  The data
//! section immediately follows, aligned to the next 2880-byte boundary.
//!
//! # Embedding strategy: `pixel_lsb`
//! The handler locates the start of the first HDU's data section (by
//! scanning for `END` and rounding up to the next 2880-byte block), then
//! embeds the payload in the LSBs of each byte of the data section using
//! the same 10-byte header format used by all other handlers:
//! `FTSJ` (4 bytes magic) | payload_len (4 bytes LE) | strategy (1 byte) | codec_mode (1 byte).

use super::DataHandler;
use crate::core::*;
use crate::crypto::CryptoLayer;
use async_trait::async_trait;
use std::time::Instant;

/// FITS file handler — pixel-LSB steganography in the data section.
pub struct FitsHandler {
    crypto: Option<CryptoLayer>,
}

/// Strategy byte stored in the FITS steganographic header.
#[derive(Debug, Clone, Copy)]
enum FitsStrategy {
    PixelLsb = 0,
}

impl FitsStrategy {
    fn to_u8(self) -> u8 {
        self as u8
    }
}

impl FitsHandler {
    pub fn new() -> Self {
        Self { crypto: None }
    }

    pub fn with_encryption(password: &str) -> Result<Self> {
        Ok(Self {
            crypto: Some(CryptoLayer::new_from_password(password)?),
        })
    }

    /// Locate the start of the first HDU data section.
    ///
    /// Scans 80-byte cards looking for the `END     ` card, then advances
    /// to the next 2880-byte boundary.  Returns `None` if the file is too
    /// short or the END card is not found within the first 100 blocks.
    fn find_data_offset(data: &[u8]) -> Option<usize> {
        if data.len() < 80 {
            return None;
        }

        let mut offset = 0usize;
        let max_scan = data.len().min(100 * 2880); // scan at most 100 blocks
        while offset + 80 <= max_scan {
            let card = &data[offset..offset + 80];
            // END card: exactly "END" followed by 77 spaces
            if card.starts_with(b"END") && card[3..].iter().all(|&b| b == b' ') {
                // Advance to next 2880-byte boundary
                let end_pos = offset + 80;
                let data_start = end_pos.div_ceil(2880) * 2880;
                return if data_start < data.len() {
                    Some(data_start)
                } else {
                    None // header fills the entire file — no data section
                };
            }
            offset += 80;
        }
        None
    }

    /// Check FITS magic: first 8 bytes must be `SIMPLE  `.
    fn is_fits(data: &[u8]) -> bool {
        data.len() >= 8 && &data[..8] == b"SIMPLE  "
    }

    /// Embed `payload` in LSBs of `image_data[data_start..]`.
    fn lsb_embed(image_data: &mut [u8], data_start: usize, payload: &[u8]) -> Result<()> {
        let available_bits = image_data.len() - data_start; // 1 bit per byte
        let needed_bits = payload.len() * 8;
        if needed_bits > available_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: needed_bits,
                available: available_bits / 8,
            });
        }

        let payload_bits: Vec<u8> = payload
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect();

        for (i, &bit) in payload_bits.iter().enumerate() {
            image_data[data_start + i] = (image_data[data_start + i] & 0xFE) | bit;
        }
        Ok(())
    }

    /// Extract `payload_bytes` bytes from LSBs of `image_data[data_start..]`.
    fn lsb_extract(image_data: &[u8], data_start: usize, payload_bytes: usize) -> Result<Vec<u8>> {
        let needed_bits = payload_bytes * 8;
        if data_start + needed_bits > image_data.len() {
            return Err(SteganoError::InvalidParameter(
                "Insufficient data section bytes for payload extraction".to_string(),
            ));
        }

        let bits: Vec<u8> = (0..needed_bits)
            .map(|i| image_data[data_start + i] & 1)
            .collect();

        Ok(bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect())
    }
}

impl Default for FitsHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataHandler for FitsHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if Self::is_fits(data) {
            Some(DataFormat::FITS)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        let data_start = Self::find_data_offset(&carrier.raw_data).unwrap_or(0);
        let data_bytes = carrier.raw_data.len().saturating_sub(data_start);
        // 1 bit per byte; header overhead = 10 bytes = 80 bits
        let capacity_bits = data_bytes;
        let overhead_bits = 80;
        let recommended_bits = capacity_bits.saturating_sub(overhead_bits);
        let mut info = CapacityInfo::new(capacity_bits, recommended_bits);
        info.format_specific_info.insert(
            "data_section_bytes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(data_bytes)),
        );
        info.format_specific_info.insert(
            "data_offset".to_string(),
            serde_json::Value::Number(serde_json::Number::from(data_start)),
        );
        Ok(info)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let embed_start = Instant::now();

        let data_start =
            Self::find_data_offset(&carrier.raw_data).ok_or(SteganoError::InvalidParameter(
                "No FITS data section found (missing END card or file too short)".to_string(),
            ))?;

        // Prepare payload (encryption + EC via codec pipeline)
        use crate::codec;
        let use_codec = !params.encryption_key.is_empty()
            || params.error_correction_level != ECLevel::None;
        let (final_payload, encryption_time_ms, codec_mode) = if use_codec {
            let enc_start = Instant::now();
            let encoded = codec::encode_payload(payload, params)?;
            let enc_ms = enc_start.elapsed().as_nanos().max(1) as u64 / 1_000_000;
            (encoded, enc_ms.max(1), 2u8)
        } else if let Some(ref crypto) = self.crypto {
            let enc_start = Instant::now();
            let sp = crypto.encrypt(payload)?;
            let enc_ms = enc_start.elapsed().as_nanos().max(1) as u64 / 1_000_000;
            (CryptoLayer::serialize_payload(&sp), enc_ms.max(1), 1u8)
        } else {
            (payload.to_vec(), 0, 0u8)
        };

        // Build 10-byte header
        let mut header = Vec::with_capacity(10);
        header.extend_from_slice(b"FTSJ");
        header.extend_from_slice(&(final_payload.len() as u32).to_le_bytes());
        header.push(FitsStrategy::PixelLsb.to_u8());
        header.push(codec_mode);

        let total_data = [header, final_payload].concat();

        // Check capacity
        let capacity = self.estimate_capacity(carrier)?;
        if total_data.len() * 8 > capacity.total_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: total_data.len() * 8,
                available: capacity.recommended_bits / 8,
            });
        }

        Self::lsb_embed(&mut carrier.raw_data, data_start, &total_data)?;

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

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let data_start =
            Self::find_data_offset(&carrier.raw_data).ok_or(SteganoError::InvalidParameter(
                "No FITS data section found (missing END card or file too short)".to_string(),
            ))?;

        // Extract 10-byte header
        let header_bytes = Self::lsb_extract(&carrier.raw_data, data_start, 10)?;

        if &header_bytes[..4] != b"FTSJ" {
            return Err(SteganoError::InvalidParameter(
                "Invalid FITS steganographic header magic".to_string(),
            ));
        }

        // strategy byte must be PixelLsb (0)
        if header_bytes[8] != FitsStrategy::PixelLsb.to_u8() {
            return Err(SteganoError::InvalidParameter(format!(
                "Unknown FITS embedding strategy byte: {}",
                header_bytes[8]
            )));
        }

        let payload_size =
            u32::from_le_bytes([header_bytes[4], header_bytes[5], header_bytes[6], header_bytes[7]])
                as usize;
        let codec_mode = header_bytes[9];

        // Header occupies 10 bytes × 8 bits/byte = 80 image bytes (1 bit per byte).
        let raw_payload = Self::lsb_extract(&carrier.raw_data, data_start + 10 * 8, payload_size)?;

        match codec_mode {
            0 => Ok(raw_payload),
            1 => {
                if let Some(ref crypto) = self.crypto {
                    let sp = CryptoLayer::deserialize_payload(&raw_payload)?;
                    crypto.decrypt(&sp)
                } else {
                    Err(SteganoError::EncryptionError(
                        "No decryption key available".to_string(),
                    ))
                }
            }
            2 => {
                use crate::codec;
                codec::decode_payload(&raw_payload, params)
            }
            _ => Err(SteganoError::IntegrityError(format!(
                "Unknown codec mode {codec_mode}"
            ))),
        }
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        if !Self::is_fits(&carrier.raw_data) {
            return Ok(false);
        }
        let data_start = match Self::find_data_offset(&carrier.raw_data) {
            Some(d) => d,
            None => return Ok(false),
        };
        // Check for FTSJ magic in first 4 bytes of data section (via LSB extraction)
        let header = match Self::lsb_extract(&carrier.raw_data, data_start, 4) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };
        Ok(&header == b"FTSJ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build a minimal synthetic FITS file with `data_bytes` bytes of pixel data.
    /// Header: SIMPLE card + END card, padded to 2880 bytes.
    fn make_fits(data_bytes: usize) -> Vec<u8> {
        let mut fits = Vec::new();

        // Card 1: SIMPLE  =                    T / file conforms to FITS standard
        let simple_card = format!("{:<80}", "SIMPLE  =                    T / synthetic test FITS");
        fits.extend_from_slice(simple_card.as_bytes());

        // Card 2: END
        let end_card = format!("{:<80}", "END");
        fits.extend_from_slice(end_card.as_bytes());

        // Pad header to 2880 bytes (36 cards of 80 bytes each)
        // We already have 2 cards (160 bytes); add 34 blank cards
        for _ in 2..36 {
            let blank = format!("{:<80}", "");
            fits.extend_from_slice(blank.as_bytes());
        }
        assert_eq!(fits.len(), 2880);

        // Data section: fill with known values
        fits.extend(std::iter::repeat_n(0x80u8, data_bytes));
        fits
    }

    #[test]
    fn test_is_fits() {
        let data = make_fits(100);
        assert!(FitsHandler::is_fits(&data));
        assert!(!FitsHandler::is_fits(b"NOTFITS"));
        assert!(!FitsHandler::is_fits(b"SIMPLE")); // only 6 bytes, need 8
    }

    #[test]
    fn test_find_data_offset() {
        let data = make_fits(100);
        // Our synthetic FITS has header of exactly 2880 bytes
        assert_eq!(FitsHandler::find_data_offset(&data), Some(2880));
    }

    #[test]
    fn test_detect_format() {
        let handler = FitsHandler::new();
        let data = make_fits(100);
        assert_eq!(handler.detect_format(&data), Some(DataFormat::FITS));
        assert_eq!(handler.detect_format(b"random"), None);
    }

    #[test]
    fn test_estimate_capacity() {
        let handler = FitsHandler::new();
        let data_bytes = 4096;
        let fits_data = make_fits(data_bytes);
        let carrier = CarrierData {
            format: DataFormat::FITS,
            raw_data: fits_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        let cap = handler.estimate_capacity(&carrier).unwrap();
        // capacity_bits = data_bytes (1 bit per byte)
        assert_eq!(cap.total_bits, data_bytes);
        assert!(cap.recommended_bits < cap.total_bits);
    }

    #[tokio::test]
    async fn test_fits_pixel_lsb_roundtrip() {
        let handler = FitsHandler::new();
        let payload = b"FITS steganography works!";

        // Need at least (10 + 25) * 8 = 280 bits = 280 data bytes
        let mut carrier = CarrierData {
            format: DataFormat::FITS,
            raw_data: make_fits(1024),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::High,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        };

        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_embedded, payload.len());

        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(&extracted, payload);
    }

    #[tokio::test]
    async fn test_fits_with_encryption() {
        let handler = FitsHandler::new();
        let payload = b"Encrypted FITS payload";

        let mut carrier = CarrierData {
            format: DataFormat::FITS,
            raw_data: make_fits(8192),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let params = EmbedParams::new(b"astro_key".to_vec());

        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);

        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(&extracted, payload);
    }

    #[tokio::test]
    async fn test_fits_insufficient_capacity() {
        let handler = FitsHandler::new();
        let large_payload = vec![0u8; 10000];

        let mut carrier = CarrierData {
            format: DataFormat::FITS,
            raw_data: make_fits(100), // only 100 data bytes = 12 usable payload bytes
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let params = EmbedParams::new(vec![]);
        let result = handler.embed(&mut carrier, &large_payload, &params).await;
        assert!(matches!(result, Err(SteganoError::InsufficientCapacity { .. })));
    }

    #[test]
    fn test_verify_integrity_with_embedded_data() {
        let handler = FitsHandler::new();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let mut carrier = CarrierData {
            format: DataFormat::FITS,
            raw_data: make_fits(1024),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        // Before embedding: no FTSJ magic → verify returns false
        assert!(!handler.verify_integrity(&carrier).unwrap());

        // After embedding (use ECLevel::None so codec overhead is zero)
        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::High,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        };
        rt.block_on(handler.embed(&mut carrier, b"test", &params)).unwrap();
        assert!(handler.verify_integrity(&carrier).unwrap());
    }

    #[test]
    fn test_find_data_offset_missing_end() {
        // A file without an END card → None
        let data = vec![0x80u8; 2880];
        assert_eq!(FitsHandler::find_data_offset(&data), None);
    }
}
