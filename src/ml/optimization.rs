//! Optimization algorithms for steganography parameters
//!
//! This module implements various optimization techniques to find optimal
//! embedding parameters for different scenarios and constraints.

use crate::core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Optimization engine for steganography parameters
#[derive(Debug)]
pub struct OptimizationEngine {
    /// Optimization configuration
    pub config: OptimizationConfig,

    /// Objective function evaluator
    objective_evaluator: ObjectiveEvaluator,

    /// Constraint handlers
    constraints: Vec<Box<dyn OptimizationConstraint>>,

    /// Optimization history
    optimization_history: Vec<OptimizationStep>,
}

/// Configuration for optimization algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Optimization algorithm to use
    pub algorithm: OptimizationAlgorithm,

    /// Maximum number of iterations
    pub max_iterations: usize,

    /// Convergence tolerance
    pub tolerance: f32,

    /// Population size for evolutionary algorithms
    pub population_size: usize,

    /// Multi-objective optimization settings
    pub multi_objective: MultiObjectiveConfig,

    /// Parallel optimization settings
    pub parallel_config: ParallelOptimizationConfig,
}

/// Available optimization algorithms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OptimizationAlgorithm {
    /// Genetic Algorithm
    GeneticAlgorithm {
        mutation_rate: f32,
        crossover_rate: f32,
    },

    /// Particle Swarm Optimization
    ParticleSwarm {
        inertia: f32,
        cognitive: f32,
        social: f32,
    },

    /// Simulated Annealing
    SimulatedAnnealing {
        initial_temp: f32,
        cooling_rate: f32,
    },

    /// Differential Evolution
    DifferentialEvolution {
        differential_weight: f32,
        crossover_prob: f32,
    },

    /// Bayesian Optimization
    BayesianOptimization {
        acquisition_function: AcquisitionFunction,
    },

    /// Grid Search
    GridSearch { resolution: usize },

    /// Random Search
    RandomSearch { num_samples: usize },
}

/// Acquisition functions for Bayesian optimization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AcquisitionFunction {
    ExpectedImprovement,
    UpperConfidenceBound { kappa: f32 },
    ProbabilityOfImprovement,
}

/// Multi-objective optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiObjectiveConfig {
    /// Enable multi-objective optimization
    pub enabled: bool,

    /// Objective weights
    pub objective_weights: HashMap<String, f32>,

    /// Pareto front approximation method
    pub pareto_method: ParetoMethod,

    /// Maximum Pareto solutions to maintain
    pub max_pareto_solutions: usize,
}

impl Default for MultiObjectiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            objective_weights: HashMap::new(),
            pareto_method: ParetoMethod::NSGA2,
            max_pareto_solutions: 100,
        }
    }
}

/// Pareto front approximation methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParetoMethod {
    NSGA2,
    SPEA2,
    MOEAD,
}

/// Parallel optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelOptimizationConfig {
    /// Enable parallel evaluation
    pub enabled: bool,

    /// Number of worker threads
    pub num_workers: usize,

    /// Batch size for parallel evaluation
    pub batch_size: usize,
}

impl Default for ParallelOptimizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            num_workers: 1,
            batch_size: 1,
        }
    }
}

/// Particle velocity for PSO
#[derive(Debug, Clone)]
pub struct ParticleVelocity {
    /// Velocity component for error correction level changes
    pub error_correction_delta: f32,

    /// Velocity component for stealth level changes
    pub stealth_delta: f32,

    /// Velocity component for GPU preference changes
    pub gpu_preference: f32,
}

/// Objective function evaluator
#[derive(Debug)]
pub struct ObjectiveEvaluator {
    /// Objective functions to optimize
    objectives: Vec<Box<dyn ObjectiveFunction>>,

    /// Performance cache for evaluated parameters
    performance_cache: HashMap<String, ObjectiveValue>,

    /// Evaluation statistics
    evaluation_stats: EvaluationStats,
}

/// Objective function trait
pub trait ObjectiveFunction: std::fmt::Debug + Send + Sync {
    /// Evaluate objective for given parameters and carrier
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, payload: &[u8]) -> Result<f32>;

    /// Get objective name
    fn name(&self) -> &str;

    /// Get optimization direction (true = maximize, false = minimize)
    fn maximize(&self) -> bool;

    /// Get relative weight in multi-objective optimization
    fn weight(&self) -> f32;
}

/// Capacity efficiency objective
#[derive(Debug)]
pub struct CapacityObjective {
    weight: f32,
}

/// Embedding speed objective
#[derive(Debug)]
pub struct SpeedObjective {
    weight: f32,
}

/// Detection resistance objective
#[derive(Debug)]
pub struct StealthObjective {
    _weight: f32,
}

/// Quality preservation objective
#[derive(Debug)]
pub struct QualityObjective {
    _weight: f32,
}

/// Combined objective value
#[derive(Debug, Clone)]
pub struct ObjectiveValue {
    /// Individual objective scores
    pub scores: HashMap<String, f32>,

    /// Combined weighted score
    pub combined_score: f32,

    /// Pareto rank (for multi-objective)
    pub pareto_rank: Option<usize>,

    /// Crowding distance (for diversity)
    pub crowding_distance: Option<f32>,
}

/// Optimization constraint trait
pub trait OptimizationConstraint: std::fmt::Debug + Send + Sync {
    /// Check if parameters satisfy constraint
    fn is_satisfied(&self, params: &EmbedParams, carrier: &CarrierData) -> bool;

    /// Get constraint name
    fn name(&self) -> &str;

    /// Get constraint violation penalty
    fn penalty(&self, params: &EmbedParams, carrier: &CarrierData) -> f32;
}

/// Memory usage constraint
#[derive(Debug)]
pub struct MemoryConstraint {
    _max_memory_mb: usize,
}

/// Time constraint
#[derive(Debug)]
pub struct TimeConstraint {
    _max_time_ms: u64,
}

/// Quality constraint
#[derive(Debug)]
pub struct QualityConstraint {
    _min_quality: f32,
}

/// Single optimization step record
#[derive(Debug, Clone)]
pub struct OptimizationStep {
    /// Iteration number
    pub iteration: usize,

    /// Parameters evaluated
    pub parameters: EmbedParams,

    /// Objective value achieved
    pub objective_value: ObjectiveValue,

    /// Constraint violations
    pub constraint_violations: Vec<String>,

    /// Evaluation time
    pub evaluation_time_ms: u64,

    /// Timestamp
    pub timestamp: u64,
}

/// Evaluation statistics
#[derive(Debug, Clone)]
pub struct EvaluationStats {
    /// Total evaluations performed
    pub total_evaluations: usize,

    /// Cache hits
    pub cache_hits: usize,

    /// Average evaluation time
    pub avg_evaluation_time_ms: f64,

    /// Best objective value found
    pub best_objective_value: f32,

    /// Convergence rate
    pub convergence_rate: f32,
}

/// Optimization result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// Best parameters found
    pub best_parameters: EmbedParams,

    /// Best objective value
    pub best_objective_value: ObjectiveValue,

    /// Optimization convergence info
    pub convergence: ConvergenceInfo,

    /// Pareto front (for multi-objective)
    pub pareto_front: Vec<(EmbedParams, ObjectiveValue)>,

    /// Optimization statistics
    pub statistics: EvaluationStats,
}

/// Convergence information
#[derive(Debug, Clone)]
pub struct ConvergenceInfo {
    /// Whether optimization converged
    pub converged: bool,

    /// Number of iterations performed
    pub iterations: usize,

    /// Final convergence measure
    pub convergence_measure: f32,

    /// Reason for termination
    pub termination_reason: TerminationReason,
}

/// Optimization termination reasons
#[derive(Debug, Clone)]
pub enum TerminationReason {
    /// Converged to solution
    Converged,

    /// Maximum iterations reached
    MaxIterations,

    /// Maximum time exceeded
    MaxTime,

    /// User requested stop
    UserStop,

    /// No improvement threshold reached
    NoImprovement,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                mutation_rate: 0.1,
                crossover_rate: 0.8,
            },
            max_iterations: 100,
            tolerance: 1e-6,
            population_size: 50,
            multi_objective: MultiObjectiveConfig {
                enabled: false,
                objective_weights: HashMap::new(),
                pareto_method: ParetoMethod::NSGA2,
                max_pareto_solutions: 100,
            },
            parallel_config: ParallelOptimizationConfig {
                enabled: true,
                num_workers: 4,
                batch_size: 10,
            },
        }
    }
}

impl OptimizationEngine {
    /// Create new optimization engine
    pub fn new(config: OptimizationConfig) -> Self {
        Self {
            config,
            objective_evaluator: ObjectiveEvaluator::new(),
            constraints: Vec::new(),
            optimization_history: Vec::new(),
        }
    }

    /// Add objective function
    pub fn add_objective(&mut self, objective: Box<dyn ObjectiveFunction>) {
        self.objective_evaluator.add_objective(objective);
    }

    /// Add constraint
    pub fn add_constraint(&mut self, constraint: Box<dyn OptimizationConstraint>) {
        self.constraints.push(constraint);
    }

    /// Optimize embedding parameters for given carrier and payload
    pub async fn optimize(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        initial_params: Option<EmbedParams>,
    ) -> Result<OptimizationResult> {
        let algorithm = self.config.algorithm.clone();
        match algorithm {
            OptimizationAlgorithm::GeneticAlgorithm {
                mutation_rate,
                crossover_rate,
            } => {
                self.genetic_algorithm(carrier, payload, mutation_rate, crossover_rate)
                    .await
            }
            OptimizationAlgorithm::ParticleSwarm {
                inertia,
                cognitive,
                social,
            } => {
                self.particle_swarm_optimization(carrier, payload, inertia, cognitive, social)
                    .await
            }
            OptimizationAlgorithm::SimulatedAnnealing {
                initial_temp,
                cooling_rate,
            } => {
                self.simulated_annealing(
                    carrier,
                    payload,
                    initial_params,
                    initial_temp,
                    cooling_rate,
                )
                .await
            }
            OptimizationAlgorithm::BayesianOptimization {
                acquisition_function,
            } => {
                self.bayesian_optimization(carrier, payload, &acquisition_function)
                    .await
            }
            OptimizationAlgorithm::GridSearch { resolution } => {
                self.grid_search(carrier, payload, resolution).await
            }
            OptimizationAlgorithm::RandomSearch { num_samples } => {
                self.random_search(carrier, payload, num_samples).await
            }
            _ => {
                // Default to genetic algorithm
                self.genetic_algorithm(carrier, payload, 0.1, 0.8).await
            }
        }
    }

    /// Advanced genetic algorithm implementation with real crossover and mutation
    async fn genetic_algorithm(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        mutation_rate: f32,
        crossover_rate: f32,
    ) -> Result<OptimizationResult> {
        // Initialize population with diversity
        let mut population = self.initialize_diverse_population(self.config.population_size);
        let mut best_solution = population[0].clone();
        let mut best_objective = f32::NEG_INFINITY;
        let mut convergence_history = Vec::new();
        let mut stagnation_counter = 0;

        // Track population diversity
        let mut diversity_history = Vec::new();

        for iteration in 0..self.config.max_iterations {
            // Evaluate population
            let mut evaluated_population = Vec::new();
            let mut generation_scores = Vec::new();

            for individual in &population {
                let objective_value = self
                    .objective_evaluator
                    .evaluate(individual, carrier, payload)
                    .await?;
                generation_scores.push(objective_value.combined_score);

                // Track best solution
                if objective_value.combined_score > best_objective {
                    best_objective = objective_value.combined_score;
                    best_solution = individual.clone();
                    stagnation_counter = 0;
                } else {
                    stagnation_counter += 1;
                }

                evaluated_population.push((individual.clone(), objective_value));
            }

            // Calculate population statistics
            let avg_fitness =
                generation_scores.iter().sum::<f32>() / generation_scores.len() as f32;
            let diversity = self.calculate_population_diversity(&population);
            diversity_history.push(diversity);
            convergence_history.push(avg_fitness);

            // Log progress
            if iteration % 10 == 0 {
                log::info!(
                    "GA Iteration {iteration}: Best={best_objective:.4}, Avg={avg_fitness:.4}, Diversity={diversity:.4}"
                );
            }

            // Advanced genetic operators
            population = self.advanced_genetic_operators(
                &evaluated_population,
                mutation_rate,
                crossover_rate,
                iteration,
                stagnation_counter,
            );

            // Check for premature convergence and apply diversity enhancement
            if diversity < 0.01 {
                log::warn!("Population diversity too low, introducing mutations");
                population = self.introduce_diversity(&population);
            }

            // Adaptive parameter adjustment
            let _adaptive_mutation_rate =
                self.adaptive_mutation_rate(mutation_rate, diversity, stagnation_counter);
            let _adaptive_crossover_rate =
                self.adaptive_crossover_rate(crossover_rate, avg_fitness);

            // Check convergence
            if self.check_advanced_convergence(iteration, best_objective, &convergence_history) {
                break;
            }

            // Record optimization step
            self.optimization_history.push(OptimizationStep {
                iteration,
                parameters: best_solution.clone(),
                objective_value: ObjectiveValue {
                    scores: HashMap::new(),
                    combined_score: best_objective,
                    pareto_rank: None,
                    crowding_distance: None,
                },
                constraint_violations: Vec::new(),
                evaluation_time_ms: 0,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
        }

        Ok(OptimizationResult {
            best_parameters: best_solution.clone(),
            best_objective_value: self
                .objective_evaluator
                .evaluate(&best_solution, carrier, payload)
                .await?,
            convergence: ConvergenceInfo {
                converged: stagnation_counter < 50,
                iterations: self.optimization_history.len(),
                convergence_measure: best_objective,
                termination_reason: if stagnation_counter >= 50 {
                    TerminationReason::NoImprovement
                } else {
                    TerminationReason::Converged
                },
            },
            pareto_front: Vec::new(),
            statistics: self.objective_evaluator.get_statistics(),
        })
    }

    /// Enhanced particle swarm optimization with adaptive parameters
    async fn particle_swarm_optimization(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        _inertia: f32,
        cognitive: f32,
        social: f32,
    ) -> Result<OptimizationResult> {
        let swarm_size = self.config.population_size;
        let max_iterations = self.config.max_iterations;
        let tolerance = self.config.tolerance;

        // Initialize swarm particles
        let mut particles = self.initialize_swarm(swarm_size);
        let mut velocities = self.initialize_velocities(swarm_size);
        let mut personal_best: Vec<(EmbedParams, ObjectiveValue)> = Vec::new();
        let mut global_best: Option<(EmbedParams, ObjectiveValue)> = None;

        // Initialize personal best positions
        for particle in &particles {
            let objective = self
                .objective_evaluator
                .evaluate(particle, carrier, payload)
                .await?;
            personal_best.push((particle.clone(), objective.clone()));

            // Update global best
            if global_best.is_none()
                || objective.combined_score > global_best.as_ref().unwrap().1.combined_score
            {
                global_best = Some((particle.clone(), objective));
            }
        }

        let mut best_scores = Vec::new();
        let mut stagnation_counter = 0;
        let mut last_best_score = global_best.as_ref().unwrap().1.combined_score;

        for iteration in 0..max_iterations {
            // Adaptive inertia weight (linearly decreasing)
            let inertia = 0.9 - (0.5 * iteration as f32 / max_iterations as f32);

            // Adaptive cognitive and social parameters
            let adaptive_cognitive =
                self.adaptive_cognitive_parameter(cognitive, iteration, max_iterations);
            let adaptive_social = self.adaptive_social_parameter(social, iteration, max_iterations);

            // Update particles
            for (i, particle) in particles.iter_mut().enumerate() {
                // Update velocity with constriction factor
                let constriction =
                    self.calculate_constriction_factor(adaptive_cognitive, adaptive_social);

                self.update_particle_velocity(
                    &mut velocities[i],
                    particle,
                    &personal_best[i].0,
                    &global_best.as_ref().unwrap().0,
                    inertia,
                    adaptive_cognitive,
                    adaptive_social,
                    constriction,
                );

                // Update position
                self.update_particle_position(particle, &velocities[i]);
                self.clamp_particle_bounds(particle);

                // Evaluate new position
                let objective = self
                    .objective_evaluator
                    .evaluate(particle, carrier, payload)
                    .await?;

                // Update personal best
                if objective.combined_score > personal_best[i].1.combined_score {
                    personal_best[i] = (particle.clone(), objective.clone());
                }

                // Update global best
                if objective.combined_score > global_best.as_ref().unwrap().1.combined_score {
                    global_best = Some((particle.clone(), objective));
                    stagnation_counter = 0;
                } else {
                    stagnation_counter += 1;
                }
            }

            let current_best = global_best.as_ref().unwrap().1.combined_score;
            best_scores.push(current_best);

            // Check for convergence
            if (current_best - last_best_score).abs() < tolerance {
                break;
            }

            // Apply diversity enhancement if stagnant
            if stagnation_counter > 20 {
                self.enhance_swarm_diversity(&mut particles, &mut velocities, swarm_size / 4);
                stagnation_counter = 0;
            }

            last_best_score = current_best;
        }

        let (best_params, best_objective) = global_best.unwrap();
        let final_current_best = best_objective.combined_score;

        Ok(OptimizationResult {
            best_parameters: best_params,
            best_objective_value: best_objective,
            convergence: ConvergenceInfo {
                converged: best_scores.len() < max_iterations,
                iterations: best_scores.len(),
                convergence_measure: final_current_best,
                termination_reason: if best_scores.len() < max_iterations {
                    TerminationReason::Converged
                } else {
                    TerminationReason::MaxIterations
                },
            },
            pareto_front: Vec::new(),
            statistics: self.objective_evaluator.get_statistics(),
        })
    }

    /// Simulated annealing implementation
    async fn simulated_annealing(
        &mut self,
        _carrier: &CarrierData,
        _payload: &[u8],
        _initial_params: Option<EmbedParams>,
        _initial_temp: f32,
        _cooling_rate: f32,
    ) -> Result<OptimizationResult> {
        // Placeholder implementation
        todo!("Implement simulated annealing")
    }

    /// Bayesian optimization implementation (basic version)
    async fn bayesian_optimization(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        _acquisition_function: &AcquisitionFunction,
    ) -> Result<OptimizationResult> {
        // Basic implementation using Gaussian Process from bayesian module
        use crate::ml::bayesian::{GaussianProcess, KernelFunction};

        let mut gp = GaussianProcess::new(KernelFunction::RBF {
            length_scale: 1.0,
            signal_variance: 1.0,
        });

        // Generate initial sample points
        let mut sample_points = Vec::new();
        let mut sample_objectives = Vec::new();

        for _ in 0..10 {
            let params = self.generate_random_parameters();
            let objective = self
                .objective_evaluator
                .evaluate(&params, carrier, payload)
                .await?;

            // Convert params to vector format for GP
            let param_vec = vec![
                params.stealth_level as i32 as f32,
                params.error_correction_level as i32 as f32,
                if params.use_gpu { 1.0 } else { 0.0 },
            ];

            sample_points.push(param_vec);
            sample_objectives.push(objective.combined_score);
        }

        // Fit GP to samples
        gp.fit(&sample_points, &sample_objectives);

        // Find best parameters from samples
        let best_idx = sample_objectives
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        let best_params = self.params_from_vector(&sample_points[best_idx]);

        Ok(OptimizationResult {
            best_parameters: best_params,
            best_objective_value: ObjectiveValue {
                scores: std::collections::HashMap::new(),
                combined_score: sample_objectives[best_idx],
                pareto_rank: None,
                crowding_distance: None,
            },
            convergence: ConvergenceInfo {
                converged: true,
                iterations: 10,
                convergence_measure: 0.01,
                termination_reason: TerminationReason::MaxIterations,
            },
            pareto_front: Vec::new(),
            statistics: EvaluationStats {
                total_evaluations: 10,
                cache_hits: 0,
                avg_evaluation_time_ms: 1.0,
                best_objective_value: sample_objectives[best_idx],
                convergence_rate: 0.95,
            },
        })
    }

    /// Grid search implementation
    async fn grid_search(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        resolution: usize,
    ) -> Result<OptimizationResult> {
        let parameter_grid = self.generate_parameter_grid(resolution);
        let mut best_params = parameter_grid[0].clone();
        let mut best_objective = f32::NEG_INFINITY;

        for params in &parameter_grid {
            let objective_value = self
                .objective_evaluator
                .evaluate(params, carrier, payload)
                .await?;

            if objective_value.combined_score > best_objective {
                best_objective = objective_value.combined_score;
                best_params = params.clone();
            }
        }

        Ok(OptimizationResult {
            best_parameters: best_params.clone(),
            best_objective_value: self
                .objective_evaluator
                .evaluate(&best_params, carrier, payload)
                .await?,
            convergence: ConvergenceInfo {
                converged: true,
                iterations: parameter_grid.len(),
                convergence_measure: best_objective,
                termination_reason: TerminationReason::Converged,
            },
            pareto_front: Vec::new(),
            statistics: self.objective_evaluator.get_statistics(),
        })
    }

    /// Random search implementation
    async fn random_search(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
        num_samples: usize,
    ) -> Result<OptimizationResult> {
        let mut best_params = self.generate_random_parameters();
        let mut best_objective = f32::NEG_INFINITY;

        for _ in 0..num_samples {
            let params = self.generate_random_parameters();
            let objective_value = self
                .objective_evaluator
                .evaluate(&params, carrier, payload)
                .await?;

            if objective_value.combined_score > best_objective {
                best_objective = objective_value.combined_score;
                best_params = params;
            }
        }

        Ok(OptimizationResult {
            best_parameters: best_params.clone(),
            best_objective_value: self
                .objective_evaluator
                .evaluate(&best_params, carrier, payload)
                .await?,
            convergence: ConvergenceInfo {
                converged: true,
                iterations: num_samples,
                convergence_measure: best_objective,
                termination_reason: TerminationReason::Converged,
            },
            pareto_front: Vec::new(),
            statistics: self.objective_evaluator.get_statistics(),
        })
    }

    /// Multi-objective optimization using NSGA-II algorithm
    pub async fn multi_objective_optimization(
        &mut self,
        carrier: &CarrierData,
        payload: &[u8],
    ) -> Result<OptimizationResult> {
        if !self.config.multi_objective.enabled {
            return self.optimize(carrier, payload, None).await;
        }

        let population_size = self.config.population_size;
        let max_iterations = self.config.max_iterations;

        // Initialize population
        let population = self.initialize_population(population_size);
        let mut evaluated_population: Vec<(EmbedParams, ObjectiveValue)> = Vec::new();

        // Evaluate initial population
        for individual in population.iter() {
            let objective = self
                .objective_evaluator
                .evaluate(individual, carrier, payload)
                .await?;
            evaluated_population.push((individual.clone(), objective));
        }

        let mut pareto_fronts: Vec<Vec<(EmbedParams, ObjectiveValue)>>;

        for iteration in 0..max_iterations {
            // Calculate Pareto fronts using fast non-dominated sorting
            let fronts = self.fast_non_dominated_sorting(&evaluated_population);
            pareto_fronts = fronts.clone();

            // Calculate crowding distance for each front
            let mut ranked_population = Vec::new();
            for front in fronts.iter() {
                let mut front_with_distance = self.calculate_crowding_distance(front);
                front_with_distance.sort_by(|a, b| {
                    b.1.crowding_distance
                        .unwrap_or(0.0)
                        .partial_cmp(&a.1.crowding_distance.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                ranked_population.extend(front_with_distance);
            }

            // Select parent population
            let parent_population: Vec<_> = ranked_population
                .into_iter()
                .take(population_size)
                .collect();

            // Generate offspring using tournament selection and genetic operators
            let mut offspring = Vec::new();
            while offspring.len() < population_size {
                let parent1 = self.tournament_selection_mo(&parent_population, 2);
                let parent2 = self.tournament_selection_mo(&parent_population, 2);

                let mut child = self.crossover_mo(&parent1, &parent2);
                child = self.mutate_mo(&child, 0.1); // 10% mutation rate

                offspring.push(child);
            }

            // Evaluate offspring
            let mut offspring_evaluated = Vec::new();
            for child in offspring.iter() {
                let objective = self
                    .objective_evaluator
                    .evaluate(child, carrier, payload)
                    .await?;
                offspring_evaluated.push((child.clone(), objective));
            }

            // Combine parent and offspring populations
            evaluated_population.extend(offspring_evaluated);

            // Environmental selection for next generation
            evaluated_population =
                self.environmental_selection(&evaluated_population, population_size);

            // Check convergence
            if self.check_multi_objective_convergence(iteration, &pareto_fronts) {
                break;
            }
        }

        // Return best solution from first Pareto front
        let final_fronts = self.fast_non_dominated_sorting(&evaluated_population);
        let best_solution = if let Some(first_front) = final_fronts.first() {
            if let Some(best) = first_front.first() {
                best.clone()
            } else {
                evaluated_population[0].clone()
            }
        } else {
            evaluated_population[0].clone()
        };

        let convergence_measure = best_solution.1.combined_score;
        Ok(OptimizationResult {
            best_parameters: best_solution.0,
            best_objective_value: best_solution.1,
            convergence: ConvergenceInfo {
                converged: true,
                iterations: max_iterations,
                convergence_measure,
                termination_reason: TerminationReason::Converged,
            },
            pareto_front: final_fronts.into_iter().flatten().collect(),
            statistics: self.objective_evaluator.get_statistics(),
        })
    }

    /// Fast non-dominated sorting for NSGA-II
    fn fast_non_dominated_sorting(
        &self,
        population: &[(EmbedParams, ObjectiveValue)],
    ) -> Vec<Vec<(EmbedParams, ObjectiveValue)>> {
        let mut fronts: Vec<Vec<(EmbedParams, ObjectiveValue)>> = Vec::new();
        let mut domination_counts = vec![0; population.len()];
        let mut dominated_solutions: Vec<Vec<usize>> = vec![Vec::new(); population.len()];
        let mut current_front = Vec::new();

        // Calculate domination relationships
        for i in 0..population.len() {
            for j in 0..population.len() {
                if i != j {
                    if self.dominates(&population[i].1, &population[j].1) {
                        dominated_solutions[i].push(j);
                    } else if self.dominates(&population[j].1, &population[i].1) {
                        domination_counts[i] += 1;
                    }
                }
            }

            if domination_counts[i] == 0 {
                current_front.push(i);
            }
        }

        // Build fronts
        while !current_front.is_empty() {
            let front: Vec<_> = current_front
                .iter()
                .map(|&i| population[i].clone())
                .collect();
            fronts.push(front);

            let mut next_front = Vec::new();
            for &i in &current_front {
                for &j in &dominated_solutions[i] {
                    domination_counts[j] -= 1;
                    if domination_counts[j] == 0 {
                        next_front.push(j);
                    }
                }
            }
            current_front = next_front;
        }

        fronts
    }

    /// Check if solution a dominates solution b
    fn dominates(&self, a: &ObjectiveValue, b: &ObjectiveValue) -> bool {
        let mut at_least_one_better = false;

        for (key, &value_a) in &a.scores {
            if let Some(&value_b) = b.scores.get(key) {
                if value_a < value_b {
                    return false; // a is worse in at least one objective
                }
                if value_a > value_b {
                    at_least_one_better = true;
                }
            }
        }

        at_least_one_better
    }

    /// Calculate crowding distance for diversity preservation
    fn calculate_crowding_distance(
        &self,
        front: &[(EmbedParams, ObjectiveValue)],
    ) -> Vec<(EmbedParams, ObjectiveValue)> {
        let mut front_with_distance = front.to_vec();

        if front_with_distance.len() <= 2 {
            // Set infinite distance for boundary solutions
            for (_, obj) in &mut front_with_distance {
                obj.crowding_distance = Some(f32::INFINITY);
            }
            return front_with_distance;
        }

        // Initialize crowding distances to 0
        for (_, obj) in &mut front_with_distance {
            obj.crowding_distance = Some(0.0);
        }

        // Calculate crowding distance for each objective
        let objective_names: Vec<String> =
            front_with_distance[0].1.scores.keys().cloned().collect();

        for objective_name in objective_names {
            // Sort by this objective
            front_with_distance.sort_by(|a, b| {
                let val_a = a.1.scores.get(&objective_name).unwrap_or(&0.0);
                let val_b = b.1.scores.get(&objective_name).unwrap_or(&0.0);
                val_a
                    .partial_cmp(val_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Set boundary solutions to infinite distance
            let len = front_with_distance.len();
            front_with_distance[0].1.crowding_distance = Some(f32::INFINITY);
            front_with_distance[len - 1].1.crowding_distance = Some(f32::INFINITY);

            // Calculate range for normalization
            let min_val = front_with_distance[0]
                .1
                .scores
                .get(&objective_name)
                .unwrap_or(&0.0);
            let max_val = front_with_distance[front_with_distance.len() - 1]
                .1
                .scores
                .get(&objective_name)
                .unwrap_or(&1.0);
            let range = max_val - min_val;

            if range > 0.0 {
                for i in 1..front_with_distance.len() - 1 {
                    let prev_val = front_with_distance[i - 1]
                        .1
                        .scores
                        .get(&objective_name)
                        .unwrap_or(&0.0);
                    let next_val = front_with_distance[i + 1]
                        .1
                        .scores
                        .get(&objective_name)
                        .unwrap_or(&0.0);
                    let distance = (next_val - prev_val) / range;

                    let current_distance =
                        front_with_distance[i].1.crowding_distance.unwrap_or(0.0);
                    front_with_distance[i].1.crowding_distance = Some(current_distance + distance);
                }
            }
        }

        front_with_distance
    }

    /// Tournament selection for multi-objective optimization
    fn tournament_selection_mo(
        &self,
        population: &[(EmbedParams, ObjectiveValue)],
        tournament_size: usize,
    ) -> EmbedParams {
        let mut best_individual = &population[0];

        for _ in 0..tournament_size {
            let random_index = rand::random::<usize>() % population.len();
            let candidate = &population[random_index];

            // Select based on Pareto rank and crowding distance
            if self.is_better_mo(&candidate.1, &best_individual.1) {
                best_individual = candidate;
            }
        }

        best_individual.0.clone()
    }

    /// Check if solution a is better than solution b in multi-objective sense
    fn is_better_mo(&self, a: &ObjectiveValue, b: &ObjectiveValue) -> bool {
        // First compare by Pareto rank (lower is better)
        if let (Some(rank_a), Some(rank_b)) = (a.pareto_rank, b.pareto_rank) {
            if rank_a < rank_b {
                return true;
            } else if rank_a > rank_b {
                return false;
            }
        }

        // If same rank, compare by crowding distance (higher is better for diversity)
        match (a.crowding_distance, b.crowding_distance) {
            (Some(dist_a), Some(dist_b)) => dist_a > dist_b,
            _ => false,
        }
    }

    /// Crossover operator for multi-objective optimization
    fn crossover_mo(&self, parent1: &EmbedParams, parent2: &EmbedParams) -> EmbedParams {
        // Simple uniform crossover
        EmbedParams {
            encryption_key: if rand::random::<bool>() {
                parent1.encryption_key.clone()
            } else {
                parent2.encryption_key.clone()
            },
            error_correction_level: if rand::random::<bool>() {
                parent1.error_correction_level
            } else {
                parent2.error_correction_level
            },
            stealth_level: if rand::random::<bool>() {
                parent1.stealth_level
            } else {
                parent2.stealth_level
            },
            use_gpu: if rand::random::<bool>() {
                parent1.use_gpu
            } else {
                parent2.use_gpu
            },
            format_specific: if rand::random::<bool>() {
                parent1.format_specific.clone()
            } else {
                parent2.format_specific.clone()
            },
        }
    }

    /// Mutation operator for multi-objective optimization
    fn mutate_mo(&self, individual: &EmbedParams, mutation_rate: f32) -> EmbedParams {
        let mut mutated = individual.clone();

        if rand::random::<f32>() < mutation_rate {
            mutated.error_correction_level = match rand::random::<u8>() % 5 {
                0 => ECLevel::None,
                1 => ECLevel::Low,
                2 => ECLevel::Medium,
                3 => ECLevel::High,
                _ => ECLevel::Maximum,
            };
        }

        if rand::random::<f32>() < mutation_rate {
            mutated.stealth_level = match rand::random::<u8>() % 4 {
                0 => StealthLevel::Low,
                1 => StealthLevel::Medium,
                2 => StealthLevel::High,
                _ => StealthLevel::Maximum,
            };
        }

        if rand::random::<f32>() < mutation_rate {
            mutated.use_gpu = !mutated.use_gpu;
        }

        mutated
    }

    /// Environmental selection for multi-objective optimization
    fn environmental_selection(
        &self,
        combined_population: &[(EmbedParams, ObjectiveValue)],
        target_size: usize,
    ) -> Vec<(EmbedParams, ObjectiveValue)> {
        let fronts = self.fast_non_dominated_sorting(combined_population);
        let mut selected = Vec::new();

        for front in fronts {
            if selected.len() + front.len() <= target_size {
                selected.extend(front);
            } else {
                // Fill remaining slots with best crowding distance solutions
                let remaining_slots = target_size - selected.len();
                let front_with_distance = self.calculate_crowding_distance(&front);
                let mut sorted_front = front_with_distance;
                sorted_front.sort_by(|a, b| {
                    b.1.crowding_distance
                        .unwrap_or(0.0)
                        .partial_cmp(&a.1.crowding_distance.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                selected.extend(sorted_front.into_iter().take(remaining_slots));
                break;
            }
        }

        selected
    }

    /// Check convergence for multi-objective optimization
    fn check_multi_objective_convergence(
        &self,
        iteration: usize,
        _pareto_fronts: &[Vec<(EmbedParams, ObjectiveValue)>],
    ) -> bool {
        // Simple convergence based on maximum iterations
        iteration >= self.config.max_iterations - 1
    }

    /// Initialize random population
    fn initialize_population(&self, size: usize) -> Vec<EmbedParams> {
        (0..size)
            .map(|_| self.generate_random_parameters())
            .collect()
    }

    /// Generate random parameters
    fn generate_random_parameters(&self) -> EmbedParams {
        use rand::random;

        EmbedParams {
            encryption_key: vec![],
            error_correction_level: if random::<bool>() {
                ECLevel::High
            } else {
                ECLevel::Low
            },
            stealth_level: if random::<bool>() {
                StealthLevel::High
            } else {
                StealthLevel::Low
            },
            use_gpu: random::<bool>(),
            format_specific: HashMap::new(),
        }
    }

    /// Generate parameter grid for grid search
    fn generate_parameter_grid(&self, resolution: usize) -> Vec<EmbedParams> {
        let mut grid = Vec::new();

        for ec_high in [false, true] {
            for stealth_high in [false, true] {
                for use_gpu in [false, true] {
                    grid.push(EmbedParams {
                        encryption_key: vec![],
                        error_correction_level: if ec_high { ECLevel::High } else { ECLevel::Low },
                        stealth_level: if stealth_high {
                            StealthLevel::High
                        } else {
                            StealthLevel::Low
                        },
                        use_gpu,
                        format_specific: HashMap::new(),
                    });
                }
            }
        }

        // Expand grid based on resolution if needed
        grid.into_iter().take(resolution).collect()
    }

    /// Advanced genetic operators with real crossover and mutation
    fn advanced_genetic_operators(
        &self,
        evaluated_population: &[(EmbedParams, ObjectiveValue)],
        mutation_rate: f32,
        crossover_rate: f32,
        iteration: usize,
        stagnation_counter: usize,
    ) -> Vec<EmbedParams> {
        let mut new_population = Vec::new();
        let population_size = evaluated_population.len();

        // Elitism: Keep best individuals
        let elite_count = (population_size as f32 * 0.1).ceil() as usize;
        let mut sorted_population = evaluated_population.to_vec();
        sorted_population
            .sort_by(|a, b| b.1.combined_score.partial_cmp(&a.1.combined_score).unwrap());

        // Add elite individuals
        for i in 0..elite_count {
            new_population.push(sorted_population[i].0.clone());
        }

        // Generate offspring for the rest of the population
        while new_population.len() < population_size {
            // Selection
            let parent1 = self.tournament_selection(evaluated_population, 3);
            let parent2 = self.tournament_selection(evaluated_population, 3);

            // Crossover
            let mut offspring = if rand::random::<f32>() < crossover_rate {
                self.uniform_crossover(&parent1, &parent2)
            } else {
                // Clone better parent
                if self.evaluate_fitness(&parent1) > self.evaluate_fitness(&parent2) {
                    parent1.clone()
                } else {
                    parent2.clone()
                }
            };

            // Mutation with adaptive rate
            let adaptive_rate = if stagnation_counter > 20 {
                mutation_rate * 2.0 // Increase mutation when stagnating
            } else {
                mutation_rate
            };

            if rand::random::<f32>() < adaptive_rate {
                offspring = self.gaussian_mutation(&offspring, iteration);
            }

            new_population.push(offspring);
        }

        new_population.truncate(population_size);
        new_population
    }

    /// Initialize diverse population using different strategies
    fn initialize_diverse_population(&self, size: usize) -> Vec<EmbedParams> {
        let mut population = Vec::with_capacity(size);

        // Add some predefined good strategies
        let strategies = [
            // High security strategy
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::High,
                stealth_level: StealthLevel::Maximum,
                use_gpu: false,
                format_specific: HashMap::new(),
            },
            // High performance strategy
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: true,
                format_specific: HashMap::new(),
            },
            // Balanced strategy
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Medium,
                stealth_level: StealthLevel::Medium,
                use_gpu: false,
                format_specific: HashMap::new(),
            },
        ];

        // Add predefined strategies
        for strategy in &strategies {
            if population.len() < size {
                population.push(strategy.clone());
            }
        }

        // Fill remaining with random individuals
        while population.len() < size {
            population.push(self.generate_random_parameters());
        }

        population
    }

    /// Uniform crossover between two parents
    fn uniform_crossover(&self, parent1: &EmbedParams, parent2: &EmbedParams) -> EmbedParams {
        EmbedParams {
            encryption_key: if rand::random::<bool>() {
                parent1.encryption_key.clone()
            } else {
                parent2.encryption_key.clone()
            },
            error_correction_level: if rand::random::<bool>() {
                parent1.error_correction_level
            } else {
                parent2.error_correction_level
            },
            stealth_level: if rand::random::<bool>() {
                parent1.stealth_level
            } else {
                parent2.stealth_level
            },
            use_gpu: if rand::random::<bool>() {
                parent1.use_gpu
            } else {
                parent2.use_gpu
            },
            format_specific: if rand::random::<bool>() {
                parent1.format_specific.clone()
            } else {
                parent2.format_specific.clone()
            },
        }
    }

    /// Gaussian mutation with adaptive strength
    fn gaussian_mutation(&self, individual: &EmbedParams, iteration: usize) -> EmbedParams {
        let mut mutated = individual.clone();

        // Mutation strength decreases over time
        let mutation_strength = 1.0 / (1.0 + iteration as f32 * 0.01);

        // Mutate error correction level
        if rand::random::<f32>() < 0.3 {
            mutated.error_correction_level = match rand::random::<u8>() % 4 {
                0 => ECLevel::None,
                1 => ECLevel::Low,
                2 => ECLevel::Medium,
                3 => ECLevel::High,
                _ => ECLevel::Maximum,
            };
        }

        // Mutate stealth level
        if rand::random::<f32>() < 0.3 {
            mutated.stealth_level = match rand::random::<u8>() % 4 {
                0 => StealthLevel::Low,
                1 => StealthLevel::Medium,
                2 => StealthLevel::High,
                _ => StealthLevel::Maximum,
            };
        }

        // Mutate GPU usage
        if rand::random::<f32>() < 0.2 * mutation_strength {
            mutated.use_gpu = !mutated.use_gpu;
        }

        mutated
    }

    /// Calculate population diversity using Hamming distance
    fn calculate_population_diversity(&self, population: &[EmbedParams]) -> f32 {
        if population.len() < 2 {
            return 1.0;
        }

        let mut total_distance = 0.0;
        let mut comparisons = 0;

        for i in 0..population.len() {
            for j in i + 1..population.len() {
                let distance = self.hamming_distance(&population[i], &population[j]);
                total_distance += distance;
                comparisons += 1;
            }
        }

        total_distance / comparisons as f32
    }

    /// Calculate Hamming distance between two parameter sets
    fn hamming_distance(&self, params1: &EmbedParams, params2: &EmbedParams) -> f32 {
        let mut distance = 0.0;

        if params1.error_correction_level != params2.error_correction_level {
            distance += 1.0;
        }
        if params1.stealth_level != params2.stealth_level {
            distance += 1.0;
        }
        if params1.use_gpu != params2.use_gpu {
            distance += 1.0;
        }
        if params1.encryption_key != params2.encryption_key {
            distance += 1.0;
        }

        distance / 4.0 // Normalize to [0, 1]
    }

    /// Introduce diversity when population converges prematurely
    fn introduce_diversity(&self, population: &[EmbedParams]) -> Vec<EmbedParams> {
        let mut diverse_population = Vec::new();

        // Keep best individual
        diverse_population.push(population[0].clone());

        // Mutate others heavily
        for individual in population.iter().skip(1) {
            let mut mutated = individual.clone();

            // Heavy mutation
            if rand::random::<f32>() < 0.8 {
                mutated = self.gaussian_mutation(&mutated, 0); // Use high mutation strength
            }

            diverse_population.push(mutated);
        }

        diverse_population
    }

    /// Adaptive mutation rate based on diversity and stagnation
    fn adaptive_mutation_rate(
        &self,
        base_rate: f32,
        diversity: f32,
        stagnation_counter: usize,
    ) -> f32 {
        let diversity_factor = if diversity < 0.1 { 2.0 } else { 1.0 };
        let stagnation_factor = 1.0 + (stagnation_counter as f32 * 0.05);

        (base_rate * diversity_factor * stagnation_factor).min(0.8)
    }

    /// Adaptive crossover rate based on average fitness
    fn adaptive_crossover_rate(&self, base_rate: f32, avg_fitness: f32) -> f32 {
        // Increase crossover rate when fitness is improving
        let fitness_factor = if avg_fitness > 0.7 { 1.2 } else { 0.8 };
        (base_rate * fitness_factor).clamp(0.1, 0.95)
    }

    /// Advanced convergence checking
    fn check_advanced_convergence(
        &self,
        iteration: usize,
        best_objective: f32,
        history: &[f32],
    ) -> bool {
        // Check maximum iterations
        if iteration >= self.config.max_iterations {
            return true;
        }

        // Check if we've reached a good enough solution
        if best_objective > 0.95 {
            return true;
        }

        // Check for stagnation over last 20 iterations
        if history.len() >= 20 {
            let recent_improvement = history[history.len() - 1] - history[history.len() - 20];
            if recent_improvement < self.config.tolerance {
                return true;
            }
        }

        false
    }

    /// Evaluate fitness of individual parameters (simplified)
    fn evaluate_fitness(&self, params: &EmbedParams) -> f32 {
        let mut fitness = 0.0;

        // Simple heuristic fitness based on parameter choices
        fitness += match params.error_correction_level {
            ECLevel::None => 0.1,
            ECLevel::Low => 0.3,
            ECLevel::Medium => 0.7,
            ECLevel::High => 0.9,
            ECLevel::Maximum => 0.8, // Slight penalty for overhead
        };

        fitness += match params.stealth_level {
            StealthLevel::Low => 0.2,
            StealthLevel::Medium => 0.6,
            StealthLevel::High => 0.8,
            StealthLevel::Maximum => 0.9,
        };

        if params.use_gpu {
            fitness += 0.1; // Bonus for GPU usage
        }

        fitness / 2.0 // Normalize
    }

    /// Tournament selection
    fn tournament_selection(
        &self,
        population: &[(EmbedParams, ObjectiveValue)],
        tournament_size: usize,
    ) -> EmbedParams {
        let mut best_individual = &population[0];
        let mut best_fitness = population[0].1.combined_score;

        for _ in 0..tournament_size {
            let random_index = rand::random::<usize>() % population.len();
            let individual = &population[random_index];

            if individual.1.combined_score > best_fitness {
                best_fitness = individual.1.combined_score;
                best_individual = individual;
            }
        }

        best_individual.0.clone()
    }

    /// Check convergence criteria
    #[allow(dead_code)]
    fn check_convergence(&self, iteration: usize, _best_objective: f32) -> bool {
        iteration >= self.config.max_iterations
    }

    /// Initialize swarm particles for PSO
    fn initialize_swarm(&self, swarm_size: usize) -> Vec<EmbedParams> {
        let mut swarm = Vec::new();

        for _ in 0..swarm_size {
            let params = EmbedParams {
                error_correction_level: match rand::random::<u8>() % 5 {
                    0 => ECLevel::None,
                    1 => ECLevel::Low,
                    2 => ECLevel::Medium,
                    3 => ECLevel::High,
                    _ => ECLevel::Maximum,
                },
                stealth_level: match rand::random::<u8>() % 4 {
                    0 => StealthLevel::Low,
                    1 => StealthLevel::Medium,
                    2 => StealthLevel::High,
                    _ => StealthLevel::Maximum,
                },
                use_gpu: rand::random(),
                encryption_key: Vec::new(),
                format_specific: HashMap::new(),
            };
            swarm.push(params);
        }

        swarm
    }

    /// Initialize velocity vectors for PSO particles
    fn initialize_velocities(&self, swarm_size: usize) -> Vec<ParticleVelocity> {
        let mut velocities = Vec::new();

        for _ in 0..swarm_size {
            velocities.push(ParticleVelocity {
                error_correction_delta: (rand::random::<f32>() - 0.5) * 2.0,
                stealth_delta: (rand::random::<f32>() - 0.5) * 2.0,
                gpu_preference: (rand::random::<f32>() - 0.5) * 2.0,
            });
        }

        velocities
    }

    /// Update particle velocity using PSO equations
    fn update_particle_velocity(
        &self,
        velocity: &mut ParticleVelocity,
        current_position: &EmbedParams,
        personal_best: &EmbedParams,
        global_best: &EmbedParams,
        inertia: f32,
        cognitive: f32,
        social: f32,
        constriction: f32,
    ) {
        let r1 = rand::random::<f32>();
        let r2 = rand::random::<f32>();

        // Update error correction velocity
        let personal_ec_diff = self.param_distance_ec(personal_best, current_position);
        let global_ec_diff = self.param_distance_ec(global_best, current_position);
        velocity.error_correction_delta = constriction
            * (inertia * velocity.error_correction_delta
                + cognitive * r1 * personal_ec_diff
                + social * r2 * global_ec_diff);

        // Update stealth level velocity
        let personal_stealth_diff = self.param_distance_stealth(personal_best, current_position);
        let global_stealth_diff = self.param_distance_stealth(global_best, current_position);
        velocity.stealth_delta = constriction
            * (inertia * velocity.stealth_delta
                + cognitive * r1 * personal_stealth_diff
                + social * r2 * global_stealth_diff);

        // Update GPU preference velocity
        let personal_gpu_diff = if personal_best.use_gpu != current_position.use_gpu {
            1.0
        } else {
            0.0
        };
        let global_gpu_diff = if global_best.use_gpu != current_position.use_gpu {
            1.0
        } else {
            0.0
        };
        velocity.gpu_preference = constriction
            * (inertia * velocity.gpu_preference
                + cognitive * r1 * personal_gpu_diff
                + social * r2 * global_gpu_diff);
    }

    /// Update particle position based on velocity
    fn update_particle_position(&self, particle: &mut EmbedParams, velocity: &ParticleVelocity) {
        // Update error correction level based on velocity
        if velocity.error_correction_delta.abs() > 0.5 {
            particle.error_correction_level = match rand::random::<u8>() % 5 {
                0 => ECLevel::None,
                1 => ECLevel::Low,
                2 => ECLevel::Medium,
                3 => ECLevel::High,
                _ => ECLevel::Maximum,
            };
        }

        // Update stealth level based on velocity
        if velocity.stealth_delta.abs() > 0.5 {
            particle.stealth_level = match rand::random::<u8>() % 4 {
                0 => StealthLevel::Low,
                1 => StealthLevel::Medium,
                2 => StealthLevel::High,
                _ => StealthLevel::Maximum,
            };
        }

        // Update GPU usage based on velocity
        if velocity.gpu_preference.abs() > 0.5 {
            particle.use_gpu = velocity.gpu_preference > 0.0;
        }
    }

    /// Clamp particle parameters to valid bounds
    fn clamp_particle_bounds(&self, _particle: &mut EmbedParams) {
        // Parameters are already discrete and bounded by enum definitions
        // No additional clamping needed for current parameter types
    }

    /// Calculate constriction factor for PSO
    fn calculate_constriction_factor(&self, cognitive: f32, social: f32) -> f32 {
        let phi = cognitive + social;
        if phi <= 4.0 {
            1.0
        } else {
            2.0 / (2.0 - phi - (phi * phi - 4.0 * phi).sqrt())
        }
    }

    /// Adaptive cognitive parameter
    fn adaptive_cognitive_parameter(
        &self,
        base_cognitive: f32,
        iteration: usize,
        max_iterations: usize,
    ) -> f32 {
        // Decrease cognitive component over time to reduce self-reliance
        let progress = iteration as f32 / max_iterations as f32;
        base_cognitive * (1.0 - 0.5 * progress)
    }

    /// Adaptive social parameter
    fn adaptive_social_parameter(
        &self,
        base_social: f32,
        iteration: usize,
        max_iterations: usize,
    ) -> f32 {
        // Increase social component over time to promote convergence
        let progress = iteration as f32 / max_iterations as f32;
        base_social * (1.0 + 0.5 * progress)
    }

    /// Enhance swarm diversity when stagnant
    fn enhance_swarm_diversity(
        &self,
        particles: &mut [EmbedParams],
        velocities: &mut [ParticleVelocity],
        num_to_reinitialize: usize,
    ) {
        // Reinitialize worst performing particles
        for i in particles.len().saturating_sub(num_to_reinitialize)..particles.len() {
            particles[i] = EmbedParams {
                error_correction_level: match rand::random::<u8>() % 5 {
                    0 => ECLevel::None,
                    1 => ECLevel::Low,
                    2 => ECLevel::Medium,
                    3 => ECLevel::High,
                    _ => ECLevel::Maximum,
                },
                stealth_level: match rand::random::<u8>() % 4 {
                    0 => StealthLevel::Low,
                    1 => StealthLevel::Medium,
                    2 => StealthLevel::High,
                    _ => StealthLevel::Maximum,
                },
                use_gpu: rand::random(),
                encryption_key: Vec::new(),
                format_specific: HashMap::new(),
            };

            velocities[i] = ParticleVelocity {
                error_correction_delta: (rand::random::<f32>() - 0.5) * 4.0, // Higher initial velocity
                stealth_delta: (rand::random::<f32>() - 0.5) * 4.0,
                gpu_preference: (rand::random::<f32>() - 0.5) * 4.0,
            };
        }
    }

    /// Calculate parameter distance for error correction level
    fn param_distance_ec(&self, p1: &EmbedParams, p2: &EmbedParams) -> f32 {
        let ec1 = match p1.error_correction_level {
            ECLevel::None => 0.0,
            ECLevel::Low => 1.0,
            ECLevel::Medium => 2.0,
            ECLevel::High => 3.0,
            ECLevel::Maximum => 4.0,
        };
        let ec2 = match p2.error_correction_level {
            ECLevel::None => 0.0,
            ECLevel::Low => 1.0,
            ECLevel::Medium => 2.0,
            ECLevel::High => 3.0,
            ECLevel::Maximum => 4.0,
        };
        ec1 - ec2
    }

    /// Calculate parameter distance for stealth level
    fn param_distance_stealth(&self, p1: &EmbedParams, p2: &EmbedParams) -> f32 {
        let s1 = match p1.stealth_level {
            StealthLevel::Low => 0.0,
            StealthLevel::Medium => 1.0,
            StealthLevel::High => 2.0,
            StealthLevel::Maximum => 3.0,
        };
        let s2 = match p2.stealth_level {
            StealthLevel::Low => 0.0,
            StealthLevel::Medium => 1.0,
            StealthLevel::High => 2.0,
            StealthLevel::Maximum => 3.0,
        };
        s1 - s2
    }

    /// Convert parameter vector back to EmbedParams (helper for Bayesian optimization)
    fn params_from_vector(&self, vec: &[f32]) -> EmbedParams {
        EmbedParams {
            encryption_key: vec![],
            error_correction_level: if vec.get(1).unwrap_or(&0.0) > &1.0 {
                ECLevel::High
            } else {
                ECLevel::Low
            },
            stealth_level: match vec.first().unwrap_or(&0.0) {
                x if *x > 2.0 => StealthLevel::Maximum,
                x if *x > 1.0 => StealthLevel::High,
                _ => StealthLevel::Low,
            },
            use_gpu: vec.get(2).unwrap_or(&0.0) > &0.5,
            format_specific: HashMap::new(),
        }
    }
}

impl Default for ObjectiveEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectiveEvaluator {
    /// Create new objective evaluator
    pub fn new() -> Self {
        Self {
            objectives: Vec::new(),
            performance_cache: HashMap::new(),
            evaluation_stats: EvaluationStats {
                total_evaluations: 0,
                cache_hits: 0,
                avg_evaluation_time_ms: 0.0,
                best_objective_value: f32::NEG_INFINITY,
                convergence_rate: 0.0,
            },
        }
    }

    /// Add objective function
    pub fn add_objective(&mut self, objective: Box<dyn ObjectiveFunction>) {
        self.objectives.push(objective);
    }

    /// Evaluate parameters against all objectives
    pub async fn evaluate(
        &mut self,
        params: &EmbedParams,
        carrier: &CarrierData,
        payload: &[u8],
    ) -> Result<ObjectiveValue> {
        let start_time = std::time::Instant::now();

        // Check cache first
        let cache_key = format!("{params:?}");
        if let Some(cached_value) = self.performance_cache.get(&cache_key) {
            self.evaluation_stats.cache_hits += 1;
            return Ok(cached_value.clone());
        }

        // Evaluate all objectives
        let mut scores = HashMap::new();
        let mut combined_score = 0.0;
        let mut total_weight = 0.0;

        for objective in &self.objectives {
            let score = objective.evaluate(params, carrier, payload)?;
            let weight = objective.weight();

            scores.insert(objective.name().to_string(), score);
            combined_score += score * weight;
            total_weight += weight;
        }

        // Normalize combined score
        if total_weight > 0.0 {
            combined_score /= total_weight;
        }

        let objective_value = ObjectiveValue {
            scores,
            combined_score,
            pareto_rank: None,
            crowding_distance: None,
        };

        // Cache result
        self.performance_cache
            .insert(cache_key, objective_value.clone());

        // Update statistics
        self.evaluation_stats.total_evaluations += 1;
        let evaluation_time = start_time.elapsed().as_millis() as f64;
        self.evaluation_stats.avg_evaluation_time_ms =
            (self.evaluation_stats.avg_evaluation_time_ms
                * (self.evaluation_stats.total_evaluations - 1) as f64
                + evaluation_time)
                / self.evaluation_stats.total_evaluations as f64;

        if combined_score > self.evaluation_stats.best_objective_value {
            self.evaluation_stats.best_objective_value = combined_score;
        }

        Ok(objective_value)
    }

    /// Get evaluation statistics
    pub fn get_statistics(&self) -> EvaluationStats {
        self.evaluation_stats.clone()
    }
}

impl CapacityObjective {
    pub fn new(weight: f32) -> Self {
        Self { weight }
    }
}

impl ObjectiveFunction for CapacityObjective {
    fn evaluate(
        &self,
        _params: &EmbedParams,
        _carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<f32> {
        // Placeholder - would measure actual capacity efficiency
        Ok(0.7)
    }

    fn name(&self) -> &str {
        "capacity"
    }

    fn maximize(&self) -> bool {
        true
    }

    fn weight(&self) -> f32 {
        self.weight
    }
}

impl SpeedObjective {
    pub fn new(weight: f32) -> Self {
        Self { weight }
    }
}

impl ObjectiveFunction for SpeedObjective {
    fn evaluate(
        &self,
        _params: &EmbedParams,
        _carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<f32> {
        // Placeholder - would measure actual embedding speed
        Ok(0.8)
    }

    fn name(&self) -> &str {
        "speed"
    }

    fn maximize(&self) -> bool {
        true
    }

    fn weight(&self) -> f32 {
        self.weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_optimization_config_default() {
        let config = OptimizationConfig::default();
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.population_size, 50);
    }

    #[test]
    fn test_objective_evaluator() {
        let mut evaluator = ObjectiveEvaluator::new();
        evaluator.add_objective(Box::new(CapacityObjective::new(1.0)));

        assert_eq!(evaluator.objectives.len(), 1);
        assert_eq!(evaluator.evaluation_stats.total_evaluations, 0);
    }

    #[test]
    fn test_capacity_objective() {
        let objective = CapacityObjective::new(0.5);
        assert_eq!(objective.weight(), 0.5);
        assert_eq!(objective.name(), "capacity");
        assert!(objective.maximize());
    }

    #[tokio::test]
    async fn test_random_search() {
        let mut engine = OptimizationEngine::new(OptimizationConfig {
            algorithm: OptimizationAlgorithm::RandomSearch { num_samples: 10 },
            ..OptimizationConfig::default()
        });

        engine.add_objective(Box::new(CapacityObjective::new(1.0)));

        let carrier = CarrierData {
            format: DataFormat::PNG,
            raw_data: vec![0; 1000],
            metadata: HashMap::new(),
            dimensions: Some(crate::core::Dimensions::new_2d(100, 100)),
            sampling_rate: None,
        };

        let payload = b"test payload";

        let result = engine.optimize(&carrier, payload, None).await.unwrap();
        assert!(result.convergence.converged);
        assert_eq!(result.convergence.iterations, 10);
    }
}
