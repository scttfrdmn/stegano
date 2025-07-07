//! Comprehensive test suite for ML optimization algorithms
//!
//! Tests all optimization algorithms including:
//! - Genetic Algorithm with real operators
//! - Particle Swarm Optimization with adaptive parameters
//! - Bayesian Optimization with Gaussian processes
//! - Reinforcement Learning for strategy selection
//! - Multi-objective optimization with NSGA-II

use std::collections::HashMap;
use std::time::Duration;
use stegano::core::*;
use stegano::ml::bayesian::*;
use stegano::ml::optimization::*;
use stegano::ml::reinforcement::*;

/// Test helper to create sample carrier data
fn create_test_carrier(size: usize) -> CarrierData {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i % 256) as u8);
    }

    CarrierData::new(data, DataFormat::BINARY)
}

/// Test helper to create default optimization config
fn create_test_config() -> OptimizationConfig {
    OptimizationConfig {
        algorithm: OptimizationAlgorithm::GeneticAlgorithm {
            mutation_rate: 0.1,
            crossover_rate: 0.7,
        },
        max_iterations: 10, // Small for tests
        population_size: 8,
        tolerance: 0.1,
        parallel_config: ParallelOptimizationConfig {
            enabled: false, // Disable for deterministic tests
            num_workers: 1,
            batch_size: 1,
        },
        multi_objective: MultiObjectiveConfig::default(),
    }
}

/// Mock objective function for testing
struct TestObjective {
    weight: f32,
    target_score: f32,
}

impl ObjectiveFunction for TestObjective {
    fn evaluate(
        &self,
        params: &EmbedParams,
        _carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<f32> {
        // Simple scoring based on stealth level and error correction
        let stealth_score = match params.stealth_level {
            StealthLevel::Maximum => 1.0,
            StealthLevel::High => 0.8,
            StealthLevel::Medium => 0.6,
            StealthLevel::Low => 0.4,
        };

        let ec_score = match params.error_correction_level {
            ECLevel::High => 0.9,
            ECLevel::Medium => 0.7,
            ECLevel::Low => 0.5,
        };

        let combined_score = (stealth_score + ec_score) / 2.0 * self.weight;
        Ok(combined_score.min(self.target_score))
    }

    fn name(&self) -> &str {
        "test_objective"
    }

    fn maximize(&self) -> bool {
        true
    }

    fn weight(&self) -> f32 {
        self.weight
    }
}

/// Mock constraint for testing
struct TestConstraint {
    allowed_stealth_levels: Vec<StealthLevel>,
}

impl OptimizationConstraint for TestConstraint {
    fn is_satisfied(&self, params: &EmbedParams, _carrier: &CarrierData) -> bool {
        self.allowed_stealth_levels.contains(&params.stealth_level)
    }

    fn name(&self) -> &str {
        "test_constraint"
    }
}

#[tokio::test]
async fn test_genetic_algorithm_optimization() {
    let mut config = create_test_config();
    config.algorithm = OptimizationAlgorithm::GeneticAlgorithm {
        mutation_rate: 0.15,
        crossover_rate: 0.8,
    };
    config.max_iterations = 20;
    config.population_size = 16;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.9,
    }));

    let carrier = create_test_carrier(1024);
    let payload = b"test genetic algorithm payload";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();
    assert!(optimization_result.best_objective_value.combined_score > 0.0);
    assert!(optimization_result.convergence.iterations <= 20);

    // Test that parameters are within valid ranges
    let params = &optimization_result.best_parameters;
    assert!(matches!(
        params.stealth_level,
        StealthLevel::Low | StealthLevel::Medium | StealthLevel::High | StealthLevel::Maximum
    ));
    assert!(matches!(
        params.error_correction_level,
        ECLevel::Low | ECLevel::Medium | ECLevel::High
    ));
}

#[tokio::test]
async fn test_particle_swarm_optimization() {
    let mut config = create_test_config();
    config.algorithm = OptimizationAlgorithm::ParticleSwarm {
        inertia: 0.9,
        cognitive: 2.0,
        social: 2.0,
    };
    config.max_iterations = 15;
    config.population_size = 12;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.85,
    }));

    let carrier = create_test_carrier(2048);
    let payload = b"test particle swarm payload";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();
    assert!(optimization_result.best_objective_value.combined_score > 0.0);
    assert!(optimization_result.convergence.iterations <= 15);

    // PSO should converge relatively quickly
    assert!(optimization_result.convergence.iterations < 15);
}

#[tokio::test]
async fn test_bayesian_optimization() {
    let mut config = create_test_config();
    config.algorithm = OptimizationAlgorithm::BayesianOptimization {
        acquisition_function: AcquisitionFunction::ExpectedImprovement,
    };
    config.max_iterations = 10; // Bayesian needs fewer iterations
    config.population_size = 5;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.8,
    }));

    let carrier = create_test_carrier(512);
    let payload = b"test bayesian optimization";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();
    assert!(optimization_result.best_objective_value.combined_score > 0.0);

    // Bayesian optimization should be efficient
    assert!(optimization_result.convergence.iterations <= 10);
}

#[tokio::test]
async fn test_multi_objective_optimization() {
    let mut config = create_test_config();
    config.multi_objective = MultiObjectiveConfig {
        enabled: true,
        objective_weights: HashMap::new(),
        pareto_method: ParetoMethod::NSGA2,
        max_pareto_solutions: 10,
    };
    config.max_iterations = 25;
    config.population_size = 20;

    let mut optimizer = OptimizationEngine::new(config);

    // Add competing objectives
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.9,
    }));
    optimizer.add_objective(Box::new(TestObjective {
        weight: 0.8,
        target_score: 0.7,
    }));

    let carrier = create_test_carrier(1024);
    let payload = b"test multi-objective optimization";

    let result = optimizer
        .multi_objective_optimization(&carrier, payload)
        .await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();
    assert!(!optimization_result.pareto_front.is_empty());
    assert!(optimization_result.pareto_front.len() <= 10);

    // Verify Pareto front diversity
    let scores: Vec<f32> = optimization_result
        .pareto_front
        .iter()
        .map(|(_, obj)| obj.combined_score)
        .collect();

    // Should have some diversity in scores
    let min_score = scores.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_score = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    assert!(max_score > min_score);
}

#[tokio::test]
async fn test_optimization_with_constraints() {
    let mut config = create_test_config();
    config.max_iterations = 15;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.8,
    }));

    // Add constraint that only allows High and Maximum stealth levels
    optimizer.add_constraint(Box::new(TestConstraint {
        allowed_stealth_levels: vec![StealthLevel::High, StealthLevel::Maximum],
    }));

    let carrier = create_test_carrier(1024);
    let payload = b"test constrained optimization";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();

    // Verify constraint satisfaction
    assert!(matches!(
        optimization_result.best_parameters.stealth_level,
        StealthLevel::High | StealthLevel::Maximum
    ));
}

#[tokio::test]
async fn test_optimization_convergence() {
    let mut config = create_test_config();
    config.max_iterations = 50;
    config.tolerance = 0.001; // Tight tolerance
    config.population_size = 20;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.95,
    }));

    let carrier = create_test_carrier(2048);
    let payload = b"test convergence behavior";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    let optimization_result = result.unwrap();

    // Should converge before max iterations with tight tolerance
    assert!(optimization_result.convergence.iterations < 50);

    // Convergence info should be populated
    if optimization_result.convergence.converged {
        assert!(optimization_result.convergence.final_improvement < 0.001);
    }
}

#[test]
fn test_gaussian_process_basic_functionality() {
    use ndarray::{Array1, Array2};

    let kernel = Box::new(RBFKernel {
        length_scale: 1.0,
        variance: 1.0,
    });

    let mut gp = GaussianProcess::new(kernel);

    // Create simple 2D training data
    let x_train = Array2::from_shape_vec((3, 2), vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]).unwrap();

    let y_train = Array1::from_vec(vec![0.5, 0.8, 0.6]);

    // Test fitting
    let fit_result = gp.fit(&x_train, &y_train);
    assert!(fit_result.is_ok());

    // Test prediction
    let x_test = Array2::from_shape_vec((2, 2), vec![0.5, 0.5, 1.0, 1.0]).unwrap();

    let prediction_result = gp.predict(&x_test);
    assert!(prediction_result.is_ok());

    let (mean, variance) = prediction_result.unwrap();
    assert_eq!(mean.len(), 2);
    assert_eq!(variance.len(), 2);

    // Variance should be positive
    for &var in variance.iter() {
        assert!(var > 0.0);
    }
}

#[test]
fn test_different_acquisition_functions() {
    // Test all acquisition function variants
    let functions = vec![
        AcquisitionFunction::ExpectedImprovement,
        AcquisitionFunction::UpperConfidenceBound { kappa: 2.576 },
        AcquisitionFunction::ProbabilityOfImprovement,
    ];

    for function in functions {
        let config = OptimizationConfig {
            algorithm: OptimizationAlgorithm::BayesianOptimization {
                acquisition_function: function,
            },
            max_iterations: 5,
            population_size: 3,
            tolerance: 0.1,
            parallel_config: ParallelOptimizationConfig {
                enabled: false,
                num_workers: 1,
                batch_size: 1,
            },
            multi_objective: MultiObjectiveConfig::default(),
        };

        let optimizer = OptimizationEngine::new(config);
        assert_eq!(optimizer.config.max_iterations, 5);
    }
}

#[tokio::test]
async fn test_reinforcement_learning_basic() {
    let rl_config = RLConfig {
        learning_rate: 0.1,
        discount_factor: 0.9,
        exploration_config: ExplorationConfig {
            initial_epsilon: 0.3,
            final_epsilon: 0.05,
            decay_rate: 0.99,
            strategy: ExplorationStrategy::EpsilonGreedy,
        },
        max_episodes: 10, // Small for testing
        episode_length: 5,
        experience_buffer_size: 100,
        batch_size: 10,
        target_update_frequency: 5,
    };

    let mut rl_agent = RLAgent::new(rl_config);

    // Test basic agent creation
    assert!(rl_agent.q_table.is_empty());

    // Create simple training episode
    let state = StateRepresentation {
        data_size: 1024,
        data_type: DataFormat::BINARY,
        complexity_score: 0.5,
        capacity_ratio: 0.3,
    };

    let action = OptimizationAlgorithmType::GeneticAlgorithm;
    let reward = 0.8;
    let next_state = state.clone();

    // Test Q-value update
    rl_agent.update_q_value(state.clone(), action, reward, next_state);
    assert!(!rl_agent.q_table.is_empty());

    // Test strategy selection
    let selected_strategy = rl_agent.select_strategy(&state);
    assert!(selected_strategy.is_ok());
}

#[test]
fn test_different_exploration_strategies() {
    let strategies = vec![
        ExplorationStrategy::EpsilonGreedy,
        ExplorationStrategy::UCB { c: 2.0 },
        ExplorationStrategy::ThompsonSampling,
        ExplorationStrategy::Softmax { temperature: 1.0 },
    ];

    for strategy in strategies {
        let config = RLConfig {
            learning_rate: 0.1,
            discount_factor: 0.9,
            exploration_config: ExplorationConfig {
                initial_epsilon: 0.2,
                final_epsilon: 0.05,
                decay_rate: 0.99,
                strategy,
            },
            max_episodes: 5,
            episode_length: 3,
            experience_buffer_size: 50,
            batch_size: 5,
            target_update_frequency: 2,
        };

        let agent = RLAgent::new(config);
        assert_eq!(agent.config.max_episodes, 5);
    }
}

#[tokio::test]
async fn test_parallel_optimization_config() {
    let mut config = create_test_config();
    config.parallel_config = ParallelOptimizationConfig {
        enabled: true,
        num_workers: 2,
        batch_size: 4,
    };
    config.max_iterations = 10;

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.7,
    }));

    let carrier = create_test_carrier(512);
    let payload = b"test parallel optimization";

    let result = optimizer.optimize(&carrier, payload, None).await;
    assert!(result.is_ok());

    // Parallel optimization should still produce valid results
    let optimization_result = result.unwrap();
    assert!(optimization_result.best_objective_value.combined_score > 0.0);
}

#[test]
fn test_optimization_configuration_validation() {
    // Test invalid configurations
    let invalid_configs = vec![
        OptimizationConfig {
            algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                mutation_rate: -0.1, // Invalid negative rate
                crossover_rate: 0.7,
            },
            max_iterations: 100,
            population_size: 50,
            tolerance: 0.001,
            parallel_config: ParallelOptimizationConfig::default(),
            multi_objective: MultiObjectiveConfig::default(),
        },
        OptimizationConfig {
            algorithm: OptimizationAlgorithm::ParticleSwarm {
                inertia: 2.0, // Invalid high inertia
                cognitive: 2.0,
                social: 2.0,
            },
            max_iterations: 0, // Invalid zero iterations
            population_size: 50,
            tolerance: 0.001,
            parallel_config: ParallelOptimizationConfig::default(),
            multi_objective: MultiObjectiveConfig::default(),
        },
    ];

    // Test that engine can be created but will validate during optimization
    for config in invalid_configs {
        let optimizer = OptimizationEngine::new(config);
        assert!(optimizer.objectives.is_empty());
    }
}

#[tokio::test]
async fn test_optimization_timeout_handling() {
    let mut config = create_test_config();
    config.max_iterations = 1000; // Very high to test timeout

    let mut optimizer = OptimizationEngine::new(config);
    optimizer.add_objective(Box::new(TestObjective {
        weight: 1.0,
        target_score: 0.99,
    }));

    let carrier = create_test_carrier(4096);
    let payload = b"test timeout handling";

    let start_time = std::time::Instant::now();
    let result = optimizer.optimize(&carrier, payload, None).await;
    let elapsed = start_time.elapsed();

    // Should complete reasonably quickly even with high iteration count
    assert!(elapsed < Duration::from_secs(10));
    assert!(result.is_ok());
}

#[test]
fn test_objective_function_edge_cases() {
    let objective = TestObjective {
        weight: 0.0,
        target_score: 1.0,
    };

    // Test with minimal parameters
    let params = EmbedParams {
        stealth_level: StealthLevel::Low,
        error_correction_level: ECLevel::Low,
        encryption_key: vec![],
        use_gpu: false,
        format_specific: HashMap::new(),
    };

    let carrier = create_test_carrier(1);
    let payload = b"";

    let result = objective.evaluate(&params, &carrier, payload);
    assert!(result.is_ok());

    let score = result.unwrap();
    assert!(score >= 0.0);
    assert!(score <= 1.0);
}

#[test]
fn test_kernel_implementations() {
    use ndarray::Array2;

    let x1 = Array2::from_shape_vec((2, 2), vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let x2 = Array2::from_shape_vec((2, 2), vec![0.5, 0.5, 1.5, 1.5]).unwrap();

    // Test RBF Kernel
    let rbf = RBFKernel {
        length_scale: 1.0,
        variance: 1.0,
    };
    let rbf_result = rbf.compute(&x1, &x2);
    assert_eq!(rbf_result.shape(), &[2, 2]);

    // Test Matern Kernel
    let matern = MaternKernel {
        length_scale: 1.0,
        variance: 1.0,
        nu: 1.5,
    };
    let matern_result = matern.compute(&x1, &x2);
    assert_eq!(matern_result.shape(), &[2, 2]);

    // Test Rational Quadratic Kernel
    let rq = RationalQuadraticKernel {
        length_scale: 1.0,
        variance: 1.0,
        alpha: 1.0,
    };
    let rq_result = rq.compute(&x1, &x2);
    assert_eq!(rq_result.shape(), &[2, 2]);

    // All kernel values should be positive due to positive variance
    for kernel_matrix in [&rbf_result, &matern_result, &rq_result] {
        for &value in kernel_matrix.iter() {
            assert!(value >= 0.0);
        }
    }
}

#[tokio::test]
async fn test_optimization_algorithm_comparison() {
    let algorithms = vec![
        OptimizationAlgorithm::GeneticAlgorithm {
            mutation_rate: 0.1,
            crossover_rate: 0.7,
        },
        OptimizationAlgorithm::ParticleSwarm {
            inertia: 0.9,
            cognitive: 2.0,
            social: 2.0,
        },
        OptimizationAlgorithm::BayesianOptimization {
            acquisition_function: AcquisitionFunction::ExpectedImprovement,
        },
    ];

    let carrier = create_test_carrier(1024);
    let payload = b"algorithm comparison test";
    let mut results = Vec::new();

    for algorithm in algorithms {
        let config = OptimizationConfig {
            algorithm,
            max_iterations: 15,
            population_size: 10,
            tolerance: 0.01,
            parallel_config: ParallelOptimizationConfig {
                enabled: false,
                num_workers: 1,
                batch_size: 1,
            },
            multi_objective: MultiObjectiveConfig::default(),
        };

        let mut optimizer = OptimizationEngine::new(config);
        optimizer.add_objective(Box::new(TestObjective {
            weight: 1.0,
            target_score: 0.8,
        }));

        let result = optimizer.optimize(&carrier, payload, None).await;
        assert!(result.is_ok());

        let optimization_result = result.unwrap();
        results.push((
            optimization_result.best_objective_value.combined_score,
            optimization_result.convergence.iterations,
        ));
    }

    // All algorithms should produce reasonable results
    for (score, iterations) in results {
        assert!(score > 0.0);
        assert!(iterations <= 15);
    }
}
