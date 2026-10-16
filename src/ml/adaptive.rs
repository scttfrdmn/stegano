//! Adaptive steganography strategies using machine learning
//!
//! This module implements adaptive algorithms that learn optimal embedding
//! strategies based on carrier characteristics and historical performance.

use super::*;
use crate::core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Adaptive strategy engine that learns from embedding history
#[derive(Debug)]
pub struct AdaptiveStrategy {
    /// Strategy performance history by format
    strategy_history: HashMap<DataFormat, Vec<StrategyPerformance>>,

    /// Current optimal strategies by format
    optimal_strategies: HashMap<DataFormat, EmbedParams>,

    /// Learning rate for strategy adaptation
    learning_rate: f32,

    /// Exploration vs exploitation balance (0.0 = exploit, 1.0 = explore)
    exploration_rate: f32,

    /// Minimum performance threshold for strategy acceptance
    min_performance_threshold: f32,
}

/// Performance record for a specific strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPerformance {
    /// The embedding parameters used
    pub params: EmbedParams,

    /// Measured performance metrics
    pub metrics: PerformanceMetrics,

    /// Carrier characteristics at time of embedding
    pub carrier_features: CarrierFeatures,

    /// Success rate for this strategy (0.0-1.0)
    pub success_rate: f32,

    /// Number of times this strategy was used
    pub usage_count: u32,

    /// Timestamp of last usage
    pub last_used: u64,

    /// Detection resistance score if available
    pub detection_resistance: Option<f32>,
}

/// Multi-armed bandit for strategy selection
#[derive(Debug)]
pub struct StrategyBandit {
    /// Strategy arms (embedding parameters)
    arms: Vec<EmbedParams>,

    /// Reward history for each arm
    rewards: Vec<Vec<f32>>,

    /// Number of times each arm was pulled
    pull_counts: Vec<u32>,

    /// Confidence bounds for each arm
    _confidence_bounds: Vec<f32>,

    /// Total number of pulls
    total_pulls: u32,
}

/// Reinforcement learning agent for strategy optimization
#[derive(Debug)]
pub struct StrategyAgent {
    /// Q-table for state-action values
    q_table: HashMap<String, HashMap<String, f32>>,

    /// Learning rate
    alpha: f32,

    /// Discount factor
    gamma: f32,

    /// Exploration rate (epsilon-greedy)
    epsilon: f32,

    /// Current state representation
    current_state: Option<String>,

    /// Previous action taken
    _previous_action: Option<String>,
}

impl Default for AdaptiveStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveStrategy {
    /// Create new adaptive strategy engine
    pub fn new() -> Self {
        Self {
            strategy_history: HashMap::new(),
            optimal_strategies: HashMap::new(),
            learning_rate: 0.1,
            exploration_rate: 0.1,
            min_performance_threshold: 0.5,
        }
    }

    /// Configure learning parameters
    pub fn configure(&mut self, learning_rate: f32, exploration_rate: f32, min_threshold: f32) {
        self.learning_rate = learning_rate.clamp(0.0, 1.0);
        self.exploration_rate = exploration_rate.clamp(0.0, 1.0);
        self.min_performance_threshold = min_threshold.clamp(0.0, 1.0);
    }

    /// Select optimal strategy for given carrier characteristics
    pub fn select_strategy(
        &mut self,
        features: &CarrierFeatures,
        available_strategies: &[EmbedParams],
    ) -> EmbedParams {
        // Check if we have an optimal strategy for this format
        if let Some(optimal) = self.optimal_strategies.get(&features.format) {
            // Use exploration vs exploitation
            if rand::random::<f32>() < self.exploration_rate {
                // Explore: choose random strategy
                available_strategies[rand::random::<usize>() % available_strategies.len()].clone()
            } else {
                // Exploit: use optimal strategy
                optimal.clone()
            }
        } else {
            // No history, start with random strategy
            available_strategies[rand::random::<usize>() % available_strategies.len()].clone()
        }
    }

    /// Update strategy performance with feedback
    pub fn update_performance(
        &mut self,
        params: &EmbedParams,
        features: &CarrierFeatures,
        metrics: &PerformanceMetrics,
        success: bool,
    ) {
        let performance = StrategyPerformance {
            params: params.clone(),
            metrics: metrics.clone(),
            carrier_features: features.clone(),
            success_rate: if success { 1.0 } else { 0.0 },
            usage_count: 1,
            last_used: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            detection_resistance: None,
        };

        // Add to history
        self.strategy_history
            .entry(features.format.clone())
            .or_default()
            .push(performance);

        // Update optimal strategy if this one performs better
        self.update_optimal_strategy(&features.format);
    }

    /// Update optimal strategy for a data format
    fn update_optimal_strategy(&mut self, format: &DataFormat) {
        if let Some(history) = self.strategy_history.get(format) {
            // Find strategy with best average performance
            let mut best_performance = 0.0;
            let mut best_params: Option<EmbedParams> = None;

            // Group by strategy parameters
            let mut strategy_groups: HashMap<String, Vec<&StrategyPerformance>> = HashMap::new();

            for perf in history {
                let key = self.strategy_key(&perf.params);
                strategy_groups.entry(key).or_default().push(perf);
            }

            // Calculate average performance for each strategy
            for (_, performances) in strategy_groups {
                let avg_performance = self.calculate_average_performance(&performances);

                if avg_performance > best_performance
                    && avg_performance > self.min_performance_threshold
                {
                    best_performance = avg_performance;
                    best_params = Some(performances[0].params.clone());
                }
            }

            if let Some(params) = best_params {
                self.optimal_strategies.insert(format.clone(), params);
            }
        }
    }

    /// Calculate composite performance score
    fn calculate_average_performance(&self, performances: &[&StrategyPerformance]) -> f32 {
        if performances.is_empty() {
            return 0.0;
        }

        let mut total_score = 0.0;

        for perf in performances {
            // Composite score considering multiple factors
            let score = (perf.metrics.capacity_efficiency * 0.3)
                + (perf.metrics.extraction_accuracy * 0.3)
                + (perf.metrics.quality_preservation * 0.2)
                + (perf.success_rate * 0.2);

            total_score += score;
        }

        total_score / performances.len() as f32
    }

    /// Generate unique key for strategy parameters
    fn strategy_key(&self, params: &EmbedParams) -> String {
        format!(
            "ec:{:?}_stealth:{:?}_gpu:{}",
            params.error_correction_level, params.stealth_level, params.use_gpu
        )
    }

    /// Get performance summary for a data format
    pub fn get_performance_summary(&self, format: &DataFormat) -> Option<PerformanceSummary> {
        self.strategy_history.get(format).map(|history| {
            let total_attempts = history.len();
            let successful_attempts = history.iter().filter(|p| p.success_rate > 0.5).count();
            let avg_capacity = history
                .iter()
                .map(|p| p.metrics.capacity_efficiency)
                .sum::<f32>()
                / total_attempts as f32;
            let avg_time = history
                .iter()
                .map(|p| p.metrics.embedding_time_ms)
                .sum::<u64>()
                / total_attempts as u64;

            PerformanceSummary {
                format: format.clone(),
                total_attempts,
                successful_attempts,
                success_rate: successful_attempts as f32 / total_attempts as f32,
                average_capacity_efficiency: avg_capacity,
                average_embedding_time_ms: avg_time,
                optimal_strategy: self.optimal_strategies.get(format).cloned(),
            }
        })
    }
}

/// Performance summary for a data format
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub format: DataFormat,
    pub total_attempts: usize,
    pub successful_attempts: usize,
    pub success_rate: f32,
    pub average_capacity_efficiency: f32,
    pub average_embedding_time_ms: u64,
    pub optimal_strategy: Option<EmbedParams>,
}

impl StrategyBandit {
    /// Create new multi-armed bandit for strategy selection
    pub fn new(strategies: Vec<EmbedParams>) -> Self {
        let num_arms = strategies.len();
        Self {
            arms: strategies,
            rewards: vec![Vec::new(); num_arms],
            pull_counts: vec![0; num_arms],
            _confidence_bounds: vec![0.0; num_arms],
            total_pulls: 0,
        }
    }

    /// Select arm using Upper Confidence Bound (UCB) algorithm
    pub fn select_arm(&mut self) -> usize {
        // If any arm hasn't been pulled, pull it
        for (i, &count) in self.pull_counts.iter().enumerate() {
            if count == 0 {
                return i;
            }
        }

        // Calculate UCB values and select best arm
        let mut best_arm = 0;
        let mut best_value = f32::NEG_INFINITY;

        for i in 0..self.arms.len() {
            let mean_reward = self.rewards[i].iter().sum::<f32>() / self.pull_counts[i] as f32;
            let confidence =
                (2.0 * (self.total_pulls as f32).ln() / self.pull_counts[i] as f32).sqrt();
            let ucb_value = mean_reward + confidence;

            if ucb_value > best_value {
                best_value = ucb_value;
                best_arm = i;
            }
        }

        best_arm
    }

    /// Update bandit with reward for selected arm
    pub fn update(&mut self, arm: usize, reward: f32) {
        if arm < self.arms.len() {
            self.rewards[arm].push(reward);
            self.pull_counts[arm] += 1;
            self.total_pulls += 1;
        }
    }

    /// Get the currently best performing arm
    pub fn best_arm(&self) -> Option<usize> {
        if self.total_pulls == 0 {
            return None;
        }

        let mut best_arm = 0;
        let mut best_reward = f32::NEG_INFINITY;

        for i in 0..self.arms.len() {
            if self.pull_counts[i] > 0 {
                let mean_reward = self.rewards[i].iter().sum::<f32>() / self.pull_counts[i] as f32;
                if mean_reward > best_reward {
                    best_reward = mean_reward;
                    best_arm = i;
                }
            }
        }

        Some(best_arm)
    }
}

impl StrategyAgent {
    /// Create new Q-learning agent
    pub fn new(alpha: f32, gamma: f32, epsilon: f32) -> Self {
        Self {
            q_table: HashMap::new(),
            alpha: alpha.clamp(0.0, 1.0),
            gamma: gamma.clamp(0.0, 1.0),
            epsilon: epsilon.clamp(0.0, 1.0),
            current_state: None,
            _previous_action: None,
        }
    }

    /// Select action using epsilon-greedy policy
    pub fn select_action(&mut self, state: &str, available_actions: &[String]) -> String {
        self.current_state = Some(state.to_string());

        if rand::random::<f32>() < self.epsilon {
            // Explore: random action
            available_actions[rand::random::<usize>() % available_actions.len()].clone()
        } else {
            // Exploit: best known action
            self.get_best_action(state, available_actions)
        }
    }

    /// Update Q-value based on reward
    pub fn update_q_value(&mut self, state: &str, action: &str, reward: f32, next_state: &str) {
        let current_q = self.get_q_value(state, action);
        let max_next_q = self.get_max_q_value(next_state);

        let new_q = current_q + self.alpha * (reward + self.gamma * max_next_q - current_q);

        self.q_table
            .entry(state.to_string())
            .or_default()
            .insert(action.to_string(), new_q);
    }

    /// Get Q-value for state-action pair
    fn get_q_value(&self, state: &str, action: &str) -> f32 {
        self.q_table
            .get(state)
            .and_then(|actions| actions.get(action))
            .copied()
            .unwrap_or(0.0)
    }

    /// Get maximum Q-value for a state
    fn get_max_q_value(&self, state: &str) -> f32 {
        self.q_table
            .get(state)
            .map(|actions| actions.values().copied().fold(f32::NEG_INFINITY, f32::max))
            .unwrap_or(0.0)
    }

    /// Get best action for a state
    fn get_best_action(&self, state: &str, available_actions: &[String]) -> String {
        if let Some(actions) = self.q_table.get(state) {
            let mut best_action = &available_actions[0];
            let mut best_value = f32::NEG_INFINITY;

            for action in available_actions {
                if let Some(&value) = actions.get(action) {
                    if value > best_value {
                        best_value = value;
                        best_action = action;
                    }
                }
            }

            best_action.clone()
        } else {
            // No history, return random action
            available_actions[rand::random::<usize>() % available_actions.len()].clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_adaptive_strategy_creation() {
        let strategy = AdaptiveStrategy::new();
        assert_eq!(strategy.strategy_history.len(), 0);
        assert_eq!(strategy.optimal_strategies.len(), 0);
    }

    #[test]
    fn test_strategy_bandit() {
        let strategies = vec![
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
                enable_validation: false,
                cipher_kind: Default::default(),
            },
            EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::High,
                stealth_level: StealthLevel::High,
                use_gpu: true,
                format_specific: HashMap::new(),
                enable_validation: false,
                cipher_kind: Default::default(),
            },
        ];

        let mut bandit = StrategyBandit::new(strategies);

        // Test initial selection (should select unplayed arms first)
        let arm1 = bandit.select_arm();
        bandit.update(arm1, 0.8);

        let arm2 = bandit.select_arm();
        bandit.update(arm2, 0.6);

        assert_ne!(arm1, arm2); // Should select different arms initially
        assert_eq!(bandit.total_pulls, 2);
    }

    #[test]
    fn test_q_learning_agent() {
        let mut agent = StrategyAgent::new(0.1, 0.9, 0.1);
        let actions = vec!["action1".to_string(), "action2".to_string()];

        let action = agent.select_action("state1", &actions);
        agent.update_q_value("state1", &action, 1.0, "state2");

        assert!(agent.q_table.contains_key("state1"));
        assert!(agent.q_table["state1"].contains_key(&action));
    }

    #[test]
    fn test_performance_calculation() {
        let strategy = AdaptiveStrategy::new();

        let performances = [StrategyPerformance {
            params: EmbedParams {
                encryption_key: vec![],
                error_correction_level: ECLevel::Low,
                stealth_level: StealthLevel::Low,
                use_gpu: false,
                format_specific: HashMap::new(),
                enable_validation: false,
                cipher_kind: Default::default(),
            },
            metrics: PerformanceMetrics {
                capacity_efficiency: 0.8,
                embedding_time_ms: 100,
                extraction_accuracy: 0.9,
                quality_preservation: 0.85,
                memory_usage: 1024,
                gpu_acceleration_benefit: 1.0,
            },
            carrier_features: CarrierFeatures {
                format: DataFormat::PNG,
                size: 1000,
                entropy: 7.5,
                frequency_characteristics: vec![],
                complexity_measures: vec![],
                format_specific: HashMap::new(),
            },
            success_rate: 1.0,
            usage_count: 1,
            last_used: 0,
            detection_resistance: None,
        }];

        let refs: Vec<&StrategyPerformance> = performances.iter().collect();
        let avg_performance = strategy.calculate_average_performance(&refs);

        assert!(avg_performance > 0.0);
        assert!(avg_performance <= 1.0);
    }
}
