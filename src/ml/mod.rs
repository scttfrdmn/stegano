//! Machine Learning Optimization Framework
//!
//! Implements adaptive steganography using machine learning to optimize
//! embedding strategies based on carrier data characteristics, detection
//! resistance, and embedding efficiency.

pub mod adaptive;
pub mod analysis;
pub mod bayesian;
pub mod cuda_models;
pub mod models;
pub mod optimization;
pub mod reinforcement;
pub mod training;

use crate::core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Machine learning-driven steganography optimizer
#[derive(Debug)]
pub struct MLOptimizer {
    models: HashMap<DataFormat, Box<dyn MLModel>>,
    config: MLConfig,
    training_data: Vec<TrainingExample>,
    _performance_cache: HashMap<String, PerformanceMetrics>,
}

/// Configuration for ML optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLConfig {
    /// Enable adaptive embedding strategy selection
    pub adaptive_strategy: bool,

    /// Minimum training examples required before optimization
    pub min_training_examples: usize,

    /// Learning rate for online adaptation
    pub learning_rate: f32,

    /// Weight for detection resistance vs embedding capacity trade-off
    pub detection_resistance_weight: f32,

    /// Weight for embedding efficiency vs quality trade-off
    pub efficiency_weight: f32,

    /// Enable real-time performance feedback
    pub real_time_feedback: bool,

    /// Maximum model training iterations
    pub max_training_iterations: usize,
}

/// Training example for ML optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Carrier data characteristics
    pub carrier_features: CarrierFeatures,

    /// Embedding parameters used
    pub embed_params: EmbedParams,

    /// Resulting performance metrics
    pub performance: PerformanceMetrics,

    /// Detection resistance score (0.0 = easily detected, 1.0 = undetectable)
    pub detection_resistance: f32,

    /// Timestamp of the example
    pub timestamp: u64,
}

/// Extracted features from carrier data for ML analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierFeatures {
    /// Data format
    pub format: DataFormat,

    /// File size in bytes
    pub size: usize,

    /// Entropy measurement
    pub entropy: f32,

    /// Frequency domain characteristics
    pub frequency_characteristics: Vec<f32>,

    /// Spatial/temporal complexity measures
    pub complexity_measures: Vec<f32>,

    /// Format-specific features
    pub format_specific: HashMap<String, f32>,
}

/// Performance metrics for ML optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Embedding capacity efficiency (0.0-1.0)
    pub capacity_efficiency: f32,

    /// Embedding time in milliseconds
    pub embedding_time_ms: u64,

    /// Extraction accuracy (0.0-1.0)
    pub extraction_accuracy: f32,

    /// Carrier quality preservation (0.0-1.0)
    pub quality_preservation: f32,

    /// Memory usage in bytes
    pub memory_usage: usize,

    /// GPU acceleration benefit (speedup factor)
    pub gpu_acceleration_benefit: f32,
}

/// ML model trait for format-specific optimization
pub trait MLModel: std::fmt::Debug + Send + Sync {
    /// Predict optimal embedding parameters for given carrier features
    fn predict_optimal_params(&self, features: &CarrierFeatures) -> Result<EmbedParams>;

    /// Update model with new training example
    fn update(&mut self, example: &TrainingExample) -> Result<()>;

    /// Get model confidence in prediction (0.0-1.0)
    fn get_confidence(&self, features: &CarrierFeatures) -> f32;

    /// Evaluate model performance on test data
    fn evaluate(&self, test_data: &[TrainingExample]) -> Result<ModelPerformance>;

    /// Save model state
    fn save(&self, path: &str) -> Result<()>;

    /// Load model state
    fn load(&mut self, path: &str) -> Result<()>;
}

/// Model performance evaluation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPerformance {
    /// Mean squared error for capacity prediction
    pub capacity_mse: f32,

    /// Mean squared error for time prediction
    pub time_mse: f32,

    /// Accuracy for optimal strategy prediction
    pub strategy_accuracy: f32,

    /// R-squared correlation coefficient
    pub r_squared: f32,

    /// Number of test examples evaluated
    pub sample_size: usize,
}

/// Adaptive embedding strategy recommendation
#[derive(Debug, Clone)]
pub struct StrategyRecommendation {
    /// Recommended embedding parameters
    pub params: EmbedParams,

    /// Predicted performance metrics
    pub predicted_performance: PerformanceMetrics,

    /// Confidence in recommendation (0.0-1.0)
    pub confidence: f32,

    /// Alternative strategies if available
    pub alternatives: Vec<(EmbedParams, f32)>, // (params, confidence)

    /// Reasoning for the recommendation
    pub reasoning: String,
}

impl Default for MLConfig {
    fn default() -> Self {
        Self {
            adaptive_strategy: true,
            min_training_examples: 100,
            learning_rate: 0.01,
            detection_resistance_weight: 0.7,
            efficiency_weight: 0.3,
            real_time_feedback: true,
            max_training_iterations: 1000,
        }
    }
}

impl Default for MLOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl MLOptimizer {
    /// Create a new ML optimizer with default configuration
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            config: MLConfig::default(),
            training_data: Vec::new(),
            _performance_cache: HashMap::new(),
        }
    }

    /// Create ML optimizer with custom configuration
    pub fn with_config(config: MLConfig) -> Self {
        Self {
            models: HashMap::new(),
            config,
            training_data: Vec::new(),
            _performance_cache: HashMap::new(),
        }
    }

    /// Register an ML model for a specific data format
    pub fn register_model(&mut self, format: DataFormat, model: Box<dyn MLModel>) {
        self.models.insert(format, model);
    }

    /// Extract features from carrier data
    pub fn extract_features(&self, carrier: &CarrierData) -> Result<CarrierFeatures> {
        // Implementation will analyze carrier data and extract relevant features
        Ok(CarrierFeatures {
            format: carrier.format.clone(),
            size: carrier.raw_data.len(),
            entropy: self.calculate_entropy(&carrier.raw_data),
            frequency_characteristics: self.analyze_frequency_domain(carrier)?,
            complexity_measures: self.calculate_complexity_measures(carrier)?,
            format_specific: self.extract_format_specific_features(carrier)?,
        })
    }

    /// Get optimal embedding strategy recommendation
    pub async fn recommend_strategy(
        &self,
        carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<StrategyRecommendation> {
        let features = self.extract_features(carrier)?;

        if let Some(model) = self.models.get(&carrier.format) {
            let params = model.predict_optimal_params(&features)?;
            let confidence = model.get_confidence(&features);

            // Predict performance based on historical data
            let predicted_performance = self.predict_performance(&features, &params)?;

            Ok(StrategyRecommendation {
                params: params.clone(),
                predicted_performance,
                confidence,
                alternatives: self.generate_alternative_strategies(&features, &params)?,
                reasoning: format!(
                    "ML model for {:?} format recommends strategy based on {} training examples",
                    carrier.format,
                    self.training_data.len()
                ),
            })
        } else {
            // Fallback to default parameters if no model available
            Ok(StrategyRecommendation {
                params: EmbedParams {
                    encryption_key: vec![],
                    error_correction_level: ECLevel::Low,
                    stealth_level: StealthLevel::Medium,
                    use_gpu: false,
                    format_specific: HashMap::new(),
                    enable_validation: false,
                    cipher_kind: Default::default(),
                },
                predicted_performance: PerformanceMetrics {
                    capacity_efficiency: 0.5,
                    embedding_time_ms: 1000,
                    extraction_accuracy: 0.9,
                    quality_preservation: 0.8,
                    memory_usage: carrier.raw_data.len() * 2,
                    gpu_acceleration_benefit: 1.0,
                },
                confidence: 0.1, // Low confidence without trained model
                alternatives: Vec::new(),
                reasoning: "Using default parameters - no trained model available".to_string(),
            })
        }
    }

    /// Add training example and update models
    pub fn add_training_example(&mut self, example: TrainingExample) -> Result<()> {
        // Add to training data
        self.training_data.push(example.clone());

        // Update relevant model if available
        if let Some(model) = self.models.get_mut(&example.carrier_features.format) {
            model.update(&example)?;
        }

        // Trigger retraining if we have enough examples
        if self.training_data.len() >= self.config.min_training_examples
            && self.training_data.len().is_multiple_of(50)
        {
            self.retrain_models()?;
        }

        Ok(())
    }

    /// Retrain all models with accumulated training data
    pub fn retrain_models(&mut self) -> Result<()> {
        // Group training data by format
        let mut format_data: HashMap<DataFormat, Vec<&TrainingExample>> = HashMap::new();

        for example in &self.training_data {
            format_data
                .entry(example.carrier_features.format.clone())
                .or_default()
                .push(example);
        }

        // Retrain each model with its format-specific data
        for (format, examples) in format_data {
            if let Some(model) = self.models.get_mut(&format) {
                for example in examples {
                    model.update(example)?;
                }
            }
        }

        Ok(())
    }

    /// Calculate Shannon entropy of data
    fn calculate_entropy(&self, data: &[u8]) -> f32 {
        let mut frequency = [0u32; 256];

        // Count byte frequencies
        for &byte in data {
            frequency[byte as usize] += 1;
        }

        // Calculate entropy
        let len = data.len() as f32;
        let mut entropy = 0.0;

        for &count in &frequency {
            if count > 0 {
                let p = count as f32 / len;
                entropy -= p * p.log2();
            }
        }

        entropy
    }

    /// Analyze frequency domain characteristics using FFT
    fn analyze_frequency_domain(&self, carrier: &CarrierData) -> Result<Vec<f32>> {
        let data = &carrier.raw_data;

        // Convert bytes to f32 for FFT processing
        let samples: Vec<f32> = data.iter().map(|&x| x as f32).collect();

        // Ensure we have a power of 2 for FFT efficiency
        let mut fft_size = 1;
        while fft_size < samples.len() {
            fft_size *= 2;
        }
        fft_size = fft_size.min(4096); // Limit to reasonable size

        let mut signal = samples.clone();
        signal.resize(fft_size, 0.0);

        // Simple FFT implementation using DFT
        let frequency_bins = self.compute_dft(&signal);

        // Calculate magnitude spectrum
        let magnitude_spectrum: Vec<f32> = frequency_bins
            .chunks(2)
            .map(|chunk| {
                let real = chunk[0];
                let imag = chunk.get(1).copied().unwrap_or(0.0);
                (real * real + imag * imag).sqrt()
            })
            .collect();

        // Extract meaningful frequency features
        let mut features = Vec::new();

        // 1. Low frequency energy (0-25% of spectrum)
        let low_freq_end = magnitude_spectrum.len() / 4;
        let low_freq_energy = magnitude_spectrum[..low_freq_end].iter().sum::<f32>();
        features.push(low_freq_energy);

        // 2. Mid frequency energy (25-50% of spectrum)
        let mid_freq_start = low_freq_end;
        let mid_freq_end = magnitude_spectrum.len() / 2;
        let mid_freq_energy = magnitude_spectrum[mid_freq_start..mid_freq_end]
            .iter()
            .sum::<f32>();
        features.push(mid_freq_energy);

        // 3. High frequency energy (50-75% of spectrum)
        let high_freq_start = mid_freq_end;
        let high_freq_end = magnitude_spectrum.len() * 3 / 4;
        let high_freq_energy = magnitude_spectrum[high_freq_start..high_freq_end]
            .iter()
            .sum::<f32>();
        features.push(high_freq_energy);

        // 4. Spectral centroid (center of mass of spectrum)
        let total_energy: f32 = magnitude_spectrum.iter().sum();
        let spectral_centroid = if total_energy > 0.0 {
            magnitude_spectrum
                .iter()
                .enumerate()
                .map(|(i, &mag)| i as f32 * mag)
                .sum::<f32>()
                / total_energy
        } else {
            0.0
        };
        features.push(spectral_centroid);

        // 5. Spectral rolloff (frequency below which 85% of energy lies)
        let rolloff_threshold = total_energy * 0.85;
        let mut cumulative_energy = 0.0;
        let mut rolloff_freq = 0.0;
        for (i, &mag) in magnitude_spectrum.iter().enumerate() {
            cumulative_energy += mag;
            if cumulative_energy >= rolloff_threshold {
                rolloff_freq = i as f32;
                break;
            }
        }
        features.push(rolloff_freq);

        // 6. Spectral flatness (measure of noise-like vs tonal content)
        let geometric_mean = if !magnitude_spectrum.is_empty() {
            let log_sum: f32 = magnitude_spectrum
                .iter()
                .filter(|&&x| x > 0.0)
                .map(|&x| x.ln())
                .sum();
            (log_sum / magnitude_spectrum.len() as f32).exp()
        } else {
            0.0
        };
        let arithmetic_mean =
            magnitude_spectrum.iter().sum::<f32>() / magnitude_spectrum.len() as f32;
        let spectral_flatness = if arithmetic_mean > 0.0 {
            geometric_mean / arithmetic_mean
        } else {
            0.0
        };
        features.push(spectral_flatness);

        // 7. Zero crossing rate (for temporal analysis)
        let zero_crossings = data
            .windows(2)
            .filter(|window| (window[0] as i16 - 128) * (window[1] as i16 - 128) < 0)
            .count();
        let zero_crossing_rate = zero_crossings as f32 / data.len() as f32;
        features.push(zero_crossing_rate);

        // 8. Peak frequency (dominant frequency component)
        let peak_index = magnitude_spectrum
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        features.push(peak_index as f32);

        // 9. Bandwidth (spread of frequencies around centroid)
        let bandwidth = if total_energy > 0.0 {
            magnitude_spectrum
                .iter()
                .enumerate()
                .map(|(i, &mag)| (i as f32 - spectral_centroid).powi(2) * mag)
                .sum::<f32>()
                / total_energy
        } else {
            0.0
        };
        features.push(bandwidth.sqrt());

        // 10. Harmonic-to-noise ratio estimate
        let harmonics_energy = magnitude_spectrum
            .iter()
            .step_by(peak_index.max(1))
            .take(5)
            .sum::<f32>();
        let hnr = if total_energy > harmonics_energy {
            harmonics_energy / (total_energy - harmonics_energy)
        } else {
            0.0
        };
        features.push(hnr);

        Ok(features)
    }

    /// Compute Discrete Fourier Transform (simple implementation)
    fn compute_dft(&self, signal: &[f32]) -> Vec<f32> {
        let n = signal.len();
        let mut result = vec![0.0; n * 2]; // Real and imaginary parts

        for k in 0..n {
            let mut real_sum = 0.0;
            let mut imag_sum = 0.0;

            for (j, &sample) in signal.iter().enumerate() {
                let angle = -2.0 * std::f32::consts::PI * (k * j) as f32 / n as f32;
                real_sum += sample * angle.cos();
                imag_sum += sample * angle.sin();
            }

            result[k * 2] = real_sum;
            result[k * 2 + 1] = imag_sum;
        }

        result
    }

    /// Calculate five carrier complexity measures, all normalised to [0, 1]:
    ///   0. Byte variance (normalised by max possible variance for 8-bit data)
    ///   1. Byte range   (max – min / 255)
    ///   2. Mean run length (fraction of bytes that are identical to their predecessor)
    ///   3. Byte uniqueness (fraction of the 256 possible byte values that appear)
    ///   4. Byte transition rate (fraction of adjacent byte pairs that differ)
    fn calculate_complexity_measures(&self, carrier: &CarrierData) -> Result<Vec<f32>> {
        let data = &carrier.raw_data;
        if data.is_empty() {
            return Ok(vec![0.0; 5]);
        }
        let n = data.len();

        // 0 — normalised byte variance
        let mean = data.iter().map(|&b| b as f32).sum::<f32>() / n as f32;
        let variance =
            data.iter().map(|&b| { let d = b as f32 - mean; d * d }).sum::<f32>() / n as f32;
        let norm_variance = variance / (127.5 * 127.5); // max variance ≈ 127.5²

        // 1 — byte range
        let min_b = *data.iter().min().unwrap() as f32;
        let max_b = *data.iter().max().unwrap() as f32;
        let byte_range = (max_b - min_b) / 255.0;

        // 2 — mean run length (fraction of bytes equal to their predecessor;
        //     0 = all bytes differ, 1 = one long run of identical bytes)
        let repeated = data.windows(2).filter(|w| w[0] == w[1]).count();
        let run_fraction = repeated as f32 / (n - 1) as f32;

        // 3 — byte uniqueness
        let mut seen = [false; 256];
        for &b in data { seen[b as usize] = true; }
        let uniqueness = seen.iter().filter(|&&v| v).count() as f32 / 256.0;

        // 4 — transition rate (fraction of adjacent pairs that differ)
        let transitions = data.windows(2).filter(|w| w[0] != w[1]).count();
        let transition_rate = transitions as f32 / (n - 1) as f32;

        Ok(vec![norm_variance, byte_range, run_fraction, uniqueness, transition_rate])
    }

    /// Extract format-specific features as a named map.
    ///
    /// FASTA/FASTQ → base composition (A, T, C, G fractions + GC content)
    /// WAV/BinarySignal → zero-crossing rate + normalised signal energy
    /// PNG/TIFF/MICROSCOPY/DICOM → pixel mean, std, dark ratio, bright ratio
    /// everything else → printable-character ratio
    fn extract_format_specific_features(
        &self,
        carrier: &CarrierData,
    ) -> Result<HashMap<String, f32>> {
        let mut features = HashMap::new();
        let data = &carrier.raw_data;
        if data.is_empty() {
            return Ok(features);
        }
        let total = data.len() as f32;

        match carrier.format {
            DataFormat::FASTA | DataFormat::FASTQ => {
                let a = data.iter().filter(|&&b| b == b'A' || b == b'a').count() as f32 / total;
                let t = data.iter().filter(|&&b| b == b'T' || b == b't').count() as f32 / total;
                let c = data.iter().filter(|&&b| b == b'C' || b == b'c').count() as f32 / total;
                let g = data.iter().filter(|&&b| b == b'G' || b == b'g').count() as f32 / total;
                features.insert("base_a".to_string(), a);
                features.insert("base_t".to_string(), t);
                features.insert("base_c".to_string(), c);
                features.insert("base_g".to_string(), g);
                features.insert("gc_content".to_string(), g + c);
            }
            DataFormat::WAV | DataFormat::BinarySignal => {
                // Treat each byte as an 8-bit unsigned sample; centre around 128
                let samples: Vec<f32> = data.iter().map(|&b| b as f32 - 128.0).collect();
                let n = samples.len();
                let zcr = samples
                    .windows(2)
                    .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                    .count() as f32
                    / (n - 1) as f32;
                let energy =
                    samples.iter().map(|&s| s * s).sum::<f32>() / n as f32 / (128.0 * 128.0);
                features.insert("zero_crossing_rate".to_string(), zcr);
                features.insert("signal_energy".to_string(), energy);
            }
            DataFormat::PNG
            | DataFormat::TIFF
            | DataFormat::MICROSCOPY
            | DataFormat::DICOM => {
                // Skip typical file header (8 bytes) and treat the rest as pixel bytes
                let pixel_start = 8.min(data.len());
                let pixels: Vec<f32> =
                    data[pixel_start..].iter().map(|&b| b as f32 / 255.0).collect();
                let pn = pixels.len() as f32;
                let mean = pixels.iter().sum::<f32>() / pn;
                let std = (pixels.iter().map(|&p| (p - mean).powi(2)).sum::<f32>() / pn).sqrt();
                let dark = pixels.iter().filter(|&&p| p < 0.25).count() as f32 / pn;
                let bright = pixels.iter().filter(|&&p| p > 0.75).count() as f32 / pn;
                features.insert("pixel_mean".to_string(), mean);
                features.insert("pixel_std".to_string(), std);
                features.insert("dark_ratio".to_string(), dark);
                features.insert("bright_ratio".to_string(), bright);
            }
            _ => {
                let printable =
                    data.iter().filter(|&&b| (32u8..127).contains(&b)).count() as f32 / total;
                features.insert("printable_ratio".to_string(), printable);
            }
        }

        Ok(features)
    }

    /// Predict performance metrics for given features and parameters based on historical data
    fn predict_performance(
        &self,
        features: &CarrierFeatures,
        params: &EmbedParams,
    ) -> Result<PerformanceMetrics> {
        if self.training_data.is_empty() {
            // Fallback to heuristic-based prediction
            return Ok(self.heuristic_performance_prediction(features, params));
        }

        // Find similar examples in training data
        let similar_examples = self.find_similar_examples(features, params, 10);

        if similar_examples.is_empty() {
            return Ok(self.heuristic_performance_prediction(features, params));
        }

        // Weight examples by similarity and recency
        let mut weighted_metrics = PerformanceMetrics {
            capacity_efficiency: 0.0,
            embedding_time_ms: 0,
            extraction_accuracy: 0.0,
            quality_preservation: 0.0,
            memory_usage: 0,
            gpu_acceleration_benefit: 0.0,
        };

        let mut total_weight = 0.0;
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (example, similarity) in &similar_examples {
            // Calculate time-based weight (more recent examples have higher weight)
            let age_weight = if current_time > example.timestamp {
                let age_hours = (current_time - example.timestamp) as f32 / 3600.0;
                (-age_hours / 168.0).exp() // Decay over one week
            } else {
                1.0
            };

            let combined_weight = similarity * age_weight;
            total_weight += combined_weight;

            // Accumulate weighted metrics
            weighted_metrics.capacity_efficiency +=
                example.performance.capacity_efficiency * combined_weight;
            weighted_metrics.embedding_time_ms +=
                (example.performance.embedding_time_ms as f32 * combined_weight) as u64;
            weighted_metrics.extraction_accuracy +=
                example.performance.extraction_accuracy * combined_weight;
            weighted_metrics.quality_preservation +=
                example.performance.quality_preservation * combined_weight;
            weighted_metrics.memory_usage +=
                (example.performance.memory_usage as f32 * combined_weight) as usize;
            weighted_metrics.gpu_acceleration_benefit +=
                example.performance.gpu_acceleration_benefit * combined_weight;
        }

        // Normalize by total weight
        if total_weight > 0.0 {
            weighted_metrics.capacity_efficiency /= total_weight;
            weighted_metrics.embedding_time_ms =
                (weighted_metrics.embedding_time_ms as f32 / total_weight) as u64;
            weighted_metrics.extraction_accuracy /= total_weight;
            weighted_metrics.quality_preservation /= total_weight;
            weighted_metrics.memory_usage =
                (weighted_metrics.memory_usage as f32 / total_weight) as usize;
            weighted_metrics.gpu_acceleration_benefit /= total_weight;
        }

        // Apply confidence-based adjustments
        let confidence = self.calculate_prediction_confidence(&similar_examples);
        let base_prediction = self.heuristic_performance_prediction(features, params);

        // Blend historical and heuristic predictions based on confidence
        Ok(PerformanceMetrics {
            capacity_efficiency: confidence * weighted_metrics.capacity_efficiency
                + (1.0 - confidence) * base_prediction.capacity_efficiency,
            embedding_time_ms: ((confidence * weighted_metrics.embedding_time_ms as f32
                + (1.0 - confidence) * base_prediction.embedding_time_ms as f32)
                as u64),
            extraction_accuracy: confidence * weighted_metrics.extraction_accuracy
                + (1.0 - confidence) * base_prediction.extraction_accuracy,
            quality_preservation: confidence * weighted_metrics.quality_preservation
                + (1.0 - confidence) * base_prediction.quality_preservation,
            memory_usage: ((confidence * weighted_metrics.memory_usage as f32
                + (1.0 - confidence) * base_prediction.memory_usage as f32)
                as usize),
            gpu_acceleration_benefit: confidence * weighted_metrics.gpu_acceleration_benefit
                + (1.0 - confidence) * base_prediction.gpu_acceleration_benefit,
        })
    }

    /// Find similar examples from training data
    fn find_similar_examples(
        &self,
        features: &CarrierFeatures,
        params: &EmbedParams,
        max_results: usize,
    ) -> Vec<(&TrainingExample, f32)> {
        let mut similarities = Vec::new();

        for example in &self.training_data {
            let feature_similarity =
                self.calculate_feature_similarity(features, &example.carrier_features);
            let param_similarity = self.calculate_param_similarity(params, &example.embed_params);

            // Combined similarity (weighted towards features)
            let combined_similarity = 0.7 * feature_similarity + 0.3 * param_similarity;

            similarities.push((example, combined_similarity));
        }

        // Sort by similarity (highest first) and take top results
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        similarities.truncate(max_results);

        // Filter out examples with very low similarity
        similarities
            .into_iter()
            .filter(|(_, similarity)| *similarity > 0.1)
            .collect()
    }

    /// Calculate similarity between carrier features
    fn calculate_feature_similarity(
        &self,
        features1: &CarrierFeatures,
        features2: &CarrierFeatures,
    ) -> f32 {
        // Format match is important
        let format_similarity = if features1.format == features2.format {
            1.0
        } else {
            0.3
        };

        // Size similarity (logarithmic scale)
        let size_ratio =
            features1.size.min(features2.size) as f32 / features1.size.max(features2.size) as f32;
        let size_similarity = size_ratio.ln().exp(); // Smooth similarity function

        // Entropy similarity
        let entropy_diff = (features1.entropy - features2.entropy).abs();
        let entropy_similarity = (-entropy_diff / 2.0).exp(); // Gaussian-like similarity

        // Frequency characteristics similarity
        let freq_similarity = if features1.frequency_characteristics.len()
            == features2.frequency_characteristics.len()
        {
            let mut sum_squared_diff = 0.0;
            for (f1, f2) in features1
                .frequency_characteristics
                .iter()
                .zip(&features2.frequency_characteristics)
            {
                sum_squared_diff += (f1 - f2).powi(2);
            }
            let mse = sum_squared_diff / features1.frequency_characteristics.len() as f32;
            (-mse.sqrt()).exp()
        } else {
            0.5 // Partial similarity if different lengths
        };

        // Complexity similarity
        let complexity_similarity =
            if features1.complexity_measures.len() == features2.complexity_measures.len() {
                let mut sum_squared_diff = 0.0;
                for (c1, c2) in features1
                    .complexity_measures
                    .iter()
                    .zip(&features2.complexity_measures)
                {
                    sum_squared_diff += (c1 - c2).powi(2);
                }
                let mse = sum_squared_diff / features1.complexity_measures.len() as f32;
                (-mse.sqrt()).exp()
            } else {
                0.5
            };

        // Weighted combination
        0.3 * format_similarity
            + 0.2 * size_similarity
            + 0.2 * entropy_similarity
            + 0.15 * freq_similarity
            + 0.15 * complexity_similarity
    }

    /// Calculate similarity between embedding parameters
    fn calculate_param_similarity(&self, params1: &EmbedParams, params2: &EmbedParams) -> f32 {
        let mut similarity_score = 0.0;
        let mut total_weight = 0.0;

        // Error correction level similarity
        let ec_similarity = if params1.error_correction_level == params2.error_correction_level {
            1.0
        } else {
            0.3 // Some similarity even if different
        };
        similarity_score += 0.3 * ec_similarity;
        total_weight += 0.3;

        // Stealth level similarity
        let stealth_similarity = if params1.stealth_level == params2.stealth_level {
            1.0
        } else {
            0.3
        };
        similarity_score += 0.3 * stealth_similarity;
        total_weight += 0.3;

        // GPU usage similarity
        let gpu_similarity = if params1.use_gpu == params2.use_gpu {
            1.0
        } else {
            0.5 // Moderate similarity even if different
        };
        similarity_score += 0.2 * gpu_similarity;
        total_weight += 0.2;

        // Encryption key similarity (simplified)
        let key_similarity =
            if params1.encryption_key.is_empty() == params2.encryption_key.is_empty() {
                1.0
            } else {
                0.2
            };
        similarity_score += 0.1 * key_similarity;
        total_weight += 0.1;

        // Format-specific parameters similarity
        let format_specific_similarity =
            if params1.format_specific.is_empty() && params2.format_specific.is_empty() {
                1.0
            } else {
                // Count matching keys
                let matching_keys = params1
                    .format_specific
                    .keys()
                    .filter(|k| params2.format_specific.contains_key(*k))
                    .count();
                let total_keys = params1
                    .format_specific
                    .len()
                    .max(params2.format_specific.len());
                if total_keys > 0 {
                    matching_keys as f32 / total_keys as f32
                } else {
                    1.0
                }
            };
        similarity_score += 0.1 * format_specific_similarity;
        total_weight += 0.1;

        similarity_score / total_weight
    }

    /// Calculate confidence in prediction based on similar examples
    fn calculate_prediction_confidence(&self, similar_examples: &[(&TrainingExample, f32)]) -> f32 {
        if similar_examples.is_empty() {
            return 0.0;
        }

        // Base confidence from number of examples
        let count_confidence = (similar_examples.len() as f32 / 10.0).min(1.0);

        // Average similarity confidence
        let avg_similarity = similar_examples
            .iter()
            .map(|(_, similarity)| *similarity)
            .sum::<f32>()
            / similar_examples.len() as f32;

        // Variance in performance (lower variance = higher confidence)
        let performance_values: Vec<f32> = similar_examples
            .iter()
            .map(|(example, _)| example.performance.capacity_efficiency)
            .collect();

        let mean_performance =
            performance_values.iter().sum::<f32>() / performance_values.len() as f32;
        let variance = performance_values
            .iter()
            .map(|&x| (x - mean_performance).powi(2))
            .sum::<f32>()
            / performance_values.len() as f32;

        let variance_confidence = (-variance * 10.0).exp(); // Lower variance = higher confidence

        // Combined confidence
        (count_confidence + avg_similarity + variance_confidence) / 3.0
    }

    /// Heuristic-based performance prediction when no historical data is available
    fn heuristic_performance_prediction(
        &self,
        features: &CarrierFeatures,
        params: &EmbedParams,
    ) -> PerformanceMetrics {
        // Base predictions based on format and size
        let base_capacity = match features.format {
            DataFormat::PNG | DataFormat::TIFF => 0.1, // Images have good capacity
            DataFormat::WAV => 0.05,                   // Audio has moderate capacity
            DataFormat::FASTQ | DataFormat::FASTA => 0.2, // Genomics data has excellent capacity
            _ => 0.03,                                 // Conservative estimate for other formats
        };

        // Adjust based on size (larger files generally have better capacity efficiency)
        let size_factor = (features.size as f32).log10() / 6.0; // Normalize to 0-1 range
        let capacity_efficiency = (base_capacity * (1.0 + size_factor)).min(1.0);

        // Estimate embedding time based on size and GPU usage
        let base_time_per_kb = if params.use_gpu { 0.1 } else { 0.5 }; // ms per KB
        let embedding_time_ms = ((features.size as f32 / 1024.0) * base_time_per_kb) as u64;

        // Accuracy typically high for well-established formats
        let extraction_accuracy = match features.format {
            DataFormat::PNG | DataFormat::WAV | DataFormat::FASTQ => 0.98,
            DataFormat::TIFF | DataFormat::FASTA => 0.95,
            _ => 0.9,
        };

        // Quality preservation depends on stealth level
        let quality_preservation = match params.stealth_level {
            StealthLevel::Maximum => 0.98,
            StealthLevel::High => 0.95,
            StealthLevel::Medium => 0.88,
            StealthLevel::Low => 0.8,
        };

        // Memory usage estimate
        let memory_usage = features.size * 2; // Rough estimate: 2x input size

        // GPU acceleration benefit
        let gpu_acceleration_benefit = if params.use_gpu {
            match features.size {
                size if size > 10_000_000 => 15.0, // Large files benefit most
                size if size > 1_000_000 => 8.0,   // Medium files
                _ => 3.0,                          // Small files have less benefit
            }
        } else {
            1.0
        };

        PerformanceMetrics {
            capacity_efficiency,
            embedding_time_ms,
            extraction_accuracy,
            quality_preservation,
            memory_usage,
            gpu_acceleration_benefit,
        }
    }

    /// Generate alternative embedding strategies with confidence scores
    fn generate_alternative_strategies(
        &self,
        features: &CarrierFeatures,
        primary_params: &EmbedParams,
    ) -> Result<Vec<(EmbedParams, f32)>> {
        let mut alternatives = Vec::new();

        // Generate parameter variations
        let parameter_variations = self.generate_parameter_variations(primary_params);

        for variant_params in parameter_variations {
            // Skip if identical to primary
            if self.params_equal(&variant_params, primary_params) {
                continue;
            }

            // Predict performance for this variant
            let predicted_performance = self.predict_performance(features, &variant_params)?;

            // Calculate confidence based on historical data and heuristics
            let confidence = self.calculate_variant_confidence(
                features,
                &variant_params,
                &predicted_performance,
            );

            // Only include alternatives with reasonable confidence
            if confidence > 0.2 {
                alternatives.push((variant_params, confidence));
            }
        }

        // Sort by confidence (highest first)
        alternatives.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Limit to top 5 alternatives
        alternatives.truncate(5);

        Ok(alternatives)
    }

    /// Generate parameter variations for alternative strategies
    fn generate_parameter_variations(&self, base_params: &EmbedParams) -> Vec<EmbedParams> {
        let mut variations = Vec::new();

        // Variation 1: Different error correction level
        let mut variant1 = base_params.clone();
        variant1.error_correction_level = match base_params.error_correction_level {
            ECLevel::Maximum => ECLevel::High,
            ECLevel::High => ECLevel::Medium,
            ECLevel::Medium => ECLevel::High,
            ECLevel::Low => ECLevel::Medium,
            ECLevel::None => ECLevel::Low,
        };
        variations.push(variant1);

        // Variation 2: Different stealth level
        let mut variant2 = base_params.clone();
        variant2.stealth_level = match base_params.stealth_level {
            StealthLevel::Maximum => StealthLevel::High,
            StealthLevel::High => StealthLevel::Medium,
            StealthLevel::Medium => StealthLevel::High,
            StealthLevel::Low => StealthLevel::Medium,
        };
        variations.push(variant2);

        // Variation 3: Toggle GPU usage
        let mut variant3 = base_params.clone();
        variant3.use_gpu = !base_params.use_gpu;
        variations.push(variant3);

        // Variation 4: High performance variant (low stealth, high error correction)
        let mut variant4 = base_params.clone();
        variant4.error_correction_level = ECLevel::High;
        variant4.stealth_level = StealthLevel::Low;
        variant4.use_gpu = true;
        variations.push(variant4);

        // Variation 5: High stealth variant (high stealth, medium error correction)
        let mut variant5 = base_params.clone();
        variant5.error_correction_level = ECLevel::Medium;
        variant5.stealth_level = StealthLevel::High;
        variations.push(variant5);

        // Variation 6: Conservative variant (low error correction, medium stealth)
        let mut variant6 = base_params.clone();
        variant6.error_correction_level = ECLevel::Low;
        variant6.stealth_level = StealthLevel::Medium;
        variant6.use_gpu = false;
        variations.push(variant6);

        // Variation 7: Speed-optimized variant
        let mut variant7 = base_params.clone();
        variant7.error_correction_level = ECLevel::Low;
        variant7.stealth_level = StealthLevel::Low;
        variant7.use_gpu = true;
        variations.push(variant7);

        variations
    }

    /// Calculate confidence for a parameter variant
    fn calculate_variant_confidence(
        &self,
        features: &CarrierFeatures,
        params: &EmbedParams,
        predicted_performance: &PerformanceMetrics,
    ) -> f32 {
        // Base confidence from historical data
        let historical_confidence = if !self.training_data.is_empty() {
            let similar_examples = self.find_similar_examples(features, params, 5);
            if !similar_examples.is_empty() {
                let avg_similarity = similar_examples
                    .iter()
                    .map(|(_, similarity)| *similarity)
                    .sum::<f32>()
                    / similar_examples.len() as f32;
                avg_similarity
            } else {
                0.3 // Low confidence without similar examples
            }
        } else {
            0.4 // Moderate confidence based on heuristics alone
        };

        // Confidence from predicted performance quality
        let performance_confidence = {
            let mut score = 0.0;

            // High capacity efficiency increases confidence
            score += predicted_performance.capacity_efficiency * 0.3;

            // High extraction accuracy increases confidence
            score += predicted_performance.extraction_accuracy * 0.3;

            // High quality preservation increases confidence
            score += predicted_performance.quality_preservation * 0.2;

            // Reasonable embedding time increases confidence
            let time_score = if predicted_performance.embedding_time_ms < 1000 {
                1.0
            } else if predicted_performance.embedding_time_ms < 5000 {
                0.7
            } else {
                0.3
            };
            score += time_score * 0.2;

            score
        };

        // Confidence from parameter compatibility with carrier format
        let format_confidence = match (features.format.clone(), params) {
            (DataFormat::PNG | DataFormat::TIFF, p)
                if matches!(p.stealth_level, StealthLevel::High) =>
            {
                0.9
            }
            (DataFormat::WAV, p) if p.use_gpu => 0.8,
            (DataFormat::FASTQ | DataFormat::FASTA, p)
                if matches!(p.error_correction_level, ECLevel::High) =>
            {
                0.9
            }
            _ => 0.6,
        };

        // Combined confidence
        let combined = (historical_confidence + performance_confidence + format_confidence) / 3.0;
        combined.clamp(0.0, 1.0)
    }

    /// Check if two parameter sets are equal
    fn params_equal(&self, params1: &EmbedParams, params2: &EmbedParams) -> bool {
        params1.error_correction_level == params2.error_correction_level
            && params1.stealth_level == params2.stealth_level
            && params1.use_gpu == params2.use_gpu
            && params1.encryption_key == params2.encryption_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_ml_optimizer_creation() {
        let optimizer = MLOptimizer::new();
        assert_eq!(optimizer.models.len(), 0);
        assert_eq!(optimizer.training_data.len(), 0);
        assert!(optimizer.config.adaptive_strategy);
    }

    #[test]
    fn test_entropy_calculation() {
        let optimizer = MLOptimizer::new();

        // Test with uniform data (maximum entropy)
        let uniform_data: Vec<u8> = (0..=255).collect();
        let entropy = optimizer.calculate_entropy(&uniform_data);
        assert!(entropy > 7.9); // Should be close to 8.0 for uniform distribution

        // Test with repetitive data (low entropy)
        let repetitive_data = vec![0u8; 1000];
        let entropy = optimizer.calculate_entropy(&repetitive_data);
        assert!(entropy < 0.1); // Should be close to 0.0 for repetitive data
    }

    #[test]
    fn test_feature_extraction() {
        let optimizer = MLOptimizer::new();
        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0, 1, 2, 3, 4, 5],
            metadata: HashMap::new(),
            dimensions: Some(crate::core::Dimensions::new_2d(100, 100)),
            sampling_rate: None,
        };

        let features = optimizer.extract_features(&carrier).unwrap();
        assert_eq!(features.format, DataFormat::PNG);
        assert_eq!(features.size, 6);
        assert!(features.entropy > 0.0);
    }

    #[tokio::test]
    async fn test_strategy_recommendation_without_model() {
        let optimizer = MLOptimizer::new();
        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0, 1, 2, 3, 4, 5],
            metadata: HashMap::new(),
            dimensions: Some(crate::core::Dimensions::new_2d(100, 100)),
            sampling_rate: None,
        };
        let payload = b"test payload";

        let recommendation = optimizer
            .recommend_strategy(&carrier, payload)
            .await
            .unwrap();
        assert!(recommendation.confidence < 0.5); // Should have low confidence without model
        assert!(recommendation.reasoning.contains("default parameters"));
    }
}
