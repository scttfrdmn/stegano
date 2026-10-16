//! GPU acceleration for high-performance steganography operations
//!
//! This module provides GPU acceleration capabilities for stegano, with automatic
//! backend selection and CPU fallbacks for optimal performance across platforms.

pub mod compute;
pub mod cuda;
pub mod foundation;
pub mod kernels;
pub mod metal;

use crate::core::*;
pub use foundation::{BenchmarkResult, GpuFoundation, PerformanceProfile};

/// Unified GPU acceleration interface
#[derive(Debug)]
pub struct GpuAccelerator {
    foundation: GpuFoundation,
}

impl GpuAccelerator {
    /// Create a new GPU accelerator with automatic backend detection
    pub fn new() -> Result<Self> {
        let foundation = GpuFoundation::new()?;

        log::info!(
            "GPU Accelerator initialized with {} backend",
            foundation.backend_info
        );

        Ok(Self { foundation })
    }

    /// Get performance information
    pub fn performance_profile(&self) -> &PerformanceProfile {
        &self.foundation.performance_profile
    }

    /// Check if GPU acceleration is available
    pub fn is_gpu_available(&self) -> bool {
        self.foundation.gpu_available
    }

    /// Get backend information
    pub fn backend_info(&self) -> &str {
        &self.foundation.backend_info
    }

    /// High-performance parallel LSB embedding
    pub fn parallel_lsb_embed(&self, data: &mut [u8], payload_bits: &[u8]) -> Result<usize> {
        self.foundation.parallel_lsb_embed(data, payload_bits)
    }

    /// High-performance parallel LSB extraction  
    pub fn parallel_lsb_extract(&self, data: &[u8], payload_size: usize) -> Result<Vec<u8>> {
        self.foundation.parallel_lsb_extract(data, payload_size)
    }

    /// Benchmark embedding performance
    pub fn benchmark_performance(&self, data_size: usize) -> BenchmarkResult {
        self.foundation.benchmark_embedding(data_size)
    }

    /// Get optimal processing chunk size
    pub fn optimal_chunk_size(&self) -> usize {
        self.foundation.optimal_chunk_size()
    }

    /// Estimate processing time for given data size
    pub fn estimate_processing_time(&self, data_size: usize) -> std::time::Duration {
        self.foundation.estimate_processing_time(data_size)
    }
}

impl Default for GpuAccelerator {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            foundation: GpuFoundation::default(),
        })
    }
}

/// GPU backend selection
#[derive(Debug, Clone, Copy)]
pub enum GpuBackend {
    #[cfg(all(target_os = "macos", feature = "metal"))]
    Metal,
    #[cfg(feature = "cuda")]
    Cuda,
    #[cfg(feature = "gpu")]
    Wgpu,
    Cpu,
}

impl GpuBackend {
    /// Get the best available backend for this system.
    ///
    /// Priority order: Metal (macOS) > CUDA > WGPU > CPU.
    pub fn best_available() -> Self {
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if metal::MetalAccelerator::is_available() {
                return Self::Metal;
            }
        }

        #[cfg(feature = "cuda")]
        {
            if cuda::CudaAccelerator::is_available() {
                return Self::Cuda;
            }
        }

        #[cfg(feature = "gpu")]
        {
            Self::Wgpu
        }

        #[cfg(not(feature = "gpu"))]
        Self::Cpu
    }

    /// Get backend name
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            Self::Metal => "Metal",
            #[cfg(feature = "cuda")]
            Self::Cuda => "CUDA",
            #[cfg(feature = "gpu")]
            Self::Wgpu => "WGPU",
            Self::Cpu => "CPU",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_accelerator_creation() {
        let accelerator = GpuAccelerator::new();
        assert!(accelerator.is_ok());

        let accelerator = accelerator.unwrap();
        assert!(!accelerator.backend_info().is_empty());
        assert!(accelerator.performance_profile().parallel_units > 0);
    }

    #[test]
    fn test_gpu_backend_selection() {
        let best_backend = GpuBackend::best_available();
        println!("Best available backend: {}", best_backend.name());

        // Should always have at least CPU backend
        match best_backend {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuBackend::Metal => {}
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => {}
            #[cfg(feature = "gpu")]
            GpuBackend::Wgpu => {}
            GpuBackend::Cpu => {}
        }
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn test_accelerator_embedding() {
        let accelerator = GpuAccelerator::default();
        let mut data = vec![0u8; 1000];
        let payload = vec![1u8; 100];

        let result = accelerator.parallel_lsb_embed(&mut data, &payload);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);

        // Verify embedding worked
        for i in 0..100 {
            assert_eq!(data[i] & 0x01, 1);
        }
    }

    #[test]
    fn test_accelerator_extraction() {
        let accelerator = GpuAccelerator::default();
        let data = vec![0x01u8; 1000]; // All LSBs set

        let result = accelerator.parallel_lsb_extract(&data, 500);
        assert!(result.is_ok());

        let extracted = result.unwrap();
        assert_eq!(extracted.len(), 500);

        // All extracted bits should be 1
        for bit in extracted {
            assert_eq!(bit, 1);
        }
    }

    #[test]
    fn test_performance_benchmarking() {
        let accelerator = GpuAccelerator::default();
        let benchmark = accelerator.benchmark_performance(50_000);

        assert!(benchmark.data_size == 50_000);
        // Duration might be 0 for very fast operations, that's OK
        // Note: Duration fields are unsigned, so >= 0 check is redundant
        assert!(benchmark.performance_score() > 0);

        println!("Benchmark results:");
        println!(
            "  Embed throughput: {:.1} MB/s",
            benchmark.embed_throughput_mbps
        );
        println!(
            "  Extract throughput: {:.1} MB/s",
            benchmark.extract_throughput_mbps
        );
        println!("  Performance score: {}/100", benchmark.performance_score());
    }

    #[test]
    fn test_optimal_chunk_size() {
        let accelerator = GpuAccelerator::default();
        let chunk_size = accelerator.optimal_chunk_size();

        assert!(chunk_size > 0);
        assert!(chunk_size <= 1024 * 1024); // Reasonable upper bound
        println!("Optimal chunk size: {chunk_size} bytes");
    }

    #[test]
    fn test_processing_time_estimation() {
        let accelerator = GpuAccelerator::default();

        let test_sizes = [1_000, 10_000, 100_000, 1_000_000];

        for size in test_sizes {
            let estimated = accelerator.estimate_processing_time(size);
            // Note: Duration values are unsigned, so >= 0 check is redundant
            println!(
                "Estimated time for {} bytes: {:.2}ms",
                size,
                estimated.as_millis()
            );
        }
    }
}
