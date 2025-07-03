//! Training infrastructure for ML models
//! 
//! This module provides training pipelines, data management, and
//! evaluation utilities for machine learning models.

use crate::core::*;
use super::*;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Training pipeline for ML models
#[derive(Debug)]
pub struct TrainingPipeline {
    /// Training configuration
    config: TrainingConfig,
    
    /// Data manager for training/validation splits
    data_manager: DataManager,
    
    /// Metrics tracker for training progress
    metrics_tracker: MetricsTracker,
    
    /// Model validators
    validators: Vec<Box<dyn ModelValidator>>,
}

/// Configuration for model training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// Batch size for training
    pub batch_size: usize,
    
    /// Number of training epochs
    pub epochs: usize,
    
    /// Validation split ratio (0.0-1.0)
    pub validation_split: f32,
    
    /// Early stopping patience
    pub early_stopping_patience: usize,
    
    /// Learning rate scheduling
    pub learning_rate_schedule: LearningRateSchedule,
    
    /// Cross-validation folds
    pub cross_validation_folds: usize,
    
    /// Enable data augmentation
    pub data_augmentation: bool,
    
    /// Regularization parameters
    pub regularization: RegularizationConfig,
}

/// Learning rate scheduling options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearningRateSchedule {
    /// Constant learning rate
    Constant(f32),
    
    /// Exponential decay
    ExponentialDecay { initial: f32, decay_rate: f32, decay_steps: usize },
    
    /// Step decay
    StepDecay { initial: f32, drop_rate: f32, epochs_drop: usize },
    
    /// Adaptive based on loss plateau
    ReduceOnPlateau { initial: f32, factor: f32, patience: usize },
}

/// Regularization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegularizationConfig {
    /// L1 regularization weight
    pub l1_weight: f32,
    
    /// L2 regularization weight
    pub l2_weight: f32,
    
    /// Dropout rate for neural networks
    pub dropout_rate: f32,
    
    /// Enable batch normalization
    pub batch_normalization: bool,
}

/// Data management for training
#[derive(Debug)]
pub struct DataManager {
    /// All training examples
    training_data: Vec<TrainingExample>,
    
    /// Training set indices
    train_indices: Vec<usize>,
    
    /// Validation set indices
    validation_indices: Vec<usize>,
    
    /// Test set indices
    test_indices: Vec<usize>,
    
    /// Data augmentation strategies
    augmentation_strategies: Vec<Box<dyn DataAugmentation>>,
}

/// Data augmentation trait
pub trait DataAugmentation: std::fmt::Debug + Send + Sync {
    /// Apply augmentation to training example
    fn augment(&self, example: &TrainingExample) -> Vec<TrainingExample>;
    
    /// Get augmentation name
    fn name(&self) -> &str;
}

/// Noise injection augmentation
#[derive(Debug)]
pub struct NoiseAugmentation {
    noise_level: f32,
}

/// Feature scaling augmentation
#[derive(Debug)]
pub struct ScalingAugmentation {
    scale_range: (f32, f32),
}

/// Metrics tracking during training
#[derive(Debug)]
pub struct MetricsTracker {
    /// Training loss history
    training_loss: Vec<f32>,
    
    /// Validation loss history
    validation_loss: Vec<f32>,
    
    /// Training accuracy history
    training_accuracy: Vec<f32>,
    
    /// Validation accuracy history
    validation_accuracy: Vec<f32>,
    
    /// Custom metrics
    custom_metrics: HashMap<String, Vec<f32>>,
    
    /// Best validation score
    best_validation_score: f32,
    
    /// Epochs since improvement
    epochs_without_improvement: usize,
}

/// Model validation trait
pub trait ModelValidator: std::fmt::Debug + Send + Sync {
    /// Validate model performance
    fn validate(&self, model: &dyn MLModel, test_data: &[TrainingExample]) -> Result<ValidationResult>;
    
    /// Get validator name
    fn name(&self) -> &str;
}

/// Cross-validation validator
#[derive(Debug)]
pub struct CrossValidator {
    folds: usize,
}

/// Hold-out validation
#[derive(Debug)]
pub struct HoldOutValidator {
    test_ratio: f32,
}

/// Bootstrap validation
#[derive(Debug)]
pub struct BootstrapValidator {
    num_samples: usize,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Validation score
    pub score: f32,
    
    /// Standard deviation of score
    pub score_std: Option<f32>,
    
    /// Detailed metrics
    pub metrics: HashMap<String, f32>,
    
    /// Validation method used
    pub method: String,
    
    /// Confidence interval
    pub confidence_interval: Option<(f32, f32)>,
}

/// Training session state
#[derive(Debug)]
pub struct TrainingSession {
    /// Current epoch
    current_epoch: usize,
    
    /// Training start time
    start_time: std::time::Instant,
    
    /// Current learning rate
    current_learning_rate: f32,
    
    /// Best model state
    best_model_state: Option<String>,
    
    /// Early stopping flag
    should_stop: bool,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            epochs: 100,
            validation_split: 0.2,
            early_stopping_patience: 10,
            learning_rate_schedule: LearningRateSchedule::Constant(0.01),
            cross_validation_folds: 5,
            data_augmentation: true,
            regularization: RegularizationConfig {
                l1_weight: 0.0,
                l2_weight: 0.001,
                dropout_rate: 0.1,
                batch_normalization: true,
            },
        }
    }
}

impl TrainingPipeline {
    /// Create new training pipeline
    pub fn new(config: TrainingConfig) -> Self {
        Self {
            config,
            data_manager: DataManager::new(),
            metrics_tracker: MetricsTracker::new(),
            validators: vec![
                Box::new(CrossValidator::new(5)),
                Box::new(HoldOutValidator::new(0.2)),
            ],
        }
    }
    
    /// Train model with provided data
    pub fn train_model(
        &mut self,
        model: &mut dyn MLModel,
        training_data: Vec<TrainingExample>,
    ) -> Result<TrainingSession> {
        // Prepare data
        self.data_manager.load_data(training_data);
        self.data_manager.split_data(self.config.validation_split);
        
        let mut session = TrainingSession {
            current_epoch: 0,
            start_time: std::time::Instant::now(),
            current_learning_rate: self.get_initial_learning_rate(),
            best_model_state: None,
            should_stop: false,
        };
        
        // Training loop
        for epoch in 0..self.config.epochs {
            session.current_epoch = epoch;
            
            // Update learning rate
            session.current_learning_rate = self.update_learning_rate(epoch, &session);
            
            // Train on batches
            let train_loss = self.train_epoch(model, &session)?;
            
            // Validate
            let val_loss = self.validate_epoch(model)?;
            
            // Update metrics
            self.metrics_tracker.record_epoch(train_loss, val_loss, 0.0, 0.0);
            
            // Check for early stopping
            if self.should_early_stop(&session) {
                session.should_stop = true;
                break;
            }
            
            // Save best model
            if val_loss < self.metrics_tracker.best_validation_score {
                self.metrics_tracker.best_validation_score = val_loss;
                session.best_model_state = Some(format!("epoch_{}", epoch));
                self.metrics_tracker.epochs_without_improvement = 0;
            } else {
                self.metrics_tracker.epochs_without_improvement += 1;
            }
        }
        
        Ok(session)
    }
    
    /// Validate model using all validators
    pub fn validate_model(&self, model: &dyn MLModel) -> Result<Vec<ValidationResult>> {
        let test_data = self.data_manager.get_test_data();
        let mut results = Vec::new();
        
        for validator in &self.validators {
            let result = validator.validate(model, &test_data)?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// Perform hyperparameter search
    pub fn hyperparameter_search(
        &mut self,
        model_factory: &dyn Fn(&HashMap<String, f32>) -> Box<dyn MLModel>,
        param_grid: HashMap<String, Vec<f32>>,
    ) -> Result<(HashMap<String, f32>, f32)> {
        let mut best_params = HashMap::new();
        let mut best_score = f32::NEG_INFINITY;
        
        // Generate parameter combinations (grid search)
        let combinations = self.generate_param_combinations(&param_grid);
        
        for params in combinations {
            let mut model = model_factory(&params);
            
            // Train model with current parameters
            let training_data = self.data_manager.get_training_data();
            self.train_model(model.as_mut(), training_data)?;
            
            // Validate
            let validation_results = self.validate_model(model.as_ref())?;
            let avg_score = validation_results.iter().map(|r| r.score).sum::<f32>() 
                / validation_results.len() as f32;
            
            if avg_score > best_score {
                best_score = avg_score;
                best_params = params;
            }
        }
        
        Ok((best_params, best_score))
    }
    
    /// Train single epoch
    fn train_epoch(&mut self, model: &mut dyn MLModel, _session: &TrainingSession) -> Result<f32> {
        let train_data = self.data_manager.get_training_data();
        let mut total_loss = 0.0;
        let mut batch_count = 0;
        
        // Process in batches
        for batch in train_data.chunks(self.config.batch_size) {
            for example in batch {
                model.update(example)?;
            }
            
            // Calculate batch loss (simplified)
            total_loss += 0.1; // Placeholder
            batch_count += 1;
        }
        
        Ok(total_loss / batch_count as f32)
    }
    
    /// Validate single epoch
    fn validate_epoch(&self, model: &dyn MLModel) -> Result<f32> {
        let val_data = self.data_manager.get_validation_data();
        
        // Evaluate on validation set
        let performance = model.evaluate(&val_data)?;
        Ok(performance.capacity_mse + performance.time_mse) // Combined loss
    }
    
    /// Check if training should stop early
    fn should_early_stop(&self, _session: &TrainingSession) -> bool {
        self.metrics_tracker.epochs_without_improvement >= self.config.early_stopping_patience
    }
    
    /// Get initial learning rate from schedule
    fn get_initial_learning_rate(&self) -> f32 {
        match &self.config.learning_rate_schedule {
            LearningRateSchedule::Constant(lr) => *lr,
            LearningRateSchedule::ExponentialDecay { initial, .. } => *initial,
            LearningRateSchedule::StepDecay { initial, .. } => *initial,
            LearningRateSchedule::ReduceOnPlateau { initial, .. } => *initial,
        }
    }
    
    /// Update learning rate based on schedule
    fn update_learning_rate(&self, epoch: usize, session: &TrainingSession) -> f32 {
        match &self.config.learning_rate_schedule {
            LearningRateSchedule::Constant(lr) => *lr,
            LearningRateSchedule::ExponentialDecay { initial, decay_rate, decay_steps } => {
                initial * decay_rate.powf((epoch / decay_steps) as f32)
            },
            LearningRateSchedule::StepDecay { initial, drop_rate, epochs_drop } => {
                initial * drop_rate.powf((epoch / epochs_drop) as f32)
            },
            LearningRateSchedule::ReduceOnPlateau { factor, .. } => {
                // Reduce LR if no improvement
                if self.metrics_tracker.epochs_without_improvement > 0 {
                    session.current_learning_rate * factor
                } else {
                    session.current_learning_rate
                }
            },
        }
    }
    
    /// Generate parameter combinations for grid search
    fn generate_param_combinations(&self, param_grid: &HashMap<String, Vec<f32>>) -> Vec<HashMap<String, f32>> {
        let mut combinations = vec![HashMap::new()];
        
        for (param_name, values) in param_grid {
            let mut new_combinations = Vec::new();
            
            for combination in combinations {
                for &value in values {
                    let mut new_combination = combination.clone();
                    new_combination.insert(param_name.clone(), value);
                    new_combinations.push(new_combination);
                }
            }
            
            combinations = new_combinations;
        }
        
        combinations
    }
}

impl DataManager {
    /// Create new data manager
    pub fn new() -> Self {
        Self {
            training_data: Vec::new(),
            train_indices: Vec::new(),
            validation_indices: Vec::new(),
            test_indices: Vec::new(),
            augmentation_strategies: Vec::new(),
        }
    }
    
    /// Load training data
    pub fn load_data(&mut self, data: Vec<TrainingExample>) {
        self.training_data = data;
    }
    
    /// Split data into train/validation/test sets
    pub fn split_data(&mut self, validation_split: f32) {
        let total_size = self.training_data.len();
        let val_size = (total_size as f32 * validation_split) as usize;
        let test_size = val_size; // Same size for test set
        
        // Create shuffled indices
        let mut indices: Vec<usize> = (0..total_size).collect();
        use rand::seq::SliceRandom;
        indices.shuffle(&mut rand::thread_rng());
        
        // Split indices
        self.test_indices = indices[0..test_size].to_vec();
        self.validation_indices = indices[test_size..test_size + val_size].to_vec();
        self.train_indices = indices[test_size + val_size..].to_vec();
    }
    
    /// Get training data
    pub fn get_training_data(&self) -> Vec<TrainingExample> {
        self.train_indices.iter()
            .map(|&i| self.training_data[i].clone())
            .collect()
    }
    
    /// Get validation data
    pub fn get_validation_data(&self) -> Vec<TrainingExample> {
        self.validation_indices.iter()
            .map(|&i| self.training_data[i].clone())
            .collect()
    }
    
    /// Get test data
    pub fn get_test_data(&self) -> Vec<TrainingExample> {
        self.test_indices.iter()
            .map(|&i| self.training_data[i].clone())
            .collect()
    }
    
    /// Add augmentation strategy
    pub fn add_augmentation(&mut self, strategy: Box<dyn DataAugmentation>) {
        self.augmentation_strategies.push(strategy);
    }
    
    /// Apply data augmentation
    pub fn augment_data(&self, data: &[TrainingExample]) -> Vec<TrainingExample> {
        let mut augmented = data.to_vec();
        
        for strategy in &self.augmentation_strategies {
            for example in data {
                let new_examples = strategy.augment(example);
                augmented.extend(new_examples);
            }
        }
        
        augmented
    }
}

impl MetricsTracker {
    /// Create new metrics tracker
    pub fn new() -> Self {
        Self {
            training_loss: Vec::new(),
            validation_loss: Vec::new(),
            training_accuracy: Vec::new(),
            validation_accuracy: Vec::new(),
            custom_metrics: HashMap::new(),
            best_validation_score: f32::INFINITY,
            epochs_without_improvement: 0,
        }
    }
    
    /// Record metrics for an epoch
    pub fn record_epoch(&mut self, train_loss: f32, val_loss: f32, train_acc: f32, val_acc: f32) {
        self.training_loss.push(train_loss);
        self.validation_loss.push(val_loss);
        self.training_accuracy.push(train_acc);
        self.validation_accuracy.push(val_acc);
    }
    
    /// Add custom metric
    pub fn add_custom_metric(&mut self, name: String, value: f32) {
        self.custom_metrics.entry(name).or_insert_with(Vec::new).push(value);
    }
    
    /// Get training summary
    pub fn get_summary(&self) -> TrainingSummary {
        TrainingSummary {
            total_epochs: self.training_loss.len(),
            best_validation_loss: self.best_validation_score,
            final_training_loss: self.training_loss.last().copied().unwrap_or(0.0),
            final_validation_loss: self.validation_loss.last().copied().unwrap_or(0.0),
            converged: self.epochs_without_improvement < 5, // Arbitrary threshold
        }
    }
}

/// Training summary
#[derive(Debug, Clone)]
pub struct TrainingSummary {
    pub total_epochs: usize,
    pub best_validation_loss: f32,
    pub final_training_loss: f32,
    pub final_validation_loss: f32,
    pub converged: bool,
}

impl NoiseAugmentation {
    pub fn new(noise_level: f32) -> Self {
        Self { noise_level }
    }
}

impl DataAugmentation for NoiseAugmentation {
    fn augment(&self, example: &TrainingExample) -> Vec<TrainingExample> {
        let mut augmented = example.clone();
        
        // Add noise to features
        for value in &mut augmented.carrier_features.frequency_characteristics {
            *value += (rand::random::<f32>() - 0.5) * self.noise_level;
        }
        
        vec![augmented]
    }
    
    fn name(&self) -> &str {
        "NoiseAugmentation"
    }
}

impl CrossValidator {
    pub fn new(folds: usize) -> Self {
        Self { folds }
    }
}

impl ModelValidator for CrossValidator {
    fn validate(&self, model: &dyn MLModel, test_data: &[TrainingExample]) -> Result<ValidationResult> {
        let fold_size = test_data.len() / self.folds;
        let mut scores = Vec::new();
        
        for fold in 0..self.folds {
            let start = fold * fold_size;
            let end = if fold == self.folds - 1 { test_data.len() } else { (fold + 1) * fold_size };
            
            let fold_data = &test_data[start..end];
            let performance = model.evaluate(fold_data)?;
            
            // Use combined metric as score
            let score = performance.strategy_accuracy;
            scores.push(score);
        }
        
        let mean_score = scores.iter().sum::<f32>() / scores.len() as f32;
        let variance = scores.iter().map(|s| (s - mean_score).powi(2)).sum::<f32>() / scores.len() as f32;
        let std_dev = variance.sqrt();
        
        Ok(ValidationResult {
            score: mean_score,
            score_std: Some(std_dev),
            metrics: HashMap::new(),
            method: format!("{}-fold cross-validation", self.folds),
            confidence_interval: Some((mean_score - 1.96 * std_dev, mean_score + 1.96 * std_dev)),
        })
    }
    
    fn name(&self) -> &str {
        "CrossValidator"
    }
}

impl HoldOutValidator {
    pub fn new(test_ratio: f32) -> Self {
        Self { test_ratio }
    }
}

impl ModelValidator for HoldOutValidator {
    fn validate(&self, model: &dyn MLModel, test_data: &[TrainingExample]) -> Result<ValidationResult> {
        let performance = model.evaluate(test_data)?;
        
        Ok(ValidationResult {
            score: performance.strategy_accuracy,
            score_std: None,
            metrics: HashMap::new(),
            method: format!("Hold-out validation ({:.1}%)", self.test_ratio * 100.0),
            confidence_interval: None,
        })
    }
    
    fn name(&self) -> &str {
        "HoldOutValidator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[test]
    fn test_training_config_default() {
        let config = TrainingConfig::default();
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.epochs, 100);
        assert_eq!(config.validation_split, 0.2);
    }
    
    #[test]
    fn test_data_manager_split() {
        let mut manager = DataManager::new();
        
        // Create dummy training data
        let data: Vec<TrainingExample> = (0..100).map(|i| TrainingExample {
            carrier_features: CarrierFeatures {
                format: DataFormat::PNG,
                size: i,
                entropy: 0.5,
                frequency_characteristics: vec![],
                complexity_measures: vec![],
                format_specific: HashMap::new(),
            },
            embed_params: EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
            },
            performance: PerformanceMetrics {
                capacity_efficiency: 0.5,
                embedding_time_ms: 100,
                extraction_accuracy: 0.9,
                quality_preservation: 0.8,
                memory_usage: 1000,
                gpu_acceleration_benefit: 1.0,
            },
            detection_resistance: 0.8,
            timestamp: 0,
        }).collect();
        
        manager.load_data(data);
        manager.split_data(0.2);
        
        assert!(!manager.train_indices.is_empty());
        assert!(!manager.validation_indices.is_empty());
        assert!(!manager.test_indices.is_empty());
        
        // Check that all indices are accounted for
        let total_indices = manager.train_indices.len() + 
                           manager.validation_indices.len() + 
                           manager.test_indices.len();
        assert_eq!(total_indices, 100);
    }
    
    #[test]
    fn test_metrics_tracker() {
        let mut tracker = MetricsTracker::new();
        
        tracker.record_epoch(0.5, 0.6, 0.8, 0.7);
        tracker.record_epoch(0.4, 0.5, 0.85, 0.75);
        
        assert_eq!(tracker.training_loss.len(), 2);
        assert_eq!(tracker.validation_loss.len(), 2);
        
        let summary = tracker.get_summary();
        assert_eq!(summary.total_epochs, 2);
    }
    
    #[test]
    fn test_noise_augmentation() {
        let augmentation = NoiseAugmentation::new(0.1);
        
        let example = TrainingExample {
            carrier_features: CarrierFeatures {
                format: DataFormat::PNG,
                size: 1000,
                entropy: 7.5,
                frequency_characteristics: vec![0.5, 0.3, 0.7],
                complexity_measures: vec![],
                format_specific: HashMap::new(),
            },
            embed_params: EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
            },
            performance: PerformanceMetrics {
                capacity_efficiency: 0.5,
                embedding_time_ms: 100,
                extraction_accuracy: 0.9,
                quality_preservation: 0.8,
                memory_usage: 1000,
                gpu_acceleration_benefit: 1.0,
            },
            detection_resistance: 0.8,
            timestamp: 0,
        };
        
        let augmented = augmentation.augment(&example);
        assert_eq!(augmented.len(), 1);
        assert_eq!(augmented[0].carrier_features.frequency_characteristics.len(), 3);
    }
}