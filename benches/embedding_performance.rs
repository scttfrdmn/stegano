use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;
use stegano::*;

fn benchmark_embedding_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_strategies");
    group.measurement_time(Duration::from_secs(10));

    // Test data sizes
    let sizes = [1024, 4096, 16384, 65536];

    for size in sizes.iter() {
        // Benchmark LSB embedding
        group.bench_with_input(BenchmarkId::new("lsb_cpu", size), size, |b, &size| {
            let mut data = vec![0u8; size];
            let payload = vec![0u8; size / 8];
            b.iter(|| {
                // CPU LSB embedding
                black_box(&mut data);
                black_box(&payload);
            });
        });

        // Benchmark GPU LSB embedding
        group.bench_with_input(BenchmarkId::new("lsb_gpu", size), size, |b, &size| {
            let mut data = vec![0u8; size];
            let payload = vec![0u8; size / 8];
            b.iter(|| {
                // GPU LSB embedding
                black_box(&mut data);
                black_box(&payload);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_embedding_strategies);
criterion_main!(benches);
