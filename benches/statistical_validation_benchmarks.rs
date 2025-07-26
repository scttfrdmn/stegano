//! Statistical Validation Performance Benchmarks
//!
//! Comprehensive benchmarking suite for statistical validation components,
//! measuring performance of all validation algorithms and tests.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use stegano::core::*;
use stegano::validation::*;

/// Generate test data for benchmarks
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i * 137 + 42) % 256) as u8).collect()
}

/// Generate realistic image data
fn generate_image_data(width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            // Create gradient with some noise
            let base = ((x + y) * 255 / (width + height)) as u8;
            let noise = ((x * y * 7) % 16) as u8;
            data.push(base.saturating_add(noise));
        }
    }
    data
}

/// Generate audio test data
fn generate_audio_data(sample_rate: usize, duration_secs: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Mix of sine waves
            0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
                + 0.1 * (rand::random::<f32>() - 0.5) // Add noise
        })
        .collect()
}

/// Benchmark classical statistical tests
fn bench_classical_tests(c: &mut Criterion) {
    let mut group = c.benchmark_group("classical_statistical_tests");

    let validation_suite = StatisticalValidationSuite::new();

    for size in [1000, 5000, 10000, 50000].iter() {
        let data1: Vec<f64> = generate_test_data(*size)
            .iter()
            .map(|&x| x as f64)
            .collect();
        let data2: Vec<f64> = generate_test_data(*size)
            .iter()
            .map(|&x| (x + 1) as f64)
            .collect();

        group.throughput(Throughput::Elements(*size as u64));

        // Chi-square test benchmark
        group.bench_with_input(BenchmarkId::new("chi_square_test", size), size, |b, _| {
            b.iter(|| {
                validation_suite
                    .classical_tests
                    .chi_square_test(black_box(&data1), black_box(&data2))
                    .unwrap()
            })
        });

        // Kolmogorov-Smirnov test benchmark
        group.bench_with_input(
            BenchmarkId::new("kolmogorov_smirnov_test", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .classical_tests
                        .kolmogorov_smirnov_test(black_box(&data1), black_box(&data2))
                        .unwrap()
                })
            },
        );

        // Anderson-Darling test benchmark
        group.bench_with_input(
            BenchmarkId::new("anderson_darling_test", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .classical_tests
                        .anderson_darling_test(black_box(&data1))
                        .unwrap()
                })
            },
        );

        // Autocorrelation analysis benchmark
        group.bench_with_input(
            BenchmarkId::new("autocorrelation_analysis", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .classical_tests
                        .autocorrelation_analysis(black_box(&data1), black_box(20))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark runs test for different sequence lengths
fn bench_runs_test(c: &mut Criterion) {
    let mut group = c.benchmark_group("runs_test");

    let validation_suite = StatisticalValidationSuite::new();

    for size in [1000, 5000, 10000, 50000].iter() {
        let binary_sequence: Vec<bool> = generate_test_data(*size)
            .iter()
            .map(|&x| x & 1 == 1)
            .collect();

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("runs_test", size), size, |b, _| {
            b.iter(|| {
                validation_suite
                    .classical_tests
                    .runs_test(black_box(&binary_sequence))
                    .unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark advanced steganalysis methods
fn bench_steganalysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("advanced_steganalysis");

    let validation_suite = StatisticalValidationSuite::new();

    // Pixel pair analysis benchmark
    for size in [256, 512, 1024].iter() {
        let image_data = generate_image_data(*size, *size);

        group.throughput(Throughput::Elements((size * size) as u64));
        group.bench_with_input(
            BenchmarkId::new("pixel_pair_analysis", format!("{}x{}", size, size)),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .steganalysis
                        .pixel_pair_analysis(
                            black_box(&image_data),
                            black_box(*size),
                            black_box(*size),
                        )
                        .unwrap()
                })
            },
        );
    }

    // Wavelet structural analysis benchmark
    for size in [1024, 4096, 16384].iter() {
        let data = generate_test_data(*size);

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("wavelet_structural_analysis", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .steganalysis
                        .wavelet_structural_analysis(black_box(&data))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark audio steganalysis
fn bench_audio_steganalysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_steganalysis");

    let validation_suite = StatisticalValidationSuite::new();

    for duration in [0.1, 0.5, 1.0, 2.0].iter() {
        let audio_data = generate_audio_data(44100, *duration);
        let sample_count = audio_data.len();

        group.throughput(Throughput::Elements(sample_count as u64));
        group.bench_with_input(
            BenchmarkId::new("audio_steganalysis", format!("{:.1}s", duration)),
            &sample_count,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .steganalysis
                        .audio_steganalysis(black_box(&audio_data), black_box(44100.0))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark ROC analysis performance
fn bench_roc_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("roc_analysis");

    let validation_suite = StatisticalValidationSuite::new();

    for size in [1000, 5000, 10000].iter() {
        // Generate realistic binary classification data
        let true_labels: Vec<bool> = (0..*size)
            .map(|i| (i * 137 + 42) % 3 == 0) // ~33% positive
            .collect();

        let predicted_scores: Vec<f64> = (0..*size)
            .map(|i| {
                let base_score = if true_labels[i] { 0.7 } else { 0.3 };
                let noise = ((i * 7919) % 1000) as f64 / 2500.0; // ±0.4 noise
                (base_score + noise - 0.2).max(0.0).min(1.0)
            })
            .collect();

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("roc_analysis", size), size, |b, _| {
            b.iter(|| {
                validation_suite
                    .performance_analysis
                    .roc_analysis(black_box(&true_labels), black_box(&predicted_scores))
                    .unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark comprehensive validation suite
fn bench_comprehensive_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("comprehensive_validation");

    let validation_suite = StatisticalValidationSuite::new();

    for size in [1024, 4096, 16384].iter() {
        let original_data = generate_test_data(*size);
        let stego_data = generate_test_data(*size + 1); // Slightly different
        let stego_data = &stego_data[0..*size]; // Ensure same size

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("comprehensive_validation", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .comprehensive_validation(
                            black_box(&original_data),
                            black_box(stego_data),
                            black_box(&DataFormat::BINARY),
                        )
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark test dataset generation
fn bench_test_datasets(c: &mut Criterion) {
    let mut group = c.benchmark_group("test_datasets");

    let dataset_manager = TestDatasetManager::new();

    // Benchmark dataset loading
    group.bench_function("load_standard_datasets", |b| {
        b.iter(|| {
            let mut manager = TestDatasetManager::new();
            let result = manager.load_standard_datasets().unwrap();
            black_box(result);
        })
    });

    // Benchmark synthetic PNG creation
    for size in [128, 256, 512].iter() {
        group.throughput(Throughput::Elements((size * size * 3) as u64)); // RGB pixels
        group.bench_with_input(
            BenchmarkId::new("synthetic_png_creation", format!("{}x{}", size, size)),
            size,
            |b, _| {
                b.iter(|| {
                    black_box(dataset_manager.create_synthetic_png(*size, *size).unwrap());
                })
            },
        );
    }

    // Benchmark synthetic audio creation
    for duration in [0.5, 1.0, 2.0].iter() {
        let sample_count = (44100.0 * duration) as usize;
        group.throughput(Throughput::Elements(sample_count as u64));
        group.bench_with_input(
            BenchmarkId::new("synthetic_audio_creation", format!("{:.1}s", duration)),
            &sample_count,
            |b, _| {
                b.iter(|| {
                    black_box(
                        dataset_manager
                            .create_synthetic_audio(44100, *duration)
                            .unwrap(),
                    );
                })
            },
        );
    }

    group.finish();
}

/// Benchmark cross-validation framework
fn bench_cross_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_validation");

    let validation_suite = StatisticalValidationSuite::new();

    for size in [100, 500, 1000].iter() {
        // Generate synthetic dataset for cross-validation
        let data: Vec<(Vec<f64>, bool)> = (0..*size)
            .map(|i| {
                let features = vec![
                    (i as f64) / (*size as f64),
                    ((i * 7) % 100) as f64 / 100.0,
                    ((i * 13) % 50) as f64 / 50.0,
                ];
                let label = i % 3 == 0;
                (features, label)
            })
            .collect();

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("k_fold_validation", size), size, |b, _| {
            b.iter(|| {
                // Simple model function for testing
                let model_fn = |_train_data: &[(Vec<f64>, bool)],
                                test_data: &[(Vec<f64>, bool)]|
                 -> Result<f64> {
                    // Dummy model that returns accuracy based on first feature
                    let correct = test_data
                        .iter()
                        .filter(|(features, label)| (features[0] > 0.5) == *label)
                        .count();
                    Ok(correct as f64 / test_data.len() as f64)
                };

                validation_suite
                    .performance_analysis
                    .cross_validation
                    .k_fold_validation(black_box(&data), model_fn)
                    .unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark scaling characteristics of validation algorithms
fn bench_algorithm_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("algorithm_scaling");
    group.sample_size(20); // Reduce sample size for expensive operations

    let validation_suite = StatisticalValidationSuite::new();

    // Test how algorithms scale with input size
    let sizes = [1000, 2000, 4000, 8000, 16000];

    for &size in &sizes {
        let data: Vec<f64> = generate_test_data(size).iter().map(|&x| x as f64).collect();

        group.throughput(Throughput::Elements(size as u64));

        // Chi-square scaling
        group.bench_with_input(
            BenchmarkId::new("chi_square_scaling", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let expected: Vec<f64> = data.iter().map(|x| x + 1.0).collect();
                    validation_suite
                        .classical_tests
                        .chi_square_test(black_box(&data), black_box(&expected))
                        .unwrap()
                })
            },
        );

        // KS test scaling
        group.bench_with_input(BenchmarkId::new("ks_test_scaling", size), &size, |b, _| {
            b.iter(|| {
                let data2: Vec<f64> = data.iter().map(|x| x + 0.5).collect();
                validation_suite
                    .classical_tests
                    .kolmogorov_smirnov_test(black_box(&data), black_box(&data2))
                    .unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark memory efficiency of validation operations
fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");

    let validation_suite = StatisticalValidationSuite::new();

    // Large dataset benchmarks to test memory efficiency
    for size in [50000, 100000].iter() {
        let data: Vec<f64> = generate_test_data(*size)
            .iter()
            .map(|&x| x as f64)
            .collect();

        group.throughput(Throughput::Elements(*size as u64));

        // Memory-intensive operations
        group.bench_with_input(
            BenchmarkId::new("large_dataset_ks_test", size),
            size,
            |b, _| {
                b.iter(|| {
                    let data2: Vec<f64> = data.iter().map(|x| x + 1.0).collect();
                    validation_suite
                        .classical_tests
                        .kolmogorov_smirnov_test(black_box(&data), black_box(&data2))
                        .unwrap()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("large_dataset_autocorrelation", size),
            size,
            |b, _| {
                b.iter(|| {
                    validation_suite
                        .classical_tests
                        .autocorrelation_analysis(black_box(&data), black_box(50))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_classical_tests,
    bench_runs_test,
    bench_steganalysis,
    bench_audio_steganalysis,
    bench_roc_analysis,
    bench_comprehensive_validation,
    bench_test_datasets,
    bench_cross_validation,
    bench_algorithm_scaling,
    bench_memory_efficiency
);

criterion_main!(benches);
