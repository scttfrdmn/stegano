//! GPU Acceleration Demo
//!
//! Demonstrates the performance difference between CPU and GPU acceleration
//! for steganography operations, with automatic backend selection.

use std::collections::HashMap;
use std::time::Instant;
use stegano::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🚀 Stegano GPU Acceleration Demo");
    println!("================================");

    // Check GPU availability
    println!("\n📊 GPU Availability Check:");
    let gpu_available = gpu::GpuAccelerator::is_available();
    println!("GPU acceleration available: {}", gpu_available);

    #[cfg(all(target_os = "macos", feature = "metal"))]
    {
        let metal_available = gpu::metal::MetalAccelerator::is_available();
        println!("Metal backend available: {}", metal_available);
    }

    if !gpu_available {
        println!("⚠️  No GPU acceleration available, demonstrating CPU-only operations");
        demo_cpu_operations().await?;
        return Ok(());
    }

    // Create GPU accelerator with automatic backend selection
    println!("\n🔧 Initializing GPU Accelerator...");
    let gpu_accelerator = match gpu::GpuAccelerator::new().await {
        Ok(acc) => {
            println!("✅ GPU accelerator initialized: {}", acc.device_info());
            acc
        }
        Err(e) => {
            println!("❌ Failed to initialize GPU accelerator: {}", e);
            println!("Falling back to CPU-only demonstration");
            demo_cpu_operations().await?;
            return Ok(());
        }
    };

    // Test different GPU backends if available
    println!("\n🧪 Testing GPU Backends:");
    test_gpu_backends().await?;

    // Performance comparison
    println!("\n⚡ Performance Comparison:");
    performance_comparison(&gpu_accelerator).await?;

    // Advanced GPU operations demo
    println!("\n🔬 Advanced GPU Operations:");
    advanced_gpu_demo(&gpu_accelerator).await?;

    Ok(())
}

async fn demo_cpu_operations() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n💻 CPU-Only Operations Demo:");

    // Create test data
    let carrier_size = 1024 * 1024; // 1MB
    let mut carrier_data = vec![0x80u8; carrier_size];
    let payload = b"This is a test payload for CPU processing demonstration. It contains enough data to show timing differences.";

    let start = Instant::now();

    // Use registry to get handler for CPU-based embedding
    let mut carrier = CarrierData {
        format: DataFormat::BINARY,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    let params = EmbedParams::new(b"test_key".to_vec());

    // Embed data
    let result = handler.embed(&mut carrier, payload, &params).await?;
    let cpu_time = start.elapsed();

    println!("CPU embedding completed:");
    println!("  - Time: {:?}", cpu_time);
    println!("  - Bytes embedded: {}", result.bytes_embedded);
    println!(
        "  - Throughput: {:.2} MB/s",
        (payload.len() as f64) / (cpu_time.as_secs_f64() * 1024.0 * 1024.0)
    );

    // Verify extraction
    let extracted = handler.extract(&carrier, &params).await?;
    assert_eq!(extracted, payload);
    println!("  - Verification: ✅ Success");

    Ok(())
}

async fn test_gpu_backends() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Test WGPU backend
    println!("Testing WGPU backend...");
    match gpu::GpuAccelerator::new_with_backend(gpu::GpuBackend::Wgpu).await {
        Ok(wgpu_acc) => {
            println!("  ✅ WGPU: {}", wgpu_acc.device_info());
        }
        Err(e) => {
            println!("  ❌ WGPU failed: {}", e);
        }
    }

    // Test Metal backend (macOS only)
    #[cfg(all(target_os = "macos", feature = "metal"))]
    {
        println!("Testing Metal backend...");
        match gpu::GpuAccelerator::new_with_backend(gpu::GpuBackend::Metal).await {
            Ok(metal_acc) => {
                println!("  ✅ Metal: {}", metal_acc.device_info());

                // Show Metal-specific capabilities
                if let gpu::GpuAccelerator::Metal(ref metal) = metal_acc {
                    let caps = metal.capabilities();
                    println!(
                        "    - Max threads per threadgroup: {:?}",
                        caps.max_threads_per_threadgroup
                    );
                    println!("    - Unified memory: {}", caps.unified_memory);
                    println!(
                        "    - Max buffer length: {:.2} GB",
                        caps.max_buffer_length as f64 / (1024.0 * 1024.0 * 1024.0)
                    );
                }
            }
            Err(e) => {
                println!("  ❌ Metal failed: {}", e);
            }
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "metal")))]
    {
        println!("Metal backend not available on this platform");
    }

    Ok(())
}

async fn performance_comparison(
    gpu_accelerator: &gpu::GpuAccelerator,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Test data sizes
    let test_sizes = vec![
        (1024, "1KB"),
        (1024 * 64, "64KB"),
        (1024 * 256, "256KB"),
        (1024 * 1024, "1MB"),
    ];

    println!("Testing different data sizes:");

    for (size, label) in test_sizes {
        println!("\n📦 Testing with {} data:", label);

        // Prepare test data
        let mut carrier = vec![0xAAu8; size * 2]; // Enough space for payload
        let payload_bits: Vec<u8> = (0..size).map(|i| (i % 2) as u8).collect();

        // CPU baseline (using generic handler)
        let start = Instant::now();
        let handler = handlers::generic::GenericHandler::new();
        let cpu_result = handler.lsb_embed(&mut carrier.clone(), &payload_bits);
        let cpu_time = start.elapsed();

        match cpu_result {
            Ok(_) => {
                println!(
                    "  CPU time: {:?} ({:.2} MB/s)",
                    cpu_time,
                    (size as f64) / (cpu_time.as_secs_f64() * 1024.0 * 1024.0)
                );
            }
            Err(e) => {
                println!("  CPU failed: {}", e);
                continue;
            }
        }

        // GPU acceleration (would need actual implementation)
        let start = Instant::now();
        let gpu_result = gpu_accelerator
            .parallel_lsb_embed(&mut carrier, &payload_bits)
            .await;
        let gpu_time = start.elapsed();

        match gpu_result {
            Ok(_) => {
                println!(
                    "  GPU time: {:?} ({:.2} MB/s)",
                    gpu_time,
                    (size as f64) / (gpu_time.as_secs_f64() * 1024.0 * 1024.0)
                );

                let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
                println!("  Speedup: {:.2}x", speedup);
            }
            Err(e) => {
                println!("  GPU implementation not ready: {}", e);
            }
        }
    }

    Ok(())
}

async fn advanced_gpu_demo(
    gpu_accelerator: &gpu::GpuAccelerator,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Testing advanced GPU operations:");

    // FFT demonstration
    println!("\n🌊 FFT Processing:");
    let signal_size = 1024;
    let test_signal: Vec<f32> = (0..signal_size)
        .map(|i| (2.0 * std::f32::consts::PI * i as f32 / signal_size as f32).sin())
        .collect();

    let start = Instant::now();
    let fft_result = gpu_accelerator.gpu_fft(&test_signal).await;
    let fft_time = start.elapsed();

    match fft_result {
        Ok(frequencies) => {
            println!("  ✅ GPU FFT completed in {:?}", fft_time);
            println!(
                "  Input samples: {}, Output frequencies: {}",
                test_signal.len(),
                frequencies.len()
            );

            // Find dominant frequency
            let max_magnitude_idx = frequencies
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.norm().partial_cmp(&b.norm()).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);

            println!(
                "  Dominant frequency bin: {} (magnitude: {:.2})",
                max_magnitude_idx,
                frequencies[max_magnitude_idx].norm()
            );
        }
        Err(e) => {
            println!("  ❌ GPU FFT not implemented: {}", e);
        }
    }

    // Reed-Solomon error correction
    println!("\n🛡️  Reed-Solomon Error Correction:");
    let test_data = b"Error correction test data for GPU demonstration";
    let parity_bytes = 16; // 16 bytes of error correction

    let start = Instant::now();
    let rs_result = gpu_accelerator
        .parallel_reed_solomon(test_data, parity_bytes)
        .await;
    let rs_time = start.elapsed();

    match rs_result {
        Ok(encoded_data) => {
            println!("  ✅ Reed-Solomon encoding completed in {:?}", rs_time);
            println!(
                "  Original: {} bytes, Encoded: {} bytes, Overhead: {} bytes",
                test_data.len(),
                encoded_data.len(),
                parity_bytes
            );
        }
        Err(e) => {
            println!("  ❌ Reed-Solomon not implemented: {}", e);
        }
    }

    Ok(())
}

/// Utility function for the generic handler (needed for CPU testing)
mod handlers {
    pub mod generic {
        use crate::*;

        pub struct GenericHandler;

        impl GenericHandler {
            pub fn new() -> Self {
                Self
            }

            pub fn lsb_embed(&self, carrier: &mut [u8], payload_bits: &[u8]) -> Result<usize> {
                if payload_bits.len() + 32 > carrier.len() {
                    return Err(SteganoError::InsufficientCapacity {
                        needed: payload_bits.len() + 32,
                        available: carrier.len(),
                    });
                }

                // Simple LSB embedding for demonstration
                for (i, &bit) in payload_bits.iter().enumerate() {
                    carrier[i + 32] = (carrier[i + 32] & 0xFE) | bit;
                }

                Ok(payload_bits.len())
            }
        }
    }
}
