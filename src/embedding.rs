//! High-level embedding API with integrated statistical validation
//!
//! This module provides the main embedding interface that combines format handlers,
//! cryptographic protection, and statistical validation into a unified API.

use crate::core::*;
use crate::handlers::{DataHandler, HandlerRegistry};
use crate::validation::{StatisticalValidationSuite, ValidationReport};
use std::time::Instant;

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
                let validation_report = self
                    .run_validation(&original, &carrier.raw_data, &carrier.format)
                    .await?;

                result.validation_report = Some(validation_report);
            }
        }

        // Update performance stats
        result.performance_stats.embedding_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(result)
    }

    /// Extract payload from carrier
    pub async fn extract(
        &self,
        carrier: &CarrierData,
        params: &EmbedParams,
    ) -> Result<Vec<u8>> {
        let handler = self
            .handler_registry
            .get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;

        handler.extract(carrier, params).await
    }

    /// Run comprehensive statistical validation
    async fn run_validation(
        &self,
        original_data: &[u8],
        modified_data: &[u8],
        format: &DataFormat,
    ) -> Result<ValidationReport> {
        let mut report = ValidationReport::new();

        // Classical statistical tests
        let chi_square_result = self
            .validation_suite
            .classical_tests()
            .chi_square_test(
                &self.bytes_to_frequencies(original_data),
                &self.bytes_to_frequencies(modified_data),
            )
            .map_err(|e| SteganoError::IntegrityError(e.to_string()))?;

        report.add_test_result("chi_square_test", serde_json::to_value(chi_square_result)
            .map_err(|e| SteganoError::SerializationError(e))?);

        // KS test for distribution comparison
        let ks_result = self
            .validation_suite
            .classical_tests()
            .kolmogorov_smirnov_test(
                &self.bytes_to_floats(original_data),
                &self.bytes_to_floats(modified_data),
            )
            .map_err(|e| SteganoError::IntegrityError(e.to_string()))?;

        report.add_test_result("ks_test", serde_json::to_value(ks_result)
            .map_err(|e| SteganoError::SerializationError(e))?);

        // Format-specific validation
        match format {
            DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY => {
                // Pixel pair analysis for images
                if let Ok(pixel_analysis) = self
                    .validation_suite
                    .advanced_steganalysis()
                    .pixel_pair_analysis(modified_data)
                {
                    report.add_test_result("pixel_pair_analysis", 
                        serde_json::to_value(pixel_analysis)
                            .map_err(|e| SteganoError::SerializationError(e))?);
                }
            }
            DataFormat::WAV | DataFormat::BinarySignal => {
                // Audio-specific validation
                if let Ok(audio_analysis) = self
                    .validation_suite
                    .domain_specific()
                    .validate_audio_steganalysis(modified_data, 44100.0)
                {
                    report.add_test_result("audio_steganalysis",
                        serde_json::to_value(audio_analysis)
                            .map_err(|e| SteganoError::SerializationError(e))?);
                }
            }
            _ => {
                // Generic analysis for other formats
            }
        }

        // ROC analysis if we have sufficient data
        if original_data.len() > 1000 && modified_data.len() > 1000 {
            let roc_analysis = self
                .validation_suite
                .performance_analysis()
                .roc_analysis(&self.create_binary_labels(original_data, modified_data))
                .map_err(|e| SteganoError::IntegrityError(e.to_string()))?;

            report.add_test_result("roc_analysis",
                serde_json::to_value(roc_analysis)
                    .map_err(|e| SteganoError::SerializationError(e))?);
        }

        report.finalize();
        Ok(report)
    }

    /// Convert bytes to frequency distribution for chi-square test
    fn bytes_to_frequencies(&self, data: &[u8]) -> Vec<f64> {
        let mut frequencies = vec![0.0; 256];
        for &byte in data {
            frequencies[byte as usize] += 1.0;
        }
        frequencies
    }

    /// Convert bytes to float values for KS test
    fn bytes_to_floats(&self, data: &[u8]) -> Vec<f64> {
        data.iter().map(|&b| b as f64 / 255.0).collect()
    }

    /// Create binary labels for ROC analysis (simple difference detection)
    fn create_binary_labels(&self, original: &[u8], modified: &[u8]) -> Vec<(f64, bool)> {
        original
            .iter()
            .zip(modified.iter())
            .take(1000) // Limit for performance
            .map(|(&o, &m)| {
                let score = (o as f64 - m as f64).abs() / 255.0;
                let label = o != m;
                (score, label)
            })
            .collect()
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
        
        // Create test carrier
        let test_data = vec![0u8; 1024];
        let mut carrier = CarrierData::new(DataFormat::BINARY, test_data);
        
        // Setup parameters with validation enabled
        let params = EmbedParams::new(b"test_key".to_vec()).with_validation();
        
        // Test payload
        let payload = b"Test secret data";
        
        // Embed with validation
        let result = embedder.embed(&mut carrier, payload, &params).await.unwrap();
        
        assert!(result.success);
        assert!(result.validation_report.is_some());
        
        // Validate that report contains expected tests
        let report = result.validation_report.unwrap();
        assert!(report.test_results().contains_key("chi_square_test"));
        assert!(report.test_results().contains_key("ks_test"));
    }

    #[tokio::test]
    async fn test_embedding_without_validation() {
        let embedder = SteganoEmbedder::new();
        
        let test_data = vec![0u8; 1024];
        let mut carrier = CarrierData::new(DataFormat::BINARY, test_data);
        
        // Parameters without validation
        let params = EmbedParams::new(b"test_key".to_vec());
        let payload = b"Test secret data";
        
        let result = embedder.embed(&mut carrier, payload, &params).await.unwrap();
        
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
}