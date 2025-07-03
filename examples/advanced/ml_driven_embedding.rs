//! ML-Driven Adaptive Steganography Example
//! 
//! This example demonstrates advanced machine learning techniques for optimizing
//! steganographic embedding based on content analysis and adaptive strategies.

use stegano::{
    core::{CarrierData, DataFormat, EmbedParams, StealthLevel, CapacityInfo},
    handlers::HandlerRegistry,
};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Content analysis metrics for adaptive embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAnalysis {
    /// Statistical complexity score (0.0 = uniform, 1.0 = highly complex)
    pub complexity_score: f64,
    
    /// Texture richness (for images)
    pub texture_richness: f64,
    
    /// Frequency domain entropy
    pub frequency_entropy: f64,
    
    /// Edge density (for images)
    pub edge_density: Option<f64>,
    
    /// Signal-to-noise ratio (for audio)
    pub snr: Option<f64>,
    
    /// Quality score distribution (for genomics)
    pub quality_distribution: Option<Vec<f64>>,
}

/// ML-driven embedding strategy selector
#[derive(Debug, Clone)]
pub enum AdaptiveStrategy {
    /// Use edge-based embedding for high-texture images
    EdgeBased { threshold: f64 },
    
    /// Use frequency domain for audio with high SNR
    FrequencyDomain { bands: Vec<(f64, f64)> },
    
    /// Use quality score zones for genomics
    QualityZones { min_quality: u8, max_quality: u8 },
    
    /// Use LSB with adaptive bit selection
    AdaptiveLSB { bit_patterns: Vec<u8> },
    
    /// Use hybrid approach combining multiple techniques
    Hybrid { strategies: Vec<Box<AdaptiveStrategy>> },
}

/// Content analyzer for different data types
pub struct ContentAnalyzer;

impl ContentAnalyzer {
    /// Analyze carrier content to determine optimal embedding strategy
    pub fn analyze_content(&self, carrier: &CarrierData) -> Result<ContentAnalysis, Box<dyn std::error::Error>> {
        match carrier.format {
            DataFormat::PNG | DataFormat::TIFF => self.analyze_image(&carrier.raw_data, carrier.dimensions),
            DataFormat::WAV => self.analyze_audio(&carrier.raw_data, carrier.sampling_rate),
            DataFormat::FASTA | DataFormat::FASTQ => self.analyze_genomics(&carrier.raw_data),
            _ => self.analyze_generic(&carrier.raw_data),
        }
    }
    
    /// Analyze image content for optimal embedding zones
    fn analyze_image(&self, data: &[u8], dimensions: Option<(u32, u32)>) -> Result<ContentAnalysis, Box<dyn std::error::Error>> {
        let (width, height) = dimensions.unwrap_or((256, 256));
        
        // Calculate complexity using histogram analysis
        let complexity = self.calculate_histogram_complexity(data);
        
        // Analyze texture using local binary patterns
        let texture_richness = self.analyze_texture(data, width, height);
        
        // Calculate edge density using simplified Sobel operator
        let edge_density = self.calculate_edge_density(data, width, height);
        
        // Frequency domain analysis using simplified DCT
        let frequency_entropy = self.calculate_frequency_entropy(data);
        
        Ok(ContentAnalysis {
            complexity_score: complexity,
            texture_richness,
            frequency_entropy,
            edge_density: Some(edge_density),
            snr: None,
            quality_distribution: None,
        })
    }
    
    /// Analyze audio content for frequency domain embedding
    fn analyze_audio(&self, data: &[u8], sampling_rate: Option<f64>) -> Result<ContentAnalysis, Box<dyn std::error::Error>> {
        let sample_rate = sampling_rate.unwrap_or(44100.0);
        
        // Convert to samples (assuming 16-bit)
        let samples: Vec<f32> = data.chunks_exact(2)
            .map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                sample as f32 / 32767.0
            })
            .collect();
        
        // Calculate signal complexity
        let complexity = self.calculate_signal_complexity(&samples);
        
        // Estimate SNR
        let snr = self.estimate_snr(&samples);
        
        // Frequency domain analysis
        let frequency_entropy = self.analyze_frequency_spectrum(&samples, sample_rate);
        
        Ok(ContentAnalysis {
            complexity_score: complexity,
            texture_richness: 0.5, // Not applicable for audio
            frequency_entropy,
            edge_density: None,
            snr: Some(snr),
            quality_distribution: None,
        })
    }
    
    /// Analyze genomics data for quality-based embedding
    fn analyze_genomics(&self, data: &[u8]) -> Result<ContentAnalysis, Box<dyn std::error::Error>> {
        let content = String::from_utf8_lossy(data);
        
        let (complexity, quality_dist) = if content.starts_with('@') {
            // FASTQ format - analyze quality scores
            self.analyze_fastq_quality(&content)
        } else if content.starts_with('>') {
            // FASTA format - analyze sequence complexity
            (self.analyze_sequence_complexity(&content), None)
        } else {
            (0.5, None) // Default for other genomics formats
        };
        
        Ok(ContentAnalysis {
            complexity_score: complexity,
            texture_richness: 0.5,
            frequency_entropy: 0.5,
            edge_density: None,
            snr: None,
            quality_distribution: quality_dist,
        })
    }
    
    /// Generic content analysis for unknown formats
    fn analyze_generic(&self, data: &[u8]) -> Result<ContentAnalysis, Box<dyn std::error::Error>> {
        let complexity = self.calculate_histogram_complexity(data);
        let entropy = self.calculate_entropy(data);
        
        Ok(ContentAnalysis {
            complexity_score: complexity,
            texture_richness: entropy,
            frequency_entropy: entropy,
            edge_density: None,
            snr: None,
            quality_distribution: None,
        })
    }
    
    // Helper methods for content analysis
    
    fn calculate_histogram_complexity(&self, data: &[u8]) -> f64 {
        let mut histogram = [0u32; 256];
        for &byte in data {
            histogram[byte as usize] += 1;
        }
        
        // Calculate normalized entropy
        let total = data.len() as f64;
        let mut entropy = 0.0;
        for &count in &histogram {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }
        
        entropy / 8.0 // Normalize to 0-1 range
    }
    
    fn analyze_texture(&self, data: &[u8], width: u32, height: u32) -> f64 {
        // Simplified texture analysis using local variance
        let mut texture_sum = 0.0;
        let mut count = 0;
        
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let center_idx = (y * width + x) as usize;
                if center_idx < data.len() {
                    let center = data[center_idx] as f64;
                    let mut local_variance = 0.0;
                    
                    // Check 3x3 neighborhood
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let nx = x as i32 + dx;
                            let ny = y as i32 + dy;
                            if nx >= 0 && ny >= 0 {
                                let neighbor_idx = (ny as u32 * width + nx as u32) as usize;
                                if neighbor_idx < data.len() {
                                    let neighbor = data[neighbor_idx] as f64;
                                    local_variance += (neighbor - center).powi(2);
                                }
                            }
                        }
                    }
                    
                    texture_sum += local_variance / 9.0;
                    count += 1;
                }
            }
        }
        
        if count > 0 {
            (texture_sum / count as f64) / 255.0 // Normalize
        } else {
            0.0
        }
    }
    
    fn calculate_edge_density(&self, data: &[u8], width: u32, height: u32) -> f64 {
        let mut edge_count = 0;
        let mut total_pixels = 0;
        
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let idx = (y * width + x) as usize;
                if idx < data.len() {
                    let center = data[idx] as i32;
                    
                    // Simplified Sobel operator
                    let gx = 
                        -1 * data[((y-1) * width + x-1) as usize] as i32 +
                         1 * data[((y-1) * width + x+1) as usize] as i32 +
                        -2 * data[(y * width + x-1) as usize] as i32 +
                         2 * data[(y * width + x+1) as usize] as i32 +
                        -1 * data[((y+1) * width + x-1) as usize] as i32 +
                         1 * data[((y+1) * width + x+1) as usize] as i32;
                    
                    let gy = 
                        -1 * data[((y-1) * width + x-1) as usize] as i32 +
                        -2 * data[((y-1) * width + x) as usize] as i32 +
                        -1 * data[((y-1) * width + x+1) as usize] as i32 +
                         1 * data[((y+1) * width + x-1) as usize] as i32 +
                         2 * data[((y+1) * width + x) as usize] as i32 +
                         1 * data[((y+1) * width + x+1) as usize] as i32;
                    
                    let gradient_magnitude = ((gx * gx + gy * gy) as f64).sqrt();
                    if gradient_magnitude > 30.0 { // Edge threshold
                        edge_count += 1;
                    }
                    total_pixels += 1;
                }
            }
        }
        
        if total_pixels > 0 {
            edge_count as f64 / total_pixels as f64
        } else {
            0.0
        }
    }
    
    fn calculate_frequency_entropy(&self, data: &[u8]) -> f64 {
        // Simplified frequency analysis
        let mut freq_bins = [0u32; 32]; // 32 frequency bins
        
        for chunk in data.chunks(8) {
            let mut sum = 0u32;
            for &byte in chunk {
                sum = sum.wrapping_add(byte as u32);
            }
            freq_bins[(sum % 32) as usize] += 1;
        }
        
        // Calculate entropy of frequency distribution
        let total = freq_bins.iter().sum::<u32>() as f64;
        let mut entropy = 0.0;
        for &count in &freq_bins {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }
        
        entropy / 5.0 // Normalize to 0-1 range
    }
    
    fn calculate_signal_complexity(&self, samples: &[f32]) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }
        
        // Calculate variance of differences (measure of signal complexity)
        let mut diff_sum = 0.0;
        let mut diff_sq_sum = 0.0;
        
        for i in 1..samples.len() {
            let diff = samples[i] - samples[i-1];
            diff_sum += diff;
            diff_sq_sum += diff * diff;
        }
        
        let n = (samples.len() - 1) as f64;
        let mean_diff = diff_sum / n;
        let variance = diff_sq_sum / n - mean_diff * mean_diff;
        
        variance.sqrt().min(1.0) // Clamp to 0-1 range
    }
    
    fn estimate_snr(&self, samples: &[f32]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        
        // Calculate signal power
        let signal_power: f64 = samples.iter().map(|&x| x * x).sum::<f64>() / samples.len() as f64;
        
        // Estimate noise power using high-frequency content
        let mut noise_power = 0.0;
        for i in 1..samples.len() {
            let high_freq = samples[i] - samples[i-1];
            noise_power += high_freq * high_freq;
        }
        noise_power /= (samples.len() - 1) as f64;
        
        if noise_power > 0.0 {
            10.0 * (signal_power / noise_power).log10()
        } else {
            60.0 // Very high SNR
        }
    }
    
    fn analyze_frequency_spectrum(&self, samples: &[f32], _sample_rate: f64) -> f64 {
        // Simplified frequency domain analysis
        let mut power_sum = 0.0;
        let chunk_size = 1024.min(samples.len());
        
        for chunk in samples.chunks(chunk_size) {
            for &sample in chunk {
                power_sum += sample * sample;
            }
        }
        
        (power_sum / samples.len() as f64).sqrt().min(1.0)
    }
    
    fn analyze_fastq_quality(&self, content: &str) -> (f64, Option<Vec<f64>>) {
        let mut quality_scores = Vec::new();
        let mut in_quality = false;
        
        for line in content.lines() {
            if line.starts_with('+') {
                in_quality = true;
                continue;
            }
            if line.starts_with('@') {
                in_quality = false;
                continue;
            }
            
            if in_quality {
                for byte in line.bytes() {
                    let quality = (byte - 33) as f64; // Phred+33 encoding
                    quality_scores.push(quality);
                }
                in_quality = false;
            }
        }
        
        if quality_scores.is_empty() {
            return (0.5, None);
        }
        
        // Calculate quality distribution
        let mut bins = [0; 41]; // Quality scores 0-40
        for &score in &quality_scores {
            let bin = (score as usize).min(40);
            bins[bin] += 1;
        }
        
        let total = quality_scores.len() as f64;
        let distribution: Vec<f64> = bins.iter().map(|&count| count as f64 / total).collect();
        
        // Calculate complexity based on quality distribution
        let mut entropy = 0.0;
        for &p in &distribution {
            if p > 0.0 {
                entropy -= p * p.log2();
            }
        }
        
        (entropy / 5.5, Some(distribution)) // Normalize entropy
    }
    
    fn analyze_sequence_complexity(&self, content: &str) -> f64 {
        let mut base_counts = [0u32; 4]; // A, T, G, C
        let mut total_bases = 0;
        
        for line in content.lines() {
            if !line.starts_with('>') {
                for byte in line.bytes() {
                    match byte.to_ascii_uppercase() {
                        b'A' => base_counts[0] += 1,
                        b'T' => base_counts[1] += 1,
                        b'G' => base_counts[2] += 1,
                        b'C' => base_counts[3] += 1,
                        _ => continue,
                    }
                    total_bases += 1;
                }
            }
        }
        
        if total_bases == 0 {
            return 0.5;
        }
        
        // Calculate Shannon entropy
        let mut entropy = 0.0;
        for &count in &base_counts {
            if count > 0 {
                let p = count as f64 / total_bases as f64;
                entropy -= p * p.log2();
            }
        }
        
        entropy / 2.0 // Normalize to 0-1 range (max entropy for 4 bases is 2)
    }
    
    fn calculate_entropy(&self, data: &[u8]) -> f64 {
        let mut counts = [0u32; 256];
        for &byte in data {
            counts[byte as usize] += 1;
        }
        
        let total = data.len() as f64;
        let mut entropy = 0.0;
        for &count in &counts {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }
        
        entropy / 8.0 // Normalize to 0-1 range
    }
}

/// Adaptive strategy selector based on content analysis
pub struct StrategySelector;

impl StrategySelector {
    /// Select optimal embedding strategy based on content analysis
    pub fn select_strategy(&self, analysis: &ContentAnalysis, carrier_format: &DataFormat) -> AdaptiveStrategy {
        match carrier_format {
            DataFormat::PNG | DataFormat::TIFF => self.select_image_strategy(analysis),
            DataFormat::WAV => self.select_audio_strategy(analysis),
            DataFormat::FASTA | DataFormat::FASTQ => self.select_genomics_strategy(analysis),
            _ => self.select_generic_strategy(analysis),
        }
    }
    
    fn select_image_strategy(&self, analysis: &ContentAnalysis) -> AdaptiveStrategy {
        if let Some(edge_density) = analysis.edge_density {
            if edge_density > 0.3 && analysis.texture_richness > 0.6 {
                // High texture and edges - use edge-based embedding
                AdaptiveStrategy::EdgeBased { threshold: edge_density * 0.8 }
            } else if analysis.complexity_score > 0.7 {
                // High complexity - use adaptive LSB with pattern selection
                AdaptiveStrategy::AdaptiveLSB { 
                    bit_patterns: self.generate_adaptive_patterns(analysis.complexity_score)
                }
            } else {
                // Standard LSB with conservative approach
                AdaptiveStrategy::AdaptiveLSB { bit_patterns: vec![1, 2, 1, 2, 1] }
            }
        } else {
            AdaptiveStrategy::AdaptiveLSB { bit_patterns: vec![1, 2, 1, 2, 1] }
        }
    }
    
    fn select_audio_strategy(&self, analysis: &ContentAnalysis) -> AdaptiveStrategy {
        if let Some(snr) = analysis.snr {
            if snr > 40.0 && analysis.frequency_entropy > 0.6 {
                // High SNR and frequency content - use frequency domain
                AdaptiveStrategy::FrequencyDomain { 
                    bands: vec![(2000.0, 8000.0), (8000.0, 16000.0)]
                }
            } else {
                // Use time domain with careful bit selection
                AdaptiveStrategy::AdaptiveLSB { 
                    bit_patterns: self.generate_audio_patterns(snr)
                }
            }
        } else {
            AdaptiveStrategy::AdaptiveLSB { bit_patterns: vec![1, 1, 1, 1, 1] }
        }
    }
    
    fn select_genomics_strategy(&self, analysis: &ContentAnalysis) -> AdaptiveStrategy {
        if let Some(ref quality_dist) = analysis.quality_distribution {
            // FASTQ - use quality-based zones
            let high_quality_ratio = quality_dist[30..].iter().sum::<f64>();
            if high_quality_ratio > 0.3 {
                AdaptiveStrategy::QualityZones { min_quality: 30, max_quality: 40 }
            } else {
                AdaptiveStrategy::QualityZones { min_quality: 20, max_quality: 35 }
            }
        } else {
            // FASTA - use sequence complexity-based embedding
            if analysis.complexity_score > 0.8 {
                AdaptiveStrategy::AdaptiveLSB { 
                    bit_patterns: vec![2, 3, 2, 3, 2] // Higher bits for complex sequences
                }
            } else {
                AdaptiveStrategy::AdaptiveLSB { bit_patterns: vec![1, 2, 1, 2, 1] }
            }
        }
    }
    
    fn select_generic_strategy(&self, analysis: &ContentAnalysis) -> AdaptiveStrategy {
        if analysis.complexity_score > 0.8 {
            AdaptiveStrategy::AdaptiveLSB { 
                bit_patterns: self.generate_adaptive_patterns(analysis.complexity_score)
            }
        } else {
            AdaptiveStrategy::AdaptiveLSB { bit_patterns: vec![1, 1, 1, 1, 1] }
        }
    }
    
    fn generate_adaptive_patterns(&self, complexity: f64) -> Vec<u8> {
        let base_pattern = if complexity > 0.9 {
            vec![2, 3, 2, 3, 2, 3]
        } else if complexity > 0.7 {
            vec![1, 2, 2, 1, 2, 1]
        } else {
            vec![1, 1, 2, 1, 1, 1]
        };
        
        base_pattern
    }
    
    fn generate_audio_patterns(&self, snr: f64) -> Vec<u8> {
        if snr > 50.0 {
            vec![2, 2, 1, 2, 2] // Higher bits for high SNR
        } else if snr > 30.0 {
            vec![1, 2, 1, 2, 1] // Balanced approach
        } else {
            vec![1, 1, 1, 1, 1] // Conservative for low SNR
        }
    }
}

/// Main ML-driven embedding function
pub async fn ml_driven_embed(
    carrier: &mut CarrierData,
    payload: &[u8],
    base_params: &EmbedParams,
) -> Result<stegano::core::EmbedResult, Box<dyn std::error::Error>> {
    println!("🧠 Starting ML-driven adaptive embedding...");
    
    // Step 1: Analyze content
    let analyzer = ContentAnalyzer;
    let analysis = analyzer.analyze_content(carrier)?;
    
    println!("📊 Content Analysis Results:");
    println!("   Complexity Score: {:.3}", analysis.complexity_score);
    println!("   Texture Richness: {:.3}", analysis.texture_richness);
    println!("   Frequency Entropy: {:.3}", analysis.frequency_entropy);
    if let Some(edge_density) = analysis.edge_density {
        println!("   Edge Density: {:.3}", edge_density);
    }
    if let Some(snr) = analysis.snr {
        println!("   SNR: {:.1} dB", snr);
    }
    
    // Step 2: Select optimal strategy
    let selector = StrategySelector;
    let strategy = selector.select_strategy(&analysis, &carrier.format);
    
    println!("🎯 Selected Strategy: {:?}", strategy);
    
    // Step 3: Configure parameters based on strategy
    let mut optimized_params = base_params.clone();
    apply_strategy_params(&mut optimized_params, &strategy, &analysis);
    
    // Step 4: Perform embedding with optimized parameters
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format)
        .ok_or("Unsupported format")?;
    
    let result = handler.embed(carrier, payload, &optimized_params).await?;
    
    println!("✅ ML-driven embedding completed:");
    println!("   Bytes embedded: {}", result.bytes_embedded);
    println!("   Time taken: {}ms", result.performance_stats.embedding_time_ms);
    println!("   GPU acceleration: {}", result.performance_stats.gpu_acceleration_used);
    
    Ok(result)
}

/// Apply strategy-specific parameters
fn apply_strategy_params(
    params: &mut EmbedParams, 
    strategy: &AdaptiveStrategy, 
    analysis: &ContentAnalysis
) {
    match strategy {
        AdaptiveStrategy::EdgeBased { threshold } => {
            params.stealth_level = if *threshold > 0.5 { 
                StealthLevel::Medium 
            } else { 
                StealthLevel::High 
            };
            params.format_specific.insert(
                "embedding_strategy".to_string(),
                serde_json::json!("edge_based")
            );
            params.format_specific.insert(
                "edge_threshold".to_string(),
                serde_json::json!(threshold)
            );
        }
        AdaptiveStrategy::FrequencyDomain { bands } => {
            params.stealth_level = StealthLevel::High;
            params.format_specific.insert(
                "embedding_strategy".to_string(),
                serde_json::json!("frequency_domain")
            );
            params.format_specific.insert(
                "frequency_bands".to_string(),
                serde_json::json!(bands)
            );
        }
        AdaptiveStrategy::QualityZones { min_quality, max_quality } => {
            params.stealth_level = StealthLevel::Medium;
            params.format_specific.insert(
                "embedding_strategy".to_string(),
                serde_json::json!("quality_zones")
            );
            params.format_specific.insert(
                "quality_range".to_string(),
                serde_json::json!({"min": min_quality, "max": max_quality})
            );
        }
        AdaptiveStrategy::AdaptiveLSB { bit_patterns } => {
            params.stealth_level = if analysis.complexity_score > 0.8 {
                StealthLevel::Low
            } else {
                StealthLevel::Medium
            };
            params.format_specific.insert(
                "embedding_strategy".to_string(),
                serde_json::json!("adaptive_lsb")
            );
            params.format_specific.insert(
                "bit_patterns".to_string(),
                serde_json::json!(bit_patterns)
            );
        }
        AdaptiveStrategy::Hybrid { strategies: _ } => {
            params.stealth_level = StealthLevel::High;
            params.format_specific.insert(
                "embedding_strategy".to_string(),
                serde_json::json!("hybrid")
            );
        }
    }
    
    // Adjust GPU usage based on data size and complexity
    if analysis.complexity_score > 0.7 || params.use_gpu {
        params.use_gpu = true;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 ML-Driven Adaptive Steganography Demo");
    println!("========================================\n");
    
    // Test with different types of data
    let test_cases = vec![
        ("High-texture image", create_test_image(true)),
        ("Low-texture image", create_test_image(false)),
        ("High-SNR audio", create_test_audio(true)),
        ("Low-SNR audio", create_test_audio(false)),
        ("High-quality genomics", create_test_genomics(true)),
        ("Low-quality genomics", create_test_genomics(false)),
    ];
    
    for (name, mut carrier) in test_cases {
        println!("🧪 Testing: {}", name);
        println!("-------------------");
        
        let payload = b"This is a test message for ML-driven embedding analysis.";
        let params = EmbedParams::new(b"ml_test_key".to_vec());
        
        match ml_driven_embed(&mut carrier, payload, &params).await {
            Ok(result) => {
                println!("✅ Success! Embedded {} bytes\n", result.bytes_embedded);
            }
            Err(e) => {
                println!("❌ Failed: {}\n", e);
            }
        }
    }
    
    Ok(())
}

// Helper functions to create test data

fn create_test_image(high_texture: bool) -> CarrierData {
    let mut data = Vec::new();
    data.extend_from_slice(b"\x89PNG\r\n\x1a\n"); // PNG header
    
    if high_texture {
        // Create high-texture data (random-like)
        for i in 0..10000 {
            data.push(((i * 31337 + i * i) % 256) as u8);
        }
    } else {
        // Create low-texture data (gradient-like)
        for i in 0..10000 {
            data.push((i % 256) as u8);
        }
    }
    
    CarrierData {
        format: DataFormat::PNG,
        raw_data: data,
        metadata: HashMap::new(),
        dimensions: Some((100, 100)),
        sampling_rate: None,
    }
}

fn create_test_audio(high_snr: bool) -> CarrierData {
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF\x24\x08\x00\x00WAVEfmt "); // WAV header
    
    if high_snr {
        // Create clean sine wave (high SNR)
        for i in 0..1000 {
            let sample = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin();
            let sample_i16 = (sample * 30000.0) as i16;
            data.extend_from_slice(&sample_i16.to_le_bytes());
        }
    } else {
        // Create noisy signal (low SNR)
        for i in 0..1000 {
            let signal = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin();
            let noise = ((i * 17) % 1000) as f64 / 1000.0 - 0.5;
            let sample_i16 = ((signal * 10000.0) + (noise * 20000.0)) as i16;
            data.extend_from_slice(&sample_i16.to_le_bytes());
        }
    }
    
    CarrierData {
        format: DataFormat::WAV,
        raw_data: data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: Some(44100.0),
    }
}

fn create_test_genomics(high_quality: bool) -> CarrierData {
    let content = if high_quality {
        // FASTQ with high quality scores
        "@seq1\nATCGATCGATCGATCG\n+\nIIIIIIIIIIIIIIII\n@seq2\nGCTAGCTAGCTAGCTA\n+\nJJJJJJJJJJJJJJJJ\n"
    } else {
        // FASTQ with mixed quality scores
        "@seq1\nATCGATCGATCGATCG\n+\n##!!##!!##!!##!!\n@seq2\nGCTAGCTAGCTAGCTA\n+\n%%!!%%!!%%!!%%!!\n"
    };
    
    CarrierData {
        format: DataFormat::FASTQ,
        raw_data: content.as_bytes().to_vec(),
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_content_analyzer() {
        let analyzer = ContentAnalyzer;
        
        // Test with simple image data
        let image_data = create_test_image(true);
        let analysis = analyzer.analyze_content(&image_data).unwrap();
        
        assert!(analysis.complexity_score >= 0.0 && analysis.complexity_score <= 1.0);
        assert!(analysis.texture_richness >= 0.0 && analysis.texture_richness <= 1.0);
        assert!(analysis.edge_density.is_some());
    }
    
    #[test]
    fn test_strategy_selector() {
        let selector = StrategySelector;
        
        let analysis = ContentAnalysis {
            complexity_score: 0.8,
            texture_richness: 0.7,
            frequency_entropy: 0.6,
            edge_density: Some(0.4),
            snr: None,
            quality_distribution: None,
        };
        
        let strategy = selector.select_strategy(&analysis, &DataFormat::PNG);
        
        match strategy {
            AdaptiveStrategy::EdgeBased { .. } => {
                // Expected for high edge density
            }
            AdaptiveStrategy::AdaptiveLSB { .. } => {
                // Also acceptable
            }
            _ => panic!("Unexpected strategy for image data"),
        }
    }
    
    #[tokio::test]
    async fn test_ml_driven_embedding() {
        let mut carrier = create_test_image(true);
        let payload = b"Test payload for ML embedding";
        let params = EmbedParams::new(b"test_key".to_vec());
        
        let result = ml_driven_embed(&mut carrier, payload, &params).await;
        assert!(result.is_ok());
        
        let embed_result = result.unwrap();
        assert!(embed_result.success);
        assert_eq!(embed_result.bytes_embedded, payload.len());
    }
}