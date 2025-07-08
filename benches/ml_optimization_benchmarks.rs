//! Performance benchmarks for ML optimization algorithms
//!
//! Comprehensive benchmarking suite to measure performance characteristics
//! of all optimization algorithms across different scenarios.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;
use std::time::Duration;
use stegano::core::*;
use stegano::ml::bayesian::*;
use stegano::ml::optimization::{
    AcquisitionFunction as OptAcquisitionFunction, MultiObjectiveConfig, ObjectiveFunction,
    OptimizationAlgorithm, OptimizationConfig, OptimizationEngine, ParallelOptimizationConfig,
    ParetoMethod,
};
use stegano::ml::reinforcement::*;
use tokio::runtime::Runtime;

/// Benchmark configuration for different scenarios
#[derive(Clone)]
struct BenchmarkConfig {
    name: &'static str,
    data_size: usize,
    payload_size: usize,
    max_iterations: usize,
    population_size: usize,
}

impl BenchmarkConfig {
    fn small() -> Self {
        Self {
            name: "small",
            data_size: 1024,  // 1KB
            payload_size: 64, // 64 bytes
            max_iterations: 20,
            population_size: 10,
        }
    }

    fn medium() -> Self {
        Self {
            name: "medium",
            data_size: 10240,  // 10KB
            payload_size: 256, // 256 bytes
            max_iterations: 50,
            population_size: 30,
        }
    }

    fn large() -> Self {
        Self {
            name: "large",
            data_size: 102400,  // 100KB
            payload_size: 1024, // 1KB
            max_iterations: 100,
            population_size: 50,
        }
    }
}

/// Test data generator for benchmarks
fn create_benchmark_data(size: usize) -> CarrierData {
    let mut data = Vec::with_capacity(size);

    // Create realistic data pattern (not just zeros)
    for i in 0..size {
        data.push(((i * 137 + 42) % 256) as u8); // Pseudo-random pattern
    }

    CarrierData::new(data, DataFormat::BINARY)
}

/// Simple objective function for benchmarking
#[derive(Debug)]
struct BenchmarkObjective {
    weight: f32,
}

impl ObjectiveFunction for BenchmarkObjective {
    fn evaluate(
        &self,
        params: &EmbedParams,
        _carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<f32> {
        // Simulate some computation cost
        let mut score = 0.5;

        // Add some complexity based on parameters
        score += match params.stealth_level {
            StealthLevel::Maximum => 0.4,
            StealthLevel::High => 0.3,
            StealthLevel::Medium => 0.2,
            StealthLevel::Low => 0.1,
        };

        score += match params.error_correction_level {
            ECLevel::High => 0.2,
            ECLevel::Medium => 0.15,
            ECLevel::Low => 0.1,
        };

        // Simulate variable computation time
        let iterations = (score * 1000.0) as usize;
        for _ in 0..iterations {
            score = (score * 1.001).min(1.0);
        }

        Ok(score * self.weight)
    }

    fn name(&self) -> &str {
        "benchmark_objective"
    }

    fn maximize(&self) -> bool {
        true
    }

    fn weight(&self) -> f32 {
        self.weight
    }
}

/// Benchmark genetic algorithm optimization
fn bench_genetic_algorithm(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let configs = vec![
        BenchmarkConfig::small(),
        BenchmarkConfig::medium(),
        BenchmarkConfig::large(),
    ];

    let mut group = c.benchmark_group("genetic_algorithm");

    for config in configs {
        group.bench_with_input(
            BenchmarkId::new("genetic_algorithm", config.name),
            &config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(config.data_size);
                    let payload = vec![0x42u8; config.payload_size];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                            mutation_rate: 0.1,
                            crossover_rate: 0.7,
                        },
                        max_iterations: config.max_iterations,
                        population_size: config.population_size,
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: false, // Single-threaded for consistent benchmarks
                            num_workers: 1,
                            batch_size: 1,
                        },
                        multi_objective: MultiObjectiveConfig::default(),
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                    black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark particle swarm optimization
fn bench_particle_swarm(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let configs = vec![
        BenchmarkConfig::small(),
        BenchmarkConfig::medium(),
        BenchmarkConfig::large(),
    ];

    let mut group = c.benchmark_group("particle_swarm");

    for config in configs {
        group.bench_with_input(
            BenchmarkId::new("particle_swarm", config.name),
            &config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(config.data_size);
                    let payload = vec![0x42u8; config.payload_size];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::ParticleSwarm {
                            inertia: 0.9,
                            cognitive: 2.0,
                            social: 2.0,
                        },
                        max_iterations: config.max_iterations,
                        population_size: config.population_size,
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: false,
                            num_workers: 1,
                            batch_size: 1,
                        },
                        multi_objective: MultiObjectiveConfig::default(),
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                    black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark Bayesian optimization
fn bench_bayesian_optimization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let configs = vec![
        BenchmarkConfig::small(),
        BenchmarkConfig::medium(),
        // Note: Bayesian optimization scales poorly, so we skip large for this benchmark
    ];

    let mut group = c.benchmark_group("bayesian_optimization");

    for config in configs {
        group.bench_with_input(
            BenchmarkId::new("bayesian_optimization", config.name),
            &config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(config.data_size);
                    let payload = vec![0x42u8; config.payload_size];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::BayesianOptimization {
                            acquisition_function: OptAcquisitionFunction::ExpectedImprovement,
                        },
                        max_iterations: (config.max_iterations / 2).max(10), // Bayesian needs fewer iterations
                        population_size: (config.population_size / 3).max(5), // Smaller initial sample
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: false,
                            num_workers: 1,
                            batch_size: 1,
                        },
                        multi_objective: MultiObjectiveConfig::default(),
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                    black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark multi-objective optimization
fn bench_multi_objective(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let configs = vec![BenchmarkConfig::small(), BenchmarkConfig::medium()];

    let mut group = c.benchmark_group("multi_objective");

    for config in configs {
        group.bench_with_input(
            BenchmarkId::new("multi_objective", config.name),
            &config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(config.data_size);
                    let payload = vec![0x42u8; config.payload_size];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                            mutation_rate: 0.12,
                            crossover_rate: 0.75,
                        },
                        max_iterations: config.max_iterations,
                        population_size: config.population_size,
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: false,
                            num_workers: 1,
                            batch_size: 1,
                        },
                        multi_objective: MultiObjectiveConfig {
                            enabled: true,
                            objective_weights: HashMap::new(),
                            pareto_method: ParetoMethod::NSGA2,
                            max_pareto_solutions: 20,
                        },
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 0.8 }));

                    black_box(
                        optimizer
                            .multi_objective_optimization(&carrier, &payload)
                            .await
                            .unwrap(),
                    )
                });
            },
        );
    }

    group.finish();
}

/// Benchmark Gaussian Process operations
fn bench_gaussian_process(c: &mut Criterion) {
    use ndarray::{Array1, Array2};

    let mut group = c.benchmark_group("gaussian_process");

    let dimensions = vec![2, 5, 10];
    let sample_sizes = vec![10, 50, 100];

    for &dim in &dimensions {
        for &samples in &sample_sizes {
            if samples > 100 && dim > 5 {
                continue; // Skip expensive combinations
            }

            group.bench_with_input(
                BenchmarkId::new(
                    format!("gp_fit_d{}_s{}", dim, samples),
                    format!("{}x{}", dim, samples),
                ),
                &(dim, samples),
                |b, &(dim, samples)| {
                    let kernel = KernelFunction::RBF {
                        length_scale: 1.0,
                        signal_variance: 1.0,
                    };

                    // Generate training data
                    let mut x_data = Vec::new();
                    let mut y_data = Vec::new();

                    for i in 0..samples {
                        for j in 0..dim {
                            x_data.push((i as f64 + j as f64 * 0.1) / samples as f64);
                        }
                        y_data.push((i as f64).sin() * 0.5 + 0.5);
                    }

                    let x_train = Array2::from_shape_vec((samples, dim), x_data).unwrap();
                    let y_train = Array1::from_vec(y_data);

                    b.iter(|| {
                        let mut gp = GaussianProcess::new(KernelFunction::RBF {
                            length_scale: 1.0,
                            signal_variance: 1.0,
                        });

                        black_box(gp.fit(&x_train, &y_train).unwrap())
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark different acquisition functions
fn bench_acquisition_functions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let acquisition_functions = vec![
        (
            "expected_improvement",
            OptAcquisitionFunction::ExpectedImprovement,
        ),
        (
            "upper_confidence_bound",
            OptAcquisitionFunction::UpperConfidenceBound { kappa: 2.576 },
        ),
        (
            "probability_improvement",
            OptAcquisitionFunction::ProbabilityOfImprovement,
        ),
    ];

    let mut group = c.benchmark_group("acquisition_functions");

    for (name, function) in acquisition_functions {
        group.bench_with_input(
            BenchmarkId::new("acquisition", name),
            &function,
            |b, function| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(2048);
                    let payload = vec![0x42u8; 128];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::BayesianOptimization {
                            acquisition_function: function.clone(),
                        },
                        max_iterations: 15,
                        population_size: 8,
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: false,
                            num_workers: 1,
                            batch_size: 1,
                        },
                        multi_objective: MultiObjectiveConfig::default(),
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                    black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark reinforcement learning agent operations
fn bench_reinforcement_learning(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("reinforcement_learning");

    // Benchmark Q-table updates
    group.bench_function("q_table_update", |b| {
        b.to_async(&rt).iter(|| async {
            let config = RLConfig {
                learning_rate: 0.1,
                discount_factor: 0.9,
                exploration_config: ExplorationConfig {
                    initial_epsilon: 0.3,
                    final_epsilon: 0.05,
                    decay_rate: 0.99,
                    strategy: ExplorationStrategy::EpsilonGreedy,
                },
                max_episodes: 100,
                episode_length: 20,
                experience_buffer_size: 1000,
                batch_size: 32,
                target_update_frequency: 10,
            };

            let mut agent = RLAgent::new(config);

            let state = RLState {
                data_size: 1024,
                data_type: DataFormat::BINARY,
                complexity_score: 0.5,
                capacity_ratio: 0.3,
            };

            // Perform multiple Q-table updates
            for i in 0..100 {
                let action = OptimizationAlgorithmType::GeneticAlgorithm;
                let reward = (i as f64 / 100.0) * 0.8 + 0.1;
                let next_state = state.clone();

                black_box(agent.update_q_value(state.clone(), action, reward, next_state));
            }
        });
    });

    // Benchmark strategy selection
    group.bench_function("strategy_selection", |b| {
        b.to_async(&rt).iter(|| async {
            let config = RLConfig {
                learning_rate: 0.1,
                discount_factor: 0.9,
                exploration_config: ExplorationConfig {
                    initial_epsilon: 0.1, // Low exploration for consistent benchmarks
                    final_epsilon: 0.01,
                    decay_rate: 0.99,
                    strategy: ExplorationStrategy::EpsilonGreedy,
                },
                max_episodes: 100,
                episode_length: 20,
                experience_buffer_size: 1000,
                batch_size: 32,
                target_update_frequency: 10,
            };

            let mut agent = RLAgent::new(config);

            // Pre-populate some Q-values
            let state = RLState {
                data_size: 2048,
                data_type: DataFormat::BINARY,
                complexity_score: 0.4,
                capacity_ratio: 0.6,
            };

            agent.update_q_value(
                state.clone(),
                OptimizationAlgorithmType::GeneticAlgorithm,
                0.8,
                state.clone(),
            );

            // Benchmark strategy selection
            for _ in 0..50 {
                black_box(agent.select_strategy(&state).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark parallel vs sequential optimization
fn bench_parallel_optimization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("parallel_optimization");

    let configs = vec![
        ("sequential", false, 1),
        ("parallel_2", true, 2),
        ("parallel_4", true, 4),
    ];

    for (name, parallel_enabled, num_workers) in configs {
        group.bench_with_input(
            BenchmarkId::new("parallel", name),
            &(parallel_enabled, num_workers),
            |b, &(parallel_enabled, num_workers)| {
                b.to_async(&rt).iter(|| async {
                    let carrier = create_benchmark_data(5120);
                    let payload = vec![0x42u8; 256];

                    let opt_config = OptimizationConfig {
                        algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                            mutation_rate: 0.1,
                            crossover_rate: 0.7,
                        },
                        max_iterations: 30,
                        population_size: 40,
                        tolerance: 0.01,
                        parallel_config: ParallelOptimizationConfig {
                            enabled: parallel_enabled,
                            num_workers,
                            batch_size: 10,
                        },
                        multi_objective: MultiObjectiveConfig::default(),
                    };

                    let mut optimizer = OptimizationEngine::new(opt_config);
                    optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                    black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark algorithm scaling with problem size
fn bench_algorithm_scaling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("algorithm_scaling");
    group.sample_size(10); // Fewer samples for scaling tests

    let problem_sizes = vec![
        ("tiny", 512, 32, 10, 8),
        ("small", 2048, 128, 20, 16),
        ("medium", 8192, 512, 40, 32),
    ];

    let algorithms = vec![
        (
            "genetic",
            OptimizationAlgorithm::GeneticAlgorithm {
                mutation_rate: 0.1,
                crossover_rate: 0.7,
            },
        ),
        (
            "particle_swarm",
            OptimizationAlgorithm::ParticleSwarm {
                inertia: 0.9,
                cognitive: 2.0,
                social: 2.0,
            },
        ),
    ];

    for (size_name, data_size, payload_size, max_iter, pop_size) in problem_sizes {
        for (alg_name, algorithm) in &algorithms {
            group.bench_with_input(
                BenchmarkId::new(
                    format!("{}_{}", alg_name, size_name),
                    format!("{}_{}", alg_name, size_name),
                ),
                &(
                    algorithm.clone(),
                    data_size,
                    payload_size,
                    max_iter,
                    pop_size,
                ),
                |b, (algorithm, data_size, payload_size, max_iter, pop_size)| {
                    b.to_async(&rt).iter(|| async {
                        let carrier = create_benchmark_data(*data_size);
                        let payload = vec![0x42u8; *payload_size];

                        let opt_config = OptimizationConfig {
                            algorithm: algorithm.clone(),
                            max_iterations: *max_iter,
                            population_size: *pop_size,
                            tolerance: 0.02, // Looser tolerance for scaling tests
                            parallel_config: ParallelOptimizationConfig {
                                enabled: false,
                                num_workers: 1,
                                batch_size: 1,
                            },
                            multi_objective: MultiObjectiveConfig::default(),
                        };

                        let mut optimizer = OptimizationEngine::new(opt_config);
                        optimizer.add_objective(Box::new(BenchmarkObjective { weight: 1.0 }));

                        black_box(optimizer.optimize(&carrier, &payload, None).await.unwrap())
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark kernel computations for different kernels
fn bench_kernel_computations(c: &mut Criterion) {
    use ndarray::Array2;

    let mut group = c.benchmark_group("kernel_computations");

    let matrix_sizes = vec![10, 50, 100];
    let dimensions = vec![2, 5];

    for &size in &matrix_sizes {
        for &dim in &dimensions {
            if size > 50 && dim > 2 {
                continue; // Skip expensive combinations
            }

            // Generate test data
            let mut data = Vec::new();
            for i in 0..(size * dim) {
                data.push((i as f64) / (size * dim) as f64);
            }
            let x1 = Array2::from_shape_vec((size, dim), data.clone()).unwrap();
            let x2 = Array2::from_shape_vec((size, dim), data).unwrap();

            // Benchmark RBF kernel
            group.bench_with_input(
                BenchmarkId::new(format!("rbf_{}x{}", size, dim), format!("{}x{}", size, dim)),
                &(&x1, &x2),
                |b, (x1, x2)| {
                    let kernel = KernelFunction::RBF {
                        length_scale: 1.0,
                        signal_variance: 1.0,
                    };
                    b.iter(|| black_box(kernel.compute(x1, x2)));
                },
            );

            // Benchmark Matern kernel
            group.bench_with_input(
                BenchmarkId::new(
                    format!("matern_{}x{}", size, dim),
                    format!("{}x{}", size, dim),
                ),
                &(&x1, &x2),
                |b, (x1, x2)| {
                    let kernel = KernelFunction::Matern32 {
                        length_scale: 1.0,
                        signal_variance: 1.0,
                    };
                    b.iter(|| black_box(kernel.compute(x1, x2)));
                },
            );
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_genetic_algorithm,
    bench_particle_swarm,
    bench_bayesian_optimization,
    bench_multi_objective,
    bench_gaussian_process,
    bench_acquisition_functions,
    bench_reinforcement_learning,
    bench_parallel_optimization,
    bench_algorithm_scaling,
    bench_kernel_computations
);

criterion_main!(benches);
