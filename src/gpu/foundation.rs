//! GPU acceleration foundation for stegano 0.2.0
//!
//! This module provides the foundational GPU acceleration interface that will be
//! expanded with full compute shader implementations in subsequent iterations.

use crate::core::*;
use rayon::prelude::*;

/// GPU acceleration foundation with performance measurement
#[derive(Debug)]
pub struct GpuFoundation {
    /// Performance baseline measurements
    pub performance_profile: PerformanceProfile,
    /// Whether GPU acceleration is available
    pub gpu_available: bool,
    /// Backend information
    pub backend_info: String,
}

/// Performance characteristics of the GPU acceleration system
#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    /// Estimated parallel processing units
    pub parallel_units: usize,
    /// Memory bandwidth in GB/s
    pub memory_bandwidth_gbps: f64,
    /// Compute capability score (0-100)
    pub compute_score: u32,
    /// Whether unified memory is supported
    pub unified_memory: bool,
}

impl GpuFoundation {
    /// Initialize GPU acceleration foundation
    pub fn new() -> Result<Self> {
        let gpu_available = Self::detect_gpu_availability();
        let performance_profile = Self::measure_performance();
        let backend_info = Self::detect_backend_info();

        log::info!(
            "GPU Foundation initialized: available={}, backend={}",
            gpu_available,
            backend_info
        );
        log::info!(
            "Performance profile: {} parallel units, {:.1} GB/s bandwidth",
            performance_profile.parallel_units,
            performance_profile.memory_bandwidth_gbps
        );

        Ok(Self {
            performance_profile,
            gpu_available,
            backend_info,
        })
    }

    /// Detect if GPU acceleration is available on this system
    fn detect_gpu_availability() -> bool {
        // Check for GPU acceleration capabilities
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if crate::gpu::metal::MetalAccelerator::is_available() {
                return true;
            }
        }

        // For now, consider GPU available if we can create a WGPU instance
        // This will be expanded with actual WGPU device detection
        #[cfg(feature = "gpu")]
        {
            // Simplified detection - in full implementation, this would
            // actually attempt to create a WGPU instance asynchronously
            true
        }

        #[cfg(not(feature = "gpu"))]
        false
    }

    /// Measure system performance characteristics
    fn measure_performance() -> PerformanceProfile {
        let cpu_cores = num_cpus::get();

        // Estimate performance based on system characteristics
        let parallel_units = if Self::detect_gpu_availability() {
            // GPU estimates
            #[cfg(target_os = "macos")]
            {
                256
            } // Apple Silicon GPU estimate
            #[cfg(not(target_os = "macos"))]
            {
                128
            } // General GPU estimate
        } else {
            cpu_cores
        };

        let memory_bandwidth_gbps = if Self::detect_gpu_availability() {
            #[cfg(target_os = "macos")]
            {
                200.0
            } // Apple Silicon unified memory bandwidth
            #[cfg(not(target_os = "macos"))]
            {
                100.0
            } // General GPU memory bandwidth
        } else {
            50.0 // System RAM bandwidth estimate
        };

        let compute_score = if Self::detect_gpu_availability() {
            85
        } else {
            25
        };
        let unified_memory = cfg!(target_os = "macos");

        PerformanceProfile {
            parallel_units,
            memory_bandwidth_gbps,
            compute_score,
            unified_memory,
        }
    }

    /// Detect backend information
    fn detect_backend_info() -> String {
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if crate::gpu::metal::MetalAccelerator::is_available() {
                return "Metal (macOS)".to_string();
            }
        }

        #[cfg(feature = "gpu")]
        {
            "WGPU (Cross-platform)".to_string()
        }

        #[cfg(not(feature = "gpu"))]
        "CPU-only (No GPU features)".to_string()
    }

    /// High-performance parallel LSB embedding with CPU fallback
    pub fn parallel_lsb_embed(&self, data: &mut [u8], payload_bits: &[u8]) -> Result<usize> {
        let start_time = std::time::Instant::now();

        // Use parallel processing for LSB embedding
        data.par_chunks_mut(1024)
            .enumerate()
            .for_each(|(chunk_idx, chunk)| {
                let base_offset = chunk_idx * 1024;

                for (i, byte) in chunk.iter_mut().enumerate() {
                    let payload_idx = base_offset + i;
                    if payload_idx < payload_bits.len() {
                        // Clear LSB and set to payload bit
                        *byte = (*byte & 0xFE) | (payload_bits[payload_idx] & 0x01);
                    }
                }
            });

        let duration = start_time.elapsed();
        let throughput_mbps = (data.len() as f64 / duration.as_secs_f64()) / 1_000_000.0;

        log::debug!(
            "LSB embedding completed: {} bytes in {:.2}ms ({:.1} MB/s)",
            data.len(),
            duration.as_millis(),
            throughput_mbps
        );

        Ok(payload_bits.len().min(data.len()))
    }

    /// High-performance parallel LSB extraction
    pub fn parallel_lsb_extract(&self, data: &[u8], payload_size: usize) -> Result<Vec<u8>> {
        let start_time = std::time::Instant::now();

        let extracted: Vec<u8> = data
            .par_iter()
            .take(payload_size)
            .map(|byte| byte & 0x01)
            .collect();

        let duration = start_time.elapsed();
        let throughput_mbps = (data.len() as f64 / duration.as_secs_f64()) / 1_000_000.0;

        log::debug!(
            "LSB extraction completed: {} bytes in {:.2}ms ({:.1} MB/s)",
            extracted.len(),
            duration.as_millis(),
            throughput_mbps
        );

        Ok(extracted)
    }

    /// Benchmark embedding performance
    pub fn benchmark_embedding(&self, data_size: usize) -> BenchmarkResult {
        let mut test_data = vec![0xAAu8; data_size]; // Alternating bit pattern
        let payload = vec![0x55u8; data_size / 8]; // Different pattern for payload

        let start = std::time::Instant::now();
        let _embedded = self.parallel_lsb_embed(&mut test_data, &payload);
        let embed_duration = start.elapsed();

        let start = std::time::Instant::now();
        let _extracted = self.parallel_lsb_extract(&test_data, payload.len());
        let extract_duration = start.elapsed();

        BenchmarkResult {
            data_size,
            embed_duration_ms: embed_duration.as_millis() as u64,
            extract_duration_ms: extract_duration.as_millis() as u64,
            embed_throughput_mbps: (data_size as f64 / embed_duration.as_secs_f64()) / 1_000_000.0,
            extract_throughput_mbps: (data_size as f64 / extract_duration.as_secs_f64())
                / 1_000_000.0,
        }
    }

    /// Get optimal chunk size for parallel processing
    pub fn optimal_chunk_size(&self) -> usize {
        if self.gpu_available {
            // GPU-optimized chunk size
            64 * 1024 // 64KB chunks for GPU processing
        } else {
            // CPU-optimized chunk size
            4 * 1024 // 4KB chunks for CPU processing
        }
    }

    /// Estimate processing time for given data size
    pub fn estimate_processing_time(&self, data_size: usize) -> std::time::Duration {
        let throughput = if self.gpu_available {
            self.performance_profile.memory_bandwidth_gbps * 0.8 // 80% efficiency
        } else {
            self.performance_profile.memory_bandwidth_gbps * 0.5 // 50% efficiency
        };

        let processing_time_secs = (data_size as f64) / (throughput * 1_000_000_000.0);
        std::time::Duration::from_secs_f64(processing_time_secs)
    }
}

impl Default for GpuFoundation {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            performance_profile: PerformanceProfile {
                parallel_units: num_cpus::get(),
                memory_bandwidth_gbps: 10.0,
                compute_score: 10,
                unified_memory: false,
            },
            gpu_available: false,
            backend_info: "Fallback CPU".to_string(),
        })
    }
}

/// Benchmark results for performance analysis
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub data_size: usize,
    pub embed_duration_ms: u64,
    pub extract_duration_ms: u64,
    pub embed_throughput_mbps: f64,
    pub extract_throughput_mbps: f64,
}

impl BenchmarkResult {
    /// Get overall performance score (0-100)
    pub fn performance_score(&self) -> u32 {
        let combined_throughput = (self.embed_throughput_mbps + self.extract_throughput_mbps) / 2.0;

        // Score based on throughput ranges
        if combined_throughput > 500.0 {
            95
        } else if combined_throughput > 200.0 {
            85
        } else if combined_throughput > 100.0 {
            75
        } else if combined_throughput > 50.0 {
            60
        } else if combined_throughput > 20.0 {
            40
        } else if combined_throughput > 5.0 {
            25
        } else {
            10
        }
    }

    /// Check if performance meets target thresholds
    pub fn meets_targets(&self) -> bool {
        // Target: > 50 MB/s for both operations
        self.embed_throughput_mbps > 50.0 && self.extract_throughput_mbps > 50.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_foundation_creation() {
        let foundation = GpuFoundation::new();
        assert!(foundation.is_ok());

        let foundation = foundation.unwrap();
        assert!(foundation.performance_profile.parallel_units > 0);
        assert!(foundation.performance_profile.memory_bandwidth_gbps > 0.0);
        assert!(!foundation.backend_info.is_empty());
    }

    #[test]
    fn test_parallel_lsb_embedding() {
        let foundation = GpuFoundation::default();
        let mut data = vec![0xAAu8; 1000]; // 10101010 pattern
        let payload = vec![0x01u8; 100]; // All 1s

        let result = foundation.parallel_lsb_embed(&mut data, &payload);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);

        // Check that LSBs were modified
        for i in 0..100 {
            assert_eq!(data[i] & 0x01, 1, "LSB should be set at position {}", i);
        }

        // Check that positions beyond payload weren't modified
        for i in 100..1000 {
            assert_eq!(
                data[i], 0xAA,
                "Data beyond payload should be unchanged at position {}",
                i
            );
        }
    }

    #[test]
    fn test_parallel_lsb_extraction() {
        let foundation = GpuFoundation::default();
        let mut data = vec![0xAAu8; 1000]; // 10101010 pattern

        // Manually set some LSBs
        for i in 0..50 {
            data[i] = 0xAB; // Set LSB to 1
        }

        let result = foundation.parallel_lsb_extract(&data, 100);
        assert!(result.is_ok());

        let extracted = result.unwrap();
        assert_eq!(extracted.len(), 100);

        // Check extracted bits
        for i in 0..50 {
            assert_eq!(
                extracted[i], 1,
                "Extracted bit should be 1 at position {}",
                i
            );
        }
        for i in 50..100 {
            assert_eq!(
                extracted[i], 0,
                "Extracted bit should be 0 at position {}",
                i
            );
        }
    }

    #[test]
    fn test_performance_benchmarking() {
        let foundation = GpuFoundation::default();
        let benchmark = foundation.benchmark_embedding(10_000);

        assert!(benchmark.data_size == 10_000);
        // Duration might be 0 for very fast operations, that's OK
        // Note: Duration fields are unsigned, so >= 0 check is redundant
        assert!(benchmark.embed_throughput_mbps > 0.0);
        assert!(benchmark.extract_throughput_mbps > 0.0);

        let score = benchmark.performance_score();
        assert!(score >= 10 && score <= 100);
    }

    #[test]
    fn test_optimal_chunk_size() {
        let foundation = GpuFoundation::default();
        let chunk_size = foundation.optimal_chunk_size();

        assert!(chunk_size > 0);
        assert!(chunk_size <= 1024 * 1024); // Reasonable upper bound
    }

    #[test]
    fn test_processing_time_estimation() {
        let foundation = GpuFoundation::default();
        let estimated_time = foundation.estimate_processing_time(1_000_000);

        // Estimated time might be very small, that's OK
        // Note: Duration values are unsigned, so >= 0 check is redundant
        assert!(estimated_time.as_secs() < 60); // Should be reasonable
    }

    #[test]
    fn test_benchmark_result_analysis() {
        let benchmark = BenchmarkResult {
            data_size: 100_000,
            embed_duration_ms: 10,
            extract_duration_ms: 8,
            embed_throughput_mbps: 100.0,
            extract_throughput_mbps: 125.0,
        };

        assert!(benchmark.performance_score() > 50);
        assert!(benchmark.meets_targets());

        let slow_benchmark = BenchmarkResult {
            data_size: 100_000,
            embed_duration_ms: 1000,
            extract_duration_ms: 1200,
            embed_throughput_mbps: 1.0,
            extract_throughput_mbps: 0.8,
        };

        assert!(!slow_benchmark.meets_targets());
        assert!(slow_benchmark.performance_score() < 30);
    }
}
