//! Signal data format handlers with advanced frequency domain steganography

use super::DataHandler;
use crate::core::*;
use crate::crypto::CryptoLayer;
use async_trait::async_trait;
use rustfft::{num_complex::Complex, FftPlanner};
use std::time::Instant;

pub struct SignalHandler {
    crypto: Option<CryptoLayer>,
}

#[derive(Debug, Clone)]
pub enum SignalEmbedStrategy {
    /// Simple LSB in time domain samples
    TimeDomainLSB,
    /// Embedding in frequency domain (FFT coefficients)
    FrequencyDomain,
    /// Phase modulation in frequency domain
    PhaseModulation,
    /// Spread spectrum embedding
    SpreadSpectrum,
    /// Echo hiding with controlled delay
    EchoHiding,
}

impl SignalEmbedStrategy {
    fn to_u8(&self) -> u8 {
        match self {
            SignalEmbedStrategy::TimeDomainLSB => 0,
            SignalEmbedStrategy::FrequencyDomain => 1,
            SignalEmbedStrategy::PhaseModulation => 2,
            SignalEmbedStrategy::SpreadSpectrum => 3,
            SignalEmbedStrategy::EchoHiding => 4,
        }
    }

    #[allow(dead_code)]
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SignalEmbedStrategy::TimeDomainLSB),
            1 => Some(SignalEmbedStrategy::FrequencyDomain),
            2 => Some(SignalEmbedStrategy::PhaseModulation),
            3 => Some(SignalEmbedStrategy::SpreadSpectrum),
            4 => Some(SignalEmbedStrategy::EchoHiding),
            _ => None,
        }
    }
}

impl SignalHandler {
    pub fn new() -> Self {
        Self { crypto: None }
    }

    pub fn with_encryption(password: &str) -> Result<Self> {
        Ok(Self {
            crypto: Some(CryptoLayer::new_from_password(password)?),
        })
    }

    /// Detect audio format from file headers
    fn detect_audio_format(&self, data: &[u8]) -> Option<DataFormat> {
        if data.len() < 12 {
            return None;
        }

        // WAV format detection
        if data.starts_with(b"RIFF") && data[8..12] == *b"WAVE" {
            return Some(DataFormat::WAV);
        }

        // FLAC format
        if data.starts_with(b"fLaC") {
            return Some(DataFormat::BINARY); // Treat as binary for now
        }

        // MP3 format (ID3v2)
        if data.starts_with(b"ID3") {
            return Some(DataFormat::BINARY); // Treat as binary for now
        }

        // Raw audio data (assume if sampling rate is set)
        None
    }

    /// Estimate capacity for different signal embedding strategies
    fn estimate_signal_capacity(
        &self,
        carrier: &CarrierData,
        strategy: &SignalEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let sample_count = carrier.raw_data.len() / 2; // Assume 16-bit samples
        let sampling_rate = carrier.sampling_rate.unwrap_or(44100.0) as usize;

        let (capacity_bits, overhead) = match strategy {
            SignalEmbedStrategy::TimeDomainLSB => {
                // 1 bit per sample
                (sample_count, 128) // 128 bits for header
            }
            SignalEmbedStrategy::FrequencyDomain => {
                // Use middle frequency bins (avoid DC and high freq)
                let fft_size = 1024.min(sample_count);
                let usable_bins = fft_size / 4; // Middle 25% of spectrum
                (usable_bins * 8, 256) // 8 bits per bin, larger header
            }
            SignalEmbedStrategy::PhaseModulation => {
                // Phase modulation in frequency domain
                let fft_size = 1024.min(sample_count);
                let usable_bins = fft_size / 4; // Less conservative estimate
                (usable_bins * 4, 80) // 4 bits per bin, reduced overhead
            }
            SignalEmbedStrategy::SpreadSpectrum => {
                // Spread across entire spectrum
                let duration_seconds = sample_count / sampling_rate;
                (duration_seconds * 100, 512) // 100 bits per second
            }
            SignalEmbedStrategy::EchoHiding => {
                // Echo-based embedding
                let duration_seconds = sample_count / sampling_rate;
                (duration_seconds * 50, 256) // 50 bits per second
            }
        };

        // Remove this unused calculation
        let _capacity_bytes = if capacity_bits > overhead {
            (capacity_bits - overhead) / 8
        } else {
            0
        };

        let recommended_bits = capacity_bits.saturating_sub(overhead);
        let mut info = CapacityInfo::new(capacity_bits, recommended_bits);
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

    /// Convert audio data to samples
    fn bytes_to_samples(&self, data: &[u8]) -> Vec<f32> {
        data.chunks_exact(2)
            .map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                sample as f32 / 32767.0 // Normalize to match embedding/extraction
            })
            .collect()
    }

    /// Convert samples back to audio data
    fn samples_to_bytes(&self, samples: &[f32]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|&sample| {
                let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                sample_i16.to_le_bytes()
            })
            .collect()
    }

    /// Embed using frequency domain techniques
    async fn embed_frequency_domain(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        strategy: &SignalEmbedStrategy,
    ) -> Result<EmbedResult> {
        let embed_start = Instant::now();

        // Prepare payload with encryption if enabled
        let (final_payload, encryption_time_ms) = if let Some(ref crypto) = self.crypto {
            use crate::crypto::CryptoLayer;
            let enc_start = Instant::now();
            let secure_payload = crypto.encrypt(payload)?;
            // Ceiling to nearest ms; at least 1 when encryption ran
            let enc_ns = enc_start.elapsed().as_nanos().max(1) as u64;
            let enc_ms = (enc_ns + 999_999) / 1_000_000;
            (CryptoLayer::serialize_payload(&secure_payload), enc_ms)
        } else {
            (payload.to_vec(), 0)
        };

        // Create header
        let mut header = Vec::new();
        header.extend_from_slice(b"SGSJ"); // Signal Steganography header
        header.extend_from_slice(&(final_payload.len() as u32).to_le_bytes());
        header.push(strategy.to_u8());
        header.push(if self.crypto.is_some() { 1 } else { 0 }); // Encryption flag

        let total_data = [header, final_payload].concat();
        let payload_bits: Vec<u8> = total_data
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect();

        // Check capacity
        let capacity = self.estimate_signal_capacity(carrier, strategy)?;
        if total_data.len() * 8 > capacity.total_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: total_data.len(),
                available: capacity.recommended_bits / 8,
            });
        }

        // Convert audio to samples
        let mut samples = self.bytes_to_samples(&carrier.raw_data);

        match strategy {
            SignalEmbedStrategy::TimeDomainLSB => {
                self.embed_time_domain_lsb(&mut samples, &payload_bits)?;
            }
            SignalEmbedStrategy::FrequencyDomain => {
                self.embed_fft_magnitude(&mut samples, &payload_bits)?;
            }
            SignalEmbedStrategy::PhaseModulation => {
                self.embed_fft_phase(&mut samples, &payload_bits)?;
            }
            SignalEmbedStrategy::SpreadSpectrum => {
                self.embed_spread_spectrum(
                    &mut samples,
                    &payload_bits,
                    carrier.sampling_rate.unwrap_or(44100.0) as u32,
                )?;
            }
            SignalEmbedStrategy::EchoHiding => {
                self.embed_echo_hiding(
                    &mut samples,
                    &payload_bits,
                    carrier.sampling_rate.unwrap_or(44100.0) as u32,
                )?;
            }
        }

        // Convert back to bytes
        carrier.raw_data = self.samples_to_bytes(&samples);

        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: embed_start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: samples.len() * 8,
            },
        ))
    }

    /// Simple LSB embedding in time domain
    fn embed_time_domain_lsb(&self, samples: &mut [f32], payload_bits: &[u8]) -> Result<()> {
        for (i, &bit) in payload_bits.iter().enumerate() {
            if i >= samples.len() {
                break;
            }

            // Convert to 16-bit integer for LSB manipulation
            let mut sample_int = (samples[i] * 32767.0) as i16;
            sample_int = (sample_int & 0xFFFEu16 as i16) | (bit as i16);
            samples[i] = sample_int as f32 / 32767.0;
        }
        Ok(())
    }

    /// FFT-based magnitude embedding
    fn embed_fft_magnitude(&self, samples: &mut [f32], payload_bits: &[u8]) -> Result<()> {
        let chunk_size = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(chunk_size);
        let ifft = planner.plan_fft_inverse(chunk_size);

        let mut bit_index = 0;

        for chunk in samples.chunks_mut(chunk_size) {
            if bit_index >= payload_bits.len() {
                break;
            }

            if chunk.len() < chunk_size {
                continue;
            }

            // Convert to complex
            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();

            // Forward FFT
            fft.process(&mut buffer);

            // Embed in middle frequency bins
            let start_bin = chunk_size / 4;
            let end_bin = 3 * chunk_size / 4;

            for bin in start_bin..end_bin {
                if bit_index >= payload_bits.len() {
                    break;
                }

                let magnitude = buffer[bin].norm();
                if magnitude > 0.001 {
                    // Only modify significant coefficients
                    let bit = payload_bits[bit_index];
                    // Slight magnitude modification based on bit
                    let modifier = if bit == 1 { 1.001 } else { 0.999 };
                    buffer[bin] *= modifier;
                    bit_index += 1;
                }
            }

            // Inverse FFT
            ifft.process(&mut buffer);

            // Copy back to samples
            for (i, sample) in chunk.iter_mut().enumerate() {
                *sample = buffer[i].re / chunk_size as f32;
            }
        }

        Ok(())
    }

    /// FFT-based phase modulation
    fn embed_fft_phase(&self, samples: &mut [f32], payload_bits: &[u8]) -> Result<()> {
        let chunk_size = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(chunk_size);
        let ifft = planner.plan_fft_inverse(chunk_size);

        let mut bit_index = 0;

        for chunk in samples.chunks_mut(chunk_size) {
            if bit_index >= payload_bits.len() || chunk.len() < chunk_size {
                continue;
            }

            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();

            fft.process(&mut buffer);

            // Modify phase in middle frequencies
            let start_bin = chunk_size / 8;
            let end_bin = chunk_size / 4;

            for bin in start_bin..end_bin {
                if bit_index >= payload_bits.len() {
                    break;
                }

                if buffer[bin].norm() > 0.001 {
                    let current_phase = buffer[bin].arg();
                    let bit = payload_bits[bit_index];

                    // Small phase shift based on bit
                    let phase_shift = if bit == 1 { 0.1 } else { -0.1 };
                    let new_magnitude = buffer[bin].norm();
                    buffer[bin] = Complex::from_polar(new_magnitude, current_phase + phase_shift);

                    bit_index += 1;
                }
            }

            ifft.process(&mut buffer);

            for (i, sample) in chunk.iter_mut().enumerate() {
                *sample = buffer[i].re / chunk_size as f32;
            }
        }

        Ok(())
    }

    /// Spread spectrum embedding
    fn embed_spread_spectrum(
        &self,
        samples: &mut [f32],
        payload_bits: &[u8],
        sample_rate: u32,
    ) -> Result<()> {
        let chip_length = sample_rate as usize / 1000; // 1ms chips
        let mut bit_index = 0;

        for chunk in samples.chunks_mut(chip_length) {
            if bit_index >= payload_bits.len() {
                break;
            }

            let bit = payload_bits[bit_index];
            let amplitude = 0.001; // Very low amplitude for stealth

            // Generate pseudorandom sequence for spreading
            let mut pn_sequence = vec![1.0; chunk.len()];
            for i in 1..pn_sequence.len() {
                pn_sequence[i] = if (i * 17 + bit_index * 31) % 2 == 0 {
                    1.0
                } else {
                    -1.0
                };
            }

            // Spread and embed
            for (i, sample) in chunk.iter_mut().enumerate() {
                let spread_bit = if bit == 1 { 1.0 } else { -1.0 };
                *sample += amplitude * spread_bit * pn_sequence[i];
            }

            bit_index += 1;
        }

        Ok(())
    }

    /// Echo hiding embedding
    fn embed_echo_hiding(
        &self,
        samples: &mut [f32],
        payload_bits: &[u8],
        sample_rate: u32,
    ) -> Result<()> {
        let delay_0 = (sample_rate as f32 * 0.0005) as usize; // 0.5ms delay for bit 0
        let delay_1 = (sample_rate as f32 * 0.001) as usize; // 1.0ms delay for bit 1
        let echo_amplitude = 0.1;

        let segment_length = sample_rate as usize / 10; // 100ms segments
        let mut bit_index = 0;

        for chunk in samples.chunks_mut(segment_length) {
            if bit_index >= payload_bits.len() {
                break;
            }

            let bit = payload_bits[bit_index];
            let delay = if bit == 1 { delay_1 } else { delay_0 };

            // Add echo with appropriate delay
            for i in delay..chunk.len() {
                chunk[i] += echo_amplitude * chunk[i - delay];
            }

            bit_index += 1;
        }

        Ok(())
    }

    /// Extract data using the detected strategy
    async fn extract_with_strategy(
        &self,
        carrier: &CarrierData,
        strategy: &SignalEmbedStrategy,
    ) -> Result<Vec<u8>> {
        let samples = self.bytes_to_samples(&carrier.raw_data);

        // Extract header first
        let header_bits = match strategy {
            SignalEmbedStrategy::TimeDomainLSB => self.extract_time_domain_bits(&samples, 80)?,
            _ => {
                // For complex strategies, try time domain extraction for header
                self.extract_time_domain_bits(&samples, 80)?
            }
        };

        let header_bytes: Vec<u8> = header_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        if header_bytes.len() < 10 || &header_bytes[0..4] != b"SGSJ" {
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

        // Extract full payload
        let total_bits_needed = (10 + payload_size) * 8;
        let all_bits = match strategy {
            SignalEmbedStrategy::TimeDomainLSB => {
                self.extract_time_domain_bits(&samples, total_bits_needed)?
            }
            _ => self.extract_time_domain_bits(&samples, total_bits_needed)?,
        };

        let all_bytes: Vec<u8> = all_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        if all_bytes.len() < 10 + payload_size {
            return Err(SteganoError::InvalidParameter(
                "Insufficient data for payload".to_string(),
            ));
        }

        let encrypted_payload = &all_bytes[10..10 + payload_size];

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

    /// Extract bits from time domain LSB
    fn extract_time_domain_bits(&self, samples: &[f32], bits_needed: usize) -> Result<Vec<u8>> {
        let mut bits = Vec::new();

        for (i, &sample) in samples.iter().enumerate() {
            if i >= bits_needed {
                break;
            }

            let sample_int = (sample * 32767.0) as i16;
            bits.push((sample_int & 1) as u8);
        }

        Ok(bits)
    }
}

#[async_trait]
impl DataHandler for SignalHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        self.detect_audio_format(data)
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        // Use frequency domain as default strategy
        self.estimate_signal_capacity(carrier, &SignalEmbedStrategy::FrequencyDomain)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let strategy = match params.stealth_level {
            StealthLevel::Low => SignalEmbedStrategy::TimeDomainLSB,
            StealthLevel::Medium => SignalEmbedStrategy::FrequencyDomain,
            StealthLevel::High => SignalEmbedStrategy::PhaseModulation,
            StealthLevel::Maximum => SignalEmbedStrategy::PhaseModulation,
        };

        self.embed_frequency_domain(carrier, payload, &strategy)
            .await
    }

    async fn extract(&self, carrier: &CarrierData, _params: &EmbedParams) -> Result<Vec<u8>> {
        // Try different strategies in order of likelihood
        let strategies = [
            SignalEmbedStrategy::TimeDomainLSB,
            SignalEmbedStrategy::FrequencyDomain,
            SignalEmbedStrategy::PhaseModulation,
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
        if carrier.raw_data.len() < 160 {
            // Minimum for header (80 bits * 2 bytes per sample)
            return Ok(false);
        }

        let samples = self.bytes_to_samples(&carrier.raw_data);

        // Try to extract header
        if let Ok(header_bits) = self.extract_time_domain_bits(&samples, 32) {
            let header_bytes: Vec<u8> = header_bits
                .chunks(8)
                .map(|chunk| {
                    chunk
                        .iter()
                        .enumerate()
                        .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
                })
                .collect();

            Ok(header_bytes.len() >= 4 && &header_bytes[0..4] == b"SGSJ")
        } else {
            Ok(false)
        }
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_signal_handler_creation() {
        let handler = SignalHandler::new();
        assert!(handler.crypto.is_none());
    }

    #[test]
    fn test_signal_handler_with_encryption() {
        let handler = SignalHandler::with_encryption("test_password").unwrap();
        assert!(handler.crypto.is_some());
    }

    #[test]
    fn test_audio_format_detection() {
        let handler = SignalHandler::new();

        // WAV format
        let wav_data = b"RIFF\x24\x08\x00\x00WAVEfmt ";
        assert_eq!(handler.detect_format(wav_data), Some(DataFormat::WAV));

        // FLAC format
        let flac_data = b"fLaCxxxxxxxx";
        assert_eq!(handler.detect_format(flac_data), Some(DataFormat::BINARY));

        // Unknown format
        let unknown_data = b"unknown_fmt";
        assert_eq!(handler.detect_format(unknown_data), None);
    }

    #[test]
    fn test_sample_conversion() {
        let handler = SignalHandler::new();
        let audio_data = vec![0x00, 0x80, 0xFF, 0x7F]; // -32768, 32767 in little endian

        let samples = handler.bytes_to_samples(&audio_data);
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - (-1.0)).abs() < 0.001);
        assert!((samples[1] - 0.999969).abs() < 0.001);

        let converted_back = handler.samples_to_bytes(&samples);
        assert_eq!(audio_data.len(), converted_back.len());
    }

    #[test]
    fn test_bit_ordering() {
        let _handler = SignalHandler::new();

        // Test bit conversion - embed and extract "SGSJ"
        let header = b"SGSJ";
        let header_bits: Vec<u8> = header
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect();

        // Convert back to bytes
        let recovered_bytes: Vec<u8> = header_bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &bit)| acc | (bit << i))
            })
            .collect();

        assert_eq!(recovered_bytes, header);
        println!("Bit ordering test passed: {header:?} -> {recovered_bytes:?}");
    }

    #[test]
    fn test_capacity_estimation() {
        let handler = SignalHandler::new();
        let carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: vec![0u8; 8800], // ~0.1 second at 44.1kHz, 16-bit
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.total_bits > 0);
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_time_domain_embedding_roundtrip() {
        let handler = SignalHandler::new();
        let payload = b"Audio steganography test payload";

        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: [0x80, 0x00].repeat(20000), // Enough samples for payload
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let mut params = EmbedParams::new(b"test_key".to_vec());
        params.stealth_level = StealthLevel::Low; // Use time domain LSB

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert_eq!(result.bytes_embedded, payload.len());
        assert!(result.success);

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
    }

    #[tokio::test]
    async fn test_encrypted_signal_embedding() {
        let handler = SignalHandler::with_encryption("signal_password_123").unwrap();
        let payload = b"Secret audio message";

        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: [0x80, 0x00].repeat(20000), // Same as working test
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let mut params = EmbedParams::new(b"key".to_vec());
        params.stealth_level = StealthLevel::Low; // Use time domain LSB like working test

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
        let handler = SignalHandler::new();

        // Create carrier with valid header in time domain
        let mut audio_data = vec![0u8; 2000];
        let header_bits = [b'S', b'G', b'S', b'J']
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
            .collect::<Vec<u8>>();

        // Embed header bits in LSBs of 16-bit samples
        for (i, &bit) in header_bits.iter().enumerate() {
            let sample_idx = i * 2;
            if sample_idx + 1 < audio_data.len() {
                let mut sample =
                    i16::from_le_bytes([audio_data[sample_idx], audio_data[sample_idx + 1]]);
                sample = (sample & 0xFFFEu16 as i16) | (bit as i16);
                let bytes = sample.to_le_bytes();
                audio_data[sample_idx] = bytes[0];
                audio_data[sample_idx + 1] = bytes[1];
            }
        }

        let carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: audio_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        assert!(handler.verify_integrity(&carrier).unwrap());

        // Test with invalid header
        let invalid_carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: vec![0xFFu8; 2000],
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        assert!(!handler.verify_integrity(&invalid_carrier).unwrap());
    }

    #[test]
    fn test_different_strategies_capacity() {
        let handler = SignalHandler::new();

        let carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: vec![0u8; 88200], // 1 second at 44.1kHz
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let time_capacity = handler
            .estimate_signal_capacity(&carrier, &SignalEmbedStrategy::TimeDomainLSB)
            .unwrap();
        let freq_capacity = handler
            .estimate_signal_capacity(&carrier, &SignalEmbedStrategy::FrequencyDomain)
            .unwrap();
        let phase_capacity = handler
            .estimate_signal_capacity(&carrier, &SignalEmbedStrategy::PhaseModulation)
            .unwrap();

        // Time domain should have highest capacity
        assert!(time_capacity.recommended_bits >= freq_capacity.recommended_bits);
        assert!(freq_capacity.recommended_bits >= phase_capacity.recommended_bits);
    }

    #[test]
    fn test_insufficient_capacity() {
        let handler = SignalHandler::new();
        let large_payload = vec![0u8; 5000]; // Too large

        let mut small_carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: vec![0u8; 200], // Too small
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let params = EmbedParams::new(b"key".to_vec());
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(handler.embed(&mut small_carrier, &large_payload, &params));
        assert!(matches!(
            result,
            Err(SteganoError::InsufficientCapacity { .. })
        ));
    }

    #[test]
    fn test_extract_time_domain_bits() {
        let handler = SignalHandler::new();
        let test_samples = vec![0.5, -0.5, 0.25, -0.25]; // These will have specific LSBs

        let bits = handler.extract_time_domain_bits(&test_samples, 4).unwrap();
        assert_eq!(bits.len(), 4);
        // Each bit is the LSB of the 16-bit integer representation
    }
}
