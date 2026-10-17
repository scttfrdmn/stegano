# Performance Benchmarking Guide

This guide provides comprehensive performance analysis, optimization strategies, and benchmarking methodologies for the Stegano steganography system.

## Table of Contents

1. [Performance Overview](#performance-overview)
2. [Benchmarking Methodology](#benchmarking-methodology)
3. [Format-Specific Performance](#format-specific-performance)
4. [GPU Acceleration Analysis](#gpu-acceleration-analysis)
5. [Memory Usage Optimization](#memory-usage-optimization)
6. [Scalability Testing](#scalability-testing)
7. [Production Monitoring](#production-monitoring)
8. [Optimization Strategies](#optimization-strategies)

## Performance Overview

### System Requirements

**Minimum Requirements:**
- Rust 1.70+
- 4GB RAM
- 2-core CPU
- 1GB available disk space

**Recommended for Production:**
- Rust 1.70+
- 16GB RAM
- 8-core CPU (Apple M4 Pro recommended)
- GPU with Metal/Vulkan support
- NVMe SSD storage

### Performance Targets

| Operation | File Size | CPU Time | GPU Time | Memory Usage |
|-----------|-----------|----------|----------|--------------|
| FASTQ Quality Embedding | 100MB | 1.2s | 0.3s | 200MB |
| TIFF LSB Embedding | 50MB | 0.8s | 0.2s | 150MB |
| WAV Signal Embedding | 200MB | 2.1s | 0.5s | 250MB |
| Binary LSB Embedding | 1GB | 4.5s | 1.1s | 500MB |

### Key Performance Factors

1. **Format Complexity**: FASTQ > TIFF > WAV > Binary
2. **Stealth Level**: Maximum (0.1x) < High (0.3x) < Medium (0.6x) < Low (1.0x)
3. **Error Correction**: None (1.0x) < Low (1.1x) < Medium (1.25x) < High (1.5x) < Maximum (2.0x)
4. **GPU Acceleration**: 3-10x improvement for large files
5. **File Size**: Linear scaling with optimizations for >100MB files

## Benchmarking Methodology

### Setting Up Benchmarks

```bash
# Build with optimizations and benchmarking features
cargo build --release --features benchmarks

# Run comprehensive benchmarks
cargo bench --features benchmarks

# Generate HTML reports
cargo bench --features benchmarks -- --output-format html
```

### Custom Benchmark Framework

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use stegano::*;
use std::time::Duration;

fn benchmark_embedding(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_performance");
    
    // Test different file sizes
    let sizes = vec![1024, 10_240, 102_400, 1_048_576, 10_485_760]; // 1KB to 10MB
    
    for size in sizes {
        let data = vec![0u8; size];
        let payload = vec![42u8; size / 100]; // 1% payload ratio
        
        group.bench_with_input(
            BenchmarkId::new("binary_cpu", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        let mut carrier = CarrierData::new(DataFormat::BINARY, data.clone());
                        let params = EmbedParams::new(b"benchmark_key".to_vec());
                        
                        let registry = handlers::HandlerRegistry::new();
                        let handler = registry.get_handler(&carrier.format).unwrap();
                        
                        black_box(handler.embed(&mut carrier, &payload, &params).await.unwrap());
                    });
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("binary_gpu", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        let mut carrier = CarrierData::new(DataFormat::BINARY, data.clone());
                        let params = EmbedParams::new(b"benchmark_key".to_vec()).with_gpu();
                        
                        let registry = handlers::HandlerRegistry::new();
                        let handler = registry.get_handler(&carrier.format).unwrap();
                        
                        black_box(handler.embed(&mut carrier, &payload, &params).await.unwrap());
                    });
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, benchmark_embedding);
criterion_main!(benches);
```

### Real-World Performance Testing

```rust
use stegano::*;
use std::time::Instant;

struct PerformanceMetrics {
    throughput_mbps: f64,
    latency_ms: u64,
    memory_peak_mb: usize,
    gpu_utilization: bool,
}

async fn benchmark_real_world_scenario() -> Result<PerformanceMetrics> {
    // Simulate real genomics workflow
    let fastq_data = generate_realistic_fastq(10_000_000); // 10M reads
    let mut carrier = CarrierData::new(DataFormat::FASTQ, fastq_data);
    
    // Research metadata payload
    let payload = serde_json::to_vec(&json!({
        "experiment_id": "EXP-2025-042",
        "researcher": "Dr. Jane Smith",
        "institution": "Research Institute",
        "findings": "Significant correlation found between gene expression and treatment",
        "statistical_significance": 0.001,
        "sample_count": 10000
    })).unwrap();
    
    let params = EmbedParams::new(b"research_key_2025".to_vec())
        .with_error_correction(ECLevel::High)
        .with_stealth_level(StealthLevel::High)
        .with_gpu();
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    // Measure embedding performance
    let start = Instant::now();
    let result = handler.embed(&mut carrier, &payload, &params).await?;
    let latency = start.elapsed();
    
    // Calculate metrics
    let throughput_mbps = (carrier.size() as f64 / 1_048_576.0) / latency.as_secs_f64();
    let memory_peak_mb = result.performance_stats.memory_usage_bytes / 1_048_576;
    
    Ok(PerformanceMetrics {
        throughput_mbps,
        latency_ms: latency.as_millis() as u64,
        memory_peak_mb,
        gpu_utilization: result.performance_stats.gpu_acceleration_used,
    })
}

fn generate_realistic_fastq(num_reads: usize) -> Vec<u8> {
    let mut fastq = String::new();
    
    for i in 0..num_reads {
        fastq.push_str(&format!("@read_{}/1\n", i));
        
        // Generate realistic DNA sequence (150bp)
        let bases = ['A', 'T', 'C', 'G'];
        for _ in 0..150 {
            fastq.push(bases[rand::random::<usize>() % 4]);
        }
        fastq.push('\n');
        fastq.push_str("+\n");
        
        // Generate realistic quality scores (Phred 20-40)
        for _ in 0..150 {
            let quality = (rand::random::<u8>() % 21) + 53; // ASCII 53-73 (Phred 20-40)
            fastq.push(quality as char);
        }
        fastq.push('\n');
    }
    
    fastq.into_bytes()
}
```

## Format-Specific Performance

### FASTQ Quality Score Embedding

```rust
use stegano::handlers::genomics::{GenomicsHandler, QualityEmbedStrategy};
use stegano::*;

async fn benchmark_fastq_strategies() -> Result<()> {
    let handler = GenomicsHandler::new();
    let fastq_data = generate_realistic_fastq(100_000); // 100K reads
    
    let strategies = vec![
        QualityEmbedStrategy::Conservative,
        QualityEmbedStrategy::Balanced,
        QualityEmbedStrategy::Aggressive,
    ];
    
    for strategy in strategies {
        let mut carrier_data = fastq_data.clone();
        let payload = vec![42u8; 10_000]; // 10KB payload
        
        let start = Instant::now();
        let result = handler.embed_fastq_qualities(
            &mut carrier_data,
            &payload,
            strategy
        )?;
        let duration = start.elapsed();
        
        println!("{:?} Strategy:", strategy);
        println!("  Time: {}ms", duration.as_millis());
        println!("  Bytes embedded: {}", result.bytes_embedded);
        println!("  Quality chars modified: {}", result.quality_chars_modified);
        println!("  Biological integrity: {}", result.biological_integrity_preserved);
        
        // Measure extraction performance
        let start = Instant::now();
        let extracted = handler.extract_fastq_qualities(&carrier_data, strategy)?;
        let extract_duration = start.elapsed();
        
        println!("  Extraction time: {}ms", extract_duration.as_millis());
        println!("  Round-trip success: {}", extracted == payload);
        println!();
    }
    
    Ok(())
}
```

### Image Processing Performance

```rust
use stegano::*;

async fn benchmark_image_formats() -> Result<()> {
    let formats = vec![
        (DataFormat::PNG, "PNG"),
        (DataFormat::TIFF, "TIFF"),
    ];
    
    let image_sizes = vec![
        (512, 512),    // 0.25MP
        (1024, 1024),  // 1MP  
        (2048, 2048),  // 4MP
        (4096, 4096),  // 16MP
    ];
    
    for (format, name) in formats {
        println!("{} Format Performance:", name);
        
        for (width, height) in image_sizes.clone() {
            let pixels = width * height;
            let image_data = vec![128u8; pixels * 3]; // RGB
            let mut carrier = CarrierData::new(format.clone(), image_data);
            carrier.dimensions = Some(Dimensions::new_2d(width as u32, height as u32));
            
            let payload_size = pixels / 1000; // 0.1% payload ratio
            let payload = vec![42u8; payload_size];
            
            let params = EmbedParams::new(b"image_key".to_vec())
                .with_stealth_level(StealthLevel::Medium);
            
            let registry = handlers::HandlerRegistry::new();
            let handler = registry.get_handler(&carrier.format).unwrap();
            
            let start = Instant::now();
            let result = handler.embed(&mut carrier, &payload, &params).await?;
            let duration = start.elapsed();
            
            let throughput = (pixels as f64 / 1_000_000.0) / duration.as_secs_f64();
            
            println!("  {}x{}: {}ms, {:.1} MP/s", 
                     width, height, duration.as_millis(), throughput);
        }
        println!();
    }
    
    Ok(())
}
```

### Signal Processing Benchmarks

```rust
use stegano::*;

async fn benchmark_signal_processing() -> Result<()> {
    let sample_rates = vec![8000, 16000, 44100, 48000, 96000]; // Hz
    let durations = vec![1, 5, 10, 30, 60]; // seconds
    
    for sample_rate in sample_rates {
        println!("Sample Rate: {} Hz", sample_rate);
        
        for duration_sec in durations.clone() {
            let num_samples = sample_rate * duration_sec;
            let audio_data = generate_sine_wave(sample_rate, duration_sec, 440.0); // A4 note
            
            let mut carrier = CarrierData::new(DataFormat::WAV, audio_data);
            carrier.sampling_rate = Some(sample_rate as f64);
            
            let payload_size = num_samples / 1000; // 0.1% ratio
            let payload = vec![42u8; payload_size];
            
            let params = EmbedParams::new(b"audio_key".to_vec())
                .with_stealth_level(StealthLevel::Low); // Audio can handle more aggressive embedding
            
            let registry = handlers::HandlerRegistry::new();
            let handler = registry.get_handler(&carrier.format).unwrap();
            
            let start = Instant::now();
            let result = handler.embed(&mut carrier, &payload, &params).await?;
            let duration_ms = start.elapsed().as_millis();
            
            let realtime_factor = duration_sec as f64 / (duration_ms as f64 / 1000.0);
            
            println!("  {}s audio: {}ms processing ({}x realtime)", 
                     duration_sec, duration_ms, realtime_factor as u32);
        }
        println!();
    }
    
    Ok(())
}

fn generate_sine_wave(sample_rate: usize, duration_sec: usize, frequency: f64) -> Vec<u8> {
    let num_samples = sample_rate * duration_sec;
    let mut samples = Vec::with_capacity(num_samples * 2); // 16-bit samples
    
    for i in 0..num_samples {
        let t = i as f64 / sample_rate as f64;
        let sample = (frequency * 2.0 * std::f64::consts::PI * t).sin();
        let quantized = (sample * 32767.0) as i16;
        
        samples.push((quantized & 0xFF) as u8);
        samples.push(((quantized >> 8) & 0xFF) as u8);
    }
    
    samples
}
```

## GPU Acceleration Analysis

### GPU vs CPU Performance Comparison

```rust
use stegano::*;

async fn compare_gpu_cpu_performance() -> Result<()> {
    let file_sizes = vec![
        1_048_576,     // 1MB
        10_485_760,    // 10MB  
        104_857_600,   // 100MB
        1_073_741_824, // 1GB
    ];
    
    println!("GPU vs CPU Performance Comparison");
    println!("Size\t\tCPU Time\tGPU Time\tSpeedup");
    println!("----\t\t--------\t--------\t-------");
    
    for size in file_sizes {
        let data = vec![0u8; size];
        let payload = vec![42u8; size / 1000]; // 0.1% payload
        
        // CPU benchmark
        let mut carrier_cpu = CarrierData::new(DataFormat::BINARY, data.clone());
        let params_cpu = EmbedParams::new(b"cpu_key".to_vec());
        
        let start = Instant::now();
        let registry = handlers::HandlerRegistry::new();
        let handler = registry.get_handler(&carrier_cpu.format).unwrap();
        let _result_cpu = handler.embed(&mut carrier_cpu, &payload, &params_cpu).await?;
        let cpu_time = start.elapsed();
        
        // GPU benchmark
        let mut carrier_gpu = CarrierData::new(DataFormat::BINARY, data);
        let params_gpu = EmbedParams::new(b"gpu_key".to_vec()).with_gpu();
        
        let start = Instant::now();
        let _result_gpu = handler.embed(&mut carrier_gpu, &payload, &params_gpu).await?;
        let gpu_time = start.elapsed();
        
        let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
        
        println!("{}MB\t\t{}ms\t\t{}ms\t\t{:.1}x",
                 size / 1_048_576,
                 cpu_time.as_millis(),
                 gpu_time.as_millis(),
                 speedup);
    }
    
    Ok(())
}
```

### GPU Memory Transfer Analysis

```rust
use stegano::gpu::GpuAccelerator;
use stegano::*;

async fn analyze_gpu_memory_transfer() -> Result<()> {
    let gpu = GpuAccelerator::new().await?;
    
    let data_sizes = vec![1_024, 10_240, 102_400, 1_048_576, 10_485_760];
    
    println!("GPU Memory Transfer Analysis");
    println!("Size\t\tTransfer\tCompute\tTotal\t\tEfficiency");
    println!("----\t\t--------\t-------\t-----\t\t----------");
    
    for size in data_sizes {
        let data = vec![0u8; size];
        
        // Measure transfer time
        let start = Instant::now();
        let gpu_buffer = gpu.upload_data(&data).await?;
        let transfer_time = start.elapsed();
        
        // Measure compute time (simplified LSB embedding)
        let start = Instant::now();
        let _result = gpu.process_lsb_embedding(&gpu_buffer, &[42u8; 100]).await?;
        let compute_time = start.elapsed();
        
        // Measure download time
        let start = Instant::now();
        let _downloaded = gpu.download_data(&gpu_buffer).await?;
        let download_time = start.elapsed();
        
        let total_time = transfer_time + compute_time + download_time;
        let efficiency = compute_time.as_secs_f64() / total_time.as_secs_f64();
        
        println!("{}KB\t\t{}ms\t\t{}ms\t\t{}ms\t\t{:.1}%",
                 size / 1024,
                 transfer_time.as_millis(),
                 compute_time.as_millis(),
                 total_time.as_millis(),
                 efficiency * 100.0);
    }
    
    Ok(())
}
```

## Memory Usage Optimization

### Memory Profiling

```rust
use stegano::*;

struct MemoryUsage {
    peak_mb: usize,
    carrier_mb: usize,
    payload_mb: usize,
    working_mb: usize,
}

async fn profile_memory_usage() -> Result<MemoryUsage> {
    let initial_memory = get_memory_usage();
    
    // Large file processing
    let carrier_size = 100_000_000; // 100MB
    let carrier_data = vec![0u8; carrier_size];
    let after_carrier = get_memory_usage();
    
    let payload_size = 1_000_000; // 1MB
    let payload = vec![42u8; payload_size];
    let after_payload = get_memory_usage();
    
    let mut carrier = CarrierData::new(DataFormat::BINARY, carrier_data);
    let params = EmbedParams::new(b"memory_key".to_vec());
    
    let registry = handlers::HandlerRegistry::new();
    let handler = registry.get_handler(&carrier.format).unwrap();
    
    let _result = handler.embed(&mut carrier, &payload, &params).await?;
    let peak_memory = get_memory_usage();
    
    Ok(MemoryUsage {
        peak_mb: peak_memory / 1_048_576,
        carrier_mb: (after_carrier - initial_memory) / 1_048_576,
        payload_mb: (after_payload - after_carrier) / 1_048_576,
        working_mb: (peak_memory - after_payload) / 1_048_576,
    })
}

fn get_memory_usage() -> usize {
    // Platform-specific memory usage measurement
    // This is a simplified example
    std::process::Command::new("ps")
        .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<usize>()
                .unwrap_or(0) * 1024 // Convert KB to bytes
        })
        .unwrap_or(0)
}
```

### Streaming Processing for Large Files

```rust
use stegano::*;
use std::io::{Read, Write};

async fn streaming_large_file_processing() -> Result<()> {
    const CHUNK_SIZE: usize = 1_048_576; // 1MB chunks
    
    let mut input = std::fs::File::open("large_dataset.bin")?;
    let mut output = std::fs::File::create("large_dataset_with_secret.bin")?;
    
    let payload = b"Secret data embedded across chunks";
    let mut payload_offset = 0;
    let chunk_payload_size = payload.len() / 10; // Distribute across 10 chunks
    
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunk_index = 0;
    
    while let Ok(bytes_read) = input.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }
        
        // Process chunk if we have payload data remaining
        if payload_offset < payload.len() {
            let chunk_payload_end = std::cmp::min(
                payload_offset + chunk_payload_size,
                payload.len()
            );
            let chunk_payload = &payload[payload_offset..chunk_payload_end];
            
            let mut carrier = CarrierData::new(DataFormat::BINARY, buffer[..bytes_read].to_vec());
            let params = EmbedParams::new(format!("chunk_key_{}", chunk_index).into_bytes());
            
            let registry = handlers::HandlerRegistry::new();
            let handler = registry.get_handler(&carrier.format).unwrap();
            
            let _result = handler.embed(&mut carrier, chunk_payload, &params).await?;
            output.write_all(&carrier.raw_data)?;
            
            payload_offset = chunk_payload_end;
        } else {
            // No more payload, just copy chunk
            output.write_all(&buffer[..bytes_read])?;
        }
        
        chunk_index += 1;
    }
    
    println!("Processed {} chunks with streaming", chunk_index);
    Ok(())
}
```

## Scalability Testing

### Concurrent Processing

```rust
use stegano::*;
use tokio::task::JoinSet;

async fn test_concurrent_processing() -> Result<()> {
    let num_concurrent = num_cpus::get();
    let mut join_set = JoinSet::new();
    
    println!("Testing concurrent processing with {} threads", num_concurrent);
    
    for i in 0..num_concurrent {
        join_set.spawn(async move {
            let data = vec![0u8; 1_048_576]; // 1MB per task
            let mut carrier = CarrierData::new(DataFormat::BINARY, data);
            let payload = vec![42u8; 1000];
            
            let params = EmbedParams::new(format!("concurrent_key_{}", i).into_bytes());
            
            let registry = handlers::HandlerRegistry::new();
            let handler = registry.get_handler(&carrier.format).unwrap();
            
            let start = Instant::now();
            let result = handler.embed(&mut carrier, &payload, &params).await.unwrap();
            let duration = start.elapsed();
            
            (i, duration, result.bytes_embedded)
        });
    }
    
    let mut total_time = Duration::ZERO;
    let mut total_bytes = 0;
    
    while let Some(result) = join_set.join_next().await {
        let (task_id, duration, bytes) = result?;
        total_time = std::cmp::max(total_time, duration);
        total_bytes += bytes;
        
        println!("Task {}: {}ms, {} bytes", task_id, duration.as_millis(), bytes);
    }
    
    let aggregate_throughput = (total_bytes as f64 / 1_048_576.0) / total_time.as_secs_f64();
    println!("Aggregate throughput: {:.1} MB/s", aggregate_throughput);
    
    Ok(())
}
```

### Load Testing

```rust
use stegano::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn load_test(duration_seconds: u64, requests_per_second: usize) -> Result<()> {
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(duration_seconds);
    
    let total_requests = Arc::new(AtomicUsize::new(0));
    let successful_requests = Arc::new(AtomicUsize::new(0));
    let failed_requests = Arc::new(AtomicUsize::new(0));
    
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / requests_per_second as u64));
    
    println!("Starting load test: {} RPS for {} seconds", requests_per_second, duration_seconds);
    
    while Instant::now() < end_time {
        interval.tick().await;
        
        let total = total_requests.clone();
        let successful = successful_requests.clone();
        let failed = failed_requests.clone();
        
        tokio::spawn(async move {
            total.fetch_add(1, Ordering::Relaxed);
            
            // Simulate typical embedding operation
            let data = vec![0u8; 10_240]; // 10KB
            let mut carrier = CarrierData::new(DataFormat::BINARY, data);
            let payload = vec![42u8; 100];
            let params = EmbedParams::new(b"load_test_key".to_vec());
            
            let registry = handlers::HandlerRegistry::new();
            let handler = registry.get_handler(&carrier.format).unwrap();
            
            match handler.embed(&mut carrier, &payload, &params).await {
                Ok(_) => { successful.fetch_add(1, Ordering::Relaxed); },
                Err(_) => { failed.fetch_add(1, Ordering::Relaxed); }
            }
        });
    }
    
    // Wait for remaining requests to complete
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let total = total_requests.load(Ordering::Relaxed);
    let successful = successful_requests.load(Ordering::Relaxed);
    let failed = failed_requests.load(Ordering::Relaxed);
    
    println!("Load test results:");
    println!("  Total requests: {}", total);
    println!("  Successful: {} ({:.1}%)", successful, successful as f64 / total as f64 * 100.0);
    println!("  Failed: {} ({:.1}%)", failed, failed as f64 / total as f64 * 100.0);
    println!("  Actual RPS: {:.1}", total as f64 / duration_seconds as f64);
    
    Ok(())
}
```

## Production Monitoring

### Performance Metrics Collection

```rust
use stegano::*;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct PerformanceMonitor {
    metrics: Mutex<HashMap<String, Vec<f64>>>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            metrics: Mutex::new(HashMap::new()),
        }
    }
    
    pub fn record_metric(&self, name: &str, value: f64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.entry(name.to_string()).or_insert_with(Vec::new).push(value);
    }
    
    pub fn get_statistics(&self, name: &str) -> Option<(f64, f64, f64)> {
        let metrics = self.metrics.lock().unwrap();
        if let Some(values) = metrics.get(name) {
            if values.is_empty() {
                return None;
            }
            
            let sum: f64 = values.iter().sum();
            let mean = sum / values.len() as f64;
            
            let mut sorted = values.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let median = sorted[sorted.len() / 2];
            
            let variance = values.iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>() / values.len() as f64;
            let std_dev = variance.sqrt();
            
            Some((mean, median, std_dev))
        } else {
            None
        }
    }
    
    pub fn generate_report(&self) -> String {
        let metrics = self.metrics.lock().unwrap();
        let mut report = String::from("Performance Report\n");
        report.push_str("==================\n\n");
        
        for (name, _) in metrics.iter() {
            if let Some((mean, median, std_dev)) = self.get_statistics(name) {
                report.push_str(&format!("{}: mean={:.2}, median={:.2}, std_dev={:.2}\n", 
                                        name, mean, median, std_dev));
            }
        }
        
        report
    }
}

async fn production_monitoring_example() -> Result<()> {
    let monitor = PerformanceMonitor::new();
    
    // Simulate production workload
    for i in 0..1000 {
        let data = vec![0u8; 10_240]; // 10KB files
        let mut carrier = CarrierData::new(DataFormat::BINARY, data);
        let payload = vec![42u8; 100];
        let params = EmbedParams::new(format!("prod_key_{}", i).into_bytes());
        
        let registry = handlers::HandlerRegistry::new();
        let handler = registry.get_handler(&carrier.format).unwrap();
        
        let start = Instant::now();
        let result = handler.embed(&mut carrier, &payload, &params).await?;
        let duration = start.elapsed();
        
        // Record metrics
        monitor.record_metric("embedding_time_ms", duration.as_millis() as f64);
        monitor.record_metric("throughput_mbps", 
                            (carrier.size() as f64 / 1_048_576.0) / duration.as_secs_f64());
        monitor.record_metric("memory_usage_mb", 
                            result.performance_stats.memory_usage_bytes as f64 / 1_048_576.0);
        
        if result.performance_stats.gpu_acceleration_used {
            monitor.record_metric("gpu_utilization", 1.0);
        } else {
            monitor.record_metric("gpu_utilization", 0.0);
        }
    }
    
    println!("{}", monitor.generate_report());
    Ok(())
}
```

## Optimization Strategies

### Algorithm Selection

```rust
use stegano::*;

fn select_optimal_parameters(file_size: usize, format: &DataFormat) -> EmbedParams {
    let base_key = b"optimization_key".to_vec();
    
    match (file_size, format) {
        // Large files benefit from GPU acceleration
        (size, _) if size > 10_485_760 => { // >10MB
            EmbedParams::new(base_key)
                .with_gpu()
                .with_stealth_level(StealthLevel::Low) // More aggressive for large files
                .with_error_correction(ECLevel::Medium)
        },
        
        // FASTQ files use specialized strategies
        (_, DataFormat::FASTQ) => {
            EmbedParams::new(base_key)
                .with_stealth_level(StealthLevel::High) // Preserve biological integrity
                .with_error_correction(ECLevel::High) // Genomics data needs protection
        },
        
        // Images can handle moderate embedding
        (_, format) if format.is_imagery() => {
            EmbedParams::new(base_key)
                .with_stealth_level(StealthLevel::Medium)
                .with_error_correction(ECLevel::Low) // Images have natural redundancy
        },
        
        // Audio signals are robust
        (_, format) if format.is_signal() => {
            EmbedParams::new(base_key)
                .with_stealth_level(StealthLevel::Low) // Audio masking allows aggressive embedding
                .with_error_correction(ECLevel::Low)
        },
        
        // Conservative defaults for other formats
        _ => {
            EmbedParams::new(base_key)
                .with_stealth_level(StealthLevel::High)
                .with_error_correction(ECLevel::Medium)
        }
    }
}
```

### Adaptive Performance Tuning

```rust
use stegano::*;

pub struct AdaptiveOptimizer {
    performance_history: Vec<(EmbedParams, Duration, bool)>, // params, time, success
}

impl AdaptiveOptimizer {
    pub fn new() -> Self {
        Self {
            performance_history: Vec::new(),
        }
    }
    
    pub fn optimize_parameters(&self, base_params: EmbedParams) -> EmbedParams {
        if self.performance_history.is_empty() {
            return base_params;
        }
        
        // Find the fastest successful configuration
        let best_config = self.performance_history.iter()
            .filter(|(_, _, success)| *success)
            .min_by_key(|(_, time, _)| time)
            .map(|(params, _, _)| params.clone());
        
        best_config.unwrap_or(base_params)
    }
    
    pub fn record_performance(&mut self, params: EmbedParams, duration: Duration, success: bool) {
        self.performance_history.push((params, duration, success));
        
        // Keep only recent history (last 100 operations)
        if self.performance_history.len() > 100 {
            self.performance_history.remove(0);
        }
    }
}

async fn adaptive_optimization_example() -> Result<()> {
    let mut optimizer = AdaptiveOptimizer::new();
    
    // Test different configurations
    let test_configs = vec![
        EmbedParams::new(b"test_key".to_vec()).with_stealth_level(StealthLevel::Low),
        EmbedParams::new(b"test_key".to_vec()).with_stealth_level(StealthLevel::Medium),
        EmbedParams::new(b"test_key".to_vec()).with_stealth_level(StealthLevel::High),
        EmbedParams::new(b"test_key".to_vec()).with_gpu(),
    ];
    
    for params in test_configs {
        let data = vec![0u8; 100_000]; // 100KB test
        let mut carrier = CarrierData::new(DataFormat::BINARY, data);
        let payload = vec![42u8; 1000];
        
        let registry = handlers::HandlerRegistry::new();
        let handler = registry.get_handler(&carrier.format).unwrap();
        
        let start = Instant::now();
        let result = handler.embed(&mut carrier, &payload, &params).await;
        let duration = start.elapsed();
        
        let success = result.is_ok();
        optimizer.record_performance(params, duration, success);
        
        if success {
            println!("Configuration succeeded in {}ms", duration.as_millis());
        } else {
            println!("Configuration failed");
        }
    }
    
    // Use optimized parameters for production
    let base_params = EmbedParams::new(b"production_key".to_vec());
    let optimized_params = optimizer.optimize_parameters(base_params);
    
    println!("Using optimized parameters for production workload");
    
    Ok(())
}
```

## Conclusion

This performance guide provides comprehensive benchmarking and optimization strategies for the Stegano system. Key recommendations:

1. **Use GPU acceleration** for files >10MB
2. **Choose appropriate stealth levels** based on format robustness
3. **Monitor performance metrics** in production environments
4. **Implement adaptive optimization** for varying workloads
5. **Use streaming processing** for very large files (>1GB)
6. **Profile memory usage** to prevent resource exhaustion

For production deployments, implement comprehensive monitoring and use the adaptive optimization patterns to automatically tune performance based on your specific workload characteristics.