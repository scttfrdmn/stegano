//! Bayesian optimization for steganography parameter optimization
//!
//! This module implements Gaussian process-based Bayesian optimization
//! for intelligent exploration of the parameter space with uncertainty quantification.

use crate::core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bayesian optimizer using Gaussian processes
#[derive(Debug)]
pub struct BayesianOptimizer {
    /// Gaussian process model
    gaussian_process: GaussianProcess,

    /// Acquisition function configuration
    acquisition_config: AcquisitionConfig,

    /// Observed data points (parameters and objective values)
    observations: Vec<Observation>,

    /// Optimization bounds for each parameter
    _parameter_bounds: ParameterBounds,

    /// Configuration for Bayesian optimization
    config: BayesianConfig,
}

/// Gaussian process model for surrogate modeling
#[derive(Debug)]
pub struct GaussianProcess {
    /// Kernel function for covariance
    kernel: KernelFunction,

    /// Hyperparameters
    hyperparams: GpHyperparams,

    /// Training data (input features)
    training_x: Vec<Vec<f32>>,

    /// Training data (output values)
    training_y: Vec<f32>,

    /// Cholesky decomposition of covariance matrix
    cholesky_l: Option<Vec<Vec<f32>>>,

    /// Alpha vector for predictions
    alpha: Vec<f32>,
}

/// Kernel functions for Gaussian process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KernelFunction {
    /// Radial basis function (RBF) kernel
    RBF {
        length_scale: f32,
        signal_variance: f32,
    },

    /// Matern kernel with nu=3/2
    Matern32 {
        length_scale: f32,
        signal_variance: f32,
    },

    /// Matern kernel with nu=5/2
    Matern52 {
        length_scale: f32,
        signal_variance: f32,
    },

    /// Rational quadratic kernel
    RationalQuadratic {
        length_scale: f32,
        signal_variance: f32,
        alpha: f32,
    },
}

/// Gaussian process hyperparameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpHyperparams {
    /// Noise variance
    pub noise_variance: f32,

    /// Signal variance
    pub signal_variance: f32,

    /// Length scale parameter
    pub length_scale: f32,

    /// Additional kernel-specific parameters
    pub kernel_params: HashMap<String, f32>,
}

/// Acquisition function configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionConfig {
    /// Type of acquisition function
    pub function_type: AcquisitionFunction,

    /// Exploration vs exploitation trade-off
    pub exploration_weight: f32,

    /// Number of Monte Carlo samples for acquisition
    pub mc_samples: usize,
}

/// Acquisition function types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcquisitionFunction {
    /// Expected improvement
    ExpectedImprovement,

    /// Upper confidence bound
    UpperConfidenceBound { kappa: f32 },

    /// Probability of improvement
    ProbabilityOfImprovement,

    /// Entropy search
    EntropySearch,

    /// Knowledge gradient
    KnowledgeGradient,
}

/// Single observation (parameter vector and objective value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Parameter vector
    pub parameters: Vec<f32>,

    /// Observed objective value
    pub objective_value: f32,

    /// Uncertainty/noise in observation
    pub noise_level: f32,

    /// Timestamp of observation
    pub timestamp: u64,
}

/// Parameter bounds for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterBounds {
    /// Lower bounds for each parameter
    pub lower_bounds: Vec<f32>,

    /// Upper bounds for each parameter
    pub upper_bounds: Vec<f32>,

    /// Parameter names for interpretation
    pub parameter_names: Vec<String>,
}

/// Bayesian optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BayesianConfig {
    /// Maximum number of iterations
    pub max_iterations: usize,

    /// Number of initial random samples
    pub initial_samples: usize,

    /// Convergence tolerance
    pub tolerance: f32,

    /// Update GP hyperparameters every N iterations
    pub hyperparameter_update_freq: usize,

    /// Use gradient-based acquisition optimization
    pub use_gradient_acquisition: bool,
}

/// Bayesian optimization result
#[derive(Debug, Clone)]
pub struct BayesianResult {
    /// Best parameters found
    pub best_parameters: EmbedParams,

    /// Best objective value
    pub best_objective_value: f32,

    /// Predicted mean at best point
    pub predicted_mean: f32,

    /// Predicted variance at best point
    pub predicted_variance: f32,

    /// Number of iterations performed
    pub iterations: usize,

    /// All observations made
    pub observation_history: Vec<Observation>,

    /// Final GP hyperparameters
    pub final_hyperparams: GpHyperparams,
}

impl Default for BayesianConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            initial_samples: 10,
            tolerance: 1e-3,
            hyperparameter_update_freq: 10,
            use_gradient_acquisition: false,
        }
    }
}

impl Default for AcquisitionConfig {
    fn default() -> Self {
        Self {
            function_type: AcquisitionFunction::ExpectedImprovement,
            exploration_weight: 1.0,
            mc_samples: 1000,
        }
    }
}

impl Default for GpHyperparams {
    fn default() -> Self {
        Self {
            noise_variance: 0.01,
            signal_variance: 1.0,
            length_scale: 1.0,
            kernel_params: HashMap::new(),
        }
    }
}

impl BayesianOptimizer {
    /// Create new Bayesian optimizer
    pub fn new(bounds: ParameterBounds, config: BayesianConfig) -> Self {
        Self {
            gaussian_process: GaussianProcess::new(KernelFunction::RBF {
                length_scale: 1.0,
                signal_variance: 1.0,
            }),
            acquisition_config: AcquisitionConfig::default(),
            observations: Vec::new(),
            _parameter_bounds: bounds,
            config,
        }
    }

    /// Optimize embedding parameters using Bayesian optimization
    pub async fn optimize(
        &mut self,
        objective_function: &dyn Fn(&EmbedParams) -> Result<f32>,
        _carrier: &CarrierData,
        _payload: &[u8],
    ) -> Result<BayesianResult> {
        // Initial random sampling
        for _ in 0..self.config.initial_samples {
            let params = self.sample_random_parameters();
            let objective_value = objective_function(&params)?;

            let observation = Observation {
                parameters: self.embed_params_to_vector(&params),
                objective_value,
                noise_level: 0.01,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            self.observations.push(observation);
        }

        // Main Bayesian optimization loop
        for iteration in 0..self.config.max_iterations {
            // Update Gaussian process with observations
            self.update_gaussian_process();

            // Update hyperparameters periodically
            if iteration % self.config.hyperparameter_update_freq == 0 {
                self.optimize_hyperparameters();
            }

            // Find next point to evaluate using acquisition function
            let next_params = self.optimize_acquisition_function().await?;
            let next_objective = objective_function(&next_params)?;

            let observation = Observation {
                parameters: self.embed_params_to_vector(&next_params),
                objective_value: next_objective,
                noise_level: 0.01,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            self.observations.push(observation);

            // Check convergence
            if self.check_convergence() {
                break;
            }

            // Log progress
            if iteration % 5 == 0 {
                let best_value = self
                    .observations
                    .iter()
                    .map(|obs| obs.objective_value)
                    .fold(f32::NEG_INFINITY, f32::max);

                log::info!("Bayesian Optimization Iteration {iteration}: Best = {best_value:.4}");
            }
        }

        // Find best result
        let best_observation = self
            .observations
            .iter()
            .max_by(|a, b| a.objective_value.partial_cmp(&b.objective_value).unwrap())
            .unwrap();

        let best_params = self.vector_to_embed_params(&best_observation.parameters);
        let (predicted_mean, predicted_variance) =
            self.gaussian_process.predict(&best_observation.parameters);

        Ok(BayesianResult {
            best_parameters: best_params,
            best_objective_value: best_observation.objective_value,
            predicted_mean,
            predicted_variance,
            iterations: self.observations.len() - self.config.initial_samples,
            observation_history: self.observations.clone(),
            final_hyperparams: self.gaussian_process.hyperparams.clone(),
        })
    }

    /// Update Gaussian process with current observations
    fn update_gaussian_process(&mut self) {
        let x_data: Vec<Vec<f32>> = self
            .observations
            .iter()
            .map(|obs| obs.parameters.clone())
            .collect();

        let y_data: Vec<f32> = self
            .observations
            .iter()
            .map(|obs| obs.objective_value)
            .collect();

        self.gaussian_process.fit(&x_data, &y_data);
    }

    /// Optimize acquisition function to find next evaluation point
    async fn optimize_acquisition_function(&self) -> Result<EmbedParams> {
        let mut best_acquisition = f32::NEG_INFINITY;
        let mut best_params = self.sample_random_parameters();

        // Multi-start random search for acquisition optimization
        for _ in 0..100 {
            let candidate_params = self.sample_random_parameters();
            let candidate_vector = self.embed_params_to_vector(&candidate_params);
            let acquisition_value = self.evaluate_acquisition_function(&candidate_vector);

            if acquisition_value > best_acquisition {
                best_acquisition = acquisition_value;
                best_params = candidate_params;
            }
        }

        Ok(best_params)
    }

    /// Evaluate acquisition function at given point
    fn evaluate_acquisition_function(&self, x: &[f32]) -> f32 {
        let (mean, variance) = self.gaussian_process.predict(x);
        let std_dev = variance.sqrt();

        match &self.acquisition_config.function_type {
            AcquisitionFunction::ExpectedImprovement => {
                let best_value = self
                    .observations
                    .iter()
                    .map(|obs| obs.objective_value)
                    .fold(f32::NEG_INFINITY, f32::max);

                if std_dev < 1e-6 {
                    0.0
                } else {
                    let z = (mean - best_value) / std_dev;
                    let phi = self.standard_normal_pdf(z);
                    let capital_phi = self.standard_normal_cdf(z);

                    (mean - best_value) * capital_phi + std_dev * phi
                }
            }

            AcquisitionFunction::UpperConfidenceBound { kappa } => mean + kappa * std_dev,

            AcquisitionFunction::ProbabilityOfImprovement => {
                let best_value = self
                    .observations
                    .iter()
                    .map(|obs| obs.objective_value)
                    .fold(f32::NEG_INFINITY, f32::max);

                if std_dev < 1e-6 {
                    0.0
                } else {
                    let z = (mean - best_value) / std_dev;
                    self.standard_normal_cdf(z)
                }
            }

            _ => mean + self.acquisition_config.exploration_weight * std_dev,
        }
    }

    /// Sample random parameters within bounds
    fn sample_random_parameters(&self) -> EmbedParams {
        use rand::random;

        EmbedParams {
            encryption_key: vec![],
            error_correction_level: match random::<u8>() % 5 {
                0 => ECLevel::None,
                1 => ECLevel::Low,
                2 => ECLevel::Medium,
                3 => ECLevel::High,
                _ => ECLevel::Maximum,
            },
            stealth_level: match random::<u8>() % 4 {
                0 => StealthLevel::Low,
                1 => StealthLevel::Medium,
                2 => StealthLevel::High,
                _ => StealthLevel::Maximum,
            },
            use_gpu: random::<bool>(),
            format_specific: HashMap::new(),
            enable_validation: false,
        }
    }

    /// Convert embedding parameters to vector for GP
    fn embed_params_to_vector(&self, params: &EmbedParams) -> Vec<f32> {
        vec![
            match params.error_correction_level {
                ECLevel::None => 0.0,
                ECLevel::Low => 0.25,
                ECLevel::Medium => 0.5,
                ECLevel::High => 0.75,
                ECLevel::Maximum => 1.0,
            },
            match params.stealth_level {
                StealthLevel::Low => 0.0,
                StealthLevel::Medium => 0.33,
                StealthLevel::High => 0.67,
                StealthLevel::Maximum => 1.0,
            },
            if params.use_gpu { 1.0 } else { 0.0 },
        ]
    }

    /// Convert vector to embedding parameters
    fn vector_to_embed_params(&self, vector: &[f32]) -> EmbedParams {
        let ec_level = match vector.first().copied().unwrap_or(0.5) {
            x if x < 0.125 => ECLevel::None,
            x if x < 0.375 => ECLevel::Low,
            x if x < 0.625 => ECLevel::Medium,
            x if x < 0.875 => ECLevel::High,
            _ => ECLevel::Maximum,
        };

        let stealth_level = match vector.get(1).copied().unwrap_or(0.5) {
            x if x < 0.167 => StealthLevel::Low,
            x if x < 0.5 => StealthLevel::Medium,
            x if x < 0.833 => StealthLevel::High,
            _ => StealthLevel::Maximum,
        };

        let use_gpu = vector.get(2).copied().unwrap_or(0.5) > 0.5;

        EmbedParams {
            encryption_key: vec![],
            error_correction_level: ec_level,
            stealth_level,
            use_gpu,
            format_specific: HashMap::new(),
            enable_validation: false,
        }
    }

    /// Optimise GP hyperparameters by maximising the log marginal likelihood.
    ///
    /// Uses gradient ascent with numerical finite-difference gradients in log-space
    /// (θ = exp(ψ)) so all three hyperparameters stay strictly positive throughout.
    /// After convergence the kernel enum fields and the internal Cholesky are updated
    /// so that subsequent `fit`/`predict` calls use the new parameters.
    fn optimize_hyperparameters(&mut self) {
        if self.gaussian_process.training_x.is_empty() {
            return;
        }

        let eps = 1e-3_f32;
        let lr  = 0.1_f32;
        let clip = 5.0_f32;

        // Work in log-space: ψ = ln(θ), θ = exp(ψ)
        let mut log_noise  = self.gaussian_process.hyperparams.noise_variance.max(1e-6).ln();
        let mut log_signal = self.gaussian_process.hyperparams.signal_variance.max(1e-6).ln();
        let mut log_length = self.gaussian_process.hyperparams.length_scale.max(1e-6).ln();

        for _ in 0..30 {
            let base_ll = self.calculate_log_likelihood(&GpHyperparams {
                noise_variance: log_noise.exp(),
                signal_variance: log_signal.exp(),
                length_scale: log_length.exp(),
                kernel_params: HashMap::new(),
            });

            if !base_ll.is_finite() {
                break;
            }

            // Numerical gradients ∂LL/∂ψ via forward finite differences
            let grad_noise = (self.calculate_log_likelihood(&GpHyperparams {
                noise_variance: (log_noise + eps).exp(),
                signal_variance: log_signal.exp(),
                length_scale: log_length.exp(),
                kernel_params: HashMap::new(),
            }) - base_ll) / eps;

            let grad_signal = (self.calculate_log_likelihood(&GpHyperparams {
                noise_variance: log_noise.exp(),
                signal_variance: (log_signal + eps).exp(),
                length_scale: log_length.exp(),
                kernel_params: HashMap::new(),
            }) - base_ll) / eps;

            let grad_length = (self.calculate_log_likelihood(&GpHyperparams {
                noise_variance: log_noise.exp(),
                signal_variance: log_signal.exp(),
                length_scale: (log_length + eps).exp(),
                kernel_params: HashMap::new(),
            }) - base_ll) / eps;

            // Gradient ascent with clipping and bounded log-space values
            log_noise  = (log_noise  + lr * grad_noise.clamp(-clip, clip)).clamp(-10.0, 4.0);
            log_signal = (log_signal + lr * grad_signal.clamp(-clip, clip)).clamp(-5.0, 5.0);
            log_length = (log_length + lr * grad_length.clamp(-clip, clip)).clamp(-5.0, 5.0);
        }

        let best = GpHyperparams {
            noise_variance: log_noise.exp(),
            signal_variance: log_signal.exp(),
            length_scale: log_length.exp(),
            kernel_params: HashMap::new(),
        };

        self.gaussian_process.hyperparams = best.clone();

        // Sync length_scale and signal_variance into the kernel enum so that
        // kernel_function() (which reads the enum fields) stays consistent.
        match &mut self.gaussian_process.kernel {
            KernelFunction::RBF { length_scale, signal_variance }
            | KernelFunction::Matern32 { length_scale, signal_variance }
            | KernelFunction::Matern52 { length_scale, signal_variance } => {
                *length_scale  = best.length_scale;
                *signal_variance = best.signal_variance;
            }
            KernelFunction::RationalQuadratic { length_scale, signal_variance, .. } => {
                *length_scale  = best.length_scale;
                *signal_variance = best.signal_variance;
            }
        }

        // Refit to rebuild Cholesky with the updated kernel parameters
        let x = self.gaussian_process.training_x.clone();
        let y = self.gaussian_process.training_y.clone();
        self.gaussian_process.fit(&x, &y);
    }

    /// Compute GP log marginal likelihood for a candidate set of hyperparameters.
    ///
    /// log p(y | X, θ) = −½ y^T α  −  Σ_i log L_{ii}  −  n/2 log(2π)
    ///
    /// where K + σ²I = L L^T (Cholesky) and α = K^{-1} y.
    fn calculate_log_likelihood(&self, hyperparams: &GpHyperparams) -> f32 {
        let x = &self.gaussian_process.training_x;
        let y = &self.gaussian_process.training_y;
        let n = x.len();

        if n == 0 {
            return 0.0;
        }

        // Build K(θ) + σ_noise² I using candidate hyperparams
        let mut k = vec![vec![0.0f32; n]; n];
        for i in 0..n {
            for j in 0..n {
                k[i][j] = self.gaussian_process.kernel_with_params(
                    &x[i],
                    &x[j],
                    hyperparams.signal_variance,
                    hyperparams.length_scale,
                );
                if i == j {
                    k[i][j] += hyperparams.noise_variance;
                }
            }
        }

        // Cholesky: K = L L^T
        let l = self.gaussian_process.cholesky_decomposition(&k);

        // Reject non-positive-definite matrices
        for (i, row) in l.iter().enumerate().take(n) {
            if row[i] <= 0.0 || !row[i].is_finite() {
                return f32::NEG_INFINITY;
            }
        }

        // α = K^{-1} y via Cholesky solve
        let alpha = self.gaussian_process.solve_cholesky(&l, y);

        // Data-fit term: −½ y^T α
        let data_fit: f32 = y.iter().zip(&alpha).map(|(yi, ai)| yi * ai).sum();

        // Complexity penalty: −Σ_i log(L_{ii})  (= −½ log|K|)
        let log_det: f32 = l.iter().enumerate().map(|(i, row)| row[i].ln()).sum();

        // Normalisation constant: −n/2 log(2π)
        let norm_const = (n as f32) * 0.5 * (2.0 * std::f32::consts::PI).ln();

        -0.5 * data_fit - log_det - norm_const
    }

    /// Check convergence based on improvement in recent iterations
    fn check_convergence(&self) -> bool {
        if self.observations.len() < 10 {
            return false;
        }

        let recent_values: Vec<f32> = self
            .observations
            .iter()
            .rev()
            .take(5)
            .map(|obs| obs.objective_value)
            .collect();

        let older_values: Vec<f32> = self
            .observations
            .iter()
            .rev()
            .skip(5)
            .take(5)
            .map(|obs| obs.objective_value)
            .collect();

        let recent_max = recent_values
            .iter()
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let older_max = older_values
            .iter()
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));

        (recent_max - older_max).abs() < self.config.tolerance
    }

    /// Standard normal probability density function
    fn standard_normal_pdf(&self, x: f32) -> f32 {
        (2.0 * std::f32::consts::PI).sqrt().recip() * (-0.5 * x * x).exp()
    }

    /// Standard normal cumulative distribution function (approximation)
    fn standard_normal_cdf(&self, x: f32) -> f32 {
        0.5 * (1.0 + self.erf(x / 2.0_f32.sqrt()))
    }

    /// Error function approximation
    fn erf(&self, x: f32) -> f32 {
        // Abramowitz and Stegun approximation
        let a1 = 0.254_829_6;
        let a2 = -0.284_496_72;
        let a3 = 1.421_413_8;
        let a4 = -1.453_152_1;
        let a5 = 1.061_405_4;
        let p = 0.3275911;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();

        let t = 1.0 / (1.0 + p * x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

        sign * y
    }
}

impl GaussianProcess {
    /// Create new Gaussian process
    pub fn new(kernel: KernelFunction) -> Self {
        Self {
            kernel,
            hyperparams: GpHyperparams::default(),
            training_x: Vec::new(),
            training_y: Vec::new(),
            cholesky_l: None,
            alpha: Vec::new(),
        }
    }

    /// Fit Gaussian process to training data
    pub fn fit(&mut self, x_data: &[Vec<f32>], y_data: &[f32]) {
        self.training_x = x_data.to_vec();
        self.training_y = y_data.to_vec();

        let n = x_data.len();
        if n == 0 {
            return;
        }

        // Compute covariance matrix
        let mut k_matrix = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in 0..n {
                k_matrix[i][j] = self.kernel_function(&x_data[i], &x_data[j]);
                if i == j {
                    k_matrix[i][j] += self.hyperparams.noise_variance;
                }
            }
        }

        // Cholesky decomposition
        self.cholesky_l = Some(self.cholesky_decomposition(&k_matrix));

        // Solve for alpha vector
        if let Some(ref l) = self.cholesky_l {
            self.alpha = self.solve_cholesky(l, y_data);
        }
    }

    /// Predict mean and variance at test point
    pub fn predict(&self, x_test: &[f32]) -> (f32, f32) {
        if self.training_x.is_empty() {
            return (0.0, self.hyperparams.signal_variance);
        }

        // k_* = covariance between test point and all training points
        let k_star: Vec<f32> = self
            .training_x
            .iter()
            .map(|x_train| self.kernel_function(x_test, x_train))
            .collect();

        // k** = prior variance at test point
        let k_star_star = self.kernel_function(x_test, x_test);

        // Posterior mean: k_*^T (K + σ²I)^{-1} y = k_*^T α
        let mean: f32 = k_star.iter().zip(&self.alpha).map(|(k, a)| k * a).sum();

        // Posterior variance: k** - k_*^T (K + σ²I)^{-1} k_*
        //   = k** - ||v||²  where  v = L^{-1} k_*
        let variance = if let Some(ref l) = self.cholesky_l {
            let v = self.forward_substitution(l, &k_star);
            let quad: f32 = v.iter().map(|vi| vi * vi).sum();
            (k_star_star - quad).max(1e-6)
        } else {
            self.hyperparams.signal_variance
        };

        (mean, variance)
    }

    /// Kernel function evaluation
    fn kernel_function(&self, x1: &[f32], x2: &[f32]) -> f32 {
        match &self.kernel {
            KernelFunction::RBF {
                length_scale,
                signal_variance,
            } => {
                let squared_distance = x1.iter().zip(x2).map(|(a, b)| (a - b).powi(2)).sum::<f32>();
                signal_variance * (-0.5 * squared_distance / (length_scale * length_scale)).exp()
            }

            KernelFunction::Matern32 {
                length_scale,
                signal_variance,
            } => {
                let distance = x1
                    .iter()
                    .zip(x2)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    .sqrt();
                let sqrt3_d_l = 3.0_f32.sqrt() * distance / length_scale;
                signal_variance * (1.0 + sqrt3_d_l) * (-sqrt3_d_l).exp()
            }

            KernelFunction::Matern52 {
                length_scale,
                signal_variance,
            } => {
                let distance = x1
                    .iter()
                    .zip(x2)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    .sqrt();
                let sqrt5_d_l = 5.0_f32.sqrt() * distance / length_scale;
                signal_variance
                    * (1.0
                        + sqrt5_d_l
                        + 5.0 * distance * distance / (3.0 * length_scale * length_scale))
                    * (-sqrt5_d_l).exp()
            }

            KernelFunction::RationalQuadratic {
                length_scale,
                signal_variance,
                alpha,
            } => {
                let squared_distance = x1.iter().zip(x2).map(|(a, b)| (a - b).powi(2)).sum::<f32>();
                signal_variance
                    * (1.0 + squared_distance / (2.0 * alpha * length_scale * length_scale))
                        .powf(-alpha)
            }
        }
    }

    /// Cholesky decomposition (simplified implementation)
    fn cholesky_decomposition(&self, matrix: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let n = matrix.len();
        let mut l = vec![vec![0.0; n]; n];

        for i in 0..n {
            for j in 0..=i {
                if i == j {
                    let sum: f32 = (0..j).map(|k| l[i][k] * l[i][k]).sum();
                    l[i][j] = (matrix[i][i] - sum).sqrt();
                } else {
                    let sum: f32 = (0..j).map(|k| l[i][k] * l[j][k]).sum();
                    l[i][j] = (matrix[i][j] - sum) / l[j][j];
                }
            }
        }

        l
    }

    /// Forward substitution only: solve L * v = b (first half of solve_cholesky)
    fn forward_substitution(&self, l: &[Vec<f32>], b: &[f32]) -> Vec<f32> {
        let n = l.len();
        let mut v = vec![0.0f32; n];
        for i in 0..n {
            let sum: f32 = (0..i).map(|j| l[i][j] * v[j]).sum();
            v[i] = if l[i][i].abs() > 1e-10 {
                (b[i] - sum) / l[i][i]
            } else {
                0.0
            };
        }
        v
    }

    /// Evaluate the kernel with explicit signal_variance and length_scale,
    /// using the same kernel *type* as `self.kernel` (ignores embedded field values).
    /// Used by log-likelihood optimisation to try candidate hyperparams.
    fn kernel_with_params(&self, x1: &[f32], x2: &[f32], signal_var: f32, length_scale: f32) -> f32 {
        let sq_dist: f32 = x1.iter().zip(x2).map(|(a, b)| (a - b).powi(2)).sum();
        match &self.kernel {
            KernelFunction::RBF { .. } => {
                signal_var * (-0.5 * sq_dist / (length_scale * length_scale)).exp()
            }
            KernelFunction::Matern32 { .. } => {
                let dist = sq_dist.sqrt();
                let t = 3.0_f32.sqrt() * dist / length_scale;
                signal_var * (1.0 + t) * (-t).exp()
            }
            KernelFunction::Matern52 { .. } => {
                let dist = sq_dist.sqrt();
                let t = 5.0_f32.sqrt() * dist / length_scale;
                signal_var
                    * (1.0 + t + 5.0 * sq_dist / (3.0 * length_scale * length_scale))
                    * (-t).exp()
            }
            KernelFunction::RationalQuadratic { alpha, .. } => {
                signal_var
                    * (1.0 + sq_dist / (2.0 * alpha * length_scale * length_scale)).powf(-alpha)
            }
        }
    }

    /// Solve linear system using Cholesky decomposition
    fn solve_cholesky(&self, l: &[Vec<f32>], b: &[f32]) -> Vec<f32> {
        let n = l.len();
        let mut y = vec![0.0; n];
        let mut x = vec![0.0; n];

        // Forward substitution: L * y = b
        for i in 0..n {
            let sum: f32 = (0..i).map(|j| l[i][j] * y[j]).sum();
            y[i] = (b[i] - sum) / l[i][i];
        }

        // Backward substitution: L^T * x = y
        for i in (0..n).rev() {
            let sum: f32 = (i + 1..n).map(|j| l[j][i] * x[j]).sum();
            x[i] = (y[i] - sum) / l[i][i];
        }

        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_process_creation() {
        let gp = GaussianProcess::new(KernelFunction::RBF {
            length_scale: 1.0,
            signal_variance: 1.0,
        });

        assert_eq!(gp.training_x.len(), 0);
        assert_eq!(gp.training_y.len(), 0);
    }

    #[test]
    fn test_bayesian_optimizer_creation() {
        let bounds = ParameterBounds {
            lower_bounds: vec![0.0, 0.0, 0.0],
            upper_bounds: vec![1.0, 1.0, 1.0],
            parameter_names: vec!["ec".to_string(), "stealth".to_string(), "gpu".to_string()],
        };

        let optimizer = BayesianOptimizer::new(bounds, BayesianConfig::default());
        assert_eq!(optimizer.observations.len(), 0);
    }

    #[test]
    fn test_embed_params_conversion() {
        let bounds = ParameterBounds {
            lower_bounds: vec![0.0, 0.0, 0.0],
            upper_bounds: vec![1.0, 1.0, 1.0],
            parameter_names: vec!["ec".to_string(), "stealth".to_string(), "gpu".to_string()],
        };

        let optimizer = BayesianOptimizer::new(bounds, BayesianConfig::default());

        let params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::High,
            stealth_level: StealthLevel::Medium,
            use_gpu: true,
            format_specific: HashMap::new(),
            enable_validation: false,
        };

        let vector = optimizer.embed_params_to_vector(&params);
        assert_eq!(vector.len(), 3);
        assert_eq!(vector[0], 0.75); // High EC level
        assert_eq!(vector[1], 0.33); // Medium stealth level
        assert_eq!(vector[2], 1.0); // GPU enabled

        let recovered_params = optimizer.vector_to_embed_params(&vector);
        assert!(matches!(
            recovered_params.error_correction_level,
            ECLevel::High
        ));
        assert!(matches!(
            recovered_params.stealth_level,
            StealthLevel::Medium
        ));
        assert!(recovered_params.use_gpu);
    }

    #[test]
    fn test_kernel_functions() {
        let gp = GaussianProcess::new(KernelFunction::RBF {
            length_scale: 1.0,
            signal_variance: 1.0,
        });

        let x1 = vec![0.0, 0.0];
        let x2 = vec![1.0, 1.0];

        let k_val = gp.kernel_function(&x1, &x2);
        assert!(k_val > 0.0 && k_val < 1.0); // RBF kernel properties

        let k_self = gp.kernel_function(&x1, &x1);
        assert!((k_self - 1.0).abs() < 1e-6); // Self-similarity should be 1
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_optimizer_with_data() -> BayesianOptimizer {
        let bounds = ParameterBounds {
            lower_bounds: vec![0.0, 0.0, 0.0],
            upper_bounds: vec![1.0, 1.0, 1.0],
            parameter_names: vec!["ec".to_string(), "stealth".to_string(), "gpu".to_string()],
        };
        let mut opt = BayesianOptimizer::new(bounds, BayesianConfig::default());
        // 5 observations along a parabola y = x*(1-x) in the first parameter dimension
        for i in 0..5 {
            let x = i as f32 / 4.0;
            opt.observations.push(Observation {
                parameters: vec![x, 0.5, 0.0],
                objective_value: x * (1.0 - x),
                noise_level: 0.01,
                timestamp: 0,
            });
        }
        opt.update_gaussian_process();
        opt
    }

    // ── log marginal likelihood ───────────────────────────────────────────────

    #[test]
    fn test_log_likelihood_is_finite_with_data() {
        let opt = make_optimizer_with_data();
        let ll = opt.calculate_log_likelihood(&GpHyperparams::default());
        assert!(
            ll.is_finite(),
            "Log marginal likelihood should be finite with training data, got {ll}"
        );
    }

    #[test]
    fn test_log_likelihood_is_not_placeholder() {
        // The old stub returned -noise_variance - length_scale regardless of data.
        // Verify the output depends on the actual training targets by checking that
        // a tiny-noise, well-scaled GP gives a meaningfully different value from
        // a wildly mismatched one.
        let opt = make_optimizer_with_data();

        let good = opt.calculate_log_likelihood(&GpHyperparams {
            noise_variance: 0.01,
            signal_variance: 1.0,
            length_scale: 0.5,
            kernel_params: HashMap::new(),
        });
        let bad = opt.calculate_log_likelihood(&GpHyperparams {
            noise_variance: 100.0,
            signal_variance: 0.001,
            length_scale: 1000.0,
            kernel_params: HashMap::new(),
        });

        assert!(
            good > bad,
            "Well-scaled hyperparams should yield higher log likelihood \
             (good={good:.3} bad={bad:.3})"
        );
    }

    // ── hyperparameter optimisation ───────────────────────────────────────────

    #[test]
    fn test_hyperparameter_optimization_does_not_decrease_likelihood() {
        let mut opt = make_optimizer_with_data();
        let ll_before =
            opt.calculate_log_likelihood(&opt.gaussian_process.hyperparams.clone());

        opt.optimize_hyperparameters();

        let ll_after =
            opt.calculate_log_likelihood(&opt.gaussian_process.hyperparams.clone());

        assert!(
            ll_after >= ll_before - 1e-2,
            "Hyperparameter optimisation should not decrease log likelihood \
             (before={ll_before:.4} after={ll_after:.4})"
        );
    }

    #[test]
    fn test_hyperparameter_optimization_updates_kernel_fields() {
        let mut opt = make_optimizer_with_data();
        // Deliberately misset kernel fields so we can check they get updated
        opt.gaussian_process.kernel = KernelFunction::RBF {
            length_scale: 99.0,
            signal_variance: 99.0,
        };
        opt.optimize_hyperparameters();

        // After optimisation the kernel fields should match hyperparams
        if let KernelFunction::RBF { length_scale, signal_variance } =
            opt.gaussian_process.kernel
        {
            assert!(
                (length_scale - opt.gaussian_process.hyperparams.length_scale).abs() < 1e-5,
                "Kernel length_scale not synced: kernel={length_scale} hyperparams={}",
                opt.gaussian_process.hyperparams.length_scale
            );
            assert!(
                (signal_variance - opt.gaussian_process.hyperparams.signal_variance).abs() < 1e-5,
                "Kernel signal_variance not synced"
            );
        }
    }

    // ── predict variance ─────────────────────────────────────────────────────

    #[test]
    fn test_predict_variance_decreases_near_data() {
        let opt = make_optimizer_with_data();

        // Near the middle training point (x=[0.5,0.5,0]) variance should be low
        let (_, near_var) = opt.gaussian_process.predict(&[0.5, 0.5, 0.0]);

        // Far outside the training distribution variance should be higher
        let (_, far_var) = opt.gaussian_process.predict(&[10.0, 10.0, 10.0]);

        assert!(
            near_var < far_var,
            "Posterior variance near data ({near_var:.5}) should be less than \
             variance far from data ({far_var:.5})"
        );
    }

    #[test]
    fn test_predict_mean_interpolates_data() {
        // With very small noise the posterior mean should be close to the
        // training target at an observed input.
        let mut gp = GaussianProcess::new(KernelFunction::RBF {
            length_scale: 0.5,
            signal_variance: 1.0,
        });
        gp.hyperparams.noise_variance = 1e-4;

        let x_data = vec![vec![0.0f32], vec![0.5], vec![1.0]];
        let y_data = vec![0.0_f32, 0.25, 0.0];
        gp.fit(&x_data, &y_data);

        let (mean, _) = gp.predict(&[0.5]);
        assert!(
            (mean - 0.25).abs() < 0.05,
            "GP mean at training point should ≈ training target: got {mean:.4}"
        );
    }
}
