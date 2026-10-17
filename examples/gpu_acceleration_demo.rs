//! Shows the experimental GPU accelerator (with CPU fallback); requires `--features experimental`.

use std::collections::HashMap;
use std::time::Instant;
use stegano::handlers::HandlerRegistry;
use stegano::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🚀 Stegano GPU Acceleration Demo");
    println!("================================");

    // Try to create GPU accelerator
    println!("\n📊 GPU Availability Check:");
    match gpu::GpuAccelerator::new() {
        Ok(gpu_accelerator) => {
            println!(
                "✅ GPU acceleration available: {}",
                gpu_accelerator.backend_info()
            );
            println!("   GPU Available: {}", gpu_accelerator.is_gpu_available());
            println!(
                "   Optimal Chunk Size: {} bytes",
                gpu_accelerator.optimal_chunk_size()
            );

            // Run GPU-accelerated demo
            demo_gpu_operations(&gpu_accelerator).await?;
        }
        Err(e) => {
            println!("⚠️  No GPU acceleration available: {e}");
            println!("    Falling back to CPU operations");
            demo_cpu_operations().await?;
        }
    }

    println!("\n✨ Demo completed successfully!");
    Ok(())
}

/// Demonstrate GPU-accelerated operations
async fn demo_gpu_operations(
    gpu_accelerator: &gpu::GpuAccelerator,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 GPU-Accelerated Operations:");

    // Create test data
    let mut test_data = vec![0u8; 1024 * 1024]; // 1MB test data
    for (i, byte) in test_data.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let payload_bits = vec![0b10101010u8; 1000]; // Test payload

    // Test parallel LSB embedding
    println!("  Testing parallel LSB embedding...");
    let start = Instant::now();
    let embedded_bits = gpu_accelerator.parallel_lsb_embed(&mut test_data, &payload_bits)?;
    let embed_duration = start.elapsed();
    println!("    ✅ Embedded {embedded_bits} bits in {embed_duration:?}");

    // Test parallel LSB extraction
    println!("  Testing parallel LSB extraction...");
    let start = Instant::now();
    let extracted_bits = gpu_accelerator.parallel_lsb_extract(&test_data, payload_bits.len())?;
    let extract_duration = start.elapsed();
    println!(
        "    ✅ Extracted {} bytes in {:?}",
        extracted_bits.len(),
        extract_duration
    );

    // Verify round-trip
    assert_eq!(
        payload_bits, extracted_bits,
        "Round-trip verification failed"
    );
    println!("    ✅ Round-trip verification successful");

    // Benchmark performance
    println!("  Running performance benchmark...");
    let benchmark = gpu_accelerator.benchmark_performance(1024 * 1024);
    println!("    📊 Benchmark Results:");
    println!("       Data Size: {} bytes", benchmark.data_size);
    println!(
        "       Embed Throughput: {:.2} MB/s",
        benchmark.embed_throughput_mbps
    );
    println!(
        "       Extract Throughput: {:.2} MB/s",
        benchmark.extract_throughput_mbps
    );
    println!(
        "       Performance Score: {}",
        benchmark.performance_score()
    );

    // Show processing time estimation
    let estimated_time = gpu_accelerator.estimate_processing_time(10 * 1024 * 1024); // 10MB
    println!("       Estimated time for 10MB: {estimated_time:?}");

    Ok(())
}

/// Demonstrate CPU-only operations for comparison
async fn demo_cpu_operations() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n🖥️  CPU-Only Operations:");

    // Create test carrier data
    let carrier_data = vec![0x42u8; 1024]; // Small test data
    let mut carrier = CarrierData {
        format: DataFormat::BINARY,
        raw_data: carrier_data,
        metadata: HashMap::new(),
        dimensions: None,
        sampling_rate: None,
    };

    let registry = HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    let params = EmbedParams::new(b"test_key".to_vec());
    let payload = b"Hello, CPU-only steganography!";

    // Embed data
    println!("  Embedding data with CPU...");
    let start = Instant::now();
    let result = handler.embed(&mut carrier, payload, &params).await?;
    let embed_duration = start.elapsed();
    println!(
        "    ✅ Embedded {} bytes in {:?}",
        result.bytes_embedded, embed_duration
    );

    // Extract data
    println!("  Extracting data with CPU...");
    let start = Instant::now();
    let extracted = handler.extract(&carrier, &params).await?;
    let extract_duration = start.elapsed();
    println!(
        "    ✅ Extracted {} bytes in {:?}",
        extracted.len(),
        extract_duration
    );

    // Verify round-trip
    assert_eq!(payload, &extracted[..], "Round-trip verification failed");
    println!("    ✅ Round-trip verification successful");

    Ok(())
}
