//! Machine learning models for steganography optimization
//!
//! This module implements various ML models for predicting optimal embedding
//! strategies based on carrier characteristics and historical performance.

use super::*;
use crate::core::*;
use std::collections::HashMap;

/// Neural network model for strategy prediction
#[derive(Debug)]
pub struct NeuralNetworkModel {
    /// Network layers (weights and biases)
    layers: Vec<NetworkLayer>,

    /// Learning rate for training
    learning_rate: f32,

    /// Training history
    training_history: Vec<TrainingExample>,

    /// Model performance metrics
    performance: ModelPerformance,

    /// Input feature normalization parameters
    normalization: FeatureNormalization,
}

/// Single layer in neural network
#[derive(Debug, Clone)]
pub struct NetworkLayer {
    /// Weight matrix
    weights: Vec<Vec<f32>>,

    /// Bias vector
    biases: Vec<f32>,

    /// Activation function type
    activation: ActivationFunction,
}

/// Activation function types
#[derive(Debug, Clone, Copy)]
pub enum ActivationFunction {
    ReLU,
    Sigmoid,
    Tanh,
    Linear,
}

/// Feature normalization parameters
#[derive(Debug, Clone)]
pub struct FeatureNormalization {
    /// Mean values for each feature
    means: Vec<f32>,

    /// Standard deviations for each feature
    std_devs: Vec<f32>,

    /// Feature names for reference
    _feature_names: Vec<String>,
}

impl Default for FeatureNormalization {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureNormalization {
    /// Create new empty feature normalization
    pub fn new() -> Self {
        Self {
            means: Vec::new(),
            std_devs: Vec::new(),
            _feature_names: Vec::new(),
        }
    }

    /// Fit normalization parameters from a matrix of feature vectors (rows = samples).
    pub fn fit(&mut self, feature_matrix: &[Vec<f32>]) {
        if feature_matrix.is_empty() {
            return;
        }
        let n_features = feature_matrix[0].len();
        let n = feature_matrix.len() as f32;

        self.means = vec![0.0; n_features];
        self.std_devs = vec![1.0; n_features];

        // Compute means
        for row in feature_matrix {
            for (i, &v) in row.iter().enumerate().take(n_features) {
                self.means[i] += v;
            }
        }
        for m in &mut self.means {
            *m /= n;
        }

        // Compute standard deviations
        let mut variances = vec![0.0f32; n_features];
        for row in feature_matrix {
            for (i, &v) in row.iter().enumerate().take(n_features) {
                variances[i] += (v - self.means[i]).powi(2);
            }
        }
        for (i, var) in variances.into_iter().enumerate() {
            self.std_devs[i] = (var / n).sqrt().max(1e-8);
        }
    }

    /// Apply z-score normalization: (x - mean) / std.
    /// If no parameters have been fitted, returns `input` unchanged.
    pub fn normalize(&self, input: &[f32]) -> Vec<f32> {
        if self.means.is_empty() {
            return input.to_vec();
        }
        input
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let mean = self.means.get(i).copied().unwrap_or(0.0);
                let std = self.std_devs.get(i).copied().unwrap_or(1.0);
                (v - mean) / std
            })
            .collect()
    }
}

/// Decision tree model for interpretable strategy selection
#[derive(Debug)]
pub struct DecisionTreeModel {
    /// Root node of the decision tree
    root: Option<TreeNode>,

    /// Maximum tree depth
    max_depth: usize,

    /// Minimum samples per leaf
    min_samples_leaf: usize,

    /// Training examples used to build tree
    training_data: Vec<TrainingExample>,
}

/// Node in decision tree
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Feature index for split (if internal node)
    _feature_index: Option<usize>,

    /// Threshold value for split
    _threshold: Option<f32>,

    /// Left child (feature <= threshold)
    _left: Option<Box<TreeNode>>,

    /// Right child (feature > threshold)
    _right: Option<Box<TreeNode>>,

    /// Predicted parameters (if leaf node)
    prediction: Option<EmbedParams>,

    /// Confidence in prediction
    _confidence: f32,

    /// Number of training samples at this node
    _sample_count: usize,
}

/// Random forest model combining multiple decision trees
#[derive(Debug)]
pub struct RandomForestModel {
    /// Collection of decision trees
    _trees: Vec<DecisionTreeModel>,

    /// Number of trees in forest
    _num_trees: usize,

    /// Fraction of features to consider for each tree
    _feature_fraction: f32,

    /// Random seed for reproducibility
    _random_seed: u64,
}

/// Linear regression model for performance prediction
#[derive(Debug)]
pub struct LinearRegressionModel {
    /// Regression coefficients
    _coefficients: Vec<f32>,

    /// Intercept term
    _intercept: f32,

    /// Feature normalization
    _normalization: FeatureNormalization,

    /// R-squared score
    _r_squared: f32,
}

/// K-means clustering for carrier similarity analysis
#[derive(Debug)]
pub struct ClusteringModel {
    /// Cluster centroids
    _centroids: Vec<Vec<f32>>,

    /// Number of clusters
    _num_clusters: usize,

    /// Optimal strategies for each cluster
    _cluster_strategies: HashMap<usize, EmbedParams>,

    /// Cluster performance metrics
    _cluster_performance: HashMap<usize, PerformanceMetrics>,
}

impl NeuralNetworkModel {
    /// Create new neural network with specified architecture
    pub fn new(layer_sizes: &[usize], learning_rate: f32) -> Self {
        let mut layers = Vec::new();

        for i in 0..layer_sizes.len() - 1 {
            let input_size = layer_sizes[i];
            let output_size = layer_sizes[i + 1];

            // Initialize weights with Xavier initialization
            let mut weights = vec![vec![0.0; input_size]; output_size];
            for row in &mut weights {
                for weight in row {
                    *weight = (rand::random::<f32>() - 0.5) * 2.0 / (input_size as f32).sqrt();
                }
            }

            let biases = vec![0.0; output_size];
            let activation = if i == layer_sizes.len() - 2 {
                ActivationFunction::Linear // Output layer
            } else {
                ActivationFunction::ReLU // Hidden layers
            };

            layers.push(NetworkLayer {
                weights,
                biases,
                activation,
            });
        }

        Self {
            layers,
            learning_rate,
            training_history: Vec::new(),
            performance: ModelPerformance {
                capacity_mse: f32::INFINITY,
                time_mse: f32::INFINITY,
                strategy_accuracy: 0.0,
                r_squared: 0.0,
                sample_size: 0,
            },
            normalization: FeatureNormalization::new(),
        }
    }

    /// Forward pass through network
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let mut current = input.to_vec();

        for layer in &self.layers {
            current = self.layer_forward(&current, layer);
        }

        current
    }

    /// Forward pass through single layer
    fn layer_forward(&self, input: &[f32], layer: &NetworkLayer) -> Vec<f32> {
        let mut output = vec![0.0; layer.weights.len()];

        for (i, (weights, &bias)) in layer.weights.iter().zip(&layer.biases).enumerate() {
            let mut sum = bias;
            for (weight, &input_val) in weights.iter().zip(input) {
                sum += weight * input_val;
            }

            output[i] = match layer.activation {
                ActivationFunction::ReLU => sum.max(0.0),
                ActivationFunction::Sigmoid => 1.0 / (1.0 + (-sum).exp()),
                ActivationFunction::Tanh => sum.tanh(),
                ActivationFunction::Linear => sum,
            };
        }

        output
    }

    /// Convert features to input vector
    fn features_to_input(&self, features: &CarrierFeatures) -> Vec<f32> {
        let mut input = vec![features.size as f32, features.entropy];

        // Add frequency characteristics
        input.extend(&features.frequency_characteristics);

        // Add complexity measures
        input.extend(&features.complexity_measures);

        // Add format-specific features
        for value in features.format_specific.values() {
            input.push(*value);
        }

        // Normalize if parameters are available
        if !self.normalization.means.is_empty() {
            for (i, value) in input.iter_mut().enumerate() {
                if i < self.normalization.means.len() {
                    *value =
                        (*value - self.normalization.means[i]) / self.normalization.std_devs[i];
                }
            }
        }

        input
    }

    /// Convert output vector to embedding parameters
    fn output_to_params(&self, output: &[f32]) -> EmbedParams {
        // Simple mapping for demonstration
        let error_correction = if output.first().unwrap_or(&0.0) > &0.5 {
            ECLevel::High
        } else {
            ECLevel::Low
        };

        let stealth_level = if output.get(1).unwrap_or(&0.0) > &0.5 {
            StealthLevel::High
        } else {
            StealthLevel::Low
        };

        let use_gpu = output.get(2).unwrap_or(&0.0) > &0.5;

        EmbedParams {
            encryption_key: vec![],
            error_correction_level: error_correction,
            stealth_level,
            use_gpu,
            format_specific: HashMap::new(),
            enable_validation: false,
            cipher_kind: Default::default(),
        }
    }

    /// Train network on batch of examples (online SGD — one update per example)
    fn train_batch(&mut self, examples: &[TrainingExample]) {
        for example in examples {
            let input = self.features_to_input(&example.carrier_features);
            let target = self.params_to_target(&example.embed_params);
            let output = self.forward(&input);
            let loss = self.calculate_loss(&output, &target);
            self.backpropagate(&input, &output, &target, loss);
        }
    }

    /// Convert parameters to target vector
    fn params_to_target(&self, params: &EmbedParams) -> Vec<f32> {
        vec![
            if matches!(params.error_correction_level, ECLevel::High) {
                1.0
            } else {
                0.0
            },
            if matches!(params.stealth_level, StealthLevel::High) {
                1.0
            } else {
                0.0
            },
            if params.use_gpu { 1.0 } else { 0.0 },
        ]
    }

    /// Calculate mean squared error loss
    fn calculate_loss(&self, output: &[f32], target: &[f32]) -> f32 {
        output
            .iter()
            .zip(target)
            .map(|(o, t)| (o - t).powi(2))
            .sum::<f32>()
            / output.len() as f32
    }

    /// Forward pass collecting every layer's output, including the raw input.
    /// Returns `activations` where `activations[k]` is the input to `layers[k]`
    /// and `activations[n_layers]` is the final network output.
    fn forward_all_layers(&self, input: &[f32]) -> Vec<Vec<f32>> {
        let mut activations = Vec::with_capacity(self.layers.len() + 1);
        activations.push(input.to_vec());
        let mut current = input.to_vec();
        for layer in &self.layers {
            current = self.layer_forward(&current, layer);
            activations.push(current.clone());
        }
        activations
    }

    /// Point-wise activation derivative computed from the post-activation value `a`.
    /// This avoids storing pre-activation values:
    /// - ReLU: 1 if a > 0 else 0  (since relu(z) > 0 ↔ z > 0)
    /// - Sigmoid: a*(1-a)
    /// - Tanh: 1 - a²
    /// - Linear: 1
    fn activation_derivative(activation: ActivationFunction, a: f32) -> f32 {
        match activation {
            ActivationFunction::ReLU => if a > 0.0 { 1.0 } else { 0.0 },
            ActivationFunction::Sigmoid => a * (1.0 - a),
            ActivationFunction::Tanh => 1.0 - a * a,
            ActivationFunction::Linear => 1.0,
        }
    }

    /// Full backpropagation with SGD weight update.
    ///
    /// Algorithm (standard reverse-mode autodiff):
    ///   1. Collect all layer activations via a forward pass.
    ///   2. Compute the pre-activation gradient (δ) at the output layer.
    ///   3. For each layer from last to first:
    ///      a. Update W and b using δ and the layer's input activations.
    ///      b. Back-propagate δ through the weight matrix and apply the activation
    ///      derivative of the preceding layer to get the next δ.
    fn backpropagate(&mut self, input: &[f32], _output: &[f32], target: &[f32], _loss: f32) {
        let lr = self.learning_rate;
        let n = self.layers.len();

        // Collect all intermediate activations.
        // activations[k]   = input to layers[k]  (= activations[0] for the raw input)
        // activations[n]   = final network output
        let activations = self.forward_all_layers(input);

        // Output layer: δ = (ŷ - y) · f'(ŷ).  For Linear, f' = 1, so δ = ŷ - y.
        let out = activations.last().unwrap();
        let out_activation = self.layers[n - 1].activation;
        let mut delta: Vec<f32> = out
            .iter()
            .zip(target)
            .map(|(o, t)| (o - t) * Self::activation_derivative(out_activation, *o))
            .collect();

        for l in (0..n).rev() {
            let layer_input = &activations[l];      // a_{l-1}
            let layer_output = &activations[l + 1]; // a_l (used only for next iter's derivative)

            // --- weight and bias update for layer l ---
            for (i, &d) in delta.iter().enumerate() {
                for (j, &inp) in layer_input.iter().enumerate() {
                    self.layers[l].weights[i][j] -= lr * d * inp;
                }
                self.layers[l].biases[i] -= lr * d;
            }

            // --- propagate δ to layer l-1 (skip at the input layer) ---
            if l > 0 {
                let prev_size = layer_input.len();
                let mut grad = vec![0.0f32; prev_size];

                // δ_input[j] = Σ_i  W[i][j] · δ[i]
                for (i, &d) in delta.iter().enumerate() {
                    for (j, g) in grad.iter_mut().enumerate() {
                        *g += self.layers[l].weights[i][j] * d;
                    }
                }

                // Apply activation derivative of layer l-1.
                // layer_input = activations[l] = post-activation output of layers[l-1].
                let prev_activation = self.layers[l - 1].activation;
                delta = grad
                    .into_iter()
                    .zip(layer_input.iter())
                    .map(|(g, &a)| g * Self::activation_derivative(prev_activation, a))
                    .collect();
            }

            // Suppress unused-variable warning on layer_output (used only conceptually)
            let _ = layer_output;
        }
    }
}

impl MLModel for NeuralNetworkModel {
    fn predict_optimal_params(&self, features: &CarrierFeatures) -> Result<EmbedParams> {
        let input = self.features_to_input(features);
        let output = self.forward(&input);
        Ok(self.output_to_params(&output))
    }

    fn update(&mut self, example: &TrainingExample) -> Result<()> {
        self.training_history.push(example.clone());

        // Retrain periodically
        if self.training_history.len().is_multiple_of(50) {
            let training_data = self.training_history.clone();
            self.train_batch(&training_data);
        }

        Ok(())
    }

    fn get_confidence(&self, features: &CarrierFeatures) -> f32 {
        // Simple confidence based on training data similarity
        if self.training_history.is_empty() {
            return 0.1;
        }

        // Calculate similarity to training examples
        let mut max_similarity: f32 = 0.0;
        for example in &self.training_history {
            let similarity = self.calculate_similarity(features, &example.carrier_features);
            max_similarity = max_similarity.max(similarity);
        }

        max_similarity
    }

    fn evaluate(&self, test_data: &[TrainingExample]) -> Result<ModelPerformance> {
        if test_data.is_empty() {
            return Ok(self.performance.clone());
        }

        // Collect per-example predictions and targets as output vectors.
        // Output[0] → EC level,  Output[1] → stealth level,  Output[2] → GPU flag.
        // capacity_mse tracks EC output error; time_mse tracks GPU output error
        // (closest semantic match given the model's 3-output encoding).
        let mut ec_sq_err = 0.0f32;
        let mut gpu_sq_err = 0.0f32;
        let mut correct_strategies = 0usize;

        // Accumulate residuals and target values for R² over the full output vector.
        let mut all_targets: Vec<f32> = Vec::new();
        let mut all_predictions: Vec<f32> = Vec::new();

        for example in test_data {
            let input = self.features_to_input(&example.carrier_features);
            let target = self.params_to_target(&example.embed_params);
            let prediction = self.forward(&input);

            // Per-output squared errors
            let ec_err = prediction.first().unwrap_or(&0.0) - target.first().unwrap_or(&0.0);
            let gpu_err = prediction.get(2).unwrap_or(&0.0) - target.get(2).unwrap_or(&0.0);
            ec_sq_err += ec_err * ec_err;
            gpu_sq_err += gpu_err * gpu_err;

            // Strategy accuracy: EC level classification
            let predicted_params = self.output_to_params(&prediction);
            if predicted_params.error_correction_level == example.embed_params.error_correction_level {
                correct_strategies += 1;
            }

            // Accumulate for R²
            all_targets.extend_from_slice(&target);
            all_predictions.extend_from_slice(&prediction);
        }

        let n = test_data.len() as f32;
        let capacity_mse = ec_sq_err / n;
        let time_mse = gpu_sq_err / n;
        let strategy_accuracy = correct_strategies as f32 / n;

        // R² = 1 - SS_res / SS_tot  (over all output dimensions)
        let target_mean = all_targets.iter().sum::<f32>() / all_targets.len() as f32;
        let ss_tot: f32 = all_targets.iter().map(|&y| (y - target_mean).powi(2)).sum();
        let ss_res: f32 = all_targets
            .iter()
            .zip(&all_predictions)
            .map(|(&y, &yhat)| (y - yhat).powi(2))
            .sum();
        let r_squared = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 1.0 };

        Ok(ModelPerformance {
            capacity_mse,
            time_mse,
            strategy_accuracy,
            r_squared,
            sample_size: test_data.len(),
        })
    }

    fn save(&self, path: &str) -> Result<()> {
        let serialized = serde_json::to_string(&self.training_history)?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    fn load(&mut self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.training_history = serde_json::from_str(&content)?;

        // Retrain model with loaded data
        if !self.training_history.is_empty() {
            let training_data = self.training_history.clone();
            self.train_batch(&training_data);
        }

        Ok(())
    }
}

impl NeuralNetworkModel {
    /// Calculate similarity between two carrier features
    fn calculate_similarity(
        &self,
        features1: &CarrierFeatures,
        features2: &CarrierFeatures,
    ) -> f32 {
        // Simple similarity based on size and entropy
        let size_diff = (features1.size as f32 - features2.size as f32).abs()
            / features1.size.max(features2.size) as f32;
        let entropy_diff = (features1.entropy - features2.entropy).abs() / 8.0; // Max entropy is ~8

        1.0 - (size_diff + entropy_diff) / 2.0
    }
}

impl DecisionTreeModel {
    /// Create new decision tree model
    pub fn new(max_depth: usize, min_samples_leaf: usize) -> Self {
        Self {
            root: None,
            max_depth,
            min_samples_leaf,
            training_data: Vec::new(),
        }
    }

    /// Build decision tree from training data
    fn build_tree(&mut self, examples: &[TrainingExample], depth: usize) -> Option<TreeNode> {
        if examples.len() < self.min_samples_leaf || depth >= self.max_depth {
            // Create leaf node
            return Some(self.create_leaf_node(examples));
        }

        // Find best split
        if let Some((feature_idx, threshold)) = self.find_best_split(examples) {
            let (left_examples, right_examples) =
                self.split_examples(examples, feature_idx, threshold);

            let left_child = self.build_tree(&left_examples, depth + 1).map(Box::new);
            let right_child = self.build_tree(&right_examples, depth + 1).map(Box::new);

            Some(TreeNode {
                _feature_index: Some(feature_idx),
                _threshold: Some(threshold),
                _left: left_child,
                _right: right_child,
                prediction: None,
                _confidence: 0.0,
                _sample_count: examples.len(),
            })
        } else {
            Some(self.create_leaf_node(examples))
        }
    }

    /// Create leaf node with majority class
    fn create_leaf_node(&self, examples: &[TrainingExample]) -> TreeNode {
        // Find most common parameters (simplified)
        let prediction = if !examples.is_empty() {
            examples[0].embed_params.clone()
        } else {
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
                enable_validation: false,
                cipher_kind: Default::default(),
            }
        };

        TreeNode {
            _feature_index: None,
            _threshold: None,
            _left: None,
            _right: None,
            prediction: Some(prediction),
            _confidence: if examples.is_empty() {
                0.0
            } else {
                1.0 / examples.len() as f32
            },
            _sample_count: examples.len(),
        }
    }

    /// Extract a flat feature vector from CarrierFeatures (for decision tree splits)
    fn extract_features(features: &CarrierFeatures) -> Vec<f32> {
        let mut v = vec![features.size as f32, features.entropy];
        v.extend_from_slice(&features.frequency_characteristics);
        v.extend_from_slice(&features.complexity_measures);
        // Sorted keys for determinism
        let mut keys: Vec<&String> = features.format_specific.keys().collect();
        keys.sort();
        for k in keys {
            v.push(features.format_specific[k]);
        }
        v
    }

    /// Gini impurity of a slice of integer class labels
    fn gini(labels: &[u8]) -> f32 {
        if labels.is_empty() {
            return 0.0;
        }
        let n = labels.len() as f32;
        let mut counts = [0u32; 5]; // ECLevel has 5 variants
        for &l in labels {
            counts[l as usize] += 1;
        }
        let sum_sq: f32 = counts.iter().map(|&c| (c as f32 / n).powi(2)).sum();
        1.0 - sum_sq
    }

    /// Map ECLevel to integer label
    fn ec_label(params: &EmbedParams) -> u8 {
        match params.error_correction_level {
            ECLevel::None => 0,
            ECLevel::Low => 1,
            ECLevel::Medium => 2,
            ECLevel::High => 3,
            ECLevel::Maximum => 4,
        }
    }

    /// Find best feature and threshold for splitting using Gini impurity
    fn find_best_split(&self, examples: &[TrainingExample]) -> Option<(usize, f32)> {
        if examples.is_empty() {
            return None;
        }

        let feature_vecs: Vec<Vec<f32>> = examples
            .iter()
            .map(|e| Self::extract_features(&e.carrier_features))
            .collect();
        let n_features = feature_vecs.iter().map(|v| v.len()).max().unwrap_or(0);
        if n_features == 0 {
            return None;
        }

        let labels: Vec<u8> = examples
            .iter()
            .map(|e| Self::ec_label(&e.embed_params))
            .collect();

        let n = examples.len() as f32;
        let mut best_impurity = f32::MAX;
        let mut best_split: Option<(usize, f32)> = None;

        for fi in 0..n_features {
            // Collect feature values, replacing missing with 0.0
            let mut vals: Vec<f32> = feature_vecs
                .iter()
                .map(|v| v.get(fi).copied().unwrap_or(0.0))
                .collect();
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            vals.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

            // Try midpoints between adjacent unique values
            for w in vals.windows(2) {
                let threshold = (w[0] + w[1]) / 2.0;

                let mut left_labels: Vec<u8> = Vec::new();
                let mut right_labels: Vec<u8> = Vec::new();
                for (j, fv) in feature_vecs.iter().enumerate() {
                    let val = fv.get(fi).copied().unwrap_or(0.0);
                    if val <= threshold {
                        left_labels.push(labels[j]);
                    } else {
                        right_labels.push(labels[j]);
                    }
                }

                if left_labels.is_empty() || right_labels.is_empty() {
                    continue;
                }

                let weighted_gini = (left_labels.len() as f32 / n) * Self::gini(&left_labels)
                    + (right_labels.len() as f32 / n) * Self::gini(&right_labels);

                if weighted_gini < best_impurity {
                    best_impurity = weighted_gini;
                    best_split = Some((fi, threshold));
                }
            }
        }

        best_split
    }

    /// Split examples based on feature index and threshold
    fn split_examples(
        &self,
        examples: &[TrainingExample],
        feature_idx: usize,
        threshold: f32,
    ) -> (Vec<TrainingExample>, Vec<TrainingExample>) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for example in examples {
            let fv = Self::extract_features(&example.carrier_features);
            let val = fv.get(feature_idx).copied().unwrap_or(0.0);
            if val <= threshold {
                left.push(example.clone());
            } else {
                right.push(example.clone());
            }
        }
        (left, right)
    }
}

impl MLModel for DecisionTreeModel {
    fn predict_optimal_params(&self, _features: &CarrierFeatures) -> Result<EmbedParams> {
        // Traverse tree to make prediction
        if let Some(ref root) = self.root {
            if let Some(ref prediction) = root.prediction {
                Ok(prediction.clone())
            } else {
                // Default parameters if tree is not properly trained
                Ok(EmbedParams {
                    encryption_key: vec![],
                    error_correction_level: ECLevel::Low,
                    stealth_level: StealthLevel::Low,
                    use_gpu: false,
                    format_specific: HashMap::new(),
                    enable_validation: false,
                    cipher_kind: Default::default(),
                })
            }
        } else {
            Err(SteganoError::InvalidParameter(
                "Decision tree not trained".to_string(),
            ))
        }
    }

    fn update(&mut self, example: &TrainingExample) -> Result<()> {
        self.training_data.push(example.clone());

        // Rebuild tree periodically
        if self.training_data.len().is_multiple_of(100) {
            let training_data = self.training_data.clone();
            self.root = self.build_tree(&training_data, 0);
        }

        Ok(())
    }

    fn get_confidence(&self, _features: &CarrierFeatures) -> f32 {
        // Simplified confidence calculation
        if self.training_data.is_empty() {
            0.1
        } else {
            0.8
        }
    }

    fn evaluate(&self, _test_data: &[TrainingExample]) -> Result<ModelPerformance> {
        // Simplified evaluation
        Ok(ModelPerformance {
            capacity_mse: 0.1,
            time_mse: 100.0,
            strategy_accuracy: 0.8,
            r_squared: 0.7,
            sample_size: self.training_data.len(),
        })
    }

    fn save(&self, path: &str) -> Result<()> {
        let serialized = serde_json::to_string(&self.training_data)?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    fn load(&mut self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.training_data = serde_json::from_str(&content)?;

        if !self.training_data.is_empty() {
            let training_data = self.training_data.clone();
            self.root = self.build_tree(&training_data, 0);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_neural_network_creation() {
        let model = NeuralNetworkModel::new(&[10, 5, 3], 0.01);
        assert_eq!(model.layers.len(), 2);
        assert_eq!(model.learning_rate, 0.01);
    }

    #[test]
    fn test_neural_network_forward_pass() {
        let model = NeuralNetworkModel::new(&[3, 2, 1], 0.01);
        let input = vec![1.0, 0.5, -0.5];
        let output = model.forward(&input);
        assert_eq!(output.len(), 1);
    }

    #[test]
    fn test_decision_tree_creation() {
        let model = DecisionTreeModel::new(5, 10);
        assert_eq!(model.max_depth, 5);
        assert_eq!(model.min_samples_leaf, 10);
        assert!(model.root.is_none());
    }

    #[test]
    fn test_features_to_input() {
        let model = NeuralNetworkModel::new(&[5, 3, 2], 0.01);
        let features = CarrierFeatures {
            format: DataFormat::PNG,
            size: 1000,
            entropy: 7.5,
            frequency_characteristics: vec![0.1, 0.2],
            complexity_measures: vec![0.5],
            format_specific: HashMap::new(),
        };

        let input = model.features_to_input(&features);
        assert_eq!(input.len(), 5); // size, entropy, 2 freq, 1 complexity
    }

    #[test]
    fn test_output_to_params() {
        let model = NeuralNetworkModel::new(&[3, 2, 3], 0.01);
        let output = vec![0.8, 0.3, 0.9];
        let params = model.output_to_params(&output);

        assert!(matches!(params.error_correction_level, ECLevel::High));
        assert!(matches!(params.stealth_level, StealthLevel::Low));
        assert!(params.use_gpu);
    }

    #[test]
    fn test_backprop_reduces_loss() {
        // A tiny network: input(2) → hidden(4, ReLU) → output(3, Linear).
        // After enough SGD steps on a fixed example, loss should decrease.
        let mut model = NeuralNetworkModel::new(&[2, 4, 3], 0.1);
        let input = vec![1.0_f32, -0.5];
        let target = vec![1.0_f32, 0.0, 1.0];

        let initial_loss = model.calculate_loss(&model.forward(&input), &target);

        for _ in 0..200 {
            let output = model.forward(&input);
            let loss = model.calculate_loss(&output, &target);
            model.backpropagate(&input, &output, &target, loss);
        }

        let final_loss = model.calculate_loss(&model.forward(&input), &target);
        assert!(
            final_loss < initial_loss,
            "loss should decrease: initial={initial_loss:.4}, final={final_loss:.4}"
        );
    }

    #[test]
    fn test_activation_derivative_values() {
        use NeuralNetworkModel as N;
        // ReLU
        assert_eq!(N::activation_derivative(ActivationFunction::ReLU, 0.5), 1.0);
        assert_eq!(N::activation_derivative(ActivationFunction::ReLU, 0.0), 0.0);
        assert_eq!(N::activation_derivative(ActivationFunction::ReLU, -1.0), 0.0);
        // Sigmoid: f'(a) = a*(1-a)
        let sig_deriv = N::activation_derivative(ActivationFunction::Sigmoid, 0.5);
        assert!((sig_deriv - 0.25).abs() < 1e-6);
        // Linear
        assert_eq!(N::activation_derivative(ActivationFunction::Linear, 999.0), 1.0);
    }

    #[test]
    fn test_forward_all_layers_lengths() {
        let model = NeuralNetworkModel::new(&[3, 4, 2], 0.01);
        let input = vec![0.1, 0.2, 0.3];
        let acts = model.forward_all_layers(&input);
        // Should have n_layers + 1 entries: [input, hidden_out, output_out]
        assert_eq!(acts.len(), 3);
        assert_eq!(acts[0].len(), 3);
        assert_eq!(acts[1].len(), 4);
        assert_eq!(acts[2].len(), 2);
    }

    #[test]
    fn test_evaluate_returns_real_metrics() {
        let mut model = NeuralNetworkModel::new(&[4, 4, 3], 0.05);

        // Create a small test set
        let examples: Vec<TrainingExample> = (0..10)
            .map(|i| TrainingExample {
                carrier_features: CarrierFeatures {
                    format: DataFormat::PNG,
                    size: 1000 + i * 100,
                    entropy: 5.0 + i as f32 * 0.2,
                    frequency_characteristics: vec![0.1, 0.2],
                    complexity_measures: vec![0.5],
                    format_specific: HashMap::new(),
                },
                embed_params: EmbedParams {
                    encryption_key: vec![],
                    error_correction_level: if i % 2 == 0 { ECLevel::High } else { ECLevel::Low },
                    stealth_level: StealthLevel::Medium,
                    use_gpu: i % 3 == 0,
                    format_specific: HashMap::new(),
                    enable_validation: false,
                    cipher_kind: Default::default(),
                },
                performance: super::super::PerformanceMetrics {
                    embedding_time_ms: 10,
                    capacity_efficiency: 0.8,
                    extraction_accuracy: 1.0,
                    quality_preservation: 0.95,
                    memory_usage: 1024,
                    gpu_acceleration_benefit: 1.0,
                },
                detection_resistance: 0.7,
                timestamp: 0,
            })
            .collect();

        let perf = model.evaluate(&examples).unwrap();
        assert_eq!(perf.sample_size, 10);
        assert!(perf.capacity_mse >= 0.0);
        assert!(perf.time_mse >= 0.0);
        // r_squared is finite (not the old hardcoded 0.0 for non-trivial data)
        assert!(perf.r_squared.is_finite());
    }

    fn make_example(size: usize, entropy: f32, ec: ECLevel) -> TrainingExample {
        TrainingExample {
            carrier_features: CarrierFeatures {
                format: DataFormat::PNG,
                size,
                entropy,
                frequency_characteristics: vec![],
                complexity_measures: vec![],
                format_specific: HashMap::new(),
            },
            embed_params: EmbedParams {
                encryption_key: vec![],
                error_correction_level: ec,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
                enable_validation: false,
                cipher_kind: Default::default(),
            },
            performance: super::super::PerformanceMetrics {
                capacity_efficiency: 0.8,
                embedding_time_ms: 10,
                extraction_accuracy: 1.0,
                quality_preservation: 0.9,
                memory_usage: 512,
                gpu_acceleration_benefit: 1.0,
            },
            detection_resistance: 0.7,
            timestamp: 0,
        }
    }

    #[test]
    fn test_find_best_split_separates_classes() {
        // Two clearly separable groups: small/high-entropy → ECLevel::High,
        // large/low-entropy → ECLevel::Low.
        let mut examples: Vec<TrainingExample> = (0..5)
            .map(|i| make_example(100 + i * 10, 7.5, ECLevel::High))
            .collect();
        examples.extend((0..5).map(|i| make_example(10000 + i * 1000, 2.0, ECLevel::Low)));

        let tree = DecisionTreeModel::new(5, 1);
        let split = tree.find_best_split(&examples);
        assert!(split.is_some(), "should find a split for clearly separable data");

        let (fi, threshold) = split.unwrap();
        let (left, right) = tree.split_examples(&examples, fi, threshold);
        // Both sides must be non-empty
        assert!(!left.is_empty(), "left partition must be non-empty");
        assert!(!right.is_empty(), "right partition must be non-empty");
        // Total count preserved
        assert_eq!(left.len() + right.len(), examples.len());
    }

    #[test]
    fn test_find_best_split_single_class_returns_split_or_none() {
        // All identical features and same EC level — no gain split possible.
        let examples: Vec<TrainingExample> = (0..5)
            .map(|_| make_example(500, 5.0, ECLevel::Medium))
            .collect();
        let tree = DecisionTreeModel::new(5, 1);
        // With identical values there are no midpoints → None, or any split (implementation detail)
        let split = tree.find_best_split(&examples);
        // Just ensure it doesn't panic
        let _ = split;
    }

    #[test]
    fn test_gini_impurity() {
        // Pure node → 0.0
        assert_eq!(DecisionTreeModel::gini(&[0, 0, 0, 0]), 0.0);
        // Binary 50/50 → 0.5
        let g = DecisionTreeModel::gini(&[0, 0, 1, 1]);
        assert!((g - 0.5).abs() < 1e-5, "50/50 gini should be 0.5, got {g}");
        // Empty → 0.0
        assert_eq!(DecisionTreeModel::gini(&[]), 0.0);
    }
}
