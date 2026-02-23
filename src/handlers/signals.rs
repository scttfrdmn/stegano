//! Signal data format handlers with advanced frequency domain steganography

use super::DataHandler;
use crate::core::*;
use crate::crypto::CryptoLayer;
use async_trait::async_trait;
use rustfft::{num_complex::Complex, FftPlanner};
use std::time::Instant;

/// Handler for digital signal carrier files (WAV, binary signal).
///
/// Supports time-domain LSB, frequency-domain, phase modulation,
/// spread-spectrum, and echo-hiding embedding strategies. The strategy
/// is selected from [`EmbedParams::stealth_level`].
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
                (sample_count, 80) // 80 bits for 10-byte header (4 magic + 4 len + 1 strategy + 1 mode)
            }
            SignalEmbedStrategy::FrequencyDomain => {
                // Only positive-frequency bins [chunk/4 .. chunk/2) = 256 bins per chunk.
                // Conjugate bins are mirrored for Hermitian symmetry but carry no extra bits.
                let chunk_size = 1024;
                let num_chunks = (sample_count / chunk_size).max(1);
                let usable_bins_per_chunk = chunk_size / 4; // [256..512) = 256 bins
                (num_chunks * usable_bins_per_chunk, 256)
            }
            SignalEmbedStrategy::PhaseModulation => {
                // Only positive-frequency bins [chunk/8 .. chunk/4) = 128 bins per chunk.
                let chunk_size = 1024;
                let num_chunks = (sample_count / chunk_size).max(1);
                let phase_bins_per_chunk = chunk_size / 8; // [128..256) = 128 bins
                (num_chunks * phase_bins_per_chunk, 80)
            }
            SignalEmbedStrategy::SpreadSpectrum => {
                // 1 bit per chip; chip_length = sample_rate / 1000 (1ms chips at 44.1kHz = 44 samples)
                let chip_length = (sampling_rate / 1000).max(1);
                (sample_count / chip_length, 80) // 80 bits for 10-byte header
            }
            SignalEmbedStrategy::EchoHiding => {
                // 1 bit per 100ms segment; segment_length = sample_rate / 10
                let segment_length = (sampling_rate / 10).max(1);
                (sample_count / segment_length, 80) // 80 bits for 10-byte header
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
        params: &EmbedParams,
        strategy: &SignalEmbedStrategy,
    ) -> Result<EmbedResult> {
        let embed_start = Instant::now();

        // Prepare payload: params key + EC takes priority over self.crypto.
        // codec_mode byte: 0 = raw, 1 = self.crypto (legacy), 2 = codec pipeline
        use crate::codec;
        let use_codec = !params.encryption_key.is_empty()
            || params.error_correction_level != ECLevel::None;
        let (final_payload, encryption_time_ms, codec_mode) = if use_codec {
            let enc_start = Instant::now();
            let encoded = codec::encode_payload(payload, params)?;
            let enc_ms = enc_start.elapsed().as_nanos().max(1) as u64 / 1_000_000;
            (encoded, enc_ms.max(1), 2u8)
        } else if let Some(ref crypto) = self.crypto {
            use crate::crypto::CryptoLayer;
            let enc_start = Instant::now();
            let secure_payload = crypto.encrypt(payload)?;
            let enc_ns = enc_start.elapsed().as_nanos().max(1) as u64;
            let enc_ms = enc_ns.div_ceil(1_000_000);
            (CryptoLayer::serialize_payload(&secure_payload), enc_ms, 1u8)
        } else {
            (payload.to_vec(), 0, 0u8)
        };

        // Create header
        let mut header = Vec::new();
        header.extend_from_slice(b"SGSJ"); // Signal Steganography header
        header.extend_from_slice(&(final_payload.len() as u32).to_le_bytes());
        header.push(strategy.to_u8());
        header.push(codec_mode); // 0=raw, 1=self.crypto, 2=codec

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

    /// FFT-based magnitude embedding using parity quantization.
    ///
    /// Encodes each bit by forcing the quantised magnitude of a frequency-domain
    /// coefficient to an even (bit=0) or odd (bit=1) multiple of `MAG_STEP`.
    ///
    /// Only the positive-frequency bins `[chunk/4 .. chunk/2]` are modified.
    /// After each modification the conjugate bin `N-k` is set to `conj(X[k])`
    /// so the IFFT output is purely real — if we modified both halves
    /// independently the IFFT real part would be the average of the two,
    /// destroying the parity encoding.
    fn embed_fft_magnitude(&self, samples: &mut [f32], payload_bits: &[u8]) -> Result<()> {
        // MAG_STEP must satisfy: MAG_STEP/2 > i16 round-trip noise per bin.
        // Worst-case noise ≈ 1024 * 1/(2*32767) ≈ 0.016.  MAG_STEP=0.5 gives 15× margin.
        const MAG_STEP: f32 = 0.5;
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

            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();

            fft.process(&mut buffer);

            // Only touch positive-frequency bins [chunk/4 .. chunk/2).
            // Conjugate bins [chunk/2+1 .. 3*chunk/4] are written separately below.
            let start_bin = chunk_size / 4; // 256
            let end_bin = chunk_size / 2; // 512

            for bin_k in start_bin..end_bin {
                if bit_index >= payload_bits.len() {
                    break;
                }

                let bit = payload_bits[bit_index];
                let phase = buffer[bin_k].arg();
                // Use max(magnitude, MAG_STEP) so near-zero bins still carry a reliable bit.
                let magnitude = buffer[bin_k].norm().max(MAG_STEP);
                let units = (magnitude / MAG_STEP).round() as u64;
                let new_units = if units % 2 == bit as u64 {
                    units
                } else {
                    units + 1
                };
                let new_mag = new_units as f32 * MAG_STEP;
                buffer[bin_k] = Complex::from_polar(new_mag, phase);
                // Enforce Hermitian symmetry: X[N-k] = conj(X[k])
                buffer[chunk_size - bin_k] = Complex::from_polar(new_mag, -phase);
                bit_index += 1;
            }

            ifft.process(&mut buffer);

            for (i, sample) in chunk.iter_mut().enumerate() {
                *sample = buffer[i].re / chunk_size as f32;
            }
        }

        Ok(())
    }

    /// Extract bits embedded by `embed_fft_magnitude` (parity quantisation).
    ///
    /// Reads only the positive-frequency bins `[chunk/4 .. chunk/2)` to match embed.
    fn extract_fft_magnitude_bits(&self, samples: &[f32], bits_needed: usize) -> Result<Vec<u8>> {
        const MAG_STEP: f32 = 0.5;
        let chunk_size = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(chunk_size);

        let mut bits = Vec::with_capacity(bits_needed);

        for chunk in samples.chunks(chunk_size) {
            if bits.len() >= bits_needed || chunk.len() < chunk_size {
                break;
            }

            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();
            fft.process(&mut buffer);

            // Read the same positive-frequency bins as embed.
            let start_bin = chunk_size / 4; // 256
            let end_bin = chunk_size / 2; // 512

            for elem in &buffer[start_bin..end_bin] {
                if bits.len() >= bits_needed {
                    break;
                }

                let magnitude = elem.norm().max(MAG_STEP);
                let units = (magnitude / MAG_STEP).round() as u64;
                bits.push((units % 2) as u8);
            }
        }

        bits.resize(bits_needed, 0);
        Ok(bits)
    }

    /// FFT-based phase modulation using parity quantisation.
    ///
    /// Encodes each bit by forcing the quantised phase (offset by π) to an even
    /// (bit=0) or odd (bit=1) multiple of `PHASE_STEP`.  Only positive-frequency
    /// bins `[chunk/8 .. chunk/4)` are modified; conjugate bins are set to
    /// `conj(X[k])` so the IFFT output stays real.
    fn embed_fft_phase(&self, samples: &mut [f32], payload_bits: &[u8]) -> Result<()> {
        // Worst-case phase error per bin ≈ 0.016 / NORM_MIN. With NORM_MIN=0.5,
        // error ≤ 0.032 rad << PHASE_STEP/2=0.05. Safe.
        const PHASE_STEP: f32 = 0.1;
        // Maximum integer index such that max_units * PHASE_STEP < 2π.
        let max_units = (2.0 * std::f32::consts::PI / PHASE_STEP).floor() as u64 - 1;
        const NORM_MIN: f32 = 0.5;
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

            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();

            fft.process(&mut buffer);

            // Only touch positive-frequency bins [chunk/8 .. chunk/4).
            let start_bin = chunk_size / 8; // 128
            let end_bin = chunk_size / 4; // 256

            for bin_k in start_bin..end_bin {
                if bit_index >= payload_bits.len() {
                    break;
                }

                let bit = payload_bits[bit_index];
                // rem_euclid maps arg() ∈ (-π, π] to [0, 2π).
                let phase_offset = (buffer[bin_k].arg() + std::f32::consts::PI)
                    .rem_euclid(2.0 * std::f32::consts::PI);
                let units = (phase_offset / PHASE_STEP).round() as u64;
                let units = units.min(max_units);
                let new_units = if units % 2 == bit as u64 {
                    units
                } else if units < max_units {
                    units + 1
                } else {
                    // At ceiling: step DOWN to avoid wrapping past π.
                    units - 1
                };
                let new_phase = new_units as f32 * PHASE_STEP - std::f32::consts::PI;
                let norm = buffer[bin_k].norm().max(NORM_MIN);
                buffer[bin_k] = Complex::from_polar(norm, new_phase);
                // Enforce Hermitian symmetry: X[N-k] = conj(X[k])
                buffer[chunk_size - bin_k] = Complex::from_polar(norm, -new_phase);
                bit_index += 1;
            }

            ifft.process(&mut buffer);

            for (i, sample) in chunk.iter_mut().enumerate() {
                *sample = buffer[i].re / chunk_size as f32;
            }
        }

        Ok(())
    }

    /// Extract bits embedded by `embed_fft_phase` (parity quantisation).
    ///
    /// Reads only the positive-frequency bins `[chunk/8 .. chunk/4)` to match embed.
    fn extract_fft_phase_bits(&self, samples: &[f32], bits_needed: usize) -> Result<Vec<u8>> {
        const PHASE_STEP: f32 = 0.1;
        let max_units = (2.0 * std::f32::consts::PI / PHASE_STEP).floor() as u64 - 1;
        let chunk_size = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(chunk_size);

        let mut bits = Vec::with_capacity(bits_needed);

        for chunk in samples.chunks(chunk_size) {
            if bits.len() >= bits_needed || chunk.len() < chunk_size {
                break;
            }

            let mut buffer: Vec<Complex<f32>> =
                chunk.iter().map(|&x| Complex::new(x, 0.0)).collect();
            fft.process(&mut buffer);

            // Read the same positive-frequency bins as embed.
            let start_bin = chunk_size / 8; // 128
            let end_bin = chunk_size / 4; // 256

            for elem in &buffer[start_bin..end_bin] {
                if bits.len() >= bits_needed {
                    break;
                }

                let phase_offset = (elem.arg() + std::f32::consts::PI)
                    .rem_euclid(2.0 * std::f32::consts::PI);
                let units = (phase_offset / PHASE_STEP).round() as u64;
                let units = units.min(max_units);
                bits.push((units % 2) as u8);
            }
        }

        bits.resize(bits_needed, 0);
        Ok(bits)
    }

    /// Spread spectrum embedding
    fn embed_spread_spectrum(
        &self,
        samples: &mut [f32],
        payload_bits: &[u8],
        sample_rate: u32,
    ) -> Result<()> {
        let chip_length = sample_rate as usize / 1000; // 1ms chips
        for (bit_index, chunk) in samples.chunks_mut(chip_length).enumerate() {
            if bit_index >= payload_bits.len() {
                break;
            }

            let bit = payload_bits[bit_index];
            let amplitude = 0.001; // Very low amplitude for stealth

            // Generate pseudorandom sequence for spreading
            let mut pn_sequence = vec![1.0; chunk.len()];
            for (i, elem) in pn_sequence.iter_mut().enumerate().skip(1) {
                *elem = if (i * 17 + bit_index * 31) % 2 == 0 {
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
        for (bit_index, chunk) in samples.chunks_mut(segment_length).enumerate() {
            if bit_index >= payload_bits.len() {
                break;
            }

            let bit = payload_bits[bit_index];
            let delay = if bit == 1 { delay_1 } else { delay_0 };

            // Add echo with appropriate delay
            for i in delay..chunk.len() {
                chunk[i] += echo_amplitude * chunk[i - delay];
            }
        }

        Ok(())
    }

    /// Extract bits embedded by `embed_spread_spectrum` via PN correlation.
    fn extract_spread_spectrum_bits(
        &self,
        samples: &[f32],
        bits_needed: usize,
        sample_rate: u32,
    ) -> Result<Vec<u8>> {
        let chip_length = sample_rate as usize / 1000;
        let mut bits = Vec::with_capacity(bits_needed);

        for (bit_index, chunk) in samples.chunks(chip_length).enumerate() {
            if bits.len() >= bits_needed {
                break;
            }

            // Reconstruct the same PN sequence used during embed
            let mut pn_sequence = vec![1.0f32; chunk.len()];
            for (i, elem) in pn_sequence.iter_mut().enumerate().skip(1) {
                *elem = if (i * 17 + bit_index * 31) % 2 == 0 {
                    1.0
                } else {
                    -1.0
                };
            }

            // Correlate chunk with +PN and -PN
            let corr_pos: f32 = chunk
                .iter()
                .zip(pn_sequence.iter())
                .map(|(&s, &p)| s * p)
                .sum();
            bits.push(if corr_pos >= 0.0 { 1 } else { 0 });
        }

        bits.resize(bits_needed, 0);
        Ok(bits)
    }

    /// Extract bits embedded by `embed_echo_hiding` via autocorrelation.
    fn extract_echo_hiding_bits(
        &self,
        samples: &[f32],
        bits_needed: usize,
        sample_rate: u32,
    ) -> Result<Vec<u8>> {
        let delay_0 = (sample_rate as f32 * 0.0005) as usize;
        let delay_1 = (sample_rate as f32 * 0.001) as usize;
        let segment_length = sample_rate as usize / 10;
        let mut bits = Vec::with_capacity(bits_needed);

        for chunk in samples.chunks(segment_length) {
            if bits.len() >= bits_needed {
                break;
            }

            // Compute autocorrelation at delay_0 and delay_1
            let len = chunk.len();
            let autocorr = |delay: usize| -> f32 {
                if delay >= len {
                    return 0.0;
                }
                chunk[delay..].iter().zip(chunk[..len - delay].iter())
                    .map(|(&a, &b)| a * b)
                    .sum::<f32>()
            };

            let r0 = autocorr(delay_0).abs();
            let r1 = autocorr(delay_1).abs();
            bits.push(if r1 > r0 { 1 } else { 0 });
        }

        bits.resize(bits_needed, 0);
        Ok(bits)
    }

    /// Extract data using the detected strategy
    async fn extract_with_strategy(
        &self,
        carrier: &CarrierData,
        strategy: &SignalEmbedStrategy,
        params: &EmbedParams,
    ) -> Result<Vec<u8>> {
        let samples = self.bytes_to_samples(&carrier.raw_data);

        // Extract header (10 bytes = 80 bits) using per-strategy extractor
        let header_bits = match strategy {
            SignalEmbedStrategy::TimeDomainLSB => {
                self.extract_time_domain_bits(&samples, 80)?
            }
            SignalEmbedStrategy::FrequencyDomain => {
                self.extract_fft_magnitude_bits(&samples, 80)?
            }
            SignalEmbedStrategy::PhaseModulation => {
                self.extract_fft_phase_bits(&samples, 80)?
            }
            SignalEmbedStrategy::SpreadSpectrum => {
                self.extract_spread_spectrum_bits(
                    &samples,
                    80,
                    carrier.sampling_rate.unwrap_or(44100.0) as u32,
                )?
            }
            SignalEmbedStrategy::EchoHiding => {
                self.extract_echo_hiding_bits(
                    &samples,
                    80,
                    carrier.sampling_rate.unwrap_or(44100.0) as u32,
                )?
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

        let codec_mode = header_bytes[9]; // 0=raw, 1=self.crypto, 2=codec

        // Extract full payload using the same per-strategy extractor
        let total_bits_needed = (10 + payload_size) * 8;
        let all_bits = match strategy {
            SignalEmbedStrategy::TimeDomainLSB => {
                self.extract_time_domain_bits(&samples, total_bits_needed)?
            }
            SignalEmbedStrategy::FrequencyDomain => {
                self.extract_fft_magnitude_bits(&samples, total_bits_needed)?
            }
            SignalEmbedStrategy::PhaseModulation => {
                self.extract_fft_phase_bits(&samples, total_bits_needed)?
            }
            SignalEmbedStrategy::SpreadSpectrum => self.extract_spread_spectrum_bits(
                &samples,
                total_bits_needed,
                carrier.sampling_rate.unwrap_or(44100.0) as u32,
            )?,
            SignalEmbedStrategy::EchoHiding => self.extract_echo_hiding_bits(
                &samples,
                total_bits_needed,
                carrier.sampling_rate.unwrap_or(44100.0) as u32,
            )?,
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

        let raw_payload = &all_bytes[10..10 + payload_size];

        // Decode according to the codec_mode flag stored in the header.
        match codec_mode {
            0 => Ok(raw_payload.to_vec()),
            1 => {
                // Legacy: encrypted with self.crypto
                if let Some(ref crypto) = self.crypto {
                    use crate::crypto::CryptoLayer;
                    let secure_payload = CryptoLayer::deserialize_payload(raw_payload)?;
                    crypto.decrypt(&secure_payload)
                } else {
                    Err(SteganoError::EncryptionError(
                        "No decryption key available".to_string(),
                    ))
                }
            }
            2 => {
                // New codec pipeline (encryption + EC via params)
                use crate::codec;
                codec::decode_payload(raw_payload, params)
            }
            _ => Err(SteganoError::IntegrityError(format!(
                "Unknown codec mode {codec_mode}"
            ))),
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

        self.embed_frequency_domain(carrier, payload, params, &strategy)
            .await
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        // Dispatch to the same strategy that embed() uses for this stealth_level,
        // so extract is always the exact inverse of embed.
        let strategy = match params.stealth_level {
            StealthLevel::Low => SignalEmbedStrategy::TimeDomainLSB,
            StealthLevel::Medium => SignalEmbedStrategy::FrequencyDomain,
            StealthLevel::High => SignalEmbedStrategy::PhaseModulation,
            StealthLevel::Maximum => SignalEmbedStrategy::PhaseModulation,
        };
        self.extract_with_strategy(carrier, &strategy, params).await
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

    /// Generate a pseudo-random carrier with frequency content across the spectrum.
    /// Using a simple linear congruential sequence to produce varied byte values.
    fn make_varied_carrier(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8) as u8)
            .collect()
    }

    #[tokio::test]
    async fn test_frequency_domain_embedding_roundtrip() {
        let handler = SignalHandler::new();
        let payload = b"frequency domain test";

        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: make_varied_carrier(40960), // 20 full 1024-sample chunks
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        // ECLevel::None: no RS overhead, verifies the raw FFT magnitude round-trip.
        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Medium, // FrequencyDomain
            use_gpu: false,
            enable_validation: false,
            format_specific: std::collections::HashMap::new(),
        };

        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_embedded, payload.len());

        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
    }

    #[tokio::test]
    async fn test_phase_modulation_embedding_roundtrip() {
        let handler = SignalHandler::new();
        let payload = b"phase modulation test";

        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: make_varied_carrier(40960), // 20 full 1024-sample chunks
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        // ECLevel::None: no RS overhead, verifies the raw FFT phase round-trip.
        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::High, // PhaseModulation
            use_gpu: false,
            enable_validation: false,
            format_specific: std::collections::HashMap::new(),
        };

        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_embedded, payload.len());

        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);
    }

    #[tokio::test]
    async fn test_spread_spectrum_embedding_roundtrip() {
        let handler = SignalHandler::new();
        let payload = b"ss";

        // Silent (all-zero) carrier: ensures the PN-correlation SNR is sufficient
        // for reliable 1-bit-per-chip extraction (amplitude=0.001, chip_length=44 at
        // 44.1 kHz → correlation magnitude ≈ 0.044, well above zero noise floor).
        let sample_rate = 44100u32;
        let chip_length = sample_rate as usize / 1000; // 44 samples per bit
        let total_bits = (10 + payload.len()) * 8; // header + payload bits
        let carrier_samples = (total_bits + 10) * chip_length; // generous headroom
        let carrier_bytes = carrier_samples * 2; // 16-bit samples

        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: vec![0u8; carrier_bytes],
            metadata: std::collections::HashMap::new(),
            dimensions: None,
            sampling_rate: Some(sample_rate as f64),
        };

        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low, // will be overridden by direct strategy call
            use_gpu: false,
            enable_validation: false,
            format_specific: std::collections::HashMap::new(),
        };

        handler
            .embed_frequency_domain(&mut carrier, payload, &params, &SignalEmbedStrategy::SpreadSpectrum)
            .await
            .unwrap();

        let extracted = handler
            .extract_with_strategy(&carrier, &SignalEmbedStrategy::SpreadSpectrum, &params)
            .await
            .unwrap();
        assert_eq!(extracted, payload);
    }

    /// Generate white-noise-like i16 samples using a Xorshift32 PRNG (iterative state
    /// update, not index-based).  Adjacent samples are nearly uncorrelated, giving
    /// echo hiding the low-baseline-autocorrelation carrier it needs.
    fn make_echo_carrier(len: usize) -> Vec<u8> {
        let mut state: u32 = 2463534242; // Xorshift seed
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect()
    }

    #[tokio::test]
    async fn test_echo_hiding_embedding_roundtrip() {
        let handler = SignalHandler::new();
        let payload = b"echo";

        // Echo hiding encodes 1 bit per 100ms segment (segment_length = 4410 samples at
        // 44.1 kHz).  Total bits = (10 + 4) * 8 = 112.  Need ≥ 112 * 4410 * 2 ≈ 988 KB.
        // Use 1.1 MB of white-noise-like i16 samples (Xorshift32) so the echo at
        // delay_0 / delay_1 is detectable against a near-zero autocorrelation baseline.
        let mut carrier = CarrierData {
            format: DataFormat::WAV,
            raw_data: make_echo_carrier(1_100_000),
            metadata: std::collections::HashMap::new(),
            dimensions: None,
            sampling_rate: Some(44100.0),
        };

        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            enable_validation: false,
            format_specific: std::collections::HashMap::new(),
        };

        handler
            .embed_frequency_domain(&mut carrier, payload, &params, &SignalEmbedStrategy::EchoHiding)
            .await
            .unwrap();

        let extracted = handler
            .extract_with_strategy(&carrier, &SignalEmbedStrategy::EchoHiding, &params)
            .await
            .unwrap();
        assert_eq!(extracted, payload);
    }
}
