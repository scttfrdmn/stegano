//! Capacity estimation for different data formats

/// Utility for estimating steganographic capacity of carrier data.
///
/// Provides conservative upper-bound estimates for different embedding
/// strategies (LSB, frequency-domain) before a full embed is attempted.
pub struct CapacityEstimator;

impl CapacityEstimator {
    /// Create a new `CapacityEstimator`.
    pub fn new() -> Self {
        Self
    }

    /// Estimate capacity for LSB embedding in binary data
    pub fn estimate_lsb_capacity(&self, carrier_size: usize) -> usize {
        // LSB embedding can use 1 bit per byte, minus header space
        carrier_size.saturating_sub(32)
    }

    /// Estimate capacity for frequency domain embedding
    pub fn estimate_frequency_capacity(&self, signal_length: usize) -> usize {
        // Conservative estimate: use middle frequency bins
        if signal_length <= 64 {
            0
        } else {
            signal_length / 4
        }
    }
}

impl Default for CapacityEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_estimator_creation() {
        let estimator = CapacityEstimator::new();
        assert_eq!(estimator.estimate_lsb_capacity(0), 0);
    }

    #[test]
    fn test_capacity_estimator_default() {
        let estimator = CapacityEstimator;
        assert_eq!(estimator.estimate_lsb_capacity(100), 68);
    }

    #[test]
    fn test_lsb_capacity_estimation() {
        let estimator = CapacityEstimator::new();

        // Small carriers should have 0 capacity
        assert_eq!(estimator.estimate_lsb_capacity(10), 0);
        assert_eq!(estimator.estimate_lsb_capacity(32), 0);

        // Larger carriers minus header
        assert_eq!(estimator.estimate_lsb_capacity(100), 68);
        assert_eq!(estimator.estimate_lsb_capacity(1024), 992);
    }

    #[test]
    fn test_frequency_capacity_estimation() {
        let estimator = CapacityEstimator::new();

        // Small signals should have 0 capacity
        assert_eq!(estimator.estimate_frequency_capacity(32), 0);
        assert_eq!(estimator.estimate_frequency_capacity(64), 0);

        // Larger signals use 1/4 of frequency bins
        assert_eq!(estimator.estimate_frequency_capacity(256), 64);
        assert_eq!(estimator.estimate_frequency_capacity(1024), 256);
    }

    #[test]
    fn test_capacity_comparison() {
        let estimator = CapacityEstimator::new();
        let data_size = 1024;

        let lsb_capacity = estimator.estimate_lsb_capacity(data_size);
        let freq_capacity = estimator.estimate_frequency_capacity(data_size);

        // LSB typically has higher capacity than frequency domain
        assert!(lsb_capacity > freq_capacity);
    }
}
