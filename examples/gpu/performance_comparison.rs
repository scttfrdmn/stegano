//! GPU vs CPU Performance Comparison Example
//! 
//! This example demonstrates the performance benefits of GPU acceleration
//! by comparing embedding and extraction times between CPU and GPU implementations.

use stegano::{
    core::{CarrierData, DataFormat, EmbedParams, StealthLevel},
    handlers::HandlerRegistry,
    gpu::GpuAccelerator,
};
use std::collections::HashMap;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 GPU vs CPU Performance Comparison");
    println!("=====================================\n");

    // Create test data of different sizes
    let test_sizes = vec![
        (1024, "1KB"),
        (10_240, "10KB"), 
        (102_400, "100KB"),
        (1_024_000, "1MB"),
        (10_240_000, "10MB"),
    ];

    // Check GPU availability
    let gpu_available = GpuAccelerator::is_available();
    println!("GPU Available: {}\n", if gpu_available { "✅ Yes" } else { "❌ No" });

    if gpu_available {
        let gpu = GpuAccelerator::new().await?;
        println!("GPU Device: {}\n", gpu.device_info());
    }

    // Run performance tests for each size
    for (size, label) in test_sizes {
        println!("📊 Testing with {} payload", label);
        println!("-------------------");
        
        // Generate test data
        let carrier_data = generate_test_image(2048, 1536); // ~4MP image
        let payload = generate_test_payload(size);
        
        // Test CPU performance
        let cpu_time = test_cpu_performance(&carrier_data, &payload).await?;
        
        // Test GPU performance if available
        let gpu_time = if gpu_available {
            Some(test_gpu_performance(&carrier_data, &payload).await?)
        } else {
            None
        };
        
        // Display results
        println!("CPU Time: {}ms", cpu_time);
        if let Some(gpu_time) = gpu_time {
            println!("GPU Time: {}ms", gpu_time);
            let speedup = cpu_time as f64 / gpu_time as f64;
            println!("Speedup: {:.2}x", speedup);
            
            if speedup > 1.5 {
                println!("🎯 GPU provides significant acceleration!");
            } else if speedup > 1.0 {
                println!("⚡ GPU provides modest acceleration");
            } else {
                println!("💭 CPU is faster for this workload");
            }
        }
        println!();
    }

    // Detailed GPU operations comparison
    if gpu_available {
        println!("🔍 Detailed GPU Operations Analysis");
        println!("==================================\n");
        
        test_gpu_operations().await?;
    }

    // Memory usage comparison
    println!("💾 Memory Usage Analysis");
    println!("========================\n");
    
    test_memory_usage().await?;

    Ok(())
}

/// Test CPU-only performance
async fn test_cpu_performance(
    carrier_data: &[u8], 
    payload: &[u8]
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: carrier_data.to_vec(),
        metadata: HashMap::new(),
        dimensions: Some((2048, 1536)),
        sampling_rate: None,
    };
    
    let mut params = EmbedParams::new(b"test_key_cpu".to_vec());
    params.use_gpu = false; // Force CPU usage
    params.stealth_level = StealthLevel::Low; // Maximize performance
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG)
        .ok_or("PNG handler not found")?;
    
    let start = Instant::now();
    let _result = handler.embed(&mut carrier, payload, &params).await?;
    let embedding_time = start.elapsed().as_millis() as u64;
    
    // Test extraction
    let start = Instant::now();
    let _extracted = handler.extract(&carrier, &params).await?;
    let extraction_time = start.elapsed().as_millis() as u64;
    
    Ok(embedding_time + extraction_time)
}

/// Test GPU-accelerated performance
async fn test_gpu_performance(
    carrier_data: &[u8], 
    payload: &[u8]
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: carrier_data.to_vec(),
        metadata: HashMap::new(),
        dimensions: Some((2048, 1536)),
        sampling_rate: None,
    };
    
    let mut params = EmbedParams::new(b"test_key_gpu".to_vec());
    params.use_gpu = true; // Enable GPU acceleration
    params.stealth_level = StealthLevel::Low; // Maximize performance
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG)
        .ok_or("PNG handler not found")?;
    
    let start = Instant::now();
    let result = handler.embed(&mut carrier, payload, &params).await?;
    let embedding_time = start.elapsed().as_millis() as u64;
    
    // Verify GPU was actually used
    if !result.performance_stats.gpu_acceleration_used {
        println!("⚠️  Warning: GPU acceleration was not used");
    }
    
    // Test extraction
    let start = Instant::now();
    let _extracted = handler.extract(&carrier, &params).await?;
    let extraction_time = start.elapsed().as_millis() as u64;
    
    Ok(embedding_time + extraction_time)
}

/// Test individual GPU operations
async fn test_gpu_operations() -> Result<(), Box<dyn std::error::Error>> {
    let gpu = GpuAccelerator::new().await?;
    
    // Test LSB embedding operation
    println!("Testing GPU LSB Embedding:");
    let mut test_data = vec![0x80u8; 1_000_000]; // 1MB test data
    let payload_bits = vec![1u8; 100_000]; // 100K bits
    
    let start = Instant::now();
    gpu.parallel_lsb_embed(&mut test_data, &payload_bits).await?;
    let lsb_time = start.elapsed().as_millis();
    println!("  LSB Embedding: {}ms", lsb_time);
    
    // Test GPU FFT operation
    println!("Testing GPU FFT:");
    let signal = vec![0.5f32; 65536]; // 64K samples
    
    let start = Instant::now();
    let _fft_result = gpu.gpu_fft(&signal).await?;
    let fft_time = start.elapsed().as_millis();
    println!("  FFT (64K samples): {}ms", fft_time);
    
    // Test GPU Reed-Solomon encoding
    println!("Testing GPU Reed-Solomon:");
    let data = vec![0xAAu8; 10000]; // 10KB data
    let parity_bytes = 1000; // 10% parity
    
    let start = Instant::now();
    let _rs_result = gpu.parallel_reed_solomon(&data, parity_bytes).await?;
    let rs_time = start.elapsed().as_millis();
    println!("  Reed-Solomon (10KB): {}ms", rs_time);
    
    Ok(())
}

/// Test memory usage patterns
async fn test_memory_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing memory efficiency...");
    
    // Large carrier data to test memory patterns
    let large_carrier = generate_test_image(4096, 3072); // ~16MP image
    let large_payload = generate_test_payload(1_000_000); // 1MB payload
    
    let mut carrier = CarrierData {
        format: DataFormat::PNG,
        raw_data: large_carrier,
        metadata: HashMap::new(),
        dimensions: Some((4096, 3072)),
        sampling_rate: None,
    };
    
    let params = EmbedParams::new(b"memory_test_key".to_vec());
    
    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&DataFormat::PNG)
        .ok_or("PNG handler not found")?;
    
    let result = handler.embed(&mut carrier, &large_payload, &params).await?;
    
    let memory_mb = result.performance_stats.memory_usage_bytes as f64 / (1024.0 * 1024.0);
    let efficiency = carrier.raw_data.len() as f64 / result.performance_stats.memory_usage_bytes as f64;
    
    println!("Memory Usage: {:.1} MB", memory_mb);
    println!("Memory Efficiency: {:.2}x (lower is better)", 1.0 / efficiency);
    
    if efficiency > 0.8 {
        println!("✅ Excellent memory efficiency");
    } else if efficiency > 0.5 {
        println!("⚡ Good memory efficiency");
    } else {
        println!("💭 Consider optimizing memory usage");
    }
    
    Ok(())
}

/// Generate test image data (simplified PNG-like data)
fn generate_test_image(width: u32, height: u32) -> Vec<u8> {
    let pixels = (width * height * 3) as usize; // RGB
    let mut data = Vec::with_capacity(pixels + 1000); // Extra space for headers
    
    // Simplified PNG header
    data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    
    // Generate pseudo-random image data
    for i in 0..pixels {
        data.push(((i * 31337) % 256) as u8);
    }
    
    data
}

/// Generate test payload data
fn generate_test_payload(size: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(size);
    
    // Generate meaningful test data (not just zeros)
    for i in 0..size {
        payload.push(((i * 17 + 42) % 256) as u8);
    }
    
    payload
}

/// Performance analysis helper
struct PerformanceAnalyzer {
    cpu_times: Vec<u64>,
    gpu_times: Vec<u64>,
}

impl PerformanceAnalyzer {
    fn new() -> Self {
        Self {
            cpu_times: Vec::new(),
            gpu_times: Vec::new(),
        }
    }
    
    fn add_cpu_time(&mut self, time: u64) {
        self.cpu_times.push(time);
    }
    
    fn add_gpu_time(&mut self, time: u64) {
        self.gpu_times.push(time);
    }
    
    fn analyze(&self) -> PerformanceReport {
        let cpu_avg = self.cpu_times.iter().sum::<u64>() as f64 / self.cpu_times.len() as f64;
        let gpu_avg = if !self.gpu_times.is_empty() {
            self.gpu_times.iter().sum::<u64>() as f64 / self.gpu_times.len() as f64
        } else {
            0.0
        };
        
        PerformanceReport {
            cpu_average: cpu_avg,
            gpu_average: gpu_avg,
            speedup: if gpu_avg > 0.0 { cpu_avg / gpu_avg } else { 0.0 },
        }
    }
}

struct PerformanceReport {
    cpu_average: f64,
    gpu_average: f64,
    speedup: f64,
}

impl std::fmt::Display for PerformanceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, 
            "Performance Report:\n  CPU Average: {:.1}ms\n  GPU Average: {:.1}ms\n  Speedup: {:.2}x",
            self.cpu_average, self.gpu_average, self.speedup
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_performance_comparison() {
        // This test ensures the performance comparison functions work
        let carrier = generate_test_image(512, 512);
        let payload = generate_test_payload(1024);
        
        let cpu_time = test_cpu_performance(&carrier, &payload).await;
        assert!(cpu_time.is_ok());
        
        if GpuAccelerator::is_available() {
            let gpu_time = test_gpu_performance(&carrier, &payload).await;
            assert!(gpu_time.is_ok());
        }
    }
    
    #[test]
    fn test_data_generation() {
        let image = generate_test_image(100, 100);
        assert!(!image.is_empty());
        assert!(image.starts_with(b"\x89PNG"));
        
        let payload = generate_test_payload(1000);
        assert_eq!(payload.len(), 1000);
        // Verify it's not all zeros
        assert!(payload.iter().any(|&x| x != 0));
    }
}