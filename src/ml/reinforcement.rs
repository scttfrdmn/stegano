//! Reinforcement learning for adaptive strategy selection
//!
//! This module implements Q-learning and policy gradient methods to automatically
//! select optimal embedding strategies based on data characteristics and context.

use crate::core::*;
use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reinforcement learning agent for strategy selection
#[derive(Debug)]
pub struct RLAgent {
    /// Q-learning table for state-action values
    q_table: QTable,

    /// Policy network for continuous action spaces
    policy_network: PolicyNetwork,

    /// Agent configuration
    pub config: RLConfig,

    /// Learning statistics
    stats: LearningStats,

    /// Experience replay buffer
    replay_buffer: ExperienceBuffer,

    /// Current exploration strategy
    exploration_strategy: ExplorationStrategy,
}

/// Q-learning table for discrete state-action spaces
#[derive(Debug, Clone)]
pub struct QTable {
    /// State-action value estimates
    values: HashMap<StateAction, f32>,

    /// Visit counts for each state-action pair
    visit_counts: HashMap<StateAction, usize>,

    /// State space discretization
    _state_discretization: StateDiscretization,
}

/// Policy network for continuous action selection
#[derive(Debug)]
pub struct PolicyNetwork {
    /// Network weights (simplified implementation)
    weights: Vec<Vec<Vec<f32>>>,

    /// Network biases
    biases: Vec<Vec<f32>>,

    /// Layer dimensions
    _layer_dims: Vec<usize>,

    /// Activation functions per layer
    activations: Vec<ActivationFunction>,
}

/// RL agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLConfig {
    /// Learning rate for Q-learning
    pub learning_rate: f32,

    /// Discount factor for future rewards
    pub discount_factor: f32,

    /// Exploration parameters
    pub exploration_config: ExplorationConfig,

    /// Experience replay settings
    pub replay_config: ReplayConfig,

    /// Policy network configuration
    pub network_config: NetworkConfig,

    /// Maximum episodes for training
    pub max_episodes: usize,

    /// Episode length in steps
    pub episode_length: usize,
}

/// Exploration strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationConfig {
    /// Initial exploration rate
    pub initial_epsilon: f32,

    /// Final exploration rate
    pub final_epsilon: f32,

    /// Exploration decay rate
    pub decay_rate: f32,

    /// Exploration strategy type
    pub strategy_type: ExplorationStrategy,
}

/// Experience replay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    /// Buffer size for experience replay
    pub buffer_size: usize,

    /// Batch size for training
    pub batch_size: usize,

    /// Update frequency for target network
    pub target_update_freq: usize,

    /// Prioritized experience replay
    pub prioritized: bool,
}

/// Neural network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Hidden layer dimensions
    pub hidden_dims: Vec<usize>,

    /// Learning rate for network training
    pub learning_rate: f32,

    /// Activation function
    pub activation: ActivationFunction,

    /// Dropout probability
    pub dropout_prob: f32,
}

/// State representation for RL agent
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RLState {
    /// Data format type
    pub format: DataFormat,

    /// Data size category
    pub size_category: SizeCategory,

    /// Stealth requirement level
    pub stealth_level: StealthLevel,

    /// Performance priority
    pub performance_priority: PerformancePriority,

    /// Security requirement level
    pub security_level: SecurityLevel,

    /// Resource constraints
    pub resource_constraints: ResourceConstraints,
}

/// Action representation for strategy selection
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RLAction {
    /// Primary embedding strategy
    pub strategy: EmbeddingStrategy,

    /// Simplified optimization algorithm identifier
    pub optimizer: OptimizationAlgorithmType,

    /// Parameter configuration
    pub params: ParameterConfig,
}

/// Simplified optimization algorithm types for hashing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptimizationAlgorithmType {
    GeneticAlgorithm,
    ParticleSwarm,
    SimulatedAnnealing,
    DifferentialEvolution,
    BayesianOptimization,
    GridSearch,
    RandomSearch,
}

/// State-action pair for Q-table
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StateAction {
    pub state: RLState,
    pub action: RLAction,
}

/// Experience tuple for replay buffer
#[derive(Debug, Clone)]
pub struct Experience {
    /// Current state
    pub state: RLState,

    /// Action taken
    pub action: RLAction,

    /// Reward received
    pub reward: f32,

    /// Next state
    pub next_state: RLState,

    /// Episode termination flag
    pub done: bool,

    /// Priority for prioritized replay
    pub priority: f32,
}

/// Data size categories for state discretization
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeCategory {
    Small,  // < 1MB
    Medium, // 1MB - 100MB
    Large,  // 100MB - 1GB
    XLarge, // > 1GB
}

/// Performance priority levels
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformancePriority {
    Speed,
    Quality,
    Stealth,
    Balanced,
}

/// Security requirement levels
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    Basic,
    Standard,
    High,
    Maximum,
}

/// Resource constraint categories
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceConstraints {
    Memory,
    Compute,
    Time,
    None,
}

/// Embedding strategy types
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingStrategy {
    LSB,
    DCT,
    DWT,
    FFT,
    Statistical,
}

/// Parameter configuration for strategies
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterConfig {
    Conservative,
    Balanced,
    Aggressive,
    Custom(u32), // Hash of custom parameters
}

/// Exploration strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExplorationStrategy {
    EpsilonGreedy,
    UCB,
    Thompson,
    Softmax,
}

/// Activation functions for neural networks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivationFunction {
    ReLU,
    Sigmoid,
    Tanh,
    Softmax,
}

/// State space discretization configuration
#[derive(Debug, Clone)]
pub struct StateDiscretization {
    /// Number of discrete bins per dimension
    pub bins_per_dimension: HashMap<String, usize>,

    /// Value ranges for continuous features
    pub feature_ranges: HashMap<String, (f32, f32)>,
}

/// Learning statistics and metrics
#[derive(Debug, Clone)]
pub struct LearningStats {
    /// Episodes completed
    pub episodes_completed: usize,

    /// Total steps taken
    pub total_steps: usize,

    /// Average reward per episode
    pub average_reward: f32,

    /// Best reward achieved
    pub best_reward: f32,

    /// Convergence metrics
    pub convergence_rate: f32,

    /// Strategy success rates
    pub strategy_success_rates: HashMap<EmbeddingStrategy, f32>,

    /// Q-value statistics
    pub q_value_stats: QValueStats,
}

/// Q-value statistics for monitoring
#[derive(Debug, Clone)]
pub struct QValueStats {
    /// Mean Q-value
    pub mean_q_value: f32,

    /// Q-value variance
    pub q_value_variance: f32,

    /// Maximum Q-value
    pub max_q_value: f32,

    /// Minimum Q-value
    pub min_q_value: f32,
}

/// Experience replay buffer
#[derive(Debug)]
pub struct ExperienceBuffer {
    /// Buffer of experiences
    experiences: Vec<Experience>,

    /// Maximum buffer size
    max_size: usize,

    /// Current position in circular buffer
    position: usize,

    /// Buffer configuration
    _config: ReplayConfig,
}

/// RL training result
#[derive(Debug, Clone)]
pub struct RLTrainingResult {
    /// Final average reward
    pub final_average_reward: f32,

    /// Episodes to convergence
    pub episodes_to_convergence: usize,

    /// Best strategy found
    pub best_strategy: RLAction,

    /// Learning curve data
    pub learning_curve: Vec<f32>,

    /// Final Q-table (if using Q-learning)
    pub final_q_values: HashMap<StateAction, f32>,

    /// Training statistics
    pub training_stats: LearningStats,
}

/// Strategy recommendation result
#[derive(Debug, Clone)]
pub struct StrategyRecommendation {
    /// Recommended action
    pub recommended_action: RLAction,

    /// Confidence score
    pub confidence: f32,

    /// Alternative actions
    pub alternatives: Vec<(RLAction, f32)>,

    /// Reasoning explanation
    pub reasoning: String,
}

impl Default for RLConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            discount_factor: 0.95,
            exploration_config: ExplorationConfig {
                initial_epsilon: 1.0,
                final_epsilon: 0.01,
                decay_rate: 0.995,
                strategy_type: ExplorationStrategy::EpsilonGreedy,
            },
            replay_config: ReplayConfig {
                buffer_size: 10000,
                batch_size: 32,
                target_update_freq: 100,
                prioritized: false,
            },
            network_config: NetworkConfig {
                hidden_dims: vec![128, 64],
                learning_rate: 0.001,
                activation: ActivationFunction::ReLU,
                dropout_prob: 0.1,
            },
            max_episodes: 1000,
            episode_length: 100,
        }
    }
}

impl RLAgent {
    /// Create new RL agent
    pub fn new(config: RLConfig) -> Self {
        let state_discretization = StateDiscretization {
            bins_per_dimension: HashMap::new(),
            feature_ranges: HashMap::new(),
        };

        let q_table = QTable {
            values: HashMap::new(),
            visit_counts: HashMap::new(),
            _state_discretization: state_discretization,
        };

        let policy_network = PolicyNetwork::new(&config.network_config);

        Self {
            q_table,
            policy_network,
            config: config.clone(),
            stats: LearningStats::new(),
            replay_buffer: ExperienceBuffer::new(config.replay_config),
            exploration_strategy: config.exploration_config.strategy_type,
        }
    }

    /// Train the RL agent on historical data
    pub async fn train(&mut self, _training_data: &[TrainingEpisode]) -> Result<RLTrainingResult> {
        let mut learning_curve = Vec::new();
        let mut episode_rewards = Vec::new();

        for episode in 0..self.config.max_episodes {
            let mut episode_reward = 0.0;
            let mut state = self.sample_initial_state();

            for _step in 0..self.config.episode_length {
                // Select action using current policy
                let action = self.select_action(&state);

                // Execute action and get reward
                let (next_state, reward, done) = self.environment_step(&state, &action).await?;

                // Store experience
                let experience = Experience {
                    state: state.clone(),
                    action: action.clone(),
                    reward,
                    next_state: next_state.clone(),
                    done,
                    priority: reward.abs(),
                };

                self.replay_buffer.add_experience(experience);

                // Update Q-values
                self.update_q_values(&state, &action, reward, &next_state, done);

                episode_reward += reward;
                state = next_state;

                if done {
                    break;
                }

                // Update policy network
                if self.replay_buffer.size() >= self.config.replay_config.batch_size {
                    self.update_policy_network()?;
                }
            }

            episode_rewards.push(episode_reward);

            // Update exploration rate
            self.update_exploration_rate(episode);

            // Calculate running average
            let window_size = 100.min(episode_rewards.len());
            let recent_rewards: Vec<f32> = episode_rewards
                .iter()
                .rev()
                .take(window_size)
                .cloned()
                .collect();
            let average_reward = recent_rewards.iter().sum::<f32>() / recent_rewards.len() as f32;

            learning_curve.push(average_reward);

            // Log progress
            if episode % 100 == 0 {
                log::info!("RL Training Episode {episode}: Average Reward = {average_reward:.4}");
            }

            // Check convergence
            if self.check_convergence(&episode_rewards) {
                log::info!("RL Agent converged after {episode} episodes");
                break;
            }
        }

        // Find best strategy
        let best_strategy = self.get_best_strategy();

        Ok(RLTrainingResult {
            final_average_reward: learning_curve.last().cloned().unwrap_or(0.0),
            episodes_to_convergence: learning_curve.len(),
            best_strategy,
            learning_curve,
            final_q_values: self.q_table.values.clone(),
            training_stats: self.stats.clone(),
        })
    }

    /// Get strategy recommendation for given context
    pub fn recommend_strategy(&self, context: &EmbeddingContext) -> StrategyRecommendation {
        let state = self.context_to_state(context);
        let action = self.select_action_greedy(&state);
        let confidence = self.calculate_confidence(&state, &action);
        let alternatives = self.get_alternative_actions(&state, 3);

        StrategyRecommendation {
            recommended_action: action,
            confidence,
            alternatives,
            reasoning: self.generate_reasoning(&state),
        }
    }

    /// Select action using current policy
    fn select_action(&self, state: &RLState) -> RLAction {
        match self.exploration_strategy {
            ExplorationStrategy::EpsilonGreedy => self.epsilon_greedy_action(state),
            ExplorationStrategy::UCB => self.ucb_action(state),
            ExplorationStrategy::Thompson => self.thompson_sampling_action(state),
            ExplorationStrategy::Softmax => self.softmax_action(state),
        }
    }

    /// Select greedy action (no exploration)
    fn select_action_greedy(&self, state: &RLState) -> RLAction {
        let mut best_action = self.get_random_action();
        let mut best_q_value = f32::NEG_INFINITY;

        for action in self.get_possible_actions(state) {
            let state_action = StateAction {
                state: state.clone(),
                action: action.clone(),
            };

            let q_value = self
                .q_table
                .values
                .get(&state_action)
                .cloned()
                .unwrap_or(0.0);

            if q_value > best_q_value {
                best_q_value = q_value;
                best_action = action;
            }
        }

        best_action
    }

    /// Epsilon-greedy action selection
    fn epsilon_greedy_action(&self, state: &RLState) -> RLAction {
        let epsilon = self.get_current_epsilon();

        if random::<f32>() < epsilon {
            self.get_random_action()
        } else {
            self.select_action_greedy(state)
        }
    }

    /// Upper Confidence Bound action selection
    fn ucb_action(&self, state: &RLState) -> RLAction {
        let mut best_action = self.get_random_action();
        let mut best_ucb_value = f32::NEG_INFINITY;

        let total_visits: usize = self
            .get_possible_actions(state)
            .iter()
            .map(|action| {
                let state_action = StateAction {
                    state: state.clone(),
                    action: action.clone(),
                };
                self.q_table
                    .visit_counts
                    .get(&state_action)
                    .cloned()
                    .unwrap_or(0)
            })
            .sum();

        for action in self.get_possible_actions(state) {
            let state_action = StateAction {
                state: state.clone(),
                action: action.clone(),
            };

            let q_value = self
                .q_table
                .values
                .get(&state_action)
                .cloned()
                .unwrap_or(0.0);
            let visits = self
                .q_table
                .visit_counts
                .get(&state_action)
                .cloned()
                .unwrap_or(0) as f32;

            let ucb_value = if visits > 0.0 {
                q_value + (2.0 * (total_visits as f32).ln() / visits).sqrt()
            } else {
                f32::INFINITY
            };

            if ucb_value > best_ucb_value {
                best_ucb_value = ucb_value;
                best_action = action;
            }
        }

        best_action
    }

    /// Thompson sampling action selection
    fn thompson_sampling_action(&self, state: &RLState) -> RLAction {
        // Simplified Thompson sampling implementation
        // In practice would use proper Bayesian inference
        let mut best_action = self.get_random_action();
        let mut best_sample = f32::NEG_INFINITY;

        for action in self.get_possible_actions(state) {
            let state_action = StateAction {
                state: state.clone(),
                action: action.clone(),
            };

            let q_value = self
                .q_table
                .values
                .get(&state_action)
                .cloned()
                .unwrap_or(0.0);
            let visits = self
                .q_table
                .visit_counts
                .get(&state_action)
                .cloned()
                .unwrap_or(0) as f32;

            // Sample from normal distribution centered at Q-value
            let uncertainty = 1.0 / (visits + 1.0);
            let sample = q_value + uncertainty * (random::<f32>() - 0.5) * 2.0;

            if sample > best_sample {
                best_sample = sample;
                best_action = action;
            }
        }

        best_action
    }

    /// Softmax action selection
    fn softmax_action(&self, state: &RLState) -> RLAction {
        let actions = self.get_possible_actions(state);
        let temperature = 1.0;

        // Calculate softmax probabilities
        let mut q_values = Vec::new();
        for action in &actions {
            let state_action = StateAction {
                state: state.clone(),
                action: action.clone(),
            };
            let q_value = self
                .q_table
                .values
                .get(&state_action)
                .cloned()
                .unwrap_or(0.0);
            q_values.push(q_value / temperature);
        }

        // Apply softmax
        let max_q = q_values.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let exp_values: Vec<f32> = q_values.iter().map(|&q| (q - max_q).exp()).collect();
        let sum_exp: f32 = exp_values.iter().sum();
        let probabilities: Vec<f32> = exp_values
            .iter()
            .map(|&exp_val| exp_val / sum_exp)
            .collect();

        // Sample action based on probabilities
        let mut cumulative = 0.0;
        let rand_val = random::<f32>();

        for (i, &prob) in probabilities.iter().enumerate() {
            cumulative += prob;
            if rand_val <= cumulative {
                return actions[i].clone();
            }
        }

        actions.last().unwrap().clone()
    }

    /// Update Q-values using Q-learning
    fn update_q_values(
        &mut self,
        state: &RLState,
        action: &RLAction,
        reward: f32,
        next_state: &RLState,
        done: bool,
    ) {
        let state_action = StateAction {
            state: state.clone(),
            action: action.clone(),
        };

        let current_q = self
            .q_table
            .values
            .get(&state_action)
            .cloned()
            .unwrap_or(0.0);

        let next_q_max = if done {
            0.0
        } else {
            self.get_possible_actions(next_state)
                .iter()
                .map(|next_action| {
                    let next_state_action = StateAction {
                        state: next_state.clone(),
                        action: next_action.clone(),
                    };
                    self.q_table
                        .values
                        .get(&next_state_action)
                        .cloned()
                        .unwrap_or(0.0)
                })
                .fold(f32::NEG_INFINITY, f32::max)
        };

        let td_target = reward + self.config.discount_factor * next_q_max;
        let new_q = current_q + self.config.learning_rate * (td_target - current_q);

        self.q_table.values.insert(state_action.clone(), new_q);

        // Update visit count
        let visits = self
            .q_table
            .visit_counts
            .get(&state_action)
            .cloned()
            .unwrap_or(0);
        self.q_table.visit_counts.insert(state_action, visits + 1);
    }

    /// Update policy network using experience replay
    fn update_policy_network(&mut self) -> Result<()> {
        let batch = self
            .replay_buffer
            .sample_batch(self.config.replay_config.batch_size);

        // Simple gradient update (placeholder implementation)
        // In practice would use proper backpropagation
        for experience in batch {
            let state_features = self.state_to_features(&experience.state);
            let target_value = experience.reward;

            // Update network weights based on prediction error
            let prediction = self.policy_network.forward(&state_features);
            let error = target_value - prediction;

            self.policy_network.backward(
                error,
                &state_features,
                self.config.network_config.learning_rate,
            );
        }

        Ok(())
    }

    /// Convert state to feature vector for neural network
    fn state_to_features(&self, state: &RLState) -> Vec<f32> {
        let mut features = Vec::new();

        // One-hot encode categorical features
        features.extend(self.format_to_onehot(&state.format));
        features.extend(self.size_category_to_onehot(&state.size_category));
        features.extend(self.stealth_level_to_onehot(&state.stealth_level));
        features.extend(self.performance_priority_to_onehot(&state.performance_priority));
        features.extend(self.security_level_to_onehot(&state.security_level));
        features.extend(self.resource_constraints_to_onehot(&state.resource_constraints));

        features
    }

    /// Convert data format to one-hot encoding
    fn format_to_onehot(&self, format: &DataFormat) -> Vec<f32> {
        match format {
            DataFormat::PNG => vec![1.0, 0.0, 0.0, 0.0, 0.0],
            DataFormat::TIFF => vec![0.0, 1.0, 0.0, 0.0, 0.0],
            DataFormat::WAV => vec![0.0, 0.0, 1.0, 0.0, 0.0],
            DataFormat::FASTA => vec![0.0, 0.0, 0.0, 1.0, 0.0],
            _ => vec![0.0, 0.0, 0.0, 0.0, 1.0], // Other
        }
    }

    /// Convert size category to one-hot encoding
    fn size_category_to_onehot(&self, size: &SizeCategory) -> Vec<f32> {
        match size {
            SizeCategory::Small => vec![1.0, 0.0, 0.0, 0.0],
            SizeCategory::Medium => vec![0.0, 1.0, 0.0, 0.0],
            SizeCategory::Large => vec![0.0, 0.0, 1.0, 0.0],
            SizeCategory::XLarge => vec![0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Convert stealth level to one-hot encoding
    fn stealth_level_to_onehot(&self, stealth: &StealthLevel) -> Vec<f32> {
        match stealth {
            StealthLevel::Low => vec![1.0, 0.0, 0.0, 0.0],
            StealthLevel::Medium => vec![0.0, 1.0, 0.0, 0.0],
            StealthLevel::High => vec![0.0, 0.0, 1.0, 0.0],
            StealthLevel::Maximum => vec![0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Convert performance priority to one-hot encoding
    fn performance_priority_to_onehot(&self, priority: &PerformancePriority) -> Vec<f32> {
        match priority {
            PerformancePriority::Speed => vec![1.0, 0.0, 0.0, 0.0],
            PerformancePriority::Quality => vec![0.0, 1.0, 0.0, 0.0],
            PerformancePriority::Stealth => vec![0.0, 0.0, 1.0, 0.0],
            PerformancePriority::Balanced => vec![0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Convert security level to one-hot encoding
    fn security_level_to_onehot(&self, security: &SecurityLevel) -> Vec<f32> {
        match security {
            SecurityLevel::Basic => vec![1.0, 0.0, 0.0, 0.0],
            SecurityLevel::Standard => vec![0.0, 1.0, 0.0, 0.0],
            SecurityLevel::High => vec![0.0, 0.0, 1.0, 0.0],
            SecurityLevel::Maximum => vec![0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Convert resource constraints to one-hot encoding
    fn resource_constraints_to_onehot(&self, constraints: &ResourceConstraints) -> Vec<f32> {
        match constraints {
            ResourceConstraints::Memory => vec![1.0, 0.0, 0.0, 0.0],
            ResourceConstraints::Compute => vec![0.0, 1.0, 0.0, 0.0],
            ResourceConstraints::Time => vec![0.0, 0.0, 1.0, 0.0],
            ResourceConstraints::None => vec![0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Simulate environment step
    async fn environment_step(
        &self,
        state: &RLState,
        action: &RLAction,
    ) -> Result<(RLState, f32, bool)> {
        // Simulate applying the action and getting a reward
        let reward = self.simulate_action_reward(state, action).await?;
        let next_state = self.simulate_next_state(state, action);
        let done = random::<f32>() < 0.1; // 10% chance of episode termination

        Ok((next_state, reward, done))
    }

    /// Simulate reward for action in given state
    async fn simulate_action_reward(&self, state: &RLState, action: &RLAction) -> Result<f32> {
        // Simplified reward function
        let mut reward = 0.0;

        // Reward for strategy-format alignment
        match (&action.strategy, &state.format) {
            (EmbeddingStrategy::LSB, DataFormat::PNG) => reward += 0.8,
            (EmbeddingStrategy::DCT, DataFormat::PNG) => reward += 0.9,
            (EmbeddingStrategy::FFT, DataFormat::WAV) => reward += 0.8,
            (EmbeddingStrategy::Statistical, DataFormat::FASTA) => reward += 0.7,
            _ => reward += 0.3,
        }

        // Penalty for mismatched stealth requirements
        match (&action.params, &state.stealth_level) {
            (ParameterConfig::Conservative, StealthLevel::Maximum) => reward += 0.5,
            (ParameterConfig::Aggressive, StealthLevel::Low) => reward += 0.5,
            (ParameterConfig::Balanced, StealthLevel::Medium) => reward += 0.6,
            _ => reward -= 0.2,
        }

        // Add some noise
        reward += (random::<f32>() - 0.5) * 0.1;

        Ok(reward.clamp(-1.0, 1.0))
    }

    /// Simulate next state transition
    fn simulate_next_state(&self, _state: &RLState, _action: &RLAction) -> RLState {
        // For simplicity, generate a random next state
        self.sample_random_state()
    }

    /// Sample initial state for episode
    fn sample_initial_state(&self) -> RLState {
        self.sample_random_state()
    }

    /// Sample random state
    fn sample_random_state(&self) -> RLState {
        let formats = [
            DataFormat::PNG,
            DataFormat::TIFF,
            DataFormat::WAV,
            DataFormat::FASTA,
        ];
        let sizes = [
            SizeCategory::Small,
            SizeCategory::Medium,
            SizeCategory::Large,
            SizeCategory::XLarge,
        ];
        let stealth_levels = [
            StealthLevel::Low,
            StealthLevel::Medium,
            StealthLevel::High,
            StealthLevel::Maximum,
        ];
        let priorities = [
            PerformancePriority::Speed,
            PerformancePriority::Quality,
            PerformancePriority::Stealth,
            PerformancePriority::Balanced,
        ];
        let security_levels = [
            SecurityLevel::Basic,
            SecurityLevel::Standard,
            SecurityLevel::High,
            SecurityLevel::Maximum,
        ];
        let constraints = [
            ResourceConstraints::Memory,
            ResourceConstraints::Compute,
            ResourceConstraints::Time,
            ResourceConstraints::None,
        ];

        RLState {
            format: formats[random::<usize>() % formats.len()].clone(),
            size_category: sizes[random::<usize>() % sizes.len()].clone(),
            stealth_level: stealth_levels[random::<usize>() % stealth_levels.len()],
            performance_priority: priorities[random::<usize>() % priorities.len()].clone(),
            security_level: security_levels[random::<usize>() % security_levels.len()].clone(),
            resource_constraints: constraints[random::<usize>() % constraints.len()].clone(),
        }
    }

    /// Get possible actions for given state
    fn get_possible_actions(&self, _state: &RLState) -> Vec<RLAction> {
        let strategies = [
            EmbeddingStrategy::LSB,
            EmbeddingStrategy::DCT,
            EmbeddingStrategy::DWT,
            EmbeddingStrategy::FFT,
            EmbeddingStrategy::Statistical,
        ];
        let optimizers = [
            OptimizationAlgorithmType::GeneticAlgorithm,
            OptimizationAlgorithmType::ParticleSwarm,
        ];
        let params = [
            ParameterConfig::Conservative,
            ParameterConfig::Balanced,
            ParameterConfig::Aggressive,
        ];

        let mut actions = Vec::new();
        for strategy in &strategies {
            for optimizer in &optimizers {
                for param in &params {
                    actions.push(RLAction {
                        strategy: strategy.clone(),
                        optimizer: optimizer.clone(),
                        params: param.clone(),
                    });
                }
            }
        }

        actions
    }

    /// Get random action
    fn get_random_action(&self) -> RLAction {
        let actions = self.get_possible_actions(&self.sample_random_state());
        actions[random::<usize>() % actions.len()].clone()
    }

    /// Get current exploration rate
    fn get_current_epsilon(&self) -> f32 {
        let progress = self.stats.episodes_completed as f32 / self.config.max_episodes as f32;
        let epsilon = self.config.exploration_config.initial_epsilon
            * (self.config.exploration_config.decay_rate.powf(progress));
        epsilon.max(self.config.exploration_config.final_epsilon)
    }

    /// Update exploration rate
    fn update_exploration_rate(&mut self, episode: usize) {
        self.stats.episodes_completed = episode;
    }

    /// Check convergence
    fn check_convergence(&self, episode_rewards: &[f32]) -> bool {
        if episode_rewards.len() < 200 {
            return false;
        }

        let recent_window: Vec<f32> = episode_rewards.iter().rev().take(50).cloned().collect();
        let older_window: Vec<f32> = episode_rewards
            .iter()
            .rev()
            .skip(50)
            .take(50)
            .cloned()
            .collect();

        let recent_avg = recent_window.iter().sum::<f32>() / recent_window.len() as f32;
        let older_avg = older_window.iter().sum::<f32>() / older_window.len() as f32;

        (recent_avg - older_avg).abs() < 0.01
    }

    /// Get best strategy
    fn get_best_strategy(&self) -> RLAction {
        self.q_table
            .values
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(state_action, _)| state_action.action.clone())
            .unwrap_or_else(|| self.get_random_action())
    }

    /// Convert embedding context to RL state
    fn context_to_state(&self, context: &EmbeddingContext) -> RLState {
        RLState {
            format: context.data_format.clone(),
            size_category: self.size_to_category(context.data_size),
            stealth_level: context.stealth_level,
            performance_priority: context.performance_priority.clone(),
            security_level: context.security_level.clone(),
            resource_constraints: context.resource_constraints.clone(),
        }
    }

    /// Convert data size to category
    fn size_to_category(&self, size: usize) -> SizeCategory {
        match size {
            s if s < 1_000_000 => SizeCategory::Small,
            s if s < 100_000_000 => SizeCategory::Medium,
            s if s < 1_000_000_000 => SizeCategory::Large,
            _ => SizeCategory::XLarge,
        }
    }

    /// Calculate confidence for action selection
    fn calculate_confidence(&self, state: &RLState, action: &RLAction) -> f32 {
        let state_action = StateAction {
            state: state.clone(),
            action: action.clone(),
        };

        let visits = self
            .q_table
            .visit_counts
            .get(&state_action)
            .cloned()
            .unwrap_or(0) as f32;
        let confidence = 1.0 - (-visits / 10.0).exp();
        confidence.clamp(0.0, 1.0)
    }

    /// Get alternative actions with their values
    fn get_alternative_actions(&self, state: &RLState, count: usize) -> Vec<(RLAction, f32)> {
        let mut actions_with_values: Vec<(RLAction, f32)> = self
            .get_possible_actions(state)
            .into_iter()
            .map(|action| {
                let state_action = StateAction {
                    state: state.clone(),
                    action: action.clone(),
                };
                let q_value = self
                    .q_table
                    .values
                    .get(&state_action)
                    .cloned()
                    .unwrap_or(0.0);
                (action, q_value)
            })
            .collect();

        actions_with_values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        actions_with_values.into_iter().take(count).collect()
    }

    /// Generate reasoning explanation
    fn generate_reasoning(&self, state: &RLState) -> String {
        format!(
            "Based on data format {:?}, size {:?}, stealth level {:?}, and performance priority {:?}",
            state.format, state.size_category, state.stealth_level, state.performance_priority
        )
    }
}

impl PolicyNetwork {
    /// Create new policy network
    pub fn new(config: &NetworkConfig) -> Self {
        let mut layer_dims = vec![24]; // Input features
        layer_dims.extend(&config.hidden_dims);
        layer_dims.push(1); // Output value

        let mut weights = Vec::new();
        let mut biases = Vec::new();
        let mut activations = Vec::new();

        for i in 0..layer_dims.len() - 1 {
            let input_dim = layer_dims[i];
            let output_dim = layer_dims[i + 1];

            // Initialize weights randomly
            let mut layer_weights = Vec::new();
            for _ in 0..output_dim {
                let mut neuron_weights = Vec::new();
                for _ in 0..input_dim {
                    neuron_weights.push((random::<f32>() - 0.5) * 0.1);
                }
                layer_weights.push(neuron_weights);
            }
            weights.push(layer_weights);

            // Initialize biases
            let layer_biases: Vec<f32> = (0..output_dim).map(|_| 0.0).collect();
            biases.push(layer_biases);

            // Set activation functions
            if i == layer_dims.len() - 2 {
                activations.push(ActivationFunction::Sigmoid); // Output layer
            } else {
                activations.push(config.activation.clone());
            }
        }

        Self {
            weights,
            biases,
            _layer_dims: layer_dims,
            activations,
        }
    }

    /// Forward pass through network
    pub fn forward(&self, input: &[f32]) -> f32 {
        let mut activations = input.to_vec();

        for layer in 0..self.weights.len() {
            let mut new_activations = Vec::new();

            for neuron in 0..self.weights[layer].len() {
                let mut sum = self.biases[layer][neuron];

                for (i, &weight) in self.weights[layer][neuron].iter().enumerate() {
                    sum += weight * activations[i];
                }

                let activated = self.apply_activation(sum, &self.activations[layer]);
                new_activations.push(activated);
            }

            activations = new_activations;
        }

        activations[0]
    }

    /// Backward pass (simplified implementation)
    pub fn backward(&mut self, error: f32, input: &[f32], learning_rate: f32) {
        // Simplified gradient update for last layer only
        let output_layer = self.weights.len() - 1;

        for neuron in 0..self.weights[output_layer].len() {
            for i in 0..self.weights[output_layer][neuron].len() {
                self.weights[output_layer][neuron][i] += learning_rate * error * input[i];
            }
            self.biases[output_layer][neuron] += learning_rate * error;
        }
    }

    /// Apply activation function
    fn apply_activation(&self, x: f32, activation: &ActivationFunction) -> f32 {
        match activation {
            ActivationFunction::ReLU => x.max(0.0),
            ActivationFunction::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            ActivationFunction::Tanh => x.tanh(),
            ActivationFunction::Softmax => x.exp(), // Simplified for single output
        }
    }
}

impl ExperienceBuffer {
    /// Create new experience buffer
    pub fn new(config: ReplayConfig) -> Self {
        Self {
            experiences: Vec::with_capacity(config.buffer_size),
            max_size: config.buffer_size,
            position: 0,
            _config: config,
        }
    }

    /// Add experience to buffer
    pub fn add_experience(&mut self, experience: Experience) {
        if self.experiences.len() < self.max_size {
            self.experiences.push(experience);
        } else {
            self.experiences[self.position] = experience;
            self.position = (self.position + 1) % self.max_size;
        }
    }

    /// Sample batch of experiences
    pub fn sample_batch(&self, batch_size: usize) -> Vec<Experience> {
        let mut batch = Vec::new();

        for _ in 0..batch_size.min(self.experiences.len()) {
            let index = random::<usize>() % self.experiences.len();
            batch.push(self.experiences[index].clone());
        }

        batch
    }

    /// Get buffer size
    pub fn size(&self) -> usize {
        self.experiences.len()
    }
}

impl Default for LearningStats {
    fn default() -> Self {
        Self::new()
    }
}

impl LearningStats {
    /// Create new learning stats
    pub fn new() -> Self {
        Self {
            episodes_completed: 0,
            total_steps: 0,
            average_reward: 0.0,
            best_reward: f32::NEG_INFINITY,
            convergence_rate: 0.0,
            strategy_success_rates: HashMap::new(),
            q_value_stats: QValueStats {
                mean_q_value: 0.0,
                q_value_variance: 0.0,
                max_q_value: f32::NEG_INFINITY,
                min_q_value: f32::INFINITY,
            },
        }
    }
}

/// Embedding context for strategy selection
#[derive(Debug, Clone)]
pub struct EmbeddingContext {
    pub data_format: DataFormat,
    pub data_size: usize,
    pub stealth_level: StealthLevel,
    pub performance_priority: PerformancePriority,
    pub security_level: SecurityLevel,
    pub resource_constraints: ResourceConstraints,
}

/// Training episode data
#[derive(Debug, Clone)]
pub struct TrainingEpisode {
    pub initial_state: RLState,
    pub actions: Vec<RLAction>,
    pub rewards: Vec<f32>,
    pub final_reward: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rl_agent_creation() {
        let config = RLConfig::default();
        let agent = RLAgent::new(config);

        assert_eq!(agent.q_table.values.len(), 0);
        assert_eq!(agent.stats.episodes_completed, 0);
    }

    #[test]
    fn test_state_to_features() {
        let config = RLConfig::default();
        let agent = RLAgent::new(config);

        let state = RLState {
            format: DataFormat::PNG,
            size_category: SizeCategory::Medium,
            stealth_level: StealthLevel::High,
            performance_priority: PerformancePriority::Speed,
            security_level: SecurityLevel::Standard,
            resource_constraints: ResourceConstraints::Memory,
        };

        let features = agent.state_to_features(&state);
        assert_eq!(features.len(), 25); // Total feature dimensions (5+4+4+4+4+4)
    }

    #[test]
    fn test_action_selection() {
        let config = RLConfig::default();
        let agent = RLAgent::new(config);
        let state = agent.sample_random_state();

        let action = agent.select_action(&state);
        assert!(matches!(
            action.strategy,
            EmbeddingStrategy::LSB
                | EmbeddingStrategy::DCT
                | EmbeddingStrategy::DWT
                | EmbeddingStrategy::FFT
                | EmbeddingStrategy::Statistical
        ));
    }

    #[test]
    fn test_experience_buffer() {
        let config = ReplayConfig {
            buffer_size: 10,
            batch_size: 5,
            target_update_freq: 100,
            prioritized: false,
        };

        let mut buffer = ExperienceBuffer::new(config);

        for i in 0..15 {
            let experience = Experience {
                state: RLState {
                    format: DataFormat::PNG,
                    size_category: SizeCategory::Small,
                    stealth_level: StealthLevel::Low,
                    performance_priority: PerformancePriority::Speed,
                    security_level: SecurityLevel::Basic,
                    resource_constraints: ResourceConstraints::None,
                },
                action: RLAction {
                    strategy: EmbeddingStrategy::LSB,
                    optimizer: OptimizationAlgorithmType::GeneticAlgorithm,
                    params: ParameterConfig::Balanced,
                },
                reward: i as f32,
                next_state: RLState {
                    format: DataFormat::PNG,
                    size_category: SizeCategory::Small,
                    stealth_level: StealthLevel::Low,
                    performance_priority: PerformancePriority::Speed,
                    security_level: SecurityLevel::Basic,
                    resource_constraints: ResourceConstraints::None,
                },
                done: false,
                priority: 1.0,
            };

            buffer.add_experience(experience);
        }

        assert_eq!(buffer.size(), 10); // Should not exceed max size

        let batch = buffer.sample_batch(5);
        assert_eq!(batch.len(), 5);
    }

    #[test]
    fn test_policy_network() {
        let config = NetworkConfig {
            hidden_dims: vec![64, 32],
            learning_rate: 0.001,
            activation: ActivationFunction::ReLU,
            dropout_prob: 0.1,
        };

        let network = PolicyNetwork::new(&config);
        assert_eq!(network._layer_dims, vec![24, 64, 32, 1]);

        let input = vec![0.5; 24];
        let output = network.forward(&input);
        assert!((0.0..=1.0).contains(&output)); // Sigmoid output
    }

    #[tokio::test]
    async fn test_strategy_recommendation() {
        let config = RLConfig::default();
        let agent = RLAgent::new(config);

        let context = EmbeddingContext {
            data_format: DataFormat::PNG,
            data_size: 1000000,
            stealth_level: StealthLevel::High,
            performance_priority: PerformancePriority::Quality,
            security_level: SecurityLevel::Standard,
            resource_constraints: ResourceConstraints::Memory,
        };

        let recommendation = agent.recommend_strategy(&context);
        assert!(recommendation.confidence >= 0.0 && recommendation.confidence <= 1.0);
        assert!(!recommendation.alternatives.is_empty());
        assert!(!recommendation.reasoning.is_empty());
    }
}
