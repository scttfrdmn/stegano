//! High-level embedding API with integrated statistical validation
//!
//! This module provides the main embedding interface that combines format handlers,
//! cryptographic protection, and statistical validation into a unified API.

use crate::core::*;
use crate::handlers::HandlerRegistry;
use crate::validation::{StatisticalValidationSuite, ValidationReport};
use crate::time::Instant;

/// Segment length (bytes) for per-segment ROC analysis in validation.
const ROC_SEGMENT_LEN: usize = 256;
/// Sampling rate assumed for signal carriers that do not specify one.
const DEFAULT_SAMPLE_RATE_HZ: f64 = 44_100.0;

/// High-level steganographic embedding service with integrated validation
pub struct SteganoEmbedder {
    handler_registry: HandlerRegistry,
    validation_suite: StatisticalValidationSuite,
}

impl SteganoEmbedder {
    /// Create new embedder with default configuration
    pub fn new() -> Self {
        Self {
            handler_registry: HandlerRegistry::new(),
            validation_suite: StatisticalValidationSuite::new(),
        }
    }

    /// Embed payload with optional validation
    pub async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start_time = Instant::now();

        // Get appropriate handler
        let handler = self
            .handler_registry
            .get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;

        // Store original data for validation if enabled
        let original_data = if params.enable_validation {
            Some(carrier.raw_data.clone())
        } else {
            None
        };

        // Perform embedding
        let mut result = handler.embed(carrier, payload, params).await?;

        // Run statistical validation if enabled
        if params.enable_validation && result.success {
            if let Some(original) = original_data {
                let validation_report = self.run_validation(&original, carrier).await?;

                result.validation_report = Some(validation_report);
            }
        }

        // Update performance stats
        result.performance_stats.embedding_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(result)
    }

    /// Extract payload from carrier
    pub async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let handler = self
            .handler_registry
            .get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;

        handler.extract(carrier, params).await
    }

    /// Run comprehensive statistical validation.
    ///
    /// Report keys and their meaning:
    /// * `pairs_of_values_test` — Westfeld–Pfitzmann PoV χ² attack on the
    ///   modified data's histogram. High `embedding_probability` ⇒ pair-equalised
    ///   histogram consistent with LSB replacement.
    /// * `ks_test` — two-sample Kolmogorov–Smirnov test between original and
    ///   modified byte distributions (small p ⇒ distributions differ).
    /// * `pixel_pair_analysis` (images) — Sample Pairs Analysis estimate of the
    ///   embedding rate, using `carrier.dimensions` when present.
    /// * `audio_steganalysis` (signals) — spectral features at
    ///   `carrier.sampling_rate`.
    /// * `roc_analysis` — SPA scores computed on the *modified data alone*, per
    ///   segment, with the ground-truth label "segment was modified". Present only
    ///   when both modified and unmodified segments exist.
    ///
    /// The PoV and SPA results are also stored in `steganalysis_results`, and
    /// `finalize()` derives `overall_score` from them (see
    /// [`crate::validation::detection_resistance_score`]).
    async fn run_validation(
        &self,
        original_data: &[u8],
        carrier: &CarrierData,
    ) -> Result<ValidationReport> {
        let modified_data = &carrier.raw_data[..];
        let mut report = ValidationReport::new();
        let steganalysis = self.validation_suite.advanced_steganalysis();

        // Westfeld–Pfitzmann pairs-of-values attack (replaces the old χ² test
        // against a uniform byte distribution, which no cover follows).
        if let Ok(pov) = steganalysis.pairs_of_values_test(modified_data) {
            report.add_test_result(
                "pairs_of_values_test".to_string(),
                serde_json::to_value(&pov).map_err(SteganoError::SerializationError)?,
            );
            report.steganalysis_results.pairs_of_values = Some(pov);
        }

        // KS test for distribution comparison
        let ks_result = self
            .validation_suite
            .classical_tests()
            .kolmogorov_smirnov_test(
                &self.bytes_to_floats(original_data),
                &self.bytes_to_floats(modified_data),
            )
            .map_err(|e| SteganoError::IntegrityError(e.to_string()))?;

        report.add_test_result(
            "ks_test".to_string(),
            serde_json::to_value(ks_result).map_err(SteganoError::SerializationError)?,
        );

        // Format-specific validation
        match carrier.format {
            DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY => {
                // Use the carrier geometry when known; otherwise analyse the bytes
                // as a single row (1-D adjacent pairs).
                let (width, height) = match &carrier.dimensions {
                    Some(d) if d.width > 0 && d.height > 0 => (d.width as usize, d.height as usize),
                    _ => (modified_data.len().max(1), 1),
                };
                if let Ok(pixel_analysis) =
                    steganalysis.pixel_pair_analysis(modified_data, width, height)
                {
                    report.add_test_result(
                        "pixel_pair_analysis".to_string(),
                        serde_json::to_value(&pixel_analysis)
                            .map_err(SteganoError::SerializationError)?,
                    );
                    report.steganalysis_results.pixel_pair_analysis = Some(pixel_analysis);
                }
            }
            DataFormat::WAV | DataFormat::BinarySignal => {
                // Skip 44-byte RIFF/WAV header if present; otherwise treat all bytes as samples
                let sample_start = if modified_data.starts_with(b"RIFF") && modified_data.len() > 44
                {
                    44
                } else {
                    0
                };
                // Fall back to 44.1 kHz only when the carrier has no sampling rate.
                let sample_rate = carrier
                    .sampling_rate
                    .filter(|r| r.is_finite() && *r > 0.0)
                    .unwrap_or(DEFAULT_SAMPLE_RATE_HZ) as f32;
                if modified_data.len() > sample_start {
                    let audio_data: Vec<f32> = modified_data[sample_start..]
                        .iter()
                        .map(|&b| (b as f32 - 128.0) / 128.0)
                        .collect();
                    if let Ok(audio_analysis) =
                        steganalysis.audio_steganalysis(&audio_data, sample_rate)
                    {
                        report.add_test_result(
                            "audio_steganalysis".to_string(),
                            serde_json::to_value(&audio_analysis)
                                .map_err(SteganoError::SerializationError)?,
                        );
                        report.steganalysis_results.audio_analysis = Some(audio_analysis);
                    }
                }
            }
            _ => {
                // Generic analysis for other formats
            }
        }

        // ROC: detector score from the modified data only; the original is used
        // solely for ground-truth labels.
        if let Some((true_labels, predicted_scores)) =
            self.segment_detection_samples(original_data, modified_data)
        {
            let roc_analysis = self
                .validation_suite
                .performance_analysis()
                .roc_analysis(&true_labels, &predicted_scores)
                .map_err(|e| SteganoError::IntegrityError(e.to_string()))?;

            report.add_test_result(
                "roc_analysis".to_string(),
                serde_json::to_value(roc_analysis).map_err(SteganoError::SerializationError)?,
            );
        }

        report.finalize();
        Ok(report)
    }

    /// Convert bytes to float values for KS test
    fn bytes_to_floats(&self, data: &[u8]) -> Vec<f64> {
        data.iter().map(|&b| b as f64 / 255.0).collect()
    }

    /// Per-segment detection samples for ROC analysis.
    ///
    /// The modified data is split into `ROC_SEGMENT_LEN`-byte segments. Each
    /// segment's score is its Sample Pairs Analysis rate estimate computed from
    /// the modified bytes alone; its label is whether any byte in the segment
    /// differs from the original. Returns `None` unless there are at least two
    /// segments of each class.
    fn segment_detection_samples(
        &self,
        original: &[u8],
        modified: &[u8],
    ) -> Option<(Vec<bool>, Vec<f64>)> {
        let n = original.len().min(modified.len());
        let steganalysis = self.validation_suite.advanced_steganalysis();
        let mut labels = Vec::new();
        let mut scores = Vec::new();
        let (orig_chunks, _) = original[..n].as_chunks::<ROC_SEGMENT_LEN>();
        let (mod_chunks, _) = modified[..n].as_chunks::<ROC_SEGMENT_LEN>();
        for (orig, modi) in orig_chunks.iter().zip(mod_chunks) {
            let spa = steganalysis
                .sample_pairs_analysis(modi, modi.len(), 1, 1)
                .ok()?;
            labels.push(orig != modi);
            scores.push(spa.raw_estimate);
        }
        let positives = labels.iter().filter(|&&l| l).count();
        let negatives = labels.len() - positives;
        (positives >= 2 && negatives >= 2).then_some((labels, scores))
    }

    /// Estimate capacity for given carrier
    pub fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        let handler = self
            .handler_registry
            .get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;

        handler.estimate_capacity(carrier)
    }

    /// Verify data integrity
    pub fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        let handler = self
            .handler_registry
            .get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;

        handler.verify_integrity(carrier)
    }
}

impl Default for SteganoEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_with_validation() {
        let embedder = SteganoEmbedder::new();

        // Create test carrier — large enough to hold encrypted + EC-encoded payload
        // EmbedParams::new defaults to ECLevel::Medium which adds RS overhead
        let test_data = vec![0u8; 8192];
        let mut carrier = CarrierData::new(DataFormat::BINARY, test_data);

        // Setup parameters with validation enabled
        let params = EmbedParams::new(b"test_key".to_vec()).with_validation();

        // Test payload
        let payload = b"Test secret data";

        // Embed with validation
        let result = embedder
            .embed(&mut carrier, payload, &params)
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.validation_report.is_some());

        // Validate that report contains expected tests
        let report = result.validation_report.unwrap();
        assert!(report.test_results().contains_key("pairs_of_values_test"));
        assert!(report.test_results().contains_key("ks_test"));
    }

    #[tokio::test]
    async fn test_embedding_without_validation() {
        let embedder = SteganoEmbedder::new();

        // Large enough to hold encrypted + EC-encoded payload
        let test_data = vec![0u8; 8192];
        let mut carrier = CarrierData::new(DataFormat::BINARY, test_data);

        // Parameters without validation
        let params = EmbedParams::new(b"test_key".to_vec());
        let payload = b"Test secret data";

        let result = embedder
            .embed(&mut carrier, payload, &params)
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.validation_report.is_none());
    }

    #[test]
    fn test_capacity_estimation() {
        let embedder = SteganoEmbedder::new();
        let carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 1024]);

        let capacity = embedder.estimate_capacity(&carrier).unwrap();
        assert!(capacity.total_bits > 0);
        assert!(capacity.recommended_bits <= capacity.total_bits);
    }

    /// Smooth synthetic image with mild noise (row-major, 8-bit grey).
    fn smooth_image(width: usize, height: usize) -> Vec<u8> {
        let mut state = 0x1234_5678u32;
        let mut out = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = (state % 5) as f64 - 2.0;
                let v =
                    90.0 + 0.4 * x as f64 + 0.3 * y as f64 + 30.0 * (x as f64 / 13.0).sin() + noise;
                out.push(v.round().clamp(0.0, 255.0) as u8);
            }
        }
        out
    }

    #[tokio::test]
    async fn test_validation_uses_carrier_dimensions_and_detector_roc() {
        let embedder = SteganoEmbedder::new();
        let (w, h) = (64usize, 64usize);
        let original = smooth_image(w, h);
        // Fully LSB-embed the first half of the image with pseudo-random bits.
        let mut modified = original.clone();
        let mut state = 0xdead_beefu32;
        for b in modified.iter_mut().take(original.len() / 2) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *b = (*b & !1) | (state & 1) as u8;
        }
        let mut carrier = CarrierData::new(DataFormat::PNG, modified);
        carrier.dimensions = Some(Dimensions::new_2d(w as u32, h as u32));

        let report = embedder.run_validation(&original, &carrier).await.unwrap();
        let results = report.test_results();

        let pp = &results["pixel_pair_analysis"];
        let rate = pp["embedding_probability"].as_f64().unwrap();
        assert!(
            rate > 0.3 && rate < 0.75,
            "SPA rate for 50% embedding: {rate}"
        );

        // ROC is scored from the modified data alone; labels come from the original.
        let auc = results["roc_analysis"]["auc"].as_f64().unwrap();
        assert!(auc > 0.9, "segment ROC AUC should be high, got {auc}");
        assert!(results.contains_key("pairs_of_values_test"));
    }

    #[tokio::test]
    async fn test_validation_uses_carrier_sampling_rate() {
        let embedder = SteganoEmbedder::new();
        let data: Vec<u8> = (0..16_000)
            .map(|i| (128.0 + 100.0 * (i as f64 * 0.05).sin()) as u8)
            .collect();
        let mut carrier = CarrierData::new(DataFormat::BinarySignal, data.clone());
        carrier.sampling_rate = Some(8_000.0);
        let report = embedder.run_validation(&data, &carrier).await.unwrap();
        // With 16 000 samples, analysis at 8 kHz must succeed; it would be skipped
        // at the old hard-coded 44.1 kHz if the analyser needs ≥ 1 s of signal.
        assert!(report.test_results().contains_key("audio_steganalysis"));
        // Identical data: no modified segments, so no ROC.
        assert!(!report.test_results().contains_key("roc_analysis"));
    }
}
