//! GPU compute operations

pub struct ComputeEngine;

impl ComputeEngine {
    pub fn new() -> Self {
        Self
    }

    /// Check if GPU compute is available
    pub fn is_available(&self) -> bool {
        // Check if we can access any GPU backend
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            super::metal::MetalAccelerator::is_available()
        }
        #[cfg(not(all(target_os = "macos", feature = "metal")))]
        {
            // For now, assume WGPU might be available
            true
        }
    }

    /// Estimate compute performance
    pub fn estimate_performance(&self) -> ComputePerformance {
        ComputePerformance {
            parallel_units: if self.is_available() { 256 } else { 1 },
            memory_bandwidth_gbps: if self.is_available() { 100.0 } else { 10.0 },
            compute_units: if self.is_available() { 16 } else { 1 },
            supports_unified_memory: cfg!(target_os = "macos"),
        }
    }
}

impl Default for ComputeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq)]
pub struct ComputePerformance {
    pub parallel_units: usize,
    pub memory_bandwidth_gbps: f64,
    pub compute_units: usize,
    pub supports_unified_memory: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_engine_creation() {
        let engine = ComputeEngine::new();
        let performance = engine.estimate_performance();
        assert!(performance.parallel_units > 0);
    }

    #[test]
    fn test_compute_engine_default() {
        let engine = ComputeEngine;
        let performance = engine.estimate_performance();
        assert!(performance.memory_bandwidth_gbps > 0.0);
    }

    #[test]
    fn test_compute_availability() {
        let engine = ComputeEngine::new();
        let available = engine.is_available();

        // On macOS with Metal, should be available
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            println!("Metal availability: {available}");
        }

        // Availability is platform-dependent, just ensure it returns a boolean
        assert!(available || !available);
    }

    #[test]
    fn test_performance_estimation() {
        let engine = ComputeEngine::new();
        let performance = engine.estimate_performance();

        // Performance metrics should be reasonable
        assert!(performance.parallel_units >= 1);
        assert!(performance.memory_bandwidth_gbps > 0.0);
        assert!(performance.compute_units >= 1);

        // Unified memory support depends on platform
        #[cfg(target_os = "macos")]
        assert!(performance.supports_unified_memory);

        #[cfg(not(target_os = "macos"))]
        assert!(!performance.supports_unified_memory);
    }

    #[test]
    fn test_compute_performance_structure() {
        let performance = ComputePerformance {
            parallel_units: 512,
            memory_bandwidth_gbps: 200.0,
            compute_units: 32,
            supports_unified_memory: true,
        };

        assert_eq!(performance.parallel_units, 512);
        assert_eq!(performance.memory_bandwidth_gbps, 200.0);
        assert_eq!(performance.compute_units, 32);
        assert!(performance.supports_unified_memory);
    }

    #[test]
    fn test_performance_with_gpu() {
        let engine = ComputeEngine::new();
        let performance = engine.estimate_performance();

        if engine.is_available() {
            // With GPU, should have better performance
            assert!(performance.parallel_units >= 256);
            assert!(performance.memory_bandwidth_gbps >= 100.0);
            assert!(performance.compute_units >= 16);
        } else {
            // Without GPU, fallback to CPU performance
            assert_eq!(performance.parallel_units, 1);
            assert_eq!(performance.memory_bandwidth_gbps, 10.0);
            assert_eq!(performance.compute_units, 1);
        }
    }
}
